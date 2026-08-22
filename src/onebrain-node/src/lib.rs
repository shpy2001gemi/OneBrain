//! OneBrain Node — shared runtime for all UI interfaces.
//!
//! This crate contains the core node logic: OneBrainNode, networking,
//! configuration, peer management, seed client, and verification.
//! All interface projects (CLI, Web, Desktop, Mobile) depend on this.

#[cfg(all(feature = "legacy-read-compat", not(feature = "base-v1")))]
compile_error!("legacy-read-compat requires base-v1");

mod activation_journal;
pub mod anti_gaming_guard;
pub mod archive;
pub mod archive_capabilities;
pub mod base_operation_store;
pub mod base_runtime;
pub mod blob_authority;
pub mod canonical_exchange;
pub mod concept_registry_runtime;
pub mod config;
pub mod dataset_generation;
pub mod dataset_path;
mod dataset_root_lease;
pub mod derived_index;
pub mod derived_projection;
pub mod display;
pub mod error;
pub mod identity_recovery;
pub mod mdns_discovery;
pub mod network;
pub mod node;
pub mod peer_manager;
pub mod peer_memory;
pub mod seed_client;
pub mod signer_ports;
pub mod source_capture_transaction;
pub mod text;
pub mod types;
pub mod upnp;
pub mod verifier_service;
#[cfg(feature = "vnext-bootstrap-https")]
pub mod vnext_bootstrap_client;
#[cfg(feature = "vnext-canary-harness")]
pub mod vnext_canary_operations;
pub mod vnext_companion;
pub mod vnext_config;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_connection_planner;
#[cfg(feature = "vnext-crash-harness")]
pub mod vnext_crash_harness;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_distributed_kql;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_distributed_pomv;
#[cfg(feature = "vnext-chaos-harness")]
pub mod vnext_fuzz_targets;
pub mod vnext_legacy_migration;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_linux_candidate_gatherer;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_linux_network_epoch;
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
pub mod vnext_p5_disk_pressure;
#[cfg(feature = "vnext-production-canary-harness")]
pub mod vnext_p5_fault_proxy;
pub mod vnext_p5_linux_admin;
#[cfg(feature = "vnext-production-canary-harness")]
pub mod vnext_p5_multi_host;
#[cfg(feature = "vnext-production-canary-harness")]
pub mod vnext_p5_multi_host_v2;
#[cfg(feature = "vnext-canary-harness")]
pub mod vnext_p5_operations;
#[cfg(feature = "vnext-production-canary-harness")]
pub mod vnext_p5_recovery_ops_v2;
#[cfg(feature = "vnext-production-canary-harness")]
pub mod vnext_p5_signer_provider;
pub mod vnext_performance_budgets;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_product_runtime;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_reachability_manager;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_reachability_replay_store;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_record_provenance;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_rendezvous_publisher;
pub mod vnext_reunion_canary;
pub mod vnext_reward_firewall;
#[cfg(feature = "vnext-network-runtime")]
pub mod vnext_route_authority;
#[cfg(feature = "vnext-outbound-first")]
pub mod vnext_route_journal;
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

