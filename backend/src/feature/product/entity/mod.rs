use serde::{Deserialize, Serialize};
pub(crate) fn active() -> String {
    "Active".into()
}
pub(crate) fn ten() -> i64 {
    10
}
#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct Product {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) quantity: i64,
    pub(crate) remaining_quantity: i64,

    pub(crate) status: String,
    pub(crate) photo: Option<String>,

    pub(crate) description: String,

    pub(crate) price: i64,

    pub(crate) reorder_level: i64,
}
#[derive(Deserialize)]
pub(crate) struct ProductInput {
    #[serde(default)]
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) quantity: i64,
    #[serde(default)]
    pub(crate) remaining_quantity: Option<i64>,
    #[serde(default = "active")]
    pub(crate) status: String,
    pub(crate) photo: Option<String>,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) price: i64,
    #[serde(default = "ten")]
    pub(crate) reorder_level: i64,
}
