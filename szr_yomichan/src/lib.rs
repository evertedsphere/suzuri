use std::{fmt, path::Path};

use glob::glob;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use snafu::{ResultExt, Snafu};
use sqlx::PgPool;
use szr_bulk_insert::PgBulkInsert;
use szr_dict::{Def, DefContent, DefTags, DictionaryFormat, NewDef};
use szr_ja_utils::kata_to_hira_str;
use tracing::{instrument, trace, warn};

pub struct Yomichan;

struct YomichanDef {
    pub spelling: String,
    pub reading: String,
    pub content: Vec<String>,
    pub tags: Vec<String>,
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
                let tags: Vec<String> = c.next_space_split()?;
                let _rule_idents: Vec<String> = c.next_space_split()?;
                let _score: i64 = c.next()?;
                let content: Vec<String> = c.next()?;
                let _sequence_num: i64 = c.next()?;
                let _term_tags: Vec<String> = c.next_space_split()?;
                let term = YomichanDef {
                    spelling,
                    // FIXME: add a normalised_reading column
                    reading: kata_to_hira_str(&reading),
                    content,
                    tags,
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

fn to_def_content(name: &str, defs: Vec<String>) -> DefContent {
    match name {
        "旺文社" => {
            debug_assert!(defs.len() == 1);
            let mut vs = defs[0].split('\n').map(|s| s.to_string());
            let header = vs.next().unwrap();
            // maximal example:
            // われ‐かえ・る【割れ返る】――カヘル（自五）《ラ（ロ）・リ（ツ）・ル・ル・レ・レ》
            let header_re = Regex::new(
                r"^(?<r>[^【]+)?(【(?<s>.+)】)?(?<ok>[^（]+)?(（(?<cf>.+)）)?(《(?<conj>.+)》)?$",
            )
            .expect("oubunsha: failed to build regex");
            if let Some(caps) = header_re.captures(&header) {
                // TODO ^」+
                let def_re = Regex::new(r"^(?<def>.+?)(?<ex>「.+」)?$")
                    .expect("oubunsha: failed to build regex");
                let definitions = vs
                    .map(|d| {
                        if let Some(def_caps) = def_re.captures(&d) {
                            let def = def_caps
                                .name("def")
                                .expect("oubunsha: unnamed def")
                                .as_str()
                                .to_owned();
                            let quotes = def_caps.name("ex").map(|s| s.as_str().to_owned());
                            (def, quotes)
                        } else {
                            (d, None)
                        }
                    })
                    .collect();

                let get_capture = |k| caps.name(k).map(|s| s.as_str().to_owned());
                DefContent::Oubunsha {
                    definitions,
                    spelling: get_capture("s"),
                    reading: get_capture("r"),
                    old_kana_spelling: get_capture("ok"),
                    conjugation_type: get_capture("cf"),
                    conjugation: get_capture("conj"),
                }
            } else {
                let mut all_definitions = vec![header];
                all_definitions.extend(vs);
                DefContent::Plain(all_definitions)
            }
        }
        _ => DefContent::Plain(defs),
    }
}

impl DictionaryFormat for Yomichan {
    type Error = Error;

    #[instrument(skip_all)]
    fn read_from_path(path: impl AsRef<Path>, name: &str) -> Result<Vec<NewDef>, Self::Error> {
        let pat = format!("{}/term_bank_*.json", path.as_ref().to_str().unwrap());
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
                             tags,
                         }| {
                            if spelling.is_empty() {
                                trace!("skipping term with empty spelling");
                                None
                            } else {
                                Some(NewDef {
                                    spelling,
                                    reading,
                                    content: to_def_content(name, content),
                                    tags: DefTags(tags),
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
    #[instrument(skip(pool, inputs), err, level = "debug")]
    pub async fn bulk_import_dicts(
        pool: &PgPool,
        inputs: Vec<(impl AsRef<Path>, &str)>,
    ) -> Result<()> {
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

    async fn import(pool: &PgPool, records: Vec<NewDef>) -> Result<()> {
        let mut tx = pool.begin().await.context(BulkInsertPreparationFailed)?;

        sqlx::query_file!("../migrations/4_enrich_defs.down.sql")
            .execute(&mut *tx)
            .await
            .context(BulkInsertPreparationFailed)?;

        Def::copy_records(&mut *tx, records)
            .await
            .context(BulkInsertFailed)?;

        sqlx::query_file!("../migrations/4_enrich_defs.up.sql")
            .execute(&mut *tx)
            .await
            .context(BulkInsertPreparationFailed)?;

        tx.commit().await.context(BulkInsertPreparationFailed)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct FreqTerm {
    pub spelling: String,
    pub reading: String,
    pub frequency: u64,
}

#[instrument(err, level = "debug")]
pub fn read_frequency_dictionary(path: &str) -> Result<Vec<FreqTerm>, Error> {
    let text = std::fs::read_to_string(format!(
        "/home/s/c/szr/input/{}/term_meta_bank_1.json",
        path
    ))
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
    let r = Yomichan::read_from_path("/home/s/c/szr/input/jmdict_klingon", "jmdict_klingon");
    assert!(matches!(r, Err(Error::NoTermBankFiles)));
}

#[test]
fn parse_dict() {
    let r = Yomichan::read_from_path("/home/s/c/szr/input/jmdict_en", "jmdict_en");
    assert!(r.is_ok());
}

#[test]
fn deserialize_frequency_dictionary() {
    let path = "/home/s/c/szr/input/Freq_CC100/term_meta_bank_1.json";
    let text = std::fs::read_to_string(path).unwrap();
    let des = serde_json::from_str::<Vec<RawFreqTerm>>(&text).unwrap();
    for d in des {
        println!("{:?}", d);
    }
}
