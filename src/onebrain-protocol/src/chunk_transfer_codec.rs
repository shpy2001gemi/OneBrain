//! Chunked blob transport (`obp/blob-chunks/1`, `P2` §5) and `D-012` node-assisted
//! content-addressed Concept Registry distribution (`obp/registry-chunks/1`, `P1` §3).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::blob_store::BlobCid;
use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ManifestCid,
    ResourceProfile,
};

pub const CHUNK_TRANSFER_SCHEMA_MAJOR: u64 = 1;

// ─── obp/blob-chunks/1 Constants ─────────────────────────────────────────────

pub const BLOB_CHUNK_BYTES: usize = 262_144; // 256 KiB
pub const MAX_CHUNKS_PER_BLOB_FRAME: u16 = 3; // 768 KiB < 1 MiB frame cap
pub const MAX_BLOB_TOTAL_BYTES: u64 = 104_857_600; // 100 MiB
pub const MAX_BLOB_CHUNKS: usize = 400; // 100 MiB / 256 KiB
pub const MAX_BLOB_WIRE_FRAME_BYTES: usize = 1_048_576; // 1 MiB

// ─── obp/registry-chunks/1 Constants (D-012) ─────────────────────────────────

pub const REGISTRY_CHUNK_BYTES: usize = 4_194_304; // 4 MiB
pub const MAX_INFLIGHT_REGISTRY_CHUNKS: usize = 4; // 16 MiB RAM cap
pub const MAX_REGISTRY_CHUNKS: u32 = 65_536; // up to 256 GiB theoretical ceiling
pub const MAX_REGISTRY_MERKLE_PROOF_DEPTH: usize = 20;
pub const MAX_REGISTRY_WIRE_FRAME_BYTES: usize = REGISTRY_CHUNK_BYTES + 65_536;

pub const REGISTRY_RELEASE_DESCRIPTOR_DOMAIN: &[u8] =
    b"onebrain:concept-registry:release-descriptor:1\0";
pub const REGISTRY_CHUNK_CID_DOMAIN: &[u8] = b"onebrain:registry-chunk:1\0";

const REGISTRY_DELIVERY_FRAME_MAGIC: [u8; 4] = *b"OBRC";
const REGISTRY_DELIVERY_FRAME_VERSION: u8 = 1;

pub mod chunk_wire_id {
    pub const BLOB_MANIFEST_REQUEST: u64 = 1;
    pub const BLOB_CHUNK_MANIFEST: u64 = 2;
    pub const BLOB_CHUNK_FETCH: u64 = 3;
    pub const BLOB_CHUNK_BATCH: u64 = 4;

    pub const REGISTRY_DESCRIPTOR_REQUEST: u64 = 10;
    pub const REGISTRY_DESCRIPTOR_RESPONSE: u64 = 11;
    pub const REGISTRY_CHUNK_REQUEST: u64 = 12;
    pub const REGISTRY_CHUNK_DELIVERY_HEADER: u64 = 13;
}

// ─── obp/blob-chunks/1 Types & Merkle Binding ────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobManifestRequest {
    pub blob_cid: BlobCid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobChunkManifest {
    pub blob_cid: BlobCid,
    pub total_bytes: u64,
    pub chunk_size: u32,
    pub chunk_hashes: Vec<[u8; 32]>,
    pub chunk_merkle_root: [u8; 32],
}

impl BlobChunkManifest {
    pub fn from_bytes(blob_cid: BlobCid, data: &[u8]) -> Result<Self, ChunkTransferCodecError> {
        validate_blob_cid(&blob_cid)?;
        if data.is_empty() || (data.len() as u64) > MAX_BLOB_TOTAL_BYTES {
            return Err(ChunkTransferCodecError::InvalidBlobLength);
        }
        let recomputed = BlobCid::from_content(blob_cid.blob_type(), data);
        if recomputed != blob_cid {
            return Err(ChunkTransferCodecError::BlobCidMismatch);
        }
        let chunk_hashes: Vec<[u8; 32]> = data
            .chunks(BLOB_CHUNK_BYTES)
            .map(|chunk| *blake3::hash(chunk).as_bytes())
            .collect();
        let total_bytes = data.len() as u64;
        let chunk_merkle_root =
            compute_blob_chunk_merkle_root(&blob_cid, total_bytes, &chunk_hashes);
        Ok(Self {
            blob_cid,
            total_bytes,
            chunk_size: BLOB_CHUNK_BYTES as u32,
            chunk_hashes,
            chunk_merkle_root,
        })
    }

