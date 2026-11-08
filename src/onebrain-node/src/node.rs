//! OneBrain Node — ties together all subsystems.
//!
//! The `OneBrainNode` is the top-level runtime struct that owns the
//! Mediator, AI backend, concept dictionary, persistent storage,
//! anti-gaming guard, and peer networking.

#[cfg(feature = "legacy")]
use crate::anti_gaming_guard::AntiGamingGuard;
use crate::base_runtime::{
    BaseManagementGrant, BaseManagementScope, BaseRuntime, BaseRuntimeConfig, BaseServices,
};
use crate::blob_authority::{
    BlobAuthority, OsPendingUploadIdSource, PendingBlobUploadId, PendingOwnedBlobUpload,
    UnavailableValidatedBlobReferenceSource,
};
use crate::canonical_exchange::{
    read_canonical_exchange, write_canonical_exchange, BaseExchangeEntryV1,
};
use crate::concept_registry_runtime::{
    initialize_concept_registry, ConceptRegistryRuntimeState, ConceptRegistryStatus,
};
use crate::config::NodeConfig;
use crate::dataset_generation::DatasetGenerationStore;
use crate::dataset_path::{BaseStorageOwnerId, DatasetPathResolver};
use crate::derived_index::{
    AcceptedRecordScan, DerivedIndexOpenState, RedbAcceptedRecordScan, VNextDerivedIndexManager,
};
use crate::derived_projection::{DerivedProjectionOpenState, RetrieverProjectionService};
use crate::error::NodeError;
use crate::identity_recovery::SignerReprovisionRequirement;
use crate::network::{recv_message, send_message, NetMessage, NodeEvent, PeerInfo};
use crate::peer_manager::PeerManager;
use crate::signer_ports::{SignerPossessionProof, SignerProviderRegistry};
use crate::text::truncate_preview;
#[cfg(feature = "legacy")]
use crate::verifier_service;
#[cfg(feature = "vnext-network-runtime")]
use crate::vnext_network_runtime::OutboundVNextSession;
#[cfg(feature = "vnext-network-runtime")]
use crate::vnext_product_runtime::{
    VNextProductRuntime, VNextProductRuntimeDependencies, VNextProductRuntimeStatus,
    VNextProductServices,
};
#[cfg(feature = "vnext-network-runtime")]
use crate::vnext_runtime_rollout::{VNextRuntimeLane, VNextRuntimeRolloutSnapshot};
use crate::vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};
#[cfg(feature = "vnext-network-runtime")]
use ku_net::vnext_session::SessionIdentitySigner;

use crate::types::*;
use crate::model_host;
use ku_core::blob_store::{BlobCid, BlobMeta, BlobType};
use ku_core::concept_registry::ConceptLookup;
use ku_core::foundation::{ObjectReference, RedbVerifiedBackend, StoredRecordKind};
use ku_core::text_parser::{default_dict, ConceptDict};
use ku_core::KuRuntime;
#[cfg(feature = "legacy")]
use ku_encoder::{AiEncoder, EncoderConfig, EncodingResult};
use ku_kql::blob_storage::{BlobStorage, BlobStorageConfig};
use ku_kql::storage::KuStorage;
use ku_mediator::input::UserInput;
use ku_mediator::mediator::{Mediator, MediatorConfig};
use ku_mediator::retriever::KuRetriever;
use ku_net::vnext_reconciliation::{PayloadSinkOutcome, ValidateThenAcceptSink};
use onebrain_protocol::ReconcileManifestKind;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

// Behavior groups of `impl OneBrainNode`, split out of this module (ARCH-09).
mod ai;
mod authoring;
mod blobs;
mod identity;
mod insights;
mod knowledge;
mod library;
mod portability;
mod settings;
mod social;
mod wallet;

/// Result of a successful encode-and-store operation.
pub struct EncodeStoreResult {
    /// CID of the stored KU (32 bytes, BLAKE3).
    pub cid: [u8; 32],
    /// Number of wire bytes in the KU.
    pub wire_size: usize,
    /// Number of Core DNA instructions.
    pub instruction_count: usize,
    /// Gene type detected by the AI encoder.
    pub gene_type: Option<String>,
    /// Encoding confidence (0.0-1.0).
    pub confidence: f32,
    /// Source text that was encoded.
    pub source_text: String,
    /// Wire bytes of the primary KU (for network broadcast).
    pub wire_bytes: Vec<u8>,
    /// Number of peers the KU was broadcast to.
    pub peers_reached: usize,
}

/// Candidate-integration evidence assembled from the live Node boundary.
///
/// This is deliberately not a qualification receipt: Task 28 is the only
/// authority that may attach production qualification to the Base candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseIntegrationReceipt {
    pub candidate_semantic_digest: [u8; 32],
    pub artifact_tuple_digest: [u8; 32],
    pub canonical_root_before_restart: [u8; 32],
    pub canonical_root_after_restart: [u8; 32],
    pub archive_restore_root: [u8; 32],
    pub registry_release_root: [u8; 32],
    pub default_active_network_lanes: u16,
    pub legacy_write_enabled: bool,
}

/// Shared state accessible from both the REPL and background tasks.
pub struct SharedState {
    /// Persistent KU storage.
    pub storage: KuStorage,
    /// Persistent blob storage.
    pub blob_store: BlobStorage,
    /// Canonical reference oracle and durable pending upload leases.
    pub blob_authority: Arc<BlobAuthority>,
    /// Rebuildable vNext graph/search generation; never a write authority.
    pub derived_index: Arc<VNextDerivedIndexManager>,
    pub derived_index_state: DerivedIndexOpenState,
    /// Rebuildable retriever projection shared with the Mediator.
    pub retriever: Arc<RwLock<KuRetriever>>,
    pub retriever_projection: Arc<RetrieverProjectionService>,
    pub retriever_projection_state: DerivedProjectionOpenState,
    /// Active generation path authority shared by every Base-owned store.
    pub dataset_paths: Arc<dyn DatasetPathResolver>,
    /// Connected peers.
    pub peer_manager: PeerManager,
    /// Node configuration (for data paths, Ollama URL, etc.).
    pub config: NodeConfig,
}

