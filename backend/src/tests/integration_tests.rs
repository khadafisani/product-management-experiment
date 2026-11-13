use crate::{
    config::App,
    feature::{
        document::{
            controller::{create_document, detail, documents, preview_document},
            entity::{DocumentInput, LineInput},
        },
        forecast::{controller::forecast, entity::ForecastQuery},
        payment::{
            controller::{pay, payments},
            entity::PaymentInput,
        },
        product::{
            controller::{products, save_product},
            entity::{ProductInput, active},
        },
    },
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde_json::json;
use sqlx::mysql::MySqlPoolOptions;
use std::env;

#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn migration_preserves_existing_inventory_without_releasing_it_twice() {
    let admin_url =
        env::var("TEST_MYSQL_ADMIN_URL").unwrap_or("mysql://root@127.0.0.1:3306/mysql".into());
    let admin = MySqlPoolOptions::new().connect(&admin_url).await.unwrap();
    let name = format!(
        "stockroom_migration_test_{}_{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
    let options: sqlx::mysql::MySqlConnectOptions = admin_url.parse().unwrap();
    let db = MySqlPoolOptions::new()
        .connect_with(options.database(&name))
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0001_initial.sql"))
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO products(name,quantity,description) VALUES('Existing stock',5,'Legacy baseline')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO documents(kind,number,date,person,grand_total,remaining_payment,status) VALUES('purchase_invoice','LEGACY-PI',CURRENT_DATE,'Test operator',500,500,'Unpaid')").execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO stock_movements(product_id,quantity,reason) VALUES(1,5,'Legacy stock')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0002_remaining_quantity.sql"))
        .execute(&db)
        .await
        .unwrap();
    let stock: (i64, i64) =
        sqlx::query_as("SELECT quantity,remaining_quantity FROM products WHERE id=1")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(stock, (5, 5));
    let (handled,): (bool,) = sqlx::query_as("SELECT stock_released FROM documents WHERE id=1")
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(handled);
    let (available_delta,): (i64,) =
        sqlx::query_as("SELECT remaining_delta FROM stock_movements WHERE id=1")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(available_delta, 5);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
}