    pub fn expected_chunk_len(&self, chunk_index: u32) -> Option<usize> {
        let count = self.chunk_hashes.len() as u32;
        if chunk_index >= count {
            return None;
        }
        if chunk_index + 1 < count {
            Some(BLOB_CHUNK_BYTES)
        } else {
            let rem = (self.total_bytes % (BLOB_CHUNK_BYTES as u64)) as usize;
            Some(if rem == 0 { BLOB_CHUNK_BYTES } else { rem })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlobChunkFetch {
    pub blob_cid: BlobCid,
    pub start_chunk: u32,
    pub count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobChunkItemV1 {
    pub chunk_index: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobChunkBatch {
    pub blob_cid: BlobCid,
    pub chunks: Vec<BlobChunkItemV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlobChunkWireMessageV1 {
    ManifestRequest(BlobManifestRequest),
    Manifest(BlobChunkManifest),
    Fetch(BlobChunkFetch),
    Batch(BlobChunkBatch),
}

/// Deterministically bind the ordered 256 KiB `chunk_hashes` binary Merkle tree
/// to the 34-byte `BlobCid` and declared `total_bytes`.
pub fn compute_blob_chunk_merkle_root(
    blob_cid: &BlobCid,
    total_bytes: u64,
    chunk_hashes: &[[u8; 32]],
) -> [u8; 32] {
    let leaves: Vec<[u8; 32]> = chunk_hashes
        .iter()
        .enumerate()
        .map(|(index, hash)| {
            let mut h = blake3::Hasher::new();
            h.update(b"onebrain:vnext:blob-chunk-leaf:1\0");
            h.update(&(index as u32).to_be_bytes());
            h.update(hash);
            *h.finalize().as_bytes()
        })
        .collect();
    let tree_root = binary_merkle_root(b"onebrain:vnext:blob-chunk-branch:1\0", &leaves);
    let mut bind = blake3::Hasher::new();
    bind.update(b"onebrain:vnext:blob-chunk-manifest-root:1\0");
    bind.update(&blob_cid.0);
    bind.update(&total_bytes.to_be_bytes());
    bind.update(&tree_root);
    *bind.finalize().as_bytes()
}

pub fn encode_blob_chunk_message(
    msg: &BlobChunkWireMessageV1,
) -> Result<Vec<u8>, ChunkTransferCodecError> {
    validate_blob_message(msg)?;
    let (wire_id, body) = match msg {
        BlobChunkWireMessageV1::ManifestRequest(req) => (
            chunk_wire_id::BLOB_MANIFEST_REQUEST,
            CanonicalValue::Map(vec![(0, CanonicalValue::Bytes(req.blob_cid.0.to_vec()))]),
        ),
        BlobChunkWireMessageV1::Manifest(manifest) => (
            chunk_wire_id::BLOB_CHUNK_MANIFEST,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(manifest.blob_cid.0.to_vec())),
                (1, CanonicalValue::Unsigned(manifest.total_bytes)),
                (2, CanonicalValue::Unsigned(u64::from(manifest.chunk_size))),
                (
                    3,
                    CanonicalValue::Array(
                        manifest
                            .chunk_hashes
                            .iter()
                            .map(|h| CanonicalValue::Bytes(h.to_vec()))
                            .collect(),
                    ),
                ),
                (
                    4,
                    CanonicalValue::Bytes(manifest.chunk_merkle_root.to_vec()),
                ),
            ]),
        ),
        BlobChunkWireMessageV1::Fetch(fetch) => (
            chunk_wire_id::BLOB_CHUNK_FETCH,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(fetch.blob_cid.0.to_vec())),
                (1, CanonicalValue::Unsigned(u64::from(fetch.start_chunk))),
                (2, CanonicalValue::Unsigned(u64::from(fetch.count))),
            ]),
        ),
        BlobChunkWireMessageV1::Batch(batch) => (
            chunk_wire_id::BLOB_CHUNK_BATCH,
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(batch.blob_cid.0.to_vec())),
                (
                    1,
                    CanonicalValue::Array(
                        batch
                            .chunks
                            .iter()
                            .map(|c| {
                                CanonicalValue::Map(vec![
                                    (0, CanonicalValue::Unsigned(u64::from(c.chunk_index))),
                                    (1, CanonicalValue::Bytes(c.bytes.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        ),
    };
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(CHUNK_TRANSFER_SCHEMA_MAJOR)),
            (1, CanonicalValue::Unsigned(wire_id)),
            (2, body),
        ]),
        ResourceProfile::ObjectV1,
    )?;
    if bytes.len() > MAX_BLOB_WIRE_FRAME_BYTES {
        return Err(ChunkTransferCodecError::FrameLimit);
    }
    Ok(bytes)
}

pub fn decode_blob_chunk_message(
    bytes: &[u8],
) -> Result<BlobChunkWireMessageV1, ChunkTransferCodecError> {
    if bytes.is_empty() || bytes.len() > MAX_BLOB_WIRE_FRAME_BYTES {
        return Err(ChunkTransferCodecError::FrameLimit);
    }
    let root_val = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
    let root = as_map(&root_val, "blob.root")?;
    if unsigned(root, 0, "blob.major")? != CHUNK_TRANSFER_SCHEMA_MAJOR {
        return Err(ChunkTransferCodecError::UnsupportedMajor);
    }
    let wire_id = unsigned(root, 1, "blob.wire_id")?;
    let body = as_map(required(root, 2, "blob.body")?, "blob.body")?;
    let parsed = match wire_id {
        chunk_wire_id::BLOB_MANIFEST_REQUEST => {
            BlobChunkWireMessageV1::ManifestRequest(BlobManifestRequest {
                blob_cid: parse_blob_cid(body, 0, "blob_req.cid")?,
            })
        }
        chunk_wire_id::BLOB_CHUNK_MANIFEST => {
            let chunk_size = u32::try_from(unsigned(body, 2, "blob_manifest.chunk_size")?)
                .map_err(|_| ChunkTransferCodecError::InvalidField("blob_manifest.chunk_size"))?;
            let chunk_hashes = array(body, 3, "blob_manifest.chunk_hashes")?
                .iter()
                .map(|v| bytes32_val(v, "blob_manifest.chunk_hash"))
                .collect::<Result<Vec<_>, _>>()?;
            BlobChunkWireMessageV1::Manifest(BlobChunkManifest {
                blob_cid: parse_blob_cid(body, 0, "blob_manifest.cid")?,
                total_bytes: unsigned(body, 1, "blob_manifest.total_bytes")?,
                chunk_size,
                chunk_hashes,
                chunk_merkle_root: bytes32(body, 4, "blob_manifest.merkle_root")?,
            })
        }
        chunk_wire_id::BLOB_CHUNK_FETCH => {
            let start_chunk = u32::try_from(unsigned(body, 1, "blob_fetch.start")?)
                .map_err(|_| ChunkTransferCodecError::ResourceLimit)?;
            let count = u16::try_from(unsigned(body, 2, "blob_fetch.count")?)
                .map_err(|_| ChunkTransferCodecError::ResourceLimit)?;
            BlobChunkWireMessageV1::Fetch(BlobChunkFetch {
                blob_cid: parse_blob_cid(body, 0, "blob_fetch.cid")?,
                start_chunk,
                count,
            })
        }
        chunk_wire_id::BLOB_CHUNK_BATCH => {
            let chunks = array(body, 1, "blob_batch.chunks")?
                .iter()
                .map(|v| {
                    let m = as_map(v, "blob_batch.item")?;
                    let chunk_index = u32::try_from(unsigned(m, 0, "blob_batch.index")?)
                        .map_err(|_| ChunkTransferCodecError::ResourceLimit)?;
                    Ok(BlobChunkItemV1 {
                        chunk_index,
                        bytes: byte_string(m, 1, "blob_batch.bytes")?.to_vec(),
                    })
                })
                .collect::<Result<Vec<_>, ChunkTransferCodecError>>()?;
            BlobChunkWireMessageV1::Batch(BlobChunkBatch {
                blob_cid: parse_blob_cid(body, 0, "blob_batch.cid")?,
                chunks,
            })
        }
        other => return Err(ChunkTransferCodecError::UnknownWireId(other)),
    };
    validate_blob_message(&parsed)?;
    if encode_blob_chunk_message(&parsed)? != bytes {
        return Err(ChunkTransferCodecError::NonCanonical);
    }
    Ok(parsed)
}

fn validate_blob_message(msg: &BlobChunkWireMessageV1) -> Result<(), ChunkTransferCodecError> {
    match msg {
        BlobChunkWireMessageV1::ManifestRequest(req) => validate_blob_cid(&req.blob_cid),
        BlobChunkWireMessageV1::Manifest(manifest) => {
            validate_blob_cid(&manifest.blob_cid)?;
            if manifest.chunk_size != BLOB_CHUNK_BYTES as u32 {
                return Err(ChunkTransferCodecError::InvalidField(
                    "blob_manifest.chunk_size",
                ));
            }
            if manifest.total_bytes == 0 || manifest.total_bytes > MAX_BLOB_TOTAL_BYTES {
                return Err(ChunkTransferCodecError::InvalidBlobLength);
            }
            let expected_chunks = manifest.total_bytes.div_ceil(BLOB_CHUNK_BYTES as u64) as usize;
            if manifest.chunk_hashes.len() != expected_chunks
                || manifest.chunk_hashes.len() > MAX_BLOB_CHUNKS
            {
                return Err(ChunkTransferCodecError::InvalidBlobLength);
            }
            if manifest.chunk_hashes.len() == 1
                && &manifest.chunk_hashes[0] != manifest.blob_cid.blake3_hash()
            {
                return Err(ChunkTransferCodecError::BlobCidMismatch);
            }
            let expected_root = compute_blob_chunk_merkle_root(
                &manifest.blob_cid,
                manifest.total_bytes,
                &manifest.chunk_hashes,
            );
            if manifest.chunk_merkle_root != expected_root {
                return Err(ChunkTransferCodecError::MerkleRootMismatch);
            }
            Ok(())
        }
        BlobChunkWireMessageV1::Fetch(fetch) => {
            validate_blob_cid(&fetch.blob_cid)?;
            if fetch.count == 0
                || fetch.count > MAX_CHUNKS_PER_BLOB_FRAME
                || fetch.start_chunk as usize >= MAX_BLOB_CHUNKS
            {
                return Err(ChunkTransferCodecError::ResourceLimit);
            }
            Ok(())
        }
        BlobChunkWireMessageV1::Batch(batch) => {
            validate_blob_cid(&batch.blob_cid)?;
            if batch.chunks.is_empty() || batch.chunks.len() > MAX_CHUNKS_PER_BLOB_FRAME as usize {
                return Err(ChunkTransferCodecError::ResourceLimit);
            }
            for item in &batch.chunks {
                if item.bytes.is_empty()
                    || item.bytes.len() > BLOB_CHUNK_BYTES
                    || item.chunk_index as usize >= MAX_BLOB_CHUNKS
                {
                    return Err(ChunkTransferCodecError::InvalidChunkLength);
                }
            }
            if !batch
                .chunks
                .windows(2)
                .all(|pair| pair[0].chunk_index < pair[1].chunk_index)
            {
                return Err(ChunkTransferCodecError::NonCanonical);
            }
            Ok(())
        }
    }
}

fn validate_blob_cid(cid: &BlobCid) -> Result<(), ChunkTransferCodecError> {
    if BlobCid::from_hex(&cid.to_hex()) != Some(*cid) {
        Err(ChunkTransferCodecError::InvalidBlobCid)
    } else {
        Ok(())
    }
}

// ─── obp/registry-chunks/1 Types & Merkle Proofs (D-012) ─────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistryDeltaBaseV1 {
    pub from_release_sequence: u64,
    pub from_release_root: [u8; 32],
    pub patch_manifest_cid: ManifestCid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryReleaseDescriptorV1 {
    pub release_sequence: u64,
    pub release_root: [u8; 32],
    pub total_bytes: u64,
    pub chunk_size_bytes: u32,
    pub total_chunks: u32,
    pub chunk_merkle_root: [u8; 32],
    pub optional_delta_from: Option<RegistryDeltaBaseV1>,
    pub minimum_compatible_reader_version: u32,
    pub signer_public_key: [u8; 32],
    pub signature: [u8; 64],
}

impl RegistryReleaseDescriptorV1 {
    pub fn expected_chunk_len(&self, chunk_index: u32) -> Option<usize> {
        if chunk_index >= self.total_chunks {
            return None;
        }
        if chunk_index + 1 < self.total_chunks {
            Some(REGISTRY_CHUNK_BYTES)
        } else {
            let rem = (self.total_bytes % (REGISTRY_CHUNK_BYTES as u64)) as usize;
            Some(if rem == 0 { REGISTRY_CHUNK_BYTES } else { rem })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryChunkMerkleProofV1 {
    pub siblings: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryChunkWireMessageV1 {
    DescriptorRequest {
        release_root: Option<[u8; 32]>,
        min_release_sequence: u64,
    },
    DescriptorResponse {
        descriptor: RegistryReleaseDescriptorV1,
    },
    ChunkRequest {
        release_root: [u8; 32],
        chunk_index: u32,
    },
    ChunkDelivery {
        release_root: [u8; 32],
        chunk_index: u32,
        chunk_bytes: Vec<u8>,
        merkle_proof: RegistryChunkMerkleProofV1,
    },
}

/// Compute `ChunkCid = BLAKE3("onebrain:registry-chunk:1\0" || u64_be(index) || bytes)` (`P1` §3.2).
pub fn compute_registry_chunk_cid(index: u64, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(REGISTRY_CHUNK_CID_DOMAIN);
    hasher.update(&index.to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

pub fn compute_registry_chunk_merkle_root(chunk_cids: &[[u8; 32]]) -> [u8; 32] {
    binary_merkle_root(b"onebrain:vnext:registry-chunk-branch:1\0", chunk_cids)
}

pub fn build_registry_chunk_merkle_proof(
    chunk_cids: &[[u8; 32]],
    index: u32,
) -> Result<RegistryChunkMerkleProofV1, ChunkTransferCodecError> {
    let idx = index as usize;
    if chunk_cids.is_empty() || idx >= chunk_cids.len() {
        return Err(ChunkTransferCodecError::InvalidField("merkle.index"));
    }
    let mut level = chunk_cids.to_vec();
    let mut pos = idx;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let sibling = if pos.is_multiple_of(2) {
            if pos + 1 < level.len() {
                level[pos + 1]
            } else {
                level[pos]
            }
        } else {
            level[pos - 1]
        };
        siblings.push(sibling);
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            next.push(merkle_branch(
                b"onebrain:vnext:registry-chunk-branch:1\0",
                &left,
                &right,
            ));
        }
        level = next;
        pos /= 2;
    }
    Ok(RegistryChunkMerkleProofV1 { siblings })
}

pub fn verify_registry_chunk_merkle_proof(
    chunk_merkle_root: &[u8; 32],
    total_chunks: u32,
    index: u32,
    chunk_cid: &[u8; 32],
    proof: &RegistryChunkMerkleProofV1,
) -> bool {
    if total_chunks == 0
        || index >= total_chunks
        || proof.siblings.len() > MAX_REGISTRY_MERKLE_PROOF_DEPTH
    {
        return false;
    }
    let mut width = total_chunks as usize;
    let mut pos = index as usize;
    let mut current = *chunk_cid;
    let mut step = 0usize;
    while width > 1 {
        let Some(sibling) = proof.siblings.get(step) else {
            return false;
        };
        if pos.is_multiple_of(2) && pos + 1 == width && sibling != &current {
            return false;
        }
        current = if pos.is_multiple_of(2) {
            merkle_branch(
                b"onebrain:vnext:registry-chunk-branch:1\0",
                &current,
                sibling,
            )
        } else {
            merkle_branch(
                b"onebrain:vnext:registry-chunk-branch:1\0",
                sibling,
                &current,
            )
        };
        width = width.div_ceil(2);
        pos /= 2;
        step += 1;
    }
    step == proof.siblings.len() && &current == chunk_merkle_root
}

pub fn sign_registry_release_descriptor(
    descriptor: &mut RegistryReleaseDescriptorV1,
    signer: &SigningKey,
) -> Result<(), ChunkTransferCodecError> {
    descriptor.signer_public_key = *signer.verifying_key().as_bytes();
    validate_registry_descriptor(descriptor, false)?;
    let preimage = registry_descriptor_signing_bytes(descriptor)?;
    descriptor.signature = signer.sign(&preimage).to_bytes();
    Ok(())
}

pub fn verify_registry_release_descriptor(
    descriptor: &RegistryReleaseDescriptorV1,
    approved_signers: &[[u8; 32]],
) -> Result<(), ChunkTransferCodecError> {
    validate_registry_descriptor(descriptor, true)?;
    if !approved_signers.contains(&descriptor.signer_public_key) {
        return Err(ChunkTransferCodecError::UnapprovedRegistrySigner);
    }
    let key = VerifyingKey::from_bytes(&descriptor.signer_public_key)
        .map_err(|_| ChunkTransferCodecError::InvalidSignature)?;
    let preimage = registry_descriptor_signing_bytes(descriptor)?;
    key.verify(&preimage, &Signature::from_bytes(&descriptor.signature))
        .map_err(|_| ChunkTransferCodecError::InvalidSignature)
}

pub fn encode_registry_chunk_message(
    msg: &RegistryChunkWireMessageV1,
) -> Result<Vec<u8>, ChunkTransferCodecError> {
    validate_registry_message(msg)?;
    match msg {
        RegistryChunkWireMessageV1::DescriptorRequest {
            release_root,
            min_release_sequence,
        } => encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(CHUNK_TRANSFER_SCHEMA_MAJOR)),
                (
                    1,
                    CanonicalValue::Unsigned(chunk_wire_id::REGISTRY_DESCRIPTOR_REQUEST),
                ),
                (
                    2,
                    CanonicalValue::Map(vec![
                        (
                            0,
                            release_root.map_or(CanonicalValue::Null, |r| {
                                CanonicalValue::Bytes(r.to_vec())
                            }),
                        ),
                        (1, CanonicalValue::Unsigned(*min_release_sequence)),
                    ]),
                ),
            ]),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into),
        RegistryChunkWireMessageV1::DescriptorResponse { descriptor } => encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(CHUNK_TRANSFER_SCHEMA_MAJOR)),
                (
                    1,
                    CanonicalValue::Unsigned(chunk_wire_id::REGISTRY_DESCRIPTOR_RESPONSE),
                ),
                (2, registry_descriptor_value(descriptor, true)),
            ]),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into),
        RegistryChunkWireMessageV1::ChunkRequest {
            release_root,
            chunk_index,
        } => encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(CHUNK_TRANSFER_SCHEMA_MAJOR)),
                (
                    1,
                    CanonicalValue::Unsigned(chunk_wire_id::REGISTRY_CHUNK_REQUEST),
                ),
                (
                    2,
                    CanonicalValue::Map(vec![
                        (0, CanonicalValue::Bytes(release_root.to_vec())),
                        (1, CanonicalValue::Unsigned(u64::from(*chunk_index))),
                    ]),
                ),
            ]),
            ResourceProfile::ControlV1,
        )
        .map_err(Into::into),
        RegistryChunkWireMessageV1::ChunkDelivery {
            release_root,
            chunk_index,
            chunk_bytes,
            merkle_proof,
        } => {
            let header = encode_canonical(
                &CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(CHUNK_TRANSFER_SCHEMA_MAJOR)),
                    (
                        1,
                        CanonicalValue::Unsigned(chunk_wire_id::REGISTRY_CHUNK_DELIVERY_HEADER),
                    ),
                    (
                        2,
                        CanonicalValue::Map(vec![
                            (0, CanonicalValue::Bytes(release_root.to_vec())),
                            (1, CanonicalValue::Unsigned(u64::from(*chunk_index))),
                            (2, CanonicalValue::Unsigned(chunk_bytes.len() as u64)),
                            (
                                3,
                                CanonicalValue::Array(
                                    merkle_proof
                                        .siblings
                                        .iter()
                                        .map(|s| CanonicalValue::Bytes(s.to_vec()))
                                        .collect(),
                                ),
                            ),
                        ]),
                    ),
                ]),
                ResourceProfile::ControlV1,
            )?;
            let mut out = Vec::with_capacity(9 + header.len() + chunk_bytes.len());
            out.extend_from_slice(&REGISTRY_DELIVERY_FRAME_MAGIC);
            out.push(REGISTRY_DELIVERY_FRAME_VERSION);
            out.extend_from_slice(&(header.len() as u32).to_be_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(chunk_bytes);
            if out.len() > MAX_REGISTRY_WIRE_FRAME_BYTES {
                return Err(ChunkTransferCodecError::FrameLimit);
            }
            Ok(out)
        }
    }
}

