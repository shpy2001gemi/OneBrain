//! Node-owned aggregate and typed service façade for vNext product lanes.
//!
//! Product and API code receives [`VNextProductServices`], never references to
//! the network, KQL, publication, or PoMV runtimes that this aggregate owns.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::BTreeSet;
use std::future::Future;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use ku_core::foundation::{
    FeedEventSigner, NodeId, ObjectReference, SelectorCid, ValidatedFeedInception,
};
use ku_kql::vnext_private_need::{LocalNeedVaultKey, PrivateNeedBundle};
use ku_kql::vnext_proposal::ProposalId;
use ku_kql::vnext_standing_need::{StandingNeed, StandingNeedId, StandingNeedWriteOutcome};
use ku_net::vnext_session::SessionIdentitySigner;
use thiserror::Error;
use tokio::sync::{watch, Notify};
use tokio::task::JoinHandle;

use crate::dataset_path::{BaseStorageOwnerId, DatasetPathResolver};
use crate::vnext_config::{VNextFeature, VNextFeatureConfig, VNextRuntimeBudgets};
use crate::vnext_distributed_kql::{
    DistributedKqlBudget, DistributedKqlError, DistributedKqlReport, DistributedKqlRuntime,
};
use crate::vnext_distributed_pomv::{
    ConfirmPublicUseEvidenceRequest, DistributedPomvError, DistributedPomvReport,
    DistributedPomvRuntime, PreparePublicUseEvidenceRequest, PreparedPublicUseIntent,
    PublicUseEvidencePublication, PublicUseEvidencePublisher, PublicUseFlushReport,
    PublicUsePublicationRecord, PublicUsePublishOutcome,
};
use crate::vnext_network_runtime::{
    prepare_vnext_identity, prepare_vnext_identity_caller_owned, OutboundVNextSession,
    VNextNetworkRuntime, VNextNetworkRuntimeError, VNextNetworkRuntimeState,
    VNextNetworkRuntimeStatus, VNextNetworkStoragePaths,
};
use crate::vnext_observability::{
    VNextObservability, VNextObservabilitySnapshot, VNextReasonCode, VNextRegistryTelemetryState,
};
use crate::vnext_route_authority::{AuthenticatedRoute, LocalPolicyRegistry, LocalPolicyVersion};
use crate::vnext_runtime_rollout::{
    VNextRuntimeGenerationLease, VNextRuntimeLane, VNextRuntimeLaneRequest, VNextRuntimeRollout,
    VNextRuntimeRolloutError, VNextRuntimeRolloutSnapshot,
};

pub const MAX_PRODUCT_BACKGROUND_WORKERS: usize = 8;
#[cfg(test)]
const VNEXT_STARTUP_ARTIFACTS: &[&str] = &[
    "vnext_identity.key",
    "vnext_private_need_vault.redb",
    "vnext_distributed_kql.redb",
    "vnext_public_use_sender.redb",
    "vnext_distributed_pomv.redb",
    "vnext_verified.redb",
    "vnext_reconciliation.redb",
    "vnext_inventory.redb",
    "vnext_record_provenance.redb",
    "vnext_outbox.redb",
];

struct VNextProductStoragePaths {
    operational: PathBuf,
    rollout: PathBuf,
    identity: PathBuf,
    private_kql: PathBuf,
    private_pomv: PathBuf,
    network: VNextNetworkStoragePaths,
    allow_compatibility_identity_file: bool,
}

impl VNextProductStoragePaths {
    fn legacy(data_dir: &Path) -> Self {
        Self {
            operational: data_dir.to_path_buf(),
            rollout: data_dir.to_path_buf(),
            identity: data_dir.to_path_buf(),
            private_kql: data_dir.to_path_buf(),
            private_pomv: data_dir.to_path_buf(),
            network: VNextNetworkStoragePaths {
                admission_root: data_dir.to_path_buf(),
                canonical: data_dir.join("vnext_verified.redb"),
                reconciliation: data_dir.join("vnext_reconciliation.redb"),
                inventory: data_dir.join("vnext_inventory.redb"),
                provenance: data_dir.join("vnext_record_provenance.redb"),
                outbox: data_dir.join("vnext_outbox.redb"),
            },
            allow_compatibility_identity_file: true,
        }
    }

    fn from_resolver(resolver: &dyn DatasetPathResolver) -> Result<Self, VNextProductRuntimeError> {
        let owner = |id| {
            resolver
                .owner_path(id)
                .map_err(|error| VNextProductRuntimeError::Configuration(error.to_string()))
        };
        let canonical = owner(BaseStorageOwnerId::CANONICAL)?;
        let reconciliation = owner(BaseStorageOwnerId::RECONCILIATION)?;
        let inventory = owner(BaseStorageOwnerId::INVENTORY)?;
        let provenance = owner(BaseStorageOwnerId::PROVENANCE)?;
        let outbox = owner(BaseStorageOwnerId::OUTBOX)?;
        let optional_network = owner(BaseStorageOwnerId::OPTIONAL_NETWORK)?;
        Ok(Self {
            operational: owner(BaseStorageOwnerId::OPERATIONAL)?,
            rollout: owner(BaseStorageOwnerId::ROLLOUT)?,
            identity: owner(BaseStorageOwnerId::IDENTITY)?,
            private_kql: owner(BaseStorageOwnerId::PRIVATE_KQL)?,
            private_pomv: owner(BaseStorageOwnerId::PRIVATE_POMV)?,
            network: VNextNetworkStoragePaths {
                admission_root: optional_network,
                canonical: canonical.join("vnext_verified.redb"),
                reconciliation: reconciliation.join("vnext_reconciliation.redb"),
                inventory: inventory.join("vnext_inventory.redb"),
                provenance: provenance.join("vnext_record_provenance.redb"),
                outbox: outbox.join("vnext_outbox.redb"),
            },
            allow_compatibility_identity_file: false,
        })
    }

    fn startup_artifacts(&self) -> Vec<PathBuf> {
        vec![
            self.identity.join("vnext_identity.key"),
            self.private_kql.join("vnext_private_need_vault.redb"),
            self.private_kql.join("vnext_distributed_kql.redb"),
            self.private_kql.join("vnext_standing_needs.redb"),
            self.private_pomv.join("vnext_public_use_sender.redb"),
            self.private_pomv.join("vnext_distributed_pomv.redb"),
            self.network.canonical.clone(),
            self.network.reconciliation.clone(),
            self.network.inventory.clone(),
            self.network.provenance.clone(),
            self.network.outbox.clone(),
        ]
    }
}

/// Caller-owned dependencies that must be supplied before product runtime
/// startup. The vault key is consumed by the encrypted local store and is not
/// retained as inspectable configuration.
pub struct VNextProductRuntimeDependencies {
    private_need_vault_key: LocalNeedVaultKey,
    policies: LocalPolicyRegistry,
}

