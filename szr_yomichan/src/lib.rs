use std::{fmt, path::Path};

use glob::glob;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snafu::{ResultExt, Snafu};
use sqlx::PgPool;
use szr_bulk_insert::PgBulkInsert;
use szr_dict::{Def, Definitions, DictionaryFormat, NewDef};
use tracing::{debug, instrument, trace, warn};

pub struct Yomichan;

struct YomichanDef {
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

struct ArrayConsumer<'a, A, V> {
    field_number: usize,
    visitor: &'a A,
    seq: V,
}

impl<'a, 'de, A, V> ArrayConsumer<'a, A, V>
where
    A: Visitor<'de>,
    V: SeqAccess<'de>,
{
    fn new(visitor: &'a A, seq: V) -> Self {
        Self {
            field_number: 0,
            visitor,
            seq,
        }
    }

    fn next<T: Deserialize<'de>>(&mut self) -> Result<T, V::Error> {
        let r = self
            .seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::invalid_length(self.field_number, self.visitor))?;
        self.field_number += 1;
        Ok(r)
    }

    fn next_split(&mut self, sep: char) -> Result<Vec<String>, V::Error> {
        Ok(self
            .next::<&str>()?
            .split(sep)
            .filter(|x| !x.is_empty())
            .map(|x| (x.to_owned()))
            .collect())
    }

    fn next_space_split(&mut self) -> Result<Vec<String>, V::Error> {
        self.next_split(' ')
    }
}

impl<'de> Deserialize<'de> for YomichanDef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<YomichanDef, D::Error> {
        struct TokenVisitor;
        impl<'de> Visitor<'de> for TokenVisitor {
            type Value = YomichanDef;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Term")
            }

            fn visit_seq<V: SeqAccess<'de>>(
                self,
                seq: V,
            ) -> std::result::Result<Self::Value, V::Error> {
                let mut c = ArrayConsumer::new(&self, seq);
                let spelling: String = c.next()?;
                let raw_reading: String = c.next()?;
                // This is fine since they're apparently all katakana loanwords.
                let reading = if raw_reading.is_empty() {
                    spelling.clone()
                } else {
                    raw_reading.to_owned()
                };
                let _tags: Vec<String> = c.next_space_split()?;
                let _rule_idents: Vec<String> = c.next_space_split()?;
                let _score: i64 = c.next()?;
                let content: Vec<String> = c.next()?;
                let _sequence_num: i64 = c.next()?;
                let _term_tags: Vec<String> = c.next_space_split()?;
                let term = YomichanDef {
                    spelling,
                    reading,
                    content: Definitions(content),
                };
                Ok(term)
            }
        }
        deserializer.deserialize_any(TokenVisitor)
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    #[snafu(display("failed to find any term bank files"))]
    NoTermBankFiles,
    #[snafu(display("failed to open term bank file"))]
    CannotOpenTermBankFile {
        source: std::io::Error,
    },
    #[snafu(display("failed to deserialise contents of term bank file"))]
    CannotDeserializeTermBankFile {
        source: serde_json::Error,
    },
    InvalidGlobPattern {
        source: glob::PatternError,
    },
    CannotReadFilePath {
        source: glob::GlobError,
    },
    #[snafu(display("Failed to bulk insert definitions from dict: {source}"))]
    BulkInsertFailed {
        source: szr_bulk_insert::Error,
    },
    #[snafu(display("Failed to bulk insert definitions from dict: {source}"))]
    BulkInsertPreparationFailed {
        source: sqlx::Error,
    },
}

impl DictionaryFormat for Yomichan {
    type Error = Error;

    fn read_from_path(path: impl AsRef<Path>, name: &str) -> Result<Vec<NewDef>, Self::Error> {
        let pat = format!("{}/term_bank_*.json", path.as_ref().to_str().unwrap());
        debug!("pattern: {}", pat);
        let term_bank_files = glob(&pat).context(InvalidGlobPattern)?.collect::<Vec<_>>();
        if term_bank_files.is_empty() {
            return NoTermBankFiles.fail();
        }
        let terms: Vec<NewDef> = term_bank_files
            .into_par_iter()
            .map(|path| {
                let text = std::fs::read_to_string(path.context(CannotReadFilePath)?)
                    .context(CannotOpenTermBankFile)?;
                Ok(serde_json::from_str::<Vec<YomichanDef>>(&text)
                    .context(CannotDeserializeTermBankFile)?
                    .into_iter()
                    .filter_map(
                        |YomichanDef {
                             spelling,
                             reading,
                             content,
                         }| {
                            if spelling.is_empty() {
                                warn!("skipping term with empty spelling");
                                None
                            } else {
                                Some(NewDef {
                                    spelling,
                                    reading,
                                    content,
                                    dict_name: name.to_owned(),
                                })
                            }
                        },
                    )
                    .collect())
            })
            .collect::<Result<Vec<Vec<_>>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(terms)
    }
}

