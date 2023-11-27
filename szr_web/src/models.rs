use crate::schema::terms;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = terms)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Term {
    pub term_id: i32,
    pub term_spelling: String,
    pub term_reading: String,
}

#[derive(Insertable)]
#[diesel(table_name = terms)]
pub struct NewTerm<'a> {
    pub term_spelling: &'a str,
    pub term_reading: &'a str,
}
