use crate::app::tpl::{Doc, Render, Z};

use crate::dict::yomichan::FreqTerm;
use crate::dict::{self, yomichan::DictDef};
use crate::epub;
use crate::furi::{self, Ruby};
use crate::features::SurfaceForm;
use crate::{self, features::LemmaId};
use crate::ServerState;
use actix_web::{
    get,
    http::{header::ContentType, StatusCode},
    post,
    web::{self},
    HttpResponse, ResponseError,
};
use anyhow::{anyhow, Context, Result};
use fsrs::{Card, Rating, State};
use furi::{KanjiDic, Span};
use hashbrown::{HashMap, HashSet};
use indexmap::IndexMap;
use itertools::Itertools;
use features::{AnalysisResult, UnidicSession};
use serde::Serialize;

use sqlx::PgPool;
use std::path::Path;

use tracing::debug;

#[derive(Debug)]
pub struct WrapError {
    #[allow(unused)]
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

#[post("/vocab_review/{id}/{rating}")]
async fn handle_vocab_review(
    state: web::Data<ServerState>,
    path: web::Path<(u64, u8)>,
) -> Result<Doc, WrapError> {
    let pool = state.pool.lock().await;
    let (id, rating_raw) = path.into_inner();
    let rating = match rating_raw {
        1 => Rating::Again,
        2 => Rating::Good,
        3 => Rating::Hard,
        4 => Rating::Easy,
        _ => {
            return Err(WrapError {
                err: anyhow!("unknown rating"),
            });
        }
    };
    debug!("reviewing card {id} with rating {rating:?}");
    let new_card = SurfaceForm::do_review_by_id(&pool, LemmaId(id), rating).await?;

    Ok(render_memory_section(Some(&new_card), LemmaId(id)))
}

/// https://docs.rs/relativetime/latest/src/relativetime/lib.rs.html#15-47
/// Thresholds are taken from day.js
pub fn english_relative_time(secs: u64) -> String {
    if secs <= 4 {
        return "a few seconds".to_string();
    } else if secs <= 44 {
        return format!("{} seconds", secs);
    } else if secs <= 89 {
        return "a minute".to_string();
    }
    let mins = secs / 60;
    if mins < 2 {
        return format!("a minute");
    } else if mins <= 44 {
        return format!("{} minutes", mins);
    } else if mins <= 89 {
        return "an hour".to_string();
    }
    let hours = mins / 60;
    if hours < 2 {
        return format!("an hour");
    } else if hours <= 21 {
        return format!("{} hours", hours);
    } else if hours <= 35 {
        return "a day".to_string();
    }
    let days = hours / 24;
    if days < 2 {
        return format!("a day");
    } else if days <= 25 {
        return format!("{} days", days);
    } else if days <= 45 {
        return "a month".to_string();
    }
    let months = days / 30;
    if months <= 10 {
        return format!("{} months", months);
    } else if months <= 17 {
        return "a year".to_string();
    }
    let years = (months as f64 / 12.0).round() as i64;
    return format!("{:.0} years", years);
}

fn render_memory_section(card: Option<&Card>, id: LemmaId) -> Doc {
    let mut status_block = Z.div().class("flex flex-col gap-2");
    let now = chrono::Utc::now();
    let mut poll_interval = None;

    status_block = match card {
        None => status_block.c(labelled_value_c(
            "state",
            "Fresh",
            "font-bold text-gray-600",
        )),
        Some(card) => {
            let diff = card.due - now;
            let diff_secs = diff.num_seconds();
            let raw_diff_str = english_relative_time(diff_secs.abs() as u64);
            if diff.num_days().abs() < 2 {
                // Checking for the review state is cheap, but it's still not
                // very useful to do it too frequently if the interval is still
                // long.
                // Here we choose to aim for 5 updates over the life of the review.
                poll_interval = Some(std::cmp::max(10, diff_secs.abs() / 5));
            }
            debug!("diff: {:?} - {:?} = {:?}", now, card.due, diff_secs);
            let diff_str = if diff_secs < 0 {
                format!("{raw_diff_str} ago")
            } else if diff_secs > 0 {
                format!("in {raw_diff_str}")
            } else {
                "right now".to_string()
            };
            status_block
                .c(labelled_value_c(
                    "state",
                    format!("{:?}", card.state),
                    "font-bold",
                ))
                .c(labelled_value_c(
                    "due",
                    format!("{}", diff_str),
                    "font-bold",
                ))
        }
    };

    let review_button = |rating_num, extra_classes, text| {
        let base_classes = "";
        Z.a()
            .class(format!("{base_classes} {extra_classes}"))
            .href(format!("/vocab_review/{}/{}", id.0, rating_num))
            .c(text)
            .up_target("#review-result")
            .up_method("post")
    };

    let review_actions_block = Z
        .div()
        .class("flex flex-row gap-2")
        .up_nav()
        .c(labelled_value_c(
            "review as",
            Z.div()
                .class("flex flex-row gap-2")
                .c(review_button(1, "text-red-800", "Nope"))
                .c(review_button(2, "text-yellow-900", "Hard"))
                .c(review_button(3, "text-green-800", "Good"))
                .c(review_button(4, "text-blue-800", "Easy")),
            "font-bold",
        ));

    let mut memory_section = Z
        .div()
        .class("flex flex-col gap-2")
        .id("review-result")
        .c(status_block)
        .c(review_actions_block);

    if let Some(poll_interval) = poll_interval {
        memory_section = memory_section
            .up_poll()
            .up_interval((1000 * poll_interval).to_string());
    }

    memory_section
}

fn labelled_value_c<'a, V: Render>(label: &'a str, value: V, classes: &'static str) -> Doc {
    Z.div()
        .class("flex flex-row gap-4")
        .c(Z.span()
            .class("italic text-gray-600 shrink-0 whitespace-nowrap")
            .c(label))
        .c(Z.span().class(classes).c(value))
}

