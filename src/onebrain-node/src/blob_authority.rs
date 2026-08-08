//! Canonical blob-reference oracle and durable pending-upload leases.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use ku_core::blob_store::{BlobCid, BlobType, BLOB_MAX_SIZE};
use ku_core::foundation::{
    decode_knowledge_object, dr_m5_failpoint, schema_registry::OBJECT_KINDS_V1, BlobRetentionState,
    CanonicalValue, EventCid, KnownObjectKind, ObjectKind, ObjectReference, ObjectSemantics,
    OwnedBlobReferenceV1, ReservedDomain, ResourceProfile,
};
use ku_kql::blob_storage::{BlobReferenceOracle, BlobStorageError};
use serde::{Deserialize, Serialize};

use crate::dataset_path::{BaseStorageOwnerId, DatasetGenerationId, DatasetPathResolver};

pub const MAX_PENDING_BLOB_UPLOADS: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingBlobUploadId([u8; 32]);

impl PendingBlobUploadId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }

    fn hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOwnedBlobUpload {
    pub id: PendingBlobUploadId,
    pub intended_owner: ObjectReference,
    pub expected_blob: BlobCid,
    pub expected_type: BlobType,
    pub expected_length: u64,
    pub dataset_generation: DatasetGenerationId,
}

pub trait PendingUploadIdSource: Send + Sync {
    fn next_id(&self) -> Result<PendingBlobUploadId, BlobAuthorityError>;
}

pub struct OsPendingUploadIdSource;

impl PendingUploadIdSource for OsPendingUploadIdSource {
    fn next_id(&self) -> Result<PendingBlobUploadId, BlobAuthorityError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| BlobAuthorityError::EntropyUnavailable)?;
        Ok(PendingBlobUploadId(bytes))
    }
}

/// Read-only snapshot of bytes that already crossed the validate-then-accept
/// boundary. Implementations must never include quarantine, legacy KU, or
/// projection bytes.
#[derive(Clone, Debug, Default)]
pub struct ValidatedBlobAuthoritySnapshot {
    pub objects: Vec<Vec<u8>>,
    pub events: Vec<Vec<u8>>,
}

pub trait ValidatedBlobReferenceSource: Send + Sync {
    fn snapshot(&self) -> Result<ValidatedBlobAuthoritySnapshot, BlobAuthorityError>;
}

pub struct UnavailableValidatedBlobReferenceSource;

impl ValidatedBlobReferenceSource for UnavailableValidatedBlobReferenceSource {
    fn snapshot(&self) -> Result<ValidatedBlobAuthoritySnapshot, BlobAuthorityError> {
        Err(BlobAuthorityError::CanonicalSourceUnavailable)
    }
}

pub struct PendingBlobUploadStore {
    resolver: Arc<dyn DatasetPathResolver>,
    ids: Arc<dyn PendingUploadIdSource>,
}

impl PendingBlobUploadStore {
    pub fn new(
        resolver: Arc<dyn DatasetPathResolver>,
        ids: Arc<dyn PendingUploadIdSource>,
    ) -> Self {
        Self { resolver, ids }
    }