pub fn decode_registry_chunk_message(
    bytes: &[u8],
) -> Result<RegistryChunkWireMessageV1, ChunkTransferCodecError> {
    if bytes.is_empty() || bytes.len() > MAX_REGISTRY_WIRE_FRAME_BYTES {
        return Err(ChunkTransferCodecError::FrameLimit);
    }
    if bytes.len() >= 9 && bytes[..4] == REGISTRY_DELIVERY_FRAME_MAGIC {
        if bytes[4] != REGISTRY_DELIVERY_FRAME_VERSION {
            return Err(ChunkTransferCodecError::UnsupportedMajor);
        }
        let header_len = u32::from_be_bytes(bytes[5..9].try_into().expect("4 bytes")) as usize;
        if header_len == 0
            || header_len > ResourceProfile::ControlV1.limits().max_bytes
            || bytes.len() <= 9 + header_len
        {
            return Err(ChunkTransferCodecError::FrameLimit);
        }
        let header_slice = &bytes[9..9 + header_len];
        let chunk_slice = &bytes[9 + header_len..];
        if chunk_slice.len() > REGISTRY_CHUNK_BYTES {
            return Err(ChunkTransferCodecError::InvalidChunkLength);
        }
        let header_val = decode_canonical(header_slice, ResourceProfile::ControlV1)?;
        let root = as_map(&header_val, "reg_delivery.root")?;
        if unsigned(root, 0, "reg_delivery.major")? != CHUNK_TRANSFER_SCHEMA_MAJOR
            || unsigned(root, 1, "reg_delivery.wire")?
                != chunk_wire_id::REGISTRY_CHUNK_DELIVERY_HEADER
        {
            return Err(ChunkTransferCodecError::UnknownWireId(0));
        }
        let body = as_map(required(root, 2, "reg_delivery.body")?, "reg_delivery.body")?;
        let chunk_index = u32::try_from(unsigned(body, 1, "reg_delivery.index")?)
            .map_err(|_| ChunkTransferCodecError::ResourceLimit)?;
        let declared_len = unsigned(body, 2, "reg_delivery.len")? as usize;
        if declared_len != chunk_slice.len() {
            return Err(ChunkTransferCodecError::InvalidChunkLength);
        }
        let siblings = array(body, 3, "reg_delivery.siblings")?
            .iter()
            .map(|v| bytes32_val(v, "reg_delivery.sibling"))
            .collect::<Result<Vec<_>, _>>()?;
        let parsed = RegistryChunkWireMessageV1::ChunkDelivery {
            release_root: bytes32(body, 0, "reg_delivery.release_root")?,
            chunk_index,
            chunk_bytes: chunk_slice.to_vec(),
            merkle_proof: RegistryChunkMerkleProofV1 { siblings },
        };
        validate_registry_message(&parsed)?;
        if encode_registry_chunk_message(&parsed)? != bytes {
            return Err(ChunkTransferCodecError::NonCanonical);
        }
        return Ok(parsed);
    }

    let root_val = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = as_map(&root_val, "reg.root")?;
    if unsigned(root, 0, "reg.major")? != CHUNK_TRANSFER_SCHEMA_MAJOR {
        return Err(ChunkTransferCodecError::UnsupportedMajor);
    }
    let wire_id = unsigned(root, 1, "reg.wire")?;
    let body = as_map(required(root, 2, "reg.body")?, "reg.body")?;
    let parsed = match wire_id {
        chunk_wire_id::REGISTRY_DESCRIPTOR_REQUEST => {
            RegistryChunkWireMessageV1::DescriptorRequest {
                release_root: optional_bytes32(required(body, 0, "reg_req.root")?, "reg_req.root")?,
                min_release_sequence: unsigned(body, 1, "reg_req.min_seq")?,
            }
        }
        chunk_wire_id::REGISTRY_DESCRIPTOR_RESPONSE => {
            RegistryChunkWireMessageV1::DescriptorResponse {
                descriptor: parse_registry_descriptor(body)?,
            }
        }
        chunk_wire_id::REGISTRY_CHUNK_REQUEST => {
            let chunk_index = u32::try_from(unsigned(body, 1, "reg_chunk_req.index")?)
                .map_err(|_| ChunkTransferCodecError::ResourceLimit)?;
            RegistryChunkWireMessageV1::ChunkRequest {
                release_root: bytes32(body, 0, "reg_chunk_req.root")?,
                chunk_index,
            }
        }
        other => return Err(ChunkTransferCodecError::UnknownWireId(other)),
    };
    validate_registry_message(&parsed)?;
    if encode_registry_chunk_message(&parsed)? != bytes {
        return Err(ChunkTransferCodecError::NonCanonical);
    }
    Ok(parsed)
}

