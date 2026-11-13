use crate::{
    config::App,
    feature::auth::Claims,
    helper::{Result, bad},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
pub(crate) struct Reason {
    pub reason: String,
}
fn reason(s: &str) -> Result<()> {
    if s.trim().is_empty() || s.len() > 500 {
        Err(bad("Reason must contain 1-500 characters"))
    } else {
        Ok(())
    }
}
pub(crate) async fn request_void(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
    Json(d): Json<Reason>,
) -> Result<Json<Value>> {
    reason(&d.reason)?;
    if crate::feature::auth::allowed(&a, c.sub, "invoices.void").await? {
        return Ok(Json(void(&a, id, &d.reason, None, c.sub).await?));
    }
    let mut tx = a.db.begin().await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind,status FROM documents WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if !row.is_some_and(|(k, s)| k != "purchase_order" && s != "Cancelled") {
        return Err(bad("Active invoice required"));
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoice_void_requests WHERE invoice_id=? AND status='Pending'",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if pending > 0 {
        return Err(bad("A void request is already pending"));
    }
    let request_id = sqlx::query(
        "INSERT INTO invoice_void_requests(invoice_id,requested_by,reason) VALUES(?,?,?)",
    )
    .bind(id)
    .bind(c.sub)
    .bind(d.reason.trim())
    .execute(&mut *tx)
    .await?
    .last_insert_id();
    tx.commit().await?;
    Ok(Json(json!({"id":request_id,"status":"Pending"})))
}
pub(crate) async fn requests(State(a): State<App>) -> Result<Json<Value>> {
    use sqlx::Row;
    let rows=sqlx::query("SELECT id,invoice_id,requested_by,reason,status,reviewed_by FROM invoice_void_requests ORDER BY id DESC").fetch_all(&a.db).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("id"),"invoice_id":r.get::<i64,_>("invoice_id"),"requested_by":r.get::<i64,_>("requested_by"),"reason":r.get::<String,_>("reason"),"status":r.get::<String,_>("status"),"reviewed_by":r.get::<Option<i64>,_>("reviewed_by")})).collect::<Vec<_>>())))
}
pub(crate) async fn approve(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let (invoice, reason): (i64, String) = sqlx::query_as(
        "SELECT invoice_id,reason FROM invoice_void_requests WHERE id=? AND status='Pending'",
    )
    .bind(id)
    .fetch_optional(&a.db)
    .await?
    .ok_or_else(|| bad("Pending request not found"))?;
    Ok(Json(void(&a, invoice, &reason, Some(id), c.sub).await?))
}
pub(crate) async fn reject(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    if sqlx::query("UPDATE invoice_void_requests SET status='Rejected',reviewed_by=? WHERE id=? AND status='Pending'").bind(c.sub).bind(id).execute(&a.db).await?.rows_affected()!=1{return Err(bad("Pending request not found"))}
    Ok(Json(json!({"id":id,"status":"Rejected"})))
}
async fn void(
    a: &App,
    id: i64,
    reason: &str,
    request: Option<i64>,
    reviewer: i64,
) -> Result<Value> {
    let mut tx = a.db.begin().await?;
    let (kind, status, released, po): (String, String, bool, Option<i64>) = sqlx::query_as(
        "SELECT kind,status,stock_released,purchase_order_id FROM documents WHERE id=? FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| bad("Invoice not found"))?;
    if kind == "purchase_order" {
        return Err(bad("Only invoices can be voided"));
    }
    if let Some(req) = request {
        let (s,): (String,) =
            sqlx::query_as("SELECT status FROM invoice_void_requests WHERE id=? FOR UPDATE")
                .bind(req)
                .fetch_one(&mut *tx)
                .await?;
        if s != "Pending" {
            return Err(bad("Request already reviewed"));
        }
    }
    if status == "Cancelled" {
        return Err(bad("Invoice already cancelled"));
    }
    if let Some(po) = po {
        crate::feature::purchase_order::service::validate_link(&mut tx, po).await?;
    }
    let deliveries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM delivery_orders WHERE sales_invoice_id=? AND status<>'Cancelled'",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if deliveries > 0 {
        return Err(bad(
            "Cancel delivery orders before voiding; delivered invoices cannot be voided",
        ));
    }
    let ls: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT product_id,quantity FROM document_lines WHERE document_id=? ORDER BY product_id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for (product, qty) in ls {
        let (physical, available) = if kind == "sales_invoice" {
            (qty, qty)
        } else {
            (-qty, if released { -qty } else { 0 })
        };
        crate::feature::product::service::adjust_stock(
            &mut tx,
            product,
            physical,
            available,
            Some(id),
            "Voided invoice",
        )
        .await?;
        if let Some(po) = po {
            crate::feature::purchase_order::service::restore_line(&mut tx, po, product, qty)
                .await?;
        }
    }
    if let Some(po) = po {
        crate::feature::purchase_order::service::refresh_status(&mut tx, po).await?
    }
    let (refund,):(i64,)=sqlx::query_as("SELECT CAST(COALESCE(SUM(amount),0) AS SIGNED) FROM payment_allocations WHERE invoice_id=?").bind(id).fetch_one(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO invoice_voids(invoice_id,reason,reviewed_by,refund_due) VALUES(?,?,?,?)",
    )
    .bind(id)
    .bind(reason)
    .bind(reviewer)
    .bind(refund)
    .execute(&mut *tx)
    .await?;
    if kind == "sales_invoice" {
        sqlx::query("INSERT INTO sales_invoice_cancellations(sales_invoice_id,reason,refund_due) VALUES(?,?,?)").bind(id).bind(reason).bind(refund).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE documents SET status='Cancelled',remaining_payment=0 WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE document_lines SET status='Cancelled' WHERE document_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE invoice_void_requests SET status='Approved',reviewed_by=? WHERE invoice_id=? AND status='Pending'").bind(reviewer).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({"id":id,"status":"Cancelled","refund_due":refund}))
}
#[derive(Deserialize)]
pub(crate) struct Update {
    number: String,
    date: String,
    person: String,
    lines: Option<Vec<crate::feature::document::entity::LineInput>>,
}
pub(crate) async fn update(
    State(a): State<App>,
    Path(id): Path<i64>,
    Json(d): Json<Update>,
) -> Result<Json<Value>> {
    crate::helper::date::date_ok(&d.date)?;
    if d.number.trim().is_empty()
        || d.number.len() > 100
        || d.person.trim().is_empty()
        || d.person.len() > 200
    {
        return Err(bad("Valid number and person required"));
    }
    let mut tx = a.db.begin().await?;
    let (kind, status, po, released): (String, String, Option<i64>, bool) = sqlx::query_as(
        "SELECT kind,status,purchase_order_id,stock_released FROM documents WHERE id=? FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| bad("Invoice not found"))?;
    if !["sales_invoice", "purchase_invoice"].contains(&kind.as_str()) || status == "Cancelled" {
        return Err(bad("Active invoice required"));
    }
    if let Some(lines) = d.lines {
        let total = crate::feature::document::validation::total(&lines)?;
        let payments: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM payment_allocations WHERE invoice_id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        let deliveries: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM delivery_orders WHERE sales_invoice_id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if payments > 0 || deliveries > 0 {
            return Err(bad(
                "Invoice lines cannot change after payments or delivery orders; void and reissue instead",
            ));
        }
        if lines.iter().any(|l| l.product_id <= 0) {
            return Err(bad("Invoice updates require existing products"));
        }
        if let Some(po) = po {
            crate::feature::purchase_order::service::validate_link(&mut tx, po).await?;
        }
        let old:Vec<(i64,i64)>=sqlx::query_as("SELECT product_id,quantity FROM document_lines WHERE document_id=? ORDER BY product_id").bind(id).fetch_all(&mut *tx).await?;
        let mut products = std::collections::BTreeSet::new();
        for (p, _) in &old {
            products.insert(*p);
        }
        for l in &lines {
            products.insert(l.product_id);
        }
        for product in products {
            let before = old
                .iter()
                .find(|(p, _)| *p == product)
                .map(|(_, q)| *q)
                .unwrap_or(0);
            let after = lines
                .iter()
                .find(|l| l.product_id == product)
                .map(|l| l.quantity)
                .unwrap_or(0);
            let delta = after - before;
            let stock_delta = if kind == "sales_invoice" {
                -delta
            } else {
                delta
            };
            crate::feature::product::service::adjust_stock(
                &mut tx,
                product,
                stock_delta,
                if kind == "sales_invoice" || released {
                    stock_delta
                } else {
                    0
                },
                Some(id),
                "Invoice line update",
            )
            .await?;
            if let Some(po) = po {
                crate::feature::purchase_order::service::restore_line(&mut tx, po, product, before)
                    .await?;
            }
        }
        sqlx::query("DELETE FROM document_lines WHERE document_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for l in &lines {
            let p = crate::feature::product::service::lookup_product(&mut tx, l)
                .await?
                .ok_or_else(|| bad("Product not found"))?;
            if p.status != "Active" {
                return Err(bad("Inactive products cannot be used"));
            }
            sqlx::query("INSERT INTO document_lines(document_id,product_id,item_name,quantity,realized_quantity,price,subtotal,status) VALUES(?,?,?,?,0,?,?,'Posted')").bind(id).bind(p.id).bind(&p.name).bind(l.quantity).bind(l.price).bind(l.quantity*l.price).execute(&mut *tx).await?;
            if let Some(po) = po {
                crate::feature::purchase_order::service::realize_line(&mut tx, po, &p, l).await?;
            }
        }
        if let Some(po) = po {
            crate::feature::purchase_order::service::refresh_status(&mut tx, po).await?;
        }
        sqlx::query("UPDATE documents SET grand_total=?,remaining_payment=? WHERE id=?")
            .bind(total)
            .bind(total)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE documents SET number=?,date=?,person=? WHERE id=?")
        .bind(d.number.trim())
        .bind(d.date)
        .bind(d.person.trim())
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
