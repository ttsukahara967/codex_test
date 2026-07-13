use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("Authentication required")]
    Unauthorized,
    #[error("Task not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("Internal server error")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(serde_json::json!({ "message": self.to_string() })),
        )
            .into_response()
    }
}

pub(crate) fn database(error: sqlx::Error) -> AppError {
    tracing::error!(?error, "database error");
    AppError::Internal
}
