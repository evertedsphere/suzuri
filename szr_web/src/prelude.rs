pub use diesel::deserialize::FromSql;
pub use diesel::pg::Pg;
pub use diesel::result::DatabaseErrorKind;
pub use diesel::serialize::ToSql;
pub use serde::{Deserialize, Serialize};
pub use snafu::{prelude::*, Whatever};
pub use tracing::{debug, error, info, instrument, trace, warn};

pub type DieselError = diesel::result::Error;

pub fn diesel_error_kind<'a, A>(
    err: &'a Result<A, diesel::result::Error>,
) -> Option<&'a DatabaseErrorKind> {
    match err {
        Err(diesel::result::Error::DatabaseError(err_kind, _)) => Some(err_kind),
        _ => None,
    }
}

mod functions {
    // use super::types::*;
    use diesel::sql_function;
    use diesel::sql_types::*;

    sql_function! {
        fn jsonb_set(target: Jsonb, path: Array<Text>, new_value: Jsonb) -> Jsonb
    }
}

mod helper_types {
    pub type JsonbSet<A, B, C> = crate::prelude::functions::jsonb_set::HelperType<A, B, C>;
}

mod dsl {
    pub use crate::prelude::functions::*;
    pub use crate::prelude::helper_types::*;
}

pub use dsl::jsonb_set;
pub use dsl::JsonbSet;
