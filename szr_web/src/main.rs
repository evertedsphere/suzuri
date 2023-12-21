mod handlers;
mod models;

use std::{env, str::FromStr, time::Duration};

use axum::{routing::get, Router};
use snafu::{ResultExt, Snafu};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions, PgPool,
};
use szr_features::UnidicSession;
use szr_yomichan::Yomichan;
use tower_http::services::ServeDir;
use tracing::{debug, info, instrument};
use tracing_subscriber::fmt::format::FmtSpan;

use crate::models::import_unidic;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    // Reading data
    YomichanImportFailed { source: szr_yomichan::Error },
    UnidicImportFailed { source: models::Error },
    KanjidicLoadingFailed { source: szr_ruby::Error },
    // Database
    UnsetEnvironmentVariable { source: std::env::VarError },
    InvalidPgConnectionString { source: sqlx::Error },
    PgConnectionFailed { source: sqlx::Error },
    // Server
    FailedToBindPort { source: std::io::Error },
    CouldNotStartAxum { source: std::io::Error },
}

async fn init_database() -> Result<sqlx::PgPool> {
    info!("connecting to database");
    let url = env::var("DATABASE_URL").context(UnsetEnvironmentVariable)?;
    let conn_opts = PgConnectOptions::from_str(&url)
        .context(InvalidPgConnectionString)?
        .log_statements(tracing::log::LevelFilter::Trace)
        .log_slow_statements(tracing::log::LevelFilter::Warn, Duration::from_millis(100))
        .disable_statement_logging();

    let pool = PgPoolOptions::default()
        .max_connections(24)
        .min_connections(2)
        .test_before_acquire(true)
        .connect_with(conn_opts)
        .await
        .context(PgConnectionFailed)?;

    // info!("running migrations");
    // sqlx::migrate!("/home/s/c/szr/migrations")
    //     .run(&pool)
    //     .await
    //     .context("running migrations")?;
    // info!("ran migrations");
    Ok(pool)
}

#[instrument(skip(pool))]
async fn init_dictionaries(pool: &PgPool) -> Result<()> {
    let unidic_path = "data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    let yomichan_dicts = vec![
        ("/home/s/c/szr/input/jmdict_en", "JMdict"),
        ("/home/s/c/szr/input/jmnedict", "JMnedict"),
        ("/home/s/c/szr/input/pixiv_summaries", "dic.pixiv.net"),
        ("/home/s/c/szr/input/oubunsha", "旺文社"),
    ];

    // This can be parallelised with [`try_join_all!`] or similar, but it's not
    // something you run every time you start the application unless you're
    // debugging this specific part of the code, which is exactly when you don't
    // want this to complicate matters. (Plus, doing that seems to mess up the
    // traces for some reason.)

    Yomichan::import_all(&pool, yomichan_dicts)
        .await
        .context(YomichanImportFailed)?;

    import_unidic(&pool, unidic_path)
        .await
        .context(UnidicImportFailed)?;

    Ok(())
}

#[snafu::report]
#[tokio::main]
async fn main() -> Result<()> {
    init_tracing()?;

    let _kd =
        szr_ruby::read_kanjidic("data/system/readings.json").context(KanjidicLoadingFailed)?;
    let pool = init_database().await?;

    init_dictionaries(&pool).await?;

    let mut session = UnidicSession::new().unwrap();

    let input_files = glob::glob(&format!("input/*.epub"))
        .unwrap()
        .collect::<Vec<_>>();
    for f in input_files {
        let f = f.unwrap();
        szr_epub::Book::import_from_file(&pool, &mut session, f)
            .await
            .unwrap();
    }

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route("/books/view/:name", get(handlers::handle_books_view))
        .route("/words/view/:id", get(handlers::handle_lemmas_view))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(pool);

    let addr = "0.0.0.0:34344";
    info!(addr, "starting axum");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context(FailedToBindPort)?;
    axum::serve(listener, app)
        .await
        .context(CouldNotStartAxum)?;

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
