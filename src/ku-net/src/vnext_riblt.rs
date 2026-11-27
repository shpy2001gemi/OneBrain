//! Negotiated `RIBLT-1` fast-path rateless IBLT sketch reconciliation (`RIB-001` / `RIB-002`).
//!
//! Enforces `ADR-OBP-056-04` and `NEG-NET-004`:
//! - Fixed 40-byte symbol unit `K_e = RecordKind(1B) || [0u8; 7] || CanonicalCID(32B)`
//!   and 64-bit checksum `C_e = u64::from_be_bytes(blake3("onebrain:vnext:riblt-check:1\0" || K_e)[..8])`.
//! - Deterministic rateless degree mapping via `blake3::Hasher` XOF
//!   (`"onebrain:vnext:riblt-map:1\0" || context_id || K_e`) using pure-integer arithmetic.
//! - Hard adversarial resource ceilings: `RANGE_ITEM_CAP = 65_536`,
//!   `MAX_CODED_SYMBOLS_PER_RANGE = 16_384`, `DECODER_MEMORY_CAP = 33_554_432` (`32 MiB`),
//!   `MAX_PEEL_OPERATIONS = 131_072`, `STOP_AT_COST_RATIO_BPS = 5_000` (`50%`).
//! - Zero probabilistic completion (`NEG-NET-004`): peeling yields only a
//!   `CandidateDifference` hypothesis that MUST pass `verify_riblt_against_hybrid_forest`
//!   against the advertised Merkle root or fall back cleanly to `RadixForest256V1`.

use std::collections::{BTreeSet, VecDeque};

use ku_core::foundation::{DisclosureClass, InventoryRecordKind};
use onebrain_protocol::{reconcile_riblt_capability, ReconciliationContext, RibltCodedSymbolV1};

use crate::vnext_inventory_forest::{HybridInventoryForest, InventoryLeaf, InventoryRange};
use crate::vnext_session::AuthenticatedSession;

pub const RANGE_ITEM_CAP: usize = 65_536;
pub const MAX_CODED_SYMBOLS_PER_RANGE: usize = 16_384;
pub const DECODER_MEMORY_CAP: usize = 33_554_432; // 32 MiB bounded arena
pub const MAX_PEEL_OPERATIONS: usize = 131_072;
pub const STOP_AT_COST_RATIO_BPS: u64 = 5_000; // 50% of Radix subtree summary byte bound

pub const RIBLT_CHECK_DOMAIN: &[u8] = b"onebrain:vnext:riblt-check:1\0";
pub const RIBLT_MAP_DOMAIN: &[u8] = b"onebrain:vnext:riblt-map:1\0";

/// Estimated canonical wire bytes per `RibltCodedSymbolV1` entry.
pub const CODED_SYMBOL_WIRE_BYTES: u64 = 64;
/// Estimated canonical wire bytes per `InventorySummaryNode` in `RadixForest256V1`.
pub const RADIX_SUMMARY_NODE_WIRE_BYTES: u64 = 56;

/// Default-off runtime gate (`FND-006`) and independent kill switch for `RIBLT-1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RibltRuntimeGate {
    pub reconcile_riblt_v1: bool,
    pub kill_switch_tripped: bool,
}

impl RibltRuntimeGate {
    pub const fn enabled() -> Self {
        Self {
            reconcile_riblt_v1: true,
            kill_switch_tripped: false,
        }
    }

    pub const fn is_permitted(self) -> bool {
        self.reconcile_riblt_v1 && !self.kill_switch_tripped
    }
}

/// Per-session bounded memory tracker enforcing `DECODER_MEMORY_CAP = 32 MiB`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RibltSessionMemoryBudget {
    allocated_bytes: usize,
    tripped: bool,
}

impl RibltSessionMemoryBudget {
    pub const fn new() -> Self {
        Self {
            allocated_bytes: 0,
            tripped: false,
        }
    }

