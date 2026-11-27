//! Node-owned `D-012` Concept Registry pull & activation coordinator (`obp/registry-chunks/1`, `P1` §3).
//!
//! Enforces:
//! - Mandatory Ed25519 verification of `RegistryReleaseDescriptorV1` against pinned
//!   governance signers BEFORE requesting chunk 0.
//! - Pull-only `4 MiB` chunk streaming (`MAX_INFLIGHT_REGISTRY_CHUNKS = 4` / `16 MiB` RAM cap)
//!   into `staging/<release_sequence>/` with immediate per-chunk Merkle branch verification
//!   and peer transport penalties on corrupt chunks (preserving already-verified chunks).
//! - Atomic generation activation via `activate_concept_registry_release` +
//!   `ConceptRegistryGenerationManager::refresh()`, retaining the `N-1` rollback generation.
//! - Optional HTTPS mirror fallback gated strictly behind `#[cfg(feature = "vnext-bootstrap-https")]`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ku_core::concept_registry_release::{
    activate_concept_registry_release, latest_concept_registry_state,
    parse_concept_registry_verifying_key, rollback_concept_registry_release,
    ConceptRegistryReleaseState,
};
use ku_core::foundation::NodeId;
use ku_net::vnext_chunk_streams::{
    ChunkStreamError, PeerTransportPenaltyHook, RegistryChunkProvider, RegistryChunkPullSession,
};
use onebrain_protocol::{
    verify_registry_release_descriptor, RegistryChunkWireMessageV1, RegistryReleaseDescriptorV1,
    CAPABILITY_REGISTRY_CHUNKS_V1,
};
use thiserror::Error;

use crate::concept_registry_runtime::{ConceptRegistryGenerationManager, ConceptRegistryStatus};

/// Bounded peer penalty tracker recording corrupt `4 MiB` registry chunk deliveries.
#[derive(Clone, Debug, Default)]
pub struct NodePeerPenaltyTracker {
    corrupt_deliveries: BTreeMap<NodeId, Vec<(u32, [u8; 32])>>,
}

impl NodePeerPenaltyTracker {
    pub fn penalty_count(&self, peer: &NodeId) -> usize {
        self.corrupt_deliveries
            .get(peer)
            .map_or(0, std::vec::Vec::len)
    }

    pub fn penalized_peers(&self) -> Vec<NodeId> {
        self.corrupt_deliveries.keys().copied().collect()
    }
}

