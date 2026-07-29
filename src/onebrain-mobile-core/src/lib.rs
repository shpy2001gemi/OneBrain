//! Autonomous mobile runtime profile for OneBrain.
//!
//! MOB-02 deliberately contains no transport stack and no concrete LLM
//! backend. It owns bounded bootstrap operational state, activation grants,
//! process-generation fencing and a signed local KQL smoke.

mod activation;
mod archive;
mod bootstrap;
mod error;
mod facade;
mod local_kql;
mod profile;
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
    BootstrapStore, InstallationAuthorityRecord, PrivacyPolicyRecord, ProcessGenerationRecord,
    ProcessLifecycle, ProcessStart, RegistryChunkRecord, RegistryOperationRecord,
    SecurityHistoryRecord, TransferLandingRecord,
};
pub use error::MobileCoreError;
pub use facade::{MobileRuntimeFacade, MobileRuntimeSnapshot};
pub use local_kql::{run_signed_local_kql_smoke, LocalKqlSmoke};
pub use profile::{MobileFeatureFlags, ResourceBudgets, MOBILE_RUNTIME_PROFILE_VERSION};
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
