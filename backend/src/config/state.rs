#[derive(Clone)]
pub(crate) struct App {
    pub(crate) db: sqlx::MySqlPool,
    pub(crate) http: reqwest::Client,
    pub(crate) meili: String,
    pub(crate) key: String,
}
