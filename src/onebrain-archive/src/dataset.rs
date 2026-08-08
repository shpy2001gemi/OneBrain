use std::io::Read;

use crate::{
    ArchiveEntryId, ArchiveEntryKind, ArchiveEntryV1, ArchiveError, DatasetManifestV1,
    PortableDataCompatibilityV1, ProducerArtifactIdentityV1,
};

pub const MAX_DATASET_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const DATASET_DOMAIN: &str = "onebrain:base:archive-dataset:1\0";
const HIGH_WATER_DOMAIN: &str = "onebrain:base:archive-high-water:1\0";

/// A source-owned, quiesced view. The opaque binding must change whenever any
/// row, high-water, generation, or held retention/blob generation changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotLease {
    pub dataset_generation: u64,
    pub canonical_source_root: [u8; 32],
    pub high_water_root: [u8; 32],
    pub blob_generation: u64,
    pub retention_generation: u64,
    pub source_binding: [u8; 32],
}

pub trait SnapshotSource {
    fn acquire_snapshot(&self) -> Result<SnapshotLease, ArchiveError>;
    fn entries(&self, lease: &SnapshotLease) -> Result<Vec<ArchiveEntryV1>, ArchiveError>;
    fn read_entry(
        &self,
        lease: &SnapshotLease,
        id: ArchiveEntryId,
    ) -> Result<Box<dyn Read>, ArchiveError>;
    fn validate_snapshot(&self, lease: &SnapshotLease) -> Result<(), ArchiveError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedDatasetV1 {
    pub lease: SnapshotLease,
    pub manifest: DatasetManifestV1,
    /// Payloads use manifest order. Plaintext exists only inside the verified,
    /// authenticated encrypted-stream boundary.
    pub payloads: Vec<Vec<u8>>,
}

impl CapturedDatasetV1 {
    pub fn canonical_plaintext(&self) -> Result<Vec<u8>, ArchiveError> {
        self.manifest.validate()?;
        if self.payloads.len() != self.manifest.entries.len() {
            return Err(ArchiveError::Integrity);
        }
        let manifest = self.manifest.canonical_bytes()?;
        let capacity = manifest.len() as u64
            + self
                .payloads
                .iter()
                .map(|value| value.len() as u64)
                .sum::<u64>();
        if capacity > MAX_DATASET_BYTES {
            return Err(ArchiveError::Limit);
        }
        let allocation = usize::try_from(capacity)
            .ok()
            .and_then(|value| value.checked_add(32))
            .ok_or(ArchiveError::Limit)?;
        let mut output = Vec::with_capacity(allocation);
        output.extend_from_slice(b"OBDSV001");
        output.extend_from_slice(&(manifest.len() as u64).to_be_bytes());
        output.extend_from_slice(&manifest);
        output.extend_from_slice(&(self.payloads.len() as u64).to_be_bytes());
        for (entry, payload) in self.manifest.entries.iter().zip(&self.payloads) {
            verify_payload(entry, payload)?;
            output.extend_from_slice(entry.id.as_bytes());
            output.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            output.extend_from_slice(payload);
        }
        let digest = blake3::derive_key(DATASET_DOMAIN, &output);
        output.extend_from_slice(&digest);
        Ok(output)
    }
}

pub fn capture_dataset<S: SnapshotSource>(
    source: &S,
    portable_data_compatibility: PortableDataCompatibilityV1,
    producer_artifact_identity: ProducerArtifactIdentityV1,
) -> Result<CapturedDatasetV1, ArchiveError> {
    let lease = source.acquire_snapshot()?;
    source.validate_snapshot(&lease)?;
    let entries = source.entries(&lease)?;
    let manifest = DatasetManifestV1::build(
        portable_data_compatibility,
        producer_artifact_identity,
        entries,
    )?;
    if manifest.canonical_root != lease.canonical_source_root
        || compute_high_water_root(&manifest.entries) != lease.high_water_root
    {
        return Err(ArchiveError::Integrity);
    }

    let total = manifest
        .entries
        .iter()
        .try_fold(0u64, |total, entry| total.checked_add(entry.length))
        .ok_or(ArchiveError::Limit)?;
    if total > MAX_DATASET_BYTES {
        return Err(ArchiveError::Limit);
    }

    let mut payloads = Vec::with_capacity(manifest.entries.len());
    for entry in &manifest.entries {
        source.validate_snapshot(&lease)?;
        let mut reader = source.read_entry(&lease, entry.id)?;
        let allocation = usize::try_from(entry.length).map_err(|_| ArchiveError::Limit)?;
        let mut payload = Vec::with_capacity(allocation);
        reader
            .by_ref()
            .take(entry.length + 1)
            .read_to_end(&mut payload)?;
        verify_payload(entry, &payload)?;
        payloads.push(payload);
    }
    source.validate_snapshot(&lease)?;
    Ok(CapturedDatasetV1 {
        lease,
        manifest,
        payloads,
    })
}

pub fn compute_high_water_root(entries: &[ArchiveEntryV1]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(HIGH_WATER_DOMAIN);
    let selected = entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind,
                ArchiveEntryKind::AuthorityHighWater | ArchiveEntryKind::RegistryHighWater
            )
        })
        .collect::<Vec<_>>();
    hasher.update(&(selected.len() as u64).to_be_bytes());
    for entry in selected {
        hasher.update(entry.id.as_bytes());
        hasher.update(&entry.length.to_be_bytes());
        hasher.update(&entry.blake3);
    }
    *hasher.finalize().as_bytes()
}

fn verify_payload(entry: &ArchiveEntryV1, payload: &[u8]) -> Result<(), ArchiveError> {
    if payload.len() as u64 != entry.length || *blake3::hash(payload).as_bytes() != entry.blake3 {
        return Err(ArchiveError::Integrity);
    }
    Ok(())
}
