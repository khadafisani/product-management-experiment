use super::entity::{Document, DocumentInput};
use crate::{config::App, helper::Result};
use serde_json::Value;
pub(crate) async fn create(a: App, d: DocumentInput) -> Result<Value> {
    super::validation::validate(&d)?;
    crate::feature::document::service::create_document(a, d).await
}
pub(crate) async fn list(a: App) -> Result<Vec<Document>> {
    Ok(crate::feature::document::service::documents(a)
        .await?
        .into_iter()
        .filter(|d| d.kind == "sales_invoice")
        .collect())
}
pub(crate) async fn cancel(a: App, id: i64, input: super::entity::CancelInput) -> Result<Value> {
    super::validation::validate_cancellation(&input.reason)?;
    let mut tx = a.db.begin().await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind,status FROM documents WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let (kind, status) = row.ok_or_else(|| crate::helper::bad("Sales invoice not found"))?;
    if kind != "sales_invoice" {
        return Err(crate::helper::bad("Only sales invoices can be cancelled"));
    }
    if status == "Cancelled" {
        let (refund_due,): (i64,) = sqlx::query_as(
            "SELECT refund_due FROM sales_invoice_cancellations WHERE sales_invoice_id=?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        return Ok(
            serde_json::json!({"id":id,"status":"Cancelled","refund_due":refund_due,"already_cancelled":true}),
        );
    }
    let lines: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT product_id,quantity FROM document_lines WHERE document_id=? ORDER BY product_id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for (product, quantity) in lines {
        crate::feature::product::service::adjust_stock(
            &mut tx,
            product,
            quantity,
            quantity,
            Some(id),
            "Cancelled sale restoration",
        )
        .await?;
    }
    let (refund_due,):(i64,)=sqlx::query_as("SELECT CAST(COALESCE(SUM(amount),0) AS SIGNED) FROM payment_allocations WHERE invoice_id=?").bind(id).fetch_one(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO sales_invoice_cancellations(sales_invoice_id,reason,refund_due) VALUES(?,?,?)",
    )
    .bind(id)
    .bind(input.reason.trim())
    .bind(refund_due)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE documents SET status='Cancelled',remaining_payment=0 WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE document_lines SET status='Cancelled' WHERE document_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(
        serde_json::json!({"id":id,"status":"Cancelled","refund_due":refund_due,"already_cancelled":false}),
    )
}

pub(crate) async fn deduct_stock(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    product: i64,
    quantity: i64,
    invoice: i64,
) -> Result<()> {
    crate::feature::product::service::adjust_stock(
        tx,
        product,
        -quantity,
        -quantity,
        Some(invoice),
        "sales_invoice",
    )
    .await
}
