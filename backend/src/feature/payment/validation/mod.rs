use crate::helper::{Result, bad};
pub(crate) fn allocate(amount: i64, balances: &[i64]) -> Result<Vec<i64>> {
    if amount <= 0 || balances.is_empty() || balances.iter().any(|b| *b <= 0) {
        return Err(bad("Select unpaid invoices and a positive payment"));
    }
    if balances.len() > 1 && amount <= balances[0] {
        return Err(bad(
            "A multi-invoice payment must exceed the first invoice's outstanding balance",
        ));
    }
    let mut left = amount;
    let mut out = vec![];
    for b in balances {
        if left == 0 {
            return Err(bad(
                "Payment does not reach every selected invoice; remove unused invoices",
            ));
        }
        let n = left.min(*b);
        out.push(n);
        left -= n;
    }
    if left > 0 {
        return Err(bad("Payment exceeds selected outstanding balances"));
    }
    Ok(out)
}

pub(crate) fn validate(p: &super::entity::PaymentInput) -> Result<()> {
    use crate::helper::date::date_ok;
    use std::collections::HashSet;
    date_ok(&p.date)?;
    if p.reference.trim().is_empty()
        || p.reference.len() > 100
        || !["purchase_invoice", "sales_invoice"].contains(&p.kind.as_str())
        || p.invoice_ids.len() > 200
    {
        return Err(bad("Invalid payment reference or type"));
    }
    let unique: HashSet<_> = p.invoice_ids.iter().collect();
    if unique.len() != p.invoice_ids.len() {
        return Err(bad("Duplicate invoices"));
    }
    Ok(())
}
