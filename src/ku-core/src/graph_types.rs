//! # OBKG — Graph Domain Types
//!
//! Core types for the OneBrain Knowledge Graph (OBKG) Phase 1:
//!
//! - [`BondMeta`]: Compact 9-byte representation for index tables
//! - [`BondEvent`]: Event-sourced bond mutations (Created, Reinforced, Weakened, StateChanged)
//! - [`WeakeningReason`]: Why a bond was weakened
//! - [`Decayable`]: Trait for time-based weight decay
//! - [`BondSnapshot`]: Compaction checkpoint of a bond's state
//! - [`CompactionReport`]: Stats from an event log compaction pass
//! - [`GraphStats`]: Aggregate edge statistics

use serde::{Serialize, Deserialize};
use crate::types::{Bond, Creator, DecayRate, EdgeState, RelationType};

// ============================================================================
// 1. BondMeta — Compact 9-byte index entry
// ============================================================================

/// Compact bond metadata for index/lookup tables.
///
/// Layout (9 bytes, big-endian):
/// ```text
/// [weight:2][creator:1][state:1][decay:1][timestamp:4]
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BondMeta {
    pub weight: u16,
    pub creator: Creator,
    pub state: EdgeState,
    pub decay: DecayRate,
    pub timestamp: u32,
}

impl BondMeta {
    /// Serialize to a fixed 9-byte big-endian representation.
    pub fn to_bytes(&self) -> [u8; 9] {
        let mut buf = [0u8; 9];
        buf[0..2].copy_from_slice(&self.weight.to_be_bytes());
        buf[2] = self.creator as u8;
        buf[3] = self.state as u8;
        buf[4] = self.decay as u8;
        buf[5..9].copy_from_slice(&self.timestamp.to_be_bytes());
        buf
    }

    /// Deserialize from a 9-byte big-endian representation.
    pub fn from_bytes(bytes: &[u8; 9]) -> Self {
        let weight = u16::from_be_bytes([bytes[0], bytes[1]]);
        let creator = match bytes[2] {
            0 => Creator::Human,
            1 => Creator::Ai,
            2 => Creator::System,
            _ => Creator::Hybrid,
        };
        let state = match bytes[3] {
            0 => EdgeState::Active,
            1 => EdgeState::Weakened,
            _ => EdgeState::Deprecated,
        };
        let decay = match bytes[4] {
            0 => DecayRate::None,
            1 => DecayRate::Slow,
            2 => DecayRate::Med,
            _ => DecayRate::Fast,
        };
        let timestamp = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        Self { weight, creator, state, decay, timestamp }
    }

    /// Extract a `BondMeta` from an existing [`Bond`].
    pub fn from_bond(bond: &Bond) -> Self {
        Self {
            weight: bond.weight,
            creator: bond.creator,
            state: bond.state,
            decay: bond.decay.unwrap_or(DecayRate::None),
            timestamp: bond.last_reinforced.unwrap_or(bond.created_at),
        }
    }
}

// ============================================================================
// 2. WeakeningReason
// ============================================================================

/// Reason a bond was weakened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeakeningReason {
    /// Natural time-based decay
    Decay,
    /// Conflicting evidence invalidated the bond
    Contradiction,
    /// Insufficient user / system engagement
    LowEngagement,
    /// Knowledge-immune system flagged the bond
    ImmuneResponse,
    /// Explicit human / admin override
    ManualOverride,
}

// ============================================================================
// 3. BondEvent — Event Sourcing
// ============================================================================

/// Event-sourced bond mutation.
///
/// Every state change on a bond is captured as an immutable event,
/// enabling full audit trail, replay, and compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BondEvent {
    /// A new bond was created between two KUs.
    Created {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        weight: u16,
        creator: Creator,
        evidence: Vec<Vec<u8>>,
        timestamp: u64,
    },
    /// An existing bond was reinforced (weight increased).
    Reinforced {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        old_weight: u16,
        new_weight: u16,
        timestamp: u64,
    },
    /// An existing bond was weakened (weight decreased).
    Weakened {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        old_weight: u16,
        new_weight: u16,
        reason: WeakeningReason,
        timestamp: u64,
    },
    /// A bond's lifecycle state was changed.
    StateChanged {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        old_state: EdgeState,
        new_state: EdgeState,
        timestamp: u64,
    },
}