    pub const fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
    }

    pub const fn is_tripped(&self) -> bool {
        self.tripped
    }

    pub fn try_reserve(&mut self, bytes: usize) -> Result<RibltArenaLease, RibltFallbackReason> {
        if self.tripped {
            return Err(RibltFallbackReason::DecoderMemoryCapExceeded);
        }
        let Some(next) = self.allocated_bytes.checked_add(bytes) else {
            self.tripped = true;
            return Err(RibltFallbackReason::DecoderMemoryCapExceeded);
        };
        if next > DECODER_MEMORY_CAP {
            self.tripped = true;
            return Err(RibltFallbackReason::DecoderMemoryCapExceeded);
        }
        self.allocated_bytes = next;
        Ok(RibltArenaLease { reserved: bytes })
    }

    pub fn release(&mut self, lease: RibltArenaLease) {
        self.allocated_bytes = self.allocated_bytes.saturating_sub(lease.reserved);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RibltArenaLease {
    reserved: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibltFallbackReason {
    FeatureDisabled,
    CapabilityNotNegotiated,
    IneligibleDisclosure,
    IdenticalSubtreeRoots,
    CardinalityOutOfBounds,
    InvalidRangeBounds,
    DecoderMemoryCapExceeded,
    MaxCodedSymbolsExceeded,
    MaxPeelOperationsExceeded,
    CostRatioExceeded,
    SingletonChecksumMismatch,
    MalformedSymbolKey,
    OutOfRangeSymbol,
    DuplicateDecodedSymbol,
    PeelStalled,
    HybridRootMismatch,
}

/// Low-cardinality fallback metric (`M5-02`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RibltFallbackMetric {
    pub metric: &'static str,
    pub reason: RibltFallbackReason,
}

impl RibltFallbackMetric {
    pub const fn new(reason: RibltFallbackReason) -> Self {
        Self {
            metric: "RibltFallbackToRadix",
            reason,
        }
    }
}

/// Peeling result (`NEG-NET-004`): `CandidateDifference` is NEVER proof of completion!
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibltDecodeOutcome {
    CandidateDifference {
        remote_only: Vec<(InventoryRecordKind, [u8; 32])>,
        local_only: Vec<(InventoryRecordKind, [u8; 32])>,
        peel_operations: usize,
    },
    FallbackToRadix {
        reason: RibltFallbackReason,
    },
}

impl RibltDecodeOutcome {
    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn establishes_completion(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedRibltSync {
    pub verified_subtree_root: [u8; 32],
    pub remote_only_count: usize,
    pub local_only_count: usize,
}

/// Encode `K_e = RecordKind(1B) || [0u8; 7] || CanonicalCID(32B)` (`RIB-001`).
pub fn encode_riblt_key(kind: InventoryRecordKind, cid: [u8; 32]) -> [u8; 40] {
    let mut key = [0u8; 40];
    key[0] = match kind {
        InventoryRecordKind::Object => 1,
        InventoryRecordKind::Event => 2,
        InventoryRecordKind::MappingKernel => 3,
        InventoryRecordKind::FeedInception => 4,
        InventoryRecordKind::AuthorityEvent => 5,
    };
    key[8..40].copy_from_slice(&cid);
    key
}

/// Decode and validate canonical `K_e = RecordKind(1B) || [0u8; 7] || CanonicalCID(32B)`.
pub fn decode_riblt_key(
    key: &[u8; 40],
) -> Result<(InventoryRecordKind, [u8; 32]), RibltFallbackReason> {
    if key[1..8] != [0u8; 7] {
        return Err(RibltFallbackReason::MalformedSymbolKey);
    }
    let kind = match key[0] {
        1 => InventoryRecordKind::Object,
        2 => InventoryRecordKind::Event,
        3 => InventoryRecordKind::MappingKernel,
        4 => InventoryRecordKind::FeedInception,
        5 => InventoryRecordKind::AuthorityEvent,
        _ => return Err(RibltFallbackReason::MalformedSymbolKey),
    };
    let mut cid = [0u8; 32];
    cid.copy_from_slice(&key[8..40]);
    Ok((kind, cid))
}

/// Compute `C_e = u64::from_be_bytes(blake3("onebrain:vnext:riblt-check:1\0" || K_e)[..8])`.
pub fn riblt_symbol_checksum(key: &[u8; 40]) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(RIBLT_CHECK_DOMAIN);
    hasher.update(key);
    let digest = hasher.finalize();
    u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes"))
}

/// Deterministic rateless coded-symbol index generator (`ADR-OBP-056-04`) using
/// `blake3::Hasher` XOF (`"onebrain:vnext:riblt-map:1\0" || context_id || K_e`)
/// and pure-integer Yang et al. Zipf-like step arithmetic (zero IEEE-754 floats).
pub struct RibltSymbolMapping {
    xof: blake3::OutputReader,
    next_index: u64,
    first: bool,
}

impl RibltSymbolMapping {
    pub fn new(context_id: &[u8; 32], key: &[u8; 40]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(RIBLT_MAP_DOMAIN);
        hasher.update(context_id);
        hasher.update(key);
        Self {
            xof: hasher.finalize_xof(),
            next_index: 0,
            first: true,
        }
    }

    pub fn next_coded_index(&mut self) -> u64 {
        if self.first {
            self.first = false;
            return 0;
        }
        let current = self.next_index;
        let mut word = [0u8; 4];
        self.xof.fill(&mut word);
        let u = u32::from_be_bytes(word) as u128;
        // Pure-integer Yang et al. rateless step:
        // next = max(current + 1, ceil((current + 1.5) / sqrt(1 - u / 2^32)) - 1.5)
        let w: u128 = (1u128 << 64) - (u << 32);
        let sqrt_w = isqrt_u128(w).max(1);
        let numer = ((2 * current + 3) as u128) << 32;
        let denom = 2 * sqrt_w;
        let ceil_term = numer.div_ceil(denom);
        let candidate = u64::try_from(ceil_term.saturating_sub(2)).unwrap_or(u64::MAX);
        let next = candidate.max(current.saturating_add(1));
        self.next_index = next;
        next
    }

    pub fn indices_below(&mut self, limit: u32) -> Vec<u32> {
        let mut out = Vec::new();
        loop {
            let idx = self.next_coded_index();
            if idx >= u64::from(limit) {
                break;
            }
            out.push(idx as u32);
        }
        out
    }
}

fn isqrt_u128(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let shift = (128 - n.leading_zeros()).div_ceil(2);
    let mut x = 1u128 << shift;
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Check all five eligibility conditions required by `P2` §2.1 before running `RIBLT-1`.
#[allow(clippy::too_many_arguments)]
pub fn check_riblt_eligibility(
    gate: RibltRuntimeGate,
    session: &AuthenticatedSession,
    context: &ReconciliationContext,
    range_start: &[u8; 32],
    range_end: Option<&[u8; 32]>,
    local_subtree_root: [u8; 32],
    remote_subtree_root: [u8; 32],
    local_range_items: usize,
    memory_budget: &RibltSessionMemoryBudget,
) -> Result<(), RibltFallbackReason> {
    if !gate.is_permitted() {
        return Err(RibltFallbackReason::FeatureDisabled);
    }
    if !session.capabilities.contains(&reconcile_riblt_capability()) {
        return Err(RibltFallbackReason::CapabilityNotNegotiated);
    }
    if !matches!(
        context.disclosure,
        DisclosureClass::Public | DisclosureClass::RouteMinimal
    ) {
        return Err(RibltFallbackReason::IneligibleDisclosure);
    }
    if let Some(end) = range_end {
        if end <= range_start {
            return Err(RibltFallbackReason::InvalidRangeBounds);
        }
    }
    if local_subtree_root == remote_subtree_root {
        return Err(RibltFallbackReason::IdenticalSubtreeRoots);
    }
    if local_range_items == 0 || local_range_items > RANGE_ITEM_CAP {
        return Err(RibltFallbackReason::CardinalityOutOfBounds);
    }
    if memory_budget.is_tripped() || memory_budget.allocated_bytes() >= DECODER_MEMORY_CAP {
        return Err(RibltFallbackReason::DecoderMemoryCapExceeded);
    }
    Ok(())
}

/// Deterministic upper bound on `RadixForest256V1` subtree summary wire bytes for a range
/// of `range_items` items (`P2` §2.2 `STOP_AT_COST_RATIO = 50%`).
pub fn radix_subtree_summary_upper_bound_bytes(range_items: usize) -> u64 {
    let clamped = (range_items.max(1) as u64).min(RANGE_ITEM_CAP as u64);
    // Internal + leaf summary nodes for `clamped` leaves in binary radix tree <= 2 * clamped
    (2 * clamped)
        .saturating_mul(RADIX_SUMMARY_NODE_WIRE_BYTES)
        .saturating_add(64)
}

/// Recommend a bounded symbol count for an estimated symmetric difference `estimated_diff`,
/// capped by `MAX_CODED_SYMBOLS_PER_RANGE` and the 50% Radix cost ratio ceiling.
pub fn recommended_riblt_symbol_count(estimated_diff: usize, local_range_items: usize) -> usize {
    let raw = estimated_diff
        .saturating_mul(7)
        .div_ceil(4)
        .saturating_add(16);
    let radix_bound = radix_subtree_summary_upper_bound_bytes(local_range_items);
    let max_by_cost =
        ((radix_bound * STOP_AT_COST_RATIO_BPS) / (10_000 * CODED_SYMBOL_WIRE_BYTES)) as usize;
    raw.min(MAX_CODED_SYMBOLS_PER_RANGE).min(max_by_cost.max(1))
}

/// Encode local range items into `symbol_count` deterministic `RibltCodedSymbolV1` cells.
pub fn encode_riblt_coded_symbols(
    context_id: [u8; 32],
    items: &[(InventoryRecordKind, [u8; 32])],
    symbol_count: usize,
) -> Result<Vec<RibltCodedSymbolV1>, RibltFallbackReason> {
    if items.is_empty() || items.len() > RANGE_ITEM_CAP {
        return Err(RibltFallbackReason::CardinalityOutOfBounds);
    }
    if symbol_count == 0 || symbol_count > MAX_CODED_SYMBOLS_PER_RANGE {
        return Err(RibltFallbackReason::MaxCodedSymbolsExceeded);
    }
    let mut cells: Vec<RibltCodedSymbolV1> = (0..symbol_count as u32)
        .map(|symbol_index| RibltCodedSymbolV1 {
            symbol_index,
            count_delta: 0,
            key_xor: [0u8; 40],
            checksum_xor: 0,
        })
        .collect();

    let limit = symbol_count as u32;
    for (kind, cid) in items {
        let key = encode_riblt_key(*kind, *cid);
        let checksum = riblt_symbol_checksum(&key);
        let mut mapping = RibltSymbolMapping::new(&context_id, &key);
        for idx in mapping.indices_below(limit) {
            apply_symbol(&mut cells[idx as usize], 1, &key, checksum);
        }
    }
    Ok(cells)
}

/// Peel a remote `RIBLT-1` sketch against `local_items` within a pre-checked `32 MiB`
/// bounded arena, enforcing `MAX_CODED_SYMBOLS_PER_RANGE`, `MAX_PEEL_OPERATIONS`,
/// `STOP_AT_COST_RATIO_BPS`, singleton checksums, and range bounds.
#[allow(clippy::too_many_arguments)]
pub fn decode_riblt_sketch(
    context_id: [u8; 32],
    range_start: [u8; 32],
    range_end: Option<[u8; 32]>,
    local_items: &[(InventoryRecordKind, [u8; 32])],
    remote_declared_items: u32,
    remote_symbols: &[RibltCodedSymbolV1],
    radix_summary_bound_bytes: u64,
    memory_budget: &mut RibltSessionMemoryBudget,
) -> RibltDecodeOutcome {
    if let Some(end) = range_end {
        if end <= range_start {
            return RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::InvalidRangeBounds,
            };
        }
    }
    if local_items.len() > RANGE_ITEM_CAP
        || remote_declared_items == 0
        || (remote_declared_items as usize) > RANGE_ITEM_CAP
    {
        return RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::CardinalityOutOfBounds,
        };
    }
    if remote_symbols.is_empty() || remote_symbols.len() > MAX_CODED_SYMBOLS_PER_RANGE {
        return RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::MaxCodedSymbolsExceeded,
        };
    }
    let Some(max_index) = remote_symbols.iter().map(|s| s.symbol_index).max() else {
        return RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::MaxCodedSymbolsExceeded,
        };
    };
    let table_len = (max_index as usize).saturating_add(1);
    if table_len > MAX_CODED_SYMBOLS_PER_RANGE {
        return RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::MaxCodedSymbolsExceeded,
        };
    }

    // Reserve worst-case bounded arena upfront (`<= 32 MiB`) — zero unbounded growth!
    let arena_bytes = table_len
        .saturating_mul(std::mem::size_of::<RibltCodedSymbolV1>() + 16)
        .saturating_add(RANGE_ITEM_CAP * 48);
    let lease = match memory_budget.try_reserve(arena_bytes) {
        Ok(lease) => lease,
        Err(reason) => return RibltDecodeOutcome::FallbackToRadix { reason },
    };

    let outcome = run_bounded_peeler(
        context_id,
        range_start,
        range_end,
        local_items,
        remote_symbols,
        table_len,
        radix_summary_bound_bytes,
    );
    memory_budget.release(lease);
    outcome
}

