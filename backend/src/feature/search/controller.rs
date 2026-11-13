use super::entity::Search;
use crate::{config::App, helper::Result};
use axum::{
    Json,
    extract::{Query, State},
};
use serde_json::Value;
pub(crate) async fn search(State(a): State<App>, Query(q): Query<Search>) -> Result<Json<Value>> {
    Ok(Json(super::service::search(a, q).await?))
}
pub(crate) async fn reindex(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(super::service::reindex(a).await?))
}
