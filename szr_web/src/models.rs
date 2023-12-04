use diesel::{
    deserialize::FromSqlRow,
    expression::AsExpression,
    pg::{sql_types::Jsonb, Pg},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use szr_diesel_macros::{impl_sql_as_jsonb, impl_sql_newtype};
use szr_schema::terms;

#[derive(FromSqlRow, AsExpression, Deserialize, Debug, Serialize)]
#[diesel(sql_type = Jsonb)]
pub struct TermData {
    pub foo: String,
}

impl_sql_as_jsonb!(TermData);

impl_sql_newtype!(TermId, i32; Copy);

#[derive(Queryable, Selectable, Debug)]
#[diesel(table_name = terms)]
#[diesel(check_for_backend(Pg))]
pub struct Term {
    pub id: TermId,
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
