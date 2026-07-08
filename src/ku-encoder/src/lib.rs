//! # ku-encoder — AI-Assisted KU Encoding
//!
//! Bridges the AI runtime (ku-ai) to ku-core's CoreDna encoding infrastructure.
//!
//! ## Pipeline
//! ```text
//! Natural Language Text
//!     │ PromptBuilder
//!     ▼
//! LLM (tool-calling via ModelBackend)
//!     │ ToolCallResponse
//!     ▼
//! KuToolExecutor (ku-core)
//!     │ CoreDna instructions
//!     ▼
//! EncodingVerifier
//!     │ Verified wire bytes
//!     ▼
//! FallbackChain (retry / Tier 1)
//! ```
//!
//! ## Key Types
//!
//! - [`AiEncoder`] — Main entry point: takes text + AI backend → CoreDna wire bytes.
//! - [`EncodingResult`] — Output of encoding: wire bytes, confidence, stats.
//! - [`EncoderConfig`] — Temperature, retry count, confidence threshold.
//! - [`EncodingVerifier`] — Validates structural integrity and completeness.
//! - [`FallbackChain`] — Decides: accept, retry, or fall back to rule-based.
//! - [`BatchEncoder`] — Encodes multiple texts sequentially.
//! - [`EncodingLog`] — Debug log with JSON serialization.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ku_encoder::{AiEncoder, EncoderConfig, EncodingVerifier, FallbackChain};
//! use ku_ai::backend::MockBackend;
//! use ku_core::text_parser::default_dict;
//!
//! let backend = MockBackend::new().with_tool_response(tool_calls);
//! let encoder = AiEncoder::new(Box::new(backend), default_dict(), EncoderConfig::default());
//! let result = encoder.encode("Water boils at 100°C").await?;
//!
//! let verifier = EncodingVerifier::new();
//! let verification = verifier.verify(&result);
//! assert!(verification.passed);
//! ```

pub mod error;
pub mod prompt;
pub mod encoder;
pub mod verifier;
pub mod fallback;
pub mod batch;
pub mod log;

pub use error::EncoderError;
pub use encoder::{AiEncoder, EncodingResult, EncoderConfig};
pub use verifier::{EncodingVerifier, VerificationResult};
pub use fallback::{FallbackChain, EncodingDecision};
pub use batch::{BatchEncoder, BatchResult};
pub use log::{EncodingLog, LogEntry};
