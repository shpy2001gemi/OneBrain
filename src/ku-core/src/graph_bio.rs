//! # Graph Bio — Bio-Inspired Graph Mechanisms
//!
//! Three biologically-inspired mechanisms for knowledge graph dynamics:
//!
//! 1. **STDP (Spike-Timing-Dependent Plasticity)**: Bonds between KUs that are
//!    accessed in causal sequence get strengthened (LTP), while anti-causal
//!    sequences get weakened (LTD). Mimics neural synaptic plasticity.
//!
//! 2. **Memory Consolidation**: KUs that are frequently retrieved, have high
//!    PoMV scores, and rich bond networks get "consolidated" — promoted from
//!    working memory to long-term memory. Mimics hippocampal replay.
//!
//! 3. **Spreading Activation**: When a KU is accessed, activation energy
//!    spreads along its bonds, decaying with distance. Used for associative
//!    retrieval and serendipitous discovery.

use crate::types::RelationType;
use std::collections::{HashMap, HashSet};
use std::cmp::Ordering;

// ============================================================================
// 1. STDP Engine — Spike-Timing-Dependent Plasticity
// ============================================================================

/// STDP (Spike-Timing-Dependent Plasticity) engine.
///
/// When KU-A is accessed at time t₁ and KU-B (bonded to A) is accessed
/// at time t₂, the bond weight is updated based on Δt = t₂ - t₁:
/// - Δt > 0 (causal): LTP (Long-Term Potentiation) — strengthen bond
/// - Δt < 0 (anti-causal): LTD (Long-Term Depression) — weaken bond
///
/// Weight change: Δw = A± × w × exp(-|Δt| / τ)
#[derive(Debug, Clone)]
pub struct StdpEngine {
    /// LTP amplitude (positive, e.g. 0.1 = +10% max boost)
    pub a_plus: f64,
    /// LTD amplitude (negative, e.g. -0.05 = -5% max reduction)
    pub a_minus: f64,
    /// Time constant in seconds (e.g. 3600.0 = 1 hour)
    pub tau: f64,
}

impl Default for StdpEngine {
    fn default() -> Self {
        Self {
            a_plus: 0.1,
            a_minus: -0.05,
            tau: 3600.0,
        }
    }
}

impl StdpEngine {
    pub fn new(a_plus: f64, a_minus: f64, tau: f64) -> Self {
        Self { a_plus, a_minus, tau }
    }

    /// Compute weight change for a bond given time difference.
    ///
    /// `delta_t` = t_post - t_pre (positive = causal order)
    /// Returns new weight (clamped to [0, 10000])
    pub fn update_weight(&self, current_weight: u16, delta_t: f64) -> u16 {
        let base = current_weight as f64;
        let decay = (-delta_t.abs() / self.tau).exp();
        let change = if delta_t > 0.0 {
            // Causal: strengthen (LTP)
            base * self.a_plus * decay
        } else if delta_t < 0.0 {
            // Anti-causal: weaken (LTD)
            base * self.a_minus * decay
        } else {
            0.0 // Simultaneous: no change
        };
        let new_weight = (base + change).round();
        new_weight.clamp(0.0, 10000.0) as u16
    }

    /// Process a batch of co-access events.
    /// Returns list of STDP updates for weight changes.
    pub fn process_co_accesses(
        &self,
        accesses: &[CoAccess],
    ) -> Vec<StdpUpdate> {
        let mut updates = Vec::new();
        for access in accesses {
            let new_weight = self.update_weight(access.current_weight, access.delta_t);
            if new_weight != access.current_weight {
                updates.push(StdpUpdate {
                    source_cid: access.source_cid,
                    target_cid: access.target_cid,
                    relation: access.relation,
                    old_weight: access.current_weight,
                    new_weight,
                    delta_t: access.delta_t,
                });
            }
        }
        updates
    }
}

