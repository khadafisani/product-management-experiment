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
        .filter(|d| d.kind == "purchase_order")
        .collect())
}

pub(crate) async fn validate_link(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    po: i64,
) -> Result<()> {
    use crate::helper::bad;
    let k: Option<(String,)> = sqlx::query_as("SELECT kind FROM documents WHERE id=? FOR UPDATE")
        .bind(po)
        .fetch_optional(&mut **tx)
        .await?;
    if k.map(|x| x.0) != Some("purchase_order".into()) {
        return Err(bad("Linked purchase order does not exist"));
    }
    Ok(())
}

pub(crate) async fn realize_line(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    po: i64,
    p: &crate::feature::product::entity::Product,
    l: &crate::feature::document::entity::LineInput,
) -> Result<()> {
    use crate::helper::bad;
    let line:Option<(i64,i64)>=sqlx::query_as("SELECT id,realized_quantity FROM document_lines WHERE document_id=? AND product_id=? FOR UPDATE").bind(po).bind(p.id).fetch_optional(&mut **tx).await?;
    let (line_id, left) = if let Some(line) = line {
        line
    } else {
        // An unplanned delivery remains visible on the PO without changing its original total.
        let line_id=sqlx::query("INSERT INTO document_lines(document_id,product_id,item_name,quantity,realized_quantity,price,subtotal,status) VALUES(?,?,?,0,0,?,0,'Warning')")
                    .bind(po).bind(p.id).bind(&p.name).bind(l.price).execute(&mut **tx).await?.last_insert_id() as i64;
        (line_id, 0)
    };
    let next = left
        .checked_sub(l.quantity)
        .ok_or_else(|| bad("Quantity too large"))?;
    sqlx::query("UPDATE document_lines SET realized_quantity=?,status=? WHERE id=?")
        .bind(next)
        .bind(if next < 0 {
            "Warning"
        } else if next == 0 {
            "Complete"
        } else {
            "Partial"
        })
        .bind(line_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn refresh_status(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    po: i64,
) -> Result<()> {
    sqlx::query("UPDATE documents SET status=CASE WHEN EXISTS(SELECT 1 FROM document_lines WHERE document_id=? AND realized_quantity<0) THEN 'Warning' WHEN NOT EXISTS(SELECT 1 FROM document_lines WHERE document_id=? AND realized_quantity<>0) THEN 'Complete' WHEN NOT EXISTS(SELECT 1 FROM document_lines WHERE document_id=? AND realized_quantity<>quantity) THEN 'Open' ELSE 'Partial' END WHERE id=?").bind(po).bind(po).bind(po).bind(po).execute(&mut **tx).await?;
    Ok(())
}

// Reverse realization when an invoice is edited or voided, retaining line status.
pub(crate) async fn restore_line(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    po: i64,
    product: i64,
    quantity: i64,
) -> Result<()> {
    sqlx::query("UPDATE document_lines SET realized_quantity=realized_quantity+?,status=CASE WHEN realized_quantity<0 THEN 'Warning' WHEN realized_quantity=0 THEN 'Complete' WHEN realized_quantity=quantity THEN 'Open' ELSE 'Partial' END WHERE document_id=? AND product_id=?").bind(quantity).bind(po).bind(product).execute(&mut **tx).await?;
    Ok(())
}
