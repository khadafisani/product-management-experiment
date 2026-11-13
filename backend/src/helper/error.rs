use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
#[derive(Debug)]
pub(crate) struct Error(pub(crate) StatusCode, pub(crate) String);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        eprintln!("Database: {e}");
        match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                bad("A record with this name or number already exists")
            }
            _ => Self(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database operation failed".into(),
            ),
        }
    }
}
pub(crate) fn bad(s: &str) -> Error {
    Error(StatusCode::BAD_REQUEST, s.into())
}
pub(crate) type Result<T> = std::result::Result<T, Error>;
