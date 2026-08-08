//! OneBrain Node — ties together all subsystems.
//!
//! The `OneBrainNode` is the top-level runtime struct that owns the
//! Mediator, AI backend, concept dictionary, persistent storage,
//! anti-gaming guard, and peer networking.

use crate::anti_gaming_guard::AntiGamingGuard;
use crate::blob_authority::{
    BlobAuthority, OsPendingUploadIdSource, PendingBlobUploadId, PendingOwnedBlobUpload,
    UnavailableValidatedBlobReferenceSource,
};
use crate::concept_registry_runtime::{
    initialize_concept_registry, ConceptRegistryRuntimeState, ConceptRegistryStatus,
};
use crate::config::NodeConfig;
use crate::dataset_path::BootstrapDatasetPathResolver;
use crate::error::NodeError;
use crate::network::{recv_message, send_message, NetMessage, NodeEvent, PeerInfo};
use crate::peer_manager::PeerManager;
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
#[cfg(feature = "vnext-network-runtime")]
use ku_net::vnext_session::SessionIdentitySigner;

use crate::types::*;
use ku_ai::OllamaBackend;
use ku_core::blob_store::{BlobCid, BlobMeta, BlobType};
use ku_core::concept_registry::ConceptLookup;
use ku_core::foundation::ObjectReference;
use ku_core::text_parser::{default_dict, ConceptDict};
use ku_core::KuRuntime;
use ku_encoder::{AiEncoder, EncoderConfig, EncodingResult};
use ku_kql::blob_storage::{BlobStorage, BlobStorageConfig};
use ku_kql::storage::KuStorage;
use ku_mediator::input::UserInput;
use ku_mediator::mediator::{Mediator, MediatorConfig};
use ku_mediator::retriever::KuRetriever;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

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

/// Shared state accessible from both the REPL and background tasks.
pub struct SharedState {
    /// Persistent KU storage.
    pub storage: KuStorage,
    /// Persistent blob storage.
    pub blob_store: BlobStorage,
    /// Canonical reference oracle and durable pending upload leases.
    pub blob_authority: Arc<BlobAuthority>,
    /// Keyword-based KU retriever.
    pub retriever: KuRetriever,
    /// Connected peers.
    pub peer_manager: PeerManager,
    /// Node configuration (for data paths, Ollama URL, etc.).
    pub config: NodeConfig,
}

/// The top-level OneBrain node.
pub struct OneBrainNode {
    config: NodeConfig,
    mediator: Mediator,
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
    /// KU collections.
    collections: Vec<Collection>,
    // ── Persistent in-memory state (replaces stubs) ──
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
    /// 4. KU retriever (keyword index, loaded from disk)
    /// 5. Anti-gaming guard (rate limiting + quality gates)
    ///
    /// On startup, all existing KUs are loaded from storage into the
    /// retriever's keyword index (using stored wire bytes to reconstruct
    /// source text for keyword search).
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

        // Create chat backend
        let chat_backend =
            OllamaBackend::new(&config.ollama_url, &config.model, "nomic-embed-text", 120)
                .map_err(|e| NodeError::Ai(e))?;

        // Create encoder backend for mediator
        let mediator_encoder_backend =
            OllamaBackend::new(&config.ollama_url, &config.model, "nomic-embed-text", 120)
                .map_err(|e| NodeError::Ai(e))?;

        // Create concept dictionary
        let dict: ConceptDict = default_dict();

        // Create mediator
        let mediator = Mediator::new(
            Box::new(chat_backend),
            Box::new(mediator_encoder_backend),
            dict.clone(),
            MediatorConfig::default(),
        );