fn run_bounded_peeler(
    context_id: [u8; 32],
    range_start: [u8; 32],
    range_end: Option<[u8; 32]>,
    local_items: &[(InventoryRecordKind, [u8; 32])],
    remote_symbols: &[RibltCodedSymbolV1],
    table_len: usize,
    radix_summary_bound_bytes: u64,
) -> RibltDecodeOutcome {
    let mut cells: Vec<RibltCodedSymbolV1> = (0..table_len as u32)
        .map(|symbol_index| RibltCodedSymbolV1 {
            symbol_index,
            count_delta: 0,
            key_xor: [0u8; 40],
            checksum_xor: 0,
        })
        .collect();

    for sym in remote_symbols {
        cells[sym.symbol_index as usize] = *sym;
    }

    // Subtract local items from index 0..table_len.
    let limit = table_len as u32;
    for (kind, cid) in local_items {
        if !cid_in_half_open_range(cid, &range_start, range_end.as_ref()) {
            return RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::OutOfRangeSymbol,
            };
        }
        let key = encode_riblt_key(*kind, *cid);
        let checksum = riblt_symbol_checksum(&key);
        let mut mapping = RibltSymbolMapping::new(&context_id, &key);
        for idx in mapping.indices_below(limit) {
            apply_symbol(&mut cells[idx as usize], -1, &key, checksum);
        }
    }

    // Prefix-streaming decode with 50% Radix cost-ratio stop (`STOP_AT_COST_RATIO_BPS = 5000`):
    // Process prefixes dynamically or check cost-ratio before/after peeling.
    let streamed_bytes = (remote_symbols.len() as u64).saturating_mul(CODED_SYMBOL_WIRE_BYTES);
    let cost_cap_exceeded = streamed_bytes.saturating_mul(10_000)
        >= radix_summary_bound_bytes.saturating_mul(STOP_AT_COST_RATIO_BPS);

    let mut queue = VecDeque::with_capacity(table_len);
    let mut queued = vec![false; table_len];
    for (idx, cell) in cells.iter().enumerate() {
        if is_pure_singleton(cell) {
            queue.push_back(idx);
            queued[idx] = true;
        }
    }

    let mut remote_only = Vec::new();
    let mut local_only = Vec::new();
    let mut seen_keys = BTreeSet::<[u8; 40]>::new();
    let mut peel_operations = 0usize;

    while let Some(idx) = queue.pop_front() {
        queued[idx] = false;
        let cell = cells[idx];
        if is_zero_cell(&cell) || !is_pure_singleton(&cell) {
            continue;
        }
        let direction = cell.count_delta; // +1 => remote_only, -1 => local_only
        let item_key = cell.key_xor;
        let (kind, cid) = match decode_riblt_key(&item_key) {
            Ok(decoded) => decoded,
            Err(reason) => return RibltDecodeOutcome::FallbackToRadix { reason },
        };
        if !cid_in_half_open_range(&cid, &range_start, range_end.as_ref()) {
            return RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::OutOfRangeSymbol,
            };
        }
        if !seen_keys.insert(item_key) || seen_keys.len() > RANGE_ITEM_CAP * 2 {
            return RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::DuplicateDecodedSymbol,
            };
        }
        if direction == 1 {
            remote_only.push((kind, cid));
        } else {
            local_only.push((kind, cid));
        }

        let checksum = riblt_symbol_checksum(&item_key);
        let mut mapping = RibltSymbolMapping::new(&context_id, &item_key);
        for mapped_idx in mapping.indices_below(limit) {
            peel_operations = peel_operations.saturating_add(1);
            if peel_operations > MAX_PEEL_OPERATIONS {
                return RibltDecodeOutcome::FallbackToRadix {
                    reason: RibltFallbackReason::MaxPeelOperationsExceeded,
                };
            }
            let target = &mut cells[mapped_idx as usize];
            apply_symbol(target, -direction, &item_key, checksum);
            if !queued[mapped_idx as usize] && is_pure_singleton(target) {
                queue.push_back(mapped_idx as usize);
                queued[mapped_idx as usize] = true;
            }
        }
    }

    // Check residual cells (`NEG-NET-004`).
    let all_zero = cells.iter().all(is_zero_cell);
    if !all_zero {
        if cost_cap_exceeded {
            return RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::CostRatioExceeded,
            };
        }
        let has_corrupt_singleton = cells.iter().any(|c| {
            (c.count_delta == 1 || c.count_delta == -1)
                && (decode_riblt_key(&c.key_xor).is_err()
                    || riblt_symbol_checksum(&c.key_xor) != c.checksum_xor)
        });
        return RibltDecodeOutcome::FallbackToRadix {
            reason: if has_corrupt_singleton {
                RibltFallbackReason::SingletonChecksumMismatch
            } else {
                RibltFallbackReason::PeelStalled
            },
        };
    }

    if cost_cap_exceeded {
        return RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::CostRatioExceeded,
        };
    }

    remote_only.sort_by_key(|(k, c)| (*k as u64, *c));
    local_only.sort_by_key(|(k, c)| (*k as u64, *c));
    RibltDecodeOutcome::CandidateDifference {
        remote_only,
        local_only,
        peel_operations,
    }
}

