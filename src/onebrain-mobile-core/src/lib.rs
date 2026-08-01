//! Autonomous mobile runtime profile for OneBrain.
//!
//! MOB-05A deliberately contains no transport stack and no concrete LLM
//! backend. It owns bounded bootstrap operational state, activation grants,
//! process-generation fencing, signed Registry trust/admission and a signed
//! local KQL smoke.

mod activation;
mod archive;
mod bootstrap;
mod draft;
mod error;
mod facade;
mod local_kql;
mod media_staging;
mod profile;
mod registry_admission;
mod security;
mod services;

pub use activation::{
    ActivationArbiter, ActivationPhase, ExecutionGrant, ExecutionGrantKind, NetworkScope,
};
pub use archive::{
    create_encrypted_archive, create_encrypted_archive_file, inspect_encrypted_archive,
    inspect_encrypted_archive_file, open_encrypted_archive, open_encrypted_archive_file,
    EncryptedArchiveInspection, EncryptedArchivePayload, RecoveryKey, MOBILE_ARCHIVE_VERSION,
};
pub use bootstrap::{
    BootstrapStore, InstallationAuthorityRecord, OnboardingCursor, PrivacyPolicyRecord,
    ProcessGenerationRecord, ProcessLifecycle, ProcessStart, RegistryChunkRecord,
    RegistryOperationRecord, SecurityHistoryRecord, TransferLandingRecord,
};
pub use draft::{PrivateDraftKey, PrivateDraftStore, RawDraftReceipt, ShareSpoolSummary};
pub use error::MobileCoreError;
pub use facade::{MobileRuntimeFacade, MobileRuntimeSnapshot};
pub use local_kql::{run_signed_local_kql_smoke, LocalKqlSmoke};
pub use media_staging::{
    MediaStageReceipt, MediaStageState, MediaStagingKey, MediaStagingStore, OwnedMediaSummary,
};
pub use profile::{MobileFeatureFlags, ResourceBudgets, MOBILE_RUNTIME_PROFILE_VERSION};
pub use registry_admission::{
    RegistryArtifact, RegistryBootstrapFloor, RegistryCapacityPlan, RegistryChannelHighWater,
    RegistryChunk, RegistryLimitedReceipt, RegistryNetworkPolicy, RegistryOperationState,
    RegistryReleaseCatalogRecord, RegistryReleaseHighWater, RegistryReleaseState,
    RegistryRuntimeRange, RegistryTrustKey, RegistryTrustProfile, RegistryWaitingReason,
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