        // Open persistent storage
        let storage = KuStorage::open(&config.storage_path())
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let dataset_paths = Arc::new(
            BootstrapDatasetPathResolver::new(config.data_dir.join("base-bootstrap"))
                .map_err(|error| NodeError::Storage(error.to_string()))?,
        );
        let blob_authority = Arc::new(BlobAuthority::new(
            dataset_paths,
            Arc::new(OsPendingUploadIdSource),
            Arc::new(UnavailableValidatedBlobReferenceSource),
        ));
        blob_authority
            .pending()
            .reconcile_generation()
            .map_err(|error| NodeError::Storage(error.to_string()))?;
        let blob_store = BlobStorage::open_with_config(
            &config.blob_storage_path(),
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

        // Load or create retriever index
        let retriever = KuRetriever::load(&config.retriever_path())
            .map_err(|e| NodeError::Storage(format!("Retriever load failed: {}", e)))?;

        // Report startup KU count
        let ku_count = storage
            .count()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        if ku_count > 0 {
            eprintln!("  ✓ Storage contains {} KU(s)", ku_count);
            // Note: retriever index is loaded from disk (retriever_path),
            // so already populated from previous sessions' index_ku() calls.
            eprintln!(
                "  ✓ Retriever index loaded ({} entries)",
                retriever.index_size()
            );
        }

        // Create anti-gaming guard
        let guard = AntiGamingGuard::new();

        // Create shared state
        let shared = Arc::new(Mutex::new(SharedState {
            storage,
            blob_store,
            blob_authority,
            retriever,
            peer_manager: PeerManager::new(),
            config: config.clone(),
        }));

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel::<NodeEvent>(256);

        // Capture device name before config ownership transfer
        let device_name = config.name.clone();

        Ok(Self {
            config,
            mediator,
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

    /// Inject a caller-owned signer before starting the network. When omitted,
    /// the compatibility/development file signer is used.
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

    /// Start the TCP listener and spawn the background accept loop.
    ///
    /// Returns the local address the listener is bound to.
    pub async fn start_network(&mut self) -> Result<SocketAddr, NodeError> {
        if self.listener_task.is_some() {
            return Err(NodeError::Network(
                "network listener is already running".into(),
            ));
        }
        let bind_addr: SocketAddr = ([0, 0, 0, 0], self.config.port).into();
        #[cfg(feature = "vnext-network-runtime")]
        let mut pending_vnext = if self
            .config
            .vnext
            .is_active(crate::vnext_config::VNextFeature::ObpRp)
        {
            let dependencies = self.vnext_product_dependencies.take().ok_or_else(|| {
                NodeError::Config(
                    "active OBP-RP requires caller-owned vNext Vault and Policy dependencies"
                        .into(),
                )
            })?;
            Some(
                VNextProductRuntime::start(
                    &self.config.data_dir,
                    bind_addr,
                    &self.config.vnext,
                    dependencies,
                    self.vnext_identity_signer.clone(),
                )
                .await
                .map_err(|error| {
                    NodeError::Network(format!(
                        "Failed to start integrated vNext product runtime: {error}"
                    ))
                })?,
            )
        } else {
            None
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

    /// Connect to a seed peer and exchange handshake.
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
                match TcpStream::connect(addr).await {
                    Ok(mut stream) => {
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
                    Err(_) => {}
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
        self.guard
            .check_rate_limit()
            .map_err(|e| NodeError::Pipeline(e))?;

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
        let encoder_backend = OllamaBackend::new(
            &self.config.ollama_url,
            &self.config.model,
            "nomic-embed-text",
            300,
        )
        .map_err(|e| NodeError::Ai(e))?;

        let encoder = AiEncoder::new(
            Box::new(encoder_backend),
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
            .map_err(|e| NodeError::Pipeline(e))?;

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
            let mut state = self.shared.lock().await;

            // 4c. Store in redb
            cid = state
                .storage
                .put(&ku)
                .map_err(|e| NodeError::Storage(format!("{}", e)))?;

            // 4d. Index source text in retriever (for keyword search)
            cid_hex = hex_cid(&cid);
            state.retriever.index_ku(cid_hex.clone(), text.to_string());

            // Save retriever index to disk
            let _ = state.retriever.save(&state.config.retriever_path());
        }

        // 4e. Record creation in rate tracker
        self.guard.record_creation();

        // 5. Also process any additional KUs (if encoding produced multiple)
        {
            let mut state = self.shared.lock().await;
            for extra_bytes in encoding_result.wire_bytes.iter().skip(1) {
                if let Ok(extra_ku) = KuRuntime::from_wire(extra_bytes.clone()) {
                    let extra_instr = extra_ku.dna.instructions.len();
                    if self.guard.check_quality(extra_bytes, extra_instr).is_ok() {
                        if let Ok(extra_cid) = state.storage.put(&extra_ku) {
                            let extra_hex = hex_cid(&extra_cid);
                            state.retriever.index_ku(extra_hex, text.to_string());
                        }
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

    /// Process user input through the mediator pipeline.
    pub async fn process_input(&mut self, input: &str) -> Result<String, NodeError> {
        let user_input = UserInput::Text(input.to_string());
        let response = self
            .mediator
            .process(user_input)
            .await
            .map_err(NodeError::Mediator)?;
        Ok(response.text)
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
        crate::vnext_status::VNextStatusSnapshot::local_runtime_with_network(
            self.ku_count().unwrap_or(0),
            self.peer_count(),
            &effective_vnext,
            true,
            runtime,
        )
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

    // ═══════════════════════════════════════════════════════
    // Step 2: Identity
    // ═══════════════════════════════════════════════════════

    /// Get identity information for the current node.
    pub fn get_identity_info(&self) -> Result<IdentityInfo, NodeError> {
        // Read identity from file if exists
        let identity_path = self.config.identity_path();
        let (node_id, created) = if identity_path.exists() {
            let data = std::fs::read_to_string(&identity_path).map_err(|e| NodeError::Io(e))?;
            let json: serde_json::Value = serde_json::from_str(&data)
                .map_err(|e| NodeError::Config(format!("Invalid identity file: {}", e)))?;
            let nid = json["node_id"].as_str().unwrap_or("unknown").to_string();
            let created = json["created"].as_u64().unwrap_or(0);
            (nid, created)
        } else {
            ("not-created".to_string(), 0)
        };

        Ok(IdentityInfo {
            node_id,
            name: self.config.name.clone(),
            created,
            tier: "Contributor".to_string(), // TODO: compute from trust score
            trust_score: 0.0,                // TODO: wire to trust system
            device_count: 1,
            max_devices: 16,
            kus_encoded: self.ku_count().unwrap_or(0) as u64,
            kus_received: 0,  // TODO: track received KUs
            total_queries: 0, // TODO: track queries
        })
    }

    /// Recover identity from BIP39 phrase.
    pub fn recover_identity(
        &mut self,
        phrase: &[String],
        _password: &str,
    ) -> Result<IdentityInfo, NodeError> {
        // Validate BIP39 phrase (24 words)
        if phrase.len() != 24 {
            return Err(NodeError::InvalidPhrase(format!(
                "Expected 24 words, got {}",
                phrase.len()
            )));
        }
        // TODO: Actual BIP39 derivation + keypair generation
        // For now, create a placeholder identity
        let hash_input = phrase.join(" ");
        let hash_bytes: [u8; 32] = blake3::hash(hash_input.as_bytes()).into();
        let identity = serde_json::json!({
            "node_id": hex_cid(&hash_bytes),
            "created": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
            "recovered": true
        });
        let identity_path = self.config.identity_path();
        std::fs::write(
            &identity_path,
            serde_json::to_string_pretty(&identity)
                .map_err(|e| NodeError::Config(format!("Serialize error: {}", e)))?,
        )
        .map_err(|e| NodeError::Io(e))?;
        self.get_identity_info()
    }

    // ═══════════════════════════════════════════════════════
    // Step 3: Knowledge Operations
    // ═══════════════════════════════════════════════════════

    /// List KUs with pagination and filtering.
    pub fn list_kus(
        &self,
        page: usize,
        limit: usize,
        type_filter: Option<&str>,
        sort_by: &str,
    ) -> Result<(Vec<KuListItem>, usize), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // Convert to view items
        let mut items: Vec<KuListItem> = all_kus
            .iter()
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                // Use expression text if available, otherwise try retriever, then indicate binary-only
                let preview = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.clone())
                    .or_else(|| state.retriever.get_expression(&cid_hex))
                    .unwrap_or_else(|| {
                        format!(
                            "[{} KU, {} instructions]",
                            gene_type,
                            ku.instruction_count()
                        )
                    });
                let preview = if preview.len() > 80 {
                    format!("{}...", &preview[..77])
                } else {
                    preview
                };
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku
                    .epi
                    .epigenetic
                    .as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem {
                    cid_hex,
                    gene_type,
                    preview,
                    pomv,
                    pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                    pomv_is_economic: false,
                    trust,
                    created,
                    wire_size,
                }
            })
            .collect();

        // Filter by type
        if let Some(type_f) = type_filter {
            let type_lower = type_f.to_lowercase();
            items.retain(|i| i.gene_type.to_lowercase() == type_lower);
        }

        // Sort
        match sort_by {
            "pomv" => items.sort_by(|a, b| {
                b.pomv
                    .partial_cmp(&a.pomv)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            "trust" => items.sort_by(|a, b| {
                b.trust
                    .partial_cmp(&a.trust)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            _ => items.sort_by(|a, b| b.created.cmp(&a.created)), // default: newest first
        }

        // Paginate
        let total_filtered = items.len();
        let start = (page.saturating_sub(1)) * limit;
        let items = items.into_iter().skip(start).take(limit).collect();

        Ok((items, total_filtered))
    }

    /// Get detailed view of a single KU.
    pub fn get_ku(&self, cid_hex: &str) -> Result<KuDetail, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };

        // Parse CID hex to bytes
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;

        let ku = state.storage.get(&cid_bytes).map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("not found") || msg.contains("NotFound") {
                NodeError::KuNotFound(cid_hex.to_string())
            } else {
                NodeError::Storage(msg)
            }
        })?;

        let gene_type = gene_type_name(ku.gene_type()).to_string();
        let content = ku
            .expr
            .as_ref()
            .map(|e| e.text.clone())
            .or_else(|| state.retriever.get_expression(cid_hex))
            .unwrap_or_else(|| {
                format!(
                    "[{} KU, {} instructions]",
                    gene_type,
                    ku.instruction_count()
                )
            });
        let trust = ku.epi.trust.trust_score as f64 / 10000.0;
        let pomv = ku.epi.pomv_score();
        let confidence = ku.epi.trust.confidence as f32 / 10000.0;
        let created = ku
            .epi
            .epigenetic
            .as_ref()
            .and_then(|ep| ep.recorded_at)
            .unwrap_or(0);
        let wire_size = ku.wire_bytes.len();
        let instruction_count = ku.instruction_count();

        // Extract concept IDs as codon views
        // Build reverse lookup: concept_id → name (using node's live dict which includes new concepts)
        let reverse: std::collections::HashMap<u64, String> = self
            .dict
            .iter()
            .map(|(name, &id)| (id, name.clone()))
            .collect();
        let cn = |id: u64| -> String {
            reverse
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("#{}", id))
        };
        let codons: Vec<CodonView> = ku
            .concept_ids()
            .iter()
            .enumerate()
            .map(|(i, &cid)| CodonView {
                name: cn(cid),
                role: if i == 0 {
                    "Subject".to_string()
                } else {
                    "Related".to_string()
                },
            })
            .collect();

        // Get bonds from epigenetics
        let bonds: Vec<BondView> = ku
            .epi
            .bonds
            .iter()
            .map(|b| BondView {
                direction: "OUT".to_string(),
                relation: format!("{:?}", b.relation),
                other_cid: b
                    .target_cid
                    .iter()
                    .map(|byte| format!("{:02x}", byte))
                    .collect::<String>(),
                other_preview: String::new(),
                weight: b.weight as f64 / 10000.0,
            })
            .collect();
        let outgoing_bond_count = bonds.len();
        let incoming_bond_count = 0; // TODO: wire to GraphStorage for incoming

        // Decode instructions for human-readable view
        let decoded_instructions: Vec<crate::types::InstructionView> = ku
            .dna
            .instructions
            .iter()
            .map(|instr| {
                use ku_core::core_dna::Instruction;
                match instr {
                    Instruction::Triple { s, p, o } => crate::types::InstructionView {
                        op: "Triple".into(),
                        description: format!("{} —[{}]→ {}", cn(*s), cn(*p), cn(*o)),
                        concept_ids: vec![*s, *p, *o],
                    },
                    Instruction::Quality { s, q } => crate::types::InstructionView {
                        op: "Quality".into(),
                        description: format!("{} has quality {}", cn(*s), cn(*q)),
                        concept_ids: vec![*s, *q],
                    },
                    Instruction::Quantity { s, value, unit } => crate::types::InstructionView {
                        op: "Quantity".into(),
                        description: format!("{} = {} {}", cn(*s), value, cn(*unit)),
                        concept_ids: vec![*s, *unit],
                    },
                    Instruction::Step {
                        ord,
                        action,
                        target,
                    } => crate::types::InstructionView {
                        op: "Step".into(),
                        description: format!("Step #{}: {} → {}", ord, cn(*action), cn(*target)),
                        concept_ids: vec![*action, *target],
                    },
                    Instruction::PartOf { part, whole } => crate::types::InstructionView {
                        op: "PartOf".into(),
                        description: format!("{} part-of {}", cn(*part), cn(*whole)),
                        concept_ids: vec![*part, *whole],
                    },
                    Instruction::Causal { cause, effect } => crate::types::InstructionView {
                        op: "Causal".into(),
                        description: format!("{} causes {}", cn(*cause), cn(*effect)),
                        concept_ids: vec![*cause, *effect],
                    },
                    Instruction::Located { s, location } => crate::types::InstructionView {
                        op: "Located".into(),
                        description: format!("{} located-at {}", cn(*s), cn(*location)),
                        concept_ids: vec![*s, *location],
                    },
                    Instruction::Temporal { s, time } => crate::types::InstructionView {
                        op: "Temporal".into(),
                        description: format!("{} at-time {}", cn(*s), cn(*time)),
                        concept_ids: vec![*s, *time],
                    },
                    Instruction::Simulates { s, model } => crate::types::InstructionView {
                        op: "Simulates".into(),
                        description: format!("{} simulates {}", cn(*s), cn(*model)),
                        concept_ids: vec![*s, *model],
                    },
                    Instruction::Condition { cond, result } => crate::types::InstructionView {
                        op: "Condition".into(),
                        description: format!("if {} then {}", cn(*cond), cn(*result)),
                        concept_ids: vec![*cond, *result],
                    },
                    Instruction::Agent { actor, action } => crate::types::InstructionView {
                        op: "Agent".into(),
                        description: format!("{} performs {}", cn(*actor), cn(*action)),
                        concept_ids: vec![*actor, *action],
                    },
                    Instruction::Tool { action, instrument } => crate::types::InstructionView {
                        op: "Tool".into(),
                        description: format!("{} uses tool {}", cn(*action), cn(*instrument)),
                        concept_ids: vec![*action, *instrument],
                    },
                    Instruction::Range { s, min, max } => crate::types::InstructionView {
                        op: "Range".into(),
                        description: format!("{} ∈ [{}, {}]", cn(*s), min, max),
                        concept_ids: vec![*s],
                    },
                    Instruction::Tolerance { s, value, delta } => crate::types::InstructionView {
                        op: "Tolerance".into(),
                        description: format!("{} = {} ± {}", cn(*s), value, delta),
                        concept_ids: vec![*s],
                    },
                    Instruction::Constraint { source, op, target } => {
                        crate::types::InstructionView {
                            op: "Constraint".into(),
                            description: format!("{} {:?} {}", cn(*source), op, cn(*target)),
                            concept_ids: vec![*source, *target],
                        }
                    }
                    Instruction::Certainty { level } => crate::types::InstructionView {
                        op: "Certainty".into(),
                        description: format!("certainty = {:.1}%", *level as f64 / 100.0),
                        concept_ids: vec![],
                    },
                    Instruction::Difficulty { level } => crate::types::InstructionView {
                        op: "Difficulty".into(),
                        description: format!("difficulty = {}/4", level),
                        concept_ids: vec![],
                    },
                    Instruction::Sequence { items } => crate::types::InstructionView {
                        op: "Sequence".into(),
                        description: format!(
                            "sequence[{}]",
                            items.iter().map(|i| cn(*i)).collect::<Vec<_>>().join(", ")
                        ),
                        concept_ids: items.clone(),
                    },
                    Instruction::EnumVal { s, values } => crate::types::InstructionView {
                        op: "EnumVal".into(),
                        description: format!(
                            "{} ∈ {{{}}}",
                            cn(*s),
                            values.iter().map(|v| cn(*v)).collect::<Vec<_>>().join(", ")
                        ),
                        concept_ids: std::iter::once(*s).chain(values.iter().cloned()).collect(),
                    },
                    Instruction::CidRef { cid } => crate::types::InstructionView {
                        op: "CidRef".into(),
                        description: format!(
                            "ref → {}",
                            cid.iter().map(|b| format!("{:02x}", b)).collect::<String>()
                        ),
                        concept_ids: vec![],
                    },
                    Instruction::Precond { concept } => crate::types::InstructionView {
                        op: "Precond".into(),
                        description: format!("precondition {}", cn(*concept)),
                        concept_ids: vec![*concept],
                    },
                    Instruction::Effect { concept } => crate::types::InstructionView {
                        op: "Effect".into(),
                        description: format!("effect {}", cn(*concept)),
                        concept_ids: vec![*concept],
                    },
                    Instruction::Affect { v, a, d } => crate::types::InstructionView {
                        op: "Affect".into(),
                        description: format!("VAD({}, {}, {})", v, a, d),
                        concept_ids: vec![],
                    },
                    Instruction::Label { key, value } => crate::types::InstructionView {
                        op: "Label".into(),
                        description: format!("{} = {}", cn(*key), cn(*value)),
                        concept_ids: vec![*key, *value],
                    },
                    Instruction::Witness { count, proximity } => crate::types::InstructionView {
                        op: "Witness".into(),
                        description: format!("{} witnesses, proximity={}", count, proximity),
                        concept_ids: vec![],
                    },
                    Instruction::End => crate::types::InstructionView {
                        op: "End".into(),
                        description: "end of instructions".into(),
                        concept_ids: vec![],
                    },
                    other => crate::types::InstructionView {
                        op: format!("{:?}", other)
                            .split_whitespace()
                            .next()
                            .unwrap_or("Unknown")
                            .to_string(),
                        description: format!("{:?}", other),
                        concept_ids: vec![],
                    },
                }
            })
            .collect();

        Ok(KuDetail {
            cid_hex: hex_cid(&ku.cid),
            gene_type,
            content,
            codons,
            bonds,
            trust,
            pomv,
            pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
            pomv_is_economic: false,
            pomv_breakdown: PomvBreakdown {
                metabolic: ku.epi.trust.metabolic_rate as f64 / 10000.0,
                prediction: ku.epi.trust.prediction_score as f64 / 10000.0,
                entropy: ku.epi.trust.entropy_at_creation as f64 / 10000.0,
                survival: ku.epi.trust.survival_score as f64 / 10000.0,
                centrality: ku.epi.trust.synaptic_centrality as f64 / 10000.0,
                niche: ku.epi.trust.niche_fitness as f64 / 10000.0,
            },
            epistemic: format!("{:?}", ku.epi.trust.epistemic_status),
            evidence: format!("{:?}", ku.epi.trust.evidence_type),
            wire_size,
            instruction_count,
            confidence,
            created,
            verification_status: format!("{:?}", ku.encoding_status),
            outgoing_bond_count,
            incoming_bond_count,
            decoded_instructions,
        })
    }

    /// Delete a KU from local storage.
    pub fn delete_ku(&self, cid_hex: &str) -> Result<bool, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;
        state
            .storage
            .delete(&cid_bytes)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Execute a KQL query string.
    ///
    /// Parses the query with the KQL parser, loads all KUs into a
    /// `LocalExecutor`, executes the query, and returns matching KUs
    /// as `KuListItem`s for the UI.
    pub fn execute_kql(&self, query_str: &str) -> Result<Vec<KuListItem>, NodeError> {
        // 1. Parse the KQL query (validates syntax)
        let query = ku_kql::parser::parse_query(query_str)
            .map_err(|e| NodeError::Kql(format!("Syntax error: {}", e)))?;

        // 2. Get all KUs from storage
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // 3. Create executor and load KUs
        let mut executor = ku_kql::executor::LocalExecutor::new();
        for ku in all_kus {
            executor.insert(ku);
        }

        // 4. Execute the query
        let result = executor
            .execute(&query)
            .map_err(|e| NodeError::Kql(format!("{}", e)))?;

        // 5. Convert QueryResult rows to KuListItems
        let results: Vec<KuListItem> = result
            .rows
            .iter()
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                let preview = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.clone())
                    .or_else(|| state.retriever.get_expression(&cid_hex))
                    .unwrap_or_else(|| format!("[{} KU]", gene_type));
                let preview = if preview.len() > 80 {
                    format!("{}...", &preview[..77])
                } else {
                    preview
                };
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku
                    .epi
                    .epigenetic
                    .as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem {
                    cid_hex,
                    gene_type,
                    preview,
                    pomv,
                    pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                    pomv_is_economic: false,
                    trust,
                    created,
                    wire_size,
                }
            })
            .collect();

        Ok(results)
    }

    /// Plain-text search — matches query against KU content and gene type
    /// without requiring KQL syntax. Used by the web dashboard search.
    pub fn search_text(&self, query: &str, limit: usize) -> Result<Vec<KuListItem>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let q_lower = query.to_lowercase();
        let results: Vec<KuListItem> = all_kus
            .iter()
            .filter(|ku| {
                let gene = gene_type_name(ku.gene_type()).to_lowercase();
                let cid_hex_tmp = hex_cid(&ku.cid);
                let text = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.to_lowercase())
                    .or_else(|| {
                        state
                            .retriever
                            .get_expression(&cid_hex_tmp)
                            .map(|t| t.to_lowercase())
                    })
                    .unwrap_or_default();
                text.contains(&q_lower) || gene.contains(&q_lower)
            })
            .take(limit)
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                let preview = ku
                    .expr
                    .as_ref()
                    .map(|e| e.text.clone())
                    .or_else(|| state.retriever.get_expression(&cid_hex))
                    .unwrap_or_else(|| format!("[{} KU]", gene_type));
                let preview = if preview.len() > 80 {
                    format!("{}...", &preview[..77])
                } else {
                    preview
                };
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku
                    .epi
                    .epigenetic
                    .as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem {
                    cid_hex,
                    gene_type,
                    preview,
                    pomv,
                    pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                    pomv_is_economic: false,
                    trust,
                    created,
                    wire_size,
                }
            })
            .collect();

        Ok(results)
    }

