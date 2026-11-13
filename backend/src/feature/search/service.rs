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
            active_hits(
                &a,
                r.json().await.map_err(|_| bad("Invalid search response"))?,
            )
            .await
        }
        _ => Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Search unavailable. Use the product list or retry indexing.".into(),
        )),
    }
}
pub(crate) async fn reindex(a: App) -> Result<Value> {
    let ps: Vec<Product> = sqlx::query_as(
        "SELECT id,name,quantity,remaining_quantity,status,photo,description,price,reorder_level,CAST(created_at AS CHAR) created_at,CAST(updated_at AS CHAR) updated_at,CAST(deleted_at AS CHAR) deleted_at FROM products WHERE deleted_at IS NULL",
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

// Indexing is asynchronous. Recheck hit IDs against the source of truth so a
// deleted product cannot leak through a stale index, even before the next sync.
pub(crate) async fn active_hits(a: &App, mut data: Value) -> Result<Value> {
    if let Some(hits) = data.get_mut("hits").and_then(Value::as_array_mut) {
        let ids: Vec<i64> = hits.iter().filter_map(|r| r["id"].as_i64()).collect();
        if ids.is_empty() {
            hits.clear();
            return Ok(data);
        }
        let mut query = sqlx::QueryBuilder::<sqlx::MySql>::new(
            "SELECT id FROM products WHERE deleted_at IS NULL AND id IN (",
        );
        let mut list = query.separated(",");
        for id in ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
        let live: std::collections::HashSet<i64> = query
            .build_query_scalar::<i64>()
            .fetch_all(&a.db)
            .await?
            .into_iter()
            .collect();
        hits.retain(|r| r["id"].as_i64().is_some_and(|id| live.contains(&id)));
    }
    Ok(data)
}
