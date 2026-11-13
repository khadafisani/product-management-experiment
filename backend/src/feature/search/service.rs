use super::entity::Search;
use crate::{
    config::App,
    helper::{Result, bad},
};
use crate::{feature::product::entity::Product, helper::Error};
use axum::http::StatusCode;
use serde_json::{Value, json};

fn search_headers(key: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if !key.is_empty() {
        if let Ok(value) = format!("Bearer {key}").parse() {
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
    }
    headers
}
pub(crate) async fn search(a: App, q: Search) -> Result<Value> {
    super::validation::validate(&q.q)?;
    let result = a
        .http
        .post(format!("{}/indexes/pms_products/search", a.meili))
        .headers(search_headers(&a.key))
        .json(&json!({"q":q.q,"limit":50}))
        .send()
        .await;
    match result {
        Ok(r) if r.status().is_success() => {
            Ok(r.json().await.map_err(|_| bad("Invalid search response"))?)
        }
        _ => Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Search unavailable. Use the product list or retry indexing.".into(),
        )),
    }
}
pub(crate) async fn reindex(a: App) -> Result<Value> {
    let ps: Vec<Product> = sqlx::query_as(
        "SELECT id,name,quantity,remaining_quantity,status,photo,description,price,reorder_level FROM products",
    )
    .fetch_all(&a.db)
    .await?;
    let r = a
        .http
        .post(format!(
            "{}/indexes/pms_products/documents?primaryKey=id",
            a.meili
        ))
        .headers(search_headers(&a.key))
        .json(&ps)
        .send()
        .await
        .map_err(|_| bad("Cannot connect to Meilisearch"))?;
    if !r.status().is_success() {
        return Err(bad("Meilisearch rejected indexing; check API key"));
    }
    Ok(r.json()
        .await
        .map_err(|_| bad("Invalid indexing response"))?)
}