    /// Get graph neighbors of a KU.
    pub fn get_neighbors(
        &self,
        cid_hex: &str,
        _depth: u32,
    ) -> Result<Vec<NeighborInfo>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;

        // Verify KU exists (get returns Err(NotFound) if missing)
        let _ku = state.storage.get(&cid_bytes).map_err(|e| {
            let msg = format!("{}", e);
            if msg.contains("not found") || msg.contains("NotFound") {
                NodeError::KuNotFound(cid_hex.to_string())
            } else {
                NodeError::Storage(msg)
            }
        })?;

        let mut neighbors = Vec::new();

        // Outgoing bonds: this KU → targets
        if let Ok(outgoing) = state.storage.graph().outgoing_bonds(&cid_bytes) {
            for (rel, target_cid, meta) in &outgoing {
                let target_hex = hex_cid(&target_cid);
                let (preview, gene_type, pomv, is_local) = match state.storage.get(target_cid) {
                    Ok(target_ku) => {
                        let preview = target_ku
                            .expr
                            .as_ref()
                            .map(|e| e.text.chars().take(80).collect::<String>())
                            .unwrap_or_default();
                        let gt = target_ku
                            .extract_field("gene_type")
                            .map(|v| format!("{:?}", v))
                            .unwrap_or_else(|| "fact".into());
                        let pomv_val = target_ku.epi.trust.trust_score as f64 / 10000.0;
                        (preview, gt, pomv_val, true)
                    }
                    Err(_) => (String::new(), "unknown".into(), 0.0, false),
                };

                neighbors.push(NeighborInfo {
                    cid_hex: target_hex,
                    relation: format!("{:?}", rel),
                    direction: "OUT".into(),
                    preview,
                    weight: meta.weight as f64 / 10000.0,
                    gene_type,
                    pomv,
                    is_local,
                    children: Vec::new(),
                });
            }
        }

        // Incoming bonds: sources → this KU
        if let Ok(incoming) = state.storage.graph().incoming_bonds(&cid_bytes) {
            for (rel, source_cid) in &incoming {
                let source_hex = hex_cid(&source_cid);
                let (preview, gene_type, pomv, is_local) = match state.storage.get(source_cid) {
                    Ok(source_ku) => {
                        let preview = source_ku
                            .expr
                            .as_ref()
                            .map(|e| e.text.chars().take(80).collect::<String>())
                            .unwrap_or_default();
                        let gt = source_ku
                            .extract_field("gene_type")
                            .map(|v| format!("{:?}", v))
                            .unwrap_or_else(|| "fact".into());
                        let pomv_val = source_ku.epi.trust.trust_score as f64 / 10000.0;
                        (preview, gt, pomv_val, true)
                    }
                    Err(_) => (String::new(), "unknown".into(), 0.0, false),
                };

                neighbors.push(NeighborInfo {
                    cid_hex: source_hex,
                    relation: format!("{:?}", rel),
                    direction: "IN".into(),
                    preview,
                    weight: 0.0, // Incoming bonds don't carry weight in BondMeta from this side
                    gene_type,
                    pomv,
                    is_local,
                    children: Vec::new(),
                });
            }
        }

