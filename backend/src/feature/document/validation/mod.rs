use super::entity::{DocumentInput, LineInput};
use crate::helper::{Result, bad, date::date_ok};
use std::collections::HashSet;
pub(crate) fn total(lines: &[LineInput]) -> Result<i64> {
    if lines.is_empty() || lines.len() > 200 {
        return Err(bad("Supply 1 to 200 line items"));
    }
    let mut seen = HashSet::new();
    let mut t = 0i64;
    for l in lines {
        let key = if l.product_id > 0 {
            format!("id:{}", l.product_id)
        } else {
            format!("name:{}", new_product_name(l)?.to_lowercase())
        };
        if l.product_id < 0 || l.quantity <= 0 || l.price < 0 || !seen.insert(key) {
            return Err(bad(
                "Each product must appear once, with positive quantity and nonnegative price",
            ));
        }
        t = t
            .checked_add(
                l.quantity
                    .checked_mul(l.price)
                    .ok_or_else(|| bad("Amount too large"))?,
            )
            .ok_or_else(|| bad("Amount too large"))?;
    }
    if t <= 0 {
        return Err(bad("Invoice total must be positive"));
    }
    Ok(t)
}
pub(crate) fn new_product_name(line: &LineInput) -> Result<&str> {
    let name = line.item_name.as_deref().unwrap_or("").trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(bad(
            "Select a product or enter a new product name (1 to 200 characters)",
        ));
    }
    Ok(name)
}
pub(crate) fn validate_document(d: &DocumentInput) -> Result<i64> {
    if !["purchase_order", "purchase_invoice", "sales_invoice"].contains(&d.kind.as_str())
        || d.number.trim().is_empty()
        || d.number.len() > 100
        || d.person.trim().is_empty()
        || d.person.len() > 200
    {
        return Err(bad("Document type, number and person are required"));
    }
    date_ok(&d.date)?;
    if d.kind != "purchase_invoice" && d.purchase_order_id.is_some() {
        return Err(bad("Only purchase invoices can link to a PO"));
    }
    total(&d.lines)
}
