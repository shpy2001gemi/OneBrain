//! Streaming chunk drivers for `obp/blob-chunks/1` (`P2` §5) and `D-012`
//! Concept Registry distribution (`obp/registry-chunks/1`, `P1` §3).
//!
//! Enforces:
//! - `CanonicalBlobReferenceOracle` proof before requesting any blob bytes.
//! - Crash-resumable `ChunkStagingBitmap` persisted atomically (`staging.bitmap`).
//! - Immediate per-chunk BLAKE3 / Merkle branch verification before staging to disk,
//!   and full `BlobCid` / `release_root` verification before atomic promotion.
//! - `MAX_INFLIGHT_REGISTRY_CHUNKS = 4` (`16 MiB` RAM cap) and peer transport
//!   penalty hooks on corrupt registry chunks (`FW-05`).

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use ku_core::blob_store::BlobCid;
use ku_core::foundation::{NodeId, OwnedBlobReferenceV1};
use onebrain_protocol::{
    build_registry_chunk_merkle_proof, compute_registry_chunk_cid,
    compute_registry_chunk_merkle_root, encode_blob_chunk_message,
    verify_registry_chunk_merkle_proof, verify_registry_release_descriptor, BlobChunkBatch,
    BlobChunkFetch, BlobChunkItemV1, BlobChunkManifest, BlobChunkWireMessageV1,
    BlobManifestRequest, ChunkTransferCodecError, RegistryChunkWireMessageV1,
    RegistryReleaseDescriptorV1, BLOB_CHUNK_BYTES, MAX_CHUNKS_PER_BLOB_FRAME,
    MAX_INFLIGHT_REGISTRY_CHUNKS, REGISTRY_CHUNK_BYTES,
};

#[cfg(feature = "quic")]
use crate::error::TransportError;
#[cfg(feature = "quic")]
use crate::transport::OBPConnection;
#[cfg(feature = "quic")]
use onebrain_protocol::{
    decode_blob_chunk_message, decode_registry_chunk_message, encode_registry_chunk_message,
    MAX_BLOB_WIRE_FRAME_BYTES, MAX_REGISTRY_WIRE_FRAME_BYTES,
};

const BITMAP_MAGIC: [u8; 4] = *b"OBBM";
const BITMAP_VERSION: u8 = 1;

/// Crash-resumable bitmask tracking verified chunks staged on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkStagingBitmap {
    total_chunks: u32,
    bits: Vec<u8>,
    verified_count: u32,
}

impl ChunkStagingBitmap {
    pub fn new(total_chunks: u32) -> Result<Self, ChunkStreamError> {
        if total_chunks == 0 {
            return Err(ChunkStreamError::InvalidManifest);
        }
        let byte_len = (total_chunks as usize).div_ceil(8);
        Ok(Self {
            total_chunks,
            bits: vec![0u8; byte_len],
            verified_count: 0,
        })
    }

    pub const fn total_chunks(&self) -> u32 {
        self.total_chunks
    }

    pub const fn verified_count(&self) -> u32 {
        self.verified_count
    }

    pub const fn is_complete(&self) -> bool {
        self.verified_count == self.total_chunks
    }

    pub fn is_verified(&self, index: u32) -> bool {
        if index >= self.total_chunks {
            return false;
        }
        let byte = self.bits[(index / 8) as usize];
        (byte & (1u8 << (index % 8))) != 0
    }

    pub fn mark_verified(&mut self, index: u32) -> Result<bool, ChunkStreamError> {
        if index >= self.total_chunks {
            return Err(ChunkStreamError::ChunkIndexOutOfBounds);
        }
        let slot = &mut self.bits[(index / 8) as usize];
        let mask = 1u8 << (index % 8);
        if (*slot & mask) != 0 {
            return Ok(false);
        }
        *slot |= mask;
        self.verified_count += 1;
        Ok(true)
    }

    pub fn next_missing_window(&self, max_count: u32) -> Option<(u32, u32)> {
        if max_count == 0 || self.is_complete() {
            return None;
        }
        let start = (0..self.total_chunks).find(|i| !self.is_verified(*i))?;
        let mut count = 1u32;
        while count < max_count
            && start + count < self.total_chunks
            && !self.is_verified(start + count)
        {
            count += 1;
        }
        Some((start, count))
    }

