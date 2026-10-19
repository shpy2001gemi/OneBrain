//! KU-RUN-001: node-owned local KU semantics behind authenticated Base sessions.
//!
//! PrivateVault remains the canonical acceptance owner. The encrypted journal
//! records the complete bundle before writes and publishes one commit marker
//! only after every record has survived acceptance. Reads require that marker.
use ku_encoder::extraction::{
    ExtractionCheckpoint, ExtractionError, ExtractionJob, ExtractionJournal,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine};
use ku_core::foundation::semantic::SemanticFrameSet;
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_knowledge_object, DisclosureClass, KnownObjectKind, ObjectCid, ObjectKind,
    ObjectSemantics, PrivateVault, PutVerifiedOutcome, RedbVerifiedBackend, ResourceProfile,
    SourceArtifact, VaultKey,
};
use onebrain_base_contract::ku::*;
use onebrain_base_contract::ku_payload::{decode_hex, hex, KuPayload};
use onebrain_base_contract::{BaseErrorCodeV1, ResourceBudgetV1};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::base_runtime::{BaseServiceError, BaseServices};
use crate::concept_registry_runtime::{
    ConceptRegistryGenerationManager, ConceptRegistryReaderLease,
};
use crate::dataset_path::{BaseStorageOwnerId, DatasetGenerationId, DatasetPathResolver};

const JOURNAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ku_private_journal_v1");
const EXTRACTION: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ku_private_extraction_v1");
const TEXT_INTAKE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("ku_private_text_intake_v1");
/// ARCH-03: committed/canceled bundles leave the active journal in the same
/// write transaction that finishes them and stay as encrypted finished records
/// for idempotent receipts, reconciliation, cancel evidence and the catalog.
const TERMINAL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("ku_private_terminal_v1");
/// Bound for active (prepared, not yet committed/canceled) operations only.
const MAX_OPERATIONS: usize = 1024;
mod review_jobs;
mod review_lowering;
const MAX_JOURNAL_BYTES: usize = 64 * 1024 * 1024;
/// Finite local bound for compact finished records; exceeding it is a
/// non-retryable `ku_storage_full`, never a retryable rate limit.
const MAX_TERMINAL_OPERATIONS: usize = 65_536;
const MAX_TERMINAL_BYTES: usize = 256 * 1024 * 1024;
const MAX_SNAPSHOTS: usize = 32;
const MAX_CURSORS: usize = 1024;
pub(crate) const KU_PREPARED_MARKER: &[u8] = b"onebrain:ku:prepared:1\0";

/// Host-only source/encoder port. Ordinary product handles cannot install a port,
/// supply authority booleans, or bypass its current custody/consent assessment.
pub trait KuInputProvider: Send + Sync {
    /// Host activation gate; never selected by a private source or model reply.
    fn review_selection_v2(&self) -> bool {
        false
    }
    fn review_provider(
        &self,
        _model: &str,
    ) -> Option<Arc<dyn ku_encoder::extraction::ExtractionProvider>> {
        None
    }
    fn experimental_ai_allowed(&self, _commitment: [u8; 32]) -> bool {
        false
    }
    fn supports_implementation(&self, mode: InputMode, commitment: [u8; 32]) -> bool {
        self.implementation(mode) == Some(commitment)
    }
    fn capture_text(
        &self,
        _principal: [u8; 32],
        _input: crate::ku_ollama::TextIntake,
    ) -> Result<crate::ku_ollama::TextIntakeRecord, BaseServiceError> {
        Err(unavailable())
    }
    fn restore_text(
        &self,
        _record: crate::ku_ollama::TextIntakeRecord,
    ) -> Result<(), BaseServiceError> {
        Err(unavailable())
    }
    fn editor(
        &self,
        _principal: [u8; 32],
        _request: crate::ku_manual::ManualEditorRequest,
        _budget: &ResourceBudgetV1,
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        Err(unavailable())
    }
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]>;
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError>;
    fn resolve(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
        registry: &ConceptRegistryReaderLease,
        budget: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError>;
    /// New extraction adapters override this finite async port. The default keeps
    /// already-resolved host draft providers source-compatible.
    fn resolve_async<'a>(
        &'a self,
        principal: [u8; 32],
        request: &'a KuPrepareV1,
        registry: &'a ConceptRegistryReaderLease,
        budget: &'a ResourceBudgetV1,
        _execution: KuExtractionExecution<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<KuResolvedInput, BaseServiceError>> + Send + 'a>> {
        Box::pin(async move { self.resolve(principal, request, registry, budget) })
    }
}

pub struct KuExtractionExecution<'a> {
    pub journal: &'a dyn ExtractionJournal,
    pub process: [u8; 32],
    pub dataset: [u8; 32],
    pub canceled: Arc<AtomicBool>,
}

/// Resolved input is unaccepted producer output. The service still validates
/// every source object, CCID selection, semantic frame and output identity.
pub struct KuResolvedInput {
    pub drafts: Vec<SemanticFrameSet>,
    pub source_objects: Vec<Vec<u8>>,
    pub bindings: Vec<KuConceptBinding>,
    pub needs_resolution: bool,
    pub extraction_budget: Option<ku_encoder::extraction::WorkBudget>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KuConceptBinding {
    pub label: String,
    pub selected: Option<[u8; 16]>,
}

fn charge_extraction(
    budget: &mut Option<ku_encoder::extraction::WorkBudget>,
    work: usize,
) -> Result<(), BaseServiceError> {
    if let Some(budget) = budget {
        budget.charge(work).map_err(|e| {
            if e.0 == "canceled" {
                conflict()
            } else {
                resource()
            }
        })?;
    }
    Ok(())
}

/// Read-only view of the already owned public acceptance store.
pub trait KuPublicReader: Send + Sync {
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError>;
    fn bind_canonical_path(&self, _path: std::path::PathBuf) {}
    fn set_live_reader(&self, _reader: Option<Arc<dyn KuPublicReader>>) {}
    #[cfg(feature = "vnext-network-runtime")]
    fn live_network_runtime(
        &self,
    ) -> Option<Arc<crate::vnext_network_runtime::VNextNetworkRuntime>> {
        None
    }
}

type LivePublicReaders = Mutex<BTreeMap<std::path::PathBuf, std::sync::Weak<dyn KuPublicReader>>>;

static LIVE_PUBLIC_READERS: std::sync::OnceLock<LivePublicReaders> = std::sync::OnceLock::new();

fn live_public_readers() -> &'static LivePublicReaders {
    LIVE_PUBLIC_READERS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[cfg(feature = "vnext-network-runtime")]
type LiveNetworkRuntimes = Mutex<
    BTreeMap<
        std::path::PathBuf,
        std::sync::Weak<crate::vnext_network_runtime::VNextNetworkRuntime>,
    >,
>;

#[cfg(feature = "vnext-network-runtime")]
static LIVE_NETWORK_RUNTIMES: std::sync::OnceLock<LiveNetworkRuntimes> = std::sync::OnceLock::new();

#[cfg(feature = "vnext-network-runtime")]
fn live_network_runtimes() -> &'static LiveNetworkRuntimes {
    LIVE_NETWORK_RUNTIMES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn normalized_reader_paths(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut keys = vec![path.to_path_buf()];
    if let Ok(canonical) = path.canonicalize() {
        if !keys.contains(&canonical) {
            keys.push(canonical);
        }
    }
    keys
}

pub fn register_live_public_reader(path: &std::path::Path, reader: &Arc<dyn KuPublicReader>) {
    if let Ok(mut map) = live_public_readers().lock() {
        map.retain(|_, weak| weak.strong_count() > 0);
        for key in normalized_reader_paths(path) {
            map.insert(key, Arc::downgrade(reader));
        }
    }
}

#[cfg(feature = "vnext-network-runtime")]
pub fn register_live_network_runtime(
    path: &std::path::Path,
    runtime: &Arc<crate::vnext_network_runtime::VNextNetworkRuntime>,
) {
    if let Ok(mut map) = live_network_runtimes().lock() {
        map.retain(|_, weak| weak.strong_count() > 0);
        for key in normalized_reader_paths(path) {
            map.insert(key, Arc::downgrade(runtime));
        }
    }
}

fn lookup_live_public_reader(path: &std::path::Path) -> Option<Arc<dyn KuPublicReader>> {
    let mut map = live_public_readers().lock().ok()?;
    map.retain(|_, weak| weak.strong_count() > 0);
    for key in normalized_reader_paths(path) {
        if let Some(reader) = map.get(&key).and_then(std::sync::Weak::upgrade) {
            return Some(reader);
        }
    }
    None
}

#[cfg(feature = "vnext-network-runtime")]
fn lookup_live_network_runtime(
    path: &std::path::Path,
) -> Option<Arc<crate::vnext_network_runtime::VNextNetworkRuntime>> {
    let mut map = live_network_runtimes().lock().ok()?;
    map.retain(|_, weak| weak.strong_count() > 0);
    for key in normalized_reader_paths(path) {
        if let Some(runtime) = map.get(&key).and_then(std::sync::Weak::upgrade) {
            if runtime.is_network_lane_active() {
                return Some(runtime);
            }
        }
    }
    None
}

/// Normal-host read-only adapter over the node's canonical verified public
/// store (`vnext_verified.redb`), delegating to an active in-process validated
/// sink when present and opening committed on-disk records on demand otherwise.
pub struct CanonicalPublicReader {
    live: Mutex<Option<Arc<dyn KuPublicReader>>>,
    paths: Mutex<Vec<std::path::PathBuf>>,
}

impl CanonicalPublicReader {
    pub fn for_data_dir(data_dir: &std::path::Path) -> Arc<Self> {
        Arc::new(Self {
            live: Mutex::new(None),
            paths: Mutex::new(vec![data_dir.join("vnext_verified.redb")]),
        })
    }

    pub fn register_live_for_data_dir(
        data_dir: &std::path::Path,
        reader: &Arc<dyn KuPublicReader>,
    ) {
        register_live_public_reader(&data_dir.join("vnext_verified.redb"), reader);
    }

    #[cfg(feature = "vnext-network-runtime")]
    pub fn register_live_runtime_for_verified_store(
        verified_store: &std::path::Path,
        runtime: &Arc<crate::vnext_network_runtime::VNextNetworkRuntime>,
    ) {
        let reader: Arc<dyn KuPublicReader> = runtime.clone();
        register_live_public_reader(verified_store, &reader);
        register_live_network_runtime(verified_store, runtime);
    }

