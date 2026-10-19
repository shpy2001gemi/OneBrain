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

#[cfg(feature = "legacy")]
pub mod batch;
#[cfg(feature = "legacy")]
pub mod encoder;
pub mod error;
#[cfg(feature = "legacy")]
pub mod fallback;
#[cfg(feature = "legacy")]
pub mod log;
#[cfg(feature = "legacy")]
pub mod prompt;
#[cfg(feature = "legacy")]
pub mod verifier;

// v2 pipeline modules
#[cfg(feature = "legacy")]
pub mod analyzer;
#[cfg(feature = "legacy")]
pub mod builder;
#[cfg(feature = "legacy")]
pub mod concept_resolver;
pub mod extraction;
#[cfg(feature = "legacy")]
pub mod extractor;
#[cfg(feature = "legacy")]
pub mod prescan;
#[cfg(feature = "legacy")]
pub mod splitter;
#[cfg(feature = "legacy")]
pub mod types;
pub mod vnext_affordance_extractor;
pub mod vnext_observation_intake;
pub mod vnext_receptor_encoder;

#[cfg(feature = "legacy")]
pub use batch::{BatchEncoder, BatchResult};
#[cfg(feature = "legacy")]
pub use encoder::{AiEncoder, EncoderConfig, EncodingResult};
pub use error::EncoderError;
#[cfg(feature = "legacy")]
pub use fallback::{EncodingDecision, FallbackChain};
#[cfg(feature = "legacy")]
pub use log::{EncodingLog, LogEntry};
#[cfg(feature = "legacy")]
pub use verifier::{EncodingVerifier, VerificationResult};

// v2 pipeline re-exports
#[cfg(feature = "legacy")]
pub use builder::KuBuilder;
#[cfg(feature = "legacy")]
pub use concept_resolver::{ConceptResolver, ResolutionWarning, ResolutionWarningType};
#[cfg(feature = "legacy")]
pub use extractor::SpoExtractor;
#[cfg(feature = "legacy")]
pub use types::{AnalyzedTriple, Anchor, NotationType, ResolvedTriple, SpoTriple};
pub use vnext_affordance_extractor::{
    AffordanceEvidenceKind, AffordanceEvidenceSnapshot, AffordanceExtractionDraft,
    AffordanceExtractionError, AffordanceExtractionTrace, ExplicitAffordanceDraft,
    ExtractedAffordance, RuleBasedAffordanceExtractor, RULE_BASED_AFFORDANCE_EXTRACTOR_PROFILE,
};
pub use vnext_observation_intake::{
    LocalObservationAdapter, LocalObservationIntake, ObservationAuthorization,
    ObservationAuthorizationState, ObservationCapture, ObservationEncodingProposal,
    ObservationExtraction, ObservationIntakeError, ObservationIntakeOutcome,
    ObservationReceptorDraft, ObservationSourceRange, StoredSourceArtifactView,
};
pub use vnext_receptor_encoder::{
    ConstraintCoverage, EncodedReceptor, EncodingLimitation, IncompleteReceptorEncoding,
    ReceptorEncoder, ReceptorEncodingDraft, ReceptorEncodingError, ReceptorEncodingOutcome,
    ReceptorEncodingTrace, ReceptorOriginDraft, ReceptorOriginKind,
};
