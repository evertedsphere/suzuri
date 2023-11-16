#![allow(unused)]
mod config;
mod dict;
mod epub;
mod furi;
mod golden;
mod tokeniser;
mod unidic;

use crate::config::CONFIG;
use anyhow::Context;
use anyhow::Result;
pub use hashbrown::HashMap;
pub use hashbrown::HashSet;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::ConnectOptions;
use std::str::FromStr;
use tracing::debug;
use tracing::error;
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

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let pool = init_database().await?;
    log_mem();

    dict::yomichan::import_dictionary(&pool, "jmdict_en", "jmdict_en").await?;
    log_mem();

    let kd = furi::read_kanjidic()?;

    let words = vec![
        ("検討", "けんとう"),
        ("人か人", "ひとかひと"),
        ("人人", "ひとびと"),
        ("山々", "やまやま"),
        ("口血", "くち"),
        ("人", "ひとこと"),
        ("劇場版", "げきじょうばん"),
        ("化粧", "けしょう"),
        ("民主主義", "みんしゅしゅぎ"),
        ("社会形成推進基本法", "しゃかいけいせいすいしんきほんほう"),
    ];

    for (spelling, reading) in words {
        let furi = furi::annotate(&spelling, &reading, &kd).context("failed to apply furi");
        if let Ok(furi) = furi {
            debug!("{} ({}), furi: {:?}", spelling, reading, furi);
        }
    }

    // let mut session = unidic::UnidicSession::new()?;
    // log_mem();
    // let input_files = glob::glob("input/*.epub")?.collect::<Vec<_>>();
    // for f in input_files {
    //     let r = epub::parse(&f?)?;
    //     let mut buf: Vec<&str> = Vec::new();
    //     for ch in r.chapters.iter() {
    //         for line in ch.lines.iter() {
    //             match line {
    //                 epub::Element::Line(content) => {
    //                     buf.push(content);
    //                 }
    //                 _ => {}
    //             }
    //         }
    //     }
    //     let mut s = String::new();
    //     s.extend(buf);
    //     let _result = session.tokenize_with_cache(&s)?;
    //     log_mem();
    // }
    // log_mem();

    Ok(())
}
