#![feature(unboxed_closures)]
#![feature(fn_traits)]
mod app;
mod config;
mod dart;
mod dict;
mod epub;
pub mod furi;
mod golden;
mod handlers;
pub mod morph;

use dart::builder::IndexBuilder;
use features::UnidicSession;

use actix_web::{web, App, HttpServer};
use anyhow::Context;
use anyhow::Result;
use furi::KanjiDic;
pub use hashbrown::HashMap;
pub use hashbrown::HashSet;

use sqlx::postgres::PgConnectOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::ConnectOptions;
use sqlx::PgPool;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::str::FromStr;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::info;

fn init_tracing() {
    use tracing::metadata::LevelFilter;
    use tracing_subscriber::{
        filter::{self, FilterExt},
        fmt::format::FmtSpan,
        prelude::*,
    };
    let mut tracing_layers = Vec::new();
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_span_events(FmtSpan::CLOSE | FmtSpan::NEW)
        // .pretty()
        .with_filter(
            filter::filter_fn(|meta| meta.target() != "tracing_actix_web::root_span_builder")
                .and(LevelFilter::DEBUG),
        )
        // .with_filter(
        //     filter::filter_fn(|meta| meta.target() != "sqlx::query").and(LevelFilter::DEBUG),
        // )
        .boxed();
    tracing_layers.push(fmt_layer);
    tracing_subscriber::registry().with(tracing_layers).init();
    debug!("tracing initialised");
}

async fn init_database() -> Result<sqlx::PgPool> {
    info!("connecting to database");
    let url = env::var("DATABASE_URL")?;
    let conn_opts = PgConnectOptions::from_str(&url)
        .unwrap()
        // .log_statements(tracing::log::LevelFilter::Trace)
        // .log_slow_statements(tracing::log::LevelFilter::Warn, Duration::from_millis(10))
        .disable_statement_logging();

    let pool = PgPoolOptions::default()
        .max_connections(24)
        .min_connections(2)
        .test_before_acquire(true)
        .connect_with(conn_opts)
        .await
        .context("initialising pool")?;

    info!("running migrations");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("running migrations")?;
    info!("ran migrations");
    Ok(pool)
}

#[cfg(test)]
fn annotate_all_of_unidic() -> Result<()> {
    use furi::MatchKind;
    use furi::Ruby;
    use furi::Span;
    use tracing::error;

    let kd = furi::read_kanjidic()?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path("data/system/unidic-cwj-3.1.0/lex_3_1.csv")?;
    let mut successes = 0;
    let mut invalids = 0;
    let mut total = 0;
    let mut unknown_count = 0;
    let mut unknown = Vec::new();
    let mut inconsistent_count = 0;
    let mut inconsistent = Vec::new();

    let mut unknown_readings: HashMap<(char, String), u32> = HashMap::new();

    for (_i, rec_full) in rdr.records().enumerate().step_by(20) {
        let rec_full = rec_full?;
        let mut rec = csv::StringRecord::new();
        for f in rec_full.iter().skip(4) {
            rec.push_field(f);
        }
        if let Ok(line) = rec.deserialize::<crate::features::Term>(None) {
            // do nothing
            let (spelling, reading) = line.surface_form();
            if let Some(reading) = reading {
                let furi = furi::annotate(spelling, reading, &kd)
                    .context("failed to parse unidic term")?;
                debug!("{} ({:?}), furi: {}", spelling, reading, furi);
                match furi {
                    Ruby::Valid { spans } => {
                        successes += 1;
                        for span in spans.iter() {
                            if let Span::Kanji {
                                kanji,
                                yomi,
                                dict_yomi: _,
                                match_kind,
                            } = span
                            {
                                if match_kind.iter().all(|z| z == &MatchKind::Wildcard) {
                                    *unknown_readings.entry((*kanji, yomi.clone())).or_default() +=
                                        1;
                                }
                            }
                        }
                    }
                    Ruby::Invalid { .. } => {
                        invalids += 1;
                    }
                    Ruby::Unknown { .. } => {
                        unknown_count += 1;
                        unknown.push((spelling.to_owned(), reading.to_owned()));
                    }
                    Ruby::Inconsistent(..) => {
                        inconsistent_count += 1;
                        inconsistent.push((spelling.to_owned(), reading.to_owned()));
                    }
                };
                total += 1;
            }
        } else {
            error!("deserialisation failed: {:?}", rec)
        }
    }

    let mut top_unknown_readings: Vec<_> = unknown_readings
        .into_iter()
        .filter_map(|((k, r), n)| if n >= 10 { Some((n, (k, r))) } else { None })
        .collect();

    top_unknown_readings.sort();
    // lol
    top_unknown_readings.reverse();

    debug!("top unknown readings:");
    for (count, (kanji, reading)) in top_unknown_readings {
        debug!("  {} ({}): {}x", kanji, reading, count);
    }

    let _fails = total - successes - invalids;
    let ns = 100.0 * successes as f32 / total as f32;
    let ni = 100.0 * invalids as f32 / total as f32;
    let nu = 100.0 * unknown_count as f32 / total as f32;
    let nb = 100.0 * inconsistent_count as f32 / total as f32;
    debug!(
        "done, success {:.3}% ({}), invalid {:.3}% ({}), fail {:.3}% ({}), bugs {:.3}% ({})",
        ns, successes, ni, invalids, nu, unknown_count, nb, inconsistent_count
    );
    Ok(())
}

pub struct ServerState {
    pub pool: Mutex<sqlx::PgPool>,
    pub session: Mutex<UnidicSession>,
    pub kd: Mutex<KanjiDic>,
}

async fn run_actix(pool: PgPool) -> Result<()> {
    let state = ServerState {
        pool: Mutex::new(pool),
        session: Mutex::new(features::UnidicSession::new()?),
        kd: Mutex::new(furi::read_kanjidic()?),
    };
    let wrapped_state = web::Data::new(state);
    HttpServer::new(move || {
        App::new()
            .wrap(tracing_actix_web::TracingLogger::default())
            .app_data(wrapped_state.clone())
            .service(crate::handlers::handle_view_book)
            .service(crate::handlers::handle_word_info)
            .service(crate::handlers::handle_vocab_review)
            .service(actix_files::Files::new("/static", "./dist"))
    })
    .bind(("0.0.0.0", 34343))
    .context("creating server")?
    .run()
    .await
    .context("running server")
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let trie = {
        let mut ib = IndexBuilder::new();
        let f = File::open("./tokens.txt")?;
        let mut buf = BufReader::new(f);
        ib.build(&mut buf)?
    };

    let input = "頑張らないとね";

    for tok in trie.search(input) {
        println!("token: {:?} = {}", tok, tok.to_str(input));
    }

    let seg = trie
        .search(input)
        .map(|s| s.to_str(input))
        .collect::<Vec<String>>()
        .join(" ");
    println!("seg: {seg}");

    return Ok(());

    let pool = init_database().await?;
    // Import stock dictionaries
    dict::yomichan::import_dictionary(&pool, "JMdict (en)", "jmdict_en").await?;
    dict::yomichan::import_dictionary(&pool, "JMnedict", "jmnedict").await?;
    dict::yomichan::import_dictionary(&pool, "dic.pixiv.net", "pixiv_summaries").await?;
    dict::yomichan::import_dictionary(&pool, "旺文社", "oubunsha").await?;
    dict::yomichan::import_frequency_dictionary(&pool, "CC100", "Freq_CC100").await?;
    run_actix(pool).await?;
    Ok(())
}

#[test]
fn test_annotate_all_of_unidic() {
    annotate_all_of_unidic().unwrap();
}
