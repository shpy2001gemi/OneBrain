use crate::{
    ArchiveEntryId, ArchiveEntryKind, ArchiveEntryV1, ArchiveError, DatasetManifestV1,
    LogicalRestoreSink, PortableDataCompatibilityV1, PortableProfileVersion,
    VerifiedDatasetArchiveV2, VerifiedMaterialization,
};

const DATASET_DOMAIN: &str = "onebrain:base:archive-dataset:1\0";
const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveRestorePolicyV1 {
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_schema_version: u32,
    pub archive_profile: PortableProfileVersion,
    pub migration_profile: PortableProfileVersion,
    pub max_dataset_bytes: u64,
}

impl ArchiveRestorePolicyV1 {
    pub const fn portable_data_compatibility(self) -> PortableDataCompatibilityV1 {
        PortableDataCompatibilityV1 {
            canonical_schema_digest: self.canonical_schema_digest,
            domain_registry_digest: self.domain_registry_digest,
            resource_registry_digest: self.resource_registry_digest,
            storage_schema_version: self.storage_schema_version,
            archive_profile: self.archive_profile,
            migration_profile: self.migration_profile,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignerRecoveryDisposition {
    PolicyPresent,
    ReprovisionRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedDatasetMaterialization {
    pub container: VerifiedMaterialization,
    pub manifest: DatasetManifestV1,
    pub entry_count: u64,
    pub signer_recovery: SignerRecoveryDisposition,
}

pub trait VerifiedDatasetMaterializer {
    fn begin(&mut self, manifest: &DatasetManifestV1) -> Result<(), ArchiveError>;
    fn materialize_entry(
        &mut self,
        entry: &ArchiveEntryV1,
        payload: &[u8],
    ) -> Result<(), ArchiveError>;
    fn flush(&mut self) -> Result<(), ArchiveError>;
    fn cleanup_failed(&mut self) -> Result<(), ArchiveError>;
}

pub fn materialize_verified_dataset(
    verified: VerifiedDatasetArchiveV2,
    expected: &ArchiveRestorePolicyV1,
    materializer: &mut dyn VerifiedDatasetMaterializer,
) -> Result<VerifiedDatasetMaterialization, ArchiveError> {
    if expected.max_dataset_bytes == 0 {
        return Err(ArchiveError::Limit);
    }
    let mut sink = RestoreAdapter {
        expected,
        materializer,
        completed: None,
    };
    let container = verified.materialize_into(&mut sink)?;
    let (manifest, signer_recovery) = sink.completed.ok_or(ArchiveError::Integrity)?;
    Ok(VerifiedDatasetMaterialization {
        container,
        entry_count: manifest.entries.len() as u64,
        manifest,
        signer_recovery,
    })
}

struct RestoreAdapter<'a> {
    expected: &'a ArchiveRestorePolicyV1,
    materializer: &'a mut dyn VerifiedDatasetMaterializer,
    completed: Option<(DatasetManifestV1, SignerRecoveryDisposition)>,
}

impl LogicalRestoreSink for RestoreAdapter<'_> {
    fn restore_verified(
        &mut self,
        plaintext: &[u8],
        inspection: &crate::ArchiveInspection,
    ) -> Result<(), ArchiveError> {
        if plaintext.len() as u64 != inspection.plaintext_length
            || plaintext.len() as u64 > self.expected.max_dataset_bytes
        {
            return Err(ArchiveError::Limit);
        }
        let (manifest, payloads) = decode_dataset(plaintext)?;
        if manifest.portable_data_compatibility != self.expected.portable_data_compatibility() {
            return Err(ArchiveError::InvalidProfile);
        }
        let outcome = (|| {
            self.materializer.begin(&manifest)?;
            for (entry, payload) in manifest.entries.iter().zip(&payloads) {
                self.materializer.materialize_entry(entry, payload)?;
            }
            self.materializer.flush()?;
            Ok(())
        })();
        if let Err(primary) = outcome {
            return match self.materializer.cleanup_failed() {
                Ok(()) => Err(primary),
                Err(cleanup) => Err(ArchiveError::CleanupFailed(format!(
                    "{primary}; materializer cleanup: {cleanup}"
                ))),
            };
        }
        let signer_payload = manifest
            .entries
            .iter()
            .position(|entry| entry.kind == ArchiveEntryKind::SignerRecoveryPolicy)
            .and_then(|index| payloads.get(index))
            .ok_or(ArchiveError::InvalidProfile)?;
        let signer_recovery = if signer_payload.as_slice() == b"reprovision-required" {
            SignerRecoveryDisposition::ReprovisionRequired
        } else {
            SignerRecoveryDisposition::PolicyPresent
        };
        self.completed = Some((manifest, signer_recovery));
        Ok(())
    }
}

fn decode_dataset(bytes: &[u8]) -> Result<(DatasetManifestV1, Vec<Vec<u8>>), ArchiveError> {
    if bytes.len() < 8 + 8 + 8 + 32 {
        return Err(ArchiveError::Malformed);
    }
    let (body, encoded_digest) = bytes.split_at(bytes.len() - 32);
    if blake3::derive_key(DATASET_DOMAIN, body) != encoded_digest {
        return Err(ArchiveError::Integrity);
    }
    let mut decoder = Decoder::new(body);
    if decoder.take(8)? != b"OBDSV001" {
        return Err(ArchiveError::InvalidProfile);
    }
    let manifest_length = usize::try_from(decoder.u64()?).map_err(|_| ArchiveError::Limit)?;
    if manifest_length == 0 || manifest_length > MAX_MANIFEST_BYTES {
        return Err(ArchiveError::Limit);
    }
    let manifest = DatasetManifestV1::from_canonical_bytes(decoder.take(manifest_length)?)?;
    let count = usize::try_from(decoder.u64()?).map_err(|_| ArchiveError::Limit)?;
    if count != manifest.entries.len() {
        return Err(ArchiveError::Integrity);
    }
    let mut payloads = Vec::with_capacity(count);
    for expected in &manifest.entries {
        let id = ArchiveEntryId::from_bytes(decoder.array()?);
        let length = usize::try_from(decoder.u64()?).map_err(|_| ArchiveError::Limit)?;
        let payload = decoder.take(length)?.to_vec();
        if id != expected.id
            || length as u64 != expected.length
            || *blake3::hash(&payload).as_bytes() != expected.blake3
        {
            return Err(ArchiveError::Integrity);
        }
        payloads.push(payload);
    }
    decoder.finish()?;
    Ok((manifest, payloads))
}

struct Decoder<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ArchiveError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ArchiveError::Limit)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ArchiveError::Malformed)?;
        self.position = end;
        Ok(value)
    }

    fn u64(&mut self) -> Result<u64, ArchiveError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| ArchiveError::Malformed)?,
        ))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ArchiveError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ArchiveError::Malformed)
    }

    fn finish(self) -> Result<(), ArchiveError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(ArchiveError::TrailingBytes)
        }
    }
}