/// The top-level OneBrain node.
pub struct OneBrainNode {
    config: NodeConfig,
    /// Holds the lifetime OS root lease and non-switched activation journal.
    _dataset_generations: Arc<DatasetGenerationStore>,
    dataset_paths: Arc<dyn DatasetPathResolver>,
    /// Sole owner of the product-neutral Base facade. Service clones are weak
    /// and remain usable without holding the node mutex.
    base_runtime: Option<BaseRuntime>,
    mediator: Mediator,
    #[cfg(feature = "legacy")]
    guard: AntiGamingGuard,
    /// Concept dictionary (shared across encoder and mediator).
    dict: ConceptDict,
    /// Shared state (protected by async mutex for background task access).
    pub shared: Arc<Mutex<SharedState>>,
    /// Channel to receive events from background listener.
    pub event_rx: mpsc::Receiver<NodeEvent>,
    /// Channel sender (cloned to background tasks).
    pub event_tx: mpsc::Sender<NodeEvent>,
    /// TCP listener address (set after start_network).
    listener_addr: Option<SocketAddr>,
    /// Node-owned legacy TCP accept loop, fenced by explicit shutdown/drop.
    listener_task: Option<JoinHandle<()>>,
    // ── Tier C in-memory state ──
    /// Search history log.
    search_history: Vec<SearchHistoryEntry>,
    /// Notification preferences.
    notification_prefs: NotificationPrefs,
    /// Saved searches.
    saved_searches: Vec<SavedSearch>,
    collections: Vec<Collection>,
    // ── Legacy session state: process memory only, lost on restart (see docs/STUB_TRACKING.md) ──
    /// Tags: CID hex → Set of tag strings.
    ku_tags: HashMap<String, HashSet<String>>,
    /// Pinned/favorited KU CID hex strings.
    pinned_kus: HashSet<String>,
    /// Followed nodes.
    following: Vec<FollowedNode>,
    /// Active WATCH standing queries.
    watches: Vec<WatchInfo>,
    /// Deprecated KU CID hex strings.
    deprecated_kus: HashSet<String>,
    /// Registered devices in identity group.
    devices: Vec<DeviceInfo>,
    /// Draft storage: draft_id → Draft.
    drafts: HashMap<String, Draft>,
    /// Staked OBT amount (milliOBT).
    staked_amount: u64,
    /// ConceptRegistry for encode_v2 (loaded from concepts.obr).
    /// None if OBR file not found — falls back to encode v1.
    #[allow(dead_code)]
    registry: Option<Box<dyn ConceptLookup>>,
    /// Observable startup policy/result for the external Concept Registry.
    registry_status: ConceptRegistryStatus,
    /// Sole owner of the integrated vNext network/KQL/Public Use/PoMV slice.
    /// It exists only after the feature is compiled, requested, not killed,
    /// supplied with local Vault/Policy dependencies, and successfully bound.
    #[cfg(feature = "vnext-network-runtime")]
    vnext_product_runtime: Option<VNextProductRuntime>,
    /// Caller-owned Vault and allow-listed policy dependencies consumed at
    /// product runtime startup.
    #[cfg(feature = "vnext-network-runtime")]
    vnext_product_dependencies: Option<VNextProductRuntimeDependencies>,
    /// Optional caller-owned node signer. Embedders can back this with an OS
    /// keystore, HSM, or remote signer; private-key bytes never enter OneBrain.
    #[cfg(feature = "vnext-network-runtime")]
    vnext_identity_signer: Option<Arc<dyn SessionIdentitySigner>>,
}

impl OneBrainNode {
    /// Create a new OneBrain node from the given configuration.
    ///
    /// Initializes:
    /// 1. AI backends (chat + encoding + mediator encoding)
    /// 2. The Mediator pipeline
    /// 3. Persistent KU storage (redb)
    /// 4. Vault-derived retriever projection (or typed degraded state)
    /// 5. Anti-gaming guard (rate limiting + quality gates)
    ///
    /// Source prose is never reconstructed from legacy wire bytes. A retriever
    /// becomes healthy only from exact Vault source records.
    pub async fn new(config: NodeConfig) -> Result<Self, NodeError> {
        config
            .vnext
            .validate()
            .map_err(|error| NodeError::Config(error.to_string()))?;
        #[cfg(not(feature = "vnext-network-runtime"))]
        if config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return Err(NodeError::Config(
                "obp_rp is active in configuration, but this binary was built without the vnext-network-runtime feature"
                    .to_string(),
            ));
        }

        // Resolve the registry before initializing AI, storage, or network side
        // effects. Required mode therefore fails fast and predictably.
        let loaded_registry = initialize_concept_registry(&config)?;
        match loaded_registry.status.state {
            ConceptRegistryRuntimeState::Loaded => eprintln!(
                "  ConceptRegistry loaded from {} ({} concepts, {} labels)",
                loaded_registry.status.path.display(),
                loaded_registry.status.concept_count.unwrap_or_default(),
                loaded_registry.status.label_count.unwrap_or_default()
            ),
            ConceptRegistryRuntimeState::FallbackV1 => eprintln!(
                "  ConceptRegistry unavailable at {}; using encoder v1: {}",
                loaded_registry.status.path.display(),
                loaded_registry
                    .status
                    .error
                    .as_deref()
                    .unwrap_or("unknown error")
            ),
            ConceptRegistryRuntimeState::Disabled => {
                eprintln!("  ConceptRegistry disabled explicitly; using encoder v1")
            }
        }
        let registry = loaded_registry.registry;
        let registry_status = loaded_registry.status;

        // Create chat backend (ARCH-09: Ollama host adapter or explicit unavailable)
        let chat_backend = model_host::model_backend(&config.ollama_url, &config.model, 120)?;

        // Create encoder backend for mediator
        let mediator_encoder_backend =
            model_host::model_backend(&config.ollama_url, &config.model, 120)?;

        // Create concept dictionary
        let dict: ConceptDict = default_dict();

        // Open persistent storage
        let storage = KuStorage::open_base_read_only(&config.storage_path())
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let dataset_generations = Arc::new(
            DatasetGenerationStore::open_exclusive(&config.base_dataset_root())
                .map_err(|error| NodeError::Storage(error.to_string()))?,
        );
        let dataset_paths: Arc<dyn DatasetPathResolver> = dataset_generations.clone();
        let blob_authority = Arc::new(BlobAuthority::new(
            dataset_paths.clone(),
            Arc::new(OsPendingUploadIdSource),
            Arc::new(UnavailableValidatedBlobReferenceSource),
        ));
        blob_authority
            .pending()
            .reconcile_generation()
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let blob_store = BlobStorage::open_with_config(
            &dataset_paths
                .owner_path(BaseStorageOwnerId::BLOB)
                .map_err(|error| NodeError::Storage(error.to_string()))?
                .join("ku.blob.redb"),
            BlobStorageConfig {
                total_quota_bytes: 10 * 1024 * 1024 * 1024,
                free_space_reserve_bytes: 64 * 1024 * 1024,
            },
            blob_authority.oracle(),
        )
        .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        blob_store
            .migrate_blob_metadata_v2()
            .map_err(|error| NodeError::Storage(error.to_string()))?;