/// A co-access event between two bonded KUs.
#[derive(Debug, Clone)]
pub struct CoAccess {
    pub source_cid: [u8; 32],
    pub target_cid: [u8; 32],
    pub relation: RelationType,
    pub current_weight: u16,
    /// t_target - t_source in seconds (positive = causal)
    pub delta_t: f64,
}

/// Result of STDP weight update.
#[derive(Debug, Clone)]
pub struct StdpUpdate {
    pub source_cid: [u8; 32],
    pub target_cid: [u8; 32],
    pub relation: RelationType,
    pub old_weight: u16,
    pub new_weight: u16,
    pub delta_t: f64,
}

// ============================================================================
// 2. Memory Consolidation Engine
// ============================================================================

/// Memory consolidation eligibility scoring.
///
/// Inspired by hippocampal replay: KUs that are frequently accessed,
/// have high quality scores, and rich connection networks are promoted
/// from "working memory" (recent, volatile) to "long-term memory"
/// (established, decay-resistant).
#[derive(Debug, Clone)]
pub struct ConsolidationEngine {
    /// Weight for retrieval_count factor [0.0, 1.0]
    pub w_retrieval: f64,
    /// Weight for pomv_score factor [0.0, 1.0]
    pub w_pomv: f64,
    /// Weight for bond_count factor [0.0, 1.0]
    pub w_bonds: f64,
    /// Weight for age factor [0.0, 1.0]
    pub w_age: f64,
    /// Minimum age in hours before eligible for consolidation
    pub min_age_hours: f64,
}

impl Default for ConsolidationEngine {
    fn default() -> Self {
        Self {
            w_retrieval: 0.30,
            w_pomv: 0.35,
            w_bonds: 0.20,
            w_age: 0.15,
            min_age_hours: 24.0, // at least 1 day old
        }
    }
}

impl ConsolidationEngine {
    /// Score a KU for consolidation eligibility [0.0, 1.0].
    ///
    /// Higher score = more eligible for promotion to long-term memory.
    pub fn consolidation_score(
        &self,
        retrieval_count: u64,
        pomv_score: f64,
        bond_count: usize,
        age_hours: f64,
    ) -> f64 {
        if age_hours < self.min_age_hours {
            return 0.0; // Too young
        }

        // Normalize each factor to [0, 1]
        let retrieval_factor = (retrieval_count as f64 / 100.0).min(1.0); // saturates at 100 retrievals
        let pomv_factor = pomv_score.clamp(0.0, 1.0);
        let bond_factor = (bond_count as f64 / 20.0).min(1.0); // saturates at 20 bonds
        let age_factor = ((age_hours - self.min_age_hours) / (168.0 - self.min_age_hours))
            .clamp(0.0, 1.0); // matures over a week

        self.w_retrieval * retrieval_factor
            + self.w_pomv * pomv_factor
            + self.w_bonds * bond_factor
            + self.w_age * age_factor
    }

