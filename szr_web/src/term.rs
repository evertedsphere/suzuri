use diesel::connection::LoadConnection;
use diesel::pg::Pg;
use diesel::prelude::*;

use crate::models::{NewTerm, Term};
use crate::prelude::*;
use crate::schema::terms;

#[instrument(skip(conn), ret)]
pub fn create_term<C>(conn: &mut C, spelling: &str, reading: &str) -> Term
where
    C: Connection<Backend = Pg> + LoadConnection,
{
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

#[instrument(skip(conn), ret)]
pub fn get_term<C>(conn: &mut C, id: i32) -> Term
where
    C: Connection<Backend = Pg> + LoadConnection,
{
    use crate::schema::terms::dsl::*;
    let r = terms
        .filter(term_id.eq(id))
        .select(Term::as_select())
        .get_result(conn)
        .expect("error");
    r
}
