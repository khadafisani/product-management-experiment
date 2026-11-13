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
    let (_, receiving, _) = call(
        &app,
        &format!("/api/documents/{purchase}"),
        &staff,
        None,
        "",
    )
    .await;
    assert_eq!(receiving["purchase_order_lines"][0]["received_quantity"], 3);
    assert_eq!(
        receiving["purchase_order_lines"][0]["remaining_quantity"],
        1
    );
    assert_eq!(receiving["purchase_order_lines"][0]["invoice_quantity"], 3);

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
    // Detail endpoints connect records in both directions, including split payments.
    let(s,split_sale,_)=call(&app,"/api/sales-invoices",&staff,Some(json!({"kind":"sales_invoice","number":"SPLIT-RECEIPT","date":"2026-11-13","person":"Customer","lines":[{"product_id":product_id,"quantity":1,"price":100}]})),"split-sale").await;
    assert_eq!(s, StatusCode::OK);
    let split_sale = split_sale["id"].as_i64().unwrap();
    let(s,split_payment,_)=call(&app,"/api/payments",&finance,Some(json!({"reference":"SPLIT-PAYMENT","date":"2026-11-13","amount":150,"kind":"sales_invoice","invoice_ids":[split_sale,invoice]})),"split-payment").await;
    assert_eq!(s, StatusCode::OK);
    let split_payment = split_payment["id"].as_i64().unwrap();
    let (s, connected, _) =
        call(&app, &format!("/api/documents/{invoice}"), &staff, None, "").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(connected["payments"].as_array().unwrap().len(), 2);
    assert_eq!(connected["delivery_orders"].as_array().unwrap().len(), 3);
    assert!(
        connected["payments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == split_payment)
    );
    let (s, receipt, _) = call(
        &app,
        &format!("/api/payments/{split_payment}"),
        &staff,
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(receipt["invoices"].as_array().unwrap().len(), 2);
    assert_eq!(
        receipt["invoices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["allocated_amount"].as_i64().unwrap())
            .sum::<i64>(),
        150
    );
    let (_, order_detail, _) = call(
        &app,
        &format!("/api/delivery-orders/{id1}"),
        &staff,
        None,
        "",
    )
    .await;
    assert_eq!(order_detail["sales_invoice"]["id"], invoice);
    assert_eq!(order_detail["details"][0]["product_id"], product_id);
    let (_, purchase_detail, _) = call(
        &app,
        &format!("/api/documents/{purchase}"),
        &staff,
        None,
        "",
    )
    .await;
    assert_eq!(purchase_detail["purchase_order"]["id"], po);
    let po_line = &purchase_detail["purchase_order_lines"][0];
    assert_eq!(po_line["ordered_quantity"], 4);
    assert_eq!(po_line["received_quantity"], 0);
    assert_eq!(po_line["remaining_quantity"], 4);
    assert_eq!(po_line["invoice_quantity"], 3);

    let (_, po_detail, _) = call(&app, &format!("/api/documents/{po}"), &staff, None, "").await;
    assert_eq!(po_detail["purchase_invoices"][0]["id"], purchase);
    let (s, catalog, _) = call(
        &app,
        &format!("/api/products/{product_id}"),
        &staff,
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(catalog["purchase_orders"][0]["id"], po);
    assert_eq!(catalog["purchase_invoices"][0]["id"], purchase);
    assert_eq!(catalog["delivery_orders"].as_array().unwrap().len(), 3);
    for path in [
        "/api/products/999999",
        "/api/payments/999999",
        "/api/documents/999999",
        "/api/delivery-orders/999999",
    ] {
        assert_eq!(
            call(&app, path, &staff, None, "").await.0,
            StatusCode::NOT_FOUND
        );
    }
    // Related-record responses respect permissions for every nested data type.
    let(s,restricted,_)=call(&app,"/api/roles",&admin_cookie,Some(json!({"name":"CatalogOnly","permissions":["profile.read","products.read","invoices.read"]})),"catalog-role").await;
    assert_eq!(s, StatusCode::OK);
    let restricted = restricted["id"].as_i64().unwrap();
    assert_eq!(
        call(
            &app,
            "/api/users",
            &admin_cookie,
            Some(json!({"username":"catalog","password":"asdqwe123!","role_id":restricted})),
            "catalog-user"
        )
        .await
        .0,
        StatusCode::OK
    );
    let catalog_user = login(&app, "catalog").await;
    let (_, restricted_invoice, _) = call(
        &app,
        &format!("/api/documents/{invoice}"),
        &catalog_user,
        None,
        "",
    )
    .await;
    assert_eq!(restricted_invoice["access"]["payments"], false);
    assert_eq!(restricted_invoice["payments"], json!([]));
    assert_eq!(restricted_invoice["delivery_orders"], json!([]));
    assert_eq!(
        call(
            &app,
            &format!("/api/roles/{restricted}"),
            &admin_cookie,
            Some(json!({"name":"CatalogOnly","permissions":["profile.read","products.read"]})),
            "restrict-catalog"
        )
        .await
        .0,
        StatusCode::OK
    );
    let catalog_user = login(&app, "catalog").await;
    let (s, restricted_product, _) = call(
        &app,
        &format!("/api/products/{product_id}"),
        &catalog_user,
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(restricted_product["purchase_orders"], json!([]));
    assert_eq!(restricted_product["purchase_invoices"], json!([]));
    assert_eq!(restricted_product["sales_invoices"], json!([]));
    assert_eq!(restricted_product["delivery_orders"], json!([]));
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

#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn cursor_lists_filters_snapshot_and_approval_numbers() {
    let url =
        std::env::var("TEST_MYSQL_ADMIN_URL").unwrap_or("mysql://root@127.0.0.1:3306/mysql".into());
    let admin = sqlx::MySqlPool::connect(&url).await.unwrap();
    let name = format!(
        "cursor_test_{}_{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
    let opts: sqlx::mysql::MySqlConnectOptions = url.parse().unwrap();
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(5)
        .connect_with(opts.database(&name))
        .await
        .unwrap();
    sqlx::migrate!().run(&db).await.unwrap();
    crate::auth_seed::seed_auth(&db).await.unwrap();
    let app = router(
        App {
            db: db.clone(),
            http: reqwest::Client::new(),
            meili: "http://localhost:7700".into(),
            key: String::new(),
        },
        "http://localhost:5173".parse().unwrap(),
    );
    let admin_cookie = login(&app, "admin").await;
    let staff = login(&app, "staff").await;
    let staff_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username='staff'")
        .fetch_one(&db)
        .await
        .unwrap();
    let mut expected = Vec::new();
    for i in 0..67 {
        let day = if i < 60 {
            "2026-11-10"
        } else if i < 64 {
            "2026-11-09"
        } else {
            "2026-11-11"
        };
        let id=sqlx::query("INSERT INTO documents(kind,number,date,person,grand_total,remaining_payment,status) VALUES('sales_invoice',?,?, 'Customer',100,100,'Unpaid')")
          .bind(format!("CURSOR-SI-{i:03}")).bind(day).execute(&db).await.unwrap().last_insert_id() as i64;
        expected.push((day.to_string(), id));
    }
    expected.sort_by(|a, b| b.cmp(a));
    let (status, first, _) = call(
        &app,
        "/api/lists/sales-invoices?limit=7",
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["total"], 67);
    assert_eq!(first["items"].as_array().unwrap().len(), 7);
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    // New backdated inserts must not enter an in-progress traversal.
    sqlx::query("INSERT INTO documents(kind,number,date,person,grand_total,remaining_payment,status) VALUES('sales_invoice','AFTER-SNAPSHOT','2026-11-09','Customer',100,100,'Unpaid')").execute(&db).await.unwrap();
    let (s, restored, _) = call(
        &app,
        &format!(
            "/api/lists/sales-invoices?limit=7&cursor={}",
            first["start_cursor"].as_str().unwrap()
        ),
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(restored["total"], 67);
    assert_eq!(restored["items"], first["items"]);
    let mut rows = first["items"].as_array().unwrap().clone();
    let mut next = Some(cursor.clone());
    while let Some(c) = next {
        let (status, page, _) = call(
            &app,
            &format!("/api/lists/sales-invoices?limit=7&cursor={c}"),
            &admin_cookie,
            None,
            "",
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["total"], 67);
        rows.extend(page["items"].as_array().unwrap().clone());
        next = page["next_cursor"].as_str().map(str::to_string);
    }
    assert_eq!(
        rows.iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        expected.iter().map(|r| r.1).collect::<Vec<_>>()
    );
    let (s, range, _) = call(
        &app,
        "/api/lists/sales-invoices?limit=100&from=2026-11-10&to=2026-11-10",
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(range["total"], 60);
    assert!(
        range["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["date"] == "2026-11-10")
    );
    for path in [
        format!("/api/lists/sales-invoices?limit=7&status=Paid&cursor={cursor}"),
        format!("/api/lists/purchase-invoices?limit=7&cursor={cursor}"),
        "/api/lists/sales-invoices?cursor=zz".into(),
        "/api/lists/sales-invoices?limit=101".into(),
        "/api/lists/sales-invoices?from=2026-02-30".into(),
        "/api/lists/sales-invoices?from=2026-11-12&to=2026-11-01".into(),
        "/api/lists/payments?kind=purchase_order".into(),
    ] {
        assert_eq!(
            call(&app, &path, &admin_cookie, None, "").await.0,
            StatusCode::BAD_REQUEST,
            "{path}"
        );
    }
    assert_eq!(
        call(
            &app,
            &format!("/api/lists/sales-invoices?limit=7&cursor={cursor}"),
            &staff,
            None,
            ""
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(&app, "/api/lists/users", &staff, None, "").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&app, "/api/lists/products", "", None, "").await.0,
        StatusCode::UNAUTHORIZED
    );
    for (kind, reference) in [
        ("sales_invoice", "RECEIPT-1"),
        ("purchase_invoice", "SUPPLIER-1"),
    ] {
        sqlx::query(
            "INSERT INTO payments(reference,date,amount,kind) VALUES(?,'2026-11-10',100,?)",
        )
        .bind(reference)
        .bind(kind)
        .execute(&db)
        .await
        .unwrap();
    }
    let (_, receipts, _) = call(
        &app,
        "/api/lists/payments?kind=sales_invoice&from=2026-11-10&to=2026-11-10",
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert_eq!(receipts["total"], 1);
    assert_eq!(receipts["items"][0]["reference"], "RECEIPT-1");
    let (_, literal, _) = call(
        &app,
        "/api/lists/sales-invoices?q=%25",
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert_eq!(literal["total"], 0);
    let invoice = expected[0].1;
    sqlx::query("INSERT INTO invoice_void_requests(invoice_id,requested_by,reason) VALUES(?,?,'Test request')").bind(invoice).bind(staff_id).execute(&db).await.unwrap();
    let delivery=sqlx::query("INSERT INTO delivery_orders(sales_invoice_id,driver_name,date,address,note,cancellation_note,status,created_by) VALUES(?,'Driver','2026-11-10','Address','','','Draft',?)").bind(invoice).bind(staff_id).execute(&db).await.unwrap().last_insert_id();
    sqlx::query("INSERT INTO delivery_cancellation_requests(delivery_order_id,requested_by,reason) VALUES(?,?,'Test cancellation')").bind(delivery).bind(staff_id).execute(&db).await.unwrap();
    let (_, approvals, _) = call(
        &app,
        "/api/lists/invoice-approvals",
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert!(
        approvals["items"][0]["invoice_number"]
            .as_str()
            .unwrap()
            .starts_with("CURSOR-SI-")
    );
    assert_eq!(approvals["items"][0]["requested_by"], "staff");
    let (_, cancellations, _) = call(
        &app,
        "/api/lists/delivery-cancellations",
        &admin_cookie,
        None,
        "",
    )
    .await;
    assert_eq!(
        cancellations["items"][0]["delivery_order_number"],
        format!("DO-{delivery:06}")
    );
    sqlx::query("INSERT INTO products(name,description,quantity,remaining_quantity) VALUES('Product','',3,3)").execute(&db).await.unwrap();
    for resource in [
        "products",
        "purchase-orders",
        "purchase-invoices",
        "documents",
        "delivery-orders",
        "roles",
        "users",
        "forecast",
    ] {
        let (s, body, _) = call(
            &app,
            &format!("/api/lists/{resource}?limit=1"),
            &admin_cookie,
            None,
            "",
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{resource}: {body}");
        assert!(body["items"].as_array().unwrap().len() <= 1);
        if let Some(cursor) = body["next_cursor"].as_str() {
            assert_eq!(
                call(
                    &app,
                    &format!("/api/lists/{resource}?limit=1&cursor={cursor}"),
                    &admin_cookie,
                    None,
                    ""
                )
                .await
                .0,
                StatusCode::OK
            );
        }
    }
    drop(app);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
}
