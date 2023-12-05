mod handlers;
mod lemma;
mod models;

use std::env;

use axum::{routing::get, Router};
use snafu::{ResultExt, Whatever};
use szr_dict::DictionaryFormat;
use szr_yomichan::Yomichan;
use tower_http::services::ServeDir;
use tracing::{debug, info};

use crate::lemma::import_unidic_lemmas;

#[snafu::report]
#[tokio::main]
async fn main() -> Result<(), Whatever> {
    init_tracing();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    // let conn_inner = PgConnection::establish(&database_url)
    //     .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));
    // let mut conn = LoggingConnection::new(conn_inner);

    let manager =
        deadpool_diesel::postgres::Manager::new(database_url, deadpool_diesel::Runtime::Tokio1);
    let pool = deadpool_diesel::postgres::Pool::builder(manager)
        .build()
        .unwrap();

    let _kd = szr_ruby::read_kanjidic("data/system/readings.json").whatever_context("kanjidic")?;

    let unidic_path = "data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    let conn = pool.get().await.unwrap();
    let dict =
        Yomichan::read_from_path("input/jmdict_en", "jmdict_en").whatever_context("read dict")?;
    conn.interact(move |conn| {
        Yomichan::save_dictionary(conn, "jmdict_en", dict.clone())
            // .whatever_context("persist dict")
            .unwrap();

        import_unidic_lemmas(conn, unidic_path).unwrap();
    })
    .await
    .unwrap();

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/books/view/:name", get(handlers::handle_books_view))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(pool);

    let addr = "0.0.0.0:34344";
    info!(addr, "starting axum");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .whatever_context("failed to bind port")?;
    axum::serve(listener, app)
        .await
        .whatever_context("axum could not start")?;

    Ok(())
}

/// Initialise the [`tracing`] library with setup appropriate for this
/// application.
fn init_tracing() {
    use time::{macros::format_description, UtcOffset};
    use tracing::metadata::LevelFilter;
    use tracing_subscriber::{
        filter::{self, FilterExt},
        fmt::time::OffsetTime,
        prelude::*,
    };

    let offset = UtcOffset::from_hms(1, 0, 0).unwrap();
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
