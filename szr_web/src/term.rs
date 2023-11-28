use diesel::connection::LoadConnection;
use diesel::pg::Pg;
use diesel::prelude::*;
use snafu::ResultExt;

use crate::models::{NewTerm, Term};
use crate::prelude::*;
use crate::schema::terms;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    #[snafu(display("Term {id} is not in the database: {source}"))]
    UnknownTermError {
        id: i32,
        source: diesel::result::Error,
    },
    #[snafu(display("Term {spelling} ({reading}) already exists: {source}"))]
    TermAlreadyExistsError {
        spelling: String,
        reading: String,
        source: diesel::result::Error,
    },
    #[snafu(whatever, display("{message}: {source:?}"))]
    GenericError {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
        source: Option<Box<dyn std::error::Error>>,
    },
}

#[instrument(skip(conn), ret, err)]
pub fn create_term<C>(conn: &mut C, spelling: &str, reading: &str) -> Result<Term>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    let new_term = NewTerm {
        term_spelling: spelling,
        term_reading: reading,
    };
    let r = diesel::insert_into(terms::table)
        .values(&new_term)
        .returning(Term::as_returning())
        .get_result(conn)
        .context(TermAlreadyExists { spelling, reading })?;
    Ok(r)
}

#[instrument(skip(conn), ret, err)]
pub fn get_term<C>(conn: &mut C, id: i32) -> Result<Term>
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    use crate::schema::terms::dsl::*;
    let r = terms
        .filter(term_id.eq(id))
        .select(Term::as_select())
        .get_result(conn)
        .context(UnknownTerm { id })?;
    Ok(r)
}
