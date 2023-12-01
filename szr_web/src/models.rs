use crate::prelude::*;
use diesel::pg::sql_types::Jsonb;
use szr_diesel_macros::impl_sql_as_jsonb;
use szr_schema::terms;

use diesel::{prelude::*, AsExpression, FromSqlRow};

#[derive(FromSqlRow, AsExpression, Deserialize, Debug, Serialize)]
#[diesel(sql_type = Jsonb)]
pub struct TermData {
    pub foo: String,
}

impl_sql_as_jsonb!(TermData);

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = terms)]
#[diesel(check_for_backend(Pg))]
pub struct Term {
    pub term_id: i32,
    pub term_spelling: String,
    pub term_reading: String,
    pub term_data: TermData,
}

#[derive(Insertable)]
#[diesel(table_name = terms)]
pub struct NewTerm<'a> {
    pub term_spelling: &'a str,
    pub term_reading: &'a str,
    pub term_data: &'a TermData,
}
