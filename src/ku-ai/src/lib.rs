//! # ku-ai — OneBrain AI Layer
//!
//! Pluggable local LLM runtime with automatic device detection, model selection,
//! and a unified trait-based interface for chat, tool calling, structured output,
//! and embeddings.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐
//! │  ku-ai API  │  ← Traits: ModelBackend, EmbeddingProvider
//! ├─────────────┤
//! │  Backend    │  ← Ollama (default), Mock (testing)
//! ├─────────────┤
//! │  Registry   │  ← Curated model catalog + tier-aware selector
//! ├─────────────┤
//! │  Device     │  ← Hardware detection + memory monitoring
//! ├─────────────┤
//! │  Config     │  ← TOML configuration + tier-aware defaults
//! └─────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use ku_ai::device::DeviceProfile;
//! use ku_ai::config::AiConfig;
//! use ku_ai::backend::create_backend;
//!
//! let profile = DeviceProfile::detect();
//! let config = AiConfig::default_for_tier(profile.tier);
//! let backend = create_backend(&config).expect("backend creation");
//! ```

pub mod backend;
pub mod config;
pub mod device;
pub mod error;
pub mod registry;
pub mod traits;
pub mod types;
pub mod vnext_executor;
pub mod vnext_fidelity;
pub mod vnext_manifest;
pub mod vnext_model_recall;

// ─── Re-exports ─────────────────────────────────────────────────────────

// Error types
pub use error::{AiError, AiResult};

// Core types
pub use types::{
    BackendStatus, ChatMessage, ChatOrToolResponse, ChatResponse, FunctionDefinition,
    InferenceOptions, ModelInfo, Role, ToolCallResponse, ToolDefinition, UsageStats,
};

// Traits
pub use traits::{EmbeddingProvider, ModelBackend};

// Device detection
pub use device::{DeviceProfile, DeviceTier, GpuBackend, GpuInfo, MemoryMonitor, MemoryPressure};

// Configuration
pub use config::AiConfig;

// Backend
pub use backend::{create_backend, MockBackend, OllamaBackend};

// Registry
pub use registry::{ModelCatalog, ModelEntry, ModelRegistry, ModelSelector, ModelType};
pub use vnext_executor::{
    cognitive_input_commitment, cognitive_output_commitment, cognitive_task_commitment,
    CancellationToken, CognitiveExecutionError, CognitiveExecutionPolicy, CognitiveExecutionResult,
    CognitiveStep, CognitiveStepBudget, CognitiveStepRequest, CognitiveTask,
    CognitiveTaskReplayGuard, CognitiveTaskReplayOutcome, CognitiveTermination,
    TypedCapabilityBackend, TypedCognitiveExecutor,
};
pub use vnext_fidelity::{
    exact_fidelity_checks, AlternateArchiveOutcome, AlternateEncodingArchive,
    BlindAttemptPortfolio, BlindEncodingCoordinator, BlindEncodingRequest, BlindFidelityError,
    BlindSessionPhase, CandidateEncodingInspection, CompletedBlindAttestation,
    EncodingAttemptArtifact, FidelityCheckPlan,
};
pub use vnext_manifest::{
    output_commitment, CapabilityConformanceExecutor, CapabilityConformanceRunner,
    CapabilityConformanceVector, ConformanceBudget, ConformanceExecution, ConformanceReport,
    ConformanceStatus, ConformanceVectorResult, LocalManifestBuild, LocalManifestBuildInput,
    LocalManifestBuilder, ManifestBuildError, PublicImplementationSketch,
};
pub use vnext_model_recall::{
    CandidateRecallAdapter, ModelRecallError, ModelRecallEvidence, ModelRecallFirewall,
    ModelRecallPage, ModelRecallRequest, ModelRecallResult, ModelScoredCandidate,
    RecallCandidateSeed, RecallEvaluation, RecallOrigin, SymbolicCheck, SymbolicDisposition,
    SymbolicMappingAssessment, SymbolicMappingValidator, SymbolicValidationRequest,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_api_accessible() {
        // Verify key types are accessible through the public API
        let _msg = ChatMessage::user("hello");
        let _opts = InferenceOptions::default();
        let _config = AiConfig::default();
        let _profile = DeviceProfile::detect();
    }

    #[test]
    fn test_registry_loads_via_public_api() {
        let registry = ModelRegistry::load().expect("registry should load");
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_create_mock_backend() {
        let mock = MockBackend::new().with_chat_response("test");
        assert_eq!(mock.backend_name(), "mock");
    }

    #[test]
    fn test_device_detection_via_public_api() {
        let profile = DeviceProfile::detect();
        assert!(profile.total_ram_bytes > 0);
        let _tier = profile.tier;
        let _config = AiConfig::default_for_tier(profile.tier);
    }
}
