//! Encoding error types.
//!
//! Defines all errors that can occur during AI-assisted KU encoding,
//! including AI backend failures, tool execution errors, verification
//! failures, and fallback scenarios.

use thiserror::Error;

/// Errors that can occur during the AI-assisted KU encoding pipeline.
#[derive(Debug, Error)]
pub enum EncoderError {
    /// The AI backend returned an error (network, inference, etc.).
    #[error("AI backend error: {0}")]
    AiBackend(#[from] ku_ai::AiError),

    /// A tool call executed by KuToolExecutor returned an error.
    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    /// The verification step failed — the encoded CoreDna did not pass checks.
    #[error("Encoding verification failed: {reason}")]
    VerificationFailed {
        /// Human-readable reason for the failure.
        reason: String,
        /// Confidence score at the time of failure (0.0-1.0).
        confidence: f32,
    },

    /// AI encoding produced low confidence; rule-based fallback is needed.
    #[error("Fallback to rule-based encoding required: {0}")]
    FallbackRequired(String),

    /// The maximum number of retry attempts was exceeded.
    #[error("Max retries exceeded ({attempts} attempts)")]
    MaxRetriesExceeded {
        /// Total number of attempts made.
        attempts: u32,
    },

    /// An error occurred while encoding/decoding CoreDna binary.
    #[error("CoreDna encoding error: {0}")]
    CoreDnaError(String),

    /// The AI model did not return any tool calls.
    #[error("No tool calls returned by AI model")]
    NoToolCalls,

    /// A file system I/O error occurred (e.g., saving/loading logs).
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    // --- v2 pipeline errors ---
    /// The AI extraction produced no triples.
    #[error("AI extraction produced no triples")]
    NoTriples,

    /// The AI response could not be parsed as JSON SPO triples.
    #[error("JSON parse failed: {0}")]
    JsonParseFailed(String),

    /// Pre-scanned anchors were modified by the AI model.
    #[error("Anchor verification failed: {0}")]
    AnchorVerificationFailed(String),

    /// The configured Concept Registry became unavailable or inconsistent.
    #[error("Concept Registry lookup failed: {0}")]
    ConceptRegistry(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_error_display() {
        let err = EncoderError::ToolExecution("add_triple failed".into());
        assert_eq!(err.to_string(), "Tool execution error: add_triple failed");
    }

    #[test]
    fn test_verification_failed_display() {
        let err = EncoderError::VerificationFailed {
            reason: "not enough instructions".into(),
            confidence: 0.3,
        };
        assert!(err.to_string().contains("not enough instructions"));
    }

    #[test]
    fn test_max_retries_display() {
        let err = EncoderError::MaxRetriesExceeded { attempts: 3 };
        assert_eq!(err.to_string(), "Max retries exceeded (3 attempts)");
    }

    #[test]
    fn test_no_tool_calls_display() {
        let err = EncoderError::NoToolCalls;
        assert_eq!(err.to_string(), "No tool calls returned by AI model");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = EncoderError::from(io_err);
        assert!(matches!(err, EncoderError::IoError(_)));
        assert!(err.to_string().contains("file missing"));
    }
}