    pub fn missing_indices(&self, limit: usize) -> Vec<u32> {
        (0..self.total_chunks)
            .filter(|i| !self.is_verified(*i))
            .take(limit)
            .collect()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(9 + self.bits.len() + 32);
        out.extend_from_slice(&BITMAP_MAGIC);
        out.push(BITMAP_VERSION);
        out.extend_from_slice(&self.total_chunks.to_be_bytes());
        out.extend_from_slice(&self.bits);
        let checksum = blake3::hash(&out);
        out.extend_from_slice(checksum.as_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ChunkStreamError> {
        if bytes.len() < 9 + 32 || bytes[..4] != BITMAP_MAGIC || bytes[4] != BITMAP_VERSION {
            return Err(ChunkStreamError::CorruptStagingBitmap);
        }
        let payload_end = bytes.len() - 32;
        let expected_hash = blake3::hash(&bytes[..payload_end]);
        if expected_hash.as_bytes() != &bytes[payload_end..] {
            return Err(ChunkStreamError::CorruptStagingBitmap);
        }
        let total_chunks = u32::from_be_bytes(bytes[5..9].try_into().expect("4 bytes"));
        if total_chunks == 0 {
            return Err(ChunkStreamError::CorruptStagingBitmap);
        }
        let expected_len = (total_chunks as usize).div_ceil(8);
        let bits = bytes[9..payload_end].to_vec();
        if bits.len() != expected_len {
            return Err(ChunkStreamError::CorruptStagingBitmap);
        }
        let rem = total_chunks % 8;
        if rem != 0 {
            let unused_mask = !((1u8 << rem) - 1);
            if let Some(last) = bits.last() {
                if (*last & unused_mask) != 0 {
                    return Err(ChunkStreamError::CorruptStagingBitmap);
                }
            }
        }
        let verified_count = bits.iter().map(|b| b.count_ones()).sum();
        Ok(Self {
            total_chunks,
            bits,
            verified_count,
        })
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), ChunkStreamError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(ChunkStreamError::Io)?;
        }
        let tmp = path.with_extension("bitmap.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(ChunkStreamError::Io)?;
            f.write_all(&self.encode()).map_err(ChunkStreamError::Io)?;
            f.sync_all().map_err(ChunkStreamError::Io)?;
        }
        fs::rename(&tmp, path).map_err(ChunkStreamError::Io)
    }

    pub fn load_or_new(path: &Path, total_chunks: u32) -> Result<Self, ChunkStreamError> {
        if !path.exists() {
            return Self::new(total_chunks);
        }
        let bytes = fs::read(path).map_err(ChunkStreamError::Io)?;
        let decoded = Self::decode(&bytes)?;
        if decoded.total_chunks != total_chunks {
            return Err(ChunkStreamError::CorruptStagingBitmap);
        }
        Ok(decoded)
    }
}

// ─── Part 1: Chunked OwnedBlob Transport (`obp/blob-chunks/1`) ───────────────

/// Oracle proving an accepted canonical `KnowledgeObjectEnvelope` references and
/// retains `blob_cid` before requesting blob bytes (`P2` §5.1).
pub trait CanonicalBlobReferenceOracle: Send + Sync {
    fn prove_accepted_reference(&self, blob_cid: &BlobCid) -> Option<OwnedBlobReferenceV1>;
}

/// Compute the canonical `BASE_V1_STORAGE_INTEGRITY_PROFILE` §2 final blob path:
/// `<blob_store_root>/v2/<digest-byte-0>/<digest-byte-1>/<68-hex-ob-cid>`.
pub fn canonical_blob_final_path(blob_store_root: &Path, blob_cid: &BlobCid) -> PathBuf {
    let digest = blob_cid.blake3_hash();
    blob_store_root
        .join("v2")
        .join(format!("{:02x}", digest[0]))
        .join(format!("{:02x}", digest[1]))
        .join(blob_cid.to_hex())
}

/// Issue a `BlobManifestRequest` ONLY after verifying an accepted canonical reference.
pub fn authorized_blob_manifest_request(
    oracle: &dyn CanonicalBlobReferenceOracle,
    blob_cid: BlobCid,
) -> Result<BlobManifestRequest, ChunkStreamError> {
    let Some(reference) = oracle.prove_accepted_reference(&blob_cid) else {
        return Err(ChunkStreamError::UnreferencedBlobRejected);
    };
    if reference.blob_cid != blob_cid || !reference.retains_blob() {
        return Err(ChunkStreamError::UnreferencedBlobRejected);
    }
    Ok(BlobManifestRequest { blob_cid })
}

/// Crash-safe `256 KiB` chunked blob receiver (`TX-BLOB-001`).
pub struct BlobChunkReceiver {
    manifest: BlobChunkManifest,
    staging_part_path: PathBuf,
    bitmap_path: PathBuf,
    final_path: PathBuf,
    bitmap: ChunkStagingBitmap,
}

impl BlobChunkReceiver {
    pub fn open_with_reference_proof(
        oracle: &dyn CanonicalBlobReferenceOracle,
        manifest: BlobChunkManifest,
        staging_dir: &Path,
        blob_store_root: &Path,
    ) -> Result<Self, ChunkStreamError> {
        let _ = authorized_blob_manifest_request(oracle, manifest.blob_cid)?;
        // Validate manifest canonically through codec check.
        let _ = encode_blob_chunk_message(&BlobChunkWireMessageV1::Manifest(manifest.clone()))?;
        fs::create_dir_all(staging_dir).map_err(ChunkStreamError::Io)?;

        let hex = manifest.blob_cid.to_hex();
        let staging_part_path = staging_dir.join(format!("{hex}.part"));
        let bitmap_path = staging_dir.join(format!("{hex}.staging.bitmap"));
        let final_path = canonical_blob_final_path(blob_store_root, &manifest.blob_cid);
        let total_chunks = manifest.chunk_hashes.len() as u32;
        let bitmap = if staging_part_path.exists() {
            ChunkStagingBitmap::load_or_new(&bitmap_path, total_chunks)?
        } else {
            let fresh = ChunkStagingBitmap::new(total_chunks)?;
            let file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&staging_part_path)
                .map_err(ChunkStreamError::Io)?;
            file.set_len(manifest.total_bytes)
                .map_err(ChunkStreamError::Io)?;
            file.sync_all().map_err(ChunkStreamError::Io)?;
            fresh.save_atomic(&bitmap_path)?;
            fresh
        };

