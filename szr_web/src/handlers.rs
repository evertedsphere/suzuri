use std::collections::{BTreeSet, HashMap};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use chrono::Utc;
use itertools::Itertools;
use serde::Deserialize;
use snafu::{ResultExt, Snafu};
use sqlx::PgPool;
use szr_dict::DefContent;
use szr_html::{Doc, DocRender, RenderExt, Z};
use szr_srs::{MemoryStatus, Mneme, Params, ReviewGrade};
use szr_textual::{Line, Token};
use tracing::warn;
use uuid::Uuid;

use crate::{
    layout::{head, is_punctuation, labelled_value, labelled_value_c},
    models::{
        self, get_mneme_refresh_batch, get_related_words, get_sentences, ContextBlock,
        ContextSentenceToken, DefGroup, LookupData, MnemeRefreshBatch, MnemeRefreshDatum,
        RelativeRubySpan, RubyMatchType, RubySpan, SentenceGroup, SpanLink, TagDefGroup, VariantId,
        VariantRuby,
    },
};

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Ctx)))]
pub enum Error {
    FetchDoc { source: szr_textual::Error },
    GetNewVariants { source: sqlx::Error },
    GetDueVariants { source: sqlx::Error },
    MnemeError { source: szr_srs::mneme::Error },
    AssignMnemeToVariant { source: sqlx::Error },
    ToggleFavourite { source: sqlx::Error },
    GetDocs { source: sqlx::Error },
    GetLookupData { source: models::Error },
    GetRelatedWords { source: models::Error },
    GetContextSentences { source: models::Error },
    GetMnemeRefreshBatch { source: models::Error },
    GetFrequentNames { source: sqlx::Error },
    GetPhraseHits { source: sqlx::Error },
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

pub async fn handle_index(State(pool): State<PgPool>) -> Result<impl IntoResponse> {
    struct DocMeta {
        id: i32,
        title: String,
    }

    let docs = sqlx::query_as!(
        DocMeta,
        r#"
select id, title from docs
order by id desc
"#
    )
    .fetch_all(&pool)
    .await
    .context(GetDocsCtx)?;

    let r = Z
        .html()
        .c(head())
        .c(Z.body().class("text-gray-600 px-20 py-20").c(Z.ul().cs(
            docs,
            |DocMeta { id, title }| {
                Z.li()
                    .c(Z.a().href(format!("/books/{id}/view/page/1")).c(title))
            },
        )));
    Ok(r)
}

pub async fn handle_create_mneme(
    State(pool): State<PgPool>,
    Path((variant_id, grade)): Path<(Uuid, ReviewGrade)>,
) -> Result<impl IntoResponse> {
    let (variant_id, mneme) = render_create_mneme(&pool, variant_id, grade).await?;
    let r = build_memory_section(MemorySectionData::KnownItem { variant_id, mneme }, false);
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

    let new_mneme_id = Mneme::create(&pool, &params, grade)
        .await
        .context(MnemeCtx)?;
    // TODO transaction
    sqlx::query!(
        r#"UPDATE variants SET mneme_id = $2 WHERE id = $1"#,
        variant_id,
        new_mneme_id
    )
    .execute(pool)
    .await
    .context(AssignMnemeToVariantCtx)?;

    let mneme = Mneme::get_by_id(&pool, new_mneme_id)
        .await
        .context(MnemeCtx)?;

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
    .context(GetNewVariantsCtx)?;

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
    .context(GetDueVariantsCtx)?;

    let mut css: Vec<String> = Default::default();
    let now = Utc::now();

    for variant_id in new_variant_ids {
        let (_, mneme) = render_create_mneme(&pool, variant_id, grade).await?;
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
        let (_, mneme) = render_review_mneme(&pool, variant_id, mneme_id, grade).await?;
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

#[derive(Deserialize)]
pub struct ReviewParams {
    redirect: Option<bool>,
}

pub async fn handle_review_mneme(
    State(pool): State<PgPool>,
    Path((variant_id, mneme_id, grade)): Path<(Uuid, Uuid, ReviewGrade)>,
    info: Query<ReviewParams>,
) -> Result<impl IntoResponse> {
    let (variant_id, mneme) = render_review_mneme(&pool, variant_id, mneme_id, grade).await?;
    if let Some(true) = info.redirect {
        return Ok(Redirect::to(&format!(
            "/srs/review",
            /* variant_id.0, mneme_id */
        ))
        .into_response());
    }
    Ok(
        build_memory_section(MemorySectionData::KnownItem { variant_id, mneme }, false)
            .render_to_html()
            .into_response(),
    )
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
        .context(MnemeCtx)?;
    let mneme = Mneme::get_by_id(pool, mneme_id).await.context(MnemeCtx)?;

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
    .context(ToggleFavouriteCtx)?;

    Ok(build_star_button(doc_id, line_index, new_status))
}

pub enum MemorySectionData {
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
    } else if days <= 8 {
        return "a week".to_string();
    } else if days <= 25 {
        return format!("{} days", days);
    } else if days <= 32 {
        return "a month".to_string();
    }
    let months = days / 30;
    if months == 1 {
        return "a month".to_string();
    } else if months <= 10 {
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

pub fn review_actions_block(data: &MemorySectionData, redirect: bool) -> Doc {
    let create_link = |grade| match data {
        MemorySectionData::NewVariant { variant_id } => {
            format!(
                "/variants/{}/create-mneme/{}?redirect={}",
                variant_id.0, grade, redirect
            )
        }
        MemorySectionData::KnownItem { variant_id, mneme } => {
            format!(
                "/variants/{}/review/{}/{}?redirect={}",
                variant_id.0, mneme.id, grade, redirect
            )
        }
    };

    let review_button = |grade, extra_classes, text, id| {
        let base_classes = "";
        let mut r = Z
            .a()
            .role("button")
            .class(format!("{base_classes} {extra_classes}"))
            .id(format!("sidebar-{id}-button"))
            .c(text);
        if redirect {
            r = r.href(create_link(grade))
        } else {
            r = r.hx_post(create_link(grade)).hx_trigger(format!("click"))
        }
        r
    };

    Z.div().class("flex flex-col gap-2").c(labelled_value_c(
        "Review as",
        Z.div()
            .class("flex flex-row gap-2")
            .c(review_button("Fail", "text-red-800", "Fail", "fail"))
            .c(review_button("Hard", "text-yellow-900", "Hard", "hard"))
            .c(review_button("Okay", "text-green-800", "Okay", "okay"))
            .c(review_button("Easy", "text-blue-800", "Easy", "easy")),
        "font-bold",
    ))
}

fn build_memory_section(data: MemorySectionData, redirect: bool) -> (Doc, Doc) {
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
            srs_status_block = srs_status_block.c(labelled_value_c(
                "Status",
                format!("{:?} (due {})", mneme.state.status, diff_str),
                "status",
            ));
            decoration_colour_rule = Some(get_decoration_colour_rule(
                *variant_id,
                diff_secs < 0,
                mneme.state.status,
            ));
        }
    };

    let mut memory_block = Z
        .div()
        .class("flex flex-col gap-2")
        .id("memory")
        .hx_swap_oob_enable();

    memory_block = memory_block
        .c(srs_status_block)
        .c(review_actions_block(&data, redirect))
        .c(Z.style().c(format!(
            ".variant-{} {{ background-color: rgb(209 213 219); }}",
            variant_id.0
        )));

    if redirect {
        memory_block = memory_block.c(labelled_value("Reveal", "").onclick("toggleVis()"));
    }

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

#[derive(Deserialize)]
pub struct VariantViewParams {
    redirect: Option<bool>,
}

pub async fn handle_variant_lookup_view(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    info: Option<Query<ReviewParams>>,
) -> Result<Html<String>> {
    let mut redirect = false;
    if let Some(info) = info {
        redirect = info.redirect.unwrap_or(false);
    }
    Ok(render_variant_lookup(pool, VariantId(id), redirect)
        .await?
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
            .class("flex flex-row flex-wrap self-center w-5/6 overflow-hidden gap-2");
        for example_raw in examples {
            any_links = true;
            let mut word_ruby = Z.span().class("px-2 text-lg");
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
                .hx_trigger("click")
                .hx_swap("none")
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
    let related_words = get_related_words(&pool, 5, 5, VariantId(id))
        .await
        .context(GetRelatedWordsCtx)?;
    render_lookup_related_section(related_words)
}

pub async fn handle_lookup_examples_section(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Doc> {
    let sentences = get_sentences(&pool, VariantId(id), 5, 20)
        .await
        .context(GetContextSentencesCtx)?;

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

pub async fn render_variant_lookup(
    pool: PgPool,
    id: VariantId,
    redirect: bool,
) -> Result<Vec<Doc>> {
    let LookupData {
        meanings,
        variant_id,
        ruby,
        mneme,
        sibling_variants_ruby,
    } = LookupData::get_by_id(&pool, id)
        .await
        .context(GetLookupDataCtx)?;

    let mut selected_variant_ruby = Z.h1().lang("ja");

    if redirect {
        selected_variant_ruby = selected_variant_ruby.class("text-6xl");
    } else {
        selected_variant_ruby = selected_variant_ruby.class("text-4xl");
    }

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
                    .hx_get(format!(
                        "/variants/view/{}?redirect={}",
                        variant_id.0, redirect
                    ))
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
    let (memory_section, memory_dynamic_css) = build_memory_section(memory_section_data, redirect);

    if redirect {
        selected_variant_ruby = selected_variant_ruby.class("self-center");
    }

    let header_section = Z
        .div()
        .id("lookup-header")
        .hx_swap_oob_enable()
        .class("flex flex-col px-3 py-2 xl:px-6 xl:py-3 gap-3")
        .c(selected_variant_ruby);
    // .c(labelled_value(
    // Z.ruby("Variants", None, None),
    // alternates_row.unwrap_or(
    //     Z.ruby("none found", None, None)
    //         .class("text-gray-600 italic"),
    // ),
    // ).id("variants-content"))

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
        .hx_trigger("load delay:300ms")
        .hx_get(format!("/variants/view/{}/example-sentences", id.0))
        .c(Z.span().class("text-gray-600 italic").c("Loading…"));

    let links_section = Z
        .div()
        .id("lookup-links")
        .hx_swap_oob_enable()
        .hx_trigger("load delay:300ms")
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
pub fn render_srs_style_patch(batch: MnemeRefreshBatch) -> Doc {
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
        .hx_get("/get-review-patch")
        .hx_swap_oob_enable()
        .hx_trigger(format!("every {}s", interval_sec));
    r
}

pub async fn handle_refresh_srs_style_patch(State(pool): State<PgPool>) -> Result<Doc> {
    let refresh_data = get_mneme_refresh_batch(&pool)
        .await
        .context(GetMnemeRefreshBatchCtx)?;
    let dynamic_section = render_srs_style_patch(refresh_data);
    Ok(dynamic_section)
}

pub async fn handle_books_view_text_section(
    State(pool): State<PgPool>,
    Path((id, page)): Path<(i32, i32)>,
) -> Result<Html<String>> {
    let (mut page, minimap) = build_books_view_text_section(&pool, id, page).await?;
    page.push(minimap.hx_swap_oob_raw("#minimap"));
    Ok(page.render_to_html())
}

pub async fn build_books_view_text_section(
    pool: &PgPool,
    id: i32,
    page: i32,
) -> Result<(Vec<Doc>, Doc)> {
    let doc = szr_textual::get_doc(&pool, id).await.context(FetchDocCtx)?;
    let mut lines = Vec::new();

    let lines_per_page = 50;
    let num_lines = doc.lines.len();
    let num_lines_to_skip = if page > 0 {
        lines_per_page * (page - 1)
    } else {
        0
    } as usize;

    let mut minimap_elements = Vec::<Doc>::new();

    let mut chars_read = 0;

    struct FrequentName {
        variant_id: Uuid,
        spelling: String,
    }

    let uuids = sqlx::query_as!(
        FrequentName,
        r#"
    with toks as (
                  select s.variant_id,
                         s.spelling,
                         lemmas.main_pos,
                         lemmas.second_pos,
                         lemmas.third_pos,
                         lemmas.fourth_pos,
                         count(*) freq
                    from tokens as t
                    join lines as l on t.line_index = l.index and t.doc_id = l.doc_id
                    join surface_forms as s on s.id = t.surface_form_id
                    join variants as v on v.id = s.variant_id
                    join lemmas on lemmas.id = v.lemma_id
                   where t.doc_id = $1
                group by variant_id,
                         s.spelling,
                         lemmas.main_pos,
                         lemmas.second_pos,
                         lemmas.third_pos,
                         lemmas.fourth_pos
              )
  select variant_id, spelling
    from toks
   where main_pos = 'Meishi' and third_pos in ('Jinmei', '地名') and freq > 100
order by freq desc
   limit 10;
"#,
        id
    )
    .fetch_all(pool)
    .await
    .context(GetFrequentNamesCtx)?;

    let mut minimap_hits = Vec::new();

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
        let mut line = Z.div().class("line").id(format!("line-{}", line_index));
        let mut token_index = 0;
        let mut line_minimap_hits = BTreeSet::new();

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

            if let Some(id) = variant_id {
                line_minimap_hits.insert(id.to_string());
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

        minimap_hits.push((line_index, line_minimap_hits));

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
                .class("bulk-easy")
                .c(Z.i().class("bx bx-check-circle text-blue-800"))
                .hx_swap("none")
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

    #[derive(Clone)]
    struct PhraseHit {
        line_index: i32,
        phrases: Vec<String>,
    }

    // TODO group and sort by hit count
    let phrase_hits = sqlx::query_as!(PhraseHit, r#"
with line_contents as (select line_index, string_agg(content, '') content from tokens where doc_id = $1 group by line_index)
select
  line_index,
  array_agg(highlights.content order by highlights.content) "phrases!: Vec<String>"
from line_contents
join highlights on line_contents.content ~ highlights.content
group by line_index;
"#, id).fetch_all(pool).await.context(GetPhraseHitsCtx)?;

    let phrases: Vec<String> = phrase_hits
        .iter()
        .flat_map(|PhraseHit { phrases, .. }| phrases.iter())
        .cloned()
        .sorted()
        .unique()
        .collect();

    minimap_hits.extend(phrase_hits.iter().cloned().map(
        |PhraseHit {
             line_index,
             phrases,
         }| (line_index, phrases.into_iter().collect()),
    ));

    minimap_hits.sort_by_key(|x| x.0);
    let minimap_hits: Vec<(i32, Vec<(i32, BTreeSet<String>)>)> = minimap_hits
        .into_iter()
        .group_by(|x| x.0)
        .into_iter()
        .map(|(k, g)| (k, g.collect()))
        .collect();

    let grouped_minimap_hits: Vec<(i32, HashMap<String, Vec<i32>>)> = minimap_hits
        .chunks(20)
        .map(|v| {
            // FIXME need per-word precision
            let default_line_index = v
                .first()
                .and_then(|w| w.1.first().map(|(x, _)| *x))
                .unwrap_or(0);
            let hit_set: Vec<(String, i32)> = v
                .into_iter()
                .flat_map(|hits_group| {
                    hits_group
                        .1
                        .clone()
                        .into_iter()
                        .flat_map(|(line_index, hits)| {
                            hits.into_iter().map(move |x| (x.to_owned(), line_index))
                        })
                })
                .collect();
            let hits = hit_set.into_iter().into_group_map();

            (default_line_index, hits)
        })
        .collect();

    let line_hit_bar_tpl = Z.div().class("flex flex-row overflow-hidden gap-0.5 grow");
    let line_span_tpl = Z.a().class("w-1");

    let mut minimap_header = line_hit_bar_tpl.clone();

    for FrequentName { spelling, .. } in uuids.iter() {
        // requires global analysis, but this is a chunked page
        minimap_header = minimap_header.c(line_span_tpl
            .clone()
            .class("bg-green-800 h-4")
            .title(spelling.clone()));
    }

    for phrase in phrases.iter() {
        minimap_header = minimap_header.c(line_span_tpl
            .clone()
            .class("bg-red-800 h-4")
            .title(phrase.clone()));
    }

    minimap_elements.push(minimap_header);

    for (_start_line_index, group_minimap_hits) in grouped_minimap_hits {
        let mut line_hit_bar = line_hit_bar_tpl.clone();
        for FrequentName {
            variant_id,
            spelling,
        } in uuids.iter()
        {
            // requires global analysis, but this is a chunked page
            if let Some(indices) = group_minimap_hits.get(&variant_id.to_string()) {
                line_hit_bar = line_hit_bar.c(line_span_tpl
                    .clone()
                    .class("bg-green-500 border-green-500/40 border-l w-1")
                    .href(format!("#line-{}", indices[0]))
                    .title(format!("{spelling} on lines {:?} (jump to first)", indices)));
            } else {
                line_hit_bar =
                    line_hit_bar.c(line_span_tpl.clone().class("border-green-500/40 border-l"));
            }
        }
        for phrase in phrases.iter() {
            if let Some(indices) = group_minimap_hits.get(phrase) {
                line_hit_bar = line_hit_bar.c(line_span_tpl
                    .clone()
                    .class("bg-red-500 border-l border-red-500/30 w-1")
                    .href(format!("#line-{}", indices[0]))
                    .title(format!("{phrase} on lines {:?} (jump to first)", indices)));
            } else {
                line_hit_bar =
                    line_hit_bar.c(line_span_tpl.clone().class("border-red-500/30 border-l"));
            }
        }
        minimap_elements.push(line_hit_bar);
    }

    // -
    //
    //
    //
    //
    //
    //
    //
    //
    //
    // view?start=x&end=y&focus=z (x<z<y)
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //
    //

    let mut current_page_minimap = Z
        .div()
        .id(format!("minimap"))
        .class("w-1/12 bg-gray-200 flex flex-col p-1");
    if num_lines_to_skip < num_lines {
        current_page_minimap = current_page_minimap.cv(minimap_elements);
    }

    lines.push(
        Z.a()
            .c("next page")
            .class("text-gray-600")
            .role("button")
            .hx_push_url(format!("/books/{id}/view/page/{}", page + 1))
            .hx_get(format!("/books/{id}/view/page/{}/text-only", page + 1))
            .hx_target(format!("#main-text"))
            .hx_swap("scroll:#main:top"),
    );

    lines.push(Z.a().c("fullscreen").onclick("enableFullscreen()"));

    let current_page = Z.div().id(format!("page-{page}")).cv(lines);

    let response_pages = vec![current_page];

    Ok((response_pages, current_page_minimap))
}

pub async fn handle_books_view(
    State(pool): State<PgPool>,
    Path((id, page)): Path<(i32, i32)>,
) -> Result<Doc> {
    let refresh_data = get_mneme_refresh_batch(&pool)
        .await
        .context(GetMnemeRefreshBatchCtx)?;
    let dynamic_section = render_srs_style_patch(refresh_data);

    let handler_scripts = Z.script().src("/static/handlers.js");

    let anon_section = || {
        Z.div()
            .class("flex flex-col px-3 py-2 xl:px-6 xl:py-4 sidebar-section")
    };

    let section = |title| anon_section().c(Z.h2().class("text-2xl font-bold pb-3").c(title));

    let sidebar = Z
        .div()
        .id("sidebar-container")
        .class("w-full xl:w-4/12 min-h-1/2 h-1/2 xl:h-full grow-0 p-0 xl:p-6 bg-gray-300 overflow-auto shadow-left-side")
        .c(Z.div()
            .id("sidebar")
            .class("flex flex-col gap-2")
            .c(Z.div()
                .id("lookup-header")
                // TODO make this consistent with the others
                .class("px-3 py-2 xl:px-6 xl:py-4")
                .c(Z.h1().class("italic").c("Click on a word to look it up.")))
            .c(anon_section().c(Z.div().id("lookup-memory").c(Z
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

    let (text_section, minimap) = build_books_view_text_section(&pool, id, page).await?;

    let main = Z
        .div()
        .id("main")
        .class("w-full xl:w-8/12 h-1/2 xl:h-full grow-0 py-2 xl:py-10 px-2 xl:pl-32 xl:pr-28 bg-gray-200 overflow-scroll text-2xl/10")
        .lang("ja")
        .c(
            dynamic_section, // clears this when dynamic is updated
        )
        .c(Z.div().id("main-text").c(text_section));

    let head = head().c(handler_scripts);
    let body = Z
        .body()
        .class("h-screen w-screen bg-gray-100 relative flex flex-col xl:flex-row overflow-hidden")
        .c(Z.div().class("grow bg-gray-200").id("left-spacer"))
        // .c(minimap)
        .c(main)
        .c(sidebar)
        .c(Z.div().class("grow bg-gray-300").id("right-spacer"))
        .hx_on("keydown", "onBodyKeypress()");
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
