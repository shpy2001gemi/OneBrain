//! Authenticated, bounded OBP-RP runtime over real QUIC transport.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use ku_core::foundation::{
    FeedId, FeedProjection, InventoryRecordKind, NodeId, RedbVerifiedBackend,
};
use ku_net::transport::{OBPConnection, QuicTransport, TransportConfig};
use ku_net::vnext_carrier::CarrierRecord;
use ku_net::vnext_inventory_forest::{InventoryLeaf, RedbInventoryForestBackend};
use ku_net::vnext_quic_session::{
    accept_authenticated_session, initiate_authenticated_session, send_carrier_record,
    AuthenticatedCarrierRecord, AuthenticatedCarrierSession,
};
use ku_net::vnext_reconciliation::{
    BoundPayloadFrame, PayloadIngestOutcome, PayloadRejectReason, PayloadSinkOutcome,
    ReceiverState, ValidateThenAcceptSink,
};
use ku_net::vnext_reconciliation_journal::persistent::RedbReconciliationJournalBackend;
use ku_net::vnext_reconciliation_journal::{
    JournaledPayloadOutcome, JournaledReconciliationSession, ReconciliationJournalConfig,
};
use ku_net::vnext_resource_gate::{
    AdmissionStage, HandshakeAdmission, ResourceAdmissionError, ResourceUsage,
    RuntimeAdmissionController, RuntimeAdmissionLimits, SessionAdmission,
};
use ku_net::vnext_session::{
    principal_node_id, AuthenticatedSession, SessionIdentitySigner, SessionReplayGuard,
};
use onebrain_protocol::{
    bind_reconciliation_message, decode_reconciliation_message, encode_reconciliation_message,
    reconciliation_capability, reconciliation_profile, ReconcileManifestEntry,
    ReconcileReceiptStatus, ReconciliationBody, ReconciliationBudget, ReconciliationContext,
    ReconciliationResumeMode, ReconciliationSummaryMethod,
};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{watch, Mutex as AsyncMutex, Notify};
use tokio::task::JoinHandle;

use crate::vnext_config::VNextNetworkPolicy;
use crate::vnext_observability::{
    VNextJournalObservation, VNextObservability, VNextObservabilitySnapshot, VNextReasonCode,
    VNextRegistryTelemetryState,
};
use crate::vnext_outbox::{
    OutboundIntentState, OutboundOutbox, OutboundTransferIntent, OutboxEnqueueOutcome,
    MAX_OUTBOX_PAYLOAD_BYTES,
};
use crate::vnext_record_provenance::RedbRecordProvenance;
use crate::vnext_route_authority::{
    resolve_authority_frontier, AuthenticatedRoute, AuthenticatedRouteDirectory,
    AuthorityFrontierResolution, AuthorityResolverError, RouteDirectoryError,
};
use crate::vnext_runtime_rollout::{
    VNextRuntimeGenerationLease, VNextRuntimeLane, VNextRuntimeRollout,
};
use crate::vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};

const IDENTITY_MAGIC: &[u8; 8] = b"OBIDV1\0\0";
const IDENTITY_BYTES: usize = 40;
const OUTBOUND_RETRY_BASE: Duration = Duration::from_millis(250);
const OUTBOUND_RETRY_MAX: Duration = Duration::from_secs(30);
const DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES: u64 = 1_024 * 1_048_576;

type PersistentSink = SharedVNextValidatedSink<RedbVerifiedBackend>;
type PersistentJournal =
    JournaledReconciliationSession<RedbReconciliationJournalBackend, InventoryingSink>;

#[derive(Clone)]
struct InventoryingSink {
    inner: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    selector: ku_core::foundation::SelectorCid,
    source_peer: NodeId,
    storage: NetworkStorageAdmission,
    observability: Arc<VNextObservability>,
}

impl ValidateThenAcceptSink for InventoryingSink {
    fn validate_then_accept(
        &mut self,
        kind: onebrain_protocol::ReconcileManifestKind,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        if let Err(error) = self.storage.ensure_writable(canonical_bytes.len() as u64) {
            self.observability.record(
                VNextReasonCode::RejectedStorage,
                canonical_bytes.len() as u64,
                1,
            );
            return Err(error);
        }
        let outcome = self
            .inner
            .validate_then_accept(kind, cid, canonical_bytes)?;
        if matches!(
            outcome,
            PayloadSinkOutcome::ValidatedStored | PayloadSinkOutcome::AlreadyPresent
        ) {
            let record_kind = match kind {
                onebrain_protocol::ReconcileManifestKind::Object => InventoryRecordKind::Object,
                onebrain_protocol::ReconcileManifestKind::Event => InventoryRecordKind::Event,
                onebrain_protocol::ReconcileManifestKind::MappingKernel => {
                    InventoryRecordKind::MappingKernel
                }
                onebrain_protocol::ReconcileManifestKind::FeedInception => {
                    InventoryRecordKind::FeedInception
                }
                onebrain_protocol::ReconcileManifestKind::AuthorityEvent => {
                    InventoryRecordKind::AuthorityEvent
                }
            };
            self.inventory
                .insert_record(
                    self.selector,
                    InventoryLeaf {
                        record_kind,
                        cid,
                        canonical_length: u64::try_from(canonical_bytes.len())
                            .map_err(|_| "VNEXT_INVENTORY_LENGTH_OVERFLOW".to_string())?,
                    },
                )
                .map_err(|error| format!("VNEXT_INVENTORY: {error:?}"))?;
            match self.inner.accepted_record_type(kind, canonical_bytes)? {
                Some(type_id) => self
                    .provenance
                    .observe_typed(
                        kind,
                        type_id,
                        cid,
                        canonical_bytes,
                        self.selector,
                        self.source_peer,
                    )
                    .map_err(|error| format!("VNEXT_PROVENANCE: {error}"))?,
                None => self
                    .provenance
                    .observe(kind, cid, self.selector, self.source_peer)
                    .map_err(|error| format!("VNEXT_PROVENANCE: {error}"))?,
            }
        }
        Ok(outcome)
    }
}

#[derive(Clone)]
struct NetworkStorageAdmission {
    data_dir: PathBuf,
    hard_watermark_bytes: u64,
}