    /// Batch process: score multiple KUs, return those above threshold.
    pub fn find_consolidation_candidates(
        &self,
        candidates: &[ConsolidationCandidate],
        threshold: f64,
    ) -> Vec<ConsolidationResult> {
        candidates
            .iter()
            .filter_map(|c| {
                let score = self.consolidation_score(
                    c.retrieval_count,
                    c.pomv_score,
                    c.bond_count,
                    c.age_hours,
                );
                if score >= threshold {
                    Some(ConsolidationResult {
                        cid: c.cid,
                        score,
                        action: if score > 0.8 {
                            ConsolidationAction::PromoteToCore
                        } else {
                            ConsolidationAction::ReduceDecayRate
                        },
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Input data for consolidation scoring.
#[derive(Debug, Clone)]
pub struct ConsolidationCandidate {
    pub cid: [u8; 32],
    pub retrieval_count: u64,
    pub pomv_score: f64,
    pub bond_count: usize,
    pub age_hours: f64,
}

/// Result of consolidation scoring.
#[derive(Debug, Clone)]
pub struct ConsolidationResult {
    pub cid: [u8; 32],
    pub score: f64,
    pub action: ConsolidationAction,
}

/// Action to take after consolidation scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidationAction {
    /// Very high score: promote to "core knowledge" (immune to decay)
    PromoteToCore,
    /// Good score: reduce decay rate by one level
    ReduceDecayRate,
}

// ============================================================================
// 3. Spreading Activation
// ============================================================================

/// Spreading activation through the knowledge graph.
///
/// When a KU is accessed, activation energy spreads to connected KUs:
/// - Activation decays by `decay_factor` at each hop
/// - Stops when activation drops below `threshold` or `max_depth` reached
/// - Uses BFS with priority queue (highest activation first)
///
/// This is a PURE function — works with adjacency list, not GraphStorage.
///
/// # Arguments
/// * `start_cid` — CID of the initially activated KU
/// * `adjacency` — cid → [(neighbor_cid, weight)] adjacency list
/// * `max_depth` — maximum BFS depth
/// * `decay_factor` — activation multiplier per hop (e.g. 0.8)
/// * `threshold` — stop propagating when activation < this (e.g. 0.01)
///
/// # Returns
/// Sorted list of (cid, activation) pairs, descending by activation.
/// The start node is excluded from results.
pub fn spreading_activation(
    start_cid: &[u8; 32],
    adjacency: &HashMap<[u8; 32], Vec<([u8; 32], u16)>>, // cid -> [(neighbor, weight)]
    max_depth: usize,
    decay_factor: f64, // 0.8 typical
    threshold: f64,    // stop when activation < this (e.g. 0.01)
) -> Vec<([u8; 32], f64)> {
    let mut activations: HashMap<[u8; 32], f64> = HashMap::new();
    let mut visited: HashSet<[u8; 32]> = HashSet::new();

    // BFS queue: (cid, current_activation, current_depth)
    let mut queue: Vec<([u8; 32], f64, usize)> = vec![(*start_cid, 1.0, 0)];
    activations.insert(*start_cid, 1.0);

    while let Some((current, activation, depth)) = queue.pop() {
        if depth >= max_depth {
            continue;
        }
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current);

        if let Some(neighbors) = adjacency.get(&current) {
            for &(neighbor, weight) in neighbors {
                let weight_factor = weight as f64 / 10000.0; // normalize weight
                let spread = activation * decay_factor * weight_factor;
                if spread < threshold {
                    continue;
                }

                let entry = activations.entry(neighbor).or_insert(0.0);
                if spread > *entry {
                    *entry = spread;
                    queue.push((neighbor, spread, depth + 1));
                }
            }
        }
    }

    // Sort by activation level (descending), exclude start node
    let mut results: Vec<([u8; 32], f64)> = activations
        .into_iter()
        .filter(|(cid, _)| cid != start_cid)
        .collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    results
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RelationType;

    // ── Helper: make deterministic CIDs ──────────────────────────────────

    fn cid(tag: u8) -> [u8; 32] {
        let mut c = [0u8; 32];
        c[0] = tag;
        c
    }

    // ════════════════════════════════════════════════════════════════════
    // STDP Tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn stdp_default_params() {
        let engine = StdpEngine::default();
        assert_eq!(engine.a_plus, 0.1);
        assert_eq!(engine.a_minus, -0.05);
        assert_eq!(engine.tau, 3600.0);
    }

    #[test]
    fn stdp_causal_strengthens() {
        let engine = StdpEngine::default();
        // delta_t > 0 → causal → LTP → weight increases
        let new_w = engine.update_weight(5000, 100.0);
        assert!(
            new_w > 5000,
            "causal access should strengthen bond: got {new_w}"
        );
    }

    #[test]
    fn stdp_anticausal_weakens() {
        let engine = StdpEngine::default();
        // delta_t < 0 → anti-causal → LTD → weight decreases
        let new_w = engine.update_weight(5000, -100.0);
        assert!(
            new_w < 5000,
            "anti-causal access should weaken bond: got {new_w}"
        );
    }

    #[test]
    fn stdp_simultaneous_no_change() {
        let engine = StdpEngine::default();
        // delta_t == 0 → no change
        let new_w = engine.update_weight(5000, 0.0);
        assert_eq!(new_w, 5000, "simultaneous access should not change weight");
    }

    #[test]
    fn stdp_far_apart_small_change() {
        let engine = StdpEngine::default();
        // delta_t very large → exp(-|dt|/tau) ≈ 0 → minimal change
        let new_w = engine.update_weight(5000, 100_000.0); // ~28 hours >> tau=1hr
        // Change should be very small: exp(-100000/3600) ≈ 0
        let diff = (new_w as i32 - 5000).abs();
        assert!(
            diff <= 1,
            "far-apart access should produce minimal change: diff={diff}"
        );
    }

    #[test]
    fn stdp_weight_floor() {
        let engine = StdpEngine::new(0.1, -0.99, 3600.0);
        // Very aggressive LTD but weight should not go below 0
        let new_w = engine.update_weight(100, -1.0);
        assert!(new_w == 0 || new_w < 100, "weight should be reduced or zero");
        // Explicitly test with higher base and immediate timing
        let engine2 = StdpEngine::new(0.1, -2.0, 3600.0);
        let new_w2 = engine2.update_weight(5000, -1.0);
        assert!(
            new_w2 <= 5000,
            "weight should not increase with LTD: got {new_w2}"
        );
        // The clamp should ensure ≥ 0
        let new_w3 = engine2.update_weight(100, -1.0);
        assert!(new_w3 <= 100, "floor should prevent negative: got {new_w3}");
    }

    #[test]
    fn stdp_weight_cap() {
        let engine = StdpEngine::new(5.0, -0.05, 3600.0); // Very aggressive LTP
        let new_w = engine.update_weight(9000, 1.0);
        assert!(
            new_w <= 10000,
            "weight should not exceed 10000: got {new_w}"
        );
    }

    #[test]
    fn stdp_batch_processing() {
        let engine = StdpEngine::default();
        let accesses = vec![
            CoAccess {
                source_cid: cid(1),
                target_cid: cid(2),
                relation: RelationType::Extends,
                current_weight: 5000,
                delta_t: 60.0, // causal, close timing
            },
            CoAccess {
                source_cid: cid(3),
                target_cid: cid(4),
                relation: RelationType::Causes,
                current_weight: 5000,
                delta_t: -60.0, // anti-causal
            },
            CoAccess {
                source_cid: cid(5),
                target_cid: cid(6),
                relation: RelationType::PartOf,
                current_weight: 5000,
                delta_t: 0.0, // simultaneous → no update
            },
        ];
        let updates = engine.process_co_accesses(&accesses);
        // Simultaneous should produce no update, so we expect 2
        assert_eq!(updates.len(), 2, "should have 2 updates (not simultaneous)");
        // First should be strengthened
        assert!(
            updates[0].new_weight > updates[0].old_weight,
            "causal should strengthen"
        );
        // Second should be weakened
        assert!(
            updates[1].new_weight < updates[1].old_weight,
            "anti-causal should weaken"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // Consolidation Tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn consolidation_too_young() {
        let engine = ConsolidationEngine::default();
        // Age < 24 hours → score should be 0
        let score = engine.consolidation_score(100, 1.0, 20, 12.0);
        assert_eq!(score, 0.0, "too young for consolidation");
    }

    #[test]
    fn consolidation_low_activity() {
        let engine = ConsolidationEngine::default();
        // Low retrieval, low pomv, few bonds → low score
        let score = engine.consolidation_score(2, 0.1, 1, 48.0);
        assert!(
            score < 0.3,
            "low activity should produce low score: got {score}"
        );
    }

    #[test]
    fn consolidation_high_activity() {
        let engine = ConsolidationEngine::default();
        // Max everything → should approach 1.0
        let score = engine.consolidation_score(200, 1.0, 30, 200.0);
        assert!(
            score > 0.8,
            "high activity should produce high score: got {score}"
        );
    }

    #[test]
    fn consolidation_pomv_weight() {
        let engine = ConsolidationEngine::default();
        // pomv has highest weight (0.35)
        assert!(engine.w_pomv > engine.w_retrieval);
        assert!(engine.w_pomv > engine.w_bonds);
        assert!(engine.w_pomv > engine.w_age);
    }

    #[test]
    fn consolidation_find_candidates() {
        let engine = ConsolidationEngine::default();
        let candidates = vec![
            ConsolidationCandidate {
                cid: cid(1),
                retrieval_count: 200,
                pomv_score: 0.95,
                bond_count: 25,
                age_hours: 200.0,
            },
            ConsolidationCandidate {
                cid: cid(2),
                retrieval_count: 1,
                pomv_score: 0.05,
                bond_count: 0,
                age_hours: 25.0,
            },
            ConsolidationCandidate {
                cid: cid(3),
                retrieval_count: 50,
                pomv_score: 0.6,
                bond_count: 10,
                age_hours: 100.0,
            },
        ];
        let results = engine.find_consolidation_candidates(&candidates, 0.4);
        // cid(2) should be excluded (too low), others should pass
        assert!(results.len() >= 1, "should find at least 1 candidate");
        assert!(
            results.iter().all(|r| r.cid != cid(2)),
            "low-activity candidate should be excluded"
        );
    }

    #[test]
    fn consolidation_promote_to_core() {
        let engine = ConsolidationEngine::default();
        // Very high everything → score > 0.8 → PromoteToCore
        let candidates = vec![ConsolidationCandidate {
            cid: cid(1),
            retrieval_count: 200,
            pomv_score: 1.0,
            bond_count: 30,
            age_hours: 200.0,
        }];
        let results = engine.find_consolidation_candidates(&candidates, 0.0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, ConsolidationAction::PromoteToCore);
    }

    #[test]
    fn consolidation_reduce_decay() {
        let engine = ConsolidationEngine::default();
        // Moderate activity → 0.4 < score < 0.8 → ReduceDecayRate
        let candidates = vec![ConsolidationCandidate {
            cid: cid(1),
            retrieval_count: 30,
            pomv_score: 0.5,
            bond_count: 8,
            age_hours: 80.0,
        }];
        let results = engine.find_consolidation_candidates(&candidates, 0.0);
        assert_eq!(results.len(), 1);
        let score = results[0].score;
        assert!(
            score > 0.0 && score <= 0.8,
            "moderate score should be <= 0.8: got {score}"
        );
        assert_eq!(results[0].action, ConsolidationAction::ReduceDecayRate);
    }

    // ════════════════════════════════════════════════════════════════════
    // Spreading Activation Tests
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn spreading_empty_graph() {
        let adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        let results = spreading_activation(&cid(1), &adj, 5, 0.8, 0.01);
        assert!(results.is_empty(), "empty graph should produce no results");
    }

    #[test]
    fn spreading_single_hop() {
        let mut adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        adj.insert(cid(1), vec![(cid(2), 10000)]); // full-weight bond
        let results = spreading_activation(&cid(1), &adj, 5, 0.8, 0.01);
        assert_eq!(results.len(), 1, "should find 1 neighbor");
        assert_eq!(results[0].0, cid(2));
        // activation = 1.0 * 0.8 * (10000/10000) = 0.8
        assert!(
            (results[0].1 - 0.8).abs() < 1e-6,
            "activation should be ~0.8: got {}",
            results[0].1
        );
    }

    #[test]
    fn spreading_multi_hop() {
        let mut adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        adj.insert(cid(1), vec![(cid(2), 10000)]);
        adj.insert(cid(2), vec![(cid(3), 10000)]);
        let results = spreading_activation(&cid(1), &adj, 5, 0.8, 0.01);
        assert_eq!(results.len(), 2, "should find 2 nodes");
        // cid(2) activation = 0.8, cid(3) activation = 0.64
        let a2 = results.iter().find(|(c, _)| *c == cid(2)).unwrap().1;
        let a3 = results.iter().find(|(c, _)| *c == cid(3)).unwrap().1;
        assert!((a2 - 0.8).abs() < 1e-6, "cid(2) should be ~0.8: got {a2}");
        assert!(
            (a3 - 0.64).abs() < 1e-6,
            "cid(3) should be ~0.64: got {a3}"
        );
    }

    #[test]
    fn spreading_decay() {
        let mut adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        adj.insert(cid(1), vec![(cid(2), 10000)]);
        adj.insert(cid(2), vec![(cid(3), 10000)]);
        adj.insert(cid(3), vec![(cid(4), 10000)]);
        let results = spreading_activation(&cid(1), &adj, 5, 0.8, 0.01);
        // Each hop decays by 0.8: 0.8, 0.64, 0.512
        let activations: Vec<f64> = results.iter().map(|(_, a)| *a).collect();
        for i in 1..activations.len() {
            assert!(
                activations[i] < activations[i - 1],
                "deeper hops should have lower activation"
            );
        }
    }

    #[test]
    fn spreading_threshold() {
        let mut adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        adj.insert(cid(1), vec![(cid(2), 100)]); // very low weight: 100/10000 = 0.01
        adj.insert(cid(2), vec![(cid(3), 10000)]);
        // activation at cid(2) = 1.0 * 0.8 * 0.01 = 0.008 < threshold 0.01
        let results = spreading_activation(&cid(1), &adj, 5, 0.8, 0.01);
        assert!(
            results.is_empty(),
            "low-weight bond should not propagate past threshold"
        );
    }

    #[test]
    fn spreading_max_depth() {
        let mut adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        adj.insert(cid(1), vec![(cid(2), 10000)]);
        adj.insert(cid(2), vec![(cid(3), 10000)]);
        adj.insert(cid(3), vec![(cid(4), 10000)]);
        // max_depth=1: should only reach cid(2)
        let results = spreading_activation(&cid(1), &adj, 1, 0.8, 0.01);
        assert_eq!(results.len(), 1, "max_depth=1 should reach only 1 hop");
        assert_eq!(results[0].0, cid(2));
    }

    #[test]
    fn spreading_weighted() {
        let mut adj: HashMap<[u8; 32], Vec<([u8; 32], u16)>> = HashMap::new();
        adj.insert(
            cid(1),
            vec![
                (cid(2), 10000), // full weight
                (cid(3), 5000),  // half weight
            ],
        );
        let results = spreading_activation(&cid(1), &adj, 5, 0.8, 0.01);
        assert_eq!(results.len(), 2);
        let a2 = results.iter().find(|(c, _)| *c == cid(2)).unwrap().1;
        let a3 = results.iter().find(|(c, _)| *c == cid(3)).unwrap().1;
        // cid(2): 1.0 * 0.8 * 1.0 = 0.8
        // cid(3): 1.0 * 0.8 * 0.5 = 0.4
        assert!(
            (a2 - 0.8).abs() < 1e-6,
            "full-weight neighbor should get 0.8: got {a2}"
        );
        assert!(
            (a3 - 0.4).abs() < 1e-6,
            "half-weight neighbor should get 0.4: got {a3}"
        );
        // Higher weight → higher activation
        assert!(
            a2 > a3,
            "higher-weight bond should propagate more activation"
        );
    }
}
