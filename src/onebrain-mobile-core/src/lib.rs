//! Autonomous mobile runtime profile for OneBrain.
//!
//! MOB-02 deliberately contains no transport stack and no concrete LLM
//! backend. It owns bounded bootstrap operational state, activation grants,
//! process-generation fencing and a signed local KQL smoke.

mod activation;
mod bootstrap;
mod error;
mod facade;
mod local_kql;
mod profile;
mod services;

pub use activation::{
    ActivationArbiter, ActivationPhase, ExecutionGrant, ExecutionGrantKind, NetworkScope,
};
pub use bootstrap::{
    BootstrapStore, ProcessGenerationRecord, ProcessLifecycle, ProcessStart, RegistryChunkRecord,
    RegistryOperationRecord, TransferLandingRecord,
};
pub use error::MobileCoreError;
pub use facade::{MobileRuntimeFacade, MobileRuntimeSnapshot};
pub use local_kql::{run_signed_local_kql_smoke, LocalKqlSmoke};
pub use profile::{MobileFeatureFlags, ResourceBudgets, MOBILE_RUNTIME_PROFILE_VERSION};
pub use services::{
    BootstrapStorageService, ClockService, ConnectivityService, FixedRuntimePaths, LlmService,
    NoBackgroundScheduler, NoLlmProvider, NoopTelemetry, OfflineConnectivity,
    ProcessMonotonicClock, RedbBootstrapStorage, RuntimePathService, RuntimeServices,
    SchedulerService, SignerService, TelemetryService, UnavailableSigner,
};
