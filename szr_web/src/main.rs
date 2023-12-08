mod handlers;
mod lemma;

use std::{env, str::FromStr};

use axum::{routing::get, Router};
use snafu::{ResultExt, Snafu};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use szr_yomichan::Yomichan;
use tower_http::services::ServeDir;
use tracing::{debug, info, instrument};
use tracing_subscriber::fmt::format::FmtSpan;

use crate::lemma::import_unidic;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    UnsetEnvironmentVariable {
        source: std::env::VarError,
    },
    YomichanDeserialisationFailed {
        source: szr_yomichan::Error,
    },
    #[snafu(context(false))]
    IoError {
        source: std::io::Error,
    },
    #[snafu(context(suffix(Error)))]
    PersistenceError {
        source: lemma::Error,
    },
    #[snafu(context(false))]
    SqlxError {
        source: sqlx::Error,
    },
    #[snafu(context(false))]
    RubyError {
        source: szr_ruby::Error,
    },
}

async fn init_database() -> Result<sqlx::PgPool> {
    info!("connecting to database");
    let url = env::var("DATABASE_URL").context(UnsetEnvironmentVariable)?;
    let conn_opts = PgConnectOptions::from_str(&url)?;
    // .log_statements(tracing::log::LevelFilter::Trace)
    // .log_slow_statements(tracing::log::LevelFilter::Warn, Duration::from_millis(10))
    // .disable_statement_logging();

    let pool = PgPoolOptions::default()
        .max_connections(24)
        .min_connections(2)
        .test_before_acquire(true)
        .connect_with(conn_opts)
        .await?;

    info!("running migrations");
    // sqlx::migrate!("../migrations")
    //     .run(&pool)
    //     .await
    //     .context("running migrations")?;
    info!("ran migrations");
    Ok(pool)
}

#[instrument(skip(pool))]
async fn init_dictionaries(pool: &PgPool) -> Result<()> {
    let unidic_path = "data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    let yomichan_dicts = vec![
        ("input/jmdict_en", "JMdict"),
        ("input/jmnedict", "JMnedict"),
        ("input/pixiv_summaries", "dic.pixiv.net"),
        ("input/oubunsha", "旺文社"),
    ];

    // This can be parallelised with [`try_join_all!`] or similar, but it's not
    // something you run every time you start the application unless you're
    // debugging this specific part of the code, which is exactly when you don't
    // want this to complicate matters. (Plus, doing that seems to mess up the
    // traces for some reason.)

    import_unidic(&pool, unidic_path)
        .await
        .context(PersistenceError)?;

    Yomichan::import_all(&pool, yomichan_dicts)
        .await
        .context(YomichanDeserialisationFailed)?;

    Ok(())
}

#[snafu::report]
#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let _kd = szr_ruby::read_kanjidic("data/system/readings.json")?;
    let pool = init_database().await?;

    init_dictionaries(&pool).await?;

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/books/view/:name", get(handlers::handle_books_view))
        .route("/words/view/:id", get(handlers::handle_lemmas_view))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(pool);

    let addr = "0.0.0.0:34344";
    info!(addr, "starting axum");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Initialise the [`tracing`] library with setup appropriate for this
/// application.
fn init_tracing() -> Result<()> {
    use time::{macros::format_description, UtcOffset};
    use tracing::metadata::LevelFilter;
    use tracing_subscriber::{
        filter::{self, FilterExt},
        fmt::time::OffsetTime,
        prelude::*,
    };

    // this is safe
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
    Ok(())
}
