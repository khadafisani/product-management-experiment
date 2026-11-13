use super::entity::*;
use crate::{ config::App, helper::Result };
use axum::{ Json, extract::{ Path, State } };
use serde_json::Value;
pub(crate) async fn documents(State(a): State<App>) -> Result<Json<Vec<Document>>> {
    Ok(Json(super::service::documents(a).await?))
}
pub(crate) async fn detail(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    Ok(Json(super::service::detail(a, id).await?))
}
pub(crate) async fn preview_document(
    State(a): State<App>,
    Json(d): Json<DocumentInput>
) -> Result<Json<Value>> {
    Ok(Json(super::service::preview_document(a, d).await?))
}
pub(crate) async fn create_document(
    State(a): State<App>,
    Json(d): Json<DocumentInput>
) -> Result<Json<Value>> {
    let v = match d.kind.as_str() {
        "purchase_order" => crate::feature::purchase_order::service::create(a, d).await?,
        "purchase_invoice" => crate::feature::purchase_invoice::service::create(a, d).await?,
        "sales_invoice" => crate::feature::sale_invoice::service::create(a, d).await?,
        _ => {
            return Err(crate::helper::bad("Unknown document type"));
        }
    };
    Ok(Json(v))
}
