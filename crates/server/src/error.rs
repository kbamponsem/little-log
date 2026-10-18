//! Everything a handler can fail with, rendered as the JSON shape the frontend
//! already understands (`{"error": "…"}`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use little_log_core::CoreError;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    /// No session, or the password/email pair is wrong.
    Unauthorized(&'static str),
    /// Signed in, but the role cannot do this.
    Forbidden(&'static str),
    NotFound,
    BadRequest(String),
    /// Too many failed sign-ins from this address.
    TooManyRequests,
    Internal(CoreError),
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        ApiError::BadRequest(message.into())
    }
}

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::NotFound(_) => ApiError::NotFound,
            CoreError::Invalid(message) | CoreError::Data(message) => ApiError::BadRequest(message),
            other => ApiError::Internal(other),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message.to_string()),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message.to_string()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many attempts — try again in a minute".to_string(),
            ),
            ApiError::Internal(error) => {
                tracing::error!(?error, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "something went wrong on the server".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
