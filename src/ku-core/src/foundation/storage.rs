//! Validate-before-persist storage boundary for vNext immutable records.
//!
//! Accepted bytes and quarantine bytes are separate namespaces. Callers cannot
//! insert into the accepted namespace without canonical/schema/CID/signature
//! validation performed by [`ValidatedStore`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use super::actor_root::{decode_actor_root_delegation, ValidatedActorRootDelegation};
use super::authority_event::{authority_event_descriptor, AuthorityEventDescriptor};
use super::canonical::ResourceProfile;
use super::content_id::{EventCid, ObjectCid, ReservedDomain};
use super::event::{decode_knowledge_event, EventType, ValidatedKnowledgeEvent};
use super::feed::{decode_feed_inception, ValidatedFeedInception};
use super::identity::FeedId;
use super::object::{
    decode_knowledge_object, DisclosureClass, KnownObjectKind, ValidatedKnowledgeObject,
};

pub const MAX_VERIFIED_ACCEPTED_RECORDS: u64 = 65_536;
pub const MAX_VERIFIED_QUARANTINE_RECORDS: u64 = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StoredRecordKind {
    Object = 1,
    Event = 2,
    FeedInception = 3,
    AuthorityEvent = 4,
}

impl StoredRecordKind {
    #[cfg(feature = "persist")]
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Object),
            2 => Some(Self::Event),
            3 => Some(Self::FeedInception),
            4 => Some(Self::AuthorityEvent),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AcceptedKey([u8; 33]);

impl AcceptedKey {
    fn new(kind: StoredRecordKind, cid: [u8; 32]) -> Self {
        let mut key = [0u8; 33];
        key[0] = kind as u8;
        key[1..].copy_from_slice(&cid);
        Self(key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantineRecord {
    pub quarantine_id: [u8; 32],
    pub record_kind: StoredRecordKind,
    pub claimed_cid: [u8; 32],
    pub reason_code: String,
    pub original_bytes: Vec<u8>,
}

impl QuarantineRecord {
    fn new(
        record_kind: StoredRecordKind,
        claimed_cid: [u8; 32],
        reason_code: impl Into<String>,
        original_bytes: &[u8],
    ) -> Self {
        let reason_code = reason_code.into();
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:local-quarantine:1\0");
        hasher.update(&[record_kind as u8]);
        hasher.update(&claimed_cid);
        hasher.update(&(reason_code.len() as u64).to_be_bytes());
        hasher.update(reason_code.as_bytes());
        hasher.update(original_bytes);
        Self {
            quarantine_id: *hasher.finalize().as_bytes(),
            record_kind,
            claimed_cid,
            reason_code,
            original_bytes: original_bytes.to_vec(),
        }
    }

    /// Quarantine is evidence for inspection/reconciliation, never executable input.
    pub const fn is_executable(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PutVerifiedOutcome {
    Stored,
    AlreadyPresent,
    Quarantined { quarantine_id: [u8; 32] },
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendAcceptOutcome {
    Stored,
    AlreadyPresent,
    CollisionQuarantined,
}

pub trait AtomicVerifiedBackend: Send + Sync {
    fn accept_or_quarantine_collision(
        &self,
        key: &[u8; 33],
        bytes: &[u8],
        collision: &QuarantineRecord,
    ) -> Result<BackendAcceptOutcome, String>;

    fn quarantine(&self, record: &QuarantineRecord) -> Result<(), String>;

    fn get_accepted(&self, key: &[u8; 33]) -> Result<Option<Vec<u8>>, String>;

    fn get_quarantine(&self, id: &[u8; 32]) -> Result<Option<QuarantineRecord>, String>;

    /// Deterministic accepted-record scan used to rebuild derived projections
    /// such as feed sequence/equivocation state after restart.
    fn accepted_records(&self, kind: StoredRecordKind) -> Result<Vec<Vec<u8>>, String>;

    /// Atomically accept canonical FeedInception bytes and index every branch
    /// by FeedId. Multiple branches remain visible; arrival order never grants
    /// authority.
    fn accept_feed_inception(
        &self,
        key: &[u8; 33],
        feed_id: &[u8; 32],
        bytes: &[u8],
        collision: &QuarantineRecord,
    ) -> Result<BackendAcceptOutcome, String>;

    fn feed_inceptions(&self, feed_id: &[u8; 32]) -> Result<Vec<Vec<u8>>, String>;
}

#[derive(Default)]
struct InMemoryState {
    accepted: HashMap<[u8; 33], Vec<u8>>,
    quarantine: HashMap<[u8; 32], QuarantineRecord>,
    feed_index: HashMap<[u8; 64], Vec<u8>>,
}

#[derive(Default)]
pub struct InMemoryVerifiedBackend {
    state: Mutex<InMemoryState>,
}

impl AtomicVerifiedBackend for InMemoryVerifiedBackend {
    fn accept_or_quarantine_collision(
        &self,
        key: &[u8; 33],
        bytes: &[u8],
        collision: &QuarantineRecord,
    ) -> Result<BackendAcceptOutcome, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?;
        match state.accepted.get(key) {
            Some(existing) if existing == bytes => Ok(BackendAcceptOutcome::AlreadyPresent),
            Some(_) => {
                state
                    .quarantine
                    .entry(collision.quarantine_id)
                    .or_insert_with(|| collision.clone());
                Ok(BackendAcceptOutcome::CollisionQuarantined)
            }
            None => {
                state.accepted.insert(*key, bytes.to_vec());
                Ok(BackendAcceptOutcome::Stored)
            }
        }
    }

    fn quarantine(&self, record: &QuarantineRecord) -> Result<(), String> {
        self.state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?
            .quarantine
            .entry(record.quarantine_id)
            .or_insert_with(|| record.clone());
        Ok(())
    }

    fn get_accepted(&self, key: &[u8; 33]) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?
            .accepted
            .get(key)
            .cloned())
    }

    fn get_quarantine(&self, id: &[u8; 32]) -> Result<Option<QuarantineRecord>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?
            .quarantine
            .get(id)
            .cloned())
    }

    fn accepted_records(&self, kind: StoredRecordKind) -> Result<Vec<Vec<u8>>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?;
        let mut records = state
            .accepted
            .iter()
            .filter(|(key, _)| key[0] == kind as u8)
            .map(|(key, bytes)| (*key, bytes.clone()))
            .collect::<Vec<_>>();
        records.sort_by_key(|(key, _)| *key);
        Ok(records.into_iter().map(|(_, bytes)| bytes).collect())
    }

