//! Node-owned `obp/blob-chunks/1` coordinator (`P2` §5).
//!
//! Enforces that a peer-requested or locally scheduled `OwnedBlob` fetch is
//! gated on an accepted canonical `OwnedBlobReferenceV1` proof (`retains_blob()`)
//! from `CanonicalBlobReferenceOracle` before requesting `BlobChunkManifest` or
//! any `256 KiB` chunk bytes, and promotes verified bytes into `BlobStorage`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ku_core::blob_store::{BlobCid, BlobMeta};
use ku_core::foundation::OwnedBlobReferenceV1;
use ku_net::vnext_chunk_streams::{
    authorized_blob_manifest_request, BlobChunkProvider, BlobChunkReceiver,
    CanonicalBlobReferenceOracle as ChunkStreamBlobOracle, ChunkStreamError,
};
use onebrain_blob_store::blob_storage::BlobStorage;
use onebrain_protocol::{
    BlobChunkManifest, BlobChunkWireMessageV1, BlobManifestRequest, CAPABILITY_BLOB_CHUNKS_V1,
};
use thiserror::Error;

use crate::blob_authority::CanonicalBlobReferenceOracle;

/// Adapter bridging `onebrain_node::CanonicalBlobReferenceOracle` to
/// `ku_net::vnext_chunk_streams::CanonicalBlobReferenceOracle`.
#[derive(Clone)]
pub struct NodeBlobChunkOracleAdapter {
    oracle: Arc<CanonicalBlobReferenceOracle>,
}

impl NodeBlobChunkOracleAdapter {
    pub const fn new(oracle: Arc<CanonicalBlobReferenceOracle>) -> Self {
        Self { oracle }
    }
}

impl ChunkStreamBlobOracle for NodeBlobChunkOracleAdapter {
    fn prove_accepted_reference(&self, blob_cid: &BlobCid) -> Option<OwnedBlobReferenceV1> {
        self.oracle.prove_retained_reference(blob_cid)
    }
}

/// Node-level coordinator for `obp/blob-chunks/1` (`256 KiB` verified chunks).
pub struct NodeBlobTransferCoordinator {
    oracle: Arc<CanonicalBlobReferenceOracle>,
    blob_storage: Arc<BlobStorage>,
    staging_root: PathBuf,
    blob_store_root: PathBuf,
}

impl NodeBlobTransferCoordinator {
    pub fn new(
        oracle: Arc<CanonicalBlobReferenceOracle>,
        blob_storage: Arc<BlobStorage>,
        staging_root: impl AsRef<Path>,
        blob_store_root: impl AsRef<Path>,
    ) -> Self {
        Self {
            oracle,
            blob_storage,
            staging_root: staging_root.as_ref().to_path_buf(),
            blob_store_root: blob_store_root.as_ref().to_path_buf(),
        }
    }