fn registry_descriptor_signing_bytes(
    descriptor: &RegistryReleaseDescriptorV1,
) -> Result<Vec<u8>, ChunkTransferCodecError> {
    let canonical = encode_canonical(
        &registry_descriptor_value(descriptor, false),
        ResourceProfile::ControlV1,
    )?;
    let mut out = Vec::with_capacity(REGISTRY_RELEASE_DESCRIPTOR_DOMAIN.len() + canonical.len());
    out.extend_from_slice(REGISTRY_RELEASE_DESCRIPTOR_DOMAIN);
    out.extend_from_slice(&canonical);
    Ok(out)
}

fn validate_registry_descriptor(
    descriptor: &RegistryReleaseDescriptorV1,
    require_sig: bool,
) -> Result<(), ChunkTransferCodecError> {
    if descriptor.release_sequence == 0
        || descriptor.release_root == [0; 32]
        || descriptor.chunk_merkle_root == [0; 32]
        || descriptor.signer_public_key == [0; 32]
    {
        return Err(ChunkTransferCodecError::InvalidField("descriptor.header"));
    }
    if descriptor.chunk_size_bytes != REGISTRY_CHUNK_BYTES as u32 || descriptor.total_bytes == 0 {
        return Err(ChunkTransferCodecError::InvalidField("descriptor.sizing"));
    }
    let expected_chunks = descriptor.total_bytes.div_ceil(REGISTRY_CHUNK_BYTES as u64);
    if expected_chunks == 0
        || expected_chunks > u64::from(MAX_REGISTRY_CHUNKS)
        || u64::from(descriptor.total_chunks) != expected_chunks
    {
        return Err(ChunkTransferCodecError::InvalidField(
            "descriptor.total_chunks",
        ));
    }
    if let Some(delta) = &descriptor.optional_delta_from {
        if delta.from_release_sequence.checked_add(1) != Some(descriptor.release_sequence)
            || delta.from_release_root == [0; 32]
        {
            return Err(ChunkTransferCodecError::InvalidField(
                "descriptor.delta_from",
            ));
        }
    }
    if require_sig && descriptor.signature == [0; 64] {
        return Err(ChunkTransferCodecError::InvalidSignature);
    }
    Ok(())
}

