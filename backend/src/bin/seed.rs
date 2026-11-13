//! Additive demo dataset. Run from backend: cargo run --bin seed
use chrono::{ Duration, FixedOffset, NaiveDate, Utc };
use sqlx::{ MySql, MySqlPool, Row, Transaction, mysql::MySqlPoolOptions };
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const NAMES: [&str; 20] = [
    "Mechanical Keyboard",
    "Wireless Mouse",
    "USB-C Hub",
    "Laptop Stand",
    "Monitor 24-inch",
    "Notebook A5",
    "Desk Lamp",
    "Webcam HD",
    "Bluetooth Speaker",
    "Headset",
    "USB-C Cable",
    "Power Bank",
    "Desk Mat",
    "Monitor Arm",
    "Portable SSD",
    "Ethernet Adapter",
    "Cable Organizer",
    "Screen Cleaner",
    "Travel Charger",
    "Drawing Tablet",
];
const PRICES: [i64; 20] = [
    450_000, 125_000, 275_000, 185_000, 1_650_000, 25_000, 175_000, 325_000, 250_000, 375_000,
    55_000, 285_000, 95_000, 425_000, 850_000, 145_000, 35_000, 45_000, 195_000, 650_000,
];
const STOCK: [i64; 20] = [
    0, 2, 4, 8, 12, 20, 30, 45, 60, 80, 100, 120, 25, 18, 12, 35, 40, 50, 0, 0,
];

struct Product {
    id: i64,
    name: String,
    price: i64,
}
struct Invoice {
    id: i64,
    total: i64,
    remaining: i64,
    date: NaiveDate,
}

async fn document(
    tx: &mut Transaction<'_, MySql>,
    kind: &str,
    number: &str,
    date: NaiveDate,
    person: &str,
    po: Option<i64>,
    product: &Product,
    quantity: i64,
    price: i64
) -> Result<Invoice> {
    let total = quantity * price;
    let is_po = kind == "purchase_order";
    let id = sqlx
        ::query(
            "INSERT INTO documents(kind,number,date,person,purchase_order_id,grand_total,remaining_payment,status,created_at) VALUES(?,?,?,?,?,?,?,?,?)"
        )
        .bind(kind)
        .bind(number)
        .bind(date.to_string())
        .bind(person)
        .bind(po)
        .bind(total)
        .bind(if is_po { 0 } else { total })
        .bind(if is_po { "Open" } else { "Unpaid" })
        .bind(format!("{date} 09:00:00"))
        .execute(&mut **tx).await?
        .last_insert_id() as i64;
    sqlx
        ::query(
            "INSERT INTO document_lines(document_id,product_id,item_name,quantity,realized_quantity,price,subtotal,status) VALUES(?,?,?,?,?,?,?,?)"
        )
        .bind(id)
        .bind(product.id)
        .bind(&product.name)
        .bind(quantity)
        .bind(if is_po { quantity } else { 0 })
        .bind(price)
        .bind(total)
        .bind(if is_po { "Open" } else { "Posted" })
        .execute(&mut **tx).await?;
    if let Some(order) = po {
        sqlx
            ::query(
                "UPDATE document_lines SET realized_quantity=realized_quantity-? WHERE document_id=? AND product_id=?"
            )
            .bind(quantity)
            .bind(order)
            .bind(product.id)
            .execute(&mut **tx).await?;
        sqlx
            ::query(
                "UPDATE document_lines SET status=CASE WHEN realized_quantity<0 THEN 'Warning' WHEN realized_quantity=0 THEN 'Complete' ELSE 'Partial' END WHERE document_id=?"
            )
            .bind(order)
            .execute(&mut **tx).await?;
        sqlx
            ::query(
                "UPDATE documents SET status=(SELECT status FROM document_lines WHERE document_id=? LIMIT 1) WHERE id=?"
            )
            .bind(order)
            .bind(order)
            .execute(&mut **tx).await?;
    }
    if !is_po {
        let delta = if kind == "sales_invoice" { -quantity } else { quantity };
        sqlx
            ::query("UPDATE products SET quantity=quantity+? WHERE id=?")
            .bind(delta)
            .bind(product.id)
            .execute(&mut **tx).await?;
        sqlx
            ::query(
                "INSERT INTO stock_movements(product_id,document_id,quantity,reason,created_at) VALUES(?,?,?,?,?)"
            )
            .bind(product.id)
            .bind(id)
            .bind(delta)
            .bind(kind)
            .bind(format!("{date} 09:00:00"))
            .execute(&mut **tx).await?;
    }
    Ok(Invoice {
        id,
        total,
        remaining: if is_po {
            0
        } else {
            total
        },
        date,
    })
}

