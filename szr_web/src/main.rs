pub mod models;
pub mod prelude;
pub mod schema;
pub mod term;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use prelude::*;
use std::env;
use szr_diesel_logger::LoggingConnection;
use test_log::test;

use crate::term::{create_term, get_term};

fn main() {
    init_tracing();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let conn_inner = PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));
    let mut conn = LoggingConnection::new(conn_inner);

    let spelling = "abc";
    let reading = "def";

    let _ = create_term(&mut conn, spelling, reading);
    let r = get_term(&mut conn, 1);
    debug!("{r:?}");
}

#[test]
fn test_result() -> Result<(), String> {
    info!("testing tracing");
    Ok(())
}

/// Initialise the [`tracing`] library with setup appropriate for this application.
fn init_tracing() {
    use time::{macros::format_description, UtcOffset};
    use tracing::metadata::LevelFilter;
    use tracing_subscriber::{
        filter::{self, FilterExt},
        fmt::time::OffsetTime,
        prelude::*,
    };

    let offset = UtcOffset::current_local_offset().expect("failed to get local offset");
    let timer = OffsetTime::new(
        offset,
        format_description!("[hour]:[minute]:[second].[subsecond digits:3]"),
    );

    let mut tracing_layers = Vec::new();
    let fmt_layer = tracing_subscriber::fmt::layer()
        // .with_span_events(FmtSpan::CLOSE | FmtSpan::NEW)
        .with_timer(timer)
        .with_level(true)
        .pretty()
        .with_target(true)
        .with_file(false)
        .with_line_number(false)
        .with_filter(
            filter::filter_fn(|meta| meta.target() != "tracing_actix_web::root_span_builder")
                .and(LevelFilter::DEBUG),
        )
        .boxed();
    tracing_layers.push(fmt_layer);
    tracing_subscriber::registry().with(tracing_layers).init();
    debug!("tracing initialised");
}
