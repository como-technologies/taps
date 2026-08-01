//! Application error types.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Application error type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("AI provider error: {0}")]
    Ai(String),

    #[error("Claude API error: {0}")]
    ClaudeApi(String),

    #[error("Ollama API error: {0}")]
    OllamaApi(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    /// Conditional-write conflict (ETag mismatch). Surfaced by storage
    /// methods that take an `expected_etag`; callers can choose to retry.
    #[error("Conflict: {0}")]
    Conflict(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Storage(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::Json(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            AppError::Ai(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::ClaudeApi(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::OllamaApi(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::ParseError(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
        };

        tracing::error!("Error: {}", message);
        (status, message).into_response()
    }
}

impl From<rig::completion::CompletionError> for AppError {
    fn from(e: rig::completion::CompletionError) -> Self {
        AppError::Ai(e.to_string())
    }
}

impl From<rig::completion::PromptError> for AppError {
    fn from(e: rig::completion::PromptError) -> Self {
        AppError::Ai(e.to_string())
    }
}

impl From<rig::model::ModelListingError> for AppError {
    fn from(e: rig::model::ModelListingError) -> Self {
        AppError::Ai(e.to_string())
    }
}