    fn accept_feed_inception(
        &self,
        key: &[u8; 33],
        feed_id: &[u8; 32],
        bytes: &[u8],
        collision: &QuarantineRecord,
    ) -> Result<BackendAcceptOutcome, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?;
        let outcome = match state.accepted.get(key) {
            Some(existing) if existing == bytes => BackendAcceptOutcome::AlreadyPresent,
            Some(_) => {
                state
                    .quarantine
                    .entry(collision.quarantine_id)
                    .or_insert_with(|| collision.clone());
                BackendAcceptOutcome::CollisionQuarantined
            }
            None => {
                state.accepted.insert(*key, bytes.to_vec());
                BackendAcceptOutcome::Stored
            }
        };
        if !matches!(outcome, BackendAcceptOutcome::CollisionQuarantined) {
            let mut index_key = [0u8; 64];
            index_key[..32].copy_from_slice(feed_id);
            index_key[32..].copy_from_slice(&key[1..]);
            state
                .feed_index
                .entry(index_key)
                .or_insert_with(|| bytes.to_vec());
        }
        Ok(outcome)
    }

    fn feed_inceptions(&self, feed_id: &[u8; 32]) -> Result<Vec<Vec<u8>>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "STORE_LOCK_POISONED".to_string())?;
        let mut branches = state
            .feed_index
            .iter()
            .filter(|(key, _)| &key[..32] == feed_id)
            .map(|(key, bytes)| (*key, bytes.clone()))
            .collect::<Vec<_>>();
        branches.sort_by_key(|(key, _)| *key);
        Ok(branches.into_iter().map(|(_, bytes)| bytes).collect())
    }
}

pub struct ValidatedStore<B> {
    backend: B,
}

