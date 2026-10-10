use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("resource not found")]
    NotFound,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::Database(error) => {
                tracing::error!(?error, "database request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database request failed".into(),
                )
            }
            Self::Internal(message) => {
                tracing::error!(%message, "internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal application error".into(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