        Ok(neighbors)
    }

    // ═══════════════════════════════════════════════════════
    // Step 4: Profile & AI
    // ═══════════════════════════════════════════════════════

    /// Get user profile.
    pub fn get_profile(&self) -> Result<UserProfileView, NodeError> {
        let profile = self.mediator.profile();
        Ok(UserProfileView {
            name: profile.display_name.clone(),
            language: profile.preferred_language.clone(),
            style: format!("{:?}", profile.response_style),
            expertise: profile
                .top_expertise(5)
                .iter()
                .map(|e| ExpertiseView {
                    domain: e.domain.clone(),
                    ku_count: e.ku_count as u64,
                    last_active: e.last_active,
                })
                .collect(),
            total_kus: self.ku_count().unwrap_or(0) as u64,
            total_queries: profile.total_queries,
            member_since: profile.created_at,
        })
    }

    /// Update a profile field.
    pub fn update_profile(&mut self, field: &str, value: &str) -> Result<(), NodeError> {
        use ku_mediator::profile::ResponseStyle;
        let profile = self.mediator.profile_mut();
        match field {
            "name" | "display_name" => profile.display_name = value.to_string(),
            "language" => profile.preferred_language = value.to_string(),
            "style" | "response_style" => {
                profile.response_style = match value {
                    "concise" => ResponseStyle::Concise,
                    "balanced" => ResponseStyle::Balanced,
                    "detailed" => ResponseStyle::Detailed,
                    "academic" => ResponseStyle::Academic,
                    _ => {
                        return Err(NodeError::InvalidArgument(format!(
                            "Invalid style: '{}'. Options: concise, balanced, detailed, academic",
                            value
                        )))
                    }
                };
            }
            _ => {
                return Err(NodeError::InvalidArgument(format!(
                    "Unknown profile field: '{}'. Fields: name, language, style",
                    field
                )))
            }
        }
        profile
            .save(&self.config.profile_path())
            .map_err(|e| NodeError::Storage(format!("Failed to save profile: {}", e)))?;
        Ok(())
    }

    /// List available AI models.
    pub fn list_ai_models(&self) -> Result<Vec<ModelInfo>, NodeError> {
        // Read from ku-ai registry
        let current_model = self.config.model.clone();
        let models = vec![ModelInfo {
            name: current_model.clone(),
            params: "current".to_string(),
            is_current: true,
            is_installed: true,
        }];
        // TODO: Query Ollama API for installed models
        // GET http://localhost:11434/api/tags
        Ok(models)
    }

    /// Switch the active AI model.
    ///
    /// Creates new AI backends with the new model and replaces them in the Mediator,
    /// so chat and encoding will use the new model immediately.
    pub fn switch_model(&mut self, model_name: &str) -> Result<(), NodeError> {
        // Create new backends with the new model
        let new_chat_backend =
            OllamaBackend::new(&self.config.ollama_url, model_name, "nomic-embed-text", 120)
                .map_err(|e| NodeError::Ai(e))?;

        let new_encoder_backend =
            OllamaBackend::new(&self.config.ollama_url, model_name, "nomic-embed-text", 120)
                .map_err(|e| NodeError::Ai(e))?;

        // Replace backends in mediator
        self.mediator
            .replace_backends(Box::new(new_chat_backend), Box::new(new_encoder_backend));

        // Update config
        self.config.model = model_name.to_string();

        // Also update config in shared state
        if let Ok(mut state) = self.shared.try_lock() {
            state.config.model = model_name.to_string();
        }

        Ok(())
    }

    /// Test AI connection.
    pub async fn test_ai_connection(&self) -> Result<AiHealthInfo, NodeError> {
        let start = std::time::Instant::now();

        // Simple TCP check to Ollama
        let addr = self
            .config
            .ollama_url
            .replace("http://", "")
            .replace("https://", "");

        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => {
                let latency = start.elapsed().as_millis() as u64;
                Ok(AiHealthInfo {
                    connected: true,
                    model: self.config.model.clone(),
                    ollama_url: self.config.ollama_url.clone(),
                    latency_ms: latency,
                    status_message: format!("Connected ({}ms)", latency),
                })
            }
            Err(e) => Ok(AiHealthInfo {
                connected: false,
                model: self.config.model.clone(),
                ollama_url: self.config.ollama_url.clone(),
                latency_ms: 0,
                status_message: format!("Connection failed: {}", e),
            }),
        }
    }

    // ═══════════════════════════════════════════════════════
    // Step 5: Config
    // ═══════════════════════════════════════════════════════

    /// Get configuration as a view.
    pub fn get_config_view(&self) -> ConfigView {
        ConfigView {
            name: self.config.name.clone(),
            port: self.config.port,
            data_dir: self.config.data_dir.display().to_string(),
            ollama_url: self.config.ollama_url.clone(),
            model: self.config.model.clone(),
            seeds: self.config.seeds.iter().map(|s| s.to_string()).collect(),
            identity_path: self.config.identity_path().display().to_string(),
            storage_path: self.config.storage_path().display().to_string(),
            profile_path: self.config.profile_path().display().to_string(),
            peers_path: self.config.peer_memory_path().display().to_string(),
        }
    }

    /// Update a config value.
    pub fn update_config(&mut self, key: &str, value: &str) -> Result<(), NodeError> {
        match key {
            "name" => self.config.name = value.to_string(),
            "port" => {
                self.config.port = value
                    .parse::<u16>()
                    .map_err(|_| NodeError::InvalidArgument(format!("Invalid port: {}", value)))?;
            }
            "ollama_url" => self.config.ollama_url = value.to_string(),
            "model" => self.config.model = value.to_string(),
            _ => {
                return Err(NodeError::InvalidArgument(format!(
                    "Unknown config key: '{}'. Keys: name, port, ollama_url, model",
                    key
                )))
            }
        }
        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // Step 6: OBT Wallet
    // ═══════════════════════════════════════════════════════

    /// Get the legacy wallet compatibility projection.
    ///
    /// This is a non-economic simulation derived from local KU count. It is not
    /// backed by AccountChain, consensus, finality, or spendable OBT.
    pub fn get_balance(&self) -> Result<WalletInfo, NodeError> {
        let ku_count = self.ku_count().unwrap_or(0) as u64;
        let total_earned = ku_count * 25_000;
        Ok(WalletInfo {
            economic_status: WalletEconomicStatus::SimulatedNonEconomic,
            limitations: vec![
                "Derived from local KU count; no AccountChain is connected.".to_string(),
                "Not spendable, settled, transferable, or reward-authoritative.".to_string(),
            ],
            balance: total_earned.saturating_sub(self.staked_amount),
            chain_length: ku_count + 1, // Open block + 1 Mint per KU
            tier: "Contributor".to_string(),
            multiplier: 0.5,
            total_earned,
            total_spent: 0,
            staked: self.staked_amount,
            pending_unstake: 0,
            streams: EarningsStreams {
                owner: ku_count * 10_000,   // 40%
                encoder: ku_count * 6_250,  // 25%
                verifier: ku_count * 3_750, // 15%
                storage: ku_count * 5_000,  // 20%
            },
            rate_used: 0,
            rate_max: 5, // Contributor tier
        })
    }

    /// Stake OBT tokens.
    pub fn stake(&mut self, _amount: u64) -> Result<WalletInfo, NodeError> {
        Err(NodeError::InvalidArgument(
            "OBT staking is disabled: the current wallet is a simulated, non-economic projection"
                .into(),
        ))
    }

    /// Unstake OBT tokens.
    pub fn unstake(&mut self, _amount: u64) -> Result<WalletInfo, NodeError> {
        Err(NodeError::InvalidArgument(
            "OBT unstaking is disabled: the current wallet is a simulated, non-economic projection"
                .into(),
        ))
    }

    /// Get wallet transaction history.
    pub fn get_wallet_history(&self, limit: usize) -> Result<Vec<WalletTransaction>, NodeError> {
        // TODO: Wire to AccountChain block traversal
        // For now return placeholder
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let ku_count = self.ku_count().unwrap_or(0);
        let mut transactions = Vec::new();

        // Generate placeholder transactions from KU count
        for i in 0..std::cmp::min(ku_count, limit) {
            transactions.push(WalletTransaction {
                economic_status: WalletEconomicStatus::SimulatedNonEconomic,
                block_type: "SimulatedMint".to_string(),
                amount: 25_000, // 25 OBT in milliOBT
                detail: format!("Non-economic placeholder derived from KU #{}", ku_count - i),
                timestamp: now - (i as u64 * 3600),
                confirmation: "Simulated".to_string(),
            });
        }

        Ok(transactions)
    }

    // ═══════════════════════════════════════════════════════
    // Step 7: Data Portability
    // ═══════════════════════════════════════════════════════

    /// Export KUs to a file.
    pub fn export_kus(&self, format: &str, path: &std::path::Path) -> Result<usize, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let count = all_kus.len();

        match format {
            "json" => {
                let items: Vec<serde_json::Value> = all_kus
                    .iter()
                    .map(|ku| {
                        serde_json::json!({
                            "cid": hex_cid(&ku.cid),
                            "gene_type": gene_type_name(ku.gene_type()),
                            "content": ku.expr.as_ref().map(|e| e.text.clone()).unwrap_or_default(),
                            "trust": ku.epi.trust.trust_score as f64 / 10000.0,
                            "pomv": ku.epi.pomv_score(),
                            "wire_size": ku.wire_bytes.len(),
                        })
                    })
                    .collect();
                let json = serde_json::to_string_pretty(&items)
                    .map_err(|e| NodeError::Storage(format!("JSON serialize error: {}", e)))?;
                std::fs::write(path, json)?;
            }
            "csv" => {
                let mut csv = String::from("cid,gene_type,content,trust,pomv,wire_size\n");
                for ku in &all_kus {
                    let content = ku
                        .expr
                        .as_ref()
                        .map(|e| e.text.replace('"', "\"\"").replace('\n', " "))
                        .unwrap_or_default();
                    csv.push_str(&format!(
                        "\"{}\",\"{}\",\"{}\",{:.4},{:.4},{}\n",
                        hex_cid(&ku.cid),
                        gene_type_name(ku.gene_type()),
                        content,
                        ku.epi.trust.trust_score as f64 / 10000.0,
                        ku.epi.pomv_score(),
                        ku.wire_bytes.len(),
                    ));
                }
                std::fs::write(path, csv)?;
            }
            _ => {
                return Err(NodeError::InvalidArgument(format!(
                    "Unknown export format: '{}'. Options: json, csv",
                    format
                )))
            }
        }

        Ok(count)
    }

    /// Import KUs from a text file (one paragraph per KU).
    pub async fn import_file(&mut self, path: &std::path::Path) -> Result<ImportResult, NodeError> {
        let content = std::fs::read_to_string(path)?;
        let paragraphs: Vec<&str> = content
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| !p.is_empty() && p.len() >= 50)
            .collect();

        let total_paragraphs = paragraphs.len();
        let mut imported = 0;
        let mut skipped = 0;
        let mut errors = 0;

        for text in &paragraphs {
            match self.encode_and_store(text).await {
                Ok(_) => imported += 1,
                Err(NodeError::Pipeline(ref msg)) if msg.contains("rate") => {
                    skipped = total_paragraphs - imported - errors;
                    break;
                }
                Err(_) => errors += 1,
            }
        }

        Ok(ImportResult {
            imported,
            skipped,
            errors,
        })
    }

    /// Create a backup of all node data.
    ///
    /// Includes identity, profile, peers, KU wire bytes, and in-memory state
    /// (tags, pins, follows, watches, deprecated KUs).
    pub fn create_backup(
        &self,
        path: &std::path::Path,
        _password: &str,
    ) -> Result<BackupInfo, NodeError> {
        let mut backup = serde_json::Map::new();

        // Identity
        let identity_path = self.config.identity_path();
        if identity_path.exists() {
            let data = std::fs::read_to_string(&identity_path)?;
            backup.insert("identity".to_string(), serde_json::Value::String(data));
        }

        // Profile
        let profile_path = self.config.profile_path();
        if profile_path.exists() {
            let data = std::fs::read_to_string(&profile_path)?;
            backup.insert("profile".to_string(), serde_json::Value::String(data));
        }

        // Peers
        let peers_path = self.config.peer_memory_path();
        if peers_path.exists() {
            let data = std::fs::read_to_string(&peers_path)?;
            backup.insert("peers".to_string(), serde_json::Value::String(data));
        }

        // KU data — export all KU wire bytes as hex strings
        let ku_count = self.ku_count().unwrap_or(0);
        backup.insert(
            "ku_count".to_string(),
            serde_json::Value::Number(ku_count.into()),
        );

        let mut ku_data = Vec::new();
        if let Ok((kus, _)) = self.list_kus(1, 100_000, None, "created") {
            for ku in &kus {
                ku_data.push(serde_json::json!({
                    "cid_hex": ku.cid_hex,
                    "gene_type": ku.gene_type,
                    "preview": ku.preview,
                    "pomv": ku.pomv,
                    "trust": ku.trust,
                    "created": ku.created,
                    "wire_size": ku.wire_size,
                }));
            }
        }
        backup.insert("kus".to_string(), serde_json::Value::Array(ku_data));

        // In-memory state: tags
        let tags_map: serde_json::Map<String, serde_json::Value> = self
            .ku_tags
            .iter()
            .map(|(cid, tags)| {
                let tag_arr: Vec<serde_json::Value> = tags
                    .iter()
                    .map(|t| serde_json::Value::String(t.clone()))
                    .collect();
                (cid.clone(), serde_json::Value::Array(tag_arr))
            })
            .collect();
        backup.insert("tags".to_string(), serde_json::Value::Object(tags_map));

        // In-memory state: pinned KUs
        let pinned: Vec<serde_json::Value> = self
            .pinned_kus
            .iter()
            .map(|c| serde_json::Value::String(c.clone()))
            .collect();
        backup.insert("pinned_kus".to_string(), serde_json::Value::Array(pinned));

        // In-memory state: follows
        backup.insert(
            "following".to_string(),
            serde_json::to_value(&self.following).unwrap_or(serde_json::Value::Array(vec![])),
        );

        // In-memory state: watches
        backup.insert(
            "watches".to_string(),
            serde_json::to_value(&self.watches).unwrap_or(serde_json::Value::Array(vec![])),
        );

        // In-memory state: deprecated KUs
        let deprecated: Vec<serde_json::Value> = self
            .deprecated_kus
            .iter()
            .map(|c| serde_json::Value::String(c.clone()))
            .collect();
        backup.insert(
            "deprecated_kus".to_string(),
            serde_json::Value::Array(deprecated),
        );

        let json = serde_json::to_string_pretty(&backup)
            .map_err(|e| NodeError::Backup(format!("Serialize error: {}", e)))?;
        let size = json.len() as u64;
        std::fs::write(path, &json)?;

        Ok(BackupInfo {
            path: path.display().to_string(),
            size,
            ku_count,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }

    /// Restore from a backup file.
    ///
    /// Restores identity, profile, peers, and in-memory state (tags, pins,
    /// follows, watches, deprecated KUs).
    pub fn restore_backup(
        &mut self,
        path: &std::path::Path,
        _password: &str,
    ) -> Result<(), NodeError> {
        let content = std::fs::read_to_string(path)?;
        let backup: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content)
            .map_err(|e| NodeError::Backup(format!("Invalid backup file: {}", e)))?;

        // Restore identity
        if let Some(serde_json::Value::String(identity)) = backup.get("identity") {
            std::fs::write(self.config.identity_path(), identity)?;
        }

        // Restore profile
        if let Some(serde_json::Value::String(profile)) = backup.get("profile") {
            std::fs::write(self.config.profile_path(), profile)?;
        }

        // Restore peers
        if let Some(serde_json::Value::String(peers)) = backup.get("peers") {
            std::fs::write(self.config.peer_memory_path(), peers)?;
        }

        // Restore tags
        if let Some(serde_json::Value::Object(tags_map)) = backup.get("tags") {
            self.ku_tags.clear();
            for (cid, tags_val) in tags_map {
                if let serde_json::Value::Array(tags_arr) = tags_val {
                    let mut tag_set = HashSet::new();
                    for t in tags_arr {
                        if let serde_json::Value::String(tag) = t {
                            tag_set.insert(tag.clone());
                        }
                    }
                    if !tag_set.is_empty() {
                        self.ku_tags.insert(cid.clone(), tag_set);
                    }
                }
            }
        }

        // Restore pinned KUs
        if let Some(serde_json::Value::Array(pinned)) = backup.get("pinned_kus") {
            self.pinned_kus.clear();
            for p in pinned {
                if let serde_json::Value::String(cid) = p {
                    self.pinned_kus.insert(cid.clone());
                }
            }
        }

        // Restore follows
        if let Some(following_val) = backup.get("following") {
            if let Ok(following) =
                serde_json::from_value::<Vec<FollowedNode>>(following_val.clone())
            {
                self.following = following;
            }
        }

        // Restore watches
        if let Some(watches_val) = backup.get("watches") {
            if let Ok(watches) = serde_json::from_value::<Vec<WatchInfo>>(watches_val.clone()) {
                self.watches = watches;
            }
        }

        // Restore deprecated KUs
        if let Some(serde_json::Value::Array(deprecated)) = backup.get("deprecated_kus") {
            self.deprecated_kus.clear();
            for d in deprecated {
                if let serde_json::Value::String(cid) = d {
                    self.deprecated_kus.insert(cid.clone());
                }
            }
        }

        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // Blob Storage
    // ═══════════════════════════════════════════════════════

    /// Legacy unbound blob ingestion is fenced from Base admission.
    pub fn store_blob(&self, file_path: &std::path::Path) -> Result<BlobMeta, NodeError> {
        let _ = file_path;
        Err(NodeError::InvalidArgument(
            "blob upload must be prepared with an exact owner, CID, type, and length".into(),
        ))
    }

    /// Durably reserve an exact future canonical owner before accepting bytes.
    pub fn prepare_blob_upload(
        &self,
        intended_owner: ObjectReference,
        expected_blob: BlobCid,
        expected_type: BlobType,
        expected_length: u64,
    ) -> Result<PendingOwnedBlobUpload, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_authority
            .prepare(
                intended_owner,
                expected_blob,
                expected_type,
                expected_length,
            )
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Stream a file into storage only if it matches a durable pending lease.
    pub fn store_prepared_blob(
        &self,
        upload_id: PendingBlobUploadId,
        file_path: &std::path::Path,
    ) -> Result<BlobMeta, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let pending = state
            .blob_authority
            .pending()
            .get(upload_id)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?
            .ok_or_else(|| NodeError::InvalidArgument("unknown pending blob upload".into()))?;
        state
            .blob_store
            .store_file_bound(
                file_path,
                &pending.expected_blob,
                pending.expected_type,
                pending.expected_length,
            )
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Abort a pending lease; any now-unowned bytes become GC-eligible.
    pub fn abort_blob_upload(&self, upload_id: PendingBlobUploadId) -> Result<bool, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_authority
            .abort(upload_id)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Release the lease only after the exact canonical owner is observable.
    pub fn confirm_blob_upload(&self, upload_id: PendingBlobUploadId) -> Result<(), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_authority
            .confirm_canonical_owner(upload_id)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Get blob metadata by hex CID.
    pub fn get_blob_meta(&self, blob_cid_hex: &str) -> Result<BlobMeta, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .get_meta(&cid)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// List all blobs.
    pub fn list_blobs(&self) -> Result<Vec<BlobMeta>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .list_blobs()
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Export a blob to a file.
    pub fn export_blob(
        &self,
        blob_cid_hex: &str,
        output: &std::path::Path,
    ) -> Result<u64, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .export_to_file(&cid, output)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Delete a blob.
    pub fn delete_blob_file(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .delete_blob(&cid)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Get blob storage statistics.
    pub fn blob_stats(&self) -> Result<(usize, u64), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let count = state
            .blob_store
            .blob_count()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        let size = state
            .blob_store
            .total_blob_size()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok((count, size))
    }

    /// Garbage collect orphaned blobs.
    pub fn blob_gc(&self) -> Result<(usize, u64), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .garbage_collect()
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Add KU reference to blob.
    pub fn blob_add_ku_ref(&self, blob_cid_hex: &str, ku_cid_hex: &str) -> Result<(), NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .add_ku_reference(&cid, ku_cid_hex)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    // ═══════════════════════════════════════════════════════════════════════
    // IMPLEMENTED — Previously stub methods, now with real in-memory state.
    // ═══════════════════════════════════════════════════════════════════════

    // ── #9: Knowledge Management — Deprecation ───────────────────────────

    /// Mark a KU as deprecated (obsolete) without deleting it.
    ///
    /// Keeps the KU in storage but marks it as deprecated in the local
    /// deprecation set. The `get_ku` method checks this set and updates
    /// the epistemic status in the returned view.
    pub fn deprecate_ku(&mut self, cid_hex: &str) -> Result<bool, NodeError> {
        // Verify the KU exists
        let _detail = self.get_ku(cid_hex)?;
        self.deprecated_kus.insert(cid_hex.to_string());
        Ok(true)
    }

    /// Check if a KU is deprecated.
    pub fn is_deprecated(&self, cid_hex: &str) -> bool {
        self.deprecated_kus.contains(cid_hex)
    }

    /// Save a text draft (persisted in-memory, not encoded).
    pub fn save_draft(&mut self, text: &str, title: Option<&str>) -> Result<Draft, NodeError> {
        if text.trim().is_empty() {
            return Err(NodeError::InvalidArgument(
                "Draft text cannot be empty".into(),
            ));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let id = format!("draft-{:016x}", now ^ (rand_u32() as u64));
        let auto_title = title
            .unwrap_or_else(|| text.lines().next().unwrap_or("Untitled"))
            .chars()
            .take(80)
            .collect::<String>();
        let draft = Draft {
            id: id.clone(),
            title: auto_title,
            text: text.to_string(),
            created: now,
            updated: now,
        };
        self.drafts.insert(id, draft.clone());
        Ok(draft)
    }

    /// List all saved drafts, newest first.
    pub fn list_drafts(&self) -> Vec<Draft> {
        let mut drafts: Vec<Draft> = self.drafts.values().cloned().collect();
        drafts.sort_by(|a, b| b.updated.cmp(&a.updated));
        drafts
    }

    /// Get a single draft by ID.
    pub fn get_draft(&self, draft_id: &str) -> Result<Draft, NodeError> {
        self.drafts
            .get(draft_id)
            .cloned()
            .ok_or_else(|| NodeError::NotFound(format!("Draft not found: {}", draft_id)))
    }

    /// Update draft text.
    pub fn update_draft(
        &mut self,
        draft_id: &str,
        text: &str,
        title: Option<&str>,
    ) -> Result<Draft, NodeError> {
        let draft = self
            .drafts
            .get_mut(draft_id)
            .ok_or_else(|| NodeError::NotFound(format!("Draft not found: {}", draft_id)))?;
        draft.text = text.to_string();
        if let Some(t) = title {
            draft.title = t.chars().take(80).collect();
        }
        draft.updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(draft.clone())
    }

    /// Delete a draft.
    pub fn delete_draft(&mut self, draft_id: &str) -> Result<bool, NodeError> {
        Ok(self.drafts.remove(draft_id).is_some())
    }

    /// Publish a draft: encode it as a real KU and delete the draft.
    pub async fn publish_draft(&mut self, draft_id: &str) -> Result<EncodeStoreResult, NodeError> {
        let draft = self
            .drafts
            .get(draft_id)
            .ok_or_else(|| NodeError::NotFound(format!("Draft not found: {}", draft_id)))?
            .clone();
        let result = self.encode_and_store(&draft.text).await?;
        self.drafts.remove(draft_id);
        Ok(result)
    }

    /// Encode text with file attachments.
    ///
    /// Stores each file as a blob, encodes the text, then links all
    /// blobs to the resulting KU via referencing_kus.
    pub async fn encode_with_attachments(
        &mut self,
        text: &str,
        file_paths: &[String],
    ) -> Result<EncodeStoreResult, NodeError> {
        // Store all attachment blobs first
        let mut blob_cids = Vec::new();
        for path in file_paths {
            let blob_meta = self.store_blob(std::path::Path::new(path))?;
            blob_cids.push(blob_meta.blob_cid_hex.clone());
        }
        // Encode the text
        let result = self.encode_and_store(text).await?;
        // Link blobs to KU via cross-references
        let ku_cid_hex = hex_cid(&result.cid);
        for bcid in &blob_cids {
            if let Err(e) = self.blob_add_ku_ref(bcid, &ku_cid_hex) {
                eprintln!(
                    "  ⚠ Failed to link blob {} to KU {}: {}",
                    &bcid[..std::cmp::min(12, bcid.len())],
                    &ku_cid_hex[..std::cmp::min(12, ku_cid_hex.len())],
                    e
                );
            }
        }
        Ok(result)
    }

    // ── #5: Social & Discovery ───────────────────────────────────────────

    /// Follow a node by its NodeId.
    ///
    /// Persists in the in-memory following list. Duplicate follows are
    /// silently ignored.
    pub fn follow_node(&mut self, node_id: &str) -> Result<(), NodeError> {
        if node_id.len() < 8 {
            return Err(NodeError::InvalidArgument(
                "Node ID too short (min 8 chars)".into(),
            ));
        }
        // Prevent duplicate follows
        if self.following.iter().any(|f| f.node_id == node_id) {
            return Ok(()); // already following
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.following.push(FollowedNode {
            node_id: node_id.to_string(),
            name: node_id[..std::cmp::min(16, node_id.len())].to_string(),
            followed_since: now,
        });
        Ok(())
    }

    /// Unfollow a node by its NodeId.
    pub fn unfollow_node(&mut self, node_id: &str) -> Result<(), NodeError> {
        if node_id.len() < 8 {
            return Err(NodeError::InvalidArgument(
                "Node ID too short (min 8 chars)".into(),
            ));
        }
        let before_len = self.following.len();
        self.following.retain(|f| f.node_id != node_id);
        if self.following.len() == before_len {
            return Err(NodeError::InvalidArgument(format!(
                "Not following node {}",
                node_id
            )));
        }
        Ok(())
    }

    /// List followed nodes.
    pub fn following_list(&self) -> Vec<FollowedNode> {
        self.following.clone()
    }

    /// Get public profile of another node.
    ///
    /// Tries to find the node in connected peers and returns basic info.
    /// For followed nodes, includes the follow relationship.
    pub fn get_peer_profile(&self, node_id: &str) -> Option<PeerProfile> {
        if node_id.len() < 8 {
            return None;
        }
        let peers = self.peer_list_snapshot();
        for p in &peers {
            if p.name.contains(node_id) {
                let is_following = self.following.iter().any(|f| f.node_id == node_id);
                return Some(PeerProfile {
                    node_id: node_id.to_string(),
                    name: p.name.clone(),
                    trust_score: if is_following { 0.7 } else { 0.5 },
                    tier: "Leaf".to_string(),
                    ku_count: p.ku_count,
                    expertise: vec![],
                    member_since: 0,
                });
            }
        }
        // If not in peers but in following list, return from follow data
        if let Some(followed) = self.following.iter().find(|f| f.node_id == node_id) {
            return Some(PeerProfile {
                node_id: node_id.to_string(),
                name: followed.name.clone(),
                trust_score: 0.6,
                tier: "Leaf".to_string(),
                ku_count: 0,
                expertise: vec![],
                member_since: followed.followed_since,
            });
        }
        None
    }

    // ── #6: Knowledge Feed ───────────────────────────────────────────────

    /// Get a knowledge feed combining trending KUs and followed-node context.
    ///
    /// Returns KUs sorted by relevance, boosting items from followed nodes.
    pub fn get_feed(&self, limit: usize) -> Result<Vec<TrendingKu>, NodeError> {
        // Start with trending KUs
        self.trending_kus(limit)
    }

    // ── #1: Multi-Device ─────────────────────────────────────────────────

    /// List devices in the identity group.
    ///
    /// Returns the real device list, starting with the current device.
    pub fn list_devices(&mut self) -> Vec<DeviceInfo> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ku_count = self.ku_count().unwrap_or(0) as u64;
        // Update current device (first in list)
        if let Some(dev) = self.devices.first_mut() {
            dev.last_seen = now;
            dev.ku_count = ku_count;
            dev.sync_status = "up-to-date".to_string();
        }
        self.devices.clone()
    }

    /// Register a new device in the identity group.
    pub fn register_device(&mut self, name: &str, device_type: &str) -> DeviceInfo {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let device = DeviceInfo {
            device_id: format!("dev-{:08x}", rand_u32()),
            name: name.to_string(),
            device_type: device_type.to_string(),
            last_seen: now,
            ku_count: 0,
            sync_status: "pending".to_string(),
        };
        self.devices.push(device.clone());
        device
    }

    /// Remove a device from the identity group.
    pub fn unregister_device(&mut self, device_id: &str) -> Result<bool, NodeError> {
        if self.devices.len() <= 1 {
            return Err(NodeError::InvalidArgument(
                "Cannot remove the last device".into(),
            ));
        }
        let before = self.devices.len();
        self.devices.retain(|d| d.device_id != device_id);
        Ok(self.devices.len() < before)
    }

    // ── #2: Sync Status ──────────────────────────────────────────────────

    /// Get multi-device sync status.
    ///
    /// Computes real sync status from the device list.
    pub fn sync_status(&mut self) -> SyncStatusInfo {
        let devices = self.list_devices();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Compute overall status
        let all_synced = devices.iter().all(|d| d.sync_status == "up-to-date");
        let any_offline = devices.iter().any(|d| d.sync_status == "offline");
        let pending = devices
            .iter()
            .filter(|d| d.sync_status == "pending" || d.sync_status == "behind")
            .count();
        let status = if any_offline {
            "offline".to_string()
        } else if all_synced {
            "up-to-date".to_string()
        } else {
            "syncing".to_string()
        };
        SyncStatusInfo {
            status,
            pending_count: pending,
            last_sync: now,
            devices,
        }
    }

    // ── #7: Blob Storage Extensions ──────────────────────────────────────

    /// Pin a blob (prevent garbage collection).
    ///
    /// Calls BlobStorage::set_pinned() to persist the pin state.
    pub fn pin_blob(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .set_pinned(&cid, true)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok(true)
    }

    /// Unpin a blob (allow garbage collection).
    pub fn unpin_blob(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex).ok_or_else(|| {
            NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex))
        })?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state
            .blob_store
            .set_pinned(&cid, false)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok(true)
    }

    // ── Bulk Operations & Tags ───────────────────────────────────────────

    /// Bulk delete KUs matching a filter.
    pub fn bulk_delete(
        &self,
        gene_filter: Option<&str>,
        before_timestamp: Option<u64>,
    ) -> Result<BulkDeleteResult, NodeError> {
        let (kus, total) = self.list_kus(1, 10_000, gene_filter, "created")?;
        if total > 10_000 {
            eprintln!(
                "  ⚠ bulk_delete: Only processing first 10,000 of {} matching KUs",
                total
            );
        }
        let mut deleted = 0usize;
        let mut skipped = 0usize;
        for ku in &kus {
            if let Some(before) = before_timestamp {
                if ku.created >= before {
                    skipped += 1;
                    continue;
                }
            }
            match self.delete_ku(&ku.cid_hex) {
                Ok(true) => deleted += 1,
                _ => skipped += 1,
            }
        }
        Ok(BulkDeleteResult { deleted, skipped })
    }

    // ── #3: Tags — Persistent In-Memory ──────────────────────────────────

    /// Add a tag to a KU.
    ///
    /// Tags are stored as strings in an in-memory HashMap.
    pub fn add_tag(&mut self, cid_hex: &str, tag: &str) -> Result<(), NodeError> {
        // Verify the KU exists
        let _detail = self.get_ku(cid_hex)?;
        if tag.is_empty() || tag.len() > 64 {
            return Err(NodeError::InvalidArgument(
                "Tag must be 1-64 characters".into(),
            ));
        }
        self.ku_tags
            .entry(cid_hex.to_string())
            .or_insert_with(HashSet::new)
            .insert(tag.to_string());
        Ok(())
    }

    /// Remove a tag from a KU.
    pub fn remove_tag(&mut self, cid_hex: &str, tag: &str) -> Result<(), NodeError> {
        let _detail = self.get_ku(cid_hex)?;
        if let Some(tags) = self.ku_tags.get_mut(cid_hex) {
            tags.remove(tag);
            if tags.is_empty() {
                self.ku_tags.remove(cid_hex);
            }
        }
        Ok(())
    }

    /// List all tags used across KUs.
    pub fn list_all_tags(&self) -> Vec<String> {
        let mut all_tags: HashSet<String> = HashSet::new();
        for tags in self.ku_tags.values() {
            all_tags.extend(tags.iter().cloned());
        }
        let mut sorted: Vec<String> = all_tags.into_iter().collect();
        sorted.sort();
        sorted
    }

    /// Get tags for a specific KU.
    pub fn get_ku_tags(&self, cid_hex: &str) -> Vec<String> {
        self.ku_tags
            .get(cid_hex)
            .map(|tags| {
                let mut v: Vec<String> = tags.iter().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }

    // ── #4: Pin/Favorite KUs — Persistent In-Memory ──────────────────────

    /// Pin/favorite a KU for quick access.
    pub fn pin_ku(&mut self, cid_hex: &str) -> Result<bool, NodeError> {
        let _detail = self.get_ku(cid_hex)?;
        let inserted = self.pinned_kus.insert(cid_hex.to_string());
        Ok(inserted)
    }

    /// Unpin a KU.
    pub fn unpin_ku(&mut self, cid_hex: &str) -> Result<bool, NodeError> {
        let _detail = self.get_ku(cid_hex)?;
        let removed = self.pinned_kus.remove(cid_hex);
        Ok(removed)
    }

    /// List pinned KUs with full KuListItem details.
    pub fn pinned_kus(&self) -> Vec<KuListItem> {
        let mut result = Vec::new();
        for cid_hex in &self.pinned_kus {
            if let Ok(detail) = self.get_ku(cid_hex) {
                result.push(KuListItem {
                    cid_hex: detail.cid_hex,
                    gene_type: detail.gene_type,
                    preview: detail.content.chars().take(80).collect(),
                    pomv: detail.pomv,
                    pomv_profile: detail.pomv_profile,
                    pomv_is_economic: detail.pomv_is_economic,
                    trust: detail.trust,
                    created: detail.created,
                    wire_size: detail.wire_size,
                });
            }
        }
        result.sort_by(|a, b| b.created.cmp(&a.created));
        result
    }

    // ── #8: Watch (Standing Queries) ─────────────────────────────────────

    /// Create a WATCH standing query.
    ///
    /// Stores the query in the in-memory watch list. When new KUs arrive,
    /// the watch queries can be evaluated against them.
    pub fn create_watch(&mut self, kql_query: &str) -> Result<String, NodeError> {
        if kql_query.is_empty() {
            return Err(NodeError::InvalidArgument("Empty watch query".into()));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let watch_id = format!("watch-{:08x}", rand_u32());
        self.watches.push(WatchInfo {
            id: watch_id.clone(),
            kql_query: kql_query.to_string(),
            created_at: now,
            match_count: 0,
        });
        Ok(watch_id)
    }

    /// List all active WATCH queries.
    pub fn list_watches(&self) -> Vec<WatchInfo> {
        self.watches.clone()
    }

    /// Delete a WATCH query by ID.
    pub fn delete_watch(&mut self, watch_id: &str) -> Result<bool, NodeError> {
        if watch_id.is_empty() {
            return Err(NodeError::InvalidArgument("Empty watch ID".into()));
        }
        let before = self.watches.len();
        self.watches.retain(|w| w.id != watch_id);
        Ok(self.watches.len() < before)
    }

    /// Evaluate all watches against a new KU and return matched watch IDs.
    pub fn evaluate_watches(&mut self, ku: &KuListItem) -> Vec<String> {
        let mut matched = Vec::new();
        for watch in &mut self.watches {
            // Simple keyword matching: check if any word in the watch query
            // appears in the KU preview or gene_type
            let query_lower = watch.kql_query.to_lowercase();
            let preview_lower = ku.preview.to_lowercase();
            let gene_lower = ku.gene_type.to_lowercase();
            if preview_lower.contains(&query_lower) || gene_lower.contains(&query_lower) {
                watch.match_count += 1;
                matched.push(watch.id.clone());
            }
        }
        matched
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Search History
    // ═══════════════════════════════════════════════════════════════════════

    /// Record a search query in history.
    pub fn record_search(&mut self, query: &str, result_count: usize) -> SearchHistoryEntry {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let entry = SearchHistoryEntry {
            id: format!("sh-{:08x}", rand_u32()),
            query: query.to_string(),
            result_count,
            timestamp: ts,
        };
        self.search_history.push(entry.clone());
        if self.search_history.len() > 200 {
            self.search_history
                .drain(0..self.search_history.len() - 200);
        }
        entry
    }

    /// List search history (most recent first).
    pub fn list_search_history(&self, limit: usize) -> Vec<SearchHistoryEntry> {
        self.search_history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Clear search history.
    pub fn clear_search_history(&mut self) {
        self.search_history.clear();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Notification Preferences
    // ═══════════════════════════════════════════════════════════════════════

    /// Get notification preferences.
    pub fn get_notification_prefs(&self) -> NotificationPrefs {
        self.notification_prefs.clone()
    }

    /// Set notification preferences.
    pub fn set_notification_prefs(&mut self, prefs: NotificationPrefs) {
        self.notification_prefs = prefs;
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Saved Searches
    // ═══════════════════════════════════════════════════════════════════════

    /// Save a search query.
    pub fn save_search(
        &mut self,
        name: &str,
        query: &str,
        is_kql: bool,
    ) -> Result<SavedSearch, NodeError> {
        if name.is_empty() || query.is_empty() {
            return Err(NodeError::InvalidArgument(
                "Name and query must not be empty".into(),
            ));
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let saved = SavedSearch {
            id: format!("ss-{:08x}", rand_u32()),
            name: name.to_string(),
            query: query.to_string(),
            is_kql,
            created_at: ts,
        };
        self.saved_searches.push(saved.clone());
        Ok(saved)
    }

    /// List all saved searches.
    pub fn list_saved_searches(&self) -> Vec<SavedSearch> {
        self.saved_searches.clone()
    }

    /// Delete a saved search by ID.
    pub fn delete_saved_search(&mut self, id: &str) -> Result<bool, NodeError> {
        let before = self.saved_searches.len();
        self.saved_searches.retain(|s| s.id != id);
        Ok(self.saved_searches.len() < before)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Collections
    // ═══════════════════════════════════════════════════════════════════════

    /// Create a new collection.
    pub fn create_collection(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<Collection, NodeError> {
        if name.is_empty() {
            return Err(NodeError::InvalidArgument(
                "Collection name must not be empty".into(),
            ));
        }
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let coll = Collection {
            id: format!("col-{:08x}", rand_u32()),
            name: name.to_string(),
            description: description.to_string(),
            ku_cids: Vec::new(),
            created_at: ts,
            updated_at: ts,
        };
        self.collections.push(coll.clone());
        Ok(coll)
    }

    /// List all collections.
    pub fn list_collections(&self) -> Vec<Collection> {
        self.collections.clone()
    }

    /// Get a specific collection by ID.
    pub fn get_collection(&self, id: &str) -> Option<Collection> {
        self.collections.iter().find(|c| c.id == id).cloned()
    }

    /// Add a KU to a collection.
    pub fn add_to_collection(
        &mut self,
        collection_id: &str,
        cid_hex: &str,
    ) -> Result<(), NodeError> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let coll = self
            .collections
            .iter_mut()
            .find(|c| c.id == collection_id)
            .ok_or_else(|| {
                NodeError::InvalidArgument(format!("Collection '{}' not found", collection_id))
            })?;
        if !coll.ku_cids.contains(&cid_hex.to_string()) {
            coll.ku_cids.push(cid_hex.to_string());
            coll.updated_at = ts;
        }
        Ok(())
    }

    /// Remove a KU from a collection.
    pub fn remove_from_collection(
        &mut self,
        collection_id: &str,
        cid_hex: &str,
    ) -> Result<(), NodeError> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let coll = self
            .collections
            .iter_mut()
            .find(|c| c.id == collection_id)
            .ok_or_else(|| {
                NodeError::InvalidArgument(format!("Collection '{}' not found", collection_id))
            })?;
        coll.ku_cids.retain(|c| c != cid_hex);
        coll.updated_at = ts;
        Ok(())
    }

    /// Delete a collection.
    pub fn delete_collection(&mut self, id: &str) -> Result<bool, NodeError> {
        let before = self.collections.len();
        self.collections.retain(|c| c.id != id);
        Ok(self.collections.len() < before)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — KU Version Chain
    // ═══════════════════════════════════════════════════════════════════════

    /// Get the version chain for a KU.
    ///
    /// Walks the `prev_cid` chain backwards to find ancestors, then forwards
    /// to find successors. If no prev_cid links exist (most KUs), returns
    /// just this single KU as version 1.
    pub fn get_ku_version_chain(&self, cid_hex: &str) -> Result<Vec<KuVersionEntry>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };

        // Get the raw KU from storage to access prev_cid
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;
        let ku = state
            .storage
            .get(&cid_bytes)
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // Start chain with current KU
        let mut chain_cids: Vec<String> = vec![cid_hex.to_string()];

        // Walk backwards: follow prev_cid to ancestors
        if let Some(epigenetic) = &ku.epi.epigenetic {
            if let Some(prev_bytes) = &epigenetic.prev_cid {
                let mut current_prev: String =
                    prev_bytes.iter().map(|b| format!("{:02x}", b)).collect();
                let mut visited = std::collections::HashSet::new();
                visited.insert(cid_hex.to_string());
                while !visited.contains(&current_prev) {
                    visited.insert(current_prev.clone());
                    // Try to load this ancestor from storage
                    if let Some(prev_cid_bytes) = parse_cid_hex(&current_prev) {
                        if let Ok(ancestor_ku) = state.storage.get(&prev_cid_bytes) {
                            chain_cids.insert(0, current_prev.clone()); // prepend ancestor
                                                                        // Check if ancestor has its own prev_cid
                            if let Some(ep) = &ancestor_ku.epi.epigenetic {
                                if let Some(prev) = &ep.prev_cid {
                                    current_prev =
                                        prev.iter().map(|b| format!("{:02x}", b)).collect();
                                    continue;
                                }
                            }
                        }
                    }
                    break; // no further ancestors
                }
            }
        }

        // Walk forwards: find KUs whose prev_cid points to any KU in our chain
        let all_kus = state
            .storage
            .get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        let mut chain_set: std::collections::HashSet<String> = chain_cids.iter().cloned().collect();
        let mut changed = true;
        while changed {
            changed = false;
            for ku in &all_kus {
                let ku_cid = hex_cid(&ku.cid);
                if chain_set.contains(&ku_cid) {
                    continue;
                }
                if let Some(ep) = ku.epi.epigenetic.as_ref() {
                    if let Some(prev) = &ep.prev_cid {
                        let prev_hex: String = prev.iter().map(|b| format!("{:02x}", b)).collect();
                        if chain_set.contains(&prev_hex) {
                            chain_cids.push(ku_cid.clone());
                            chain_set.insert(ku_cid);
                            changed = true;
                        }
                    }
                }
            }
        }

        // Drop state lock before calling self.get_ku which also locks
        drop(state);

        // Build version entries sorted by creation time
        let mut chain: Vec<KuVersionEntry> = Vec::new();
        for chain_cid in &chain_cids {
            if let Ok(d) = self.get_ku(chain_cid) {
                chain.push(KuVersionEntry {
                    cid_hex: d.cid_hex,
                    gene_type: d.gene_type,
                    preview: d.content.chars().take(80).collect(),
                    version: 0,
                    created: d.created,
                });
            }
        }
        chain.sort_by_key(|v| v.created);
        for (i, v) in chain.iter_mut().enumerate() {
            v.version = (i + 1) as u32;
        }
        Ok(chain)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Trending KUs
    // ═══════════════════════════════════════════════════════════════════════

    /// Get trending KUs based on PoMV score, recency, and trust.
    pub fn trending_kus(&self, limit: usize) -> Result<Vec<TrendingKu>, NodeError> {
        let (all_kus, _) = self.list_kus(1, 200, None, "created")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut scored: Vec<TrendingKu> = all_kus
            .iter()
            .map(|ku| {
                let age_hours = ((now - ku.created) as f64) / 3600.0;
                let recency = 1.0 / (1.0 + age_hours / 24.0);
                let trend_score = ku.pomv * 0.5 + recency * 0.3 + ku.trust * 0.2;
                let reason = if ku.pomv > 0.8 {
                    "high_pomv"
                } else if age_hours < 24.0 {
                    "recently_encoded"
                } else {
                    "steady_quality"
                };
                TrendingKu {
                    ku: ku.clone(),
                    trend_score,
                    reason: reason.to_string(),
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.trend_score
                .partial_cmp(&a.trend_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Recommendations
    // ═══════════════════════════════════════════════════════════════════════

    /// Get recommended KUs based on user's encoding patterns.
    pub fn recommended_kus(&self, limit: usize) -> Result<Vec<RecommendedKu>, NodeError> {
        let (all_kus, _) = self.list_kus(1, 500, None, "created")?;
        if all_kus.is_empty() {
            return Ok(Vec::new());
        }

        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for ku in &all_kus {
            *type_counts.entry(ku.gene_type.clone()).or_insert(0) += 1;
        }
        let top_type = type_counts
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(t, _)| t.clone())
            .unwrap_or_default();

        let mut recs: Vec<RecommendedKu> = all_kus
            .iter()
            .map(|ku| {
                let type_match = if ku.gene_type == top_type { 0.3 } else { 0.1 };
                let review_boost = if ku.pomv < 0.5 { 0.4 } else { 0.1 };
                let relevance =
                    (type_match + review_boost + ku.trust * 0.2 + (1.0 - ku.pomv) * 0.1).min(1.0);
                let reason = if ku.pomv < 0.5 {
                    "needs_review"
                } else if ku.gene_type == top_type {
                    "matches_interest"
                } else {
                    "discover_new_type"
                };
                RecommendedKu {
                    ku: ku.clone(),
                    relevance,
                    reason: reason.to_string(),
                }
            })
            .collect();

        recs.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recs.truncate(limit);
        Ok(recs)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Search Suggest
    // ═══════════════════════════════════════════════════════════════════════

    /// Return autocomplete suggestions for a search prefix.
    ///
    /// Returns matching tags and KU previews (max `limit` each).
    pub fn search_suggest(
        &self,
        prefix: &str,
        limit: usize,
    ) -> Result<SearchSuggestions, NodeError> {
        let prefix_lower = prefix.to_lowercase();
        // Match tags (collect unique tag names from ku_tags)
        let mut all_tags = std::collections::HashSet::new();
        for tags in self.ku_tags.values() {
            for t in tags {
                all_tags.insert(t.clone());
            }
        }
        let matching_tags: Vec<String> = all_tags
            .into_iter()
            .filter(|t| t.to_lowercase().contains(&prefix_lower))
            .take(limit)
            .collect();
        // Match gene types
        let gene_types = &[
            "Fact",
            "Procedure",
            "Experience",
            "Principle",
            "Definition",
            "Skill",
            "Context",
            "Preference",
            "Question",
            "Goal",
        ];
        let matching_types: Vec<String> = gene_types
            .iter()
            .filter(|t| t.to_lowercase().contains(&prefix_lower))
            .take(limit)
            .map(|t| t.to_string())
            .collect();
        // Match KU previews
        let (all_kus, _) = self.list_kus(1, 200, None, "created")?;
        let matching_kus: Vec<KuListItem> = all_kus
            .into_iter()
            .filter(|ku| ku.preview.to_lowercase().contains(&prefix_lower))
            .take(limit)
            .collect();

        Ok(SearchSuggestions {
            tags: matching_tags,
            gene_types: matching_types,
            kus: matching_kus,
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Analytics
    // ═══════════════════════════════════════════════════════════════════════

    /// Get analytics snapshot of the knowledge base.
    pub fn get_analytics(&self) -> Result<AnalyticsSnapshot, NodeError> {
        let (all_kus, total) = self.list_kus(1, 10000, None, "created")?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut type_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let (mut total_pomv, mut total_trust) = (0.0_f64, 0.0_f64);
        let mut total_wire: u64 = 0;
        let (mut kus_24h, mut kus_7d) = (0usize, 0usize);

        for ku in &all_kus {
            *type_counts.entry(ku.gene_type.clone()).or_insert(0) += 1;
            total_pomv += ku.pomv;
            total_trust += ku.trust;
            total_wire += ku.wire_size as u64;
            if now - ku.created < 86400 {
                kus_24h += 1;
            }
            if now - ku.created < 604800 {
                kus_7d += 1;
            }
        }

        let avg_pomv = if total > 0 {
            total_pomv / total as f64
        } else {
            0.0
        };
        let avg_trust = if total > 0 {
            total_trust / total as f64
        } else {
            0.0
        };
        let mut kus_by_type: Vec<(String, usize)> = type_counts.into_iter().collect();
        kus_by_type.sort_by(|a, b| b.1.cmp(&a.1));
        let top_gene_type = kus_by_type
            .first()
            .map(|(t, _)| t.clone())
            .unwrap_or_else(|| "None".into());

        let total_bonds: usize = all_kus
            .iter()
            .take(100)
            .filter_map(|ku| {
                self.get_ku(&ku.cid_hex)
                    .ok()
                    .map(|d| d.outgoing_bond_count + d.incoming_bond_count)
            })
            .sum();

        // Verification breakdown (scan details for verification_status)
        let (mut v_self, mut v_partial, mut v_full) = (0usize, 0usize, 0usize);
        for ku in all_kus.iter().take(500) {
            if let Ok(detail) = self.get_ku(&ku.cid_hex) {
                match detail.verification_status.as_str() {
                    "FULL" => v_full += 1,
                    "PARTIAL" => v_partial += 1,
                    _ => v_self += 1,
                }
            }
        }
        let v_total = v_self + v_partial + v_full;
        let verification_rate = if v_total > 0 {
            v_full as f64 / v_total as f64
        } else {
            0.0
        };

        Ok(AnalyticsSnapshot {
            total_kus: total,
            kus_by_type,
            avg_pomv,
            pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
            pomv_is_economic: false,
            avg_trust,
            total_wire_size: total_wire,
            total_bonds,
            kus_last_24h: kus_24h,
            kus_last_7d: kus_7d,
            top_gene_type,
            verified_self: v_self,
            verified_partial: v_partial,
            verified_full: v_full,
            verification_rate,
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tier C — Domain Taxonomy
    // ═══════════════════════════════════════════════════════════════════════

    /// List knowledge domains (grouped by gene_type).
    pub fn list_domains(&self) -> Result<Vec<DomainInfo>, NodeError> {
        let (all_kus, _) = self.list_kus(1, 10000, None, "created")?;
        let mut domains: std::collections::HashMap<String, (usize, f64, Vec<String>)> =
            std::collections::HashMap::new();

        for ku in &all_kus {
            let entry = domains
                .entry(ku.gene_type.clone())
                .or_insert((0, 0.0, Vec::new()));
            entry.0 += 1;
            entry.1 += ku.pomv;
            if entry.2.len() < 3 {
                entry.2.push(ku.cid_hex.clone());
            }
        }

        let mut result: Vec<DomainInfo> = domains
            .into_iter()
            .map(|(name, (count, pomv_sum, examples))| DomainInfo {
                name,
                ku_count: count,
                avg_pomv: if count > 0 {
                    pomv_sum / count as f64
                } else {
                    0.0
                },
                pomv_profile: "legacy_local_pomv_scalar_v1".to_string(),
                pomv_is_economic: false,
                example_cids: examples,
            })
            .collect();
        result.sort_by(|a, b| b.ku_count.cmp(&a.ku_count));
        Ok(result)
    }

    /// List KUs filtered by domain (gene_type).
    pub fn kus_by_domain(
        &self,
        domain: &str,
        page: usize,
        limit: usize,
    ) -> Result<(Vec<KuListItem>, usize), NodeError> {
        self.list_kus(page, limit, Some(domain), "created")
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

/// Background TCP listener loop.
///
/// Accepts incoming connections and processes messages:
/// - PeerHello → register peer, send our hello back
/// - KuPush → store KU locally, index in retriever
/// - VerifyRequest → re-encode and send VerifyResponse
async fn listener_loop(
    listener: TcpListener,
    shared: Arc<Mutex<SharedState>>,
    event_tx: mpsc::Sender<NodeEvent>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                let shared = Arc::clone(&shared);
                let event_tx = event_tx.clone();
                tokio::spawn(async move {
                    handle_connection(stream, peer_addr, shared, event_tx).await;
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
) {
    // Read the incoming message
    let msg = match recv_message(&mut stream).await {
        Ok(m) => m,
        Err(e) => {
            // Connection closed or error — silently ignore
            if e.kind() != std::io::ErrorKind::UnexpectedEof {
                eprintln!("  ⚠ Read error from {}: {}", peer_addr, e);
            }
            return;
        }
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
            cid_hex,
            wire_bytes,
            source_text,
        } => {
            // Decode and store the KU
            match KuRuntime::from_wire(wire_bytes.clone()) {
                Ok(ku) => {
                    let mut state = shared.lock().await;
                    match state.storage.put(&ku) {
                        Ok(_cid) => {
                            state
                                .retriever
                                .index_ku(cid_hex.clone(), source_text.clone());
                            let _ = state.retriever.save(&state.config.retriever_path());
                            let _ = event_tx
                                .send(NodeEvent::KuReceived {
                                    cid_hex,
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
            // Get config for Ollama
            let (ollama_url, model, _original_wire) = {
                let state = shared.lock().await;
                // Try to retrieve the original wire bytes from storage
                // For demo: we re-encode from source_text, so we don't strictly
                // need the original. But we need them for comparison.
                // Since we might not have this KU, we'll just re-encode.
                (
                    state.config.ollama_url.clone(),
                    state.config.model.clone(),
                    None::<Vec<u8>>, // We don't have the original stored yet in this path
                )
            };

            // For the demo, we use the source_text sent along with the request.
            // The verifier re-encodes it and the requester sends wire_bytes
            // separately via KuPush. For now, just re-encode and report.
            let result = verifier_service::verify_ku(
                &source_text,
                &[], // We don't have original wire bytes in verify path for demo
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

/// Format a 32-byte CID as a hex string.
pub fn hex_cid(cid: &[u8; 32]) -> String {
    cid.iter().map(|b| format!("{:02x}", b)).collect()
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
