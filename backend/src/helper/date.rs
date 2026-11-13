use super::{Result, bad};
pub(crate) fn date_ok(s: &str) -> Result<()> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| bad("Use a valid YYYY-MM-DD date"))?;
    Ok(())
}
