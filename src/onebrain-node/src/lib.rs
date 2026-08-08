//! OneBrain Node — shared runtime for all UI interfaces.
//!
//! This crate contains the core node logic: OneBrainNode, networking,
//! configuration, peer management, seed client, and verification.
//! All interface projects (CLI, Web, Desktop, Mobile) depend on this.

pub mod anti_gaming_guard;
pub mod blob_authority;
pub mod concept_registry_runtime;
pub mod config;
pub mod dataset_path;
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
#[cfg(feature = "vnext-canary-harness")]
pub mod vnext_canary_operations;
pub mod vnext_companion;
pub mod vnext_config;
#[cfg(feature = "vnext-crash-harness")]
pub mod vnext_crash_harness;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_distributed_kql;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_distributed_pomv;
#[cfg(feature = "vnext-chaos-harness")]
pub mod vnext_fuzz_targets;
pub mod vnext_legacy_migration;
pub mod vnext_local_runtime;
pub mod vnext_m5_benchmark;
pub mod vnext_m6_model;
pub mod vnext_mixed_conformance;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_network_runtime;
pub mod vnext_observability;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_operational_compaction;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_outbox;
#[cfg(feature = "vnext-canary-harness")]
pub mod vnext_p5_operations;
pub mod vnext_performance_budgets;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_product_runtime;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_record_provenance;
pub mod vnext_reunion_canary;
pub mod vnext_reward_firewall;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_route_authority;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_runtime_rollout;
pub mod vnext_scale_simulation;
#[cfg(test)]
pub mod vnext_security_suite;
#[cfg(feature = "vnext-soak-harness")]
pub mod vnext_soak_release;
pub mod vnext_status;
pub mod vnext_validated_sink;
pub mod vnext_workflow_surface;

