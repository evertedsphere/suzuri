mod handlers;
mod lemma;
mod models;

use std::{env, str::FromStr};

use axum::{routing::get, Router};
use snafu::{ResultExt, Whatever};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use szr_dict::DictionaryFormat;
use szr_yomichan::Yomichan;
use tower_http::services::ServeDir;
use tracing::{debug, info};
use tracing_subscriber::fmt::format::FmtSpan;

use crate::lemma::import_unidic_lemmas;

async fn init_database() -> Result<sqlx::PgPool, Whatever> {
    info!("connecting to database");
    let url = env::var("DATABASE_URL").whatever_context("??")?;
    let conn_opts = PgConnectOptions::from_str(&url).unwrap();
    // .log_statements(tracing::log::LevelFilter::Trace)
    // .log_slow_statements(tracing::log::LevelFilter::Warn, Duration::from_millis(10))
    // .disable_statement_logging();

    let pool = PgPoolOptions::default()
        .max_connections(24)
        .min_connections(2)
        .test_before_acquire(true)
        .connect_with(conn_opts)
        .await
        .whatever_context("initialising pool")?;

    info!("running migrations");
    // sqlx::migrate!("../migrations")
    //     .run(&pool)
    //     .await
    //     .context("running migrations")?;
    info!("ran migrations");
    Ok(pool)
}

#[snafu::report]
#[tokio::main]
async fn main() -> Result<(), Whatever> {
    init_tracing();

    let _kd = szr_ruby::read_kanjidic("data/system/readings.json").whatever_context("kanjidic")?;

    let unidic_path = "data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    let sqlx_pool = init_database().await?;

    let dict =
        Yomichan::read_from_path("input/jmdict_en", "jmdict_en").whatever_context("read dict")?;

    dict.bulk_insert(&sqlx_pool).await.unwrap();

    import_unidic_lemmas(&sqlx_pool, unidic_path).await.unwrap();

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/books/view/:name", get(handlers::handle_books_view))
        .route("/lemmas/view/:id", get(handlers::handle_lemmas_view))
        .nest_service("/static", ServeDir::new("static"))
        // .with_state(pool)
        .with_state(sqlx_pool);

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
        .with_span_events(FmtSpan::CLOSE)
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
