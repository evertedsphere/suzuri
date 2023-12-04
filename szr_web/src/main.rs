pub mod models;
pub mod term;

use std::{env, path::Path};

use axum::{routing::get, Router};
use diesel::{pg::PgConnection, prelude::*};
use snafu::{ResultExt, Whatever};
use szr_dict::DictionaryFormat;
use szr_diesel_logger::LoggingConnection;
use szr_features::UnidicSession;
use szr_tokenise::Tokeniser;
use szr_yomichan::Yomichan;
use tracing::debug;

fn parse_book<'a>(
    // pool: &'a mut C,
    // _kd: &'a KanjiDic,
    session: &'a mut UnidicSession,
    epub_file: impl AsRef<Path>,
) -> Result<Vec<(String, String)>, Whatever>
// where
//     C: Connection<Backend = Pg> + LoadConnection,
{
    let r = szr_epub::parse(epub_file.as_ref()).whatever_context("parsing epub")?;
    let mut buf: Vec<&str> = Vec::new();
    let mut n = 0;
    for ch in r.chapters.iter() {
        for line in ch.lines.iter() {
            match line {
                szr_epub::Element::Line(content) => {
                    buf.push(content);
                    buf.push("\n");
                    n += 1;
                    if n == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let mut input = String::new();
    input.extend(buf);
    debug!("parsed epub");
    let tokens = session.tokenise_mut(&input)?;
    debug!("analysed text");
    // SurfaceForm::insert_terms(pool, terms.clone().into_values()).await?;
    // debug!("inserted {} terms", terms.len());

    Ok(tokens
        .0
        .into_iter()
        .map(|x| (x.spelling, x.reading))
        .collect())
}

#[snafu::report]
#[tokio::main]
async fn main() -> Result<(), Whatever> {
    init_tracing();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let conn_inner = PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));
    let mut conn = LoggingConnection::new(conn_inner);

    let mut session = UnidicSession::new()?;
    let _kd = szr_ruby::read_kanjidic("data/system/readings.json").whatever_context("kanjidic")?;
    let dict =
        Yomichan::read_from_path("input/jmdict_en", "jmdict_en").whatever_context("read dict")?;
    Yomichan::save_dictionary(&mut conn, "jmdict_en", dict.clone())
        .whatever_context("persist dict")?;
    let mut content = parse_book(&mut session, "input/km.epub")?;
    content.truncate(200);

    debug!("{:?}", content);

    let app = Router::new().route("/", get(|| async { "Hello, World!" }));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:34343")
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
