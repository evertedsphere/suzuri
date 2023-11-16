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
    let _r = session.tokenize_with_cache(&input)?;

    Ok(())
}
