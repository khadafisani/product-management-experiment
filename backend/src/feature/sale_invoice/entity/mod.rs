pub(crate) use crate::feature::document::entity::{Document, DocumentInput};
#[cfg(test)]
#[derive(serde::Deserialize)]
pub(crate) struct CancelInput {
    pub(crate) reason: String,
}
