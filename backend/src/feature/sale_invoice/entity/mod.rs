pub(crate) use crate::feature::document::entity::{Document, DocumentInput};
#[derive(serde::Deserialize)]
pub(crate) struct CancelInput {
    pub(crate) reason: String,
}