        Ok(Self {
            manifest,
            staging_part_path,
            bitmap_path,
            final_path,
            bitmap,
        })
    }

    pub fn bitmap(&self) -> &ChunkStagingBitmap {
        &self.bitmap
    }

    pub fn next_fetch(&self) -> Option<BlobChunkFetch> {
        let (start_chunk, count) = self
            .bitmap
            .next_missing_window(u32::from(MAX_CHUNKS_PER_BLOB_FRAME))?;
        Some(BlobChunkFetch {
            blob_cid: self.manifest.blob_cid,
            start_chunk,
            count: count as u16,
        })
    }

    /// Verify every chunk in `batch` against `manifest.chunk_hashes[i]` BEFORE staging
    /// bytes to disk (`TX-BLOB-001`).
    pub fn ingest_batch(&mut self, batch: &BlobChunkBatch) -> Result<u32, ChunkStreamError> {
        if batch.blob_cid != self.manifest.blob_cid {
            return Err(ChunkStreamError::BlobCidMismatch);
        }
        if batch.chunks.is_empty() || batch.chunks.len() > MAX_CHUNKS_PER_BLOB_FRAME as usize {
            return Err(ChunkStreamError::Codec(
                ChunkTransferCodecError::ResourceLimit,
            ));
        }
        for item in &batch.chunks {
            let Some(expected_len) = self.manifest.expected_chunk_len(item.chunk_index) else {
                return Err(ChunkStreamError::ChunkIndexOutOfBounds);
            };
            if item.bytes.len() != expected_len {
                return Err(ChunkStreamError::CorruptBlobChunk(item.chunk_index));
            }
            let digest = *blake3::hash(&item.bytes).as_bytes();
            if digest != self.manifest.chunk_hashes[item.chunk_index as usize] {
                return Err(ChunkStreamError::CorruptBlobChunk(item.chunk_index));
            }
        }

        let mut file = OpenOptions::new()
            .write(true)
            .open(&self.staging_part_path)
            .map_err(ChunkStreamError::Io)?;
        let mut newly_verified = 0u32;
        for item in &batch.chunks {
            if self.bitmap.is_verified(item.chunk_index) {
                continue;
            }
            let offset = u64::from(item.chunk_index) * (BLOB_CHUNK_BYTES as u64);
            file.seek(SeekFrom::Start(offset))
                .map_err(ChunkStreamError::Io)?;
            file.write_all(&item.bytes).map_err(ChunkStreamError::Io)?;
            self.bitmap.mark_verified(item.chunk_index)?;
            newly_verified += 1;
        }
        file.sync_all().map_err(ChunkStreamError::Io)?;
        self.bitmap.save_atomic(&self.bitmap_path)?;
        Ok(newly_verified)
    }

    /// Verify full `BlobCid` digest over staged file and atomically promote to
    /// `v2/<b0>/<b1>/<68-hex-ob-cid>`.
    pub fn finalize_and_promote(self) -> Result<PathBuf, ChunkStreamError> {
        if !self.bitmap.is_complete() {
            return Err(ChunkStreamError::IncompleteTransfer);
        }
        let mut file = fs::File::open(&self.staging_part_path).map_err(ChunkStreamError::Io)?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; 65_536];
        let mut read_total = 0u64;
        loop {
            let n = file.read(&mut buf).map_err(ChunkStreamError::Io)?;
            if n == 0 {
                break;
            }
            read_total += n as u64;
            hasher.update(&buf[..n]);
        }
        drop(file);

        if read_total != self.manifest.total_bytes
            || hasher.finalize().as_bytes() != self.manifest.blob_cid.blake3_hash()
        {
            return Err(ChunkStreamError::BlobCidMismatch);
        }

        if let Some(parent) = self.final_path.parent() {
            fs::create_dir_all(parent).map_err(ChunkStreamError::Io)?;
        }
        if !self.final_path.exists() {
            fs::rename(&self.staging_part_path, &self.final_path).map_err(ChunkStreamError::Io)?;
        } else {
            let _ = fs::remove_file(&self.staging_part_path);
        }
        let _ = fs::remove_file(&self.bitmap_path);
        Ok(self.final_path)
    }
}

/// In-memory / file-backed blob chunk server responder.
#[derive(Default)]
pub struct BlobChunkProvider {
    blobs: BTreeMap<[u8; 34], (BlobChunkManifest, Vec<u8>)>,
}

impl BlobChunkProvider {
    pub fn insert_blob(
        &mut self,
        blob_cid: BlobCid,
        bytes: Vec<u8>,
    ) -> Result<(), ChunkStreamError> {
        let manifest = BlobChunkManifest::from_bytes(blob_cid, &bytes)?;
        self.blobs.insert(blob_cid.0, (manifest, bytes));
        Ok(())
    }

