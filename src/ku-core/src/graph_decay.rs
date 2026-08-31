//! # Graph Decay — Unified Decay Engine
//!
//! Provides concrete `Decayable` implementations for `Bond` and `BondMeta`,
//! plus a `DecayRunner` that processes batch decay across all active bonds.
//!
//! ## Design Philosophy: "River, Not Lake"
//! Knowledge that flows (is accessed, cited, reinforced) stays strong.
//! Knowledge that stagnates naturally weakens — but never dies.
//! Structural bonds (PartOf, InstanceOf) are immune to decay.

use crate::graph_types::{decay_lambda, BondEvent, BondMeta, Decayable, WeakeningReason};
use crate::types::{Bond, DecayRate, EdgeState, RelationType};

// ============================================================================
// Decayable implementations
// ============================================================================

impl Decayable for Bond {
    fn decay_rate(&self) -> f64 {
        decay_lambda(self.relation)
    }
    fn last_reinforced_secs(&self) -> u64 {
        self.last_reinforced.unwrap_or(self.created_at) as u64
    }
}

impl Decayable for BondMeta {
    fn decay_rate(&self) -> f64 {
        // BondMeta doesn't store relation type, use DecayRate enum
        decay_rate_to_lambda(self.decay)
    }
    fn last_reinforced_secs(&self) -> u64 {
        self.timestamp as u64
    }
}

// ============================================================================
// DecayRunner — batch decay processing
// ============================================================================

/// Report from a decay tick run.
#[derive(Debug, Clone, Default)]
pub struct DecayReport {
    /// Total bonds checked
    pub bonds_checked: u64,
    /// Bonds whose effective weight dropped below weakening threshold
    pub bonds_weakened: u64,
    /// Bonds whose effective weight dropped to floor (deprecated)
    pub bonds_deprecated: u64,
    /// Bonds with λ=0 (immune to decay)
    pub bonds_immune: u64,
    /// Events generated (for event store)
    pub events: Vec<BondEvent>,
}

/// Thresholds for decay state transitions
pub const WEAKEN_THRESHOLD: f64 = 0.3; // below 30% of original → Weakened
pub const DEPRECATE_THRESHOLD: f64 = 0.05; // below 5% of original → Deprecated

/// Batch decay runner.
pub struct DecayRunner;

pub type DecayBond = (([u8; 32], [u8; 32]), Bond);

impl DecayRunner {
    /// Process decay for a list of bonds at the given timestamp.
    ///
    /// Returns a DecayReport with state transitions and events.
    /// Does NOT modify the bonds — caller uses the report to update storage.
    pub fn run_decay(
        bonds: &[DecayBond], // ((source, target), bond)
        now_secs: u64,
    ) -> DecayReport {
        let mut report = DecayReport::default();

        for ((source_cid, target_cid), bond) in bonds {
            report.bonds_checked += 1;

            let lambda = bond.decay_rate();
            if lambda == 0.0 {
                report.bonds_immune += 1;
                continue;
            }

            let base = bond.weight as f64;
            let effective = bond.effective_weight(base, now_secs);
            let ratio = if base > 0.0 { effective / base } else { 1.0 };

            match bond.state {
                EdgeState::Active if ratio < DEPRECATE_THRESHOLD => {
                    // Skip directly to Deprecated
                    report.bonds_deprecated += 1;
                    report.events.push(BondEvent::StateChanged {
                        source_cid: *source_cid,
                        target_cid: *target_cid,
                        relation: bond.relation,
                        old_state: EdgeState::Active,
                        new_state: EdgeState::Deprecated,
                        timestamp: now_secs,
                    });
                }
                EdgeState::Active if ratio < WEAKEN_THRESHOLD => {
                    report.bonds_weakened += 1;
                    let new_weight = effective.round() as u16;
                    report.events.push(BondEvent::Weakened {
                        source_cid: *source_cid,
                        target_cid: *target_cid,
                        relation: bond.relation,
                        old_weight: bond.weight,
                        new_weight,
                        reason: WeakeningReason::Decay,
                        timestamp: now_secs,
                    });
                }
                EdgeState::Weakened if ratio < DEPRECATE_THRESHOLD => {
                    report.bonds_deprecated += 1;
                    report.events.push(BondEvent::StateChanged {
                        source_cid: *source_cid,
                        target_cid: *target_cid,
                        relation: bond.relation,
                        old_state: EdgeState::Weakened,
                        new_state: EdgeState::Deprecated,
                        timestamp: now_secs,
                    });
                }
                _ => {} // Already deprecated or ratio still healthy
            }
        }

        report
    }