fn validate_registry_message(
    msg: &RegistryChunkWireMessageV1,
) -> Result<(), ChunkTransferCodecError> {
    match msg {
        RegistryChunkWireMessageV1::DescriptorRequest { .. } => Ok(()),
        RegistryChunkWireMessageV1::DescriptorResponse { descriptor } => {
            validate_registry_descriptor(descriptor, true)
        }
        RegistryChunkWireMessageV1::ChunkRequest {
            release_root,
            chunk_index,
        } => {
            if *release_root == [0; 32] || *chunk_index >= MAX_REGISTRY_CHUNKS {
                return Err(ChunkTransferCodecError::InvalidField("chunk_request"));
            }
            Ok(())
        }
        RegistryChunkWireMessageV1::ChunkDelivery {
            release_root,
            chunk_index,
            chunk_bytes,
            merkle_proof,
        } => {
            if *release_root == [0; 32] || *chunk_index >= MAX_REGISTRY_CHUNKS {
                return Err(ChunkTransferCodecError::InvalidField("chunk_delivery"));
            }
            if chunk_bytes.is_empty() || chunk_bytes.len() > REGISTRY_CHUNK_BYTES {
                return Err(ChunkTransferCodecError::InvalidChunkLength);
            }
            if merkle_proof.siblings.len() > MAX_REGISTRY_MERKLE_PROOF_DEPTH {
                return Err(ChunkTransferCodecError::ResourceLimit);
            }
            Ok(())
        }
    }
}

