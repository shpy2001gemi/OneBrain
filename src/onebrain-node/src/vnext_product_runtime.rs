//! Node-owned aggregate and typed service façade for vNext product lanes.
//!
//! Product and API code receives [`VNextProductServices`], never references to
//! the network, KQL, publication, or PoMV runtimes that this aggregate owns.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::BTreeSet;
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ku_core::foundation::{
    FeedEventSigner, NodeId, ObjectReference, SelectorCid, ValidatedFeedInception,
};
use ku_kql::vnext_private_need::{LocalNeedVaultKey, PrivateNeedBundle};
use ku_kql::vnext_proposal::ProposalId;
use ku_kql::vnext_standing_need::{StandingNeed, StandingNeedId, StandingNeedWriteOutcome};
use ku_net::vnext_session::SessionIdentitySigner;
use thiserror::Error;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::vnext_config::{VNextFeature, VNextFeatureConfig, VNextRuntimeBudgets};
use crate::vnext_distributed_kql::{
    DistributedKqlBudget, DistributedKqlError, DistributedKqlReport, DistributedKqlRuntime,
};
use crate::vnext_distributed_pomv::{
    ConfirmPublicUseEvidenceRequest, DistributedPomvError, DistributedPomvReport,
    DistributedPomvRuntime, PreparePublicUseEvidenceRequest, PreparedPublicUseIntent,
    PublicUseEvidencePublication, PublicUseEvidencePublisher, PublicUseFlushReport,
    PublicUsePublishOutcome,
};
use crate::vnext_network_runtime::{
    prepare_vnext_identity, OutboundVNextSession, VNextNetworkRuntime, VNextNetworkRuntimeError,
    VNextNetworkRuntimeState, VNextNetworkRuntimeStatus,
};
use crate::vnext_route_authority::{AuthenticatedRoute, LocalPolicyRegistry, LocalPolicyVersion};

