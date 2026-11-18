//! ARCH-09: read-only public acceptance store adapters for KU public share/export.
//!
//! Moved verbatim from ku_product.rs; ku_product re-exports the public
//! items so onebrain_node::ku_product::{KuPublicReader, CanonicalPublicReader}
//! paths are unchanged.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use ku_core::concept_registry::ResolveResult;
use ku_core::foundation::semantic::{LiteralValue, SemanticFrameSet, TermRef};
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_knowledge_object, DisclosureClass, KnownObjectKind, ObjectCid, ObjectKind,
    ObjectSemantics, RedbVerifiedBackend, ReservedDomain, ResourceProfile,
};
use onebrain_base_contract::ku_payload::hex;

use super::corrupt;
use crate::base_runtime::BaseServiceError;
use crate::concept_registry_runtime::ConceptRegistryReaderLease;

pub const MAX_PUBLIC_KU_LIST_ITEMS: u32 = 64;
pub const MAX_PUBLIC_KU_QUERY_BYTES: usize = 256;
/// Read-only view of the already owned public acceptance store.
pub trait KuPublicReader: Send + Sync {
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError>;
    fn list_public_objects(&self) -> Result<Vec<([u8; 32], Vec<u8>)>, BaseServiceError> {
        Ok(Vec::new())
    }
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
pub(super) fn lookup_live_network_runtime(
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

    fn list_public_objects(&self) -> Result<Vec<([u8; 32], Vec<u8>)>, BaseServiceError> {
        let mut merged: BTreeMap<[u8; 32], Vec<u8>> = BTreeMap::new();
        if let Some(live) = self.live.lock().map_err(|_| corrupt())?.clone() {
            for (cid, bytes) in live.list_public_objects()? {
                merged.entry(cid).or_insert(bytes);
            }
        }
        let paths = self.paths.lock().map_err(|_| corrupt())?.clone();
        for path in paths {
            if let Some(live) = lookup_live_public_reader(&path) {
                for (cid, bytes) in live.list_public_objects()? {
                    merged.entry(cid).or_insert(bytes);
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
            for (cid, bytes) in sink.list_public_objects()? {
                merged.entry(cid).or_insert(bytes);
            }
        }
        Ok(merged.into_iter().collect())
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

    fn list_public_objects(&self) -> Result<Vec<([u8; 32], Vec<u8>)>, BaseServiceError> {
        Ok(self
            .accepted_objects()
            .map_err(|_| corrupt())?
            .into_iter()
            .map(|entry| (entry.claimed_cid, entry.canonical_bytes))
            .collect())
    }
}

#[cfg(feature = "vnext-network-runtime")]
impl KuPublicReader for crate::vnext_network_runtime::VNextNetworkRuntime {
    fn get_public_object(&self, cid: [u8; 32]) -> Result<Option<Vec<u8>>, BaseServiceError> {
        self.public_object(ObjectCid::from_bytes(cid))
            .map_err(|_| corrupt())
    }

    fn list_public_objects(&self) -> Result<Vec<([u8; 32], Vec<u8>)>, BaseServiceError> {
        self.public_objects().map_err(|_| corrupt())
    }
}

/// Render content of a known public kernel for explicit review/inspection.
/// This comparison identity and display grant no authority or source access.
pub fn inspect_public_ku(bytes: &[u8]) -> Result<serde_json::Value, &'static str> {
    let object = decode_knowledge_object(
        bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(ObjectKind(2), 1)],
        &[],
    )
    .map_err(|_| "invalid_public_ku")?;
    let ObjectSemantics::Known(envelope) = object.semantics() else {
        return Err("unsupported_public_ku");
    };
    if envelope.kind != ObjectKind(2)
        || envelope.disclosure != DisclosureClass::Public
        || !envelope.references.is_empty()
        || !envelope.extensions.is_empty()
        || !envelope.critical_extensions.is_empty()
        || envelope.limits.is_some()
    {
        return Err("unsupported_public_ku");
    }
    let semantic = SemanticFrameSet::from_canonical_value(&envelope.payload)
        .map_err(|_| "unsupported_public_ku")?;
    if semantic.statements.len() != 1
        || semantic.statements.iter().any(|s| {
            s.arguments.len() != 1
                || !matches!(&s.arguments[0], TermRef::Literal(LiteralValue::Text(_)))
                || !s.constraints.is_empty()
                || s.qualifiers != Default::default()
        })
    {
        return Err("unsupported_public_ku");
    }
    let normalized = normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE)
        .map_err(|_| "unsupported_public_ku")?;
    let statements: Vec<_> = semantic
        .statements
        .iter()
        .map(|s| {
            let arguments: Vec<_> = s
                .arguments
                .iter()
                .map(|a| match a {
                    TermRef::Literal(LiteralValue::Text(t)) => Some(t.as_str().to_owned()),
                    _ => None,
                })
                .collect();
            serde_json::json!({
                "predicate_ccid": hex(s.operator_or_predicate.as_bytes()),
                "text_arguments": arguments
            })
        })
        .collect();
    Ok(serde_json::json!({
        "semantic_content_cid": hex(normalized.cid.as_bytes()),
        "statements": statements,
        "fidelity": "unassessed"
    }))
}

fn fold_query_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.extend(word.chars().flat_map(char::to_lowercase));
    }
    out
}

