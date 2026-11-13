use super::{entity::PaymentInput, validation::allocate};

use crate::{
    config::App,
    helper::{Result, bad},
};
use serde_json::{Value, json};
use sqlx::Row;

pub(crate) async fn pay(a: App, p: PaymentInput) -> Result<Value> {
    super::validation::validate(&p)?;
    let mut tx = a.db.begin().await?;
    let mut ids = p.invoice_ids.clone();
    ids.sort();
    let mut balances = std::collections::HashMap::new();
    for id in ids {
        let row: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT kind,remaining_payment,status FROM documents WHERE deleted_at IS NULL AND id=? FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let (kind, balance, status) = row.ok_or_else(|| bad("Invoice not found"))?;
        if status == "Cancelled" {
            return Err(bad("Cancelled invoices cannot receive payments"));
        }
        if kind != p.kind {
            return Err(bad("Payment invoice types must match"));
        }
        balances.insert(id, balance);
    }
    let amounts = allocate(
        p.amount,
        &p.invoice_ids
            .iter()
            .map(|id| balances[id])
            .collect::<Vec<_>>(),
    )?;
    let id = sqlx::query("INSERT INTO payments(reference,date,amount,kind) VALUES(?,?,?,?)")
        .bind(&p.reference)
        .bind(&p.date)
        .bind(p.amount)
        .bind(&p.kind)
        .execute(&mut *tx)
        .await?
        .last_insert_id() as i64;
    for (invoice, n) in p.invoice_ids.iter().zip(amounts) {
        let left = balances[invoice] - n;
        sqlx::query("INSERT INTO payment_allocations(payment_id,invoice_id,amount) VALUES(?,?,?)")
            .bind(id)
            .bind(invoice)
            .bind(n)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE documents SET remaining_payment=?,status=? WHERE deleted_at IS NULL AND id=?")
            .bind(left)
            .bind(if left == 0 { "Paid" } else { "Partial Paid" })
            .bind(invoice)
            .execute(&mut *tx)
            .await?;
    }
    if p.kind == "purchase_invoice" {
        for invoice in &p.invoice_ids {
            crate::feature::purchase_invoice::service::release_paid_stock(&mut tx, *invoice)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(json!({"id":id}))
}
pub(crate) async fn payments(a: App) -> Result<Value> {
    let rows=sqlx::query("SELECT p.id,p.reference,CAST(p.date AS CHAR) date,p.amount,p.kind,CAST(p.created_at AS CHAR) created_at,CAST(p.updated_at AS CHAR) updated_at,CAST(p.deleted_at AS CHAR) deleted_at, COALESCE(GROUP_CONCAT(d.number ORDER BY d.id SEPARATOR ', '),'') invoices FROM payments p LEFT JOIN payment_allocations pa ON pa.payment_id=p.id AND pa.deleted_at IS NULL LEFT JOIN documents d ON d.id=pa.invoice_id AND d.deleted_at IS NULL WHERE p.deleted_at IS NULL GROUP BY p.id ORDER BY p.date DESC,p.id DESC").fetch_all(&a.db).await?;
    let links=sqlx::query("SELECT pa.payment_id,d.id,d.number,d.kind FROM payment_allocations pa JOIN documents d ON d.id=pa.invoice_id WHERE pa.deleted_at IS NULL AND d.deleted_at IS NULL ORDER BY d.id").fetch_all(&a.db).await?;
    let mut grouped=std::collections::HashMap::<i64,Vec<Value>>::new();
    for r in links {grouped.entry(r.get("payment_id")).or_default().push(json!({"id":r.get::<i64,_>("id"),"number":r.get::<String,_>("number"),"kind":r.get::<String,_>("kind")}));}
    Ok(json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("id"),"reference":r.get::<String,_>("reference"),"date":r.get::<String,_>("date"),"amount":r.get::<i64,_>("amount"),"kind":r.get::<String,_>("kind"),"created_at":r.get::<String,_>("created_at"),"updated_at":r.get::<String,_>("updated_at"),"deleted_at":r.get::<Option<String>,_>("deleted_at"),"invoices":r.get::<String,_>("invoices"),"invoice_links":grouped.get(&r.get::<i64,_>("id")).cloned().unwrap_or_default()})).collect::<Vec<_>>()))
}