impl<B: AtomicVerifiedBackend> ValidatedStore<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn put_verified_object(
        &self,
        claimed_cid: ObjectCid,
        bytes: &[u8],
        profile: ResourceProfile,
        known_kinds: &[KnownObjectKind],
        known_critical_extensions: &[u64],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let validated =
            match decode_knowledge_object(bytes, profile, known_kinds, known_critical_extensions) {
                Ok(validated) => validated,
                Err(error) => {
                    return self.quarantine(
                        StoredRecordKind::Object,
                        claimed_cid.into_bytes(),
                        error.code(),
                        bytes,
                    )
                }
            };
        self.put_validated_object(claimed_cid, &validated)
    }

    /// Persist an object after a schema-specific decoder has validated its
    /// typed payload in addition to the generic envelope boundary.
    pub fn put_validated_object(
        &self,
        claimed_cid: ObjectCid,
        validated: &ValidatedKnowledgeObject,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        if !matches!(
            validated.disclosure(),
            DisclosureClass::Public | DisclosureClass::RouteMinimal
        ) {
            return Err(VerifiedStoreError::StorageClassMismatch(
                validated.disclosure(),
            ));
        }
        if validated.cid() != claimed_cid {
            return self.quarantine(
                StoredRecordKind::Object,
                claimed_cid.into_bytes(),
                "CID_MISMATCH",
                validated.original_bytes(),
            );
        }
        self.accept(
            StoredRecordKind::Object,
            claimed_cid.into_bytes(),
            validated.original_bytes(),
        )
    }

    pub fn quarantine_object(
        &self,
        claimed_cid: ObjectCid,
        bytes: &[u8],
        reason: &'static str,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        self.quarantine(
            StoredRecordKind::Object,
            claimed_cid.into_bytes(),
            reason,
            bytes,
        )
    }

    pub fn put_verified_event(
        &self,
        claimed_cid: EventCid,
        bytes: &[u8],
        author: &ValidatedFeedInception,
        known_event_types: &[EventType],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let validated = match decode_knowledge_event(bytes, author, known_event_types) {
            Ok(validated) => validated,
            Err(error) => {
                return self.quarantine(
                    StoredRecordKind::Event,
                    claimed_cid.into_bytes(),
                    error.code(),
                    bytes,
                )
            }
        };
        self.put_validated_event(claimed_cid, &validated)
    }

    /// Persist an event that has already crossed the canonical decoder and
    /// signature boundary. This lets dependency-aware callers inspect payload
    /// and causal references without decoding the same signed event twice.
    pub fn put_validated_event(
        &self,
        claimed_cid: EventCid,
        validated: &ValidatedKnowledgeEvent,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        if !matches!(
            validated.signed.event.disclosure,
            DisclosureClass::Public | DisclosureClass::RouteMinimal
        ) {
            return Err(VerifiedStoreError::StorageClassMismatch(
                validated.signed.event.disclosure,
            ));
        }
        if validated.cid() != claimed_cid {
            return self.quarantine(
                StoredRecordKind::Event,
                claimed_cid.into_bytes(),
                "CID_MISMATCH",
                validated.original_bytes(),
            );
        }
        self.accept(
            StoredRecordKind::Event,
            claimed_cid.into_bytes(),
            validated.original_bytes(),
        )
    }

    /// Validate and atomically persist a signed feed inception plus its FeedId
    /// index. `claimed_cid` commits the complete canonical control record; it
    /// is deliberately distinct from the stable FeedId.
    pub fn put_verified_feed_inception(
        &self,
        claimed_cid: [u8; 32],
        bytes: &[u8],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let validated = match decode_feed_inception(bytes) {
            Ok(validated) => validated,
            Err(error) => {
                return self.quarantine(
                    StoredRecordKind::FeedInception,
                    claimed_cid,
                    error.code(),
                    bytes,
                )
            }
        };
        self.put_validated_feed_inception(claimed_cid, &validated)
    }

    pub fn put_validated_feed_inception(
        &self,
        claimed_cid: [u8; 32],
        validated: &ValidatedFeedInception,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let bytes = validated.original_bytes();
        if ReservedDomain::FeedInception.digest(bytes) != claimed_cid {
            return self.quarantine(
                StoredRecordKind::FeedInception,
                claimed_cid,
                "CID_MISMATCH",
                bytes,
            );
        }
        let key = AcceptedKey::new(StoredRecordKind::FeedInception, claimed_cid).0;
        let collision = QuarantineRecord::new(
            StoredRecordKind::FeedInception,
            claimed_cid,
            "SAME_CID_DIFFERENT_BYTES",
            bytes,
        );
        match self
            .backend
            .accept_feed_inception(&key, validated.feed_id.as_bytes(), bytes, &collision)
            .map_err(VerifiedStoreError::Backend)?
        {
            BackendAcceptOutcome::Stored => Ok(PutVerifiedOutcome::Stored),
            BackendAcceptOutcome::AlreadyPresent => Ok(PutVerifiedOutcome::AlreadyPresent),
            BackendAcceptOutcome::CollisionQuarantined => Ok(PutVerifiedOutcome::Quarantined {
                quarantine_id: collision.quarantine_id,
            }),
        }
    }

    /// Validate and persist a self-certifying actor-root delegation. Authority
    /// records have a separate domain and namespace from authored knowledge
    /// events, so arbitrary event bytes can never enter the authority reducer.
    pub fn put_verified_actor_root_delegation(
        &self,
        claimed_cid: EventCid,
        bytes: &[u8],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let validated = match decode_actor_root_delegation(bytes) {
            Ok(validated) => validated,
            Err(error) => {
                return self.quarantine(
                    StoredRecordKind::AuthorityEvent,
                    claimed_cid.into_bytes(),
                    error.code(),
                    bytes,
                )
            }
        };
        self.put_validated_actor_root_delegation(claimed_cid, &validated)
    }

    pub fn put_validated_actor_root_delegation(
        &self,
        claimed_cid: EventCid,
        validated: &ValidatedActorRootDelegation,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        if validated.cid != claimed_cid {
            return self.quarantine(
                StoredRecordKind::AuthorityEvent,
                claimed_cid.into_bytes(),
                "CID_MISMATCH",
                validated.original_bytes(),
            );
        }
        self.accept(
            StoredRecordKind::AuthorityEvent,
            claimed_cid.into_bytes(),
            validated.original_bytes(),
        )
    }

    /// Persist an authority event that has already crossed its canonical,
    /// signature, parent and attenuation checks in the dependency-aware sink.
    pub fn put_validated_authority_event(
        &self,
        claimed_cid: EventCid,
        actual_cid: EventCid,
        bytes: &[u8],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        if claimed_cid != actual_cid {
            return self.quarantine(
                StoredRecordKind::AuthorityEvent,
                claimed_cid.into_bytes(),
                "CID_MISMATCH",
                bytes,
            );
        }
        self.accept(
            StoredRecordKind::AuthorityEvent,
            claimed_cid.into_bytes(),
            bytes,
        )
    }

    pub fn quarantine_authority_event(
        &self,
        claimed_cid: EventCid,
        bytes: &[u8],
        reason: &'static str,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        self.quarantine(
            StoredRecordKind::AuthorityEvent,
            claimed_cid.into_bytes(),
            reason,
            bytes,
        )
    }

    pub fn quarantine_feed_inception(
        &self,
        claimed_cid: [u8; 32],
        bytes: &[u8],
        reason: &'static str,
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        self.quarantine(StoredRecordKind::FeedInception, claimed_cid, reason, bytes)
    }

    /// Return all valid inception branches for a FeedId in deterministic CID
    /// order. Key-rotation/delegation authority remains a higher-layer policy.
    pub fn feed_inceptions(
        &self,
        feed_id: FeedId,
    ) -> Result<Vec<ValidatedFeedInception>, VerifiedStoreError> {
        self.backend
            .feed_inceptions(feed_id.as_bytes())
            .map_err(VerifiedStoreError::Backend)?
            .into_iter()
            .map(|bytes| {
                decode_feed_inception(&bytes)
                    .map_err(|error| VerifiedStoreError::Backend(error.to_string()))
            })
            .collect()
    }

    pub fn get_object(&self, cid: ObjectCid) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        self.get(StoredRecordKind::Object, cid.into_bytes())
    }

    /// Deterministic CID-ordered scan of objects that crossed the complete
    /// canonical/schema/CID validation boundary.
    pub fn accepted_objects(&self) -> Result<Vec<Vec<u8>>, VerifiedStoreError> {
        self.backend
            .accepted_records(StoredRecordKind::Object)
            .map_err(VerifiedStoreError::Backend)
    }

    pub fn get_event(&self, cid: EventCid) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        self.get(StoredRecordKind::Event, cid.into_bytes())
    }

    pub fn accepted_events(&self) -> Result<Vec<Vec<u8>>, VerifiedStoreError> {
        self.backend
            .accepted_records(StoredRecordKind::Event)
            .map_err(VerifiedStoreError::Backend)
    }

    pub fn get_actor_root_delegation(
        &self,
        cid: EventCid,
    ) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        self.get(StoredRecordKind::AuthorityEvent, cid.into_bytes())
    }

    pub fn get_authority_event(
        &self,
        cid: EventCid,
    ) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        self.get(StoredRecordKind::AuthorityEvent, cid.into_bytes())
    }

    pub fn accepted_authority_events(&self) -> Result<Vec<Vec<u8>>, VerifiedStoreError> {
        self.backend
            .accepted_records(StoredRecordKind::AuthorityEvent)
            .map_err(VerifiedStoreError::Backend)
    }

    /// Deterministic CID-ordered scan used to rebuild frontier-relative actor
    /// authority after restart. Every returned record crossed the canonical
    /// decoder, self-certifying ActorId check, and root-key signature boundary.
    pub fn accepted_actor_root_delegations(
        &self,
    ) -> Result<Vec<ValidatedActorRootDelegation>, VerifiedStoreError> {
        let mut roots = Vec::new();
        for bytes in self.accepted_authority_events()? {
            match authority_event_descriptor(&bytes)
                .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?
            {
                AuthorityEventDescriptor::Root => roots.push(
                    decode_actor_root_delegation(&bytes)
                        .map_err(|error| VerifiedStoreError::Backend(error.to_string()))?,
                ),
                AuthorityEventDescriptor::Delegation { .. }
                | AuthorityEventDescriptor::Revocation { .. } => {}
            }
        }
        Ok(roots)
    }

    pub fn get_quarantine(
        &self,
        quarantine_id: &[u8; 32],
    ) -> Result<Option<QuarantineRecord>, VerifiedStoreError> {
        self.backend
            .get_quarantine(quarantine_id)
            .map_err(VerifiedStoreError::Backend)
    }

    pub(crate) fn get(
        &self,
        kind: StoredRecordKind,
        cid: [u8; 32],
    ) -> Result<Option<Vec<u8>>, VerifiedStoreError> {
        self.backend
            .get_accepted(&AcceptedKey::new(kind, cid).0)
            .map_err(VerifiedStoreError::Backend)
    }

    pub(crate) fn accept(
        &self,
        kind: StoredRecordKind,
        cid: [u8; 32],
        bytes: &[u8],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let collision = QuarantineRecord::new(kind, cid, "SAME_CID_DIFFERENT_BYTES", bytes);
        match self
            .backend
            .accept_or_quarantine_collision(&AcceptedKey::new(kind, cid).0, bytes, &collision)
            .map_err(VerifiedStoreError::Backend)?
        {
            BackendAcceptOutcome::Stored => Ok(PutVerifiedOutcome::Stored),
            BackendAcceptOutcome::AlreadyPresent => Ok(PutVerifiedOutcome::AlreadyPresent),
            BackendAcceptOutcome::CollisionQuarantined => Ok(PutVerifiedOutcome::Quarantined {
                quarantine_id: collision.quarantine_id,
            }),
        }
    }

    pub(crate) fn quarantine(
        &self,
        kind: StoredRecordKind,
        claimed_cid: [u8; 32],
        reason: &str,
        bytes: &[u8],
    ) -> Result<PutVerifiedOutcome, VerifiedStoreError> {
        let record = QuarantineRecord::new(kind, claimed_cid, reason, bytes);
        self.backend
            .quarantine(&record)
            .map_err(VerifiedStoreError::Backend)?;
        Ok(PutVerifiedOutcome::Quarantined {
            quarantine_id: record.quarantine_id,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifiedStoreError {
    Backend(String),
    StorageClassMismatch(DisclosureClass),
    VaultCrypto,
    VaultCidMismatch,
}

impl fmt::Display for VerifiedStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(message) => write!(f, "STORE_BACKEND: {message}"),
            Self::StorageClassMismatch(disclosure) => {
                write!(f, "STORE_CLASS_MISMATCH: {disclosure:?}")
            }
            Self::VaultCrypto => f.write_str("VAULT_CRYPTO"),
            Self::VaultCidMismatch => f.write_str("VAULT_CID_MISMATCH"),
        }
    }
}

