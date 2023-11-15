mod dict;
mod epub;
mod golden;
mod tokeniser;
mod unidic;

use tracing::error;

use anyhow::Result;

pub use hashbrown::HashMap;
pub use hashbrown::HashSet;
use tracing::debug;

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

fn log_mem() {
    let mem = memory_stats::memory_stats().unwrap();
    let rss = (mem.physical_mem as f32 / 1e6) as u32;
    let virt = (mem.virtual_mem as f32 / 1e6) as u32;
    error!("memory usage: rss = {}M, virt = {}M", rss, virt);
}

fn main() -> Result<()> {
    init_tracing();
    log_mem();
    let mut session = unidic::UnidicSession::new()?;
    log_mem();

    let input_files = glob::glob("input/*.epub")?.collect::<Vec<_>>();
    for f in input_files {
        let r = epub::parse(&f?)?;
        let mut buf: Vec<&str> = Vec::new();
        for ch in r.chapters.iter() {
            for line in ch.lines.iter() {
                match line {
                    epub::Element::Line(content) => {
                        buf.push(content);
                    }
                    _ => {}
                }
            }
        }
        let mut s = String::new();
        s.extend(buf);
        let _result = session.tokenize_with_cache(&s)?;
        log_mem();
    }

    Ok(())
}
