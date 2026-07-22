//! # OBKG ↔ OBT Bridge — Graph Contribution Scoring
//!
//! Pure functions that read OBKG graph metrics and produce a normalized
//! [`GraphContributionScore`] suitable for the OBT reward pipeline.
//!
//! ## Four scoring dimensions
//! | Dimension          | Weight | Source                 |
//! |--------------------|--------|------------------------|
//! | Bond richness      | 0.35   | `BondMeta` index       |
//! | Dream contribution | 0.25   | `DreamReport`          |
//! | FedR participation | 0.20   | `RelationDelta`        |
//! | Graph health       | 0.20   | `BondMeta` active ratio|

use crate::graph_dream::DreamReport;
use crate::graph_fedr::RelationDelta;
use crate::graph_types::BondMeta;
use crate::types::{EdgeState, RelationType};
use std::collections::HashMap;

// ============================================================================
// Constants
// ============================================================================

/// Weight for bond richness dimension.
const W_BOND: f64 = 0.35;
/// Weight for dream contribution dimension.
const W_DREAM: f64 = 0.25;
/// Weight for FedR participation dimension.
const W_FEDR: f64 = 0.20;
/// Weight for graph health dimension.
const W_HEALTH: f64 = 0.20;

/// Maximum bond count before saturation (for normalization).
const BOND_COUNT_CAP: f64 = 100.0;
/// Maximum total weight sum before saturation.
const WEIGHT_SUM_CAP: f64 = 500_000.0;
/// Maximum dream actions (reinforced + associations + pruned) before saturation.
const DREAM_ACTIONS_CAP: f64 = 200.0;
/// Maximum relation coverage (out of 33 relations) — full coverage = 1.0.
const RELATION_TOTAL: f64 = 33.0;
/// Maximum triple count before saturation.
const TRIPLE_COUNT_CAP: f64 = 10_000.0;

// ============================================================================
// GraphContributionScore
// ============================================================================

/// Normalized contribution score from OBKG graph activity.
///
/// All fields are clamped to `[0.0, 1.0]`. The `total` field is the
/// weighted average using `W_BOND`, `W_DREAM`, `W_FEDR`, `W_HEALTH`.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphContributionScore {
    /// Bond richness: active count + weight sum, normalized.
    pub bond_richness: f64,
    /// Dream mode contribution: reinforced + associations + pruned.
    pub dream_contribution: f64,
    /// FedR participation: relation coverage + triple count.
    pub fedr_participation: f64,
    /// Graph health: active/total bond ratio.
    pub graph_health: f64,
    /// Weighted total across all four dimensions.
    pub total: f64,
}

// ============================================================================
// Scoring functions
// ============================================================================

/// Clamp a value to `[0.0, 1.0]`.
#[inline]
fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Bond richness score.
///
/// Combines:
/// - Active bond count (normalized to `BOND_COUNT_CAP`)
/// - Total weight sum of active bonds (normalized to `WEIGHT_SUM_CAP`)
///
/// Final score = 0.5 × count_ratio + 0.5 × weight_ratio, clamped to [0, 1].
pub fn bond_richness_score(bonds: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>) -> f64 {
    if bonds.is_empty() {
        return 0.0;
    }

    let (active_count, weight_sum) = bonds.values().fold((0u64, 0u64), |(cnt, wsum), meta| {
        if meta.state == EdgeState::Active {
            (cnt + 1, wsum + meta.weight as u64)
        } else {
            (cnt, wsum)
        }
    });

    let count_ratio = active_count as f64 / BOND_COUNT_CAP;
    let weight_ratio = weight_sum as f64 / WEIGHT_SUM_CAP;
    clamp01(0.5 * count_ratio + 0.5 * weight_ratio)
}

/// Dream contribution score.
///
/// Measures activity across all three dream phases:
/// - `bonds_reinforced` (replay value)
/// - `associations_created` (creative discovery)
/// - `bonds_pruned` (graph hygiene)
///
/// Score = total_actions / `DREAM_ACTIONS_CAP`, clamped to [0, 1].
pub fn dream_contribution_score(report: &DreamReport) -> f64 {
    let total_actions = report.bonds_reinforced as f64
        + report.associations_created as f64
        + report.bonds_pruned as f64;
    clamp01(total_actions / DREAM_ACTIONS_CAP)
}

/// FedR participation score.
///
/// Combines:
/// - Relation coverage: how many relations have deltas (out of 33)
/// - Triple count: volume of training data contributed
///
/// Score = 0.5 × coverage + 0.5 × triple_ratio, clamped to [0, 1].
pub fn fedr_participation_score(delta: &RelationDelta) -> f64 {
    let coverage = delta.deltas.len() as f64 / RELATION_TOTAL;
    let triple_ratio = delta.triple_count as f64 / TRIPLE_COUNT_CAP;
    clamp01(0.5 * coverage + 0.5 * triple_ratio)
}

