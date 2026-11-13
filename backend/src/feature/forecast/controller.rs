use super::entity::ForecastQuery;
use crate::{config::App, helper::Result};
use axum::{
    Json,
    extract::{Query, State},
};
use serde_json::Value;
pub(crate) async fn forecast(
    State(a): State<App>,
    Query(q): Query<ForecastQuery>,
) -> Result<Json<Value>> {
    Ok(Json(super::service::forecast(a, q).await?))
}
