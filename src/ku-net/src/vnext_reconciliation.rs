//! Deterministic selector-scoped Merkle reconciliation state machine.
//!
//! This module consumes the canonical `obp/reconcile/1` contract. It keeps
//! manifest-before-payload and validate-then-accept as one-way API boundaries;
//! it never interprets a receipt as semantic adoption or authority.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    CheckpointCid, EventCid, FeedId, InventoryRecordKind, ReservedDomain, SelectorCid,
};
use onebrain_protocol::{
    bind_reconciliation_message, make_frontier_cursor, reconciliation_binding_digest,
    validate_reconciliation_context, verify_frontier_cursor, FrontierContinuationCursorV1,
    InventoryDiffRange, InventoryLane, InventorySummaryNode, ReconcileManifestEntry,
    ReconcileManifestKind, ReconcileReceiptEntry, ReconcileReceiptStatus, ReconciliationBody,
    ReconciliationContext, ReconciliationMessage, ReconciliationPhase, ReconciliationResumeToken,
};

use crate::vnext_inventory_forest::{
    FeedPrefixInventory, HybridInventoryForest, InventoryLeaf, InventoryRange,
};
use crate::vnext_riblt::{
    decode_riblt_sketch, encode_riblt_coded_symbols, radix_subtree_summary_upper_bound_bytes,
    recommended_riblt_symbol_count, verify_riblt_against_hybrid_forest, RibltDecodeOutcome,
    RibltFallbackMetric, RibltFallbackReason, RibltSessionMemoryBudget, VerifiedRibltSync,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadSinkOutcome {
    ValidatedStored,
    AlreadyPresent,
    DeferredMissingDependency,
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
    DeferredMissingDependency,
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
            self.mark_accepted(key, ReconcileReceiptStatus::AlreadyPresent);
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
            Ok(PayloadSinkOutcome::DeferredMissingDependency) => {
                self.receipt_status
                    .insert(key, ReconcileReceiptStatus::DeferredMissingDependency);
                PayloadIngestOutcome::DeferredMissingDependency
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
        self.progress_message_with_resume(sequence, None)
    }

    pub fn progress_message_with_resume(
        &self,
        sequence: u64,
        resume_token: Option<ReconciliationResumeToken>,
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
                resume_token,
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
        canonical_length: u64,
    ) {
        let key = record_key(kind, cid);
        if canonical_length != 0 {
            self.manifest_lengths
                .entry(key)
                .or_default()
                .insert(canonical_length);
        }
        self.mark_accepted(key, status);
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

// ─── RIBLT-1 Fast-Path & Seamless Radix Fallback (`RIB-001` / `RIB-002`) ─────

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum FastPathDiffResolution {
    RibltCandidate {
        remote_only: Vec<(InventoryRecordKind, [u8; 32])>,
        local_only: Vec<(InventoryRecordKind, [u8; 32])>,
        target_subtree_root: [u8; 32],
    },
    FallbackToRadix {
        metric: RibltFallbackMetric,
        radix_summary_request: ReconciliationMessage,
    },
}

/// Build a canonical `ReconciliationBody::RibltSketch` message for `[range_start, range_end)`.
pub fn riblt_sketch_message(
    context: ReconciliationContext,
    sequence: u64,
    forest: &HybridInventoryForest,
    range_start: [u8; 32],
    range_end: Option<[u8; 32]>,
    estimated_diff: usize,
) -> Result<ReconciliationMessage, ReconciliationError> {
    ensure_selector(&context, forest)?;
    let binding_digest = reconciliation_binding_digest(&context)?;
    let items: Vec<(InventoryRecordKind, [u8; 32])> = forest
        .all_records()
        .into_iter()
        .filter(|leaf| {
            leaf.cid >= range_start && range_end.as_ref().is_none_or(|end| leaf.cid < *end)
        })
        .map(|leaf| (leaf.record_kind, leaf.cid))
        .collect();
    let local_range_items =
        u32::try_from(items.len()).map_err(|_| ReconciliationError::BudgetExceeded)?;
    let raw_needed = estimated_diff
        .saturating_mul(7)
        .div_ceil(4)
        .saturating_add(16);
    let radix_bound = radix_subtree_summary_upper_bound_bytes(items.len());
    if (raw_needed as u64).saturating_mul(crate::vnext_riblt::CODED_SYMBOL_WIRE_BYTES * 10_000)
        >= radix_bound.saturating_mul(crate::vnext_riblt::STOP_AT_COST_RATIO_BPS)
    {
        return Err(ReconciliationError::RibltFallback(
            RibltFallbackReason::CostRatioExceeded,
        ));
    }
    let symbol_count = recommended_riblt_symbol_count(estimated_diff, items.len());
    let coded_symbols = encode_riblt_coded_symbols(binding_digest, &items, symbol_count)
        .map_err(ReconciliationError::RibltFallback)?;
    Ok(bind_reconciliation_message(
        context,
        sequence,
        ReconciliationBody::RibltSketch {
            range_start,
            range_end,
            target_subtree_root: forest.root(),
            local_range_items,
            coded_symbols,
        },
    )?)
}

/// Process an inbound `RibltSketch` inside the active `ReconciliationContext`.
/// On any peel stall, checksum mismatch, or cost-ratio hit, emits `FallbackToRadix`
/// with a ready-to-send `RadixForest256V1` `InventorySummary` in the **same**
/// `ReconciliationContext` without losing any accepted records in `ReconciliationReceiver`.
pub fn evaluate_riblt_sketch_or_fallback(
    context: &ReconciliationContext,
    fallback_sequence: u64,
    local_forest: &HybridInventoryForest,
    remote_sketch_msg: &ReconciliationMessage,
    memory_budget: &mut RibltSessionMemoryBudget,
) -> Result<FastPathDiffResolution, ReconciliationError> {
    ensure_selector(context, local_forest)?;
    validate_reconciliation_context(context, remote_sketch_msg)?;
    let ReconciliationBody::RibltSketch {
        range_start,
        range_end,
        target_subtree_root,
        local_range_items,
        coded_symbols,
    } = &remote_sketch_msg.body
    else {
        return Err(ReconciliationError::ExpectedRibltSketch);
    };

    let local_items: Vec<(InventoryRecordKind, [u8; 32])> = local_forest
        .all_records()
        .into_iter()
        .filter(|leaf| {
            leaf.cid >= *range_start && range_end.as_ref().is_none_or(|end| leaf.cid < *end)
        })
        .map(|leaf| (leaf.record_kind, leaf.cid))
        .collect();
    let max_cardinality = local_items.len().max(*local_range_items as usize);
    let radix_bound = radix_subtree_summary_upper_bound_bytes(max_cardinality);

    match decode_riblt_sketch(
        remote_sketch_msg.binding_digest,
        *range_start,
        *range_end,
        &local_items,
        *local_range_items,
        coded_symbols,
        radix_bound,
        memory_budget,
    ) {
        RibltDecodeOutcome::CandidateDifference {
            remote_only,
            local_only,
            ..
        } => Ok(FastPathDiffResolution::RibltCandidate {
            remote_only,
            local_only,
            target_subtree_root: *target_subtree_root,
        }),
        RibltDecodeOutcome::FallbackToRadix { reason } => {
            let radix_summary_request =
                inventory_summary_message(context.clone(), fallback_sequence, local_forest)?;
            Ok(FastPathDiffResolution::FallbackToRadix {
                metric: RibltFallbackMetric::new(reason),
                radix_summary_request,
            })
        }
    }
}

/// Complete the mandatory post-fetch Merkle root gate (`NEG-NET-004`). If the simulated
/// post-fetch `HybridInventoryForest` root does not match `target_subtree_root`,
/// seamlessly transitions to `RadixForest256V1` while preserving all `receiver.accepted_cids()`.
pub fn finalize_riblt_or_fallback_to_radix<S: ValidateThenAcceptSink>(
    context: &ReconciliationContext,
    fallback_sequence: u64,
    local_forest: &HybridInventoryForest,
    receiver: &ReconciliationReceiver<S>,
    local_only: &[(InventoryRecordKind, [u8; 32])],
    validated_remote_only: &[InventoryLeaf],
    target_subtree_root: [u8; 32],
) -> Result<
    Result<VerifiedRibltSync, (RibltFallbackMetric, ReconciliationMessage)>,
    ReconciliationError,
> {
    let _accepted_preserved = receiver.accepted_cids().len();
    match verify_riblt_against_hybrid_forest(
        local_forest,
        local_only,
        validated_remote_only,
        None,
        target_subtree_root,
    ) {
        Ok(verified) => Ok(Ok(verified)),
        Err(RibltDecodeOutcome::FallbackToRadix { reason }) => {
            let radix_summary =
                inventory_summary_message(context.clone(), fallback_sequence, local_forest)?;
            Ok(Err((RibltFallbackMetric::new(reason), radix_summary)))
        }
        Err(RibltDecodeOutcome::CandidateDifference { .. }) => unreachable!(),
    }
}

// ─── Feed-Prefix & Checkpoint Frontier Exchange (`P2` §3, `NEG-NET-006`) ─────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrontierHeadConflict {
    pub feed_id: [u8; 32],
    pub covered_sequence: u64,
    pub local_head_event_cid: [u8; 32],
    pub remote_head_event_cid: [u8; 32],
    pub remote_equivocation_proof_cid: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontierReconciliationOutcome {
    pub missing_head_events: Vec<[u8; 32]>,
    pub equivocation_proofs_to_fetch: Vec<[u8; 32]>,
    pub unverified_checkpoints: Vec<[u8; 32]>,
    pub conflicting_heads: Vec<FrontierHeadConflict>,
    pub next_cursor: Option<FrontierContinuationCursorV1>,
    pub coverage_complete: bool,
}

impl FrontierReconciliationOutcome {
    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

pub fn frontier_summary_request_message(
    context: ReconciliationContext,
    sequence: u64,
    after_feed_id: Option<[u8; 32]>,
    cursor: Option<FrontierContinuationCursorV1>,
    max_entries: u16,
) -> Result<ReconciliationMessage, ReconciliationError> {
    Ok(bind_reconciliation_message(
        context,
        sequence,
        ReconciliationBody::FrontierSummaryRequest {
            after_feed_id,
            cursor,
            max_entries,
        },
    )?)
}

/// Serve a paginated `FrontierSummaryPage` from `forest`, verifying the incoming
/// MAC-bound continuation cursor if present.
pub fn frontier_summary_page_message(
    context: ReconciliationContext,
    sequence: u64,
    forest: &HybridInventoryForest,
    request: &ReconciliationMessage,
    equivocation_proofs: &BTreeMap<([u8; 32], u64), [u8; 32]>,
    cursor_key: [u8; 32],
) -> Result<ReconciliationMessage, ReconciliationError> {
    ensure_selector(&context, forest)?;
    validate_reconciliation_context(&context, request)?;
    let ReconciliationBody::FrontierSummaryRequest {
        after_feed_id,
        cursor,
        max_entries,
    } = &request.body
    else {
        return Err(ReconciliationError::ExpectedFrontierRequest);
    };

    let after_key = if let Some(c) = cursor {
        verify_frontier_cursor(&context, c, cursor_key)?;
        Some((
            c.last_feed_id,
            c.last_covered_sequence,
            c.last_head_event_cid,
        ))
    } else {
        after_feed_id.map(|feed_id| (feed_id, u64::MAX, [0xff; 32]))
    };

    let (entries, more_after) =
        forest.feed_prefix_page(after_key, *max_entries as usize, equivocation_proofs)?;
    let next_cursor = match (more_after, entries.last()) {
        (Some(_), Some(last)) => Some(make_frontier_cursor(&context, last, cursor_key)?),
        _ => None,
    };

    Ok(bind_reconciliation_message(
        context,
        sequence,
        ReconciliationBody::FrontierSummaryPage {
            inventory_root: forest.root(),
            entries,
            next_cursor,
        },
    )?)
}

/// Reconcile a remote `FrontierSummaryPage` when all 5 object/event radix lanes match
/// yet `HybridInventoryForest::root()` differs (`UnexplainedHybridRootDifference`, `P2` §3):
/// - Preserves conflicting heads at the same `(feed_id, covered_sequence)` (`NEG-NET-006`)
///   so neither overwrites the other and `FeedEquivocationProof`s can be fetched.
/// - Requires referenced `checkpoint_cid`s to be verified (`verified_checkpoints`) before
///   marking selector coverage complete (`NEG-GC-001/002`).
pub fn reconcile_frontier_page(
    context: &ReconciliationContext,
    local_forest: &mut HybridInventoryForest,
    page_message: &ReconciliationMessage,
    verified_events: &BTreeSet<[u8; 32]>,
    verified_checkpoints: &BTreeSet<[u8; 32]>,
) -> Result<FrontierReconciliationOutcome, ReconciliationError> {
    ensure_selector(context, local_forest)?;
    validate_reconciliation_context(context, page_message)?;
    let ReconciliationBody::FrontierSummaryPage {
        inventory_root: remote_inventory_root,
        entries,
        next_cursor,
    } = &page_message.body
    else {
        return Err(ReconciliationError::ExpectedFrontierPage);
    };

    let existing = local_forest.feed_prefixes();
    let mut missing_heads = BTreeSet::new();
    let mut equivocation_proofs = BTreeSet::new();
    let mut unverified_checkpoints = BTreeSet::new();
    let mut conflicts = Vec::new();

    for remote in entries {
        for local in existing.iter().filter(|p| {
            p.feed.as_bytes() == &remote.feed_id && p.through_sequence == remote.covered_sequence
        }) {
            if local.head_event.as_bytes() != &remote.head_event_cid {
                conflicts.push(FrontierHeadConflict {
                    feed_id: remote.feed_id,
                    covered_sequence: remote.covered_sequence,
                    local_head_event_cid: *local.head_event.as_bytes(),
                    remote_head_event_cid: remote.head_event_cid,
                    remote_equivocation_proof_cid: remote.equivocation_proof_cid,
                });
                if !verified_events.contains(&remote.head_event_cid) {
                    missing_heads.insert(remote.head_event_cid);
                }
                if let Some(proof_cid) = remote.equivocation_proof_cid {
                    equivocation_proofs.insert(proof_cid);
                }
            }
        }

        if let Some(cp_cid) = remote.checkpoint_cid {
            if !verified_checkpoints.contains(&cp_cid) {
                unverified_checkpoints.insert(cp_cid);
            }
        }

        let head_verified = verified_events.contains(&remote.head_event_cid);
        let checkpoint_ok = remote
            .checkpoint_cid
            .is_none_or(|cp| verified_checkpoints.contains(&cp));

        if !head_verified {
            missing_heads.insert(remote.head_event_cid);
        } else if checkpoint_ok {
            // Only insert into authoritative local feed_prefixes once head & checkpoint are verified;
            // conflicting verified heads at the same `(feed_id, covered_sequence)` coexist in BTreeMap!
            let checkpoint_frontier_refs = remote
                .checkpoint_cid
                .into_iter()
                .map(CheckpointCid::from_bytes)
                .collect();
            let _ = local_forest.insert_feed_prefix(FeedPrefixInventory {
                feed: FeedId::from_bytes(remote.feed_id),
                through_sequence: remote.covered_sequence,
                head_event: EventCid::from_bytes(remote.head_event_cid),
                checkpoint_frontier_refs,
            })?;
        }
    }

    let coverage = local_forest.assess_coverage(*remote_inventory_root, verified_checkpoints);
    let coverage_complete = next_cursor.is_none()
        && missing_heads.is_empty()
        && unverified_checkpoints.is_empty()
        && coverage.complete_within_selector();

    Ok(FrontierReconciliationOutcome {
        missing_head_events: missing_heads.into_iter().collect(),
        equivocation_proofs_to_fetch: equivocation_proofs.into_iter().collect(),
        unverified_checkpoints: unverified_checkpoints.into_iter().collect(),
        conflicting_heads: conflicts,
        next_cursor: *next_cursor,
        coverage_complete,
    })
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
            InventoryRecordKind::FeedInception => ReconcileManifestKind::FeedInception,
            InventoryRecordKind::AuthorityEvent => ReconcileManifestKind::AuthorityEvent,
        },
        cid: leaf.cid,
        canonical_length: leaf.canonical_length,
    }
}

fn lane_kinds() -> [(InventoryLane, InventoryRecordKind); 5] {
    [
        (InventoryLane::Object, InventoryRecordKind::Object),
        (InventoryLane::Event, InventoryRecordKind::Event),
        (
            InventoryLane::MappingKernel,
            InventoryRecordKind::MappingKernel,
        ),
        (
            InventoryLane::FeedInception,
            InventoryRecordKind::FeedInception,
        ),
        (
            InventoryLane::AuthorityEvent,
            InventoryRecordKind::AuthorityEvent,
        ),
    ]
}

fn record_kind(lane: InventoryLane) -> InventoryRecordKind {
    match lane {
        InventoryLane::Object => InventoryRecordKind::Object,
        InventoryLane::Event => InventoryRecordKind::Event,
        InventoryLane::MappingKernel => InventoryRecordKind::MappingKernel,
        InventoryLane::FeedInception => InventoryRecordKind::FeedInception,
        InventoryLane::AuthorityEvent => InventoryRecordKind::AuthorityEvent,
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
        4 => Some(ReconcileManifestKind::FeedInception),
        5 => Some(ReconcileManifestKind::AuthorityEvent),
        _ => None,
    }
}

fn content_digest(kind: ReconcileManifestKind, bytes: &[u8]) -> [u8; 32] {
    let domain = match kind {
        ReconcileManifestKind::Object => ReservedDomain::Object,
        ReconcileManifestKind::Event => ReservedDomain::Event,
        ReconcileManifestKind::MappingKernel => ReservedDomain::MappingKernel,
        ReconcileManifestKind::FeedInception => ReservedDomain::FeedInception,
        ReconcileManifestKind::AuthorityEvent => ReservedDomain::AuthorityEvent,
    };
    domain.digest(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconciliationError {
    Protocol(onebrain_protocol::ReconciliationCodecError),
    Inventory(crate::vnext_inventory_forest::InventoryForestError),
    RibltFallback(RibltFallbackReason),
    ExpectedManifest,
    ExpectedInventorySummary,
    ExpectedRibltSketch,
    ExpectedFrontierRequest,
    ExpectedFrontierPage,
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
            match self.stored.entry(key) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(canonical_bytes.to_vec());
                    Ok(PayloadSinkOutcome::ValidatedStored)
                }
                std::collections::btree_map::Entry::Occupied(_) => {
                    Ok(PayloadSinkOutcome::AlreadyPresent)
                }
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
            .ingest_manifest(&manifest(std::slice::from_ref(&valid)))
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
        let sink = TestSink {
            reject_prefix: Some(b'r'),
            ..TestSink::default()
        };
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

    #[test]
    fn frontier_exchange_resolves_unexplained_root_diff_preserves_conflicts_and_gates_checkpoints()
    {
        let context = context();
        let mut source = HybridInventoryForest::new(context.selector);
        // NEG-NET-006: two conflicting heads at the exact same (feed, sequence = 3)
        let feed = FeedId::from_bytes([0x41; 32]);
        let head_a = [0x51; 32];
        let head_b = [0x52; 32];
        let cp_cid = [0x61; 32];
        source
            .insert_feed_prefix(FeedPrefixInventory {
                feed,
                through_sequence: 3,
                head_event: EventCid::from_bytes(head_a),
                checkpoint_frontier_refs: vec![CheckpointCid::from_bytes(cp_cid)],
            })
            .unwrap();
        source
            .insert_feed_prefix(FeedPrefixInventory {
                feed,
                through_sequence: 3,
                head_event: EventCid::from_bytes(head_b),
                checkpoint_frontier_refs: vec![CheckpointCid::from_bytes(cp_cid)],
            })
            .unwrap();

        let mut target = HybridInventoryForest::new(context.selector);
        target
            .insert_feed_prefix(FeedPrefixInventory {
                feed,
                through_sequence: 3,
                head_event: EventCid::from_bytes(head_a),
                checkpoint_frontier_refs: vec![CheckpointCid::from_bytes(cp_cid)],
            })
            .unwrap();

        let summary = inventory_summary_message(context.clone(), 1, &source).unwrap();
        assert_eq!(
            deterministic_diff_ranges(&context, &target, &summary).unwrap_err(),
            ReconciliationError::UnexplainedHybridRootDifference
        );

        let req = frontier_summary_request_message(context.clone(), 2, None, None, 512).unwrap();
        let mut proofs = BTreeMap::new();
        proofs.insert(([0x41; 32], 3), [0x71; 32]);
        let page =
            frontier_summary_page_message(context.clone(), 3, &source, &req, &proofs, [0x99; 32])
                .unwrap();

        // Step 1: checkpoint is NOT verified yet -> coverage_complete MUST be false!
        let verified_events = BTreeSet::from([head_a, head_b]);
        let empty_checkpoints = BTreeSet::new();
        let partial = reconcile_frontier_page(
            &context,
            &mut target,
            &page,
            &verified_events,
            &empty_checkpoints,
        )
        .unwrap();
        assert!(!partial.coverage_complete);
        assert_eq!(partial.unverified_checkpoints, vec![cp_cid]);
        assert_eq!(partial.conflicting_heads.len(), 1);
        assert_eq!(partial.equivocation_proofs_to_fetch, vec![[0x71; 32]]);

        // Step 2: checkpoint verified -> both conflicting heads preserved & roots converge!
        let verified_checkpoints = BTreeSet::from([cp_cid]);
        let complete = reconcile_frontier_page(
            &context,
            &mut target,
            &page,
            &verified_events,
            &verified_checkpoints,
        )
        .unwrap();
        assert!(complete.coverage_complete);
        assert_eq!(
            target.feed_prefixes().len(),
            2,
            "both conflicting heads retained"
        );
        assert_eq!(target.root(), source.root());
    }

    #[test]
    fn riblt_fallback_to_radix_preserves_receiver_accepted_records_in_same_context() {
        let context = context();
        let pre_accepted = frame(b"already-validated-before-fallback");
        let mut receiver =
            ReconciliationReceiver::new(context.clone(), TestSink::default()).unwrap();
        receiver
            .ingest_manifest(&manifest(std::slice::from_ref(&pre_accepted)))
            .unwrap();
        assert_eq!(
            receiver.ingest_payload(&pre_accepted),
            PayloadIngestOutcome::ValidatedStored
        );

        let mut local_forest = HybridInventoryForest::new(context.selector);
        let mut remote_forest = HybridInventoryForest::new(context.selector);
        local_forest
            .insert_record(InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid: pre_accepted.cid,
                canonical_length: pre_accepted.canonical_bytes.len() as u64,
            })
            .unwrap();
        for idx in 0..64u32 {
            let shared = BoundPayloadFrame::new(
                &context,
                ReconcileManifestKind::Object,
                format!("shared-{idx}").into_bytes(),
            )
            .unwrap();
            let leaf = InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid: shared.cid,
                canonical_length: shared.canonical_bytes.len() as u64,
            };
            local_forest.insert_record(leaf).unwrap();
            remote_forest.insert_record(leaf).unwrap();
        }

        // Remote sends a poisoned RIBLT sketch in the same context
        let extra = frame(b"remote-extra-object");
        remote_forest
            .insert_record(InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid: extra.cid,
                canonical_length: extra.canonical_bytes.len() as u64,
            })
            .unwrap();
        let mut sketch =
            riblt_sketch_message(context.clone(), 4, &remote_forest, [0; 32], None, 4).unwrap();
        if let ReconciliationBody::RibltSketch { coded_symbols, .. } = &mut sketch.body {
            coded_symbols[0].checksum_xor ^= 0xdead_beef;
            coded_symbols.truncate(1);
        }
        sketch = bind_reconciliation_message(context.clone(), 4, sketch.body).unwrap();

        let mut budget = RibltSessionMemoryBudget::new();
        let res =
            evaluate_riblt_sketch_or_fallback(&context, 5, &local_forest, &sketch, &mut budget)
                .unwrap();
        let FastPathDiffResolution::FallbackToRadix {
            metric,
            radix_summary_request,
        } = res
        else {
            panic!("expected fallback to radix");
        };
        assert_eq!(metric.metric, "RibltFallbackToRadix");
        assert_eq!(radix_summary_request.context, context);
        assert_eq!(receiver.accepted_cids(), vec![pre_accepted.cid]);
    }
}