impl VNextProductRuntimeDependencies {
    pub fn new(private_need_vault_key: LocalNeedVaultKey, policies: LocalPolicyRegistry) -> Self {
        Self {
            private_need_vault_key,
            policies,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextProductRuntimeState {
    Running,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextStartupPhase {
    ConfigurationValidated,
    SignerAndVaultValidated,
    StoresOpened,
    AuthenticatedQuicStarted,
    PrivateNeedsRehydrated,
    PublicationOutboxDrained,
    WorkersStarted,
    Running,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextShutdownPhase {
    OperationsFenced,
    WorkersCancelled,
    SafeMetadataFlushed,
    NetworkStopped,
    StoresClosed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextProductWorkerKind {
    DistributedKql,
    PublicUsePublication,
    DistributedPomv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextProductSignerMode {
    CallerOwned,
    CompatibilityFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VNextProductLaneStatus {
    pub distributed_kql_one_hop: bool,
    pub public_use_evidence_publish: bool,
    pub distributed_pomv_view: bool,
}

impl VNextProductLaneStatus {
    fn from_rollout(snapshot: &VNextRuntimeRolloutSnapshot) -> Self {
        Self {
            distributed_kql_one_hop: snapshot.lane(VNextRuntimeLane::DistributedKql).enabled,
            public_use_evidence_publish: snapshot
                .lane(VNextRuntimeLane::PublicUseEvidencePublish)
                .enabled,
            distributed_pomv_view: snapshot.lane(VNextRuntimeLane::DistributedPomvView).enabled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextStoragePressure {
    Normal,
    SoftWatermark,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VNextProductRuntimeStatus {
    pub state: VNextProductRuntimeState,
    pub signer_mode: VNextProductSignerMode,
    pub network: VNextNetworkRuntimeStatus,
    pub authenticated_routes: usize,
    pub active_private_needs: usize,
    pub durable_matches: u64,
    pub pending_publications: u64,
    pub observability: VNextObservabilitySnapshot,
    pub policy_versions: Vec<u32>,
    pub lanes: VNextProductLaneStatus,
    pub rollout: VNextRuntimeRolloutSnapshot,
    pub budgets: VNextRuntimeBudgets,
    pub storage_bytes: u64,
    pub storage_pressure: VNextStoragePressure,
    pub active_product_workers: usize,
    pub max_product_workers: usize,
    pub cancellation_requested: bool,
    pub startup_trace: Vec<VNextStartupPhase>,
    pub shutdown_trace: Vec<VNextShutdownPhase>,
    pub rehydrated_private_needs: usize,
    pub startup_pending_publications: u64,
    pub worker_poll_ticks: u64,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub claims_network_completion: bool,
}

/// Sole owner of the first integrated vNext product runtime slice.
///
/// Raw subsystem references intentionally have no public accessor. All product
/// operations cross [`VNextProductServices`].
pub struct VNextProductRuntime {
    core: Option<Arc<VNextProductServiceCore>>,
    last_network_status: Arc<Mutex<VNextNetworkRuntimeStatus>>,
    local_addr: SocketAddr,
    workers: BoundedProductWorkers,
    state: VNextProductRuntimeState,
    shutdown_trace: Vec<VNextShutdownPhase>,
    rehydrated_private_needs: usize,
    startup_pending_publications: u64,
    startup_artifacts: Vec<PathBuf>,
    startup_data_dir_created: bool,
    data_dir: PathBuf,
}

struct VNextProductServiceCore {
    lifecycle: Mutex<VNextServiceLifecycle>,
    drained: Notify,
    network: Mutex<Option<Arc<VNextNetworkRuntime>>>,
    distributed_kql: OptionalKqlOwner,
    public_use: Mutex<Option<Arc<PublicUseEvidencePublisher>>>,
    distributed_pomv: Mutex<Option<Arc<DistributedPomvRuntime>>>,
    policy_versions: Vec<LocalPolicyVersion>,
    rollout: VNextRuntimeRollout,
    budgets: VNextRuntimeBudgets,
    storage: ProductStorageGuard,
    signer_mode: VNextProductSignerMode,
    startup_trace: Vec<VNextStartupPhase>,
    rehydrated_private_needs: usize,
    startup_pending_publications: u64,
    active_product_workers: usize,
    max_product_workers: usize,
    worker_cancellation: watch::Receiver<bool>,
    worker_poll_ticks: Arc<AtomicU64>,
    observability: Arc<VNextObservability>,
}

struct VNextServiceLifecycle {
    accepting: bool,
    in_flight: usize,
}

fn rollout_requested_lanes(config: &VNextFeatureConfig) -> VNextRuntimeLaneRequest {
    let network = config.enabled.object_event_v1 && config.enabled.obp_rp;
    VNextRuntimeLaneRequest {
        network,
        distributed_kql: network && config.enabled.distributed_kql_one_hop,
        public_use_evidence_publish: network && config.enabled.public_use_evidence_publish,
        distributed_pomv_view: network && config.enabled.distributed_pomv_view,
    }
}

fn rollout_configured_kills(config: &VNextFeatureConfig) -> VNextRuntimeLaneRequest {
    VNextRuntimeLaneRequest {
        network: config.kill_switches.obp_rp,
        distributed_kql: config.kill_switches.distributed_kql_one_hop,
        public_use_evidence_publish: config.kill_switches.public_use_evidence_publish,
        distributed_pomv_view: config.kill_switches.distributed_pomv_view,
    }
}

fn lanes_network_enabled(rollout: &VNextRuntimeRollout) -> Result<bool, VNextRuntimeRolloutError> {
    Ok(rollout.snapshot()?.lane(VNextRuntimeLane::Network).enabled)
}

struct OptionalKqlOwner {
    runtime: Mutex<Option<DistributedKqlRuntime>>,
}

struct KqlOwnerGuard<'a> {
    guard: MutexGuard<'a, Option<DistributedKqlRuntime>>,
}

impl Deref for KqlOwnerGuard<'_> {
    type Target = DistributedKqlRuntime;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_ref()
            .expect("lane presence checked before KQL guard construction")
    }
}

impl DerefMut for KqlOwnerGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_mut()
            .expect("lane presence checked before KQL guard construction")
    }
}

impl VNextProductRuntime {
    pub async fn start(
        data_dir: &Path,
        bind_addr: SocketAddr,
        config: &VNextFeatureConfig,
        dependencies: VNextProductRuntimeDependencies,
        identity_signer: Option<Arc<dyn SessionIdentitySigner>>,
    ) -> Result<Self, VNextProductRuntimeError> {
        Self::start_with_paths(
            VNextProductStoragePaths::legacy(data_dir),
            bind_addr,
            config,
            dependencies,
            identity_signer,
        )
        .await
    }

    pub async fn start_in_dataset(
        resolver: &dyn DatasetPathResolver,
        bind_addr: SocketAddr,
        config: &VNextFeatureConfig,
        dependencies: VNextProductRuntimeDependencies,
        identity_signer: Option<Arc<dyn SessionIdentitySigner>>,
    ) -> Result<Self, VNextProductRuntimeError> {
        Self::start_with_paths(
            VNextProductStoragePaths::from_resolver(resolver)?,
            bind_addr,
            config,
            dependencies,
            identity_signer,
        )
        .await
    }

    async fn start_with_paths(
        paths: VNextProductStoragePaths,
        bind_addr: SocketAddr,
        config: &VNextFeatureConfig,
        dependencies: VNextProductRuntimeDependencies,
        identity_signer: Option<Arc<dyn SessionIdentitySigner>>,
    ) -> Result<Self, VNextProductRuntimeError> {
        let mut startup_trace = Vec::with_capacity(8);
        config
            .validate()
            .map_err(|error| VNextProductRuntimeError::Configuration(error.to_string()))?;
        startup_trace.push(VNextStartupPhase::ConfigurationValidated);
        let requested_lanes = rollout_requested_lanes(config);
        let provisioned_lanes = VNextProductLaneStatus {
            distributed_kql_one_hop: requested_lanes.distributed_kql,
            public_use_evidence_publish: requested_lanes.public_use_evidence_publish,
            distributed_pomv_view: requested_lanes.distributed_pomv_view,
        };
        let mut artifact_guard =
            StartupArtifactGuard::new_candidates(paths.startup_artifacts(), &paths.operational)?;
        let rollout = VNextRuntimeRollout::open(
            &paths.rollout,
            requested_lanes,
            rollout_configured_kills(config),
        )?;
        let lanes = VNextProductLaneStatus::from_rollout(&rollout.snapshot()?);
        let budgets = config.runtime_budgets;
        let storage = ProductStorageGuard::new(&paths.operational, budgets);
        storage.ensure_writable()?;
        let VNextProductRuntimeDependencies {
            private_need_vault_key,
            policies,
        } = dependencies;
        let policy_versions = policies.versions();
        let signer_mode = if identity_signer.is_some() {
            VNextProductSignerMode::CallerOwned
        } else {
            VNextProductSignerMode::CompatibilityFile
        };
        // LocalNeedVaultKey is a fixed-size, caller-owned capability. Its
        // persisted ciphertext proof is checked when the unopened KQL owner is
        // rehydrated below; signer proof-of-possession is checked here before
        // any durable subsystem store is opened.
        let prepared_identity = if paths.allow_compatibility_identity_file {
            prepare_vnext_identity(&paths.identity, identity_signer)?
        } else {
            prepare_vnext_identity_caller_owned(identity_signer)?
        };
        startup_trace.push(VNextStartupPhase::SignerAndVaultValidated);

        // A never-requested lane has no owner. A provisioned lane stays open
        // behind its durable generation fence so it can be re-enabled without
        // recreating durable state.
        let mut distributed_kql = provisioned_lanes
            .distributed_kql_one_hop
            .then(|| {
                DistributedKqlRuntime::open_unhydrated(&paths.private_kql, private_need_vault_key)
            })
            .transpose()?;
        let public_use = provisioned_lanes
            .public_use_evidence_publish
            .then(|| PublicUseEvidencePublisher::open(&paths.private_pomv))
            .transpose()?
            .map(Arc::new);
        let distributed_pomv = provisioned_lanes
            .distributed_pomv_view
            .then(|| {
                DistributedPomvRuntime::open_with_limits(
                    &paths.private_pomv,
                    budgets.pomv_max_records,
                    budgets.pomv_max_view_records,
                    policies,
                )
            })
            .transpose()?;
        startup_trace.push(VNextStartupPhase::StoresOpened);

        let network = Arc::new(
            VNextNetworkRuntime::start_prepared_with_paths(
                &paths.network,
                bind_addr,
                config.network,
                budgets.storage_hard_watermark_bytes,
                prepared_identity,
                rollout.clone(),
            )
            .await?,
        );
        startup_trace.push(VNextStartupPhase::AuthenticatedQuicStarted);
        let last_network_status = network.status();
        let observability = network.observability();
        let local_addr = network.local_addr();

        let rehydrated_private_needs = distributed_kql
            .as_mut()
            .map(|kql| {
                kql.rehydrate_private_needs()
                    .map_err(VNextProductRuntimeError::from)
            })
            .transpose()?
            .unwrap_or_default();
        startup_trace.push(VNextStartupPhase::PrivateNeedsRehydrated);

        // Recover the logical publication outbox before scheduling retries.
        // Routes are session-derived, so startup records durable pending work;
        // the publication worker may retry it only after a route is available.
        let startup_pending_publications =
            if lanes.public_use_evidence_publish && lanes_network_enabled(&rollout)? {
                if let Some(public_use) = public_use.as_ref() {
                    match public_use.flush_pending(&network, budgets.publication_flush_batch) {
                        Ok(_) | Err(DistributedPomvError::AuthenticatedRouteUnavailable) => {}
                        Err(error) => return Err(error.into()),
                    }
                    public_use.pending_publication_count()?
                } else {
                    0
                }
            } else {
                0
            };
        startup_trace.push(VNextStartupPhase::PublicationOutboxDrained);

        let mut workers = BoundedProductWorkers::new(MAX_PRODUCT_BACKGROUND_WORKERS);
        workers.start_lane_workers(
            provisioned_lanes,
            budgets.worker_poll_interval_millis,
            public_use
                .as_ref()
                .map(|publisher| (Arc::clone(publisher), Arc::clone(&network))),
            budgets.publication_flush_batch,
            rollout.clone(),
        )?;
        startup_trace.push(VNextStartupPhase::WorkersStarted);
        startup_trace.push(VNextStartupPhase::Running);
        let startup_data_dir_created = !artifact_guard.data_dir_preexisting;
        let startup_artifacts = artifact_guard.commit();
        let last_network_status = Arc::new(Mutex::new(last_network_status));
        let core = Arc::new(VNextProductServiceCore {
            lifecycle: Mutex::new(VNextServiceLifecycle {
                accepting: true,
                in_flight: 0,
            }),
            drained: Notify::new(),
            network: Mutex::new(Some(network)),
            distributed_kql: OptionalKqlOwner {
                runtime: Mutex::new(distributed_kql),
            },
            public_use: Mutex::new(public_use),
            distributed_pomv: Mutex::new(distributed_pomv.map(Arc::new)),
            policy_versions,
            rollout,
            budgets,
            storage,
            signer_mode,
            startup_trace,
            rehydrated_private_needs,
            startup_pending_publications,
            active_product_workers: workers.len(),
            max_product_workers: workers.capacity(),
            worker_cancellation: workers.cancellation.subscribe(),
            worker_poll_ticks: Arc::clone(&workers.poll_ticks),
            observability,
        });

        Ok(Self {
            core: Some(core),
            last_network_status,
            local_addr,
            workers,
            state: VNextProductRuntimeState::Running,
            shutdown_trace: Vec::with_capacity(5),
            rehydrated_private_needs,
            startup_pending_publications,
            startup_artifacts,
            startup_data_dir_created,
            data_dir: paths.operational,
        })
    }

    pub fn services(&self) -> VNextProductServices {
        VNextProductServices {
            core: self.core.as_ref().map(Arc::downgrade).unwrap_or_default(),
            local_addr: self.local_addr,
            last_network_status: Arc::clone(&self.last_network_status),
        }
    }

    pub async fn shutdown(&mut self) {
        if self.state == VNextProductRuntimeState::Stopped {
            return;
        }
        self.state = VNextProductRuntimeState::Stopped;
        if let Some(core) = self.core.as_ref() {
            core.fence_operations();
        }
        self.shutdown_trace
            .push(VNextShutdownPhase::OperationsFenced);
        if let Some(core) = self.core.as_ref() {
            core.wait_until_drained().await;
        }
        self.workers.shutdown().await;
        self.shutdown_trace
            .push(VNextShutdownPhase::WorkersCancelled);
        self.flush_safe_pending_metadata();
        self.shutdown_trace
            .push(VNextShutdownPhase::SafeMetadataFlushed);
        let Some(core) = self.core.take() else {
            return;
        };
        if let Some(network) = core.take_network() {
            let mut last_network_status = network.status();
            if let Ok(mut network) = Arc::try_unwrap(network) {
                network.shutdown().await;
            }
            last_network_status.state = VNextNetworkRuntimeState::Stopped;
            *self
                .last_network_status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = last_network_status;
        }
        self.shutdown_trace.push(VNextShutdownPhase::NetworkStopped);
        core.close_stores();
        self.shutdown_trace.push(VNextShutdownPhase::StoresClosed);
    }

    /// Abort an otherwise successful product startup because a later
    /// node-owned startup phase failed (for example, the legacy TCP bind).
    /// Only artifacts that did not exist before this startup are removed.
    pub async fn rollback_startup(mut self) -> Result<(), VNextProductRuntimeError> {
        self.shutdown().await;
        let startup_artifacts = std::mem::take(&mut self.startup_artifacts);
        let remove_data_dir = self.startup_data_dir_created;
        let data_dir = self.data_dir.clone();
        drop(self);
        remove_startup_artifacts(startup_artifacts)?;
        if remove_data_dir {
            match std::fs::remove_dir(data_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                // The durable rollout decision is operator state, not a
                // partially initialized product artifact.
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn flush_safe_pending_metadata(&mut self) {
        let Some(core) = self.core.as_ref() else {
            return;
        };
        if let Ok(kql) = core.kql() {
            self.rehydrated_private_needs = kql.active_target_count();
        }
        if let Ok(public_use) = core.publisher() {
            if let Ok(pending) = public_use.pending_publication_count() {
                self.startup_pending_publications = pending;
            }
        }
        let _ = core.storage.used_bytes();
    }
}

impl Drop for VNextProductRuntime {
    fn drop(&mut self) {
        if let Some(core) = self.core.as_ref() {
            core.fence_operations();
        }
        self.workers.cancel_and_abort();
        self.state = VNextProductRuntimeState::Stopped;
    }
}

impl VNextProductServiceCore {
    fn acquire(
        self: &Arc<Self>,
        lane: Option<VNextRuntimeLane>,
    ) -> Result<VNextServiceLease, VNextProductRuntimeError> {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .map_err(|_| VNextProductRuntimeError::LifecycleLockPoisoned)?;
        if !lifecycle.accepting {
            return Err(VNextProductRuntimeError::Stopped);
        }
        let generation = lane.map(|lane| self.rollout.acquire(lane)).transpose()?;
        lifecycle.in_flight = lifecycle
            .in_flight
            .checked_add(1)
            .ok_or(VNextProductRuntimeError::InFlightOverflow)?;
        drop(lifecycle);
        Ok(VNextServiceLease {
            core: Arc::clone(self),
            _generation: generation,
        })
    }

    fn fence_operations(&self) {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        lifecycle.accepting = false;
    }

    async fn wait_until_drained(&self) {
        loop {
            let notified = self.drained.notified();
            let in_flight = self
                .lifecycle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .in_flight;
            if in_flight == 0 {
                return;
            }
            notified.await;
        }
    }

    fn network(&self) -> Result<Arc<VNextNetworkRuntime>, VNextProductRuntimeError> {
        self.network
            .lock()
            .map_err(|_| VNextProductRuntimeError::SubsystemLockPoisoned("network"))?
            .as_ref()
            .map(Arc::clone)
            .ok_or(VNextProductRuntimeError::Stopped)
    }

    fn kql(&self) -> Result<KqlOwnerGuard<'_>, VNextProductRuntimeError> {
        self.distributed_kql.lock()
    }

    fn publisher(&self) -> Result<Arc<PublicUseEvidencePublisher>, VNextProductRuntimeError> {
        self.public_use
            .lock()
            .map_err(|_| VNextProductRuntimeError::SubsystemLockPoisoned("publication"))?
            .as_ref()
            .map(Arc::clone)
            .ok_or(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::PublicUseEvidencePublish,
            ))
    }

    fn pomv(&self) -> Result<Arc<DistributedPomvRuntime>, VNextProductRuntimeError> {
        self.distributed_pomv
            .lock()
            .map_err(|_| VNextProductRuntimeError::SubsystemLockPoisoned("PoMV"))?
            .as_ref()
            .map(Arc::clone)
            .ok_or(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::DistributedPomvView,
            ))
    }

    fn take_network(&self) -> Option<Arc<VNextNetworkRuntime>> {
        self.network
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn close_stores(&self) {
        self.distributed_kql.take();
        self.public_use
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        self.distributed_pomv
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }

    fn ensure_storage_writable(&self) -> Result<(), VNextProductRuntimeError> {
        self.storage.ensure_writable()
    }

    fn ensure_kql_budget(
        &self,
        budget: DistributedKqlBudget,
    ) -> Result<(), VNextProductRuntimeError> {
        budget.validate()?;
        if budget.max_scan_records > self.budgets.kql_max_scan_records
            || budget.max_affordances > self.budgets.kql_max_affordances
            || budget.max_pairs > self.budgets.kql_max_pairs
            || budget.max_proposals > self.budgets.kql_max_proposals
        {
            Err(VNextProductRuntimeError::BudgetExceeded(
                VNextFeature::DistributedKqlOneHop,
            ))
        } else {
            Ok(())
        }
    }
}

impl OptionalKqlOwner {
    fn lock(&self) -> Result<KqlOwnerGuard<'_>, VNextProductRuntimeError> {
        let guard = self
            .runtime
            .lock()
            .map_err(|_| VNextProductRuntimeError::KqlLockPoisoned)?;
        if guard.is_none() {
            return Err(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::DistributedKqlOneHop,
            ));
        }
        Ok(KqlOwnerGuard { guard })
    }

    fn take(&self) {
        self.runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }
}

struct VNextServiceLease {
    core: Arc<VNextProductServiceCore>,
    _generation: Option<VNextRuntimeGenerationLease>,
}

impl Drop for VNextServiceLease {
    fn drop(&mut self) {
        let mut lifecycle = self
            .core
            .lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        debug_assert!(lifecycle.in_flight > 0);
        lifecycle.in_flight = lifecycle.in_flight.saturating_sub(1);
        let drained = lifecycle.in_flight == 0;
        drop(lifecycle);
        if drained {
            self.core.drained.notify_waiters();
        }
    }
}

/// Cloneable product-facing handle. It owns no subsystem and can therefore be
/// snapshotted under the aggregate node mutex, then used after that mutex has
/// been released.
#[derive(Clone)]
pub struct VNextProductServices {
    core: Weak<VNextProductServiceCore>,
    local_addr: SocketAddr,
    last_network_status: Arc<Mutex<VNextNetworkRuntimeStatus>>,
}

impl VNextProductServices {
    fn lease(&self) -> Result<VNextServiceLease, VNextProductRuntimeError> {
        self.core
            .upgrade()
            .ok_or(VNextProductRuntimeError::Stopped)?
            .acquire(None)
    }

    fn lane_lease(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextServiceLease, VNextProductRuntimeError> {
        self.core
            .upgrade()
            .ok_or(VNextProductRuntimeError::Stopped)?
            .acquire(Some(lane))
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn network_status(&self) -> VNextNetworkRuntimeStatus {
        if let Ok(lease) = self.lease() {
            if let Ok(network) = lease.core.network() {
                return network.status();
            }
        }
        self.last_network_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn observe_registry_state(&self, state: VNextRegistryTelemetryState) {
        if let Ok(lease) = self.lease() {
            lease.core.observability.observe_registry_state(state);
        }
    }

    pub fn status(&self) -> Result<VNextProductRuntimeStatus, VNextProductRuntimeError> {
        let lease = self.lease()?;
        let core = &lease.core;
        let rollout = core.rollout.snapshot()?;
        let lanes = VNextProductLaneStatus::from_rollout(&rollout);
        let (active_private_needs, durable_matches) = match core.kql() {
            Ok(kql) => (
                kql.active_target_count(),
                kql.durable_match_count()
                    .map_err(VNextProductRuntimeError::from)?,
            ),
            Err(VNextProductRuntimeError::LaneDisabled(_)) => (0, 0),
            Err(error) => return Err(error),
        };
        let pending_publications = match core.publisher() {
            Ok(publisher) => publisher.pending_publication_count()?,
            Err(VNextProductRuntimeError::LaneDisabled(_)) => 0,
            Err(error) => return Err(error),
        };
        let network = core.network()?;
        let storage_bytes = core.storage.used_bytes()?;
        let cancellation_requested = *core.worker_cancellation.borrow();
        let status = VNextProductRuntimeStatus {
            state: VNextProductRuntimeState::Running,
            signer_mode: core.signer_mode,
            network: network.status(),
            authenticated_routes: network.authenticated_route_count()?,
            active_private_needs,
            durable_matches,
            pending_publications,
            observability: core
                .observability
                .snapshot(VNextRegistryTelemetryState::Unknown),
            policy_versions: core
                .policy_versions
                .iter()
                .map(|version| version.get())
                .collect(),
            lanes,
            rollout,
            budgets: core.budgets,
            storage_bytes,
            storage_pressure: core.storage.pressure(storage_bytes),
            active_product_workers: core.active_product_workers,
            max_product_workers: core.max_product_workers,
            cancellation_requested,
            startup_trace: core.startup_trace.clone(),
            shutdown_trace: Vec::new(),
            rehydrated_private_needs: core.rehydrated_private_needs,
            startup_pending_publications: core.startup_pending_publications,
            worker_poll_ticks: core.worker_poll_ticks.load(Ordering::Relaxed),
            changes_wallet_state: false,
            changes_obt_state: false,
            claims_network_completion: false,
        };
        Ok(status)
    }

    /// Persist an emergency lane kill. Operations already admitted on the
    /// previous generation may drain; subsequent acquisitions fail closed.
    pub fn kill_runtime_lane(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeRolloutSnapshot, VNextProductRuntimeError> {
        let lease = self.lease()?;
        lease.core.rollout.kill(lane)?;
        Ok(lease.core.rollout.snapshot()?)
    }

    /// Explicit operator acknowledgement that advances the generation and
    /// re-enables one configured lane.
    pub fn reenable_runtime_lane(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeRolloutSnapshot, VNextProductRuntimeError> {
        let lease = self.lease()?;
        lease.core.rollout.reenable(lane)?;
        Ok(lease.core.rollout.snapshot()?)
    }

    /// Atomically fence network and all product lanes. No raw, journal,
    /// outbox, quarantine, wallet, or OBT data is removed.
    pub fn rollback_runtime(
        &self,
    ) -> Result<VNextRuntimeRolloutSnapshot, VNextProductRuntimeError> {
        let lease = self.lease()?;
        Ok(lease.core.rollout.rollback()?)
    }

    pub async fn connect_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::Network)?;
        lease
            .core
            .network()?
            .connect(addr)
            .await
            .map_err(Into::into)
    }

    pub fn authenticated_route(
        &self,
        peer: NodeId,
    ) -> Result<Option<AuthenticatedRoute>, VNextProductRuntimeError> {
        let lease = self.lease()?;
        lease
            .core
            .network()?
            .authenticated_route(peer)
            .map_err(Into::into)
    }

    pub fn register_private_need(
        &self,
        bundle: PrivateNeedBundle,
    ) -> Result<(StandingNeedId, StandingNeedWriteOutcome), VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::DistributedKql)?;
        lease.core.ensure_storage_writable()?;
        let result = lease
            .core
            .kql()?
            .register_private_need(bundle)
            .map_err(Into::into);
        result
    }

    pub fn standing_need(
        &self,
        id: StandingNeedId,
    ) -> Result<Option<StandingNeed>, VNextProductRuntimeError> {
        let lease = self.lease()?;
        let result = lease.core.kql()?.standing_need(id).map_err(Into::into);
        result
    }

    pub fn standing_needs(
        &self,
    ) -> Result<Vec<(StandingNeedId, StandingNeed)>, VNextProductRuntimeError> {
        let lease = self.lease()?;
        let result = lease.core.kql()?.standing_needs().map_err(Into::into);
        result
    }

    pub fn pause_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::DistributedKql)?;
        lease.core.ensure_storage_writable()?;
        let result = lease
            .core
            .kql()?
            .pause(id, expected_generation)
            .map_err(Into::into);
        result
    }

    pub fn resume_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::DistributedKql)?;
        lease.core.ensure_storage_writable()?;
        let result = lease
            .core
            .kql()?
            .resume(id, expected_generation)
            .map_err(Into::into);
        result
    }

    pub fn cancel_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::DistributedKql)?;
        lease.core.ensure_storage_writable()?;
        let result = lease
            .core
            .kql()?
            .cancel(id, expected_generation)
            .map_err(Into::into);
        result
    }

    pub fn retire_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::DistributedKql)?;
        lease.core.ensure_storage_writable()?;
        let result = lease
            .core
            .kql()?
            .retire(id, expected_generation)
            .map_err(Into::into);
        result
    }

    pub fn process_one_hop_affordance_delta(
        &self,
        selector: SelectorCid,
        budget: DistributedKqlBudget,
    ) -> Result<DistributedKqlReport, VNextProductRuntimeError> {
        let _network_lease = self.lane_lease(VNextRuntimeLane::Network)?;
        let lease = self.lane_lease(VNextRuntimeLane::DistributedKql)?;
        lease.core.ensure_kql_budget(budget)?;
        lease.core.ensure_storage_writable()?;
        let network = lease.core.network()?;
        let result = lease
            .core
            .kql()?
            .process_one_hop_affordance_delta(&network, selector, budget)
            .map_err(Into::into);
        if let Ok(report) = &result {
            lease.core.observability.observe_selector_coverage(
                report.coverage.assessed_frontier.len() as u64,
                report.coverage.continuation.is_some(),
            );
            lease.core.observability.record_count(
                VNextReasonCode::AcceptedNew,
                report.new_matches,
                0,
                report.scanned_public_affordances,
            );
            lease.core.observability.record_count(
                VNextReasonCode::Replayed,
                report
                    .replayed_matches
                    .saturating_add(report.duplicate_frontier_objects),
                0,
                0,
            );
            lease.core.observability.record_count(
                VNextReasonCode::QuarantinedInvalid,
                report.ignored_invalid_affordances,
                0,
                0,
            );
        } else {
            lease
                .core
                .observability
                .record(VNextReasonCode::JournalFailure, 0, 1);
            tracing::warn!(
                target: "onebrain::vnext::observability",
                reason_code = VNextReasonCode::JournalFailure.code(),
                operation = "distributed_kql_scan",
                "vNext product operation failed"
            );
        }
        result
    }

    pub fn prepare_public_use(
        &self,
        request: &PreparePublicUseEvidenceRequest,
        author: &ValidatedFeedInception,
    ) -> Result<PreparedPublicUseIntent, VNextProductRuntimeError> {
        let lease = self.lane_lease(VNextRuntimeLane::PublicUseEvidencePublish)?;
        lease.core.ensure_storage_writable()?;
        lease
            .core
            .publisher()?
            .prepare_public_use(request, author)
            .map_err(Into::into)
    }

    pub fn publish_confirmed_public_use(
        &self,
        request: &ConfirmPublicUseEvidenceRequest,
        author: &ValidatedFeedInception,
        signer: &dyn FeedEventSigner,
    ) -> Result<(PublicUseEvidencePublication, PublicUsePublishOutcome), VNextProductRuntimeError>
    {
        let lease = self.lane_lease(VNextRuntimeLane::PublicUseEvidencePublish)?;
        lease.core.ensure_storage_writable()?;
        lease
            .core
            .publisher()?
            .publish_confirmed(request, author, signer)
            .map_err(Into::into)
    }

    pub fn flush_pending_public_use(
        &self,
        limit: usize,
    ) -> Result<PublicUseFlushReport, VNextProductRuntimeError> {
        let _network_lease = self.lane_lease(VNextRuntimeLane::Network)?;
        let lease = self.lane_lease(VNextRuntimeLane::PublicUseEvidencePublish)?;
        lease.core.ensure_storage_writable()?;
        if limit > lease.core.budgets.publication_flush_batch {
            return Err(VNextProductRuntimeError::BudgetExceeded(
                VNextFeature::PublicUseEvidencePublish,
            ));
        }
        let network = lease.core.network()?;
        lease
            .core
            .publisher()?
            .flush_pending(&network, limit)
            .map_err(Into::into)
    }

    pub fn public_use_publication(
        &self,
        publication_id: [u8; 32],
    ) -> Result<Option<PublicUsePublicationRecord>, VNextProductRuntimeError> {
        let lease = self.lease()?;
        lease
            .core
            .publisher()?
            .publication(publication_id)
            .map_err(Into::into)
    }

    pub fn public_use_selectors_for_target(
        &self,
        target: &ObjectReference,
    ) -> Result<Vec<SelectorCid>, VNextProductRuntimeError> {
        let lease = self.lease()?;
        lease
            .core
            .publisher()?
            .publication_selectors_for_target(target)
            .map_err(Into::into)
    }

    pub fn materialize_public_use_view(
        &self,
        selector: SelectorCid,
        target: ObjectReference,
        policy_version: LocalPolicyVersion,
    ) -> Result<DistributedPomvReport, VNextProductRuntimeError> {
        let _network_lease = self.lane_lease(VNextRuntimeLane::Network)?;
        let lease = self.lane_lease(VNextRuntimeLane::DistributedPomvView)?;
        lease.core.ensure_storage_writable()?;
        let network = lease.core.network()?;
        let result = lease
            .core
            .pomv()?
            .materialize_public_use_view(&network, selector, target, policy_version)
            .map_err(Into::into);
        if let Ok(report) = &result {
            lease.core.observability.observe_selector_coverage(
                report.observations.len() as u64,
                report.incremental_continuation,
            );
            lease
                .core
                .observability
                .observe_pomv(report.idempotency_conflicts, report.view.revision);
            lease.core.observability.record_count(
                VNextReasonCode::AcceptedNew,
                report.newly_indexed_events,
                0,
                report
                    .changed_object_records
                    .saturating_add(report.changed_event_records),
            );
            lease.core.observability.record_count(
                VNextReasonCode::Replayed,
                report.replayed_index_events,
                0,
                0,
            );
            lease.core.observability.record_count(
                VNextReasonCode::QuarantinedInvalid,
                report.invalid_or_unbound_records,
                0,
                0,
            );
        } else {
            lease
                .core
                .observability
                .record(VNextReasonCode::JournalFailure, 0, 1);
            tracing::warn!(
                target: "onebrain::vnext::observability",
                reason_code = VNextReasonCode::JournalFailure.code(),
                operation = "distributed_pomv_view",
                "vNext product operation failed"
            );
        }
        result
    }

    pub fn proposal(
        &self,
        id: ProposalId,
    ) -> Result<Option<ku_kql::vnext_proposal::BindingProposal>, VNextProductRuntimeError> {
        let lease = self.lease()?;
        let proposal = lease.core.kql()?.proposal(id).cloned();
        Ok(proposal)
    }
}

