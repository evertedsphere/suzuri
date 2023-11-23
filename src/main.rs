#![feature(unboxed_closures)]
#![feature(fn_traits)]
#![allow(unused)]
mod app;
mod config;
mod dict;
mod epub;
pub mod furi;
mod golden;
mod handlers;
pub mod morph;

use crate::config::CONFIG;
use crate::furi::MatchKind;
use crate::furi::Ruby;
use crate::morph::features::ExtraPos;
use crate::morph::features::LemmaId;
use crate::morph::features::TertiaryPos;
use actix_web::get;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::web::Json;
use actix_web::HttpResponse;
use actix_web::ResponseError;
use actix_web::{web, App, HttpServer};
use anyhow::Context;
use anyhow::Result;
use furi::KanjiDic;
use furi::Span;
pub use hashbrown::HashMap;
pub use hashbrown::HashSet;
use morph::features::AnalysisResult;
use morph::features::Term;
use morph::features::UnidicSession;
use serde::Serialize;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::ConnectOptions;
use sqlx::SqlitePool;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::log::warn;

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
        .boxed();
    tracing_layers.push(fmt_layer);
    tracing_subscriber::registry().with(tracing_layers).init();
    debug!("tracing initialised");
}

async fn init_database() -> Result<sqlx::SqlitePool> {
    info!("connecting to database");
    let url = &format!("sqlite:file:{}/data.db?mode=rwc", &CONFIG.storage.data_dir);
    let conn_opts = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .log_statements(tracing::log::LevelFilter::Debug);

    let pool = SqlitePoolOptions::default()
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

fn log_mem() {
    let mem = memory_stats::memory_stats().unwrap();
    let rss = (mem.physical_mem as f32 / 1e6) as u32;
    let virt = (mem.virtual_mem as f32 / 1e6) as u32;
    info!("memory usage: rss = {}M, virt = {}M", rss, virt);
}

fn annotate_all_of_unidic() -> Result<()> {
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

    for (i, rec_full) in rdr.records().enumerate().step_by(20) {
        let rec_full = rec_full?;
        let mut rec = csv::StringRecord::new();
        for f in rec_full.iter().skip(4) {
            rec.push_field(f);
        }
        if let Ok(line) = rec.deserialize::<crate::morph::features::Term>(None) {
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
                                dict_yomi,
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
                // } else {
                //     error!("failed: {}, {}", spelling.to_owned(), reading.to_owned());
                //     failures.push((spelling.to_owned(), reading.to_owned()));
                // }
                // warn!("{} ({}), furi failed", spelling, reading,);
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

    // for (spelling, reading) in unknown.iter() {
    //     let furi = furi::annotate(spelling, reading, &kd).context("failed to parse unidic term")?;
    //     debug!("failed: {}", furi);
    // }

    // for (spelling, reading) in inconsistent.iter() {
    //     let furi = furi::annotate(spelling, reading, &kd).context("failed to parse unidic term")?;
    //     debug!("failed: {}", furi);
    // }

    let fails = total - successes - invalids;
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

// fn parse_books() -> Result<()> {
//     let kd = furi::read_kanjidic()?;
//     let mut session = morph::features::UnidicSession::new()?;

//     let input_files = glob::glob("input/*.epub")?.collect::<Vec<_>>();

//     for epub_file in input_files {
//         let epub_file = epub_file?;
//         let _ = parse_book(&kd, &mut session, &epub_file)?;
//     }
//     Ok(())
// }

pub struct ServerState {
    pub pool: Mutex<sqlx::SqlitePool>,
    pub session: Mutex<UnidicSession>,
}

async fn run_actix(pool: SqlitePool) -> Result<()> {
    let state = ServerState {
        pool: Mutex::new(pool),
        session: Mutex::new(morph::features::UnidicSession::new()?),
    };
    let wrapped_state = web::Data::new(state);
    HttpServer::new(move || {
        App::new()
            .wrap(tracing_actix_web::TracingLogger::default())
            .app_data(wrapped_state.clone())
            .service(crate::handlers::handle_view_book)
            .service(crate::handlers::handle_query_dict)
    })
    .bind(("127.0.0.1", 8081))
    .context("creating server")?
    .run()
    .await
    .context("running server")
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let pool = init_database().await?;
    dict::yomichan::import_dictionary(&pool, "JMdict (en)", "jmdict_en").await?;
    dict::yomichan::import_dictionary(&pool, "dic.pixiv.net", "pixiv_summaries").await?;
    dict::yomichan::import_dictionary(&pool, "旺文社", "oubunsha").await?;
    run_actix(pool).await?;
    Ok(())
}

#[test]
fn test_annotate_all_of_unidic() {
    annotate_all_of_unidic().unwrap();
}
