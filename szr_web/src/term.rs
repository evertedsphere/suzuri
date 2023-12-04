use diesel::{
    connection::LoadConnection, pg::Pg, result::DatabaseErrorKind, Connection, ExpressionMethods,
    QueryDsl, RunQueryDsl, SelectableHelper,
};
use snafu::{ResultExt, Snafu};
use szr_diesel_macros::diesel_error_kind;
use szr_schema::terms;
use tracing::{debug, instrument};

use crate::models::{NewTerm, Term, TermData, TermId};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(display("Term {id} is not in the database: {source}"))]
    TermNotFoundError {
        id: TermId,
        source: diesel::result::Error,
    },
    #[snafu(display("Term {spelling} ({reading}) is not in the database: {source}"))]
    NoMatchingTermError {
        spelling: String,
        reading: String,
        source: diesel::result::Error,
    },
    #[snafu(display("Term {spelling} ({reading}) already exists: {source}"))]
    TermAlreadyExistsError {
        spelling: String,
        reading: String,
        source: diesel::result::Error,
    },
    #[snafu(display("Failed to bulk insert terms: {source}"))]
    TermInitError { source: diesel::result::Error },
    // #[snafu(whatever, display("{message}: {source:?}"))]
    // OtherError {
    //     message: String,
    //     #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
    //     source: Option<Box<dyn std::error::Error>>,
    // },
}

#[instrument(skip(conn, data), err)]
pub fn create_terms<C>(conn: &mut C, data: &[(String, String)]) -> Result<()>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let empty = TermData {
        foo: "".to_string(),
    };
    let new_terms: Vec<NewTerm> = data
        .into_iter()
        .map(|(spelling, reading)| NewTerm {
            spelling,
            reading,
            data: &empty,
        })
        .collect();

    let r = diesel::insert_into(terms::table)
        .values(&new_terms)
        .on_conflict_do_nothing()
        .execute(conn)
        .context(TermInitError)?;

    debug!("inserted {} terms", r);

    Ok(())

    // match diesel_error_kind(&r) {
    //     Some(DatabaseErrorKind::UniqueViolation) => {
    //         return r.context(TermAlreadyExistsError { spelling, reading });
    //     }
    //     _ => {
    //         unimplemented!()
    //         // return r.whatever_context("unknown error");
    //     }
    // }
}

#[instrument(skip(conn), ret, err)]
pub fn create_term<C>(conn: &mut C, spelling: &str, reading: &str) -> Result<()>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    create_terms(conn, &[(spelling.to_owned(), reading.to_owned())])
}

#[instrument(skip(conn), ret, err)]
pub fn get_term_by_id<C>(conn: &mut C, id: TermId) -> Result<Term>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let r = terms::table
        .filter(terms::id.eq(id))
        .select(Term::as_select())
        .get_result(conn)
        .context(TermNotFoundError { id })?;
    Ok(r)
}

#[instrument(skip(conn), ret, err)]
pub fn get_term<C>(conn: &mut C, spelling: &str, reading: &str) -> Result<Term>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let r = terms::table
        .filter(terms::spelling.eq(spelling))
        .filter(terms::reading.eq(reading))
        .select(Term::as_select())
        .get_result(conn)
        .context(NoMatchingTermError { spelling, reading })?;
    Ok(r)
}