impl std::error::Error for VerifiedStoreError {}

#[cfg(feature = "persist")]
mod persistent {
    use std::path::Path;

    use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

    use super::*;

    const ACCEPTED: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_accepted_records");
    const QUARANTINE: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_quarantine_records");
    const FEED_INDEX: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_feed_inception_index");

    pub struct RedbVerifiedBackend {
        db: Database,
    }

    impl RedbVerifiedBackend {
        pub fn open(path: &Path) -> Result<Self, String> {
            let db = Database::create(path).map_err(|error| error.to_string())?;
            let write = db.begin_write().map_err(|error| error.to_string())?;
            {
                write
                    .open_table(ACCEPTED)
                    .map_err(|error| error.to_string())?;
                write
                    .open_table(QUARANTINE)
                    .map_err(|error| error.to_string())?;
                write
                    .open_table(FEED_INDEX)
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(Self { db })
        }
    }

    impl AtomicVerifiedBackend for RedbVerifiedBackend {
        fn accept_or_quarantine_collision(
            &self,
            key: &[u8; 33],
            bytes: &[u8],
            collision: &QuarantineRecord,
        ) -> Result<BackendAcceptOutcome, String> {
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            let outcome;
            {
                let mut accepted = write
                    .open_table(ACCEPTED)
                    .map_err(|error| error.to_string())?;
                let existing = accepted
                    .get(key.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec());
                outcome = match existing {
                    Some(existing) if existing == bytes => BackendAcceptOutcome::AlreadyPresent,
                    Some(_) => {
                        let mut quarantine = write
                            .open_table(QUARANTINE)
                            .map_err(|error| error.to_string())?;
                        let encoded = encode_quarantine(collision)?;
                        if quarantine
                            .get(collision.quarantine_id.as_slice())
                            .map_err(|error| error.to_string())?
                            .is_none()
                            && quarantine.len().map_err(|error| error.to_string())?
                                >= MAX_VERIFIED_QUARANTINE_RECORDS
                        {
                            return Err("VNEXT_VERIFIED_QUARANTINE_LIMIT".to_string());
                        }
                        quarantine
                            .insert(collision.quarantine_id.as_slice(), encoded.as_slice())
                            .map_err(|error| error.to_string())?;
                        BackendAcceptOutcome::CollisionQuarantined
                    }
                    None => {
                        if accepted.len().map_err(|error| error.to_string())?
                            >= MAX_VERIFIED_ACCEPTED_RECORDS
                        {
                            return Err("VNEXT_VERIFIED_ACCEPTED_LIMIT".to_string());
                        }
                        accepted
                            .insert(key.as_slice(), bytes)
                            .map_err(|error| error.to_string())?;
                        BackendAcceptOutcome::Stored
                    }
                };
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(outcome)
        }

        fn quarantine(&self, record: &QuarantineRecord) -> Result<(), String> {
            let encoded = encode_quarantine(record)?;
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            {
                let mut table = write
                    .open_table(QUARANTINE)
                    .map_err(|error| error.to_string())?;
                if table
                    .get(record.quarantine_id.as_slice())
                    .map_err(|error| error.to_string())?
                    .is_none()
                    && table.len().map_err(|error| error.to_string())?
                        >= MAX_VERIFIED_QUARANTINE_RECORDS
                {
                    return Err("VNEXT_VERIFIED_QUARANTINE_LIMIT".to_string());
                }
                table
                    .insert(record.quarantine_id.as_slice(), encoded.as_slice())
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())
        }

        fn get_accepted(&self, key: &[u8; 33]) -> Result<Option<Vec<u8>>, String> {
            let read = self.db.begin_read().map_err(|error| error.to_string())?;
            let table = read
                .open_table(ACCEPTED)
                .map_err(|error| error.to_string())?;
            table
                .get(key.as_slice())
                .map_err(|error| error.to_string())
                .map(|value| value.map(|guard| guard.value().to_vec()))
        }

        fn get_quarantine(&self, id: &[u8; 32]) -> Result<Option<QuarantineRecord>, String> {
            let read = self.db.begin_read().map_err(|error| error.to_string())?;
            let table = read
                .open_table(QUARANTINE)
                .map_err(|error| error.to_string())?;
            let encoded = table
                .get(id.as_slice())
                .map_err(|error| error.to_string())?
                .map(|guard| guard.value().to_vec());
            encoded.map(|bytes| decode_quarantine(&bytes)).transpose()
        }

        fn accepted_records(&self, kind: StoredRecordKind) -> Result<Vec<Vec<u8>>, String> {
            let read = self.db.begin_read().map_err(|error| error.to_string())?;
            let table = read
                .open_table(ACCEPTED)
                .map_err(|error| error.to_string())?;
            let mut records = Vec::new();
            for entry in table.iter().map_err(|error| error.to_string())? {
                let (key, bytes) = entry.map_err(|error| error.to_string())?;
                if key.value().first() == Some(&(kind as u8)) {
                    records.push((key.value().to_vec(), bytes.value().to_vec()));
                }
            }
            records.sort_by(|left, right| left.0.cmp(&right.0));
            Ok(records.into_iter().map(|(_, bytes)| bytes).collect())
        }

        fn accept_feed_inception(
            &self,
            key: &[u8; 33],
            feed_id: &[u8; 32],
            bytes: &[u8],
            collision: &QuarantineRecord,
        ) -> Result<BackendAcceptOutcome, String> {
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            let outcome;
            {
                let mut accepted = write
                    .open_table(ACCEPTED)
                    .map_err(|error| error.to_string())?;
                let existing = accepted
                    .get(key.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec());
                outcome = match existing {
                    Some(existing) if existing == bytes => BackendAcceptOutcome::AlreadyPresent,
                    Some(_) => {
                        let mut quarantine = write
                            .open_table(QUARANTINE)
                            .map_err(|error| error.to_string())?;
                        let encoded = encode_quarantine(collision)?;
                        if quarantine
                            .get(collision.quarantine_id.as_slice())
                            .map_err(|error| error.to_string())?
                            .is_none()
                            && quarantine.len().map_err(|error| error.to_string())?
                                >= MAX_VERIFIED_QUARANTINE_RECORDS
                        {
                            return Err("VNEXT_VERIFIED_QUARANTINE_LIMIT".to_string());
                        }
                        quarantine
                            .insert(collision.quarantine_id.as_slice(), encoded.as_slice())
                            .map_err(|error| error.to_string())?;
                        BackendAcceptOutcome::CollisionQuarantined
                    }
                    None => {
                        if accepted.len().map_err(|error| error.to_string())?
                            >= MAX_VERIFIED_ACCEPTED_RECORDS
                        {
                            return Err("VNEXT_VERIFIED_ACCEPTED_LIMIT".to_string());
                        }
                        accepted
                            .insert(key.as_slice(), bytes)
                            .map_err(|error| error.to_string())?;
                        BackendAcceptOutcome::Stored
                    }
                };
                if !matches!(outcome, BackendAcceptOutcome::CollisionQuarantined) {
                    let mut index_key = [0u8; 64];
                    index_key[..32].copy_from_slice(feed_id);
                    index_key[32..].copy_from_slice(&key[1..]);
                    write
                        .open_table(FEED_INDEX)
                        .map_err(|error| error.to_string())?
                        .insert(index_key.as_slice(), bytes)
                        .map_err(|error| error.to_string())?;
                }
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(outcome)
        }

        fn feed_inceptions(&self, feed_id: &[u8; 32]) -> Result<Vec<Vec<u8>>, String> {
            let read = self.db.begin_read().map_err(|error| error.to_string())?;
            let table = read
                .open_table(FEED_INDEX)
                .map_err(|error| error.to_string())?;
            let mut branches = Vec::new();
            for entry in table.iter().map_err(|error| error.to_string())? {
                let (key, bytes) = entry.map_err(|error| error.to_string())?;
                if key.value().get(..32) == Some(feed_id.as_slice()) {
                    branches.push((key.value().to_vec(), bytes.value().to_vec()));
                }
            }
            branches.sort_by(|left, right| left.0.cmp(&right.0));
            Ok(branches.into_iter().map(|(_, bytes)| bytes).collect())
        }
    }

    fn encode_quarantine(record: &QuarantineRecord) -> Result<Vec<u8>, String> {
        let reason = record.reason_code.as_bytes();
        let reason_len = u16::try_from(reason.len()).map_err(|_| "reason too long".to_string())?;
        let bytes_len = u64::try_from(record.original_bytes.len())
            .map_err(|_| "record too long".to_string())?;
        let mut output = Vec::with_capacity(43 + reason.len() + record.original_bytes.len());
        output.push(record.record_kind as u8);
        output.extend_from_slice(&record.claimed_cid);
        output.extend_from_slice(&reason_len.to_be_bytes());
        output.extend_from_slice(reason);
        output.extend_from_slice(&bytes_len.to_be_bytes());
        output.extend_from_slice(&record.original_bytes);
        Ok(output)
    }

    fn decode_quarantine(bytes: &[u8]) -> Result<QuarantineRecord, String> {
        if bytes.len() < 43 {
            return Err("quarantine record truncated".to_string());
        }
        let record_kind = StoredRecordKind::from_byte(bytes[0])
            .ok_or_else(|| "quarantine record kind".to_string())?;
        let mut claimed_cid = [0u8; 32];
        claimed_cid.copy_from_slice(&bytes[1..33]);
        let reason_len = u16::from_be_bytes([bytes[33], bytes[34]]) as usize;
        let reason_end = 35usize
            .checked_add(reason_len)
            .ok_or_else(|| "quarantine reason overflow".to_string())?;
        let length_end = reason_end
            .checked_add(8)
            .ok_or_else(|| "quarantine length overflow".to_string())?;
        if length_end > bytes.len() {
            return Err("quarantine record truncated".to_string());
        }
        let reason_code = std::str::from_utf8(&bytes[35..reason_end])
            .map_err(|_| "quarantine reason UTF-8".to_string())?
            .to_string();
        let original_len = u64::from_be_bytes(
            bytes[reason_end..length_end]
                .try_into()
                .map_err(|_| "quarantine length".to_string())?,
        ) as usize;
        if bytes.len().checked_sub(length_end) != Some(original_len) {
            return Err("quarantine payload length".to_string());
        }
        Ok(QuarantineRecord::new(
            record_kind,
            claimed_cid,
            reason_code,
            &bytes[length_end..],
        ))
    }

    #[cfg(test)]
    mod tests {
        use std::time::{SystemTime, UNIX_EPOCH};

        use super::*;

        fn test_path(label: &str) -> std::path::PathBuf {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!(
                "onebrain-vnext-{label}-{}-{nonce}.redb",
                std::process::id()
            ))
        }

        #[test]
        fn redb_accept_and_collision_quarantine_commit_atomically() {
            let path = test_path("atomic");
            let backend = RedbVerifiedBackend::open(&path).unwrap();
            let key = AcceptedKey::new(StoredRecordKind::Object, [1; 32]).0;
            let first = [0xa0];
            let changed = [0xa1, 0x00, 0x01];
            let collision = QuarantineRecord::new(
                StoredRecordKind::Object,
                [1; 32],
                "SAME_CID_DIFFERENT_BYTES",
                &changed,
            );
            assert_eq!(
                backend
                    .accept_or_quarantine_collision(&key, &first, &collision)
                    .unwrap(),
                BackendAcceptOutcome::Stored
            );
            assert_eq!(
                backend
                    .accept_or_quarantine_collision(&key, &changed, &collision)
                    .unwrap(),
                BackendAcceptOutcome::CollisionQuarantined
            );
            assert_eq!(backend.get_accepted(&key).unwrap().unwrap(), first);
            assert_eq!(
                backend
                    .get_quarantine(&collision.quarantine_id)
                    .unwrap()
                    .unwrap(),
                collision
            );
            drop(backend);
            std::fs::remove_file(path).unwrap();
        }

        #[test]
        fn dropped_redb_transaction_leaves_no_partial_accept() {
            let path = test_path("crash");
            let backend = RedbVerifiedBackend::open(&path).unwrap();
            let key = AcceptedKey::new(StoredRecordKind::Event, [2; 32]).0;
            {
                let write = backend.db.begin_write().unwrap();
                {
                    let mut accepted = write.open_table(ACCEPTED).unwrap();
                    accepted.insert(key.as_slice(), &[0xa0][..]).unwrap();
                }
                drop(write); // simulated process stop before commit
            }
            assert!(backend.get_accepted(&key).unwrap().is_none());
            drop(backend);

            let reopened = RedbVerifiedBackend::open(&path).unwrap();
            assert!(reopened.get_accepted(&key).unwrap().is_none());
            drop(reopened);
            std::fs::remove_file(path).unwrap();
        }
    }

