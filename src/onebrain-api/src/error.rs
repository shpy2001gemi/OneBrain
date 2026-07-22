//! API error handling.
//!
//! Maps `NodeError` variants to appropriate HTTP status codes
//! and wraps them in the standard `ApiErrorResponse` envelope.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use onebrain_node::NodeError;

use crate::types::{ApiErrorResponse, ApiSuccess};

/// Wrapper around `NodeError` that implements `IntoResponse`.
pub struct ApiError(pub NodeError);

impl From<NodeError> for ApiError {
    fn from(e: NodeError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            NodeError::KuNotFound(_) => (StatusCode::NOT_FOUND, "KU_NOT_FOUND"),
            NodeError::AiUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "AI_UNAVAILABLE"),
            NodeError::RateLimit(_) => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMIT_EXCEEDED"),
            NodeError::QualityGate(_) => (StatusCode::BAD_REQUEST, "KU_LOW_QUALITY"),
            NodeError::IdentityExists(_) => (StatusCode::CONFLICT, "IDENTITY_EXISTS"),
            NodeError::InvalidPhrase(_) => (StatusCode::BAD_REQUEST, "RECOVERY_INVALID"),
            NodeError::InvalidArgument(_) => (StatusCode::BAD_REQUEST, "INVALID_ARGUMENT"),
            NodeError::Kql(_) => (StatusCode::BAD_REQUEST, "KQL_ERROR"),
            NodeError::Config(_) => (StatusCode::BAD_REQUEST, "CONFIG_ERROR"),
            NodeError::Backup(_) => (StatusCode::INTERNAL_SERVER_ERROR, "BACKUP_ERROR"),
            NodeError::Encoder(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ENCODING_FAILED"),
            NodeError::Mediator(_) => (StatusCode::INTERNAL_SERVER_ERROR, "MEDIATOR_ERROR"),
            NodeError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "STORAGE_ERROR"),
            NodeError::Network(_) => (StatusCode::SERVICE_UNAVAILABLE, "NETWORK_ERROR"),
            NodeError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO_ERROR"),
            NodeError::Pipeline(_) => (StatusCode::INTERNAL_SERVER_ERROR, "PIPELINE_ERROR"),
            NodeError::Ai(_) => (StatusCode::INTERNAL_SERVER_ERROR, "AI_ERROR"),
            NodeError::Timeout(_) => (StatusCode::REQUEST_TIMEOUT, "TIMEOUT"),
            NodeError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        };

        let body = ApiErrorResponse::new(code, self.0.to_string());
        (status, Json(body)).into_response()
    }
}

/// Convenience result type for API handlers.
pub type ApiResult<T> = Result<Json<ApiSuccess<T>>, ApiError>;