/// Transitional host adapter: existing typed vNext status is reachable
/// through `BaseServices::query` while products migrate to the sole Base
/// facade. Mutating legacy product commands are deliberately not decoded from
/// opaque bytes here; Task 18 projects their generated Base command forms.
impl crate::base_runtime::BaseLocalOperationAdapter for VNextProductServices {
    fn query(
        &self,
        request: onebrain_base_contract::BaseQueryRequestV1,
    ) -> Result<
        (
            onebrain_base_contract::TypedPayloadV1,
            Option<onebrain_base_contract::BaseOpaqueContinuation>,
        ),
        crate::base_runtime::BaseServiceError,
    > {
        if request.payload.as_bytes() != b"vnext.runtime.status.v1" {
            return Err(crate::base_runtime::BaseServiceError {
                code: onebrain_base_contract::BaseErrorCodeV1::InvalidRequest,
                reason: "unsupported_vnext_base_query",
                retryable: false,
                reconcile_before_retry: false,
            });
        }
        let status = self
            .status()
            .map_err(|_| crate::base_runtime::BaseServiceError {
                code: onebrain_base_contract::BaseErrorCodeV1::DependencyUnavailable,
                reason: "vnext_runtime_status_unavailable",
                retryable: true,
                reconcile_before_retry: true,
            })?;
        let bytes = serde_json::to_vec(&serde_json::json!({
            "profile": "vnext.runtime.status.v1",
            "state": format!("{:?}", status.state),
            "authenticated_routes": status.authenticated_routes,
            "active_private_needs": status.active_private_needs,
            "durable_matches": status.durable_matches,
            "pending_publications": status.pending_publications,
            "storage_bytes": status.storage_bytes,
            "network_enabled": status.rollout.lane(VNextRuntimeLane::Network).enabled,
            "claims_network_completion": false,
        }))
        .map_err(|_| crate::base_runtime::BaseServiceError {
            code: onebrain_base_contract::BaseErrorCodeV1::InternalError,
            reason: "vnext_runtime_status_encoding_failed",
            retryable: false,
            reconcile_before_retry: true,
        })?;
        let payload =
            onebrain_base_contract::TypedPayloadV1::try_from_bytes(bytes).map_err(|_| {
                crate::base_runtime::BaseServiceError {
                    code: onebrain_base_contract::BaseErrorCodeV1::ResourceExhausted,
                    reason: "vnext_runtime_status_too_large",
                    retryable: true,
                    reconcile_before_retry: true,
                }
            })?;
        Ok((payload, None))
    }