    pub fn handle_request(
        &self,
        request: &BlobChunkWireMessageV1,
    ) -> Result<BlobChunkWireMessageV1, ChunkStreamError> {
        match request {
            BlobChunkWireMessageV1::ManifestRequest(req) => {
                let (manifest, _) = self
                    .blobs
                    .get(&req.blob_cid.0)
                    .ok_or(ChunkStreamError::UnknownBlob)?;
                Ok(BlobChunkWireMessageV1::Manifest(manifest.clone()))
            }
            BlobChunkWireMessageV1::Fetch(fetch) => {
                if fetch.count == 0 || fetch.count > MAX_CHUNKS_PER_BLOB_FRAME {
                    return Err(ChunkStreamError::Codec(
                        ChunkTransferCodecError::ResourceLimit,
                    ));
                }
                let (manifest, data) = self
                    .blobs
                    .get(&fetch.blob_cid.0)
                    .ok_or(ChunkStreamError::UnknownBlob)?;
                let mut chunks = Vec::new();
                for offset_idx in 0..u32::from(fetch.count) {
                    let idx = fetch
                        .start_chunk
                        .checked_add(offset_idx)
                        .ok_or(ChunkStreamError::ChunkIndexOutOfBounds)?;
                    if idx >= manifest.chunk_hashes.len() as u32 {
                        break;
                    }
                    let start = (idx as usize) * BLOB_CHUNK_BYTES;
                    let end = (start + BLOB_CHUNK_BYTES).min(data.len());
                    chunks.push(BlobChunkItemV1 {
                        chunk_index: idx,
                        bytes: data[start..end].to_vec(),
                    });
                }
                if chunks.is_empty() {
                    return Err(ChunkStreamError::ChunkIndexOutOfBounds);
                }
                Ok(BlobChunkWireMessageV1::Batch(BlobChunkBatch {
                    blob_cid: fetch.blob_cid,
                    chunks,
                }))
            }
            _ => Err(ChunkStreamError::UnexpectedMessage),
        }
    }
}

// ─── Part 2: D-012 Registry Chunk Distribution (`obp/registry-chunks/1`) ─────

/// Hook invoked immediately when a peer delivers a corrupt `4 MiB` registry chunk (`P1` §3.2).
pub trait PeerTransportPenaltyHook: Send + Sync {
    fn penalize_corrupt_registry_chunk(
        &mut self,
        peer: NodeId,
        release_root: [u8; 32],
        chunk_index: u32,
    );
}

/// Pull-only `obp/registry-chunks/1` staging session enforcing `MAX_INFLIGHT_REGISTRY_CHUNKS = 4`
/// (`16 MiB` RAM cap), Merkle branch verification, `staging.bitmap` crash resume, and
/// immediate peer penalty on corrupt chunks without discarding verified chunks (`D-012`).
pub struct RegistryChunkPullSession {
    descriptor: RegistryReleaseDescriptorV1,
    staging_dir: PathBuf,
    bitmap_path: PathBuf,
    package_part_path: PathBuf,
    bitmap: ChunkStagingBitmap,
    inflight_chunks: BTreeMap<u32, NodeId>,
}

impl RegistryChunkPullSession {
    pub fn open_verified(
        descriptor: RegistryReleaseDescriptorV1,
        approved_signers: &[[u8; 32]],
        registry_root_dir: &Path,
    ) -> Result<Self, ChunkStreamError> {
        verify_registry_release_descriptor(&descriptor, approved_signers)?;
        let staging_dir = registry_root_dir
            .join("staging")
            .join(descriptor.release_sequence.to_string());
        fs::create_dir_all(&staging_dir).map_err(ChunkStreamError::Io)?;
        let bitmap_path = staging_dir.join("staging.bitmap");
        let package_part_path = staging_dir.join("release.obr.part");

        let bitmap = if package_part_path.exists() {
            ChunkStagingBitmap::load_or_new(&bitmap_path, descriptor.total_chunks)?
        } else {
            let fresh = ChunkStagingBitmap::new(descriptor.total_chunks)?;
            let f = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&package_part_path)
                .map_err(ChunkStreamError::Io)?;
            f.set_len(descriptor.total_bytes)
                .map_err(ChunkStreamError::Io)?;
            f.sync_all().map_err(ChunkStreamError::Io)?;
            fresh.save_atomic(&bitmap_path)?;
            fresh
        };

