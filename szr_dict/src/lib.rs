use std::fmt::Debug;

use async_trait::async_trait;
use csv::StringRecord;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use sqlx::{
    postgres::PgArguments,
    query::{Query, QueryScalar},
    types::Json,
    PgPool, Postgres,
};
use tracing::{debug, instrument, warn};

pub trait BulkCopyInsert {
    type Key: Debug;
    type InsertFields;
    fn copy_in_statement() -> &'static str;
    fn create_indexes_query() -> Query<'static, Postgres, PgArguments>;
    fn drop_indexes_query() -> Query<'static, Postgres, PgArguments>;
    fn analyze_query() -> Query<'static, Postgres, PgArguments>;
    fn exists_query(key: &Self::Key) -> QueryScalar<'static, Postgres, Option<bool>, PgArguments>;
    fn to_string_record(ins: Self::InsertFields) -> StringRecord;
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

    fn copy_in_statement() -> &'static str {
        "COPY defs (dict_name, spelling, reading, content) FROM STDIN WITH (FORMAT CSV)"
    }

    fn create_indexes_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("CREATE INDEX defs_spelling_reading ON defs (spelling, reading)")
    }

    fn drop_indexes_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("DROP INDEX IF EXISTS defs_spelling_reading")
    }

    fn analyze_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("ANALYZE defs")
    }

    fn exists_query(
        dict_name: &Self::Key,
    ) -> QueryScalar<'static, Postgres, Option<bool>, PgArguments> {
        sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM defs WHERE dict_name = $1)",
            dict_name
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

pub struct BulkCopyInsertData<T: BulkCopyInsert> {
    pub records: Vec<T::InsertFields>,
    pub key: T::Key,
}

impl<T: BulkCopyInsert> BulkCopyInsertData<T> {
    #[instrument(skip(pool, self))]
    pub async fn bulk_insert(self, pool: &PgPool) -> Result<(), Error> {
        let already_exists: bool = T::exists_query(&self.key)
            .fetch_one(pool)
            .await
            .unwrap()
            .unwrap();
        if already_exists {
            warn!(
                "table already contains objects with identifiers {:?}; not persisting to database",
                self.key
            );
            return Ok(());
        }
        debug!("dropping index if any");
        T::drop_indexes_query().execute(pool).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let mut handle = conn.copy_in_raw(T::copy_in_statement()).await.unwrap();

        let mut buf: Vec<u8> = Vec::new();
        {
            let mut writer = csv::Writer::from_writer(&mut buf);
            for d in self.records.into_iter() {
                let rec = T::to_string_record(d);
                writer.write_record(&rec).unwrap();
            }
            writer.flush().unwrap();
        }
        debug!("serialized; sending");
        handle.send(buf).await.unwrap();
        debug!("sent");
        handle.finish().await.unwrap();
        debug!("recreating index");
        T::create_indexes_query().execute(pool).await.unwrap();
        debug!("running ANALYZE");
        T::analyze_query().execute(pool).await.unwrap();
        debug!("done");

        Ok(())
    }
}
