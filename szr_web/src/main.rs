#![feature(let_chains, iter_intersperse, slice_group_by)]
mod handlers;
mod models;

use std::{env, str::FromStr, time::Duration};

use axum::{
    routing::{get, post},
    Router,
};
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

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(false)))]
pub enum Error {
    MigrationFailed { source: sqlx::migrate::MigrateError },
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
        .log_slow_statements(
            tracing::log::LevelFilter::Warn,
            Duration::from_millis(15000),
        );

    let pool = PgPoolOptions::default()
        .max_connections(24)
        .min_connections(2)
        .test_before_acquire(true)
        .connect_with(conn_opts)
        .await
        .context(PgConnectionFailed)?;

    MIGRATOR.run(&pool).await.context(MigrationFailed)?;
    Ok(pool)
}

#[instrument(skip(pool), level = "debug")]
async fn init_dictionaries(pool: &PgPool) -> Result<()> {
    let unidic_path = "/home/s/c/szr/data/system/unidic-cwj-3.1.0/lex_3_1.csv";
    let user_dict_path = "/home/s/c/szr/data/user/auto_dictionary.csv";
    let yomichan_dicts = vec![
        ("/home/s/c/szr/input/jmdict_en", "JMdict"),
        ("/home/s/c/szr/input/jmnedict", "JMnedict"),
        // ("/home/s/c/szr/input/pixiv_summaries", "dic.pixiv.net"),
        ("/home/s/c/szr/input/oubunsha", "旺文社"),
    ];

    // This can be parallelised with [`try_join_all!`] or similar, but it's not
    // something you run every time you start the application unless you're
    // debugging this specific part of the code, which is exactly when you don't
    // want this to complicate matters. (Plus, doing that seems to mess up the
    // traces for some reason.)

    import_unidic(&pool, unidic_path, Some(user_dict_path))
        .await
        .context(UnidicImportFailed)?;

    Yomichan::bulk_import_dicts(&pool, yomichan_dicts)
        .await
        .context(YomichanImportFailed)?;

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

    let mut session =
        UnidicSession::new("data/user/auto_dictionary.csv").expect("cannot open auto dictionary");

    let input_files = glob::glob(&format!("input/epub/*.epub"))
        .unwrap()
        .filter_map(|x| x.ok())
        .collect::<Vec<_>>();
    szr_epub::Book::import_from_files(&pool, &mut session, input_files)
        .await
        .expect("import failed");

    let app = Router::new()
        .route("/", get(handlers::handle_index))
        .route(
            "/books/:id/view/page/:page",
            get(handlers::handle_books_view),
        )
        .route(
            "/books/:id/view/page/:page/text-only",
            get(handlers::handle_books_view_text_section),
        )
        .route(
            "/variants/view/:id",
            get(handlers::handle_variant_lookup_view),
        )
        .route(
            "/variants/view/:id/related-words",
            get(handlers::handle_lookup_related_section),
        )
        .route(
            "/variants/view/:id/example-sentences",
            get(handlers::handle_lookup_examples_section),
        )
        .route(
            "/variants/:id/create-mneme/:grade",
            post(handlers::handle_create_mneme),
        )
        .route(
            "/variants/:id/review/:mneme_id/:grade",
            post(handlers::handle_review_mneme),
        )
        .route(
            "/variants/bulk-review-for-line/:doc_id/:line_index/:grade",
            post(handlers::handle_bulk_create_mneme),
        )
        .route(
            "/lines/toggle-favourite/:doc_id/:line_index",
            post(handlers::handle_toggle_favourite_line),
        )
        .route(
            "/books/:id/get-review-patch",
            get(handlers::handle_refresh_srs_style_patch),
        )
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
    let offset = UtcOffset::from_hms(1, 0, 0).expect("europe is no more");
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

    opentelemetry::global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name("suzuri")
        .with_auto_split_batch(true)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .expect("failed to build jaeger tracer");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer).boxed();
    tracing_layers.push(otel_layer);

    tracing_subscriber::registry().with(tracing_layers).init();
    debug!("tracing initialised");

    Ok(())
}

#[sqlx::test(migrations = false)]
async fn basic_sqlx_test_works(pool: PgPool) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;
    let one = sqlx::query_scalar!("SELECT 1")
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(one, Some(1));
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn migrations_applied(pool: PgPool) -> sqlx::Result<()> {
    let mut conn = pool.acquire().await?;
    let one = sqlx::query_scalar!("SELECT count(*) FROM lemmas")
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(one, Some(0));
    Ok(())
}

// #[sqlx::test(migrator = "MIGRATOR")]
// async fn import_data(pool: PgPool) -> sqlx::Result<()> {
//     init_dictionaries(&pool).await.unwrap();
//     Ok(())
// }
