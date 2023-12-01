pub mod models;
pub mod prelude;
pub mod schema;

pub mod term;

use diesel::pg::PgConnection;
use diesel::prelude::*;
use prelude::*;
use std::env;
use szr_diesel_logger::LoggingConnection;
use szr_features::UnidicSession;
use szr_ja_utils::kata_to_hira;
use szr_tokenise::{AnnToken, Tokeniser};
use szr_yomichan::read_dictionary;
use term::get_term;

use crate::term::{create_term, get_term_by_id};

pub enum Pos {}

fn main() -> Result<(), Whatever> {
    init_tracing();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let conn_inner = PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));
    let mut conn = LoggingConnection::new(conn_inner);

    let spelling = "abc";
    let reading = "def";

    let _ = create_term(&mut conn, spelling, reading);
    let _ = get_term_by_id(&mut conn, 1);
    let _ = get_term_by_id(&mut conn, 2);
    let _ = get_term(&mut conn, spelling, reading);
    let _ = get_term(&mut conn, spelling, spelling);

    let text = r#"南に二十メートルほど歩いたところで、太い道路に出た。新大橋通りだ。左に、つまり東へ進めば江戸川区に向かい、西へ行けば日本橋に出る。日本橋の手前には隅田川があり、それを渡るのが新大橋だ。石神の職場へ行くには、このまま真っ直ぐ南下するのが最短だ。数百メートル行けば、清澄庭園という公園に突き当たる。その手前にある私立高校が彼の職場だった。つまり彼は教師だった。数学を教えている。石神は目の前の信号が赤になるのを見て、右に曲がった。新大橋に向かって歩いた。向かい風が彼のコートをはためかせた。彼は両手をポケットに突っ込み、身体をやや前屈みにして足を送りだした。厚い雲が空を覆っていた。その色を反射させ、隅田川も濁った色に見えた。小さな船が上流に向かって進んでいく。それを眺めながら石神は新大橋を渡った。"#;
    let text = "世界人権宣言は、この宣言の後に国際連合で結ばれた人権規約の基礎となっており、世界の人権に関する規律の中でもっとも基本的な意義を有する。国際連合経済社会理事会の機能委員会として1946年に国際連合人権委員会が設置されると、同委員会は国際人権章典と呼ばれる単一規範の作成を目指し起草委員会を設置したが、権利の範囲や拘束力の有無を巡って意見が対立し作成のめどが立たなかったため、いったん基礎となる宣言を採択し、その後それを補強する複数の条約及び実施措置を採択することとなった。";
    // let text = "中途半端はしないで";
    // let text = "しないで";

    let mut session = UnidicSession::new()?;
    let res = session.tokenise_mut(&text)?;
    println!("{}\n", res);

    let dict = read_dictionary("input/jmdict_en").whatever_context("read dict")?;

    let mut out: (String, String, String) = Default::default();
    let mut v = Vec::new();
    let mut fused_count = 0;

    for AnnToken {
        token,
        spelling,
        reading,
        ..
    } in res.0.into_iter()
    {
        let reading: String = reading.chars().map(kata_to_hira).collect();
        let candidate = (
            (out.0.clone() + "-" + token),
            (out.1.clone() + &spelling),
            (out.2.clone() + &reading),
        );
        if let Some(_) = dict
            .iter()
            .find(|term| term.spelling == candidate.1 && term.reading == candidate.2)
        {
            out = candidate;
            fused_count += 1;
        } else {
            if fused_count > 1 {
                // debug!("pushing fused word {:?}", out);
            }
            v.push(out.clone());
            out = (token.to_owned(), spelling, reading);
            fused_count = 1;
        }
    }
    v.push(out.clone());
    // v.push((fused_spelling.clone(), fused_reading.clone()));
    debug!("done");
    for (t, _s, _r) in v.iter() {
        print!("{} ", t);
    }

    Ok(())
}

/// Initialise the [`tracing`] library with setup appropriate for this application.
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
