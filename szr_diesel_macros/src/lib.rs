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