pub const MAX_PRODUCT_BACKGROUND_WORKERS: usize = 8;
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
    fn from_config(config: &VNextFeatureConfig) -> Self {
        Self {
            distributed_kql_one_hop: config.is_active(VNextFeature::DistributedKqlOneHop),
            public_use_evidence_publish: config.is_active(VNextFeature::PublicUseEvidencePublish),
            distributed_pomv_view: config.is_active(VNextFeature::DistributedPomvView),
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
    pub policy_versions: Vec<u32>,
    pub lanes: VNextProductLaneStatus,
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
    network: Option<Arc<VNextNetworkRuntime>>,
    last_network_status: VNextNetworkRuntimeStatus,
    local_addr: SocketAddr,
    distributed_kql: Option<Mutex<DistributedKqlRuntime>>,
    public_use: Option<Arc<PublicUseEvidencePublisher>>,
    distributed_pomv: Option<DistributedPomvRuntime>,
    policy_versions: Vec<LocalPolicyVersion>,
    lanes: VNextProductLaneStatus,
    budgets: VNextRuntimeBudgets,
    storage: ProductStorageGuard,
    workers: BoundedProductWorkers,
    signer_mode: VNextProductSignerMode,
    state: VNextProductRuntimeState,
    startup_trace: Vec<VNextStartupPhase>,
    shutdown_trace: Vec<VNextShutdownPhase>,
    rehydrated_private_needs: usize,
    startup_pending_publications: u64,
    startup_artifacts: Vec<PathBuf>,
    startup_data_dir_created: bool,
}

impl VNextProductRuntime {
    pub async fn start(
        data_dir: &Path,
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
        let lanes = VNextProductLaneStatus::from_config(config);
        let budgets = config.runtime_budgets;
        let storage = ProductStorageGuard::new(data_dir, budgets);
        storage.ensure_writable()?;
        let mut artifact_guard = StartupArtifactGuard::new(data_dir)?;
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
        let prepared_identity = prepare_vnext_identity(data_dir, identity_signer)?;
        startup_trace.push(VNextStartupPhase::SignerAndVaultValidated);

        // A disabled lane has no owner and therefore creates no lane database.
        let mut distributed_kql = lanes
            .distributed_kql_one_hop
            .then(|| DistributedKqlRuntime::open_unhydrated(data_dir, private_need_vault_key))
            .transpose()?
            .map(Mutex::new);
        let public_use = lanes
            .public_use_evidence_publish
            .then(|| PublicUseEvidencePublisher::open(data_dir))
            .transpose()?
            .map(Arc::new);
        let distributed_pomv = lanes
            .distributed_pomv_view
            .then(|| {
                DistributedPomvRuntime::open_with_limits(
                    data_dir,
                    budgets.pomv_max_records,
                    budgets.pomv_max_view_records,
                    policies,
                )
            })
            .transpose()?;
        startup_trace.push(VNextStartupPhase::StoresOpened);

        let network = Arc::new(
            VNextNetworkRuntime::start_prepared(
                data_dir,
                bind_addr,
                config.network,
                prepared_identity,
            )
            .await?,
        );
        startup_trace.push(VNextStartupPhase::AuthenticatedQuicStarted);
        let last_network_status = network.status();
        let local_addr = network.local_addr();

        let rehydrated_private_needs = distributed_kql
            .as_mut()
            .map(|kql| {
                kql.get_mut()
                    .map_err(|_| VNextProductRuntimeError::KqlLockPoisoned)?
                    .rehydrate_private_needs()
                    .map_err(VNextProductRuntimeError::from)
            })
            .transpose()?
            .unwrap_or_default();
        startup_trace.push(VNextStartupPhase::PrivateNeedsRehydrated);

        // Recover the logical publication outbox before scheduling retries.
        // Routes are session-derived, so startup records durable pending work;
        // the publication worker may retry it only after a route is available.
        let startup_pending_publications = if let Some(public_use) = public_use.as_ref() {
            match public_use.flush_pending(&network, budgets.publication_flush_batch) {
                Ok(_) | Err(DistributedPomvError::AuthenticatedRouteUnavailable) => {}
                Err(error) => return Err(error.into()),
            }
            public_use.pending_publication_count()?
        } else {
            0
        };
        startup_trace.push(VNextStartupPhase::PublicationOutboxDrained);

        let mut workers = BoundedProductWorkers::new(MAX_PRODUCT_BACKGROUND_WORKERS);
        workers.start_lane_workers(
            lanes,
            budgets.worker_poll_interval_millis,
            public_use
                .as_ref()
                .map(|publisher| (Arc::clone(publisher), Arc::clone(&network))),
            budgets.publication_flush_batch,
        )?;
        startup_trace.push(VNextStartupPhase::WorkersStarted);
        startup_trace.push(VNextStartupPhase::Running);
        let startup_data_dir_created = !artifact_guard.data_dir_preexisting;
        let startup_artifacts = artifact_guard.commit();

        Ok(Self {
            network: Some(network),
            last_network_status,
            local_addr,
            distributed_kql,
            public_use,
            distributed_pomv,
            policy_versions,
            lanes,
            budgets,
            storage,
            workers,
            signer_mode,
            state: VNextProductRuntimeState::Running,
            startup_trace,
            shutdown_trace: Vec::with_capacity(5),
            rehydrated_private_needs,
            startup_pending_publications,
            startup_artifacts,
            startup_data_dir_created,
        })
    }

    pub fn services(&self) -> VNextProductServices<'_> {
        VNextProductServices { runtime: self }
    }

    pub async fn shutdown(&mut self) {
        if self.state == VNextProductRuntimeState::Stopped {
            return;
        }
        self.state = VNextProductRuntimeState::Stopped;
        self.shutdown_trace
            .push(VNextShutdownPhase::OperationsFenced);
        self.workers.shutdown().await;
        self.shutdown_trace
            .push(VNextShutdownPhase::WorkersCancelled);
        self.flush_safe_pending_metadata();
        self.shutdown_trace
            .push(VNextShutdownPhase::SafeMetadataFlushed);
        if let Some(network) = self.network.take() {
            self.last_network_status = network.status();
            if let Ok(mut network) = Arc::try_unwrap(network) {
                network.shutdown().await;
            }
            self.last_network_status.state = VNextNetworkRuntimeState::Stopped;
        }
        self.shutdown_trace.push(VNextShutdownPhase::NetworkStopped);
        self.distributed_kql.take();
        self.public_use.take();
        self.distributed_pomv.take();
        self.shutdown_trace.push(VNextShutdownPhase::StoresClosed);
    }

    /// Abort an otherwise successful product startup because a later
    /// node-owned startup phase failed (for example, the legacy TCP bind).
    /// Only artifacts that did not exist before this startup are removed.
    pub async fn rollback_startup(mut self) -> Result<(), VNextProductRuntimeError> {
        self.shutdown().await;
        let startup_artifacts = std::mem::take(&mut self.startup_artifacts);
        let remove_data_dir = self.startup_data_dir_created;
        let data_dir = self.storage.data_dir.clone();
        drop(self);
        remove_startup_artifacts(startup_artifacts)?;
        if remove_data_dir {
            match std::fs::remove_dir(data_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn ensure_running(&self) -> Result<(), VNextProductRuntimeError> {
        if self.state == VNextProductRuntimeState::Running {
            Ok(())
        } else {
            Err(VNextProductRuntimeError::Stopped)
        }
    }

    fn network(&self) -> Result<&VNextNetworkRuntime, VNextProductRuntimeError> {
        self.ensure_running()?;
        self.network
            .as_ref()
            .map(Arc::as_ref)
            .ok_or(VNextProductRuntimeError::Stopped)
    }

    fn flush_safe_pending_metadata(&mut self) {
        if let Some(kql) = self.distributed_kql.as_ref() {
            if let Ok(kql) = kql.lock() {
                self.rehydrated_private_needs = kql.active_target_count();
            }
        }
        if let Some(public_use) = self.public_use.as_ref() {
            if let Ok(pending) = public_use.pending_publication_count() {
                self.startup_pending_publications = pending;
            }
        }
        let _ = self.storage.used_bytes();
    }

    fn kql(&self) -> Result<MutexGuard<'_, DistributedKqlRuntime>, VNextProductRuntimeError> {
        self.distributed_kql
            .as_ref()
            .ok_or(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::DistributedKqlOneHop,
            ))?
            .lock()
            .map_err(|_| VNextProductRuntimeError::KqlLockPoisoned)
    }

    fn publisher(&self) -> Result<&PublicUseEvidencePublisher, VNextProductRuntimeError> {
        self.public_use
            .as_ref()
            .map(Arc::as_ref)
            .ok_or(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::PublicUseEvidencePublish,
            ))
    }

    fn pomv(&self) -> Result<&DistributedPomvRuntime, VNextProductRuntimeError> {
        self.distributed_pomv
            .as_ref()
            .ok_or(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::DistributedPomvView,
            ))
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

impl Drop for VNextProductRuntime {
    fn drop(&mut self) {
        self.workers.cancel_and_abort();
        self.state = VNextProductRuntimeState::Stopped;
    }
}

/// Narrow product-facing operations. Its private fields make it impossible for
/// API callers to acquire a raw subsystem runtime through this façade.
pub struct VNextProductServices<'a> {
    runtime: &'a VNextProductRuntime,
}

