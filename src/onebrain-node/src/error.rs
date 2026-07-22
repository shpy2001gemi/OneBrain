//! Node-level error types.
//!
//! Wraps errors from all subsystems into a single unified `NodeError`.

use thiserror::Error;

/// Unified error type for the OneBrain node.
#[derive(Debug, Error)]
pub enum NodeError {
    /// AI backend error.
    #[error("AI error: {0}")]
    Ai(#[from] ku_ai::AiError),

    /// KU encoding error.
    #[error("Encoder error: {0}")]
    Encoder(#[from] ku_encoder::EncoderError),

    /// Mediator orchestration error.
    #[error("Mediator error: {0}")]
    Mediator(#[from] ku_mediator::MediatorError),

    /// Persistent storage error.
    #[error("Storage error: {0}")]
    Storage(String),

    /// Network / P2P error.
    #[error("Network error: {0}")]
    Network(String),

    /// Configuration error.
    #[error("Config error: {0}")]
    Config(String),

    /// File system I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Full pipeline error (catch-all for composed failures).
    #[error("Pipeline error: {0}")]
    Pipeline(String),

    /// KU not found in storage.
    #[error("KU not found: {0}")]
    KuNotFound(String),

    /// KQL syntax or execution error.
    #[error("KQL error: {0}")]
    Kql(String),

    /// AI service unavailable.
    #[error("AI unavailable: {0}")]
    AiUnavailable(String),

    /// Identity already exists.
    #[error("Identity already exists: {0}")]
    IdentityExists(String),

    /// Invalid BIP39 recovery phrase.
    #[error("Invalid recovery phrase: {0}")]
    InvalidPhrase(String),

    /// Backup/restore error.
    #[error("Backup error: {0}")]
    Backup(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Content quality too low.
    #[error("Quality gate failed: {0}")]
    QualityGate(String),

    /// Invalid argument.
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Operation timed out.
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Resource not found (drafts, etc.).
    #[error("Not found: {0}")]
    NotFound(String),
}