    #[cfg(feature = "vnext-network-runtime")]
    pub fn register_live_runtime_for_data_dir(
        data_dir: &std::path::Path,
        runtime: &Arc<crate::vnext_network_runtime::VNextNetworkRuntime>,
    ) {
        Self::register_live_runtime_for_verified_store(
            &data_dir.join("vnext_verified.redb"),
            runtime,
        );
    }
}

impl KuPublicReader for CanonicalPublicReader {
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError> {
        if let Some(live) = self.live.lock().map_err(|_| corrupt())?.clone() {
            if let Some(bytes) = live.get_public_object(cid)? {
                return Ok(Some(bytes));
            }
        }
        let paths = self.paths.lock().map_err(|_| corrupt())?.clone();
        for path in paths {
            if let Some(live) = lookup_live_public_reader(&path) {
                if let Some(bytes) = live.get_public_object(cid)? {
                    return Ok(Some(bytes));
                }
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let backend = RedbVerifiedBackend::open(&path).map_err(|_| corrupt())?;
            let sink = crate::vnext_validated_sink::SharedVNextValidatedSink::new(
                crate::vnext_validated_sink::VNextValidatedSink::new(backend),
            );
            if let Some(bytes) = sink.get_public_object(cid)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    fn bind_canonical_path(&self, path: std::path::PathBuf) {
        if let Ok(mut paths) = self.paths.lock() {
            if !paths.contains(&path) {
                paths.insert(0, path);
            }
        }
    }

    fn set_live_reader(&self, reader: Option<Arc<dyn KuPublicReader>>) {
        if let Ok(mut live) = self.live.lock() {
            *live = reader;
        }
    }

    #[cfg(feature = "vnext-network-runtime")]
    fn live_network_runtime(
        &self,
    ) -> Option<Arc<crate::vnext_network_runtime::VNextNetworkRuntime>> {
        if let Ok(Some(live)) = self.live.lock().map(|g| g.clone()) {
            if let Some(runtime) = live.live_network_runtime() {
                if runtime.is_network_lane_active() {
                    return Some(runtime);
                }
            }
        }
        let paths = self.paths.lock().ok()?.clone();
        for path in paths {
            if let Some(runtime) = lookup_live_network_runtime(&path) {
                return Some(runtime);
            }
        }
        None
    }
}

impl<B: ku_core::foundation::AtomicVerifiedBackend + Send> KuPublicReader
    for crate::vnext_validated_sink::SharedVNextValidatedSink<B>
{
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError> {
        self.get_object(ObjectCid::from_bytes(cid))
            .map_err(|_| corrupt())
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl KuPublicReader for crate::vnext_network_runtime::VNextNetworkRuntime {
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError> {
        self.public_object(ObjectCid::from_bytes(cid))
            .map_err(|_| corrupt())
    }
}

pub struct KuRuntimeConfig {
    pub vault_key: VaultKey,
    pub registry: Option<Arc<ConceptRegistryGenerationManager>>,
    pub inputs: Arc<dyn KuInputProvider>,
    pub public: Option<Arc<dyn KuPublicReader>>,
}

#[derive(Clone)]
pub struct KuServices {
    pub(crate) base: BaseServices,
}

impl KuServices {
    pub fn experimental_ai_allowed(&self, commitment: [u8; 32]) -> Result<bool, BaseServiceError> {
        self.base.ku_experimental_ai_allowed(commitment)
    }
    pub async fn editor(
        &self,
        request: crate::ku_manual::ManualEditorRequest,
        budget: ResourceBudgetV1,
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        self.base.ku_editor(request, budget).await
    }
    pub async fn reserve(&self) -> Result<OperationId, BaseServiceError> {
        self.base.ku_reserve().await
    }
    pub async fn invoke(
        &self,
        request: KuRequestV1,
        budget: ResourceBudgetV1,
    ) -> Result<KuResponseV1, BaseServiceError> {
        request.validate().map_err(|_| invalid())?;
        self.base.ku_invoke(request, budget).await
    }
    pub async fn invoke_payload(
        &self,
        kind: u16,
        bytes: &[u8],
        budget: ResourceBudgetV1,
    ) -> Result<KuResponseV1, BaseServiceError> {
        if bytes.len() as u64 > budget.max_bytes {
            return Err(resource());
        }
        let request = KuRequestV1::decode(kind, bytes).map_err(|_| invalid())?;
        self.invoke(request, budget).await
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PreparedBundle {
    principal: [u8; 32],
    dataset: DatasetGenerationId,
    request: KuPrepareV1,
    predecessor: Option<ObjectCID>,
    expected_frontier: Option<RevisionFrontier>,
    preview: KuPreparedV1,
    sources: Vec<Vec<u8>>,
    /// Input bytes contain source spans; the accepted semantic payload does not.
    private_semantic_inputs: Vec<Vec<u8>>,
    original_statement_ids: Vec<Vec<u32>>,
    bindings: Vec<KuConceptBinding>,
    registry_builder: String,
    committed: bool,
    canceled: bool,
    /// Finished record held in TERMINAL (ARCH-03); committed ones omit source
    /// bytes kept in the Vault. Absent in records written before compaction.
    #[serde(default)]
    compacted: bool,
}

struct Snapshot {
    principal: [u8; 32],
    dataset: DatasetGenerationId,
    query: Option<String>,
    items: Vec<KuSummaryV1>,
    frontier: RevisionFrontier,
}

struct Cursor {
    snapshot: [u8; 32],
    last_cid: [u8; 32],
}

struct ExtractionFlight<'a> {
    store: &'a KuStore,
    operation: [u8; 32],
    cancel: Arc<AtomicBool>,
}
impl Drop for ExtractionFlight<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.store.extracting.lock() {
            active.remove(&self.operation);
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractionEnvelope {
    principal: [u8; 32],
    dataset: [u8; 32],
    operation: [u8; 32],
    sources: Vec<[u8; 32]>,
    checkpoint: ExtractionCheckpoint,
}

#[derive(Default)]
struct Catalog {
    keys: BTreeMap<[u8; 32], OperationId>,
    items: BTreeMap<[u8; 32], BTreeMap<[u8; 32], (KuSummaryV1, String)>>,
    revisions: BTreeMap<[u8; 32], BTreeSet<([u8; 32], [u8; 32])>>,
}

#[derive(Default)]
struct Paging {
    snapshots: BTreeMap<[u8; 32], Snapshot>,
    order: VecDeque<[u8; 32]>,
    cursors: BTreeMap<String, Cursor>,
}

pub(crate) struct KuStore {
    review_running: Mutex<BTreeMap<[u8; 32], Arc<AtomicBool>>>,
    review_gate: tokio::sync::Semaphore,
    /// ARCH-07 lowered review drafts awaiting prepare; process memory only.
    lowered: Mutex<BTreeMap<[u8; 32], review_lowering::LoweredDraft>>,
    vault: PrivateVault<RedbVerifiedBackend>,
    journal: Database,
    dataset: DatasetGenerationId,
    process: [u8; 32],
    extracting: Mutex<BTreeMap<[u8; 32], Arc<AtomicBool>>>,
    canceled: Mutex<BTreeSet<[u8; 32]>>,
    pub(crate) inputs: Arc<dyn KuInputProvider>,
    registry: Option<Arc<ConceptRegistryGenerationManager>>,
    releases: Mutex<BTreeMap<[u8; 32], ConceptRegistryReaderLease>>,
    public: Option<Arc<dyn KuPublicReader>>,
    #[cfg(feature = "vnext-network-runtime")]
    canonical_verified_path: Option<std::path::PathBuf>,
    #[cfg(feature = "vnext-network-runtime")]
    public_share: Mutex<crate::ku_public_share::PublicKuShareCoordinator>,
    /// Serializes validation/revision checks with journal commit publication.
    mutation: Mutex<()>,
    paging: Mutex<Paging>,
    catalog: Mutex<Catalog>,
    /// Sealed bytes held in TERMINAL; recomputed on open, updated after commit.
    terminal_bytes: Mutex<usize>,
}

#[cfg(feature = "vnext-network-runtime")]
fn now_unix_seconds() -> Result<u64, BaseServiceError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| public_share_error("invalid_clock"))
}

fn public_share_network_unavailable() -> BaseServiceError {
    BaseServiceError::new(
        BaseErrorCodeV1::CapabilityDisabled,
        "ku_public_share_network_unavailable",
    )
}

#[cfg(feature = "vnext-network-runtime")]
fn public_share_error(code: &'static str) -> BaseServiceError {
    let base_code = match code {
        "explicit_public_confirmation_required"
        | "unsupported_public_ku"
        | "invalid_saved_ku"
        | "invalid_public_ku"
        | "invalid_public_ku_intent"
        | "invalid_peer"
        | "invalid_peer_address"
        | "invalid_selector"
        | "invalid_namespace"
        | "invalid_public_identity"
        | "invalid_intent_identity" => BaseErrorCodeV1::InvalidRequest,
        "prepared_share_missing"
        | "saved_ku_not_accessible"
        | "public_object_not_found"
        | "intent_not_found" => BaseErrorCodeV1::NotFound,
        "saved_access_changed" => BaseErrorCodeV1::Conflict,
        "public_ku_consent_expired" => BaseErrorCodeV1::Expired,
        "prepared_limit" => BaseErrorCodeV1::ResourceExhausted,
        "ku_public_share_network_unavailable" => BaseErrorCodeV1::CapabilityDisabled,
        _ => BaseErrorCodeV1::DependencyUnavailable,
    };
    BaseServiceError::new(base_code, code)
}

impl KuStore {
    #[cfg(feature = "vnext-network-runtime")]
    fn live_network_runtime(
        &self,
    ) -> Result<Arc<crate::vnext_network_runtime::VNextNetworkRuntime>, BaseServiceError> {
        if let Some(runtime) = self
            .public
            .as_ref()
            .and_then(|public| public.live_network_runtime())
        {
            return Ok(runtime);
        }
        if let Some(runtime) = self
            .canonical_verified_path
            .as_deref()
            .and_then(lookup_live_network_runtime)
        {
            return Ok(runtime);
        }
        Err(public_share_network_unavailable())
    }
    fn intake_key(&self, operation: [u8; 32]) -> [u8; 32] {
        let mut hash = blake3::Hasher::new_derive_key("onebrain:ku:private-text-intake:1");
        hash.update(&self.dataset.0);
        hash.update(&operation);
        *hash.finalize().as_bytes()
    }
    fn decode_intake(
        &self,
        key: [u8; 32],
        sealed: &[u8],
    ) -> Result<crate::ku_ollama::TextIntakeRecord, BaseServiceError> {
        let bytes = zeroize::Zeroizing::new(
            self.vault
                .open_local_metadata(self.intake_key(key), sealed)
                .map_err(|_| corrupt())?,
        );
        let record: crate::ku_ollama::TextIntakeRecord =
            serde_json::from_slice(&bytes).map_err(|_| corrupt())?;
        if record.input.operation_id.0 != key {
            return Err(corrupt());
        }
        Ok(record)
    }
    pub(crate) fn editor(
        &self,
        principal: [u8; 32],
        request: crate::ku_manual::ManualEditorRequest,
        budget: &ResourceBudgetV1,
    ) -> Result<crate::ku_manual::ManualEditorResponse, BaseServiceError> {
        use crate::ku_manual::ManualEditorRequest as Req;
        #[cfg(feature = "vnext-network-runtime")]
        use crate::ku_manual::ManualEditorResponse as Resp;
        let input = match request {
            Req::EncodeText(input) => input,
            #[cfg(feature = "vnext-network-runtime")]
            Req::SharePrepare(input) => {
                let now = now_unix_seconds()?;
                self.public_share
                    .lock()
                    .map_err(|_| corrupt())?
                    .check_capacity(now)
                    .map_err(public_share_error)?;
                let cid = ObjectCID(
                    decode_hex::<32>(&input.object_cid)
                        .map_err(|_| public_share_error("invalid_saved_ku"))?,
                );
                let view = self
                    .get(principal, cid)
                    .map_err(|_| public_share_error("saved_ku_not_accessible"))?;
                let preview = self
                    .public_share
                    .lock()
                    .map_err(|_| corrupt())?
                    .prepare_view(
                        &input.object_cid,
                        &view,
                        &input.expected_node_id,
                        input.address,
                        &input.selector,
                        &input.namespace,
                        now,
                    )
                    .map_err(public_share_error)?;
                if serde_json::to_vec(&preview).map_err(|_| invalid())?.len() as u64
                    > budget.max_bytes
                {
                    return Err(resource());
                }
                return Ok(Resp::PublicShare(preview));
            }
            #[cfg(feature = "vnext-network-runtime")]
            Req::ShareConfirm(input) => {
                let cid_hex = self
                    .public_share
                    .lock()
                    .map_err(|_| corrupt())?
                    .prepared_object_cid(&input.prepared_id, input.confirm_public)
                    .map_err(public_share_error)?
                    .to_owned();
                let cid = ObjectCID(decode_hex::<32>(&cid_hex).map_err(|_| invalid())?);
                let view = self
                    .get(principal, cid)
                    .map_err(|_| public_share_error("saved_access_changed"))?;
                let runtime = self.live_network_runtime()?;
                let now = now_unix_seconds()?;
                let confirmed = self
                    .public_share
                    .lock()
                    .map_err(|_| corrupt())?
                    .confirm_view(
                        &input.prepared_id,
                        input.confirm_public,
                        &view,
                        &runtime,
                        now,
                    )
                    .map_err(public_share_error)?;
                if serde_json::to_vec(&confirmed).map_err(|_| invalid())?.len() as u64
                    > budget.max_bytes
                {
                    return Err(resource());
                }
                return Ok(Resp::PublicShare(confirmed));
            }
            #[cfg(feature = "vnext-network-runtime")]
            Req::ShareInspect(input) => {
                let runtime = self.live_network_runtime()?;
                let inspected = crate::ku_public_share::inspect_accepted_public_ku(
                    &runtime,
                    &input.object_cid,
                    &input.selector,
                )
                .map_err(public_share_error)?;
                if serde_json::to_vec(&inspected).map_err(|_| invalid())?.len() as u64
                    > budget.max_bytes
                {
                    return Err(resource());
                }
                return Ok(Resp::PublicShare(inspected));
            }
            #[cfg(feature = "vnext-network-runtime")]
            Req::ShareIntent(input) => {
                let runtime = self.live_network_runtime()?;
                let status =
                    crate::ku_public_share::inspect_outbound_intent(&runtime, &input.intent_id)
                        .map_err(public_share_error)?;
                if serde_json::to_vec(&status).map_err(|_| invalid())?.len() as u64
                    > budget.max_bytes
                {
                    return Err(resource());
                }
                return Ok(Resp::PublicShare(status));
            }
            #[cfg(not(feature = "vnext-network-runtime"))]
            Req::SharePrepare(_)
            | Req::ShareConfirm(_)
            | Req::ShareInspect(_)
            | Req::ShareIntent(_) => return Err(public_share_network_unavailable()),
            Req::ReviewLower(input) => return self.review_lower(principal, input, budget),
            other => return self.inputs.editor(principal, other, budget),
        };
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        let operation = input.operation_id.0;
        let tx = self.journal.begin_write().map_err(|_| corrupt())?;
        let record;
        {
            let mut table = tx.open_table(TEXT_INTAKE).map_err(|_| corrupt())?;
            if let Some(value) = table.get(operation.as_slice()).map_err(|_| corrupt())? {
                let old = self.decode_intake(operation, value.value())?;
                if old.principal != principal {
                    return Err(not_found());
                }
                if serde_json::to_vec(&old.input).ok() != serde_json::to_vec(&input).ok() {
                    return Err(conflict());
                }
                self.inputs.restore_text(old.clone())?;
                return Ok(crate::ku_manual::ManualEditorResponse::Draft(
                    old.preparation,
                ));
            }
            record = self.inputs.capture_text(principal, input)?;
            let bytes =
                zeroize::Zeroizing::new(serde_json::to_vec(&record).map_err(|_| invalid())?);
            if bytes.len() as u64 > budget.max_bytes {
                return Err(resource());
            }
            let sealed = self
                .vault
                .seal_local_metadata(self.intake_key(operation), &bytes)
                .map_err(|_| corrupt())?;
            let mut total = sealed.len();
            let mut count = 0;
            for row in table.iter().map_err(|_| corrupt())? {
                let (_, value) = row.map_err(|_| corrupt())?;
                total += value.value().len();
                count += 1;
            }
            if count >= 256 || total > 8 * 1024 * 1024 {
                return Err(resource());
            }
            table
                .insert(operation.as_slice(), sealed.as_slice())
                .map_err(|_| corrupt())?;
        }
        tx.commit().map_err(|_| unknown())?;
        self.inputs.restore_text(record.clone())?;
        Ok(crate::ku_manual::ManualEditorResponse::Draft(
            record.preparation,
        ))
    }
    fn begin_extraction(
        &self,
        operation: OperationId,
    ) -> Result<ExtractionFlight<'_>, BaseServiceError> {
        let canceled = self.canceled.lock().map_err(|_| corrupt())?;
        if canceled.contains(&operation.0) {
            return Err(conflict());
        }
        let mut active = self.extracting.lock().map_err(|_| corrupt())?;
        if active.contains_key(&operation.0) {
            return Err(conflict());
        }
        if active.len() >= MAX_OPERATIONS {
            return Err(resource());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        active.insert(operation.0, cancel.clone());
        Ok(ExtractionFlight {
            store: self,
            operation: operation.0,
            cancel,
        })
    }

    fn extraction_key(&self, operation: [u8; 32]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("onebrain:ku:private-extraction-binding:1");
        h.update(&self.dataset.0);
        h.update(&operation);
        *h.finalize().as_bytes()
    }

    fn mark_extraction_prepared(
        &self,
        principal: [u8; 32],
        operation: OperationId,
        remaining: Option<&ku_encoder::extraction::WorkBudget>,
        work_limit: u64,
    ) -> Result<(), BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(EXTRACTION).map_err(|_| corrupt())?;
        let value = table.get(operation.0.as_slice()).map_err(|_| corrupt())?;
        let Some(value) = value else {
            return Ok(());
        };
        let envelope = self.decode_extraction(operation.0, value.value())?;
        if envelope.principal != principal {
            return Err(not_found());
        }
        let mut checkpoint = envelope.checkpoint;
        // Reopening a prepared bundle must not revive an old process attempt.
        if checkpoint.process != self.process {
            return Ok(());
        }
        checkpoint.phase = "prepared".into();
        if let Some(remaining) = remaining {
            checkpoint.work_charged = work_limit.saturating_sub(remaining.remaining());
            checkpoint.elapsed_ms = remaining.elapsed_ms();
        }
        for attempt in &mut checkpoint.attempts {
            attempt["phase"] = "prepared".into();
            attempt["work_units_charged"] = checkpoint.work_charged.into();
            if let Some(remaining) = remaining {
                attempt["remaining_deadline_ms"] = remaining.remaining_deadline_ms().into();
            }
        }
        let job = ExtractionJob {
            principal,
            operation: operation.0,
            process: self.process,
            dataset: self.dataset.0,
            contexts: checkpoint.contexts.clone(),
        };
        drop(value);
        drop(table);
        drop(tx);
        ExtractionJournal::store(self, &job, &checkpoint).map_err(|_| unknown())?;
        if let Some(remaining) = remaining {
            if let Err(error) = remaining.clone().charge(0) {
                checkpoint.phase = "failed".into();
                checkpoint.reason = error.0.into();
                checkpoint.elapsed_ms = remaining.elapsed_ms();
                for attempt in &mut checkpoint.attempts {
                    attempt["phase"] = "failed".into();
                    attempt["reason"] = "budget_exhausted".into();
                    attempt["remaining_deadline_ms"] = 0.into();
                }
                ExtractionJournal::store(self, &job, &checkpoint).map_err(|_| unknown())?;
                return Err(resource());
            }
        }
        Ok(())
    }

    fn decode_extraction(
        &self,
        operation: [u8; 32],
        sealed: &[u8],
    ) -> Result<ExtractionEnvelope, BaseServiceError> {
        let plaintext = zeroize::Zeroizing::new(
            self.vault
                .open_local_metadata(self.extraction_key(operation), sealed)
                .map_err(|_| corrupt())?,
        );
        let envelope: ExtractionEnvelope =
            serde_json::from_slice(&plaintext).map_err(|_| corrupt())?;
        if envelope.operation != operation || envelope.dataset != self.dataset.0 {
            return Err(corrupt());
        }
        Ok(envelope)
    }
    pub(crate) fn open(
        paths: &dyn DatasetPathResolver,
        config: KuRuntimeConfig,
        process: [u8; 32],
    ) -> Result<Self, BaseServiceError> {
        let root = paths
            .owner_path(BaseStorageOwnerId::VAULT)
            .map_err(|_| corrupt())?
            .join("ku-product-v1");
        std::fs::create_dir_all(&root).map_err(|_| unavailable())?;
        let backend =
            RedbVerifiedBackend::open(&root.join("objects.redb")).map_err(|_| corrupt())?;
        let vault = PrivateVault::new(backend, config.vault_key);
        let journal = Database::create(root.join("journal.redb")).map_err(|_| corrupt())?;
        let tx = journal.begin_write().map_err(|_| corrupt())?;
        {
            tx.open_table(JOURNAL).map_err(|_| corrupt())?;
            tx.open_table(EXTRACTION).map_err(|_| corrupt())?;
            tx.open_table(TEXT_INTAKE).map_err(|_| corrupt())?;
            tx.open_table(TERMINAL).map_err(|_| corrupt())?;
        }
        tx.commit().map_err(|_| corrupt())?;
        let canonical_verified_path = paths
            .owner_path(BaseStorageOwnerId::CANONICAL)
            .ok()
            .map(|canonical| canonical.join("vnext_verified.redb"));
        if let (Some(public), Some(canonical)) =
            (config.public.as_ref(), canonical_verified_path.as_ref())
        {
            public.bind_canonical_path(canonical.clone());
        }
        let store = Self {
            review_running: Mutex::new(BTreeMap::new()),
            review_gate: tokio::sync::Semaphore::new(1),
            lowered: Mutex::new(BTreeMap::new()),
            vault,
            journal,
            dataset: paths.current_generation(),
            process,
            extracting: Mutex::new(BTreeMap::new()),
            canceled: Mutex::new(BTreeSet::new()),
            inputs: config.inputs,
            registry: config.registry,
            public: config.public,
            #[cfg(feature = "vnext-network-runtime")]
            canonical_verified_path,
            #[cfg(feature = "vnext-network-runtime")]
            public_share: Mutex::new(crate::ku_public_share::PublicKuShareCoordinator::new()),
            releases: Mutex::new(BTreeMap::new()),
            mutation: Mutex::new(()),
            paging: Mutex::new(Paging::default()),
            catalog: Mutex::new(Catalog::default()),
            terminal_bytes: Mutex::new(0),
        };
        store.recover_review_jobs()?;
        {
            let tx = store.journal.begin_read().map_err(|_| corrupt())?;
            let table = tx.open_table(TEXT_INTAKE).map_err(|_| corrupt())?;
            let mut total = 0usize;
            for (i, row) in table.iter().map_err(|_| corrupt())?.enumerate() {
                let (key, value) = row.map_err(|_| corrupt())?;
                total += value.value().len();
                if i >= 256 || total > 8 * 1024 * 1024 {
                    return Err(resource());
                }
                let record = store.decode_intake(
                    key.value().try_into().map_err(|_| corrupt())?,
                    value.value(),
                )?;
                store.inputs.restore_text(record)?;
            }
        }
        // Authentication failure, a damaged record or a partial committed bundle
        // is a typed open failure, never an empty store with reconstructed success.
        let mut legacy_finished = Vec::new();
        let mut active = BTreeSet::new();
        for bundle in store.bundles()? {
            store.validate_bundle(&bundle)?;
            if bundle.compacted {
                return Err(corrupt());
            }
            if bundle.committed {
                store.verify_committed(&bundle)?;
            }
            store.index_bundle(&bundle)?;
            active.insert(bundle.request.operation_id.0);
            if bundle.committed || bundle.canceled {
                legacy_finished.push(bundle);
            }
        }
        {
            let tx = store.journal.begin_read().map_err(|_| corrupt())?;
            let table = tx.open_table(TERMINAL).map_err(|_| corrupt())?;
            let mut total = 0usize;
            for row in table.iter().map_err(|_| corrupt())? {
                let (key, value) = row.map_err(|_| corrupt())?;
                let operation = OperationId(key.value().try_into().map_err(|_| corrupt())?);
                if active.contains(&operation.0) {
                    return Err(corrupt());
                }
                total = total.checked_add(value.value().len()).ok_or_else(corrupt)?;
                let bundle = store.decode_bundle(operation, value.value())?;
                if !bundle.compacted {
                    return Err(corrupt());
                }
                store.validate_bundle(&bundle)?;
                if bundle.committed {
                    store.verify_committed(&bundle)?;
                }
                store.index_bundle(&bundle)?;
            }
            *store.terminal_bytes.lock().map_err(|_| corrupt())? = total;
        }
        // Journals written before ARCH-03 kept finished bundles in the active
        // table. Move them once; each move is one atomic redb transaction.
        for bundle in legacy_finished {
            store.finish_bundle(&bundle)?;
        }
        {
            let tx = store.journal.begin_read().map_err(|_| corrupt())?;
            let table = tx.open_table(EXTRACTION).map_err(|_| corrupt())?;
            let mut total = 0usize;
            for (i, row) in table.iter().map_err(|_| corrupt())?.enumerate() {
                let (key, value) = row.map_err(|_| corrupt())?;
                total = total
                    .checked_add(value.value().len())
                    .ok_or_else(resource)?;
                if i >= MAX_OPERATIONS || total > MAX_JOURNAL_BYTES {
                    return Err(resource());
                }
                store.decode_extraction(
                    key.value().try_into().map_err(|_| corrupt())?,
                    value.value(),
                )?;
            }
        }
        Ok(store)
    }

    pub(crate) fn check_dataset(
        &self,
        current: DatasetGenerationId,
    ) -> Result<(), BaseServiceError> {
        if current != self.dataset {
            Err(conflict())
        } else {
            Ok(())
        }
    }

    fn bundle_key(&self, operation: OperationId) -> [u8; 32] {
        let mut h = blake3::Hasher::new_derive_key("onebrain:ku:private-journal-binding:1");
        h.update(&self.dataset.0);
        h.update(&operation.0);
        *h.finalize().as_bytes()
    }

    fn read_bundle(
        &self,
        operation: OperationId,
    ) -> Result<Option<PreparedBundle>, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(JOURNAL).map_err(|_| corrupt())?;
        if let Some(value) = table.get(operation.0.as_slice()).map_err(|_| corrupt())? {
            return self.decode_bundle(operation, value.value()).map(Some);
        }
        let terminal = tx.open_table(TERMINAL).map_err(|_| corrupt())?;
        let value = terminal
            .get(operation.0.as_slice())
            .map_err(|_| corrupt())?;
        value
            .map(|v| self.decode_bundle(operation, v.value()))
            .transpose()
    }

    fn require_extraction_prepared(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<(), BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(EXTRACTION).map_err(|_| corrupt())?;
        if let Some(value) = table.get(operation.0.as_slice()).map_err(|_| corrupt())? {
            let evidence = self.decode_extraction(operation.0, value.value())?;
            if evidence.principal != principal {
                return Err(not_found());
            }
            if evidence.checkpoint.phase != "prepared" {
                return Err(conflict());
            }
        }
        Ok(())
    }

    fn decode_bundle(
        &self,
        operation: OperationId,
        bytes: &[u8],
    ) -> Result<PreparedBundle, BaseServiceError> {
        let plaintext = zeroize::Zeroizing::new(
            self.vault
                .open_local_metadata(self.bundle_key(operation), bytes)
                .map_err(|_| corrupt())?,
        );
        let result: PreparedBundle = serde_json::from_slice(&plaintext).map_err(|_| corrupt())?;
        if result.request.operation_id != operation || result.dataset != self.dataset {
            return Err(corrupt());
        }
        Ok(result)
    }

    fn bundles(&self) -> Result<Vec<PreparedBundle>, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(JOURNAL).map_err(|_| corrupt())?;
        let mut result = Vec::new();
        let mut total = 0usize;
        for row in table.iter().map_err(|_| corrupt())? {
            let (key, value) = row.map_err(|_| corrupt())?;
            total = total
                .checked_add(value.value().len())
                .ok_or_else(resource)?;
            if total > MAX_JOURNAL_BYTES || result.len() >= MAX_OPERATIONS {
                return Err(resource());
            }
            let operation = OperationId(key.value().try_into().map_err(|_| corrupt())?);
            result.push(self.decode_bundle(operation, value.value())?);
        }
        Ok(result)
    }

    /// Writes an active (prepared) bundle. Finished bundles use `finish_bundle`.
    fn write_bundle(&self, bundle: &PreparedBundle) -> Result<(), BaseServiceError> {
        if bundle.committed || bundle.canceled || bundle.compacted {
            return Err(corrupt());
        }
        let bytes = zeroize::Zeroizing::new(serde_json::to_vec(bundle).map_err(|_| corrupt())?);
        let sealed = self
            .vault
            .seal_local_metadata(self.bundle_key(bundle.request.operation_id), &bytes)
            .map_err(|_| resource())?;
        let tx = self.journal.begin_write().map_err(|_| unknown())?;
        {
            let terminal = tx.open_table(TERMINAL).map_err(|_| unknown())?;
            if terminal
                .get(bundle.request.operation_id.0.as_slice())
                .map_err(|_| unknown())?
                .is_some()
            {
                return Err(conflict());
            }
            let mut table = tx.open_table(JOURNAL).map_err(|_| unknown())?;
            let mut count = 0;
            let mut total = sealed.len();
            for row in table.iter().map_err(|_| unknown())? {
                let (key, value) = row.map_err(|_| unknown())?;
                if key.value() != bundle.request.operation_id.0 {
                    count += 1;
                    total = total
                        .checked_add(value.value().len())
                        .ok_or_else(storage_full)?;
                }
            }
            if count >= MAX_OPERATIONS || total > MAX_JOURNAL_BYTES {
                return Err(storage_full());
            }
            let extraction = tx.open_table(EXTRACTION).map_err(|_| unknown())?;
            for row in extraction.iter().map_err(|_| unknown())? {
                let (_, value) = row.map_err(|_| unknown())?;
                total = total
                    .checked_add(value.value().len())
                    .ok_or_else(storage_full)?;
                if total > MAX_JOURNAL_BYTES {
                    return Err(storage_full());
                }
            }
            table
                .insert(bundle.request.operation_id.0.as_slice(), sealed.as_slice())
                .map_err(|_| unknown())?;
        }
        tx.commit().map_err(|_| unknown())?;
        self.index_bundle(bundle)
    }

    /// Finished record. Exact request, preview, private semantic inputs and
    /// bindings stay (VALIDATED_STORAGE_PROFILE_V1: cancellation retains its
    /// encrypted staging as evidence). A committed record drops only its source
    /// bytes: exact copies are in the Vault and verified on every reopen.
    fn compact(bundle: &PreparedBundle) -> PreparedBundle {
        let mut b = bundle.clone();
        if b.committed {
            b.sources.clear();
        }
        b.compacted = true;
        b
    }

    fn compact_sealed(&self, bundle: &PreparedBundle) -> Result<Vec<u8>, BaseServiceError> {
        let compact = Self::compact(bundle);
        let bytes = zeroize::Zeroizing::new(serde_json::to_vec(&compact).map_err(|_| corrupt())?);
        self.vault
            .seal_local_metadata(self.bundle_key(bundle.request.operation_id), &bytes)
            .map_err(|_| resource())
    }

    /// Fails before any durable effect when a finished record cannot be kept.
    fn require_terminal_room(&self, additional: usize) -> Result<(), BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let count = {
            use redb::ReadableTableMetadata;
            tx.open_table(TERMINAL)
                .map_err(|_| corrupt())?
                .len()
                .map_err(|_| corrupt())? as usize
        };
        let bytes = *self.terminal_bytes.lock().map_err(|_| corrupt())?;
        if count >= MAX_TERMINAL_OPERATIONS || bytes.saturating_add(additional) > MAX_TERMINAL_BYTES
        {
            return Err(storage_full());
        }
        Ok(())
    }

    /// One redb transaction moves a committed/canceled bundle out of the active
    /// journal: insert compact TERMINAL record, delete JOURNAL and EXTRACTION rows.
    fn finish_bundle(&self, bundle: &PreparedBundle) -> Result<(), BaseServiceError> {
        if !(bundle.committed ^ bundle.canceled) {
            return Err(corrupt());
        }
        let operation = bundle.request.operation_id.0;
        let sealed = self.compact_sealed(bundle)?;
        let mut usage = self.terminal_bytes.lock().map_err(|_| corrupt())?;
        let tx = self.journal.begin_write().map_err(|_| unknown())?;
        let next;
        {
            use redb::ReadableTableMetadata;
            let mut terminal = tx.open_table(TERMINAL).map_err(|_| unknown())?;
            let previous = terminal
                .get(operation.as_slice())
                .map_err(|_| unknown())?
                .map(|v| v.value().len());
            let count = terminal.len().map_err(|_| unknown())? as usize;
            next = usage
                .saturating_sub(previous.unwrap_or(0))
                .checked_add(sealed.len())
                .ok_or_else(storage_full)?;
            if (previous.is_none() && count >= MAX_TERMINAL_OPERATIONS) || next > MAX_TERMINAL_BYTES
            {
                return Err(storage_full());
            }
            terminal
                .insert(operation.as_slice(), sealed.as_slice())
                .map_err(|_| unknown())?;
            tx.open_table(JOURNAL)
                .map_err(|_| unknown())?
                .remove(operation.as_slice())
                .map_err(|_| unknown())?;
            tx.open_table(EXTRACTION)
                .map_err(|_| unknown())?
                .remove(operation.as_slice())
                .map_err(|_| unknown())?;
        }
        tx.commit().map_err(|_| unknown())?;
        *usage = next;
        drop(usage);
        self.index_bundle(bundle)
    }

    /// Closes an operation that will never be saved (Base `failed`): an active
    /// bundle becomes a compact non-saveable record and its extraction row goes.
    pub(crate) fn retire_unsaved(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<(), BaseServiceError> {
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        match self.read_bundle(operation)? {
            Some(b) if b.principal != principal => Err(not_found()),
            Some(b) if b.committed || b.canceled => Ok(()),
            Some(mut b) => {
                b.canceled = true;
                self.finish_bundle(&b)
            }
            None => self.drop_extraction(principal, operation),
        }
    }

    fn drop_extraction(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<(), BaseServiceError> {
        let tx = self.journal.begin_write().map_err(|_| unknown())?;
        {
            let mut table = tx.open_table(EXTRACTION).map_err(|_| unknown())?;
            let owner = match table.get(operation.0.as_slice()).map_err(|_| unknown())? {
                Some(value) => Some(
                    self.decode_extraction(operation.0, value.value())?
                        .principal,
                ),
                None => None,
            };
            match owner {
                None => return Ok(()),
                Some(owner) if owner != principal => return Err(not_found()),
                Some(_) => {
                    table
                        .remove(operation.0.as_slice())
                        .map_err(|_| unknown())?;
                }
            }
        }
        tx.commit().map_err(|_| unknown())
    }

    fn index_bundle(&self, b: &PreparedBundle) -> Result<(), BaseServiceError> {
        let mut catalog = self.catalog.lock().map_err(|_| corrupt())?;
        if let Some(existing) = catalog
            .keys
            .insert(b.request.idempotency_key.0, b.request.operation_id)
        {
            if existing != b.request.operation_id {
                return Err(corrupt());
            }
        }
        if !b.committed {
            return Ok(());
        }
        for a in &b.preview.artifacts {
            let bytes = STANDARD
                .decode(&a.canonical_preview)
                .map_err(|_| corrupt())?;
            let text = semantic_text(&bytes)?;
            let summary = KuSummaryV1 {
                object_cid: a.object_cid,
                semantic_content_cid: Some(a.semantic_content_cid),
                disclosure_class: artifact_disclosure(disclosure(b.request.destination)),
                artifact_validity: ArtifactValidity::AcceptedKnown,
                coverage: Coverage::LocalOnly,
                limitations: vec!["fidelity_unassessed".into()],
                executable: false,
                fidelity_policy_cid: None,
                fidelity_frontier: None,
            };
            catalog
                .items
                .entry(b.principal)
                .or_default()
                .insert(a.object_cid.0, (summary, text));
            if let Some(predecessor) = b.predecessor {
                if predecessor != a.object_cid {
                    catalog
                        .revisions
                        .entry(b.principal)
                        .or_default()
                        .insert((predecessor.0, a.object_cid.0));
                }
            }
        }
        Ok(())
    }

    fn owned_bundle(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<PreparedBundle, BaseServiceError> {
        let b = self.read_bundle(operation)?.ok_or_else(not_found)?;
        if b.principal != principal {
            return Err(not_found());
        }
        self.require_extraction_prepared(principal, operation)?;
        self.validate_bundle(&b)?;
        Ok(b)
    }

    fn pinned_release(
        &self,
        root: ReleaseRoot,
    ) -> Result<ConceptRegistryReaderLease, BaseServiceError> {
        let mut releases = self.releases.lock().map_err(|_| corrupt())?;
        if let Some(release) = releases.get(&root.0) {
            return Ok(release.clone());
        }
        let release = self
            .registry
            .as_ref()
            .ok_or_else(unavailable)?
            .reader_lease();
        let actual = release
            .status()
            .release_aggregate_root
            .as_ref()
            .ok_or_else(unavailable)?;
        if decode_hex::<32>(actual).map_err(|_| corrupt())? != root.0 {
            return Err(unavailable());
        }
        if releases.len() >= 16 {
            return Err(resource());
        }
        releases.insert(root.0, release.clone());
        Ok(release)
    }

    pub(crate) fn preview(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<KuPreparedV1, BaseServiceError> {
        let bundle = self.owned_bundle(principal, operation)?;
        if bundle.canceled {
            return Err(conflict());
        }
        Ok(bundle.preview)
    }

    pub(crate) fn is_prepared(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<bool, BaseServiceError> {
        Ok(self
            .read_bundle(operation)?
            .is_some_and(|b| b.principal == principal && !b.canceled))
    }

    pub(crate) fn owns_operation(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<bool, BaseServiceError> {
        Ok(self
            .read_bundle(operation)?
            .is_some_and(|b| b.principal == principal))
    }

    pub(crate) async fn prepare(
        &self,
        principal: [u8; 32],
        request: KuPrepareV1,
        revision: Option<(ObjectCID, RevisionFrontier)>,
        budget: &ResourceBudgetV1,
    ) -> Result<KuPreparedV1, BaseServiceError> {
        request.validate().map_err(|_| invalid())?;
        let (predecessor, expected_frontier) = revision
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or_default();
        if let Some(existing) = self.read_bundle(request.operation_id)? {
            if existing.principal != principal
                || existing.request != request
                || existing.predecessor != predecessor
                || existing.expected_frontier != expected_frontier
                || existing.canceled
            {
                return Err(conflict());
            }
            self.validate_bundle(&existing)?;
            self.require_extraction_prepared(principal, request.operation_id)?;
            return Ok(existing.preview);
        }
        if self
            .catalog
            .lock()
            .map_err(|_| corrupt())?
            .keys
            .contains_key(&request.idempotency_key.0)
        {
            return Err(conflict());
        }
        if request.semantic_profile != SEMANTIC_CONTENT_PROFILE {
            return Err(incompatible());
        }
        if request.source_refs.is_empty() || request.source_refs.len() > budget.max_items as usize {
            return Err(invalid());
        }
        if (request.input_mode == InputMode::ResolvedSemanticDraft) != request.draft_ref.is_some() {
            return Err(invalid());
        }
        let lowered = request.input_mode == InputMode::ResolvedSemanticDraft
            && review_lowering::is_lowering_commitment(request.implementation_commitment.0);
        if !lowered
            && !self
                .inputs
                .supports_implementation(request.input_mode, request.implementation_commitment.0)
        {
            return Err(unavailable());
        }
        let sources: Vec<_> = request.source_refs.iter().map(|s| s.0).collect();
        self.inputs.check_access(principal, &sources)?;
        let release = self.pinned_release(request.registry_release_root)?;
        if let Some(cid) = predecessor {
            self.get(principal, cid)?;
            if self.frontier(principal)? != expected_frontier.unwrap() {
                return Err(conflict());
            }
        }
        let flight = self.begin_extraction(request.operation_id)?;
        let mut input = if lowered {
            self.resolve_lowered(principal, &request)?
        } else {
            self.inputs
                .resolve_async(
                    principal,
                    &request,
                    &release,
                    budget,
                    KuExtractionExecution {
                        journal: self,
                        process: self.process,
                        dataset: self.dataset.0,
                        canceled: flight.cancel.clone(),
                    },
                )
                .await?
        };
        // Inference holds no service mutation lock. Revalidate the mutation and
        // cancellation frontier after the await and before any prepared write.
        let mut extraction_budget = input.extraction_budget.take();
        charge_extraction(&mut extraction_budget, 0)?;
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        if flight.cancel.load(Ordering::Acquire) {
            return Err(conflict());
        }
        if self
            .catalog
            .lock()
            .map_err(|_| corrupt())?
            .keys
            .contains_key(&request.idempotency_key.0)
        {
            return Err(conflict());
        }
        if predecessor.is_some() && self.frontier(principal)? != expected_frontier.unwrap() {
            return Err(conflict());
        }
        if (input.drafts.is_empty() && !input.needs_resolution)
            || input.drafts.len() > budget.max_items as usize
            || input.drafts.len() > 256
            || input.source_objects.len() != request.source_refs.len()
            || input.bindings.len() > 4096
        {
            return Err(resource());
        }
        let mut source_sizes = BTreeMap::new();
        let mut total = 0usize;
        for (bytes, cid) in input.source_objects.iter().zip(&request.source_refs) {
            charge_extraction(&mut extraction_budget, bytes.len())?;
            total = total.checked_add(bytes.len()).ok_or_else(resource)?;
            if total > budget.max_bytes as usize {
                return Err(resource());
            }
            let object = decode_object(bytes)?;
            if object.cid().into_bytes() != cid.0 {
                return Err(invalid());
            }
            let source = SourceArtifact::from_validated(&object).map_err(|_| invalid())?;
            source_sizes.insert(cid.0, source.raw_bytes.len());
        }
        let selected = validate_bindings(&release, &input.bindings)?;
        charge_extraction(&mut extraction_budget, input.bindings.len() * 1024)?;
        let mut artifacts = Vec::new();
        let mut private_semantic_inputs = Vec::new();
        let mut original_statement_ids = Vec::new();
        let mut unresolved = input.needs_resolution;
        let mut seen = BTreeSet::new();
        let mut work = 0u64;
        for draft in &input.drafts {
            charge_extraction(&mut extraction_budget, draft.statements.len())?;
            work = work
                .checked_add(draft.statements.len() as u64)
                .ok_or_else(resource)?;
            if work > budget.max_work_units {
                return Err(resource());
            }
            for statement in &draft.statements {
                for span in &statement.qualifiers.source_spans {
                    if span.source.reference_kind != 1
                        || span.start > span.end
                        || source_sizes
                            .get(&span.source.cid)
                            .is_none_or(|len| span.end > *len as u64)
                    {
                        return Err(invalid());
                    }
                }
            }
            let normalized = normalize_semantic_content(draft, &request.semantic_profile)
                .map_err(|_| invalid())?;
            for ccid in semantic_ccids(&normalized.semantic)? {
                if !selected.contains(&ccid) {
                    unresolved = true;
                }
            }
            let (bytes, cid) = normalized
                .semantic
                .to_knowledge_object(disclosure(request.destination))
                .map_err(|_| invalid())?
                .encode(ResourceProfile::ObjectV1)
                .map_err(|_| invalid())?;
            total = total
                .checked_add(bytes.len())
                .and_then(|n| n.checked_add(normalized.private_input_bytes.len()))
                .ok_or_else(resource)?;
            charge_extraction(
                &mut extraction_budget,
                bytes.len() + normalized.private_input_bytes.len(),
            )?;
            if total > budget.max_bytes as usize {
                return Err(resource());
            }
            private_semantic_inputs.push(normalized.private_input_bytes);
            original_statement_ids.push(normalized.original_statement_ids);
            if !seen.insert(cid.into_bytes()) {
                continue;
            }
            artifacts.push(KuPreparedArtifactV1 {
                object_cid: ObjectCID(cid.into_bytes()),
                semantic_content_cid: SemanticContentCID(normalized.cid.into_bytes()),
                canonical_preview: STANDARD.encode(&bytes),
            });
        }
        if unresolved {
            artifacts.clear();
        }
        let preview = KuPreparedV1 {
            operation_id: request.operation_id,
            validity: if unresolved {
                Validity::NeedsResolution
            } else {
                Validity::Ready
            },
            object_cids: artifacts.iter().map(|a| a.object_cid).collect(),
            registry_release_root: request.registry_release_root,
            semantic_profile: request.semantic_profile.clone(),
            destination: request.destination,
            limitations: if input.needs_resolution {
                vec!["extraction_incomplete".into()]
            } else if unresolved {
                vec!["unresolved_ccid".into()]
            } else {
                vec!["fidelity_unassessed".into()]
            },
            executable: false,
            artifacts,
        };
        if preview.encode().map_err(|_| resource())?.len() > budget.max_bytes as usize {
            return Err(resource());
        }
        let bundle = PreparedBundle {
            principal,
            dataset: self.dataset,
            request,
            predecessor,
            expected_frontier,
            preview: preview.clone(),
            sources: input.source_objects,
            private_semantic_inputs,
            original_statement_ids,
            bindings: input.bindings,
            registry_builder: release.status().builder_version.clone().unwrap_or_default(),
            committed: false,
            canceled: false,
            compacted: false,
        };
        self.validate_bundle(&bundle)?;
        self.inputs.check_access(principal, &sources)?;
        charge_extraction(&mut extraction_budget, total)?;
        self.write_bundle(&bundle)?;
        if extraction_budget.is_some() {
            failpoint("after_extraction_bundle")?;
        }
        charge_extraction(&mut extraction_budget, 0)?;
        self.mark_extraction_prepared(
            principal,
            bundle.request.operation_id,
            extraction_budget.as_ref(),
            budget.max_work_units,
        )?;
        failpoint("after_prepared_journal")?;
        Ok(preview)
    }

    fn validate_bundle(&self, bundle: &PreparedBundle) -> Result<(), BaseServiceError> {
        bundle.request.validate().map_err(|_| corrupt())?;
        bundle.preview.validate().map_err(|_| corrupt())?;
        if bundle.dataset != self.dataset
            || bundle.request.operation_id != bundle.preview.operation_id
            || bundle.preview.object_cids.len() != bundle.preview.artifacts.len()
            || bundle.private_semantic_inputs.len() != bundle.original_statement_ids.len()
            || (!(bundle.compacted && bundle.committed)
                && bundle.request.source_refs.len() != bundle.sources.len())
            || bundle.committed && bundle.canceled
            || bundle.compacted && !(bundle.committed || bundle.canceled)
            || bundle.compacted && bundle.committed && !bundle.sources.is_empty()
        {
            return Err(corrupt());
        }
        for (i, artifact) in bundle.preview.artifacts.iter().enumerate() {
            let bytes = STANDARD
                .decode(&artifact.canonical_preview)
                .map_err(|_| corrupt())?;
            let object = decode_object(&bytes)?;
            if object.cid().into_bytes() != artifact.object_cid.0
                || bundle.preview.object_cids[i] != artifact.object_cid
                || object.disclosure() != disclosure(bundle.request.destination)
            {
                return Err(corrupt());
            }
            let ObjectSemantics::Known(envelope) = object.semantics() else {
                return Err(corrupt());
            };
            let semantic =
                SemanticFrameSet::from_canonical_value(&envelope.payload).map_err(|_| corrupt())?;
            let normalized =
                normalize_semantic_content(&semantic, &bundle.request.semantic_profile)
                    .map_err(|_| corrupt())?;
            if normalized.cid.into_bytes() != artifact.semantic_content_cid.0
                || normalized.semantic != semantic
            {
                return Err(corrupt());
            }
        }
        let mut input_cids = BTreeSet::new();
        for (bytes, ids) in bundle
            .private_semantic_inputs
            .iter()
            .zip(&bundle.original_statement_ids)
        {
            let original = ku_core::foundation::decode_canonical(bytes, ResourceProfile::ObjectV1)
                .map_err(|_| corrupt())?;
            let original =
                SemanticFrameSet::from_canonical_value(&original).map_err(|_| corrupt())?;
            if ids.len() != original.statements.len() {
                return Err(corrupt());
            }
            input_cids.insert(
                normalize_semantic_content(&original, &bundle.request.semantic_profile)
                    .map_err(|_| corrupt())?
                    .cid
                    .into_bytes(),
            );
        }
        if bundle.preview.validity == Validity::Ready
            && input_cids
                != bundle
                    .preview
                    .artifacts
                    .iter()
                    .map(|a| a.semantic_content_cid.0)
                    .collect()
        {
            return Err(corrupt());
        }
        for (cid, bytes) in bundle.request.source_refs.iter().zip(&bundle.sources) {
            let source = decode_object(bytes)?;
            if source.cid().into_bytes() != cid.0
                || SourceArtifact::from_validated(&source).is_err()
            {
                return Err(corrupt());
            }
        }
        Ok(())
    }

    pub(crate) fn preflight_save(
        &self,
        principal: [u8; 32],
        save: &KuSaveV1,
    ) -> Result<(), BaseServiceError> {
        let b = self.owned_bundle(principal, save.operation_id)?;
        if b.request.idempotency_key != save.idempotency_key
            || b.preview.object_cids != save.object_cids
            || b.canceled
            || b.preview.validity != Validity::Ready
        {
            return Err(conflict());
        }
        if b.committed {
            return Ok(());
        }
        self.pinned_release(b.request.registry_release_root)?;
        self.inputs.check_access(
            principal,
            &b.request
                .source_refs
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
        )?;
        if b.predecessor.is_some() && b.expected_frontier != Some(self.frontier(principal)?) {
            return Err(conflict());
        }
        // Capacity is checked before Base confirmation and Vault writes so a
        // full store is a clean non-retryable refusal, not an unknown outcome.
        self.require_terminal_room(self.compact_sealed(&b)?.len())?;
        Ok(())
    }

    pub(crate) fn save(
        &self,
        principal: [u8; 32],
        save: &KuSaveV1,
    ) -> Result<KuReceiptV1, BaseServiceError> {
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        self.preflight_save(principal, save)?;
        let mut b = self.owned_bundle(principal, save.operation_id)?;
        if b.committed {
            self.verify_committed(&b)?;
            return Ok(receipt(&b, BaseState::Committed));
        }
        failpoint("before_objects")?;
        for (index, bytes) in b
            .sources
            .iter()
            .cloned()
            .chain(b.preview.artifacts.iter().map(|a| {
                STANDARD
                    .decode(&a.canonical_preview)
                    .expect("prevalidated base64")
            }))
            .enumerate()
        {
            // Recheck current source authority immediately before EACH durable write.
            self.inputs.check_access(
                principal,
                &b.request
                    .source_refs
                    .iter()
                    .map(|s| s.0)
                    .collect::<Vec<_>>(),
            )?;
            let object = decode_object(&bytes)?;
            let result = self
                .vault
                .put_verified_object(
                    object.cid(),
                    &bytes,
                    ResourceProfile::ObjectV1,
                    &known_kinds(),
                    &[],
                )
                .map_err(|_| unknown())?;
            if !matches!(
                result,
                PutVerifiedOutcome::Stored | PutVerifiedOutcome::AlreadyPresent
            ) {
                return Err(corrupt());
            }
            failpoint("after_object")?;
            failpoint(&format!("after_object_{index}"))?;
        }
        failpoint("before_commit_marker")?;
        self.inputs.check_access(
            principal,
            &b.request
                .source_refs
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
        )?;
        b.committed = true;
        // The commit marker is the atomic move into TERMINAL (ARCH-03).
        self.finish_bundle(&b)?;
        failpoint("after_commit_marker")?;
        Ok(receipt(&b, BaseState::Committed))
    }

    fn verify_committed(&self, b: &PreparedBundle) -> Result<(), BaseServiceError> {
        for cid in b
            .request
            .source_refs
            .iter()
            .map(|s| s.0)
            .chain(b.preview.object_cids.iter().map(|s| s.0))
        {
            self.vault
                .get_object(ObjectCid::from_bytes(cid))
                .map_err(|_| corrupt())?
                .ok_or_else(corrupt)?;
        }
        Ok(())
    }

    pub(crate) fn saved_receipt(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<Option<KuReceiptV1>, BaseServiceError> {
        // Extraction can be interrupted before any prepared bundle exists.
        let Some(b) = self.read_bundle(operation)? else {
            return Ok(None);
        };
        if b.principal != principal {
            return Err(not_found());
        }
        self.validate_bundle(&b)?;
        if b.committed {
            self.verify_committed(&b)?;
            self.index_bundle(&b)?;
            Ok(Some(receipt(&b, BaseState::Committed)))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn confirmation_receipt(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<KuReceiptV1, BaseServiceError> {
        Ok(receipt(
            &self.owned_bundle(principal, operation)?,
            BaseState::Committed,
        ))
    }

    pub(crate) fn recovery_save(
        &self,
        principal: [u8; 32],
        operation: OperationId,
        key: [u8; 32],
    ) -> Result<KuReceiptV1, BaseServiceError> {
        let bundle = self.owned_bundle(principal, operation)?;
        self.save(
            principal,
            &KuSaveV1 {
                operation_id: operation,
                idempotency_key: IdempotencyKey(key),
                object_cids: bundle.preview.object_cids,
            },
        )
    }

    pub(crate) fn cancel(
        &self,
        principal: [u8; 32],
        operation: OperationId,
    ) -> Result<(), BaseServiceError> {
        let _guard = self.mutation.lock().map_err(|_| corrupt())?;
        self.cancel_review_if_present(principal, operation.0)?;
        {
            let mut canceled = self.canceled.lock().map_err(|_| corrupt())?;
            let extracting = self.extracting.lock().map_err(|_| corrupt())?;
            if !canceled.contains(&operation.0) && canceled.len() >= MAX_OPERATIONS {
                // Only a running extraction needs this in-memory fence; later
                // prepares are fenced by Base state and the durable record.
                canceled.retain(|op| extracting.contains_key(op));
                if canceled.len() >= MAX_OPERATIONS {
                    return Err(storage_full());
                }
            }
            canceled.insert(operation.0);
            if let Some(signal) = extracting.get(&operation.0) {
                signal.store(true, Ordering::Release);
            }
        }
        let Some(mut b) = self.read_bundle(operation)? else {
            // Interrupted before a bundle existed: drop its extraction row.
            return self.drop_extraction(principal, operation);
        };
        if b.principal != principal {
            return Err(not_found());
        }
        if b.committed {
            return Err(conflict());
        }
        if b.canceled {
            return Ok(());
        }
        b.canceled = true;
        // Retain the full encrypted exact intent for reconciliation (moved out
        // of the active table); never undo accepted bytes.
        self.finish_bundle(&b)
    }

    pub(crate) fn get(
        &self,
        principal: [u8; 32],
        cid: ObjectCID,
    ) -> Result<KuViewV1, BaseServiceError> {
        let summary = self
            .catalog
            .lock()
            .map_err(|_| corrupt())?
            .items
            .get(&principal)
            .and_then(|m| m.get(&cid.0))
            .map(|v| v.0.clone());
        if let Some(summary) = summary {
            let bytes = self
                .vault
                .get_object(ObjectCid::from_bytes(cid.0))
                .map_err(|_| corrupt())?
                .ok_or_else(corrupt)?;
            let object = decode_object(&bytes)?;
            return Ok(view(
                cid,
                bytes.clone(),
                summary.semantic_content_cid,
                object.disclosure(),
            ));
        }
        if let Some(public) = &self.public {
            if let Some(bytes) = public.get_public_object(cid.0)? {
                let object = decode_object(&bytes)?;
                if object.cid().into_bytes() != cid.0
                    || object.disclosure() != DisclosureClass::Public
                {
                    return Err(not_found());
                }
                let opaque = object.is_opaque();
                let semantic_cid = match object.semantics() {
                    ObjectSemantics::Known(envelope) if envelope.kind == ObjectKind(2) => {
                        SemanticFrameSet::from_canonical_value(&envelope.payload)
                            .ok()
                            .and_then(|semantic| {
                                normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE).ok()
                            })
                            .map(|normalized| SemanticContentCID(normalized.cid.into_bytes()))
                    }
                    _ => None,
                };
                let mut result = view(cid, bytes, semantic_cid, DisclosureClass::Public);
                if opaque {
                    result.artifact_validity = ArtifactValidity::AcceptedOpaque;
                }
                return Ok(result);
            }
        }
        Err(not_found())
    }

    fn frontier(&self, principal: [u8; 32]) -> Result<RevisionFrontier, BaseServiceError> {
        let catalog = self.catalog.lock().map_err(|_| corrupt())?;
        Ok(self.catalog_frontier(principal, &catalog))
    }

    fn catalog_frontier(&self, principal: [u8; 32], catalog: &Catalog) -> RevisionFrontier {
        let mut h = blake3::Hasher::new_derive_key("onebrain:ku:local-revision-frontier:1");
        h.update(&principal);
        h.update(&self.dataset.0);
        if let Some(items) = catalog.items.get(&principal) {
            for cid in items.keys() {
                h.update(&[1]);
                h.update(cid);
            }
        }
        if let Some(rows) = catalog.revisions.get(&principal) {
            for (a, b) in rows {
                h.update(&[2]);
                h.update(a);
                h.update(b);
            }
        }
        RevisionFrontier(*h.finalize().as_bytes())
    }

    pub(crate) fn page(
        &self,
        principal: [u8; 32],
        query: Option<String>,
        limit: u64,
        continuation: Option<String>,
        budget: &ResourceBudgetV1,
    ) -> Result<KuPageV1, BaseServiceError> {
        let mut paging = self.paging.lock().map_err(|_| corrupt())?;
        let (snapshot_id, last) = if let Some(token) = continuation {
            let cursor = paging.cursors.get(&token).ok_or_else(expired)?;
            (cursor.snapshot, Some(cursor.last_cid))
        } else {
            let catalog = self.catalog.lock().map_err(|_| corrupt())?;
            let mut items = Vec::new();
            let mut work = 0u64;
            if let Some(owned) = catalog.items.get(&principal) {
                for (summary, text) in owned.values() {
                    work = work
                        .checked_add(if query.is_some() {
                            text.len() as u64 + 1
                        } else {
                            1
                        })
                        .ok_or_else(resource)?;
                    if work > budget.max_work_units || items.len() >= 4096 {
                        return Err(resource());
                    }
                    if query.as_ref().is_none_or(|q| text.contains(q)) {
                        items.push(summary.clone());
                    }
                }
            }
            let id = random()?;
            if paging.snapshots.len() >= MAX_SNAPSHOTS {
                if let Some(old) = paging.order.pop_front() {
                    paging.snapshots.remove(&old);
                    paging.cursors.retain(|_, v| v.snapshot != old);
                }
            }
            let snapshot = Snapshot {
                principal,
                dataset: self.dataset,
                query: query.clone(),
                items,
                frontier: self.catalog_frontier(principal, &catalog),
            };
            paging.snapshots.insert(id, snapshot);
            paging.order.push_back(id);
            (id, None)
        };
        let snapshot = paging.snapshots.get(&snapshot_id).ok_or_else(expired)?;
        if snapshot.principal != principal
            || snapshot.dataset != self.dataset
            || snapshot.query != query
        {
            return Err(conflict());
        }
        let mut page = KuPageV1 {
            items: Vec::new(),
            coverage: Coverage::LocalOnly,
            snapshot_frontier: snapshot.frontier,
            limitations: vec![
                "private_owned_store_only".into(),
                "index_version_ku_text_1".into(),
                "frontier_is_local_revision_journal".into(),
            ],
            continuation: None,
        };
        let candidates: Vec<_> = snapshot
            .items
            .iter()
            .filter(|s| last.is_none_or(|v| s.object_cid.0 > v))
            .collect();
        let take = limit
            .min(budget.max_items as u64)
            .min(budget.max_work_units)
            .min(256) as usize;
        for item in candidates.iter().take(take) {
            page.items.push((*item).clone());
            if page.encode().map_err(|_| resource())?.len() + 128 > budget.max_bytes as usize {
                page.items.pop();
                break;
            }
        }
        if page.items.len() < candidates.len() {
            let last = page.items.last().ok_or_else(resource)?.object_cid.0;
            if paging.cursors.len() >= MAX_CURSORS {
                return Err(resource());
            }
            let token = format!("obc1.{}", hex(&random()?));
            paging.cursors.insert(
                token.clone(),
                Cursor {
                    snapshot: snapshot_id,
                    last_cid: last,
                },
            );
            page.coverage = Coverage::Partial;
            page.continuation = Some(token);
        }
        Ok(page)
    }

    pub(crate) fn status(&self) -> KuStatusV1 {
        let registry_ready = self
            .registry
            .as_ref()
            .is_some_and(|r| r.reader_lease().status().release_aggregate_root.is_some());
        KuStatusV1 {
            lifecycle: if registry_ready {
                Lifecycle::Active
            } else {
                Lifecycle::Degraded
            },
            coverage: Coverage::LocalOnly,
            limitations: vec!["local_only".into(), "fidelity_unassessed".into()],
            registry_ready,
            local_encoder_ready: self.inputs.implementation(InputMode::LocalRule).is_some()
                || self.inputs.implementation(InputMode::LocalAi).is_some(),
            remote_encoding_enabled: false,
            direct_issuance_enabled: false,
            receipt: None,
        }
    }

    pub(crate) fn public_export(
        &self,
        principal: [u8; 32],
        request: KuExportV1,
    ) -> Result<KuExportViewV1, BaseServiceError> {
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        let mut pending = request.object_cids.clone();
        let mut total = 0usize;
        while let Some(cid) = pending.pop() {
            if !seen.insert(cid.0) {
                continue;
            }
            if seen.len() > 256 {
                return Err(resource());
            }
            let object = self.get(principal, cid)?;
            if object.disclosure_class != ArtifactDisclosure::PUBLIC {
                return Err(conflict());
            }
            let bytes = STANDARD
                .decode(object.canonical_bytes)
                .map_err(|_| corrupt())?;
            total += bytes.len();
            if total > 262144 {
                return Err(resource());
            }
            let decoded = decode_object(&bytes)?;
            // Opaque records remain readable, but their dependency semantics
            // cannot establish a complete public export closure here.
            if decoded.is_opaque() {
                return Err(unavailable());
            }
            if let ObjectSemantics::Known(envelope) = decoded.semantics() {
                for reference in &envelope.references {
                    if reference.reference_kind != 1 {
                        return Err(unavailable());
                    }
                    pending.push(ObjectCID(reference.cid));
                }
            }
            entries.push(
                crate::canonical_exchange::BaseExchangeEntryV1::VNextPublic {
                    kind: ku_core::foundation::StoredRecordKind::Object,
                    cid: cid.0,
                    canonical_bytes: bytes,
                },
            );
        }
        let mut bytes = Vec::new();
        crate::canonical_exchange::write_canonical_exchange(&entries, &mut bytes)
            .map_err(|_| invalid())?;
        Ok(KuExportViewV1 {
            mode: ExportMode::CanonicalPublicExchange,
            object_cids: request.object_cids,
            limitations: vec![],
            requires_base_management: false,
            public_records: Some(STANDARD.encode(bytes)),
            archive_operation_id: None,
        })
    }
}

impl ExtractionJournal for KuStore {
    fn load(&self, job: &ExtractionJob) -> Result<Option<ExtractionCheckpoint>, ExtractionError> {
        if job.dataset != self.dataset.0 {
            return Err(ExtractionError("dataset_binding"));
        }
        let tx = self
            .journal
            .begin_read()
            .map_err(|_| ExtractionError("journal_unavailable"))?;
        let table = tx
            .open_table(EXTRACTION)
            .map_err(|_| ExtractionError("journal_unavailable"))?;
        let value = table
            .get(job.operation.as_slice())
            .map_err(|_| ExtractionError("journal_unavailable"))?;
        let Some(value) = value else {
            return Ok(None);
        };
        let e = self
            .decode_extraction(job.operation, value.value())
            .map_err(|_| ExtractionError("journal_corrupt"))?;
        if e.principal != job.principal {
            return Err(ExtractionError("source_unavailable"));
        }
        Ok(Some(e.checkpoint))
    }
    fn store(
        &self,
        job: &ExtractionJob,
        checkpoint: &ExtractionCheckpoint,
    ) -> Result<(), ExtractionError> {
        let error = || ExtractionError("journal_unavailable");
        if job.dataset != self.dataset.0 || job.process != self.process {
            return Err(ExtractionError("dataset_binding"));
        }
        if checkpoint.calls > 32
            || checkpoint.validation_diagnostics.len() > 8
            || checkpoint.validation_diagnostics.iter().any(|s| {
                !(s.starts_with("schema: ") || s.starts_with("grounding: "))
                    || s.chars().count() > 240
            })
            || checkpoint.work_charged > 1_000_000
            || checkpoint.input_tokens > 262144
            || checkpoint.output_tokens > 65536
        {
            return Err(ExtractionError("resource"));
        }
        let sources = job
            .contexts
            .iter()
            .map(|c| {
                decode_hex::<32>(c["source_ref"].as_str().unwrap_or(""))
                    .map_err(|_| ExtractionError("source_binding"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Revocation may prevent storing new source-bearing checkpoints; no
        // model output can bypass this existing custody port at the write edge.
        self.inputs
            .check_access(job.principal, &sources)
            .map_err(|_| ExtractionError("source_revoked"))?;
        let envelope = ExtractionEnvelope {
            principal: job.principal,
            dataset: job.dataset,
            operation: job.operation,
            sources,
            checkpoint: checkpoint.clone(),
        };
        let plaintext =
            zeroize::Zeroizing::new(serde_json::to_vec(&envelope).map_err(|_| error())?);
        if plaintext.len() > 8 * 1024 * 1024 {
            return Err(ExtractionError("resource"));
        }
        let sealed = self
            .vault
            .seal_local_metadata(self.extraction_key(job.operation), &plaintext)
            .map_err(|_| error())?;
        let tx = self.journal.begin_write().map_err(|_| error())?;
        {
            // Checked under the single redb writer: cancel inserts its fence
            // before its own cleanup transaction, so a late checkpoint cannot
            // resurrect a row for a canceled or finished operation.
            if self
                .canceled
                .lock()
                .map_err(|_| error())?
                .contains(&job.operation)
            {
                return Err(ExtractionError("canceled"));
            }
            if tx
                .open_table(TERMINAL)
                .map_err(|_| error())?
                .get(job.operation.as_slice())
                .map_err(|_| error())?
                .is_some()
            {
                return Err(ExtractionError("canceled"));
            }
            let mut table = tx.open_table(EXTRACTION).map_err(|_| error())?;
            if let Some(old) = table.get(job.operation.as_slice()).map_err(|_| error())? {
                let old = self
                    .decode_extraction(job.operation, old.value())
                    .map_err(|_| ExtractionError("journal_corrupt"))?;
                if old.principal != job.principal
                    || old.checkpoint.binding != checkpoint.binding
                    || old.checkpoint.calls > checkpoint.calls
                    || old.checkpoint.input_tokens > checkpoint.input_tokens
                    || old.checkpoint.output_tokens > checkpoint.output_tokens
                    || old.checkpoint.work_charged > checkpoint.work_charged
                    || old.checkpoint.elapsed_ms > checkpoint.elapsed_ms
                {
                    return Err(ExtractionError("checkpoint_regression"));
                }
            }
            let mut total = sealed.len();
            let mut count = 0;
            for row in table.iter().map_err(|_| error())? {
                let (key, value) = row.map_err(|_| error())?;
                if key.value() != job.operation {
                    count += 1;
                    total = total.checked_add(value.value().len()).ok_or_else(error)?;
                }
            }
            let bundles = tx.open_table(JOURNAL).map_err(|_| error())?;
            for row in bundles.iter().map_err(|_| error())? {
                let (_, value) = row.map_err(|_| error())?;
                total = total.checked_add(value.value().len()).ok_or_else(error)?;
            }
            if count >= MAX_OPERATIONS || total > MAX_JOURNAL_BYTES {
                return Err(ExtractionError("resource"));
            }
            table
                .insert(job.operation.as_slice(), sealed.as_slice())
                .map_err(|_| error())?;
        }
        tx.commit().map_err(|_| error())?;
        let phase = match checkpoint.phase.as_str() {
            "extracting" => "after_extraction_reservation",
            "candidate_recorded" => "after_extraction_candidate",
            "validated" => "after_extraction_validated",
            _ => "after_extraction_checkpoint",
        };
        failpoint(phase).map_err(|_| error())?;
        Ok(())
    }
}

fn validate_bindings(
    release: &ConceptRegistryReaderLease,
    bindings: &[KuConceptBinding],
) -> Result<BTreeSet<[u8; 16]>, BaseServiceError> {
    use ku_core::concept_registry::ResolveResult;
    let mut selected = BTreeSet::new();
    for binding in bindings {
        if binding.label.len() > 4096 {
            return Err(resource());
        }
        let Some(ccid) = binding.selected else {
            continue;
        };
        let candidates = match release
            .resolve_checked(&binding.label)
            .map_err(|_| unavailable())?
        {
            ResolveResult::Found(value) | ResolveResult::Fuzzy(value) => vec![value],
            ResolveResult::Ambiguous(values) => values,
            ResolveResult::NotFound => Vec::new(),
        };
        if !candidates.iter().any(|c| c.ccid == ccid) {
            return Err(invalid());
        }
        selected.insert(ccid);
    }
    Ok(selected)
}

fn semantic_ccids(semantic: &SemanticFrameSet) -> Result<BTreeSet<[u8; 16]>, BaseServiceError> {
    // The typed walker covers predicates, every term/constraint and unit binding.
    Ok(semantic
        .concept_ccids()
        .map_err(|_| invalid())?
        .into_iter()
        .map(|c| *c.as_bytes())
        .collect())
}

fn semantic_text(bytes: &[u8]) -> Result<String, BaseServiceError> {
    fn walk(v: &ku_core::foundation::CanonicalValue, text: &mut String) {
        use ku_core::foundation::CanonicalValue;
        match v {
            CanonicalValue::Text(t) => {
                text.push_str(t);
                text.push('\0');
            }
            CanonicalValue::Array(a) => {
                for v in a {
                    walk(v, text);
                }
            }
            CanonicalValue::Map(m) => {
                for (_, v) in m {
                    walk(v, text);
                }
            }
            _ => {}
        }
    }
    let mut text = String::new();
    if let ObjectSemantics::Known(e) = decode_object(bytes)?.semantics() {
        walk(&e.payload, &mut text);
    }
    Ok(text)
}

fn known_kinds() -> Vec<KnownObjectKind> {
    ku_core::foundation::schema_registry::OBJECT_KINDS_V1
        .iter()
        .map(|k| KnownObjectKind::new(ObjectKind(k.id), 1))
        .collect()
}
pub(crate) fn decode_object(
    bytes: &[u8],
) -> Result<ku_core::foundation::ValidatedKnowledgeObject, BaseServiceError> {
    decode_knowledge_object(bytes, ResourceProfile::ObjectV1, &known_kinds(), &[])
        .map_err(|_| invalid())
}
fn disclosure(d: Disclosure) -> DisclosureClass {
    match d {
        Disclosure::LOCALONLY => DisclosureClass::LocalOnly,
        Disclosure::NEGOTIATEDENCRYPTED => DisclosureClass::NegotiatedEncrypted,
    }
}
fn artifact_disclosure(d: DisclosureClass) -> ArtifactDisclosure {
    match d {
        DisclosureClass::LocalOnly => ArtifactDisclosure::LOCALONLY,
        DisclosureClass::NegotiatedEncrypted => ArtifactDisclosure::NEGOTIATEDENCRYPTED,
        DisclosureClass::Public => ArtifactDisclosure::PUBLIC,
        DisclosureClass::RouteMinimal => ArtifactDisclosure::ROUTEMINIMAL,
    }
}
fn view(
    cid: ObjectCID,
    bytes: Vec<u8>,
    semantic: Option<SemanticContentCID>,
    d: DisclosureClass,
) -> KuViewV1 {
    KuViewV1 {
        object_cid: cid,
        semantic_content_cid: semantic,
        disclosure_class: artifact_disclosure(d),
        artifact_validity: ArtifactValidity::AcceptedKnown,
        coverage: Coverage::LocalOnly,
        limitations: vec!["fidelity_unassessed".into()],
        executable: false,
        canonical_bytes: STANDARD.encode(bytes),
        fidelity_policy_cid: None,
        fidelity_frontier: None,
    }
}
fn receipt(b: &PreparedBundle, state: BaseState) -> KuReceiptV1 {
    KuReceiptV1 {
        operation_id: b.request.operation_id,
        state,
        object_cids: b.preview.object_cids.clone(),
        limitations: vec!["fidelity_unassessed".into()],
        published: false,
        authorizes_reward: false,
    }
}
fn random() -> Result<[u8; 32], BaseServiceError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
    Ok(bytes)
}
pub(crate) fn invalid() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::InvalidRequest, "ku_invalid_request")
}
pub(crate) fn conflict() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::Conflict, "ku_conflict")
}
pub(crate) fn corrupt() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::CorruptState, "ku_corrupt_state")
}
pub(crate) fn resource() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::ResourceExhausted, "ku_resource_bound")
}
/// Local KU storage bound (ARCH-03). Same wire code as other resource bounds,
/// but never retryable: retrying cannot succeed until records are freed.
pub(crate) fn storage_full() -> BaseServiceError {
    let mut error = BaseServiceError::new(BaseErrorCodeV1::ResourceExhausted, KU_STORAGE_FULL);
    error.retryable = false;
    error.reconcile_before_retry = false;
    error
}
pub(crate) const KU_STORAGE_FULL: &str = "ku_storage_full";
pub(crate) fn unavailable() -> BaseServiceError {
    BaseServiceError::new(
        BaseErrorCodeV1::DependencyUnavailable,
        "ku_dependency_unavailable",
    )
}
pub(crate) fn unknown() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::UnknownOutcome, "ku_reconcile_required")
}
fn incompatible() -> BaseServiceError {
    BaseServiceError::new(
        BaseErrorCodeV1::IncompatibleProfile,
        "ku_profile_unsupported",
    )
}
pub(crate) fn not_found() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::NotFound, "ku_not_found")
}
fn expired() -> BaseServiceError {
    BaseServiceError::new(BaseErrorCodeV1::Expired, "ku_snapshot_expired")
}

fn failpoint(_phase: &str) -> Result<(), BaseServiceError> {
    #[cfg(test)]
    {
        if FAILPOINT.with(|p| p.borrow().as_deref() == Some(&format!("crash:{_phase}"))) {
            std::process::exit(86);
        }
        if FAILPOINT.with(|p| p.borrow().as_deref() == Some(_phase)) {
            return Err(unknown());
        }
    }
    Ok(())
}
#[cfg(test)]
thread_local! { static FAILPOINT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
#[path = "ku_product_tests.rs"]
mod tests;
