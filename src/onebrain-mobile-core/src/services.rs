use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Instant,
};

use crate::{BootstrapStore, MobileCoreError};

pub trait ClockService: Send + Sync {
    fn monotonic_millis(&self) -> u64;
}

pub trait SignerService: Send + Sync {
    fn is_available(&self) -> bool;
}

pub trait LlmService: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn is_available(&self) -> bool;
}

pub trait ConnectivityService: Send + Sync {
    fn is_online(&self) -> bool;
}

pub trait SchedulerService: Send + Sync {
    fn background_execution_available(&self) -> bool;
}

pub trait TelemetryService: Send + Sync {
    fn record(&self, event: &'static str);
}

pub trait RuntimePathService: Send + Sync {
    fn bootstrap_database_path(&self) -> PathBuf;
    fn private_vault_database_path(&self) -> PathBuf;
}

pub trait BootstrapStorageService: Send + Sync {
    fn open(&self, path: &Path) -> Result<BootstrapStore, MobileCoreError>;
}

pub struct RuntimeServices {
    pub clock: Arc<dyn ClockService>,
    pub signer: Arc<dyn SignerService>,
    pub llm: Arc<dyn LlmService>,
    pub connectivity: Arc<dyn ConnectivityService>,
    pub scheduler: Arc<dyn SchedulerService>,
    pub telemetry: Arc<dyn TelemetryService>,
    pub paths: Arc<dyn RuntimePathService>,
    pub storage: Arc<dyn BootstrapStorageService>,
}

impl RuntimeServices {
    pub fn bootstrap_only(data_root: impl Into<PathBuf>) -> Self {
        Self {
            clock: Arc::new(ProcessMonotonicClock),
            signer: Arc::new(UnavailableSigner),
            llm: Arc::new(NoLlmProvider),
            connectivity: Arc::new(OfflineConnectivity),
            scheduler: Arc::new(NoBackgroundScheduler),
            telemetry: Arc::new(NoopTelemetry),
            paths: Arc::new(FixedRuntimePaths::new(data_root)),
            storage: Arc::new(RedbBootstrapStorage),
        }
    }
}

pub struct ProcessMonotonicClock;

impl ClockService for ProcessMonotonicClock {
    fn monotonic_millis(&self) -> u64 {
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        let elapsed = ORIGIN.get_or_init(Instant::now).elapsed().as_millis();
        u64::try_from(elapsed).unwrap_or(u64::MAX)
    }
}

pub struct UnavailableSigner;

impl SignerService for UnavailableSigner {
    fn is_available(&self) -> bool {
        false
    }
}

pub struct NoLlmProvider;

impl LlmService for NoLlmProvider {
    fn provider_id(&self) -> &'static str {
        "none"
    }

    fn is_available(&self) -> bool {
        false
    }
}

pub struct OfflineConnectivity;

impl ConnectivityService for OfflineConnectivity {
    fn is_online(&self) -> bool {
        false
    }
}

pub struct NoBackgroundScheduler;

impl SchedulerService for NoBackgroundScheduler {
    fn background_execution_available(&self) -> bool {
        false
    }
}

pub struct NoopTelemetry;

impl TelemetryService for NoopTelemetry {
    fn record(&self, _event: &'static str) {}
}

pub struct FixedRuntimePaths {
    root: PathBuf,
}

impl FixedRuntimePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl RuntimePathService for FixedRuntimePaths {
    fn bootstrap_database_path(&self) -> PathBuf {
        self.root.join("bootstrap.redb")
    }

    fn private_vault_database_path(&self) -> PathBuf {
        self.root.join("private-vault.redb")
    }
}

pub struct RedbBootstrapStorage;

impl BootstrapStorageService for RedbBootstrapStorage {
    fn open(&self, path: &Path) -> Result<BootstrapStore, MobileCoreError> {
        BootstrapStore::open(path)
    }
}
