use super::entity::PaymentInput;
use crate::{config::App, helper::Result};
use axum::{Json, extract::State};
use serde_json::Value;
pub(crate) async fn pay(State(a): State<App>, Json(p): Json<PaymentInput>) -> Result<Json<Value>> {
    Ok(Json(super::service::pay(a, p).await?))
}
pub(crate) async fn payments(State(a): State<App>) -> Result<Json<Value>> {
    Ok(Json(super::service::payments(a).await?))
}
