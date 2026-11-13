use super::App;
use crate::feature::{
    document::controller::{create_document, detail, documents, preview_document},
    forecast::controller::forecast,
    payment::controller::{pay, payments},
    product::controller::{products, save_product},
    search::controller::{reindex, search},
};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::env;
use tower_http::cors::CorsLayer;
pub(crate) async fn health(State(a): State<App>) -> Json<Value> {
    let db = sqlx::query("SELECT 1").execute(&a.db).await.is_ok();
    let meili = a
        .http
        .get(format!("{}/health", a.meili))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    Json(json!({"database":db,"search":meili}))
}
pub(crate) fn router(app: App, origin: axum::http::HeaderValue) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/sales-invoices/{id}/cancel",
            post(crate::feature::sale_invoice::controller::cancel),
        )
        .route(
            "/api/purchase-orders",
            get(crate::feature::purchase_order::controller::list)
                .post(crate::feature::purchase_order::controller::create),
        )
        .route(
            "/api/purchase-invoices",
            get(crate::feature::purchase_invoice::controller::list)
                .post(crate::feature::purchase_invoice::controller::create),
        )
        .route(
            "/api/sales-invoices",
            get(crate::feature::sale_invoice::controller::list)
                .post(crate::feature::sale_invoice::controller::create),
        )
        .route("/api/products", get(products).post(save_product))
        .route("/api/documents", get(documents).post(create_document))
        .route("/api/documents/preview", post(preview_document))
        .route("/api/documents/{id}", get(detail))
        .route("/api/payments", get(payments).post(pay))
        .route("/api/forecast", get(forecast))
        .route("/api/search", get(search))
        .route("/api/search/reindex", post(reindex))
        .fallback_service(tower_http::services::ServeDir::new(
            env::var("STATIC_DIR").unwrap_or("../frontend/build".into()),
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers([axum::http::header::CONTENT_TYPE]),
        )
        .with_state(app)
}