        let derived_index = Arc::new(
            VNextDerivedIndexManager::new(
                dataset_paths
                    .owner_path(BaseStorageOwnerId::DERIVED_INDEX)
                    .map_err(|error| NodeError::Storage(error.to_string()))?,
            )
            .map_err(|error| NodeError::Storage(error.to_string()))?,
        );
        let canonical_scan = RedbAcceptedRecordScan::new(
            dataset_paths
                .owner_path(BaseStorageOwnerId::CANONICAL)
                .map_err(|error| NodeError::Storage(error.to_string()))?
                .join("vnext_verified.redb"),
        );
        let (derived_index_state, derived_report) = derived_index.open_or_rebuild(&canonical_scan);
        let accepted_vnext_root = derived_report
            .as_ref()
            .map(|report| report.source_root)
            .unwrap_or([0; 32]);
        // Task 12 will inject the platform Vault/key provider. Until that
        // source snapshot is available, never publish an empty projection as
        // healthy; canonical reads remain available in typed degraded mode.
        let (retriever_projection, retriever_projection_state) =
            RetrieverProjectionService::unavailable(
                dataset_paths
                    .owner_path(BaseStorageOwnerId::RETRIEVER_PROJECTION)
                    .map_err(|error| NodeError::Storage(error.to_string()))?,
                accepted_vnext_root,
                "VAULT_SOURCE_SNAPSHOT_UNAVAILABLE",
            );
        let retriever_projection = Arc::new(retriever_projection);
        let retriever = retriever_projection.retriever();

        let mediator = Mediator::new_with_retriever(
            chat_backend,
            mediator_encoder_backend,
            dict.clone(),
            MediatorConfig::default(),
            retriever.clone(),
        );

        // Report startup KU count
        let ku_count = storage
            .count()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        if ku_count > 0 {
            eprintln!("  ✓ Storage contains {} KU(s)", ku_count);
            eprintln!(
                "  ✓ Retriever projection state: {:?} ({} entries)",
                retriever_projection_state,
                retriever
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .index_size()
            );
        }

        // Create anti-gaming guard
        #[cfg(feature = "legacy")]
        let guard = AntiGamingGuard::new();

        // Create shared state
        let shared = Arc::new(Mutex::new(SharedState {
            storage,
            blob_store,
            blob_authority,
            derived_index,
            derived_index_state,
            retriever,
            retriever_projection,
            retriever_projection_state,
            dataset_paths: dataset_paths.clone(),
            peer_manager: PeerManager::new(),
            config: config.clone(),
        }));

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel::<NodeEvent>(256);

        // Capture device name before config ownership transfer
        let device_name = config.name.clone();