fn registry_descriptor_value(
    descriptor: &RegistryReleaseDescriptorV1,
    include_sig: bool,
) -> CanonicalValue {
    let delta = descriptor
        .optional_delta_from
        .map_or(CanonicalValue::Null, |d| {
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(d.from_release_sequence)),
                (1, CanonicalValue::Bytes(d.from_release_root.to_vec())),
                (
                    2,
                    CanonicalValue::Bytes(d.patch_manifest_cid.as_bytes().to_vec()),
                ),
            ])
        });
    let mut fields = vec![
        (0, CanonicalValue::Unsigned(descriptor.release_sequence)),
        (1, CanonicalValue::Bytes(descriptor.release_root.to_vec())),
        (2, CanonicalValue::Unsigned(descriptor.total_bytes)),
        (
            3,
            CanonicalValue::Unsigned(u64::from(descriptor.chunk_size_bytes)),
        ),
        (
            4,
            CanonicalValue::Unsigned(u64::from(descriptor.total_chunks)),
        ),
        (
            5,
            CanonicalValue::Bytes(descriptor.chunk_merkle_root.to_vec()),
        ),
        (6, delta),
        (
            7,
            CanonicalValue::Unsigned(u64::from(descriptor.minimum_compatible_reader_version)),
        ),
        (
            8,
            CanonicalValue::Bytes(descriptor.signer_public_key.to_vec()),
        ),
    ];
    if include_sig {
        fields.push((9, CanonicalValue::Bytes(descriptor.signature.to_vec())));
    }
    CanonicalValue::Map(fields)
}