/// Mandatory Merkle root verification gate (`RIB-002`, `NEG-NET-004`).
///
/// Reconstructs the simulated remote `HybridInventoryForest` (or scoped `(kind, range)`
/// subtree) from `local_forest` minus `local_only` plus `validated_remote_only` leaves
/// (obtained ONLY after `Manifest -> Payload -> ValidateThenAcceptSink`) and verifies
/// exact equality with `target_subtree_root`. Any mismatch emits `FallbackToRadix`.
pub fn verify_riblt_against_hybrid_forest(
    local_forest: &HybridInventoryForest,
    local_only: &[(InventoryRecordKind, [u8; 32])],
    validated_remote_only: &[InventoryLeaf],
    scoped_subtree: Option<(InventoryRecordKind, InventoryRange)>,
    target_subtree_root: [u8; 32],
) -> Result<VerifiedRibltSync, RibltDecodeOutcome> {
    let local_only_set: BTreeSet<(u64, [u8; 32])> = local_only
        .iter()
        .map(|(k, cid)| (*k as u64, *cid))
        .collect();
    if local_only_set.len() != local_only.len() {
        return Err(RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::DuplicateDecodedSymbol,
        });
    }

    let mut simulated = HybridInventoryForest::new(local_forest.selector());
    for prefix in local_forest.feed_prefixes() {
        if simulated.insert_feed_prefix(prefix).is_err() {
            return Err(RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::HybridRootMismatch,
            });
        }
    }

    let mut matched_local_only = 0usize;
    for leaf in local_forest.all_records() {
        if let Some((kind, range)) = scoped_subtree {
            if leaf.record_kind == kind
                && range.contains(&leaf.cid)
                && local_only_set.contains(&(leaf.record_kind as u64, leaf.cid))
            {
                matched_local_only += 1;
                continue;
            }
        } else if local_only_set.contains(&(leaf.record_kind as u64, leaf.cid)) {
            matched_local_only += 1;
            continue;
        }
        if simulated.insert_record(leaf).is_err() {
            return Err(RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::HybridRootMismatch,
            });
        }
    }
    if matched_local_only != local_only.len() {
        return Err(RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::HybridRootMismatch,
        });
    }

    for remote_leaf in validated_remote_only {
        if let Some((kind, range)) = scoped_subtree {
            if remote_leaf.record_kind != kind || !range.contains(&remote_leaf.cid) {
                return Err(RibltDecodeOutcome::FallbackToRadix {
                    reason: RibltFallbackReason::OutOfRangeSymbol,
                });
            }
        }
        if simulated.insert_record(*remote_leaf).is_err() {
            return Err(RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::HybridRootMismatch,
            });
        }
    }

    let computed_root = match scoped_subtree {
        Some((kind, range)) => simulated.range_summary(kind, range).root,
        None => simulated.root(),
    };
    if computed_root != target_subtree_root {
        return Err(RibltDecodeOutcome::FallbackToRadix {
            reason: RibltFallbackReason::HybridRootMismatch,
        });
    }

    Ok(VerifiedRibltSync {
        verified_subtree_root: computed_root,
        remote_only_count: validated_remote_only.len(),
        local_only_count: local_only.len(),
    })
}

