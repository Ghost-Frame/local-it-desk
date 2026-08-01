//! HTTP-safe application errors.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Application-wide error type with deliberately non-sensitive client messages.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The requested resource does not exist.
    #[error("not found")]
    NotFound,
    /// The request is malformed or violates a business rule.
    #[error("bad request: {0}")]
    BadRequest(String),
    /// The request conflicts with current persisted state.
    #[error("conflict: {0}")]
    Conflict(String),
    /// Authentication is required or the supplied identity is invalid.
    #[error("unauthorized")]
    Unauthorized,
    /// The authenticated identity lacks permission for the operation.
    #[error("forbidden")]
    Forbidden,
    /// A public authentication endpoint has exceeded its bounded retry policy.
    #[error("too many requests")]
    TooManyRequests,
    /// The retained API boundary exists but its implementation belongs to a later plan.
    #[error("not implemented")]
    NotImplemented,
    /// The request body exceeds the configured limit.
    #[error("payload too large")]
    PayloadTooLarge,
    /// The submitted media type is not allowed.
    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),
    /// SQLite rejected an operation.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// A pooled SQLite interaction failed.
    #[error("database pool error: {0}")]
    Pool(#[from] deadpool_sqlite::InteractError),
    /// JSON serialization or deserialization failed.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// A filesystem operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// An unexpected internal invariant failed.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Convenience result alias for application operations.
pub type AppResult<T> = std::result::Result<T, AppError>;

/// Stable JSON shape returned for application errors.
#[derive(Serialize)]
struct ErrorBody {
    /// Human-readable error that never includes an internal failure detail.
    error: String,
}

/// Converts application failures into bounded HTTP responses.
impl IntoResponse for AppError {
    /// Maps an application error to a bounded status code and public JSON body.
    fn into_response(self) -> Response {
        let status = match &self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Database(_) | Self::Pool(_) | Self::Json(_) | Self::Io(_) | Self::Internal(_) => {
                tracing::error!(error = %self, "internal server error");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        let error = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "internal server error".to_string()
        } else {
            self.to_string()
        };
        (status, axum::Json(ErrorBody { error })).into_response()
    }
}
