use serde::Deserialize;
#[derive(Deserialize)]
pub(crate) struct PaymentInput {
    pub(crate) reference: String,
    pub(crate) date: String,
    pub(crate) amount: i64,
    pub(crate) invoice_ids: Vec<i64>,
    pub(crate) kind: String,
}