        Ok(Self {
            descriptor,
            staging_dir,
            bitmap_path,
            package_part_path,
            bitmap,
            inflight_chunks: BTreeMap::new(),
        })
    }

    pub fn descriptor(&self) -> &RegistryReleaseDescriptorV1 {
        &self.descriptor
    }

    pub fn bitmap(&self) -> &ChunkStagingBitmap {
        &self.bitmap
    }

    pub fn inflight_count(&self) -> usize {
        self.inflight_chunks.len()
    }

    /// Claim the next missing chunk for dispatch to `peer`, strictly enforcing
    /// `MAX_INFLIGHT_REGISTRY_CHUNKS = 4` (`16 MiB` RAM cap).
    pub fn schedule_next_chunk_request(
        &mut self,
        peer: NodeId,
    ) -> Result<Option<RegistryChunkWireMessageV1>, ChunkStreamError> {
        if self.bitmap.is_complete() {
            return Ok(None);
        }
        if self.inflight_chunks.len() >= MAX_INFLIGHT_REGISTRY_CHUNKS {
            return Err(ChunkStreamError::RegistryInflightCapExceeded);
        }
        let next_idx = (0..self.descriptor.total_chunks)
            .find(|i| !self.bitmap.is_verified(*i) && !self.inflight_chunks.contains_key(i));
        let Some(chunk_index) = next_idx else {
            return Ok(None);
        };
        self.inflight_chunks.insert(chunk_index, peer);
        Ok(Some(RegistryChunkWireMessageV1::ChunkRequest {
            release_root: self.descriptor.release_root,
            chunk_index,
        }))
    }

    /// Verify a delivered `4 MiB` chunk against its `ChunkCid` Merkle branch BEFORE
    /// writing to `registry/staging/<release_sequence>/`. Corrupt chunks immediately
    /// penalize `peer` and release the inflight slot for retry from an alternate source
    /// while preserving all already-verified chunks.
    pub fn ingest_chunk_delivery(
        &mut self,
        peer: NodeId,
        delivery: &RegistryChunkWireMessageV1,
        penalty_hook: &mut dyn PeerTransportPenaltyHook,
    ) -> Result<bool, ChunkStreamError> {
        let RegistryChunkWireMessageV1::ChunkDelivery {
            release_root,
            chunk_index,
            chunk_bytes,
            merkle_proof,
        } = delivery
        else {
            return Err(ChunkStreamError::UnexpectedMessage);
        };

        self.inflight_chunks.remove(chunk_index);

        if *release_root != self.descriptor.release_root {
            penalty_hook.penalize_corrupt_registry_chunk(
                peer,
                self.descriptor.release_root,
                *chunk_index,
            );
            return Err(ChunkStreamError::CorruptRegistryChunk {
                peer,
                chunk_index: *chunk_index,
            });
        }
        let Some(expected_len) = self.descriptor.expected_chunk_len(*chunk_index) else {
            penalty_hook.penalize_corrupt_registry_chunk(
                peer,
                self.descriptor.release_root,
                *chunk_index,
            );
            return Err(ChunkStreamError::ChunkIndexOutOfBounds);
        };
        if chunk_bytes.len() != expected_len {
            penalty_hook.penalize_corrupt_registry_chunk(
                peer,
                self.descriptor.release_root,
                *chunk_index,
            );
            return Err(ChunkStreamError::CorruptRegistryChunk {
                peer,
                chunk_index: *chunk_index,
            });
        }

        let chunk_cid = compute_registry_chunk_cid(u64::from(*chunk_index), chunk_bytes);
        if !verify_registry_chunk_merkle_proof(
            &self.descriptor.chunk_merkle_root,
            self.descriptor.total_chunks,
            *chunk_index,
            &chunk_cid,
            merkle_proof,
        ) {
            penalty_hook.penalize_corrupt_registry_chunk(
                peer,
                self.descriptor.release_root,
                *chunk_index,
            );
            return Err(ChunkStreamError::CorruptRegistryChunk {
                peer,
                chunk_index: *chunk_index,
            });
        }

        if self.bitmap.is_verified(*chunk_index) {
            return Ok(false);
        }

        let offset = u64::from(*chunk_index) * (REGISTRY_CHUNK_BYTES as u64);
        let mut f = OpenOptions::new()
            .write(true)
            .open(&self.package_part_path)
            .map_err(ChunkStreamError::Io)?;
        f.seek(SeekFrom::Start(offset))
            .map_err(ChunkStreamError::Io)?;
        f.write_all(chunk_bytes).map_err(ChunkStreamError::Io)?;
        f.sync_all().map_err(ChunkStreamError::Io)?;

        self.bitmap.mark_verified(*chunk_index)?;
        self.bitmap.save_atomic(&self.bitmap_path)?;
        Ok(true)
    }

    /// Finalize staged package once all chunks are verified and full `release_root`
    /// check (`verify_full_release_root`) passes (`P1` §3.2(3)).
    pub fn finalize_staged_release<F>(
        self,
        verify_full_release_root: F,
    ) -> Result<PathBuf, ChunkStreamError>
    where
        F: FnOnce(&Path) -> Result<[u8; 32], String>,
    {
        if !self.bitmap.is_complete() {
            return Err(ChunkStreamError::IncompleteTransfer);
        }
        let observed_root = verify_full_release_root(&self.package_part_path)
            .map_err(ChunkStreamError::ReleaseRootVerificationFailed)?;
        if observed_root != self.descriptor.release_root {
            return Err(ChunkStreamError::ReleaseRootVerificationFailed(
                "observed release_root mismatch".to_owned(),
            ));
        }
        let final_path = self.staging_dir.join("release.obr");
        fs::rename(&self.package_part_path, &final_path).map_err(ChunkStreamError::Io)?;
        let _ = fs::remove_file(&self.bitmap_path);
        Ok(final_path)
    }
}

/// Untrusted mirror / peer responder serving `obp/registry-chunks/1` (`D-012`).
pub struct RegistryChunkProvider {
    descriptor: RegistryReleaseDescriptorV1,
    chunks: Vec<Vec<u8>>,
    chunk_cids: Vec<[u8; 32]>,
}

