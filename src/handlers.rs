use crate::app::tpl::{Doc, Render, Z};
use crate::config::CONFIG;
use crate::dict::{self, yomichan::DictDef};
use crate::epub;
use crate::furi::{self, MatchKind, Ruby};
use crate::morph::{
    self,
    features::{ExtraPos, LemmaId, TertiaryPos},
};
use crate::ServerState;
use actix_web::{
    get,
    http::{header::ContentType, StatusCode},
    post,
    web::{self, Json},
    App, HttpResponse, HttpServer, Responder, ResponseError,
};
use anyhow::{Context, Result};
use furi::{KanjiDic, Span};
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use morph::features::{AnalysisResult, UnidicSession};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::ConnectOptions;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub struct WrapError {
    err: anyhow::Error,
}

impl Serialize for WrapError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        pub struct H {
            error: String,
        }
        let h = H {
            error: format!("{:?}", self),
        };
        h.serialize(serializer)
    }
}

impl std::fmt::Display for WrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ResponseError for WrapError {
    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::json())
            .json(self)
    }
    fn status_code(&self) -> actix_web::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl From<anyhow::Error> for WrapError {
    fn from(err: anyhow::Error) -> WrapError {
        WrapError { err }
    }
}

//-----------------------------------------------------------------------------

fn badge() -> Doc {
    let size = BadgeSize::Xs;
    let colour = "green";

    enum BadgeSize {
        Xs,
        S,
    }

    let xs_classes = "text-xs font-medium me-1 px-2 py-0.5 rounded";
    let s_classes = "text-sm font-medium me-1 px-2 py-0.5 rounded";

    let size_classes = match size {
        BadgeSize::Xs => xs_classes,
        BadgeSize::S => s_classes,
    };
    let colour_classes =
        format!("bg-{colour}-100 text-{colour}-800 dark:bg-{colour}-900 dark:text-{colour}-300");
    let all_classes = format!("{size_classes} {colour_classes}");

    Z.span().class(all_classes)
}

#[get("/query_dict/{id}")]
async fn handle_query_dict(
    state: web::Data<ServerState>,
    path: web::Path<u64>,
) -> Result<Doc, WrapError> {
    let pool = state.pool.lock().await;
    let id = path.into_inner();
    let terms = state.terms.lock().await;
    let term = terms.get(&LemmaId(id)).context("term not known")?;

    let mut candidate_searches = Vec::new();

    let (spelling, reading) = term.surface_form();

    if let Some(reading) = reading {
        candidate_searches.push((spelling, reading));
        candidate_searches.push((&term.orth_form, reading));
    }

    let mut dict_defs = Vec::new();

    for (spelling, reading) in candidate_searches.into_iter().unique() {
        debug!(spelling, reading, "query_dict");
        let reading: String = reading.chars().map(furi::kata_to_hira).collect();
        let new_dict_defs = dict::yomichan::query_dict(&pool, &spelling, &reading)
            .await
            .context("querying dict")?;
        dict_defs.extend(new_dict_defs);
    }

    let defs_section = Z.div().cs(
        dict_defs,
        |DictDef {
             dict,
             defs,
             spelling,
             reading,
         }| {
            // intersperse with commas
            // bit ugly but it's fine
            let mut it = defs.0.into_iter().peekable();
            Z.div().c(badge().c(spelling)).c(badge().c(dict)).cv({
                let mut v = Vec::new();
                while let Some(def) = it.next() {
                    v.push(Z.span().c(def));
                    if it.peek().is_some() {
                        v.push(Z.span().c(", "));
                    }
                }
                v
            })
        },
    );

    let html = Z
        .div()
        .id("defs")
        .c(Z.h1()
            .class("text-2xl pb-3")
            .c(Z.ruby().c(spelling).c(Z.rt().c(reading))))
        .c(defs_section);

    Ok(html)
}

//-----------------------------------------------------------------------------

