use glob::glob;
use rayon::prelude::*;
use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
pub use snafu::prelude::*;
// use sqlx::{types::Json, FromRow, PgPool, QueryBuilder};
use diesel::connection::LoadConnection;
use diesel::pg::Pg;
use diesel::prelude::*;
use std::{borrow::Cow, fmt};
use tracing::{instrument, trace, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TermTag(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Def(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DefTag(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleIdent(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Term {
    pub spelling: String,
    pub reading: String,
    pub defs: Vec<Def>,
    rule_idents: Vec<RuleIdent>,
    def_tags: Vec<DefTag>,
    term_tags: Vec<TermTag>,
    score: i64,
    sequence_num: i64,
}

struct CustomDe<T>(T);

impl<'de> Deserialize<'de> for CustomDe<Term> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<CustomDe<Term>, D::Error> {
        struct TokenVisitor;
        impl<'de> Visitor<'de> for TokenVisitor {
            type Value = CustomDe<Term>;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Term")
            }
            fn visit_seq<V: SeqAccess<'de>>(
                self,
                mut seq: V,
            ) -> std::result::Result<Self::Value, V::Error> {
                let spelling: String = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let raw_reading = seq
                    .next_element::<String>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(1, &self))?;
                let reading = if raw_reading.is_empty() {
                    spelling.clone()
                } else {
                    raw_reading.to_owned()
                };
                // let reading = reading.chars().map(|c| kata_to_hira(c)).collect();
                let def_tags: Vec<DefTag> = seq
                    .next_element::<&str>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(2, &self))?
                    .split(' ')
                    .filter(|x| !x.is_empty())
                    .map(|x| DefTag(x.to_owned()))
                    .collect();
                let rule_idents: Vec<RuleIdent> = seq
                    .next_element::<&str>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(3, &self))?
                    .split(" ")
                    .filter(|x| !x.is_empty())
                    .map(|x| RuleIdent(x.to_string()))
                    .collect();
                let score: i64 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(4, &self))?;
                let defs: Vec<Def> = seq
                    .next_element::<Vec<Cow<'_, str>>>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(5, &self))?
                    .into_iter()
                    .map(|x| Def(x.to_string()))
                    .collect();
                let sequence_num = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(6, &self))?;
                let term_tags = seq
                    .next_element::<&str>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(7, &self))?
                    .split(' ')
                    .filter(|x| !x.is_empty())
                    .map(|x| TermTag(x.to_string()))
                    .collect();
                let term = Term {
                    spelling,
                    reading,
                    defs,
                    rule_idents,
                    def_tags,
                    term_tags,
                    score,
                    sequence_num,
                };
                Ok(CustomDe(term))
            }
        }
        deserializer.deserialize_any(TokenVisitor)
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Ctx)))]
pub enum DictError {
    #[snafu(display("failed to find any term bank files"))]
    NoTermBankFiles,
    #[snafu(display("failed to open term bank file"))]
    OpenTermBankFile {
        source: std::io::Error,
    },
    #[snafu(display("failed to deserialise contents of term bank file"))]
    DeserializeTermBankFile {
        source: serde_json::Error,
    },
    ParseGlobPattern {
        source: glob::PatternError,
    },
    ReadFilePath {
        source: glob::GlobError,
    },
    // PersistenceError { source: sqlx::Error },
    // QueryError { source: sqlx::Error },
}

pub fn read_dictionary(path: &str) -> Result<Vec<Term>, DictError> {
    let term_bank_files = glob(&format!("{}/term_bank_*.json", path))
        .context(ParseGlobPatternCtx)?
        .collect::<Vec<_>>();

    if term_bank_files.is_empty() {
        return Err(DictError::NoTermBankFiles);
    }

    let terms: Vec<Term> = term_bank_files
        .into_par_iter()
        .map(|path| {
            let text = std::fs::read_to_string(path.context(ReadFilePathCtx)?)
                .context(OpenTermBankFileCtx)?;
            Ok(serde_json::from_str::<Vec<CustomDe<Term>>>(&text)
                .context(DeserializeTermBankFileCtx)?
                .into_iter()
                .map(|x| x.0)
                .collect())
        })
        .collect::<Result<Vec<Vec<_>>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(terms)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DictDef {
    pub dict: String,
    pub spelling: String,
    pub reading: String,
    pub defs: Vec<String>,
}

#[instrument(skip(pool, dict))]
async fn persist_dictionary<C>(pool: &mut C, name: &str, dict: Vec<Term>) -> Result<(), DictError>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let max_arg_count = 301;
    trace!(size = dict.len(), "persisting");
    // let chunks: Vec<Vec<Term>> = dict
    //     .into_iter()
    //     .chunks(max_arg_count / 3)
    //     .into_iter()
    //     .map(|chunk| chunk.collect())
    //     .collect::<Vec<_>>();
    // for input in chunks.into_iter() {
    //     let conn = pool.clone();
    //     let name = name.to_string();
    //     trace!("building query");
    //     let mut qb: QueryBuilder<_> = QueryBuilder::new(
    //         r#"
    //     INSERT INTO terms(dict, spelling, reading, defs)
    // "#,
    //     );
    //     qb.push_values(input, |mut b, term| {
    //         b.push_bind(name.clone())
    //             .push_bind(term.spelling.clone())
    //             .push_bind(term.reading.clone())
    //             .push_bind(Json(term.defs.clone()));
    //     });
    //     let query = qb.build();
    //     query.execute(&conn).await.context(PersistenceCtx)?;
    // }
    Ok(())
}

