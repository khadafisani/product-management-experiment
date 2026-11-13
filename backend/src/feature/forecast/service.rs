use super::entity::ForecastQuery;
use crate::{ config::App, helper::Result };
use serde_json::{ Value, json };
use sqlx::Row;
pub(crate) async fn forecast(a: App, q: ForecastQuery) -> Result<Value> {
    super::validation::validate(&q)?;
    let rows = sqlx
        ::query(
            "SELECT p.id,p.name,p.remaining_quantity quantity,p.reorder_level,COALESCE(SUM(CASE WHEN d.kind='sales_invoice' AND d.status<>'Cancelled' AND d.date BETWEEN DATE_SUB(CURRENT_DATE, INTERVAL 29 DAY) AND CURRENT_DATE THEN l.quantity ELSE 0 END),0) sold FROM products p LEFT JOIN document_lines l ON l.product_id=p.id LEFT JOIN documents d ON d.id=l.document_id WHERE p.status='Active' GROUP BY p.id ORDER BY p.remaining_quantity"
        )
        .fetch_all(&a.db).await?;
    Ok(
        json!(
            rows
                .iter()
                .map(|r| {
                    let stock = r.get::<i64, _>("quantity");
                    let sold = r.get::<sqlx::types::Decimal, _>("sold");
                    let rate = sold.to_string().parse::<f64>().unwrap_or(0.0) / 30.0;
                    let reorder = r.get::<i64, _>("reorder_level");
                    json!({"id":r.get::<i64,_>("id"),"name":r.get::<String,_>("name"),"quantity":stock,"remaining_quantity":stock,"daily_sales":rate,"days_remaining":if rate>0.0 {Some(stock as f64/rate)} else {None},"projected_stock":stock as f64-rate*q.days as f64,"suggested_order":((rate*(q.days+q.lead_time) as f64+reorder as f64-stock as f64).ceil() as i64).max(0),"risk":if stock==0 {"Out of stock"} else if stock as f64<=rate*q.lead_time as f64 || stock<=reorder {"Reorder now"} else {"Healthy"}})
                })
                .collect::<Vec<_>>()
        )
    )
}
