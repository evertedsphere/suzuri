use std::fmt::Debug;

use async_trait::async_trait;
use csv::StringRecord;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu, Whatever};
use sqlx::{
    postgres::PgArguments,
    query::{Query, QueryScalar},
    types::Json,
    Execute, PgConnection, PgPool, Postgres,
};
use tracing::{debug, instrument, warn};

pub struct BulkCopyInsertData<T: BulkCopyInsert> {
    pub records: Vec<T::InsertFields>,
    pub key: T::Key,
}

/// This trait allows for efficient insertion of a batch of related data in bulk into a table
/// that may contain data already.
///
/// Indexes are dropped before insertion and recreated after insertion.
///
/// We also run `ANALYZE` afterwards, to make sure that the ingestion of a large amount of data
/// does not negatively impact query planning due to stale statistics.
#[async_trait]
pub trait BulkCopyInsert: Sized {
    /// This key is used when checking to see if this data is already in the table,
    /// in [`exists_query`] below.
    type Key: Debug;

    /// A related type, usually a struct containing all but the primary key(s),
    /// that can be inserted into the database.
    type InsertFields: Send;

    async fn copy_records(
        conn: &mut PgConnection,
        records: Vec<Self::InsertFields>,
    ) -> Result<(), sqlx::Error> {
        let mut handle = conn.copy_in_raw(Self::copy_in_statement().sql()).await?;
        let buf = Self::to_string_record_vec(records);
        debug!("sending buffer of size {}", buf.len());
        handle.send(buf).await?;
        let num_rows = handle.finish().await?;
        debug!("rows affected = {}", num_rows);
        Ok(())
    }

    /// The `COPY IN ... STDIN` statement to use to begin the transfer.
    fn copy_in_statement() -> Query<'static, Postgres, PgArguments>;

    /// Convert an object into a [`csv::StringRecord`] for insertion.
    fn to_string_record(ins: Self::InsertFields) -> StringRecord;

    fn to_string_record_vec(records: Vec<Self::InsertFields>) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut writer = csv::Writer::from_writer(&mut buf);
            for d in records.into_iter() {
                let rec = Self::to_string_record(d);
                writer.write_record(&rec).unwrap();
            }
            writer.flush().unwrap();
        }
        buf
    }

    fn build_bulk_insert_batch(
        key: Self::Key,
        records: Vec<Self::InsertFields>,
    ) -> BulkCopyInsertData<Self> {
        BulkCopyInsertData { records, key }
    }
}

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

impl BulkCopyInsert for Def {
    type InsertFields = NewDef;
    type Key = String;

    fn copy_in_statement() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!(
            "COPY defs (dict_name, spelling, reading, content) FROM STDIN WITH (FORMAT CSV)"
        )
    }

    fn to_string_record(ins: Self::InsertFields) -> StringRecord {
        StringRecord::from(
            &[
                ins.dict_name,
                ins.spelling,
                ins.reading,
                serde_json::to_string(&ins.content.0).unwrap(),
            ][..],
        )
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

    fn read_from_path(path: &str, name: &str) -> Result<BulkCopyInsertData<Def>, Self::Error>;
}
