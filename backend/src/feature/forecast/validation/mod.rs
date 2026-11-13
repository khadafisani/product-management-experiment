use super::entity::ForecastQuery;
use crate::helper::{ Result, bad };
pub(crate) fn validate(q: &ForecastQuery) -> Result<()> {
    if !(1..=365).contains(&q.days) || !(1..=365).contains(&q.lead_time) {
        return Err(bad("Days and lead time must be between 1 and 365"));
    }
    Ok(())
}