    pub use RedbVerifiedBackend as Backend;
}

#[cfg(feature = "persist")]
pub use persistent::Backend as RedbVerifiedBackend;

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, ActorRootDelegation, CanonicalValue, DeviceId, DisclosureClass,
        FeedInception, KnowledgeEventEnvelope, KnowledgeObjectEnvelope, NamespaceCommitment,
        ObjectKind, SchemaVersion,
    };

    const KNOWN_KIND: KnownObjectKind = KnownObjectKind::new(ObjectKind(10), 1);
    const KNOWN_EVENT: EventType = EventType(1);

    fn object() -> (Vec<u8>, ObjectCid) {
        KnowledgeObjectEnvelope::new(
            ObjectKind(10),
            SchemaVersion::new(1, 0),
            DisclosureClass::Public,
            CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(1))]),
        )
        .encode(ResourceProfile::ObjectV1)
        .unwrap()
    }

    fn author() -> (SigningKey, ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[1; 32]);
        let feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"store-test", [2; 32]).unwrap(),
            0,
            super::super::identity::DeviceId::from_bytes([3; 32]),
        )
        .sign(&key)
        .unwrap();
        let feed = decode_feed_inception(&feed.encode().unwrap()).unwrap();
        (key, feed)
    }

    #[test]
    fn valid_object_is_exact_and_idempotent() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let (bytes, cid) = object();
        assert_eq!(
            store
                .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
                .unwrap(),
            PutVerifiedOutcome::Stored
        );
        assert_eq!(
            store
                .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
                .unwrap(),
            PutVerifiedOutcome::AlreadyPresent
        );
        assert_eq!(store.get_object(cid).unwrap().unwrap(), bytes);
    }

    #[test]
    fn same_claimed_cid_with_changed_bytes_never_replaces_accepted_bytes() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let (bytes, cid) = object();
        store
            .put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
            .unwrap();
        let mut changed = bytes.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        let PutVerifiedOutcome::Quarantined { quarantine_id } = store
            .put_verified_object(cid, &changed, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[])
            .unwrap()
        else {
            panic!("changed bytes must be quarantined");
        };
        assert_eq!(store.get_object(cid).unwrap().unwrap(), bytes);
        let record = store.get_quarantine(&quarantine_id).unwrap().unwrap();
        assert!(!record.is_executable());
        assert_eq!(record.original_bytes, changed);
    }

    #[test]
    fn malformed_object_goes_only_to_quarantine() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let claimed = ObjectCid::from_bytes([9; 32]);
        let PutVerifiedOutcome::Quarantined { quarantine_id } = store
            .put_verified_object(
                claimed,
                &[0xff],
                ResourceProfile::ObjectV1,
                &[KNOWN_KIND],
                &[],
            )
            .unwrap()
        else {
            panic!("malformed object must be quarantined");
        };
        assert!(store.get_object(claimed).unwrap().is_none());
        assert_eq!(
            store
                .get_quarantine(&quarantine_id)
                .unwrap()
                .unwrap()
                .reason_code,
            "CANONICAL_FORBIDDEN_TYPE"
        );
    }

    #[test]
    fn private_disclosure_is_never_persisted_by_public_store() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let (bytes, cid) = KnowledgeObjectEnvelope::new(
            ObjectKind(10),
            SchemaVersion::new(1, 0),
            DisclosureClass::LocalOnly,
            CanonicalValue::Bytes(b"private".to_vec()),
        )
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        assert!(matches!(
            store.put_verified_object(cid, &bytes, ResourceProfile::ObjectV1, &[KNOWN_KIND], &[],),
            Err(VerifiedStoreError::StorageClassMismatch(
                DisclosureClass::LocalOnly
            ))
        ));
        assert!(store.get_object(cid).unwrap().is_none());
    }

    #[test]
    fn invalid_event_signature_cannot_enter_accepted_namespace() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let (key, author) = author();
        let event = KnowledgeEventEnvelope::new(
            KNOWN_EVENT,
            author.feed_id,
            0,
            DisclosureClass::Public,
            [8; 32],
        )
        .sign(&author, &key)
        .unwrap();
        let (bytes, cid) = event.encode().unwrap();
        let mut tampered = bytes.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        let outcome = store
            .put_verified_event(cid, &tampered, &author, &[KNOWN_EVENT])
            .unwrap();
        assert!(matches!(outcome, PutVerifiedOutcome::Quarantined { .. }));
        assert!(store.get_event(cid).unwrap().is_none());
    }

    #[test]
    fn feed_inception_is_verified_and_indexed_by_feed_id() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let (_, author) = author();
        let bytes = author.original_bytes().to_vec();
        let cid = ReservedDomain::FeedInception.digest(&bytes);
        assert_eq!(
            store.put_verified_feed_inception(cid, &bytes).unwrap(),
            PutVerifiedOutcome::Stored
        );
        assert_eq!(
            store.put_verified_feed_inception(cid, &bytes).unwrap(),
            PutVerifiedOutcome::AlreadyPresent
        );
        let branches = store.feed_inceptions(author.feed_id).unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].original_bytes(), bytes);
    }

    #[test]
    fn actor_root_delegation_is_domain_separated_verified_and_idempotent() {
        let store = ValidatedStore::new(InMemoryVerifiedBackend::default());
        let root_key = SigningKey::from_bytes(&[0x31; 32]);
        let (_, feed) = author();
        let proof = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            feed.feed_id,
            DeviceId::from_bytes([3; 32]),
            Some(feed.signed.inception.namespace_commitment),
            0,
            0,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let cid = EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&proof));
        assert_eq!(
            store
                .put_verified_actor_root_delegation(cid, &proof)
                .unwrap(),
            PutVerifiedOutcome::Stored
        );
        assert_eq!(
            store
                .put_verified_actor_root_delegation(cid, &proof)
                .unwrap(),
            PutVerifiedOutcome::AlreadyPresent
        );
        assert_eq!(
            store.get_actor_root_delegation(cid).unwrap().unwrap(),
            proof
        );
        assert_eq!(store.accepted_actor_root_delegations().unwrap().len(), 1);

        let false_cid = EventCid::from_bytes(ReservedDomain::Event.digest(&proof));
        assert!(matches!(
            store
                .put_verified_actor_root_delegation(false_cid, &proof)
                .unwrap(),
            PutVerifiedOutcome::Quarantined { .. }
        ));
        assert!(store
            .get_actor_root_delegation(false_cid)
            .unwrap()
            .is_none());
    }

    #[cfg(feature = "persist")]
    #[test]
    fn feed_inception_index_survives_redb_restart() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "onebrain-feed-index-{}-{nonce}.redb",
            std::process::id()
        ));
        let (_, author) = author();
        let feed_id = author.feed_id;
        let bytes = author.original_bytes().to_vec();
        let cid = ReservedDomain::FeedInception.digest(&bytes);
        {
            let store = ValidatedStore::new(RedbVerifiedBackend::open(&path).unwrap());
            assert_eq!(
                store.put_verified_feed_inception(cid, &bytes).unwrap(),
                PutVerifiedOutcome::Stored
            );
        }
        let reopened = ValidatedStore::new(RedbVerifiedBackend::open(&path).unwrap());
        let branches = reopened.feed_inceptions(feed_id).unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].original_bytes(), bytes);
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(feature = "persist")]
    #[test]
    fn actor_root_delegation_survives_redb_restart() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "onebrain-authority-root-{}-{nonce}.redb",
            std::process::id()
        ));
        let root_key = SigningKey::from_bytes(&[0x32; 32]);
        let (_, feed) = author();
        let bytes = ActorRootDelegation::new(
            *root_key.verifying_key().as_bytes(),
            feed.feed_id,
            DeviceId::from_bytes([3; 32]),
            Some(feed.signed.inception.namespace_commitment),
            0,
            0,
        )
        .unwrap()
        .sign(&root_key)
        .unwrap()
        .encode()
        .unwrap();
        let cid = EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&bytes));
        {
            let store = ValidatedStore::new(RedbVerifiedBackend::open(&path).unwrap());
            assert_eq!(
                store
                    .put_verified_actor_root_delegation(cid, &bytes)
                    .unwrap(),
                PutVerifiedOutcome::Stored
            );
        }
        let reopened = ValidatedStore::new(RedbVerifiedBackend::open(&path).unwrap());
        assert_eq!(
            reopened.get_actor_root_delegation(cid).unwrap().unwrap(),
            bytes
        );
        assert_eq!(reopened.accepted_actor_root_delegations().unwrap().len(), 1);
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }
}
