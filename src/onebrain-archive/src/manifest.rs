use std::collections::BTreeSet;

use crate::ArchiveError;

const ENTRY_ID_DOMAIN: &str = "onebrain:base:archive-entry-id:1";
const ROOT_DOMAIN: &str = "onebrain:base:archive-entry-root:1";
const MANIFEST_DOMAIN: &str = "onebrain:base:archive-manifest:1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchiveEntryId([u8; 32]);

impl ArchiveEntryId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoundedBytes<const MAX: usize>(Vec<u8>);

impl<const MAX: usize> BoundedBytes<MAX> {
    pub fn new(bytes: Vec<u8>) -> Result<Self, ArchiveError> {
        if bytes.is_empty() || bytes.len() > MAX {
            return Err(ArchiveError::Limit);
        }
        Ok(Self(bytes))
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ArchiveProfileId {
    ObarV2 = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum ArchiveEntryKind {
    CanonicalObject = 1,
    CanonicalEvent = 2,
    FeedInception = 3,
    AuthorityEvent = 4,
    AuthorityHighWater = 5,
    VaultRecord = 6,
    QuarantineRecord = 7,
    OwnedBlob = 8,
    IdentityEnvelope = 9,
    ReconciliationJournalRecord = 10,
    InventoryRecord = 11,
    OutboxRecord = 12,
    ProvenanceRecord = 13,
    PrivateNeedRecord = 14,
    ReceivedUseRecord = 15,
    OperationalRecord = 16,
    RolloutRecord = 17,
    BaseOperationRecord = 18,
    PendingBlobUploadIntent = 19,
    SourceCaptureIntent = 20,
    MigrationState = 21,
    InterpretationConfig = 22,
    RegistryHighWater = 23,
    SignerRecoveryPolicy = 24,
}

impl ArchiveEntryKind {
    pub const REQUIRED_METADATA: [Self; 5] = [
        Self::AuthorityHighWater,
        Self::MigrationState,
        Self::InterpretationConfig,
        Self::RegistryHighWater,
        Self::SignerRecoveryPolicy,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchiveOwner(u16);

impl ArchiveOwner {
    pub const CANONICAL: Self = Self(0x0001);
    pub const VAULT: Self = Self(0x0002);
    pub const QUARANTINE: Self = Self(0x0003);
    pub const BLOB: Self = Self(0x0004);
    pub const PENDING_BLOB_INTENT: Self = Self(0x0005);
    pub const SOURCE_CAPTURE_INTENT: Self = Self(0x0006);
    pub const RECONCILIATION: Self = Self(0x0007);
    pub const INVENTORY: Self = Self(0x0008);
    pub const OUTBOX: Self = Self(0x0009);
    pub const PROVENANCE: Self = Self(0x000A);
    pub const PRIVATE_KQL: Self = Self(0x000B);
    pub const PRIVATE_POMV: Self = Self(0x000C);
    pub const OPERATIONAL: Self = Self(0x000D);
    pub const ROLLOUT: Self = Self(0x000E);
    pub const OPTIONAL_NETWORK: Self = Self(0x000F);
    pub const MIGRATION: Self = Self(0x0010);
    pub const BASE_OPERATIONS: Self = Self(0x0011);
    pub const INTERPRETATION_CONFIG: Self = Self(0x0012);
    pub const IDENTITY: Self = Self(0x0013);
    pub const REGISTRY_METADATA: Self = Self(0x0014);
    pub const DERIVED_INDEX: Self = Self(0x0015);
    pub const RETRIEVER_PROJECTION: Self = Self(0x0016);

    pub fn new(value: u16) -> Result<Self, ArchiveError> {
        if (0x0001..=0x0016).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ArchiveError::InvalidProfile)
        }
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn is_disposable_projection(self) -> bool {
        self.0 == Self::DERIVED_INDEX.0 || self.0 == Self::RETRIEVER_PROJECTION.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchiveLogicalKey {
    pub owner: ArchiveOwner,
    pub namespace: u16,
    pub key: BoundedBytes<256>,
}

impl ArchiveLogicalKey {
    pub fn new(owner: ArchiveOwner, namespace: u16, key: Vec<u8>) -> Result<Self, ArchiveError> {
        if namespace == 0 || owner.is_disposable_projection() || looks_like_path(&key) {
            return Err(ArchiveError::InvalidProfile);
        }
        Ok(Self {
            owner,
            namespace,
            key: BoundedBytes::new(key)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntryV1 {
    pub id: ArchiveEntryId,
    pub kind: ArchiveEntryKind,
    pub logical_key: ArchiveLogicalKey,
    pub length: u64,
    pub blake3: [u8; 32],
    pub required: bool,
}

impl ArchiveEntryV1 {
    pub fn new(
        kind: ArchiveEntryKind,
        logical_key: ArchiveLogicalKey,
        length: u64,
        blake3: [u8; 32],
        required: bool,
    ) -> Result<Self, ArchiveError> {
        if length == 0 {
            return Err(ArchiveError::Limit);
        }
        let id = compute_entry_id(kind, &logical_key);
        Ok(Self {
            id,
            kind,
            logical_key,
            length,
            blake3,
            required,
        })
    }

    pub fn verify_identity(&self) -> Result<(), ArchiveError> {
        if self.id != compute_entry_id(self.kind, &self.logical_key) || self.length == 0 {
            return Err(ArchiveError::Integrity);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortableProfileVersion {
    pub major: u16,
    pub minor: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProducerArtifactIdentityV1 {
    Known([u8; 32]),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortableDataCompatibilityV1 {
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_schema_version: u32,
    pub archive_profile: PortableProfileVersion,
    pub migration_profile: PortableProfileVersion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasetManifestV1 {
    pub profile: ArchiveProfileId,
    pub portable_data_compatibility: PortableDataCompatibilityV1,
    pub producer_artifact_identity: ProducerArtifactIdentityV1,
    pub canonical_root: [u8; 32],
    pub object_root: [u8; 32],
    pub blob_root: [u8; 32],
    pub feed_root: [u8; 32],
    pub entries: Vec<ArchiveEntryV1>,
    pub aggregate_root: [u8; 32],
}

impl DatasetManifestV1 {
    pub fn build(
        portable_data_compatibility: PortableDataCompatibilityV1,
        producer_artifact_identity: ProducerArtifactIdentityV1,
        mut entries: Vec<ArchiveEntryV1>,
    ) -> Result<Self, ArchiveError> {
        entries.sort_by_key(|entry| entry.id);
        validate_entries(&entries)?;
        let canonical_root = entry_root(
            b"canonical",
            entries.iter().filter(|entry| {
                matches!(
                    entry.kind,
                    ArchiveEntryKind::CanonicalObject
                        | ArchiveEntryKind::CanonicalEvent
                        | ArchiveEntryKind::FeedInception
                        | ArchiveEntryKind::AuthorityEvent
                )
            }),
        );
        let object_root = entry_root(
            b"object",
            entries
                .iter()
                .filter(|entry| entry.kind == ArchiveEntryKind::CanonicalObject),
        );
        let blob_root = entry_root(
            b"blob",
            entries
                .iter()
                .filter(|entry| entry.kind == ArchiveEntryKind::OwnedBlob),
        );
        let feed_root = entry_root(
            b"feed",
            entries.iter().filter(|entry| {
                matches!(
                    entry.kind,
                    ArchiveEntryKind::FeedInception
                        | ArchiveEntryKind::AuthorityEvent
                        | ArchiveEntryKind::AuthorityHighWater
                )
            }),
        );
        let mut manifest = Self {
            profile: ArchiveProfileId::ObarV2,
            portable_data_compatibility,
            producer_artifact_identity,
            canonical_root,
            object_root,
            blob_root,
            feed_root,
            entries,
            aggregate_root: [0; 32],
        };
        manifest.aggregate_root = manifest.recompute_aggregate_root();
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ArchiveError> {
        if self.profile != ArchiveProfileId::ObarV2 {
            return Err(ArchiveError::InvalidProfile);
        }
        validate_entries(&self.entries)?;
        let rebuilt = Self::build(
            self.portable_data_compatibility,
            self.producer_artifact_identity,
            self.entries.clone(),
        )?;
        if rebuilt.entries != self.entries
            || rebuilt.canonical_root != self.canonical_root
            || rebuilt.object_root != self.object_root
            || rebuilt.blob_root != self.blob_root
            || rebuilt.feed_root != self.feed_root
            || rebuilt.aggregate_root != self.aggregate_root
        {
            return Err(ArchiveError::Integrity);
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ArchiveError> {
        self.validate()?;
        Ok(self.encode(true))
    }

    pub fn portable_compatible_with(&self, target: &Self) -> bool {
        self.portable_data_compatibility == target.portable_data_compatibility
    }

    pub const fn supports_qualified_release_claim(&self) -> bool {
        matches!(
            self.producer_artifact_identity,
            ProducerArtifactIdentityV1::Known(_)
        )
    }

    fn recompute_aggregate_root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_derive_key(MANIFEST_DOMAIN);
        hasher.update(&self.encode(false));
        *hasher.finalize().as_bytes()
    }

    fn encode(&self, include_aggregate: bool) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend_from_slice(b"OBDMV001");
        push_u16(&mut output, self.profile as u16);
        encode_compatibility(&mut output, self.portable_data_compatibility);
        match self.producer_artifact_identity {
            ProducerArtifactIdentityV1::Known(digest) => {
                output.push(1);
                output.extend_from_slice(&digest);
            }
            ProducerArtifactIdentityV1::Unknown => output.push(0),
        }
        output.extend_from_slice(&self.canonical_root);
        output.extend_from_slice(&self.object_root);
        output.extend_from_slice(&self.blob_root);
        output.extend_from_slice(&self.feed_root);
        push_u32(&mut output, self.entries.len() as u32);
        for entry in &self.entries {
            output.extend_from_slice(entry.id.as_bytes());
            push_u16(&mut output, entry.kind as u16);
            push_u16(&mut output, entry.logical_key.owner.get());
            push_u16(&mut output, entry.logical_key.namespace);
            push_u16(&mut output, entry.logical_key.key.as_slice().len() as u16);
            output.extend_from_slice(entry.logical_key.key.as_slice());
            push_u64(&mut output, entry.length);
            output.extend_from_slice(&entry.blake3);
            output.push(u8::from(entry.required));
        }
        if include_aggregate {
            output.extend_from_slice(&self.aggregate_root);
        }
        output
    }
}

pub fn compute_entry_id(kind: ArchiveEntryKind, logical_key: &ArchiveLogicalKey) -> ArchiveEntryId {
    let mut hasher = blake3::Hasher::new_derive_key(ENTRY_ID_DOMAIN);
    hasher.update(&(kind as u16).to_be_bytes());
    hasher.update(&logical_key.owner.get().to_be_bytes());
    hasher.update(&logical_key.namespace.to_be_bytes());
    hasher.update(&(logical_key.key.as_slice().len() as u16).to_be_bytes());
    hasher.update(logical_key.key.as_slice());
    ArchiveEntryId(*hasher.finalize().as_bytes())
}

fn validate_entries(entries: &[ArchiveEntryV1]) -> Result<(), ArchiveError> {
    if entries.is_empty() || entries.len() > 1_000_000 {
        return Err(ArchiveError::Limit);
    }
    let mut ids = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut prior = None;
    for entry in entries {
        entry.verify_identity()?;
        if prior.is_some_and(|value| value >= entry.id)
            || !ids.insert(entry.id)
            || !keys.insert(entry.logical_key.clone())
        {
            return Err(ArchiveError::Integrity);
        }
        kinds.insert(entry.kind);
        prior = Some(entry.id);
    }
    if ArchiveEntryKind::REQUIRED_METADATA
        .iter()
        .any(|required| !kinds.contains(required))
    {
        return Err(ArchiveError::InvalidProfile);
    }
    Ok(())
}

fn entry_root<'a>(label: &[u8], entries: impl Iterator<Item = &'a ArchiveEntryV1>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(ROOT_DOMAIN);
    hasher.update(&(label.len() as u16).to_be_bytes());
    hasher.update(label);
    let entries = entries.collect::<Vec<_>>();
    hasher.update(&(entries.len() as u64).to_be_bytes());
    for entry in entries {
        hasher.update(entry.id.as_bytes());
        hasher.update(&entry.length.to_be_bytes());
        hasher.update(&entry.blake3);
    }
    *hasher.finalize().as_bytes()
}

fn encode_compatibility(output: &mut Vec<u8>, value: PortableDataCompatibilityV1) {
    output.extend_from_slice(&value.canonical_schema_digest);
    output.extend_from_slice(&value.domain_registry_digest);
    output.extend_from_slice(&value.resource_registry_digest);
    output.extend_from_slice(&value.storage_schema_version.to_be_bytes());
    push_u16(output, value.archive_profile.major);
    push_u16(output, value.archive_profile.minor);
    push_u16(output, value.migration_profile.major);
    push_u16(output, value.migration_profile.minor);
}

fn looks_like_path(bytes: &[u8]) -> bool {
    bytes.contains(&0)
        || bytes.contains(&b'/')
        || bytes.contains(&b'\\')
        || (bytes.len() >= 2 && bytes[1] == b':')
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}
