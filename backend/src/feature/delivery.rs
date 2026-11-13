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
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
#[derive(Deserialize)]
pub(crate) struct DetailInput {
    sales_invoice_detail_id: i64,
    quantity: i64,
    #[serde(default)]
    notes: String,
}
#[derive(Deserialize)]
pub(crate) struct Input {
    sales_invoice_id: i64,
    driver_name: String,
    date: String,
    address: String,
    #[serde(default)]
    note: String,
    details: Vec<DetailInput>,
}
fn validate(d: &Input) -> Result<()> {
    crate::helper::date::date_ok(&d.date)?;
    if d.driver_name.trim().is_empty()
        || d.driver_name.len() > 200
        || d.address.trim().is_empty()
        || d.address.len() > 1000
        || d.note.len() > 10000
        || d.details.is_empty()
        || d.details.len() > 500
    {
        return Err(bad("Driver, address and 1-500 details required"));
    }
    let mut seen = std::collections::HashSet::new();
    for l in &d.details {
        if l.quantity <= 0 || l.notes.len() > 10000 || !seen.insert(l.sales_invoice_detail_id) {
            return Err(bad(
                "Positive quantities and unique invoice details required",
            ));
        }
    }
    Ok(())
}
pub(crate) async fn list(State(a): State<App>) -> Result<Json<Value>> {
    let rows=sqlx::query("SELECT id,sales_invoice_id,driver_name,CAST(date AS CHAR) date,address,note,cancellation_note,status,created_by,approved_by FROM delivery_orders ORDER BY id DESC").fetch_all(&a.db).await?;
    Ok(Json(json!(rows.iter().map(order_json).collect::<Vec<_>>())))
}
fn order_json(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({"id":r.get::<i64,_>("id"),"sales_invoice_id":r.get::<i64,_>("sales_invoice_id"),"driver_name":r.get::<String,_>("driver_name"),"date":r.get::<String,_>("date"),"address":r.get::<String,_>("address"),"note":r.get::<String,_>("note"),"cancellation_note":r.get::<String,_>("cancellation_note"),"status":r.get::<String,_>("status"),"created_by":r.get::<i64,_>("created_by"),"approved_by":r.get::<Option<i64>,_>("approved_by")})
}
pub(crate) async fn detail(State(a): State<App>, Path(id): Path<i64>) -> Result<Json<Value>> {
    let r=sqlx::query("SELECT id,sales_invoice_id,driver_name,CAST(date AS CHAR) date,address,note,cancellation_note,status,created_by,approved_by FROM delivery_orders WHERE id=?").bind(id).fetch_optional(&a.db).await?.ok_or_else(||bad("Delivery order not found"))?;
    let lines=sqlx::query("SELECT dd.id,dd.sales_invoice_detail_id,l.product_id,dd.product_name,dd.quantity,dd.notes FROM delivery_order_details dd JOIN document_lines l ON l.id=dd.sales_invoice_detail_id WHERE dd.delivery_order_id=? ORDER BY dd.sales_invoice_detail_id").bind(id).fetch_all(&a.db).await?;
    Ok(Json(
        json!({"order":order_json(&r),"details":lines.iter().map(|l|json!({"id":l.get::<i64,_>("id"),"product_id":l.get::<i64,_>("product_id"),"sales_invoice_detail_id":l.get::<i64,_>("sales_invoice_detail_id"),"product_name":l.get::<String,_>("product_name"),"quantity":l.get::<i64,_>("quantity"),"notes":l.get::<String,_>("notes")})).collect::<Vec<_>>()}),
    ))
}
async fn invoice(tx: &mut sqlx::Transaction<'_, sqlx::MySql>, id: i64) -> Result<()> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind,status FROM documents WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    if !row.is_some_and(|(k, s)| k == "sales_invoice" && (s == "Paid" || s == "Partial Paid")) {
        return Err(bad(
            "Delivery requires a Paid or Partial Paid sales invoice",
        ));
    }
    Ok(())
}
async fn lines(tx: &mut sqlx::Transaction<'_, sqlx::MySql>, id: i64, d: &Input) -> Result<()> {
    let mut details = d.details.iter().collect::<Vec<_>>();
    details.sort_by_key(|l| l.sales_invoice_detail_id);
    for l in details {
        let row:Option<(String,i64)>=sqlx::query_as("SELECT item_name,quantity-delivered_quantity-on_delivery_quantity FROM document_lines WHERE id=? AND document_id=? FOR UPDATE").bind(l.sales_invoice_detail_id).bind(d.sales_invoice_id).fetch_optional(&mut **tx).await?;
        let (name, available) = row.ok_or_else(|| bad("Detail does not belong to invoice"))?;
        if l.quantity > available {
            return Err(bad("Quantity exceeds invoice delivery balance"));
        }
        sqlx::query("INSERT INTO delivery_order_details(delivery_order_id,sales_invoice_detail_id,product_name,quantity,notes) VALUES(?,?,?,?,?)").bind(id).bind(l.sales_invoice_detail_id).bind(name).bind(l.quantity).bind(&l.notes).execute(&mut **tx).await?;
    }
    Ok(())
}
pub(crate) async fn create(
    State(a): State<App>,
    Extension(c): Extension<Claims>,
    Json(d): Json<Input>,
) -> Result<Json<Value>> {
    validate(&d)?;
    let mut tx = a.db.begin().await?;
    invoice(&mut tx, d.sales_invoice_id).await?;
    let id=sqlx::query("INSERT INTO delivery_orders(sales_invoice_id,driver_name,date,address,note,cancellation_note,created_by) VALUES(?,?,?,?,?,'',?)").bind(d.sales_invoice_id).bind(d.driver_name.trim()).bind(&d.date).bind(d.address.trim()).bind(&d.note).bind(c.sub).execute(&mut *tx).await?.last_insert_id() as i64;
    lines(&mut tx, id, &d).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"status":"Draft"})))
}
pub(crate) async fn update(
    State(a): State<App>,
    Path(id): Path<i64>,
    Json(d): Json<Input>,
) -> Result<Json<Value>> {
    validate(&d)?;
    let mut tx = a.db.begin().await?;
    invoice(&mut tx, d.sales_invoice_id).await?;
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT sales_invoice_id,status FROM delivery_orders WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if row != Some((d.sales_invoice_id, "Draft".into())) {
        return Err(bad(
            "Only draft orders can be edited; invoice cannot change",
        ));
    }
    sqlx::query("DELETE FROM delivery_order_details WHERE delivery_order_id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    lines(&mut tx, id, &d).await?;
    sqlx::query("UPDATE delivery_orders SET driver_name=?,date=?,address=?,note=? WHERE id=?")
        .bind(d.driver_name.trim())
        .bind(&d.date)
        .bind(d.address.trim())
        .bind(d.note)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[derive(Deserialize)]
pub(crate) struct Transition {
    status: String,
    #[serde(default)]
    cancellation_note: String,
}
pub(crate) async fn approve(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    transition(a, id, c, "Approved".into(), String::new(), None).await
}
pub(crate) async fn status(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
    Json(d): Json<Transition>,
) -> Result<Json<Value>> {
    if !["On Delivery", "Delivered", "Cancelled"].contains(&d.status.as_str()) {
        return Err(bad("Invalid status"));
    }
    transition(a, id, c, d.status, d.cancellation_note, None).await
}
async fn transition(
    a: App,
    id: i64,
    c: Claims,
    target: String,
    note: String,
    request_id: Option<i64>,
) -> Result<Json<Value>> {
    let invoice_id: Option<i64> =
        sqlx::query_scalar("SELECT sales_invoice_id FROM delivery_orders WHERE id=?")
            .bind(id)
            .fetch_optional(&a.db)
            .await?;
    let invoice_id = invoice_id.ok_or_else(|| bad("Order not found"))?;
    let can_cancel_approved = if target == "Cancelled" {
        crate::feature::auth::allowed(&a, c.sub, "delivery.approve").await?
    } else {
        false
    };
    let mut tx = a.db.begin().await?;
    // All delivery, payment and void operations serialize on the parent invoice.
    let (invoice_status,): (String,) =
        sqlx::query_as("SELECT status FROM documents WHERE id=? FOR UPDATE")
            .bind(invoice_id)
            .fetch_one(&mut *tx)
            .await?;
    let (old,): (String,) =
        sqlx::query_as("SELECT status FROM delivery_orders WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(request) = request_id {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM delivery_cancellation_requests WHERE id=? AND delivery_order_id=? FOR UPDATE").bind(request).bind(id).fetch_optional(&mut *tx).await?;
        if status.as_deref() != Some("Pending") {
            return Err(bad("Pending cancellation request not found"));
        }
    }
    if old == target {
        return Ok(Json(json!({"id":id,"status":target})));
    }
    let legal = matches!(
        (old.as_str(), target.as_str()),
        ("Draft", "Approved")
            | ("Draft", "Cancelled")
            | ("Approved", "On Delivery")
            | ("Approved", "Delivered")
            | ("Approved", "Cancelled")
            | ("On Delivery", "Delivered")
            | ("On Delivery", "Cancelled")
    );
    if !legal {
        return Err(bad("Invalid delivery status transition"));
    }
    if target == "Cancelled" {
        if note.trim().is_empty() || note.len() > 10000 {
            return Err(bad("Cancellation note required"));
        }
        if old != "Draft" && !can_cancel_approved {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Finance must cancel approved deliveries".into(),
            ));
        }
    }
    if target != "Cancelled" && invoice_status != "Paid" && invoice_status != "Partial Paid" {
        return Err(bad("Invoice must be Paid or Partial Paid"));
    }
    let ls:Vec<(i64,i64)>=sqlx::query_as("SELECT sales_invoice_detail_id,quantity FROM delivery_order_details WHERE delivery_order_id=? ORDER BY sales_invoice_detail_id").bind(id).fetch_all(&mut *tx).await?;
    for (line, qty) in ls {
        if target == "Approved" {
            let affected=sqlx::query("UPDATE document_lines SET on_delivery_quantity=on_delivery_quantity+? WHERE id=? AND document_id=? AND quantity-delivered_quantity-on_delivery_quantity>=?").bind(qty).bind(line).bind(invoice_id).bind(qty).execute(&mut *tx).await?.rows_affected();
            if affected != 1 {
                return Err(bad("Insufficient undelivered quantity"));
            }
        } else if target == "Delivered" {
            let affected=sqlx::query("UPDATE document_lines SET on_delivery_quantity=on_delivery_quantity-?,delivered_quantity=delivered_quantity+? WHERE id=? AND on_delivery_quantity>=?").bind(qty).bind(qty).bind(line).bind(qty).execute(&mut *tx).await?.rows_affected();
            if affected != 1 {
                return Err(bad("Delivery reservation missing"));
            }
        } else if target == "Cancelled" && old != "Draft" {
            sqlx::query("UPDATE document_lines SET on_delivery_quantity=on_delivery_quantity-? WHERE id=? AND on_delivery_quantity>=?").bind(qty).bind(line).bind(qty).execute(&mut *tx).await?;
        }
    }
    sqlx::query("UPDATE delivery_orders SET status=?,cancellation_note=?,approved_by=IF(?='Approved',?,approved_by) WHERE id=?").bind(&target).bind(note).bind(&target).bind(c.sub).bind(id).execute(&mut *tx).await?;
    if target == "Cancelled" {
        sqlx::query("UPDATE delivery_cancellation_requests SET status='Approved',reviewed_by=? WHERE delivery_order_id=? AND status='Pending'").bind(c.sub).bind(id).execute(&mut *tx).await?;
    } else if target == "Delivered" {
        sqlx::query("UPDATE delivery_cancellation_requests SET status='Rejected',reviewed_by=? WHERE delivery_order_id=? AND status='Pending'").bind(c.sub).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id,"status":target})))
}