impl NetworkStorageAdmission {
    fn used_bytes(&self) -> Result<u64, String> {
        let entries = std::fs::read_dir(&self.data_dir).map_err(|error| error.to_string())?;
        let mut used = 0u64;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry.file_name().to_string_lossy().starts_with("vnext_") {
                continue;
            }
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            if metadata.is_file() {
                used = used
                    .checked_add(metadata.len())
                    .ok_or_else(|| "VNEXT_STORAGE_SIZE_OVERFLOW".to_string())?;
            }
        }
        Ok(used)
    }

    fn ensure_writable(&self, incoming_bytes: u64) -> Result<(), String> {
        let projected = self
            .used_bytes()?
            .checked_add(incoming_bytes)
            .ok_or_else(|| "VNEXT_STORAGE_SIZE_OVERFLOW".to_string())?;
        if projected > self.hard_watermark_bytes {
            Err("VNEXT_STORAGE_HARD_WATERMARK".to_string())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VNextNetworkRuntimeState {
    Listening,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextNetworkRuntimeStatus {
    pub state: VNextNetworkRuntimeState,
    pub listen_addr: SocketAddr,
    pub principal: [u8; 32],
    pub authenticated_sessions: u64,
    pub active_sessions: usize,
    pub rejected_sessions: u64,
    pub accepted_records: u64,
    pub deferred_records: u64,
    pub rejected_records: u64,
    pub observability: VNextObservabilitySnapshot,
    pub claims_network_completion: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundDeliveryReport {
    pub scanned: usize,
    pub attempted: usize,
    pub acknowledged: usize,
    pub deferred: usize,
    pub rejected: usize,
    pub failed: usize,
    pub retry_exhausted: usize,
    pub claims_network_completion: bool,
}

#[derive(Default)]
struct RuntimeCounters {
    authenticated_sessions: AtomicU64,
    active_sessions: AtomicUsize,
    rejected_sessions: AtomicU64,
    accepted_records: AtomicU64,
    deferred_records: AtomicU64,
    rejected_records: AtomicU64,
}

struct OutboundDeliveryEngine {
    transport: Arc<QuicTransport>,
    identity: Arc<dyn SessionIdentitySigner>,
    replay_guard: Arc<Mutex<SessionReplayGuard>>,
    routes: AuthenticatedRouteDirectory,
    principal: NodeId,
    counters: Arc<RuntimeCounters>,
    observability: Arc<VNextObservability>,
    admission: RuntimeAdmissionController,
    outbox: OutboundOutbox,
    scheduler: AsyncMutex<()>,
    policy: VNextNetworkPolicy,
    rollout: Option<VNextRuntimeRollout>,
}

/// Owns the real QUIC endpoint, persistent validated store, persistent
/// reconciliation journal, session replay guard, and bounded accept loop.
pub struct VNextNetworkRuntime {
    transport: Arc<QuicTransport>,
    principal: NodeId,
    listen_addr: SocketAddr,
    counters: Arc<RuntimeCounters>,
    observability: Arc<VNextObservability>,
    validated_sink: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    routes: AuthenticatedRouteDirectory,
    outbound: Arc<OutboundDeliveryEngine>,
    outbound_notify: Arc<Notify>,
    outbound_shutdown: watch::Sender<bool>,
    outbound_task: Option<JoinHandle<()>>,
    accept_task: JoinHandle<()>,
    state: VNextNetworkRuntimeState,
}

/// Identity material that has already passed proof-of-possession validation.
///
/// Product startup prepares this value before opening any durable network
/// stores, so an unavailable or mismatched external signer fails without
/// leaving a partially initialized runtime behind.
pub(crate) struct PreparedVNextIdentity {
    signer: Arc<dyn SessionIdentitySigner>,
    public_key: [u8; 32],
}

#[derive(Clone, Debug)]
pub(crate) struct VNextNetworkStoragePaths {
    pub admission_root: PathBuf,
    pub canonical: PathBuf,
    pub reconciliation: PathBuf,
    pub inventory: PathBuf,
    pub provenance: PathBuf,
    pub outbox: PathBuf,
}

impl VNextNetworkStoragePaths {
    fn legacy(data_dir: &Path) -> Self {
        Self {
            admission_root: data_dir.to_path_buf(),
            canonical: data_dir.join("vnext_verified.redb"),
            reconciliation: data_dir.join("vnext_reconciliation.redb"),
            inventory: data_dir.join("vnext_inventory.redb"),
            provenance: data_dir.join("vnext_record_provenance.redb"),
            outbox: data_dir.join("vnext_outbox.redb"),
        }
    }
}

pub(crate) fn prepare_vnext_identity(
    data_dir: &Path,
    identity: Option<Arc<dyn SessionIdentitySigner>>,
) -> Result<PreparedVNextIdentity, VNextNetworkRuntimeError> {
    let signer: Arc<dyn SessionIdentitySigner> = match identity {
        Some(identity) => identity,
        None => {
            std::fs::create_dir_all(data_dir)?;
            Arc::new(load_or_create_identity(
                &data_dir.join("vnext_identity.key"),
            )?)
        }
    };
    let public_key = validate_identity_signer(signer.as_ref())?;
    Ok(PreparedVNextIdentity { signer, public_key })
}

pub(crate) fn prepare_vnext_identity_caller_owned(
    identity: Option<Arc<dyn SessionIdentitySigner>>,
) -> Result<PreparedVNextIdentity, VNextNetworkRuntimeError> {
    let signer = identity.ok_or(VNextNetworkRuntimeError::IdentityReprovisionRequired)?;
    let public_key = validate_identity_signer(signer.as_ref())?;
    Ok(PreparedVNextIdentity { signer, public_key })
}

impl VNextNetworkRuntime {
    /// Start with the built-in local file signer. This is a compatibility and
    /// development path; production deployments should use
    /// [`Self::start_with_signer`] with an OS keystore, HSM or remote signer.
    pub async fn start(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        let identity = prepare_vnext_identity(data_dir, None)?;
        std::fs::create_dir_all(data_dir)?;
        Self::start_initialized(
            data_dir,
            bind_addr,
            policy,
            true,
            DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
            identity.signer,
            identity.public_key,
            None,
        )
        .await
    }

    #[cfg(test)]
    async fn start_inner(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        continuous_outbound: bool,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        policy
            .validate()
            .map_err(|error| VNextNetworkRuntimeError::Config(error.to_string()))?;
        let identity = prepare_vnext_identity(data_dir, None)?;
        std::fs::create_dir_all(data_dir)?;
        Self::start_initialized(
            data_dir,
            bind_addr,
            policy,
            continuous_outbound,
            DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
            identity.signer,
            identity.public_key,
            None,
        )
        .await
    }

    /// Start a bounded canary runtime with an explicit storage watermark and
    /// optional outbound worker. This is compiled only for the P5 harness so
    /// production callers cannot bypass the product-owned startup path.
    #[cfg(feature = "vnext-canary-harness")]
    pub(crate) async fn start_canary_harness(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        continuous_outbound: bool,
        storage_hard_watermark_bytes: u64,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        policy
            .validate()
            .map_err(|error| VNextNetworkRuntimeError::Config(error.to_string()))?;
        let identity = prepare_vnext_identity(data_dir, None)?;
        std::fs::create_dir_all(data_dir)?;
        Self::start_initialized(
            data_dir,
            bind_addr,
            policy,
            continuous_outbound,
            storage_hard_watermark_bytes,
            identity.signer,
            identity.public_key,
            None,
        )
        .await
    }

    pub(crate) async fn start_prepared_with_paths(
        paths: &VNextNetworkStoragePaths,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        storage_hard_watermark_bytes: u64,
        identity: PreparedVNextIdentity,
        rollout: VNextRuntimeRollout,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        policy
            .validate()
            .map_err(|error| VNextNetworkRuntimeError::Config(error.to_string()))?;
        std::fs::create_dir_all(&paths.admission_root)?;
        Self::start_initialized_with_paths(
            paths,
            bind_addr,
            policy,
            true,
            storage_hard_watermark_bytes,
            identity.signer,
            identity.public_key,
            Some(rollout),
        )
        .await
    }

    /// Start with a caller-owned signer. Only the public key and signature
    /// operation cross this boundary; private key material need never enter
    /// OneBrain memory or its data directory.
    pub async fn start_with_signer(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        identity: Arc<dyn SessionIdentitySigner>,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        let identity = prepare_vnext_identity(data_dir, Some(identity))?;
        policy
            .validate()
            .map_err(|error| VNextNetworkRuntimeError::Config(error.to_string()))?;
        std::fs::create_dir_all(data_dir)?;
        Self::start_initialized(
            data_dir,
            bind_addr,
            policy,
            true,
            DEFAULT_NETWORK_STORAGE_HARD_WATERMARK_BYTES,
            identity.signer,
            identity.public_key,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_initialized(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        continuous_outbound: bool,
        storage_hard_watermark_bytes: u64,
        identity: Arc<dyn SessionIdentitySigner>,
        identity_public_key: [u8; 32],
        rollout: Option<VNextRuntimeRollout>,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        let paths = VNextNetworkStoragePaths::legacy(data_dir);
        Self::start_initialized_with_paths(
            &paths,
            bind_addr,
            policy,
            continuous_outbound,
            storage_hard_watermark_bytes,
            identity,
            identity_public_key,
            rollout,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_initialized_with_paths(
        paths: &VNextNetworkStoragePaths,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        continuous_outbound: bool,
        storage_hard_watermark_bytes: u64,
        identity: Arc<dyn SessionIdentitySigner>,
        identity_public_key: [u8; 32],
        rollout: Option<VNextRuntimeRollout>,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        let principal = principal_node_id(&identity_public_key);
        if storage_hard_watermark_bytes == 0 {
            return Err(VNextNetworkRuntimeError::Config(
                "storage hard watermark must be non-zero".to_string(),
            ));
        }
        let storage_admission = NetworkStorageAdmission {
            data_dir: paths.admission_root.clone(),
            hard_watermark_bytes: storage_hard_watermark_bytes,
        };
        let sink = SharedVNextValidatedSink::new(VNextValidatedSink::new(
            RedbVerifiedBackend::open(&paths.canonical)
                .map_err(VNextNetworkRuntimeError::Storage)?,
        ));
        let journal = RedbReconciliationJournalBackend::open(paths.reconciliation.clone())
            .map_err(VNextNetworkRuntimeError::Storage)?;
        let inventory = RedbInventoryForestBackend::open(&paths.inventory)
            .map_err(|error| VNextNetworkRuntimeError::Inventory(format!("{error:?}")))?;
        let provenance = RedbRecordProvenance::open(paths.provenance.clone())
            .map_err(VNextNetworkRuntimeError::Provenance)?;
        let outbox = OutboundOutbox::open(&paths.outbox)
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;

        let transport = Arc::new(
            QuicTransport::bind(TransportConfig {
                bind_addr,
                max_bi_streams: policy.max_concurrent_sessions.min(u32::MAX as usize) as u32,
                max_uni_streams: policy.max_concurrent_sessions.min(u32::MAX as usize) as u32,
                ..TransportConfig::default()
            })
            .await
            .map_err(|error| VNextNetworkRuntimeError::Transport(error.to_string()))?,
        );
        let listen_addr = transport
            .local_addr()
            .map_err(|error| VNextNetworkRuntimeError::Transport(error.to_string()))?;
        let replay_guard = Arc::new(Mutex::new(
            SessionReplayGuard::with_capacity(policy.max_replay_entries)
                .map_err(|error| VNextNetworkRuntimeError::Config(format!("{error:?}")))?,
        ));
        let routes = AuthenticatedRouteDirectory::default();
        let counters = Arc::new(RuntimeCounters::default());
        let observability = Arc::new(VNextObservability::default());
        let outbox_stats = outbox
            .stats()
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
        observability.observe_outbox(
            outbox_stats.pending,
            outbox_stats.retry_exhausted,
            outbox_stats.oldest_pending_age_seconds,
        );
        let admission = RuntimeAdmissionController::new(admission_limits(policy))
            .map_err(|error| VNextNetworkRuntimeError::Config(format!("{error:?}")))?;
        let accept_task = tokio::spawn(accept_loop(
            Arc::clone(&transport),
            Arc::clone(&identity),
            Arc::clone(&replay_guard),
            routes.clone(),
            principal,
            Arc::clone(&counters),
            Arc::clone(&observability),
            admission.clone(),
            journal,
            sink.clone(),
            inventory.clone(),
            provenance.clone(),
            storage_admission,
            policy,
            rollout.clone(),
        ));
        let outbound = Arc::new(OutboundDeliveryEngine {
            transport: Arc::clone(&transport),
            identity: Arc::clone(&identity),
            replay_guard: Arc::clone(&replay_guard),
            routes: routes.clone(),
            principal,
            counters: Arc::clone(&counters),
            observability: Arc::clone(&observability),
            admission,
            outbox,
            scheduler: AsyncMutex::new(()),
            policy,
            rollout,
        });
        let outbound_notify = Arc::new(Notify::new());
        let (outbound_shutdown, shutdown_rx) = watch::channel(false);
        let outbound_task = continuous_outbound.then(|| {
            tokio::spawn(run_outbound_scheduler(
                Arc::clone(&outbound),
                Arc::clone(&outbound_notify),
                shutdown_rx,
            ))
        });
        Ok(Self {
            transport,
            principal,
            listen_addr,
            counters,
            observability,
            validated_sink: sink,
            inventory,
            provenance,
            routes,
            outbound,
            outbound_notify,
            outbound_shutdown,
            outbound_task,
            accept_task,
            state: VNextNetworkRuntimeState::Listening,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    pub fn status(&self) -> VNextNetworkRuntimeStatus {
        if let Err(error) = self.outbound.refresh_outbox_observability() {
            self.observability
                .record(VNextReasonCode::JournalFailure, 0, 1);
            tracing::warn!(
                target: "onebrain::vnext::observability",
                reason_code = VNextReasonCode::JournalFailure.code(),
                error = %error,
                "vNext outbox gauge refresh failed"
            );
        }
        VNextNetworkRuntimeStatus {
            state: self.state,
            listen_addr: self.listen_addr,
            principal: *self.principal.as_bytes(),
            authenticated_sessions: self.counters.authenticated_sessions.load(Ordering::Relaxed),
            active_sessions: self.counters.active_sessions.load(Ordering::Relaxed),
            rejected_sessions: self.counters.rejected_sessions.load(Ordering::Relaxed),
            accepted_records: self.counters.accepted_records.load(Ordering::Relaxed),
            deferred_records: self.counters.deferred_records.load(Ordering::Relaxed),
            rejected_records: self.counters.rejected_records.load(Ordering::Relaxed),
            observability: self
                .observability
                .snapshot(VNextRegistryTelemetryState::Unknown),
            claims_network_completion: false,
        }
    }

    pub(crate) fn observability(&self) -> Arc<VNextObservability> {
        Arc::clone(&self.observability)
    }

    pub fn inventory_root(
        &self,
        selector: ku_core::foundation::SelectorCid,
    ) -> Result<[u8; 32], VNextNetworkRuntimeError> {
        self.inventory
            .load(selector)
            .map(|forest| forest.root())
            .map_err(|error| VNextNetworkRuntimeError::Inventory(format!("{error:?}")))
    }

    pub(crate) fn typed_record_delta(
        &self,
        selector: ku_core::foundation::SelectorCid,
        kind: onebrain_protocol::ReconcileManifestKind,
        type_id: u64,
        after_sequence: u64,
        limit: usize,
    ) -> Result<crate::vnext_record_provenance::IndexedTypedDelta, VNextNetworkRuntimeError> {
        self.provenance
            .typed_delta(selector, kind, type_id, after_sequence, limit)
            .map_err(VNextNetworkRuntimeError::Provenance)
    }

    pub fn feed_inception_branches(
        &self,
        feed_id: FeedId,
    ) -> Result<Vec<ku_core::foundation::ValidatedFeedInception>, VNextNetworkRuntimeError> {
        self.validated_sink
            .feed_inceptions(feed_id)
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    pub fn record_source_peers(
        &self,
        kind: onebrain_protocol::ReconcileManifestKind,
        cid: [u8; 32],
        selector: ku_core::foundation::SelectorCid,
    ) -> Result<Vec<NodeId>, VNextNetworkRuntimeError> {
        self.provenance
            .peers(kind, cid, selector)
            .map_err(VNextNetworkRuntimeError::Provenance)
    }

    pub fn authenticated_route(
        &self,
        peer: NodeId,
    ) -> Result<Option<AuthenticatedRoute>, VNextNetworkRuntimeError> {
        self.routes
            .resolve(peer)
            .map_err(VNextNetworkRuntimeError::RouteDirectory)
    }

    pub fn authenticated_route_count(&self) -> Result<usize, VNextNetworkRuntimeError> {
        self.routes
            .len()
            .map_err(VNextNetworkRuntimeError::RouteDirectory)
    }

    /// Resolve authority exclusively from locally validated authority records.
    /// A caller cannot name a favorable historical frontier.
    pub fn resolve_feed_authority(
        &self,
        feed_id: FeedId,
    ) -> Result<AuthorityFrontierResolution, VNextNetworkRuntimeError> {
        let events = self
            .validated_sink
            .accepted_authority_events()
            .map_err(VNextNetworkRuntimeError::Storage)?;
        resolve_authority_frontier(&events, |frontier| {
            self.validated_sink.feed_authority_at(feed_id, frontier)
        })
        .map_err(VNextNetworkRuntimeError::AuthorityResolver)
    }

    pub fn feed_projection(
        &self,
        feed_id: FeedId,
    ) -> Result<FeedProjection, VNextNetworkRuntimeError> {
        self.validated_sink
            .feed_projection(feed_id)
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    pub fn feed_inception_branch_count(
        &self,
        feed_id: FeedId,
    ) -> Result<usize, VNextNetworkRuntimeError> {
        self.validated_sink
            .feed_inceptions(feed_id)
            .map(|branches| branches.len())
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    /// Root-only v1 authority projection at one exact, durable proof frontier.
    /// This does not claim global validity or knowledge of later revocations.
    #[cfg(test)]
    pub(crate) fn feed_authority_at_root(
        &self,
        feed_id: FeedId,
        authority_root: ku_core::foundation::EventCid,
    ) -> Result<Vec<ku_core::foundation::FeedAuthorityDecision>, VNextNetworkRuntimeError> {
        self.validated_sink
            .feed_authority_at(feed_id, authority_root)
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    #[cfg(test)]
    pub(crate) fn feed_authority_at(
        &self,
        feed_id: FeedId,
        authority_frontier: ku_core::foundation::EventCid,
    ) -> Result<Vec<ku_core::foundation::FeedAuthorityDecision>, VNextNetworkRuntimeError> {
        self.validated_sink
            .feed_authority_at(feed_id, authority_frontier)
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    pub fn enqueue_outbound(
        &self,
        intent: &OutboundTransferIntent,
    ) -> Result<OutboxEnqueueOutcome, VNextNetworkRuntimeError> {
        let outcome = self
            .outbound
            .outbox
            .enqueue(intent)
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
        match outcome {
            OutboxEnqueueOutcome::Added => {
                self.observability
                    .record(VNextReasonCode::AcceptedNew, 0, 1)
            }
            OutboxEnqueueOutcome::Existing => {
                self.observability.record(VNextReasonCode::Replayed, 0, 1)
            }
            OutboxEnqueueOutcome::RouteUpdated => {
                self.observability
                    .record(VNextReasonCode::AlreadyPresent, 0, 1)
            }
        }
        self.outbound.refresh_outbox_observability()?;
        self.outbound_notify.notify_one();
        Ok(outcome)
    }

    pub fn outbound_intent(
        &self,
        id: &[u8; 32],
    ) -> Result<Option<OutboundTransferIntent>, VNextNetworkRuntimeError> {
        self.outbound
            .outbox
            .get(id)
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))
    }

    /// Run one bounded outbound scheduling pass. Pending intents survive
    /// process restart and are rebound to a fresh session only after the
    /// authenticated responder NodeId matches the durable target principal.
    pub async fn deliver_outbound_once(
        &self,
        limit: usize,
    ) -> Result<OutboundDeliveryReport, VNextNetworkRuntimeError> {
        self.outbound.deliver_once(limit).await
    }

    /// Establish an authenticated outbound session. The returned handle lets a
    /// higher-level reconciler build contexts from the negotiated session id.
    pub async fn connect(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, VNextNetworkRuntimeError> {
        self.outbound.connect(addr).await
    }
}

impl OutboundDeliveryEngine {
    fn refresh_outbox_observability(&self) -> Result<(), VNextNetworkRuntimeError> {
        let stats = self
            .outbox
            .stats()
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
        self.observability.observe_outbox(
            stats.pending,
            stats.retry_exhausted,
            stats.oldest_pending_age_seconds,
        );
        Ok(())
    }

    async fn deliver_once(
        &self,
        limit: usize,
    ) -> Result<OutboundDeliveryReport, VNextNetworkRuntimeError> {
        let _runtime_generation = self
            .rollout
            .as_ref()
            .map(|rollout| rollout.acquire(VNextRuntimeLane::Network))
            .transpose()
            .map_err(|error| VNextNetworkRuntimeError::RuntimeFenced(error.to_string()))?;
        let _scheduler = self.scheduler.lock().await;
        let max_payload_records = self.policy.max_records_per_session.saturating_sub(1) as usize;
        let bounded_limit = limit.min(max_payload_records);
        let mut pending = self
            .outbox
            .pending_fair(bounded_limit)
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
        let mut report = OutboundDeliveryReport {
            scanned: pending.len(),
            claims_network_completion: false,
            ..OutboundDeliveryReport::default()
        };

        while !pending.is_empty() {
            let seed = pending.remove(0);
            if seed.transport_attempts >= self.policy.max_retries_per_record {
                self.outbox
                    .mark_retry_exhausted(&seed.id, self.policy.max_retries_per_record)
                    .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
                self.observability
                    .record(VNextReasonCode::OutboxRetryExhausted, 0, 1);
                report.retry_exhausted += 1;
                continue;
            }

            let mut batch = vec![seed];
            let mut batch_bytes = batch[0].canonical_bytes.len() as u64;
            let mut candidate = 0;
            while candidate < pending.len() {
                let next = &pending[candidate];
                let next_bytes = next.canonical_bytes.len() as u64;
                if next.transport_attempts < self.policy.max_retries_per_record
                    && same_delivery_batch(&batch[0], next)
                    && batch_bytes.saturating_add(next_bytes) <= self.policy.max_inflight_bytes
                {
                    batch_bytes += next_bytes;
                    batch.push(pending.remove(candidate));
                } else {
                    candidate += 1;
                }
            }

            self.deliver_outbound_batch(&batch, &mut report).await?;
        }
        self.refresh_outbox_observability()?;
        Ok(report)
    }

    async fn deliver_outbound_batch(
        &self,
        batch: &[OutboundTransferIntent],
        report: &mut OutboundDeliveryReport,
    ) -> Result<(), VNextNetworkRuntimeError> {
        debug_assert!(!batch.is_empty());
        for intent in batch {
            self.outbox
                .record_transport_attempt(&intent.id)
                .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
            report.attempted += 1;
        }

        let first = &batch[0];
        let mut session = match self.connect(first.last_known_addr).await {
            Ok(session) => session,
            Err(error) => {
                self.observability
                    .record(VNextReasonCode::TransportFailure, 0, batch.len() as u64);
                tracing::warn!(
                    target: "onebrain::vnext::observability",
                    reason_code = VNextReasonCode::TransportFailure.code(),
                    error = %error,
                    "vNext outbound connection failed"
                );
                report.failed += batch.len();
                self.mark_transport_failures(batch, report)?;
                return Ok(());
            }
        };
        if session.authenticated().responder != first.expected_peer {
            self.observability
                .record(VNextReasonCode::RejectedAuthority, batch.len() as u64, 1);
            session.close();
            report.failed += batch.len();
            self.mark_transport_failures(batch, report)?;
            return Ok(());
        }

        let context = ReconciliationContext {
            authenticated_transcript: session.authenticated().session_id,
            selector: first.selector,
            namespace: first.namespace,
            disclosure: first.disclosure,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 1,
                max_diff_ranges: 1,
                // Keep the resumable scope stable when the pending subset
                // changes after a reconnect. The manifest still contains only
                // this bounded batch.
                max_manifest_entries: self.policy.max_records_per_session.saturating_sub(1),
                max_payload_bytes: MAX_OUTBOX_PAYLOAD_BYTES as u64,
            },
            resume_mode: ReconciliationResumeMode::PeerBoundTokenV2,
        };
        let mut frames = Vec::with_capacity(batch.len());
        for intent in batch {
            let frame =
                BoundPayloadFrame::new(&context, intent.kind, intent.canonical_bytes.clone())
                    .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
            if frame.cid != intent.cid {
                session.close();
                return Err(VNextNetworkRuntimeError::Outbox(
                    "intent CID changed after durable validation".to_string(),
                ));
            }
            frames.push(frame);
        }
        // The durable outbox is ordered by IntentID, while the protocol
        // manifest is a canonical set ordered by record kind and content CID.
        // Never let database iteration order leak into signed wire bytes.
        frames.sort_by_key(|frame| (frame.kind as u64, frame.cid));
        let entries = frames
            .iter()
            .map(|frame| ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .collect();
        let manifest =
            bind_reconciliation_message(context, 1, ReconciliationBody::Manifest { entries })
                .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
        let manifest_record = CarrierRecord::reconciliation_message(
            &encode_reconciliation_message(&manifest)
                .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?,
        )
        .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
        if session.send(&manifest_record).await.is_err() {
            self.observability
                .record(VNextReasonCode::TransportFailure, 0, batch.len() as u64);
            session.close();
            report.failed += batch.len();
            self.mark_transport_failures(batch, report)?;
            return Ok(());
        }
        for frame in frames {
            if session
                .send(&CarrierRecord::BoundPayload(frame))
                .await
                .is_err()
            {
                self.observability
                    .record(VNextReasonCode::TransportFailure, 0, 1);
                session.close();
                report.failed += batch.len();
                self.mark_transport_failures(batch, report)?;
                return Ok(());
            }
        }

        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(self.policy.handshake_timeout_seconds);
        let mut statuses = BTreeMap::new();
        let mut received_resume_token = false;
        while statuses.len() < batch.len() || !received_resume_token {
            let response = tokio::time::timeout_at(deadline, session.recv()).await;
            let Ok(Ok(AuthenticatedCarrierRecord::Reconciliation(response))) = response else {
                break;
            };
            match response.body {
                ReconciliationBody::Receipt { entries } => {
                    for entry in entries {
                        if batch
                            .iter()
                            .any(|intent| intent.kind == entry.kind && intent.cid == entry.cid)
                        {
                            statuses.insert((entry.kind as u64, entry.cid), entry.status);
                        }
                    }
                }
                ReconciliationBody::Progress {
                    resume_token: Some(_),
                    ..
                } => {
                    received_resume_token = true;
                }
                _ => {}
            }
        }
        session.close();

        for intent in batch {
            let Some(status) = statuses.get(&(intent.kind as u64, intent.cid)).copied() else {
                self.observability
                    .record(VNextReasonCode::TransportFailure, 0, 1);
                report.failed += 1;
                self.mark_transport_failures(std::slice::from_ref(intent), report)?;
                continue;
            };
            let state = self
                .outbox
                .apply_receipt(&intent.id, status, self.policy.max_retries_per_record)
                .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
            match (state, status) {
                (OutboundIntentState::Acknowledged, ReconcileReceiptStatus::ValidatedStored) => {
                    self.observability.record(
                        VNextReasonCode::AcceptedNew,
                        intent.canonical_bytes.len() as u64,
                        1,
                    );
                    report.acknowledged += 1;
                }
                (OutboundIntentState::Acknowledged, ReconcileReceiptStatus::AlreadyPresent) => {
                    self.observability.record(
                        VNextReasonCode::AlreadyPresent,
                        intent.canonical_bytes.len() as u64,
                        1,
                    );
                    report.acknowledged += 1;
                }
                (OutboundIntentState::Acknowledged, _) => report.acknowledged += 1,
                (OutboundIntentState::DeadLetter, _) => {
                    self.observability.record(
                        VNextReasonCode::RejectedSink,
                        intent.canonical_bytes.len() as u64,
                        1,
                    );
                    report.rejected += 1;
                }
                (OutboundIntentState::RetryExhausted, _) => {
                    self.observability
                        .record(VNextReasonCode::OutboxRetryExhausted, 0, 1);
                    report.retry_exhausted += 1;
                }
                (OutboundIntentState::Pending, ReconcileReceiptStatus::DeferredBudget) => {
                    self.observability
                        .record(VNextReasonCode::DeferredBudget, 0, 1);
                    report.deferred += 1;
                }
                (
                    OutboundIntentState::Pending,
                    ReconcileReceiptStatus::DeferredMissingDependency,
                ) => {
                    self.observability
                        .record(VNextReasonCode::DeferredMissingDependency, 0, 1);
                    report.deferred += 1;
                }
                _ => report.failed += 1,
            }
        }
        Ok(())
    }

    fn mark_transport_failures(
        &self,
        batch: &[OutboundTransferIntent],
        report: &mut OutboundDeliveryReport,
    ) -> Result<(), VNextNetworkRuntimeError> {
        for intent in batch {
            let state = self
                .outbox
                .mark_retry_exhausted(&intent.id, self.policy.max_retries_per_record)
                .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
            if state == OutboundIntentState::RetryExhausted {
                self.observability
                    .record(VNextReasonCode::OutboxRetryExhausted, 0, 1);
                report.retry_exhausted += 1;
            }
        }
        Ok(())
    }

    async fn connect(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, VNextNetworkRuntimeError> {
        let runtime_generation = self
            .rollout
            .as_ref()
            .map(|rollout| rollout.acquire(VNextRuntimeLane::Network))
            .transpose()
            .map_err(|error| VNextNetworkRuntimeError::RuntimeFenced(error.to_string()))?;
        let handshake_admission = self
            .admission
            .try_begin_handshake(addr.ip())
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        let connection = self
            .transport
            .connect(addr)
            .await
            .map_err(|error| VNextNetworkRuntimeError::Transport(error.to_string()))?;
        let authenticated = tokio::time::timeout(
            Duration::from_secs(self.policy.handshake_timeout_seconds),
            initiate_authenticated_session(
                &connection,
                self.identity.as_ref(),
                random_nonce(),
                &[reconciliation_profile()],
                &[reconciliation_capability()],
                Vec::new(),
            ),
        )
        .await
        .map_err(|_| VNextNetworkRuntimeError::HandshakeTimeout)?
        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
        let session_admission = handshake_admission
            .promote(authenticated.responder)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        self.replay_guard
            .lock()
            .map_err(|_| {
                self.observability
                    .record(VNextReasonCode::RejectedReplay, 0, 1);
                VNextNetworkRuntimeError::ReplayGuard
            })?
            .accept(&authenticated)
            .map_err(|error| {
                self.observability
                    .record(VNextReasonCode::RejectedReplay, 0, 1);
                VNextNetworkRuntimeError::Session(format!("{error:?}"))
            })?;
        self.routes
            .observe_outbound(self.principal, &authenticated, connection.remote_addr())
            .map_err(VNextNetworkRuntimeError::RouteDirectory)?;
        self.counters
            .authenticated_sessions
            .fetch_add(1, Ordering::Relaxed);
        let carrier = AuthenticatedCarrierSession::with_context_limit(
            authenticated.clone(),
            self.policy.max_contexts_per_session,
        )
        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
        Ok(OutboundVNextSession {
            connection,
            authenticated,
            carrier,
            admission: session_admission,
            observability: Arc::clone(&self.observability),
            runtime_generation,
        })
    }
}

impl VNextNetworkRuntime {
    pub async fn shutdown(&mut self) {
        if self.state == VNextNetworkRuntimeState::Stopped {
            return;
        }
        self.state = VNextNetworkRuntimeState::Stopped;
        let _ = self.outbound_shutdown.send(true);
        self.outbound_notify.notify_waiters();
        if let Some(mut task) = self.outbound_task.take() {
            task.abort();
            let _ = (&mut task).await;
        }
        self.transport.shutdown().await;
        self.accept_task.abort();
        let _ = (&mut self.accept_task).await;
    }
}

impl Drop for VNextNetworkRuntime {
    fn drop(&mut self) {
        let _ = self.outbound_shutdown.send(true);
        self.outbound_notify.notify_waiters();
        if let Some(task) = self.outbound_task.take() {
            task.abort();
        }
        self.accept_task.abort();
        self.transport.close();
        self.state = VNextNetworkRuntimeState::Stopped;
    }
}

fn same_delivery_batch(left: &OutboundTransferIntent, right: &OutboundTransferIntent) -> bool {
    left.expected_peer == right.expected_peer
        && left.last_known_addr == right.last_known_addr
        && left.selector == right.selector
        && left.namespace == right.namespace
        && left.disclosure == right.disclosure
}

fn admission_limits(policy: VNextNetworkPolicy) -> RuntimeAdmissionLimits {
    let per_peer_window = ResourceUsage {
        records: policy.max_records_per_peer_window,
        bytes: policy.max_bytes_per_peer_window,
        work: policy.max_work_per_peer_window,
    };
    let scale = |usage: ResourceUsage, factor: usize| ResourceUsage {
        records: usage.records.saturating_mul(factor as u64),
        bytes: usage.bytes.saturating_mul(factor as u64),
        work: usage.work.saturating_mul(factor as u64),
    };
    RuntimeAdmissionLimits {
        max_handshakes_global: policy.max_concurrent_handshakes as u64,
        max_handshakes_per_ip: policy.max_handshakes_per_ip as u64,
        max_sessions_global: policy.max_concurrent_sessions as u64,
        max_sessions_per_ip: policy.max_sessions_per_ip as u64,
        max_sessions_per_peer: policy.max_sessions_per_peer as u64,
        max_contexts_per_session: policy.max_contexts_per_session as u64,
        per_session: ResourceUsage {
            records: policy.max_records_per_session,
            bytes: policy.max_bytes_per_peer_window,
            work: policy.max_work_per_session,
        },
        rate_window: Duration::from_secs(policy.rate_window_seconds),
        global_per_window: scale(per_peer_window, policy.max_concurrent_sessions),
        per_ip_per_window: scale(per_peer_window, policy.max_sessions_per_ip),
        per_peer_per_window: per_peer_window,
    }
}

async fn run_outbound_scheduler(
    engine: Arc<OutboundDeliveryEngine>,
    notify: Arc<Notify>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut run_now = true;
    let mut retry_delay = OUTBOUND_RETRY_BASE;
    loop {
        if !run_now && !wait_for_outbound_wake(&notify, &mut shutdown, None).await {
            return;
        }
        if *shutdown.borrow() {
            return;
        }

        let limit = engine.policy.max_records_per_session.saturating_sub(1) as usize;
        match engine.deliver_once(limit).await {
            Ok(report) if report.scanned == 0 => {
                retry_delay = OUTBOUND_RETRY_BASE;
                run_now = false;
            }
            Ok(report) if report.attempted == 0 => {
                // Every visible pending record exhausted its bounded retry
                // budget. A route update/re-enqueue is the explicit wake-up.
                run_now = false;
            }
            Ok(report) if report.failed > 0 || report.deferred > 0 => {
                if !wait_for_outbound_wake(&notify, &mut shutdown, Some(retry_delay)).await {
                    return;
                }
                retry_delay = next_retry_delay(retry_delay);
                run_now = true;
            }
            Ok(_) => {
                // The bounded scan may have left more durable work behind.
                // Continue immediately only after terminal receipts made
                // progress; the next empty scan returns to notification mode.
                retry_delay = OUTBOUND_RETRY_BASE;
                run_now = true;
            }
            Err(error) => {
                engine
                    .observability
                    .record(VNextReasonCode::JournalFailure, 0, 1);
                tracing::warn!(
                    target: "onebrain::vnext::observability",
                    reason_code = VNextReasonCode::JournalFailure.code(),
                    error = %error,
                    "vNext outbound scheduling pass failed"
                );
                if !wait_for_outbound_wake(&notify, &mut shutdown, Some(retry_delay)).await {
                    return;
                }
                retry_delay = next_retry_delay(retry_delay);
                run_now = true;
            }
        }
    }
}

async fn wait_for_outbound_wake(
    notify: &Notify,
    shutdown: &mut watch::Receiver<bool>,
    delay: Option<Duration>,
) -> bool {
    if *shutdown.borrow() {
        return false;
    }
    match delay {
        Some(delay) => {
            tokio::select! {
                _ = tokio::time::sleep(delay) => true,
                _ = notify.notified() => !*shutdown.borrow(),
                changed = shutdown.changed() => changed.is_ok() && !*shutdown.borrow(),
            }
        }
        None => {
            tokio::select! {
                _ = notify.notified() => !*shutdown.borrow(),
                changed = shutdown.changed() => changed.is_ok() && !*shutdown.borrow(),
            }
        }
    }
}

fn next_retry_delay(current: Duration) -> Duration {
    let doubled_millis = current.as_millis().saturating_mul(2);
    Duration::from_millis(doubled_millis.min(OUTBOUND_RETRY_MAX.as_millis()) as u64)
}

pub struct OutboundVNextSession {
    connection: OBPConnection,
    authenticated: AuthenticatedSession,
    carrier: AuthenticatedCarrierSession,
    admission: SessionAdmission,
    observability: Arc<VNextObservability>,
    runtime_generation: Option<VNextRuntimeGenerationLease>,
}

impl OutboundVNextSession {
    pub fn authenticated(&self) -> &AuthenticatedSession {
        &self.authenticated
    }

    pub async fn send(&self, record: &CarrierRecord) -> Result<(), VNextNetworkRuntimeError> {
        self.ensure_runtime_generation()?;
        if let CarrierRecord::ReconciliationMessage(bytes) = record {
            let message = decode_reconciliation_message(bytes)
                .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
            self.admission
                .admit_context(message.binding_digest)
                .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        }
        let frame_bytes = record
            .canonical_bytes()
            .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?
            .len() as u64;
        let mut admission = self
            .admission
            .begin_record(frame_bytes)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        admission
            .advance(AdmissionStage::Frame, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        admission
            .advance(AdmissionStage::Protocol, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        admission
            .advance(AdmissionStage::Journal, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        send_carrier_record(&self.connection, record)
            .await
            .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
        admission
            .advance(AdmissionStage::Application, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))
    }

    pub async fn recv(&mut self) -> Result<AuthenticatedCarrierRecord, VNextNetworkRuntimeError> {
        self.ensure_runtime_generation()?;
        let payload = self
            .carrier
            .recv_frame_payload(&self.connection)
            .await
            .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
        let mut admission = self
            .admission
            .begin_record(payload.len() as u64)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        admission
            .advance(AdmissionStage::Frame, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        let record = self
            .carrier
            .decode_and_validate_payload(&payload)
            .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
        if let AuthenticatedCarrierRecord::Reconciliation(message) = &record {
            self.admission
                .admit_context(message.binding_digest)
                .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        }
        admission
            .advance(AdmissionStage::Protocol, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        admission
            .advance(AdmissionStage::Journal, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        admission
            .advance(AdmissionStage::Application, 1)
            .map_err(|error| observable_resource_admission_error(&self.observability, error))?;
        Ok(record)
    }

    pub fn close(&self) {
        self.connection.close("OBP-RP session complete");
    }

    fn ensure_runtime_generation(&self) -> Result<(), VNextNetworkRuntimeError> {
        if self
            .runtime_generation
            .as_ref()
            .is_some_and(|generation| !generation.is_current())
        {
            Err(VNextNetworkRuntimeError::RuntimeFenced(
                "network generation changed".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_loop(
    transport: Arc<QuicTransport>,
    identity: Arc<dyn SessionIdentitySigner>,
    replay_guard: Arc<Mutex<SessionReplayGuard>>,
    routes: AuthenticatedRouteDirectory,
    principal: NodeId,
    counters: Arc<RuntimeCounters>,
    observability: Arc<VNextObservability>,
    admission: RuntimeAdmissionController,
    journal: RedbReconciliationJournalBackend,
    sink: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    storage: NetworkStorageAdmission,
    policy: VNextNetworkPolicy,
    rollout: Option<VNextRuntimeRollout>,
) {
    loop {
        let connection = match transport.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(
                    target: "onebrain::vnext::observability",
                    reason_code = VNextReasonCode::TransportFailure.code(),
                    error = %error,
                    "vNext accept loop stopped"
                );
                break;
            }
        };
        let runtime_generation = match rollout
            .as_ref()
            .map(|rollout| rollout.acquire(VNextRuntimeLane::Network))
            .transpose()
        {
            Ok(generation) => generation,
            Err(error) => {
                tracing::warn!(
                    target: "onebrain::vnext::observability",
                    reason_code = VNextReasonCode::RejectedProtocol.code(),
                    error = %error,
                    "vNext runtime generation rejected inbound session"
                );
                counters.rejected_sessions.fetch_add(1, Ordering::Relaxed);
                connection.close("OBP-RP runtime generation fenced");
                continue;
            }
        };
        let handshake_admission = match admission.try_begin_handshake(connection.remote_addr().ip())
        {
            Ok(admission) => admission,
            Err(error) => {
                let _ = observable_resource_admission_error(&observability, error);
                counters.rejected_sessions.fetch_add(1, Ordering::Relaxed);
                connection.close("OBP-RP handshake resource budget exhausted");
                continue;
            }
        };
        let identity = Arc::clone(&identity);
        let replay_guard = Arc::clone(&replay_guard);
        let counters = Arc::clone(&counters);
        let observability = Arc::clone(&observability);
        let routes = routes.clone();
        let journal = journal.clone();
        let sink = sink.clone();
        let inventory = inventory.clone();
        let provenance = provenance.clone();
        let storage = storage.clone();
        tokio::spawn(async move {
            if handle_inbound_connection(
                connection,
                handshake_admission,
                identity,
                replay_guard,
                routes,
                principal,
                Arc::clone(&counters),
                observability,
                journal,
                sink,
                inventory,
                provenance,
                storage,
                policy,
                runtime_generation,
            )
            .await
            .is_err()
            {
                counters.rejected_sessions.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_inbound_connection(
    connection: OBPConnection,
    handshake_admission: HandshakeAdmission,
    identity: Arc<dyn SessionIdentitySigner>,
    replay_guard: Arc<Mutex<SessionReplayGuard>>,
    routes: AuthenticatedRouteDirectory,
    principal: NodeId,
    counters: Arc<RuntimeCounters>,
    observability: Arc<VNextObservability>,
    journal_backend: RedbReconciliationJournalBackend,
    sink: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    storage: NetworkStorageAdmission,
    policy: VNextNetworkPolicy,
    runtime_generation: Option<VNextRuntimeGenerationLease>,
) -> Result<(), VNextNetworkRuntimeError> {
    let authenticated = tokio::time::timeout(
        Duration::from_secs(policy.handshake_timeout_seconds),
        accept_authenticated_session(
            &connection,
            identity.as_ref(),
            random_nonce(),
            &[reconciliation_profile()],
            &[reconciliation_capability()],
            Vec::new(),
        ),
    )
    .await
    .map_err(|_| VNextNetworkRuntimeError::HandshakeTimeout)?
    .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
    let session_admission = handshake_admission
        .promote(authenticated.initiator)
        .map_err(|error| observable_resource_admission_error(&observability, error))?;
    replay_guard
        .lock()
        .map_err(|_| {
            observability.record(VNextReasonCode::RejectedReplay, 0, 1);
            VNextNetworkRuntimeError::ReplayGuard
        })?
        .accept(&authenticated)
        .map_err(|error| {
            observability.record(VNextReasonCode::RejectedReplay, 0, 1);
            VNextNetworkRuntimeError::Session(format!("{error:?}"))
        })?;
    routes
        .observe_inbound(principal, &authenticated, connection.remote_addr())
        .map_err(VNextNetworkRuntimeError::RouteDirectory)?;
    counters
        .authenticated_sessions
        .fetch_add(1, Ordering::Relaxed);
    counters.active_sessions.fetch_add(1, Ordering::Relaxed);
    let _active = ActiveSessionCounter(Arc::clone(&counters));

    let resume_token_key = peer_bound_resume_token_key(identity.as_ref(), &authenticated)?;
    let source_peer = authenticated.initiator;
    let mut carrier = AuthenticatedCarrierSession::with_context_limit(
        authenticated,
        policy.max_contexts_per_session,
    )
    .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
    let mut journals = BTreeMap::<[u8; 32], PersistentJournal>::new();
    let mut journal_observations = BTreeMap::<[u8; 32], VNextJournalObservation>::new();
    let mut outbound_sequence = 1u64;
    for _ in 0..policy.max_records_per_session {
        ensure_runtime_generation(&runtime_generation)?;
        let frame_payload = match carrier.recv_frame_payload(&connection).await {
            Ok(payload) => payload,
            Err(error) => {
                observability.record(VNextReasonCode::RejectedProtocol, 0, 1);
                tracing::warn!(
                    target: "onebrain::vnext::observability",
                    reason_code = VNextReasonCode::RejectedProtocol.code(),
                    error = %error,
                    "vNext carrier receive rejected"
                );
                break;
            }
        };
        ensure_runtime_generation(&runtime_generation)?;
        let mut resource_record = session_admission
            .begin_record(frame_payload.len() as u64)
            .map_err(|error| observable_resource_admission_error(&observability, error))?;
        resource_record
            .advance(AdmissionStage::Frame, 1)
            .map_err(|error| observable_resource_admission_error(&observability, error))?;
        let record = match carrier.decode_and_validate_payload(&frame_payload) {
            Ok(record) => record,
            Err(error) => {
                observability.record(
                    VNextReasonCode::RejectedProtocol,
                    frame_payload.len() as u64,
                    1,
                );
                tracing::warn!(
                    target: "onebrain::vnext::observability",
                    reason_code = VNextReasonCode::RejectedProtocol.code(),
                    error = %error,
                    "vNext carrier payload rejected"
                );
                break;
            }
        };
        resource_record
            .advance(AdmissionStage::Protocol, 1)
            .map_err(|error| observable_resource_admission_error(&observability, error))?;
        match record {
            AuthenticatedCarrierRecord::Reconciliation(message) => {
                let binding = message.binding_digest;
                session_admission
                    .admit_context(binding)
                    .map_err(|error| observable_resource_admission_error(&observability, error))?;
                if let ReconciliationBody::Resume { token } = &message.body {
                    if journals.contains_key(&binding) {
                        return Err(VNextNetworkRuntimeError::Journal(
                            "duplicate Resume context in one authenticated session".to_string(),
                        ));
                    }
                    let inventorying_sink = InventoryingSink {
                        inner: sink.clone(),
                        inventory: inventory.clone(),
                        provenance: provenance.clone(),
                        selector: message.context.selector,
                        source_peer,
                        storage: storage.clone(),
                        observability: Arc::clone(&observability),
                    };
                    let mut resumed = JournaledReconciliationSession::resume(
                        journal_backend.clone(),
                        message.context.clone(),
                        ReconciliationJournalConfig {
                            max_retries_per_record: policy.max_retries_per_record,
                            max_inflight_bytes: policy.max_inflight_bytes,
                        },
                        inventorying_sink,
                        token,
                        resume_token_key,
                    )
                    .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    let progress_sequence = message.sequence.saturating_add(1);
                    let next_token = resumed
                        .issue_resume_token(progress_sequence.saturating_add(1), resume_token_key)
                        .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    let progress = resumed
                        .progress_message(progress_sequence, Some(next_token))
                        .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    let progress_record = CarrierRecord::reconciliation_message(
                        &encode_reconciliation_message(&progress).map_err(|error| {
                            VNextNetworkRuntimeError::Session(error.to_string())
                        })?,
                    )
                    .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
                    send_carrier_record(&connection, &progress_record)
                        .await
                        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
                    outbound_sequence = progress_sequence.saturating_add(2);
                    journals.insert(binding, resumed);
                    journal_observations.insert(binding, observability.begin_journal());
                    resource_record
                        .advance(AdmissionStage::Journal, 1)
                        .map_err(|error| {
                            observable_resource_admission_error(&observability, error)
                        })?;
                    resource_record
                        .advance(AdmissionStage::Application, 1)
                        .map_err(|error| {
                            observable_resource_admission_error(&observability, error)
                        })?;
                    continue;
                }
                if !journals.contains_key(&binding) {
                    let inventorying_sink = InventoryingSink {
                        inner: sink.clone(),
                        inventory: inventory.clone(),
                        provenance: provenance.clone(),
                        selector: message.context.selector,
                        source_peer,
                        storage: storage.clone(),
                        observability: Arc::clone(&observability),
                    };
                    let session = JournaledReconciliationSession::open(
                        journal_backend.clone(),
                        message.context.clone(),
                        ReconciliationJournalConfig {
                            max_retries_per_record: policy.max_retries_per_record,
                            max_inflight_bytes: policy.max_inflight_bytes,
                        },
                        inventorying_sink,
                    )
                    .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    journals.insert(binding, session);
                    journal_observations.insert(binding, observability.begin_journal());
                }
                if matches!(message.body, ReconciliationBody::Manifest { .. }) {
                    let manifest_outcome = journals
                        .get_mut(&binding)
                        .expect("journal inserted")
                        .ingest_manifest(&message)
                        .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    observability.record_count(
                        VNextReasonCode::AcceptedNew,
                        manifest_outcome.new_entries,
                        0,
                        manifest_outcome.new_entries,
                    );
                    observability.record_count(
                        VNextReasonCode::Replayed,
                        manifest_outcome.replayed_entries,
                        0,
                        manifest_outcome.replayed_entries,
                    );
                    observability.record_count(
                        VNextReasonCode::RejectedLength,
                        manifest_outcome.conflicting_lengths,
                        0,
                        manifest_outcome.conflicting_lengths,
                    );
                }
                resource_record
                    .advance(AdmissionStage::Journal, 1)
                    .map_err(|error| observable_resource_admission_error(&observability, error))?;
                resource_record
                    .advance(AdmissionStage::Application, 1)
                    .map_err(|error| observable_resource_admission_error(&observability, error))?;
            }
            AuthenticatedCarrierRecord::BoundPayload(frame) => {
                let Some(journal) = journals.get_mut(&frame.binding_digest) else {
                    counters.deferred_records.fetch_add(1, Ordering::Relaxed);
                    observability.record(
                        VNextReasonCode::DeferredMissingDependency,
                        frame.canonical_bytes.len() as u64,
                        1,
                    );
                    resource_record
                        .advance(AdmissionStage::Journal, 1)
                        .map_err(|error| {
                            observable_resource_admission_error(&observability, error)
                        })?;
                    resource_record
                        .advance(AdmissionStage::Application, 1)
                        .map_err(|error| {
                            observable_resource_admission_error(&observability, error)
                        })?;
                    continue;
                };
                let payload_outcome = journal
                    .ingest_payload(&frame)
                    .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                if let Some(receipt) = journal
                    .receipt_message(outbound_sequence)
                    .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?
                {
                    let bytes = encode_reconciliation_message(&receipt)
                        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
                    let record = CarrierRecord::reconciliation_message(&bytes)
                        .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
                    send_carrier_record(&connection, &record)
                        .await
                        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
                    outbound_sequence = outbound_sequence.saturating_add(1);
                }
                if journal.resume_mode() == ReconciliationResumeMode::PeerBoundTokenV2 {
                    let next_sequence = outbound_sequence.saturating_add(1);
                    let token = journal
                        .issue_resume_token(next_sequence, resume_token_key)
                        .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    let progress = journal
                        .progress_message(outbound_sequence, Some(token))
                        .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                    let bytes = encode_reconciliation_message(&progress)
                        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
                    let record = CarrierRecord::reconciliation_message(&bytes)
                        .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
                    send_carrier_record(&connection, &record)
                        .await
                        .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))?;
                    outbound_sequence = outbound_sequence.saturating_add(1);
                }
                match payload_outcome {
                    JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::ValidatedStored) => {
                        observability.record(
                            VNextReasonCode::AcceptedNew,
                            frame.canonical_bytes.len() as u64,
                            5,
                        );
                        counters.accepted_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::AlreadyPresent) => {
                        observability.record(
                            VNextReasonCode::AlreadyPresent,
                            frame.canonical_bytes.len() as u64,
                            5,
                        );
                        counters.accepted_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::Delivered(
                        PayloadIngestOutcome::DeferredUntilManifest
                        | PayloadIngestOutcome::DeferredMissingDependency,
                    ) => {
                        observability.record(
                            VNextReasonCode::DeferredMissingDependency,
                            frame.canonical_bytes.len() as u64,
                            5,
                        );
                        counters.deferred_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::Backpressured => {
                        observability.record(
                            VNextReasonCode::DeferredBudget,
                            frame.canonical_bytes.len() as u64,
                            5,
                        );
                        counters.deferred_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::Rejected(reason)) => {
                        observability.record(
                            payload_reject_reason(reason),
                            frame.canonical_bytes.len() as u64,
                            5,
                        );
                        counters.rejected_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::RetryExhausted => {
                        observability.record(
                            VNextReasonCode::OutboxRetryExhausted,
                            frame.canonical_bytes.len() as u64,
                            5,
                        );
                        counters.rejected_records.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let pending = match journal.state() {
                    ReceiverState::ReceivingPayloads { pending }
                    | ReceiverState::PartialInvalid { pending, .. } => pending,
                    ReceiverState::AwaitingManifest | ReceiverState::ManifestBatchComplete => 0,
                };
                observability.observe_reconciliation_lag(pending);
                debug_assert!(!journal.state().is_globally_complete());
                let _ = ReceiverState::AwaitingManifest;
                resource_record
                    .advance(AdmissionStage::Journal, 1)
                    .map_err(|error| observable_resource_admission_error(&observability, error))?;
                resource_record
                    .advance(AdmissionStage::Application, 1)
                    .map_err(|error| observable_resource_admission_error(&observability, error))?;
            }
        }
        debug_assert!(resource_record.is_complete());
    }
    connection.close("OBP-RP record budget reached");
    Ok(())
}

struct ActiveSessionCounter(Arc<RuntimeCounters>);

impl Drop for ActiveSessionCounter {
    fn drop(&mut self) {
        self.0.active_sessions.fetch_sub(1, Ordering::Relaxed);
    }
}

fn peer_bound_resume_token_key(
    local_identity: &dyn SessionIdentitySigner,
    session: &AuthenticatedSession,
) -> Result<[u8; 32], VNextNetworkRuntimeError> {
    let mut preimage = Vec::with_capacity(44 + 64);
    preimage.extend_from_slice(b"onebrain:vnext:peer-bound-resume-key:2\0");
    preimage.extend_from_slice(session.initiator.as_bytes());
    preimage.extend_from_slice(session.responder.as_bytes());
    let signature = local_identity
        .sign_session_message(&preimage)
        .map_err(VNextNetworkRuntimeError::IdentitySignerUnavailable)?;
    Ok(blake3::derive_key(
        "onebrain vnext peer-bound resume token key v2",
        &signature,
    ))
}

fn random_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

fn resource_admission_error(error: ResourceAdmissionError) -> VNextNetworkRuntimeError {
    VNextNetworkRuntimeError::ResourceAdmission(format!("{error:?}"))
}

fn observable_resource_admission_error(
    observability: &VNextObservability,
    error: ResourceAdmissionError,
) -> VNextNetworkRuntimeError {
    let reason = match error {
        ResourceAdmissionError::WindowGlobal
        | ResourceAdmissionError::WindowIp
        | ResourceAdmissionError::WindowPeer
        | ResourceAdmissionError::Contexts
        | ResourceAdmissionError::SessionRecords
        | ResourceAdmissionError::SessionBytes
        | ResourceAdmissionError::SessionWork => VNextReasonCode::RejectedRateLimit,
        ResourceAdmissionError::HandshakeGlobal
        | ResourceAdmissionError::HandshakeIp
        | ResourceAdmissionError::SessionGlobal
        | ResourceAdmissionError::SessionIp
        | ResourceAdmissionError::SessionPeer => VNextReasonCode::RejectedSession,
        ResourceAdmissionError::InvalidLimits
        | ResourceAdmissionError::StageOrder
        | ResourceAdmissionError::LockPoisoned => VNextReasonCode::RejectedProtocol,
    };
    observability.record(reason, 0, 1);
    resource_admission_error(error)
}

fn payload_reject_reason(reason: PayloadRejectReason) -> VNextReasonCode {
    match reason {
        PayloadRejectReason::ContextBinding => VNextReasonCode::RejectedContextBinding,
        PayloadRejectReason::Selector => VNextReasonCode::RejectedSelector,
        PayloadRejectReason::UndeclaredLength => VNextReasonCode::RejectedLength,
        PayloadRejectReason::ContentCid => VNextReasonCode::RejectedContentCid,
        PayloadRejectReason::SinkValidation => VNextReasonCode::QuarantinedInvalid,
        PayloadRejectReason::SinkFailure => VNextReasonCode::RejectedSink,
    }
}

fn validate_identity_signer(
    identity: &dyn SessionIdentitySigner,
) -> Result<[u8; 32], VNextNetworkRuntimeError> {
    const PROOF: &[u8] = b"onebrain:vnext:identity-signer-proof-of-possession:1\0";
    let public_key = identity.public_key();
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| VNextNetworkRuntimeError::IdentitySignerInvalid)?;
    let signature = identity
        .sign_session_message(PROOF)
        .map_err(VNextNetworkRuntimeError::IdentitySignerUnavailable)?;
    verifying_key
        .verify_strict(PROOF, &Signature::from_bytes(&signature))
        .map_err(|_| VNextNetworkRuntimeError::IdentitySignerProofInvalid)?;
    Ok(public_key)
}

fn load_or_create_identity(path: &Path) -> Result<SigningKey, VNextNetworkRuntimeError> {
    if path.exists() {
        return load_identity(path);
    }
    let seed = random_nonce();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(IDENTITY_MAGIC)?;
            file.write_all(&seed)?;
            file.sync_all()?;
            Ok(SigningKey::from_bytes(&seed))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => load_identity(path),
        Err(error) => Err(error.into()),
    }
}

fn load_identity(path: &Path) -> Result<SigningKey, VNextNetworkRuntimeError> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.len() != IDENTITY_BYTES || &bytes[..8] != IDENTITY_MAGIC {
        return Err(VNextNetworkRuntimeError::IdentityCorrupt(
            path.to_path_buf(),
        ));
    }
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&bytes[8..]);
    Ok(SigningKey::from_bytes(&seed))
}

fn ensure_runtime_generation(
    generation: &Option<VNextRuntimeGenerationLease>,
) -> Result<(), VNextNetworkRuntimeError> {
    if generation
        .as_ref()
        .is_some_and(|generation| !generation.is_current())
    {
        Err(VNextNetworkRuntimeError::RuntimeFenced(
            "network generation changed".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum VNextNetworkRuntimeError {
    #[error("vNext runtime configuration failed: {0}")]
    Config(String),
    #[error("vNext transport failed: {0}")]
    Transport(String),
    #[error("vNext authenticated session failed: {0}")]
    Session(String),
    #[error("vNext handshake timed out")]
    HandshakeTimeout,
    #[error("vNext replay guard unavailable")]
    ReplayGuard,
    #[error("vNext authenticated route directory failed: {0}")]
    RouteDirectory(#[from] RouteDirectoryError),
    #[error("vNext authority-frontier resolver failed: {0}")]
    AuthorityResolver(#[from] AuthorityResolverError),
    #[error("vNext persistent storage failed: {0}")]
    Storage(String),
    #[error("vNext reconciliation journal failed: {0}")]
    Journal(String),
    #[error("vNext inventory persistence failed: {0}")]
    Inventory(String),
    #[error("vNext record provenance persistence failed: {0}")]
    Provenance(String),
    #[error("vNext unified resource admission rejected work: {0}")]
    ResourceAdmission(String),
    #[error("vNext outbound outbox failed: {0}")]
    Outbox(String),
    #[error("vNext runtime generation is fenced: {0}")]
    RuntimeFenced(String),
    #[error("vNext identity file is corrupt: {}", .0.display())]
    IdentityCorrupt(PathBuf),
    #[error("vNext identity signer returned an invalid Ed25519 public key")]
    IdentitySignerInvalid,
    #[error("vNext identity signer failed proof of possession")]
    IdentitySignerProofInvalid,
    #[error("vNext identity signer is unavailable: {0}")]
    IdentitySignerUnavailable(String),
    #[error("vNext identity signer requires caller-owned reprovisioning")]
    IdentityReprovisionRequired,
    #[error("vNext filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use ku_core::foundation::{
        decode_actor_root_delegation, decode_feed_inception, ActorDelegation, ActorRevocation,
        ActorRootDelegation, ConceptCcid, DeviceId, DisclosureClass, EventCid, FeedInception,
        KnowledgeEventEnvelope, NamespaceCommitment, ObjectReference, ReservedDomain, SelectorCid,
        UseEvidencePayload, UseMode, USE_EVIDENCE_EVENT_TYPE,
    };
    use ku_net::vnext_inventory_forest::HybridInventoryForest;
    use ku_net::vnext_reconciliation::BoundPayloadFrame;
    use onebrain_protocol::{
        bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
        ReconcileManifestKind, ReconcileReceiptStatus, ReconciliationBudget, ReconciliationContext,
        ReconciliationResumeMode, ReconciliationSummaryMethod,
    };

    struct CountingExternalSigner {
        key: SigningKey,
        signatures: AtomicU64,
    }

    impl CountingExternalSigner {
        fn new(seed: [u8; 32]) -> Self {
            Self {
                key: SigningKey::from_bytes(&seed),
                signatures: AtomicU64::new(0),
            }
        }
    }

    impl SessionIdentitySigner for CountingExternalSigner {
        fn public_key(&self) -> [u8; 32] {
            *self.key.verifying_key().as_bytes()
        }

        fn sign_session_message(&self, message: &[u8]) -> Result<[u8; 64], String> {
            self.signatures.fetch_add(1, Ordering::Relaxed);
            Ok(self.key.sign(message).to_bytes())
        }
    }

    #[test]
    fn network_storage_admission_enforces_projected_hard_watermark() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("vnext_existing.redb"), [0u8; 8]).unwrap();
        std::fs::write(directory.path().join("unrelated.bin"), [0u8; 64]).unwrap();
        let admission = NetworkStorageAdmission {
            data_dir: directory.path().to_path_buf(),
            hard_watermark_bytes: 10,
        };
        assert!(admission.ensure_writable(2).is_ok());
        assert_eq!(
            admission.ensure_writable(3),
            Err("VNEXT_STORAGE_HARD_WATERMARK".to_string())
        );
    }

    struct MismatchedExternalSigner {
        advertised_key: SigningKey,
        signing_key: SigningKey,
    }

    impl SessionIdentitySigner for MismatchedExternalSigner {
        fn public_key(&self) -> [u8; 32] {
            *self.advertised_key.verifying_key().as_bytes()
        }

        fn sign_session_message(&self, message: &[u8]) -> Result<[u8; 64], String> {
            Ok(self.signing_key.sign(message).to_bytes())
        }
    }

    fn context(session_id: [u8; 32], marker: u8) -> ReconciliationContext {
        ReconciliationContext {
            authenticated_transcript: session_id,
            selector: SelectorCid::from_bytes([marker; 32]),
            namespace: NamespaceCommitment::from_bytes([marker.wrapping_add(1); 32]),
            disclosure: DisclosureClass::Public,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 16,
                max_diff_ranges: 16,
                max_manifest_entries: 16,
                max_payload_bytes: 4096,
            },
            resume_mode: ReconciliationResumeMode::BoundTokenV1,
        }
    }

    fn peer_bound_context(session_id: [u8; 32], marker: u8) -> ReconciliationContext {
        let mut context = context(session_id, marker);
        context.resume_mode = ReconciliationResumeMode::PeerBoundTokenV2;
        context
    }

    fn feed_and_event() -> (Vec<u8>, Vec<u8>) {
        let key = SigningKey::from_bytes(&[0x71; 32]);
        let feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"runtime-feed", [0x72; 32]).unwrap(),
            0,
            DeviceId::from_bytes([0x73; 32]),
        )
        .sign(&key)
        .unwrap();
        let feed_bytes = feed.encode().unwrap();
        let author = decode_feed_inception(&feed_bytes).unwrap();
        let event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [0x74; 32],
        )
        .sign(&author, &key)
        .unwrap();
        (feed_bytes, event.encode().unwrap().0)
    }

    fn actor_root_feed_and_replay_attacker() -> (Vec<u8>, EventCid, Vec<u8>, FeedId, Vec<u8>, FeedId)
    {
        let root_key = SigningKey::from_bytes(&[0xa1; 32]);
        let feed_key = SigningKey::from_bytes(&[0xa2; 32]);
        let attacker_key = SigningKey::from_bytes(&[0xa3; 32]);
        let device = DeviceId::from_bytes([0xa4; 32]);
        let namespace = NamespaceCommitment::derive(b"runtime-authority-root", [0xa5; 32]).unwrap();
        let mut feed =
            FeedInception::new(*feed_key.verifying_key().as_bytes(), namespace, 0, device);
        let feed_id = feed.feed_id().unwrap();
        let authority_bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            feed_id,
            device,
            Some(namespace),
            0,
            0,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let authority_cid =
            EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&authority_bytes));
        feed.actor_delegation_ref = Some(authority_cid.into_bytes());
        let feed_bytes = feed.sign(&feed_key).unwrap().encode().unwrap();

        // Same public device/namespace/delegation reference, different feed
        // key and FeedId: this must never inherit the root grant.
        let mut attacker = FeedInception::new(
            *attacker_key.verifying_key().as_bytes(),
            namespace,
            0,
            device,
        );
        attacker.actor_delegation_ref = Some(authority_cid.into_bytes());
        let attacker_bytes = attacker.sign(&attacker_key).unwrap().encode().unwrap();
        let attacker_id = decode_feed_inception(&attacker_bytes).unwrap().feed_id;
        (
            authority_bytes,
            authority_cid,
            feed_bytes,
            feed_id,
            attacker_bytes,
            attacker_id,
        )
    }

    struct AuthorityChainFixture {
        root_bytes: Vec<u8>,
        parent_feed_bytes: Vec<u8>,
        delegation_bytes: Vec<u8>,
        delegation_cid: EventCid,
        child_feed_bytes: Vec<u8>,
        child_feed_id: FeedId,
        revocation_bytes: Vec<u8>,
        revocation_cid: EventCid,
    }

    fn authority_chain_fixture() -> AuthorityChainFixture {
        let root_key = SigningKey::from_bytes(&[0xb1; 32]);
        let parent_key = SigningKey::from_bytes(&[0xb2; 32]);
        let child_key = SigningKey::from_bytes(&[0xb3; 32]);
        let namespace =
            NamespaceCommitment::derive(b"runtime-authority-chain", [0xb4; 32]).unwrap();
        let parent_device = DeviceId::from_bytes([0xb5; 32]);
        let child_device = DeviceId::from_bytes([0xb6; 32]);
        let mut parent_feed = FeedInception::new(
            *parent_key.verifying_key().as_bytes(),
            namespace,
            0,
            parent_device,
        );
        let parent_feed_id = parent_feed.feed_id().unwrap();
        let root_bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            parent_feed_id,
            parent_device,
            Some(namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let root = decode_actor_root_delegation(&root_bytes).unwrap();
        parent_feed.actor_delegation_ref = Some(root.cid.into_bytes());
        let parent_feed_bytes = parent_feed.sign(&parent_key).unwrap().encode().unwrap();
        let parent_feed = decode_feed_inception(&parent_feed_bytes).unwrap();

        let mut child_feed = FeedInception::new(
            *child_key.verifying_key().as_bytes(),
            namespace,
            0,
            child_device,
        );
        let child_feed_id = child_feed.feed_id().unwrap();
        let delegation_bytes = ActorDelegation::new(
            root.signed.delegation.actor,
            root.cid,
            parent_feed.feed_id,
            child_feed_id,
            child_device,
            Some(namespace),
            0,
            1,
        )
        .unwrap()
        .sign(&parent_feed, &parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let delegation_cid =
            EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&delegation_bytes));
        child_feed.actor_delegation_ref = Some(delegation_cid.into_bytes());
        let child_feed_bytes = child_feed.sign(&child_key).unwrap().encode().unwrap();

        let revocation_bytes = ActorRevocation::new(
            root.signed.delegation.actor,
            delegation_cid,
            child_device,
            0,
            root.cid,
            parent_feed.feed_id,
        )
        .unwrap()
        .sign(&parent_feed, &parent_key)
        .unwrap()
        .encode()
        .unwrap();
        let revocation_cid =
            EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&revocation_bytes));
        AuthorityChainFixture {
            root_bytes,
            parent_feed_bytes,
            delegation_bytes,
            delegation_cid,
            child_feed_bytes,
            child_feed_id,
            revocation_bytes,
            revocation_cid,
        }
    }

    fn enqueue_for_runtime(
        sender: &VNextNetworkRuntime,
        receiver: &VNextNetworkRuntime,
        selector: SelectorCid,
        namespace: NamespaceCommitment,
        records: impl IntoIterator<Item = (ReconcileManifestKind, Vec<u8>)>,
    ) {
        let peer = NodeId::from_bytes(receiver.status().principal);
        for (kind, bytes) in records {
            let intent = OutboundTransferIntent::new(
                peer,
                receiver.local_addr(),
                selector,
                namespace,
                DisclosureClass::Public,
                kind,
                bytes,
            )
            .unwrap();
            sender.enqueue_outbound(&intent).unwrap();
        }
    }

    fn feed_object_and_event() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let key = SigningKey::from_bytes(&[0x75; 32]);
        let feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"runtime-object-dependency", [0x76; 32]).unwrap(),
            0,
            DeviceId::from_bytes([0x77; 32]),
        )
        .sign(&key)
        .unwrap();
        let feed_bytes = feed.encode().unwrap();
        let author = decode_feed_inception(&feed_bytes).unwrap();
        let (object_bytes, object_cid) = UseEvidencePayload {
            subjects: vec![ObjectReference::new(0, [0x78; 32])],
            mode: UseMode::Application,
            actor_class: ConceptCcid::from_bytes([0x79; 16]),
            task_context_commitment: [0x7A; 32],
            causal_role: ConceptCcid::from_bytes([0x7B; 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: ObjectReference::new(0, [0x7C; 32]),
            observed_frontier: [0x7D; 32],
        }
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ku_core::foundation::ResourceProfile::ObjectV1)
        .unwrap();
        let mut event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [0x78; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let event_bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
        (feed_bytes, object_bytes, event_bytes)
    }

    fn feed_and_equivocated_events() -> (Vec<u8>, FeedId, Vec<u8>, Vec<u8>) {
        let key = SigningKey::from_bytes(&[0x79; 32]);
        let feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"runtime-equivocation", [0x7A; 32]).unwrap(),
            0,
            DeviceId::from_bytes([0x7B; 32]),
        )
        .sign(&key)
        .unwrap();
        let feed_bytes = feed.encode().unwrap();
        let author = decode_feed_inception(&feed_bytes).unwrap();
        let left = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [0x7C; 32],
        )
        .sign(&author, &key)
        .unwrap()
        .encode()
        .unwrap()
        .0;
        let right = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [0x7D; 32],
        )
        .sign(&author, &key)
        .unwrap()
        .encode()
        .unwrap()
        .0;
        (feed_bytes, author.feed_id, left, right)
    }

    fn rotated_feed_bytes() -> (Vec<u8>, Vec<u8>, FeedId) {
        let previous_key = SigningKey::from_bytes(&[0x85; 32]);
        let successor_key = SigningKey::from_bytes(&[0x86; 32]);
        let namespace = NamespaceCommitment::derive(b"runtime-rotation", [0x87; 32]).unwrap();
        let device = DeviceId::from_bytes([0x88; 32]);
        let mut previous = FeedInception::new(
            *previous_key.verifying_key().as_bytes(),
            namespace,
            0,
            device,
        );
        let mut successor = FeedInception::new(
            *successor_key.verifying_key().as_bytes(),
            namespace,
            1,
            device,
        );
        successor.predecessor_feed = Some(previous.feed_id().unwrap());
        previous.commit_to_successor(&successor).unwrap();
        let previous_bytes = previous.sign(&previous_key).unwrap().encode().unwrap();
        let successor_bytes = successor.sign(&successor_key).unwrap().encode().unwrap();
        let successor_id = decode_feed_inception(&successor_bytes).unwrap().feed_id;
        (previous_bytes, successor_bytes, successor_id)
    }

    async fn send_event_feed_event(
        outbound: &OutboundVNextSession,
        marker: u8,
        feed_bytes: &[u8],
        event_bytes: &[u8],
    ) {
        let context = context(outbound.authenticated().session_id, marker);
        let event =
            BoundPayloadFrame::new(&context, ReconcileManifestKind::Event, event_bytes.to_vec())
                .unwrap();
        let feed = BoundPayloadFrame::new(
            &context,
            ReconcileManifestKind::FeedInception,
            feed_bytes.to_vec(),
        )
        .unwrap();
        assert_eq!(feed.cid, ReservedDomain::FeedInception.digest(feed_bytes));
        let manifest = bind_reconciliation_message(
            context,
            1,
            ReconciliationBody::Manifest {
                entries: vec![
                    ReconcileManifestEntry {
                        kind: event.kind,
                        cid: event.cid,
                        canonical_length: event.canonical_bytes.len() as u64,
                    },
                    ReconcileManifestEntry {
                        kind: feed.kind,
                        cid: feed.cid,
                        canonical_length: feed.canonical_bytes.len() as u64,
                    },
                ],
            },
        )
        .unwrap();
        outbound
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&manifest).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(event.clone()))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(feed))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(event))
            .await
            .unwrap();
    }

    async fn send_feed_event_object_event(
        outbound: &OutboundVNextSession,
        marker: u8,
        feed_bytes: &[u8],
        object_bytes: &[u8],
        event_bytes: &[u8],
    ) {
        let context = context(outbound.authenticated().session_id, marker);
        let feed = BoundPayloadFrame::new(
            &context,
            ReconcileManifestKind::FeedInception,
            feed_bytes.to_vec(),
        )
        .unwrap();
        let event =
            BoundPayloadFrame::new(&context, ReconcileManifestKind::Event, event_bytes.to_vec())
                .unwrap();
        let object = BoundPayloadFrame::new(
            &context,
            ReconcileManifestKind::Object,
            object_bytes.to_vec(),
        )
        .unwrap();
        let mut entries = [&feed, &event, &object]
            .into_iter()
            .map(|frame| ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
        let manifest =
            bind_reconciliation_message(context, 1, ReconciliationBody::Manifest { entries })
                .unwrap();
        outbound
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&manifest).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(feed))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(event.clone()))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(object))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(event))
            .await
            .unwrap();
    }

    async fn send_feed_and_equivocated_events(
        outbound: &OutboundVNextSession,
        marker: u8,
        feed_bytes: &[u8],
        left_event: &[u8],
        right_event: &[u8],
    ) {
        let context = context(outbound.authenticated().session_id, marker);
        let frames = [
            BoundPayloadFrame::new(
                &context,
                ReconcileManifestKind::FeedInception,
                feed_bytes.to_vec(),
            )
            .unwrap(),
            BoundPayloadFrame::new(&context, ReconcileManifestKind::Event, left_event.to_vec())
                .unwrap(),
            BoundPayloadFrame::new(&context, ReconcileManifestKind::Event, right_event.to_vec())
                .unwrap(),
        ];
        let mut entries = frames
            .iter()
            .map(|frame| ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
        let manifest =
            bind_reconciliation_message(context, 1, ReconciliationBody::Manifest { entries })
                .unwrap();
        outbound
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&manifest).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        for frame in frames {
            outbound
                .send(&CarrierRecord::BoundPayload(frame))
                .await
                .unwrap();
        }
    }

    async fn send_successor_predecessor_successor(
        outbound: &OutboundVNextSession,
        marker: u8,
        predecessor_bytes: &[u8],
        successor_bytes: &[u8],
    ) {
        let context = context(outbound.authenticated().session_id, marker);
        let predecessor = BoundPayloadFrame::new(
            &context,
            ReconcileManifestKind::FeedInception,
            predecessor_bytes.to_vec(),
        )
        .unwrap();
        let successor = BoundPayloadFrame::new(
            &context,
            ReconcileManifestKind::FeedInception,
            successor_bytes.to_vec(),
        )
        .unwrap();
        let mut entries = [&predecessor, &successor]
            .into_iter()
            .map(|frame| ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
        let manifest =
            bind_reconciliation_message(context, 1, ReconciliationBody::Manifest { entries })
                .unwrap();
        outbound
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&manifest).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(successor.clone()))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(predecessor))
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(successor))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn two_runtime_listeners_authenticate_and_reject_unvalidated_payload_bytes() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(left.authenticated_route_count().unwrap(), 0);
        assert_eq!(right.authenticated_route_count().unwrap(), 0);
        let left_principal = NodeId::from_bytes(left.status().principal);
        let right_principal = NodeId::from_bytes(right.status().principal);
        let mut outbound = left.connect(right.local_addr()).await.unwrap();
        let outbound_route = left
            .authenticated_route(right_principal)
            .unwrap()
            .expect("valid outbound handshake records responder route");
        assert_eq!(outbound_route.addr, right.local_addr());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if right.authenticated_route(left_principal).unwrap().is_some() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let context = context(outbound.authenticated().session_id, 0x61);
        let frame = BoundPayloadFrame::new(
            &context,
            ReconcileManifestKind::Object,
            b"not-a-canonical-object".to_vec(),
        )
        .unwrap();
        let manifest = bind_reconciliation_message(
            context,
            1,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: frame.kind,
                    cid: frame.cid,
                    canonical_length: frame.canonical_bytes.len() as u64,
                }],
            },
        )
        .unwrap();
        outbound
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&manifest).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        outbound
            .send(&CarrierRecord::BoundPayload(frame))
            .await
            .unwrap();

        let receipt = tokio::time::timeout(Duration::from_secs(5), outbound.recv())
            .await
            .unwrap()
            .unwrap();
        let AuthenticatedCarrierRecord::Reconciliation(receipt) = receipt else {
            panic!("receiver must return a reconciliation receipt");
        };
        let ReconciliationBody::Receipt { entries } = receipt.body else {
            panic!("receiver must return a receipt body");
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, ReconcileReceiptStatus::RejectedInvalid);

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if right.status().rejected_records == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(right.status().authenticated_sessions, 1);
        assert_eq!(right.status().accepted_records, 0);
        let status = right.status();
        assert_eq!(status.observability.outcomes.quarantined, 1);
        assert_eq!(
            status
                .observability
                .reasons
                .iter()
                .find(|counter| counter.reason == VNextReasonCode::QuarantinedInvalid)
                .unwrap()
                .count,
            1
        );
        assert!(!status.observability.contains_high_cardinality_labels);
        assert!(!status.observability.claims_network_completion);
        assert!(!status.claims_network_completion);

        outbound.close();
        left.shutdown().await;
        right.shutdown().await;
    }

    #[tokio::test]
    async fn peer_bound_resume_rebinds_durable_journal_after_quic_reconnect_and_restart() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let external_signer = Arc::new(CountingExternalSigner::new([0x69; 32]));
        let expected_principal = principal_node_id(&external_signer.public_key());
        let mut left = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start_with_signer(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            external_signer.clone(),
        )
        .await
        .unwrap();
        assert_eq!(right.status().principal, *expected_principal.as_bytes());
        assert!(!right_dir.path().join("vnext_identity.key").exists());
        let (_, object_bytes, _) = feed_object_and_event();

        let mut first = left.connect(right.local_addr()).await.unwrap();
        let first_context = peer_bound_context(first.authenticated().session_id, 0x6A);
        let first_frame = BoundPayloadFrame::new(
            &first_context,
            ReconcileManifestKind::Object,
            object_bytes.clone(),
        )
        .unwrap();
        let manifest = bind_reconciliation_message(
            first_context,
            1,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: first_frame.kind,
                    cid: first_frame.cid,
                    canonical_length: first_frame.canonical_bytes.len() as u64,
                }],
            },
        )
        .unwrap();
        first
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&manifest).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        first
            .send(&CarrierRecord::BoundPayload(first_frame.clone()))
            .await
            .unwrap();

        let mut resume_token = None;
        let mut first_status = None;
        for _ in 0..2 {
            let response = tokio::time::timeout(Duration::from_secs(5), first.recv())
                .await
                .unwrap()
                .unwrap();
            let AuthenticatedCarrierRecord::Reconciliation(message) = response else {
                panic!("receiver must return reconciliation control records");
            };
            match message.body {
                ReconciliationBody::Receipt { entries } => {
                    first_status = entries.first().map(|entry| entry.status);
                }
                ReconciliationBody::Progress {
                    resume_token: Some(token),
                    ..
                } => resume_token = Some(token),
                _ => {}
            }
        }
        assert_eq!(first_status, Some(ReconcileReceiptStatus::ValidatedStored));
        let resume_token = resume_token.expect("receiver must issue a V2 resume token");
        first.close();

        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().active_sessions != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        right.shutdown().await;
        drop(right);
        let signatures_before_restart = external_signer.signatures.load(Ordering::Relaxed);
        let mut restarted = VNextNetworkRuntime::start_with_signer(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            external_signer.clone(),
        )
        .await
        .unwrap();
        assert_eq!(restarted.status().principal, *expected_principal.as_bytes());
        assert!(!right_dir.path().join("vnext_identity.key").exists());

        let mut second = left.connect(restarted.local_addr()).await.unwrap();
        let second_context = peer_bound_context(second.authenticated().session_id, 0x6A);
        assert_ne!(
            manifest.context.authenticated_transcript,
            second_context.authenticated_transcript
        );
        let resume = bind_reconciliation_message(
            second_context.clone(),
            resume_token.next_sequence,
            ReconciliationBody::Resume {
                token: resume_token.clone(),
            },
        )
        .unwrap();
        assert_ne!(resume.binding_digest, resume_token.binding_digest);
        second
            .send(
                &CarrierRecord::reconciliation_message(
                    &encode_reconciliation_message(&resume).unwrap(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let resumed = tokio::time::timeout(Duration::from_secs(5), second.recv())
            .await
            .unwrap()
            .unwrap();
        let AuthenticatedCarrierRecord::Reconciliation(resumed) = resumed else {
            panic!("receiver must acknowledge Resume with Progress");
        };
        assert!(matches!(
            resumed.body,
            ReconciliationBody::Progress {
                resume_token: Some(_),
                ..
            }
        ));

        // The rebound journal restores and rebinds the old manifest. The
        // sender therefore retransmits only the payload, not the manifest.
        let second_frame =
            BoundPayloadFrame::new(&second_context, ReconcileManifestKind::Object, object_bytes)
                .unwrap();
        assert_eq!(second_frame.cid, first_frame.cid);
        second
            .send(&CarrierRecord::BoundPayload(second_frame))
            .await
            .unwrap();
        let mut resumed_status = None;
        let mut refreshed_token = false;
        for _ in 0..2 {
            let response = tokio::time::timeout(Duration::from_secs(5), second.recv())
                .await
                .unwrap()
                .unwrap();
            let AuthenticatedCarrierRecord::Reconciliation(message) = response else {
                panic!("receiver must return reconciliation control records");
            };
            match message.body {
                ReconciliationBody::Receipt { entries } => {
                    resumed_status = entries.first().map(|entry| entry.status);
                }
                ReconciliationBody::Progress {
                    resume_token: Some(_),
                    ..
                } => refreshed_token = true,
                _ => {}
            }
        }
        assert_eq!(resumed_status, Some(ReconcileReceiptStatus::AlreadyPresent));
        assert!(refreshed_token);
        assert!(
            external_signer.signatures.load(Ordering::Relaxed) > signatures_before_restart,
            "the restarted receiver must delegate handshake and resume-key signing"
        );
        second.close();

        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn event_before_feed_is_deferred_then_persisted_across_receiver_restart() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (feed_bytes, event_bytes) = feed_and_event();
        let outbound = left.connect(right.local_addr()).await.unwrap();
        send_event_feed_event(&outbound, 0x81, &feed_bytes, &event_bytes).await;

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = right.status();
                if status.deferred_records == 1 && status.accepted_records == 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        outbound.close();
        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().active_sessions != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        right.shutdown().await;
        drop(right);

        let mut restarted = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let replay = left.connect(restarted.local_addr()).await.unwrap();
        send_event_feed_event(&replay, 0x82, &feed_bytes, &event_bytes).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if restarted.status().accepted_records == 3 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(restarted.status().deferred_records, 0);
        assert_eq!(restarted.status().rejected_records, 0);
        assert!(!restarted.status().claims_network_completion);

        replay.close();
        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn event_before_payload_object_defers_over_two_authenticated_runtimes() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (feed_bytes, object_bytes, event_bytes) = feed_object_and_event();
        let outbound = left.connect(right.local_addr()).await.unwrap();
        send_feed_event_object_event(&outbound, 0x83, &feed_bytes, &object_bytes, &event_bytes)
            .await;

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = right.status();
                if status.deferred_records == 1 && status.accepted_records == 3 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(right.status().rejected_records, 0);
        assert!(!right.status().claims_network_completion);

        let selector = SelectorCid::from_bytes([0x83; 32]);
        let empty_root = HybridInventoryForest::new(selector).root();
        let inventory_root = right.inventory_root(selector).unwrap();
        assert_ne!(inventory_root, empty_root);

        outbound.close();
        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().active_sessions != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        right.shutdown().await;
        drop(right);

        let mut restarted = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(restarted.inventory_root(selector).unwrap(), inventory_root);

        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn feed_equivocation_projection_keeps_all_branches_across_restart() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (feed_bytes, feed_id, left_event, right_event) = feed_and_equivocated_events();
        let outbound = left.connect(right.local_addr()).await.unwrap();
        send_feed_and_equivocated_events(&outbound, 0x84, &feed_bytes, &left_event, &right_event)
            .await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().accepted_records != 3 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let projection = right.feed_projection(feed_id).unwrap();
        assert_eq!(projection.contiguous_through, Some(0));
        assert_eq!(projection.contiguous_tips.len(), 2);
        assert_eq!(projection.equivocations.len(), 1);
        assert_eq!(projection.equivocations[0].event_cids.len(), 2);

        outbound.close();
        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().active_sessions != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        right.shutdown().await;
        drop(right);
        let mut restarted = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(restarted.feed_projection(feed_id).unwrap(), projection);

        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn rotated_feed_defers_until_predecessor_and_survives_restart() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (predecessor, successor, successor_id) = rotated_feed_bytes();
        let outbound = left.connect(right.local_addr()).await.unwrap();
        send_successor_predecessor_successor(&outbound, 0x89, &predecessor, &successor).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = right.status();
                if status.deferred_records == 1 && status.accepted_records == 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(right.feed_inception_branch_count(successor_id).unwrap(), 1);

        outbound.close();
        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().active_sessions != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        right.shutdown().await;
        drop(right);
        let mut restarted = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            restarted.feed_inception_branch_count(successor_id).unwrap(),
            1
        );

        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn durable_outbox_authenticates_target_applies_receipt_and_survives_restart() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start_inner(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (feed_bytes, _) = feed_and_event();
        let intent = OutboundTransferIntent::new(
            NodeId::from_bytes(right.status().principal),
            right.local_addr(),
            SelectorCid::from_bytes([0x91; 32]),
            NamespaceCommitment::from_bytes([0x92; 32]),
            DisclosureClass::Public,
            ReconcileManifestKind::FeedInception,
            feed_bytes,
        )
        .unwrap();
        assert_eq!(
            left.enqueue_outbound(&intent).unwrap(),
            OutboxEnqueueOutcome::Added
        );
        let report = left.deliver_outbound_once(8).await.unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.attempted, 1);
        assert_eq!(report.acknowledged, 1);
        assert_eq!(report.deferred, 0);
        assert_eq!(report.rejected, 0);
        assert_eq!(report.failed, 0);
        assert!(!report.claims_network_completion);
        let stored = left.outbound_intent(&intent.id).unwrap().unwrap();
        assert_eq!(stored.state, OutboundIntentState::Acknowledged);
        assert_eq!(stored.transport_attempts, 0);
        assert_eq!(stored.validation_retries, 0);
        assert!(stored.terminal_sequence > 0);

        tokio::time::timeout(Duration::from_secs(5), async {
            while right.status().accepted_records != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        left.shutdown().await;
        drop(left);

        let mut restarted = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            restarted
                .outbound_intent(&intent.id)
                .unwrap()
                .unwrap()
                .state,
            OutboundIntentState::Acknowledged
        );
        assert_eq!(restarted.deliver_outbound_once(8).await.unwrap().scanned, 0);

        restarted.shutdown().await;
        right.shutdown().await;
    }

    #[tokio::test]
    async fn actor_root_authority_reconciles_over_quic_survives_restart_and_rejects_feed_replay() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start_inner(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (authority, authority_cid, feed, feed_id, attacker, attacker_id) =
            actor_root_feed_and_replay_attacker();
        let selector = SelectorCid::from_bytes([0xa6; 32]);
        let namespace = NamespaceCommitment::from_bytes([0xa7; 32]);
        for (kind, bytes) in [
            (ReconcileManifestKind::AuthorityEvent, authority),
            (ReconcileManifestKind::FeedInception, feed),
            (ReconcileManifestKind::FeedInception, attacker),
        ] {
            let intent = OutboundTransferIntent::new(
                NodeId::from_bytes(right.status().principal),
                right.local_addr(),
                selector,
                namespace,
                DisclosureClass::Public,
                kind,
                bytes,
            )
            .unwrap();
            assert_eq!(
                left.enqueue_outbound(&intent).unwrap(),
                OutboxEnqueueOutcome::Added
            );
        }
        let report = left.deliver_outbound_once(8).await.unwrap();
        assert_eq!(report.acknowledged, 3);
        assert_eq!(report.rejected, 0);
        assert_eq!(report.deferred, 0);

        assert_eq!(
            right
                .feed_authority_at_root(feed_id, authority_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );
        assert_eq!(
            right
                .feed_authority_at_root(attacker_id, authority_cid)
                .unwrap()[0]
                .code(),
            "STALE_OR_UNRESOLVED"
        );
        right.shutdown().await;
        drop(right);

        let mut restarted = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            restarted
                .feed_authority_at_root(feed_id, authority_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );
        assert_eq!(
            restarted
                .feed_authority_at_root(attacker_id, authority_cid)
                .unwrap()[0]
                .code(),
            "STALE_OR_UNRESOLVED"
        );

        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn delegated_authority_and_revocation_reconcile_over_quic_and_rebuild_after_restart() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start_inner(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let fixture = authority_chain_fixture();
        let peer = NodeId::from_bytes(right.status().principal);
        let selector = SelectorCid::from_bytes([0xb7; 32]);
        let namespace = NamespaceCommitment::from_bytes([0xb8; 32]);

        // A child arriving first is explicitly deferred and remains pending
        // in the durable outbox rather than being rejected or trusted.
        let delegation_intent = OutboundTransferIntent::new(
            peer,
            right.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::AuthorityEvent,
            fixture.delegation_bytes.clone(),
        )
        .unwrap();
        left.enqueue_outbound(&delegation_intent).unwrap();
        let deferred = left.deliver_outbound_once(8).await.unwrap();
        assert_eq!(deferred.deferred, 1);
        assert_eq!(deferred.rejected, 0);

        // Phase 1 establishes the self-certifying root and its online
        // authorizing feed, then redelivers the pending child. CID ordering
        // may accept the child in this batch or the immediately following one.
        for (kind, bytes) in [
            (
                ReconcileManifestKind::AuthorityEvent,
                fixture.root_bytes.clone(),
            ),
            (
                ReconcileManifestKind::FeedInception,
                fixture.parent_feed_bytes.clone(),
            ),
        ] {
            let intent = OutboundTransferIntent::new(
                peer,
                right.local_addr(),
                selector,
                namespace,
                DisclosureClass::Public,
                kind,
                bytes,
            )
            .unwrap();
            left.enqueue_outbound(&intent).unwrap();
        }
        left.deliver_outbound_once(8).await.unwrap();
        if left
            .outbound_intent(&delegation_intent.id)
            .unwrap()
            .unwrap()
            .state
            != OutboundIntentState::Acknowledged
        {
            left.deliver_outbound_once(8).await.unwrap();
        }
        assert_eq!(
            left.outbound_intent(&delegation_intent.id)
                .unwrap()
                .unwrap()
                .state,
            OutboundIntentState::Acknowledged
        );

        // Phase 2 carries the exact child FeedInception named by the admitted
        // parent-signed delegation.
        let child_intent = OutboundTransferIntent::new(
            peer,
            right.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::FeedInception,
            fixture.child_feed_bytes.clone(),
        )
        .unwrap();
        left.enqueue_outbound(&child_intent).unwrap();
        assert_eq!(left.deliver_outbound_once(8).await.unwrap().acknowledged, 1);
        assert_eq!(
            right
                .feed_authority_at(fixture.child_feed_id, fixture.delegation_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );

        // Phase 3 revokes from generation zero. The old child frontier remains
        // a historical Authorized projection; the new frontier is revoked.
        let revocation_intent = OutboundTransferIntent::new(
            peer,
            right.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::AuthorityEvent,
            fixture.revocation_bytes.clone(),
        )
        .unwrap();
        left.enqueue_outbound(&revocation_intent).unwrap();
        assert_eq!(left.deliver_outbound_once(8).await.unwrap().acknowledged, 1);
        assert_eq!(
            right
                .feed_authority_at(fixture.child_feed_id, fixture.revocation_cid)
                .unwrap()[0]
                .code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
        assert_eq!(
            right
                .feed_authority_at(fixture.child_feed_id, fixture.delegation_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );
        right.shutdown().await;
        drop(right);

        let mut restarted = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            restarted
                .feed_authority_at(fixture.child_feed_id, fixture.revocation_cid)
                .unwrap()[0]
                .code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
        assert_eq!(
            restarted
                .feed_authority_at(fixture.child_feed_id, fixture.delegation_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );

        left.shutdown().await;
        restarted.shutdown().await;
    }

    #[tokio::test]
    async fn partitioned_authority_views_remain_relative_then_converge_after_reunion() {
        let sender_dir = tempfile::tempdir().unwrap();
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let mut sender = VNextNetworkRuntime::start_inner(
            sender_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut first = VNextNetworkRuntime::start(
            first_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let mut second = VNextNetworkRuntime::start(
            second_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let fixture = authority_chain_fixture();
        let selector = SelectorCid::from_bytes([0xc1; 32]);
        let namespace = NamespaceCommitment::from_bytes([0xc2; 32]);

        // Both components receive the same root and authorizing feed.
        for receiver in [&first, &second] {
            enqueue_for_runtime(
                &sender,
                receiver,
                selector,
                namespace,
                [
                    (
                        ReconcileManifestKind::AuthorityEvent,
                        fixture.root_bytes.clone(),
                    ),
                    (
                        ReconcileManifestKind::FeedInception,
                        fixture.parent_feed_bytes.clone(),
                    ),
                ],
            );
        }
        assert_eq!(
            sender.deliver_outbound_once(8).await.unwrap().acknowledged,
            4
        );

        // Both components receive the same child delegation and child feed.
        for receiver in [&first, &second] {
            enqueue_for_runtime(
                &sender,
                receiver,
                selector,
                namespace,
                [
                    (
                        ReconcileManifestKind::AuthorityEvent,
                        fixture.delegation_bytes.clone(),
                    ),
                    (
                        ReconcileManifestKind::FeedInception,
                        fixture.child_feed_bytes.clone(),
                    ),
                ],
            );
        }
        assert_eq!(
            sender.deliver_outbound_once(8).await.unwrap().acknowledged,
            4
        );
        for receiver in [&first, &second] {
            assert_eq!(
                receiver
                    .feed_authority_at(fixture.child_feed_id, fixture.delegation_cid)
                    .unwrap()[0]
                    .code(),
                "AUTHORIZED_RELATIVE"
            );
        }

        // During the partition only the first component observes revocation.
        enqueue_for_runtime(
            &sender,
            &first,
            selector,
            namespace,
            [(
                ReconcileManifestKind::AuthorityEvent,
                fixture.revocation_bytes.clone(),
            )],
        );
        assert_eq!(
            sender.deliver_outbound_once(8).await.unwrap().acknowledged,
            1
        );
        assert_eq!(
            first
                .feed_authority_at(fixture.child_feed_id, fixture.revocation_cid)
                .unwrap()[0]
                .code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
        assert!(second
            .feed_authority_at(fixture.child_feed_id, fixture.revocation_cid)
            .unwrap()
            .is_empty());
        assert_eq!(
            second
                .feed_authority_at(fixture.child_feed_id, fixture.delegation_cid)
                .unwrap()[0]
                .code(),
            "AUTHORIZED_RELATIVE"
        );

        // Reunion carries the immutable revocation proof; no seed, quorum or
        // geography input participates in the converged decision.
        enqueue_for_runtime(
            &sender,
            &second,
            selector,
            namespace,
            [(
                ReconcileManifestKind::AuthorityEvent,
                fixture.revocation_bytes.clone(),
            )],
        );
        assert_eq!(
            sender.deliver_outbound_once(8).await.unwrap().acknowledged,
            1
        );
        assert_eq!(
            second
                .feed_authority_at(fixture.child_feed_id, fixture.revocation_cid)
                .unwrap()[0]
                .code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );

        sender.shutdown().await;
        first.shutdown().await;
        second.shutdown().await;
    }

    #[tokio::test]
    async fn outbound_records_with_one_context_share_one_authenticated_batch() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start_inner(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let selector = SelectorCid::from_bytes([0x95; 32]);
        let namespace = NamespaceCommitment::from_bytes([0x96; 32]);
        let peer = NodeId::from_bytes(right.status().principal);
        let mut intents = Vec::new();
        for marker in [1u8, 2] {
            let (bytes, _) = UseEvidencePayload {
                subjects: vec![ObjectReference::new(0, [marker; 32])],
                mode: UseMode::Application,
                actor_class: ConceptCcid::from_bytes([marker.wrapping_add(1); 16]),
                task_context_commitment: [marker.wrapping_add(2); 32],
                causal_role: ConceptCcid::from_bytes([marker.wrapping_add(3); 16]),
                assembly: None,
                mapping: None,
                outcome_observation: None,
                use_policy: ObjectReference::new(0, [marker.wrapping_add(4); 32]),
                observed_frontier: [marker.wrapping_add(5); 32],
            }
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ku_core::foundation::ResourceProfile::ObjectV1)
            .unwrap();
            let intent = OutboundTransferIntent::new(
                peer,
                right.local_addr(),
                selector,
                namespace,
                DisclosureClass::Public,
                ReconcileManifestKind::Object,
                bytes,
            )
            .unwrap();
            left.enqueue_outbound(&intent).unwrap();
            intents.push(intent);
        }

        let report = left.deliver_outbound_once(8).await.unwrap();
        assert_eq!(report.scanned, 2);
        assert_eq!(report.attempted, 2);
        assert_eq!(report.acknowledged, 2);
        assert_eq!(report.failed, 0);
        assert_eq!(right.status().authenticated_sessions, 1);
        assert_eq!(right.status().accepted_records, 2);
        for intent in intents {
            assert_eq!(
                left.outbound_intent(&intent.id).unwrap().unwrap().state,
                OutboundIntentState::Acknowledged
            );
        }

        left.shutdown().await;
        right.shutdown().await;
    }

    #[tokio::test]
    async fn continuous_scheduler_replays_restart_pending_and_wakes_on_enqueue() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut dormant = VNextNetworkRuntime::start_inner(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let peer = NodeId::from_bytes(right.status().principal);
        let selector = SelectorCid::from_bytes([0x97; 32]);
        let namespace = NamespaceCommitment::from_bytes([0x98; 32]);
        let make_intent = |marker: u8| {
            let (bytes, _) = UseEvidencePayload {
                subjects: vec![ObjectReference::new(0, [marker; 32])],
                mode: UseMode::Application,
                actor_class: ConceptCcid::from_bytes([marker.wrapping_add(1); 16]),
                task_context_commitment: [marker.wrapping_add(2); 32],
                causal_role: ConceptCcid::from_bytes([marker.wrapping_add(3); 16]),
                assembly: None,
                mapping: None,
                outcome_observation: None,
                use_policy: ObjectReference::new(0, [marker.wrapping_add(4); 32]),
                observed_frontier: [marker.wrapping_add(5); 32],
            }
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ku_core::foundation::ResourceProfile::ObjectV1)
            .unwrap();
            OutboundTransferIntent::new(
                peer,
                right.local_addr(),
                selector,
                namespace,
                DisclosureClass::Public,
                ReconcileManifestKind::Object,
                bytes,
            )
            .unwrap()
        };
        let restart_pending = make_intent(3);
        dormant.enqueue_outbound(&restart_pending).unwrap();
        dormant.shutdown().await;
        drop(dormant);

        let mut active = VNextNetworkRuntime::start(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if active
                    .outbound_intent(&restart_pending.id)
                    .unwrap()
                    .is_some_and(|intent| intent.state == OutboundIntentState::Acknowledged)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();

        let notified = make_intent(4);
        active.enqueue_outbound(&notified).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if active
                    .outbound_intent(&notified.id)
                    .unwrap()
                    .is_some_and(|intent| intent.state == OutboundIntentState::Acknowledged)
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(right.status().accepted_records, 2);

        active.shutdown().await;
        right.shutdown().await;
    }

    #[tokio::test]
    async fn outbound_target_node_mismatch_never_sends_payload() {
        let left_dir = tempfile::tempdir().unwrap();
        let right_dir = tempfile::tempdir().unwrap();
        let mut left = VNextNetworkRuntime::start_inner(
            left_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
        )
        .await
        .unwrap();
        let mut right = VNextNetworkRuntime::start(
            right_dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
        )
        .await
        .unwrap();
        let (feed_bytes, _) = feed_and_event();
        let intent = OutboundTransferIntent::new(
            NodeId::from_bytes([0xFF; 32]),
            right.local_addr(),
            SelectorCid::from_bytes([0x93; 32]),
            NamespaceCommitment::from_bytes([0x94; 32]),
            DisclosureClass::Public,
            ReconcileManifestKind::FeedInception,
            feed_bytes,
        )
        .unwrap();
        left.enqueue_outbound(&intent).unwrap();
        let report = left.deliver_outbound_once(1).await.unwrap();
        assert_eq!(report.attempted, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.acknowledged, 0);
        assert_eq!(right.status().accepted_records, 0);
        assert_eq!(
            left.outbound_intent(&intent.id).unwrap().unwrap().state,
            OutboundIntentState::Pending
        );

        left.shutdown().await;
        right.shutdown().await;
    }

    #[test]
    fn persistent_identity_is_stable_and_corruption_is_explicit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("identity.key");
        let first = load_or_create_identity(&path).unwrap();
        let second = load_or_create_identity(&path).unwrap();
        assert_eq!(first.verifying_key(), second.verifying_key());
        std::fs::write(&path, b"truncated").unwrap();
        assert!(matches!(
            load_or_create_identity(&path),
            Err(VNextNetworkRuntimeError::IdentityCorrupt(_))
        ));
    }

    #[tokio::test]
    async fn external_signer_must_prove_key_possession_before_runtime_side_effects() {
        let parent = tempfile::tempdir().unwrap();
        let data_dir = parent.path().join("must-not-be-created");
        let signer: Arc<dyn SessionIdentitySigner> = Arc::new(MismatchedExternalSigner {
            advertised_key: SigningKey::from_bytes(&[0xA1; 32]),
            signing_key: SigningKey::from_bytes(&[0xA2; 32]),
        });
        let result = VNextNetworkRuntime::start_with_signer(
            &data_dir,
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            signer,
        )
        .await;
        assert!(matches!(
            result,
            Err(VNextNetworkRuntimeError::IdentitySignerProofInvalid)
        ));
        assert!(!data_dir.exists());
    }

    #[test]
    fn outbound_retry_delay_is_exponential_and_bounded() {
        assert_eq!(
            next_retry_delay(OUTBOUND_RETRY_BASE),
            Duration::from_millis(500)
        );
        assert_eq!(
            next_retry_delay(Duration::from_secs(20)),
            OUTBOUND_RETRY_MAX
        );
        assert_eq!(next_retry_delay(OUTBOUND_RETRY_MAX), OUTBOUND_RETRY_MAX);
    }

    #[test]
    fn admission_and_payload_failures_map_to_stable_low_cardinality_reasons() {
        let observability = VNextObservability::default();
        let _ =
            observable_resource_admission_error(&observability, ResourceAdmissionError::WindowIp);
        let _ = observable_resource_admission_error(
            &observability,
            ResourceAdmissionError::SessionPeer,
        );
        let _ =
            observable_resource_admission_error(&observability, ResourceAdmissionError::StageOrder);
        assert_eq!(
            payload_reject_reason(PayloadRejectReason::ContextBinding),
            VNextReasonCode::RejectedContextBinding
        );
        assert_eq!(
            payload_reject_reason(PayloadRejectReason::SinkValidation),
            VNextReasonCode::QuarantinedInvalid
        );

        let snapshot = observability.snapshot(VNextRegistryTelemetryState::Unknown);
        assert_eq!(snapshot.resources.rate_limited, 1);
        assert_eq!(snapshot.outcomes.rejected, 3);
        assert!(!snapshot.contains_high_cardinality_labels);
        assert!(!snapshot.contains_private_need_labels);
    }
}
