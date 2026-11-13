use super::entity::DocumentInput;
use crate::helper::{Result, bad};
pub(crate) fn validate(d: &DocumentInput) -> Result<()> {
    if d.kind != "purchase_order" {
        return Err(bad("Incorrect document type"));
    }
    crate::feature::document::validation::validate_document(d)?;
    Ok(())
}
