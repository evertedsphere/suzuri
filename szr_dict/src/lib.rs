use std::fmt::Debug;

use async_trait::async_trait;
use csv::StringRecord;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use sqlx::{postgres::PgArguments, query::Query, types::Json, Postgres};
use szr_bulk_insert::PgBulkInsert;

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Definitions(pub Vec<String>);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Def {
    pub id: i32,
    pub dict_name: String,
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewDef {
    pub dict_name: String,
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

impl PgBulkInsert for Def {
    type InsertFields = NewDef;

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!(
            "COPY defs (dict_name, spelling, reading, content) FROM STDIN WITH (FORMAT CSV)"
        )
    }

    fn to_string_record(ins: Self::InsertFields) -> szr_bulk_insert::Result<StringRecord> {
        Ok(StringRecord::from(
            &[
                ins.dict_name,
                ins.spelling,
                ins.reading,
                serde_json::to_string(&ins.content.0)
                    .map_err(|source| szr_bulk_insert::Error::SerialisationError { source })?,
            ][..],
        ))
    }
}

impl From<Json<Vec<String>>> for Definitions {
    fn from(value: Json<Vec<String>>) -> Self {
        Self(value.0)
    }
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    InsertFailedError { source: sqlx::Error },
}

#[async_trait]
pub trait DictionaryFormat {
    type Error: std::error::Error;

    fn read_from_path(path: &str, name: &str) -> Result<Vec<NewDef>, Self::Error>;
}