        Ok(Self {
            config,
            _dataset_generations: dataset_generations,
            dataset_paths: dataset_paths.clone(),
            base_runtime: None,
            mediator,
            #[cfg(feature = "legacy")]
            guard,
            dict,
            shared,
            event_rx,
            event_tx,
            listener_addr: None,
            listener_task: None,
            search_history: Vec::new(),
            notification_prefs: NotificationPrefs::default(),
            saved_searches: Vec::new(),
            collections: Vec::new(),
            ku_tags: HashMap::new(),
            pinned_kus: HashSet::new(),
            following: Vec::new(),
            watches: Vec::new(),
            deprecated_kus: HashSet::new(),
            drafts: HashMap::new(),
            staked_amount: 0,
            registry,
            registry_status,
            #[cfg(feature = "vnext-network-runtime")]
            vnext_product_runtime: None,
            #[cfg(feature = "vnext-network-runtime")]
            vnext_product_dependencies: None,
            #[cfg(feature = "vnext-network-runtime")]
            vnext_identity_signer: None,
            devices: vec![DeviceInfo {
                device_id: format!("dev-{:08x}", rand_u32()),
                name: device_name,
                device_type: "CLI".to_string(),
                last_seen: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                ku_count: ku_count as u64,
                sync_status: "up-to-date".to_string(),
            }],
        })
    }

    /// Inject a caller-owned signer before starting the network. A generation
    /// restore may also supply this handle through typed identity recovery;
    /// the Base dataset path never creates a compatibility key file.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn set_vnext_identity_signer(&mut self, signer: Arc<dyn SessionIdentitySigner>) {
        self.vnext_identity_signer = Some(signer);
    }

    /// Inject caller-owned encrypted Vault and allow-listed policy handles
    /// before starting the integrated vNext product runtime.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn set_vnext_product_dependencies(
        &mut self,
        dependencies: VNextProductRuntimeDependencies,
    ) -> Result<(), NodeError> {
        if self.vnext_product_runtime.is_some() {
            return Err(NodeError::Config(
                "vNext product dependencies cannot change after runtime startup".into(),
            ));
        }
        self.vnext_product_dependencies = Some(dependencies);
        Ok(())
    }

    /// Whether [`Self::start_network`] has anything to start: the explicitly
    /// opted-in legacy TCP listener or the configured vNext OBP-RP runtime.
    pub fn network_listener_requested(&self) -> bool {
        if self
            .config
            .legacy_tcp_listener
            .bind_addr(self.config.port)
            .is_some()
        {
            return true;
        }
        #[cfg(feature = "vnext-network-runtime")]
        if self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return true;
        }
        false
    }

    /// Start the configured network listeners.
    ///
    /// The authenticated vNext runtime starts when OBP-RP is active. The
    /// unauthenticated legacy TCP/JSON listener binds only when
    /// [`NodeConfig::legacy_tcp_listener`] opts in, on loopback or on every
    /// interface. Returns the legacy listener address when it is bound,
    /// otherwise the vNext listener address. Fails without side effects when
    /// neither listener is requested.
    pub async fn start_network(&mut self) -> Result<SocketAddr, NodeError> {
        if self.listener_task.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        #[cfg(feature = "vnext-network-runtime")]
        if self.vnext_product_runtime.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        if !self.network_listener_requested() {
            return Err(NodeError::Config(NO_NETWORK_LISTENER_REQUESTED.into()));
        }
        let legacy_bind = self.config.legacy_tcp_listener.bind_addr(self.config.port);
        #[cfg(feature = "vnext-outbound-first")]
        if let Some(dependencies) = self.vnext_product_dependencies.as_ref() {
            dependencies
                .validate_outbound_first(&self.config.vnext)
                .map_err(|error| NodeError::Config(error.to_string()))?;
        }
        // The authenticated vNext carrier keeps its all-interface bind.
        #[cfg(feature = "vnext-network-runtime")]
        let mut pending_vnext = self
            .start_vnext_product_runtime(([0, 0, 0, 0], self.config.port).into())
            .await?;
        let Some(bind_addr) = legacy_bind else {
            #[cfg(feature = "vnext-network-runtime")]
            return self.adopt_vnext_only_runtime(pending_vnext.take());
            #[cfg(not(feature = "vnext-network-runtime"))]
            return Err(NodeError::Config(NO_NETWORK_LISTENER_REQUESTED.into()));
        };
        let listener = match TcpListener::bind(bind_addr).await {
            Ok(listener) => listener,
            Err(error) => {
                #[cfg(feature = "vnext-network-runtime")]
                if let Some(runtime) = pending_vnext.take() {
                    if let Err(rollback_error) = runtime.rollback_startup().await {
                        return Err(NodeError::Network(format!(
                            "Failed to bind TCP on {bind_addr}: {error}; vNext startup rollback failed: {rollback_error}"
                        )));
                    }
                }
                return Err(NodeError::Network(format!(
                    "Failed to bind TCP on {}: {}",
                    bind_addr, error
                )));
            }
        };

        let local_addr = listener
            .local_addr()
            .map_err(|e| NodeError::Network(format!("Failed to get local addr: {}", e)))?;
        self.listener_addr = Some(local_addr);
        #[cfg(feature = "vnext-network-runtime")]
        {
            self.vnext_product_runtime = pending_vnext;
        }

        // Spawn background listener task
        let shared = Arc::clone(&self.shared);
        let event_tx = self.event_tx.clone();
        self.listener_task = Some(tokio::spawn(async move {
            listener_loop(listener, shared, event_tx).await;
        }));

        Ok(local_addr)
    }

    #[cfg(feature = "vnext-network-runtime")]
    fn adopt_vnext_only_runtime(
        &mut self,
        runtime: Option<VNextProductRuntime>,
    ) -> Result<SocketAddr, NodeError> {
        let runtime =
            runtime.ok_or_else(|| NodeError::Config(NO_NETWORK_LISTENER_REQUESTED.into()))?;
        let addr = runtime.services().local_addr();
        self.vnext_product_runtime = Some(runtime);
        Ok(addr)
    }

    #[cfg(feature = "vnext-network-runtime")]
    async fn start_vnext_product_runtime(
        &mut self,
        bind_addr: SocketAddr,
    ) -> Result<Option<VNextProductRuntime>, NodeError> {
        if !self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return Ok(None);
        }
        let dependencies = self.vnext_product_dependencies.take().ok_or_else(|| {
            NodeError::Config(
                "active OBP-RP requires caller-owned vNext Vault and Policy dependencies".into(),
            )
        })?;
        let identity_signer = match self.vnext_identity_signer.clone() {
            Some(signer) => Some(signer),
            None => self
                ._dataset_generations
                .session_identity_signer()
                .map_err(|error| NodeError::Storage(error.to_string()))?,
        };
        VNextProductRuntime::start_in_dataset(
            self.dataset_paths.as_ref(),
            bind_addr,
            &self.config.vnext,
            dependencies,
            identity_signer,
        )
        .await
        .map(Some)
        .map_err(|error| {
            NodeError::Network(format!(
                "Failed to start integrated vNext product runtime: {error}"
            ))
        })
    }

    /// Start only the node-owned vNext runtime. Legacy TCP discovery and
    /// inbound peer service remain stopped; existing legacy state is untouched.
    #[cfg(feature = "vnext-network-runtime")]
    pub async fn start_vnext_network_only(&mut self) -> Result<SocketAddr, NodeError> {
        if self.listener_task.is_some() || self.vnext_product_runtime.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        if !self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            return Err(NodeError::Config(
                "vNext product runtime is not requested".into(),
            ));
        }
        #[cfg(feature = "vnext-outbound-first")]
        if let Some(dependencies) = self.vnext_product_dependencies.as_ref() {
            dependencies
                .validate_outbound_first(&self.config.vnext)
                .map_err(|error| NodeError::Config(error.to_string()))?;
        }
        let bind_addr: SocketAddr = ([0, 0, 0, 0], self.config.port).into();
        let runtime = self
            .start_vnext_product_runtime(bind_addr)
            .await?
            .ok_or_else(|| NodeError::Config("vNext product runtime is not requested".into()))?;
        let addr = runtime.services().local_addr();
        self.vnext_product_runtime = Some(runtime);
        Ok(addr)
    }

    /// Fence new network operations and stop every node-owned listener and
    /// integrated vNext owner in deterministic order.
    pub async fn shutdown_network(&mut self) {
        if let Some(mut task) = self.listener_task.take() {
            task.abort();
            let _ = (&mut task).await;
        }
        self.listener_addr = None;
        #[cfg(feature = "vnext-network-runtime")]
        if let Some(mut runtime) = self.vnext_product_runtime.take() {
            runtime.shutdown().await;
        }
    }

    /// Legacy TCP peer handshake; never vNext seed or relay authority.
    pub async fn connect_to_seed(&self, addr: SocketAddr) -> Result<(), NodeError> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| NodeError::Network(format!("Failed to connect to {}: {}", addr, e)))?;

        // Send our hello
        let ku_count = {
            let state = self.shared.lock().await;
            state.storage.count().unwrap_or(0) as u64
        };
        let hello = NetMessage::PeerHello {
            name: self.config.name.clone(),
            port: self.config.port,
            ku_count,
        };
        send_message(&mut stream, &hello)
            .await
            .map_err(|e| NodeError::Network(format!("Failed to send hello: {}", e)))?;

        // Receive peer's hello
        match recv_message(&mut stream).await {
            Ok(NetMessage::PeerHello {
                name,
                port: _,
                ku_count: peer_ku_count,
            }) => {
                let peer_info = PeerInfo {
                    name: name.clone(),
                    addr,
                    ku_count: peer_ku_count,
                };
                let mut state = self.shared.lock().await;
                state.peer_manager.add_peer(peer_info);
                eprintln!(
                    "  ✓ Connected to peer '{}' at {} ({} KUs)",
                    name, addr, peer_ku_count
                );
            }
            Ok(other) => {
                eprintln!("  ⚠ Unexpected message from seed {}: {:?}", addr, other);
            }
            Err(e) => {
                eprintln!("  ⚠ No hello response from {}: {}", addr, e);
                // Still add as peer (one-way handshake is OK for demo)
                let peer_info = PeerInfo {
                    name: format!("peer@{}", addr),
                    addr,
                    ku_count: 0,
                };
                let mut state = self.shared.lock().await;
                state.peer_manager.add_peer(peer_info);
            }
        }

        Ok(())
    }

    /// Broadcast a KU to all connected peers.
    ///
    /// Sends KuPush to all known peers (fire-and-forget, non-blocking).
    pub async fn broadcast_ku(&self, cid_hex: &str, wire_bytes: &[u8], source_text: &str) -> usize {
        let addrs = {
            let state = self.shared.lock().await;
            state.peer_manager.known_addrs()
        };

        if addrs.is_empty() {
            return 0;
        }

        let peer_count = addrs.len();
        let msg = NetMessage::KuPush {
            cid_hex: cid_hex.to_string(),
            wire_bytes: wire_bytes.to_vec(),
            source_text: source_text.to_string(),
        };

        for addr in &addrs {
            let msg = msg.clone();
            let addr = *addr;
            tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(addr).await {
                    let _ = send_message(&mut stream, &msg).await;
                }
            });
        }

        peer_count
    }

    /// Request verification from all connected peers.
    ///
    /// Sends VerifyRequest and collects responses asynchronously.
    pub async fn request_verification(&self, cid_hex: &str, source_text: &str) {
        let addrs = {
            let state = self.shared.lock().await;
            state.peer_manager.known_addrs()
        };

        if addrs.is_empty() {
            return;
        }

        let event_tx = self.event_tx.clone();

        for addr in addrs {
            let msg = NetMessage::VerifyRequest {
                cid_hex: cid_hex.to_string(),
                source_text: source_text.to_string(),
            };
            let event_tx = event_tx.clone();
            let cid_hex = cid_hex.to_string();
            tokio::spawn(async move {
                if let Ok(mut stream) = TcpStream::connect(addr).await {
                    if send_message(&mut stream, &msg).await.is_ok() {
                        // Wait for response (with timeout)
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(60),
                            recv_message(&mut stream),
                        )
                        .await
                        {
                            Ok(Ok(NetMessage::VerifyResponse {
                                agreement_score,
                                verified,
                                ..
                            })) => {
                                let _ = event_tx
                                    .send(NodeEvent::VerifyResult {
                                        cid_hex,
                                        agreement_score,
                                        verified,
                                        from: format!("{}", addr),
                                    })
                                    .await;
                            }
                            _ => {
                                let _ = event_tx
                                    .send(NodeEvent::Notification(format!(
                                        "  ⚠ Verification timeout from {}",
                                        addr
                                    )))
                                    .await;
                            }
                        }
                    }
                }
            });
        }
    }

    /// Encode text into a KU and store it persistently.
    ///
    /// Pipeline:
    /// 1. Check rate limit (anti-gaming)
    /// 2. Create a fresh AiEncoder + OllamaBackend
    /// 3. Encode via AI → EncodingResult with wire_bytes
    /// 4. For the primary KU:
    ///    a. Decode wire bytes → KuRuntime
    ///    b. Check quality gates (anti-gaming)
    ///    c. Store in redb via KuStorage
    ///    d. Index source text in retriever
    ///    e. Record creation in rate tracker
    /// 5. Broadcast to peers + request verification
    /// 6. Return CID + stats
    pub async fn encode_and_store(&mut self, text: &str) -> Result<EncodeStoreResult, NodeError> {
        self.encode_and_store_with_progress(text, None).await
    }

    /// Encode with optional broadcast progress sender.
    /// When `progress_tx` is Some, progress events are sent directly to the
    /// broadcast channel (bypassing the node's mpsc event channel and avoiding
    /// the lock deadlock issue with WS handlers).
    pub async fn encode_and_store_with_progress(
        &mut self,
        text: &str,
        progress_tx: Option<&tokio::sync::broadcast::Sender<String>>,
    ) -> Result<EncodeStoreResult, NodeError> {
        #[cfg(not(feature = "legacy"))]
        {
            let _ = (text, progress_tx);
            return Err(NodeError::InvalidArgument(
                "Legacy Core DNA encoding (/api/encode) is disabled in the default build; use the canonical vNext KU workflow (/api/vnext/ku or `onebrain ku`) or build with `--features legacy`."
                    .into(),
            ));
        }

        #[cfg(feature = "legacy")]
        {
            let total_steps: u8 = 6;

            // Helper to send progress
            let send_progress = |step: u8, msg: String| {
                if let Some(tx) = progress_tx {
                    let event = serde_json::json!({
                        "event_type": "encode_progress",
                        "timestamp": std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default().as_secs(),
                        "data": { "step": step, "total_steps": total_steps, "message": msg }
                    });
                    let _ = tx.send(event.to_string());
                }
            };

            // 1. Rate limit check
            send_progress(1, "Rate limit check...".into());
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 1,
                    total_steps,
                    message: "Rate limit check...".into(),
                })
                .await;
            self.guard.check_rate_limit().map_err(NodeError::Pipeline)?;

            // 2. Create a fresh encoder backend (OllamaBackend doesn't impl Clone)
            send_progress(
                2,
                format!("Creating AI encoder (model: {})...", self.config.model),
            );
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 2,
                    total_steps,
                    message: format!("Creating AI encoder (model: {})...", self.config.model),
                })
                .await;
            let encoder_backend =
                model_host::model_backend(&self.config.ollama_url, &self.config.model, 300)?;

            let encoder = AiEncoder::new(
                encoder_backend,
                self.dict.clone(),
                EncoderConfig::default(),
            );

            // 3. Encode via AI (this is the slow step)
            send_progress(
                3,
                "AI generating tool calls (this may take a while)...".into(),
            );
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 3,
                    total_steps,
                    message: "AI generating tool calls (this may take a while)...".into(),
                })
                .await;

            // Smart dispatch: v2 if ConceptRegistry available, v1 fallback
            let encoding_result: EncodingResult = if let Some(ref reg) = self.registry {
                encoder
                    .encode_v2(text, reg.as_ref())
                    .await
                    .map_err(NodeError::Encoder)?
            } else {
                encoder.encode(text).await.map_err(NodeError::Encoder)?
            };

            if encoding_result.wire_bytes.is_empty() {
                return Err(NodeError::Pipeline("Encoding produced no KUs".into()));
            }

            // 4. Process the first (primary) KU
            send_progress(
                4,
                format!(
                    "Processing KU ({} bytes wire data)...",
                    encoding_result.wire_bytes[0].len()
                ),
            );
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 4,
                    total_steps,
                    message: format!(
                        "Processing KU ({} bytes wire data)...",
                        encoding_result.wire_bytes[0].len()
                    ),
                })
                .await;
            let wire_bytes = &encoding_result.wire_bytes[0];

            // 4a. Decode wire bytes → KuRuntime
            let ku = KuRuntime::from_wire(wire_bytes.clone())
                .map_err(|e| NodeError::Pipeline(format!("KuRuntime decode failed: {}", e)))?;

            let instruction_count = ku.dna.instructions.len();

            // 4b. Quality gate check
            self.guard
                .check_quality(wire_bytes, instruction_count)
                .map_err(NodeError::Pipeline)?;

            // 4c-4e. Store, index, record (using shared state)
            send_progress(5, "Storing KU and indexing...".into());
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 5,
                    total_steps,
                    message: "Storing KU and indexing...".into(),
                })
                .await;
            let cid;
            let cid_hex;
            {
                let state = self.shared.lock().await;

                // 4c. Store in redb
                cid = state
                    .storage
                    .put(&ku)
                    .map_err(|e| NodeError::Storage(format!("{}", e)))?;

                // Legacy KU persistence is fenced in Base mode. It never grants
                // retriever authority; only a proven Vault source binding may do so.
                cid_hex = hex_cid(&cid);
            }

            // 4e. Record creation in rate tracker
            self.guard.record_creation();

            // 5. Also process any additional KUs (if encoding produced multiple)
            {
                let state = self.shared.lock().await;
                for extra_bytes in encoding_result.wire_bytes.iter().skip(1) {
                    if let Ok(extra_ku) = KuRuntime::from_wire(extra_bytes.clone()) {
                        let extra_instr = extra_ku.dna.instructions.len();
                        if self.guard.check_quality(extra_bytes, extra_instr).is_ok() {
                            let _ = state.storage.put(&extra_ku);
                        }
                    }
                }
            }

            // 6. Broadcast to peers + request verification
            send_progress(6, "Broadcasting to peers...".into());
            let _ = self
                .event_tx
                .send(NodeEvent::EncodeProgress {
                    step: 6,
                    total_steps,
                    message: "Broadcasting to peers...".into(),
                })
                .await;
            let peers_reached = self.broadcast_ku(&cid_hex, wire_bytes, text).await;
            self.request_verification(&cid_hex, text).await;

            Ok(EncodeStoreResult {
                cid,
                wire_size: wire_bytes.len(),
                instruction_count,
                gene_type: encoding_result.gene_type,
                confidence: encoding_result.confidence,
                source_text: text.to_string(),
                wire_bytes: wire_bytes.clone(),
                peers_reached,
            })
        }
    }

    /// Get the number of KUs in persistent storage.
    pub fn ku_count(&self) -> Result<usize, NodeError> {
        // Try to get count without blocking; fallback to 0 if locked
        match self.shared.try_lock() {
            Ok(state) => state
                .storage
                .count()
                .map_err(|e| NodeError::Storage(format!("{}", e))),
            Err(_) => Ok(0), // Can't lock right now, return 0
        }
    }

    /// Get the node display name.
    pub fn node_name(&self) -> &str {
        &self.config.name
    }

    /// Get the node configuration.
    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    /// Return the effective Concept Registry startup policy and load result.
    pub fn concept_registry_status(&self) -> &ConceptRegistryStatus {
        &self.registry_status
    }

    /// Close the Task 25 integration receipt from independently measured
    /// archive/restart roots and live Node-owned tuple, Registry, rollout, and
    /// legacy-fence state. Root disagreement is rejected instead of being
    /// recorded as apparently successful evidence.
    pub fn base_integration_receipt(
        &self,
        canonical_root_before_restart: [u8; 32],
        canonical_root_after_restart: [u8; 32],
        archive_restore_root: [u8; 32],
    ) -> Result<BaseIntegrationReceipt, NodeError> {
        if canonical_root_before_restart != canonical_root_after_restart
            || canonical_root_before_restart != archive_restore_root
        {
            return Err(NodeError::Storage(
                "Base integration canonical/archive roots disagree".into(),
            ));
        }
        let status = self
            .base_services()
            .ok_or_else(|| NodeError::Config("Base runtime is not installed".into()))?
            .snapshot()
            .map_err(|error| NodeError::Config(error.to_string()))?;
        let registry_release_root = self
            .registry_status
            .release_aggregate_root
            .as_deref()
            .ok_or_else(|| {
                NodeError::Config("signed Concept Registry release is not active".into())
            })
            .and_then(decode_integration_root)?;
        let features = self.vnext_status().features;
        let default_active_network_lanes = [
            features.obp_rp,
            features.distributed_kql_one_hop,
            features.public_use_evidence_publish,
            features.distributed_pomv_view,
        ]
        .into_iter()
        .filter(|active| *active)
        .count() as u16;

        Ok(BaseIntegrationReceipt {
            candidate_semantic_digest: status.version.candidate_semantic_digest.0,
            artifact_tuple_digest: status.version.artifact_tuple_digest.0,
            canonical_root_before_restart,
            canonical_root_after_restart,
            archive_restore_root,
            registry_release_root,
            default_active_network_lanes,
            // KuStorage is opened through the read-only compatibility port;
            // no feature combination restores legacy write authority.
            legacy_write_enabled: false,
        })
    }

    /// Build a scope-aware, display-only vNext status projection.
    pub fn vnext_status(&self) -> crate::vnext_status::VNextStatusSnapshot {
        #[cfg(feature = "vnext-network-runtime")]
        let mut effective_vnext = self.config.vnext.clone();
        #[cfg(feature = "vnext-network-runtime")]
        let runtime = self.vnext_product_runtime.as_ref().map(|runtime| {
            let services = runtime.services();
            if let Ok(status) = services.status() {
                effective_vnext.kill_switches.obp_rp =
                    !status.rollout.lane(VNextRuntimeLane::Network).enabled;
                effective_vnext.kill_switches.distributed_kql_one_hop =
                    !status.lanes.distributed_kql_one_hop;
                effective_vnext.kill_switches.public_use_evidence_publish =
                    !status.lanes.public_use_evidence_publish;
                effective_vnext.kill_switches.distributed_pomv_view =
                    !status.lanes.distributed_pomv_view;
            }
            let registry_state = match self.registry_status.state {
                crate::ConceptRegistryRuntimeState::Loaded => {
                    crate::vnext_observability::VNextRegistryTelemetryState::Loaded
                }
                crate::ConceptRegistryRuntimeState::FallbackV1 => {
                    crate::vnext_observability::VNextRegistryTelemetryState::FallbackV1
                }
                crate::ConceptRegistryRuntimeState::Disabled => {
                    crate::vnext_observability::VNextRegistryTelemetryState::Disabled
                }
            };
            services.observe_registry_state(registry_state);
            let status = services.network_status();
            crate::vnext_status::NetworkRuntimeObservation {
                listen_addr: status.listen_addr.to_string(),
                authenticated_sessions: status.authenticated_sessions,
                active_sessions: status.active_sessions,
                accepted_records: status.accepted_records,
                deferred_records: status.deferred_records,
                rejected_records: status.rejected_records,
                observability: status.observability,
            }
        });
        #[cfg(not(feature = "vnext-network-runtime"))]
        let runtime = None;
        #[cfg(not(feature = "vnext-network-runtime"))]
        let effective_vnext = self.config.vnext.clone();
        let mut snapshot = crate::vnext_status::VNextStatusSnapshot::local_runtime_with_network(
            self.ku_count().unwrap_or(0),
            self.peer_count(),
            &effective_vnext,
            true,
            runtime,
        );
        snapshot.base_runtime = self
            .base_services()
            .and_then(|services| services.snapshot().ok())
            .map(crate::vnext_status::BaseRuntimeStatusView::from_base);
        snapshot
    }

    /// Address of the real UDP/QUIC OBP-RP listener, if it is running.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn vnext_listener_addr(&self) -> Option<SocketAddr> {
        self.vnext_product_runtime
            .as_ref()
            .map(|runtime| runtime.services().local_addr())
    }

    /// Typed product façade for API/CLI/Desktop integration. Raw subsystem
    /// runtime references are deliberately not exposed.
    #[cfg(feature = "vnext-outbound-first")]
    pub fn obp_host(&self) -> Option<crate::vnext_product_runtime::obp::Host> {
        self.vnext_product_runtime.as_ref().map(|r| r.obp_host())
    }

    #[cfg(feature = "vnext-network-runtime")]
    pub fn vnext_product_services(&self) -> Option<VNextProductServices> {
        self.vnext_product_runtime
            .as_ref()
            .map(VNextProductRuntime::services)
    }

    #[cfg(feature = "vnext-network-runtime")]
    pub fn vnext_product_runtime_status(
        &self,
    ) -> Result<Option<VNextProductRuntimeStatus>, NodeError> {
        self.vnext_product_services()
            .map(|services| {
                services
                    .status()
                    .map_err(|error| NodeError::Network(error.to_string()))
            })
            .transpose()
    }

    /// Establish a transcript-authenticated outbound vNext session.
    #[cfg(feature = "vnext-network-runtime")]
    pub async fn connect_vnext_peer(
        &self,
        addr: SocketAddr,
    ) -> Result<OutboundVNextSession, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .connect_peer(addr)
            .await
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Persist an emergency kill for one vNext runtime lane. Legacy TCP and
    /// local/offline knowledge paths remain owned and available independently.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn kill_vnext_runtime_lane(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeRolloutSnapshot, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .kill_runtime_lane(lane)
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Explicitly re-enable one configured lane on a later generation.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn reenable_vnext_runtime_lane(
        &self,
        lane: VNextRuntimeLane,
    ) -> Result<VNextRuntimeRolloutSnapshot, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .reenable_runtime_lane(lane)
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Atomically disable every vNext network/product lane without deleting
    /// raw, journal, outbox, quarantine, wallet, or OBT state.
    #[cfg(feature = "vnext-network-runtime")]
    pub fn rollback_vnext_runtime(&self) -> Result<VNextRuntimeRolloutSnapshot, NodeError> {
        self.vnext_product_services()
            .ok_or_else(|| NodeError::Network("authenticated OBP-RP runtime is not active".into()))?
            .rollback_runtime()
            .map_err(|error| NodeError::Network(error.to_string()))
    }

    /// Get the listener address (if network is started).
    pub fn listener_addr(&self) -> Option<SocketAddr> {
        self.listener_addr
    }

    /// Drain pending events (non-blocking).
    pub fn drain_events(&mut self) -> Vec<NodeEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Get peer count from shared state (non-blocking).
    pub fn peer_count(&self) -> usize {
        match self.shared.try_lock() {
            Ok(state) => state.peer_manager.peer_count(),
            Err(_) => 0,
        }
    }

    /// Get peer list snapshot (non-blocking).
    pub fn peer_list_snapshot(&self) -> Vec<PeerInfo> {
        match self.shared.try_lock() {
            Ok(state) => state.peer_manager.peer_list().to_vec(),
            Err(_) => Vec::new(),
        }
    }
}

