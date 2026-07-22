//! Crash-resumable journal around the deterministic reconciliation receiver.
//!
//! The durable journal stores manifests, accepted identities, retry counters
//! and continuation state; payload bytes remain owned by the validated sink.
//! If the process crashes after sink acceptance but before journal commit,
//! fair redelivery observes `AlreadyPresent` and repairs the journal.

use std::collections::BTreeMap;
use std::sync::Mutex;

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ReservedDomain,
    ResourceProfile,
};
use onebrain_protocol::{
    decode_reconciliation_message, encode_reconciliation_message, make_resume_token,
    reconciliation_binding_digest, validate_reconciliation_context, ReconcileManifestKind,
    ReconcileReceiptStatus, ReconciliationBody, ReconciliationContext, ReconciliationMessage,
    ReconciliationResumeMode, ReconciliationResumeToken,
};

use crate::vnext_reconciliation::{
    BoundPayloadFrame, ManifestIngestOutcome, PayloadIngestOutcome, ReceiverState,
    ReconciliationError, ReconciliationReceiver, ValidateThenAcceptSink,
};

const JOURNAL_MAJOR: u64 = 1;
const JOURNAL_MINOR: u64 = 0;
const MAX_MANIFEST_BATCHES: usize = 4_096;
const MAX_JOURNAL_RECORDS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconciliationJournalConfig {
    pub max_retries_per_record: u64,
    pub max_inflight_bytes: u64,
}

impl ReconciliationJournalConfig {
    pub fn validate(self) -> Result<Self, ReconciliationJournalError> {
        if self.max_retries_per_record == 0
            || self.max_retries_per_record > 1_024
            || self.max_inflight_bytes == 0
            || self.max_inflight_bytes > 16 * 1_048_576
        {
            return Err(ReconciliationJournalError::InvalidConfig);
        }
        Ok(self)
    }
}

pub trait ReconciliationJournalBackend: Send + Sync {
    fn load(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String>;
    fn store_atomically(&self, binding: &[u8; 32], bytes: &[u8]) -> Result<(), String>;
}

#[derive(Default)]
pub struct InMemoryReconciliationJournalBackend {
    snapshots: Mutex<BTreeMap<[u8; 32], Vec<u8>>>,
}

impl ReconciliationJournalBackend for InMemoryReconciliationJournalBackend {
    fn load(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .snapshots
            .lock()
            .map_err(|_| "RECONCILIATION_JOURNAL_LOCK_POISONED".to_owned())?
            .get(binding)
            .cloned())
    }