    /// Compute the effective weight of a single bond at a given time.
    pub fn effective_bond_weight(bond: &Bond, now_secs: u64) -> f64 {
        bond.effective_weight(bond.weight as f64, now_secs)
    }

    /// Compute the effective weight using BondMeta.
    pub fn effective_meta_weight(meta: &BondMeta, now_secs: u64) -> f64 {
        meta.effective_weight(meta.weight as f64, now_secs)
    }

    /// Reinforce a bond: compute new weight and generate a Reinforced event.
    ///
    /// Returns a Reinforced event. Does NOT modify the bond — caller
    /// uses the event to update storage.
    pub fn reinforce(
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        bond: &Bond,
        boost: u16,
        now_secs: u64,
    ) -> BondEvent {
        let new_weight = bond.weight.saturating_add(boost).min(10000);
        BondEvent::Reinforced {
            source_cid,
            target_cid,
            relation: bond.relation,
            old_weight: bond.weight,
            new_weight,
            timestamp: now_secs,
        }
    }
}

// ============================================================================
// Helper: DecayRate enum ↔ lambda
// ============================================================================

/// Convert [`DecayRate`] enum to lambda value (per-day exponential rate).
pub fn decay_rate_to_lambda(rate: DecayRate) -> f64 {
    match rate {
        DecayRate::None => 0.0,
        DecayRate::Slow => 0.0019,
        DecayRate::Med => 0.0077,
        DecayRate::Fast => 0.099,
    }
}

