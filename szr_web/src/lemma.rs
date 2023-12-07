use std::path::Path;

use itertools::Itertools;
use snafu::{ResultExt, Snafu};
use sqlx::{types::Json, PgPool};
use szr_dict::{BulkCopyInsert, Def};
use szr_features::UnidicSession;
use szr_ja_utils::kata_to_hira;
use tracing::{debug, instrument};

use crate::models::{Lemma, LemmaId, NewLemma};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(display("Lemma {id} is not in the database: {source}"))]
    LemmaNotFoundError { id: LemmaId, source: sqlx::Error },
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
    LemmaInitError { source: sqlx::Error },
}

#[instrument(skip(conn, path), err)]
pub async fn import_unidic_lemmas(conn: &PgPool, path: impl AsRef<Path>) -> Result<()> {
    let unidic_terms = UnidicSession::all_terms(path).unwrap();
    let v: Vec<_> = unidic_terms
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
    debug!("inserting lemmas");
    create_lemmas(conn, v).await?;
    Ok(())
}

#[instrument(skip(conn, new_lemmas), err)]
pub async fn create_lemmas(conn: &PgPool, new_lemmas: Vec<NewLemma>) -> Result<()> {
    Lemma::build_bulk_insert_batch((), new_lemmas)
        .bulk_insert(conn)
        .await
        .unwrap();
    Ok(())
}

#[instrument(skip(conn), ret, err)]
pub async fn create_lemma(conn: &PgPool, spelling: String, reading: String) -> Result<()> {
    create_lemmas(conn, vec![NewLemma { spelling, reading }]).await
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
    .await
    .unwrap();

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