    fn store_atomically(&self, binding: &[u8; 32], bytes: &[u8]) -> Result<(), String> {
        self.snapshots
            .lock()
            .map_err(|_| "RECONCILIATION_JOURNAL_LOCK_POISONED".to_owned())?
            .insert(*binding, bytes.to_vec());
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AcceptedRecord {
    kind: ReconcileManifestKind,
    cid: [u8; 32],
    status: ReconcileReceiptStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JournalProjection {
    binding: [u8; 32],
    config: ReconciliationJournalConfig,
    next_sequence: u64,
    manifests: BTreeMap<[u8; 32], Vec<u8>>,
    accepted: BTreeMap<(u64, [u8; 32]), AcceptedRecord>,
    retries: BTreeMap<(u64, [u8; 32]), u64>,
    inflight_bytes: u64,
}

impl JournalProjection {
    fn new(binding: [u8; 32], config: ReconciliationJournalConfig) -> Self {
        Self {
            binding,
            config,
            next_sequence: 0,
            manifests: BTreeMap::new(),
            accepted: BTreeMap::new(),
            retries: BTreeMap::new(),
            inflight_bytes: 0,
        }
    }

    fn encode(&self) -> Result<Vec<u8>, ReconciliationJournalError> {
        let manifests = self
            .manifests
            .iter()
            .map(|(digest, bytes)| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Bytes(digest.to_vec())),
                    (1, CanonicalValue::Bytes(bytes.clone())),
                ])
            })
            .collect();
        let accepted = self
            .accepted
            .values()
            .map(|record| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(record.kind as u64)),
                    (1, CanonicalValue::Bytes(record.cid.to_vec())),
                    (2, CanonicalValue::Unsigned(record.status as u64)),
                ])
            })
            .collect();
        let retries = self
            .retries
            .iter()
            .map(|((kind, cid), count)| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(*kind)),
                    (1, CanonicalValue::Bytes(cid.to_vec())),
                    (2, CanonicalValue::Unsigned(*count)),
                ])
            })
            .collect();
        encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(JOURNAL_MAJOR)),
                (1, CanonicalValue::Unsigned(JOURNAL_MINOR)),
                (2, CanonicalValue::Bytes(self.binding.to_vec())),
                (
                    3,
                    CanonicalValue::Map(vec![
                        (
                            0,
                            CanonicalValue::Unsigned(self.config.max_retries_per_record),
                        ),
                        (1, CanonicalValue::Unsigned(self.config.max_inflight_bytes)),
                    ]),
                ),
                (4, CanonicalValue::Unsigned(self.next_sequence)),
                (5, CanonicalValue::Array(manifests)),
                (6, CanonicalValue::Array(accepted)),
                (7, CanonicalValue::Array(retries)),
                (8, CanonicalValue::Unsigned(self.inflight_bytes)),
            ]),
            ResourceProfile::ManifestV1,
        )
        .map_err(Into::into)
    }

    fn decode(bytes: &[u8]) -> Result<Self, ReconciliationJournalError> {
        let value = decode_canonical(bytes, ResourceProfile::ManifestV1)?;
        let root = map(&value, "journal")?;
        if unsigned(root, 0, "journal.major")? != JOURNAL_MAJOR {
            return Err(ReconciliationJournalError::UnsupportedVersion);
        }
        let binding = bytes32(root, 2, "journal.binding")?;
        let config_map = map(required(root, 3, "journal.config")?, "journal.config")?;
        let config = ReconciliationJournalConfig {
            max_retries_per_record: unsigned(config_map, 0, "journal.max_retries")?,
            max_inflight_bytes: unsigned(config_map, 1, "journal.max_inflight")?,
        }
        .validate()?;
        let manifest_values = array(root, 5, "journal.manifests")?;
        let accepted_values = array(root, 6, "journal.accepted")?;
        let retry_values = array(root, 7, "journal.retries")?;
        if manifest_values.len() > MAX_MANIFEST_BATCHES
            || accepted_values.len() > MAX_JOURNAL_RECORDS
            || retry_values.len() > MAX_JOURNAL_RECORDS
        {
            return Err(ReconciliationJournalError::Limit);
        }
        let mut projection = Self::new(binding, config);
        projection.next_sequence = unsigned(root, 4, "journal.next_sequence")?;
        projection.inflight_bytes = unsigned(root, 8, "journal.inflight")?;
        if projection.inflight_bytes > config.max_inflight_bytes {
            return Err(ReconciliationJournalError::InvalidField("journal.inflight"));
        }
        for value in manifest_values {
            let entry = map(value, "journal.manifest")?;
            let digest = bytes32(entry, 0, "journal.manifest.digest")?;
            let bytes = byte_string(entry, 1, "journal.manifest.bytes")?.to_vec();
            if ReservedDomain::Manifest.digest(&bytes) != digest
                || projection.manifests.insert(digest, bytes).is_some()
            {
                return Err(ReconciliationJournalError::InvalidManifestRecord);
            }
        }
        for value in accepted_values {
            let entry = map(value, "journal.accepted")?;
            let kind = parse_kind(unsigned(entry, 0, "journal.accepted.kind")?)?;
            let cid = bytes32(entry, 1, "journal.accepted.cid")?;
            let status = parse_status(unsigned(entry, 2, "journal.accepted.status")?)?;
            let record = AcceptedRecord { kind, cid, status };
            if projection
                .accepted
                .insert((kind as u64, cid), record)
                .is_some()
            {
                return Err(ReconciliationJournalError::DuplicateRecord);
            }
        }
        for value in retry_values {
            let entry = map(value, "journal.retry")?;
            let kind = unsigned(entry, 0, "journal.retry.kind")?;
            let _ = parse_kind(kind)?;
            let cid = bytes32(entry, 1, "journal.retry.cid")?;
            let count = unsigned(entry, 2, "journal.retry.count")?;
            if count == 0
                || count > config.max_retries_per_record
                || projection.retries.insert((kind, cid), count).is_some()
            {
                return Err(ReconciliationJournalError::InvalidRetryRecord);
            }
        }
        if projection.encode()? != bytes {
            return Err(ReconciliationJournalError::NonCanonicalJournal);
        }
        Ok(projection)
    }

    fn checkpoint_digest(&self) -> Result<[u8; 32], ReconciliationJournalError> {
        Ok(ReservedDomain::Manifest.digest(&self.encode()?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournaledPayloadOutcome {
    Delivered(PayloadIngestOutcome),
    Backpressured,
    RetryExhausted,
}

pub struct JournaledReconciliationSession<B, S> {
    backend: B,
    projection: JournalProjection,
    receiver: ReconciliationReceiver<S>,
    context: ReconciliationContext,
}

impl<B: ReconciliationJournalBackend, S: ValidateThenAcceptSink>
    JournaledReconciliationSession<B, S>
{
    pub fn open(
        backend: B,
        context: ReconciliationContext,
        config: ReconciliationJournalConfig,
        sink: S,
    ) -> Result<Self, ReconciliationJournalError> {
        let config = config.validate()?;
        let binding = reconciliation_binding_digest(&context)?;
        let projection = match backend
            .load(&binding)
            .map_err(ReconciliationJournalError::Backend)?
        {
            Some(bytes) => {
                let stored = JournalProjection::decode(&bytes)?;
                if stored.binding != binding || stored.config != config {
                    return Err(ReconciliationJournalError::ContextOrConfigMismatch);
                }
                stored
            }
            None => JournalProjection::new(binding, config),
        };
        let mut receiver = ReconciliationReceiver::new(context.clone(), sink)?;
        for bytes in projection.manifests.values() {
            let message = decode_reconciliation_message(bytes)?;
            validate_reconciliation_context(&context, &message)?;
            receiver.ingest_manifest(&message)?;
        }
        for record in projection.accepted.values() {
            receiver.restore_accepted(record.kind, record.cid, record.status);
        }
        let mut session = Self {
            backend,
            projection,
            receiver,
            context,
        };
        // A crash can leave a durable reservation. No payload side effect is
        // inferred from it; reopening releases it before more work.
        if session.projection.inflight_bytes != 0 {
            let mut recovered = session.projection.clone();
            recovered.inflight_bytes = 0;
            session.persist(recovered)?;
        }
        Ok(session)
    }

    pub fn ingest_manifest(
        &mut self,
        message: &ReconciliationMessage,
    ) -> Result<ManifestIngestOutcome, ReconciliationJournalError> {
        validate_reconciliation_context(&self.context, message)?;
        if !matches!(message.body, ReconciliationBody::Manifest { .. }) {
            return Err(ReconciliationJournalError::ExpectedManifest);
        }
        let bytes = encode_reconciliation_message(message)?;
        let digest = ReservedDomain::Manifest.digest(&bytes);
        let mut next = self.projection.clone();
        if !next.manifests.contains_key(&digest) {
            if next.manifests.len() >= MAX_MANIFEST_BATCHES {
                return Err(ReconciliationJournalError::Limit);
            }
            next.manifests.insert(digest, bytes);
            self.persist(next)?;
        }
        self.receiver.ingest_manifest(message).map_err(Into::into)
    }

    pub fn ingest_payload(
        &mut self,
        frame: &BoundPayloadFrame,
    ) -> Result<JournaledPayloadOutcome, ReconciliationJournalError> {
        let size = frame.canonical_bytes.len() as u64;
        if size > self.projection.config.max_inflight_bytes {
            return Ok(JournaledPayloadOutcome::Backpressured);
        }
        let key = (frame.kind as u64, frame.cid);
        if self.projection.retries.get(&key).copied().unwrap_or(0)
            >= self.projection.config.max_retries_per_record
        {
            return Ok(JournaledPayloadOutcome::RetryExhausted);
        }
        if self.projection.inflight_bytes != 0 {
            let mut cleared = self.projection.clone();
            cleared.inflight_bytes = 0;
            self.persist(cleared)?;
        }
        let mut reserved = self.projection.clone();
        reserved.inflight_bytes = size;
        self.persist(reserved)?;

        let outcome = self.receiver.ingest_payload(frame);
        let mut completed = self.projection.clone();
        completed.inflight_bytes = 0;
        match outcome {
            PayloadIngestOutcome::ValidatedStored | PayloadIngestOutcome::AlreadyPresent => {
                let status = match outcome {
                    PayloadIngestOutcome::ValidatedStored => {
                        ReconcileReceiptStatus::ValidatedStored
                    }
                    _ => ReconcileReceiptStatus::AlreadyPresent,
                };
                completed.accepted.insert(
                    key,
                    AcceptedRecord {
                        kind: frame.kind,
                        cid: frame.cid,
                        status,
                    },
                );
                completed.retries.remove(&key);
            }
            PayloadIngestOutcome::Rejected(_) => {
                let retry = completed.retries.entry(key).or_default();
                *retry = retry
                    .saturating_add(1)
                    .min(completed.config.max_retries_per_record);
            }
            PayloadIngestOutcome::DeferredUntilManifest => {}
        }
        self.persist(completed)?;
        Ok(JournaledPayloadOutcome::Delivered(outcome))
    }

    pub fn issue_resume_token(
        &mut self,
        next_sequence: u64,
        token_key: [u8; 32],
    ) -> Result<ReconciliationResumeToken, ReconciliationJournalError> {
        if self.context.resume_mode != ReconciliationResumeMode::BoundTokenV1 {
            return Err(ReconciliationJournalError::ResumeNotNegotiated);
        }
        let mut next = self.projection.clone();
        next.next_sequence = next_sequence;
        self.persist(next)?;
        let checkpoint = self.projection.checkpoint_digest()?;
        let opaque = token_mac(
            token_key,
            self.projection.binding,
            checkpoint,
            next_sequence,
        );
        make_resume_token(&self.context, checkpoint, next_sequence, opaque).map_err(Into::into)
    }

    pub fn validate_resume_token(
        &self,
        token: &ReconciliationResumeToken,
        token_key: [u8; 32],
    ) -> Result<(), ReconciliationJournalError> {
        let checkpoint = self.projection.checkpoint_digest()?;
        if token.binding_digest != self.projection.binding
            || token.checkpoint_digest != checkpoint
            || token.next_sequence != self.projection.next_sequence
            || token.opaque
                != token_mac(
                    token_key,
                    self.projection.binding,
                    checkpoint,
                    token.next_sequence,
                )
        {
            return Err(ReconciliationJournalError::InvalidResumeToken);
        }
        Ok(())
    }

    pub fn state(&self) -> ReceiverState {
        self.receiver.state()
    }

    pub fn accepted_cids(&self) -> Vec<[u8; 32]> {
        self.receiver.accepted_cids()
    }

    pub fn journal_checkpoint(&self) -> Result<[u8; 32], ReconciliationJournalError> {
        self.projection.checkpoint_digest()
    }

    pub fn into_sink(self) -> S {
        self.receiver.into_sink()
    }

    fn persist(&mut self, next: JournalProjection) -> Result<(), ReconciliationJournalError> {
        let bytes = next.encode()?;
        self.backend
            .store_atomically(&next.binding, &bytes)
            .map_err(ReconciliationJournalError::Backend)?;
        self.projection = next;
        Ok(())
    }
}

fn token_mac(key: [u8; 32], binding: [u8; 32], checkpoint: [u8; 32], sequence: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&key);
    hasher.update(b"onebrain:vnext:reconciliation-resume-token:1\0");
    hasher.update(&binding);
    hasher.update(&checkpoint);
    hasher.update(&sequence.to_be_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(feature = "persist")]
pub mod persistent {
    use std::path::Path;

    use redb::{Database, TableDefinition};

    use super::ReconciliationJournalBackend;

    const JOURNALS: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_reconciliation_journals");

    pub struct RedbReconciliationJournalBackend {
        db: Database,
    }

    impl RedbReconciliationJournalBackend {
        pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
            let db = Database::create(path).map_err(|error| error.to_string())?;
            let write = db.begin_write().map_err(|error| error.to_string())?;
            {
                write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(Self { db })
        }
    }

    impl ReconciliationJournalBackend for RedbReconciliationJournalBackend {
        fn load(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
            let read = self.db.begin_read().map_err(|error| error.to_string())?;
            let table = read
                .open_table(JOURNALS)
                .map_err(|error| error.to_string())?;
            Ok(table
                .get(binding.as_slice())
                .map_err(|error| error.to_string())?
                .map(|value| value.value().to_vec()))
        }

        fn store_atomically(&self, binding: &[u8; 32], bytes: &[u8]) -> Result<(), String> {
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            {
                let mut table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                table
                    .insert(binding.as_slice(), bytes)
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())
        }
    }
}

fn parse_kind(value: u64) -> Result<ReconcileManifestKind, ReconciliationJournalError> {
    match value {
        1 => Ok(ReconcileManifestKind::Object),
        2 => Ok(ReconcileManifestKind::Event),
        3 => Ok(ReconcileManifestKind::MappingKernel),
        _ => Err(ReconciliationJournalError::InvalidField("record.kind")),
    }
}

fn parse_status(value: u64) -> Result<ReconcileReceiptStatus, ReconciliationJournalError> {
    match value {
        1 => Ok(ReconcileReceiptStatus::ValidatedStored),
        2 => Ok(ReconcileReceiptStatus::AlreadyPresent),
        3 => Ok(ReconcileReceiptStatus::RejectedInvalid),
        4 => Ok(ReconcileReceiptStatus::DeferredBudget),
        _ => Err(ReconciliationJournalError::InvalidField("record.status")),
    }
}

fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ReconciliationJournalError> {
    match value {
        CanonicalValue::Map(value) => Ok(value),
        _ => Err(ReconciliationJournalError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ReconciliationJournalError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ReconciliationJournalError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ReconciliationJournalError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ReconciliationJournalError::InvalidField(field)),
    }
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ReconciliationJournalError> {
    match required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(ReconciliationJournalError::InvalidField(field)),
    }
}

fn byte_string<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], ReconciliationJournalError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(ReconciliationJournalError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ReconciliationJournalError> {
    let bytes = byte_string(map, key, field)?;
    if bytes.len() != 32 {
        return Err(ReconciliationJournalError::InvalidField(field));
    }
    let mut result = [0; 32];
    result.copy_from_slice(bytes);
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationJournalError {
    Canonical(CanonicalError),
    Protocol(onebrain_protocol::ReconciliationCodecError),
    Reconciliation(ReconciliationError),
    Backend(String),
    InvalidConfig,
    InvalidField(&'static str),
    UnsupportedVersion,
    NonCanonicalJournal,
    ContextOrConfigMismatch,
    InvalidManifestRecord,
    DuplicateRecord,
    InvalidRetryRecord,
    ExpectedManifest,
    ResumeNotNegotiated,
    InvalidResumeToken,
    Limit,
}

impl From<CanonicalError> for ReconciliationJournalError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<onebrain_protocol::ReconciliationCodecError> for ReconciliationJournalError {
    fn from(error: onebrain_protocol::ReconciliationCodecError) -> Self {
        Self::Protocol(error)
    }
}

impl From<ReconciliationError> for ReconciliationJournalError {
    fn from(error: ReconciliationError) -> Self {
        Self::Reconciliation(error)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
    use onebrain_protocol::{
        bind_reconciliation_message, ReconcileManifestEntry, ReconciliationBudget,
        ReconciliationSummaryMethod,
    };

    use super::*;
    use crate::vnext_reconciliation::{PayloadRejectReason, PayloadSinkOutcome};

    #[derive(Clone, Default)]
    struct SharedSink {
        state: Arc<Mutex<BTreeMap<(u64, [u8; 32]), Vec<u8>>>>,
        insertions: Arc<Mutex<u64>>,
        reject: bool,
    }

    impl ValidateThenAcceptSink for SharedSink {
        fn validate_then_accept(
            &mut self,
            kind: ReconcileManifestKind,
            cid: [u8; 32],
            bytes: &[u8],
        ) -> Result<crate::vnext_reconciliation::PayloadSinkOutcome, String> {
            if self.reject {
                return Ok(PayloadSinkOutcome::RejectedInvalid);
            }
            let mut state = self.state.lock().unwrap();
            let key = (kind as u64, cid);
            if state.contains_key(&key) {
                return Ok(PayloadSinkOutcome::AlreadyPresent);
            }
            state.insert(key, bytes.to_vec());
            *self.insertions.lock().unwrap() += 1;
            Ok(PayloadSinkOutcome::ValidatedStored)
        }
    }

    #[derive(Clone, Default)]
    struct SharedMemoryBackend(Arc<InMemoryReconciliationJournalBackend>);

    impl ReconciliationJournalBackend for SharedMemoryBackend {
        fn load(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
            self.0.load(binding)
        }

        fn store_atomically(&self, binding: &[u8; 32], bytes: &[u8]) -> Result<(), String> {
            self.0.store_atomically(binding, bytes)
        }
    }

    #[derive(Clone)]
    struct CrashOnceBackend {
        inner: SharedMemoryBackend,
        fail_on: u64,
        calls: Arc<Mutex<u64>>,
    }

    impl ReconciliationJournalBackend for CrashOnceBackend {
        fn load(&self, binding: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
            self.inner.load(binding)
        }

        fn store_atomically(&self, binding: &[u8; 32], bytes: &[u8]) -> Result<(), String> {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if *calls == self.fail_on {
                return Err("INJECTED_CRASH".to_owned());
            }
            self.inner.store_atomically(binding, bytes)
        }
    }

    fn context() -> ReconciliationContext {
        ReconciliationContext {
            authenticated_transcript: [1; 32],
            selector: SelectorCid::from_bytes([2; 32]),
            namespace: NamespaceCommitment::from_bytes([3; 32]),
            disclosure: DisclosureClass::Public,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 32,
                max_diff_ranges: 32,
                max_manifest_entries: 32,
                max_payload_bytes: 4096,
            },
            resume_mode: ReconciliationResumeMode::BoundTokenV1,
        }
    }

    fn config() -> ReconciliationJournalConfig {
        ReconciliationJournalConfig {
            max_retries_per_record: 2,
            max_inflight_bytes: 1024,
        }
    }

    fn frame() -> BoundPayloadFrame {
        BoundPayloadFrame::new(
            &context(),
            ReconcileManifestKind::Object,
            b"journaled-object".to_vec(),
        )
        .unwrap()
    }

    fn manifest(frame: &BoundPayloadFrame) -> ReconciliationMessage {
        bind_reconciliation_message(
            context(),
            1,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: frame.kind,
                    cid: frame.cid,
                    canonical_length: frame.canonical_bytes.len() as u64,
                }],
            },
        )
        .unwrap()
    }

    #[test]
    fn crash_at_each_journal_transition_resumes_without_loss_or_duplicate_accept() {
        for fail_on in 1..=6 {
            let durable = SharedMemoryBackend::default();
            let backend = CrashOnceBackend {
                inner: durable,
                fail_on,
                calls: Arc::new(Mutex::new(0)),
            };
            let sink = SharedSink::default();
            let frame = frame();
            let manifest = manifest(&frame);

            for _restart in 0..8 {
                let Ok(mut session) = JournaledReconciliationSession::open(
                    backend.clone(),
                    context(),
                    config(),
                    sink.clone(),
                ) else {
                    continue;
                };
                if session.ingest_manifest(&manifest).is_err() {
                    continue;
                }
                if session.ingest_payload(&frame).is_err() {
                    continue;
                }
                if session.state() == ReceiverState::ManifestBatchComplete {
                    break;
                }
            }

            let session =
                JournaledReconciliationSession::open(backend, context(), config(), sink.clone())
                    .unwrap();
            assert_eq!(
                session.state(),
                ReceiverState::ManifestBatchComplete,
                "crash transition {fail_on}"
            );
            assert_eq!(session.accepted_cids(), vec![frame.cid]);
            assert_eq!(*sink.insertions.lock().unwrap(), 1, "crash {fail_on}");
        }
    }

    #[test]
    fn continuation_token_is_bound_to_current_journal_and_key() {
        let backend = SharedMemoryBackend::default();
        let sink = SharedSink::default();
        let mut session = JournaledReconciliationSession::open(
            backend.clone(),
            context(),
            config(),
            sink.clone(),
        )
        .unwrap();
        let token = session.issue_resume_token(7, [9; 32]).unwrap();
        session.validate_resume_token(&token, [9; 32]).unwrap();
        assert_eq!(
            session.validate_resume_token(&token, [8; 32]).unwrap_err(),
            ReconciliationJournalError::InvalidResumeToken
        );

        drop(session);
        let reopened =
            JournaledReconciliationSession::open(backend, context(), config(), sink).unwrap();
        reopened.validate_resume_token(&token, [9; 32]).unwrap();
    }

    #[test]
    fn retry_and_backpressure_are_bounded_and_persisted() {
        let backend = SharedMemoryBackend::default();
        let mut rejecting = SharedSink::default();
        rejecting.reject = true;
        let frame = frame();
        let mut session = JournaledReconciliationSession::open(
            backend.clone(),
            context(),
            config(),
            rejecting.clone(),
        )
        .unwrap();
        session.ingest_manifest(&manifest(&frame)).unwrap();
        for _ in 0..2 {
            assert_eq!(
                session.ingest_payload(&frame).unwrap(),
                JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::Rejected(
                    PayloadRejectReason::SinkValidation
                ))
            );
        }
        assert_eq!(
            session.ingest_payload(&frame).unwrap(),
            JournaledPayloadOutcome::RetryExhausted
        );
        drop(session);
        let mut reopened =
            JournaledReconciliationSession::open(backend, context(), config(), rejecting).unwrap();
        assert_eq!(
            reopened.ingest_payload(&frame).unwrap(),
            JournaledPayloadOutcome::RetryExhausted
        );

        let oversized =
            BoundPayloadFrame::new(&context(), ReconcileManifestKind::Object, vec![0; 1025])
                .unwrap();
        assert_eq!(
            reopened.ingest_payload(&oversized).unwrap(),
            JournaledPayloadOutcome::Backpressured
        );
    }

    #[cfg(feature = "persist")]
    #[test]
    fn redb_journal_reopens_with_manifest_and_accepted_identity() {
        use super::persistent::RedbReconciliationJournalBackend;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reconciliation.redb");
        let sink = SharedSink::default();
        let frame = frame();
        {
            let backend = RedbReconciliationJournalBackend::open(&path).unwrap();
            let mut session =
                JournaledReconciliationSession::open(backend, context(), config(), sink.clone())
                    .unwrap();
            session.ingest_manifest(&manifest(&frame)).unwrap();
            session.ingest_payload(&frame).unwrap();
        }
        let backend = RedbReconciliationJournalBackend::open(&path).unwrap();
        let reopened =
            JournaledReconciliationSession::open(backend, context(), config(), sink.clone())
                .unwrap();
        assert_eq!(reopened.state(), ReceiverState::ManifestBatchComplete);
        assert_eq!(reopened.accepted_cids(), vec![frame.cid]);
        assert_eq!(*sink.insertions.lock().unwrap(), 1);
    }
}
