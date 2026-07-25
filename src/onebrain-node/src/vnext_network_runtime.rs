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
    EventCid, FeedAuthorityDecision, FeedId, FeedProjection, InventoryRecordKind, NodeId,
    RedbVerifiedBackend,
};
use ku_net::transport::{OBPConnection, QuicTransport, TransportConfig};
use ku_net::vnext_carrier::CarrierRecord;
use ku_net::vnext_inventory_forest::{InventoryLeaf, RedbInventoryForestBackend};
use ku_net::vnext_quic_session::{
    accept_authenticated_session, initiate_authenticated_session, send_carrier_record,
    AuthenticatedCarrierRecord, AuthenticatedCarrierSession,
};
use ku_net::vnext_reconciliation::{
    BoundPayloadFrame, PayloadIngestOutcome, PayloadSinkOutcome, ReceiverState,
    ValidateThenAcceptSink,
};
use ku_net::vnext_reconciliation_journal::persistent::RedbReconciliationJournalBackend;
use ku_net::vnext_reconciliation_journal::{
    JournaledPayloadOutcome, JournaledReconciliationSession, ReconciliationJournalConfig,
};
use ku_net::vnext_session::{
    principal_node_id, AuthenticatedSession, SessionIdentitySigner, SessionReplayGuard,
};
use onebrain_protocol::{
    bind_reconciliation_message, encode_reconciliation_message, reconciliation_capability,
    reconciliation_profile, ReconcileManifestEntry, ReconcileReceiptStatus, ReconciliationBody,
    ReconciliationBudget, ReconciliationContext, ReconciliationResumeMode,
    ReconciliationSummaryMethod,
};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{watch, Mutex as AsyncMutex, Notify, Semaphore};
use tokio::task::JoinHandle;

use crate::vnext_config::VNextNetworkPolicy;
use crate::vnext_outbox::{
    OutboundIntentState, OutboundOutbox, OutboundTransferIntent, OutboxEnqueueOutcome,
    MAX_OUTBOX_PAYLOAD_BYTES,
};
use crate::vnext_record_provenance::RedbRecordProvenance;
use crate::vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};

const IDENTITY_MAGIC: &[u8; 8] = b"OBIDV1\0\0";
const IDENTITY_BYTES: usize = 40;
const OUTBOUND_RETRY_BASE: Duration = Duration::from_millis(250);
const OUTBOUND_RETRY_MAX: Duration = Duration::from_secs(30);

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
}

impl ValidateThenAcceptSink for InventoryingSink {
    fn validate_then_accept(
        &mut self,
        kind: onebrain_protocol::ReconcileManifestKind,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
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
            self.provenance
                .observe(kind, cid, self.selector, self.source_peer)
                .map_err(|error| format!("VNEXT_PROVENANCE: {error}"))?;
        }
        Ok(outcome)
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
    counters: Arc<RuntimeCounters>,
    outbox: OutboundOutbox,
    scheduler: AsyncMutex<()>,
    policy: VNextNetworkPolicy,
}

