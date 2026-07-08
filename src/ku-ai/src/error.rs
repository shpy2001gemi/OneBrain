//! # AI Layer Error Types
//!
//! Unified error type for the `ku-ai` crate covering backend communication,
//! model management, device detection, and inference failures.

use thiserror::Error;

/// Unified error type for all AI layer operations.
#[derive(Debug, Error)]
pub enum AiError {
    /// The requested backend (e.g. Ollama) is not reachable or not running.
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),

    /// An error occurred during model inference.
    #[error("inference error: {0}")]
    InferenceError(String),

    /// The requested model was not found in the registry or backend.
    #[error("model not found: {0}")]
    ModelNotFound(String),

    /// Configuration loading or validation failed.
    #[error("config error: {0}")]
    ConfigError(String),

    /// Device/hardware detection failed.
    #[error("device detection error: {0}")]
    DeviceDetectionError(String),

    /// Model download failed.
    #[error("download error: {0}")]
    DownloadError(String),

    /// Input or output validation failed.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Operation timed out after the specified number of seconds.
    #[error("operation timed out after {0}s")]
    Timeout(u64),

    /// Tool calling protocol error.
    #[error("tool calling error: {0}")]
    ToolCallingError(String),

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// HTTP request error.
    #[error("http error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// File system I/O error.
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Convenience type alias for AI layer results.
pub type AiResult<T> = Result<T, AiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_backend_unavailable() {
        let err = AiError::BackendUnavailable("ollama not running".to_string());
        assert_eq!(err.to_string(), "backend unavailable: ollama not running");
    }

    #[test]
    fn test_error_display_timeout() {
        let err = AiError::Timeout(30);
        assert_eq!(err.to_string(), "operation timed out after 30s");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let ai_err = AiError::from(io_err);
        assert!(matches!(ai_err, AiError::IoError(_)));
        assert!(ai_err.to_string().contains("file missing"));
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<String>("not valid json").unwrap_err();
        let ai_err = AiError::from(json_err);
        assert!(matches!(ai_err, AiError::JsonError(_)));
    }

    #[test]
    fn test_all_variants_have_display() {
        let variants: Vec<AiError> = vec![
            AiError::BackendUnavailable("test".into()),
            AiError::InferenceError("test".into()),
            AiError::ModelNotFound("test".into()),
            AiError::ConfigError("test".into()),
            AiError::DeviceDetectionError("test".into()),
            AiError::DownloadError("test".into()),
            AiError::ValidationError("test".into()),
            AiError::Timeout(10),
            AiError::ToolCallingError("test".into()),
        ];
        for v in variants {
            assert!(!v.to_string().is_empty());
        }
    }
}
