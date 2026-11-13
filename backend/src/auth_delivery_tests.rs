use crate::config::{App, router::router};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;
async fn call(
    app: &Router,
    path: &str,
    cookie: &str,
    body: Option<Value>,
    key: &str,
) -> (StatusCode, Value, HeaderMap) {
    let mut request = Request::builder()
        .uri(path)
        .method(if body.is_some() { "POST" } else { "GET" })
        .header("cookie", cookie)
        .header("content-type", "application/json");
    if !key.is_empty() {
        request = request.header("idempotency-key", key);
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value, headers)
}
fn cookie(headers: &HeaderMap) -> String {
    headers
        .get_all("set-cookie")
        .iter()
        .map(|h| h.to_str().unwrap().split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ")
}
async fn login(app: &Router, name: &str) -> String {
    let (s, _, h) = call(
        app,
        "/api/auth/login",
        "",
        Some(json!({"username":name,"password":"asdqwe123!"})),
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    for h in h.get_all("set-cookie") {
        let h = h.to_str().unwrap();
        assert!(h.contains("HttpOnly") && h.contains("SameSite=Strict"));
    }
    cookie(&h)
}
#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn auth_permissions_idempotency_and_delivery_workflow() {
    let url =
        std::env::var("TEST_MYSQL_ADMIN_URL").unwrap_or("mysql://root@127.0.0.1:3306/mysql".into());
    let admin = sqlx::MySqlPool::connect(&url).await.unwrap();
    let name = format!(
        "auth_delivery_test_{}_{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
    let opts: sqlx::mysql::MySqlConnectOptions = url.parse().unwrap();
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .connect_with(opts.database(&name))
        .await
        .unwrap();
    sqlx::migrate!().run(&db).await.unwrap();
    crate::auth_seed::seed_auth(&db).await.unwrap();
    crate::auth_seed::seed_auth(&db).await.unwrap();
    let a = App {
        db: db.clone(),
        http: reqwest::Client::new(),
        meili: "http://localhost:7700".into(),
        key: String::new(),
    };
    let app = router(a.clone(), "http://localhost:5173".parse().unwrap());
    assert_eq!(
        call(&app, "/api/documents", "", None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    let admin_cookie = login(&app, "admin").await;
    let staff = login(&app, "staff").await;
    let finance = login(&app, "finance").await;
    assert_eq!(
        call(&app, "/api/roles", &staff, None, "").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "/api/payments",
            &staff,
            Some(json!({})),
            "forbid-payment"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "/api/products",
            &staff,
            Some(json!({})),
            "forbid-product"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let hostile = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("origin", "https://attacker.example")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(hostile).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let product = json!({"id":0,"name":"Delivery test product","quantity":30,"status":"Active","description":"","price":100,"reorder_level":0});
    assert_eq!(
        call(
            &app,
            "/api/products",
            &admin_cookie,
            Some(product.clone()),
            ""
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (s, p, _) = call(
        &app,
        "/api/products",
        &admin_cookie,
        Some(product.clone()),
        "product",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let product_id = p["id"].as_i64().unwrap();
    let (s, p2, h) = call(
        &app,
        "/api/products",
        &admin_cookie,
        Some(product.clone()),
        "product",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p, p2);
    assert_eq!(h["idempotency-replayed"], "true");
    assert_eq!(
        call(
            &app,
            "/api/products",
            &admin_cookie,
            Some(json!({"different":true})),
            "product"
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let doc = json!({"kind":"sales_invoice","number":"SALE-DELIVERY","date":"2026-11-13","person":"Customer","lines":[{"product_id":product_id,"quantity":10,"price":100}]});
    let (s, d, _) = call(&app, "/api/sales-invoices", &staff, Some(doc), "sale").await;
    assert_eq!(s, StatusCode::OK, "{d}");
    let invoice = d["id"].as_i64().unwrap();
    // Updating lines adjusts the ledger before payment.
    let update = json!({"number":"SALE-DELIVERY","date":"2026-11-13","person":"Customer","lines":[{"product_id":product_id,"quantity":12,"price":100}]});
    let (s, b, _) = call(
        &app,
        &format!("/api/invoices/{invoice}"),
        &staff,
        Some(update.clone()),
        "edit-sale",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, detail, _) = call(&app, &format!("/api/documents/{invoice}"), &staff, None, "").await;
    let line = detail["lines"][0]["id"].as_i64().unwrap();
    let order = |qty: i64| json!({"sales_invoice_id":invoice,"driver_name":"Driver","date":"2026-11-13","address":"Jakarta","note":"Careful","details":[{"sales_invoice_detail_id":line,"quantity":qty,"notes":"Box"}]});
    assert_eq!(
        call(
            &app,
            "/api/delivery-orders",
            &staff,
            Some(order(7)),
            "unpaid"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let payment = json!({"reference":"PAY-DELIVERY","date":"2026-11-13","amount":200,"kind":"sales_invoice","invoice_ids":[invoice]});
    let (s, b, _) = call(
        &app,
        "/api/payments",
        &finance,
        Some(payment.clone()),
        "payment",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        call(&app, "/api/payments", &finance, Some(payment), "payment")
            .await
            .1,
        b
    );
    assert_eq!(
        call(
            &app,
            &format!("/api/invoices/{invoice}"),
            &staff,
            Some(update),
            "edit-paid"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (s, o1, _) = call(
        &app,
        "/api/delivery-orders",
        &staff,
        Some(order(7)),
        "order1",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{o1}");
    let id1 = o1["id"].as_i64().unwrap();
    let (s, o2, _) = call(
        &app,
        "/api/delivery-orders",
        &staff,
        Some(order(6)),
        "order2",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id2 = o2["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &app,
            &format!("/api/delivery-orders/{id1}/approve"),
            &staff,
            Some(json!({})),
            "staff-approve"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let path1 = format!("/api/delivery-orders/{id1}/approve");
    let path2 = format!("/api/delivery-orders/{id2}/approve");
    let (r1, r2) = tokio::join!(
        call(&app, &path1, &finance, Some(json!({})), "approve1"),
        call(&app, &path2, &finance, Some(json!({})), "approve2")
    );
    assert_eq!(
        usize::from(r1.0 == StatusCode::OK) + usize::from(r2.0 == StatusCode::OK),
        1,
        "{r1:?} {r2:?}"
    );
    let (winner, qty) = if r1.0 == StatusCode::OK {
        (id1, 7)
    } else {
        (id2, 6)
    };
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT delivered_quantity,on_delivery_quantity FROM document_lines WHERE id=?",
    )
    .bind(line)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(counts, (0, qty));
    let path = format!("/api/delivery-orders/{winner}/status");
    assert_eq!(
        call(
            &app,
            &path,
            &staff,
            Some(json!({"status":"On Delivery"})),
            "dispatch"
        )
        .await
        .0,
        StatusCode::OK
    );
    // Staff cannot cancel an approved delivery.
    assert_eq!(
        call(
            &app,
            &path,
            &staff,
            Some(json!({"status":"Cancelled","cancellation_note":"No"})),
            "staff-cancel"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (s, _, _) = call(
        &app,
        &path,
        &staff,
        Some(json!({"status":"Delivered"})),
        "delivered",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        call(
            &app,
            &path,
            &staff,
            Some(json!({"status":"Delivered"})),
            "delivered"
        )
        .await
        .0,
        StatusCode::OK
    );
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT delivered_quantity,on_delivery_quantity FROM document_lines WHERE id=?",
    )
    .bind(line)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(counts, (qty, 0));
    assert_eq!(
        call(
            &app,
            &path,
            &finance,
            Some(json!({"status":"Cancelled","cancellation_note":"No"})),
            "delivered-cancel"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    // Reserve the remaining quantity, cancel it, and verify the reservation is released.
    let (s, o3, _) = call(
        &app,
        "/api/delivery-orders",
        &staff,
        Some(order(12 - qty)),
        "order3",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id3 = o3["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &app,
            &format!("/api/delivery-orders/{id3}/approve"),
            &finance,
            Some(json!({})),
            "approve3"
        )
        .await
        .0,
        StatusCode::OK
    );
    let (s, req, _) = call(
        &app,
        &format!("/api/delivery-orders/{id3}/cancel-request"),
        &staff,
        Some(json!({"reason":"Customer request"})),
        "request-cancel3",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{req}");
    let request = req["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &app,
            &format!("/api/delivery-cancellation-requests/{request}/approve"),
            &staff,
            Some(json!({})),
            "staff-cancel-approve"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            &format!("/api/delivery-cancellation-requests/{request}/reject"),
            &finance,
            Some(json!({})),
            "reject-cancel3"
        )
        .await
        .0,
        StatusCode::OK
    );
    let (s, req, _) = call(
        &app,
        &format!("/api/delivery-orders/{id3}/cancel-request"),
        &staff,
        Some(json!({"reason":"Confirmed cancellation"})),
        "request-again3",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let request = req["id"].as_i64().unwrap();
    let (s, b, _) = call(
        &app,
        &format!("/api/delivery-cancellation-requests/{request}/approve"),
        &finance,
        Some(json!({})),
        "approve-cancel3",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let reserved: i64 =
        sqlx::query_scalar("SELECT on_delivery_quantity FROM document_lines WHERE id=?")
            .bind(line)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(reserved, 0);
    assert_eq!(
        call(
            &app,
            &format!("/api/invoices/{invoice}/cancel"),
            &finance,
            Some(json!({"reason":"Cannot void delivered"})),
            "void-delivered"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    // An ordinary undelivered invoice uses the Staff -> Finance approval flow.
    let(s,sale2,_)=call(&app,"/api/sales-invoices",&staff,Some(json!({"kind":"sales_invoice","number":"VOID-ME","date":"2026-11-13","person":"Customer","lines":[{"product_id":product_id,"quantity":2,"price":100}]})),"sale2").await;
    assert_eq!(s, StatusCode::OK);
    let sale2 = sale2["id"].as_i64().unwrap();
    let (s, req, _) = call(
        &app,
        &format!("/api/invoices/{sale2}/cancel"),
        &staff,
        Some(json!({"reason":"Mistake"})),
        "void-request",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(req["status"], "Pending");
    let req = req["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &app,
            &format!("/api/void-requests/{req}/approve"),
            &staff,
            Some(json!({})),
            "self-approve"
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (s, b, _) = call(
        &app,
        &format!("/api/void-requests/{req}/approve"),
        &finance,
        Some(json!({})),
        "approve-void",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    // Purchase edits and voids restore linked PO balances and both stock ledgers.
    let(s,po,_)=call(&app,"/api/purchase-orders",&staff,Some(json!({"kind":"purchase_order","number":"PO-EDIT-VOID","date":"2026-11-13","person":"Supplier","lines":[{"product_id":product_id,"quantity":4,"price":100}]})),"po-edit-void").await;
    assert_eq!(s, StatusCode::OK);
    let po = po["id"].as_i64().unwrap();
    let(s,purchase,_)=call(&app,"/api/purchase-invoices",&staff,Some(json!({"kind":"purchase_invoice","number":"PURCHASE-EDIT-VOID","date":"2026-11-13","person":"Supplier","purchase_order_id":po,"lines":[{"product_id":product_id,"quantity":4,"price":100}]})),"purchase-edit-void").await;
    assert_eq!(s, StatusCode::OK);
    let purchase = purchase["id"].as_i64().unwrap();
    let(s,b,_)=call(&app,&format!("/api/invoices/{purchase}"),&staff,Some(json!({"number":"PURCHASE-EDIT-VOID","date":"2026-11-13","person":"Supplier","lines":[{"product_id":product_id,"quantity":3,"price":100}]})),"edit-purchase").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, pd, _) = call(&app, &format!("/api/documents/{po}"), &staff, None, "").await;
    assert_eq!(pd["lines"][0]["realized_quantity"], 1);
    assert_eq!(pd["document"]["status"], "Partial");
    let (s, b, _) = call(
        &app,
        &format!("/api/invoices/{purchase}/cancel"),
        &finance,
        Some(json!({"reason":"Wrong receipt"})),
        "void-purchase",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, pd, _) = call(&app, &format!("/api/documents/{po}"), &staff, None, "").await;
    assert_eq!(pd["lines"][0]["realized_quantity"], 4);
    assert_eq!(pd["lines"][0]["status"], "Open");
    assert_eq!(pd["document"]["status"], "Open");
    let (_, pd, _) = call(
        &app,
        &format!("/api/documents/{purchase}"),
        &staff,
        None,
        "",
    )
    .await;
    assert_eq!(pd["cancellation"]["reason"], "Wrong receipt");
    let stock: (i64, i64) =
        sqlx::query_as("SELECT quantity,remaining_quantity FROM products WHERE id=?")
            .bind(product_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(stock, (18, 18));
    // Rotation replay revokes both access and refresh tokens in that session, not other sessions.
    let old = login(&app, "staff").await;
    let (s, _, headers) = call(&app, "/api/auth/refresh", &old, Some(json!({})), "").await;
    assert_eq!(s, StatusCode::OK);
    let rotated = cookie(&headers);
    assert_eq!(
        call(&app, "/api/auth/me", &rotated, None, "").await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "/api/auth/refresh", &old, Some(json!({})), "")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "/api/auth/me", &rotated, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "/api/auth/refresh", &rotated, Some(json!({})), "")
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&app, "/api/auth/me", &staff, None, "").await.0,
        StatusCode::OK
    );
    let (s, _, h) = call(&app, "/api/auth/logout", &finance, Some(json!({})), "").await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        h.get_all("set-cookie")
            .iter()
            .all(|h| h.to_str().unwrap().contains("Max-Age=0"))
    );
    assert_eq!(
        call(&app, "/api/auth/me", &finance, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    // Two simultaneous uses of the same refresh token trigger family revocation.
    let racing = login(&app, "staff").await;
    let (first, second) = tokio::join!(
        call(&app, "/api/auth/refresh", &racing, Some(json!({})), ""),
        call(&app, "/api/auth/refresh", &racing, Some(json!({})), "")
    );
    assert_eq!(
        usize::from(first.0 == StatusCode::OK) + usize::from(second.0 == StatusCode::OK),
        1
    );
    let rotated = if first.0 == StatusCode::OK {
        cookie(&first.2)
    } else {
        cookie(&second.2)
    };
    assert_eq!(
        call(&app, "/api/auth/me", &rotated, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    // Concurrent duplicate writes cannot create two records.
    let product = json!({"id":0,"name":"Concurrent product","quantity":3,"status":"Active","description":"","price":100,"reorder_level":0});
    let (first, second) = tokio::join!(
        call(
            &app,
            "/api/products",
            &admin_cookie,
            Some(product.clone()),
            "same-concurrent-key"
        ),
        call(
            &app,
            "/api/products",
            &admin_cookie,
            Some(product.clone()),
            "same-concurrent-key"
        )
    );
    assert!(first.0 == StatusCode::OK || second.0 == StatusCode::OK);
    assert!([StatusCode::OK, StatusCode::CONFLICT].contains(&first.0));
    assert!([StatusCode::OK, StatusCode::CONFLICT].contains(&second.0));
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE name='Concurrent product'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(count, 1);
    // Role assignment and permission edits invalidate sessions immediately.
    let (s, custom, _) = call(
        &app,
        "/api/roles",
        &admin_cookie,
        Some(json!({"name":"Reader","permissions":["profile.read"]})),
        "reader-role",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let role = custom["id"].as_i64().unwrap();
    let staff_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username='staff'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            &format!("/api/users/{staff_id}/role"),
            &admin_cookie,
            Some(json!({"role_id":role,"active":true})),
            "assign-reader"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "/api/auth/me", &staff, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    let reader = login(&app, "staff").await;
    assert_eq!(
        call(&app, "/api/documents", &reader, None, "").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            &format!("/api/roles/{role}"),
            &admin_cookie,
            Some(json!({"name":"Reader","permissions":["profile.read","invoices.read"]})),
            "edit-reader"
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "/api/auth/me", &reader, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    let reader = login(&app, "staff").await;
    let another = login(&app, "staff").await;
    assert_eq!(
        call(&app, "/api/documents", &reader, None, "").await.0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "/api/auth/logout-all", &reader, Some(json!({})), "")
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "/api/auth/me", &another, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    let session = login(&app, "staff").await;
    assert_eq!(
        call(
            &app,
            "/api/auth/password",
            &session,
            Some(
                json!({"current_password":"wrong-password","new_password":"new-strong-password!"})
            ),
            ""
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "/api/auth/password",
            &session,
            Some(json!({"current_password":"asdqwe123!","new_password":"new-strong-password!"})),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "/api/auth/me", &session, None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "/api/auth/login",
            "",
            Some(json!({"username":"staff","password":"asdqwe123!"})),
            ""
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "/api/auth/login",
            "",
            Some(json!({"username":"staff","password":"new-strong-password!"})),
            ""
        )
        .await
        .0,
        StatusCode::OK
    );
    // Used refresh tokens are retained until the family's absolute expiry for reuse detection.
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE used=TRUE")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(before > 0);
    crate::feature::auth::cleanup(&a).await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE used=TRUE")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(before, after);
    sqlx::query("UPDATE auth_sessions SET expires_at=0")
        .execute(&db)
        .await
        .unwrap();
    crate::feature::auth::cleanup(&a).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    drop(app);
    drop(a);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
}
