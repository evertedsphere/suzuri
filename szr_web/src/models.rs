use crate::schema::terms;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = terms)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Term {
    pub id: i32,
    pub spelling: String,
    pub reading: String,
}

#[derive(Insertable)]
#[diesel(table_name = terms)]
pub struct NewTerm<'a> {
    pub spelling: &'a str,
    pub reading: &'a str,
}