impl RegistryChunkProvider {
    pub fn new(
        descriptor: RegistryReleaseDescriptorV1,
        package_bytes: &[u8],
    ) -> Result<Self, ChunkStreamError> {
        if package_bytes.len() as u64 != descriptor.total_bytes {
            return Err(ChunkStreamError::InvalidManifest);
        }
        let chunks: Vec<Vec<u8>> = package_bytes
            .chunks(REGISTRY_CHUNK_BYTES)
            .map(|c| c.to_vec())
            .collect();
        let chunk_cids: Vec<[u8; 32]> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| compute_registry_chunk_cid(i as u64, c))
            .collect();
        if compute_registry_chunk_merkle_root(&chunk_cids) != descriptor.chunk_merkle_root {
            return Err(ChunkStreamError::InvalidManifest);
        }
        Ok(Self {
            descriptor,
            chunks,
            chunk_cids,
        })
    }

    pub fn handle_request(
        &self,
        request: &RegistryChunkWireMessageV1,
    ) -> Result<RegistryChunkWireMessageV1, ChunkStreamError> {
        match request {
            RegistryChunkWireMessageV1::DescriptorRequest {
                release_root,
                min_release_sequence,
            } => {
                if self.descriptor.release_sequence < *min_release_sequence {
                    return Err(ChunkStreamError::UnknownRegistryRelease);
                }
                if let Some(expected_root) = release_root {
                    if *expected_root != self.descriptor.release_root {
                        return Err(ChunkStreamError::UnknownRegistryRelease);
                    }
                }
                Ok(RegistryChunkWireMessageV1::DescriptorResponse {
                    descriptor: self.descriptor.clone(),
                })
            }
            RegistryChunkWireMessageV1::ChunkRequest {
                release_root,
                chunk_index,
            } => {
                if *release_root != self.descriptor.release_root {
                    return Err(ChunkStreamError::UnknownRegistryRelease);
                }
                let Some(chunk_bytes) = self.chunks.get(*chunk_index as usize) else {
                    return Err(ChunkStreamError::ChunkIndexOutOfBounds);
                };
                let merkle_proof =
                    build_registry_chunk_merkle_proof(&self.chunk_cids, *chunk_index)?;
                Ok(RegistryChunkWireMessageV1::ChunkDelivery {
                    release_root: self.descriptor.release_root,
                    chunk_index: *chunk_index,
                    chunk_bytes: chunk_bytes.clone(),
                    merkle_proof,
                })
            }
            _ => Err(ChunkStreamError::UnexpectedMessage),
        }
    }
}

// ─── QUIC Bidirectional Stream Drivers over `OBPConnection` (`quic`) ─────────

#[cfg(feature = "quic")]
pub async fn fetch_blob_over_quic(
    connection: &OBPConnection,
    oracle: &dyn CanonicalBlobReferenceOracle,
    blob_cid: BlobCid,
    staging_dir: &Path,
    blob_store_root: &Path,
) -> Result<PathBuf, ChunkStreamError> {
    let manifest_req = authorized_blob_manifest_request(oracle, blob_cid)?;
    let req_bytes =
        encode_blob_chunk_message(&BlobChunkWireMessageV1::ManifestRequest(manifest_req))?;
    let resp_bytes = connection
        .request_with_limit(&req_bytes, MAX_BLOB_WIRE_FRAME_BYTES)
        .await
        .map_err(ChunkStreamError::Transport)?;
    let BlobChunkWireMessageV1::Manifest(manifest) = decode_blob_chunk_message(&resp_bytes)? else {
        return Err(ChunkStreamError::UnexpectedMessage);
    };
    if manifest.blob_cid != blob_cid {
        return Err(ChunkStreamError::BlobCidMismatch);
    }

    let mut receiver = BlobChunkReceiver::open_with_reference_proof(
        oracle,
        manifest,
        staging_dir,
        blob_store_root,
    )?;
    while let Some(fetch) = receiver.next_fetch() {
        let fetch_bytes = encode_blob_chunk_message(&BlobChunkWireMessageV1::Fetch(fetch))?;
        let batch_bytes = connection
            .request_with_limit(&fetch_bytes, MAX_BLOB_WIRE_FRAME_BYTES)
            .await
            .map_err(ChunkStreamError::Transport)?;
        let BlobChunkWireMessageV1::Batch(batch) = decode_blob_chunk_message(&batch_bytes)? else {
            return Err(ChunkStreamError::UnexpectedMessage);
        };
        receiver.ingest_batch(&batch)?;
    }
    receiver.finalize_and_promote()
}

#[cfg(feature = "quic")]
pub async fn serve_one_blob_chunk_request_over_quic(
    connection: &OBPConnection,
    provider: &BlobChunkProvider,
) -> Result<(), ChunkStreamError> {
    let (req_bytes, responder) = connection
        .accept_bi_with_limit(MAX_BLOB_WIRE_FRAME_BYTES)
        .await
        .map_err(ChunkStreamError::Transport)?;
    let req = decode_blob_chunk_message(&req_bytes)?;
    let resp = provider.handle_request(&req)?;
    let resp_bytes = encode_blob_chunk_message(&resp)?;
    responder
        .respond_with_limit(&resp_bytes, MAX_BLOB_WIRE_FRAME_BYTES)
        .await
        .map_err(ChunkStreamError::Transport)
}