// Creates its own database; never writes to the application database.
#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn purchase_payment_sales_and_forecast_workflow() {
    let admin_url =
        env::var("TEST_MYSQL_ADMIN_URL").unwrap_or("mysql://root@127.0.0.1:3306/mysql".into());
    let admin = MySqlPoolOptions::new().connect(&admin_url).await.unwrap();
    let name = format!(
        "stockroom_test_{}_{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
    let options: sqlx::mysql::MySqlConnectOptions = admin_url.parse().unwrap();
    let db = MySqlPoolOptions::new()
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("SET time_zone = '+07:00'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options.database(&name))
        .await
        .unwrap();
    sqlx::migrate!().run(&db).await.unwrap();
    let a = App {
        db: db.clone(),
        http: reqwest::Client::new(),
        meili: "http://127.0.0.1:7700".into(),
        key: String::new(),
    };
    let p = save_product(
        State(a.clone()),
        Json(ProductInput {
            id: 0,
            name: "Test keyboard".into(),
            quantity: 0,
            remaining_quantity: None,
            status: active(),
            photo: None,
            description: String::new(),
            price: 100_000,
            reorder_level: 5,
        }),
    )
    .await
    .unwrap()
    .0["id"]
        .as_i64()
        .unwrap();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let doc = |kind: &str, number: &str, qty: i64, po: Option<i64>| DocumentInput {
        kind: kind.into(),
        number: number.into(),
        date: date.clone(),
        person: "Test operator".into(),
        purchase_order_id: po,
        confirm_new_products: false,
        lines: vec![LineInput {
            product_id: p,
            item_name: None,
            quantity: qty,
            price: 100_000,
        }],
    };
    let po = create_document(
        State(a.clone()),
        Json(doc("purchase_order", "PO-TEST", 8, None)),
    )
    .await
    .unwrap()
    .0["id"]
        .as_i64()
        .unwrap();
    assert_eq!(products(State(a.clone())).await.unwrap().0[0].quantity, 0);
    let inv1 = create_document(
        State(a.clone()),
        Json(doc("purchase_invoice", "PINV-1", 10, Some(po))),
    )
    .await
    .unwrap()
    .0["id"]
        .as_i64()
        .unwrap();
    let inv2 = create_document(
        State(a.clone()),
        Json(doc("purchase_invoice", "PINV-2", 10, None)),
    )
    .await
    .unwrap()
    .0["id"]
        .as_i64()
        .unwrap();
    let po_info = detail(State(a.clone()), Path(po)).await.unwrap().0;
    assert_eq!(po_info["lines"][0]["realized_quantity"], -2);
    assert_eq!(po_info["document"]["status"], "Warning");
    assert_eq!(po_info["total_product"], 1);
    assert_eq!(po_info["total_quantity"], 8);
    let payment = |reference: &str, amount: i64| PaymentInput {
        reference: reference.into(),
        date: date.clone(),
        amount,
        invoice_ids: vec![inv1, inv2],
        kind: "purchase_invoice".into(),
    };
    assert!(
        pay(State(a.clone()), Json(payment("INVALID", 900_000)))
            .await
            .is_err()
    );
    assert!(
        payments(State(a.clone()))
            .await
            .unwrap()
            .0
            .as_array()
            .unwrap()
            .is_empty()
    );
    let _ = pay(State(a.clone()), Json(payment("PAY-1", 1_200_000)))
        .await
        .unwrap();
    let docs = documents(State(a.clone())).await.unwrap().0;
    assert_eq!(docs.iter().find(|d| d.id == inv1).unwrap().status, "Paid");
    let second = docs.iter().find(|d| d.id == inv2).unwrap();
    assert_eq!(second.status, "Partial Paid");
    assert_eq!(second.remaining_payment, 800_000);
    let sale = create_document(
        State(a.clone()),
        Json(doc("sales_invoice", "SINV-1", 2, None)),
    )
    .await
    .unwrap()
    .0["id"]
        .as_i64()
        .unwrap();
    assert_eq!(products(State(a.clone())).await.unwrap().0[0].quantity, 18);
    assert!(
        create_document(
            State(a.clone()),
            Json(doc("sales_invoice", "OVERSELL", 19, None))
        )
        .await
        .is_err()
    );
    assert_eq!(documents(State(a.clone())).await.unwrap().0.len(), 4);
    assert_eq!(products(State(a.clone())).await.unwrap().0[0].quantity, 18);
    let _ = pay(
        State(a.clone()),
        Json(PaymentInput {
            reference: "RECEIPT-1".into(),
            date: date.clone(),
            amount: 50_000,
            invoice_ids: vec![sale],
            kind: "sales_invoice".into(),
        }),
    )
    .await
    .unwrap();
    let docs = documents(State(a.clone())).await.unwrap().0;
    assert_eq!(
        docs.iter()
            .find(|d| d.id == sale)
            .unwrap()
            .remaining_payment,
        150_000
    );
    let f = forecast(
        State(a.clone()),
        Query(ForecastQuery {
            days: 30,
            lead_time: 7,
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(
        (f[0]["daily_sales"].as_f64().unwrap() - 2.0 / 30.0).abs() < 0.001,
        "forecast={f}"
    );
    assert_eq!(f[0]["quantity"], 8);
    assert_eq!(f[0]["remaining_quantity"], 8);
    assert_eq!(f[0]["suggested_order"], 0);
    let movement_total: (sqlx::types::Decimal,) =
        sqlx::query_as("SELECT SUM(quantity) FROM stock_movements WHERE product_id=?")
            .bind(p)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(movement_total.0.to_string(), "18");

    // Preview is read-only. New products require confirmation and share the document transaction.
    let fresh = |kind: &str,
                 number: &str,
                 product_name: &str,
                 linked: Option<i64>,
                 confirmed: bool|
     -> DocumentInput {
        serde_json::from_value(
            json!({"kind":kind,"number":number,"date":date,"person":"Test operator",
            "purchase_order_id":linked,"confirm_new_products":confirmed,
            "lines":[{"item_name":product_name,"quantity":3,"price":125_000}]}),
        )
        .unwrap()
    };
    let preview = preview_document(
        State(a.clone()),
        Json(fresh("purchase_order", "PO-NEW", "New chair", None, false)),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(preview["new_products"][0]["name"], "New chair");
    assert_eq!(preview["new_products"][0]["quantity"], 0);
    assert_eq!(products(State(a.clone())).await.unwrap().0.len(), 1);
    assert!(
        create_document(
            State(a.clone()),
            Json(fresh("purchase_order", "PO-NEW", "New chair", None, false))
        )
        .await
        .is_err()
    );
    assert_eq!(products(State(a.clone())).await.unwrap().0.len(), 1);
    let new_po = create_document(
        State(a.clone()),
        Json(fresh("purchase_order", "PO-NEW", "New chair", None, true)),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(new_po["created_products"].as_array().unwrap().len(), 1);
    let chair = products(State(a.clone()))
        .await
        .unwrap()
        .0
        .into_iter()
        .find(|p| p.name == "New chair")
        .unwrap();
    assert_eq!(chair.quantity, 0);
    let preview = preview_document(
        State(a.clone()),
        Json(fresh(
            "purchase_invoice",
            "PI-REUSE",
            "  NEW CHAIR  ",
            None,
            false,
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(preview["new_products"].as_array().unwrap().is_empty());
    let reused = create_document(
        State(a.clone()),
        Json(fresh(
            "purchase_invoice",
            "PI-REUSE",
            "  NEW CHAIR  ",
            None,
            false,
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(reused["created_products"].as_array().unwrap().is_empty());
    assert_eq!(
        products(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .find(|p| p.id == chair.id)
            .unwrap()
            .quantity,
        3
    );
    let inv_preview = preview_document(
        State(a.clone()),
        Json(fresh(
            "purchase_invoice",
            "PI-NEW",
            "New screen",
            None,
            false,
        )),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(inv_preview["new_products"][0]["quantity"], 3);
    let unplanned = create_document(
        State(a.clone()),
        Json(fresh(
            "purchase_invoice",
            "PI-NEW",
            "New screen",
            Some(po),
            true,
        )),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(unplanned["created_products"][0]["name"], "New screen");
    let po_detail = detail(State(a.clone()), Path(po)).await.unwrap().0;
    let extra = po_detail["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|line| line["item_name"] == "New screen")
        .unwrap();
    assert_eq!(extra["quantity"], 0);
    assert_eq!(extra["realized_quantity"], -3);
    assert_eq!(extra["status"], "Warning");
    assert_eq!(po_detail["document"]["grand_total"], 800_000);
    assert!(
        create_document(
            State(a.clone()),
            Json(fresh(
                "sales_invoice",
                "SALE-NEW",
                "Forbidden new product",
                None,
                true
            ))
        )
        .await
        .is_err()
    );
    let mut failing = fresh(
        "purchase_invoice",
        "FAIL-ATOMIC",
        "Rolled back product",
        None,
        true,
    );
    failing.lines.push(LineInput {
        product_id: 999999,
        item_name: None,
        quantity: 1,
        price: 1,
    });
    assert!(
        create_document(State(a.clone()), Json(failing))
            .await
            .is_err()
    );
    assert!(
        !products(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .any(|p| p.name == "Rolled back product")
    );
    assert!(
        !documents(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .any(|d| d.number == "FAIL-ATOMIC")
    );
    let repeated=serde_json::from_value(json!({"kind":"purchase_order","number":"DUP-PRODUCT","date":date,"person":"Test operator",
        "lines":[{"product_id":chair.id,"quantity":1,"price":1},{"item_name":"NEW CHAIR","quantity":1,"price":1}]})).unwrap();
    assert!(
        create_document(State(a.clone()), Json(repeated))
            .await
            .is_err()
    );

    // Unpaid purchases are physically received but unavailable to sales.
    assert_eq!(
        products(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .find(|p| p.id == chair.id)
            .unwrap()
            .remaining_quantity,
        0
    );
    let sale_input = |number: &str| {
        serde_json::from_value::<DocumentInput>(json!({"kind":"sales_invoice","number":number,"date":date,"person":"Test customer","lines":[{"product_id":chair.id,"quantity":2,"price":200_000}]})).unwrap()
    };
    assert!(
        create_document(State(a.clone()), Json(sale_input("BLOCK-UNPAID")))
            .await
            .is_err()
    );
    let chair_invoice = reused["id"].as_i64().unwrap();
    let chair_payment = |reference: &str, amount: i64| PaymentInput {
        reference: reference.into(),
        date: date.clone(),
        amount,
        invoice_ids: vec![chair_invoice],
        kind: "purchase_invoice".into(),
    };
    let _ = pay(
        State(a.clone()),
        Json(chair_payment("CHAIR-PARTIAL", 100_000)),
    )
    .await
    .unwrap();
    assert_eq!(
        products(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .find(|p| p.id == chair.id)
            .unwrap()
            .remaining_quantity,
        0
    );
    let _ = pay(
        State(a.clone()),
        Json(chair_payment("CHAIR-FINAL", 275_000)),
    )
    .await
    .unwrap();
    assert_eq!(
        products(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .find(|p| p.id == chair.id)
            .unwrap()
            .remaining_quantity,
        3
    );
    let mut tx = db.begin().await.unwrap();
    crate::feature::purchase_invoice::service::release_paid_stock(&mut tx, chair_invoice)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        products(State(a.clone()))
            .await
            .unwrap()
            .0
            .iter()
            .find(|p| p.id == chair.id)
            .unwrap()
            .remaining_quantity,
        3
    );

    // Concurrent sales cannot both claim the same remaining inventory.
    let (first, second) = tokio::join!(
        create_document(State(a.clone()), Json(sale_input("RACE-A"))),
        create_document(State(a.clone()), Json(sale_input("RACE-B")))
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let winner = first.or(second).unwrap().0["id"].as_i64().unwrap();
    let current = products(State(a.clone()))
        .await
        .unwrap()
        .0
        .into_iter()
        .find(|p| p.id == chair.id)
        .unwrap();
    assert_eq!((current.quantity, current.remaining_quantity), (1, 1));
    let cancel = || crate::feature::sale_invoice::entity::CancelInput {
        reason: "Customer cancelled".into(),
    };
    let result = crate::feature::sale_invoice::service::cancel(a.clone(), winner, cancel())
        .await
        .unwrap();
    assert_eq!(result["refund_due"], 0);
    let repeated = crate::feature::sale_invoice::service::cancel(a.clone(), winner, cancel())
        .await
        .unwrap();
    assert_eq!(repeated["already_cancelled"], true);
    let current = products(State(a.clone()))
        .await
        .unwrap()
        .0
        .into_iter()
        .find(|p| p.id == chair.id)
        .unwrap();
    assert_eq!((current.quantity, current.remaining_quantity), (3, 3));
    let refund = crate::feature::sale_invoice::service::cancel(a.clone(), sale, cancel())
        .await
        .unwrap();
    assert_eq!(refund["refund_due"], 50_000);
    assert!(
        pay(
            State(a.clone()),
            Json(PaymentInput {
                reference: "CANCELLED-PAYMENT".into(),
                date: date.clone(),
                amount: 1,
                kind: "sales_invoice".into(),
                invoice_ids: vec![sale]
            })
        )
        .await
        .is_err()
    );
    let f = forecast(
        State(a.clone()),
        Query(ForecastQuery {
            days: 30,
            lead_time: 7,
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(
        f.as_array()
            .unwrap()
            .iter()
            .all(|p| p["daily_sales"] == 0.0)
    );
    let cancelled_detail = detail(State(a.clone()), Path(sale)).await.unwrap().0;
    assert_eq!(cancelled_detail["document"]["status"], "Cancelled");
    assert_eq!(cancelled_detail["cancellation"]["refund_due"], 50_000);
    let ledger_violations:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM products p LEFT JOIN (SELECT product_id,SUM(quantity) physical,SUM(remaining_delta) available FROM stock_movements GROUP BY product_id) m ON m.product_id=p.id WHERE p.quantity<>COALESCE(m.physical,0) OR p.remaining_quantity<>COALESCE(m.available,0) OR p.remaining_quantity<0 OR p.remaining_quantity>p.quantity").fetch_one(&db).await.unwrap();
    assert_eq!(ledger_violations.0, 0);
    drop(a);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
}
