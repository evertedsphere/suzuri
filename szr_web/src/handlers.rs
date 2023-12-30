use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use snafu::Snafu;
use sqlx::PgPool;
use szr_dict::{Def, DefContent};
use szr_html::{Doc, DocRender, Render, RenderExt, Z};
use szr_srs::{MemoryStatus, Mneme, Params, ReviewGrade};
use szr_textual::{Line, Token};
use tracing::warn;
use uuid::Uuid;

use crate::models::{
    get_mneme_refresh_batch, ContextSentence, ContextSentenceToken, LookupData, MnemeRefreshBatch,
    MnemeRefreshDatum, RelativeRubySpan, RubyMatchType, RubySpan, SentenceGroup, SpanLink,
    VariantId,
};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Error)))]
pub enum Error {
    #[snafu(whatever, display("{message}: {source:?}"))]
    CatchallError {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error>, Some)))]
        source: Option<Box<dyn std::error::Error>>,
    },
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal error: {}", self),
        )
            .into_response()
    }
}

fn is_punctuation(s: &str) -> bool {
    s.chars().count() == 1
        && matches!(
            s.chars().next(),
            Some('「' | '」' | '。' | '、' | '？' | '！' | '　')
        )
}

fn labelled_value_c<'a, V: Render>(label: &'a str, value: V, classes: &'static str) -> Doc {
    Z.div()
        .class("flex flex-row gap-4")
        .c(Z.span()
            .class("font-bold text-gray-600 shrink-0 whitespace-nowrap")
            .c(label))
        .c(Z.span().class(classes).c(value))
}

fn labelled_value<V: Render>(label: &str, value: V) -> Doc {
    labelled_value_c(label, value, "")
}