#[cfg(feature = "quic")]
pub async fn request_registry_message_over_quic(
    connection: &OBPConnection,
    request: &RegistryChunkWireMessageV1,
) -> Result<RegistryChunkWireMessageV1, ChunkStreamError> {
    let req_bytes = encode_registry_chunk_message(request)?;
    let resp_bytes = connection
        .request_with_limit(&req_bytes, MAX_REGISTRY_WIRE_FRAME_BYTES)
        .await
        .map_err(ChunkStreamError::Transport)?;
    Ok(decode_registry_chunk_message(&resp_bytes)?)
}

#[cfg(feature = "quic")]
pub async fn serve_one_registry_chunk_request_over_quic(
    connection: &OBPConnection,
    provider: &RegistryChunkProvider,
) -> Result<(), ChunkStreamError> {
    let (req_bytes, responder) = connection
        .accept_bi_with_limit(MAX_REGISTRY_WIRE_FRAME_BYTES)
        .await
        .map_err(ChunkStreamError::Transport)?;
    let req = decode_registry_chunk_message(&req_bytes)?;
    let resp = provider.handle_request(&req)?;
    let resp_bytes = encode_registry_chunk_message(&resp)?;
    responder
        .respond_with_limit(&resp_bytes, MAX_REGISTRY_WIRE_FRAME_BYTES)
        .await
        .map_err(ChunkStreamError::Transport)
}

#[derive(Debug)]
pub enum ChunkStreamError {
    Codec(ChunkTransferCodecError),
    Io(std::io::Error),
    #[cfg(feature = "quic")]
    Transport(TransportError),
    UnreferencedBlobRejected,
    InvalidManifest,
    CorruptStagingBitmap,
    ChunkIndexOutOfBounds,
    CorruptBlobChunk(u32),
    CorruptRegistryChunk {
        peer: NodeId,
        chunk_index: u32,
    },
    BlobCidMismatch,
    UnknownBlob,
    UnknownRegistryRelease,
    RegistryInflightCapExceeded,
    IncompleteTransfer,
    ReleaseRootVerificationFailed(String),
    UnexpectedMessage,
}

impl From<ChunkTransferCodecError> for ChunkStreamError {
    fn from(err: ChunkTransferCodecError) -> Self {
        Self::Codec(err)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::blob_store::BlobType;
    use ku_core::foundation::{BlobRetentionState, ObjectReference, OwnedBlobRole};
    use onebrain_protocol::sign_registry_release_descriptor;

    use super::*;

    struct TestBlobOracle {
        allowed: Option<OwnedBlobReferenceV1>,
    }

    impl CanonicalBlobReferenceOracle for TestBlobOracle {
        fn prove_accepted_reference(&self, blob_cid: &BlobCid) -> Option<OwnedBlobReferenceV1> {
            self.allowed
                .as_ref()
                .filter(|r| &r.blob_cid == blob_cid)
                .cloned()
        }
    }

    #[derive(Default)]
    struct TestPenaltyHook {
        penalties: Vec<(NodeId, [u8; 32], u32)>,
    }

    impl PeerTransportPenaltyHook for TestPenaltyHook {
        fn penalize_corrupt_registry_chunk(
            &mut self,
            peer: NodeId,
            release_root: [u8; 32],
            chunk_index: u32,
        ) {
            self.penalties.push((peer, release_root, chunk_index));
        }
    }

    #[test]
    fn blob_chunk_transfer_enforces_oracle_resumes_across_reopen_and_promotes_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");
        let store_root = dir.path().join("blobs");

        let data = vec![0x7Cu8; BLOB_CHUNK_BYTES * 4 + 7_777];
        let cid = BlobCid::from_content(BlobType::Document, &data);
        let mut provider = BlobChunkProvider::default();
        provider.insert_blob(cid, data.clone()).unwrap();

        // Unreferenced blob MUST be rejected!
        let empty_oracle = TestBlobOracle { allowed: None };
        assert!(matches!(
            authorized_blob_manifest_request(&empty_oracle, cid),
            Err(ChunkStreamError::UnreferencedBlobRejected)
        ));

        let reference = OwnedBlobReferenceV1::new(
            ObjectReference::new(0, [0x11; 32]),
            cid,
            OwnedBlobRole::SourceArtifact,
            BlobRetentionState::Live,
            None,
        )
        .unwrap();
        let oracle = TestBlobOracle {
            allowed: Some(reference),
        };
        let req = authorized_blob_manifest_request(&oracle, cid).unwrap();
        let BlobChunkWireMessageV1::Manifest(manifest) = provider
            .handle_request(&BlobChunkWireMessageV1::ManifestRequest(req))
            .unwrap()
        else {
            unreachable!()
        };

        // Receive first 3-chunk batch, then simulate crash/reopen!
        {
            let mut receiver = BlobChunkReceiver::open_with_reference_proof(
                &oracle,
                manifest.clone(),
                &staging,
                &store_root,
            )
            .unwrap();
            let first_fetch = receiver.next_fetch().unwrap();
            assert_eq!(first_fetch.start_chunk, 0);
            assert_eq!(first_fetch.count, 3);
            let BlobChunkWireMessageV1::Batch(batch) = provider
                .handle_request(&BlobChunkWireMessageV1::Fetch(first_fetch))
                .unwrap()
            else {
                unreachable!()
            };
            assert_eq!(receiver.ingest_batch(&batch).unwrap(), 3);
        }

        // Reopen from `staging.bitmap` and complete remaining 2 chunks.
        let mut resumed =
            BlobChunkReceiver::open_with_reference_proof(&oracle, manifest, &staging, &store_root)
                .unwrap();
        assert_eq!(resumed.bitmap().verified_count(), 3);
        let second_fetch = resumed.next_fetch().unwrap();
        assert_eq!(second_fetch.start_chunk, 3);
        assert_eq!(second_fetch.count, 2);
        let BlobChunkWireMessageV1::Batch(batch2) = provider
            .handle_request(&BlobChunkWireMessageV1::Fetch(second_fetch))
            .unwrap()
        else {
            unreachable!()
        };
        resumed.ingest_batch(&batch2).unwrap();
        let final_path = resumed.finalize_and_promote().unwrap();
        assert_eq!(final_path, canonical_blob_final_path(&store_root, &cid));
        assert_eq!(fs::read(&final_path).unwrap(), data);
    }

