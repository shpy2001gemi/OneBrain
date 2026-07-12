//! OneBrain Node — ties together all subsystems.
//!
//! The `OneBrainNode` is the top-level runtime struct that owns the
//! Mediator, AI backend, concept dictionary, persistent storage,
//! anti-gaming guard, and peer networking.

use crate::anti_gaming_guard::AntiGamingGuard;
use crate::config::NodeConfig;
use crate::error::NodeError;
use crate::network::{NetMessage, NodeEvent, PeerInfo, send_message, recv_message};
use crate::peer_manager::PeerManager;
use crate::verifier_service;

use ku_ai::OllamaBackend;
use ku_core::text_parser::{ConceptDict, default_dict};
use ku_core::KuRuntime;
use ku_encoder::{AiEncoder, EncoderConfig, EncodingResult};
use ku_kql::storage::KuStorage;
use ku_kql::blob_storage::BlobStorage;
use ku_core::blob_store::{BlobCid, BlobMeta};
use ku_mediator::mediator::{Mediator, MediatorConfig};
use ku_mediator::input::UserInput;
use ku_mediator::retriever::KuRetriever;
use crate::types::*;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

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
}

/// Shared state accessible from both the REPL and background tasks.
pub struct SharedState {
    /// Persistent KU storage.
    pub storage: KuStorage,
    /// Persistent blob storage.
    pub blob_store: BlobStorage,
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
        // Create chat backend
        let chat_backend = OllamaBackend::new(
            &config.ollama_url,
            &config.model,
            "nomic-embed-text",
            120,
        ).map_err(|e| NodeError::Ai(e))?;

        // Create encoder backend for mediator
        let mediator_encoder_backend = OllamaBackend::new(
            &config.ollama_url,
            &config.model,
            "nomic-embed-text",
            120,
        ).map_err(|e| NodeError::Ai(e))?;

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

        // Open blob storage
        let blob_store = BlobStorage::open(&config.blob_storage_path())
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // Load or create retriever index
        let retriever = KuRetriever::load(&config.retriever_path())
            .map_err(|e| NodeError::Storage(format!("Retriever load failed: {}", e)))?;

        // Report startup KU count
        let ku_count = storage.count()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        if ku_count > 0 {
            eprintln!("  ✓ Storage contains {} KU(s)", ku_count);
            // Note: retriever index is loaded from disk (retriever_path),
            // so already populated from previous sessions' index_ku() calls.
            eprintln!("  ✓ Retriever index loaded ({} entries)", retriever.index_size());
        }

        // Create anti-gaming guard
        let guard = AntiGamingGuard::new();

        // Create shared state
        let shared = Arc::new(Mutex::new(SharedState {
            storage,
            blob_store,
            retriever,
            peer_manager: PeerManager::new(),
            config: config.clone(),
        }));

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel::<NodeEvent>(256);

