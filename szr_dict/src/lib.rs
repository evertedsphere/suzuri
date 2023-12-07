use std::fmt::Debug;

use async_trait::async_trait;
use csv::StringRecord;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use sqlx::{
    postgres::PgArguments,
    query::{Query, QueryScalar},
    types::Json,
    Execute, PgPool, Postgres,
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
pub trait BulkCopyInsert: Sized {
    /// This key is used when checking to see if this data is already in the table,
    /// in [`exists_query`] below.
    type Key: Debug;

    /// A related type, usually a struct containing all but the primary key(s),
    /// that can be inserted into the database.
    type InsertFields;

    /// The `COPY IN ... STDIN` statement to use to begin the transfer.
    fn copy_in_statement() -> Query<'static, Postgres, PgArguments>;

    /// A single (for now) query that creates an index on the table.
    fn create_indexes_query() -> Query<'static, Postgres, PgArguments>;

    /// A single (for now) query that drops an index on the table.
    fn drop_indexes_query() -> Query<'static, Postgres, PgArguments>;

    /// The `ANALYZE` query to run. The default implementation runs a database-wide unqualified
    /// `ANALYZE`.
    fn analyze_query() -> Query<'static, Postgres, PgArguments> {
        sqlx::query!("ANALYZE")
    }

    /// A query to use to check if we should skip inserting this batch because it has likely
    /// already been inserted into the database.
    fn exists_query(key: &Self::Key) -> QueryScalar<'static, Postgres, Option<bool>, PgArguments>;

    /// Convert an object into a [`csv::StringRecord`] for insertion.
    fn to_string_record(ins: Self::InsertFields) -> StringRecord;

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

impl<T: BulkCopyInsert> BulkCopyInsertData<T> {
    #[instrument(skip(pool, self))]
    pub async fn bulk_insert(self, pool: &PgPool) -> Result<(), Error> {
        let mut tx = pool.begin().await.unwrap();
        let already_exists: bool = T::exists_query(&self.key)
            .fetch_one(&mut *tx)
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
        T::drop_indexes_query().execute(&mut *tx).await.unwrap();
        let mut handle = tx.copy_in_raw(T::copy_in_statement().sql()).await.unwrap();

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
        T::create_indexes_query().execute(&mut *tx).await.unwrap();
        debug!("running ANALYZE");
        T::analyze_query().execute(&mut *tx).await.unwrap();
        debug!("done");

        Ok(())
    }
}