/// Suggest the appropriate [`DecayRate`] for a [`RelationType`].
pub fn suggested_decay_rate(relation: RelationType) -> DecayRate {
    let lambda = decay_lambda(relation);
    if lambda == 0.0 {
        DecayRate::None
    } else if lambda < 0.005 {
        DecayRate::Slow
    } else if lambda < 0.05 {
        DecayRate::Med
    } else {
        DecayRate::Fast
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn make_bond(
        rel: RelationType,
        weight: u16,
        created_at: u32,
        last_reinforced: Option<u32>,
    ) -> Bond {
        Bond {
            target_cid: vec![0u8; 32],
            relation: rel,
            weight,
            creator: Creator::Human,
            created_at,
            evidence: vec![],
            state: EdgeState::Active,
            initial_weight: Some(weight),
            decay: Some(suggested_decay_rate(rel)),
            last_reinforced,
            reinforce_count: Some(0),
            bidirectional: None,
            context: vec![],
            order: None,
            required: None,
        }
    }

    fn cid(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    // ── 1. Bond Decayable: structural = no decay ────────────────────────

    #[test]
    fn bond_decayable_structural_no_decay() {
        let bond = make_bond(RelationType::PartOf, 8000, 1_000_000, None);
        assert_eq!(bond.decay_rate(), 0.0);
        // Even after 10 years, weight should stay the same
        let ten_years = 1_000_000 + 86400 * 365 * 10;
        let w = bond.effective_weight(bond.weight as f64, ten_years as u64);
        assert_eq!(w, 8000.0);
    }

    // ── 2. Bond Decayable: fast decay (ReactionTo) ──────────────────────

    #[test]
    fn bond_decayable_fast_decay() {
        let bond = make_bond(RelationType::ReactionTo, 10000, 0, None);
        assert!((bond.decay_rate() - 0.099).abs() < 1e-10);
        // After 7 days, weight ≈ 50%
        let one_week = 86400 * 7;
        let w = bond.effective_weight(10000.0, one_week);
        assert!(w > 4900.0 && w < 5100.0, "fast decay 1wk: got {w}");
    }

    // ── 3. BondMeta Decayable ───────────────────────────────────────────

    #[test]
    fn bond_meta_decayable() {
        let meta = BondMeta {
            weight: 10000,
            creator: Creator::Human,
            state: EdgeState::Active,
            decay: DecayRate::Fast,
            timestamp: 0,
        };
        assert!((meta.decay_rate() - 0.099).abs() < 1e-10);
        let one_week = 86400u64 * 7;
        let w = meta.effective_weight(10000.0, one_week);
        assert!(w > 4900.0 && w < 5100.0, "meta fast decay 1wk: got {w}");
    }

    // ── 4. decay_rate_to_lambda: all variants ───────────────────────────

    #[test]
    fn decay_rate_to_lambda_all() {
        assert_eq!(decay_rate_to_lambda(DecayRate::None), 0.0);
        assert!((decay_rate_to_lambda(DecayRate::Slow) - 0.0019).abs() < 1e-10);
        assert!((decay_rate_to_lambda(DecayRate::Med) - 0.0077).abs() < 1e-10);
        assert!((decay_rate_to_lambda(DecayRate::Fast) - 0.099).abs() < 1e-10);
    }

    // ── 5. suggested_decay_rate: structural ─────────────────────────────

    #[test]
    fn suggested_decay_rate_structural() {
        assert_eq!(suggested_decay_rate(RelationType::PartOf), DecayRate::None);
        assert_eq!(
            suggested_decay_rate(RelationType::InstanceOf),
            DecayRate::None
        );
        assert_eq!(suggested_decay_rate(RelationType::Cites), DecayRate::None);
    }

    // ── 6. suggested_decay_rate: experiential ───────────────────────────

    #[test]
    fn suggested_decay_rate_experiential() {
        assert_eq!(
            suggested_decay_rate(RelationType::ReactionTo),
            DecayRate::Fast
        );
        assert_eq!(
            suggested_decay_rate(RelationType::SensoryEvidenceFor),
            DecayRate::Fast
        );
    }

    // ── 7. run_decay: empty list ────────────────────────────────────────

    #[test]
    fn run_decay_empty() {
        let report = DecayRunner::run_decay(&[], 1_000_000);
        assert_eq!(report.bonds_checked, 0);
        assert_eq!(report.bonds_weakened, 0);
        assert_eq!(report.bonds_deprecated, 0);
        assert_eq!(report.bonds_immune, 0);
        assert!(report.events.is_empty());
    }

    // ── 8. run_decay: immune bonds ──────────────────────────────────────

    #[test]
    fn run_decay_immune_bonds() {
        let bond = make_bond(RelationType::PartOf, 8000, 0, None);
        let bonds = vec![((cid(1), cid(2)), bond)];
        let report = DecayRunner::run_decay(&bonds, 86400 * 365 * 10); // 10 years
        assert_eq!(report.bonds_checked, 1);
        assert_eq!(report.bonds_immune, 1);
        assert_eq!(report.bonds_weakened, 0);
        assert_eq!(report.bonds_deprecated, 0);
        assert!(report.events.is_empty());
    }

    // ── 9. run_decay: active → weakened ─────────────────────────────────

    #[test]
    fn run_decay_active_to_weakened() {
        // ReactionTo has λ=0.099, half-life ~7 days.
        // After ~12 days, ratio ≈ exp(-0.099*12) ≈ 0.305 → just above DEPRECATE
        // After ~14 days, ratio ≈ exp(-0.099*14) ≈ 0.25 → below WEAKEN (0.3)
        let bond = make_bond(RelationType::ReactionTo, 10000, 0, None);
        let bonds = vec![((cid(1), cid(2)), bond)];
        let now = 86400 * 14; // 14 days
        let report = DecayRunner::run_decay(&bonds, now);
        assert_eq!(report.bonds_weakened, 1);
        assert_eq!(report.bonds_deprecated, 0);
        assert_eq!(report.events.len(), 1);
        match &report.events[0] {
            BondEvent::Weakened {
                old_weight, reason, ..
            } => {
                assert_eq!(*old_weight, 10000);
                assert_eq!(*reason, WeakeningReason::Decay);
            }
            other => panic!("expected Weakened, got {:?}", other),
        }
    }

    // ── 10. run_decay: active → deprecated (very old) ───────────────────

    #[test]
    fn run_decay_active_to_deprecated() {
        // ReactionTo, after ~30 days: ratio ≈ exp(-0.099*30) ≈ 0.052
        // After ~35 days: ratio ≈ exp(-0.099*35) ≈ 0.032 → below DEPRECATE (0.05)
        let bond = make_bond(RelationType::ReactionTo, 10000, 0, None);
        let bonds = vec![((cid(1), cid(2)), bond)];
        let now = 86400 * 35; // 35 days
        let report = DecayRunner::run_decay(&bonds, now);
        assert_eq!(report.bonds_deprecated, 1);
        assert_eq!(report.bonds_weakened, 0);
        assert_eq!(report.events.len(), 1);
        match &report.events[0] {
            BondEvent::StateChanged {
                old_state,
                new_state,
                ..
            } => {
                assert_eq!(*old_state, EdgeState::Active);
                assert_eq!(*new_state, EdgeState::Deprecated);
            }
            other => panic!("expected StateChanged, got {:?}", other),
        }
    }

    // ── 11. run_decay: weakened → deprecated ────────────────────────────

    #[test]
    fn run_decay_weakened_to_deprecated() {
        let mut bond = make_bond(RelationType::ReactionTo, 10000, 0, None);
        bond.state = EdgeState::Weakened;
        let bonds = vec![((cid(1), cid(2)), bond)];
        let now = 86400 * 35; // 35 days → ratio ≈ 0.032
        let report = DecayRunner::run_decay(&bonds, now);
        assert_eq!(report.bonds_deprecated, 1);
        assert_eq!(report.events.len(), 1);
        match &report.events[0] {
            BondEvent::StateChanged {
                old_state,
                new_state,
                ..
            } => {
                assert_eq!(*old_state, EdgeState::Weakened);
                assert_eq!(*new_state, EdgeState::Deprecated);
            }
            other => panic!("expected StateChanged, got {:?}", other),
        }
    }

    // ── 12. run_decay: still healthy ────────────────────────────────────

    #[test]
    fn run_decay_still_healthy() {
        // ReactionTo, only 1 day old → ratio ≈ exp(-0.099*1) ≈ 0.906
        let bond = make_bond(RelationType::ReactionTo, 10000, 0, None);
        let bonds = vec![((cid(1), cid(2)), bond)];
        let now = 86400; // 1 day
        let report = DecayRunner::run_decay(&bonds, now);
        assert_eq!(report.bonds_checked, 1);
        assert_eq!(report.bonds_weakened, 0);
        assert_eq!(report.bonds_deprecated, 0);
        assert!(report.events.is_empty());
    }

    // ── 13. run_decay: events match transitions ─────────────────────────

    #[test]
    fn run_decay_events_generated() {
        // Mix of immune, healthy, weakened, deprecated
        let structural = make_bond(RelationType::PartOf, 8000, 0, None);
        let healthy = make_bond(RelationType::ReactionTo, 10000, 0, Some(86400 * 33)); // reinforced 2 days ago
        let weak_candidate = make_bond(RelationType::ReactionTo, 10000, 0, None); // 35 days old

        let bonds = vec![
            ((cid(1), cid(2)), structural),
            ((cid(3), cid(4)), healthy),
            ((cid(5), cid(6)), weak_candidate),
        ];
        let now = 86400 * 35;
        let report = DecayRunner::run_decay(&bonds, now);
        assert_eq!(report.bonds_checked, 3);
        assert_eq!(report.bonds_immune, 1);
        // healthy was reinforced 2 days ago → ratio ≈ 0.82 → no event
        // weak_candidate: 35 days → deprecated
        assert_eq!(report.events.len(), 1);
    }

    // ── 14. effective_bond_weight helper ─────────────────────────────────

    #[test]
    fn effective_bond_weight() {
        let bond = make_bond(RelationType::Supplements, 10000, 0, None);
        // λ=0.0077, after 90 days: exp(-0.0077*90) ≈ 0.500
        let w = DecayRunner::effective_bond_weight(&bond, 86400 * 90);
        assert!(w > 4900.0 && w < 5100.0, "supplements 90d: got {w}");
    }

    // ── 15. reinforce creates event ─────────────────────────────────────

    #[test]
    fn reinforce_creates_event() {
        let bond = make_bond(RelationType::Extends, 5000, 0, None);
        let event = DecayRunner::reinforce(cid(1), cid(2), &bond, 2000, 1_000_000);
        match event {
            BondEvent::Reinforced {
                old_weight,
                new_weight,
                ..
            } => {
                assert_eq!(old_weight, 5000);
                assert_eq!(new_weight, 7000);
            }
            other => panic!("expected Reinforced, got {:?}", other),
        }
    }

    // ── 16. reinforce weight cap at 10000 ───────────────────────────────

    #[test]
    fn reinforce_weight_cap() {
        let bond = make_bond(RelationType::Extends, 9000, 0, None);
        let event = DecayRunner::reinforce(cid(1), cid(2), &bond, 5000, 1_000_000);
        match event {
            BondEvent::Reinforced { new_weight, .. } => {
                assert_eq!(new_weight, 10000, "weight should be capped at 10000");
            }
            other => panic!("expected Reinforced, got {:?}", other),
        }
    }
}
