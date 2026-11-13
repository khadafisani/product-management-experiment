use super::entity::ProductInput;
use crate::helper::{Result, bad};
pub(crate) fn validate(p: &ProductInput) -> Result<()> {
    if p.name.trim().is_empty()
        || p.name.chars().count() > 200
        || p.quantity < 0
        || p.price < 0
        || p.reorder_level < 0
        || !["Active", "Inactive"].contains(&p.status.as_str())
        || p.remaining_quantity
            .is_some_and(|n| n < 0 || n > p.quantity)
    {
        return Err(bad(
            "Invalid product values; remaining quantity must be between zero and physical quantity",
        ));
    }
    Ok(())
}
