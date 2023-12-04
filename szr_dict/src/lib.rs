use serde::{Deserialize, Serialize};
// use sqlx::{types::Json, FromRow, PgPool, QueryBuilder};
use diesel::deserialize::FromSql;
use diesel::prelude::*;
use diesel::result::DatabaseErrorKind;
use diesel::serialize::ToSql;
use diesel::{deserialize::FromSqlRow, expression::AsExpression};
use diesel::{pg::Pg, sql_types::Jsonb};
use szr_diesel_macros::impl_sql_as_jsonb;
use szr_schema::defs;

#[derive(FromSqlRow, AsExpression, Deserialize, Debug, Clone, Serialize)]
#[diesel(sql_type = Jsonb)]
pub struct Definitions(pub Vec<String>);

impl_sql_as_jsonb!(Definitions);

#[derive(Queryable, Selectable, Debug, Clone)]
#[diesel(table_name = defs)]
#[diesel(check_for_backend(Pg))]
pub struct Def {
    pub id: i32,
    pub dict_name: String,
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

#[derive(Insertable, Clone)]
#[diesel(table_name = defs)]
pub struct NewDef {
    pub dict_name: String,
    pub spelling: String,
    pub reading: String,
    pub content: Definitions,
}

pub trait PersistDict {
    type Error: std::error::Error;
    fn read_dictionary(path: &str, name: &str) -> Result<Vec<NewDef>, Self::Error>;
}
