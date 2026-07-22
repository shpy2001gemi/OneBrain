//! OneBrain Node — shared runtime for all UI interfaces.
//!
//! This crate contains the core node logic: OneBrainNode, networking,
//! configuration, peer management, seed client, and verification.
//! All interface projects (CLI, Web, Desktop, Mobile) depend on this.

pub mod anti_gaming_guard;
pub mod config;
pub mod display;
pub mod error;
pub mod mdns_discovery;
pub mod network;
pub mod node;
pub mod peer_manager;
pub mod peer_memory;
pub mod seed_client;
pub mod types;
pub mod upnp;
pub mod verifier_service;
pub mod vnext_companion;
pub mod vnext_config;
pub mod vnext_legacy_migration;
pub mod vnext_local_runtime;
pub mod vnext_m5_benchmark;
pub mod vnext_m6_model;
pub mod vnext_mixed_conformance;
pub mod vnext_performance_budgets;
pub mod vnext_reunion_canary;
pub mod vnext_reward_firewall;
pub mod vnext_scale_simulation;
#[cfg(test)]
pub mod vnext_security_suite;
pub mod vnext_status;
pub mod vnext_workflow_surface;

pub use config::NodeConfig;
pub use error::NodeError;
pub use network::{NetMessage, NodeEvent, PeerInfo};
pub use node::{EncodeStoreResult, OneBrainNode};
pub use vnext_companion::{
    companion_disclosure_scope, CompanionContext, CompanionDisclosureGrants, CompanionError,
    CompanionMaterializationCandidate, CompanionMultipathRequest, CompanionNetworkStatus,
    CompanionOpportunities, CompanionPlan, CompanionRecommendation, CompanionRecommendationKind,
    CompanionShareGrant, LocalCompanionPolicy, LocalKnowledgeCompanion,
    OptionalCompanionMultipathPlanner, RecommendationGateStatus, RecommendationGuard,
};
pub use vnext_config::{
    VNextFeature, VNextFeatureConfig, VNextFeatureConfigError, VNextFeatureFlags,
};
pub use vnext_local_runtime::{
    LocalCandidateInput, LocalCandidateOutcome, LocalMaterializationRequest, LocalRuntimeError,
    LocalVerticalSlice,
};
pub use vnext_m5_benchmark::{
    AssemblyBenchmarkCase, AssemblyMetrics, BenchmarkThresholds, BenchmarkVariant,
    BenchmarkedSideEffectKind, ConsentBoundaryCase, ConsentMetrics, GapFillCase, GapFillMetrics,
    LongTailExposureCase, LongTailMetrics, M5AblationComparison, M5BenchmarkError,
    M5BenchmarkInput, M5BenchmarkReport, M5BenchmarkRunner, M5GateVector, MappingValidityCase,
    MetricFraction, PrivacyLeakageProbe, PrivacyMetrics,
};
pub use vnext_m6_model::{run_m6_bounded_models, BoundedInvariantResult, M6BoundedModelReport};
pub use vnext_performance_budgets::{
    run_performance_budget_suite, PerformanceBudgetReport, PerformanceBudgetV1,
    PerformanceSuiteError, TimedMetric, PERFORMANCE_BUDGET_PROFILE,
};
pub use vnext_reunion_canary::{DeterministicReunionTrace, ReunionTraceEntry, ReunionTracePhase};
pub use vnext_reward_firewall::{
    execute_knowledge_operation, KnowledgeRewardFirewall, RewardConsumerError, RewardDrainReport,
    RewardEvidenceConsumer, RewardEvidenceKind, RewardEvidenceNotice, RewardFirewallConfigError,
    RewardFirewallPolicy, RewardObserveOutcome,
};
pub use vnext_workflow_surface::{
    workflow_stage_view, workflow_surface, WorkflowStage, WorkflowStageView,
};
