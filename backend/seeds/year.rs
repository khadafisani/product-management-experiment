//! Additive, atomic year-long demo history. See the seed_year executable.
use crate::feature::{
    document::{ entity::LineInput, validation::total },
    payment::validation::allocate,
    product::{ entity::Product, service::adjust_stock },
    purchase_invoice::service::release_paid_stock,
    purchase_order::service::{ realize_line, refresh_status },
};
use chrono::{ Datelike, Duration, Months, NaiveDate };
use serde::{ Deserialize, Serialize };
use sqlx::{ MySql, MySqlPool, Transaction };
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const RUN: &str = "demo-year-v1";
const NAMES: [&str; 40] = [
    "Mechanical Keyboard",
    "Wireless Mouse",
    "USB-C Hub",
    "Laptop Stand",
    "27-inch Monitor",
    "Notebook A5",
    "Desk Lamp",
    "Webcam HD",
    "Bluetooth Speaker",
    "USB Headset",
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
    "Office Chair",
    "Standing Desk",
    "Laser Printer",
    "Document Scanner",
    "Wi-Fi Router",
    "Network Switch",
    "HDMI Cable",
    "DisplayPort Cable",
    "USB Microphone",
    "Conference Camera",
    "Laptop Sleeve",
    "Backpack",
    "Surge Protector",
    "Cable Tray",
    "Whiteboard",
    "Desk Fan",
    "Barcode Scanner",
    "Receipt Printer",
    "Label Maker",
    "Projector",
];
const PRICES: [i64; 40] = [
    450000, 125000, 275000, 185000, 2850000, 25000, 175000, 325000, 250000, 375000, 55000, 285000,
    95000, 425000, 850000, 145000, 35000, 45000, 195000, 650000, 1750000, 3250000, 2150000, 1550000,
    450000, 625000, 95000, 125000, 725000, 1850000, 195000, 425000, 185000, 275000, 650000, 225000,
    575000, 1250000, 825000, 4750000,
];
const CUSTOMERS: [&str; 12] = [
    "Toko Maju Jaya",
    "Studio Nusantara",
    "Kantor Sentosa",
    "Kopi Kreatif",
    "PT Sinar Digital",
    "Ruang Kerja Bandung",
    "CV Mitra Surabaya",
    "Kampus Merdeka",
    "Hotel Cendana",
    "Bali Creative Hub",
    "PT Solusi Medan",
    "Makassar Office Supply",
];
const SUPPLIERS: [&str; 5] = [
    "PT Sumber Perkasa",
    "CV Teknologi Nusantara",
    "Jakarta Office Wholesale",
    "PT Digital Distribusi",
    "Surabaya Stationery Supply",
];
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct YearReport {
    pub start_date: String,
    pub end_date: String,
    pub days: i64,
    pub products: i64,
    pub purchase_orders: i64,
    pub purchase_invoices: i64,
    pub sales_invoices: i64,
    pub document_lines: i64,
    pub payments: i64,
    pub payment_allocations: i64,
    pub delivery_orders: i64,
    pub delivery_details: i64,
    pub stock_movements: i64,
    pub invoice_void_requests: i64,
    pub delivery_cancellation_requests: i64,
}
#[derive(Debug)]
pub struct YearOutcome {
    pub inserted: bool,
    pub report: YearReport,
}
#[derive(Clone)]
struct Item {
    id: i64,
    name: String,
    price: i64,
}
impl Item {
    fn entity(&self) -> Product {
        Product {
            id: self.id,
            name: self.name.clone(),
            price: self.price,
            quantity: 0,
            remaining_quantity: 0,
            status: "Active".into(),
            photo: None,
            description: String::new(),
            reorder_level: 15,
        }
    }
}
struct Line {
    id: i64,
    item: Item,
    quantity: i64,
}
struct Invoice {
    id: i64,
    kind: String,
    total: i64,
    remaining: i64,
    lines: Vec<Line>,
}
struct Rng(u64);
impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 32) as usize) % n
    }
}
fn stamp(date: NaiveDate, hour: u32) -> String {
    format!("{date} {hour:02}:00:00")
}
async fn movement(
    tx: &mut Transaction<'_, MySql>,
    item: i64,
    physical: i64,
    available: i64,
    document: Option<i64>,
    date: NaiveDate,
    hour: u32,
    reason: &str,
    r: &mut YearReport
) -> Result<()> {
    adjust_stock(tx, item, physical, available, document, reason).await.map_err(|e| e.1)?;
    if physical != 0 || available != 0 {
        sqlx
            ::query("UPDATE stock_movements SET created_at=? WHERE id=LAST_INSERT_ID()")
            .bind(stamp(date, hour))
            .execute(&mut **tx).await?;
        r.stock_movements += 1;
    }
    Ok(())
}
async fn create(
    tx: &mut Transaction<'_, MySql>,
    kind: &str,
    date: NaiveDate,
    person: &str,
    po: Option<i64>,
    lines: &[(Item, i64, i64)],
    r: &mut YearReport
) -> Result<Invoice> {
    let (prefix, serial) = match kind {
        "purchase_order" => {
            r.purchase_orders += 1;
            ("PO", r.purchase_orders)
        }
        "purchase_invoice" => {
            r.purchase_invoices += 1;
            ("PI", r.purchase_invoices)
        }
        _ => {
            r.sales_invoices += 1;
            ("SI", r.sales_invoices)
        }
    };
    let inputs: Vec<LineInput> = lines
        .iter()
        .map(|(p, q, price)| LineInput {
            product_id: p.id,
            item_name: None,
            quantity: *q,
            price: *price,
        })
        .collect();
    let sum = total(&inputs).map_err(|e| e.1)?;
    let is_po = kind == "purchase_order";
    let id = sqlx
        ::query(
            "INSERT INTO documents(kind,number,date,person,purchase_order_id,grand_total,remaining_payment,status,created_at) VALUES(?,?,?,?,?,?,?,?,?)"
        )
        .bind(kind)
        .bind(format!("YEAR-{prefix}-{serial:06}"))
        .bind(date.to_string())
        .bind(person)
        .bind(po)
        .bind(sum)
        .bind(if is_po { 0 } else { sum })
        .bind(if is_po { "Open" } else { "Unpaid" })
        .bind(stamp(date, if kind == "sales_invoice" { 14 } else { 9 }))
        .execute(&mut **tx).await?
        .last_insert_id() as i64;
    let mut stored = Vec::new();
    for (p, qty, price) in lines {
        let line = sqlx
            ::query(
                "INSERT INTO document_lines(document_id,product_id,item_name,quantity,realized_quantity,price,subtotal,status) VALUES(?,?,?,?,?,?,?,?)"
            )
            .bind(id)
            .bind(p.id)
            .bind(&p.name)
            .bind(qty)
            .bind(if is_po { *qty } else { 0 })
            .bind(price)
            .bind(qty * price)
            .bind(if is_po { "Open" } else { "Posted" })
            .execute(&mut **tx).await?
            .last_insert_id() as i64;
        r.document_lines += 1;
        if kind == "sales_invoice" {
            movement(tx, p.id, -qty, -qty, Some(id), date, 14, "sales_invoice", r).await?;
        } else if kind == "purchase_invoice" {
            movement(tx, p.id, *qty, 0, Some(id), date, 9, "purchase_invoice", r).await?;
        }
        if let Some(order) = po {
            realize_line(
                tx,
                order,
                &p.entity(),
                &(LineInput {
                    product_id: p.id,
                    item_name: None,
                    quantity: *qty,
                    price: *price,
                })
            ).await.map_err(|e| e.1)?;
        }
        stored.push(Line {
            id: line,
            item: p.clone(),
            quantity: *qty,
        });
    }
    if let Some(order) = po {
        refresh_status(tx, order).await.map_err(|e| e.1)?;
    }
    Ok(Invoice {
        id,
        kind: kind.into(),
        total: sum,
        remaining: if is_po {
            0
        } else {
            sum
        },
        lines: stored,
    })
}
async fn pay(
    tx: &mut Transaction<'_, MySql>,
    invoices: &mut [&mut Invoice],
    amount: i64,
    date: NaiveDate,
    r: &mut YearReport
) -> Result<()> {
    let kind = invoices.first().ok_or("No invoices for payment")?.kind.clone();
    if invoices.iter().any(|i| i.kind != kind) {
        return Err("Payment kinds differ".into());
    }
    let allocated = allocate(
        amount,
        &invoices
            .iter()
            .map(|i| i.remaining)
            .collect::<Vec<_>>()
    ).map_err(|e| e.1)?;
    r.payments += 1;
    let prefix = if kind == "sales_invoice" { "RCP" } else { "PAY" };
    let id = sqlx
        ::query("INSERT INTO payments(reference,date,amount,kind,created_at) VALUES(?,?,?,?,?)")
        .bind(format!("YEAR-{prefix}-{:06}", r.payments))
        .bind(date.to_string())
        .bind(amount)
        .bind(&kind)
        .bind(stamp(date, if kind == "sales_invoice" { 15 } else { 11 }))
        .execute(&mut **tx).await?
        .last_insert_id() as i64;
    for (invoice, n) in invoices.iter_mut().zip(allocated) {
        invoice.remaining -= n;
        sqlx
            ::query("INSERT INTO payment_allocations(payment_id,invoice_id,amount) VALUES(?,?,?)")
            .bind(id)
            .bind(invoice.id)
            .bind(n)
            .execute(&mut **tx).await?;
        r.payment_allocations += 1;
        sqlx
            ::query("UPDATE documents SET remaining_payment=?,status=? WHERE id=?")
            .bind(invoice.remaining)
            .bind(if invoice.remaining == 0 { "Paid" } else { "Partial Paid" })
            .bind(invoice.id)
            .execute(&mut **tx).await?;
        if kind == "purchase_invoice" && invoice.remaining == 0 {
            release_paid_stock(tx, invoice.id).await.map_err(|e| e.1)?;
            r.stock_movements += invoice.lines.len() as i64;
            sqlx
                ::query(
                    "UPDATE stock_movements SET created_at=? WHERE document_id=? AND reason='Paid purchase release'"
                )
                .bind(stamp(date, 11))
                .bind(invoice.id)
                .execute(&mut **tx).await?;
        }
    }
    Ok(())
}
async fn delivery(
    tx: &mut Transaction<'_, MySql>,
    invoice: &Invoice,
    status: &str,
    part: u8,
    date: NaiveDate,
    staff: i64,
    finance: i64,
    r: &mut YearReport
) -> Result<i64> {
    if invoice.remaining == invoice.total {
        return Err("Unpaid invoice cannot have deliveries".into());
    }
    let id = sqlx
        ::query(
            "INSERT INTO delivery_orders(sales_invoice_id,driver_name,date,address,note,cancellation_note,status,created_by,approved_by) VALUES(?,?,?,?,?,?,?,?,?)"
        )
        .bind(invoice.id)
        .bind(
            ["Agus Santoso", "Budi Pratama", "Dedi Setiawan", "Rina Kartika"]
                [(r.delivery_orders as usize) % 4]
        )
        .bind(date.to_string())
        .bind(
            format!(
                "{}, Jakarta, Indonesia",
                CUSTOMERS[(r.delivery_orders as usize) % CUSTOMERS.len()]
            )
        )
        .bind("Year demo: handle equipment carefully and confirm quantities.")
        .bind(if status == "Cancelled" { "Customer changed the delivery schedule." } else { "" })
        .bind(status)
        .bind(staff)
        .bind(if status == "Draft" { None } else { Some(finance) })
        .execute(&mut **tx).await?
        .last_insert_id() as i64;
    r.delivery_orders += 1;
    for l in &invoice.lines {
        let qty = match part {
            1 => l.quantity / 2,
            2 => l.quantity - l.quantity / 2,
            _ => l.quantity,
        };
        if qty <= 0 {
            continue;
        }
        sqlx
            ::query(
                "INSERT INTO delivery_order_details(delivery_order_id,sales_invoice_detail_id,product_name,quantity,notes) VALUES(?,?,?,?,?)"
            )
            .bind(id)
            .bind(l.id)
            .bind(&l.item.name)
            .bind(qty)
            .bind(if part == 0 { "Standard shipment" } else { "Split shipment" })
            .execute(&mut **tx).await?;
        r.delivery_details += 1;
        if status == "Delivered" || status == "Approved" || status == "On Delivery" {
            let affected = sqlx
                ::query(
                    "UPDATE document_lines SET delivered_quantity=delivered_quantity+?,on_delivery_quantity=on_delivery_quantity+? WHERE id=? AND quantity-delivered_quantity-on_delivery_quantity>=?"
                )
                .bind(if status == "Delivered" { qty } else { 0 })
                .bind(if status == "Delivered" { 0 } else { qty })
                .bind(l.id)
                .bind(qty)
                .execute(&mut **tx).await?
                .rows_affected();
            if affected != 1 {
                return Err("Delivery exceeds invoice quantity".into());
            }
        }
    }
    Ok(id)
}
async fn cancel_sale(
    tx: &mut Transaction<'_, MySql>,
    invoice: &Invoice,
    date: NaiveDate,
    staff: i64,
    finance: i64,
    r: &mut YearReport
) -> Result<()> {
    for l in &invoice.lines {
        movement(
            tx,
            l.item.id,
            l.quantity,
            l.quantity,
            Some(invoice.id),
            date,
            16,
            "Voided invoice",
            r
        ).await?;
    }
    let refund = invoice.total - invoice.remaining;
    let reason = "Duplicate customer order in yearly demo";
    sqlx
        ::query(
            "INSERT INTO invoice_voids(invoice_id,reason,reviewed_by,refund_due,created_at) VALUES(?,?,?,?,?)"
        )
        .bind(invoice.id)
        .bind(reason)
        .bind(finance)
        .bind(refund)
        .bind(stamp(date, 16))
        .execute(&mut **tx).await?;
    sqlx
        ::query(
            "INSERT INTO sales_invoice_cancellations(sales_invoice_id,reason,refund_due,cancelled_at) VALUES(?,?,?,?)"
        )
        .bind(invoice.id)
        .bind(reason)
        .bind(refund)
        .bind(stamp(date, 16))
        .execute(&mut **tx).await?;
    sqlx
        ::query("UPDATE documents SET status='Cancelled',remaining_payment=0 WHERE id=?")
        .bind(invoice.id)
        .execute(&mut **tx).await?;
    sqlx
        ::query("UPDATE document_lines SET status='Cancelled' WHERE document_id=?")
        .bind(invoice.id)
        .execute(&mut **tx).await?;
    sqlx
        ::query(
            "INSERT INTO invoice_void_requests(invoice_id,requested_by,reason,status,reviewed_by) VALUES(?,?,?,'Approved',?)"
        )
        .bind(invoice.id)
        .bind(staff)
        .bind(reason)
        .bind(finance)
        .execute(&mut **tx).await?;
    r.invoice_void_requests += 1;
    Ok(())
}

