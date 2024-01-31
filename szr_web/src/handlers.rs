use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
};
use chrono::Utc;
use snafu::Snafu;
use sqlx::PgPool;
use szr_dict::DefContent;
use szr_html::{Doc, DocRender, Render, RenderExt, Z};
use szr_srs::{MemoryStatus, Mneme, Params, ReviewGrade};
use szr_textual::{Line, Token};
use tracing::warn;
use uuid::Uuid;

use crate::models::{
    get_mneme_refresh_batch, get_related_words, get_sentences, ContextBlock, ContextSentenceToken,
    DefGroup, LookupData, MnemeRefreshBatch, MnemeRefreshDatum, RelativeRubySpan, RubyMatchType,
    RubySpan, SentenceGroup, SpanLink, TagDefGroup, VariantId, VariantRuby,
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
            Some(
                '「' | '」'
                    | '。'
                    | '、'
                    | '？'
                    | '！'
                    | '　'
                    | '─'
                    | '）'
                    | '（'
                    | '…'
                    | '︙'
                    | '《'
                    | '》'
            )
        )
}

fn labelled_value_c<'a, V: Render, W: Render>(label: W, value: V, classes: &'static str) -> Doc {
    Z.div()
        .class("flex flex-row gap-4 items-baseline")
        .c(Z.span()
            .class("font-bold text-gray-600 shrink-0 whitespace-nowrap")
            .c(label))
        .c(Z.div().class(classes).c(value))
}

fn labelled_value<W: Render, V: Render>(label: V, value: W) -> Doc {
    labelled_value_c(label, value, "")
}

pub async fn handle_create_mneme(
    State(pool): State<PgPool>,
    Path((variant_id, grade)): Path<(Uuid, ReviewGrade)>,
) -> Result<impl IntoResponse> {
    let (variant_id, mneme) = render_create_mneme(&pool, variant_id, grade).await?;
    let r = build_memory_section(MemorySectionData::KnownItem { variant_id, mneme });
    Ok(r.render_to_html())
}

pub async fn render_create_mneme(
    pool: &PgPool,
    variant_id: Uuid,
    grade: ReviewGrade,
) -> Result<(VariantId, Mneme)> {
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
    .execute(pool)
    .await
    .unwrap();

    let mneme = Mneme::get_by_id(&pool, new_mneme_id).await.unwrap();

    Ok((VariantId(variant_id), mneme))
}