impl BondEvent {
    /// Returns the timestamp of this event.
    pub fn timestamp(&self) -> u64 {
        match self {
            BondEvent::Created { timestamp, .. } => *timestamp,
            BondEvent::Reinforced { timestamp, .. } => *timestamp,
            BondEvent::Weakened { timestamp, .. } => *timestamp,
            BondEvent::StateChanged { timestamp, .. } => *timestamp,
        }
    }

    /// Returns a reference to the source CID of this event.
    pub fn source_cid(&self) -> &[u8; 32] {
        match self {
            BondEvent::Created { source_cid, .. } => source_cid,
            BondEvent::Reinforced { source_cid, .. } => source_cid,
            BondEvent::Weakened { source_cid, .. } => source_cid,
            BondEvent::StateChanged { source_cid, .. } => source_cid,
        }
    }

    /// Returns a reference to the target CID of this event.
    pub fn target_cid(&self) -> &[u8; 32] {
        match self {
            BondEvent::Created { target_cid, .. } => target_cid,
            BondEvent::Reinforced { target_cid, .. } => target_cid,
            BondEvent::Weakened { target_cid, .. } => target_cid,
            BondEvent::StateChanged { target_cid, .. } => target_cid,
        }
    }

    /// Serialize this event to CBOR bytes using ciborium.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .expect("BondEvent CBOR serialization should not fail");
        buf
    }

    /// Deserialize a `BondEvent` from CBOR bytes.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, String> {
        ciborium::from_reader(bytes)
            .map_err(|e| format!("BondEvent CBOR deserialization failed: {e}"))
    }
}

// ============================================================================
// 4. decay_lambda — Per-relation decay rate (per day)
// ============================================================================

/// Returns the per-day exponential decay rate λ for a given [`RelationType`].
///
/// Weight decays as: `w(t) = w₀ · exp(-λ · t)` where `t` is elapsed days.
///
/// | λ       | Half-life  | Category                    |
/// |---------|------------|-----------------------------|
/// | 0.0     | ∞          | Structural / formal / provenance |
/// | 0.0019  | ~1 year    | Epistemic / causal / derivation (slow) |
/// | 0.0077  | ~90 days   | Supplementary / derivation (medium) |
/// | 0.099   | ~7 days    | Experiential (fast)          |
pub fn decay_lambda(relation: RelationType) -> f64 {
    match relation {
        // λ = 0.0 — structural, temporal, provenance, formal: never decay
        RelationType::PartOf
        | RelationType::InstanceOf
        | RelationType::Specializes
        | RelationType::Generalizes
        | RelationType::Precedes
        | RelationType::Cooccurs
        | RelationType::Cites
        | RelationType::AuthoredBy
        | RelationType::ReviewedBy
        | RelationType::Duplicates
        | RelationType::Supersedes
        | RelationType::FormallyProves => 0.0,

        // λ = 0.0019 — slow decay (~1 year half-life)
        RelationType::Extends
        | RelationType::Corroborates
        | RelationType::Causes
        | RelationType::Enables
        | RelationType::Prevents
        | RelationType::DependsOn
        | RelationType::AppliesTo
        | RelationType::DerivedFrom
        | RelationType::Translates
        | RelationType::Paraphrases
        | RelationType::EvolvesInto
        | RelationType::VariantOf => 0.0019,

        // λ = 0.0077 — medium decay (~90 day half-life)
        RelationType::Supplements
        | RelationType::Refutes
        | RelationType::Qualifies
        | RelationType::ExampleOf
        | RelationType::AnalogyOf
        | RelationType::Inspires
        | RelationType::TestimonyAbout
        | RelationType::CulturallyContextualizes => 0.0077,

        // λ = 0.099 — fast decay (~7 day half-life)
        RelationType::ReactionTo
        | RelationType::SensoryEvidenceFor => 0.099,
    }
}