fn parse_registry_descriptor(
    map: &[(u64, CanonicalValue)],
) -> Result<RegistryReleaseDescriptorV1, ChunkTransferCodecError> {
    let chunk_size_bytes = u32::try_from(unsigned(map, 3, "descriptor.chunk_size")?)
        .map_err(|_| ChunkTransferCodecError::InvalidField("descriptor.chunk_size"))?;
    let total_chunks = u32::try_from(unsigned(map, 4, "descriptor.total_chunks")?)
        .map_err(|_| ChunkTransferCodecError::InvalidField("descriptor.total_chunks"))?;
    let minimum_compatible_reader_version =
        u32::try_from(unsigned(map, 7, "descriptor.min_reader")?)
            .map_err(|_| ChunkTransferCodecError::InvalidField("descriptor.min_reader"))?;
    let optional_delta_from = match required(map, 6, "descriptor.delta")? {
        CanonicalValue::Null => None,
        other => {
            let dm = as_map(other, "descriptor.delta")?;
            Some(RegistryDeltaBaseV1 {
                from_release_sequence: unsigned(dm, 0, "delta.seq")?,
                from_release_root: bytes32(dm, 1, "delta.root")?,
                patch_manifest_cid: ManifestCid::from_bytes(bytes32(dm, 2, "delta.manifest")?),
            })
        }
    };
    let mut sig = [0u8; 64];
    let sig_bytes = byte_string(map, 9, "descriptor.signature")?;
    if sig_bytes.len() != 64 {
        return Err(ChunkTransferCodecError::InvalidSignature);
    }
    sig.copy_from_slice(sig_bytes);
    Ok(RegistryReleaseDescriptorV1 {
        release_sequence: unsigned(map, 0, "descriptor.seq")?,
        release_root: bytes32(map, 1, "descriptor.root")?,
        total_bytes: unsigned(map, 2, "descriptor.total_bytes")?,
        chunk_size_bytes,
        total_chunks,
        chunk_merkle_root: bytes32(map, 5, "descriptor.merkle_root")?,
        optional_delta_from,
        minimum_compatible_reader_version,
        signer_public_key: bytes32(map, 8, "descriptor.signer_pk")?,
        signature: sig,
    })
}

fn binary_merkle_root(branch_domain: &[u8], leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return *blake3::hash(branch_domain).as_bytes();
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = if pair.len() == 2 { pair[1] } else { pair[0] };
            next.push(merkle_branch(branch_domain, &left, &right));
        }
        level = next;
    }
    level[0]
}

