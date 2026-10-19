//! ARCH-09: read-only public acceptance store adapters for KU public share/export.
//!
//! Moved verbatim from ku_product.rs; ku_product re-exports the public
//! items so onebrain_node::ku_product::{KuPublicReader, CanonicalPublicReader}
//! paths are unchanged.
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ku_core::foundation::{ObjectCid, RedbVerifiedBackend};

use super::corrupt;
use crate::base_runtime::BaseServiceError;
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
