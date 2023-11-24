use crate::app::tpl::{Doc, Render, Z};
use crate::config::CONFIG;
use crate::dict::yomichan::FreqTerm;
use crate::dict::{self, yomichan::DictDef};
use crate::epub;
use crate::furi::{self, MatchKind, Ruby};
use crate::morph::features::SurfaceForm;
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
use sqlx::{ConnectOptions, SqlitePool};
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

enum BadgeSize {
    Xs,
    S,
}

fn badge(size: BadgeSize) -> Doc {
    let colour = "gray";

    let xs_classes = "text-xs font-medium me-2 px-2 py-0.5 rounded";
    let s_classes = "text-sm font-medium me-2 px-2 py-0.5 rounded";

    let size_classes = match size {
        BadgeSize::Xs => xs_classes,
        BadgeSize::S => s_classes,
    };
    let colour_classes =
        format!("bg-{colour}-100 text-{colour}-800 dark:bg-{colour}-900 dark:text-{colour}-300");
    let all_classes = format!("{size_classes} {colour_classes}");

    Z.span().class(all_classes)
}

#[get("/word_info/{id}")]
async fn handle_word_info(
    state: web::Data<ServerState>,
    path: web::Path<u64>,
) -> Result<Doc, WrapError> {
    let pool = state.pool.lock().await;
    let kd = state.kd.lock().await;
    let id = path.into_inner();

    let term = SurfaceForm::get_term(&pool, LemmaId(id))
        .await
        .context("term not known")?;

    let mut candidate_searches = Vec::new();

    let (spelling, reading) = term.surface_form();

    if let Some(reading) = reading {
        candidate_searches.push((spelling, reading));
        candidate_searches.push((&term.orth_form, reading));
        candidate_searches.push((reading, reading));
    }

    let mut dict_defs = Vec::new();

    let mut word_header_ruby = Z.ruby().c(spelling).c(Z.rt().c(reading));

    let mut max_freq = 0;

    let mut related_words = Z.span().c("no related word information");

    let mut links: HashMap<(char, String), HashSet<(FreqTerm, Vec<Span>)>> = Default::default();

    for (spelling, reading) in candidate_searches.into_iter().unique() {
        debug!(spelling, reading, "word_info");
        let reading: String = reading.chars().map(furi::kata_to_hira).collect();
        let new_dict_defs = dict::yomichan::query_dict(&pool, &spelling, &reading)
            .await
            .context("querying dicts")?;
        let freq_term = FreqTerm::get(&pool, &spelling, &reading).await.unwrap_or(0);
        let furi = furi::annotate(spelling, &reading, &kd).context("failed to parse unidic term");

        if let Ok(Ruby::Valid { spans }) = furi {
            // annotate the spelling at the top
            word_header_ruby = Z.ruby();
            for span in spans.iter() {
                match span {
                    Span::Kanji { kanji, yomi, .. } => {
                        word_header_ruby = word_header_ruby.c(*kanji).c(Z.rt().c(yomi.clone()));
                    }
                    Span::Kana { kana, .. } => {
                        word_header_ruby = word_header_ruby.c(*kana).c(Z.rt());
                    }
                }
            }

            // related words

            for span in spans.into_iter() {
                match span {
                    Span::Kanji { kanji, yomi, .. } => {
                        let candidates = FreqTerm::get_all_with_character(&pool, kanji)
                            .await
                            .context("get related words")?;
                        for FreqTerm {
                            spelling: f_spelling,
                            reading: f_reading,
                            frequency,
                        } in candidates.into_iter()
                        {
                            if spelling == f_spelling && reading == f_reading {
                                continue;
                            }
                            if let Ok(Ruby::Valid { spans: f_spans }) =
                                furi::annotate(&f_spelling, &f_reading, &kd)
                            {
                                for f_span in f_spans.iter() {
                                    match f_span {
                                        Span::Kanji {
                                            kanji: f_kanji,
                                            yomi: f_yomi,
                                            ..
                                        } => {
                                            if f_kanji == &kanji && f_yomi == &yomi {
                                                links
                                                    .entry((kanji, yomi.clone()))
                                                    .or_default()
                                                    .insert((
                                                        FreqTerm {
                                                            spelling: f_spelling.clone(),
                                                            reading: f_reading.clone(),
                                                            frequency,
                                                        },
                                                        f_spans.clone(),
                                                    ));
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    Span::Kana { kana, .. } => {}
                }
            }
        }

        if !new_dict_defs.is_empty() {
            debug!("found defs with {spelling}, {reading}");
            dict_defs.extend(new_dict_defs);
            max_freq = std::cmp::max(max_freq, freq_term);
            // only take the first that produces anything
            break;
        }
    }

    // --------------------------------------------------------------------------------
    // Generate the links section

    let mut related_words = Z.div().class("flex flex-col gap-4 text-lg");
    let related_word_limit = 10;
    for ((kanji, yomi), examples) in links.into_iter() {
        let rel_section_header = Z
            .ruby()
            .class("font-bold text-2xl self-center")
            .c(kanji)
            .c(Z.rt().c(yomi.clone()));
        let mut rel_section_body = Z.div().class("flex flex-row flex-wrap text-xl");
        let example_count = examples.len();
        for (
            FreqTerm {
                spelling,
                reading,
                frequency,
            },
            spans,
        ) in examples
            .into_iter()
            .sorted_by_key(|(f, _)| f.frequency)
            .take(related_word_limit)
        {
            let mut word_ruby = Z.span().class("me-3");
            for span in spans.into_iter() {
                match span {
                    Span::Kanji {
                        kanji: f_kanji,
                        yomi: f_yomi,
                        ..
                    } => {
                        let classes = if kanji == f_kanji && yomi == f_yomi {
                            "text-blue-800"
                        } else {
                            "text-gray-600"
                        };
                        word_ruby = word_ruby.c(Z
                            .ruby()
                            .class(classes)
                            .c(f_kanji)
                            .c(Z.rt().class("relative top-1").c(f_yomi.clone())));
                    }
                    Span::Kana { kana, .. } => {
                        word_ruby = word_ruby.c(Z
                            .ruby()
                            .class("text-gray-600")
                            .c(kana)
                            .c(Z.rt().class("opacity-0").c("-")));
                    }
                }
            }
            rel_section_body = rel_section_body.c(word_ruby);
        }
        // if example_count > related_word_limit {
        //     rel_section_body = rel_section_body.c(Z
        //         .ruby()
        //         .class("text-gray-400 italic")
        //         .c(format!("+ {}", example_count - related_word_limit))
        //         .c(Z.rt().class("opacity-0").c("blank")));
        // }
        let rel_section = Z
            .div()
            .class("flex flex-row gap-4 pt-2")
            .c(rel_section_header)
            .c(rel_section_body);
        related_words = related_words.c(rel_section);
    }

    let word_header = Z.h1().class("text-2xl px-6 py-3").c(word_header_ruby);

    let defs_section = Z.ol().class("flex flex-col gap-2").cs(
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
            Z.li()
                .class("list-decimal list-inside")
                .c(badge(BadgeSize::S).c(dict))
                // .c(Z.span().class("italic text-gray-600 me-1").c(dict))
                .cv({
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

    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4")
            .c(Z.h2().class("text-xl font-bold pb-2").c(title))
    };

    let html = Z
        .div()
        .id("defs")
        .class("flex flex-col gap-2")
        .c(word_header)
        .c(section("Stats").c(Z.div().class("flex flex-col").c(Z
            .div()
            .class("flex flex-row gap-2")
            .c(Z.span().class("italic text-gray-600").c("Frequency rank"))
            .c(Z.span().class("").c(max_freq)))))
        .c(section("Links").c(related_words))
        .c(section("Definitions").c(defs_section));

    Ok(html)
}

//-----------------------------------------------------------------------------

#[get("/view_file/{filename}")]
pub async fn handle_view_book(
    state: web::Data<ServerState>,
    path: web::Path<String>,
) -> Result<Doc, WrapError> {
    let pool = state.pool.lock().await;
    let kd = state.kd.lock().await;
    let mut session = state.session.lock().await;
    let path = path.into_inner();

    let (book, terms) = parse_book(&pool, &kd, &mut session, &format!("input/{}", path)).await?;

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
        .class("w-5/12 p-12 bg-gray-200 overflow-scroll text-2xl/10")
        .cs(book, |(tok, id)| {
            if tok == "\n" {
                Z.br()
            } else {
                let text = tok.to_owned();
                if let Some(term) = terms.get(&id) {
                    if let (spelling, Some(reading)) = term.surface_form() {
                        return Z
                            .a()
                            .href(format!("/word_info/{}", id.0))
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
        .c(Z.div().class("w-2/12 bg-gray-200").id("left-spacer"))
        .c(main)
        .c(sidebar)
        .c(Z.div().class("w-2/12 bg-gray-300").id("right-spacer"));
    let ret = Z
        .fragment()
        .c(Z.doctype("html"))
        .c(Z.meta().charset("UTF-8"))
        .c(Z.meta()
            .name("viewport")
            .content("width=device-width, initial-scale=1.0"))
        .c(Z.html().lang("ja").c(head).c(body));

    Ok(ret)
}

//-----------------------------------------------------------------------------

async fn parse_book(
    pool: &SqlitePool,
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
    let mut input = String::new();
    input.extend(buf);
    debug!("parsed epub");
    let AnalysisResult { tokens, terms } = session.analyse_without_cache(&input)?;
    debug!("analysed text");
    SurfaceForm::insert_terms(pool, terms.clone().into_values()).await?;
    debug!("inserted {} terms", terms.len());

    /*
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
    */

    Ok((
        tokens
            .iter()
            .map(|(x, y)| (x.to_string(), y.to_owned()))
            .collect(),
        terms,
    ))
}
