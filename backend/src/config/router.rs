use super::App;
use crate::feature::{
    document::controller::{create_document, documents, preview_document},
    forecast::controller::forecast,
    payment::controller::pay,
    product::controller::{products, save_product},
    search::controller::{reindex, search},
};
use axum::{
    Json, Router,
    extract::State,
    routing::{delete, get, post},
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
        .route("/api/void-requests/{id}", get(crate::feature::related::invoice_approval))
        .route("/api/delivery-cancellation-requests/{id}", get(crate::feature::related::delivery_approval))
        .route("/api/users/{id}", delete(crate::feature::lifecycle::user))
        .route(
            "/api/lists/{resource}",
            get(crate::feature::pagination::list),
        )
        .route(
            "/api/delivery-orders/{id}/cancel-request",
            post(crate::feature::delivery::request_cancel),
        )
        .route(
            "/api/delivery-cancellation-requests",
            get(crate::feature::delivery::cancellation_requests),
        )
        .route(
            "/api/delivery-cancellation-requests/{id}/approve",
            post(crate::feature::delivery::approve_cancel),
        )
        .route(
            "/api/delivery-cancellation-requests/{id}/reject",
            post(crate::feature::delivery::reject_cancel),
        )
        .route("/api/auth/login", post(crate::feature::auth::login))
        .route("/api/auth/refresh", post(crate::feature::auth::refresh))
        .route("/api/auth/logout", post(crate::feature::auth::logout))
        .route(
            "/api/auth/logout-all",
            post(crate::feature::auth::logout_all),
        )
        .route("/api/auth/me", get(crate::feature::auth::me))
        .route(
            "/api/auth/password",
            post(crate::feature::auth::change_password),
        )
        .route(
            "/api/roles",
            get(crate::feature::auth::roles).post(crate::feature::auth::save_role),
        )
        .route(
            "/api/users",
            get(crate::feature::auth::users).post(crate::feature::auth::create_user),
        )
        .route(
            "/api/roles/{id}",
            post(crate::feature::auth::update_role).delete(crate::feature::lifecycle::role),
        )
        .route(
            "/api/users/{id}/role",
            post(crate::feature::auth::user_role),
        )
        .route(
            "/api/invoices/{id}",
            post(crate::feature::invoice_admin::update),
        )
        .route(
            "/api/invoices/{id}/cancel",
            post(crate::feature::invoice_admin::request_void),
        )
        .route(
            "/api/void-requests",
            get(crate::feature::invoice_admin::requests),
        )
        .route(
            "/api/void-requests/{id}/approve",
            post(crate::feature::invoice_admin::approve),
        )
        .route(
            "/api/void-requests/{id}/reject",
            post(crate::feature::invoice_admin::reject),
        )
        .route(
            "/api/delivery-orders",
            get(crate::feature::delivery::list).post(crate::feature::delivery::create),
        )
        .route(
            "/api/delivery-orders/{id}",
            get(crate::feature::related::delivery)
                .post(crate::feature::delivery::update)
                .delete(crate::feature::lifecycle::delivery),
        )
        .route(
            "/api/delivery-orders/{id}/approve",
            post(crate::feature::delivery::approve),
        )
        .route(
            "/api/delivery-orders/{id}/status",
            post(crate::feature::delivery::status),
        )
        .route("/api/health", get(health))
        .route(
            "/api/sales-invoices/{id}/cancel",
            post(crate::feature::invoice_admin::request_void),
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
        .route(
            "/api/documents/{id}",
            get(crate::feature::related::document).delete(crate::feature::lifecycle::document),
        )
        .route(
            "/api/payments",
            get(crate::feature::related::payments).post(pay),
        )
        .route(
            "/api/payments/{id}",
            get(crate::feature::related::payment).delete(crate::feature::lifecycle::payment),
        )
        .route(
            "/api/products/{id}",
            get(crate::feature::related::product).delete(crate::feature::lifecycle::product),
        )
        .route("/api/forecast", get(forecast))
        .route("/api/search", get(search))
        .route("/api/search/reindex", post(reindex))
        .fallback_service(
            tower_http::services::ServeDir::new(
                env::var("STATIC_DIR").unwrap_or("../frontend/build".into()),
            )
            .fallback(tower_http::services::ServeFile::new(
                std::path::Path::new(&env::var("STATIC_DIR").unwrap_or("../frontend/build".into()))
                    .join("200.html"),
            )),
        )
        .layer(axum::middleware::from_fn_with_state(
            app.clone(),
            crate::feature::auth::guard,
        ))
        .layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::DELETE,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderName::from_static("idempotency-key"),
                ])
                .allow_credentials(true),
        )
        .with_state(app)
}
