//! Arrival-order-independent merge of redundant OBP bridge paths.

use std::collections::{BTreeMap, BTreeSet};

use onebrain_protocol::{
    decode_reconciliation_message, encode_reconciliation_message, validate_reconciliation_context,
    ReconciliationBody, ReconciliationContext,
};

use crate::vnext_reconciliation::{BoundPayloadFrame, ValidateThenAcceptSink};
use crate::vnext_reconciliation_journal::{
    JournaledPayloadOutcome, JournaledReconciliationSession, ReconciliationJournalBackend,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BridgePathId([u8; 32]);

impl BridgePathId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeIngestOutcome {
    NewLogicalRecord,
    DuplicateOnSamePath,
    DuplicateAcrossPath,
}

#[derive(Clone, Debug)]
struct MessageRecord {
    bytes: Vec<u8>,
    paths: BTreeSet<BridgePathId>,
}

#[derive(Clone, Debug)]
struct PayloadRecord {
    frame: BoundPayloadFrame,
    paths: BTreeSet<BridgePathId>,
}

/// Bridge identities are delivery observations only. Logical state is keyed by
/// canonical message/payload identity and never chooses a first-arrival winner.
pub struct MultiBridgeInbox {
    context: ReconciliationContext,
    messages: BTreeMap<[u8; 32], MessageRecord>,
    payloads: BTreeMap<(u64, [u8; 32], [u8; 32]), PayloadRecord>,
}

impl MultiBridgeInbox {
    pub fn new(context: ReconciliationContext) -> Self {
        Self {
            context,
            messages: BTreeMap::new(),
            payloads: BTreeMap::new(),
        }
    }

    pub fn ingest_message(
        &mut self,
        path: BridgePathId,
        bytes: &[u8],
    ) -> Result<BridgeIngestOutcome, BridgeMergeError> {
        let message = decode_reconciliation_message(bytes)?;
        validate_reconciliation_context(&self.context, &message)?;
        let canonical = encode_reconciliation_message(&message)?;
        let digest = logical_message_digest(&canonical);
        Ok(match self.messages.get_mut(&digest) {
            Some(record) if record.paths.contains(&path) => {
                BridgeIngestOutcome::DuplicateOnSamePath
            }
            Some(record) => {
                record.paths.insert(path);
                BridgeIngestOutcome::DuplicateAcrossPath
            }
            None => {
                self.messages.insert(
                    digest,
                    MessageRecord {
                        bytes: canonical,
                        paths: BTreeSet::from([path]),
                    },
                );
                BridgeIngestOutcome::NewLogicalRecord
            }
        })
    }

    pub fn ingest_payload(
        &mut self,
        path: BridgePathId,
        frame: BoundPayloadFrame,
    ) -> Result<BridgeIngestOutcome, BridgeMergeError> {
        if frame.selector != self.context.selector {
            return Err(BridgeMergeError::SelectorMismatch);
        }
        let expected = onebrain_protocol::reconciliation_binding_digest(&self.context)?;
        if frame.binding_digest != expected {
            return Err(BridgeMergeError::BindingMismatch);
        }
        let variant = payload_variant_digest(&frame);
        let key = (frame.kind as u64, frame.cid, variant);
        Ok(match self.payloads.get_mut(&key) {
            Some(record) if record.paths.contains(&path) => {
                BridgeIngestOutcome::DuplicateOnSamePath
            }
            Some(record) => {
                record.paths.insert(path);
                BridgeIngestOutcome::DuplicateAcrossPath
            }
            None => {
                self.payloads.insert(
                    key,
                    PayloadRecord {
                        frame,
                        paths: BTreeSet::from([path]),
                    },
                );
                BridgeIngestOutcome::NewLogicalRecord
            }
        })
    }

    pub fn logical_message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn logical_payload_variant_count(&self) -> usize {
        self.payloads.len()
    }

    /// Digest of logical delivery contents only. Adding redundant bridges does
    /// not change it and therefore cannot amplify authority or fidelity.
    pub fn semantic_delivery_digest(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:multi-bridge-semantic-delivery:1\0");
        for (digest, record) in &self.messages {
            hasher.update(digest);
            hasher.update(&(record.bytes.len() as u64).to_be_bytes());
        }
        for ((kind, cid, variant), record) in &self.payloads {
            hasher.update(&kind.to_be_bytes());
            hasher.update(cid);
            hasher.update(variant);
            hasher.update(&(record.frame.canonical_bytes.len() as u64).to_be_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub fn deliver<B, S>(
        &self,
        session: &mut JournaledReconciliationSession<B, S>,
    ) -> BridgeDeliveryReport
    where
        B: ReconciliationJournalBackend,
        S: ValidateThenAcceptSink,
    {
        let mut report = BridgeDeliveryReport::default();
        // Canonical manifest identities are always applied before any payload,
        // regardless of physical arrival order across bridges.
        for record in self.messages.values() {
            match decode_reconciliation_message(&record.bytes) {
                Ok(message) if matches!(message.body, ReconciliationBody::Manifest { .. }) => {
                    match session.ingest_manifest(&message) {
                        Ok(_) => report.manifests_applied += 1,
                        Err(_) => report.errors += 1,
                    }
                }
                Ok(_) => report.control_records_skipped += 1,
                Err(_) => report.errors += 1,
            }
        }
        for record in self.payloads.values() {
            match session.ingest_payload(&record.frame) {
                Ok(JournaledPayloadOutcome::Delivered(outcome)) => {
                    report.payload_outcomes.push((record.frame.cid, outcome));
                }
                Ok(JournaledPayloadOutcome::Backpressured) => report.backpressured += 1,
                Ok(JournaledPayloadOutcome::RetryExhausted) => report.retry_exhausted += 1,
                Err(_) => report.errors += 1,
            }
        }
        report.payload_outcomes.sort_by_key(|(cid, _)| *cid);
        report
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BridgeDeliveryReport {
    pub manifests_applied: u64,
    pub control_records_skipped: u64,
    pub payload_outcomes: Vec<([u8; 32], crate::vnext_reconciliation::PayloadIngestOutcome)>,
    pub backpressured: u64,
    pub retry_exhausted: u64,
    pub errors: u64,
}

fn logical_message_digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:bridge-logical-message:1\0");
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn payload_variant_digest(frame: &BoundPayloadFrame) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:bridge-payload-variant:1\0");
    hasher.update(&frame.binding_digest);
    hasher.update(frame.selector.as_bytes());
    hasher.update(&(frame.kind as u64).to_be_bytes());
    hasher.update(&frame.cid);
    hasher.update(&(frame.canonical_bytes.len() as u64).to_be_bytes());
    hasher.update(&frame.canonical_bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeMergeError {
    Protocol(onebrain_protocol::ReconciliationCodecError),
    SelectorMismatch,
    BindingMismatch,
}

impl From<onebrain_protocol::ReconciliationCodecError> for BridgeMergeError {
    fn from(error: onebrain_protocol::ReconciliationCodecError) -> Self {
        Self::Protocol(error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
    use onebrain_protocol::{
        bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
        ReconcileManifestKind, ReconciliationBudget, ReconciliationResumeMode,
        ReconciliationSummaryMethod,
    };

    use super::*;
    use crate::vnext_reconciliation::{PayloadSinkOutcome, ReceiverState};
    use crate::vnext_reconciliation_journal::{
        InMemoryReconciliationJournalBackend, ReconciliationJournalConfig,
    };

    #[derive(Clone, Default)]
    struct Sink {
        records: Arc<Mutex<BTreeMap<(u64, [u8; 32]), Vec<u8>>>>,
        insertions: Arc<Mutex<u64>>,
    }

    impl ValidateThenAcceptSink for Sink {
        fn validate_then_accept(
            &mut self,
            kind: ReconcileManifestKind,
            cid: [u8; 32],
            bytes: &[u8],
        ) -> Result<PayloadSinkOutcome, String> {
            let key = (kind as u64, cid);
            let mut records = self.records.lock().unwrap();
            if records.contains_key(&key) {
                return Ok(PayloadSinkOutcome::AlreadyPresent);
            }
            records.insert(key, bytes.to_vec());
            *self.insertions.lock().unwrap() += 1;
            Ok(PayloadSinkOutcome::ValidatedStored)
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

    fn frame(bytes: &[u8]) -> BoundPayloadFrame {
        BoundPayloadFrame::new(&context(), ReconcileManifestKind::Object, bytes.to_vec()).unwrap()
    }

    fn manifest(frames: &[BoundPayloadFrame]) -> Vec<u8> {
        let mut entries = frames
            .iter()
            .map(|frame| ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
        let message =
            bind_reconciliation_message(context(), 1, ReconciliationBody::Manifest { entries })
                .unwrap();
        encode_reconciliation_message(&message).unwrap()
    }

    fn session(
        sink: Sink,
    ) -> JournaledReconciliationSession<InMemoryReconciliationJournalBackend, Sink> {
        JournaledReconciliationSession::open(
            InMemoryReconciliationJournalBackend::default(),
            context(),
            ReconciliationJournalConfig {
                max_retries_per_record: 4,
                max_inflight_bytes: 4096,
            },
            sink,
        )
        .unwrap()
    }

    #[test]
    fn one_two_and_five_bridges_have_the_same_semantic_delivery() {
        let frames = vec![frame(b"alpha"), frame(b"beta")];
        let manifest = manifest(&frames);
        let mut expected_digest = None;
        let mut expected_cids = None;
        for bridge_count in [1u8, 2, 5] {
            let mut inbox = MultiBridgeInbox::new(context());
            for bridge in 1..=bridge_count {
                let path = BridgePathId::from_bytes([bridge; 32]);
                inbox.ingest_message(path, &manifest).unwrap();
                for frame in &frames {
                    inbox.ingest_payload(path, frame.clone()).unwrap();
                }
            }
            assert_eq!(inbox.logical_message_count(), 1);
            assert_eq!(inbox.logical_payload_variant_count(), 2);
            expected_digest.get_or_insert(inbox.semantic_delivery_digest());
            assert_eq!(expected_digest, Some(inbox.semantic_delivery_digest()));
            let sink = Sink::default();
            let mut session = session(sink.clone());
            let report = inbox.deliver(&mut session);
            assert_eq!(report.errors, 0);
            assert_eq!(session.state(), ReceiverState::ManifestBatchComplete);
            expected_cids.get_or_insert(session.accepted_cids());
            assert_eq!(expected_cids, Some(session.accepted_cids()));
            assert_eq!(*sink.insertions.lock().unwrap(), 2);
            assert!(!inbox.grants_authority());
        }
    }

    #[test]
    fn replay_one_thousand_times_does_not_duplicate_sink_or_journal_identity() {
        let frame = frame(b"replayed");
        let manifest = manifest(std::slice::from_ref(&frame));
        let mut inbox = MultiBridgeInbox::new(context());
        for replay in 0..1_000u64 {
            let path = BridgePathId::from_bytes([1 + (replay % 5) as u8; 32]);
            inbox.ingest_message(path, &manifest).unwrap();
            inbox.ingest_payload(path, frame.clone()).unwrap();
        }
        let sink = Sink::default();
        let mut session = session(sink.clone());
        let first = inbox.deliver(&mut session);
        let second = inbox.deliver(&mut session);
        assert_eq!(first.errors, 0);
        assert_eq!(second.errors, 0);
        assert_eq!(session.accepted_cids(), vec![frame.cid]);
        assert_eq!(*sink.insertions.lock().unwrap(), 1);
    }

    #[test]
    fn payload_variant_conflict_retains_both_and_arrival_order_never_wins() {
        let valid = frame(b"valid");
        let manifest = manifest(std::slice::from_ref(&valid));
        let mut corrupt = valid.clone();
        corrupt.canonical_bytes[0] ^= 1;

        let build = |reverse: bool| {
            let mut inbox = MultiBridgeInbox::new(context());
            inbox
                .ingest_message(BridgePathId::from_bytes([1; 32]), &manifest)
                .unwrap();
            let variants = if reverse {
                vec![valid.clone(), corrupt.clone()]
            } else {
                vec![corrupt.clone(), valid.clone()]
            };
            for (index, frame) in variants.into_iter().enumerate() {
                inbox
                    .ingest_payload(BridgePathId::from_bytes([index as u8 + 1; 32]), frame)
                    .unwrap();
            }
            inbox
        };
        let left = build(false);
        let right = build(true);
        assert_eq!(
            left.semantic_delivery_digest(),
            right.semantic_delivery_digest()
        );
        assert_eq!(left.logical_payload_variant_count(), 2);
        for inbox in [left, right] {
            let sink = Sink::default();
            let mut session = session(sink.clone());
            inbox.deliver(&mut session);
            assert_eq!(session.accepted_cids(), vec![valid.cid]);
            assert_eq!(*sink.insertions.lock().unwrap(), 1);
        }
    }
}