    pub fn prepare(
        &self,
        intended_owner: ObjectReference,
        expected_blob: BlobCid,
        expected_type: BlobType,
        expected_length: u64,
    ) -> Result<PendingOwnedBlobUpload, BlobAuthorityError> {
        if expected_length > BLOB_MAX_SIZE
            || expected_blob.version() != ku_core::blob_store::BLOB_CID_VERSION
            || expected_blob.blob_type() != expected_type
            || expected_blob.0[1] != expected_type as u8
        {
            return Err(BlobAuthorityError::BindingMismatch);
        }
        let id = self.ids.next_id()?;
        let pending = PendingOwnedBlobUpload {
            id,
            intended_owner,
            expected_blob,
            expected_type,
            expected_length,
            dataset_generation: self.resolver.current_generation(),
        };
        let directory = self.directory()?;
        if self.list()?.len() >= MAX_PENDING_BLOB_UPLOADS {
            return Err(BlobAuthorityError::PendingLimit);
        }
        let final_path = directory.join(format!("{}.json", id.hex()));
        let staging_path = directory.join(format!("{}.preparing", id.hex()));
        if final_path.exists() || staging_path.exists() {
            return Err(BlobAuthorityError::IdCollision(id));
        }

        dr_m5_failpoint::hit("TX-BLOB-UPLOAD-001", "before_begin_write");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)?;
        dr_m5_failpoint::hit("TX-BLOB-UPLOAD-001", "after_begin_write_before_mutation");
        let bytes = serde_json::to_vec(&PendingUploadRecord::from_pending(&pending))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        dr_m5_failpoint::hit("TX-BLOB-UPLOAD-001", "after_mutation_before_commit");
        std::fs::rename(&staging_path, &final_path)?;
        sync_directory(&directory)?;
        dr_m5_failpoint::hit("TX-BLOB-UPLOAD-001", "after_commit_before_next_side_effect");
        dr_m5_failpoint::hit("TX-BLOB-UPLOAD-001", "after_next_side_effect_before_ack");
        Ok(pending)
    }

    pub fn abort(&self, id: PendingBlobUploadId) -> Result<bool, BlobAuthorityError> {
        let path = self.directory()?.join(format!("{}.json", id.hex()));
        match std::fs::remove_file(path) {
            Ok(()) => {
                sync_directory(&self.directory()?)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn list(&self) -> Result<Vec<PendingOwnedBlobUpload>, BlobAuthorityError> {
        let directory = self.directory()?;
        let mut pending = Vec::new();
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("preparing") {
                std::fs::remove_file(path)?;
                sync_directory(&directory)?;
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(&path)?;
            let record: PendingUploadRecord = serde_json::from_slice(&bytes)?;
            let decoded = record.into_pending()?;
            let expected_name = format!("{}.json", decoded.id.hex());
            if path.file_name().and_then(|value| value.to_str()) != Some(&expected_name) {
                return Err(BlobAuthorityError::CorruptIntent);
            }
            pending.push(decoded);
        }
        pending.sort_by_key(|value| value.id.into_bytes());
        if pending.len() > MAX_PENDING_BLOB_UPLOADS {
            return Err(BlobAuthorityError::PendingLimit);
        }
        Ok(pending)
    }

    pub fn get(
        &self,
        id: PendingBlobUploadId,
    ) -> Result<Option<PendingOwnedBlobUpload>, BlobAuthorityError> {
        Ok(self.list()?.into_iter().find(|pending| pending.id == id))
    }

    pub fn reconcile_generation(&self) -> Result<u64, BlobAuthorityError> {
        let current = self.resolver.current_generation();
        let mut removed = 0u64;
        for pending in self.list()? {
            if pending.dataset_generation != current && self.abort(pending.id)? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn directory(&self) -> Result<PathBuf, BlobAuthorityError> {
        let path = self
            .resolver
            .owner_path(BaseStorageOwnerId::PENDING_BLOB_INTENT)?;
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }
}

pub struct CanonicalBlobReferenceOracle {
    source: Arc<dyn ValidatedBlobReferenceSource>,
    pending: Arc<PendingBlobUploadStore>,
}

impl CanonicalBlobReferenceOracle {
    pub fn new(
        source: Arc<dyn ValidatedBlobReferenceSource>,
        pending: Arc<PendingBlobUploadStore>,
    ) -> Self {
        Self { source, pending }
    }

    fn canonical_records(&self) -> Result<Vec<OwnedBlobReferenceV1>, BlobAuthorityError> {
        let snapshot = self.source.snapshot()?;
        let terminal_events = snapshot
            .events
            .iter()
            .map(|bytes| {
                EventCid::compute(ReservedDomain::Event, bytes)
                    .map(EventCid::into_bytes)
                    .map_err(|_| BlobAuthorityError::InvalidCanonicalReference)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let known_kinds = OBJECT_KINDS_V1
            .iter()
            .map(|entry| KnownObjectKind::new(ObjectKind(entry.id), 1))
            .collect::<Vec<_>>();
        let mut records = Vec::new();
        for bytes in snapshot.objects {
            let validated =
                decode_knowledge_object(&bytes, ResourceProfile::ObjectV1, &known_kinds, &[])
                    .map_err(|_| BlobAuthorityError::InvalidCanonicalReference)?;
            if validated.original_bytes() != bytes {
                return Err(BlobAuthorityError::InvalidCanonicalReference);
            }
            let ObjectSemantics::Known(envelope) = validated.semantics() else {
                continue;
            };
            if !is_owned_blob_reference_payload(&envelope.payload) {
                continue;
            }
            let record = OwnedBlobReferenceV1::from_value(&envelope.payload)
                .map_err(|_| BlobAuthorityError::InvalidCanonicalReference)?;
            if let Some(event) = record.terminal_event {
                if !terminal_events.contains(event.as_bytes()) {
                    return Err(BlobAuthorityError::InvalidCanonicalReference);
                }
            }
            records.push(record);
        }
        Ok(records)
    }

    fn canonical_owners(&self, cid: &BlobCid) -> Result<Vec<ObjectReference>, BlobAuthorityError> {
        let records = self.canonical_records()?;
        let mut reduced: BTreeMap<(u64, [u8; 32]), OwnedBlobReferenceV1> = BTreeMap::new();
        for record in records.into_iter().filter(|record| record.blob_cid == *cid) {
            let key = (record.owner.reference_kind, record.owner.cid);
            match reduced.get(&key) {
                None => {
                    reduced.insert(key, record);
                }
                Some(existing) if existing == &record => {}
                Some(existing)
                    if matches!(existing.retention_state, BlobRetentionState::Live)
                        && !matches!(record.retention_state, BlobRetentionState::Live) =>
                {
                    reduced.insert(key, record);
                }
                Some(existing)
                    if !matches!(existing.retention_state, BlobRetentionState::Live)
                        && matches!(record.retention_state, BlobRetentionState::Live) => {}
                Some(_) => return Err(BlobAuthorityError::InvalidCanonicalReference),
            }
        }
        Ok(reduced
            .into_values()
            .filter(OwnedBlobReferenceV1::retains_blob)
            .map(|record| record.owner)
            .collect())
    }
}

fn is_owned_blob_reference_payload(value: &CanonicalValue) -> bool {
    let CanonicalValue::Map(fields) = value else {
        return false;
    };
    fields.iter().any(|(key, value)| {
        *key == 0
            && matches!(
                value,
                CanonicalValue::Unsigned(id)
                    if *id == ku_core::foundation::schema_registry::SCHEMA_OWNED_BLOB_REFERENCE
            )
    })
}

#[cfg(unix)]
fn sync_directory(path: &std::path::Path) -> Result<(), BlobAuthorityError> {
    std::fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &std::path::Path) -> Result<(), BlobAuthorityError> {
    // The create-new file is synced before rename; the intent is the Windows
    // recovery authority because portable directory flushing is unavailable.
    Ok(())
}

impl BlobReferenceOracle for CanonicalBlobReferenceOracle {
    fn referencing_records(&self, cid: &BlobCid) -> Result<Vec<ObjectReference>, BlobStorageError> {
        let mut owners = self
            .canonical_owners(cid)
            .map_err(|_| BlobStorageError::ReferenceParityUnknown)?;
        for pending in self
            .pending
            .list()
            .map_err(|_| BlobStorageError::ReferenceParityUnknown)?
            .into_iter()
            .filter(|pending| pending.expected_blob == *cid)
        {
            if pending.dataset_generation != self.pending.resolver.current_generation() {
                return Err(BlobStorageError::ReferenceParityUnknown);
            }
            owners.push(pending.intended_owner);
        }
        owners.sort_by_key(|owner| (owner.reference_kind, owner.cid));
        owners.dedup_by_key(|owner| (owner.reference_kind, owner.cid));
        Ok(owners)
    }
}

pub struct BlobAuthority {
    pending: Arc<PendingBlobUploadStore>,
    oracle: Arc<CanonicalBlobReferenceOracle>,
}

impl BlobAuthority {
    pub fn new(
        resolver: Arc<dyn DatasetPathResolver>,
        ids: Arc<dyn PendingUploadIdSource>,
        source: Arc<dyn ValidatedBlobReferenceSource>,
    ) -> Self {
        let pending = Arc::new(PendingBlobUploadStore::new(resolver, ids));
        let oracle = Arc::new(CanonicalBlobReferenceOracle::new(source, pending.clone()));
        Self { pending, oracle }
    }

    pub fn pending(&self) -> &Arc<PendingBlobUploadStore> {
        &self.pending
    }

    pub fn oracle(&self) -> Arc<dyn BlobReferenceOracle> {
        self.oracle.clone()
    }

    pub fn prepare(
        &self,
        intended_owner: ObjectReference,
        expected_blob: BlobCid,
        expected_type: BlobType,
        expected_length: u64,
    ) -> Result<PendingOwnedBlobUpload, BlobAuthorityError> {
        self.pending.prepare(
            intended_owner,
            expected_blob,
            expected_type,
            expected_length,
        )
    }

    pub fn abort(&self, id: PendingBlobUploadId) -> Result<bool, BlobAuthorityError> {
        self.pending.abort(id)
    }

    pub fn confirm_canonical_owner(
        &self,
        id: PendingBlobUploadId,
    ) -> Result<(), BlobAuthorityError> {
        let pending = self
            .pending
            .get(id)?
            .ok_or(BlobAuthorityError::BindingMismatch)?;
        let owners = self.oracle.canonical_owners(&pending.expected_blob)?;
        if !owners.contains(&pending.intended_owner) {
            return Err(BlobAuthorityError::BindingMismatch);
        }
        if !self.pending.abort(id)? {
            return Err(BlobAuthorityError::BindingMismatch);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum BlobAuthorityError {
    EntropyUnavailable,
    IdCollision(PendingBlobUploadId),
    PendingLimit,
    BindingMismatch,
    CanonicalSourceUnavailable,
    InvalidCanonicalReference,
    CorruptIntent,
    Storage(BlobStorageError),
    Io(std::io::Error),
    Codec(serde_json::Error),
}

impl std::fmt::Display for BlobAuthorityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl From<std::io::Error> for BlobAuthorityError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for BlobAuthorityError {
    fn from(error: serde_json::Error) -> Self {
        Self::Codec(error)
    }
}

impl From<BlobStorageError> for BlobAuthorityError {
    fn from(error: BlobStorageError) -> Self {
        Self::Storage(error)
    }
}

#[derive(Serialize, Deserialize)]
struct PendingUploadRecord {
    id: PendingBlobUploadId,
    owner_kind: u64,
    owner_cid: [u8; 32],
    expected_blob: BlobCid,
    expected_type: BlobType,
    expected_length: u64,
    dataset_generation: DatasetGenerationId,
}

impl PendingUploadRecord {
    fn from_pending(value: &PendingOwnedBlobUpload) -> Self {
        Self {
            id: value.id,
            owner_kind: value.intended_owner.reference_kind,
            owner_cid: value.intended_owner.cid,
            expected_blob: value.expected_blob,
            expected_type: value.expected_type,
            expected_length: value.expected_length,
            dataset_generation: value.dataset_generation,
        }
    }

    fn into_pending(self) -> Result<PendingOwnedBlobUpload, BlobAuthorityError> {
        if self.expected_length > BLOB_MAX_SIZE
            || self.expected_blob.version() != ku_core::blob_store::BLOB_CID_VERSION
            || self.expected_blob.blob_type() != self.expected_type
            || self.expected_blob.0[1] != self.expected_type as u8
        {
            return Err(BlobAuthorityError::CorruptIntent);
        }
        Ok(PendingOwnedBlobUpload {
            id: self.id,
            intended_owner: ObjectReference::new(self.owner_kind, self.owner_cid),
            expected_blob: self.expected_blob,
            expected_type: self.expected_type,
            expected_length: self.expected_length,
            dataset_generation: self.dataset_generation,
        })
    }
}