        Ok(Self {
            config,
            mediator,
            guard,
            dict,
            shared,
            event_rx,
            event_tx,
            listener_addr: None,
        })
    }

    /// Start the TCP listener and spawn the background accept loop.
    ///
    /// Returns the local address the listener is bound to.
    pub async fn start_network(&mut self) -> Result<SocketAddr, NodeError> {
        let bind_addr: SocketAddr = ([0, 0, 0, 0], self.config.port).into();
        let listener = TcpListener::bind(bind_addr).await
            .map_err(|e| NodeError::Network(format!("Failed to bind TCP on {}: {}", bind_addr, e)))?;

        let local_addr = listener.local_addr()
            .map_err(|e| NodeError::Network(format!("Failed to get local addr: {}", e)))?;
        self.listener_addr = Some(local_addr);

        // Spawn background listener task
        let shared = Arc::clone(&self.shared);
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            listener_loop(listener, shared, event_tx).await;
        });

        Ok(local_addr)
    }

    /// Connect to a seed peer and exchange handshake.
    pub async fn connect_to_seed(&self, addr: SocketAddr) -> Result<(), NodeError> {
        let mut stream = TcpStream::connect(addr).await
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
        send_message(&mut stream, &hello).await
            .map_err(|e| NodeError::Network(format!("Failed to send hello: {}", e)))?;

        // Receive peer's hello
        match recv_message(&mut stream).await {
            Ok(NetMessage::PeerHello { name, port: _, ku_count: peer_ku_count }) => {
                let peer_info = PeerInfo {
                    name: name.clone(),
                    addr,
                    ku_count: peer_ku_count,
                };
                let mut state = self.shared.lock().await;
                state.peer_manager.add_peer(peer_info);
                eprintln!("  ✓ Connected to peer '{}' at {} ({} KUs)", name, addr, peer_ku_count);
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
    pub async fn broadcast_ku(&self, cid_hex: &str, wire_bytes: &[u8], source_text: &str) {
        let addrs = {
            let state = self.shared.lock().await;
            state.peer_manager.known_addrs()
        };

        if addrs.is_empty() {
            return;
        }

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
                            ).await {
                                Ok(Ok(NetMessage::VerifyResponse { agreement_score, verified, .. })) => {
                                    let _ = event_tx.send(NodeEvent::VerifyResult {
                                        cid_hex,
                                        agreement_score,
                                        verified,
                                        from: format!("{}", addr),
                                    }).await;
                                }
                                _ => {
                                    let _ = event_tx.send(NodeEvent::Notification(
                                        format!("  ⚠ Verification timeout from {}", addr),
                                    )).await;
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
    pub async fn encode_and_store(
        &mut self,
        text: &str,
    ) -> Result<EncodeStoreResult, NodeError> {
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
        let _ = self.event_tx.send(NodeEvent::EncodeProgress {
            step: 1, total_steps, message: "Rate limit check...".into(),
        }).await;
        self.guard.check_rate_limit()
            .map_err(|e| NodeError::Pipeline(e))?;

        // 2. Create a fresh encoder backend (OllamaBackend doesn't impl Clone)
        send_progress(2, format!("Creating AI encoder (model: {})...", self.config.model));
        let _ = self.event_tx.send(NodeEvent::EncodeProgress {
            step: 2, total_steps, message: format!("Creating AI encoder (model: {})...", self.config.model),
        }).await;
        let encoder_backend = OllamaBackend::new(
            &self.config.ollama_url,
            &self.config.model,
            "nomic-embed-text",
            300,
        ).map_err(|e| NodeError::Ai(e))?;

        let encoder = AiEncoder::new(
            Box::new(encoder_backend),
            self.dict.clone(),
            EncoderConfig::default(),
        );

        // 3. Encode via AI (this is the slow step)
        send_progress(3, "AI generating tool calls (this may take a while)...".into());
        let _ = self.event_tx.send(NodeEvent::EncodeProgress {
            step: 3, total_steps, message: "AI generating tool calls (this may take a while)...".into(),
        }).await;
        let encoding_result: EncodingResult = encoder.encode(text).await
            .map_err(NodeError::Encoder)?;

        if encoding_result.wire_bytes.is_empty() {
            return Err(NodeError::Pipeline("Encoding produced no KUs".into()));
        }

        // 4. Process the first (primary) KU
        send_progress(4, format!("Processing KU ({} bytes wire data)...", encoding_result.wire_bytes[0].len()));
        let _ = self.event_tx.send(NodeEvent::EncodeProgress {
            step: 4, total_steps, message: format!("Processing KU ({} bytes wire data)...", encoding_result.wire_bytes[0].len()),
        }).await;
        let wire_bytes = &encoding_result.wire_bytes[0];

        // 4a. Decode wire bytes → KuRuntime
        let ku = KuRuntime::from_wire(wire_bytes.clone())
            .map_err(|e| NodeError::Pipeline(format!("KuRuntime decode failed: {}", e)))?;

        let instruction_count = ku.dna.instructions.len();

        // 4b. Quality gate check
        self.guard.check_quality(wire_bytes, instruction_count)
            .map_err(|e| NodeError::Pipeline(e))?;

        // 4c-4e. Store, index, record (using shared state)
        send_progress(5, "Storing KU and indexing...".into());
        let _ = self.event_tx.send(NodeEvent::EncodeProgress {
            step: 5, total_steps, message: "Storing KU and indexing...".into(),
        }).await;
        let cid;
        let cid_hex;
        {
            let mut state = self.shared.lock().await;

            // 4c. Store in redb
            cid = state.storage.put(&ku)
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
        let _ = self.event_tx.send(NodeEvent::EncodeProgress {
            step: 6, total_steps, message: "Broadcasting to peers...".into(),
        }).await;
        self.broadcast_ku(&cid_hex, wire_bytes, text).await;
        self.request_verification(&cid_hex, text).await;

        Ok(EncodeStoreResult {
            cid,
            wire_size: wire_bytes.len(),
            instruction_count,
            gene_type: encoding_result.gene_type,
            confidence: encoding_result.confidence,
            source_text: text.to_string(),
            wire_bytes: wire_bytes.clone(),
        })
    }

    /// Process user input through the mediator pipeline.
    pub async fn process_input(&mut self, input: &str) -> Result<String, NodeError> {
        let user_input = UserInput::Text(input.to_string());
        let response = self.mediator.process(user_input).await
            .map_err(NodeError::Mediator)?;
        Ok(response.text)
    }

    /// Get the number of KUs in persistent storage.
    pub fn ku_count(&self) -> Result<usize, NodeError> {
        // Try to get count without blocking; fallback to 0 if locked
        match self.shared.try_lock() {
            Ok(state) => state.storage.count()
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
            let data = std::fs::read_to_string(&identity_path)
                .map_err(|e| NodeError::Io(e))?;
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
            trust_score: 0.0, // TODO: wire to trust system
            device_count: 1,
            max_devices: 16,
            kus_encoded: self.ku_count().unwrap_or(0) as u64,
            kus_received: 0, // TODO: track received KUs
            total_queries: 0, // TODO: track queries
        })
    }

    /// Recover identity from BIP39 phrase.
    pub fn recover_identity(&mut self, phrase: &[String], _password: &str) -> Result<IdentityInfo, NodeError> {
        // Validate BIP39 phrase (24 words)
        if phrase.len() != 24 {
            return Err(NodeError::InvalidPhrase(
                format!("Expected 24 words, got {}", phrase.len())
            ));
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
        std::fs::write(&identity_path, serde_json::to_string_pretty(&identity)
            .map_err(|e| NodeError::Config(format!("Serialize error: {}", e)))?
        ).map_err(|e| NodeError::Io(e))?;
        self.get_identity_info()
    }

    // ═══════════════════════════════════════════════════════
    // Step 3: Knowledge Operations
    // ═══════════════════════════════════════════════════════

    /// List KUs with pagination and filtering.
    pub fn list_kus(&self, page: usize, limit: usize, type_filter: Option<&str>, sort_by: &str) -> Result<(Vec<KuListItem>, usize), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state.storage.get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // Convert to view items
        let mut items: Vec<KuListItem> = all_kus.iter().map(|ku| {
            let cid_hex = hex_cid(&ku.cid);
            let gene_type = gene_type_name(ku.gene_type()).to_string();
            // Use expression text if available, otherwise indicate binary-only
            let preview = ku.expr.as_ref()
                .map(|e| e.text.clone())
                .unwrap_or_else(|| format!("[{} KU, {} instructions]", gene_type, ku.instruction_count()));
            let preview = if preview.len() > 80 { format!("{}...", &preview[..77]) } else { preview };
            let trust = ku.epi.trust.trust_score as f64 / 10000.0;
            let pomv = ku.epi.pomv_score();
            let created = ku.epi.epigenetic.as_ref()
                .and_then(|ep| ep.recorded_at)
                .unwrap_or(0);
            let wire_size = ku.wire_bytes.len();
            KuListItem { cid_hex, gene_type, preview, pomv, trust, created, wire_size }
        }).collect();

        // Filter by type
        if let Some(type_f) = type_filter {
            let type_lower = type_f.to_lowercase();
            items.retain(|i| i.gene_type.to_lowercase() == type_lower);
        }

        // Sort
        match sort_by {
            "pomv" => items.sort_by(|a, b| b.pomv.partial_cmp(&a.pomv).unwrap_or(std::cmp::Ordering::Equal)),
            "trust" => items.sort_by(|a, b| b.trust.partial_cmp(&a.trust).unwrap_or(std::cmp::Ordering::Equal)),
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

        let ku = state.storage.get(&cid_bytes)
            .map_err(|e| {
                let msg = format!("{}", e);
                if msg.contains("not found") || msg.contains("NotFound") {
                    NodeError::KuNotFound(cid_hex.to_string())
                } else {
                    NodeError::Storage(msg)
                }
            })?;

        let gene_type = gene_type_name(ku.gene_type()).to_string();
        let content = ku.expr.as_ref()
            .map(|e| e.text.clone())
            .unwrap_or_else(|| format!("[{} KU, {} instructions]", gene_type, ku.instruction_count()));
        let trust = ku.epi.trust.trust_score as f64 / 10000.0;
        let pomv = ku.epi.pomv_score();
        let confidence = ku.epi.trust.confidence as f32 / 10000.0;
        let created = ku.epi.epigenetic.as_ref()
            .and_then(|ep| ep.recorded_at)
            .unwrap_or(0);
        let wire_size = ku.wire_bytes.len();
        let instruction_count = ku.instruction_count();

        // Extract concept IDs as codon views
        let codons: Vec<CodonView> = ku.concept_ids().iter().enumerate().map(|(i, _cid)| {
            CodonView {
                name: format!("concept_{}", i),
                role: if i == 0 { "Subject".to_string() } else { "Related".to_string() },
            }
        }).collect();

        // Get bonds from epigenetics
        let bonds: Vec<BondView> = ku.epi.bonds.iter().map(|b| {
            BondView {
                direction: "OUT".to_string(),
                relation: format!("{:?}", b.relation),
                other_cid: b.target_cid.iter().map(|byte| format!("{:02x}", byte)).collect::<String>(),
                other_preview: String::new(),
                weight: b.weight as f64 / 10000.0,
            }
        }).collect();
        let outgoing_bond_count = bonds.len();
        let incoming_bond_count = 0; // TODO: wire to GraphStorage for incoming

        // Build reverse lookup: concept_id → name (using node's live dict which includes new concepts)
        let reverse: std::collections::HashMap<u64, String> = self.dict.iter()
            .map(|(name, &id)| (id, name.clone()))
            .collect();
        let cn = |id: u64| -> String {
            reverse.get(&id).cloned().unwrap_or_else(|| format!("#{}", id))
        };

        // Decode instructions for human-readable view
        let decoded_instructions: Vec<crate::types::InstructionView> = ku.dna.instructions.iter().map(|instr| {
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
                Instruction::Step { ord, action, target } => crate::types::InstructionView {
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
                Instruction::Constraint { source, op, target } => crate::types::InstructionView {
                    op: "Constraint".into(),
                    description: format!("{} {:?} {}", cn(*source), op, cn(*target)),
                    concept_ids: vec![*source, *target],
                },
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
                    description: format!("sequence[{}]", items.iter().map(|i| cn(*i)).collect::<Vec<_>>().join(", ")),
                    concept_ids: items.clone(),
                },
                Instruction::EnumVal { s, values } => crate::types::InstructionView {
                    op: "EnumVal".into(),
                    description: format!("{} ∈ {{{}}}", cn(*s), values.iter().map(|v| cn(*v)).collect::<Vec<_>>().join(", ")),
                    concept_ids: std::iter::once(*s).chain(values.iter().cloned()).collect(),
                },
                Instruction::CidRef { cid } => crate::types::InstructionView {
                    op: "CidRef".into(),
                    description: format!("ref → {}", cid.iter().map(|b| format!("{:02x}", b)).collect::<String>()),
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
                    op: format!("{:?}", other).split_whitespace().next().unwrap_or("Unknown").to_string(),
                    description: format!("{:?}", other),
                    concept_ids: vec![],
                },
            }
        }).collect();

        Ok(KuDetail {
            cid_hex: hex_cid(&ku.cid),
            gene_type,
            content,
            codons,
            bonds,
            trust,
            pomv,
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
        state.storage.delete(&cid_bytes)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Execute a KQL query string.
    pub fn execute_kql(&self, query_str: &str) -> Result<Vec<KuListItem>, NodeError> {
        // Parse the KQL query (validates syntax)
        let _query = ku_kql::parser::parse_query(query_str)
            .map_err(|e| NodeError::Kql(format!("Syntax error: {}", e)))?;

        // Execute: get all KUs and filter
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let all_kus = state.storage.get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        // TODO: Wire to full KQL executor (ku_kql::executor) when available.
        // For now, use simple text-based matching as a fallback.
        let q_lower = query_str.to_lowercase();
        let results: Vec<KuListItem> = all_kus.iter()
            .filter(|ku| {
                let gene = gene_type_name(ku.gene_type()).to_lowercase();
                let text = ku.expr.as_ref().map(|e| e.text.to_lowercase()).unwrap_or_default();
                text.contains(&q_lower) || gene.contains(&q_lower)
            })
            .map(|ku| {
                let cid_hex = hex_cid(&ku.cid);
                let gene_type = gene_type_name(ku.gene_type()).to_string();
                let preview = ku.expr.as_ref()
                    .map(|e| e.text.clone())
                    .unwrap_or_else(|| format!("[{} KU]", gene_type));
                let preview = if preview.len() > 80 { format!("{}...", &preview[..77]) } else { preview };
                let trust = ku.epi.trust.trust_score as f64 / 10000.0;
                let pomv = ku.epi.pomv_score();
                let created = ku.epi.epigenetic.as_ref()
                    .and_then(|ep| ep.recorded_at)
                    .unwrap_or(0);
                let wire_size = ku.wire_bytes.len();
                KuListItem { cid_hex, gene_type, preview, pomv, trust, created, wire_size }
            })
            .collect();

        Ok(results)
    }

    /// Get graph neighbors of a KU.
    pub fn get_neighbors(&self, cid_hex: &str, _depth: u32) -> Result<Vec<NeighborInfo>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let cid_bytes = parse_cid_hex(cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid CID hex: {}", cid_hex)))?;

        // Verify KU exists (get returns Err(NotFound) if missing)
        let _ku = state.storage.get(&cid_bytes)
            .map_err(|e| {
                let msg = format!("{}", e);
                if msg.contains("not found") || msg.contains("NotFound") {
                    NodeError::KuNotFound(cid_hex.to_string())
                } else {
                    NodeError::Storage(msg)
                }
            })?;

        // TODO: Wire to GraphStorage for real bond/neighbor data
        // For now return empty — bonds will be populated when graph module is wired
        Ok(Vec::new())
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
            expertise: profile.top_expertise(5).iter().map(|e| ExpertiseView {
                domain: e.domain.clone(),
                ku_count: e.ku_count as u64,
                last_active: e.last_active,
            }).collect(),
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
                    _ => return Err(NodeError::InvalidArgument(
                        format!("Invalid style: '{}'. Options: concise, balanced, detailed, academic", value)
                    )),
                };
            }
            _ => return Err(NodeError::InvalidArgument(
                format!("Unknown profile field: '{}'. Fields: name, language, style", field)
            )),
        }
        profile.save(&self.config.profile_path())
            .map_err(|e| NodeError::Storage(format!("Failed to save profile: {}", e)))?;
        Ok(())
    }

    /// List available AI models.
    pub fn list_ai_models(&self) -> Result<Vec<ModelInfo>, NodeError> {
        // Read from ku-ai registry
        let current_model = self.config.model.clone();
        let models = vec![
            ModelInfo {
                name: current_model.clone(),
                params: "current".to_string(),
                is_current: true,
                is_installed: true,
            },
        ];
        // TODO: Query Ollama API for installed models
        // GET http://localhost:11434/api/tags
        Ok(models)
    }

    /// Switch the active AI model.
    pub fn switch_model(&mut self, model_name: &str) -> Result<(), NodeError> {
        self.config.model = model_name.to_string();
        // TODO: Reinitialize AI backends with new model
        // For now just update config
        Ok(())
    }

    /// Test AI connection.
    pub async fn test_ai_connection(&self) -> Result<AiHealthInfo, NodeError> {
        let start = std::time::Instant::now();

        // Simple TCP check to Ollama
        let addr = self.config.ollama_url
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
                self.config.port = value.parse::<u16>()
                    .map_err(|_| NodeError::InvalidArgument(format!("Invalid port: {}", value)))?;
            }
            "ollama_url" => self.config.ollama_url = value.to_string(),
            "model" => self.config.model = value.to_string(),
            _ => return Err(NodeError::InvalidArgument(
                format!("Unknown config key: '{}'. Keys: name, port, ollama_url, model", key)
            )),
        }
        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // Step 6: OBT Wallet
    // ═══════════════════════════════════════════════════════

    /// Get OBT wallet info (from local AccountState).
    /// OBT uses Nano-style block-lattice — no central ledger.
    /// Balance = head_block.balance (authoritative, local).
    pub fn get_balance(&self) -> Result<WalletInfo, NodeError> {
        // TODO: Wire to obt_ledger::AccountChain when identity is connected
        // For now, return placeholder based on local activity
        let ku_count = self.ku_count().unwrap_or(0) as u64;
        Ok(WalletInfo {
            balance: ku_count * 25_000, // Placeholder: ~25 OBT per KU
            chain_length: ku_count + 1, // Open block + 1 Mint per KU
            tier: "Contributor".to_string(),
            multiplier: 0.5,
            total_earned: ku_count * 25_000,
            total_spent: 0,
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

    /// Get wallet transaction history.
    pub fn get_wallet_history(&self, limit: usize) -> Result<Vec<WalletTransaction>, NodeError> {
        // TODO: Wire to AccountChain block traversal
        // For now return placeholder
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs();

        let ku_count = self.ku_count().unwrap_or(0);
        let mut transactions = Vec::new();

        // Generate placeholder transactions from KU count
        for i in 0..std::cmp::min(ku_count, limit) {
            transactions.push(WalletTransaction {
                block_type: "Mint".to_string(),
                amount: 25_000, // 25 OBT in milliOBT
                detail: format!("R1:Owner — KU #{}", ku_count - i),
                timestamp: now - (i as u64 * 3600),
                confirmation: "Settled".to_string(),
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
        let all_kus = state.storage.get_all()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;

        let count = all_kus.len();

        match format {
            "json" => {
                let items: Vec<serde_json::Value> = all_kus.iter().map(|ku| {
                    serde_json::json!({
                        "cid": hex_cid(&ku.cid),
                        "gene_type": gene_type_name(ku.gene_type()),
                        "content": ku.expr.as_ref().map(|e| e.text.clone()).unwrap_or_default(),
                        "trust": ku.epi.trust.trust_score as f64 / 10000.0,
                        "pomv": ku.epi.pomv_score(),
                        "wire_size": ku.wire_bytes.len(),
                    })
                }).collect();
                let json = serde_json::to_string_pretty(&items)
                    .map_err(|e| NodeError::Storage(format!("JSON serialize error: {}", e)))?;
                std::fs::write(path, json)?;
            }
            "csv" => {
                let mut csv = String::from("cid,gene_type,content,trust,pomv,wire_size\n");
                for ku in &all_kus {
                    let content = ku.expr.as_ref()
                        .map(|e| e.text.replace('"', "\"\"")
                            .replace('\n', " "))
                        .unwrap_or_default();
                    csv.push_str(&format!("\"{}\",\"{}\",\"{}\",{:.4},{:.4},{}\n",
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
            _ => return Err(NodeError::InvalidArgument(
                format!("Unknown export format: '{}'. Options: json, csv", format)
            )),
        }

        Ok(count)
    }

    /// Import KUs from a text file (one paragraph per KU).
    pub async fn import_file(&mut self, path: &std::path::Path) -> Result<ImportResult, NodeError> {
        let content = std::fs::read_to_string(path)?;
        let paragraphs: Vec<&str> = content.split("\n\n")
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

        Ok(ImportResult { imported, skipped, errors })
    }

    /// Create a backup of all node data.
    pub fn create_backup(&self, path: &std::path::Path, _password: &str) -> Result<BackupInfo, NodeError> {
        // Collect all data files
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

        // KU count
        let ku_count = self.ku_count().unwrap_or(0);
        backup.insert("ku_count".to_string(), serde_json::Value::Number(ku_count.into()));
        backup.insert("storage_path".to_string(),
            serde_json::Value::String(self.config.storage_path().display().to_string()));

        // TODO: Include encrypted KU data, for now just metadata
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
                .unwrap_or_default().as_secs(),
        })
    }

    /// Restore from a backup file.
    pub fn restore_backup(&mut self, path: &std::path::Path, _password: &str) -> Result<(), NodeError> {
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

        // TODO: Restore KU storage
        Ok(())
    }

    // ═══════════════════════════════════════════════════════
    // Blob Storage
    // ═══════════════════════════════════════════════════════

    /// Store a file as a blob and return its metadata.
    pub fn store_blob(&self, file_path: &std::path::Path) -> Result<BlobMeta, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.store_file(file_path)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Get blob metadata by hex CID.
    pub fn get_blob_meta(&self, blob_cid_hex: &str) -> Result<BlobMeta, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex)))?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.get_meta(&cid)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// List all blobs.
    pub fn list_blobs(&self) -> Result<Vec<BlobMeta>, NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.list_blobs()
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Export a blob to a file.
    pub fn export_blob(&self, blob_cid_hex: &str, output: &std::path::Path) -> Result<u64, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex)))?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.export_to_file(&cid, output)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Delete a blob.
    pub fn delete_blob_file(&self, blob_cid_hex: &str) -> Result<bool, NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex)))?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.delete_blob(&cid)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Get blob storage statistics.
    pub fn blob_stats(&self) -> Result<(usize, u64), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        let count = state.blob_store.blob_count()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        let size = state.blob_store.total_blob_size()
            .map_err(|e| NodeError::Storage(format!("{}", e)))?;
        Ok((count, size))
    }

    /// Garbage collect orphaned blobs.
    pub fn blob_gc(&self) -> Result<(usize, u64), NodeError> {
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.garbage_collect()
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }

    /// Add KU reference to blob.
    pub fn blob_add_ku_ref(&self, blob_cid_hex: &str, ku_cid_hex: &str) -> Result<(), NodeError> {
        let cid = BlobCid::from_hex(blob_cid_hex)
            .ok_or_else(|| NodeError::InvalidArgument(format!("Invalid blob CID: {}", blob_cid_hex)))?;
        let state = match self.shared.try_lock() {
            Ok(s) => s,
            Err(_) => return Err(NodeError::Storage("Storage busy".into())),
        };
        state.blob_store.add_ku_reference(&cid, ku_cid_hex)
            .map_err(|e| NodeError::Storage(format!("{}", e)))
    }
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
        NetMessage::PeerHello { name, port: _, ku_count } => {
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

        NetMessage::KuPush { cid_hex, wire_bytes, source_text } => {
            // Decode and store the KU
            match KuRuntime::from_wire(wire_bytes.clone()) {
                Ok(ku) => {
                    let mut state = shared.lock().await;
                    match state.storage.put(&ku) {
                        Ok(_cid) => {
                            state.retriever.index_ku(cid_hex.clone(), source_text.clone());
                            let _ = state.retriever.save(&state.config.retriever_path());
                            let _ = event_tx.send(NodeEvent::KuReceived {
                                cid_hex,
                                wire_bytes,
                                source_text,
                                from: format!("{}", peer_addr),
                            }).await;
                        }
                        Err(e) => {
                            let _ = event_tx.send(NodeEvent::Notification(
                                format!("  ⚠ Failed to store KU from {}: {}", peer_addr, e),
                            )).await;
                        }
                    }
                }
                Err(e) => {
                    let _ = event_tx.send(NodeEvent::Notification(
                        format!("  ⚠ Failed to decode KU from {}: {}", peer_addr, e),
                    )).await;
                }
            }
        }

        NetMessage::VerifyRequest { cid_hex, source_text } => {
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
            ).await;

            // Send response
            let response = NetMessage::VerifyResponse {
                cid_hex: cid_hex.clone(),
                agreement_score: result.agreement_score,
                verified: result.verified,
            };
            let _ = send_message(&mut stream, &response).await;

            let _ = event_tx.send(NodeEvent::Notification(
                format!("  📋 Verified KU {} for {} (score: {:.0}%)",
                    &cid_hex[..8], peer_addr, result.agreement_score * 100.0),
            )).await;
        }

        NetMessage::VerifyResponse { cid_hex, agreement_score, verified } => {
            let _ = event_tx.send(NodeEvent::VerifyResult {
                cid_hex,
                agreement_score,
                verified,
                from: format!("{}", peer_addr),
            }).await;
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
        .map(|i| u8::from_str_radix(&padded[i*2..i*2+2], 16))
        .collect();
    bytes.ok().and_then(|b| b.try_into().ok())
}

/// Convert gene type u8 to human-readable name.
fn gene_type_name(gt: u8) -> &'static str {
    match gt {
        0 => "Fact",
        1 => "Hypothesis",
        2 => "Experience",
        3 => "Procedure",
        4 => "Rule",
        5 => "Definition",
        6 => "Relation",
        7 => "Meta",
        8 => "Creative",
        9 => "Belief",
        10 => "FormalProof",
        _ => "Unknown",
    }
}