pub use activation_journal::{
    ActivationOperationContext, ActivationPhase, DatasetGenerationReceipt,
};
pub use archive::DatasetRestoreReceipt;
pub use archive_capabilities::{
    ArchiveCapabilityId, ArchiveCapabilityRegistry, ArchiveOperationReservationId,
    ArchiveProcessGeneration, ArchiveSecretHandle, BoundedArchiveChunk, ReadableArchiveSinkHandle,
    SealedArchiveSourceHandle, WritableArchiveSinkHandle, WritableArchiveSourceHandle,
};
pub use base_operation_store::{
    BaseOperationReceiptV1, BaseOperationStateV1, BaseOperationStore, PreparedBaseIntentV1,
    ProcessGenerationId, ProcessGenerationIdSource, ReconciliationResultV1,
};
pub use base_runtime::{
    compiled_base_runtime_config, BaseArchiveServiceFactory, BaseCloseReceiptV1,
    BaseDrainReceiptV1, BaseEventBatchV1, BaseEventV1, BaseHostAuthorizer,
    BaseLocalOperationAdapter, BaseManagementCloseReceiptV1, BaseManagementGrant,
    BaseManagementResponseV1, BaseManagementScope, BaseManagementServices, BaseNegotiationRequest,
    BaseNegotiationResponse, BaseResponseV1, BaseRuntime, BaseRuntimeConfig, BaseRuntimeLifecycle,
    BaseServiceError, BaseServiceErrorCode, BaseServices, BaseStatusV1, DenyAllBaseHostAuthorizer,
    UnavailableBaseLocalOperationAdapter,
};
pub use blob_authority::{
    BlobAuthority, BlobAuthorityError, CanonicalBlobReferenceOracle, OsPendingUploadIdSource,
    PendingBlobUploadId, PendingBlobUploadStore, PendingOwnedBlobUpload, PendingUploadIdSource,
    UnavailableValidatedBlobReferenceSource, ValidatedBlobAuthoritySnapshot,
    ValidatedBlobReferenceSource, MAX_PENDING_BLOB_UPLOADS,
};
pub use canonical_exchange::{
    read_canonical_exchange, write_canonical_exchange, BaseExchangeEntryV1, ExchangeError,
    ExchangeReceipt, CANONICAL_EXCHANGE_MAGIC,
};
pub use concept_registry_runtime::{
    ConceptRegistryBackendKind, ConceptRegistryFailureKind, ConceptRegistryRuntimeState,
    ConceptRegistryStatus,
};
pub use config::{ConceptRegistryMode, NodeConfig};
pub use dataset_generation::{
    ActivationReadyGeneration, DatasetGenerationStore, RestoreError, RestoreOperationBinding,
    StagedDatasetGeneration,
};
pub use dataset_path::{
    ActiveDatasetPathResolver, BaseStorageOwnerId, BootstrapDatasetPathResolver,
    DatasetGenerationId, DatasetPathResolver,
};
pub use derived_index::{
    AcceptedRecordScan, DerivedIndexError, DerivedIndexOpenState, DerivedIndexReaderLease,
    RedbAcceptedRecordScan, VNextDerivedIndexManager, VNextIndexParityReport,
    VNEXT_DERIVED_INDEX_PROFILE,
};
pub use derived_projection::{DerivedProjectionOpenState, RetrieverProjectionService};
pub use error::NodeError;
pub use identity_recovery::{
    evaluate_signer_recovery, recover_staged_identity, BoundedIdentityDomains,
    BoundedReprovisionRequirements, IdentityRecoveryError, IdentityRecoveryReceipt,
    SignerRecoveryPolicy, SignerReprovisionRequirement,
};
#[cfg(feature = "vnext-network-runtime")]
pub use ku_net::vnext_session::SessionIdentitySigner;
pub use network::{NetMessage, NodeEvent, PeerInfo};
pub use node::{BaseIntegrationReceipt, EncodeStoreResult, OneBrainNode};
pub use signer_ports::{
    ActorRootIdentity, ActorRootPublicKey, ActorRootSigner, ActorRootStatementV1,
    ExpectedSignerIdentity, FeedAuthorIdentity, FeedPublicKey, IdentityDomain,
    NodeTransportIdentity, SessionPublicKey, SignerCapability, SignerCapabilitySet, SignerError,
    SignerPossessionChallengeV1, SignerPossessionProof, SignerProvider, SignerProviderId,
    SignerProviderRegistry,
};
pub use source_capture_transaction::{
    EncryptedSourceCaptureIntentV1, SourceCaptureError, SourceCaptureRecoveryState,
    SourceCaptureTransactionStore, MAX_SOURCE_CAPTURE_INTENTS, SOURCE_CAPTURE_BOUNDARY,
};
pub use text::truncate_preview;
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
#[cfg(feature = "vnext-production-canary-harness")]
pub use vnext_p5_fault_proxy::{
    P5DeliveryBatch, P5FaultKind, P5FaultProxy, P5FaultProxyConfig, P5FaultProxyError,
    P5_MAX_DUPLICATE_COPIES, P5_MAX_FAULT_DELAY_MS, P5_MAX_PROXY_FRAME_BYTES,
    P5_MAX_REORDERED_FRAMES,
};
#[cfg(feature = "vnext-production-canary-harness")]
pub use vnext_p5_multi_host::{
    evaluate_host_claims, P5CandidateBindingV1, P5ChildReceiptPayloadV1, P5ChildReceiptV1,
    P5ControlCommandV1, P5ControlPayloadV1, P5ControlVerifier, P5DirectoryRootObserver,
    P5HostAgentConfig, P5HostClaimEvaluationV1, P5HostClaimV1, P5LinuxProcessResourceObserver,
    P5MultiHostAgent, P5MultiHostError, P5ResourceObservationV1, P5ResourceObserver,
    P5RootObserver, P5RootSetV1, P5SignedControlV1, P5_CHILD_RECEIPT_FORMAT, P5_CONTROL_FORMAT,
    P5_MAX_CLOCK_SKEW_MS, P5_MAX_COMMAND_LIFETIME_MS, P5_MAX_CONTROL_MESSAGE_BYTES,
    P5_MAX_QUIESCENCE_MS, P5_ORCHESTRATOR_FINGERPRINT, P5_ORCHESTRATOR_PUBLIC_KEY,
    P5_TRUST_POLICY_DIGEST,
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
