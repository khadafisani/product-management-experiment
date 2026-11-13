use crate::{
    config::App,
    feature::auth::{Claims, allowed},
    helper::{Error, Result, bad},
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{MySql, QueryBuilder, Row};
#[derive(Default, Clone, Serialize, Deserialize)]
pub(crate) struct Filters {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub q: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub status: Option<String>,
    pub kind: Option<String>,
    pub days: Option<i64>,
    pub lead_time: Option<i64>,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    #[serde(default)]
    start: bool,
    v: u8,
    scope: String,
    date: String,
    id: i64,
    bound: i64,
}
struct Spec {
    source: String,
    date: &'static str,
    base: &'static str,
    payload: &'static str,
    search: Vec<&'static str>,
    status: &'static str,
    table: &'static str,
}
pub(crate) fn permission(resource: &str) -> Option<&'static str> {
    Some(match resource {
        "products" => "products.read",
        "payments" => "payments.read",
        "delivery-orders" | "delivery-cancellations" => "delivery.read",
        "invoice-approvals" => "invoices.request_void",
        "roles" | "users" => "roles.manage",
        "purchase-orders" | "purchase-invoices" | "sales-invoices" | "documents" | "forecast" => {
            "invoices.read"
        }
        _ => return None,
    })
}
fn spec(resource: &str, f: &Filters) -> Result<Spec> {
    let s = match resource {
        "products" => Spec {
            source: "products t".into(),
            date: "t.created_at",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'name',t.name,'description',t.description,'photo',t.photo,'quantity',t.quantity,'remaining_quantity',t.remaining_quantity,'status',t.status,'price',t.price,'reorder_level',t.reorder_level,'date',DATE_FORMAT(t.created_at,'%Y-%m-%d'))",
            search: vec!["t.name",
                "t.description"],
            status: "t.status",
            table: "products"},
        "documents"|"purchase-orders"|"purchase-invoices"|"sales-invoices" => Spec {
            source: "documents t LEFT JOIN documents po ON po.id=t.purchase_order_id AND po.deleted_at IS NULL".into(),
            date: "t.date",
            base: match resource {
                "purchase-orders" => "t.kind='purchase_order'",
                "purchase-invoices" => "t.kind='purchase_invoice'",
                "sales-invoices" => "t.kind='sales_invoice'",
                _ => "1=1"},
            payload: "JSON_OBJECT('id',t.id,'kind',t.kind,'number',t.number,'date',CAST(t.date AS CHAR),'person',t.person,'purchase_order_id',t.purchase_order_id,'purchase_order_number',po.number,'grand_total',t.grand_total,'remaining_payment',t.remaining_payment,'status',t.status)",
            search: vec!["t.number",
                "t.person",
                "po.number"],
            status: "t.status",
            table: "documents"},
        "payments" => Spec {
            source: "payments t".into(),
            date: "t.date",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'reference',t.reference,'date',CAST(t.date AS CHAR),'kind',t.kind,'amount',t.amount,'invoice_count',(SELECT COUNT(*) FROM payment_allocations a JOIN documents invoice ON invoice.id=a.invoice_id AND invoice.deleted_at IS NULL WHERE a.deleted_at IS NULL AND a.payment_id=t.id))",
            search: vec!["t.reference"],
            status: "",
            table: "payments"},
        "delivery-orders" => Spec {
            source: "delivery_orders t JOIN documents d ON d.id=t.sales_invoice_id AND d.deleted_at IS NULL".into(),
            date: "t.date",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'number',CONCAT('DO-',LPAD(t.id,GREATEST(6,CHAR_LENGTH(t.id)),'0')),'sales_invoice_id',d.id,'sales_invoice_number',d.number,'date',CAST(t.date AS CHAR),'driver_name',t.driver_name,'address',t.address,'note',t.note,'cancellation_note',t.cancellation_note,'status',t.status)",
            search: vec!["CONCAT('DO-',LPAD(t.id,GREATEST(6,CHAR_LENGTH(t.id)),'0'))",
                "d.number",
                "t.driver_name",
                "t.address"],
            status: "t.status",
            table: "delivery_orders"},
        "invoice-approvals" => Spec {
            source: "invoice_void_requests t JOIN documents d ON d.id=t.invoice_id AND d.deleted_at IS NULL JOIN users u ON u.id=t.requested_by LEFT JOIN users reviewer ON reviewer.id=t.reviewed_by".into(),
            date: "d.date",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'invoice_id',d.id,'invoice_number',d.number,'date',CAST(d.date AS CHAR),'requested_by',u.username,'reviewed_by',reviewer.username,'reason',t.reason,'status',t.status)",
            search: vec!["d.number",
                "u.username",
                "t.reason"],
            status: "t.status",
            table: "invoice_void_requests"},
        "delivery-cancellations" => Spec {
            source: "delivery_cancellation_requests t JOIN delivery_orders o ON o.id=t.delivery_order_id AND o.deleted_at IS NULL JOIN documents d ON d.id=o.sales_invoice_id AND d.deleted_at IS NULL JOIN users u ON u.id=t.requested_by LEFT JOIN users reviewer ON reviewer.id=t.reviewed_by".into(),
            date: "o.date",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'delivery_order_id',o.id,'delivery_order_number',CONCAT('DO-',LPAD(o.id,GREATEST(6,CHAR_LENGTH(o.id)),'0')),'invoice_number',d.number,'date',CAST(o.date AS CHAR),'requested_by',u.username,'reviewed_by',reviewer.username,'reason',t.reason,'status',t.status)",
            search: vec!["CONCAT('DO-',LPAD(o.id,GREATEST(6,CHAR_LENGTH(o.id)),'0'))",
                "d.number",
                "u.username",
                "t.reason"],
            status: "t.status",
            table: "delivery_cancellation_requests"},
        "roles" => Spec {
            source: "roles t".into(),
            date: "t.created_at",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'name',t.name,'date',DATE_FORMAT(t.created_at,'%Y-%m-%d'))",
            search: vec!["t.name"],
            status: "",
            table: "roles"},
        "users" => Spec {
            source: "users t JOIN roles r ON r.id=t.role_id AND r.deleted_at IS NULL".into(),
            date: "t.created_at",
            base: "1=1",
            payload: "JSON_OBJECT('id',t.id,'username',t.username,'role_id',t.role_id,'role_name',r.name,'active',t.active,'status',IF(t.active,'Active','Inactive'),'date',DATE_FORMAT(t.created_at,'%Y-%m-%d'))",
            search: vec!["t.username",
                "r.name"],
            status: "IF(t.active,'Active','Inactive')",
            table: "users"},
        "forecast" => {
            let days = f.days.unwrap_or(30);
            let lead = f.lead_time.unwrap_or(7);
            if !(1..=365).contains(&days) || !(1..=365).contains(&lead) {
                return Err(bad("Forecast days and lead time must be 1-365"))}
            let source = format!("(SELECT p.*,COALESCE(s.sold,0)/30.0 daily_sales,p.remaining_quantity-COALESCE(s.sold,0)/30.0*{days} projected_stock,GREATEST(0,CEIL(COALESCE(s.sold,0)/30.0*({days}+{lead})+p.reorder_level-p.remaining_quantity)) suggested_order,CASE WHEN p.remaining_quantity=0 THEN 'Out of stock' WHEN p.remaining_quantity<=COALESCE(s.sold,0)/30.0*{lead} OR p.remaining_quantity<=p.reorder_level THEN 'Reorder now' ELSE 'Healthy' END risk FROM products p LEFT JOIN (SELECT l.product_id,SUM(l.quantity) sold FROM document_lines l JOIN documents d ON d.id=l.document_id WHERE d.deleted_at IS NULL AND l.deleted_at IS NULL AND d.kind='sales_invoice' AND d.status<>'Cancelled' AND d.date BETWEEN DATE_SUB(CURRENT_DATE,INTERVAL 29 DAY) AND CURRENT_DATE GROUP BY l.product_id) s ON s.product_id=p.id WHERE p.deleted_at IS NULL AND p.status='Active') t");
            Spec {
                source,
                date: "t.created_at",
                base: "1=1",
                payload: "JSON_OBJECT('id',t.id,'name',t.name,'date',DATE_FORMAT(t.created_at,'%Y-%m-%d'),'quantity',t.remaining_quantity,'remaining_quantity',t.remaining_quantity,'reorder_level',t.reorder_level,'daily_sales',t.daily_sales,'days_remaining',IF(t.daily_sales>0,t.remaining_quantity/t.daily_sales,NULL),'projected_stock',t.projected_stock,'suggested_order',t.suggested_order,'risk',t.risk)",
                search: vec!["t.name",
                    "t.description"],
                status: "t.risk",
                table: "products"}
        },
        _ => return Err(Error(StatusCode::NOT_FOUND,
        "List not found".into()))};

    Ok(s)
}
fn encode(c: &Cursor) -> Result<String> {
    Ok(serde_json::to_vec(c)
        .map_err(|_| bad("Invalid cursor"))?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn decode(s: &str) -> Result<Cursor> {
    if s.len() > 4096 || s.len() % 2 != 0 || !s.is_ascii() {
        return Err(bad("Invalid cursor"));
    }
    let bytes = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| bad("Invalid cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| bad("Invalid cursor"))
}
// Keep the cursor's ID bound on every page: later inserts (even backdated ones)
// must not enter an existing traversal. Date + ID resolves ties deterministically.
fn conditions(
    b: &mut QueryBuilder<'_, MySql>,
    s: &Spec,
    f: &Filters,
    bound: i64,
    payments_search: bool,
) {
    b.push(" WHERE ")
        .push(s.base)
        .push(" AND t.deleted_at IS NULL AND t.id<=")
        .push_bind(bound);
    if let Some(from) = &f.from {
        b.push(" AND ")
            .push(s.date)
            .push(">=")
            .push_bind(from.clone());
    }
    if let Some(to) = &f.to {
        let after = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .unwrap()
            .succ_opt()
            .unwrap()
            .to_string();
        b.push(" AND ").push(s.date).push("<").push_bind(after);
    }
    if let Some(status) = &f.status {
        if !s.status.is_empty() {
            b.push(" AND ")
                .push(s.status)
                .push("=")
                .push_bind(status.clone());
        }
    }
    if let Some(kind) = &f.kind {
        b.push(" AND t.kind=").push_bind(kind.clone());
    }
    if let Some(q) = &f.q {
        let needle = format!(
            "%{}%",
            q.replace('=', "==").replace('%', "=%").replace('_', "=_")
        );
        b.push(" AND (");
        for (i, column) in s.search.iter().enumerate() {
            if i > 0 {
                b.push(" OR ");
            }
            b.push(*column)
                .push(" LIKE ")
                .push_bind(needle.clone())
                .push(" ESCAPE '='");
        }
        if payments_search {
            b.push(" OR EXISTS(SELECT 1 FROM payment_allocations pa JOIN documents d ON d.id=pa.invoice_id WHERE pa.deleted_at IS NULL AND d.deleted_at IS NULL AND pa.payment_id=t.id AND d.number LIKE ").push_bind(needle).push(" ESCAPE '=')");
        }
        b.push(")");
    }
}
pub(crate) async fn list(
    State(a): State<App>,
    Path(resource): Path<String>,
    Query(mut f): Query<Filters>,
    Extension(c): Extension<Claims>,
) -> Result<Json<Value>> {
    let limit = f.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(bad("Page size must be 1-100"));
    }
    f.limit = Some(limit);
    for value in [&mut f.q, &mut f.from, &mut f.to, &mut f.status, &mut f.kind] {
        if let Some(v) = value {
            let t = v.trim().to_string();
            *value = if t.is_empty() { None } else { Some(t) };
        }
    }
    if f.q.as_ref().is_some_and(|q| q.len() > 200) {
        return Err(bad("Search must be at most 200 characters"));
    }
    for date in [&f.from, &f.to].into_iter().flatten() {
        let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| bad("Invalid date filter"))?;
        if date.len() != 10 || d.succ_opt().is_none() {
            return Err(bad("Invalid date filter"));
        }
    }
    if let (Some(from), Some(to)) = (&f.from, &f.to) {
        if from > to {
            return Err(bad("Start date must not be after end date"));
        }
    }
    if let Some(kind) = &f.kind {
        if !["payments", "documents"].contains(&resource.as_str())
            || !["purchase_order", "purchase_invoice", "sales_invoice"].contains(&kind.as_str())
            || (resource == "payments" && kind == "purchase_order")
        {
            return Err(bad("Invalid type filter"));
        }
    }
    let s = spec(&resource, &f)?;
    if f.status.is_some() && s.status.is_empty() {
        return Err(bad("This list has no status filter"));
    }
    let raw = f.cursor.take();
    let scope = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(c.sub, &resource, &f)).map_err(|_| bad("Invalid filters"))?
        )
    );
    let cursor = raw.as_deref().map(decode).transpose()?;
    if let Some(cursor) = &cursor {
        if cursor.v != 1
            || cursor.scope != scope
            || (!cursor.start && cursor.id <= 0)
            || cursor.id < 0
            || cursor.bound < 0
            || cursor.id > cursor.bound
            || chrono::NaiveDate::parse_from_str(&cursor.date, "%Y-%m-%d").is_err()
                && chrono::NaiveDateTime::parse_from_str(&cursor.date, "%Y-%m-%d %H:%M:%S%.f").is_err()
        {
            return Err(bad("Cursor does not match this list and its filters"));
        }
    }
    let bound = if let Some(cursor) = &cursor {
        cursor.bound
    } else {
        sqlx::query_scalar::<_, i64>(&format!("SELECT COALESCE(MAX(id),0) FROM {}", s.table))
            .fetch_one(&a.db)
            .await?
    };
    let invoice_access = allowed(&a, c.sub, "invoices.read").await?;
    let search_payments = resource == "payments" && invoice_access;
    let mut count = QueryBuilder::<MySql>::new(format!("SELECT COUNT(*) FROM {}", s.source));
    conditions(&mut count, &s, &f, bound, search_payments);
    let total: i64 = count.build_query_scalar().fetch_one(&a.db).await?;
    let mut query = QueryBuilder::<MySql>::new(format!(
        "SELECT CAST({} AS CHAR) payload,CAST({} AS CHAR) cursor_date,t.id cursor_id FROM {}",
        format!("JSON_SET({},'$.created_at',CAST(t.created_at AS CHAR),'$.updated_at',CAST(t.updated_at AS CHAR),'$.deleted_at',CAST(t.deleted_at AS CHAR))",s.payload), s.date, s.source
    ));
    conditions(&mut query, &s, &f, bound, search_payments);
    if let Some(cursor) = &cursor {
        if !cursor.start {
            query
                .push(" AND (")
                .push(s.date)
                .push("<")
                .push_bind(cursor.date.clone())
                .push(" OR (")
                .push(s.date)
                .push("=")
                .push_bind(cursor.date.clone())
                .push(" AND t.id<")
                .push_bind(cursor.id)
                .push("))");
        }
    }
    query
        .push(" ORDER BY ")
        .push(s.date)
        .push(" DESC,t.id DESC LIMIT ")
        .push_bind(limit + 1);
    let mut rows = query.build().fetch_all(&a.db).await?;
    let more = rows.len() > limit as usize;
    if more {
        rows.pop();
    }
    let start_cursor = encode(&Cursor {
        start: true,
        v: 1,
        scope: scope.clone(),
        date: "1970-01-01".into(),
        id: bound,
        bound,
    })?;
    let next = if more {
        let last = rows.last().unwrap();
        Some(encode(&Cursor {
            start: false,
            v: 1,
            scope,
            date: last.get("cursor_date"),
            id: last.get("cursor_id"),
            bound,
        })?)
    } else {
        None
    };
    let mut items = Vec::new();
    for row in rows {
        let raw: String = row.get("payload");
        let mut item: Value = serde_json::from_str(&raw).map_err(|_| bad("Invalid list record"))?;
        if resource == "roles" {
            let permissions: Vec<String> = sqlx::query_scalar(
                "SELECT permission FROM role_permissions WHERE deleted_at IS NULL AND role_id=? ORDER BY permission",
            )
            .bind(item["id"].as_i64())
            .fetch_all(&a.db)
            .await?;
            item["permissions"] = json!(permissions);
        }
        if resource == "payments" && !invoice_access {
            item["invoice_count"] = Value::Null;
        }
        if resource == "delivery-orders" && !invoice_access {
            item["sales_invoice_number"] = Value::Null;
        }
        items.push(item);
    }
    let summary = if resource == "forecast" {
        let mut query = QueryBuilder::<MySql>::new(format!(
            "SELECT CAST(COALESCE(SUM(t.risk<>'Healthy'),0) AS SIGNED) needs_replenishment,CAST(COALESCE(SUM(t.suggested_order),0) AS SIGNED) suggested_order FROM {}",
            s.source
        ));
        conditions(&mut query, &s, &f, bound, false);
        let row = query.build().fetch_one(&a.db).await?;
        json!({"needs_replenishment":row.get::<i64,_>("needs_replenishment"),"suggested_order":row.get::<i64,_>("suggested_order")})
    } else {
        Value::Null
    };
    Ok(Json(
        json!({"items":items,"next_cursor":next,"start_cursor":start_cursor,"has_more":more,"total":total,"limit":limit,"summary":summary}),
    ))
}
