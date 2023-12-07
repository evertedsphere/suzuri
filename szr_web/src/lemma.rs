use std::path::Path;

use itertools::Itertools;
use snafu::{ResultExt, Snafu};
use sqlx::{types::Json, PgPool};
use szr_dict::{BulkCopyInsert, Def};
use szr_features::UnidicSession;
use szr_ja_utils::kata_to_hira;
use tracing::{debug, instrument, warn};

use crate::models::{Lemma, LemmaId, NewLemma};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(display("Lemma {id} is not in the database: {source}"))]
    LemmaNotFoundError {
        id: LemmaId,
        source: sqlx::Error,
    },
    #[snafu(display("Lemma {spelling} ({reading}) is not in the database: {source}"))]
    NoMatchingLemmaError {
        spelling: String,
        reading: String,
        source: sqlx::Error,
    },
    #[snafu(display("Lemma {spelling} ({reading}) already exists: {source}"))]
    LemmaAlreadyExistsError {
        spelling: String,
        reading: String,
        source: sqlx::Error,
    },
    #[snafu(display("Failed to bulk insert lemmas: {source}"))]
    LemmaInitError {
        source: sqlx::Error,
    },
    #[snafu(context(false))]
    MiscSqlxError {
        source: sqlx::Error,
    },
    EmptyResultError,
}

#[instrument(skip(pool, path), err)]
pub async fn import_unidic_lemmas(pool: &PgPool, path: impl AsRef<Path>) -> Result<()> {
    let mut tx = pool.begin().await?;

    let already_exists = sqlx::query_scalar!("SELECT EXISTS(SELECT 1 FROM lemmas)")
        .fetch_one(&mut *tx)
        .await?;

    match already_exists {
        Some(false) => {}
        Some(true) => {
            warn!("already imported, skipping");
            return Ok(());
        }
        None => {
            return EmptyResultError.fail();
        }
    }

    let unidic_terms = UnidicSession::all_terms(path).unwrap();
    let records: Vec<_> = unidic_terms
        .into_iter()
        .map(|term| {
            let (_ls, _lr, s, r) = term.surface_form();
            NewLemma {
                spelling: s,
                reading: r.chars().map(kata_to_hira).collect(),
            }
        })
        .sorted()
        .unique()
        .collect();

    sqlx::query!("DROP INDEX IF EXISTS lemmas_spelling_reading")
        .execute(&mut *tx)
        .await?;

    Lemma::copy_records(&mut *tx, records).await?;

    sqlx::query!("CREATE UNIQUE INDEX lemmas_spelling_reading ON lemmas (spelling, reading)")
        .execute(&mut *tx)
        .await?;

    sqlx::query!("ANALYZE lemmas").execute(&mut *tx).await?;

    tx.commit().await?;

    Ok(())
}

#[instrument(skip(pool), ret, err)]
pub async fn get_lemma_by_id<C>(pool: &PgPool, id: LemmaId) -> Result<Lemma> {
    sqlx::query_as!(
        Lemma,
        r#"SELECT id as "id: LemmaId", spelling, reading FROM lemmas WHERE id = $1"#,
        id.0
    )
    .fetch_one(pool)
    .await
    .context(LemmaNotFoundError { id })
}

#[instrument(skip(pool), err)]
pub async fn get_lemma_meanings(pool: &PgPool, id: LemmaId) -> Result<Vec<Def>> {
    let ret = sqlx::query_as!(
        Def,
        r#"SELECT
             lemmas.id, defs.dict_name, defs.spelling, defs.reading,
             defs.content as "content: Json<Vec<String>>"
           FROM lemmas INNER JOIN defs
           ON lemmas.spelling = defs.spelling
           AND lemmas.reading = defs.reading
           WHERE lemmas.id = $1
          "#,
        // FIXME
        id.0
    )
    .fetch_all(pool)
    .await?;

    Ok(ret)
}

#[instrument(skip(pool), err)]
pub async fn get_lemma(pool: &PgPool, spelling: &str, reading: &str) -> Result<Lemma> {
    let ret = sqlx::query_as!(
        Lemma,
        r#"SELECT id as "id: LemmaId", spelling, reading FROM lemmas WHERE spelling = $1 AND reading = $2"#,
        spelling,
        reading,
    )
    .fetch_one(pool)
    .await
    .context(NoMatchingLemmaError { spelling, reading })?;
    Ok(ret)
}