impl VNextProductServices<'_> {
    pub fn local_addr(&self) -> SocketAddr {
        self.runtime.local_addr
    }

    pub fn network_status(&self) -> VNextNetworkRuntimeStatus {
        self.runtime
            .network
            .as_ref()
            .map(|network| network.status())
            .unwrap_or_else(|| self.runtime.last_network_status.clone())
    }

    pub fn status(&self) -> Result<VNextProductRuntimeStatus, VNextProductRuntimeError> {
        let active_private_needs = self
            .runtime
            .distributed_kql
            .as_ref()
            .map(|kql| {
                kql.lock()
                    .map(|kql| kql.active_target_count())
                    .map_err(|_| VNextProductRuntimeError::KqlLockPoisoned)
            })
            .transpose()?
            .unwrap_or_default();
        let durable_matches = self
            .runtime
            .distributed_kql
            .as_ref()
            .map(|kql| {
                kql.lock()
                    .map_err(|_| VNextProductRuntimeError::KqlLockPoisoned)?
                    .durable_match_count()
                    .map_err(VNextProductRuntimeError::from)
            })
            .transpose()?
            .unwrap_or_default();
        let pending_publications = self
            .runtime
            .public_use
            .as_ref()
            .map(|publisher| publisher.pending_publication_count())
            .transpose()?
            .unwrap_or_default();
        let storage_bytes = self.runtime.storage.used_bytes()?;
        Ok(VNextProductRuntimeStatus {
            state: self.runtime.state,
            signer_mode: self.runtime.signer_mode,
            network: self.network_status(),
            authenticated_routes: self
                .runtime
                .network
                .as_ref()
                .map(|network| network.authenticated_route_count())
                .transpose()?
                .unwrap_or_default(),
            active_private_needs,
            durable_matches,
            pending_publications,
            policy_versions: self
                .runtime
                .policy_versions
                .iter()
                .map(|version| version.get())
                .collect(),
            lanes: self.runtime.lanes,
            budgets: self.runtime.budgets,
            storage_bytes,
            storage_pressure: self.runtime.storage.pressure(storage_bytes),
            active_product_workers: self.runtime.workers.len(),
            max_product_workers: self.runtime.workers.capacity(),
            cancellation_requested: self.runtime.workers.is_cancelled(),
            startup_trace: self.runtime.startup_trace.clone(),
            shutdown_trace: self.runtime.shutdown_trace.clone(),
            rehydrated_private_needs: self.runtime.rehydrated_private_needs,
            startup_pending_publications: self.runtime.startup_pending_publications,
            worker_poll_ticks: self.runtime.workers.poll_ticks(),
            changes_wallet_state: false,
            changes_obt_state: false,
            claims_network_completion: false,
        })
    }

    pub async fn connect_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, VNextProductRuntimeError> {
        self.runtime
            .network()?
            .connect(addr)
            .await
            .map_err(Into::into)
    }

    pub fn authenticated_route(
        &self,
        peer: NodeId,
    ) -> Result<Option<AuthenticatedRoute>, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime
            .network()?
            .authenticated_route(peer)
            .map_err(Into::into)
    }

    pub fn register_private_need(
        &self,
        bundle: PrivateNeedBundle,
    ) -> Result<(StandingNeedId, StandingNeedWriteOutcome), VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .kql()?
            .register_private_need(bundle)
            .map_err(Into::into)
    }

    pub fn standing_need(
        &self,
        id: StandingNeedId,
    ) -> Result<Option<StandingNeed>, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.kql()?.standing_need(id).map_err(Into::into)
    }

    pub fn pause_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .kql()?
            .pause(id, expected_generation)
            .map_err(Into::into)
    }

    pub fn resume_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .kql()?
            .resume(id, expected_generation)
            .map_err(Into::into)
    }

    pub fn cancel_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .kql()?
            .cancel(id, expected_generation)
            .map_err(Into::into)
    }

    pub fn retire_private_need(
        &self,
        id: StandingNeedId,
        expected_generation: u64,
    ) -> Result<u64, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .kql()?
            .retire(id, expected_generation)
            .map_err(Into::into)
    }

    pub fn process_one_hop_affordance_delta(
        &self,
        selector: SelectorCid,
        budget: DistributedKqlBudget,
    ) -> Result<DistributedKqlReport, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_kql_budget(budget)?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .kql()?
            .process_one_hop_affordance_delta(self.runtime.network()?, selector, budget)
            .map_err(Into::into)
    }

    pub fn prepare_public_use(
        &self,
        request: &PreparePublicUseEvidenceRequest,
        author: &ValidatedFeedInception,
    ) -> Result<PreparedPublicUseIntent, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
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
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .publisher()?
            .publish_confirmed(request, author, signer)
            .map_err(Into::into)
    }

    pub fn flush_pending_public_use(
        &self,
        limit: usize,
    ) -> Result<PublicUseFlushReport, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        if limit > self.runtime.budgets.publication_flush_batch {
            return Err(VNextProductRuntimeError::BudgetExceeded(
                VNextFeature::PublicUseEvidencePublish,
            ));
        }
        self.runtime
            .publisher()?
            .flush_pending(self.runtime.network()?, limit)
            .map_err(Into::into)
    }

    pub fn materialize_public_use_view(
        &self,
        selector: SelectorCid,
        target: ObjectReference,
        policy_version: LocalPolicyVersion,
    ) -> Result<DistributedPomvReport, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.ensure_storage_writable()?;
        self.runtime
            .pomv()?
            .materialize_public_use_view(self.runtime.network()?, selector, target, policy_version)
            .map_err(Into::into)
    }

    pub fn proposal(
        &self,
        id: ProposalId,
    ) -> Result<Option<ku_kql::vnext_proposal::BindingProposal>, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        Ok(self.runtime.kql()?.proposal(id).cloned())
    }
}