pub async fn handle_create_mneme(
    State(pool): State<PgPool>,
    Path((variant_id, grade)): Path<(Uuid, ReviewGrade)>,
) -> Result<impl IntoResponse> {
    let w = [
        0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49, 0.14, 0.94, 2.18, 0.05, 0.34, 1.26, 0.29,
        2.61,
    ];
    let params = Params::from_weight_vector(w);

    let new_mneme_id = Mneme::create(&pool, &params, grade).await.unwrap();
    // TODO transaction
    sqlx::query!(
        r#"UPDATE variants SET mneme_id = $2 WHERE id = $1"#,
        variant_id,
        new_mneme_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let mneme = Mneme::get_by_id(&pool, new_mneme_id).await.unwrap();

    Ok(build_memory_section(MemorySectionData::KnownItem {
        variant_id: VariantId(variant_id),
        mneme,
    })
    .render_to_html())
}

pub async fn handle_review_mneme(
    State(pool): State<PgPool>,
    Path((variant_id, mneme_id, grade)): Path<(Uuid, Uuid, ReviewGrade)>,
) -> Result<impl IntoResponse> {
    let w = [
        0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49, 0.14, 0.94, 2.18, 0.05, 0.34, 1.26, 0.29,
        2.61,
    ];
    let params = Params::from_weight_vector(w);
    Mneme::review_by_id(&pool, mneme_id, &params, grade)
        .await
        .unwrap();
    let mneme = Mneme::get_by_id(&pool, mneme_id).await.unwrap();

    Ok(build_memory_section(MemorySectionData::KnownItem {
        variant_id: VariantId(variant_id),
        mneme,
    })
    .render_to_html())
}

enum MemorySectionData {
    NewVariant { variant_id: VariantId },
    KnownItem { variant_id: VariantId, mneme: Mneme },
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

// Yes, this is ugly. No, I don't know how to work around this short of having Tailwind
// expose colour variables somehow.
fn get_decoration_colour_rule(variant_id: VariantId, is_due: bool, status: MemoryStatus) -> String {
    let colour = if is_due {
        "rgb(153 27 27)"
    } else {
        match status {
            MemoryStatus::Learning => "#2563eb",
            MemoryStatus::Relearning => "#d97706",
            // used to be #16a34a
            MemoryStatus::Reviewing => "transparent",
        }
    };

    format!(
        ".variant-{} {{ text-decoration-color: {colour}; }} \n ",
        variant_id.0
    )
}

fn build_memory_section(data: MemorySectionData) -> (Doc, Doc) {
    let mut status_block = Z.div().class("flex flex-col gap-2");

    let mut poll_interval = None;

    let variant_id = match &data {
        MemorySectionData::NewVariant { variant_id } => variant_id,
        MemorySectionData::KnownItem { variant_id, .. } => variant_id,
    };

    let mut decoration_colour_rule = None;

    match &data {
        MemorySectionData::NewVariant { .. } => {
            status_block = status_block.c(labelled_value_c("Status", "New", "text-gray-800"))
        }
        MemorySectionData::KnownItem { mneme, .. } => {
            let now = chrono::Utc::now();
            let diff = mneme.next_due - now;
            let diff_secs = diff.num_seconds();
            let raw_diff_str = english_relative_time(diff_secs.abs() as u64);
            if diff.num_days().abs() < 2 {
                // Checking for the review state is cheap, but it's still not
                // very useful to do it too frequently if the interval is still
                // long.
                // Here we choose to aim for 5 updates over the life of the review.
                poll_interval = Some(std::cmp::max(10, diff_secs.abs() / 5));
                warn!(poll_interval, "interval");
            }
            let diff_str = if diff_secs < 0 {
                format!("{raw_diff_str} ago")
            } else if diff_secs > 0 {
                format!("in {raw_diff_str}")
            } else {
                "right now".to_string()
            };
            status_block = status_block
                .c(labelled_value_c(
                    "Status",
                    format!("{:?}", mneme.state.status),
                    "",
                ))
                .c(labelled_value_c("Due", format!("{}", diff_str), ""));
            decoration_colour_rule = Some(get_decoration_colour_rule(
                *variant_id,
                diff_secs < 0,
                mneme.state.status,
            ));
        }
    };

    // review block

    let create_link = |grade| match &data {
        MemorySectionData::NewVariant { variant_id } => {
            format!("/variants/{}/create-mneme/{}", variant_id.0, grade)
        }
        MemorySectionData::KnownItem { variant_id, mneme } => {
            format!("/variants/{}/review/{}/{}", variant_id.0, mneme.id, grade)
        }
    };

    let review_button = |grade, extra_classes, text| {
        let base_classes = "";
        Z.a()
            .class(format!("{base_classes} {extra_classes}"))
            .href(create_link(grade))
            .c(text)
            .up_target("#memory, #dynamic-patch:after")
            .up_method("post")
    };

    let review_actions_block = Z
        .div()
        .up_nav()
        .class("flex flex-col gap-2")
        .c(labelled_value_c(
            "Review as",
            Z.div()
                .class("flex flex-row gap-2")
                .c(review_button("Fail", "text-red-800", "Fail"))
                .c(review_button("Hard", "text-yellow-900", "Hard"))
                .c(review_button("Okay", "text-green-800", "Okay"))
                .c(review_button("Easy", "text-blue-800", "Easy")),
            "font-bold",
        ));

    let mut memory_block = Z.div().class("flex flex-col gap-2").id("memory");

    memory_block = memory_block.c(status_block).c(review_actions_block);

    if let Some(poll_interval) = poll_interval {
        memory_block = memory_block
            .up_poll()
            .up_interval((1000 * poll_interval).to_string());
    }

    let dynamic_css_patch = decoration_colour_rule.map(|rule| Z.style().raw_text(&rule));

    let dynamic_section = Z.div().id("dynamic-patch").c(dynamic_css_patch);

    (memory_block, dynamic_section)
}

pub async fn handle_variant_lookup_view(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Doc> {
    render_variant_lookup(pool, VariantId(id)).await
}

pub async fn render_variant_lookup(pool: PgPool, id: VariantId) -> Result<Doc> {
    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4")
            .c(Z.h2().class("text-2xl font-bold pb-3").c(title))
    };

    let LookupData {
        meanings,
        variant_id,
        related_words,
        sentences,
        ruby,
        mneme,
    } = LookupData::get_by_id(&pool, id).await.unwrap();

    let mut header = Z.h1().class("text-4xl px-6 py-3").lang("ja");
    if let Some(ruby) = ruby {
        for ruby_span in ruby {
            let r = match ruby_span {
                RubySpan::Kana { kana } => Z.ruby().c(kana),
                RubySpan::Kanji { spelling, reading } => Z
                    .ruby()
                    .c(spelling)
                    .c(Z.rt().c(reading).class("relative top-1")),
            };
            header = header.c(r);
        }
    } else {
        header = header.c("unknown");
    }

    let mut related_section = Z.div().class("flex flex-col gap-4 text-lg").lang("ja");

    let mut any_links = false;

    for SpanLink {
        index: _,
        ruby,
        examples,
    } in related_words
    {
        let ruby_doc = ruby.to_doc();
        let rel_row_header = ruby_doc.class("text-4xl text-center w-1/6 self-center");
        let Some(examples) = examples else { continue };
        let mut rel_row_body = Z
            .div()
            .class("flex flex-row flex-wrap text-xl self-center w-5/6 overflow-hidden -ml-4");
        for example_raw in examples {
            any_links = true;
            let mut word_ruby = Z.span().class("px-4 -ml-2 relative link-span");
            for span in example_raw.ruby {
                let span_rendered = match span {
                    RelativeRubySpan {
                        ruby_span: RubySpan::Kana { kana, .. },
                        ..
                    } => Z
                        .ruby()
                        .class("text-gray-600")
                        .c(kana)
                        .c(Z.rt().class("relative top-1 opacity-0").c("-")),
                    RelativeRubySpan {
                        ruby_span: RubySpan::Kanji { spelling, reading },
                        match_type,
                    } => {
                        let classes = match match_type {
                            RubyMatchType::FullMatch => "text-blue-800",
                            RubyMatchType::AlternateReading => "text-amber-800",
                            RubyMatchType::NonMatch => "text-gray-600",
                        };
                        Z.ruby()
                            .class(classes)
                            .c(spelling)
                            .c(Z.rt().class("relative top-1").c(reading))
                    }
                };
                word_ruby = word_ruby.c(span_rendered);
            }

            rel_row_body = rel_row_body.c(Z
                .a()
                .href(format!("/variants/view/{}", example_raw.variant_id.0))
                .up_preload()
                .up_target("#defs,#dynamic-patch:after")
                .up_cache("false")
                .c(word_ruby));
        }
        let rel_row = Z
            .div()
            .class("flex flex-row gap-4 pt-2")
            .c(rel_row_header)
            .c(rel_row_body);
        related_section = related_section.c(rel_row);
    }

    let any_defs = !meanings.is_empty();

    let defs_section = Z.div().class("flex flex-col gap-2").cs(
        meanings,
        |Def {
             dict_name,
             content,
             tags,
             ..
         }| {
            let lang = match dict_name.as_str() {
                "dic.pixiv.net" | "旺文社" => "ja",
                _ => "en",
            };
            // TODO only break if just one result for that dictionary?
            // might be weirdly inconsistent
            match content {
                DefContent::Plain(content) => {
                    let tags = Z
                        .span()
                        .class("flex flex-row gap-1")
                        .cs(tags.0, |tag| Z.span().c(tag).class("text-gray-600 italic"));
                    labelled_value(
                        &dict_name,
                        Z.div().class("flex flex-col").lang(lang).c(tags).c({
                            let mut it = content.into_iter().peekable();
                            let mut s = String::new();
                            // intersperse with commas
                            // bit ugly but it's fine
                            while let Some(def) = it.next() {
                                s += &def;
                                if it.peek().is_some() {
                                    s += ", ";
                                }
                            }
                            s
                        }),
                    )
                }
                DefContent::Oubunsha { definitions, .. } => labelled_value(
                    &dict_name,
                    Z.div().lang(lang).c(Z.ul().cs(definitions, |(def, ex)| {
                        let mut r = Z.li().c(def);
                        if let Some(ex) = ex {
                            r = r.c(Z.span().c(ex).class("text-gray-600"));
                        }
                        r
                    })),
                ),
            }
        },
    );

    let any_sentences = !sentences.is_empty();

    // idk why but it looks nicer with the pt-1
    let sentences_section = Z.div().class("flex flex-col gap-6 pt-1").cs(
        sentences,
        |SentenceGroup {
             doc_title,
             sentences,
             num_hits,
             ..
         }| {
            // let num_hits_shown = sentences.len();
            Z.div()
                .class("flex flex-col gap-1")
                .cs(sentences, |ContextSentence { tokens, .. }| {
                    let ret = Z.div().lang("ja").cs(
                        tokens,
                        |ContextSentenceToken {
                             variant_id,
                             content,
                             is_active_word,
                         }| {
                            let mut z = Z.a().c(content);
                            if let Some(id) = variant_id {
                                z = z
                                    .class(format!("underlined-word variant-{}", id.0))
                                    .href(format!("/variants/view/{}", id.0))
                                    .up_target("#defs,#dynamic-patch:after")
                                    .up_cache("false");
                            };
                            if is_active_word {
                                z = z.class("text-blue-800");
                            }
                            z
                        },
                    );
                    ret
                })
                .c(Z.div()
                    .class("flex flex-row justify-between grow text-sm gap-2 pt-1")
                    .c(Z.span()
                        .c(format!("({num_hits} hits)"))
                        .class("grow text-gray-500 shrink-0 whitespace-nowrap"))
                    .c(Z.span()
                        .c(doc_title)
                        .class("font-bold text-gray-600 w-2/3 text-right truncate")
                        .lang("ja")))
        },
    );

    let mut lookup_view = Z.div().id("defs").class("flex flex-col gap-2");

    let memory_section_data = match mneme {
        None => MemorySectionData::NewVariant { variant_id },
        Some(mneme) => MemorySectionData::KnownItem { variant_id, mneme },
    };
    let (memory_section, memory_dynamic_css) = build_memory_section(memory_section_data);

    lookup_view = lookup_view.c(header);
    lookup_view = lookup_view.c(section("Memory").c(memory_section));
    lookup_view = lookup_view.c(section("Definitions").c(if any_defs {
        defs_section
    } else {
        Z.span()
            .c("No definitions found in any available dictionaries.")
    }));
    lookup_view = lookup_view.c(section("Examples").c(if any_sentences {
        sentences_section
    } else {
        Z.span().c("This word does not appear to be used in (the already-read parts of) any books in your library.")
    }));
    lookup_view = lookup_view.c(section("Links").c(if any_links {
        related_section
    } else {
        Z.span()
            .c("This word has no morphological links to other words in the database.")
    }));

    let transient_stylesheet = Z.style().raw_text(&format!(
        ".variant-{} {{ background-color: #cccccc; }}",
        variant_id.0
    ));

    let html = Z
        .html()
        .c(lookup_view.c(transient_stylesheet))
        .c(memory_dynamic_css);

    Ok(html)
}

// returns the new contents for #dynamic
fn render_srs_style_patch(id: i32, batch: MnemeRefreshBatch) -> Doc {
    let mut r = Z
        .div()
        .id("dynamic")
        .c(Z.style().cs(
            batch.mneme_refresh_data.0,
            |MnemeRefreshDatum {
                 variant_id,
                 is_due,
                 status,
             }| { get_decoration_colour_rule(variant_id, is_due, status) },
        ))
        .c(Z.div().id("dynamic-patch"));
    if let Some(next_refresh_in_sec) = batch.next_refresh_in_sec {
        r = r
            .up_poll()
            .up_source(format!("/books/{}/get-review-patch", id))
            .up_interval((1000 * next_refresh_in_sec).to_string());
    }
    r
}

pub async fn handle_refresh_srs_style_patch(
    State(pool): State<PgPool>,
    Path(book_id): Path<i32>,
) -> Result<Doc> {
    let refresh_data = get_mneme_refresh_batch(&pool).await.unwrap();
    let dynamic_section = render_srs_style_patch(book_id, refresh_data);
    Ok(dynamic_section)
}

pub async fn handle_books_view(State(pool): State<PgPool>, Path(id): Path<i32>) -> Result<Doc> {
    let doc = szr_textual::get_doc(&pool, id).await.unwrap();

    let refresh_data = get_mneme_refresh_batch(&pool).await.unwrap();
    let dynamic_section = render_srs_style_patch(id, refresh_data);

    let unpoly_preamble = (
        Z.script()
            .src("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.js"),
        Z.stylesheet("https://cdn.jsdelivr.net/npm/unpoly@3.5.2/unpoly.min.css"),
    );
    let fonts_preamble = (
        Z.link()
            .rel("preconnect")
            .href("https://fonts.googleapis.com"),
        Z.link()
            .rel("preconnect")
            .href("https://fonts.gstatic.com")
            .crossorigin(),
        Z.stylesheet("https://fonts.googleapis.com/css2?family=Sawarabi+Gothic&display=swap"),
    );
    let tailwind_preamble = Z.stylesheet("/static/output.css");

    let sidebar = Z
        .div()
        .id("sidebar")
        .class("w-4/12 grow-0 p-6 bg-gray-300 overflow-auto shadow-left-side")
        .c(Z.div()
            .id("defs")
            .c(Z.span().c("Click on a word to look it up")));

    let mut lines = Vec::new();

    for Line {
        doc_id: _,
        index: line_index,
    } in doc.lines
    {
        let mut line = Z.div();
        let mut token_index = 0;
        while let Some(Token {
            content,
            variant_id,
            ..
        }) = doc.tokens.get(&(line_index, token_index))
        {
            let mut rendered_token = Z.span().c(content.as_str());
            if !is_punctuation(content)
                && let Some(id) = variant_id
            {
                let base_classes = format!("underlined-word variant-{}", id);
                rendered_token = Z
                    .a()
                    .href(format!("/variants/view/{}", id))
                    .up_target("#defs,#dynamic-patch:after")
                    .up_cache("false")
                    .c(content.as_str())
                    .class(base_classes);
            }
            line = line.c(rendered_token);
            token_index += 1;
        }

        lines.push(line);
    }

    let main = Z
        .div()
        .id("main")
        .class("w-6/12 grow-0 p-12 bg-gray-200 overflow-scroll text-2xl/10")
        .up_nav()
        .lang("ja")
        .c(
            dynamic_section, // clears this when dynamic is updated
        )
        .cv(lines);

    let head = Z
        .head()
        .c(unpoly_preamble)
        .c(fonts_preamble)
        .c(tailwind_preamble);
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
        .c(Z.html().lang("en").c(head).c(body));

    Ok(ret)
}
