use super::entity::DocumentInput;
use crate::helper::{Result, bad};
pub(crate) fn validate(d: &DocumentInput) -> Result<()> {
    if d.kind != "sales_invoice" {
        return Err(bad("Incorrect document type"));
    }
    crate::feature::document::validation::validate_document(d)?;
    Ok(())
}
#[cfg(test)]
pub(crate) fn validate_cancellation(reason: &str) -> Result<()> {
    if reason.trim().is_empty() || reason.chars().count() > 500 {
        return Err(bad(
            "A cancellation reason of 1 to 500 characters is required",
        ));
    }
    Ok(())
}