fn merkle_branch(domain: &[u8], left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(domain);
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

fn parse_blob_cid(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<BlobCid, ChunkTransferCodecError> {
    let b = byte_string(map, key, field)?;
    let arr: [u8; 34] = b
        .try_into()
        .map_err(|_| ChunkTransferCodecError::InvalidBlobCid)?;
    let cid = BlobCid(arr);
    validate_blob_cid(&cid)?;
    Ok(cid)
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ChunkTransferCodecError> {
    match value {
        CanonicalValue::Map(m) => Ok(m),
        _ => Err(ChunkTransferCodecError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ChunkTransferCodecError> {
    map.iter()
        .find_map(|(k, v)| (*k == key).then_some(v))
        .ok_or(ChunkTransferCodecError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ChunkTransferCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(v) => Ok(*v),
        _ => Err(ChunkTransferCodecError::InvalidField(field)),
    }
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ChunkTransferCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Array(v) => Ok(v),
        _ => Err(ChunkTransferCodecError::InvalidField(field)),
    }
}

fn byte_string<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], ChunkTransferCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(v) => Ok(v),
        _ => Err(ChunkTransferCodecError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ChunkTransferCodecError> {
    bytes32_val(required(map, key, field)?, field)
}

fn bytes32_val(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 32], ChunkTransferCodecError> {
    let CanonicalValue::Bytes(b) = value else {
        return Err(ChunkTransferCodecError::InvalidField(field));
    };
    if b.len() != 32 {
        return Err(ChunkTransferCodecError::InvalidField(field));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(b);
    Ok(out)
}

fn optional_bytes32(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<Option<[u8; 32]>, ChunkTransferCodecError> {
    match value {
        CanonicalValue::Null => Ok(None),
        _ => bytes32_val(value, field).map(Some),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkTransferCodecError {
    Canonical(CanonicalError),
    UnsupportedMajor,
    UnknownWireId(u64),
    InvalidField(&'static str),
    InvalidBlobCid,
    InvalidBlobLength,
    InvalidChunkLength,
    BlobCidMismatch,
    MerkleRootMismatch,
    InvalidSignature,
    UnapprovedRegistrySigner,
    ResourceLimit,
    FrameLimit,
    NonCanonical,
}

impl From<CanonicalError> for ChunkTransferCodecError {
    fn from(err: CanonicalError) -> Self {
        Self::Canonical(err)
    }
}

impl std::fmt::Display for ChunkTransferCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_CHUNK_CODEC: {self:?}")
    }
}

impl std::error::Error for ChunkTransferCodecError {}

#[cfg(test)]
mod tests {
    use ku_core::blob_store::BlobType;

    use super::*;

    #[test]
    fn blob_chunk_manifest_and_three_chunk_batch_fit_under_one_mib() {
        let data = vec![0x5Au8; BLOB_CHUNK_BYTES * 3 + 123];
        let cid = BlobCid::from_content(BlobType::Raw, &data);
        let manifest = BlobChunkManifest::from_bytes(cid, &data).unwrap();
        assert_eq!(manifest.chunk_hashes.len(), 4);
        let encoded_manifest =
            encode_blob_chunk_message(&BlobChunkWireMessageV1::Manifest(manifest.clone())).unwrap();
        assert_eq!(
            decode_blob_chunk_message(&encoded_manifest).unwrap(),
            BlobChunkWireMessageV1::Manifest(manifest)
        );

        let batch = BlobChunkBatch {
            blob_cid: cid,
            chunks: (0..3u32)
                .map(|i| BlobChunkItemV1 {
                    chunk_index: i,
                    bytes: vec![0x5Au8; BLOB_CHUNK_BYTES],
                })
                .collect(),
        };
        let encoded_batch =
            encode_blob_chunk_message(&BlobChunkWireMessageV1::Batch(batch.clone())).unwrap();
        assert!(encoded_batch.len() < MAX_BLOB_WIRE_FRAME_BYTES);
        assert_eq!(
            decode_blob_chunk_message(&encoded_batch).unwrap(),
            BlobChunkWireMessageV1::Batch(batch)
        );

        let four_chunks = BlobChunkBatch {
            blob_cid: cid,
            chunks: (0..4u32)
                .map(|i| BlobChunkItemV1 {
                    chunk_index: i,
                    bytes: vec![0x5Au8; BLOB_CHUNK_BYTES],
                })
                .collect(),
        };
        assert_eq!(
            encode_blob_chunk_message(&BlobChunkWireMessageV1::Batch(four_chunks)).unwrap_err(),
            ChunkTransferCodecError::ResourceLimit
        );
    }

    #[test]
    fn registry_release_descriptor_merkle_proofs_and_4mib_chunk_round_trip() {
        let signer = SigningKey::from_bytes(&[0x77; 32]);
        let chunks = vec![
            vec![0x11u8; REGISTRY_CHUNK_BYTES],
            vec![0x22u8; REGISTRY_CHUNK_BYTES],
            vec![0x33u8; 65_536],
        ];
        let chunk_cids: Vec<[u8; 32]> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| compute_registry_chunk_cid(i as u64, c))
            .collect();
        let merkle_root = compute_registry_chunk_merkle_root(&chunk_cids);
        let mut descriptor = RegistryReleaseDescriptorV1 {
            release_sequence: 2,
            release_root: [0x88; 32],
            total_bytes: (REGISTRY_CHUNK_BYTES * 2 + 65_536) as u64,
            chunk_size_bytes: REGISTRY_CHUNK_BYTES as u32,
            total_chunks: 3,
            chunk_merkle_root: merkle_root,
            optional_delta_from: Some(RegistryDeltaBaseV1 {
                from_release_sequence: 1,
                from_release_root: [0x87; 32],
                patch_manifest_cid: ManifestCid::from_bytes([0x86; 32]),
            }),
            minimum_compatible_reader_version: 1,
            signer_public_key: [0; 32],
            signature: [0; 64],
        };
        sign_registry_release_descriptor(&mut descriptor, &signer).unwrap();
        verify_registry_release_descriptor(&descriptor, &[*signer.verifying_key().as_bytes()])
            .unwrap();
        assert_eq!(
            verify_registry_release_descriptor(&descriptor, &[[0x99; 32]]).unwrap_err(),
            ChunkTransferCodecError::UnapprovedRegistrySigner
        );

        for (index, chunk_bytes) in chunks.into_iter().enumerate() {
            let proof = build_registry_chunk_merkle_proof(&chunk_cids, index as u32).unwrap();
            assert!(verify_registry_chunk_merkle_proof(
                &merkle_root,
                3,
                index as u32,
                &chunk_cids[index],
                &proof
            ));
            let msg = RegistryChunkWireMessageV1::ChunkDelivery {
                release_root: descriptor.release_root,
                chunk_index: index as u32,
                chunk_bytes,
                merkle_proof: proof,
            };
            let encoded = encode_registry_chunk_message(&msg).unwrap();
            assert_eq!(decode_registry_chunk_message(&encoded).unwrap(), msg);
        }
    }
}