    fn confirm_local(
        &self,
        _command: onebrain_base_contract::BaseLocalCommandV1,
    ) -> Result<Vec<u8>, crate::base_runtime::BaseServiceError> {
        Err(crate::base_runtime::BaseServiceError {
            code: onebrain_base_contract::BaseErrorCodeV1::CapabilityDisabled,
            reason: "vnext_mutation_requires_generated_base_command",
            retryable: false,
            reconcile_before_retry: false,
        })
    }
}

struct StartupArtifactGuard {
    data_dir: PathBuf,
    candidates: Vec<PathBuf>,
    preexisting: BTreeSet<PathBuf>,
    data_dir_preexisting: bool,
    committed: bool,
}

impl StartupArtifactGuard {
    fn new_candidates(
        mut candidates: Vec<PathBuf>,
        data_dir: &Path,
    ) -> Result<Self, VNextProductRuntimeError> {
        let data_dir_preexisting = data_dir.exists();
        if data_dir_preexisting && !data_dir.is_dir() {
            return Err(VNextProductRuntimeError::InvalidDataDirectory(
                data_dir.to_path_buf(),
            ));
        }
        candidates.sort();
        candidates.dedup();
        let preexisting = candidates
            .iter()
            .filter(|path| path.exists())
            .cloned()
            .collect();
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            candidates,
            preexisting,
            data_dir_preexisting,
            committed: false,
        })
    }

    fn new_artifacts(&self) -> Vec<PathBuf> {
        self.candidates
            .iter()
            .filter(|path| !self.preexisting.contains(*path))
            .cloned()
            .filter(|path| path.exists())
            .collect()
    }

    fn commit(&mut self) -> Vec<PathBuf> {
        let artifacts = self.new_artifacts();
        self.committed = true;
        artifacts
    }

    fn rollback(&self) {
        let _ = remove_startup_artifacts(self.new_artifacts());
        if !self.data_dir_preexisting {
            let _ = std::fs::remove_dir(&self.data_dir);
        }
    }
}