fn labelled_value<V: Render>(label: &str, value: V) -> Doc {
    labelled_value_c(label, value, "")
}

#[get("/word_info/{id}")]
async fn handle_word_info(
    state: web::Data<ServerState>,
    path: web::Path<u64>,
) -> Result<Doc, WrapError> {
    let pool = state.pool.lock().await;
    let kd = state.kd.lock().await;
    let id = path.into_inner();

    let surface_form = SurfaceForm::get_by_id(&pool, LemmaId(id))
        .await
        .context("term not known")?;
    let term = surface_form.term;

    // --------------------------------------------------------------------------------
    // Memory
    // TODO: pull this out into a separate function; regenerate only this via an unpoly
    // callback when the review is done

    let start_card = surface_form.card;
    let memory_section = render_memory_section(start_card.as_ref(), LemmaId(id));

    // --------------------------------------------------------------------------------
    // Gather data for the links

    let (spelling, reading) = term.surface_form();

    let mut candidate_searches: Vec<(&str, &str)> = Vec::new();
    if let Some(reading) = reading {
        if spelling == reading {
            // stuff like names gets the katakana treatment from unidic
            candidate_searches.push((&term.orth_form, reading));
            // candidate_searches.push((spelling, reading));
        } else {
            candidate_searches.push((spelling, reading));
            candidate_searches.push((&term.orth_form, reading));
        }
        // candidate_searches.push((reading, reading));
    }

    // This is what goes on top. We start out with a fallback that just
    // lays the spelling across the reading in a single block.
    // TODO this could be refactored; we have some information already
    // from what we just did five lines ago...
    let mut word_header_ruby = Z.ruby().c(spelling).c(Z.rt().c(reading));

    let mut dict_defs = Vec::new();
    let mut max_freq = 0;
    let _related_words = Z.span().c("no related word information");
    let mut links: IndexMap<
        (char, String),
        (
            HashSet<(FreqTerm, Vec<Span>)>,
            HashSet<(FreqTerm, Vec<Span>)>,
        ),
    > = Default::default();

    for (spelling, reading) in candidate_searches.into_iter().unique() {
        debug!(spelling, reading, "word_info");
        let reading: String = reading.chars().map(furi::kata_to_hira).collect();
        let new_dict_defs = dict::yomichan::query_dict(&pool, &spelling, &reading)
            .await
            .context("querying dicts")?;

        let freq_term = FreqTerm::get(&pool, &spelling, &reading).await.unwrap_or(0);
        let furi = furi::annotate(spelling, &reading, &kd).context("failed to parse unidic term");

        if let Ok(Ruby::Valid { spans }) = furi {
            debug!("found valid parse {spelling} ({reading})");
            // annotate the spelling at the top
            word_header_ruby = Z.ruby();
            for span in spans.iter() {
                match span {
                    Span::Kanji { kanji, yomi, .. } => {
                        word_header_ruby = word_header_ruby
                            .c(*kanji)
                            .c(Z.rt().class("relative top-1").c(yomi.clone()));
                    }
                    Span::Kana { kana, .. } => {
                        word_header_ruby =
                            word_header_ruby.c(*kana).c(Z.rt().class("relative top-1"));
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
                                            if f_kanji == &kanji {
                                                let payload = (
                                                    FreqTerm {
                                                        spelling: f_spelling.clone(),
                                                        reading: f_reading.clone(),
                                                        frequency,
                                                    },
                                                    f_spans.clone(),
                                                );
                                                let value = &mut links
                                                    .entry((kanji, yomi.clone()))
                                                    .or_default();
                                                let target = if f_yomi == &yomi {
                                                    &mut value.0
                                                } else {
                                                    &mut value.1
                                                };
                                                target.insert(payload);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    Span::Kana { .. } => {}
                }
            }
        }

        if !new_dict_defs.is_empty() {
            debug!("found defs with {spelling}, {reading}");
            if dict_defs.is_empty() {
                dict_defs.extend(new_dict_defs);
            } else {
                debug!("was already empty, leaving alone as this is a lower-priority match");
            }
            max_freq = std::cmp::max(max_freq, freq_term);
            // only take the first that produces anything
            break;
        }
    }

    // --------------------------------------------------------------------------------
    // Generate links

    let mut related_words = Z.div().class("flex flex-col gap-4 text-lg");

    let any_links = !links.is_empty();

    for ((kanji, yomi), (same_reading, other_readings)) in links.into_iter() {
        // the big kanji
        let rel_section_header = Z
            .ruby()
            .class("text-4xl text-center w-1/6 self-center")
            .c(kanji)
            .c(Z.rt().class("relative top-1").c(yomi.clone()));

        // the list of words
        let mut rel_section_body = Z
            .div()
            .class("flex flex-row flex-wrap text-xl self-center w-5/6 overflow-hidden -ml-4");

        for (examples, flag, related_word_limit) in
            [(same_reading, false, 5), (other_readings, true, 5)]
        {
            let _example_count = examples.len();
            for (
                FreqTerm {
                    spelling: _,
                    reading: _,
                    frequency: _,
                },
                spans,
            ) in examples
                .into_iter()
                .sorted_by_key(|(f, _)| f.frequency)
                .take(related_word_limit)
            {
                // an individual word
                let mut word_ruby = Z.span().class("px-4 -ml-2 relative link-span");
                for span in spans.into_iter() {
                    match span {
                        Span::Kanji {
                            kanji: f_kanji,
                            yomi: f_yomi,
                            ..
                        } => {
                            let classes = if kanji == f_kanji {
                                if yomi == f_yomi {
                                    "text-blue-800"
                                } else if flag {
                                    "text-amber-800"
                                } else {
                                    "text-red-800"
                                }
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
                                .c(Z.rt().class("relative top-1 opacity-0").c("-")));
                        }
                    }
                }
                rel_section_body = rel_section_body.c(word_ruby);
            }
        }
        let rel_section = Z
            .div()
            .class("flex flex-row gap-4 pt-2")
            .c(rel_section_header)
            .c(rel_section_body);
        related_words = related_words.c(rel_section);
    }

    let word_header = Z.h1().class("text-4xl px-6 py-3").c(word_header_ruby);

    let any_defs = !dict_defs.is_empty();

    let defs_section = Z.div().class("flex flex-col gap-2").cs(
        dict_defs,
        |DictDef {
             dict,
             defs,
             spelling: _,
             reading: _,
         }| {
            // intersperse with commas
            // bit ugly but it's fine
            let mut it = defs.0.into_iter().peekable();

            labelled_value(
                &dict,
                Z.div().cv({
                    let mut v = Vec::new();
                    while let Some(def) = it.next() {
                        v.push(Z.span().c(def));
                        if it.peek().is_some() {
                            v.push(Z.span().c(", "));
                        }
                    }
                    v
                }),
            )
        },
    );

    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4")
            .c(Z.h2().class("text-2xl font-bold pb-3").c(title))
    };

    let freq_label = if max_freq == 0 {
        "unknown".to_string()
    } else {
        let max_freq_percentile = 100.0 * (max_freq as f32 / 160836 as f32);
        format!("top {:.2}%", max_freq_percentile)
    };

    let mut html = Z
        .div()
        .id("defs")
        .class("flex flex-col gap-2")
        .c(word_header)
        .c(section("Memory").c(memory_section))
        .c(
            section("Stats").c(Z.div().class("flex flex-col").c(labelled_value_c(
                "frequency",
                freq_label,
                "font-bold",
            ))),
        );
    if any_links {
        html = html.c(section("Links").c(related_words));
    }
    if any_defs {
        html = html.c(section("Definitions").c(defs_section));
    }

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
    let tailwind_preamble = Z.stylesheet("/static/output.css");

    let sidebar = Z
        .div()
        .id("sidebar")
        .class("w-4/12 grow-0 p-6 bg-gray-300 overflow-auto shadow-left-side")
        .c(Z.div()
            .id("defs")
            .c(Z.span().c("Click on a word to look it up")));

    let mut words = Vec::new();

    for (tok, id) in book.into_iter() {
        if tok == "\n" {
            words.push(Z.br());
        } else {
            let text = tok.to_owned();
            if let Some(term) = terms.get(&id) {
                if let (_spelling, Some(_reading)) = term.surface_form() {
                    let card = SurfaceForm::get_by_id(&pool, id).await?.card;
                    let state_classes = match card {
                        None => "decoration-transparent",
                        Some(card) => match card.state {
                            State::New => "decoration-blue-600",
                            State::Review => "decoration-green-600",
                            _ => "decoration-amber-600",
                        },
                    };
                    words.push(
                        Z.a()
                            .href(format!("/word_info/{}", id.0))
                            .class(format!(
                                "{state_classes} decoration-2 decoration-solid underline underline-offset-4 word-{}",
                                id.0
                            ))
                            // .up_instant()
                            // .up_preload()
                            .up_target("#defs")
                            .up_cache("false")
                            .c(text),
                    );
                    continue;
                }
            }
            words.push(Z.span().c(text));
        }
    }

    let main = Z
        .div()
        .id("main")
        .class("w-6/12 grow-0 p-12 bg-gray-200 overflow-scroll text-2xl/10")
        .up_nav()
        .cv(words);

    let head = Z.head().c(unpoly_preamble).c(tailwind_preamble);
    let body = Z
        .body()
        .class("h-screen w-screen bg-gray-100 relative flex flex-row overflow-hidden")
        .c(Z.div().class("grow bg-gray-200").id("left-spacer"))
        .c(main)
        .c(sidebar)
        .c(Z.div().class("grow bg-gray-300").id("right-spacer"));
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
    pool: &PgPool,
    _kd: &KanjiDic,
    session: &mut UnidicSession,
    epub_file: impl AsRef<Path>,
) -> Result<(
    Vec<(String, LemmaId)>,
    HashMap<LemmaId, features::Term>,
)> {
    let _yomi_freq: HashMap<furi::Span, u64> = HashMap::new();
    let _yomi_uniq_freq: HashMap<furi::Span, u64> = HashMap::new();
    let _lemma_freq: HashMap<LemmaId, u64> = HashMap::new();
    let _name_count = 0;

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

    Ok((
        tokens
            .iter()
            .map(|(x, y)| (x.to_string(), y.to_owned()))
            .collect(),
        terms,
    ))
}