async fn payment(
    tx: &mut Transaction<'_, MySql>,
    reference: &str,
    kind: &str,
    invoices: &mut [Invoice],
    allocations: &[(usize, i64)]
) -> Result<()> {
    let amount: i64 = allocations
        .iter()
        .map(|(_, n)| n)
        .sum();
    let date = allocations
        .iter()
        .map(|(i, _)| invoices[*i].date)
        .max()
        .ok_or("Missing allocation")?;
    if allocations.len() > 1 && amount <= invoices[allocations[0].0].remaining {
        return Err("Multi-invoice demo payment does not exceed first balance".into());
    }
    // Earlier invoices must be fully settled before a subsequent allocation.
    for (position, (i, n)) in allocations.iter().enumerate() {
        if
            *n <= 0 ||
            *n > invoices[*i].remaining ||
            (position + 1 < allocations.len() && *n != invoices[*i].remaining)
        {
            return Err("Invalid demo allocation".into());
        }
    }
    let id = sqlx
        ::query("INSERT INTO payments(reference,date,amount,kind,created_at) VALUES(?,?,?,?,?)")
        .bind(reference)
        .bind(date.to_string())
        .bind(amount)
        .bind(kind)
        .bind(format!("{date} 15:00:00"))
        .execute(&mut **tx).await?
        .last_insert_id() as i64;
    for (index, amount) in allocations {
        let invoice = &mut invoices[*index];
        invoice.remaining -= amount;
        sqlx
            ::query("INSERT INTO payment_allocations(payment_id,invoice_id,amount) VALUES(?,?,?)")
            .bind(id)
            .bind(invoice.id)
            .bind(amount)
            .execute(&mut **tx).await?;
        sqlx
            ::query("UPDATE documents SET remaining_payment=?,status=? WHERE id=?")
            .bind(invoice.remaining)
            .bind(if invoice.remaining == 0 { "Paid" } else { "Partial Paid" })
            .bind(invoice.id)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

async fn seed(db: &MySqlPool) -> Result<bool> {
    let mut tx = db.begin().await?;
    // Serialize simultaneous runs without altering or deleting existing records.
    let locked: Option<i64> = sqlx
        ::query_scalar("SELECT GET_LOCK('stockroom_demo_seed_v1',10)")
        .fetch_one(&mut *tx).await?;
    if locked != Some(1) {
        return Err("Another seeder is running; no data was changed".into());
    }
    let existing: (i64,) = sqlx
        ::query_as("SELECT COUNT(*) FROM products WHERE name LIKE '[Demo] %'")
        .fetch_one(&mut *tx).await?;
    if existing.0 != 0 {
        let docs: (i64,) = sqlx
            ::query_as("SELECT COUNT(*) FROM documents WHERE number LIKE 'DEMO-%'")
            .fetch_one(&mut *tx).await?;
        let pays: (i64,) = sqlx
            ::query_as("SELECT COUNT(*) FROM payments WHERE reference LIKE 'DEMO-%'")
            .fetch_one(&mut *tx).await?;
        if existing.0 != 20 || docs.0 != 40 || pays.0 != 20 {
            return Err(
                "Demo namespace already exists but is incomplete; no data was changed".into()
            );
        }
        tx.commit().await?;
        sqlx::query("SELECT RELEASE_LOCK('stockroom_demo_seed_v1')").execute(db).await?;
        return Ok(false);
    }
    let today = Utc::now()
        .with_timezone(&FixedOffset::east_opt(7 * 3600).unwrap())
        .date_naive();
    let mut products = Vec::new();
    for i in 0..20 {
        let sales = if i < 12 { 8 + ((i as i64) % 4) + 10 + ((i as i64) % 5) } else { 0 };
        let received = if i < 8 { 12 + ((i as i64) % 4) * 4 } else { 0 };
        let opening = STOCK[i] + sales - received;
        let name = format!("[Demo] {}", NAMES[i]);
        let status = if i == 19 { "Inactive" } else { "Active" };
        let description = if i >= 12 {
            "Demo product with no recent sales; useful for checking no-demand forecasts."
        } else {
            "Demo product with recent sales and reconciled stock history."
        };
        let id = sqlx
            ::query(
                "INSERT INTO products(name,quantity,status,photo,description,price,reorder_level,created_at) VALUES(?,?,?,NULL,?,?,?,?)"
            )
            .bind(&name)
            .bind(opening)
            .bind(status)
            .bind(description)
            .bind(PRICES[i])
            .bind(if i < 4 { 10 } else { 5 })
            .bind(format!("{} 08:00:00", today - Duration::days(180)))
            .execute(&mut *tx).await?
            .last_insert_id() as i64;
        if opening > 0 {
            sqlx
                ::query(
                    "INSERT INTO stock_movements(product_id,quantity,reason,created_at) VALUES(?,?,'Opening stock',?)"
                )
                .bind(id)
                .bind(opening)
                .bind(format!("{} 08:00:00", today - Duration::days(180)))
                .execute(&mut *tx).await?;
        }
        products.push(Product {
            id,
            name,
            price: PRICES[i],
        });
    }
    let mut purchases = Vec::new();
    for (i, p) in products.iter().take(8).enumerate() {
        let qty = 12 + ((i as i64) % 4) * 4;
        let planned = match i % 4 {
            0 => qty - 2,
            1 => qty + 4,
            2 => qty,
            _ => qty + 6,
        };
        let date = today - Duration::days(150 - (i as i64) * 16);
        let cost = (p.price * 4) / 5;
        let po = document(
            &mut tx,
            "purchase_order",
            &format!("DEMO-PO-{:03}", i + 1),
            date - Duration::days(4),
            "Dewi / Purchasing",
            None,
            p,
            planned,
            cost
        ).await?;
        purchases.push(
            document(
                &mut tx,
                "purchase_invoice",
                &format!("DEMO-PI-{:03}", i + 1),
                date,
                "Rizky / Receiving",
                if i < 6 {
                    Some(po.id)
                } else {
                    None
                },
                p,
                qty,
                cost
            ).await?
        );
    }
    let customers = [
        "Toko Maju Jaya",
        "Studio Nusantara",
        "Kantor Sentosa",
        "Kopi Kreatif",
        "PT Sinar Digital",
        "Ruang Kerja Bandung",
    ];
    let mut sales = Vec::new();
    for round in 0..2 {
        for (i, p) in products.iter().take(12).enumerate() {
            let qty = if round == 0 { 8 + ((i as i64) % 4) } else { 10 + ((i as i64) % 5) };
            let days = if round == 0 { 27 - ((i as i64) % 7) } else { (i as i64) % 12 };
            sales.push(
                document(
                    &mut tx,
                    "sales_invoice",
                    &format!("DEMO-SI-{:03}", round * 12 + i + 1),
                    today - Duration::days(days),
                    customers[i % customers.len()],
                    None,
                    p,
                    qty,
                    p.price
                ).await?
            );
        }
    }
    let supplier_allocations = vec![
        vec![(0, purchases[0].total)],
        vec![(1, purchases[1].total / 2)],
        vec![(1, purchases[1].total - purchases[1].total / 2)],
        vec![(2, purchases[2].total), (3, purchases[3].total / 2)],
        vec![(3, purchases[3].total - purchases[3].total / 2), (4, purchases[4].total / 4)],
        vec![(5, purchases[5].total / 2)]
    ];
    for (i, alloc) in supplier_allocations.iter().enumerate() {
        payment(
            &mut tx,
            &format!("DEMO-PAY-{:03}", i + 1),
            "purchase_invoice",
            &mut purchases,
            alloc
        ).await?;
    }
    for i in 0..12 {
        let amount = if i % 3 == 0 { sales[i].total / 2 } else { sales[i].total };
        payment(
            &mut tx,
            &format!("DEMO-RCP-{:03}", i + 1),
            "sales_invoice",
            &mut sales,
            &[(i, amount)]
        ).await?;
    }
    for (i, first, second) in [
        (13, 0, 12),
        (14, 3, 13),
    ] {
        let allocations = [
            (first, sales[first].remaining),
            (second, sales[second].total / 2),
        ];
        payment(
            &mut tx,
            &format!("DEMO-RCP-{i:03}"),
            "sales_invoice",
            &mut sales,
            &allocations
        ).await?;
    }
    // Historical demo fixtures are carried forward as already available stock,
    // exactly like the upgrade migration. New invoices use paid-only availability.
    sqlx
        ::query("UPDATE products SET remaining_quantity=quantity WHERE name LIKE '[Demo] %'")
        .execute(&mut *tx).await?;
    sqlx
        ::query(
            "UPDATE stock_movements m JOIN products p ON p.id=m.product_id SET m.remaining_delta=m.quantity WHERE p.name LIKE '[Demo] %'"
        )
        .execute(&mut *tx).await?;
    sqlx
        ::query(
            "UPDATE documents SET stock_released=TRUE WHERE kind='purchase_invoice' AND number LIKE 'DEMO-%'"
        )
        .execute(&mut *tx).await?;
    // Check the complete insert before commit. Ledger balances must equal master stock.
    for (i, p) in products.iter().enumerate() {
        let row = sqlx
            ::query(
                "SELECT p.quantity,CAST(COALESCE(SUM(m.quantity),0) AS SIGNED) ledger FROM products p LEFT JOIN stock_movements m ON m.product_id=p.id WHERE p.id=? GROUP BY p.id"
            )
            .bind(p.id)
            .fetch_one(&mut *tx).await?;
        if row.get::<i64, _>("quantity") != STOCK[i] || row.get::<i64, _>("ledger") != STOCK[i] {
            return Err("Demo stock did not reconcile; transaction rolled back".into());
        }
    }
    tx.commit().await?;
    sqlx::query("SELECT RELEASE_LOCK('stockroom_demo_seed_v1')").execute(db).await?;
    Ok(true)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let url = std::env
        ::var("DATABASE_URL")
        .map_err(|_| "Run from backend with DATABASE_URL configured in .env")?;
    let db = MySqlPoolOptions::new()
        .max_connections(1)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET time_zone='+07:00'").execute(c).await?;
                Ok(())
            })
        })
        .connect(&url).await?;
    sqlx::migrate!().run(&db).await?;
    let changed = seed(&db).await?;
    println!("{}", if changed {
        "Demo seed committed: 20 products, 40 documents, 40 lines, 20 payments, 24 allocations, 50 stock movements."
    } else {
        "Demo seed already exists. No records changed."
    });
    db.close().await;
    Ok(())
}
