use std::path::Path;

use diesel::{
    connection::LoadConnection, dsl::exists, pg::Pg, BoolExpressionMethods, Connection,
    ExpressionMethods, JoinOnDsl, QueryDsl, RunQueryDsl, SelectableHelper,
};
use itertools::Itertools;
use snafu::{ResultExt, Snafu};
use szr_dict::Def;
use szr_features::UnidicSession;
use szr_ja_utils::kata_to_hira;
use szr_schema::{defs, lemmas};
use tracing::{instrument, warn};

use crate::models::{Lemma, LemmaId, NewLemma};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(display("Lemma {id} is not in the database: {source}"))]
    LemmaNotFoundError {
        id: LemmaId,
        source: diesel::result::Error,
    },
    #[snafu(display("Lemma {spelling} ({reading}) is not in the database: {source}"))]
    NoMatchingLemmaError {
        spelling: String,
        reading: String,
        source: diesel::result::Error,
    },
    #[snafu(display("Lemma {spelling} ({reading}) already exists: {source}"))]
    LemmaAlreadyExistsError {
        spelling: String,
        reading: String,
        source: diesel::result::Error,
    },
    #[snafu(display("Failed to bulk insert lemmas: {source}"))]
    LemmaInitError { source: diesel::result::Error },
    // #[snafu(whatever, display("{message}: {source:?}"))]
    // OtherError {
    //     message: String,
    //     #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
    //     source: Option<Box<dyn std::error::Error>>,
    // },
}

#[instrument(skip(conn, path), err)]
pub fn import_unidic_lemmas<C>(conn: &mut C, path: impl AsRef<Path>) -> Result<()>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let already_exists = diesel::select(exists(lemmas::table.select(lemmas::id)))
        .get_result(conn)
        .context(LemmaInitError)?;

    if already_exists {
        warn!("unidic lemmas already imported; not persisting to database");
        return Ok(());
    }

    let unidic_terms = UnidicSession::all_terms(path).unwrap();

    let v: Vec<(String, String)> = unidic_terms
        .into_iter()
        .map(|term| {
            let (_ls, _lr, s, r) = term.surface_form();
            (s, r.chars().map(kata_to_hira).collect())
        })
        .collect();

    v.into_iter().chunks(10000).into_iter().for_each(|chunk| {
        let chunk: Vec<(String, String)> = chunk.collect();
        create_lemmas(conn, &chunk).unwrap();
    });

    Ok(())
}

#[instrument(skip(conn, data), err)]
pub fn create_lemmas<C>(conn: &mut C, data: &[(String, String)]) -> Result<()>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let new_lemmas: Vec<NewLemma> = data
        .into_iter()
        .map(|(spelling, reading)| NewLemma { spelling, reading })
        .collect();

    let _r = diesel::insert_into(lemmas::table)
        .values(&new_lemmas)
        .on_conflict_do_nothing()
        .execute(conn)
        .context(LemmaInitError)?;

    Ok(())
}

#[instrument(skip(conn), ret, err)]
pub fn create_lemma<C>(conn: &mut C, spelling: &str, reading: &str) -> Result<()>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    create_lemmas(conn, &[(spelling.to_owned(), reading.to_owned())])
}

#[instrument(skip(conn), ret, err)]
pub fn get_lemma_by_id<C>(conn: &mut C, id: LemmaId) -> Result<Lemma>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let r = lemmas::table
        .filter(lemmas::id.eq(id))
        .select(Lemma::as_select())
        .get_result(conn)
        .context(LemmaNotFoundError { id })?;
    Ok(r)
}

#[instrument(skip(conn), err)]
pub fn get_lemma<C>(conn: &mut C, spelling: &str, reading: &str) -> Result<Lemma>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let r = lemmas::table
        .filter(lemmas::spelling.eq(spelling))
        .filter(lemmas::reading.eq(reading))
        .select(Lemma::as_select())
        .get_result(conn)
        .context(NoMatchingLemmaError { spelling, reading })?;
    Ok(r)
}

#[instrument(skip(conn), err)]
pub fn get_lemma_meanings<C>(conn: &mut C, id: LemmaId) -> Result<Vec<Def>>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let r = lemmas::table
        .inner_join(
            defs::table.on(lemmas::spelling
                .eq(defs::spelling)
                .and(lemmas::reading.eq(defs::reading))),
        )
        .filter(lemmas::id.eq(id))
        .select(Def::as_select())
        .get_results(conn)
        .context(LemmaNotFoundError { id })?;
    Ok(r)
}