/// Owns the real QUIC endpoint, persistent validated store, persistent
/// reconciliation journal, session replay guard, and bounded accept loop.
pub struct VNextNetworkRuntime {
    transport: Arc<QuicTransport>,
    principal: NodeId,
    listen_addr: SocketAddr,
    counters: Arc<RuntimeCounters>,
    validated_sink: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    outbound: Arc<OutboundDeliveryEngine>,
    outbound_notify: Arc<Notify>,
    outbound_shutdown: watch::Sender<bool>,
    outbound_task: Option<JoinHandle<()>>,
    accept_task: JoinHandle<()>,
    state: VNextNetworkRuntimeState,
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
        Self::start_inner(data_dir, bind_addr, policy, true).await
    }

    async fn start_inner(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        continuous_outbound: bool,
    ) -> Result<Self, VNextNetworkRuntimeError> {
        policy
            .validate()
            .map_err(|error| VNextNetworkRuntimeError::Config(error.to_string()))?;
        std::fs::create_dir_all(data_dir)?;
        let identity: Arc<dyn SessionIdentitySigner> = Arc::new(load_or_create_identity(
            &data_dir.join("vnext_identity.key"),
        )?);
        let identity_public_key = validate_identity_signer(identity.as_ref())?;
        Self::start_initialized(
            data_dir,
            bind_addr,
            policy,
            continuous_outbound,
            identity,
            identity_public_key,
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
        policy
            .validate()
            .map_err(|error| VNextNetworkRuntimeError::Config(error.to_string()))?;
        let identity_public_key = validate_identity_signer(identity.as_ref())?;
        std::fs::create_dir_all(data_dir)?;
        Self::start_initialized(
            data_dir,
            bind_addr,
            policy,
            true,
            identity,
            identity_public_key,
        )
        .await
    }

    async fn start_initialized(
        data_dir: &Path,
        bind_addr: SocketAddr,
        policy: VNextNetworkPolicy,
        continuous_outbound: bool,
        identity: Arc<dyn SessionIdentitySigner>,
        identity_public_key: [u8; 32],
    ) -> Result<Self, VNextNetworkRuntimeError> {
        let principal = principal_node_id(&identity_public_key);
        let sink = SharedVNextValidatedSink::new(VNextValidatedSink::new(
            RedbVerifiedBackend::open(&data_dir.join("vnext_verified.redb"))
                .map_err(VNextNetworkRuntimeError::Storage)?,
        ));
        let journal =
            RedbReconciliationJournalBackend::open(data_dir.join("vnext_reconciliation.redb"))
                .map_err(VNextNetworkRuntimeError::Storage)?;
        let inventory = RedbInventoryForestBackend::open(&data_dir.join("vnext_inventory.redb"))
            .map_err(|error| VNextNetworkRuntimeError::Inventory(format!("{error:?}")))?;
        let provenance = RedbRecordProvenance::open(data_dir.join("vnext_record_provenance.redb"))
            .map_err(VNextNetworkRuntimeError::Provenance)?;
        let outbox = OutboundOutbox::open(&data_dir.join("vnext_outbox.redb"))
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
        let replay_guard = Arc::new(Mutex::new(SessionReplayGuard::default()));
        let counters = Arc::new(RuntimeCounters::default());
        let semaphore = Arc::new(Semaphore::new(policy.max_concurrent_sessions));
        let accept_task = tokio::spawn(accept_loop(
            Arc::clone(&transport),
            Arc::clone(&identity),
            Arc::clone(&replay_guard),
            Arc::clone(&counters),
            semaphore,
            journal,
            sink.clone(),
            inventory.clone(),
            provenance.clone(),
            policy,
        ));
        let outbound = Arc::new(OutboundDeliveryEngine {
            transport: Arc::clone(&transport),
            identity: Arc::clone(&identity),
            replay_guard: Arc::clone(&replay_guard),
            counters: Arc::clone(&counters),
            outbox,
            scheduler: AsyncMutex::new(()),
            policy,
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
            validated_sink: sink,
            inventory,
            provenance,
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
            claims_network_completion: false,
        }
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

    pub fn accepted_object_bytes(&self) -> Result<Vec<Vec<u8>>, VNextNetworkRuntimeError> {
        self.validated_sink
            .accepted_objects()
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    pub fn accepted_event_bytes(&self) -> Result<Vec<Vec<u8>>, VNextNetworkRuntimeError> {
        self.validated_sink
            .accepted_events()
            .map_err(VNextNetworkRuntimeError::Storage)
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
    pub fn feed_authority_at_root(
        &self,
        feed_id: FeedId,
        authority_root: EventCid,
    ) -> Result<Vec<FeedAuthorityDecision>, VNextNetworkRuntimeError> {
        self.validated_sink
            .feed_authority_at_root(feed_id, authority_root)
            .map_err(VNextNetworkRuntimeError::Storage)
    }

    pub fn feed_authority_at(
        &self,
        feed_id: FeedId,
        authority_frontier: EventCid,
    ) -> Result<Vec<FeedAuthorityDecision>, VNextNetworkRuntimeError> {
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
    async fn deliver_once(
        &self,
        limit: usize,
    ) -> Result<OutboundDeliveryReport, VNextNetworkRuntimeError> {
        let _scheduler = self.scheduler.lock().await;
        let max_payload_records = self.policy.max_records_per_session.saturating_sub(1) as usize;
        let bounded_limit = limit.min(max_payload_records);
        let mut pending = self
            .outbox
            .pending(bounded_limit)
            .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
        let mut report = OutboundDeliveryReport {
            scanned: pending.len(),
            claims_network_completion: false,
            ..OutboundDeliveryReport::default()
        };

        while !pending.is_empty() {
            let seed = pending.remove(0);
            if seed.attempts >= self.policy.max_retries_per_record {
                report.retry_exhausted += 1;
                continue;
            }

            let mut batch = vec![seed];
            let mut batch_bytes = batch[0].canonical_bytes.len() as u64;
            let mut candidate = 0;
            while candidate < pending.len() {
                let next = &pending[candidate];
                let next_bytes = next.canonical_bytes.len() as u64;
                if next.attempts < self.policy.max_retries_per_record
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
                .record_attempt(&intent.id)
                .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
            report.attempted += 1;
        }

        let first = &batch[0];
        let mut session = match self.connect(first.last_known_addr).await {
            Ok(session) => session,
            Err(_) => {
                report.failed += batch.len();
                return Ok(());
            }
        };
        if session.authenticated().responder != first.expected_peer {
            session.close();
            report.failed += batch.len();
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
            session.close();
            report.failed += batch.len();
            return Ok(());
        }
        for frame in frames {
            if session
                .send(&CarrierRecord::BoundPayload(frame))
                .await
                .is_err()
            {
                session.close();
                report.failed += batch.len();
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
                report.failed += 1;
                continue;
            };
            let state = self
                .outbox
                .apply_receipt(&intent.id, status)
                .map_err(|error| VNextNetworkRuntimeError::Outbox(error.to_string()))?;
            match (state, status) {
                (OutboundIntentState::Acknowledged, _) => report.acknowledged += 1,
                (OutboundIntentState::Rejected, _) => report.rejected += 1,
                (
                    OutboundIntentState::Pending,
                    ReconcileReceiptStatus::DeferredBudget
                    | ReconcileReceiptStatus::DeferredMissingDependency,
                ) => report.deferred += 1,
                _ => report.failed += 1,
            }
        }
        Ok(())
    }

    async fn connect(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, VNextNetworkRuntimeError> {
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
        self.replay_guard
            .lock()
            .map_err(|_| VNextNetworkRuntimeError::ReplayGuard)?
            .accept(&authenticated)
            .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
        self.counters
            .authenticated_sessions
            .fetch_add(1, Ordering::Relaxed);
        let carrier = AuthenticatedCarrierSession::new(authenticated.clone());
        Ok(OutboundVNextSession {
            connection,
            authenticated,
            carrier,
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
            Err(_) => {
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
}

impl OutboundVNextSession {
    pub fn authenticated(&self) -> &AuthenticatedSession {
        &self.authenticated
    }

    pub async fn send(&self, record: &CarrierRecord) -> Result<(), VNextNetworkRuntimeError> {
        send_carrier_record(&self.connection, record)
            .await
            .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))
    }

    pub async fn recv(&mut self) -> Result<AuthenticatedCarrierRecord, VNextNetworkRuntimeError> {
        self.carrier
            .recv(&self.connection)
            .await
            .map_err(|error| VNextNetworkRuntimeError::Session(error.to_string()))
    }

    pub fn close(&self) {
        self.connection.close("OBP-RP session complete");
    }
}

#[allow(clippy::too_many_arguments)]
async fn accept_loop(
    transport: Arc<QuicTransport>,
    identity: Arc<dyn SessionIdentitySigner>,
    replay_guard: Arc<Mutex<SessionReplayGuard>>,
    counters: Arc<RuntimeCounters>,
    semaphore: Arc<Semaphore>,
    journal: RedbReconciliationJournalBackend,
    sink: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    policy: VNextNetworkPolicy,
) {
    loop {
        let connection = match transport.accept().await {
            Ok(connection) => connection,
            Err(_) => break,
        };
        let permit = match Arc::clone(&semaphore).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                counters.rejected_sessions.fetch_add(1, Ordering::Relaxed);
                connection.close("OBP-RP session budget exhausted");
                continue;
            }
        };
        let identity = Arc::clone(&identity);
        let replay_guard = Arc::clone(&replay_guard);
        let counters = Arc::clone(&counters);
        let journal = journal.clone();
        let sink = sink.clone();
        let inventory = inventory.clone();
        let provenance = provenance.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if handle_inbound_connection(
                connection,
                identity,
                replay_guard,
                Arc::clone(&counters),
                journal,
                sink,
                inventory,
                provenance,
                policy,
            )
            .await
            .is_err()
            {
                counters.rejected_sessions.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
}

async fn handle_inbound_connection(
    connection: OBPConnection,
    identity: Arc<dyn SessionIdentitySigner>,
    replay_guard: Arc<Mutex<SessionReplayGuard>>,
    counters: Arc<RuntimeCounters>,
    journal_backend: RedbReconciliationJournalBackend,
    sink: PersistentSink,
    inventory: RedbInventoryForestBackend,
    provenance: RedbRecordProvenance,
    policy: VNextNetworkPolicy,
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
    replay_guard
        .lock()
        .map_err(|_| VNextNetworkRuntimeError::ReplayGuard)?
        .accept(&authenticated)
        .map_err(|error| VNextNetworkRuntimeError::Session(format!("{error:?}")))?;
    counters
        .authenticated_sessions
        .fetch_add(1, Ordering::Relaxed);
    counters.active_sessions.fetch_add(1, Ordering::Relaxed);
    let _active = ActiveSessionCounter(Arc::clone(&counters));

    let resume_token_key = peer_bound_resume_token_key(identity.as_ref(), &authenticated)?;
    let source_peer = authenticated.initiator;
    let mut carrier = AuthenticatedCarrierSession::new(authenticated);
    let mut journals = BTreeMap::<[u8; 32], PersistentJournal>::new();
    let mut outbound_sequence = 1u64;
    for _ in 0..policy.max_records_per_session {
        let record = match carrier.recv(&connection).await {
            Ok(record) => record,
            Err(_) => break,
        };
        match record {
            AuthenticatedCarrierRecord::Reconciliation(message) => {
                let binding = message.binding_digest;
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
                    continue;
                }
                if !journals.contains_key(&binding) {
                    let inventorying_sink = InventoryingSink {
                        inner: sink.clone(),
                        inventory: inventory.clone(),
                        provenance: provenance.clone(),
                        selector: message.context.selector,
                        source_peer,
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
                }
                if matches!(message.body, ReconciliationBody::Manifest { .. }) {
                    journals
                        .get_mut(&binding)
                        .expect("journal inserted")
                        .ingest_manifest(&message)
                        .map_err(|error| VNextNetworkRuntimeError::Journal(format!("{error:?}")))?;
                }
            }
            AuthenticatedCarrierRecord::BoundPayload(frame) => {
                let Some(journal) = journals.get_mut(&frame.binding_digest) else {
                    counters.deferred_records.fetch_add(1, Ordering::Relaxed);
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
                    JournaledPayloadOutcome::Delivered(
                        PayloadIngestOutcome::ValidatedStored
                        | PayloadIngestOutcome::AlreadyPresent,
                    ) => {
                        counters.accepted_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::Delivered(
                        PayloadIngestOutcome::DeferredUntilManifest
                        | PayloadIngestOutcome::DeferredMissingDependency,
                    )
                    | JournaledPayloadOutcome::Backpressured => {
                        counters.deferred_records.fetch_add(1, Ordering::Relaxed);
                    }
                    JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::Rejected(_))
                    | JournaledPayloadOutcome::RetryExhausted => {
                        counters.rejected_records.fetch_add(1, Ordering::Relaxed);
                    }
                }
                debug_assert!(!journal.state().is_globally_complete());
                let _ = ReceiverState::AwaitingManifest;
            }
        }
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
    #[error("vNext persistent storage failed: {0}")]
    Storage(String),
    #[error("vNext reconciliation journal failed: {0}")]
    Journal(String),
    #[error("vNext inventory persistence failed: {0}")]
    Inventory(String),
    #[error("vNext record provenance persistence failed: {0}")]
    Provenance(String),
    #[error("vNext outbound outbox failed: {0}")]
    Outbox(String),
    #[error("vNext identity file is corrupt: {}", .0.display())]
    IdentityCorrupt(PathBuf),
    #[error("vNext identity signer returned an invalid Ed25519 public key")]
    IdentitySignerInvalid,
    #[error("vNext identity signer failed proof of possession")]
    IdentitySignerProofInvalid,
    #[error("vNext identity signer is unavailable: {0}")]
    IdentitySignerUnavailable(String),
    #[error("vNext filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use ku_core::foundation::{
        decode_actor_root_delegation, decode_feed_inception, ActorDelegation, ActorRevocation,
        ActorRootDelegation, ConceptCcid, DeviceId, DisclosureClass, FeedInception,
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
        let mut outbound = left.connect(right.local_addr()).await.unwrap();
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
        assert!(!right.status().claims_network_completion);

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
        assert_eq!(stored.attempts, 1);

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
}