pub use blob_authority::{
    BlobAuthority, BlobAuthorityError, CanonicalBlobReferenceOracle, OsPendingUploadIdSource,
    PendingBlobUploadId, PendingBlobUploadStore, PendingOwnedBlobUpload, PendingUploadIdSource,
    UnavailableValidatedBlobReferenceSource, ValidatedBlobAuthoritySnapshot,
    ValidatedBlobReferenceSource, MAX_PENDING_BLOB_UPLOADS,
};
pub use concept_registry_runtime::{
    ConceptRegistryBackendKind, ConceptRegistryFailureKind, ConceptRegistryRuntimeState,
    ConceptRegistryStatus,
};
pub use config::{ConceptRegistryMode, NodeConfig};
pub use dataset_path::{
    BaseStorageOwnerId, BootstrapDatasetPathResolver, DatasetGenerationId, DatasetPathResolver,
};
pub use error::NodeError;
#[cfg(feature = "vnext-network-runtime")]
pub use ku_net::vnext_session::SessionIdentitySigner;
pub use network::{NetMessage, NodeEvent, PeerInfo};
pub use node::{EncodeStoreResult, OneBrainNode};
#[cfg(feature = "vnext-canary-harness")]
pub use vnext_canary_operations::{
    run_p5_canary_preflight, P5CanaryPreflightError, P5CanaryPreflightReport, P5_CANARY_NODE_COUNT,
    P5_CANARY_PREFLIGHT_PROFILE, P5_CANARY_RING_DELIVERIES, P5_CANARY_ROUTE_OBSERVATIONS,
};
pub use vnext_companion::{
    companion_disclosure_scope, CompanionContext, CompanionDisclosureGrants, CompanionError,
    CompanionMaterializationCandidate, CompanionMultipathRequest, CompanionNetworkStatus,
    CompanionOpportunities, CompanionPlan, CompanionRecommendation, CompanionRecommendationKind,
    CompanionShareGrant, LocalCompanionPolicy, LocalKnowledgeCompanion,
    OptionalCompanionMultipathPlanner, RecommendationGateStatus, RecommendationGuard,
};
pub use vnext_config::{
    VNextFeature, VNextFeatureConfig, VNextFeatureConfigError, VNextFeatureFlags,
    VNextNetworkPolicy, VNextRuntimeBudgets,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_distributed_kql::{
    DistributedKqlBudget, DistributedKqlError, DistributedKqlMatch, DistributedKqlReport,
    DistributedKqlRuntime,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_distributed_pomv::{
    ConfirmPublicUseEvidenceRequest, DistributedPomvError, DistributedPomvReport,
    DistributedPomvRuntime, DistributedUseEvidenceObservation, PreparePublicUseEvidenceRequest,
    PreparedPublicUseIntent, PublicUseEvidencePublication, PublicUseEvidencePublisher,
    PublicUseFlushReport, PublicUseIntentCid, PublicUsePublishOutcome,
    MAX_PUBLIC_USE_CONSENT_TTL_SECONDS,
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
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_network_runtime::{
    OutboundDeliveryReport, OutboundVNextSession, VNextNetworkRuntime, VNextNetworkRuntimeError,
    VNextNetworkRuntimeState, VNextNetworkRuntimeStatus,
};
pub use vnext_observability::{
    VNextHistogramSnapshot, VNextObservability, VNextObservabilitySnapshot, VNextOutcomeSnapshot,
    VNextPomvSnapshot, VNextReasonCode, VNextReasonCount, VNextReconciliationSnapshot,
    VNextRegistryTelemetryState, VNextResourceSnapshot, VNextRuntimeGaugeSnapshot,
    VNEXT_OBSERVABILITY_PROFILE_MAJOR,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_operational_compaction::{
    BoundedEvidenceKind, DerivedIndexLane, DerivedIndexRow, DerivedIndexSnapshot,
    EvidenceRecordOutcome, OperationalCompactionError, OperationalCompactionPolicy,
    OperationalCompactionStore, OperationalEvidenceStats, OverflowEvidence,
    MAX_DERIVED_SNAPSHOT_BYTES, MAX_DERIVED_SNAPSHOT_ROWS, MAX_OPERATIONAL_EVIDENCE_BYTES,
    MAX_OPERATIONAL_EVIDENCE_RECORDS, OPERATIONAL_COMPACTION_PROFILE_MAJOR,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_outbox::{
    OutboundAuditTombstone, OutboundCompactionReport, OutboundIntentState, OutboundOutbox,
    OutboundOutboxError, OutboundOutboxStats, OutboundTransferIntent, OutboxEnqueueOutcome,
};
#[cfg(feature = "vnext-canary-harness")]
pub use vnext_p5_operations::{
    build_operator_dashboard, run_p5_operations_preflight, P5BackupRestoreReport,
    P5DefaultOffRolloutReport, P5FaultDrillReport, P5OperationsError, P5OperationsPreflightReport,
    P5OperatorAction, P5OperatorDashboardReport, P5OperatorDashboardSnapshot, P5OperatorIncident,
    P5OperatorLaneSnapshot, P5RegistryHealth, P5RollbackReenableReport, P5SignerHealth,
    P5_OPERATIONS_PREFLIGHT_PROFILE, P5_PROTECTED_DURABLE_FILES,
};
pub use vnext_performance_budgets::{
    run_performance_budget_suite, PerformanceBudgetReport, PerformanceBudgetV1,
    PerformanceSuiteError, TimedMetric, PERFORMANCE_BUDGET_PROFILE,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_product_runtime::{
    VNextProductLaneStatus, VNextProductRuntime, VNextProductRuntimeDependencies,
    VNextProductRuntimeError, VNextProductRuntimeState, VNextProductRuntimeStatus,
    VNextProductServices, VNextProductSignerMode, VNextProductWorkerKind, VNextShutdownPhase,
    VNextStartupPhase, VNextStoragePressure, MAX_PRODUCT_BACKGROUND_WORKERS,
};
pub use vnext_reunion_canary::{DeterministicReunionTrace, ReunionTraceEntry, ReunionTracePhase};
pub use vnext_reward_firewall::{
    execute_knowledge_operation, KnowledgeRewardFirewall, RewardConsumerError, RewardDrainReport,
    RewardEvidenceConsumer, RewardEvidenceKind, RewardEvidenceNotice, RewardFirewallConfigError,
    RewardFirewallPolicy, RewardObserveOutcome,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_route_authority::{
    AuthenticatedRoute, AuthenticatedRouteDirectory, AuthenticatedRouteOrigin,
    AuthorityFrontierResolution, AuthorityResolverError, LocalPolicyRegistry,
    LocalPolicyRegistryError, LocalPolicyVersion, RouteDirectoryError,
};
#[cfg(feature = "vnext-network-runtime")]
pub use vnext_runtime_rollout::{
    VNextRuntimeGenerationLease, VNextRuntimeLane, VNextRuntimeLaneRequest,
    VNextRuntimeLaneSnapshot, VNextRuntimeRollout, VNextRuntimeRolloutError,
    VNextRuntimeRolloutSnapshot, VNEXT_RUNTIME_ROLLOUT_PROFILE_MAJOR,
};
#[cfg(feature = "vnext-soak-harness")]
pub use vnext_soak_release::{
    run_soak_release, GrowthMetric, IncrementalScanMetric, LatencyPercentiles,
    RuntimeSignalSnapshot, SoakProfile, SoakReleaseBudgets, SoakReleaseError, SoakReleaseReport,
    SoakRunConfig, SOAK_RELEASE_PROFILE,
};
pub use vnext_status::{NetworkRuntimeLifecycle, NetworkRuntimeStatusView};
pub use vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};
pub use vnext_workflow_surface::{
    workflow_stage_view, workflow_surface, WorkflowStage, WorkflowStageView,
};
