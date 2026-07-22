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

pub mod batch;
pub mod encoder;
pub mod error;
pub mod fallback;
pub mod log;
pub mod prompt;
pub mod verifier;

// v2 pipeline modules
pub mod analyzer;
pub mod builder;
pub mod concept_resolver;
pub mod extractor;
pub mod prescan;
pub mod splitter;
pub mod types;
pub mod vnext_affordance_extractor;
pub mod vnext_observation_intake;
pub mod vnext_receptor_encoder;

pub use batch::{BatchEncoder, BatchResult};
pub use encoder::{AiEncoder, EncoderConfig, EncodingResult};
pub use error::EncoderError;
pub use fallback::{EncodingDecision, FallbackChain};
pub use log::{EncodingLog, LogEntry};
pub use verifier::{EncodingVerifier, VerificationResult};

// v2 pipeline re-exports
pub use builder::KuBuilder;
pub use concept_resolver::{ConceptResolver, ResolutionWarning, ResolutionWarningType};
pub use extractor::SpoExtractor;
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
