//! Crash-resumable journal around the deterministic reconciliation receiver.
//!
//! The durable journal stores manifests, accepted identities, retry counters
//! and continuation state; payload bytes remain owned by the validated sink.
//! If the process crashes after sink acceptance but before journal commit,
//! fair redelivery observes `AlreadyPresent` and repairs the journal.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use ku_core::foundation::{
    decode_canonical, dr_m5_failpoint, encode_canonical, CanonicalError, CanonicalValue,
    OperationalCompactionPermit, ReservedDomain, ResourceProfile,
};
use onebrain_protocol::{
    bind_reconciliation_message, decode_reconciliation_message, encode_reconciliation_message,
    make_peer_bound_resume_token, make_resume_token, reconciliation_binding_digest,
    reconciliation_resume_scope_digest, validate_reconciliation_context, ReconcileManifestKind,
    ReconcileReceiptStatus, ReconciliationBody, ReconciliationContext, ReconciliationMessage,
    ReconciliationResumeMode, ReconciliationResumeToken,
};

use crate::vnext_reconciliation::{
    BoundPayloadFrame, ManifestIngestOutcome, PayloadIngestOutcome, ReceiverState,
    ReconciliationError, ReconciliationReceiver, ValidateThenAcceptSink,
};

const JOURNAL_MAJOR: u64 = 1;
const JOURNAL_MINOR: u64 = 2;
const MAX_MANIFEST_BATCHES: usize = 4_096;
const MAX_JOURNAL_RECORDS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconciliationJournalConfig {
    pub max_retries_per_record: u64,
    pub max_inflight_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconciliationJournalCompactionReport {
    pub removed_completed_manifests: u64,
    pub snapshot_bytes_before: u64,
    pub snapshot_bytes_after: u64,
    pub compacted_audit_entries: u64,
    pub semantic_root: [u8; 32],
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
    fn store_compaction_atomically(
        &self,
        binding: &[u8; 32],
        bytes: &[u8],
        permit: &OperationalCompactionPermit,
    ) -> Result<(), String> {
        permit
            .run_if_current(|| self.store_atomically(binding, bytes))
            .map_err(|_| "COMPACTION_FENCED".to_owned())?
    }
    /// Replace one exact snapshot. This consumes a resume token without a
    /// check-then-write race when two fresh sessions replay it concurrently.
    fn compare_and_swap(
        &self,
        binding: &[u8; 32],
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<bool, String>;
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

    fn compare_and_swap(
        &self,
        binding: &[u8; 32],
        expected: &[u8],
        replacement: &[u8],
    ) -> Result<bool, String> {
        let mut snapshots = self
            .snapshots
            .lock()
            .map_err(|_| "RECONCILIATION_JOURNAL_LOCK_POISONED".to_owned())?;
        let Some(current) = snapshots.get(binding) else {
            return Ok(false);
        };
        if current.as_slice() != expected {
            return Ok(false);
        }
        snapshots.insert(*binding, replacement.to_vec());
        Ok(true)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AcceptedRecord {
    kind: ReconcileManifestKind,
    cid: [u8; 32],
    status: ReconcileReceiptStatus,
    canonical_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JournalProjection {
    binding: [u8; 32],
    resume_scope: [u8; 32],
    config: ReconciliationJournalConfig,
    next_sequence: u64,
    manifests: BTreeMap<[u8; 32], Vec<u8>>,
    compacted_manifests: BTreeSet<[u8; 32]>,
    accepted: BTreeMap<(u64, [u8; 32]), AcceptedRecord>,
    retries: BTreeMap<(u64, [u8; 32]), u64>,
    inflight_bytes: u64,
}

impl JournalProjection {
    fn new(binding: [u8; 32], resume_scope: [u8; 32], config: ReconciliationJournalConfig) -> Self {
        Self {
            binding,
            resume_scope,
            config,
            next_sequence: 0,
            manifests: BTreeMap::new(),
            compacted_manifests: BTreeSet::new(),
            accepted: BTreeMap::new(),
            retries: BTreeMap::new(),
            inflight_bytes: 0,
        }
    }

    fn encode(&self) -> Result<Vec<u8>, ReconciliationJournalError> {
        self.encode_version(JOURNAL_MINOR)
    }

    fn encode_version(&self, minor: u64) -> Result<Vec<u8>, ReconciliationJournalError> {
        if self.manifests.len() > MAX_MANIFEST_BATCHES
            || self.compacted_manifests.len() > MAX_MANIFEST_BATCHES
            || self.accepted.len() > MAX_JOURNAL_RECORDS
            || self.retries.len() > MAX_JOURNAL_RECORDS
        {
            return Err(ReconciliationJournalError::Limit);
        }
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
                let mut fields = vec![
                    (0, CanonicalValue::Unsigned(record.kind as u64)),
                    (1, CanonicalValue::Bytes(record.cid.to_vec())),
                    (2, CanonicalValue::Unsigned(record.status as u64)),
                ];
                if minor >= 2 {
                    fields.push((3, CanonicalValue::Unsigned(record.canonical_length)));
                }
                CanonicalValue::Map(fields)
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
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(JOURNAL_MAJOR)),
            (1, CanonicalValue::Unsigned(minor)),
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
        ];
        if minor >= 1 {
            fields.push((9, CanonicalValue::Bytes(self.resume_scope.to_vec())));
        }
        if minor >= 2 {
            fields.push((
                10,
                CanonicalValue::Array(
                    self.compacted_manifests
                        .iter()
                        .map(|digest| CanonicalValue::Bytes(digest.to_vec()))
                        .collect(),
                ),
            ));
        }
        encode_canonical(&CanonicalValue::Map(fields), ResourceProfile::ManifestV1)
            .map_err(Into::into)
    }

    fn decode(bytes: &[u8]) -> Result<Self, ReconciliationJournalError> {
        let value = decode_canonical(bytes, ResourceProfile::ManifestV1)?;
        let root = map(&value, "journal")?;
        if unsigned(root, 0, "journal.major")? != JOURNAL_MAJOR {
            return Err(ReconciliationJournalError::UnsupportedVersion);
        }
        let minor = unsigned(root, 1, "journal.minor")?;
        if minor > JOURNAL_MINOR {
            return Err(ReconciliationJournalError::UnsupportedVersion);
        }
        let binding = bytes32(root, 2, "journal.binding")?;
        let resume_scope = if minor == 0 {
            [0; 32]
        } else {
            bytes32(root, 9, "journal.resume_scope")?
        };
        let config_map = map(required(root, 3, "journal.config")?, "journal.config")?;
        let config = ReconciliationJournalConfig {
            max_retries_per_record: unsigned(config_map, 0, "journal.max_retries")?,
            max_inflight_bytes: unsigned(config_map, 1, "journal.max_inflight")?,
        }
        .validate()?;
        let manifest_values = array(root, 5, "journal.manifests")?;
        let accepted_values = array(root, 6, "journal.accepted")?;
        let retry_values = array(root, 7, "journal.retries")?;
        let compacted_values = if minor >= 2 {
            array(root, 10, "journal.compacted_manifests")?
        } else {
            &[]
        };
        if manifest_values.len() > MAX_MANIFEST_BATCHES
            || compacted_values.len() > MAX_MANIFEST_BATCHES
            || accepted_values.len() > MAX_JOURNAL_RECORDS
            || retry_values.len() > MAX_JOURNAL_RECORDS
        {
            return Err(ReconciliationJournalError::Limit);
        }
        let mut projection = Self::new(binding, resume_scope, config);
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
            let canonical_length = if minor >= 2 {
                unsigned(entry, 3, "journal.accepted.canonical_length")?
            } else {
                0
            };
            let record = AcceptedRecord {
                kind,
                cid,
                status,
                canonical_length,
            };
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
        for value in compacted_values {
            let CanonicalValue::Bytes(bytes) = value else {
                return Err(ReconciliationJournalError::InvalidField(
                    "journal.compacted_manifest",
                ));
            };
            let digest = fixed32(bytes, "journal.compacted_manifest")?;
            if projection.manifests.contains_key(&digest)
                || !projection.compacted_manifests.insert(digest)
            {
                return Err(ReconciliationJournalError::InvalidManifestRecord);
            }
        }
        if projection.encode_version(minor)? != bytes {
            return Err(ReconciliationJournalError::NonCanonicalJournal);
        }
        Ok(projection)
    }

    fn checkpoint_digest(&self) -> Result<[u8; 32], ReconciliationJournalError> {
        Ok(ReservedDomain::Manifest.digest(&self.encode()?))
    }

    fn completed_manifest_digests(&self) -> Result<Vec<[u8; 32]>, ReconciliationJournalError> {
        let mut completed = Vec::new();
        for (digest, bytes) in &self.manifests {
            let message = decode_reconciliation_message(bytes)?;
            let ReconciliationBody::Manifest { entries } = message.body else {
                return Err(ReconciliationJournalError::InvalidManifestRecord);
            };
            if entries.iter().all(|entry| {
                self.accepted
                    .get(&(entry.kind as u64, entry.cid))
                    .is_some_and(|accepted| {
                        accepted.canonical_length != 0
                            && accepted.canonical_length == entry.canonical_length
                    })
            }) {
                completed.push(*digest);
            }
        }
        Ok(completed)
    }

    fn semantic_root(&self) -> Result<[u8; 32], ReconciliationJournalError> {
        let mut pending = BTreeSet::new();
        for bytes in self.manifests.values() {
            let message = decode_reconciliation_message(bytes)?;
            let ReconciliationBody::Manifest { entries } = message.body else {
                return Err(ReconciliationJournalError::InvalidManifestRecord);
            };
            for entry in entries {
                if !self.accepted.contains_key(&(entry.kind as u64, entry.cid)) {
                    pending.insert((entry.kind as u64, entry.cid, entry.canonical_length));
                }
            }
        }

        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:reconciliation-journal-semantic:1\0");
        hasher.update(&self.binding);
        hasher.update(&self.resume_scope);
        hasher.update(&self.config.max_retries_per_record.to_be_bytes());
        hasher.update(&self.config.max_inflight_bytes.to_be_bytes());
        hasher.update(&self.next_sequence.to_be_bytes());
        hasher.update(&self.inflight_bytes.to_be_bytes());
        for record in self.accepted.values() {
            hasher.update(&(record.kind as u64).to_be_bytes());
            hasher.update(&record.cid);
            hasher.update(&(record.status as u64).to_be_bytes());
            hasher.update(&record.canonical_length.to_be_bytes());
        }
        for ((kind, cid), count) in &self.retries {
            hasher.update(&kind.to_be_bytes());
            hasher.update(cid);
            hasher.update(&count.to_be_bytes());
        }
        for (kind, cid, canonical_length) in pending {
            hasher.update(&kind.to_be_bytes());
            hasher.update(&cid);
            hasher.update(&canonical_length.to_be_bytes());
        }
        Ok(*hasher.finalize().as_bytes())
    }
}

/// Exercise private journal token/snapshot parsers from the DR-M5 fuzz harness
/// without exposing journal internals to production callers.
#[cfg(feature = "dr-m5-chaos-harness")]
pub fn fuzz_decode_journal_token_and_snapshot(bytes: &[u8]) {
    if let Ok(snapshot) = JournalProjection::decode(bytes) {
        let encoded = snapshot
            .encode()
            .expect("a decoded journal snapshot must re-encode");
        assert_eq!(encoded, bytes, "accepted journal bytes must be canonical");
    }
    if let Ok(message) = decode_reconciliation_message(bytes) {
        let token = match &message.body {
            ReconciliationBody::Progress {
                resume_token: Some(token),
                ..
            }
            | ReconciliationBody::Resume { token } => Some(token),
            _ => None,
        };
        if let Some(token) = token {
            let _candidate_mac = token_mac(
                [0xa5; 32],
                token.binding_digest,
                token.checkpoint_digest,
                token.next_sequence,
            );
            assert_eq!(
                encode_reconciliation_message(&message)
                    .expect("accepted journal token message re-encodes"),
                bytes
            );
            assert!(!message.grants_authority());
        }
    }

    let seed = *blake3::hash(bytes).as_bytes();
    let token_key = *blake3::keyed_hash(&[0xa5; 32], bytes).as_bytes();
    let mut projection = JournalProjection::new(
        seed,
        [0; 32],
        ReconciliationJournalConfig {
            max_retries_per_record: 1,
            max_inflight_bytes: 4_096,
        },
    );
    projection.next_sequence = u64::from_be_bytes(
        seed[..8]
            .try_into()
            .expect("a digest always has eight sequence bytes"),
    );
    let checkpoint = projection
        .checkpoint_digest()
        .expect("bounded synthesized journal checkpoint");
    let mut token = ReconciliationResumeToken {
        binding_digest: projection.binding,
        checkpoint_digest: checkpoint,
        next_sequence: projection.next_sequence,
        opaque: token_mac(
            token_key,
            projection.binding,
            checkpoint,
            projection.next_sequence,
        ),
    };
    validate_token_against(&projection, &token, token_key)
        .expect("synthesized journal token validates");
    match seed[8] % 4 {
        0 => token.binding_digest[0] ^= 1,
        1 => token.checkpoint_digest[0] ^= 1,
        2 => token.next_sequence ^= 1,
        3 => token.opaque[0] ^= 1,
        _ => unreachable!("modulo four"),
    }
    assert_eq!(
        validate_token_against(&projection, &token, token_key),
        Err(ReconciliationJournalError::InvalidResumeToken)
    );
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
        let resume_scope = reconciliation_resume_scope_digest(&context)?;
        let mut projection = match backend
            .load(&binding)
            .map_err(ReconciliationJournalError::Backend)?
        {
            Some(bytes) => {
                let stored = JournalProjection::decode(&bytes)?;
                if stored.binding != binding
                    || stored.config != config
                    || (stored.resume_scope != [0; 32] && stored.resume_scope != resume_scope)
                {
                    return Err(ReconciliationJournalError::ContextOrConfigMismatch);
                }
                stored
            }
            None => JournalProjection::new(binding, resume_scope, config),
        };
        // Minor-0 snapshots predate cross-session scope binding. They may be
        // upgraded only through an exact full-context open (the lookup key is
        // the recomputed context binding), never through a Resume token.
        if projection.resume_scope == [0; 32] {
            projection.resume_scope = resume_scope;
            backend
                .store_atomically(&binding, &projection.encode()?)
                .map_err(ReconciliationJournalError::Backend)?;
        }
        let receiver = restore_receiver(&context, &projection, sink)?;
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

    /// Consume a receiver-issued V2 token and rebind its durable journal to a
    /// fresh authenticated context with the exact same selector/privacy/budget
    /// scope. The backend compare-and-swap makes a token single-use even when
    /// concurrent sessions race to replay it.
    pub fn resume(
        backend: B,
        context: ReconciliationContext,
        config: ReconciliationJournalConfig,
        sink: S,
        token: &ReconciliationResumeToken,
        token_key: [u8; 32],
    ) -> Result<Self, ReconciliationJournalError> {
        if context.resume_mode != ReconciliationResumeMode::PeerBoundTokenV2 {
            return Err(ReconciliationJournalError::ResumeNotNegotiated);
        }
        let config = config.validate()?;
        let resume_scope = reconciliation_resume_scope_digest(&context)?;
        let original = backend
            .load(&token.binding_digest)
            .map_err(ReconciliationJournalError::Backend)?
            .ok_or(ReconciliationJournalError::InvalidResumeToken)?;
        let stored = JournalProjection::decode(&original)?;
        if stored.binding != token.binding_digest
            || stored.resume_scope != resume_scope
            || stored.config != config
        {
            return Err(ReconciliationJournalError::InvalidResumeToken);
        }
        validate_token_against(&stored, token, token_key)?;

        let mut consumed = stored;
        consumed.next_sequence = token
            .next_sequence
            .checked_add(1)
            .ok_or(ReconciliationJournalError::InvalidResumeToken)?;
        consumed.inflight_bytes = 0;
        let replacement = consumed.encode()?;
        if !backend
            .compare_and_swap(&consumed.binding, &original, &replacement)
            .map_err(ReconciliationJournalError::Backend)?
        {
            return Err(ReconciliationJournalError::InvalidResumeToken);
        }
        let receiver = restore_receiver(&context, &consumed, sink)?;
        Ok(Self {
            backend,
            projection: consumed,
            receiver,
            context,
        })
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
        if !next.manifests.contains_key(&digest) && !next.compacted_manifests.contains(&digest) {
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
                        canonical_length: size,
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
            PayloadIngestOutcome::DeferredUntilManifest
            | PayloadIngestOutcome::DeferredMissingDependency => {}
        }
        self.persist(completed)?;
        Ok(JournaledPayloadOutcome::Delivered(outcome))
    }

    pub fn issue_resume_token(
        &mut self,
        next_sequence: u64,
        token_key: [u8; 32],
    ) -> Result<ReconciliationResumeToken, ReconciliationJournalError> {
        if !matches!(
            self.context.resume_mode,
            ReconciliationResumeMode::BoundTokenV1 | ReconciliationResumeMode::PeerBoundTokenV2
        ) {
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
        match self.context.resume_mode {
            ReconciliationResumeMode::BoundTokenV1 => {
                make_resume_token(&self.context, checkpoint, next_sequence, opaque)
                    .map_err(Into::into)
            }
            ReconciliationResumeMode::PeerBoundTokenV2 => make_peer_bound_resume_token(
                &self.context,
                self.projection.binding,
                checkpoint,
                next_sequence,
                opaque,
            )
            .map_err(Into::into),
            ReconciliationResumeMode::Disabled => {
                Err(ReconciliationJournalError::ResumeNotNegotiated)
            }
        }
    }

    pub fn validate_resume_token(
        &self,
        token: &ReconciliationResumeToken,
        token_key: [u8; 32],
    ) -> Result<(), ReconciliationJournalError> {
        validate_token_against(&self.projection, token, token_key)
    }

    pub fn state(&self) -> ReceiverState {
        self.receiver.state()
    }

    pub fn resume_mode(&self) -> ReconciliationResumeMode {
        self.context.resume_mode
    }

    pub fn accepted_cids(&self) -> Vec<[u8; 32]> {
        self.receiver.accepted_cids()
    }

    /// Produce a transcript/context-bound cumulative receipt projection for
    /// the peer. A receipt reports local validation state only; it does not
    /// establish truth, authority, adoption, benefit, reward, or completion.
    pub fn receipt_message(
        &self,
        sequence: u64,
    ) -> Result<Option<ReconciliationMessage>, ReconciliationJournalError> {
        self.receiver.receipt_message(sequence).map_err(Into::into)
    }

    pub fn progress_message(
        &self,
        sequence: u64,
        resume_token: Option<ReconciliationResumeToken>,
    ) -> Result<ReconciliationMessage, ReconciliationJournalError> {
        self.receiver
            .progress_message_with_resume(sequence, resume_token)
            .map_err(Into::into)
    }

    pub fn journal_checkpoint(&self) -> Result<[u8; 32], ReconciliationJournalError> {
        self.projection.checkpoint_digest()
    }

    /// Replace fully completed manifest payloads with bounded audit digests.
    /// Accepted records, retries, inflight reservations and every pending or
    /// missing-dependency manifest remain durable.
    pub fn compact_completed_manifests(
        &mut self,
        permit: &OperationalCompactionPermit,
        limit: usize,
    ) -> Result<ReconciliationJournalCompactionReport, ReconciliationJournalError> {
        if limit == 0 || limit > MAX_MANIFEST_BATCHES {
            return Err(ReconciliationJournalError::Limit);
        }
        permit
            .ensure_current()
            .map_err(|_| ReconciliationJournalError::CompactionFenced)?;
        let before = self.projection.encode()?;
        let semantic_root = self.projection.semantic_root()?;
        let completed = self.projection.completed_manifest_digests()?;
        let available_audit_slots =
            MAX_MANIFEST_BATCHES.saturating_sub(self.projection.compacted_manifests.len());
        let remove = completed
            .into_iter()
            .take(limit.min(available_audit_slots))
            .collect::<Vec<_>>();

        dr_m5_failpoint::hit("TX-CMP-JRN-001", "before_begin_write");
        dr_m5_failpoint::hit("TX-CMP-JRN-001", "after_begin_write_before_mutation");
        let mut next = self.projection.clone();
        for digest in &remove {
            // The audit digest is inserted in the replacement snapshot before
            // the verbose manifest bytes are removed from that same snapshot.
            next.compacted_manifests.insert(*digest);
            next.manifests.remove(digest);
        }
        let after = next.encode()?;
        if next.semantic_root()? != semantic_root {
            return Err(ReconciliationJournalError::SemanticDrift);
        }
        dr_m5_failpoint::hit("TX-CMP-JRN-001", "after_mutation_before_commit");
        permit
            .ensure_current()
            .map_err(|_| ReconciliationJournalError::CompactionFenced)?;
        self.backend
            .store_compaction_atomically(&next.binding, &after, permit)
            .map_err(|error| {
                if error == "COMPACTION_FENCED" {
                    ReconciliationJournalError::CompactionFenced
                } else {
                    ReconciliationJournalError::Backend(error)
                }
            })?;
        self.projection = next;
        dr_m5_failpoint::hit("TX-CMP-JRN-001", "after_commit_before_next_side_effect");
        let report = ReconciliationJournalCompactionReport {
            removed_completed_manifests: remove.len() as u64,
            snapshot_bytes_before: before.len() as u64,
            snapshot_bytes_after: after.len() as u64,
            compacted_audit_entries: self.projection.compacted_manifests.len() as u64,
            semantic_root,
        };
        dr_m5_failpoint::hit("TX-CMP-JRN-001", "after_next_side_effect_before_ack");
        Ok(report)
    }

    pub fn journal_semantic_root(&self) -> Result<[u8; 32], ReconciliationJournalError> {
        self.projection.semantic_root()
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

fn restore_receiver<S: ValidateThenAcceptSink>(
    context: &ReconciliationContext,
    projection: &JournalProjection,
    sink: S,
) -> Result<ReconciliationReceiver<S>, ReconciliationJournalError> {
    let mut receiver = ReconciliationReceiver::new(context.clone(), sink)?;
    for bytes in projection.manifests.values() {
        let stored = decode_reconciliation_message(bytes)?;
        let ReconciliationBody::Manifest { entries } = stored.body else {
            return Err(ReconciliationJournalError::InvalidManifestRecord);
        };
        let rebound = bind_reconciliation_message(
            context.clone(),
            stored.sequence,
            ReconciliationBody::Manifest { entries },
        )?;
        receiver.ingest_manifest(&rebound)?;
    }
    for record in projection.accepted.values() {
        receiver.restore_accepted(
            record.kind,
            record.cid,
            record.status,
            record.canonical_length,
        );
    }
    Ok(receiver)
}

fn validate_token_against(
    projection: &JournalProjection,
    token: &ReconciliationResumeToken,
    token_key: [u8; 32],
) -> Result<(), ReconciliationJournalError> {
    let checkpoint = projection.checkpoint_digest()?;
    if token.binding_digest != projection.binding
        || token.checkpoint_digest != checkpoint
        || token.next_sequence != projection.next_sequence
        || token.opaque
            != token_mac(
                token_key,
                projection.binding,
                checkpoint,
                token.next_sequence,
            )
    {
        return Err(ReconciliationJournalError::InvalidResumeToken);
    }
    Ok(())
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
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use redb::{Database, ReadableTable, TableDefinition};

    use ku_core::foundation::{dr_m5_failpoint, OperationalCompactionPermit};

    use super::ReconciliationJournalBackend;

    const JOURNALS: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_reconciliation_journals");

    #[derive(Clone)]
    pub struct RedbReconciliationJournalBackend {
        db: Arc<Database>,
        path: Arc<PathBuf>,
    }

    impl RedbReconciliationJournalBackend {
        pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
            let path = path.as_ref().to_path_buf();
            let db = Database::create(&path).map_err(|error| error.to_string())?;
            let write = db.begin_write().map_err(|error| error.to_string())?;
            {
                write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(Self {
                db: Arc::new(db),
                path: Arc::new(path),
            })
        }

        pub fn disk_bytes(&self) -> Result<u64, String> {
            std::fs::metadata(self.path.as_ref())
                .map(|metadata| metadata.len())
                .map_err(|error| error.to_string())
        }

        pub fn reclaim_disk(
            &mut self,
            permit: &OperationalCompactionPermit,
        ) -> Result<bool, String> {
            permit
                .ensure_current()
                .map_err(|_| "COMPACTION_FENCED".to_owned())?;
            let database =
                Arc::get_mut(&mut self.db).ok_or_else(|| "COMPACTION_DATABASE_BUSY".to_owned())?;
            let mut reclaimed = false;
            for _ in 0..64 {
                if !permit
                    .run_if_current(|| database.compact())
                    .map_err(|_| "COMPACTION_FENCED".to_owned())?
                    .map_err(|error| error.to_string())?
                {
                    break;
                }
                reclaimed = true;
            }
            Ok(reclaimed)
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
            dr_m5_failpoint::hit("TX-JRN-001", "before_begin_write");
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            dr_m5_failpoint::hit("TX-JRN-001", "after_begin_write_before_mutation");
            {
                let mut table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                table
                    .insert(binding.as_slice(), bytes)
                    .map_err(|error| error.to_string())?;
            }
            dr_m5_failpoint::hit("TX-JRN-001", "after_mutation_before_commit");
            write.commit().map_err(|error| error.to_string())?;
            dr_m5_failpoint::hit("TX-JRN-001", "after_commit_before_next_side_effect");
            dr_m5_failpoint::hit("TX-JRN-001", "after_next_side_effect_before_ack");
            Ok(())
        }

        fn store_compaction_atomically(
            &self,
            binding: &[u8; 32],
            bytes: &[u8],
            permit: &OperationalCompactionPermit,
        ) -> Result<(), String> {
            permit
                .ensure_current()
                .map_err(|_| "COMPACTION_FENCED".to_owned())?;
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            {
                let mut table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                table
                    .insert(binding.as_slice(), bytes)
                    .map_err(|error| error.to_string())?;
            }
            permit
                .run_if_current(|| write.commit())
                .map_err(|_| "COMPACTION_FENCED".to_owned())?
                .map_err(|error| error.to_string())
        }

        fn compare_and_swap(
            &self,
            binding: &[u8; 32],
            expected: &[u8],
            replacement: &[u8],
        ) -> Result<bool, String> {
            let write = self.db.begin_write().map_err(|error| error.to_string())?;
            let swapped;
            {
                let mut table = write
                    .open_table(JOURNALS)
                    .map_err(|error| error.to_string())?;
                let matches = table
                    .get(binding.as_slice())
                    .map_err(|error| error.to_string())?
                    .is_some_and(|value| value.value() == expected);
                if matches {
                    table
                        .insert(binding.as_slice(), replacement)
                        .map_err(|error| error.to_string())?;
                }
                swapped = matches;
            }
            write.commit().map_err(|error| error.to_string())?;
            Ok(swapped)
        }
    }
}

fn parse_kind(value: u64) -> Result<ReconcileManifestKind, ReconciliationJournalError> {
    match value {
        1 => Ok(ReconcileManifestKind::Object),
        2 => Ok(ReconcileManifestKind::Event),
        3 => Ok(ReconcileManifestKind::MappingKernel),
        4 => Ok(ReconcileManifestKind::FeedInception),
        5 => Ok(ReconcileManifestKind::AuthorityEvent),
        _ => Err(ReconciliationJournalError::InvalidField("record.kind")),
    }
}

fn parse_status(value: u64) -> Result<ReconcileReceiptStatus, ReconciliationJournalError> {
    match value {
        1 => Ok(ReconcileReceiptStatus::ValidatedStored),
        2 => Ok(ReconcileReceiptStatus::AlreadyPresent),
        3 => Ok(ReconcileReceiptStatus::RejectedInvalid),
        4 => Ok(ReconcileReceiptStatus::DeferredBudget),
        5 => Ok(ReconcileReceiptStatus::DeferredMissingDependency),
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
    fixed32(byte_string(map, key, field)?, field)
}

fn fixed32(bytes: &[u8], field: &'static str) -> Result<[u8; 32], ReconciliationJournalError> {
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
    CompactionFenced,
    SemanticDrift,
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
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    use std::fs;
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    use std::path::Path;
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    use std::thread;
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    use std::time::{Duration, Instant};

    use ku_core::foundation::{
        DisclosureClass, NamespaceCommitment, OperationalCompactionSwitch, SelectorCid,
    };
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
        defer_missing_dependency: Arc<AtomicBool>,
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
            if self.defer_missing_dependency.load(Ordering::SeqCst) {
                return Ok(PayloadSinkOutcome::DeferredMissingDependency);
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

        fn compare_and_swap(
            &self,
            binding: &[u8; 32],
            expected: &[u8],
            replacement: &[u8],
        ) -> Result<bool, String> {
            self.0.compare_and_swap(binding, expected, replacement)
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

        fn compare_and_swap(
            &self,
            binding: &[u8; 32],
            expected: &[u8],
            replacement: &[u8],
        ) -> Result<bool, String> {
            self.inner.compare_and_swap(binding, expected, replacement)
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

    fn peer_bound_context(transcript: [u8; 32]) -> ReconciliationContext {
        let mut context = context();
        context.authenticated_transcript = transcript;
        context.resume_mode = ReconciliationResumeMode::PeerBoundTokenV2;
        context
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
        manifest_for(&context(), frame)
    }

    fn manifest_for(
        context: &ReconciliationContext,
        frame: &BoundPayloadFrame,
    ) -> ReconciliationMessage {
        bind_reconciliation_message(
            context.clone(),
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
    fn minor_zero_snapshot_upgrades_only_through_exact_context_open() {
        let backend = SharedMemoryBackend::default();
        let binding = reconciliation_binding_digest(&context()).unwrap();
        let legacy = JournalProjection::new(binding, [0; 32], config())
            .encode_version(0)
            .unwrap();
        backend.store_atomically(&binding, &legacy).unwrap();

        JournaledReconciliationSession::open(
            backend.clone(),
            context(),
            config(),
            SharedSink::default(),
        )
        .unwrap();
        let upgraded = backend.load(&binding).unwrap().unwrap();
        let projection = JournalProjection::decode(&upgraded).unwrap();
        assert_eq!(
            projection.resume_scope,
            reconciliation_resume_scope_digest(&context()).unwrap()
        );
        assert_ne!(legacy, upgraded);
    }

    #[test]
    fn peer_bound_token_rebinds_a_journal_once_to_a_fresh_transcript() {
        let backend = SharedMemoryBackend::default();
        let sink = SharedSink::default();
        let origin_context = peer_bound_context([0x31; 32]);
        let origin_frame = BoundPayloadFrame::new(
            &origin_context,
            ReconcileManifestKind::Object,
            b"cross-session-object".to_vec(),
        )
        .unwrap();
        let token_key = [0x41; 32];
        let token = {
            let mut origin = JournaledReconciliationSession::open(
                backend.clone(),
                origin_context,
                config(),
                sink.clone(),
            )
            .unwrap();
            origin
                .ingest_manifest(&manifest_for(&origin.context, &origin_frame))
                .unwrap();
            assert_eq!(
                origin.ingest_payload(&origin_frame).unwrap(),
                JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::ValidatedStored)
            );
            origin.issue_resume_token(7, token_key).unwrap()
        };

        let fresh_context = peer_bound_context([0x32; 32]);
        let fresh_frame = BoundPayloadFrame::new(
            &fresh_context,
            ReconcileManifestKind::Object,
            b"cross-session-object".to_vec(),
        )
        .unwrap();
        let mut resumed = JournaledReconciliationSession::resume(
            backend.clone(),
            fresh_context,
            config(),
            sink.clone(),
            &token,
            token_key,
        )
        .unwrap();
        assert_eq!(resumed.state(), ReceiverState::ManifestBatchComplete);
        assert_eq!(
            resumed.ingest_payload(&fresh_frame).unwrap(),
            JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::AlreadyPresent)
        );
        assert_eq!(*sink.insertions.lock().unwrap(), 1);

        assert!(matches!(
            JournaledReconciliationSession::resume(
                backend.clone(),
                peer_bound_context([0x33; 32]),
                config(),
                sink.clone(),
                &token,
                token_key,
            ),
            Err(ReconciliationJournalError::InvalidResumeToken)
        ));

        let next = resumed.issue_resume_token(12, token_key).unwrap();
        drop(resumed);
        assert!(matches!(
            JournaledReconciliationSession::resume(
                backend.clone(),
                peer_bound_context([0x34; 32]),
                config(),
                sink.clone(),
                &next,
                [0x42; 32],
            ),
            Err(ReconciliationJournalError::InvalidResumeToken)
        ));
        let mut wrong_scope = peer_bound_context([0x35; 32]);
        wrong_scope.selector = SelectorCid::from_bytes([0x99; 32]);
        assert!(matches!(
            JournaledReconciliationSession::resume(
                backend.clone(),
                wrong_scope,
                config(),
                sink.clone(),
                &next,
                token_key,
            ),
            Err(ReconciliationJournalError::InvalidResumeToken)
        ));
        JournaledReconciliationSession::resume(
            backend,
            peer_bound_context([0x36; 32]),
            config(),
            sink,
            &next,
            token_key,
        )
        .unwrap();
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

    #[test]
    fn missing_dependency_is_non_terminal_and_does_not_consume_retry_budget() {
        let backend = SharedMemoryBackend::default();
        let sink = SharedSink::default();
        sink.defer_missing_dependency.store(true, Ordering::SeqCst);
        let frame = frame();
        let mut session = JournaledReconciliationSession::open(
            backend.clone(),
            context(),
            config(),
            sink.clone(),
        )
        .unwrap();
        session.ingest_manifest(&manifest(&frame)).unwrap();

        for _ in 0..5 {
            assert_eq!(
                session.ingest_payload(&frame).unwrap(),
                JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::DeferredMissingDependency)
            );
        }
        assert_eq!(
            session.state(),
            ReceiverState::ReceivingPayloads { pending: 1 }
        );
        drop(session);

        sink.defer_missing_dependency.store(false, Ordering::SeqCst);
        let mut reopened =
            JournaledReconciliationSession::open(backend, context(), config(), sink.clone())
                .unwrap();
        assert_eq!(
            reopened.ingest_payload(&frame).unwrap(),
            JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::ValidatedStored)
        );
        assert_eq!(reopened.state(), ReceiverState::ManifestBatchComplete);
        assert_eq!(*sink.insertions.lock().unwrap(), 1);
    }

    #[test]
    fn compaction_removes_only_completed_manifests_and_restores_exact_semantics() {
        let backend = SharedMemoryBackend::default();
        let sink = SharedSink::default();
        let completed = frame();
        let pending = BoundPayloadFrame::new(
            &context(),
            ReconcileManifestKind::Object,
            b"pending-missing-dependency".to_vec(),
        )
        .unwrap();
        let mut session = JournaledReconciliationSession::open(
            backend.clone(),
            context(),
            config(),
            sink.clone(),
        )
        .unwrap();
        session.ingest_manifest(&manifest(&completed)).unwrap();
        session.ingest_manifest(&manifest(&pending)).unwrap();
        session.ingest_payload(&completed).unwrap();
        assert_eq!(
            session.state(),
            ReceiverState::ReceivingPayloads { pending: 1 }
        );
        let semantic_root = session.journal_semantic_root().unwrap();
        let receipt = session.receipt_message(11).unwrap();

        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        let report = session
            .compact_completed_manifests(&permit, MAX_MANIFEST_BATCHES)
            .unwrap();
        assert_eq!(report.removed_completed_manifests, 1);
        assert_eq!(report.compacted_audit_entries, 1);
        assert!(report.snapshot_bytes_after < report.snapshot_bytes_before);
        assert_eq!(report.semantic_root, semantic_root);
        assert_eq!(session.journal_semantic_root().unwrap(), semantic_root);
        assert_eq!(session.receipt_message(11).unwrap(), receipt);
        drop(session);

        let mut reopened =
            JournaledReconciliationSession::open(backend, context(), config(), sink.clone())
                .unwrap();
        assert_eq!(
            reopened.state(),
            ReceiverState::ReceivingPayloads { pending: 1 }
        );
        assert_eq!(reopened.accepted_cids(), vec![completed.cid]);
        assert_eq!(reopened.journal_semantic_root().unwrap(), semantic_root);
        assert_eq!(reopened.receipt_message(11).unwrap(), receipt);
        let compacted_checkpoint = reopened.journal_checkpoint().unwrap();
        reopened.ingest_manifest(&manifest(&completed)).unwrap();
        assert_eq!(reopened.journal_checkpoint().unwrap(), compacted_checkpoint);
        assert_eq!(
            reopened.ingest_payload(&pending).unwrap(),
            JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::ValidatedStored)
        );
        assert_eq!(reopened.state(), ReceiverState::ManifestBatchComplete);
    }

    #[test]
    fn stale_compaction_permit_does_not_mutate_journal() {
        let backend = SharedMemoryBackend::default();
        let sink = SharedSink::default();
        let frame = frame();
        let mut session =
            JournaledReconciliationSession::open(backend, context(), config(), sink).unwrap();
        session.ingest_manifest(&manifest(&frame)).unwrap();
        session.ingest_payload(&frame).unwrap();
        let before = session.journal_checkpoint().unwrap();

        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        switch.disable();
        assert_eq!(
            session.compact_completed_manifests(&permit, 1),
            Err(ReconciliationJournalError::CompactionFenced)
        );
        assert_eq!(session.journal_checkpoint().unwrap(), before);
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

    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    const COMPACTION_CHILD_ENV: &str = "ONEBRAIN_M5_05_JOURNAL_CHILD";
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    const COMPACTION_DATABASE_ENV: &str = "ONEBRAIN_M5_05_JOURNAL_DATABASE";
    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    const COMPACTION_CHILD_TEST: &str =
        "vnext_reconciliation_journal::tests::m5_05_journal_compaction_worker";

    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    #[test]
    fn m5_05_journal_compaction_worker() {
        if std::env::var_os(COMPACTION_CHILD_ENV).is_none() {
            return;
        }
        let database = std::env::var_os(COMPACTION_DATABASE_ENV).unwrap();
        compact_redb_journal(Path::new(&database));
    }

    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    #[test]
    fn m5_05_journal_process_kill_matrix_restores_exact_root() {
        let expected_directory = tempfile::tempdir().unwrap();
        let expected_path = expected_directory.path().join("expected.redb");
        initialize_redb_journal(&expected_path);
        let expected = compact_redb_journal(&expected_path);

        for phase in dr_m5_failpoint::FAILPOINT_PHASES {
            let directory = tempfile::tempdir().unwrap();
            let database = directory.path().join("journal.redb");
            let marker = directory.path().join("armed.json");
            initialize_redb_journal(&database);
            let token = format!("journal-{phase}-{}", std::process::id());
            let mut child = Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(COMPACTION_CHILD_TEST)
                .arg("--nocapture")
                .env(COMPACTION_CHILD_ENV, "1")
                .env(COMPACTION_DATABASE_ENV, &database)
                .env(dr_m5_failpoint::ENABLE_ENV, "1")
                .env(
                    dr_m5_failpoint::FAILPOINT_ENV,
                    format!("TX-CMP-JRN-001:{phase}"),
                )
                .env(dr_m5_failpoint::MARKER_ENV, &marker)
                .env(dr_m5_failpoint::TOKEN_ENV, &token)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            wait_for_compaction_marker(&mut child, &marker, &token, phase);
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());

            let recovered = compact_redb_journal(&database);
            assert_eq!(recovered, expected, "journal phase {phase}");
        }
    }

    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    fn initialize_redb_journal(path: &Path) {
        use super::persistent::RedbReconciliationJournalBackend;

        let backend = RedbReconciliationJournalBackend::open(path).unwrap();
        let completed = frame();
        let pending = BoundPayloadFrame::new(
            &context(),
            ReconcileManifestKind::Object,
            b"process-kill-pending".to_vec(),
        )
        .unwrap();
        let mut session = JournaledReconciliationSession::open(
            backend,
            context(),
            config(),
            SharedSink::default(),
        )
        .unwrap();
        session.ingest_manifest(&manifest(&completed)).unwrap();
        session.ingest_manifest(&manifest(&pending)).unwrap();
        session.ingest_payload(&completed).unwrap();
    }

    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    fn compact_redb_journal(path: &Path) -> ([u8; 32], [u8; 32], ReceiverState, Vec<[u8; 32]>) {
        use super::persistent::RedbReconciliationJournalBackend;

        let backend = RedbReconciliationJournalBackend::open(path).unwrap();
        let mut session = JournaledReconciliationSession::open(
            backend,
            context(),
            config(),
            SharedSink::default(),
        )
        .unwrap();
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        session
            .compact_completed_manifests(&permit, MAX_MANIFEST_BATCHES)
            .unwrap();
        (
            session.journal_checkpoint().unwrap(),
            session.journal_semantic_root().unwrap(),
            session.state(),
            session.accepted_cids(),
        )
    }

    #[cfg(all(feature = "persist", feature = "dr-m5-crash-harness"))]
    fn wait_for_compaction_marker(
        child: &mut std::process::Child,
        marker: &Path,
        token: &str,
        phase: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if marker.is_file() {
                let body = fs::read_to_string(marker).unwrap();
                assert!(body.contains("\"boundary\":\"TX-CMP-JRN-001\""));
                assert!(body.contains(&format!("\"phase\":\"{phase}\"")));
                assert!(body.contains(&format!("\"token\":\"{token}\"")));
                return;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("journal {phase} exited before marker: {status}");
            }
            assert!(Instant::now() < deadline, "journal {phase} marker timeout");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
