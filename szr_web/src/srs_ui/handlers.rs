use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use sqlx::PgPool;
use szr_html::{Doc, Z};
use uuid::Uuid;

use crate::{layout::head, models::VariantId};

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
pub async fn pick_srs_item(pool: &PgPool) -> Result<VariantId> {
    let r = sqlx::query!(
        r#"
select
variants.id "variant_id!: VariantId",
mnemes.id mneme_id
from mnemes
join variants on variants.mneme_id = mnemes.id
order by next_due asc
limit 1"#
    )
    .fetch_one(pool)
    .await?;

    Ok(r.variant_id)
}

#[axum::debug_handler]
pub async fn review_page(State(pool): State<PgPool>) -> Result<Redirect> {
    let variant_id = pick_srs_item(&pool).await?;
    Ok(Redirect::to(&format!("/srs/review/{}", variant_id.0)))
}

#[axum::debug_handler]
pub async fn review_item_page(
    State(pool): State<PgPool>,
    Path(variant_id): Path<Uuid>,
) -> Result<Doc> {
    let variant_id = VariantId(variant_id);
    let html = Z
        .html()
        .c(head())
        .c(Z.body().c(Z.div().c(variant_id.0.to_string())));
    Ok(html)
}