pub async fn seed_year(db: &MySqlPool, end: NaiveDate) -> Result<YearOutcome> {
    let start =
        end.checked_sub_months(Months::new(12)).ok_or("Invalid end date")? + Duration::days(1);
    let mut tx = db.begin().await?;
    // The unique row serializes simultaneous invocations and commits with the dataset.
    let inserted = sqlx
        ::query(
            "INSERT IGNORE INTO seed_runs(name,start_date,end_date,summary) VALUES(?,?,?,JSON_OBJECT())"
        )
        .bind(RUN)
        .bind(start.to_string())
        .bind(end.to_string())
        .execute(&mut *tx).await?
        .rows_affected();
    if inserted == 0 {
        let stored: String = sqlx
            ::query_scalar("SELECT CAST(summary AS CHAR) FROM seed_runs WHERE name=?")
            .bind(RUN)
            .fetch_one(&mut *tx).await?;
        let report = serde_json::from_str(&stored)?;
        tx.commit().await?;
        return Ok(YearOutcome {
            inserted: false,
            report,
        });
    }
    let conflicts: i64 = sqlx
        ::query_scalar(
            "SELECT (SELECT COUNT(*) FROM products WHERE name LIKE '[Year Demo] %')+(SELECT COUNT(*) FROM documents WHERE number LIKE 'YEAR-%')+(SELECT COUNT(*) FROM payments WHERE reference LIKE 'YEAR-%')"
        )
        .fetch_one(&mut *tx).await?;
    if conflicts > 0 {
        return Err(
            "Year demo namespace already contains records without a completed seed marker; nothing changed".into()
        );
    }
    let staff: i64 = sqlx
        ::query_scalar(
            "SELECT u.id FROM users u JOIN role_permissions rp ON rp.role_id=u.role_id WHERE u.active=TRUE AND rp.permission='invoices.write' ORDER BY (u.username='staff') DESC,u.id LIMIT 1"
        )
        .fetch_optional(&mut *tx).await?
        .ok_or("Seed role accounts first; an invoice writer is required")?;
    let finance: i64 = sqlx
        ::query_scalar(
            "SELECT u.id FROM users u JOIN role_permissions rp ON rp.role_id=u.role_id WHERE u.active=TRUE AND rp.permission='delivery.approve' ORDER BY (u.username='finance') DESC,u.id LIMIT 1"
        )
        .fetch_optional(&mut *tx).await?
        .ok_or("Seed role accounts first; a delivery approver is required")?;
    let mut r = YearReport {
        start_date: start.to_string(),
        end_date: end.to_string(),
        days: (end - start).num_days() + 1,
        ..Default::default()
    };
    let mut items = Vec::new();
    for (i, name) in NAMES.iter().enumerate() {
        let name = format!("[Year Demo] {name}");
        let id = sqlx
            ::query(
                "INSERT INTO products(name,quantity,remaining_quantity,status,description,price,reorder_level,created_at) VALUES(?,0,0,'Active',?,?,?,?)"
            )
            .bind(&name)
            .bind("Year-long demo history with linked purchasing, payments and deliveries.")
            .bind(PRICES[i])
            .bind(10 + ((i as i64) % 4) * 5)
            .bind(stamp(start, 8))
            .execute(&mut *tx).await?
            .last_insert_id() as i64;
        r.products += 1;
        movement(&mut tx, id, 40, 40, None, start, 8, "Year demo opening stock", &mut r).await?;
        items.push(Item {
            id,
            name,
            price: PRICES[i],
        });
    }
    let mut rng = Rng(0x5eed2026);
    let mut previous_month = 0;
    let mut cycle = 0usize;
    let mut paired: Option<Invoice> = None;
    for day in 0..r.days {
        let date = start + Duration::days(day);
        if date.month() != previous_month {
            previous_month = date.month();
            println!("Generating yearly demo: {}", date.format("%Y-%m"));
            for basket in 0..8 {
                let mode = (basket + cycle) % 5;
                let mut receipt = Vec::new();
                for i in basket * 5..basket * 5 + 5 {
                    let available: i64 = sqlx
                        ::query_scalar("SELECT remaining_quantity FROM products WHERE id=?")
                        .bind(items[i].id)
                        .fetch_one(&mut *tx).await?;
                    let qty = (65 + (((i * 7 + cycle * 13) % 50) as i64) - available).clamp(12, 95);
                    receipt.push((
                        items[i].clone(),
                        qty,
                        (items[i].price * (65 + ((cycle % 12) as i64))) / 100,
                    ));
                }
                let ordered: Vec<_> = receipt
                    .iter()
                    .map(|(p, q, cost)| {
                        (
                            p.clone(),
                            if mode == 1 { q + 8 } else if mode == 2 { q - 3 } else { *q },
                            *cost,
                        )
                    })
                    .collect();
                let po = create(
                    &mut tx,
                    "purchase_order",
                    date,
                    SUPPLIERS[basket % 5],
                    None,
                    &ordered,
                    &mut r
                ).await?;
                if mode == 3 {
                    continue;
                }
                if mode <= 1 {
                    let first_lines: Vec<_> = receipt
                        .iter()
                        .map(|(p, q, c)| (p.clone(), q / 2, *c))
                        .collect();
                    let second_lines: Vec<_> = receipt
                        .iter()
                        .map(|(p, q, c)| (p.clone(), q - q / 2, *c))
                        .collect();
                    let mut first = create(
                        &mut tx,
                        "purchase_invoice",
                        date,
                        SUPPLIERS[basket % 5],
                        Some(po.id),
                        &first_lines,
                        &mut r
                    ).await?;
                    let mut second = create(
                        &mut tx,
                        "purchase_invoice",
                        date,
                        SUPPLIERS[basket % 5],
                        Some(po.id),
                        &second_lines,
                        &mut r
                    ).await?;
                    if mode == 0 {
                        let amount = first.total + second.total / 2;
                        pay(&mut tx, &mut [&mut first, &mut second], amount, date, &mut r).await?;
                        let left = second.remaining;
                        pay(&mut tx, &mut [&mut second], left, date, &mut r).await?;
                    } else {
                        let amount = first.total;
                        pay(&mut tx, &mut [&mut first], amount, date, &mut r).await?;
                        let amount = second.total / 2;
                        pay(&mut tx, &mut [&mut second], amount, date, &mut r).await?;
                    }
                } else {
                    let mut inv = create(
                        &mut tx,
                        "purchase_invoice",
                        date,
                        SUPPLIERS[basket % 5],
                        if mode == 4 {
                            None
                        } else {
                            Some(po.id)
                        },
                        &receipt,
                        &mut r
                    ).await?;
                    if mode == 2 {
                        let amount = inv.total / 3;
                        pay(&mut tx, &mut [&mut inv], amount, date, &mut r).await?;
                        let amount = inv.remaining;
                        pay(&mut tx, &mut [&mut inv], amount, date, &mut r).await?;
                    } else if (cycle + basket) % 2 == 0 {
                        let amount = inv.total;
                        pay(&mut tx, &mut [&mut inv], amount, date, &mut r).await?;
                    }
                }
            }
            cycle += 1;
        }
        for _ in 0..2 + ((day as usize) % 5) {
            let mode = (r.sales_invoices + 1) % 10;
            let pool = if day < r.days - 90 { 38 } else { 32 };
            let count = 2 + rng.next(3);
            let mut chosen = std::collections::BTreeSet::new();
            while chosen.len() < count {
                chosen.insert(rng.next(pool));
            }
            let mut lines = Vec::new();
            for i in chosen {
                let qty = if mode == 3 {
                    2 + (rng.next(4) as i64)
                } else {
                    1 + (rng.next(if date.month() >= 10 { 6 } else { 4 }) as i64)
                };
                let available: i64 = sqlx
                    ::query_scalar("SELECT remaining_quantity FROM products WHERE id=?")
                    .bind(items[i].id)
                    .fetch_one(&mut *tx).await?;
                if available < qty {
                    let supply = vec![(
                        items[i].clone(),
                        45 + (day % 20),
                        (items[i].price * 7) / 10,
                    )];
                    let mut inv = create(
                        &mut tx,
                        "purchase_invoice",
                        date,
                        SUPPLIERS[i % 5],
                        None,
                        &supply,
                        &mut r
                    ).await?;
                    let amount = inv.total;
                    pay(&mut tx, &mut [&mut inv], amount, date, &mut r).await?;
                }
                lines.push((items[i].clone(), qty, items[i].price));
            }
            let mut sale = create(
                &mut tx,
                "sales_invoice",
                date,
                CUSTOMERS[rng.next(CUSTOMERS.len())],
                None,
                &lines,
                &mut r
            ).await?;
            match mode {
                0 => {
                    if day >= r.days - 14 {
                        sqlx
                            ::query(
                                "INSERT INTO invoice_void_requests(invoice_id,requested_by,reason) VALUES(?,?,'Customer requested review of this unpaid order')"
                            )
                            .bind(sale.id)
                            .bind(staff)
                            .execute(&mut *tx).await?;
                        r.invoice_void_requests += 1;
                    }
                }
                1 => {
                    let amount = sale.total / 2;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    delivery(
                        &mut tx,
                        &sale,
                        if day < r.days - 21 {
                            "Delivered"
                        } else {
                            "Draft"
                        },
                        0,
                        date,
                        staff,
                        finance,
                        &mut r
                    ).await?;
                }
                2 => {
                    let amount = sale.total;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    delivery(&mut tx, &sale, "Delivered", 0, date, staff, finance, &mut r).await?;
                }
                3 => {
                    let amount = sale.total / 3;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    let amount = sale.remaining;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    delivery(&mut tx, &sale, "Delivered", 1, date, staff, finance, &mut r).await?;
                    delivery(&mut tx, &sale, "Delivered", 2, date, staff, finance, &mut r).await?;
                }
                4 => {
                    paired = Some(sale);
                    continue;
                }
                5 => {
                    if let Some(mut first) = paired.take() {
                        let amount = first.remaining + sale.total / 2;
                        pay(&mut tx, &mut [&mut first, &mut sale], amount, date, &mut r).await?;
                        delivery(
                            &mut tx,
                            &first,
                            "Delivered",
                            0,
                            date,
                            staff,
                            finance,
                            &mut r
                        ).await?;
                    } else {
                        let amount = sale.total / 2;
                        pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    }
                    delivery(
                        &mut tx,
                        &sale,
                        if day < r.days - 21 {
                            "Delivered"
                        } else {
                            "On Delivery"
                        },
                        0,
                        date,
                        staff,
                        finance,
                        &mut r
                    ).await?;
                }
                6 => {
                    let amount = sale.total / 3;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    let order = delivery(
                        &mut tx,
                        &sale,
                        if day < r.days - 21 {
                            "Delivered"
                        } else {
                            "Approved"
                        },
                        0,
                        date,
                        staff,
                        finance,
                        &mut r
                    ).await?;
                    if day >= r.days - 14 {
                        sqlx
                            ::query(
                                "INSERT INTO delivery_cancellation_requests(delivery_order_id,requested_by,reason) VALUES(?,?,'Customer requested a new delivery schedule')"
                            )
                            .bind(order)
                            .bind(staff)
                            .execute(&mut *tx).await?;
                        r.delivery_cancellation_requests += 1;
                    }
                }
                7 => {
                    let amount = sale.total;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    let order = delivery(
                        &mut tx,
                        &sale,
                        "Cancelled",
                        0,
                        date,
                        staff,
                        finance,
                        &mut r
                    ).await?;
                    sqlx
                        ::query(
                            "INSERT INTO delivery_cancellation_requests(delivery_order_id,requested_by,reason,status,reviewed_by) VALUES(?,?,'Customer changed the delivery schedule','Approved',?)"
                        )
                        .bind(order)
                        .bind(staff)
                        .bind(finance)
                        .execute(&mut *tx).await?;
                    r.delivery_cancellation_requests += 1;
                    delivery(&mut tx, &sale, "Delivered", 0, date, staff, finance, &mut r).await?;
                }
                8 => {
                    let amount = sale.total;
                    pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    delivery(
                        &mut tx,
                        &sale,
                        if day < r.days - 21 {
                            "Delivered"
                        } else {
                            "On Delivery"
                        },
                        0,
                        date,
                        staff,
                        finance,
                        &mut r
                    ).await?;
                }
                _ => {
                    if r.sales_invoices % 20 == 9 {
                        let amount = sale.total / 4;
                        pay(&mut tx, &mut [&mut sale], amount, date, &mut r).await?;
                    }
                    cancel_sale(&mut tx, &sale, date, staff, finance, &mut r).await?;
                }
            }
        }
    }
    if let Some(mut inv) = paired {
        let amount = inv.remaining;
        pay(&mut tx, &mut [&mut inv], amount, end, &mut r).await?;
        delivery(&mut tx, &inv, "Draft", 0, end, staff, finance, &mut r).await?;
    }
    for item in items.iter().skip(36) {
        sqlx
            ::query("UPDATE products SET status='Inactive' WHERE id=?")
            .bind(item.id)
            .execute(&mut *tx).await?;
    }
    verify(&mut tx).await?;
    sqlx
        ::query("UPDATE seed_runs SET summary=?,completed_at=CURRENT_TIMESTAMP WHERE name=?")
        .bind(serde_json::to_string(&r)?)
        .bind(RUN)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(YearOutcome {
        inserted: true,
        report: r,
    })
}

