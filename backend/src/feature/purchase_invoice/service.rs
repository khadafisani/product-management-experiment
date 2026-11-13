use super::entity::{ Document, DocumentInput };
use crate::{ config::App, helper::Result };
use serde_json::Value;
pub(crate) async fn create(a: App, d: DocumentInput) -> Result<Value> {
    super::validation::validate(&d)?;
    crate::feature::document::service::create_document(a, d).await
}
pub(crate) async fn list(a: App) -> Result<Vec<Document>> {
    Ok(
        crate::feature::document::service
            ::documents(a).await?
            .into_iter()
            .filter(|d| d.kind == "purchase_invoice")
            .collect()
    )
}
/// Release all units exactly once, only on full settlement. Caller owns the transaction.
pub(crate) async fn release_paid_stock(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    invoice: i64
) -> Result<()> {
    let (kind, status, released): (String, String, bool) = sqlx
        ::query_as("SELECT kind,status,stock_released FROM documents WHERE id=? FOR UPDATE")
        .bind(invoice)
        .fetch_one(&mut **tx).await?;
    if kind != "purchase_invoice" || status != "Paid" || released {
        return Ok(());
    }
    let lines: Vec<(i64, i64)> = sqlx
        ::query_as(
            "SELECT product_id,quantity FROM document_lines WHERE document_id=? ORDER BY product_id"
        )
        .bind(invoice)
        .fetch_all(&mut **tx).await?;
    for (product, quantity) in lines {
        crate::feature::product::service::adjust_stock(
            tx,
            product,
            0,
            quantity,
            Some(invoice),
            "Paid purchase release"
        ).await?;
    }
    sqlx
        ::query("UPDATE documents SET stock_released=TRUE WHERE id=?")
        .bind(invoice)
        .execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn receive_stock(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    product: i64,
    quantity: i64,
    invoice: i64
) -> Result<()> {
    crate::feature::product::service::adjust_stock(
        tx,
        product,
        quantity,
        0,
        Some(invoice),
        "purchase_invoice"
    ).await
}