impl PeerTransportPenaltyHook for NodePeerPenaltyTracker {
    fn penalize_corrupt_registry_chunk(
        &mut self,
        peer: NodeId,
        release_root: [u8; 32],
        chunk_index: u32,
    ) {
        let entries = self.corrupt_deliveries.entry(peer).or_default();
        if entries.len() < 256 {
            entries.push((chunk_index, release_root));
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryActivationOutcome {
    pub release_state: ConceptRegistryReleaseState,
    pub refreshed_status: Option<ConceptRegistryStatus>,
}

/// Host coordinator for `D-012` pull-only Concept Registry distribution.
pub struct NodeRegistryPullCoordinator {
    registry_root: PathBuf,
    approved_signers: Vec<[u8; 32]>,
    generation_manager: Option<Arc<ConceptRegistryGenerationManager>>,
    penalties: Mutex<NodePeerPenaltyTracker>,
}

impl NodeRegistryPullCoordinator {
    pub fn new(
        registry_root: impl AsRef<Path>,
        approved_signers: Vec<[u8; 32]>,
        generation_manager: Option<Arc<ConceptRegistryGenerationManager>>,
    ) -> Result<Self, NodeRegistryPullError> {
        if approved_signers.is_empty() || approved_signers.contains(&[0u8; 32]) {
            return Err(NodeRegistryPullError::NoApprovedSigners);
        }
        Ok(Self {
            registry_root: registry_root.as_ref().to_path_buf(),
            approved_signers,
            generation_manager,
            penalties: Mutex::new(NodePeerPenaltyTracker::default()),
        })
    }

    pub const fn capability_id(&self) -> &'static str {
        CAPABILITY_REGISTRY_CHUNKS_V1
    }

    pub fn registry_root(&self) -> &Path {
        &self.registry_root
    }

    pub fn penalty_snapshot(&self) -> Result<NodePeerPenaltyTracker, NodeRegistryPullError> {
        self.penalties
            .lock()
            .map(|g| g.clone())
            .map_err(|_| NodeRegistryPullError::LockPoisoned)
    }

    /// Mandatory pre-chunk-0 verification of `RegistryReleaseDescriptorV1`.
    pub fn verify_descriptor_before_chunk_zero(
        &self,
        descriptor: &RegistryReleaseDescriptorV1,
    ) -> Result<(), NodeRegistryPullError> {
        verify_registry_release_descriptor(descriptor, &self.approved_signers)
            .map_err(|e| NodeRegistryPullError::ChunkStream(ChunkStreamError::Codec(e)))
    }

    /// Open or resume a crash-safe `RegistryChunkPullSession` ONLY after
    /// `RegistryReleaseDescriptorV1` passes Ed25519 verification.
    pub fn begin_or_resume_pull(
        &self,
        descriptor: RegistryReleaseDescriptorV1,
    ) -> Result<RegistryChunkPullSession, NodeRegistryPullError> {
        self.verify_descriptor_before_chunk_zero(&descriptor)?;
        Ok(RegistryChunkPullSession::open_verified(
            descriptor,
            &self.approved_signers,
            &self.registry_root,
        )?)
    }

    /// Ingest one `ChunkDelivery` into an open `RegistryChunkPullSession`, recording
    /// transport penalties if the peer delivered a corrupt chunk while preserving
    /// all previously verified chunks.
    pub fn ingest_chunk_with_penalty(
        &self,
        session: &mut RegistryChunkPullSession,
        peer: NodeId,
        delivery: &RegistryChunkWireMessageV1,
    ) -> Result<bool, NodeRegistryPullError> {
        let mut tracker = self
            .penalties
            .lock()
            .map_err(|_| NodeRegistryPullError::LockPoisoned)?;
        Ok(session.ingest_chunk_delivery(peer, delivery, &mut *tracker)?)
    }

    /// Pull a complete release from an untrusted `RegistryChunkProvider` (`4 MiB` chunks),
    /// verifying descriptor signature before chunk 0, Merkle branches on every chunk,
    /// and full `release_root` before promoting `staging/<seq>/release.obr`.
    pub fn pull_from_provider<F>(
        &self,
        peer: NodeId,
        provider: &RegistryChunkProvider,
        min_release_sequence: u64,
        verify_full_release_root: F,
    ) -> Result<PathBuf, NodeRegistryPullError>
    where
        F: FnOnce(&Path) -> Result<[u8; 32], String>,
    {
        let resp = provider.handle_request(&RegistryChunkWireMessageV1::DescriptorRequest {
            release_root: None,
            min_release_sequence,
        })?;
        let RegistryChunkWireMessageV1::DescriptorResponse { descriptor } = resp else {
            return Err(NodeRegistryPullError::ChunkStream(
                ChunkStreamError::UnexpectedMessage,
            ));
        };
        let mut session = self.begin_or_resume_pull(descriptor)?;
        while !session.bitmap().is_complete() {
            let Some(req) = session.schedule_next_chunk_request(peer)? else {
                break;
            };
            let delivery = provider.handle_request(&req)?;
            self.ingest_chunk_with_penalty(&mut session, peer, &delivery)?;
        }
        Ok(session.finalize_staged_release(verify_full_release_root)?)
    }

    /// Atomically activate an installed & verified release (`releases/<release_id>`),
    /// preserving the `N-1` generation in `state/` for deterministic rollback and
    /// refreshing any attached `ConceptRegistryGenerationManager`.
    pub fn activate_verified_release(
        &self,
        release_id: &str,
        signer_public_key_hex: &str,
    ) -> Result<RegistryActivationOutcome, NodeRegistryPullError> {
        let verifying_key = parse_concept_registry_verifying_key(signer_public_key_hex)
            .map_err(|e| NodeRegistryPullError::ReleaseActivation(e.to_string()))?;
        let release_state =
            activate_concept_registry_release(&self.registry_root, release_id, &verifying_key)
                .map_err(|e| NodeRegistryPullError::ReleaseActivation(e.to_string()))?;
        let refreshed_status = match &self.generation_manager {
            Some(manager) => Some(
                manager
                    .refresh()
                    .map_err(|e| NodeRegistryPullError::ManagerRefresh(e.to_string()))?,
            ),
            None => None,
        };
        Ok(RegistryActivationOutcome {
            release_state,
            refreshed_status,
        })
    }

    /// Deterministically roll back to the retained `N-1` `previous_release` generation
    /// and refresh any attached `ConceptRegistryGenerationManager`.
    pub fn rollback_to_previous_generation(
        &self,
        signer_public_key_hex: &str,
    ) -> Result<RegistryActivationOutcome, NodeRegistryPullError> {
        let verifying_key = parse_concept_registry_verifying_key(signer_public_key_hex)
            .map_err(|e| NodeRegistryPullError::ReleaseActivation(e.to_string()))?;
        let release_state = rollback_concept_registry_release(&self.registry_root, &verifying_key)
            .map_err(|e| NodeRegistryPullError::ReleaseActivation(e.to_string()))?;
        let refreshed_status = match &self.generation_manager {
            Some(manager) => Some(
                manager
                    .refresh()
                    .map_err(|e| NodeRegistryPullError::ManagerRefresh(e.to_string()))?,
            ),
            None => None,
        };
        Ok(RegistryActivationOutcome {
            release_state,
            refreshed_status,
        })
    }

    pub fn active_release_state(
        &self,
    ) -> Result<Option<ConceptRegistryReleaseState>, NodeRegistryPullError> {
        latest_concept_registry_state(&self.registry_root)
            .map_err(|e| NodeRegistryPullError::ReleaseActivation(e.to_string()))
    }

    /// Optional HTTPS mirror fallback strictly gated behind `vnext-bootstrap-https`.
    /// Verifies `descriptor` BEFORE any network fetch, rejects non-HTTPS URLs, and
    /// validates every `4 MiB` chunk against `descriptor.chunk_merkle_root`.
    #[cfg(feature = "vnext-bootstrap-https")]
    pub async fn pull_from_https_mirror<F>(
        &self,
        https_url: &str,
        descriptor: RegistryReleaseDescriptorV1,
        verify_full_release_root: F,
    ) -> Result<PathBuf, NodeRegistryPullError>
    where
        F: FnOnce(&Path) -> Result<[u8; 32], String>,
    {
        if !https_url.starts_with("https://") {
            return Err(NodeRegistryPullError::HttpsSchemeRequired);
        }
        self.verify_descriptor_before_chunk_zero(&descriptor)?;
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let client = reqwest::Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| NodeRegistryPullError::HttpsTransport(e.to_string()))?;
        let response = client
            .get(https_url)
            .send()
            .await
            .map_err(|e| NodeRegistryPullError::HttpsTransport(e.to_string()))?
            .error_for_status()
            .map_err(|e| NodeRegistryPullError::HttpsTransport(e.to_string()))?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| NodeRegistryPullError::HttpsTransport(e.to_string()))?;
        if bytes.len() as u64 != descriptor.total_bytes {
            return Err(NodeRegistryPullError::ChunkStream(
                ChunkStreamError::InvalidManifest,
            ));
        }
        let provider = RegistryChunkProvider::new(descriptor.clone(), &bytes)?;
        let mirror_peer = NodeId::from_bytes(*blake3::hash(https_url.as_bytes()).as_bytes());
        self.pull_from_provider(
            mirror_peer,
            &provider,
            descriptor.release_sequence,
            verify_full_release_root,
        )
    }
}

#[derive(Debug, Error)]
pub enum NodeRegistryPullError {
    #[error("registry pull requires at least one non-zero approved signer")]
    NoApprovedSigners,
    #[error("registry chunk stream failed: {0:?}")]
    ChunkStream(ChunkStreamError),
    #[error("registry release activation failed: {0}")]
    ReleaseActivation(String),
    #[error("registry generation manager refresh failed: {0}")]
    ManagerRefresh(String),
    #[error("registry penalty tracker lock poisoned")]
    LockPoisoned,
    #[cfg(feature = "vnext-bootstrap-https")]
    #[error("HTTPS mirror fallback requires an https:// URL")]
    HttpsSchemeRequired,
    #[cfg(feature = "vnext-bootstrap-https")]
    #[error("HTTPS mirror transport failed: {0}")]
    HttpsTransport(String),
}

impl From<ChunkStreamError> for NodeRegistryPullError {
    fn from(error: ChunkStreamError) -> Self {
        Self::ChunkStream(error)
    }
}
