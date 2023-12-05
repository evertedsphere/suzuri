use diesel::{pg::Pg, prelude::*};
use szr_diesel_macros::impl_sql_newtype;
use szr_schema::lemmas;

impl_sql_newtype!(LemmaId, i32; Copy);

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = lemmas)]
#[diesel(check_for_backend(Pg))]
pub struct Lemma {
    pub id: LemmaId,
    pub spelling: String,
    pub reading: String,
}

#[derive(Insertable)]
#[diesel(table_name = lemmas)]
pub struct NewLemma<'a> {
    pub spelling: &'a str,
    pub reading: &'a str,
}
