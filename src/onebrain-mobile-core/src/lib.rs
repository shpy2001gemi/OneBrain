//! Autonomous mobile runtime profile for OneBrain.
//!
//! Contains bounded operational state, activation grants, process-generation
//! fencing, signed Concept Registry admission & streaming A/B slot activation,
//! trait-injected multi-provider failover, shared `ARCH-10` private KU &
//! fidelity portfolio workflows, durable `LLM-0` tool orchestration, verified
//! media range staging, durable activity notifications, gated passive OBP
//! reunion matching, and dataset migration/redacted diagnostics.

mod activation;
pub mod ai_tools;
pub mod archive;
pub mod backup;
mod bootstrap;
mod draft;
mod error;
mod facade;
pub mod knowledge;
mod local_kql;
mod media_staging;
pub mod migrations;
pub mod notifications;
pub mod obp_reunion;
mod profile;
pub mod registry_activation;
mod registry_admission;
pub mod registry_source_plan;
mod security;
mod services;

pub use activation::{
    ActivationArbiter, ActivationPhase, EnergyPolicyEvaluation, ExecutionGrant, ExecutionGrantKind,
    LifecyclePressureKind, NetworkScope,
};
pub use ai_tools::{
    canonical_tool_catalog, model_release_signing_message, tool_catalog_digest,
    ActiveModelRouteRecord, AiJournalKey, AssistantThreadMessage, AssistantThreadRecord,
    CloudDisclosurePreview, CompletedLlmResponse, Llm0DeterministicAssistant, LlmStreamAssembler,
    MobileLlmProvider, ModelCatalogRecord, ModelReleaseManifest, ModelReleaseState, ModelRouteKind,
    NormalizedLlmStreamEvent, ToolCatalogEntry, ToolEffectClass, ToolExecutionReceiptView,
    ToolLifecycleState, ToolOrchestrator, ToolProposalView, ToolResultVisibility,
};
pub use archive::RecoveryKey;
pub use backup::{
    create_encrypted_backup_v2, export_reviewed_scoped_bundle, restore_encrypted_backup_v2,
    ArchivedKnowledgeUnitEntry, ArchivedRawDraftEntry, MobileBackupReceipt,
    MobileDatasetBackupPayloadV2, MobileRestoreReceipt, ScopedExportBundle, ScopedExportItem,
};
pub use bootstrap::{
    BootstrapStore, InstallationAuthorityRecord, OnboardingCursor, PrivacyPolicyRecord,
    ProcessGenerationRecord, ProcessLifecycle, ProcessStart, RegistryChunkRecord,
    RegistryChunkSourceTarget, RegistryChunkState, RegistryChunkWriteProgress,
    RegistryChunkWriteSession, RegistryChunkWriteStart, RegistryLandingProgress,
    RegistryOperationRecord, RegistrySourceKind, RegistryTransferPlatform,
    RegistryTransferScheduleRecord, RegistryTransferScheduleState, SecurityHistoryRecord,
    TransferLandingRecord, REGISTRY_NATIVE_BLOCK_MAX_BYTES,
};
pub use draft::{PrivateDraftKey, PrivateDraftStore, RawDraftReceipt, ShareSpoolSummary};
pub use error::MobileCoreError;
pub use facade::{MobileRuntimeFacade, MobileRuntimeSnapshot, RegistryInitPlan};
pub use knowledge::{
    EncodingDraftRecord, KnowledgeMetaKey, KuLocalOrganizationRecord, LocalNeighborhoodProjection,
    LocalReadOnlyKqlView, MobileFidelityStatusView, MobileKnowledgeEngine, MobileKuDetailView,
    MobileKuSummary, MobileRevisionEntry, NeighborhoodEdgeView, NeighborhoodNodeView,
    PaginatedKuPage, PreparedKuPreviewView, WorkflowStageView,
};
pub use local_kql::{run_signed_local_kql_smoke, LocalKqlSmoke};
pub use media_staging::{
    DerivedCacheGcReport, DerivedCacheRecord, MediaStageReceipt, MediaStageState, MediaStagingKey,
    MediaStagingStore, MediaVerifiedRangeView, OwnedMediaSummary, PieceLeaf, SignedMediaManifest,
};
pub use migrations::{
    build_redacted_diagnostics_report, DatasetGenerationRecord, DatasetRollbackMode,
    MigrationCoordinator, RedactedDiagnosticsReport, SchemaMetaRecord,
};
pub use notifications::{
    ActivityInboxEntry, ActivityInboxStore, NotificationChannelKind, NotificationIntentRecord,
    NotificationPolicyConfig, NotificationReceiptRecord, NotificationReceiptState,
    ReversibleNotificationAction,
};
pub use obp_reunion::{
    evaluate_feature_gate_status, FeatureGateStatusView, MatchAuthorStatus,
    MatchProposalDisposition, MatchTargetLifecycle, PassiveObpReunionEngine, PrivateMatchTarget,
    PublicCandidateAffordance, QuarantinedReunionProposal, ReunionCoverageClass, ReunionVaultKey,
};
pub use profile::{MobileFeatureFlags, ResourceBudgets, MOBILE_RUNTIME_PROFILE_VERSION};
pub use registry_activation::{
    activate_verified_registry_release, build_obr_and_sidecar_bytes,
    build_signed_obr_release_bundle, derive_registry_readiness, gc_retired_registry_releases,
    list_registry_concepts, recover_or_open_active_registry, rollback_to_previous_registry_release,
    run_registry_health_v1, ActivationReceiptStatus, DerivedRegistryReadiness,
    RegistryActivationReceiptRecord, RegistryActiveStateRecord, RegistryConceptView,
    RegistryGarbageCollectionReport, RegistryGeneration, SignedObrReleaseBundle,
    REGISTRY_HEALTH_SUITE_V1,
};
pub use registry_admission::{
    RegistryArtifact, RegistryBootstrapFloor, RegistryCapacityPlan, RegistryChannelHighWater,
    RegistryChunk, RegistryLimitedReceipt, RegistryNetworkPolicy, RegistryOperationState,
    RegistryReleaseCatalogRecord, RegistryReleaseHighWater, RegistryReleaseState,
    RegistryRuntimeRange, RegistryTrustKey, RegistryTrustProfile, RegistryWaitingReason,
};
pub use registry_source_plan::{
    evaluate_registry_resource_policy, execute_multi_provider_registry_transfer,
    MultiProviderTransferOutcome, ObservedNetworkClass, RegistryByteProvider,
    RegistryProviderFault, RegistryRemainingCapacityPlan, RegistrySourceDescriptor,
    RegistrySourceObservation, RegistrySourcePlan, ResourcePolicyDecision, ResourceSnapshot,
    ThermalLevel,
};
pub use security::{
    AppLockPolicy, DomainSignature, IdentityDomain, MobileIdentityPublic, SecureIdentitySession,
    SecurityBootstrapMaterial, SecuritySessionState, SECURITY_BOOTSTRAP_MATERIAL_BYTES,
};
pub use services::{
    BootstrapStorageService, ClockService, ConnectivityService, FixedRuntimePaths, LlmService,
    NoBackgroundScheduler, NoLlmProvider, NoopTelemetry, OfflineConnectivity,
    ProcessMonotonicClock, RedbBootstrapStorage, RuntimePathService, RuntimeServices,
    SchedulerService, SignerService, TelemetryService, UnavailableSigner,
};
