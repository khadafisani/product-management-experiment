use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
pub(crate) struct LineInput {
    #[serde(default)]
    pub(crate) product_id: i64,
    pub(crate) item_name: Option<String>,
    pub(crate) quantity: i64,
    pub(crate) price: i64,
}
#[derive(Deserialize)]
pub(crate) struct DocumentInput {
    pub(crate) kind: String,
    pub(crate) number: String,
    pub(crate) date: String,
    pub(crate) person: String,
    pub(crate) purchase_order_id: Option<i64>,
    pub(crate) lines: Vec<LineInput>,
    #[serde(default)]
    pub(crate) confirm_new_products: bool,
}
#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct Document {
    #[sqlx(default)]
    pub(crate) created_at: Option<String>,
    #[sqlx(default)]
    pub(crate) updated_at: Option<String>,
    #[sqlx(default)]
    pub(crate) deleted_at: Option<String>,
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) number: String,
    pub(crate) date: String,
    pub(crate) person: String,
    pub(crate) purchase_order_id: Option<i64>,
    pub(crate) grand_total: i64,
    pub(crate) remaining_payment: i64,
    pub(crate) status: String,
}
#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct Line {
    #[sqlx(default)]
    pub(crate) created_at: Option<String>,
    #[sqlx(default)]
    pub(crate) updated_at: Option<String>,
    #[sqlx(default)]
    pub(crate) deleted_at: Option<String>,
    pub(crate) id: i64,
    pub(crate) product_id: i64,
    pub(crate) item_name: String,
    pub(crate) quantity: i64,
    pub(crate) realized_quantity: i64,
    pub(crate) delivered_quantity: i64,
    pub(crate) on_delivery_quantity: i64,
    pub(crate) price: i64,
    pub(crate) subtotal: i64,
    pub(crate) status: String,
}
pub(crate) const DOC_SELECT: &str = "SELECT id,kind,number,CAST(date AS CHAR) date,person,purchase_order_id,grand_total,remaining_payment,status,CAST(created_at AS CHAR) created_at,CAST(updated_at AS CHAR) updated_at,CAST(deleted_at AS CHAR) deleted_at FROM documents WHERE deleted_at IS NULL";
