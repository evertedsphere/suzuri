use std::fmt::Debug;

use async_trait::async_trait;
use serde::Serialize;
use snafu::{ResultExt, Snafu};
use sqlx::{postgres::PgArguments, query::Query, Execute, PgConnection, Postgres};
use tracing::debug;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    PostgresCopyError { source: sqlx::Error },
    SerialisationError { source: serde_json::Error },
    CsvSerialisationError { source: csv::Error },
    CsvFinaliseError { source: std::io::Error },
}

/// This trait allows for efficient insertion of a batch of related data in bulk into a table
/// that may contain data already.
#[async_trait]
pub trait PgBulkInsert {
    /// A related type, usually a struct containing all but the primary key(s),
    /// that can be inserted into the database.
    type InsertFields: Send;

    /// horrible hack
    type SerializeAs: Serialize;

    async fn copy_records(conn: &mut PgConnection, records: Vec<Self::InsertFields>) -> Result<()> {
        let mut handle = conn
            .copy_in_raw(Self::copy_in_statement().sql())
            .await
            .context(PostgresCopyError)?;
        let buf = Self::to_string_record_vec(records)?;
        debug!("sending buffer of size {}", buf.len());
        handle.send(buf).await.context(PostgresCopyError)?;
        let num_rows = handle.finish().await.context(PostgresCopyError)?;
        debug!("rows affected = {}", num_rows);
        Ok(())
    }

    /// The `COPY IN ... STDIN` statement to use to begin the transfer.
    fn copy_in_statement() -> Query<'static, Postgres, PgArguments>;

    /// Convert an object into a [`csv::StringRecord`] for insertion.
    fn to_record(ins: Self::InsertFields) -> Result<Self::SerializeAs>;

    fn to_string_record_vec(records: Vec<Self::InsertFields>) -> Result<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut writer = csv::Writer::from_writer(&mut buf);
            for d in records.into_iter() {
                let rec = Self::to_record(d)?;
                writer.serialize(&rec).context(CsvSerialisationError)?;
            }
            writer.flush().context(CsvFinaliseError)?;
        }
        Ok(buf)
    }
}