// pub async fn query_dict(
//     pool: &PgPool,
//     spelling: &str,
//     reading: &str,
// ) -> Result<Vec<DictDef>, DictError> {
//     let terms = sqlx::query_as::<_, DictDef>(
//         "SELECT dict, spelling, reading, defs FROM terms WHERE spelling = $1 AND reading = $2",
//     )
//     .bind(spelling)
//     .bind(reading)
//     .fetch_all(&*pool)
//     .await
//     .context(QueryCtx)?;

//     Ok(terms)
// }

// #[instrument(skip(pool, path))]
// pub async fn import_dictionary(pool: &PgPool, name: &str, path: &str) -> Result<(), DictError> {
//     let dict_terms = sqlx::query!(
//         r#"SELECT EXISTS(SELECT 1 FROM terms WHERE dict = $1) AS "exists!: bool""#,
//         name
//     )
//     .fetch_one(pool)
//     .await
//     .context(QueryCtx)?;

//     if dict_terms.exists {
//         info!("dictionary {} already imported, skipping", name);
//         return Ok(());
//     } else {
//         info!("dictionary {} not found, importing", name);
//     }

//     let dict = read_dictionary(path)?;
//     persist_dictionary(pool, name, dict).await?;
//     Ok(())
// }

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct FreqTerm {
    pub spelling: String,
    pub reading: String,
    pub frequency: u64,
}

#[instrument(err)]
pub fn read_frequency_dictionary(path: &str) -> Result<Vec<FreqTerm>, DictError> {
    let text = std::fs::read_to_string(format!("input/{}/term_meta_bank_1.json", path))
        .context(OpenTermBankFileCtx)?;
    let raws =
        serde_json::from_str::<Vec<RawFreqTerm>>(&text).context(DeserializeTermBankFileCtx)?;
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
// ) -> Result<(), DictError> {
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
//         query.execute(pool).await.context(PersistenceCtx)?;
//     }
//     Ok(())
// }

// pub async fn import_frequency_dictionary(
//     pool: &PgPool,
//     name: &str,
//     path: &str,
// ) -> Result<(), DictError> {
//     let dict_terms = sqlx::query!(
//         r#"SELECT EXISTS(SELECT 1 FROM freq_terms WHERE dict = $1) AS "exists!: bool""#,
//         name
//     )
//     .fetch_one(pool)
//     .await
//     .context(QueryCtx)?;

//     if dict_terms.exists {
//         info!("frequency dictionary {} already imported, skipping", name);
//         return Ok(());
//     }

//     let dict = read_frequency_dictionary(path)?;
//     persist_frequency_dictionary(pool, name, dict).await?;
//     Ok(())
// }

impl FreqTerm {
    // pub async fn get(pool: &PgPool, spelling: &str, reading: &str) -> Result<u64, DictError> {
    //     let rec = sqlx::query!(
    //         r#"SELECT frequency FROM freq_terms WHERE spelling = $1 AND reading = $2"#,
    //         spelling,
    //         reading,
    //     )
    //     .fetch_one(pool)
    //     .await
    //     .context(PersistenceCtx)?;
    //     Ok(rec.frequency as u64)
    // }

    // pub async fn get_all_with_character(
    //     pool: &PgPool,
    //     kanji: char,
    // ) -> Result<Vec<FreqTerm>, DictError> {
    //     let kanji = String::from(kanji);
    //     let terms = sqlx::query!(
    //         r#"SELECT spelling, reading, frequency FROM freq_terms WHERE spelling LIKE '%' || $1 || '%'"#,
    //         kanji,
    //     )
    //     .fetch_all(pool)
    //     .await
    //     .context(PersistenceCtx)?;
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
    let r = read_dictionary("../input/jmdict_klingon");
    assert!(matches!(r, Err(DictError::NoTermBankFiles)));
}

#[test]
fn parse_dict() {
    let r = read_dictionary("../input/jmdict_en");
    println!("{:?}", r);
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
