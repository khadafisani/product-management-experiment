use super::*;
#[tokio::test]
#[ignore = "Requires local MySQL create/drop-database permission"]
async fn yearly_history_is_atomic_connected_and_safe_to_rerun() {
    let url =
        std::env::var("TEST_MYSQL_ADMIN_URL").unwrap_or("mysql://root@127.0.0.1:3306/mysql".into());
    let admin = MySqlPool::connect(&url).await.unwrap();
    let name = format!(
        "year_seed_test_{}_{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
    let opts: sqlx::mysql::MySqlConnectOptions = url.parse().unwrap();
    let db = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(4)
        .connect_with(opts.database(&name))
        .await
        .unwrap();
    sqlx::migrate!().run(&db).await.unwrap();
    crate::seeds::auth::seed_auth(&db).await.unwrap();
    let sentinel = sqlx
        ::query(
            "INSERT INTO products(name,quantity,remaining_quantity,description,price) VALUES('Existing user product',17,7,'Keep me',100)"
        )
        .execute(&db).await
        .unwrap()
        .last_insert_id() as i64;
    let end = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
    let (first, second) = tokio::join!(seed_year(&db, end), seed_year(&db, end));
    let first = first.unwrap();
    let second = second.unwrap();
    assert_ne!(first.inserted, second.inserted);
    assert_eq!(first.report, second.report);
    let r = first.report;
    assert_eq!(r.products, 40);
    assert_eq!(r.days, 365);
    assert_eq!(r.start_date, "2025-11-15");
    assert_eq!(r.end_date, "2026-11-14");
    assert_eq!(r.sales_invoices, 1460);
    assert!(
        r.purchase_orders >= 96
            && r.purchase_invoices >= 100
            && r.delivery_orders > 1200
            && r.payments > 1000
            && r.document_lines > 4000
    );
    let days: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT date) FROM documents WHERE number LIKE 'YEAR-SI-%'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(days, r.days);
    let range: (String, String) = sqlx
        ::query_as(
            "SELECT CAST(MIN(date) AS CHAR),CAST(MAX(date) AS CHAR) FROM documents WHERE number LIKE 'YEAR-SI-%'"
        )
        .fetch_one(&db).await
        .unwrap();
    assert_eq!(range, (r.start_date.clone(), r.end_date.clone()));
    let invalid_dates: i64 = sqlx
        ::query_scalar(
            "SELECT COUNT(*) FROM payment_allocations a JOIN payments p ON p.id=a.payment_id JOIN documents d ON d.id=a.invoice_id WHERE p.reference LIKE 'YEAR-%' AND p.date<d.date"
        )
        .fetch_one(&db).await
        .unwrap();
    assert_eq!(invalid_dates, 0);
    let statuses: Vec<String> = sqlx
        ::query_scalar(
            "SELECT DISTINCT o.status FROM delivery_orders o JOIN documents d ON d.id=o.sales_invoice_id WHERE d.number LIKE 'YEAR-%'"
        )
        .fetch_all(&db).await
        .unwrap();
    for status in ["Draft", "Approved", "On Delivery", "Delivered", "Cancelled"] {
        assert!(statuses.iter().any(|s| s == status));
    }
    let po_statuses: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT status FROM documents WHERE number LIKE 'YEAR-PO-%'")
            .fetch_all(&db)
            .await
            .unwrap();
    for status in ["Open", "Partial", "Complete", "Warning"] {
        assert!(po_statuses.iter().any(|s| s == status));
    }
    let payments: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE reference LIKE 'YEAR-%'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(payments, r.payments);
    let lines: i64 = sqlx
        ::query_scalar(
            "SELECT COUNT(*) FROM document_lines l JOIN documents d ON d.id=l.document_id WHERE d.number LIKE 'YEAR-%'"
        )
        .fetch_one(&db).await
        .unwrap();
    assert_eq!(lines, r.document_lines);
    let movements: i64 = sqlx
        ::query_scalar(
            "SELECT COUNT(*) FROM stock_movements m JOIN products p ON p.id=m.product_id WHERE p.name LIKE '[Year Demo] %'"
        )
        .fetch_one(&db).await
        .unwrap();
    assert_eq!(movements, r.stock_movements);
    let sentinel_row: (i64, i64, String) =
        sqlx::query_as("SELECT quantity,remaining_quantity,description FROM products WHERE id=?")
            .bind(sentinel)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(sentinel_row, (17, 7, "Keep me".into()));
    sqlx::query(
        "UPDATE products SET description='User edited fixture' WHERE name='[Year Demo] Mechanical Keyboard'"
    )
        .execute(&db).await
        .unwrap();
    let again = seed_year(&db, end + Duration::days(1)).await.unwrap();
    assert!(!again.inserted);
    assert_eq!(again.report, r);
    let description: String = sqlx::query_scalar(
        "SELECT description FROM products WHERE name='[Year Demo] Mechanical Keyboard'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(description, "User edited fixture");
    // A conflicting namespace must roll back even the newly inserted completion marker.
    sqlx::query("DELETE FROM seed_runs WHERE name=?")
        .bind(RUN)
        .execute(&db)
        .await
        .unwrap();
    assert!(seed_year(&db, end).await.is_err());
    let marker: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM seed_runs WHERE name=?")
        .bind(RUN)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(marker, 0);
    db.close().await;
    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .unwrap();
}
