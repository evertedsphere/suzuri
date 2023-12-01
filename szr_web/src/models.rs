use crate::prelude::*;
use diesel::pg::sql_types::Jsonb;
use szr_schema::terms;

use diesel::{prelude::*, AsExpression, FromSqlRow};

#[derive(FromSqlRow, AsExpression, Deserialize, Debug, Serialize)]
#[diesel(sql_type = Jsonb)]
pub struct TermData {
    pub foo: String,
}

macro_rules! impl_sql_as_jsonb {
    ($name: ident) => {
        impl ::diesel::deserialize::FromSql<::diesel::sql_types::Jsonb, ::diesel::pg::Pg>
            for $name
        {
            fn from_sql(bytes: ::diesel::pg::PgValue) -> diesel::deserialize::Result<Self> {
                let value = <::serde_json::Value as ::diesel::deserialize::FromSql<
                    ::diesel::pg::sql_types::Jsonb,
                    ::diesel::pg::Pg,
                >>::from_sql(bytes)?;
                Ok(::serde_json::from_value(value)?)
            }
        }
        impl ::diesel::serialize::ToSql<::diesel::pg::sql_types::Jsonb, ::diesel::pg::Pg>
            for $name
        {
            fn to_sql(
                &self,
                out: &mut ::diesel::serialize::Output<::diesel::pg::Pg>,
            ) -> ::diesel::serialize::Result {
                let value = ::serde_json::to_value(self)?;
                let mut out = out.reborrow();
                <::serde_json::Value as ::diesel::serialize::ToSql<
                    ::diesel::sql_types::Jsonb,
                    ::diesel::pg::Pg,
                >>::to_sql(&value, &mut out)
            }
        }
    };
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