impl Drop for StartupArtifactGuard {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback();
        }
    }
}

fn remove_startup_artifacts(paths: Vec<PathBuf>) -> Result<(), VNextProductRuntimeError> {
    for path in paths {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

struct ProductStorageGuard {
    data_dir: PathBuf,
    soft_watermark_bytes: u64,
    hard_watermark_bytes: u64,
}

impl ProductStorageGuard {
    fn new(data_dir: &Path, budgets: VNextRuntimeBudgets) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            soft_watermark_bytes: budgets.storage_soft_watermark_bytes,
            hard_watermark_bytes: budgets.storage_hard_watermark_bytes,
        }
    }

    fn used_bytes(&self) -> Result<u64, VNextProductRuntimeError> {
        let entries = match std::fs::read_dir(&self.data_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut used = 0u64;
        for entry in entries {
            let entry = entry?;
            if !entry.file_name().to_string_lossy().starts_with("vnext_") {
                continue;
            }
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                used = used
                    .checked_add(metadata.len())
                    .ok_or(VNextProductRuntimeError::StorageSizeOverflow)?;
            }
        }
        Ok(used)
    }

    fn pressure(&self, used_bytes: u64) -> VNextStoragePressure {
        if used_bytes >= self.soft_watermark_bytes {
            VNextStoragePressure::SoftWatermark
        } else {
            VNextStoragePressure::Normal
        }
    }

    fn ensure_writable(&self) -> Result<(), VNextProductRuntimeError> {
        let used_bytes = self.used_bytes()?;
        if used_bytes >= self.hard_watermark_bytes {
            Err(VNextProductRuntimeError::StorageHardWatermark {
                used_bytes,
                hard_watermark_bytes: self.hard_watermark_bytes,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Clone)]
struct ProductCancellation {
    receiver: watch::Receiver<bool>,
}

impl ProductCancellation {
    async fn cancelled(&mut self) {
        while !*self.receiver.borrow() {
            if self.receiver.changed().await.is_err() {
                break;
            }
        }
    }

    #[cfg(test)]
    fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }
}

struct BoundedProductWorkers {
    cancellation: watch::Sender<bool>,
    tasks: Vec<(VNextProductWorkerKind, JoinHandle<()>)>,
    max_workers: usize,
    poll_ticks: Arc<AtomicU64>,
}

impl BoundedProductWorkers {
    fn new(max_workers: usize) -> Self {
        let (cancellation, _) = watch::channel(false);
        Self {
            cancellation,
            tasks: Vec::new(),
            max_workers,
            poll_ticks: Arc::new(AtomicU64::new(0)),
        }
    }

    fn len(&self) -> usize {
        self.tasks.len()
    }

    fn capacity(&self) -> usize {
        self.max_workers
    }

    #[cfg(test)]
    fn is_cancelled(&self) -> bool {
        *self.cancellation.borrow()
    }

    fn start_lane_workers(
        &mut self,
        lanes: VNextProductLaneStatus,
        poll_interval_millis: u64,
        publication: Option<(Arc<PublicUseEvidencePublisher>, Arc<VNextNetworkRuntime>)>,
        publication_flush_batch: usize,
        rollout: VNextRuntimeRollout,
    ) -> Result<(), VNextProductRuntimeError> {
        let interval = Duration::from_millis(poll_interval_millis);
        for (enabled, kind) in [
            (
                lanes.distributed_kql_one_hop,
                VNextProductWorkerKind::DistributedKql,
            ),
            (
                lanes.public_use_evidence_publish,
                VNextProductWorkerKind::PublicUsePublication,
            ),
            (
                lanes.distributed_pomv_view,
                VNextProductWorkerKind::DistributedPomv,
            ),
        ] {
            if !enabled {
                continue;
            }
            let poll_ticks = Arc::clone(&self.poll_ticks);
            let publication = if kind == VNextProductWorkerKind::PublicUsePublication {
                publication.clone()
            } else {
                None
            };
            let rollout = rollout.clone();
            self.spawn(kind, move |mut cancellation| async move {
                let mut ticker = tokio::time::interval(interval);
                loop {
                    tokio::select! {
                        _ = cancellation.cancelled() => break,
                        _ = ticker.tick() => {
                            poll_ticks.fetch_add(1, Ordering::Relaxed);
                            if let Some((publisher, network)) = publication.as_ref() {
                                let Ok(_network_generation) =
                                    rollout.acquire(VNextRuntimeLane::Network)
                                else {
                                    continue;
                                };
                                let Ok(_publication_generation) = rollout
                                    .acquire(VNextRuntimeLane::PublicUseEvidencePublish)
                                else {
                                    continue;
                                };
                                // Missing authenticated routes are retryable;
                                // durable publications remain unexported.
                                let _ = publisher.flush_pending(
                                    network,
                                    publication_flush_batch,
                                );
                            }
                        }
                    }
                }
            })?;
        }
        Ok(())
    }

    fn spawn<F, Fut>(
        &mut self,
        kind: VNextProductWorkerKind,
        worker: F,
    ) -> Result<(), VNextProductRuntimeError>
    where
        F: FnOnce(ProductCancellation) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.tasks.len() >= self.max_workers {
            return Err(VNextProductRuntimeError::WorkerCapacityReached);
        }
        let token = ProductCancellation {
            receiver: self.cancellation.subscribe(),
        };
        self.tasks.push((kind, tokio::spawn(worker(token))));
        Ok(())
    }

    async fn shutdown(&mut self) {
        self.cancellation.send_replace(true);
        for (_, mut task) in self.tasks.drain(..) {
            if tokio::time::timeout(Duration::from_secs(5), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = (&mut task).await;
            }
        }
    }

    fn cancel_and_abort(&mut self) {
        self.cancellation.send_replace(true);
        for (_, task) in self.tasks.drain(..) {
            task.abort();
        }
    }
}

#[derive(Debug, Error)]
pub enum VNextProductRuntimeError {
    #[error("vNext product runtime is stopped")]
    Stopped,
    #[error("vNext product service lifecycle lock is poisoned")]
    LifecycleLockPoisoned,
    #[error("vNext product {0} subsystem lock is poisoned")]
    SubsystemLockPoisoned(&'static str),
    #[error("vNext product in-flight operation counter overflowed")]
    InFlightOverflow,
    #[error("vNext product KQL owner lock is poisoned")]
    KqlLockPoisoned,
    #[error("vNext product background worker capacity reached")]
    WorkerCapacityReached,
    #[error(
        "vNext product lane {feature_name} is disabled",
        feature_name = .0.name()
    )]
    LaneDisabled(VNextFeature),
    #[error(
        "vNext product lane {feature_name} request exceeds its configured budget",
        feature_name = .0.name()
    )]
    BudgetExceeded(VNextFeature),
    #[error("vNext product runtime configuration failed: {0}")]
    Configuration(String),
    #[error("vNext storage hard watermark reached ({used_bytes}/{hard_watermark_bytes} bytes)")]
    StorageHardWatermark {
        used_bytes: u64,
        hard_watermark_bytes: u64,
    },
    #[error("vNext storage size accounting overflowed")]
    StorageSizeOverflow,
    #[error("vNext product data path is not a directory: {0}")]
    InvalidDataDirectory(PathBuf),
    #[error("vNext runtime rollout failed: {0}")]
    RuntimeRollout(#[from] VNextRuntimeRolloutError),
    #[error("vNext network runtime failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("vNext distributed KQL runtime failed: {0}")]
    DistributedKql(#[from] DistributedKqlError),
    #[error("vNext distributed PoMV runtime failed: {0}")]
    DistributedPomv(#[from] DistributedPomvError),
    #[error("vNext product filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset_path::BootstrapDatasetPathResolver;
    use crate::vnext_config::VNextFeatureConfig;
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        decode_feed_inception, ConceptCcid, DeviceId, DisclosureClass, FeedInception,
        MetabolicViewPolicy, NamespaceCommitment, ObjectReference, UseEvidencePayload, UseMode,
    };
    use ku_net::vnext_carrier::CarrierRecord;

    #[test]
    fn integrated_runtime_paths_are_scoped_by_closed_storage_owner() {
        let directory = tempfile::tempdir().unwrap();
        let resolver = BootstrapDatasetPathResolver::new(directory.path()).unwrap();
        let paths = VNextProductStoragePaths::from_resolver(&resolver).unwrap();
        assert_eq!(
            paths.network.canonical.parent().unwrap(),
            resolver.owner_path(BaseStorageOwnerId::CANONICAL).unwrap()
        );
        assert_eq!(
            paths.network.outbox.parent().unwrap(),
            resolver.owner_path(BaseStorageOwnerId::OUTBOX).unwrap()
        );
        assert_ne!(paths.private_kql, paths.private_pomv);
        assert_ne!(paths.identity, paths.rollout);
        assert_ne!(paths.operational, paths.network.admission_root);
    }

    struct UnavailableIdentitySigner {
        public_key: [u8; 32],
    }

    impl SessionIdentitySigner for UnavailableIdentitySigner {
        fn public_key(&self) -> [u8; 32] {
            self.public_key
        }

        fn sign_session_message(&self, _message: &[u8]) -> Result<[u8; 64], String> {
            Err("signer offline".into())
        }
    }

    fn dependencies(marker: u8) -> VNextProductRuntimeDependencies {
        let version = LocalPolicyVersion::new(1).unwrap();
        let policies = LocalPolicyRegistry::new([(
            version,
            MetabolicViewPolicy {
                policy_ref: ObjectReference::new(0, [marker; 32]),
                accepted_evidence_policies: vec![ObjectReference::new(
                    0,
                    [marker.wrapping_add(1); 32],
                )],
                recent_event_horizon: 64,
            },
        )])
        .unwrap();
        VNextProductRuntimeDependencies::new(
            LocalNeedVaultKey::from_bytes([marker.wrapping_add(2); 32]),
            policies,
        )
    }

    fn all_lanes_config() -> VNextFeatureConfig {
        let mut config = VNextFeatureConfig::default();
        config.enabled.object_event_v1 = true;
        config.enabled.obp_rp = true;
        config.enabled.distributed_kql_one_hop = true;
        config.enabled.public_use_evidence_publish = true;
        config.enabled.distributed_pomv_view = true;
        config
    }

    fn assert_cloneable_static_service_handle<T: Clone + Send + Sync + 'static>() {}

    #[test]
    fn product_service_handle_is_cloneable_send_sync_and_static() {
        assert_cloneable_static_service_handle::<VNextProductServices>();
    }

    #[tokio::test]
    async fn aggregate_owns_every_runtime_and_exposes_typed_services() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let left_signer: Arc<dyn SessionIdentitySigner> =
            Arc::new(SigningKey::from_bytes(&[0x31; 32]));
        let config = all_lanes_config();
        let mut left = VNextProductRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(10),
            Some(left_signer),
        )
        .await
        .unwrap();
        let mut right = VNextProductRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(20),
            None,
        )
        .await
        .unwrap();

        for file in [
            "vnext_private_need_vault.redb",
            "vnext_distributed_kql.redb",
            "vnext_public_use_sender.redb",
            "vnext_distributed_pomv.redb",
            "vnext_verified.redb",
        ] {
            assert!(left_dir.path().join(file).exists(), "missing {file}");
        }
        assert!(!left_dir.path().join("vnext_identity.key").exists());

        let before = left.services().status().unwrap();
        assert_eq!(before.state, VNextProductRuntimeState::Running);
        assert_eq!(before.signer_mode, VNextProductSignerMode::CallerOwned);
        assert_eq!(before.active_private_needs, 0);
        assert_eq!(before.pending_publications, 0);
        assert_eq!(before.policy_versions, vec![1]);
        assert!(before.lanes.distributed_kql_one_hop);
        assert!(before.lanes.public_use_evidence_publish);
        assert!(before.lanes.distributed_pomv_view);
        assert_eq!(before.budgets, config.runtime_budgets);
        assert_eq!(before.active_product_workers, 3);
        assert_eq!(before.max_product_workers, MAX_PRODUCT_BACKGROUND_WORKERS);
        assert!(!before.cancellation_requested);
        assert_eq!(
            before.startup_trace,
            vec![
                VNextStartupPhase::ConfigurationValidated,
                VNextStartupPhase::SignerAndVaultValidated,
                VNextStartupPhase::StoresOpened,
                VNextStartupPhase::AuthenticatedQuicStarted,
                VNextStartupPhase::PrivateNeedsRehydrated,
                VNextStartupPhase::PublicationOutboxDrained,
                VNextStartupPhase::WorkersStarted,
                VNextStartupPhase::Running,
            ]
        );
        assert_eq!(before.rehydrated_private_needs, 0);
        assert_eq!(before.startup_pending_publications, 0);
        assert!(!before.changes_wallet_state);
        assert!(!before.changes_obt_state);
        assert!(!before.claims_network_completion);

        let right_addr = right.services().local_addr();
        let right_principal = NodeId::from_bytes(right.services().network_status().principal);
        let _session = left.services().connect_peer(right_addr).await.unwrap();
        assert!(left
            .services()
            .authenticated_route(right_principal)
            .unwrap()
            .is_some());
        assert_eq!(left.services().status().unwrap().authenticated_routes, 1);

        left.shutdown().await;
        right.shutdown().await;
        assert_eq!(left.state, VNextProductRuntimeState::Stopped);
        assert!(left.workers.is_cancelled());
        assert_eq!(
            left.shutdown_trace,
            vec![
                VNextShutdownPhase::OperationsFenced,
                VNextShutdownPhase::WorkersCancelled,
                VNextShutdownPhase::SafeMetadataFlushed,
                VNextShutdownPhase::NetworkStopped,
                VNextShutdownPhase::StoresClosed,
            ]
        );
        assert!(matches!(
            left.services().authenticated_route(right_principal),
            Err(VNextProductRuntimeError::Stopped)
        ));
    }

    #[tokio::test]
    async fn service_handle_runs_while_aggregate_owner_mutexes_are_held() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let config = all_lanes_config();
        let left = Arc::new(tokio::sync::Mutex::new(
            VNextProductRuntime::start(
                left_dir.path(),
                "127.0.0.1:0".parse().unwrap(),
                &config,
                dependencies(81),
                Some(Arc::new(SigningKey::from_bytes(&[0x81; 32]))),
            )
            .await
            .unwrap(),
        ));
        let right = Arc::new(tokio::sync::Mutex::new(
            VNextProductRuntime::start(
                right_dir.path(),
                "127.0.0.1:0".parse().unwrap(),
                &config,
                dependencies(82),
                Some(Arc::new(SigningKey::from_bytes(&[0x82; 32]))),
            )
            .await
            .unwrap(),
        ));
        let left_services = left.lock().await.services();
        let right_services = right.lock().await.services();
        let right_addr = right_services.local_addr();
        let right_principal = NodeId::from_bytes(right_services.network_status().principal);
        let feed_key = SigningKey::from_bytes(&[0x86; 32]);
        let namespace = NamespaceCommitment::derive(b"p2.5-service-handle", [0x87; 32]).unwrap();
        let feed_bytes = FeedInception::new(
            *feed_key.verifying_key().as_bytes(),
            namespace,
            0,
            DeviceId::from_bytes([0x88; 32]),
        )
        .sign(&feed_key)
        .unwrap()
        .encode()
        .unwrap();
        let author = decode_feed_inception(&feed_bytes).unwrap();
        let target = ObjectReference::new(0, [0x84; 32]);
        let policy = ObjectReference::new(0, [0x85; 32]);
        let request = PreparePublicUseEvidenceRequest {
            payload: UseEvidencePayload {
                subjects: vec![target.clone()],
                mode: UseMode::Application,
                actor_class: ConceptCcid::from_bytes([0x89; 16]),
                task_context_commitment: [0x8A; 32],
                causal_role: ConceptCcid::from_bytes([0x8B; 16]),
                assembly: None,
                mapping: None,
                outcome_observation: None,
                use_policy: policy,
                observed_frontier: [0x8C; 32],
            },
            exact_target: target.clone(),
            expected_peer: right_principal,
            selector: SelectorCid::from_bytes([0x83; 32]),
            namespace,
            disclosure: DisclosureClass::Public,
            idempotency_key: [0x8D; 32],
            expires_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60,
        };

        // These guards model API/CLI/Desktop owning the aggregate
        // Arc<Mutex<OneBrainNode>>. Service work must not attempt to reacquire
        // either aggregate mutex.
        let left_owner = left.lock().await;
        let right_owner = right.lock().await;
        tokio::time::timeout(
            Duration::from_secs(3),
            left_services.connect_peer(right_addr),
        )
        .await
        .expect("network wait tried to reacquire an aggregate owner mutex")
        .unwrap();
        let status = tokio::time::timeout(Duration::from_secs(1), async { left_services.status() })
            .await
            .expect("Redb status scan tried to reacquire an aggregate owner mutex")
            .unwrap();
        assert_eq!(status.authenticated_routes, 1);
        let prepared = left_services.prepare_public_use(&request, &author).unwrap();
        left_services
            .publish_confirmed_public_use(&prepared.confirm(), &author, &feed_key)
            .expect("signer call tried to reacquire an aggregate owner mutex");
        let view = tokio::time::timeout(Duration::from_secs(1), async {
            left_services.materialize_public_use_view(
                SelectorCid::from_bytes([0x83; 32]),
                target,
                LocalPolicyVersion::new(1).unwrap(),
            )
        })
        .await
        .expect("view materialization tried to reacquire an aggregate owner mutex")
        .unwrap();
        assert!(view.view.cumulative_event_ids.is_empty());
        drop(right_owner);
        drop(left_owner);

        left.lock().await.shutdown().await;
        right.lock().await.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_fences_new_work_and_drains_existing_service_leases() {
        let directory = tempfile::tempdir().unwrap();
        let config = all_lanes_config();
        let runtime = VNextProductRuntime::start(
            directory.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(85),
            Some(Arc::new(SigningKey::from_bytes(&[0x85; 32]))),
        )
        .await
        .unwrap();
        let services = runtime.services();
        let lease = services.lease().unwrap();
        let mut shutdown = tokio::spawn(async move {
            let mut runtime = runtime;
            runtime.shutdown().await;
            runtime
        });

        tokio::task::yield_now().await;
        assert!(matches!(
            services.status(),
            Err(VNextProductRuntimeError::Stopped)
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut shutdown)
                .await
                .is_err(),
            "shutdown completed before the active service lease drained"
        );
        drop(lease);
        let runtime = tokio::time::timeout(Duration::from_secs(3), shutdown)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            services.network_status().state,
            VNextNetworkRuntimeState::Stopped
        );
        assert_eq!(
            runtime.shutdown_trace,
            vec![
                VNextShutdownPhase::OperationsFenced,
                VNextShutdownPhase::WorkersCancelled,
                VNextShutdownPhase::SafeMetadataFlushed,
                VNextShutdownPhase::NetworkStopped,
                VNextShutdownPhase::StoresClosed,
            ]
        );
    }

    #[tokio::test]
    async fn product_worker_owner_is_bounded_and_cancelled() {
        let mut workers = BoundedProductWorkers::new(MAX_PRODUCT_BACKGROUND_WORKERS);
        for _ in 0..MAX_PRODUCT_BACKGROUND_WORKERS {
            workers
                .spawn(
                    VNextProductWorkerKind::DistributedKql,
                    |mut cancellation| async move {
                        cancellation.cancelled().await;
                    },
                )
                .unwrap();
        }
        assert!(matches!(
            workers.spawn(VNextProductWorkerKind::DistributedKql, |_| async {}),
            Err(VNextProductRuntimeError::WorkerCapacityReached)
        ));
        let cancellation = ProductCancellation {
            receiver: workers.cancellation.subscribe(),
        };
        workers.shutdown().await;
        assert!(cancellation.is_cancelled());
        assert_eq!(workers.len(), 0);
    }

    #[tokio::test]
    async fn signer_failure_and_post_store_bind_failure_rollback_cleanly() {
        let signer_failure_dir = tempfile::tempdir().unwrap();
        let config = all_lanes_config();
        let public_key = *SigningKey::from_bytes(&[0x71; 32])
            .verifying_key()
            .as_bytes();
        let error = VNextProductRuntime::start(
            signer_failure_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(71),
            Some(Arc::new(UnavailableIdentitySigner { public_key })),
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(
            error,
            VNextProductRuntimeError::Network(VNextNetworkRuntimeError::IdentitySignerUnavailable(
                _
            ))
        ));
        assert!(VNEXT_STARTUP_ARTIFACTS
            .iter()
            .all(|name| !signer_failure_dir.path().join(name).exists()));

        let occupied_dir = tempfile::tempdir().unwrap();
        let failed_dir = tempfile::tempdir().unwrap();
        let mut occupied = VNextProductRuntime::start(
            occupied_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(72),
            Some(Arc::new(SigningKey::from_bytes(&[0x72; 32]))),
        )
        .await
        .unwrap();
        let occupied_addr = occupied.services().local_addr();
        let error = VNextProductRuntime::start(
            failed_dir.path(),
            occupied_addr,
            &config,
            dependencies(73),
            Some(Arc::new(SigningKey::from_bytes(&[0x73; 32]))),
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(error, VNextProductRuntimeError::Network(_)));
        assert!(VNEXT_STARTUP_ARTIFACTS
            .iter()
            .all(|name| !failed_dir.path().join(name).exists()));
        occupied.shutdown().await;
    }

    #[tokio::test]
    async fn explicit_startup_rollback_closes_stores_and_removes_only_new_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let preserved = directory.path().join("vnext_verified.redb");
        std::fs::write(&preserved, b"caller-owned-existing-artifact").unwrap();
        let config = all_lanes_config();
        let error = VNextProductRuntime::start(
            directory.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(74),
            Some(Arc::new(SigningKey::from_bytes(&[0x74; 32]))),
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(error, VNextProductRuntimeError::Network(_)));
        assert_eq!(
            std::fs::read(&preserved).unwrap(),
            b"caller-owned-existing-artifact"
        );

        let clean_dir = tempfile::tempdir().unwrap();
        let runtime = VNextProductRuntime::start(
            clean_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(75),
            Some(Arc::new(SigningKey::from_bytes(&[0x75; 32]))),
        )
        .await
        .unwrap();
        runtime.rollback_startup().await.unwrap();
        assert!(VNEXT_STARTUP_ARTIFACTS
            .iter()
            .all(|name| !clean_dir.path().join(name).exists()));

        let parent = tempfile::tempdir().unwrap();
        let newly_created = parent.path().join("new-runtime-data");
        let runtime = VNextProductRuntime::start(
            &newly_created,
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(76),
            Some(Arc::new(SigningKey::from_bytes(&[0x76; 32]))),
        )
        .await
        .unwrap();
        runtime.rollback_startup().await.unwrap();
        assert!(newly_created.join("vnext_runtime_rollout.redb").is_file());
        assert!(VNEXT_STARTUP_ARTIFACTS
            .iter()
            .all(|name| !newly_created.join(name).exists()));
    }

    #[tokio::test]
    // Historical evidence ID:
    // independent_lane_kill_switches_prevent_store_creation_and_operations.
    // M5-06 supersedes store deletion with retained provisioned owners.
    async fn independent_lane_kill_switches_fence_operations_and_remain_reenableable() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = all_lanes_config();
        config.kill_switches.distributed_kql_one_hop = true;
        config.kill_switches.public_use_evidence_publish = true;
        config.kill_switches.distributed_pomv_view = true;
        let mut runtime = VNextProductRuntime::start(
            directory.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(40),
            None,
        )
        .await
        .unwrap();

        for file in [
            "vnext_private_need_vault.redb",
            "vnext_distributed_kql.redb",
            "vnext_public_use_sender.redb",
            "vnext_distributed_pomv.redb",
        ] {
            assert!(
                directory.path().join(file).exists(),
                "provisioned lane must retain {file} for explicit re-enable"
            );
        }
        assert!(directory.path().join("vnext_verified.redb").exists());
        let status = runtime.services().status().unwrap();
        assert!(!status.lanes.distributed_kql_one_hop);
        assert!(!status.lanes.public_use_evidence_publish);
        assert!(!status.lanes.distributed_pomv_view);
        assert!(matches!(
            runtime.services().process_one_hop_affordance_delta(
                SelectorCid::from_bytes([0x41; 32]),
                DistributedKqlBudget::default(),
            ),
            Err(VNextProductRuntimeError::RuntimeRollout(
                VNextRuntimeRolloutError::LaneFenced {
                    lane: VNextRuntimeLane::DistributedKql,
                    ..
                }
            ))
        ));
        let services = runtime.services();
        services
            .reenable_runtime_lane(VNextRuntimeLane::DistributedKql)
            .unwrap();
        assert!(services.status().unwrap().lanes.distributed_kql_one_hop);
        assert!(services
            .process_one_hop_affordance_delta(
                SelectorCid::from_bytes([0x41; 32]),
                DistributedKqlBudget::default(),
            )
            .is_ok());
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn runtime_kill_rollback_restart_and_explicit_reenable_use_real_quic() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let config = all_lanes_config();
        let mut left = VNextProductRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(91),
            Some(Arc::new(SigningKey::from_bytes(&[0x91; 32]))),
        )
        .await
        .unwrap();
        let mut right = VNextProductRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(92),
            Some(Arc::new(SigningKey::from_bytes(&[0x92; 32]))),
        )
        .await
        .unwrap();
        let left_services = left.services();
        let right_services = right.services();
        let left_addr = left_services.local_addr();
        let right_addr = right_services.local_addr();

        left_services
            .kill_runtime_lane(VNextRuntimeLane::Network)
            .unwrap();
        assert!(matches!(
            left_services.connect_peer(right_addr).await,
            Err(VNextProductRuntimeError::RuntimeRollout(
                VNextRuntimeRolloutError::LaneFenced {
                    lane: VNextRuntimeLane::Network,
                    ..
                }
            ))
        ));
        assert!(
            tokio::time::timeout(
                Duration::from_secs(3),
                right_services.connect_peer(left_addr)
            )
            .await
            .unwrap()
            .is_err(),
            "durably killed inbound QUIC lane accepted a new session"
        );
        left_services
            .reenable_runtime_lane(VNextRuntimeLane::Network)
            .unwrap();
        let old_generation_session = left_services.connect_peer(right_addr).await.unwrap();
        left_services
            .kill_runtime_lane(VNextRuntimeLane::Network)
            .unwrap();
        assert!(matches!(
            old_generation_session
                .send(&CarrierRecord::ReconciliationMessage(Vec::new()))
                .await,
            Err(VNextNetworkRuntimeError::RuntimeFenced(_))
        ));
        old_generation_session.close();
        left_services
            .reenable_runtime_lane(VNextRuntimeLane::Network)
            .unwrap();

        left_services
            .kill_runtime_lane(VNextRuntimeLane::PublicUseEvidencePublish)
            .unwrap();
        assert!(matches!(
            left_services.flush_pending_public_use(1),
            Err(VNextProductRuntimeError::RuntimeRollout(
                VNextRuntimeRolloutError::LaneFenced {
                    lane: VNextRuntimeLane::PublicUseEvidencePublish,
                    ..
                }
            ))
        ));
        assert!(
            left_services
                .status()
                .unwrap()
                .lanes
                .distributed_kql_one_hop
        );

        let protected_databases = [
            "vnext_verified.redb",
            "vnext_reconciliation.redb",
            "vnext_record_provenance.redb",
            "vnext_outbox.redb",
            "vnext_distributed_kql.redb",
            "vnext_public_use_sender.redb",
            "vnext_distributed_pomv.redb",
        ];
        for name in [
            "raw-retained.bin",
            "journal-retained.bin",
            "quarantine-retained.bin",
            "wallet-retained.bin",
            "obt-retained.bin",
        ] {
            std::fs::write(left_dir.path().join(name), name.as_bytes()).unwrap();
        }
        let rolled_back = left_services.rollback_runtime().unwrap();
        assert!(rolled_back.lanes.iter().all(|lane| !lane.enabled));
        assert!(!rolled_back.changes_wallet_state);
        assert!(!rolled_back.changes_obt_state);
        for name in protected_databases {
            assert!(
                left_dir.path().join(name).is_file(),
                "rollback deleted protected runtime database {name}"
            );
        }
        for name in [
            "raw-retained.bin",
            "journal-retained.bin",
            "quarantine-retained.bin",
            "wallet-retained.bin",
            "obt-retained.bin",
        ] {
            assert_eq!(
                std::fs::read(left_dir.path().join(name)).unwrap(),
                name.as_bytes(),
                "rollback changed protected runtime evidence {name}"
            );
        }

        left.shutdown().await;
        let mut restarted = VNextProductRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(91),
            Some(Arc::new(SigningKey::from_bytes(&[0x91; 32]))),
        )
        .await
        .unwrap();
        let restarted_services = restarted.services();
        let stale = restarted_services.status().unwrap().rollout;
        assert!(
            stale.lanes.iter().all(|lane| !lane.enabled),
            "stale enabled config re-enabled a durably rolled back lane"
        );
        for lane in VNextRuntimeLane::ALL {
            restarted_services.reenable_runtime_lane(lane).unwrap();
        }
        assert!(restarted_services
            .status()
            .unwrap()
            .rollout
            .lanes
            .iter()
            .all(|lane| lane.enabled));
        restarted_services
            .connect_peer(right_addr)
            .await
            .unwrap()
            .close();

        restarted.shutdown().await;
        right.shutdown().await;
    }

    #[tokio::test]
    async fn configured_budgets_and_storage_hard_watermark_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = all_lanes_config();
        config.runtime_budgets.kql_max_scan_records = 8;
        config.runtime_budgets.kql_max_affordances = 4;
        config.runtime_budgets.kql_max_pairs = 16;
        config.runtime_budgets.kql_max_proposals = 8;
        let mut runtime = VNextProductRuntime::start(
            directory.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(50),
            None,
        )
        .await
        .unwrap();
        let oversized = DistributedKqlBudget {
            max_scan_records: 9,
            max_affordances: 4,
            max_pairs: 16,
            max_proposals: 8,
        };
        assert!(matches!(
            runtime
                .services()
                .process_one_hop_affordance_delta(SelectorCid::from_bytes([0x51; 32]), oversized,),
            Err(VNextProductRuntimeError::BudgetExceeded(
                VNextFeature::DistributedKqlOneHop
            ))
        ));
        Arc::get_mut(runtime.core.as_mut().unwrap())
            .unwrap()
            .storage
            .hard_watermark_bytes = 1;
        assert!(matches!(
            runtime.services().flush_pending_public_use(1),
            Err(VNextProductRuntimeError::StorageHardWatermark { .. })
        ));
        runtime.shutdown().await;
    }
}