// ============================================================================
// 5. Decayable trait
// ============================================================================

/// Trait for types whose weight decays over time.
///
/// Default implementation provides exponential decay:
/// `w(t) = w₀ · exp(-λ · Δt / 86400)`, clamped to `floor()`.
pub trait Decayable {
    /// Per-day decay rate λ. Zero means no decay.
    fn decay_rate(&self) -> f64;

    /// Timestamp (unix seconds) of the last reinforcement.
    fn last_reinforced_secs(&self) -> u64;

    /// Grace period (seconds) during which no decay is applied.
    fn grace_period_secs(&self) -> f64 {
        0.0
    }

    /// Minimum weight floor after decay.
    fn floor(&self) -> f64 {
        0.0
    }

    /// Compute the effective weight at `now_secs`, given a `base_weight`.
    fn effective_weight(&self, base_weight: f64, now_secs: u64) -> f64 {
        let elapsed = (now_secs.saturating_sub(self.last_reinforced_secs())) as f64;
        if elapsed < self.grace_period_secs() {
            return base_weight;
        }
        let lambda = self.decay_rate();
        if lambda == 0.0 {
            return base_weight;
        }
        let decayed = base_weight * (-lambda * elapsed / 86400.0).exp();
        decayed.max(self.floor())
    }
}

// ============================================================================
// 6. BondSnapshot — Compaction checkpoint
// ============================================================================

/// Snapshot of a bond's state at a compaction boundary.
///
/// Used to collapse an event stream into a single materialized state,
/// allowing older events to be pruned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BondSnapshot {
    pub source_cid: [u8; 32],
    pub target_cid: [u8; 32],
    pub relation: RelationType,
    pub weight: u16,
    pub state: EdgeState,
}

// ============================================================================
// 7. CompactionReport
// ============================================================================

/// Statistics from an event log compaction pass.
#[derive(Debug, Clone)]
pub struct CompactionReport {
    /// Sequence number of the snapshot this compaction produced.
    pub snapshot_seq: u64,
    /// Number of events removed (collapsed into the snapshot).
    pub events_removed: u64,
    /// Number of events retained (post-snapshot events).
    pub events_retained: u64,
    /// Size of the resulting snapshot in bytes.
    pub snapshot_size_bytes: u64,
}

// ============================================================================
// 8. GraphStats
// ============================================================================