pub async fn handle_bulk_create_mneme(
    State(pool): State<PgPool>,
    Path((doc_id, line_index, grade)): Path<(u32, u32, ReviewGrade)>,
) -> Result<impl IntoResponse> {
    let new_variant_ids = sqlx::query_scalar!(
        r#"
select v.id "id!: Uuid"
from tokens
join surface_forms s on surface_form_id = s.id
join variants v on s.variant_id = v.id
where doc_id = $1
and line_index = $2
and v.mneme_id IS NULL;
"#,
        doc_id as i32,
        line_index as i32,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    struct DueVariant {
        variant_id: Uuid,
        mneme_id: Uuid,
    }

    let due_variant_ids = sqlx::query_as!(
        DueVariant,
        r#"
select v.id "variant_id!: Uuid", m.id "mneme_id!: Uuid"
from tokens
join surface_forms s on surface_form_id = s.id
join variants v on s.variant_id = v.id
join mnemes m on m.id = v.mneme_id
where doc_id = $1
and line_index = $2
and m.next_due < CURRENT_TIMESTAMP;
"#,
        doc_id as i32,
        line_index as i32,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut css: Vec<String> = Default::default();
    let now = Utc::now();

    for variant_id in new_variant_ids {
        let (_, mneme) = render_create_mneme(&pool, variant_id, grade).await.unwrap();
        css.push(get_decoration_colour_rule(
            VariantId(variant_id),
            // technically always false, but
            mneme.next_due < now,
            mneme.state.status,
        ));
    }

    for DueVariant {
        variant_id,
        mneme_id,
    } in due_variant_ids
    {
        let (_, mneme) = render_review_mneme(&pool, variant_id, mneme_id, grade)
            .await
            .unwrap();
        css.push(get_decoration_colour_rule(
            VariantId(variant_id),
            mneme.next_due < now,
            mneme.state.status,
        ));
    }

    let r = Z
        .div()
        .hx_swap_oob_raw("beforeend:#dynamic-patch")
        .c(Z.style().raw_text(&css.concat()));

    Ok(r)
}

pub async fn handle_review_mneme(
    State(pool): State<PgPool>,
    Path((variant_id, mneme_id, grade)): Path<(Uuid, Uuid, ReviewGrade)>,
) -> Result<impl IntoResponse> {
    let (variant_id, mneme) = render_review_mneme(&pool, variant_id, mneme_id, grade).await?;

    Ok(build_memory_section(MemorySectionData::KnownItem { variant_id, mneme }).render_to_html())
}

pub async fn render_review_mneme(
    pool: &PgPool,
    variant_id: Uuid,
    mneme_id: Uuid,
    grade: ReviewGrade,
) -> Result<(VariantId, Mneme)> {
    let w = [
        0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49, 0.14, 0.94, 2.18, 0.05, 0.34, 1.26, 0.29,
        2.61,
    ];
    let params = Params::from_weight_vector(w);
    Mneme::review_by_id(pool, mneme_id, &params, grade)
        .await
        .unwrap();
    let mneme = Mneme::get_by_id(pool, mneme_id).await.unwrap();

    Ok((VariantId(variant_id), mneme))
}

pub async fn handle_toggle_favourite_line(
    State(pool): State<PgPool>,
    Path((doc_id, line_index)): Path<(i32, i32)>,
) -> Result<Doc> {
    let new_status = sqlx::query_scalar!(
        "update lines set is_favourite = not is_favourite where doc_id = $1 and index = $2 returning is_favourite",
        doc_id,
        line_index
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    Ok(build_star_button(doc_id, line_index, new_status))
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
        ".variant-{} {{ text-decoration-color: {colour}; }} ",
        variant_id.0
    )
}

fn build_memory_section(data: MemorySectionData) -> (Doc, Doc) {
    let mut srs_status_block = Z.div().class("flex flex-col gap-2");
    let mut poll_interval = None;
    let variant_id = match &data {
        MemorySectionData::NewVariant { variant_id } => variant_id,
        MemorySectionData::KnownItem { variant_id, .. } => variant_id,
    };

    let mut decoration_colour_rule = None;

    match &data {
        MemorySectionData::NewVariant { .. } => {
            srs_status_block =
                srs_status_block.c(labelled_value_c("Status", "New", "text-gray-800"))
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
            srs_status_block = srs_status_block
                .c(labelled_value_c(
                    "Status",
                    format!("{:?}", mneme.state.status),
                    "status",
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

    let review_button = |grade, extra_classes, text, key| {
        let base_classes = "";
        Z.a()
            .role("button")
            .class(format!("{base_classes} {extra_classes}"))
            .hx_post(create_link(grade))
            .hx_trigger(format!("click, keydown[key=='{key}'] from:body"))
            .c(text)
        // .up_target("#memory, #dynamic-patch:after")
    };

    let review_actions_block = Z.div().class("flex flex-col gap-2").c(labelled_value_c(
        "Review as",
        Z.div()
            .class("flex flex-row gap-2")
            .c(review_button("Fail", "text-red-800", "Fail", "j"))
            .c(review_button("Hard", "text-yellow-900", "Hard", "k"))
            .c(review_button("Okay", "text-green-800", "Okay", "l"))
            .c(review_button("Easy", "text-blue-800", "Easy", ";")),
        "font-bold",
    ));

    let mut memory_block = Z
        .div()
        .class("flex flex-col gap-2")
        .id("memory")
        .hx_swap_oob_enable();

    memory_block = memory_block
        .c(srs_status_block)
        .c(review_actions_block)
        .c(Z.style().c(format!(
            ".variant-{} {{ background-color: rgb(209 213 219); }}",
            variant_id.0
        )));

    if let Some(poll_interval) = poll_interval {
        memory_block = memory_block.hx_trigger(format!("every {}s", poll_interval));
    }

    let dynamic_css_patch = decoration_colour_rule.map(|rule| Z.style().raw_text(&rule));

    let dynamic_section = Z
        .div()
        .id("dynamic-patch")
        .c(dynamic_css_patch)
        .hx_swap_oob_raw("beforeend:#dynamic-patch");

    (memory_block, dynamic_section)
}

pub async fn handle_variant_lookup_view(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>> {
    Ok(render_variant_lookup(pool, VariantId(id))
        .await
        .unwrap()
        .render_to_html())
}

fn render_lookup_related_section(related_words: Vec<SpanLink>) -> Result<Doc> {
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
            .class("flex flex-row flex-wrap text-xl self-center w-5/6 overflow-hidden gap-2");
        for example_raw in examples {
            any_links = true;
            let mut word_ruby = Z.span().class("px-2");
            for span in example_raw.ruby {
                let span_rendered = match span {
                    RelativeRubySpan {
                        ruby_span: RubySpan::Kana { kana, .. },
                        ..
                    } => Z.ruby(&kana, None, Some("text-gray-600")),
                    RelativeRubySpan {
                        ruby_span: RubySpan::Kanji { spelling, reading },
                        match_type,
                    } => {
                        let classes = match match_type {
                            RubyMatchType::FullMatch => "text-blue-800",
                            RubyMatchType::AlternateReading => "text-amber-800",
                            RubyMatchType::NonMatch => "text-gray-600",
                        };
                        Z.ruby(&spelling, Some(&reading), Some(classes))
                    }
                };
                word_ruby = word_ruby.c(span_rendered);
            }

            rel_row_body = rel_row_body.c(Z
                .a()
                .role("button")
                .hx_get(format!("/variants/view/{}", example_raw.variant_id.0))
                .class(format!("variant variant-{}", example_raw.variant_id.0))
                .hx_swap("none")
                // .up_target("#lookup-header, #lookup-memory, #lookup-definitions, #lookup-examples, #lookup-links, #dynamic-patch:after")
                .c(word_ruby));
        }
        let rel_row = Z
            .div()
            .class("flex flex-row gap-4 pt-2")
            .c(rel_row_header)
            .c(rel_row_body);
        related_section = related_section.c(rel_row);
    }
    let r = if any_links {
        related_section
    } else {
        Z.span().class("text-gray-600 italic").c(
            "This word, in this form, has no morphological links to other words in the database.",
        )
    };
    Ok(Z.div().id("lookup-links").hx_swap_oob_enable().c(r))
}

pub async fn handle_lookup_related_section(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Doc> {
    let related_words = get_related_words(&pool, 5, 5, VariantId(id)).await.unwrap();
    render_lookup_related_section(related_words)
}

pub async fn handle_lookup_examples_section(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Doc> {
    let sentences = get_sentences(&pool, VariantId(id), 5, 20).await.unwrap();

    let any_sentences = !sentences.is_empty();
    let sentences_section = Z.div().class("flex flex-col gap-6 pt-1").cs(
        sentences,
        |SentenceGroup {
             doc_title,
             sentences,
             num_hits,
             doc_id,
             ..
         }| {
            // let num_hits_shown = sentences.len();
            Z.div()
                .class("flex flex-col gap-2")
                .cs(
                    sentences,
                    |ContextBlock {
                         hit_context,
                         hit_pre_context,
                         hit_post_context,
                         is_favourite,
                         line_index,
                         ..
                     }| {
                        let render_line = |extra_classes, hit_line| {
                            Z.span().cs(
                                hit_line,
                                |ContextSentenceToken {
                                     variant_id,
                                     content,
                                     ..
                                 }| {
                                    let mut z = Z
                                        .a()
                                        .role("button")
                                        .c(content.clone())
                                        .class(extra_classes);
                                    if !is_punctuation(&content)
                                        && let Some(id) = variant_id
                                    {
                                        z = z
                                            .class(format!("variant variant-{}", id.0))
                                            .hx_get(format!("/variants/view/{}", id.0))
                                            .hx_swap("none")
                                    };
                                    z
                                },
                            )
                        };
                        let mut ret = Z.div().lang("ja");

                        let star_button = build_star_button(doc_id, line_index, is_favourite);

                        ret = ret.c(star_button);
                        ret = ret
                            .cs(hit_pre_context, |line| render_line("text-gray-500", line))
                            .cs(hit_context, |line| render_line("", line))
                            .cs(hit_post_context, |line| render_line("text-gray-500", line));
                        ret
                    },
                )
                .c(Z.div()
                    .class("flex flex-row justify-between grow text-sm gap-3 pt-1")
                    .c(Z.span()
                        .c({
                            if num_hits == 1 {
                                "(1 hit)".to_owned()
                            } else {
                                format!("({num_hits} hits)")
                            }
                        })
                        .class("grow text-gray-500 shrink-0 whitespace-nowrap"))
                    .c(Z.a()
                        .c(doc_title)
                        .class("font-bold text-gray-600 w-2/3 text-right truncate")
                        .href(format!("/books/{}/view/page/1", doc_id))
                        .lang("ja")))
        },
    );

    let r = if any_sentences {
        sentences_section
    } else {
        Z.span()
            .class("text-gray-600 italic")
            .c("This word, in this form, does not appear to be used in ")
            .c("(the already-read parts of) any books in your library.")
    };

    Ok(Z.div().id("lookup-examples").hx_swap_oob_enable().c(r))
}

fn build_star_button(doc_id: i32, line_index: i32, is_favourite: bool) -> Doc {
    let star_icon = if is_favourite { "bxs-star" } else { "bx-star" };
    let star_button_class = if is_favourite { "favourite" } else { "" };
    let star_button = Z
        .a()
        .class(format!("favourite-btn {star_button_class}"))
        .role("button")
        .title("Set favourite line (unscoped)")
        .c(Z.i().class(format!("bx {star_icon} text-yellow-800")))
        .hx_swap("outerHTML")
        .hx_post(format!("/lines/toggle-favourite/{}/{}", doc_id, line_index));
    star_button
}

pub async fn render_variant_lookup(pool: PgPool, id: VariantId) -> Result<Vec<Doc>> {
    let LookupData {
        meanings,
        variant_id,
        ruby,
        mneme,
        sibling_variants_ruby,
    } = LookupData::get_by_id(&pool, id).await.unwrap();

    let mut selected_variant_ruby = Z.h1().class("text-4xl").lang("ja");
    if let Some(ruby) = ruby {
        for ruby_span in ruby {
            selected_variant_ruby = selected_variant_ruby.c(ruby_span.to_doc());
        }
    } else {
        selected_variant_ruby = selected_variant_ruby.c("え？");
    }

    let alternates_row = if sibling_variants_ruby.is_empty() {
        None
    } else {
        let alternates = sibling_variants_ruby
            .into_iter()
            .map(|VariantRuby { variant_id, ruby }| {
                Z.a()
                    .role("button")
                    .hx_get(format!("/variants/view/{}", variant_id.0))
                    .hx_swap("none")
                    .class(format!("me-2 variant variant-{}", variant_id.0))
                    .lang("ja")
                    .cs(ruby, |ruby_span| ruby_span.to_doc())
            })
            .collect();
        let r = Z.div().class("flex flex-wrap flex-row").cv(alternates);
        Some(r)
    };

    let any_defs = !meanings.is_empty();

    let defs_section = Z.div().class("flex flex-col gap-3").cv({
        let mut all_defs = Vec::new();

        for DefGroup {
            dict_name,
            groups_by_tag,
        } in meanings
        {
            let lang = match dict_name.as_str() {
                "dic.pixiv.net" | "旺文社" => "ja",
                _ => "en",
            };

            let mut rendered_group_for_dict = Z.div().class("flex flex-col gap-2");

            for TagDefGroup { tags, contents } in groups_by_tag.0 {
                let mut rendered_group_for_tags = Z.div();
                let num_contents = contents.len();

                let tags = if tags.is_empty() {
                    None
                } else {
                    Some(
                        Z.span()
                            .class("flex flex-row gap-1")
                            .cs(tags, |tag| Z.span().c(tag).class("text-gray-600 italic")),
                    )
                };
                rendered_group_for_tags = rendered_group_for_tags.c(tags);

                let mut def_list = Z.ol();
                if num_contents > 1 {
                    def_list = def_list.class("list-decimal list-outside list-muted-markers");
                }
                for content in contents {
                    // TODO only break if just one result for that dictionary?
                    // might be weirdly inconsistent
                    let item = match content {
                        DefContent::Plain(content) => {
                            Z.li().lang(lang).c({
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
                            })
                        }
                        DefContent::Oubunsha { definitions, .. } => {
                            Z.div().lang(lang).c(Z.ul().cs(definitions, |(def, ex)| {
                                let mut r = Z.li().c(def);
                                if let Some(ex) = ex {
                                    r = r.c(Z.span().c(ex).class("text-gray-600"));
                                }
                                r
                            }))
                        }
                    };
                    def_list = def_list.c(item);
                }
                rendered_group_for_tags = rendered_group_for_tags.c(def_list);

                let group_for_tags_classes = "flex flex-col";

                rendered_group_for_tags = rendered_group_for_tags.class(group_for_tags_classes);
                rendered_group_for_dict = rendered_group_for_dict.c(rendered_group_for_tags);
            }

            all_defs.push(labelled_value(dict_name.as_str(), rendered_group_for_dict));
        }
        all_defs
    });

    let memory_section_data = match mneme {
        None => MemorySectionData::NewVariant { variant_id },
        Some(mneme) => MemorySectionData::KnownItem { variant_id, mneme },
    };
    let (memory_section, memory_dynamic_css) = build_memory_section(memory_section_data);

    let header_section = Z
        .div()
        .id("lookup-header")
        .hx_swap_oob_enable()
        .class("flex flex-col px-6 py-3 gap-3")
        .c(selected_variant_ruby)
        .c(labelled_value(
            Z.ruby("Variants", None, None),
            alternates_row.unwrap_or(Z.span().c("none found").class("text-gray-600 italic")),
        ));

    let memory_section = Z
        .div()
        .id("lookup-memory")
        .hx_swap_oob_enable()
        .c(memory_section);

    let defs_section = Z
        .div()
        .id("lookup-definitions")
        .hx_swap_oob_enable()
        .c(if any_defs {
            defs_section
        } else {
            Z.span()
                .class("text-gray-600 italic")
                .c("No definitions were found in any available dictionaries.")
        });

    let examples_section = Z
        .div()
        .id("lookup-examples")
        .hx_swap_oob_enable()
        .hx_trigger("load delay:200ms")
        .hx_get(format!("/variants/view/{}/example-sentences", id.0))
        .c(Z.span().class("text-gray-600 italic").c("Loading…"));

    let links_section = Z
        .div()
        .id("lookup-links")
        .hx_swap_oob_enable()
        .hx_trigger("load delay:200ms")
        .hx_get(format!("/variants/view/{}/related-words", id.0))
        .c(Z.span().class("text-gray-600 italic").c("Loading…"));

    let html = vec![
        header_section,
        memory_section,
        defs_section,
        examples_section,
        links_section,
        memory_dynamic_css,
    ];

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
    let mut interval_sec = 60;
    if let Some(next_refresh_in_sec) = batch.next_refresh_in_sec {
        interval_sec = next_refresh_in_sec.clamp(10, 60);
    }
    r = r
        .hx_get(format!("/books/{}/get-review-patch", id))
        .hx_swap_oob_enable()
        .hx_trigger(format!("every {}s", interval_sec));
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

pub async fn handle_books_view_text_section(
    State(pool): State<PgPool>,
    Path((id, page)): Path<(i32, i32)>,
) -> Result<Html<String>> {
    let r = build_books_view_text_section(&pool, id, page).await?;
    Ok(r.render_to_html())
}

pub async fn build_books_view_text_section(pool: &PgPool, id: i32, page: i32) -> Result<Vec<Doc>> {
    let doc = szr_textual::get_doc(&pool, id).await.unwrap();
    let mut lines = Vec::new();
    let lines_per_page = 200;
    let num_lines = doc.lines.len();
    let num_lines_to_skip = if page > 0 {
        lines_per_page * (page - 1)
    } else {
        0
    } as usize;

    let mut chars_read = 0;

    for (
        i,
        Line {
            doc_id,
            index: line_index,
            is_favourite,
        },
    ) in doc
        .lines
        .into_iter()
        .take(num_lines_to_skip + lines_per_page as usize)
        .enumerate()
    {
        let mut line = Z.div().class("line");
        let mut token_index = 0;

        // add the tokens
        while let Some(Token {
            content,
            variant_id,
            ..
        }) = doc.tokens.get(&(line_index, token_index))
        {
            token_index += 1;
            chars_read += content.chars().count();
            if i < num_lines_to_skip {
                continue;
            }

            let mut rendered_token = Z.span().c(content.as_str());

            if !is_punctuation(content)
                && let Some(id) = variant_id
            {
                let base_classes = format!("variant variant-{}", id);
                rendered_token = Z
                    .a()
                    .role("button")
                    .hx_get(format!("/variants/view/{}", id))
                    .hx_trigger("click, focus")
                    .hx_swap("none")
                    // TODO: only words that are useful; fetch srs data here
                    .tabindex("0")
                    .c(content.as_str())
                    .class(base_classes);
            }
            line = line.c(rendered_token);
        }

        if i < num_lines_to_skip {
            continue;
        }

        let line_control_buttons = Z
            .div()
            .class("flex flex-row gap-1")
            .c(Z.a()
                .role("button")
                .title("Grade all as Okay")
                .class("bulk-okay")
                .c(Z.i().class("bx bx-check-circle text-green-800"))
                .hx_swap("none")
                .hx_post(format!(
                    "/variants/bulk-review-for-line/{}/{}/Okay",
                    doc_id, line_index
                )))
            .c(Z.a()
                .role("button")
                .title("Grade all as Easy")
                .c(Z.i().class("bx bx-check-circle text-blue-800"))
                .hx_post(format!(
                    "/variants/bulk-review-for-line/{}/{}/Easy",
                    doc_id, line_index
                )))
            .c(build_star_button(doc_id, line_index, is_favourite));

        line = line.c(Z
            .div()
            .class("flex flex-col line-controls")
            .c(line_control_buttons)
            .c(Z.div().class("flex flex-row text-sm self-center").c(Z
                .span()
                .title(format!(
                    "line {} of {} (char {})",
                    line_index, num_lines, chars_read
                ))
                .c(format!(
                    "{:.1}%",
                    100.0 * line_index as f32 / num_lines as f32
                )))));

        lines.push(line);
    }

    if let Some(last_line) = lines.last_mut() {
        // beforeend on #main-text deletes the old page fsr
        // so we use afterend on the page itself
        *last_line = last_line
            .clone()
            .hx_get(format!("/books/{id}/view/page/{}/text-only", page + 1))
            .hx_trigger("intersect once")
            .hx_target(format!("#page-{page}"))
            .hx_swap("afterend");
    }

    let current_page = Z.div().id(format!("page-{page}")).cv(lines);
    let response_pages = vec![current_page];
    // if page >= 3 {
    //     response_pages.push(
    //         Z.div()
    //             .id(format!("page-{}", page - 2))
    //             .hx_swap_oob_enable()
    //             .c("deleted"),
    //     );
    // }

    Ok(response_pages)
}

pub async fn handle_books_view(
    State(pool): State<PgPool>,
    Path((id, page)): Path<(i32, i32)>,
) -> Result<Doc> {
    let refresh_data = get_mneme_refresh_batch(&pool).await.unwrap();
    let dynamic_section = render_srs_style_patch(id, refresh_data);

    let icons_preamble = Z.stylesheet("https://unpkg.com/boxicons@2.1.4/css/boxicons.min.css");
    let htmx_preamble = Z.script().src("/static/htmx.min.js");
    let fonts_preamble = (
        Z.link()
            .rel("preconnect")
            .href("https://fonts.googleapis.com"),
        Z.link()
            .rel("preconnect")
            .href("https://fonts.gstatic.com")
            .crossorigin(""),
        Z.stylesheet("https://fonts.googleapis.com/css2?family=Sawarabi+Gothic&display=swap"),
    );
    let tailwind_preamble = Z.stylesheet("/static/output.css");

    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4 sidebar-section")
            .c(Z.h2().class("text-2xl font-bold pb-3").c(title))
    };

    let sidebar = Z
        .div()
        .id("sidebar-container")
        .class("w-4/12 grow-0 p-6 bg-gray-300 overflow-auto shadow-left-side")
        .c(Z.div()
            .id("sidebar")
            .class("flex flex-col gap-2")
            .c(Z.div()
                .id("lookup-header")
                // TODO make this consistent with the others
                .class("px-6 py-3")
                .c(Z.h1().class("italic").c("Click on a word to look it up.")))
            .c(section("Memory").c(Z.div().id("lookup-memory").c(Z
                .span()
                .class("italic")
                .c("Information about the state of the word in the ")
                .c("spaced repetition system is displayed here, along with controls for SRS ")
                .c("review functionality."))))
            .c(
                section("Definitions").c(Z.div().id("lookup-definitions").c(Z
                    .span()
                    .class("italic")
                    .c("Dictionary definitions matching the word are listed here, grouped by ")
                    .c("part of speech."))),
            )
            .c(section("Links").c(Z.div().id("lookup-links").c(Z
                .span()
                .class("italic")
                .c("Other words that use the same characters or roots—")
                .c("in particular, for CJK languages, words that use the same Chinese character, ")
                .c("especially with the same reading—are listed here."))))
            .c(section("Examples").c(Z.div().id("lookup-examples").c(Z
                .span()
                .class("italic")
                .c("Any sentences from books you've read (excluding parts not yet read) ")
                .c("that use the word being looked up are shown here to display uses of ")
                .c("the word in context.")))));

    let text_section = build_books_view_text_section(&pool, id, page).await?;

    let main = Z
        .div()
        .id("main")
        .class("w-7/12 grow-0 py-10 pl-32 pr-10 bg-gray-200 overflow-scroll text-2xl/10")
        .lang("ja")
        .c(
            dynamic_section, // clears this when dynamic is updated
        )
        .c(Z.div().id("main-text").c(text_section));

    let head = Z
        .head()
        .c(htmx_preamble)
        .c(fonts_preamble)
        .c(tailwind_preamble)
        .c(icons_preamble);
    let body = Z
        .body()
        .class("h-screen w-screen bg-gray-100 relative flex flex-row overflow-hidden")
        .c(Z.div().class("grow bg-gray-200").id("left-spacer"))
        .c(main)
        .c(sidebar)
        .c(Z.div().class("grow bg-gray-300").id("right-spacer"))
        .hx_on(
            "keyup",
            r#"
(function () {
event.key === "a" && htmx.trigger(".line:hover .bulk-okay", "click");
event.key === "f" && htmx.trigger(".line:hover .favourite-btn", "click");
})()
"#,
        );
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
