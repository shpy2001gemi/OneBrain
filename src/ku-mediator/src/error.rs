//! Mediator error types.
//!
//! Unified error type covering all failure modes of the Personal AI Mediator:
//! AI backend failures, encoding errors, intent classification, context overflow,
//! session management, profile operations, retrieval, and graph queries.

use thiserror::Error;

/// Unified error type for all Mediator operations.
#[derive(Debug, Error)]
pub enum MediatorError {
    /// The underlying AI backend returned an error.
    #[error("AI error: {0}")]
    Ai(#[from] ku_ai::AiError),

    /// KU encoding failed.
    #[error("Encoding error: {0}")]
    Encoding(#[from] ku_encoder::EncoderError),

    /// Intent classification could not determine the user's intent.
    #[error("Intent classification failed: {0}")]
    IntentError(String),

    /// The assembled context exceeds the available token budget.
    #[error("Context overflow: {used} tokens exceeds budget {budget}")]
    ContextOverflow { used: usize, budget: usize },

    /// Session management error (e.g., expired or invalid session).
    #[error("Session error: {0}")]
    SessionError(String),

    /// User profile loading, saving, or update error.
    #[error("Profile error: {0}")]
    ProfileError(String),

    /// Knowledge retrieval error (embedding search, keyword match, etc.).
    #[error("Retrieval error: {0}")]
    RetrievalError(String),

    /// Graph query translation or execution error.
    #[error("Graph query error: {0}")]
    GraphQueryError(String),

    /// File system I/O error.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mediator_error_display() {
        let err = MediatorError::IntentError("ambiguous input".to_string());
        assert_eq!(err.to_string(), "Intent classification failed: ambiguous input");
    }

    #[test]
    fn test_context_overflow_display() {
        let err = MediatorError::ContextOverflow { used: 10000, budget: 8000 };
        assert!(err.to_string().contains("10000"));
        assert!(err.to_string().contains("8000"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = MediatorError::from(io_err);
        assert!(matches!(err, MediatorError::IoError(_)));
        assert!(err.to_string().contains("file missing"));
    }

    #[test]
    fn test_all_variants_have_display() {
        let variants: Vec<MediatorError> = vec![
            MediatorError::IntentError("test".into()),
            MediatorError::ContextOverflow { used: 100, budget: 50 },
            MediatorError::SessionError("test".into()),
            MediatorError::ProfileError("test".into()),
            MediatorError::RetrievalError("test".into()),
            MediatorError::GraphQueryError("test".into()),
        ];
        for v in variants {
            assert!(!v.to_string().is_empty());
        }
    }
}
