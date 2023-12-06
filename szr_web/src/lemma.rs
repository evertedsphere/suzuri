use std::path::Path;

use itertools::Itertools;
use snafu::{ResultExt, Snafu};
use sqlx::{types::Json, PgPool};
use szr_dict::Def;
use szr_features::UnidicSession;
use szr_ja_utils::kata_to_hira;
use tracing::{instrument, warn};

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
    // #[snafu(whatever, display("{message}: {source:?}"))]
    // OtherError {
    //     message: String,
    //     #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
    //     source: Option<Box<dyn std::error::Error>>,
    // },
}

#[instrument(skip(conn, path), err)]
pub fn import_unidic_lemmas(conn: &PgPool, path: impl AsRef<Path>) -> Result<()> {
    // let already_exists = diesel::select(exists(lemmas::table.select(lemmas::id)))
    //     .get_result(conn)
    //     .context(LemmaInitError)?;

    // if already_exists {
    //     warn!("unidic lemmas already imported; not persisting to database");
    //     return Ok(());
    // }

    // let unidic_terms = UnidicSession::all_terms(path).unwrap();

    // let v: Vec<(String, String)> = unidic_terms
    //     .into_iter()
    //     .map(|term| {
    //         let (_ls, _lr, s, r) = term.surface_form();
    //         (s, r.chars().map(kata_to_hira).collect())
    //     })
    //     .collect();

    // v.into_iter().chunks(10000).into_iter().for_each(|chunk| {
    //     let chunk: Vec<(String, String)> = chunk.collect();
    //     create_lemmas(conn, &chunk).unwrap();
    // });

    Ok(())
}

#[instrument(skip(conn, data), err)]
pub fn create_lemmas(conn: &PgPool, data: &[(String, String)]) -> Result<()> {
    // let new_lemmas: Vec<NewLemma> = data
    //     .into_iter()
    //     .map(|(spelling, reading)| NewLemma { spelling, reading })
    //     .collect();

    // let _r = diesel::insert_into(lemmas::table)
    //     .values(&new_lemmas)
    //     .on_conflict_do_nothing()
    //     .execute(conn)
    //     .context(LemmaInitError)?;

    Ok(())
}

#[instrument(skip(conn), ret, err)]
pub fn create_lemma(conn: &PgPool, spelling: &str, reading: &str) -> Result<()> {
    create_lemmas(conn, &[(spelling.to_owned(), reading.to_owned())])
}

#[instrument(skip(pool), ret, err)]
pub fn get_lemma_by_id<C>(pool: &PgPool, id: LemmaId) -> Result<Lemma> {
    let r = unimplemented!();
    // let r = lemmas::table
    //     .filter(lemmas::id.eq(id))
    //     .select(Lemma::as_select())
    //     .get_result(conn)
    //     .context(LemmaNotFoundError { id })?;
    Ok(r)
}

#[instrument(skip(pool), err)]
pub async fn get_lemma_meanings(pool: &PgPool, id: LemmaId) -> Result<Vec<Def>> {
    let ret = sqlx::query_as!(
        Def,
        r#"SELECT lemmas.id, defs.dict_name, defs.spelling, defs.reading,
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