/// Aggregate statistics about the knowledge graph's edges.
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    pub total_edges: u64,
    pub active_edges: u64,
    pub weakened_edges: u64,
    pub deprecated_edges: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Bond, Creator, DecayRate, EdgeState, RelationType};

    // ── Test helper: Decayable impl ──────────────────────────────────────

    struct TestDecayable {
        lambda: f64,
        last: u64,
        grace: f64,
        fl: f64,
    }

    impl Decayable for TestDecayable {
        fn decay_rate(&self) -> f64 { self.lambda }
        fn last_reinforced_secs(&self) -> u64 { self.last }
        fn grace_period_secs(&self) -> f64 { self.grace }
        fn floor(&self) -> f64 { self.fl }
    }

    // ── BondMeta tests ───────────────────────────────────────────────────

    #[test]
    fn bond_meta_roundtrip() {
        let meta = BondMeta {
            weight: 8500,
            creator: Creator::Ai,
            state: EdgeState::Active,
            decay: DecayRate::Slow,
            timestamp: 1_700_000_000,
        };
        let bytes = meta.to_bytes();
        assert_eq!(bytes.len(), 9);
        let restored = BondMeta::from_bytes(&bytes);
        assert_eq!(meta, restored);
    }

    #[test]
    fn bond_meta_roundtrip_all_variants() {
        let meta = BondMeta {
            weight: 10000,
            creator: Creator::Hybrid,
            state: EdgeState::Deprecated,
            decay: DecayRate::Fast,
            timestamp: u32::MAX,
        };
        let bytes = meta.to_bytes();
        let restored = BondMeta::from_bytes(&bytes);
        assert_eq!(meta, restored);
    }

    #[test]
    fn bond_meta_from_bond() {
        let bond = Bond {
            target_cid: vec![0xAA; 32],
            relation: RelationType::Extends,
            weight: 7000,
            creator: Creator::Human,
            created_at: 1_600_000_000,
            evidence: vec![],
            state: EdgeState::Weakened,
            initial_weight: Some(9000),
            decay: Some(DecayRate::Med),
            last_reinforced: Some(1_650_000_000),
            reinforce_count: Some(3),
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        };
        let meta = BondMeta::from_bond(&bond);
        assert_eq!(meta.weight, 7000);
        assert_eq!(meta.creator, Creator::Human);
        assert_eq!(meta.state, EdgeState::Weakened);
        assert_eq!(meta.decay, DecayRate::Med);
        assert_eq!(meta.timestamp, 1_650_000_000); // last_reinforced
    }

    #[test]
    fn bond_meta_from_bond_no_reinforced() {
        let bond = Bond {
            target_cid: vec![0xBB; 32],
            relation: RelationType::PartOf,
            weight: 5000,
            creator: Creator::System,
            created_at: 1_500_000_000,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: None,
            decay: None,
            last_reinforced: None,
            reinforce_count: None,
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        };
        let meta = BondMeta::from_bond(&bond);
        assert_eq!(meta.decay, DecayRate::None);
        assert_eq!(meta.timestamp, 1_500_000_000); // falls back to created_at
    }

    // ── BondEvent CBOR roundtrip tests ───────────────────────────────────

    fn test_cids() -> ([u8; 32], [u8; 32]) {
        let mut src = [0u8; 32];
        let mut tgt = [0u8; 32];
        src[0] = 0x11;
        src[31] = 0xAA;
        tgt[0] = 0x22;
        tgt[31] = 0xBB;
        (src, tgt)
    }

    #[test]
    fn bond_event_cbor_roundtrip_created() {
        let (src, tgt) = test_cids();
        let event = BondEvent::Created {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::Extends,
            weight: 8000,
            creator: Creator::Ai,
            evidence: vec![vec![1, 2, 3], vec![4, 5]],
            timestamp: 1_700_000_000_000,
        };
        let cbor = event.to_cbor();
        let restored = BondEvent::from_cbor(&cbor).unwrap();
        assert_eq!(event.timestamp(), restored.timestamp());
        assert_eq!(event.source_cid(), restored.source_cid());
        assert_eq!(event.target_cid(), restored.target_cid());
    }

    #[test]
    fn bond_event_cbor_roundtrip_reinforced() {
        let (src, tgt) = test_cids();
        let event = BondEvent::Reinforced {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::Corroborates,
            old_weight: 5000,
            new_weight: 7500,
            timestamp: 1_700_001_000_000,
        };
        let cbor = event.to_cbor();
        let restored = BondEvent::from_cbor(&cbor).unwrap();
        assert_eq!(event.timestamp(), restored.timestamp());
        if let BondEvent::Reinforced { old_weight, new_weight, .. } = restored {
            assert_eq!(old_weight, 5000);
            assert_eq!(new_weight, 7500);
        } else {
            panic!("expected Reinforced variant");
        }
    }

    #[test]
    fn bond_event_cbor_roundtrip_weakened() {
        let (src, tgt) = test_cids();
        let event = BondEvent::Weakened {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::Supplements,
            old_weight: 6000,
            new_weight: 3000,
            reason: WeakeningReason::Decay,
            timestamp: 1_700_002_000_000,
        };
        let cbor = event.to_cbor();
        let restored = BondEvent::from_cbor(&cbor).unwrap();
        assert_eq!(event.timestamp(), restored.timestamp());
        if let BondEvent::Weakened { reason, old_weight, new_weight, .. } = restored {
            assert_eq!(reason, WeakeningReason::Decay);
            assert_eq!(old_weight, 6000);
            assert_eq!(new_weight, 3000);
        } else {
            panic!("expected Weakened variant");
        }
    }

    #[test]
    fn bond_event_cbor_roundtrip_state_changed() {
        let (src, tgt) = test_cids();
        let event = BondEvent::StateChanged {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::PartOf,
            old_state: EdgeState::Active,
            new_state: EdgeState::Deprecated,
            timestamp: 1_700_003_000_000,
        };
        let cbor = event.to_cbor();
        let restored = BondEvent::from_cbor(&cbor).unwrap();
        if let BondEvent::StateChanged { old_state, new_state, .. } = restored {
            assert_eq!(old_state, EdgeState::Active);
            assert_eq!(new_state, EdgeState::Deprecated);
        } else {
            panic!("expected StateChanged variant");
        }
    }

    #[test]
    fn bond_event_timestamp() {
        let (src, tgt) = test_cids();
        let ts = 42_000_000_u64;
        let event = BondEvent::Created {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::Causes,
            weight: 1000,
            creator: Creator::System,
            evidence: vec![],
            timestamp: ts,
        };
        assert_eq!(event.timestamp(), ts);
    }

    #[test]
    fn bond_event_source_cid() {
        let (src, tgt) = test_cids();
        let event = BondEvent::Reinforced {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::Enables,
            old_weight: 100,
            new_weight: 200,
            timestamp: 999,
        };
        assert_eq!(event.source_cid(), &src);
    }

    #[test]
    fn bond_event_target_cid() {
        let (src, tgt) = test_cids();
        let event = BondEvent::StateChanged {
            source_cid: src,
            target_cid: tgt,
            relation: RelationType::Prevents,
            old_state: EdgeState::Weakened,
            new_state: EdgeState::Active,
            timestamp: 1234,
        };
        assert_eq!(event.target_cid(), &tgt);
    }

    // ── decay_lambda tests ───────────────────────────────────────────────

    #[test]
    fn decay_lambda_never() {
        // Structural, temporal, provenance, formal — λ = 0.0
        let never_decay = [
            RelationType::PartOf, RelationType::InstanceOf,
            RelationType::Specializes, RelationType::Generalizes,
            RelationType::Precedes, RelationType::Cooccurs,
            RelationType::Cites, RelationType::AuthoredBy,
            RelationType::ReviewedBy, RelationType::Duplicates,
            RelationType::Supersedes, RelationType::FormallyProves,
        ];
        for rel in &never_decay {
            assert_eq!(decay_lambda(*rel), 0.0, "expected λ=0.0 for {:?}", rel);
        }
    }

    #[test]
    fn decay_lambda_slow() {
        // Epistemic / causal — λ = 0.0019
        let slow = [
            RelationType::Extends, RelationType::Corroborates,
            RelationType::Causes, RelationType::Enables,
            RelationType::Prevents, RelationType::DependsOn,
            RelationType::AppliesTo, RelationType::DerivedFrom,
            RelationType::Translates, RelationType::Paraphrases,
            RelationType::EvolvesInto, RelationType::VariantOf,
        ];
        for rel in &slow {
            assert!((decay_lambda(*rel) - 0.0019).abs() < 1e-10,
                "expected λ≈0.0019 for {:?}", rel);
        }
    }

    #[test]
    fn decay_lambda_medium() {
        // Derivation / supplementary — λ = 0.0077
        let med = [
            RelationType::Supplements, RelationType::Refutes,
            RelationType::Qualifies, RelationType::ExampleOf,
            RelationType::AnalogyOf, RelationType::Inspires,
            RelationType::TestimonyAbout, RelationType::CulturallyContextualizes,
        ];
        for rel in &med {
            assert!((decay_lambda(*rel) - 0.0077).abs() < 1e-10,
                "expected λ≈0.0077 for {:?}", rel);
        }
    }

    #[test]
    fn decay_lambda_fast() {
        // Experiential — λ = 0.099
        let fast = [
            RelationType::ReactionTo, RelationType::SensoryEvidenceFor,
        ];
        for rel in &fast {
            assert!((decay_lambda(*rel) - 0.099).abs() < 1e-10,
                "expected λ≈0.099 for {:?}", rel);
        }
    }

    // ── Decayable trait tests ────────────────────────────────────────────

    #[test]
    fn decayable_no_decay() {
        let d = TestDecayable { lambda: 0.0, last: 0, grace: 0.0, fl: 0.0 };
        // With λ=0, weight should remain unchanged regardless of time
        let w = d.effective_weight(10000.0, 86400 * 365);
        assert_eq!(w, 10000.0);
    }

    #[test]
    fn decayable_slow_decay() {
        let now = 86400 * 365; // 1 year in seconds
        let d = TestDecayable { lambda: 0.0019, last: 0, grace: 0.0, fl: 0.0 };
        let w = d.effective_weight(10000.0, now);
        // exp(-0.0019 * 365) ≈ 0.5, so weight ≈ 5000
        assert!(w > 4900.0 && w < 5100.0, "slow decay 1yr: got {w}");
    }

    #[test]
    fn decayable_fast_decay() {
        let one_week = 86400 * 7;
        let d = TestDecayable { lambda: 0.099, last: 0, grace: 0.0, fl: 0.0 };
        let w = d.effective_weight(10000.0, one_week);
        // exp(-0.099 * 7) ≈ 0.5, so weight ≈ 5000
        assert!(w > 4900.0 && w < 5100.0, "fast decay 1wk: got {w}");
    }

    #[test]
    fn decayable_grace_period() {
        let d = TestDecayable { lambda: 0.099, last: 0, grace: 100_000.0, fl: 0.0 };
        // Within grace period — weight should not decay
        let w = d.effective_weight(10000.0, 50_000);
        assert_eq!(w, 10000.0);
        // Past grace period — weight should decay
        let w2 = d.effective_weight(10000.0, 200_000);
        assert!(w2 < 10000.0, "past grace should decay, got {w2}");
    }

    #[test]
    fn decayable_floor() {
        let huge_time = 86400 * 365 * 100; // 100 years
        let d = TestDecayable { lambda: 0.099, last: 0, grace: 0.0, fl: 500.0 };
        let w = d.effective_weight(10000.0, huge_time);
        assert_eq!(w, 500.0, "decayed weight should clamp to floor");
    }

    // ── WeakeningReason roundtrip ────────────────────────────────────────

    #[test]
    fn weakening_reason_roundtrip() {
        let reasons = [
            WeakeningReason::Decay,
            WeakeningReason::Contradiction,
            WeakeningReason::LowEngagement,
            WeakeningReason::ImmuneResponse,
            WeakeningReason::ManualOverride,
        ];
        for r in &reasons {
            let json = serde_json::to_string(r).unwrap();
            let restored: WeakeningReason = serde_json::from_str(&json).unwrap();
            assert_eq!(*r, restored);
        }
    }

    // ── BondSnapshot & friends ───────────────────────────────────────────

    #[test]
    fn bond_snapshot_equality() {
        let snap1 = BondSnapshot {
            source_cid: [1u8; 32],
            target_cid: [2u8; 32],
            relation: RelationType::PartOf,
            weight: 9000,
            state: EdgeState::Active,
        };
        let snap2 = snap1.clone();
        assert_eq!(snap1, snap2);
    }

    #[test]
    fn graph_stats_default() {
        let stats = GraphStats::default();
        assert_eq!(stats.total_edges, 0);
        assert_eq!(stats.active_edges, 0);
        assert_eq!(stats.weakened_edges, 0);
        assert_eq!(stats.deprecated_edges, 0);
    }

    #[test]
    fn compaction_report_fields() {
        let report = CompactionReport {
            snapshot_seq: 42,
            events_removed: 1000,
            events_retained: 50,
            snapshot_size_bytes: 4096,
        };
        assert_eq!(report.snapshot_seq, 42);
        assert_eq!(report.events_removed, 1000);
        assert_eq!(report.events_retained, 50);
        assert_eq!(report.snapshot_size_bytes, 4096);
    }
}