struct StartupArtifactGuard {
    data_dir: PathBuf,
    preexisting: BTreeSet<&'static str>,
    data_dir_preexisting: bool,
    committed: bool,
}

impl StartupArtifactGuard {
    fn new(data_dir: &Path) -> Result<Self, VNextProductRuntimeError> {
        let data_dir_preexisting = data_dir.exists();
        if data_dir_preexisting && !data_dir.is_dir() {
            return Err(VNextProductRuntimeError::InvalidDataDirectory(
                data_dir.to_path_buf(),
            ));
        }
        let preexisting = VNEXT_STARTUP_ARTIFACTS
            .iter()
            .copied()
            .filter(|name| data_dir.join(name).exists())
            .collect();
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            preexisting,
            data_dir_preexisting,
            committed: false,
        })
    }

    fn new_artifacts(&self) -> Vec<PathBuf> {
        VNEXT_STARTUP_ARTIFACTS
            .iter()
            .copied()
            .filter(|name| !self.preexisting.contains(name))
            .map(|name| self.data_dir.join(name))
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

    fn is_cancelled(&self) -> bool {
        *self.cancellation.borrow()
    }

    fn poll_ticks(&self) -> u64 {
        self.poll_ticks.load(Ordering::Relaxed)
    }

    fn start_lane_workers(
        &mut self,
        lanes: VNextProductLaneStatus,
        poll_interval_millis: u64,
        publication: Option<(Arc<PublicUseEvidencePublisher>, Arc<VNextNetworkRuntime>)>,
        publication_flush_batch: usize,
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
            self.spawn(kind, move |mut cancellation| async move {
                let mut ticker = tokio::time::interval(interval);
                loop {
                    tokio::select! {
                        _ = cancellation.cancelled() => break,
                        _ = ticker.tick() => {
                            poll_ticks.fetch_add(1, Ordering::Relaxed);
                            if let Some((publisher, network)) = publication.as_ref() {
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
    use crate::vnext_config::VNextFeatureConfig;
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};

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
    }

    #[tokio::test]
    async fn independent_lane_kill_switches_prevent_store_creation_and_operations() {
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
            assert!(!directory.path().join(file).exists(), "unexpected {file}");
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
            Err(VNextProductRuntimeError::LaneDisabled(
                VNextFeature::DistributedKqlOneHop
            ))
        ));
        runtime.shutdown().await;
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
        runtime.storage.hard_watermark_bytes = 1;
        assert!(matches!(
            runtime.services().flush_pending_public_use(1),
            Err(VNextProductRuntimeError::StorageHardWatermark { .. })
        ));
        runtime.shutdown().await;
    }
}