    #[test]
    fn registry_pull_enforces_4_inflight_cap_penalizes_corrupt_peer_and_keeps_verified_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let signer = SigningKey::from_bytes(&[0x33; 32]);
        let approved = [*signer.verifying_key().as_bytes()];

        // 5 chunks (4 * 4 MiB + 128 KiB)
        let total_len = REGISTRY_CHUNK_BYTES * 4 + 131_072;
        let package: Vec<u8> = (0..total_len).map(|i| (i % 251) as u8).collect();
        let release_root = *blake3::hash(&package).as_bytes();
        let chunk_cids: Vec<[u8; 32]> = package
            .chunks(REGISTRY_CHUNK_BYTES)
            .enumerate()
            .map(|(i, c)| compute_registry_chunk_cid(i as u64, c))
            .collect();
        let mut descriptor = RegistryReleaseDescriptorV1 {
            release_sequence: 1,
            release_root,
            total_bytes: total_len as u64,
            chunk_size_bytes: REGISTRY_CHUNK_BYTES as u32,
            total_chunks: 5,
            chunk_merkle_root: compute_registry_chunk_merkle_root(&chunk_cids),
            optional_delta_from: None,
            minimum_compatible_reader_version: 1,
            signer_public_key: [0; 32],
            signature: [0; 64],
        };
        sign_registry_release_descriptor(&mut descriptor, &signer).unwrap();

        let provider = RegistryChunkProvider::new(descriptor.clone(), &package).unwrap();
        let mut session =
            RegistryChunkPullSession::open_verified(descriptor, &approved, dir.path()).unwrap();

        let peer_good = NodeId::from_bytes([0x01; 32]);
        let peer_bad = NodeId::from_bytes([0x02; 32]);

        let mut requests = Vec::new();
        for _ in 0..4 {
            requests.push(
                session
                    .schedule_next_chunk_request(peer_good)
                    .unwrap()
                    .unwrap(),
            );
        }
        assert!(matches!(
            session.schedule_next_chunk_request(peer_good),
            Err(ChunkStreamError::RegistryInflightCapExceeded)
        ));

        let mut penalties = TestPenaltyHook::default();
        // Deliver chunks 0..3 validly, corrupt chunk 3 from peer_bad, then retry from peer_good!
        for (idx, req) in requests.into_iter().enumerate() {
            let mut delivery = provider.handle_request(&req).unwrap();
            if idx == 3 {
                if let RegistryChunkWireMessageV1::ChunkDelivery { chunk_bytes, .. } = &mut delivery
                {
                    chunk_bytes[0] ^= 0xff;
                }
                assert!(matches!(
                    session.ingest_chunk_delivery(peer_bad, &delivery, &mut penalties),
                    Err(ChunkStreamError::CorruptRegistryChunk { .. })
                ));
                assert_eq!(penalties.penalties, vec![(peer_bad, release_root, 3)]);
                assert_eq!(
                    session.bitmap().verified_count(),
                    3,
                    "previously verified chunks remain intact"
                );
            } else {
                assert!(session
                    .ingest_chunk_delivery(peer_good, &delivery, &mut penalties)
                    .unwrap());
            }
        }

        // Complete remaining chunks 3 and 4 from peer_good
        while let Some(req) = session.schedule_next_chunk_request(peer_good).unwrap() {
            let delivery = provider.handle_request(&req).unwrap();
            session
                .ingest_chunk_delivery(peer_good, &delivery, &mut penalties)
                .unwrap();
        }
        let final_obr = session
            .finalize_staged_release(|p| Ok(*blake3::hash(&fs::read(p).unwrap()).as_bytes()))
            .unwrap();
        assert_eq!(fs::read(final_obr).unwrap(), package);
    }
}
