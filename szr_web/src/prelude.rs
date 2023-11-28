use diesel::result::DatabaseErrorKind;
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