/// Graph health score.
///
/// Ratio of active bonds to total bonds.
/// Returns 1.0 for an empty graph (no degradation).
pub fn graph_health_score(bonds: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>) -> f64 {
    if bonds.is_empty() {
        return 1.0;
    }
    let total = bonds.len() as f64;
    let active = bonds
        .values()
        .filter(|m| m.state == EdgeState::Active)
        .count() as f64;
    clamp01(active / total)
}

/// Compute the full [`GraphContributionScore`] from all four dimensions.
///
/// Weights: bond=0.35, dream=0.25, fedr=0.20, health=0.20.
pub fn compute_graph_contribution(
    bonds: &HashMap<([u8; 32], [u8; 32], RelationType), BondMeta>,
    dream: &DreamReport,
    fedr: &RelationDelta,
) -> GraphContributionScore {
    let bond_richness = bond_richness_score(bonds);
    let dream_contribution = dream_contribution_score(dream);
    let fedr_participation = fedr_participation_score(fedr);
    let graph_health = graph_health_score(bonds);

    let total = W_BOND * bond_richness
        + W_DREAM * dream_contribution
        + W_FEDR * fedr_participation
        + W_HEALTH * graph_health;

    GraphContributionScore {
        bond_richness,
        dream_contribution,
        fedr_participation,
        graph_health,
        total,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph_types::BondMeta;
    use crate::types::{Creator, DecayRate, EdgeState, RelationType};
    use std::collections::HashMap;

    // ── Helpers ──────────────────────────────────────────────────────────

    type BondKey = ([u8; 32], [u8; 32], RelationType);
    type BondMap = HashMap<BondKey, BondMeta>;

    fn cid(b: u8) -> [u8; 32] {
        [b; 32]
    }

    fn meta(weight: u16, state: EdgeState) -> BondMeta {
        BondMeta {
            weight,
            creator: Creator::System,
            state,
            decay: DecayRate::None,
            timestamp: 1_000_000,
        }
    }

    fn empty_dream() -> DreamReport {
        DreamReport {
            bonds_reinforced: 0,
            total_weight_added: 0,
            associations_created: 0,
            bonds_pruned: 0,
            events: vec![],
        }
    }

    fn empty_delta() -> RelationDelta {
        RelationDelta {
            deltas: HashMap::new(),
            peer_id: [0u8; 32],
            epoch: 0,
            triple_count: 0,
        }
    }

    // ── 1. Empty bonds → richness = 0 ───────────────────────────────────

    #[test]
    fn bond_richness_empty() {
        let bonds: BondMap = HashMap::new();
        assert_eq!(bond_richness_score(&bonds), 0.0);
    }

    // ── 2. Active bonds contribute to richness ──────────────────────────

    #[test]
    fn bond_richness_active_bonds() {
        let mut bonds: BondMap = HashMap::new();
        for i in 0..10u8 {
            bonds.insert(
                (cid(i), cid(i + 100), RelationType::Extends),
                meta(5000, EdgeState::Active),
            );
        }
        let score = bond_richness_score(&bonds);
        assert!(score > 0.0 && score <= 1.0, "score={score}");
    }

    // ── 3. Deprecated bonds don't count for richness ────────────────────

    #[test]
    fn bond_richness_ignores_deprecated() {
        let mut bonds: BondMap = HashMap::new();
        bonds.insert(
            (cid(1), cid(2), RelationType::PartOf),
            meta(9000, EdgeState::Deprecated),
        );
        // Deprecated bond → active count = 0, weight sum = 0
        assert_eq!(bond_richness_score(&bonds), 0.0);
    }

    // ── 4. Dream score with zero actions ────────────────────────────────

    #[test]
    fn dream_score_zero() {
        assert_eq!(dream_contribution_score(&empty_dream()), 0.0);
    }

    // ── 5. Dream score scales with actions ──────────────────────────────

    #[test]
    fn dream_score_scales() {
        let report = DreamReport {
            bonds_reinforced: 50,
            total_weight_added: 25000,
            associations_created: 30,
            bonds_pruned: 20,
            events: vec![],
        };
        // total_actions = 100, cap = 200 → 0.5
        let score = dream_contribution_score(&report);
        assert!((score - 0.5).abs() < 1e-9, "score={score}");
    }

    // ── 6. Dream score saturates at 1.0 ─────────────────────────────────

    #[test]
    fn dream_score_saturates() {
        let report = DreamReport {
            bonds_reinforced: 300,
            total_weight_added: 0,
            associations_created: 300,
            bonds_pruned: 300,
            events: vec![],
        };
        assert_eq!(dream_contribution_score(&report), 1.0);
    }

    // ── 7. FedR score with empty delta ──────────────────────────────────

    #[test]
    fn fedr_score_empty() {
        assert_eq!(fedr_participation_score(&empty_delta()), 0.0);
    }

    // ── 8. FedR score with coverage and triples ─────────────────────────

    #[test]
    fn fedr_score_coverage_and_triples() {
        let mut deltas = HashMap::new();
        deltas.insert(RelationType::Extends, ([1i8; 32], [1i8; 32]));
        deltas.insert(RelationType::Causes, ([1i8; 32], [1i8; 32]));

        let delta = RelationDelta {
            deltas,
            peer_id: [1u8; 32],
            epoch: 1,
            triple_count: 5000,
        };
        let score = fedr_participation_score(&delta);
        let expected = 0.5 * (2.0 / 33.0) + 0.5 * 0.5;
        assert!(
            (score - expected).abs() < 1e-9,
            "score={score}, expected={expected}"
        );
    }

    // ── 9. Health score: all active = 1.0 ───────────────────────────────

    #[test]
    fn health_all_active() {
        let mut bonds: BondMap = HashMap::new();
        for i in 0..5u8 {
            bonds.insert(
                (cid(i), cid(i + 50), RelationType::PartOf),
                meta(5000, EdgeState::Active),
            );
        }
        assert_eq!(graph_health_score(&bonds), 1.0);
    }

    // ── 10. Health score: mixed states ──────────────────────────────────

    #[test]
    fn health_mixed() {
        let mut bonds: BondMap = HashMap::new();
        bonds.insert(
            (cid(1), cid(2), RelationType::Extends),
            meta(5000, EdgeState::Active),
        );
        bonds.insert(
            (cid(3), cid(4), RelationType::Causes),
            meta(5000, EdgeState::Active),
        );
        bonds.insert(
            (cid(5), cid(6), RelationType::PartOf),
            meta(5000, EdgeState::Active),
        );
        bonds.insert(
            (cid(7), cid(8), RelationType::Enables),
            meta(3000, EdgeState::Weakened),
        );
        bonds.insert(
            (cid(9), cid(10), RelationType::Cites),
            meta(1000, EdgeState::Deprecated),
        );
        assert!((graph_health_score(&bonds) - 0.6).abs() < 1e-9);
    }

    // ── 11. Health score: empty graph = 1.0 ─────────────────────────────

    #[test]
    fn health_empty_graph() {
        let bonds: BondMap = HashMap::new();
        assert_eq!(graph_health_score(&bonds), 1.0);
    }

    // ── 12. Full contribution with all zeroes ───────────────────────────

    #[test]
    fn contribution_all_zero() {
        let bonds: BondMap = HashMap::new();
        let dream = empty_dream();
        let fedr = empty_delta();
        let score = compute_graph_contribution(&bonds, &dream, &fedr);

        assert_eq!(score.bond_richness, 0.0);
        assert_eq!(score.dream_contribution, 0.0);
        assert_eq!(score.fedr_participation, 0.0);
        assert_eq!(score.graph_health, 1.0);
        assert!((score.total - 0.20).abs() < 1e-9, "total={}", score.total);
    }

    // ── 13. Full contribution with active graph ─────────────────────────

    #[test]
    fn contribution_active_graph() {
        let mut bonds: BondMap = HashMap::new();
        for i in 0..20u8 {
            bonds.insert(
                (cid(i), cid(i + 100), RelationType::Extends),
                meta(5000, EdgeState::Active),
            );
        }
        let dream = DreamReport {
            bonds_reinforced: 10,
            total_weight_added: 5000,
            associations_created: 5,
            bonds_pruned: 5,
            events: vec![],
        };
        let mut deltas_map = HashMap::new();
        deltas_map.insert(RelationType::Extends, ([1i8; 32], [1i8; 32]));
        let fedr = RelationDelta {
            deltas: deltas_map,
            peer_id: [1u8; 32],
            epoch: 1,
            triple_count: 1000,
        };

        let score = compute_graph_contribution(&bonds, &dream, &fedr);
        assert!(
            score.total > 0.0 && score.total <= 1.0,
            "total={}",
            score.total
        );
        assert!(score.bond_richness > 0.0);
        assert!(score.dream_contribution > 0.0);
        assert!(score.fedr_participation > 0.0);
        assert_eq!(score.graph_health, 1.0);
    }

    // ── 14. Weights sum to 1.0 ──────────────────────────────────────────

    #[test]
    fn weights_sum_to_one() {
        let sum = W_BOND + W_DREAM + W_FEDR + W_HEALTH;
        assert!((sum - 1.0).abs() < 1e-9, "weights sum={sum}");
    }
}
