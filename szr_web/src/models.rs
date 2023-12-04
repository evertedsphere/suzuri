use crate::prelude::*;
use diesel::pg::sql_types::Jsonb;
use szr_diesel_macros::impl_sql_as_jsonb;
use szr_schema::terms;

use diesel::{deserialize::FromSqlRow, expression::AsExpression, prelude::*};

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
    pub id: i32,
    pub spelling: String,
    pub reading: String,
    pub data: TermData,
}

#[derive(Insertable)]
#[diesel(table_name = terms)]
pub struct NewTerm<'a> {
    pub spelling: &'a str,
    pub reading: &'a str,
    pub data: &'a TermData,
}
