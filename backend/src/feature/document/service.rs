use super::{ entity::*, validation::* };
use crate::feature::product::{ entity::{ Product, active }, service::lookup_product };
use crate::{ config::App, helper::{ Result, bad } };
use serde_json::{ Value, json };
use sqlx::Row;
use std::collections::HashSet;
pub(crate) async fn documents(a: App) -> Result<Vec<Document>> {
    Ok(sqlx::query_as(&format!("{DOC_SELECT} ORDER BY date DESC,id DESC")).fetch_all(&a.db).await?)
}
pub(crate) async fn detail(a: App, id: i64) -> Result<Value> {
    let doc: Option<Document> = sqlx
        ::query_as(&format!("{DOC_SELECT} WHERE id=?"))
        .bind(id)
        .fetch_optional(&a.db).await?;
    let doc = doc.ok_or_else(|| bad("Document not found"))?;
    let lines: Vec<Line> = sqlx
        ::query_as(
            "SELECT id,product_id,item_name,quantity,realized_quantity,delivered_quantity,on_delivery_quantity,price,subtotal,status FROM document_lines WHERE document_id=? ORDER BY id"
        )
        .bind(id)
        .fetch_all(&a.db).await?;
    let total_quantity: i64 = lines
        .iter()
        .map(|l| l.quantity)
        .sum();
    let allocations = sqlx
        ::query(
            "SELECT p.reference,pa.amount FROM payment_allocations pa JOIN payments p ON p.id=pa.payment_id WHERE pa.invoice_id=?"
        )
        .bind(id)
        .fetch_all(&a.db).await?;
    let cancellation = sqlx
        ::query(
            "SELECT reason,refund_due,CAST(created_at AS CHAR) cancelled_at FROM invoice_voids WHERE invoice_id=? UNION ALL SELECT reason,refund_due,CAST(cancelled_at AS CHAR) cancelled_at FROM sales_invoice_cancellations WHERE sales_invoice_id=? AND NOT EXISTS (SELECT 1 FROM invoice_voids WHERE invoice_id=?)"
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .fetch_optional(&a.db).await?
        .map(
            |r|
                json!({"reason":r.get::<String,_>("reason"),"refund_due":r.get::<i64,_>("refund_due"),"cancelled_at":r.get::<String,_>("cancelled_at")})
        );
    Ok(
        json!({"cancellation":cancellation,"document":doc,"total_product":lines.len(),"total_quantity":total_quantity,"lines":lines,"payments":allocations.iter().map(|r|json!({"reference":r.get::<String,_>("reference"),"amount":r.get::<i64,_>("amount")})).collect::<Vec<_>>()})
    )
}
pub(crate) async fn preview_document(a: App, d: DocumentInput) -> Result<Value> {
    let sum = validate_document(&d)?;
    let mut tx = a.db.begin().await?;
    let mut new_products = Vec::new();
    let mut resolved = HashSet::new();
    for l in &d.lines {
        if let Some(p) = lookup_product(&mut tx, l).await? {
            if p.status != "Active" {
                return Err(bad("Inactive products cannot be used"));
            }
            if !resolved.insert(p.id) {
                return Err(bad("Each product must appear once"));
            }
            if d.kind == "sales_invoice" && p.remaining_quantity < l.quantity {
                return Err(bad(&format!("Insufficient remaining quantity for {}", p.name)));
            }
        } else {
            if d.kind == "sales_invoice" {
                return Err(bad("Sales invoices require existing products"));
            }
            new_products.push(
                json!({"name":new_product_name(l)?,"price":l.price,"quantity":if d.kind=="purchase_order" {0} else {l.quantity},"remaining_quantity":0,"status":"Active","reorder_level":10})
            );
        }
    }
    tx.rollback().await?;
    Ok(json!({"new_products":new_products,"grand_total":sum}))
}
pub(crate) async fn create_document(a: App, mut d: DocumentInput) -> Result<Value> {
    let sum = validate_document(&d)?;
    d.lines.sort_by(|a, b| {
        a.product_id.cmp(&b.product_id).then_with(|| a.item_name.cmp(&b.item_name))
    });
    let mut tx = a.db.begin().await?;
    if let Some(po) = d.purchase_order_id {
        crate::feature::purchase_order::service::validate_link(&mut tx, po).await?;
    }
    let status = if d.kind == "purchase_order" { "Open" } else { "Unpaid" };
    let remaining = if d.kind == "purchase_order" { 0 } else { sum };
    let id = sqlx
        ::query(
            "INSERT INTO documents(kind,number,date,person,purchase_order_id,grand_total,remaining_payment,status) VALUES(?,?,?,?,?,?,?,?)"
        )
        .bind(&d.kind)
        .bind(d.number.trim())
        .bind(&d.date)
        .bind(d.person.trim())
        .bind(d.purchase_order_id)
        .bind(sum)
        .bind(remaining)
        .bind(status)
        .execute(&mut *tx).await?
        .last_insert_id() as i64;
    let mut resolved = HashSet::new();
    let mut created_products = Vec::new();
    for l in &d.lines {
        let p = if let Some(p) = lookup_product(&mut tx, l).await? {
            p
        } else {
            if d.kind == "sales_invoice" {
                return Err(bad("Sales invoices require existing products"));
            }
            if !d.confirm_new_products {
                return Err(bad("Preview and confirm creation of the new products before posting"));
            }
            let name = new_product_name(l)?.to_string();
            let product_id = sqlx
                ::query(
                    "INSERT INTO products(name,quantity,status,description,price,reorder_level) VALUES(?,0,'Active','',?,10)"
                )
                .bind(&name)
                .bind(l.price)
                .execute(&mut *tx).await?
                .last_insert_id() as i64;
            created_products.push(json!({"id":product_id,"name":name}));
            Product {
                id: product_id,
                name,
                quantity: 0,
                remaining_quantity: 0,
                status: active(),
                photo: None,
                description: String::new(),
                price: l.price,
                reorder_level: 10,
            }
        };
        if !resolved.insert(p.id) {
            return Err(bad("Each product must appear once"));
        }
        if p.status != "Active" {
            return Err(bad("Inactive products cannot be used"));
        }
        if d.kind == "sales_invoice" && p.remaining_quantity < l.quantity {
            return Err(bad(&format!("Insufficient remaining quantity for {}", p.name)));
        }
        sqlx
            ::query(
                "INSERT INTO document_lines(document_id,product_id,item_name,quantity,realized_quantity,price,subtotal,status) VALUES(?,?,?,?,?,?,?,?)"
            )
            .bind(id)
            .bind(p.id)
            .bind(&p.name)
            .bind(l.quantity)
            .bind(if d.kind == "purchase_order" { l.quantity } else { 0 })
            .bind(l.price)
            .bind(l.quantity * l.price)
            .bind(if d.kind == "purchase_order" { "Open" } else { "Posted" })
            .execute(&mut *tx).await?;
        if let Some(po) = d.purchase_order_id {
            crate::feature::purchase_order::service::realize_line(&mut tx, po, &p, l).await?;
        }
        match d.kind.as_str() {
            "sales_invoice" => {
                crate::feature::sale_invoice::service::deduct_stock(
                    &mut tx,
                    p.id,
                    l.quantity,
                    id
                ).await?;
            }
            "purchase_invoice" => {
                crate::feature::purchase_invoice::service::receive_stock(
                    &mut tx,
                    p.id,
                    l.quantity,
                    id
                ).await?;
            }
            _ => {}
        }
    }
    if let Some(po) = d.purchase_order_id {
        crate::feature::purchase_order::service::refresh_status(&mut tx, po).await?;
    }
    tx.commit().await?;
    Ok(json!({"id":id,"created_products":created_products}))
}
