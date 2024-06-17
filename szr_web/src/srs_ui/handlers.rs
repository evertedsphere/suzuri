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
    handlers::{render_srs_style_patch, review_actions_block, MemorySectionData},
    layout::head,
    models::{get_mneme_refresh_batch, VariantId},
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

    let refresh_data = get_mneme_refresh_batch(&pool).await.unwrap();
    let dynamic_section = render_srs_style_patch(refresh_data);

    let link = Z
        .a()
        .hx_swap_oob_enable()
        .hx_trigger("load")
        .hx_get(format!("/variants/view/{}?redirect=true", variant_id.0))
        .hx_on("htmx:after-request", "toggleVis()")
        .hx_swap("none");

    let section = |title| {
        Z.div()
            .class("flex flex-col px-6 py-4 sidebar-section")
            .c(Z.h2().class("text-2xl font-bold pb-3").c(title))
    };

    let handler_scripts = Z.script().src("/static/handlers.js");

    let main = Z
        .div()
        .class("flex flex-col w-full xl:w-8/12 text-lg grow-0 py-2 xl:py-10 px-2 xl:px-32 bg-gray-200 overflow-scroll")
        .c(Z.div().id("load-link").c(link))
        .c(Z.div()
            .id("lookup-header")
            // TODO make this consistent with the others
            .class("px-6 py-3"))
        .c(section("Memory").c(Z.div().id("lookup-memory").c(Z.span().class("italic"))))
        .c(section("Definitions")
            .id("section-definitions")
            .c(Z.div().id("lookup-definitions").c(Z.span().class("italic"))))
        .c(section("Links")
            .id("section-links")
            .c(Z.div().id("lookup-links").c(Z.span().class("italic"))))
        .c(section("Examples")
            .id("section-examples")
            .c(Z.div().id("lookup-examples").c(Z.span().class("italic"))))
        .c(dynamic_section);

    let head = head().c(handler_scripts);
    let body = Z
        .body()
        .hx_on("keydown", "onBodyKeypress()")
        .class("h-screen w-screen bg-gray-100 relative flex flex-row overflow-hidden")
        .c(Z.div().class("grow bg-gray-300").id("left-spacer"))
        .c(main)
        .c(Z.div().class("grow bg-gray-300").id("right-spacer"));
    let html = Z
        .fragment()
        .c(Z.doctype("html"))
        .c(Z.meta().charset("UTF-8"))
        .c(Z.meta()
            .name("viewport")
            .content("width=device-width, initial-scale=1.0"))
        .c(Z.html().lang("en").c(head).c(body));

    Ok(html)
}
