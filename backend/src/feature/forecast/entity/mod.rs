use serde::Deserialize;
pub(crate) fn thirty() -> i64 {
    30
}
pub(crate) fn seven() -> i64 {
    7
}
#[derive(Deserialize)]
pub(crate) struct ForecastQuery {
    #[serde(default = "thirty")]
    pub(crate) days: i64,
    #[serde(default = "seven")]
    pub(crate) lead_time: i64,
}
