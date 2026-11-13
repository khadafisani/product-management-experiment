use super::entity::{Document, DocumentInput};
use crate::{config::App, helper::Result};
use axum::{Json, extract::State};
use serde_json::Value;
pub(crate) async fn create(
    State(a): State<App>,
    Json(d): Json<DocumentInput>,
) -> Result<Json<Value>> {
    Ok(Json(super::service::create(a, d).await?))
}
pub(crate) async fn list(State(a): State<App>) -> Result<Json<Vec<Document>>> {
    Ok(Json(super::service::list(a).await?))
}
pub(crate) async fn cancel(
    State(a): State<App>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(input): Json<super::entity::CancelInput>,
) -> Result<Json<Value>> {
    Ok(Json(super::service::cancel(a, id, input).await?))
}
