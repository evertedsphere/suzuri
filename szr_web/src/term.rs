use diesel::prelude::*;

use crate::models::{NewTerm, Term};
use crate::prelude::*;
use crate::schema::terms;

#[instrument(skip(conn))]
pub fn create_term(conn: &mut PgConnection, spelling: &str, reading: &str) -> Term {
    let new_term = NewTerm {
        term_spelling: spelling,
        term_reading: reading,
    };
    diesel::insert_into(terms::table)
        .values(&new_term)
        .returning(Term::as_returning())
        .get_result(conn)
        .expect("error saving post")
}

#[instrument(skip(conn))]
pub fn get_term(conn: &mut PgConnection, wanted_id: i32) -> Term {
    use crate::schema::terms::dsl::*;
    let r = terms
        .filter(term_id.eq(id))
        .select(Term::as_select())
        .get_result(conn)
        .expect("error");
    r
}
