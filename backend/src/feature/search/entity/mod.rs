use serde::Deserialize;
#[derive(Deserialize)]
pub(crate) struct Search {
    #[serde(default)]
    pub(crate) q: String,
}
