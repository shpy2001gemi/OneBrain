//! Node-owned aggregate and typed service façade for vNext product lanes.
//!
//! Product and API code receives [`VNextProductServices`], never references to
//! the network, KQL, publication, or PoMV runtimes that this aggregate owns.

#![cfg(feature = "vnext-network-runtime")]

use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

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

use crate::vnext_config::VNextNetworkPolicy;
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
    OutboundVNextSession, VNextNetworkRuntime, VNextNetworkRuntimeError, VNextNetworkRuntimeStatus,
};
use crate::vnext_route_authority::{AuthenticatedRoute, LocalPolicyRegistry, LocalPolicyVersion};

pub const MAX_PRODUCT_BACKGROUND_WORKERS: usize = 8;
pub const DEFAULT_PRODUCT_POMV_RECORDS: usize = 4_096;

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
pub enum VNextProductSignerMode {
    CallerOwned,
    CompatibilityFile,
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
    pub active_product_workers: usize,
    pub max_product_workers: usize,
    pub cancellation_requested: bool,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub claims_network_completion: bool,
}

/// Sole owner of the first integrated vNext product runtime slice.
///
/// Raw subsystem references intentionally have no public accessor. All product
/// operations cross [`VNextProductServices`].
pub struct VNextProductRuntime {
    network: VNextNetworkRuntime,
    distributed_kql: Mutex<DistributedKqlRuntime>,
    public_use: PublicUseEvidencePublisher,
    distributed_pomv: DistributedPomvRuntime,
    policy_versions: Vec<LocalPolicyVersion>,
    workers: BoundedProductWorkers,
    signer_mode: VNextProductSignerMode,
    state: VNextProductRuntimeState,
}

impl VNextProductRuntime {
    pub async fn start(
        data_dir: &Path,
        bind_addr: SocketAddr,
        network_policy: VNextNetworkPolicy,
        dependencies: VNextProductRuntimeDependencies,
        identity_signer: Option<Arc<dyn SessionIdentitySigner>>,
    ) -> Result<Self, VNextProductRuntimeError> {
        let VNextProductRuntimeDependencies {
            private_need_vault_key,
            policies,
        } = dependencies;
        let policy_versions = policies.versions();

        // Durable product stores are opened before the listener. This makes
        // dependency failure visible without leaving a live network runtime.
        let distributed_kql = DistributedKqlRuntime::open(data_dir, private_need_vault_key)?;
        let public_use = PublicUseEvidencePublisher::open(data_dir)?;
        let distributed_pomv =
            DistributedPomvRuntime::open(data_dir, DEFAULT_PRODUCT_POMV_RECORDS, policies)?;
        let signer_mode = if identity_signer.is_some() {
            VNextProductSignerMode::CallerOwned
        } else {
            VNextProductSignerMode::CompatibilityFile
        };
        let network = match identity_signer {
            Some(signer) => {
                VNextNetworkRuntime::start_with_signer(data_dir, bind_addr, network_policy, signer)
                    .await?
            }
            None => VNextNetworkRuntime::start(data_dir, bind_addr, network_policy).await?,
        };

        Ok(Self {
            network,
            distributed_kql: Mutex::new(distributed_kql),
            public_use,
            distributed_pomv,
            policy_versions,
            workers: BoundedProductWorkers::new(MAX_PRODUCT_BACKGROUND_WORKERS),
            signer_mode,
            state: VNextProductRuntimeState::Running,
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
        self.workers.shutdown().await;
        self.network.shutdown().await;
    }

    fn ensure_running(&self) -> Result<(), VNextProductRuntimeError> {
        if self.state == VNextProductRuntimeState::Running {
            Ok(())
        } else {
            Err(VNextProductRuntimeError::Stopped)
        }
    }

    fn kql(&self) -> Result<MutexGuard<'_, DistributedKqlRuntime>, VNextProductRuntimeError> {
        self.distributed_kql
            .lock()
            .map_err(|_| VNextProductRuntimeError::KqlLockPoisoned)
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
        self.runtime.network.local_addr()
    }

    pub fn network_status(&self) -> VNextNetworkRuntimeStatus {
        self.runtime.network.status()
    }

    pub fn status(&self) -> Result<VNextProductRuntimeStatus, VNextProductRuntimeError> {
        let kql = self.runtime.kql()?;
        Ok(VNextProductRuntimeStatus {
            state: self.runtime.state,
            signer_mode: self.runtime.signer_mode,
            network: self.runtime.network.status(),
            authenticated_routes: self.runtime.network.authenticated_route_count()?,
            active_private_needs: kql.active_target_count(),
            durable_matches: kql.durable_match_count()?,
            pending_publications: self.runtime.public_use.pending_publication_count()?,
            policy_versions: self
                .runtime
                .policy_versions
                .iter()
                .map(|version| version.get())
                .collect(),
            active_product_workers: self.runtime.workers.len(),
            max_product_workers: self.runtime.workers.capacity(),
            cancellation_requested: self.runtime.workers.is_cancelled(),
            changes_wallet_state: false,
            changes_obt_state: false,
            claims_network_completion: false,
        })
    }

    pub async fn connect_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime.network.connect(addr).await.map_err(Into::into)
    }