#[get("/view_file/{filename}")]
pub async fn handle_view_book(
    state: web::Data<ServerState>,
    path: web::Path<String>,
) -> Result<Doc, WrapError> {
    let path = path.into_inner();
    let kd = furi::read_kanjidic()?;
    let mut session = morph::features::UnidicSession::new()?;

    let (book, terms) = parse_book(&kd, &mut session, &format!("input/{}", path))?;

    let unpoly_preamble = (
        Z.script()
            .src("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.js"),
        Z.stylesheet("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.css"),
    );
    let tailwind_preamble = Z.script().src("https://cdn.tailwindcss.com");

    let sidebar = Z
        .div()
        .id("sidebar")
        .class("w-3/12 p-6 bg-gray-300 overflow-auto")
        .c(Z.div().id("defs"));

    let main = Z
        .div()
        .id("main")
        .class("w-5/12 p-6 bg-gray-200 overflow-scroll")
        .cs(book, |(tok, id)| {
            if tok == "\n" {
                Z.br()
            } else {
                let text = tok.to_owned();
                if let Some(term) = terms.get(&id) {
                    if let (spelling, Some(reading)) = term.surface_form() {
                        return Z
                            .a()
                            .href(format!("/query_dict/{}", id.0))
                            .attr("up-target", "#defs")
                            .c(text);
                    }
                }
                Z.span().c(text)
            }
        });

    let head = Z.head().c(unpoly_preamble).c(tailwind_preamble);
    let body = Z
        .body()
        .class("h-screen w-screen bg-gray-100 relative flex flex-row overflow-hidden")
        .c(Z.div().class("w-2/12 min-w-2/12").id("left-spacer"))
        .c(main)
        .c(sidebar)
        .c(Z.div().class("w-2/12").id("right-spacer"));
    let ret = Z
        .fragment()
        .c(Z.doctype("html"))
        .c(Z.meta().charset("UTF-8"))
        .c(Z.meta()
            .name("viewport")
            .content("width=device-width, initial-scale=1.0"))
        .c(Z.html().lang("ja").c(head).c(body));

    state.terms.lock().await.extend(terms);

    Ok(ret)
}

//-----------------------------------------------------------------------------

fn parse_book(
    kd: &KanjiDic,
    session: &mut UnidicSession,
    epub_file: impl AsRef<Path>,
) -> Result<(
    Vec<(String, LemmaId)>,
    HashMap<LemmaId, morph::features::Term>,
)> {
    let mut yomi_freq: HashMap<furi::Span, u64> = HashMap::new();
    let mut yomi_uniq_freq: HashMap<furi::Span, u64> = HashMap::new();
    let mut lemma_freq: HashMap<LemmaId, u64> = HashMap::new();
    let mut name_count = 0;

    let r = epub::parse(epub_file.as_ref())?;
    let mut buf: Vec<&str> = Vec::new();
    let mut n = 0;
    for ch in r.chapters.iter() {
        for line in ch.lines.iter() {
            match line {
                epub::Element::Line(content) => {
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
    let mut input_ = String::new();
    input_.extend(buf);
    let AnalysisResult { tokens, terms } = session.analyse_with_cache(&input_)?;

    // let mut after = 0;
    for (text, term_id) in tokens.iter() {
        // if term_id.0 == 8235625660686848 {
        //     print!("\nstart");
        //     after = 1;
        // }
        // if after >= 1 && after < 20 {
        //     after += 1;
        //     print!("{}", text);
        // }
        *lemma_freq.entry(*term_id).or_default() += 1;
    }
    // println!("done");

    for (term_id, term) in terms.iter() {
        let (spelling, reading) = term.surface_form();
        if term.extra_pos == ExtraPos::Myou || term.extra_pos == ExtraPos::Sei {
            debug!("skipping name term {} ({:?})", term, term_id);
            name_count += 1;
            continue;
        }
        if let Some(reading) = reading {
            let furi =
                furi::annotate(spelling, reading, &kd).context("failed to parse unidic term");
            if let Ok(Ruby::Valid { spans }) = furi {
                for span_ in spans.into_iter() {
                    if let Span::Kanji { .. } = span_ {
                        *yomi_uniq_freq.entry(span_.clone()).or_default() += 1;
                        *yomi_freq.entry(span_).or_default() += lemma_freq[term_id];
                    }
                }
            }
        }
    }

    debug!("skipped {} name terms", name_count);

    debug!("yomi freq (coverage)");
    let mut freqs = yomi_freq.into_iter().collect::<Vec<_>>();
    freqs.sort_by(|x, y| x.1.cmp(&y.1).reverse());
    freqs.truncate(100);
    for (span_, freq) in freqs.iter() {
        print!("{}: {}x, ", span_, freq);
    }
    println!();

    debug!("yomi freq (unique)");
    let mut uniq_freqs = yomi_uniq_freq.into_iter().collect::<Vec<_>>();
    uniq_freqs.sort_by(|x, y| x.1.cmp(&y.1).reverse());
    uniq_freqs.truncate(100);
    for (span_, freq) in uniq_freqs.iter() {
        print!("{}: {}x, ", span_, freq);
    }
    println!();

    Ok((
        tokens
            .iter()
            .map(|(x, y)| (x.to_string(), y.to_owned()))
            .collect(),
        terms,
    ))
}