/// Receiver-side D-045 bounded browse/filter of accepted Public KUs in the
/// node's verified public store (`vnext_verified.redb`). Separate from private
/// `Saved local knowledge` (`list`/`search`), because receiving a Public KU is
/// transport storage, never adoption into owned private knowledge.
pub fn list_accepted_public_kus(
    public: &dyn KuPublicReader,
    registry: Option<&ConceptRegistryReaderLease>,
    query: Option<&str>,
    limit: Option<u32>,
) -> Result<serde_json::Value, &'static str> {
    let limit = limit.unwrap_or(MAX_PUBLIC_KU_LIST_ITEMS);
    if limit == 0 || limit > MAX_PUBLIC_KU_LIST_ITEMS {
        return Err("invalid_public_list_limit");
    }
    let query = match query {
        Some(raw) => {
            if raw.len() > MAX_PUBLIC_KU_QUERY_BYTES {
                return Err("invalid_public_query");
            }
            let folded = fold_query_text(raw);
            (!folded.is_empty()).then_some(folded)
        }
        None => None,
    };
    let mut label_ccids = BTreeSet::new();
    if let (Some(text), Some(reg)) = (query.as_deref(), registry) {
        if let Ok(resolved) = reg.resolve_checked(text) {
            let candidates = match resolved {
                ResolveResult::Found(c) | ResolveResult::Fuzzy(c) => vec![c],
                ResolveResult::Ambiguous(values) => values,
                ResolveResult::NotFound => Vec::new(),
            };
            for candidate in candidates.into_iter().take(64) {
                label_ccids.insert(hex(&candidate.ccid));
            }
        }
    }
    let records = public
        .list_public_objects()
        .map_err(|_| "public_store_unavailable")?;
    let mut items = Vec::new();
    let mut has_more = false;
    for (cid, bytes) in records {
        if ReservedDomain::Object.digest(&bytes) != cid {
            continue;
        }
        let Ok(semantic) = inspect_public_ku(&bytes) else {
            continue;
        };
        let object_cid = hex(&cid);
        if let Some(ref folded) = query {
            let cid_match = object_cid.contains(folded.as_str())
                || semantic["semantic_content_cid"]
                    .as_str()
                    .is_some_and(|s| s.contains(folded.as_str()));
            let stmt_match = semantic["statements"].as_array().is_some_and(|stmts| {
                stmts.iter().any(|stmt| {
                    let pred_match = stmt["predicate_ccid"].as_str().is_some_and(|pred| {
                        pred.contains(folded.as_str()) || label_ccids.contains(pred)
                    });
                    let text_match = stmt["text_arguments"].as_array().is_some_and(|args| {
                        args.iter().any(|arg| {
                            arg.as_str()
                                .is_some_and(|t| fold_query_text(t).contains(folded.as_str()))
                        })
                    });
                    pred_match || text_match
                })
            });
            if !cid_match && !stmt_match {
                continue;
            }
        }
        if items.len() < limit as usize {
            items.push(serde_json::json!({
                "object_cid": object_cid,
                "semantic": semantic,
                "fidelity": "unassessed",
                "authorizes_reward": false
            }));
        } else {
            has_more = true;
            break;
        }
    }
    Ok(serde_json::json!({
        "event": "public_objects",
        "items": items,
        "has_more": has_more,
        "scope": "accepted_public_store_only; not adopted into private Saved local knowledge",
        "fidelity": "unassessed",
        "authorizes_reward": false
    }))
}
