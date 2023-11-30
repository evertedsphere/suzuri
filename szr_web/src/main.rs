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
use szr_tokenise::{AnnToken, Tokeniser};
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

    let text = "午前七時三十五分、石神はいつものようにアパートを出た。三月に入ったとはいえ、まだ風はかなり冷たい。マフラーに顎を埋めるようにして歩きだした。通りに出る前に、ちらりと自転車置き場に目を向けた。そこには数台並んでいたが、彼が気にかけている緑色の自転車はなかった。";

    let mut session = UnidicSession::new()?;
    let res = session.tokenise_mut(&text)?;
    debug!(%res);

    for AnnToken {
        token,
        spelling,
        reading,
    } in res.0.into_iter()
    {
        //
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
