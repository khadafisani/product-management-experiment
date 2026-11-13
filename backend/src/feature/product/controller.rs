use super::entity::{Product, ProductInput};
use crate::{config::App, helper::Result};
use axum::{Json, extract::State};
pub(crate) async fn products(State(a): State<App>) -> Result<Json<Vec<Product>>> {
    Ok(Json(super::service::products(a).await?))
}
pub(crate) async fn save_product(
    State(a): State<App>,
    Json(p): Json<ProductInput>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(super::service::save_product(a, p).await?))
}
