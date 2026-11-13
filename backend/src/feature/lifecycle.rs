use crate::{
    config::App,
    feature::auth::Claims,
    helper::{Error, Result, bad},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use sqlx::{MySql, Transaction};
fn missing() -> Error {
    Error(StatusCode::NOT_FOUND, "Record not found".into())
}
async fn mark(tx: &mut Transaction<'_, MySql>, table: &str, id: i64) -> Result<Value> {
    // Table names are constants supplied by the handlers below.
    if sqlx::query(&format!(
        "UPDATE {table} SET deleted_at=CURRENT_TIMESTAMP(6) WHERE id=? AND deleted_at IS NULL"
    ))
    .bind(id)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        != 1
    {
        return Err(missing());
    }
    let (created_at,updated_at,deleted_at):(String,String,String)=sqlx::query_as(&format!("SELECT CAST(created_at AS CHAR),CAST(updated_at AS CHAR),CAST(deleted_at AS CHAR) FROM {table} WHERE id=?")).bind(id).fetch_one(&mut **tx).await?;
    Ok(
        json!({"id":id,"deleted":true,"created_at":created_at,"updated_at":updated_at,"deleted_at":deleted_at}),
    )
}
pub(crate) async fn product(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let mut tx = a.db.begin().await?;
    let stock:Option<(i64,i64)>=sqlx::query_as("SELECT quantity,remaining_quantity FROM products WHERE id=? AND deleted_at IS NULL FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    let stock = stock.ok_or_else(missing)?;
    let linked:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_lines l JOIN documents d ON d.id=l.document_id WHERE l.product_id=? AND l.deleted_at IS NULL AND d.deleted_at IS NULL AND d.status<>'Cancelled'").bind(id).fetch_one(&mut *tx).await?;
    if stock != (0, 0) || linked > 0 {
        return Err(bad(
            "Only zero-stock products without active document references can be deleted",
        ));
    }
    let result = mark(&mut tx, "products", id).await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub(crate) async fn document(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let mut tx = a.db.begin().await?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT kind,status FROM documents WHERE id=? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (kind, status) = row.ok_or_else(missing)?;
    if kind == "purchase_order" {
        let children: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM documents WHERE purchase_order_id=? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if children > 0 {
            return Err(bad(
                "Delete or unlink related purchase invoices before deleting their purchase order",
            ));
        }
    } else if status != "Cancelled" {
        return Err(bad(
            "Void the invoice through the Finance approval workflow before deleting it",
        ));
    }
    sqlx::query("UPDATE delivery_order_details dd JOIN delivery_orders o ON o.id=dd.delivery_order_id SET dd.deleted_at=CURRENT_TIMESTAMP(6) WHERE o.sales_invoice_id=? AND o.status='Cancelled' AND dd.deleted_at IS NULL").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE delivery_orders SET deleted_at=CURRENT_TIMESTAMP(6) WHERE sales_invoice_id=? AND status='Cancelled' AND deleted_at IS NULL").bind(id).execute(&mut *tx).await?;
    let result = mark(&mut tx, "documents", id).await?;
    sqlx::query("UPDATE document_lines SET deleted_at=CURRENT_TIMESTAMP(6) WHERE document_id=? AND deleted_at IS NULL").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub(crate) async fn delivery(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let invoice: Option<i64> = sqlx::query_scalar(
        "SELECT sales_invoice_id FROM delivery_orders WHERE id=? AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&a.db)
    .await?;
    let invoice = invoice.ok_or_else(missing)?;
    let mut tx = a.db.begin().await?;
    // Match the invoice-first lock order used by delivery transitions and voiding.
    sqlx::query("SELECT id FROM documents WHERE id=? FOR UPDATE")
        .bind(invoice)
        .fetch_one(&mut *tx)
        .await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM delivery_orders WHERE id=? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let status = status.ok_or_else(missing)?;
    if !["Draft", "Cancelled"].contains(&status.as_str()) {
        return Err(bad(
            "Only draft or cancelled deliveries can be deleted; cancel approved deliveries through Finance first",
        ));
    }
    let result = mark(&mut tx, "delivery_orders", id).await?;
    sqlx::query("UPDATE delivery_order_details SET deleted_at=CURRENT_TIMESTAMP(6) WHERE delivery_order_id=? AND deleted_at IS NULL").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub(crate) async fn payment(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let mut tx = a.db.begin().await?;
    let allocations:Vec<(i64,i64)>=sqlx::query_as("SELECT invoice_id,amount FROM payment_allocations WHERE payment_id=? AND deleted_at IS NULL ORDER BY invoice_id").bind(id).fetch_all(&mut *tx).await?;
    let mut invoices = Vec::new();
    for (invoice, amount) in allocations {
        let row:Option<(String,String,i64,i64,bool)>=sqlx::query_as("SELECT kind,status,grand_total,remaining_payment,stock_released FROM documents WHERE id=? AND deleted_at IS NULL FOR UPDATE").bind(invoice).fetch_optional(&mut *tx).await?;
        let row = row.ok_or_else(|| {
            bad("Payments linked to archived invoices must be retained for audit")
        })?;
        if row.1 == "Cancelled" {
            return Err(bad(
                "Payments linked to voided invoices must be retained for refund audit",
            ));
        }
        invoices.push((invoice, amount, row));
    }
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM payments WHERE id=? AND deleted_at IS NULL FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(missing());
    }
    // Recheck under the payment lock: another delete may have completed while we waited.
    for (invoice, amount, (kind, _status, total, balance, released)) in invoices {
        let left = balance
            .checked_add(amount)
            .filter(|n| *n <= total)
            .ok_or_else(|| bad("Invalid invoice balance"))?;
        if kind == "sales_invoice" && left == total {
            let deliveries:i64=sqlx::query_scalar("SELECT COUNT(*) FROM delivery_orders WHERE sales_invoice_id=? AND deleted_at IS NULL AND status IN ('Approved','On Delivery','Delivered')").bind(invoice).fetch_one(&mut *tx).await?;
            if deliveries > 0 {
                return Err(bad(
                    "Cancel approved deliveries before removing the invoice's last payment; delivered invoices cannot become unpaid",
                ));
            }
        }
        if kind == "purchase_invoice" && released && left > 0 {
            let lines:Vec<(i64,i64)>=sqlx::query_as("SELECT product_id,quantity FROM document_lines WHERE document_id=? AND deleted_at IS NULL ORDER BY product_id").bind(invoice).fetch_all(&mut *tx).await?;
            for (product, quantity) in lines {
                crate::feature::product::service::adjust_stock(
                    &mut tx,
                    product,
                    0,
                    -quantity,
                    Some(invoice),
                    "Deleted payment: undo paid purchase release",
                )
                .await?;
            }
            sqlx::query("UPDATE documents SET stock_released=FALSE WHERE id=?")
                .bind(invoice)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE documents SET remaining_payment=?,status=? WHERE id=?")
            .bind(left)
            .bind(if left == 0 {
                "Paid"
            } else if left == total {
                "Unpaid"
            } else {
                "Partial Paid"
            })
            .bind(invoice)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE payment_allocations SET deleted_at=CURRENT_TIMESTAMP(6) WHERE payment_id=? AND deleted_at IS NULL").bind(id).execute(&mut *tx).await?;
    let result = mark(&mut tx, "payments", id).await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub(crate) async fn user(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    if id == c.sub {
        return Err(bad("Cannot delete your own account"));
    }
    let mut tx = a.db.begin().await?;
    let username: Option<String> = sqlx::query_scalar(
        "SELECT username FROM users WHERE id=? AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let username = username.ok_or_else(missing)?;
    if username == "admin" {
        return Err(bad("The seeded admin account is protected"));
    }
    sqlx::query("UPDATE users SET active=FALSE WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let result = mark(&mut tx, "users", id).await?;
    sqlx::query("UPDATE auth_sessions SET revoked=TRUE WHERE user_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(result))
}
pub(crate) async fn role(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let mut tx = a.db.begin().await?;
    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM roles WHERE id=? AND deleted_at IS NULL FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let name = name.ok_or_else(missing)?;
    if ["Admin", "Staff", "Finance"].contains(&name.as_str()) {
        return Err(bad("Built-in roles are protected"));
    }
    let users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role_id=? AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if users > 0 {
        return Err(bad("Reassign this role's users before deleting it"));
    }
    let result = mark(&mut tx, "roles", id).await?;
    sqlx::query("UPDATE role_permissions SET deleted_at=CURRENT_TIMESTAMP(6) WHERE role_id=? AND deleted_at IS NULL").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}
