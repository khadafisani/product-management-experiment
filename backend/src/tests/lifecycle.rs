use super::auth_delivery_tests::{call, login};
use crate::config::{App, router::router};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
async fn delete(app: &Router, path: &str, cookie: &str, key: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .method("DELETE")
                .header("cookie", cookie)
                .header("content-type", "application/json")
                .header("idempotency-key", key)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn post(app: &Router, path: &str, cookie: &str, body: Value, key: &str) -> Value {
    let (s, b, _) = call(app, path, cookie, Some(body), key).await;
    assert_eq!(s, StatusCode::OK, "{path}: {b}");
    b
}
async fn metadata(db: &sqlx::MySqlPool, table: &str, id: i64) -> (String, String, Option<String>) {
    sqlx::query_as(&format!("SELECT CAST(created_at AS CHAR),CAST(updated_at AS CHAR),CAST(deleted_at AS CHAR) FROM {table} WHERE id=?")).bind(id).fetch_one(db).await.unwrap()
}
async fn fixture(label: &str) -> (sqlx::MySqlPool, sqlx::MySqlPool, String) {
    let url =
        std::env::var("TEST_MYSQL_ADMIN_URL").unwrap_or("mysql://root@127.0.0.1:3306/mysql".into());
    let admin = sqlx::MySqlPool::connect(&url).await.unwrap();
    let name = format!(
        "lifecycle_{label}_{}_{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
    let opts: sqlx::mysql::MySqlConnectOptions = url.parse().unwrap();
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(8)
        .connect_with(opts.database(&name))
        .await
        .unwrap();
    (admin, db, name)
}
async fn cleanup(admin: sqlx::MySqlPool, db: sqlx::MySqlPool, name: &str) {
    db.close().await;
    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
}
#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn migration_backfills_existing_dates_and_preserves_epoch_idempotency_age() {
    let (admin, db, name) = fixture("upgrade").await;
    let migrator = sqlx::migrate!();
    for migration in migrator.iter().filter(|m| m.version < 7) {
        sqlx::raw_sql(&migration.sql).execute(&db).await.unwrap();
    }
    sqlx::query("INSERT INTO products(name,description,created_at) VALUES('Legacy product','','2024-01-02 03:04:05')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO roles(name) VALUES('Legacy role')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users(username,password_hash,role_id) VALUES('legacy','unused',1)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO idempotency_requests(user_id,request_key,fingerprint,created_at) VALUES(1,'old',REPEAT('a',64),1700000000)").execute(&db).await.unwrap();
    let migration = migrator.iter().find(|m| m.version == 7).unwrap();
    sqlx::raw_sql(&migration.sql).execute(&db).await.unwrap();
    let (created, updated, deleted) = metadata(&db, "products", 1).await;
    assert!(created.starts_with("2024-01-02 03:04:05"));
    assert_eq!(created, updated);
    assert!(deleted.is_none());
    let age:i64=sqlx::query_scalar("SELECT CAST(UNIX_TIMESTAMP(created_at) AS SIGNED) FROM idempotency_requests WHERE request_key='old'").fetch_one(&db).await.unwrap();
    assert_eq!(age, 1700000000);
    cleanup(admin, db, &name).await;
}
#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn timestamps_soft_deletes_versioned_details_financial_reversal_and_access() {
    let (admin, db, name) = fixture("api").await;
    sqlx::migrate!().run(&db).await.unwrap();
    crate::seeds::auth::seed_auth(&db).await.unwrap();
    let a = App {
        db: db.clone(),
        http: reqwest::Client::new(),
        meili: "http://127.0.0.1:7700".into(),
        key: String::new(),
    };
    let app = router(a.clone(), "http://localhost:5173".parse().unwrap());
    let lifecycle_columns:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.columns WHERE table_schema=DATABASE() AND table_name<>'_sqlx_migrations' AND column_name IN ('created_at','updated_at','deleted_at')").fetch_one(&db).await.unwrap();
    assert_eq!(lifecycle_columns, 63);
    let admin_cookie = login(&app, "admin").await;
    let finance = login(&app, "finance").await;
    let staff = login(&app, "staff").await;
    let product = post(
        &app,
        "/api/products",
        &admin_cookie,
        json!({"name":"Empty product","quantity":0}),
        "p-empty",
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let before = metadata(&db, "products", product).await;
    assert_eq!(before.0, before.1);
    assert!(before.2.is_none());
    post(
        &app,
        "/api/products",
        &admin_cookie,
        json!({"id":product,"name":"Renamed product","quantity":0}),
        "p-update",
    )
    .await;
    let after = metadata(&db, "products", product).await;
    assert_eq!(before.0, after.0);
    assert!(after.1 > before.1);
    let (_, list, _) = call(&app, "/api/lists/products", &admin_cookie, None, "").await;
    assert_eq!(list["items"][0]["created_at"], after.0);
    assert!(list["items"][0].get("deleted_at").unwrap().is_null());
    assert_eq!(
        delete(
            &app,
            &format!("/api/products/{product}"),
            &staff,
            "staff-delete"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let path = format!("/api/products/{product}");
    let first = delete(&app, &path, &admin_cookie, "delete-product").await;
    assert_eq!(first.0, StatusCode::OK, "{:?}", first.1);
    let tombstone = metadata(&db, "products", product).await;
    assert!(tombstone.2.is_some());
    assert_eq!(first.1["deleted_at"], tombstone.2.clone().unwrap());
    assert_eq!(tombstone.0, before.0);
    assert_eq!(
        delete(&app, &path, &admin_cookie, "delete-product").await,
        first
    );
    assert_eq!(metadata(&db, "products", product).await, tombstone);
    assert_eq!(
        delete(&app, &path, &admin_cookie, "delete-product-again")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, &path, &admin_cookie, None, "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, "/api/lists/products", &admin_cookie, None, "")
            .await
            .1["total"],
        0
    );
    assert_eq!(
        call(&app, "/api/products", &admin_cookie, None, "")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        call(
            &app,
            "/api/products",
            &admin_cookie,
            Some(json!({"id":product,"name":"Resurrect","quantity":0})),
            "cannot-edit-deleted"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let hits = crate::feature::search::service::active_hits(
        &a,
        json!({"hits":[{"id":product,"name":"stale index"}]}),
    )
    .await
    .unwrap();
    assert_eq!(hits["hits"], json!([]));
    // Invoice edits retain old line IDs, but domain calculations/read models see only live lines.
    let p = post(
        &app,
        "/api/products",
        &admin_cookie,
        json!({"name":"Stock product","quantity":20,"price":100}),
        "p-stock",
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let invoice=post(&app,"/api/documents",&staff,json!({"kind":"sales_invoice","number":"LIFE-SI-1","date":"2026-11-14","person":"Customer","lines":[{"product_id":p,"quantity":4,"price":100}]}),"invoice").await["id"].as_i64().unwrap();
    post(&app,&format!("/api/invoices/{invoice}"),&staff,json!({"number":"LIFE-SI-1","date":"2026-11-14","person":"Customer","lines":[{"product_id":p,"quantity":3,"price":100}]}),"edit-lines").await;
    let counts:(i64,i64)=sqlx::query_as("SELECT COUNT(*),CAST(SUM(deleted_at IS NULL) AS SIGNED) FROM document_lines WHERE document_id=?").bind(invoice).fetch_one(&db).await.unwrap();
    assert_eq!(counts, (2, 1));
    let detail = call(
        &app,
        &format!("/api/documents/{invoice}"),
        &admin_cookie,
        None,
        "",
    )
    .await
    .1;
    assert_eq!(detail["lines"].as_array().unwrap().len(), 1);
    assert_eq!(detail["lines"][0]["quantity"], 3);
    assert!(detail["lines"][0]["created_at"].is_string());
    assert_eq!(
        delete(
            &app,
            &format!("/api/documents/{invoice}"),
            &admin_cookie,
            "cannot-hide-posted"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        delete(
            &app,
            &format!("/api/documents/{invoice}"),
            &staff,
            "staff-invoice-delete"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let request = post(
        &app,
        &format!("/api/invoices/{invoice}/cancel"),
        &staff,
        json!({"reason":"Delete after Finance reversal"}),
        "request-void",
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let request_path = format!("/api/void-requests/{request}");
    let (status, detail, _) = call(&app, &request_path, &staff, None, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["requested_by"], "staff");
    assert_eq!(detail["status"], "Pending");
    assert!(detail["created_at"].as_str().is_some());
    assert!(detail["updated_at"].as_str().is_some());
    assert_eq!(call(&app, &request_path, "", None, "").await.0, StatusCode::UNAUTHORIZED);
    post(
        &app,
        &format!("/api/void-requests/{request}/approve"),
        &finance,
        json!({}),
        "approve-void",
    )
    .await;
    assert_eq!(
        delete(
            &app,
            &format!("/api/documents/{invoice}"),
            &finance,
            "delete-voided"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(metadata(&db, "documents", invoice).await.2.is_some());
    assert_eq!(call(&app, &request_path, &finance, None, "").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        call(
            &app,
            &format!("/api/documents/{invoice}"),
            &admin_cookie,
            None,
            ""
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_lines WHERE document_id=? AND deleted_at IS NULL",
    )
    .bind(invoice)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(remaining, 0);
    let product_detail = call(&app, &format!("/api/products/{p}"), &admin_cookie, None, "")
        .await
        .1;
    assert_eq!(product_detail["sales_invoices"], json!([]));
    // Deleting a supplier payment reverses paid-only availability, not physical receiving.
    let pi=post(&app,"/api/documents",&staff,json!({"kind":"purchase_invoice","number":"LIFE-PI-1","date":"2026-11-14","person":"Supplier","lines":[{"product_id":p,"quantity":3,"price":100}]}),"pi").await["id"].as_i64().unwrap();
    let pay=post(&app,"/api/payments",&finance,json!({"reference":"LIFE-PAY-1","date":"2026-11-14","amount":300,"kind":"purchase_invoice","invoice_ids":[pi]}),"supplier-payment").await["id"].as_i64().unwrap();
    let path = format!("/api/payments/{pay}");
    let (d1, d2) = tokio::join!(
        delete(&app, &path, &finance, "payment-delete-one"),
        delete(&app, &path, &finance, "payment-delete-two")
    );
    let mut statuses = vec![d1.0, d2.0];
    statuses.sort();
    assert_eq!(statuses, vec![StatusCode::OK, StatusCode::NOT_FOUND]);
    let stock: (i64, i64) =
        sqlx::query_as("SELECT quantity,remaining_quantity FROM products WHERE id=?")
            .bind(p)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(stock, (23, 20));
    let balance: (i64, String, bool) =
        sqlx::query_as("SELECT remaining_payment,status,stock_released FROM documents WHERE id=?")
            .bind(pi)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(balance, (300, "Unpaid".into(), false));
    assert!(metadata(&db, "payments", pay).await.2.is_some());
    assert_eq!(
        call(&app, &path, &finance, None, "").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, &format!("/api/documents/{pi}"), &finance, None, "")
            .await
            .1["payments"],
        json!([])
    );
    let repay=post(&app,"/api/payments",&finance,json!({"reference":"LIFE-PAY-2","date":"2026-11-14","amount":300,"kind":"purchase_invoice","invoice_ids":[pi]}),"repay").await["id"].as_i64().unwrap();
    // Existing stock can be consumed; failure to reverse a payment must roll back everything.
    let sale=post(&app,"/api/documents",&staff,json!({"kind":"sales_invoice","number":"LIFE-SI-2","date":"2026-11-14","person":"Customer","lines":[{"product_id":p,"quantity":23,"price":100}]}),"consume-stock").await["id"].as_i64().unwrap();
    assert_eq!(
        delete(
            &app,
            &format!("/api/payments/{repay}"),
            &finance,
            "reject-consumed-payment"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert!(metadata(&db, "payments", repay).await.2.is_none());
    // Delivery edits retain old details; only live rows reserve/deliver quantities.
    let receipt=post(&app,"/api/payments",&finance,json!({"reference":"LIFE-RCP-1","date":"2026-11-14","amount":2300,"kind":"sales_invoice","invoice_ids":[sale]}),"receipt").await["id"].as_i64().unwrap();
    let line: i64 = sqlx::query_scalar(
        "SELECT id FROM document_lines WHERE document_id=? AND deleted_at IS NULL",
    )
    .bind(sale)
    .fetch_one(&db)
    .await
    .unwrap();
    let input = json!({"sales_invoice_id":sale,"driver_name":"Driver","date":"2026-11-14","address":"Warehouse","details":[{"sales_invoice_detail_id":line,"quantity":2}]});
    let order = post(&app, "/api/delivery-orders", &staff, input.clone(), "order").await["id"]
        .as_i64()
        .unwrap();
    post(
        &app,
        &format!("/api/delivery-orders/{order}"),
        &staff,
        input,
        "edit-order",
    )
    .await;
    let counts:(i64,i64)=sqlx::query_as("SELECT COUNT(*),CAST(SUM(deleted_at IS NULL) AS SIGNED) FROM delivery_order_details WHERE delivery_order_id=?").bind(order).fetch_one(&db).await.unwrap();
    assert_eq!(counts, (2, 1));
    post(
        &app,
        &format!("/api/delivery-orders/{order}/approve"),
        &finance,
        json!({}),
        "approve-order",
    )
    .await;
    assert_eq!(
        delete(
            &app,
            &format!("/api/delivery-orders/{order}"),
            &staff,
            "cannot-delete-approved"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        delete(
            &app,
            &format!("/api/payments/{receipt}"),
            &finance,
            "cannot-unpay-delivery"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    post(
        &app,
        &format!("/api/delivery-orders/{order}/status"),
        &finance,
        json!({"status":"Cancelled","cancellation_note":"Reverse shipment"}),
        "cancel-order",
    )
    .await;
    assert_eq!(
        delete(
            &app,
            &format!("/api/payments/{receipt}"),
            &finance,
            "remove-receipt"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        delete(
            &app,
            &format!("/api/delivery-orders/{order}"),
            &staff,
            "delete-cancelled-order"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(metadata(&db, "delivery_orders", order).await.2.is_some());
    let reserved: i64 =
        sqlx::query_scalar("SELECT on_delivery_quantity FROM document_lines WHERE id=?")
            .bind(line)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(reserved, 0);
    // Soft-deleted roles/users cannot be assigned, edited, authenticated or seen in lists.
    let role = post(
        &app,
        "/api/roles",
        &admin_cookie,
        json!({"name":"Temporary role","permissions":["profile.read"]}),
        "role",
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let role_before = metadata(&db, "roles", role).await;
    post(
        &app,
        &format!("/api/roles/{role}"),
        &admin_cookie,
        json!({"name":"Temporary role","permissions":[]}),
        "remove-role-permission",
    )
    .await;
    let role_after = metadata(&db, "roles", role).await;
    assert_eq!(role_before.0, role_after.0);
    assert!(role_after.1 > role_before.1);
    let deleted_grants: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM role_permissions WHERE role_id=? AND deleted_at IS NOT NULL",
    )
    .bind(role)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(deleted_grants, 1);
    post(
        &app,
        &format!("/api/roles/{role}"),
        &admin_cookie,
        json!({"name":"Temporary role","permissions":["profile.read"]}),
        "restore-role-permission",
    )
    .await;
    let grants:(i64,i64)=sqlx::query_as("SELECT COUNT(*),CAST(SUM(deleted_at IS NULL) AS SIGNED) FROM role_permissions WHERE role_id=?").bind(role).fetch_one(&db).await.unwrap();
    assert_eq!(grants, (1, 1));
    let user = post(
        &app,
        "/api/users",
        &admin_cookie,
        json!({"username":"temporary","password":"asdqwe123!","role_id":role}),
        "user",
    )
    .await["id"]
        .as_i64()
        .unwrap();
    let user_cookie = login(&app, "temporary").await;
    assert_eq!(
        delete(
            &app,
            &format!("/api/roles/{role}"),
            &admin_cookie,
            "role-assigned"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        delete(
            &app,
            &format!("/api/users/{user}"),
            &admin_cookie,
            "delete-user"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(metadata(&db, "users", user).await.2.is_some());
    assert_eq!(
        call(&app, "/api/auth/me", &user_cookie, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "/api/auth/refresh", &user_cookie, Some(json!({})), "")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        delete(
            &app,
            &format!("/api/roles/{role}"),
            &admin_cookie,
            "delete-role"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(metadata(&db, "roles", role).await.2.is_some());
    assert_eq!(
        call(
            &app,
            "/api/users",
            &admin_cookie,
            Some(json!({"username":"bad-role","password":"asdqwe123!","role_id":role})),
            "assign-deleted-role"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            "/api/lists/roles?q=Temporary",
            &admin_cookie,
            None,
            ""
        )
        .await
        .1["total"],
        0
    );
    drop(app);
    drop(a);
    cleanup(admin, db, &name).await;
}