async fn verify(tx: &mut Transaction<'_, MySql>) -> Result<()> {
    let checks = [
        (
            "stock ledger",
            "SELECT COUNT(*) FROM products p LEFT JOIN (SELECT product_id,SUM(quantity) physical,SUM(remaining_delta) available FROM stock_movements GROUP BY product_id) m ON m.product_id=p.id WHERE p.name LIKE '[Year Demo] %' AND (p.quantity<>COALESCE(m.physical,0) OR p.remaining_quantity<>COALESCE(m.available,0) OR p.remaining_quantity<0 OR p.remaining_quantity>p.quantity)",
        ),
        (
            "invoice totals",
            "SELECT COUNT(*) FROM documents d LEFT JOIN (SELECT document_id,SUM(subtotal) total FROM document_lines GROUP BY document_id) l ON l.document_id=d.id WHERE d.number LIKE 'YEAR-%' AND d.grand_total<>COALESCE(l.total,0)",
        ),
        (
            "invoice balances",
            "SELECT COUNT(*) FROM documents d LEFT JOIN (SELECT invoice_id,SUM(amount) paid FROM payment_allocations GROUP BY invoice_id) p ON p.invoice_id=d.id WHERE d.number LIKE 'YEAR-%' AND d.kind<>'purchase_order' AND d.status<>'Cancelled' AND (d.remaining_payment<>d.grand_total-COALESCE(p.paid,0) OR d.remaining_payment<0 OR d.status<>CASE WHEN d.remaining_payment=0 THEN 'Paid' WHEN d.remaining_payment=d.grand_total THEN 'Unpaid' ELSE 'Partial Paid' END)",
        ),
        (
            "payment allocations",
            "SELECT COUNT(*) FROM payments p LEFT JOIN (SELECT payment_id,SUM(amount) amount FROM payment_allocations GROUP BY payment_id) a ON a.payment_id=p.id WHERE p.reference LIKE 'YEAR-%' AND p.amount<>COALESCE(a.amount,0)",
        ),
        (
            "PO balance",
            "SELECT COUNT(*) FROM document_lines l JOIN documents po ON po.id=l.document_id LEFT JOIN (SELECT d.purchase_order_id,l.product_id,SUM(l.quantity) received FROM documents d JOIN document_lines l ON l.document_id=d.id WHERE d.kind='purchase_invoice' AND d.status<>'Cancelled' GROUP BY d.purchase_order_id,l.product_id) r ON r.purchase_order_id=po.id AND r.product_id=l.product_id WHERE po.number LIKE 'YEAR-PO-%' AND l.realized_quantity<>l.quantity-COALESCE(r.received,0)",
        ),
        (
            "delivery quantities",
            "SELECT COUNT(*) FROM document_lines l JOIN documents d ON d.id=l.document_id LEFT JOIN (SELECT dd.sales_invoice_detail_id,SUM(CASE WHEN o.status='Delivered' THEN dd.quantity ELSE 0 END) delivered,SUM(CASE WHEN o.status IN ('Approved','On Delivery') THEN dd.quantity ELSE 0 END) reserved FROM delivery_order_details dd JOIN delivery_orders o ON o.id=dd.delivery_order_id GROUP BY dd.sales_invoice_detail_id) q ON q.sales_invoice_detail_id=l.id WHERE d.number LIKE 'YEAR-SI-%' AND (l.delivered_quantity<>COALESCE(q.delivered,0) OR l.on_delivery_quantity<>COALESCE(q.reserved,0) OR l.quantity<l.delivered_quantity+l.on_delivery_quantity)",
        ),
        (
            "unpaid deliveries",
            "SELECT COUNT(*) FROM delivery_orders o JOIN documents d ON d.id=o.sales_invoice_id WHERE d.number LIKE 'YEAR-%' AND d.status NOT IN ('Paid','Partial Paid')",
        ),
        (
            "paid purchase release",
            "SELECT COUNT(*) FROM documents d WHERE d.number LIKE 'YEAR-PI-%' AND d.stock_released<>(d.status='Paid')",
        ),
    ];
    for (name, sql) in checks {
        let bad: i64 = sqlx::query_scalar(sql).fetch_one(&mut **tx).await?;
        if bad > 0 {
            return Err(
                format!("Year seed {name}: {bad} violations; entire dataset rolled back").into()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../src/tests/year_seed.rs"]
mod tests;