impl Yomichan {
    #[instrument(skip(pool, inputs), err)]
    pub async fn import_all(pool: &PgPool, inputs: Vec<(impl AsRef<Path>, &str)>) -> Result<()> {
        let mut records = Vec::new();

        for (path, name) in inputs.into_iter() {
            let already_exists = sqlx::query_scalar!(
                r#"SELECT EXISTS(SELECT 1 FROM defs WHERE dict_name = $1) as "already_exists!: bool""#,
                name
            )
            .fetch_one(pool)
            .await
            .context(BulkInsertPreparationFailed)?;

            if already_exists {
                trace!("yomichan dictionary {} already imported, skipping", name);
                continue;
            }
            records.extend(Self::read_from_path(path, name)?);
        }

        if !records.is_empty() {
            Self::import(pool, records).await?;
        }

        Ok(())
    }

    pub async fn import_from_file(pool: &PgPool, path: impl AsRef<Path>, name: &str) -> Result<()> {
        Self::import_all(pool, vec![(path, name)]).await
    }

    #[instrument(skip(pool, records), err)]
    async fn import(pool: &PgPool, records: Vec<NewDef>) -> Result<()> {
        let mut tx = pool.begin().await.context(BulkInsertPreparationFailed)?;

        sqlx::query!("DROP INDEX IF EXISTS defs_spelling_reading")
            .execute(&mut *tx)
            .await
            .context(BulkInsertPreparationFailed)?;

        Def::copy_records(&mut *tx, records)
            .await
            .context(BulkInsertFailed)?;

        sqlx::query!("CREATE INDEX defs_spelling_reading ON defs (spelling, reading)")
            .execute(&mut *tx)
            .await
            .context(BulkInsertPreparationFailed)?;

        sqlx::query!("ANALYZE defs")
            .execute(&mut *tx)
            .await
            .context(BulkInsertPreparationFailed)?;

        tx.commit().await.context(BulkInsertPreparationFailed)?;

        Ok(())
    }
}

// pub async fn query_dict(
//     pool: &PgPool,
//     spelling: &str,
//     reading: &str,
// ) -> Result<Vec<DictDef>, Error> {
//     let terms = sqlx::query_as::<_, DictDef>(
//         "SELECT dict, spelling, reading, defs FROM terms WHERE spelling = $1 AND reading = $2",
//     )
//     .bind(spelling)
//     .bind(reading)
//     .fetch_all(&*pool)
//     .await
//     .context(Query)?;
//     Ok(terms)
// }

// #[instrument(skip(pool, path))]
// pub async fn import_dictionary(pool: &PgPool, name: &str, path: &str) -> Result<(), Error> {
//     let dict_terms = sqlx::query!(
//         r#"SELECT EXISTS(SELECT 1 FROM terms WHERE dict = $1) AS "exists!: bool""#,
//         name
//     )
//     .fetch_one(pool)
//     .await
//     .context(Query)?;

//     if dict_terms.exists {
//         info!("dictionary {} already imported, skipping", name);
//         return Ok(());
//     } else {
//         info!("dictionary {} not found, importing", name);
//     }

//     let dict = read_from_path(path)?;
//     save_dictionary(pool, name, dict).await?;
//     Ok(())
// }

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct FreqTerm {
    pub spelling: String,
    pub reading: String,
    pub frequency: u64,
}

#[instrument(err)]
pub fn read_frequency_dictionary(path: &str) -> Result<Vec<FreqTerm>, Error> {
    let text = std::fs::read_to_string(format!("input/{}/term_meta_bank_1.json", path))
        .context(CannotOpenTermBankFile)?;
    let raws =
        serde_json::from_str::<Vec<RawFreqTerm>>(&text).context(CannotDeserializeTermBankFile)?;
    let freqs = raws
        .into_iter()
        .filter_map(|RawFreqTerm(spelling, _, body)| match body {
            RawFreq::NoReading(freq) => {
                warn!("empty reading for {:?} with freq {}", spelling, freq);
                None
            }
            RawFreq::WithReading { reading, frequency } => Some(FreqTerm {
                spelling,
                reading,
                frequency,
            }),
        })
        .collect();
    Ok(freqs)
}

