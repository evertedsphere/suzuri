mod epub;
mod golden;
mod tokeniser;
mod unidic;

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

fn main() -> Result<()> {
    init_tracing();

    #[allow(unused)]
    let zh_input = "異丙醇可與水、醇、醚和氯仿混溶。它會溶解乙基纖維素、聚乙烯醇縮丁醛、多種油、生物鹼、樹膠和天然樹脂。";
    #[allow(unused)]
    let ja_input = "帰ってくれるかな、こいつ。直せられるか。";

    let input = ja_input;

    let mut session = unidic::UnidicSession::new()?;
    // let _r = session.tokenize_with_cache(&input)?;

    let limit = 10;

    let input_files = glob::glob("input/*.epub").unwrap().collect::<Vec<_>>();
    for f in input_files {
        let f = f.unwrap();
        let r = epub::parse(&f).unwrap();
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
        let result = session.tokenize_with_cache(&s)?;
        println!("done with file: {:?}, produced {} tokens", f, result.len());
    }

    Ok(())
}