fn cid_in_half_open_range(
    cid: &[u8; 32],
    range_start: &[u8; 32],
    range_end: Option<&[u8; 32]>,
) -> bool {
    cid >= range_start && range_end.is_none_or(|end| cid < end)
}

fn apply_symbol(cell: &mut RibltCodedSymbolV1, delta: i64, key: &[u8; 40], checksum: u64) {
    cell.count_delta = cell.count_delta.saturating_add(delta);
    for (dst, src) in cell.key_xor.iter_mut().zip(key.iter()) {
        *dst ^= *src;
    }
    cell.checksum_xor ^= checksum;
}

fn is_zero_cell(cell: &RibltCodedSymbolV1) -> bool {
    cell.count_delta == 0 && cell.key_xor == [0u8; 40] && cell.checksum_xor == 0
}

fn is_pure_singleton(cell: &RibltCodedSymbolV1) -> bool {
    (cell.count_delta == 1 || cell.count_delta == -1)
        && decode_riblt_key(&cell.key_xor).is_ok()
        && riblt_symbol_checksum(&cell.key_xor) == cell.checksum_xor
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::SelectorCid;

    use super::*;

    fn cid(index: u32) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"riblt-test-item:");
        h.update(&index.to_be_bytes());
        *h.finalize().as_bytes()
    }

    #[test]
    fn riblt_peels_symmetric_difference_and_passes_merkle_root_gate() {
        let selector = SelectorCid::from_bytes([0x11; 32]);
        let mut local_forest = HybridInventoryForest::new(selector);
        let mut remote_forest = HybridInventoryForest::new(selector);

        // 300 shared items, 45 remote-only, 35 local-only (delta = 80).
        for i in 0..300u32 {
            let leaf = InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid: cid(i),
                canonical_length: 256,
            };
            local_forest.insert_record(leaf).unwrap();
            remote_forest.insert_record(leaf).unwrap();
        }
        let mut expected_local_only = Vec::new();
        for i in 300..335u32 {
            let leaf = InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid: cid(i),
                canonical_length: 256,
            };
            local_forest.insert_record(leaf).unwrap();
            expected_local_only.push((InventoryRecordKind::Object, leaf.cid));
        }
        let mut expected_remote_leaves = Vec::new();
        for i in 1_000..1_045u32 {
            let leaf = InventoryLeaf {
                record_kind: InventoryRecordKind::Event,
                cid: cid(i),
                canonical_length: 512,
            };
            remote_forest.insert_record(leaf).unwrap();
            expected_remote_leaves.push(leaf);
        }

        let context_id = [0x42; 32];
        let remote_items: Vec<_> = remote_forest
            .all_records()
            .into_iter()
            .map(|l| (l.record_kind, l.cid))
            .collect();
        let local_items: Vec<_> = local_forest
            .all_records()
            .into_iter()
            .map(|l| (l.record_kind, l.cid))
            .collect();

        let symbol_count = recommended_riblt_symbol_count(80, local_items.len());
        let coded = encode_riblt_coded_symbols(context_id, &remote_items, symbol_count).unwrap();
        let mut budget = RibltSessionMemoryBudget::new();
        let radix_bound = radix_subtree_summary_upper_bound_bytes(local_items.len() * 2);

        let outcome = decode_riblt_sketch(
            context_id,
            [0x00; 32],
            None,
            &local_items,
            remote_items.len() as u32,
            &coded,
            radix_bound,
            &mut budget,
        );
        assert_eq!(budget.allocated_bytes(), 0, "arena lease released");
        assert!(!outcome.grants_authority());
        assert!(!outcome.establishes_completion());

        let RibltDecodeOutcome::CandidateDifference {
            remote_only,
            local_only,
            ..
        } = outcome
        else {
            panic!("expected CandidateDifference, got {outcome:?}");
        };
        expected_local_only.sort_by_key(|(k, c)| (*k as u64, *c));
        assert_eq!(local_only, expected_local_only);
        assert_eq!(remote_only.len(), 45);

        let verified = verify_riblt_against_hybrid_forest(
            &local_forest,
            &local_only,
            &expected_remote_leaves,
            None,
            remote_forest.root(),
        )
        .unwrap();
        assert_eq!(verified.verified_subtree_root, remote_forest.root());

        // NEG-NET-004: forged target root MUST reject and emit FallbackToRadix!
        let mut forged_root = remote_forest.root();
        forged_root[0] ^= 0xff;
        assert_eq!(
            verify_riblt_against_hybrid_forest(
                &local_forest,
                &local_only,
                &expected_remote_leaves,
                None,
                forged_root,
            )
            .unwrap_err(),
            RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::HybridRootMismatch
            }
        );
    }

    #[test]
    fn poisoned_checksum_and_cost_ratio_fall_back_deterministically() {
        let context_id = [0x77; 32];
        let items = vec![(InventoryRecordKind::Object, cid(1))];
        let mut coded = encode_riblt_coded_symbols(context_id, &items, 8).unwrap();
        // Poison checksum of singleton cell 0
        coded[0].checksum_xor ^= 1;
        for cell in coded.iter_mut().skip(1) {
            *cell = RibltCodedSymbolV1 {
                symbol_index: cell.symbol_index,
                count_delta: 0,
                key_xor: [0; 40],
                checksum_xor: 0,
            };
        }
        let mut budget = RibltSessionMemoryBudget::new();
        let poisoned = decode_riblt_sketch(
            context_id,
            [0; 32],
            None,
            &[],
            1,
            &coded,
            100_000,
            &mut budget,
        );
        assert_eq!(
            poisoned,
            RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::SingletonChecksumMismatch
            }
        );

        // Cost ratio >= 50% triggers CostRatioExceeded
        let valid_coded = encode_riblt_coded_symbols(context_id, &items, 16).unwrap();
        let tiny_radix_bound = 512;
        let cost_hit = decode_riblt_sketch(
            context_id,
            [0; 32],
            None,
            &[],
            1,
            &valid_coded,
            tiny_radix_bound,
            &mut budget,
        );
        assert_eq!(
            cost_hit,
            RibltDecodeOutcome::FallbackToRadix {
                reason: RibltFallbackReason::CostRatioExceeded
            }
        );
    }
}