// #[instrument(skip(pool, dict))]
// async fn persist_frequency_dictionary(
//     pool: &PgPool,
//     name: &str,
//     dict: Vec<FreqTerm>,
// ) -> Result<(), Error> {
//     let max_arg_count = 301;
//     trace!(size = dict.len(), "persisting");
//     let chunks: Vec<Vec<FreqTerm>> = dict
//         .into_iter()
//         .chunks(max_arg_count / 3)
//         .into_iter()
//         .map(|chunk| chunk.collect())
//         .collect::<Vec<_>>();
//     for input in chunks.into_iter() {
//         let name = name.to_string();
//         trace!("building query");
//         let mut qb =
//             QueryBuilder::new("INSERT INTO freq_terms(dict, spelling, reading, frequency)");
//         qb.push_values(input, |mut b, term| {
//             b.push_bind(name.clone())
//                 .push_bind(term.spelling.clone())
//                 .push_bind(term.reading.clone())
//                 .push_bind(term.frequency as i64);
//         });
//         let query = qb.build();
//         query.execute(pool).await.context(Persistence)?;
//     }
//     Ok(())
// }

// pub async fn import_frequency_dictionary(
//     pool: &PgPool,
//     name: &str,
//     path: &str,
// ) -> Result<(), Error> {
//     let dict_terms = sqlx::query!(
//         r#"SELECT EXISTS(SELECT 1 FROM freq_terms WHERE dict = $1) AS "exists!: bool""#,
//         name
//     )
//     .fetch_one(pool)
//     .await
//     .context(Query)?;

//     if dict_terms.exists {
//         info!("frequency dictionary {} already imported, skipping", name);
//         return Ok(());
//     }

//     let dict = read_frequency_dictionary(path)?;
//     persist_frequency_dictionary(pool, name, dict).await?;
//     Ok(())
// }

impl FreqTerm {
    // pub async fn get(pool: &PgPool, spelling: &str, reading: &str) -> Result<u64, Error> {
    //     let rec = sqlx::query!(
    //         r#"SELECT frequency FROM freq_terms WHERE spelling = $1 AND reading = $2"#,
    //         spelling,
    //         reading,
    //     )
    //     .fetch_one(pool)
    //     .await
    //     .context(Persistence)?;
    //     Ok(rec.frequency as u64)
    // }

    // pub async fn get_all_with_character(
    //     pool: &PgPool,
    //     kanji: char,
    // ) -> Result<Vec<FreqTerm>, Error> {
    //     let kanji = String::from(kanji);
    //     let terms = sqlx::query!(
    //         r#"SELECT spelling, reading, frequency FROM freq_terms WHERE spelling LIKE '%' || $1 || '%'"#,
    //         kanji,
    //     )
    //     .fetch_all(pool)
    //     .await
    //     .context(Persistence)?;
    //     Ok(terms
    //         .into_iter()
    //         .map(|rec| FreqTerm {
    //             spelling: rec.spelling,
    //             reading: rec.reading,
    //             frequency: rec.frequency as u64,
    //         })
    //         .collect())
    // }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum RawFreq {
    WithReading { reading: String, frequency: u64 },
    NoReading(u64),
}

// TODO enum Freq(#[serde(rename = "freq")] Freq) etc
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawFreqTerm(String, String, RawFreq);

#[test]
fn parse_nonexistent_dict_fail() {
    let r = Yomichan::read_from_path("../input/jmdict_klingon", "jmdict_klingon");
    assert!(matches!(r, Err(Error::NoTermBankFiles)));
}

#[test]
fn parse_dict() {
    let r = Yomichan::read_from_path("../input/jmdict_en", "jmdict_en");
    assert!(r.is_ok());
}

#[test]
fn deserialize_frequency_dictionary() {
    let path = "../input/Freq_CC100/term_meta_bank_1.json";
    let text = std::fs::read_to_string(path).unwrap();
    let des = serde_json::from_str::<Vec<RawFreqTerm>>(&text).unwrap();
    for d in des {
        println!("{:?}", d);
    }
}
