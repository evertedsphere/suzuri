#[macro_export]
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

pub fn diesel_error_kind<'a, A>(
    err: &'a Result<A, diesel::result::Error>,
) -> Option<&'a diesel::result::DatabaseErrorKind> {
    match err {
        Err(diesel::result::Error::DatabaseError(err_kind, _)) => Some(err_kind),
        _ => None,
    }
}

mod functions {
    use diesel::{sql_function, sql_types::*};

    sql_function! {
        fn jsonb_set(target: Jsonb, path: Array<Text>, new_value: Jsonb) -> Jsonb
    }
}

mod helper_types {
    pub type JsonbSet<A, B, C> = crate::functions::jsonb_set::HelperType<A, B, C>;
}

mod dsl {
    pub use crate::{functions::*, helper_types::*};
}

pub use crate::dsl::{jsonb_set, JsonbSet};
