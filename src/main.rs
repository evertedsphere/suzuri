mod unidic;

use anyhow::{Context, Result};

use tracing::{debug, instrument};

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
        .pretty()
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
    let ja_input = "京の都には当時、but 剣の道場が大小含めて六百四十五、数えられた──むろんこれはあくまでも表向きの話であり、裏向き潜り非合法を含めれば、その数は軽く千を越えていただろうことはおよそ間違いがない。その中でも左京の氷床道場と言えば武芸を嗜む者ならば誰もが知るであろう戦国時代から続く名門であり、幕府将軍家とのゆかりも深い。";

    let input = zh_input;

    let mut session = unidic::UnidicSession::new()?;
    let r = session.tokenize_with_cache(&input)?;

    Ok(())
}

#[test]
fn main_does_not_error() {
    _ = main();
}
