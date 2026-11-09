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

use crate::model_host;
use crate::types::*;
use ku_core::blob_store::{BlobCid, BlobMeta, BlobType};
use ku_core::concept_registry::ConceptLookup;
use ku_core::foundation::{ObjectReference, RedbVerifiedBackend, StoredRecordKind};
use ku_core::text_parser::{default_dict, ConceptDict};
use ku_core::KuRuntime;
#[cfg(feature = "legacy")]
use ku_encoder::{AiEncoder, EncoderConfig, EncodingResult};
use onebrain_blob_store::blob_storage::{BlobStorage, BlobStorageConfig};
use onebrain_legacy_ku_store::storage::KuStorage;
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
mod encode;
mod identity;
mod insights;
mod knowledge;
mod library;
mod network;
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
