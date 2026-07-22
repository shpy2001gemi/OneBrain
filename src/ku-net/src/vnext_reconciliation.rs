//! Deterministic selector-scoped Merkle reconciliation state machine.
//!
//! This module consumes the canonical `obp/reconcile/1` contract. It keeps
//! manifest-before-payload and validate-then-accept as one-way API boundaries;
//! it never interprets a receipt as semantic adoption or authority.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{InventoryRecordKind, ReservedDomain, SelectorCid};
use onebrain_protocol::{
    bind_reconciliation_message, reconciliation_binding_digest, validate_reconciliation_context,
    InventoryDiffRange, InventoryLane, InventorySummaryNode, ReconcileManifestEntry,
    ReconcileManifestKind, ReconcileReceiptEntry, ReconcileReceiptStatus, ReconciliationBody,
    ReconciliationContext, ReconciliationMessage, ReconciliationPhase,
};

use crate::vnext_inventory_forest::{HybridInventoryForest, InventoryLeaf, InventoryRange};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadSinkOutcome {
    ValidatedStored,
    AlreadyPresent,
    RejectedInvalid,
}

/// Durable adapters implement validation and acceptance as one operation.
/// There is intentionally no separate unchecked `accept` method.
pub trait ValidateThenAcceptSink {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundPayloadFrame {
    pub binding_digest: [u8; 32],
    pub selector: SelectorCid,
    pub kind: ReconcileManifestKind,
    pub cid: [u8; 32],
    pub canonical_bytes: Vec<u8>,
}

impl BoundPayloadFrame {
    pub fn new(
        context: &ReconciliationContext,
        kind: ReconcileManifestKind,
        canonical_bytes: Vec<u8>,
    ) -> Result<Self, ReconciliationError> {
        let cid = content_digest(kind, &canonical_bytes);
        Ok(Self {
            binding_digest: reconciliation_binding_digest(context)?,
            selector: context.selector,
            kind,
            cid,
            canonical_bytes,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverState {
    AwaitingManifest,
    ReceivingPayloads { pending: u64 },
    PartialInvalid { pending: u64, rejected: u64 },
    ManifestBatchComplete,
}

impl ReceiverState {
    pub const fn is_globally_complete(self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManifestIngestOutcome {
    pub new_entries: u64,
    pub replayed_entries: u64,
    pub conflicting_lengths: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadIngestOutcome {
    ValidatedStored,
    AlreadyPresent,
    DeferredUntilManifest,
    Rejected(PayloadRejectReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadRejectReason {
    ContextBinding,
    Selector,
    UndeclaredLength,
    ContentCid,
    SinkValidation,
    SinkFailure,
}

pub struct ReconciliationReceiver<S> {
    expected_context: ReconciliationContext,
    sink: S,
    /// Multiple lengths are retained so conflicting arrival order cannot pick
    /// a winner. Full payload CID validation remains decisive.
    manifest_lengths: BTreeMap<(u64, [u8; 32]), BTreeSet<u64>>,
    accepted: BTreeSet<(u64, [u8; 32])>,
    rejected: BTreeSet<(u64, [u8; 32])>,
    receipt_status: BTreeMap<(u64, [u8; 32]), ReconcileReceiptStatus>,
}

impl<S: ValidateThenAcceptSink> ReconciliationReceiver<S> {
    pub fn new(
        expected_context: ReconciliationContext,
        sink: S,
    ) -> Result<Self, ReconciliationError> {
        let _ = reconciliation_binding_digest(&expected_context)?;
        Ok(Self {
            expected_context,
            sink,
            manifest_lengths: BTreeMap::new(),
            accepted: BTreeSet::new(),
            rejected: BTreeSet::new(),
            receipt_status: BTreeMap::new(),
        })
    }

    pub fn ingest_manifest(
        &mut self,
        message: &ReconciliationMessage,
    ) -> Result<ManifestIngestOutcome, ReconciliationError> {
        validate_reconciliation_context(&self.expected_context, message)?;
        let ReconciliationBody::Manifest { entries } = &message.body else {
            return Err(ReconciliationError::ExpectedManifest);
        };
        let mut outcome = ManifestIngestOutcome {
            new_entries: 0,
            replayed_entries: 0,
            conflicting_lengths: 0,
        };
        for entry in entries {
            let key = record_key(entry.kind, entry.cid);
            let lengths = self.manifest_lengths.entry(key).or_default();
            if lengths.contains(&entry.canonical_length) {
                outcome.replayed_entries += 1;
            } else {
                if !lengths.is_empty() {
                    outcome.conflicting_lengths += 1;
                }
                lengths.insert(entry.canonical_length);
                outcome.new_entries += 1;
            }
        }
        Ok(outcome)
    }

    pub fn ingest_payload(&mut self, frame: &BoundPayloadFrame) -> PayloadIngestOutcome {
        let expected_binding = match reconciliation_binding_digest(&self.expected_context) {
            Ok(binding) => binding,
            Err(_) => return PayloadIngestOutcome::Rejected(PayloadRejectReason::ContextBinding),
        };
        if frame.binding_digest != expected_binding {
            return PayloadIngestOutcome::Rejected(PayloadRejectReason::ContextBinding);
        }
        if frame.selector != self.expected_context.selector {
            return PayloadIngestOutcome::Rejected(PayloadRejectReason::Selector);
        }
        let key = record_key(frame.kind, frame.cid);
        let Some(lengths) = self.manifest_lengths.get(&key) else {
            return PayloadIngestOutcome::DeferredUntilManifest;
        };
        if !lengths.contains(&(frame.canonical_bytes.len() as u64)) {
            self.mark_rejected(key);
            return PayloadIngestOutcome::Rejected(PayloadRejectReason::UndeclaredLength);
        }
        if content_digest(frame.kind, &frame.canonical_bytes) != frame.cid {
            self.mark_rejected(key);
            return PayloadIngestOutcome::Rejected(PayloadRejectReason::ContentCid);
        }
        if self.accepted.contains(&key) {
            return PayloadIngestOutcome::AlreadyPresent;
        }
        match self
            .sink
            .validate_then_accept(frame.kind, frame.cid, &frame.canonical_bytes)
        {
            Ok(PayloadSinkOutcome::ValidatedStored) => {
                self.mark_accepted(key, ReconcileReceiptStatus::ValidatedStored);
                PayloadIngestOutcome::ValidatedStored
            }
            Ok(PayloadSinkOutcome::AlreadyPresent) => {
                self.mark_accepted(key, ReconcileReceiptStatus::AlreadyPresent);
                PayloadIngestOutcome::AlreadyPresent
            }
            Ok(PayloadSinkOutcome::RejectedInvalid) => {
                self.mark_rejected(key);
                PayloadIngestOutcome::Rejected(PayloadRejectReason::SinkValidation)
            }
            Err(_) => {
                self.mark_rejected(key);
                PayloadIngestOutcome::Rejected(PayloadRejectReason::SinkFailure)
            }
        }
    }

    pub fn state(&self) -> ReceiverState {
        if self.manifest_lengths.is_empty() {
            return ReceiverState::AwaitingManifest;
        }
        let pending = self
            .manifest_lengths
            .keys()
            .filter(|key| !self.accepted.contains(key))
            .count() as u64;
        if !self.rejected.is_empty() {
            return ReceiverState::PartialInvalid {
                pending,
                rejected: self.rejected.len() as u64,
            };
        }
        if pending == 0 {
            ReceiverState::ManifestBatchComplete
        } else {
            ReceiverState::ReceivingPayloads { pending }
        }
    }

    pub fn accepted_cids(&self) -> Vec<[u8; 32]> {
        self.accepted.iter().map(|(_, cid)| *cid).collect()
    }

    pub fn receipt_message(
        &self,
        sequence: u64,
    ) -> Result<Option<ReconciliationMessage>, ReconciliationError> {
        if self.receipt_status.is_empty() {
            return Ok(None);
        }
        let entries = self
            .receipt_status
            .iter()
            .map(|((kind, cid), status)| ReconcileReceiptEntry {
                kind: manifest_kind(*kind).expect("only canonical keys are inserted"),
                cid: *cid,
                status: *status,
            })
            .collect();
        Ok(Some(bind_reconciliation_message(
            self.expected_context.clone(),
            sequence,
            ReconciliationBody::Receipt { entries },
        )?))
    }

    pub fn progress_message(
        &self,
        sequence: u64,
    ) -> Result<ReconciliationMessage, ReconciliationError> {
        let (phase, pending) = match self.state() {
            ReceiverState::AwaitingManifest => (ReconciliationPhase::Diffing, None),
            ReceiverState::ReceivingPayloads { pending }
            | ReceiverState::PartialInvalid { pending, .. } => {
                (ReconciliationPhase::Receiving, Some(pending))
            }
            ReceiverState::ManifestBatchComplete => {
                (ReconciliationPhase::ManifestBatchComplete, Some(0))
            }
        };
        Ok(bind_reconciliation_message(
            self.expected_context.clone(),
            sequence,
            ReconciliationBody::Progress {
                phase,
                processed: self.accepted.len() as u64,
                pending_upper_bound: pending,
                resume_token: None,
            },
        )?)
    }

    pub fn into_sink(self) -> S {
        self.sink
    }

    #[doc(hidden)]
    pub(crate) fn restore_accepted(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        status: ReconcileReceiptStatus,
    ) {
        self.mark_accepted(record_key(kind, cid), status);
    }

    fn mark_rejected(&mut self, key: (u64, [u8; 32])) {
        if !self.accepted.contains(&key) {
            self.rejected.insert(key);
            self.receipt_status
                .insert(key, ReconcileReceiptStatus::RejectedInvalid);
        }
    }

    fn mark_accepted(&mut self, key: (u64, [u8; 32]), status: ReconcileReceiptStatus) {
        self.accepted.insert(key);
        self.rejected.remove(&key);
        self.receipt_status.insert(key, status);
    }
}

/// Produce the deterministic top-level radix summary for all authoritative
/// inventory lanes. Feed-prefix details remain explicit follow-up inventory;
/// semantic shard hints never enter this summary.
pub fn inventory_summary_message(
    context: ReconciliationContext,
    sequence: u64,
    forest: &HybridInventoryForest,
) -> Result<ReconciliationMessage, ReconciliationError> {
    ensure_selector(&context, forest)?;
    let mut nodes = Vec::with_capacity(3);
    let range = InventoryRange::new(0, [0; 32])?;
    let mut total = 0u64;
    for (lane, kind) in lane_kinds() {
        let summary = forest.range_summary(kind, range);
        total = total.saturating_add(summary.record_count);
        nodes.push(InventorySummaryNode {
            lane,
            prefix_bits: 0,
            prefix: Vec::new(),
            digest: summary.root,
            leaf_count: summary.record_count,
        });
    }
    Ok(bind_reconciliation_message(
        context,
        sequence,
        ReconciliationBody::InventorySummary {
            inventory_root: forest.root(),
            leaf_count: total,
            nodes,
        },
    )?)
}

/// Compare a canonical remote summary to the local forest. The returned ranges
/// are sorted and duplicate-free regardless of message arrival order.
pub fn deterministic_diff_ranges(
    context: &ReconciliationContext,
    local: &HybridInventoryForest,
    remote_summary: &ReconciliationMessage,
) -> Result<Vec<InventoryDiffRange>, ReconciliationError> {
    ensure_selector(context, local)?;
    validate_reconciliation_context(context, remote_summary)?;
    let ReconciliationBody::InventorySummary {
        inventory_root,
        nodes,
        ..
    } = &remote_summary.body
    else {
        return Err(ReconciliationError::ExpectedInventorySummary);
    };
    let mut ranges = Vec::new();
    for node in nodes {
        let range = inventory_range(node.prefix_bits, &node.prefix)?;
        let local_summary = local.range_summary(record_kind(node.lane), range);
        if node.digest != local_summary.root {
            ranges.push(InventoryDiffRange {
                lane: node.lane,
                prefix_bits: node.prefix_bits,
                prefix: node.prefix.clone(),
                offered_digest: node.digest,
                observed_digest: local_summary.root,
            });
        }
    }
    ranges.sort_by(|left, right| {
        (left.lane as u64, left.prefix_bits, &left.prefix).cmp(&(
            right.lane as u64,
            right.prefix_bits,
            &right.prefix,
        ))
    });
    ranges.dedup_by(|left, right| {
        left.lane == right.lane
            && left.prefix_bits == right.prefix_bits
            && left.prefix == right.prefix
    });
    if ranges.is_empty() && *inventory_root != local.root() {
        // The v1 record-lane summary intentionally excludes feed-prefix detail.
        // Never turn that unexplained hybrid-root difference into false closure.
        return Err(ReconciliationError::UnexplainedHybridRootDifference);
    }
    Ok(ranges)
}

pub fn manifest_entries_for_diff(
    forest: &HybridInventoryForest,
    ranges: &[InventoryDiffRange],
    max_entries: u64,
) -> Result<Vec<ReconcileManifestEntry>, ReconciliationError> {
    let mut entries = BTreeMap::<(u64, [u8; 32]), ReconcileManifestEntry>::new();
    for requested in ranges {
        let range = inventory_range(requested.prefix_bits, &requested.prefix)?;
        for leaf in forest.records_in_range(record_kind(requested.lane), range) {
            let entry = manifest_entry(leaf);
            entries.insert((entry.kind as u64, entry.cid), entry);
            if entries.len() as u64 > max_entries {
                return Err(ReconciliationError::BudgetExceeded);
            }
        }
    }
    Ok(entries.into_values().collect())
}

fn ensure_selector(
    context: &ReconciliationContext,
    forest: &HybridInventoryForest,
) -> Result<(), ReconciliationError> {
    if context.selector != forest.selector() {
        Err(ReconciliationError::SelectorMismatch)
    } else {
        Ok(())
    }
}

fn inventory_range(bits: u64, prefix: &[u8]) -> Result<InventoryRange, ReconciliationError> {
    if bits > 256 || prefix.len() as u64 != bits.div_ceil(8) {
        return Err(ReconciliationError::InvalidPrefix);
    }
    let mut full = [0; 32];
    full[..prefix.len()].copy_from_slice(prefix);
    Ok(InventoryRange::new(bits as u16, full)?)
}

fn manifest_entry(leaf: InventoryLeaf) -> ReconcileManifestEntry {
    ReconcileManifestEntry {
        kind: match leaf.record_kind {
            InventoryRecordKind::Object => ReconcileManifestKind::Object,
            InventoryRecordKind::Event => ReconcileManifestKind::Event,
            InventoryRecordKind::MappingKernel => ReconcileManifestKind::MappingKernel,
        },
        cid: leaf.cid,
        canonical_length: leaf.canonical_length,
    }
}

fn lane_kinds() -> [(InventoryLane, InventoryRecordKind); 3] {
    [
        (InventoryLane::Object, InventoryRecordKind::Object),
        (InventoryLane::Event, InventoryRecordKind::Event),
        (
            InventoryLane::MappingKernel,
            InventoryRecordKind::MappingKernel,
        ),
    ]
}

fn record_kind(lane: InventoryLane) -> InventoryRecordKind {
    match lane {
        InventoryLane::Object => InventoryRecordKind::Object,
        InventoryLane::Event => InventoryRecordKind::Event,
        InventoryLane::MappingKernel => InventoryRecordKind::MappingKernel,
    }
}

fn record_key(kind: ReconcileManifestKind, cid: [u8; 32]) -> (u64, [u8; 32]) {
    (kind as u64, cid)
}

fn manifest_kind(value: u64) -> Option<ReconcileManifestKind> {
    match value {
        1 => Some(ReconcileManifestKind::Object),
        2 => Some(ReconcileManifestKind::Event),
        3 => Some(ReconcileManifestKind::MappingKernel),
        _ => None,
    }
}

fn content_digest(kind: ReconcileManifestKind, bytes: &[u8]) -> [u8; 32] {
    let domain = match kind {
        ReconcileManifestKind::Object => ReservedDomain::Object,
        ReconcileManifestKind::Event => ReservedDomain::Event,
        ReconcileManifestKind::MappingKernel => ReservedDomain::MappingKernel,
    };
    domain.digest(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationError {
    Protocol(onebrain_protocol::ReconciliationCodecError),
    Inventory(crate::vnext_inventory_forest::InventoryForestError),
    ExpectedManifest,
    ExpectedInventorySummary,
    SelectorMismatch,
    InvalidPrefix,
    BudgetExceeded,
    UnexplainedHybridRootDifference,
}

impl From<onebrain_protocol::ReconciliationCodecError> for ReconciliationError {
    fn from(error: onebrain_protocol::ReconciliationCodecError) -> Self {
        Self::Protocol(error)
    }
}

impl From<crate::vnext_inventory_forest::InventoryForestError> for ReconciliationError {
    fn from(error: crate::vnext_inventory_forest::InventoryForestError) -> Self {
        Self::Inventory(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{DisclosureClass, NamespaceCommitment};
    use onebrain_protocol::{
        ReconciliationBudget, ReconciliationResumeMode, ReconciliationSummaryMethod,
    };

    use super::*;

    #[derive(Default)]
    struct TestSink {
        stored: BTreeMap<(u64, [u8; 32]), Vec<u8>>,
        calls: u64,
        reject_prefix: Option<u8>,
    }

    impl ValidateThenAcceptSink for TestSink {
        fn validate_then_accept(
            &mut self,
            kind: ReconcileManifestKind,
            cid: [u8; 32],
            canonical_bytes: &[u8],
        ) -> Result<PayloadSinkOutcome, String> {
            self.calls += 1;
            if self
                .reject_prefix
                .is_some_and(|prefix| canonical_bytes.first() == Some(&prefix))
            {
                return Ok(PayloadSinkOutcome::RejectedInvalid);
            }
            let key = record_key(kind, cid);
            if self.stored.contains_key(&key) {
                Ok(PayloadSinkOutcome::AlreadyPresent)
            } else {
                self.stored.insert(key, canonical_bytes.to_vec());
                Ok(PayloadSinkOutcome::ValidatedStored)
            }
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

    fn manifest(frames: &[BoundPayloadFrame]) -> ReconciliationMessage {
        let mut entries = frames
            .iter()
            .map(|frame| ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
        bind_reconciliation_message(context(), 1, ReconciliationBody::Manifest { entries }).unwrap()
    }

    #[test]
    fn payload_cannot_reach_sink_before_manifest_or_cid_validation() {
        let valid = frame(b"canonical-object");
        let mut receiver = ReconciliationReceiver::new(context(), TestSink::default()).unwrap();
        assert_eq!(
            receiver.ingest_payload(&valid),
            PayloadIngestOutcome::DeferredUntilManifest
        );
        assert_eq!(receiver.sink.calls, 0);

        receiver
            .ingest_manifest(&manifest(&[valid.clone()]))
            .unwrap();
        let mut corrupt = valid.clone();
        corrupt.canonical_bytes[0] ^= 1;
        assert_eq!(
            receiver.ingest_payload(&corrupt),
            PayloadIngestOutcome::Rejected(PayloadRejectReason::ContentCid)
        );
        assert_eq!(receiver.sink.calls, 0);
        assert_eq!(
            receiver.ingest_payload(&valid),
            PayloadIngestOutcome::ValidatedStored
        );
        assert_eq!(receiver.sink.calls, 1);
    }

    #[test]
    fn qa006_completion_trace_permutations_converge_under_fair_redelivery() {
        let frames = vec![frame(b"alpha"), frame(b"beta"), frame(b"gamma")];
        let manifest = manifest(&frames);

        let mut first = ReconciliationReceiver::new(context(), TestSink::default()).unwrap();
        let mut second = ReconciliationReceiver::new(context(), TestSink::default()).unwrap();

        // First schedule loses/reorders early frames; fair later rounds repeat them.
        first.ingest_payload(&frames[2]);
        first.ingest_manifest(&manifest).unwrap();
        first.ingest_payload(&frames[0]);
        for index in [2, 1, 0, 2, 1] {
            first.ingest_payload(&frames[index]);
        }

        // A different arrival order, with duplicate manifests and payloads.
        second.ingest_manifest(&manifest).unwrap();
        second.ingest_manifest(&manifest).unwrap();
        for index in [1, 1, 0, 2, 0] {
            second.ingest_payload(&frames[index]);
        }

        assert_eq!(first.state(), ReceiverState::ManifestBatchComplete);
        assert_eq!(second.state(), ReceiverState::ManifestBatchComplete);
        assert_eq!(first.accepted_cids(), second.accepted_cids());
        assert_eq!(first.into_sink().stored, second.into_sink().stored);
    }

    #[test]
    fn corrupt_branch_does_not_block_valid_branch() {
        let good = frame(b"good");
        let bad = frame(b"reject-me");
        let mut sink = TestSink::default();
        sink.reject_prefix = Some(b'r');
        let mut receiver = ReconciliationReceiver::new(context(), sink).unwrap();
        receiver
            .ingest_manifest(&manifest(&[good.clone(), bad.clone()]))
            .unwrap();

        assert_eq!(
            receiver.ingest_payload(&bad),
            PayloadIngestOutcome::Rejected(PayloadRejectReason::SinkValidation)
        );
        assert_eq!(
            receiver.ingest_payload(&good),
            PayloadIngestOutcome::ValidatedStored
        );
        assert!(receiver.accepted_cids().contains(&good.cid));
        assert_eq!(
            receiver.state(),
            ReceiverState::PartialInvalid {
                pending: 1,
                rejected: 1
            }
        );
        let receipt = receiver.receipt_message(9).unwrap().unwrap();
        assert!(!receipt.grants_authority());
        assert!(!receipt.establishes_global_completion());
    }

    #[test]
    fn merkle_summary_diff_and_manifest_are_deterministic() {
        let context = context();
        let mut source = HybridInventoryForest::new(context.selector);
        let payload = b"source-object";
        let cid = content_digest(ReconcileManifestKind::Object, payload);
        source
            .insert_record(InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid,
                canonical_length: payload.len() as u64,
            })
            .unwrap();
        let target = HybridInventoryForest::new(context.selector);
        let summary = inventory_summary_message(context.clone(), 1, &source).unwrap();
        let first = deterministic_diff_ranges(&context, &target, &summary).unwrap();
        let second = deterministic_diff_ranges(&context, &target, &summary).unwrap();
        assert_eq!(first, second);
        let entries = manifest_entries_for_diff(&source, &first, 32).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].cid, cid);
    }

    #[test]
    fn unexplained_feed_prefix_root_difference_never_becomes_false_closure() {
        use ku_core::foundation::{CheckpointCid, EventCid, FeedId};

        use crate::vnext_inventory_forest::FeedPrefixInventory;

        let context = context();
        let mut source = HybridInventoryForest::new(context.selector);
        source
            .insert_feed_prefix(FeedPrefixInventory {
                feed: FeedId::from_bytes([4; 32]),
                through_sequence: 1,
                head_event: EventCid::from_bytes([5; 32]),
                checkpoint_frontier_refs: vec![CheckpointCid::from_bytes([6; 32])],
            })
            .unwrap();
        let target = HybridInventoryForest::new(context.selector);
        let summary = inventory_summary_message(context.clone(), 1, &source).unwrap();
        assert_eq!(
            deterministic_diff_ranges(&context, &target, &summary).unwrap_err(),
            ReconciliationError::UnexplainedHybridRootDifference
        );
    }

    #[test]
    fn wrong_selector_and_context_are_rejected_without_sink_calls() {
        let valid = frame(b"object");
        let mut receiver = ReconciliationReceiver::new(context(), TestSink::default()).unwrap();
        let mut wrong = context();
        wrong.selector = SelectorCid::from_bytes([9; 32]);
        let wrong_manifest = bind_reconciliation_message(
            wrong,
            1,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: valid.kind,
                    cid: valid.cid,
                    canonical_length: valid.canonical_bytes.len() as u64,
                }],
            },
        )
        .unwrap();
        assert!(matches!(
            receiver.ingest_manifest(&wrong_manifest),
            Err(ReconciliationError::Protocol(_))
        ));
        assert_eq!(receiver.sink.calls, 0);
    }
}
