use diesel::connection::LoadConnection;
use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind;
use snafu::ResultExt;

use crate::models::{NewTerm, Term, TermData};
use crate::prelude::*;
use szr_schema::terms;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(display("Term {id} is not in the database: {source}"))]
    TermNotFoundError { id: i32, source: DieselError },
    #[snafu(display("Term {spelling} ({reading}) is not in the database: {source}"))]
    NoMatchingTermError {
        spelling: String,
        reading: String,
        source: DieselError,
    },
    #[snafu(display("Term {spelling} ({reading}) already exists: {source}"))]
    TermAlreadyExistsError {
        spelling: String,
        reading: String,
        source: DieselError,
    },
    #[snafu(whatever, display("{message}: {source:?}"))]
    OtherError {
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
        spelling,
        reading,
        data: &TermData {
            foo: "".to_string(),
        },
    };
    let r = diesel::insert_into(terms::table)
        .values(&new_term)
        .returning(Term::as_returning())
        .get_result(conn);

    match diesel_error_kind(&r) {
        Some(DatabaseErrorKind::UniqueViolation) => {
            return r.context(TermAlreadyExistsError { spelling, reading });
        }
        _ => {
            return r.whatever_context("unknown error");
        }
    }
}

#[instrument(skip(conn), ret, err)]
pub fn get_term_by_id<C>(conn: &mut C, id: i32) -> Result<Term>
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