impl Drop for OneBrainNode {
    fn drop(&mut self) {
        if let Some(task) = self.listener_task.take() {
            task.abort();
        }
    }
}

/// Generate a pseudo-random u32 (for stub watch IDs).
fn rand_u32() -> u32 {
    use std::time::SystemTime;
    let t = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    (t.as_nanos() & 0xFFFF_FFFF) as u32
}

// ═══════════════════════════════════════════════════════════════════════════
// Background Listener
// ═══════════════════════════════════════════════════════════════════════════

/// Returned by [`OneBrainNode::start_network`] when nothing is configured.
const NO_NETWORK_LISTENER_REQUESTED: &str = "no network listener requested: the unauthenticated legacy TCP listener is disabled (set legacy_tcp_listener to loopback or all_interfaces to opt in) and the vNext OBP-RP runtime is not active";

/// Concurrent connections served by the opt-in legacy listener; excess
/// connections are closed immediately instead of spawning more tasks.
const LEGACY_MAX_CONNECTIONS: usize = 64;

/// Deadline for a legacy peer to deliver its single request message.
const LEGACY_FIRST_MESSAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Concurrent remote `VerifyRequest`s allowed to drive local Ollama calls.
const LEGACY_MAX_CONCURRENT_VERIFICATIONS: usize = 2;

