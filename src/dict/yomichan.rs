use glob::glob;
use itertools::Itertools;
use rayon::prelude::*;
use serde::{
    de::{SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
pub use snafu::prelude::*;
use sqlx::{types::Json, FromRow, QueryBuilder, Sqlite, SqlitePool};
use std::{borrow::Cow, fmt};
use tokio::task::JoinSet;
use tracing::{debug, error, instrument, trace, warn};

use crate::furi::kata_to_hira;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TermTag(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Def(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DefTag(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleIdent(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Term {
    spelling: String,
    reading: String,
    defs: Vec<Def>,
    rule_idents: Vec<RuleIdent>,
    def_tags: Vec<DefTag>,
    term_tags: Vec<TermTag>,
    score: i64,
    sequence_num: i64,
}

pub struct CustomDe<T>(T);

impl<'de> Deserialize<'de> for CustomDe<Term> {
    fn deserialize<D>(deserializer: D) -> Result<CustomDe<Term>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TokenVisitor;

        impl<'de> Visitor<'de> for TokenVisitor {
            type Value = CustomDe<Term>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Term")
            }

            fn visit_seq<V>(self, mut seq: V) -> std::result::Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let spelling: String = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let raw_reading = seq
                    .next_element::<String>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let reading = if raw_reading.is_empty() {
                    spelling.clone()
                } else {
                    raw_reading.to_owned()
                };
                let reading = reading.chars().map(|c| kata_to_hira(c)).collect();
                let def_tags: Vec<DefTag> = seq
                    .next_element::<&str>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?
                    .split(' ')
                    .filter(|x| !x.is_empty())
                    .map(|x| DefTag(x.to_owned()))
                    .collect();
                let rule_idents: Vec<RuleIdent> = seq
                    .next_element::<&str>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?
                    .split(" ")
                    .filter(|x| !x.is_empty())
                    .map(|x| RuleIdent(x.to_string()))
                    .collect();
                let score: i64 = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let defs: Vec<Def> = seq
                    .next_element::<Vec<Cow<'_, str>>>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?
                    .into_iter()
                    .map(|x| Def(x.to_string()))
                    .collect();
                let sequence_num = seq
                    .next_element()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                let term_tags = seq
                    .next_element::<&str>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?
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
    NoTermBankFiles,
    OpenTermBankFile { source: std::io::Error },
    DeserializeTermBankFile { source: serde_json::Error },
    ParseGlobPattern { source: glob::PatternError },
    ReadFilePath { source: glob::GlobError },
    PersistenceError { source: sqlx::Error },
    QueryError { source: sqlx::Error },
}

#[instrument]
fn read_dictionary(path: &str) -> Result<Vec<Term>, DictError> {
    let term_bank_files = glob(&format!("input/{}/term_bank_*.json", path))
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

// pub struct Dict {
//     pub title: String,
//     pub terms: Vec<Term>,
// }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, FromRow)]
pub struct DictDef {
    pub dict: String,
    pub spelling: String,
    pub reading: String,
    pub defs: Json<Vec<String>>,
}

#[instrument(skip_all)]
async fn persist_dictionary(
    pool: &SqlitePool,
    name: &str,
    dict: Vec<Term>,
) -> Result<(), DictError> {
    let already_exists = sqlx::query!("SELECT spelling FROM terms WHERE dict = ? LIMIT 1", name)
        .fetch_one(pool)
        .await;

    if already_exists.is_ok() {
        warn!("dictionary {} already imported, skipping", name);
        return Ok(());
    }

    // see jpdb::parse
    let max_arg_count = 301;

    trace!(size = dict.len(), "persisting");

    let chunks: Vec<Vec<Term>> = dict
        .into_iter()
        .chunks(max_arg_count / 3)
        .into_iter()
        .map(|chunk| chunk.collect())
        .collect::<Vec<_>>();

    let mut set = JoinSet::new();

    for input in chunks.into_iter() {
        let conn = pool.clone();
        let name = name.to_string();
        set.spawn(async move {
            trace!("building query");
            let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
                r#"
        INSERT INTO terms(dict, spelling, reading, defs)
    "#,
            );
            qb.push_values(input, |mut b, term| {
                b.push_bind(name.clone())
                    .push_bind(term.spelling.clone())
                    .push_bind(term.reading.clone())
                    .push_bind(Json(term.defs.clone()));
            });
            let query = qb.build();
            query.execute(&conn).await.context(PersistenceCtx)
        });
    }

    while let Some(next) = set.join_next().await {
        trace!("joined {:?}", next);
    }

    Ok(())
}

pub async fn query_dict(
    pool: &SqlitePool,
    spelling: &str,
    reading: &str,
) -> Result<Vec<DictDef>, DictError> {
    let terms = sqlx::query_as::<_, DictDef>(
        "SELECT dict, spelling, reading, defs FROM terms WHERE spelling = $1 AND reading = $2",
    )
    .bind(spelling)
    .bind(reading)
    .fetch_all(&*pool)
    .await
    .context(QueryCtx)?;

    Ok(terms)
}

pub async fn import_dictionary(pool: &SqlitePool, name: &str, path: &str) -> Result<(), DictError> {
    let dict = read_dictionary(path)?;
    persist_dictionary(pool, name, dict).await?;
    Ok(())
}

#[test]
fn parse_nonexistent_dict_fail() {
    let r = read_dictionary("input/jmdict_klingon");
    assert!(matches!(r, Err(DictError::NoTermBankFiles)));
}