    pub fn authenticated_route(
        &self,
        peer: NodeId,
    ) -> Result<Option<AuthenticatedRoute>, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime
            .network
            .authenticated_route(peer)
            .map_err(Into::into)
    }

    pub fn register_private_need(
        &self,
        bundle: PrivateNeedBundle,
    ) -> Result<(StandingNeedId, StandingNeedWriteOutcome), VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
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
        self.runtime
            .kql()?
            .process_one_hop_affordance_delta(&self.runtime.network, selector, budget)
            .map_err(Into::into)
    }

    pub fn prepare_public_use(
        &self,
        request: &PreparePublicUseEvidenceRequest,
        author: &ValidatedFeedInception,
    ) -> Result<PreparedPublicUseIntent, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime
            .public_use
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
        self.runtime
            .public_use
            .publish_confirmed(request, author, signer)
            .map_err(Into::into)
    }

    pub fn flush_pending_public_use(
        &self,
        limit: usize,
    ) -> Result<PublicUseFlushReport, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime
            .public_use
            .flush_pending(&self.runtime.network, limit)
            .map_err(Into::into)
    }

    pub fn materialize_public_use_view(
        &self,
        selector: SelectorCid,
        target: ObjectReference,
        policy_version: LocalPolicyVersion,
    ) -> Result<DistributedPomvReport, VNextProductRuntimeError> {
        self.runtime.ensure_running()?;
        self.runtime
            .distributed_pomv
            .materialize_public_use_view(&self.runtime.network, selector, target, policy_version)
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

#[derive(Clone)]
#[allow(dead_code)] // Concrete worker loops are registered by P2.3.
struct ProductCancellation {
    receiver: watch::Receiver<bool>,
}

#[allow(dead_code)] // Concrete worker loops are registered by P2.3.
impl ProductCancellation {
    async fn cancelled(&mut self) {
        while !*self.receiver.borrow() {
            if self.receiver.changed().await.is_err() {
                break;
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }
}

struct BoundedProductWorkers {
    cancellation: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
    max_workers: usize,
}

impl BoundedProductWorkers {
    fn new(max_workers: usize) -> Self {
        let (cancellation, _) = watch::channel(false);
        Self {
            cancellation,
            tasks: Vec::new(),
            max_workers,
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

    /// P2.3 will register the concrete product loops through this bounded
    /// owner. P2.1 freezes ownership and cancellation semantics first.
    #[allow(dead_code)]
    fn spawn<F, Fut>(&mut self, worker: F) -> Result<(), VNextProductRuntimeError>
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
        self.tasks.push(tokio::spawn(worker(token)));
        Ok(())
    }

    async fn shutdown(&mut self) {
        self.cancellation.send_replace(true);
        for mut task in self.tasks.drain(..) {
            task.abort();
            let _ = (&mut task).await;
        }
    }

    fn cancel_and_abort(&mut self) {
        self.cancellation.send_replace(true);
        for task in self.tasks.drain(..) {
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
    #[error("vNext network runtime failed: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("vNext distributed KQL runtime failed: {0}")]
    DistributedKql(#[from] DistributedKqlError),
    #[error("vNext distributed PoMV runtime failed: {0}")]
    DistributedPomv(#[from] DistributedPomvError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};

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

    #[tokio::test]
    async fn aggregate_owns_every_runtime_and_exposes_typed_services() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let left_signer: Arc<dyn SessionIdentitySigner> =
            Arc::new(SigningKey::from_bytes(&[0x31; 32]));
        let mut left = VNextProductRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            dependencies(10),
            Some(left_signer),
        )
        .await
        .unwrap();
        let mut right = VNextProductRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
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
        assert_eq!(before.active_product_workers, 0);
        assert_eq!(before.max_product_workers, MAX_PRODUCT_BACKGROUND_WORKERS);
        assert!(!before.cancellation_requested);
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
        assert!(matches!(
            left.services().authenticated_route(right_principal),
            Err(VNextProductRuntimeError::Stopped)
        ));
    }

    #[tokio::test]
    async fn product_worker_owner_is_bounded_and_cancelled() {
        let directory = tempfile::tempdir().unwrap();
        let mut runtime = VNextProductRuntime::start(
            directory.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            dependencies(30),
            None,
        )
        .await
        .unwrap();
        for _ in 0..MAX_PRODUCT_BACKGROUND_WORKERS {
            runtime
                .workers
                .spawn(|mut cancellation| async move {
                    cancellation.cancelled().await;
                })
                .unwrap();
        }
        assert!(matches!(
            runtime.workers.spawn(|_| async {}),
            Err(VNextProductRuntimeError::WorkerCapacityReached)
        ));
        let cancellation = ProductCancellation {
            receiver: runtime.workers.cancellation.subscribe(),
        };
        runtime.shutdown().await;
        assert!(cancellation.is_cancelled());
        assert_eq!(runtime.workers.len(), 0);
    }
}