/// Background TCP listener loop.
///
/// Accepts incoming connections and processes messages:
/// - PeerHello → register peer, send our hello back
/// - KuPush → store KU locally, index in retriever
/// - VerifyRequest → re-encode and send VerifyResponse
///
/// The protocol is unauthenticated, so the loop only runs after an explicit
/// [`NodeConfig::legacy_tcp_listener`] opt-in and bounds its own work.
async fn listener_loop(
    listener: TcpListener,
    shared: Arc<Mutex<SharedState>>,
    event_tx: mpsc::Sender<NodeEvent>,
) {
    let connections = Arc::new(tokio::sync::Semaphore::new(LEGACY_MAX_CONNECTIONS));
    let verifications = Arc::new(tokio::sync::Semaphore::new(
        LEGACY_MAX_CONCURRENT_VERIFICATIONS,
    ));
    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let Ok(permit) = Arc::clone(&connections).try_acquire_owned() else {
                    // Over capacity: close without reading or spawning.
                    drop(stream);
                    continue;
                };
                let shared = Arc::clone(&shared);
                let event_tx = event_tx.clone();
                let verifications = Arc::clone(&verifications);
                tokio::spawn(async move {
                    let _permit = permit;
                    handle_connection(stream, peer_addr, shared, event_tx, verifications).await;
                });
            }
            Err(e) => {
                eprintln!("  ⚠ Listener accept error: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Handle a single incoming TCP connection.
async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    shared: Arc<Mutex<SharedState>>,
    event_tx: mpsc::Sender<NodeEvent>,
    verifications: Arc<tokio::sync::Semaphore>,
) {
    // Read the incoming message
    let msg =
        match tokio::time::timeout(LEGACY_FIRST_MESSAGE_TIMEOUT, recv_message(&mut stream)).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => {
                // Connection closed or error — silently ignore
                if e.kind() != std::io::ErrorKind::UnexpectedEof {
                    eprintln!("  ⚠ Read error from {}: {}", peer_addr, e);
                }
                return;
            }
            Err(_) => return,
        };

    match msg {
        NetMessage::PeerHello {
            name,
            port: _,
            ku_count,
        } => {
            let peer_info = PeerInfo {
                name: name.clone(),
                addr: peer_addr,
                ku_count,
            };

            // Send our hello back
            let (our_name, our_port, our_ku_count) = {
                let state = shared.lock().await;
                let count = state.storage.count().unwrap_or(0) as u64;
                (state.config.name.clone(), state.config.port, count)
            };
            let our_hello = NetMessage::PeerHello {
                name: our_name,
                port: our_port,
                ku_count: our_ku_count,
            };
            let _ = send_message(&mut stream, &our_hello).await;

            // Register peer
            {
                let mut state = shared.lock().await;
                state.peer_manager.add_peer(peer_info.clone());
            }

            let _ = event_tx.send(NodeEvent::PeerConnected(peer_info)).await;
        }

        NetMessage::KuPush {
            cid_hex: _peer_claimed_cid,
            wire_bytes,
            source_text,
        } => {
            // Decode and store the KU
            match KuRuntime::from_wire(wire_bytes.clone()) {
                Ok(ku) => {
                    let state = shared.lock().await;
                    match state.storage.put(&ku) {
                        Ok(actual_cid) => {
                            let verified_cid_hex = hex_cid(&actual_cid);
                            let _ = event_tx
                                .send(NodeEvent::KuReceived {
                                    cid_hex: verified_cid_hex,
                                    wire_bytes,
                                    source_text,
                                    from: format!("{}", peer_addr),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = event_tx
                                .send(NodeEvent::Notification(format!(
                                    "  ⚠ Failed to store KU from {}: {}",
                                    peer_addr, e
                                )))
                                .await;
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx
                        .send(NodeEvent::Notification(format!(
                            "  ⚠ Failed to decode KU from {}: {}",
                            peer_addr, e
                        )))
                        .await;
                }
            }
        }

        NetMessage::VerifyRequest {
            cid_hex,
            source_text,
        } => {
            // Remote peers must not be able to queue unbounded model work.
            // Over capacity, close without a response; the requester sees EOF.
            let Ok(_verification) = verifications.try_acquire() else {
                return;
            };
            #[cfg(not(feature = "legacy"))]
            {
                let _ = source_text;
                let response = NetMessage::VerifyResponse {
                    cid_hex,
                    agreement_score: 0.0,
                    verified: false,
                };
                let _ = send_message(&mut stream, &response).await;
            }
            #[cfg(feature = "legacy")]
            {
                // Get config for Ollama
                let (ollama_url, model, _original_wire) = {
                    let state = shared.lock().await;
                    (
                        state.config.ollama_url.clone(),
                        state.config.model.clone(),
                        None::<Vec<u8>>,
                    )
                };

                let result = verifier_service::verify_ku(
                    &source_text,
                    &[],
                    &ollama_url,
                    &model,
                )
                .await;

                // Send response
                let response = NetMessage::VerifyResponse {
                    cid_hex: cid_hex.clone(),
                    agreement_score: result.agreement_score,
                    verified: result.verified,
                };
                let _ = send_message(&mut stream, &response).await;

                let _ = event_tx
                    .send(NodeEvent::Notification(format!(
                        "  📋 Verified KU {} for {} (score: {:.0}%)",
                        &cid_hex[..8],
                        peer_addr,
                        result.agreement_score * 100.0
                    )))
                    .await;
            }
        }

        NetMessage::VerifyResponse {
            cid_hex,
            agreement_score,
            verified,
        } => {
            let _ = event_tx
                .send(NodeEvent::VerifyResult {
                    cid_hex,
                    agreement_score,
                    verified,
                    from: format!("{}", peer_addr),
                })
                .await;
        }

        NetMessage::PeerList { peers } => {
            let mut state = shared.lock().await;
            for addr in peers {
                // Add as unnamed peer (we'll get their name on hello)
                let info = PeerInfo {
                    name: format!("peer@{}", addr),
                    addr,
                    ku_count: 0,
                };
                state.peer_manager.add_peer(info);
            }
        }
    }
}

fn admission_priority(kind: StoredRecordKind) -> u8 {
    match kind {
        StoredRecordKind::FeedInception => 0,
        StoredRecordKind::Object => 1,
        StoredRecordKind::AuthorityEvent => 2,
        StoredRecordKind::Event => 3,
    }
}

fn reconcile_kind(kind: StoredRecordKind) -> ReconcileManifestKind {
    match kind {
        StoredRecordKind::Object => ReconcileManifestKind::Object,
        StoredRecordKind::Event => ReconcileManifestKind::Event,
        StoredRecordKind::FeedInception => ReconcileManifestKind::FeedInception,
        StoredRecordKind::AuthorityEvent => ReconcileManifestKind::AuthorityEvent,
    }
}

/// Format a 32-byte CID as a hex string.
pub fn hex_cid(cid: &[u8; 32]) -> String {
    cid.iter().map(|b| format!("{:02x}", b)).collect()
}

fn decode_integration_root(value: &str) -> Result<[u8; 32], NodeError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(NodeError::Config(
            "Concept Registry release root is not 32-byte hex".into(),
        ));
    }
    let mut decoded = [0_u8; 32];
    for (index, output) in decoded.iter_mut().enumerate() {
        *output = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| {
            NodeError::Config("Concept Registry release root is not valid hex".into())
        })?;
    }
    Ok(decoded)
}

/// Parse a hex CID string (prefix match) to 32-byte array.
fn parse_cid_hex(hex_str: &str) -> Option<[u8; 32]> {
    if hex_str.len() < 8 {
        return None;
    }
    // For prefix match, pad with zeros
    let padded = if hex_str.len() < 64 {
        format!("{:0<64}", hex_str)
    } else {
        hex_str[..64].to_string()
    };
    let bytes: Result<Vec<u8>, _> = (0..32)
        .map(|i| u8::from_str_radix(&padded[i * 2..i * 2 + 2], 16))
        .collect();
    bytes.ok().and_then(|b| b.try_into().ok())
}

/// Convert gene type u8 to human-readable name.
///
/// Delegates to [`crate::display::gene_type_name()`] for the canonical
/// KU v7 mapping shared across all platforms.
fn gene_type_name(gt: u8) -> &'static str {
    crate::display::gene_type_name(gt)
}
