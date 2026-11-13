use crate::{
    config::App,
    feature::{
        auth::{Claims, allowed},
        document::entity::{DOC_SELECT, Document},
        product::entity::Product,
    },
    helper::{Error, Result},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use sqlx::Row;
fn missing() -> Error {
    Error(StatusCode::NOT_FOUND, "Record not found".into())
}
async fn document_link(a: &App, id: i64) -> Result<Option<Document>> {
    Ok(sqlx::query_as(&format!("{DOC_SELECT} WHERE id=?"))
        .bind(id)
        .fetch_optional(&a.db)
        .await?)
}
pub(crate) async fn document(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    if document_link(&a, id).await?.is_none() {
        return Err(missing());
    }
    let mut data = crate::feature::document::service::detail(a.clone(), id).await?;
    let payments = allowed(&a, c.sub, "payments.read").await?;
    let delivery = allowed(&a, c.sub, "delivery.read").await?;
    data["access"] = json!({"payments":payments,"delivery":delivery});
    if !payments {
        data["payments"] = json!([]);
    }
    let po = data["document"]["purchase_order_id"].as_i64();
    data["purchase_order"] = match po {
        Some(id) => json!(document_link(&a, id).await?),
        None => Value::Null,
    };
    data["purchase_order_lines"] = if let Some(po_id) = po {
        let rows=sqlx::query("SELECT po.product_id,po.item_name,po.quantity,po.realized_quantity,po.price,po.subtotal,po.status,COALESCE(current_line.quantity,0) invoice_quantity FROM document_lines po LEFT JOIN document_lines current_line ON current_line.document_id=? AND current_line.product_id=po.product_id WHERE po.document_id=? ORDER BY po.id").bind(id).bind(po_id).fetch_all(&a.db).await?;
        json!(rows.iter().map(|r|json!({"product_id":r.get::<i64,_>("product_id"),"item_name":r.get::<String,_>("item_name"),"ordered_quantity":r.get::<i64,_>("quantity"),"received_quantity":r.get::<i64,_>("quantity")-r.get::<i64,_>("realized_quantity"),"remaining_quantity":r.get::<i64,_>("realized_quantity"),"invoice_quantity":r.get::<i64,_>("invoice_quantity"),"price":r.get::<i64,_>("price"),"subtotal":r.get::<i64,_>("subtotal"),"status":r.get::<String,_>("status")})).collect::<Vec<_>>())
    } else {
        json!([])
    };
    let children: Vec<Document> = sqlx::query_as(&format!(
        "{DOC_SELECT} WHERE purchase_order_id=? ORDER BY date DESC,id DESC"
    ))
    .bind(id)
    .fetch_all(&a.db)
    .await?;
    data["purchase_invoices"] = json!(children);
    data["delivery_orders"] = if delivery {
        let rows=sqlx::query("SELECT id,driver_name,CAST(date AS CHAR) date,status FROM delivery_orders WHERE sales_invoice_id=? ORDER BY date DESC,id DESC").bind(id).fetch_all(&a.db).await?;
        json!(rows.iter().map(delivery_json).collect::<Vec<_>>())
    } else {
        json!([])
    };
    Ok(Json(data))
}
fn delivery_json(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({"id":r.get::<i64,_>("id"),"driver_name":r.get::<String,_>("driver_name"),"date":r.get::<String,_>("date"),"status":r.get::<String,_>("status")})
}
pub(crate) async fn payment(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let row = sqlx::query(
        "SELECT id,reference,CAST(date AS CHAR) date,amount,kind FROM payments WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&a.db)
    .await?
    .ok_or_else(missing)?;
    let can_read = allowed(&a, c.sub, "invoices.read").await?;
    let invoices = if can_read {
        let rows=sqlx::query("SELECT d.id,d.number,d.kind,CAST(d.date AS CHAR) date,d.person,d.status,d.grand_total,d.remaining_payment,pa.amount FROM payment_allocations pa JOIN documents d ON d.id=pa.invoice_id WHERE pa.payment_id=? ORDER BY d.id").bind(id).fetch_all(&a.db).await?;
        json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("id"),"number":r.get::<String,_>("number"),"kind":r.get::<String,_>("kind"),"date":r.get::<String,_>("date"),"person":r.get::<String,_>("person"),"status":r.get::<String,_>("status"),"grand_total":r.get::<i64,_>("grand_total"),"remaining_payment":r.get::<i64,_>("remaining_payment"),"allocated_amount":r.get::<i64,_>("amount")})).collect::<Vec<_>>())
    } else {
        json!([])
    };
    Ok(Json(
        json!({"payment":{"id":id,"reference":row.get::<String,_>("reference"),"date":row.get::<String,_>("date"),"amount":row.get::<i64,_>("amount"),"kind":row.get::<String,_>("kind")},"invoices":invoices,"access":{"invoices":can_read}}),
    ))
}
pub(crate) async fn product(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let p:Product=sqlx::query_as("SELECT id,name,quantity,remaining_quantity,status,photo,description,price,reorder_level FROM products WHERE id=?").bind(id).fetch_optional(&a.db).await?.ok_or_else(missing)?;
    let invoices = allowed(&a, c.sub, "invoices.read").await?;
    let delivery = allowed(&a, c.sub, "delivery.read").await?;
    let mut data = json!({"product":p,"purchase_orders":[],"purchase_invoices":[],"sales_invoices":[],"delivery_orders":[],"access":{"invoices":invoices,"delivery":delivery},"recent_limit":10});
    if invoices {
        for (kind, key) in [
            ("purchase_order", "purchase_orders"),
            ("purchase_invoice", "purchase_invoices"),
            ("sales_invoice", "sales_invoices"),
        ] {
            let rows=sqlx::query("SELECT d.id,d.kind,d.number,CAST(d.date AS CHAR) date,d.person,d.status,l.quantity FROM documents d JOIN document_lines l ON l.document_id=d.id WHERE l.product_id=? AND d.kind=? ORDER BY d.date DESC,d.id DESC LIMIT 10").bind(id).bind(kind).fetch_all(&a.db).await?;
            data[key]=json!(rows.iter().map(|r|json!({"id":r.get::<i64,_>("id"),"kind":r.get::<String,_>("kind"),"number":r.get::<String,_>("number"),"date":r.get::<String,_>("date"),"person":r.get::<String,_>("person"),"status":r.get::<String,_>("status"),"quantity":r.get::<i64,_>("quantity")})).collect::<Vec<_>>());
        }
    }
    if delivery {
        let rows=sqlx::query("SELECT o.id,o.driver_name,CAST(o.date AS CHAR) date,o.status,dd.quantity FROM delivery_orders o JOIN delivery_order_details dd ON dd.delivery_order_id=o.id JOIN document_lines l ON l.id=dd.sales_invoice_detail_id WHERE l.product_id=? ORDER BY o.date DESC,o.id DESC LIMIT 10").bind(id).fetch_all(&a.db).await?;
        data["delivery_orders"] = json!(
            rows.iter()
                .map(|r| {
                    let mut d = delivery_json(r);
                    d["quantity"] = json!(r.get::<i64, _>("quantity"));
                    d
                })
                .collect::<Vec<_>>()
        );
    }
    Ok(Json(data))
}
pub(crate) async fn delivery(
    State(a): State<App>,
    Path(id): Path<i64>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM delivery_orders WHERE id=?")
        .bind(id)
        .fetch_optional(&a.db)
        .await?;
    if exists.is_none() {
        return Err(missing());
    }
    let mut data = crate::feature::delivery::detail(State(a.clone()), Path(id))
        .await?
        .0;
    let invoices = allowed(&a, c.sub, "invoices.read").await?;
    data["access"] = json!({"invoices":invoices});
    data["sales_invoice"] = if invoices {
        json!(document_link(&a, data["order"]["sales_invoice_id"].as_i64().unwrap()).await?)
    } else {
        Value::Null
    };
    Ok(Json(data))
}

pub(crate) async fn payments(
    State(a): State<App>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let mut data = crate::feature::payment::service::payments(a.clone()).await?;
    if !allowed(&a, c.sub, "invoices.read").await? {
        if let Some(rows) = data.as_array_mut() {
            for row in rows {
                row["invoices"] = json!("");
                row["invoice_links"] = json!([]);
            }
        }
    }
    Ok(Json(data))
}
