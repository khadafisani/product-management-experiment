use crate::helper::{Result, bad};
pub(crate) fn validate(q: &str) -> Result<()> {
    if q.len() > 2000 {
        return Err(bad("Search query is too long"));
    }
    Ok(())
}