    pub const fn capability_id(&self) -> &'static str {
        CAPABILITY_BLOB_CHUNKS_V1
    }

    pub fn oracle_adapter(&self) -> NodeBlobChunkOracleAdapter {
        NodeBlobChunkOracleAdapter::new(Arc::clone(&self.oracle))
    }

    /// Gate `BlobManifestRequest` on an accepted canonical `OwnedBlobReferenceV1`
    /// proof (`retains_blob() == true`) before any network request is emitted.
    pub fn request_manifest(
        &self,
        blob_cid: BlobCid,
    ) -> Result<BlobManifestRequest, NodeBlobTransferError> {
        let adapter = self.oracle_adapter();
        Ok(authorized_blob_manifest_request(&adapter, blob_cid)?)
    }

    /// Open a crash-resumable `BlobChunkReceiver` (`256 KiB` chunks + bitmap)
    /// gated by the canonical reference oracle.
    pub fn open_receiver(
        &self,
        manifest: BlobChunkManifest,
    ) -> Result<BlobChunkReceiver, NodeBlobTransferError> {
        let adapter = self.oracle_adapter();
        Ok(BlobChunkReceiver::open_with_reference_proof(
            &adapter,
            manifest,
            &self.staging_root,
            &self.blob_store_root,
        )?)
    }

    /// Build a `BlobChunkProvider` serving only canonically retained owned blobs
    /// currently stored in `BlobStorage`.
    pub fn build_provider_from_blob_storage(
        &self,
    ) -> Result<BlobChunkProvider, NodeBlobTransferError> {
        let mut provider = BlobChunkProvider::default();
        let retained = self
            .oracle
            .canonical_retained_blob_cids()
            .map_err(|err| NodeBlobTransferError::BlobStorage(err.to_string()))?;
        for cid in retained {
            if let Ok(bytes) = self.blob_storage.read_full_blob(&cid) {
                provider.insert_blob(cid, bytes)?;
            }
        }
        Ok(provider)
    }

    /// Drive a complete reference-gated `256 KiB` chunk transfer from a
    /// `BlobChunkProvider`, verifying per-chunk BLAKE3 hashes before disk
    /// staging and full `BlobCid` digest before registering in `BlobStorage`.
    pub fn fetch_from_provider_into_blob_storage(
        &self,
        provider: &BlobChunkProvider,
        blob_cid: BlobCid,
        label: &str,
    ) -> Result<BlobMeta, NodeBlobTransferError> {
        let manifest_req = self.request_manifest(blob_cid)?;
        let response =
            provider.handle_request(&BlobChunkWireMessageV1::ManifestRequest(manifest_req))?;
        let BlobChunkWireMessageV1::Manifest(manifest) = response else {
            return Err(NodeBlobTransferError::ChunkStream(
                ChunkStreamError::UnexpectedMessage,
            ));
        };
        if manifest.blob_cid != blob_cid {
            return Err(NodeBlobTransferError::ChunkStream(
                ChunkStreamError::BlobCidMismatch,
            ));
        }

        let mut receiver = self.open_receiver(manifest)?;
        while let Some(fetch) = receiver.next_fetch() {
            let batch_msg = provider.handle_request(&BlobChunkWireMessageV1::Fetch(fetch))?;
            let BlobChunkWireMessageV1::Batch(batch) = batch_msg else {
                return Err(NodeBlobTransferError::ChunkStream(
                    ChunkStreamError::UnexpectedMessage,
                ));
            };
            receiver.ingest_batch(&batch)?;
        }
        let promoted_path = receiver.finalize_and_promote()?;
        let verified_bytes =
            std::fs::read(&promoted_path).map_err(|e| NodeBlobTransferError::Io(e.to_string()))?;
        let meta = self
            .blob_storage
            .store_bytes(label, &verified_bytes, blob_cid.blob_type())
            .map_err(|err| NodeBlobTransferError::BlobStorage(err.to_string()))?;
        if meta.blob_cid_hex != blob_cid.to_hex() {
            return Err(NodeBlobTransferError::ChunkStream(
                ChunkStreamError::BlobCidMismatch,
            ));
        }
        Ok(meta)
    }

    /// Fetch an authorized `OwnedBlob` over an authenticated QUIC `OBPConnection`
    /// (`obp/blob-chunks/1`) and register the verified bytes in `BlobStorage`.
    #[cfg(feature = "vnext-network-runtime")]
    pub async fn fetch_over_quic_into_blob_storage(
        &self,
        connection: &ku_net::transport::OBPConnection,
        blob_cid: BlobCid,
        label: &str,
    ) -> Result<BlobMeta, NodeBlobTransferError> {
        let adapter = self.oracle_adapter();
        let promoted_path = ku_net::vnext_chunk_streams::fetch_blob_over_quic(
            connection,
            &adapter,
            blob_cid,
            &self.staging_root,
            &self.blob_store_root,
        )
        .await?;
        let verified_bytes =
            std::fs::read(&promoted_path).map_err(|e| NodeBlobTransferError::Io(e.to_string()))?;
        let meta = self
            .blob_storage
            .store_bytes(label, &verified_bytes, blob_cid.blob_type())
            .map_err(|err| NodeBlobTransferError::BlobStorage(err.to_string()))?;
        if meta.blob_cid_hex != blob_cid.to_hex() {
            return Err(NodeBlobTransferError::ChunkStream(
                ChunkStreamError::BlobCidMismatch,
            ));
        }
        Ok(meta)
    }
}

#[derive(Debug, Error)]
pub enum NodeBlobTransferError {
    #[error("blob chunk stream error: {0:?}")]
    ChunkStream(ChunkStreamError),
    #[error("blob storage error: {0}")]
    BlobStorage(String),
    #[error("blob transfer I/O error: {0}")]
    Io(String),
}

impl From<ChunkStreamError> for NodeBlobTransferError {
    fn from(error: ChunkStreamError) -> Self {
        Self::ChunkStream(error)
    }
}