#[derive(Deserialize)]
pub(crate) struct CancellationReason {
    reason: String,
}
pub(crate) async fn request_cancel(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
    Json(d): Json<CancellationReason>,
) -> Result<Json<Value>> {
    if d.reason.trim().is_empty() || d.reason.len() > 500 {
        return Err(bad("Reason must contain 1-500 characters"));
    }
    let mut tx = a.db.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM delivery_orders WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if !status.is_some_and(|s| s == "Approved" || s == "On Delivery") {
        return Err(bad(
            "Only approved or in-transit deliveries need cancellation approval",
        ));
    }
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM delivery_cancellation_requests WHERE delivery_order_id=? AND status='Pending'").bind(id).fetch_one(&mut *tx).await?;
    if count > 0 {
        return Err(bad("A cancellation request is already pending"));
    }
    let request=sqlx::query("INSERT INTO delivery_cancellation_requests(delivery_order_id,requested_by,reason) VALUES(?,?,?)").bind(id).bind(c.sub).bind(d.reason.trim()).execute(&mut *tx).await?.last_insert_id();
    tx.commit().await?;
    Ok(Json(json!({"id":request,"status":"Pending"})))
}
pub(crate) async fn cancellation_requests(State(a): State<App>) -> Result<Json<Value>> {
    let rows=sqlx::query("SELECT id,delivery_order_id,requested_by,reason,status,reviewed_by FROM delivery_cancellation_requests ORDER BY id DESC").fetch_all(&a.db).await?;
    Ok(Json(json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("id"),"delivery_order_id":r.get::<i64,_>("delivery_order_id"),"requested_by":r.get::<i64,_>("requested_by"),"reason":r.get::<String,_>("reason"),"status":r.get::<String,_>("status"),"reviewed_by":r.get::<Option<i64>,_>("reviewed_by")})).collect::<Vec<_>>())))
}
pub(crate) async fn approve_cancel(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let row:Option<(i64,String)>=sqlx::query_as("SELECT delivery_order_id,reason FROM delivery_cancellation_requests WHERE id=? AND status='Pending'").bind(id).fetch_optional(&a.db).await?;
    let (order, reason) = row.ok_or_else(|| bad("Pending cancellation request not found"))?;
    transition(a, order, c, "Cancelled".into(), reason, Some(id)).await
}
pub(crate) async fn reject_cancel(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    if sqlx::query("UPDATE delivery_cancellation_requests SET status='Rejected',reviewed_by=? WHERE id=? AND status='Pending'").bind(c.sub).bind(id).execute(&a.db).await?.rows_affected()!=1{return Err(bad("Pending cancellation request not found"))}
    Ok(Json(json!({"id":id,"status":"Rejected"})))
}
