use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use sqlx::PgPool;
use szr_html::{Doc, Z};
use szr_srs::Mneme;
use uuid::Uuid;

use crate::{
    handlers::{review_actions_block, MemorySectionData},
    layout::head,
    models::VariantId,
};

// Make our own error that wraps `anyhow::Error`.
pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

type Result<T, E = AppError> = std::result::Result<T, E>;

// TODO queue?
pub async fn pick_srs_item(pool: &PgPool, offset: u8) -> Result<(Uuid, VariantId)> {
    let r = sqlx::query!(
        r#"
select
variants.id "variant_id!: VariantId",
mnemes.id "mneme_id!: Uuid"
from mnemes
join variants on variants.mneme_id = mnemes.id
order by next_due asc
offset $1
limit 1"#,
        offset as i64
    )
    .fetch_one(pool)
    .await?;

    Ok((r.mneme_id, r.variant_id))
}

#[axum::debug_handler]
pub async fn review_page(State(pool): State<PgPool>) -> Result<Redirect> {
    let (mneme_id, variant_id) = pick_srs_item(&pool, 0).await?;
    Ok(Redirect::to(&format!(
        "/srs/review/{}/{}",
        variant_id.0, mneme_id
    )))
}

#[axum::debug_handler]
pub async fn review_item_page(
    State(pool): State<PgPool>,
    Path((variant_id, mneme_id)): Path<(Uuid, Uuid)>,
) -> Result<Doc> {
    let variant_id = VariantId(variant_id);
    // let mneme = Mneme::get_by_id(&pool, mneme_id).await.unwrap();
    // let msd = MemorySectionData::KnownItem { variant_id, mneme };
    // let body = review_actions_block(&msd);

    let link = Z
        .a()
        .hx_swap_oob_enable()
        .hx_trigger("load")
        .hx_get(format!("/variants/view/{}?redirect=true", variant_id.0))
        .hx_swap("none");

    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4 sidebar-section")
            .c(Z.h2().class("text-2xl font-bold pb-3").c(title))
    };

    let html = Z.html().c(head()).c(Z
        .body()
        .c(Z.div().id("load-link").c(link))
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

    Ok(html)
}
