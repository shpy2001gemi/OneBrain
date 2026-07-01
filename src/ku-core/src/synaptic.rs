//! # Synaptic Bonds — Hebbian Learning Between KUs
//!
//! PoK v2 Signal #5: "Neurons that fire together, wire together."
//! When two KUs are co-retrieved/co-cited, the bond between them
//! strengthens. KUs with strong bonds to other valuable KUs
//! inherit network position value (synaptic centrality).
//!
//! ## Design:
//! - Each bond has a strength [0.0, 1.0]
//! - Co-retrieval: two KUs retrieved in the same session → strengthen
//! - Co-citation: KU_A and KU_B both cited by KU_C → strengthen
//! - Decay: bonds weaken without reinforcement (evaporation)
//! - Centrality: PageRank-like value based on bond network
//!
//! ## Reuse:
//! - PheromoneTable pattern from stigmergy.rs (reinforce/evaporate)
//! - Bond types from types.rs (RelationType)

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Initial bond strength for co-retrieval
pub const INITIAL_CO_RETRIEVAL_STRENGTH: f32 = 0.1;

/// Initial bond strength for co-citation
pub const INITIAL_CO_CITATION_STRENGTH: f32 = 0.15;

/// Reinforcement increment on repeated co-occurrence
pub const REINFORCE_INCREMENT: f32 = 0.05;

/// Maximum bond strength
pub const MAX_BOND_STRENGTH: f32 = 1.0;

/// Minimum bond strength (below this = garbage collect)
pub const MIN_BOND_STRENGTH: f32 = 0.001;

/// Evaporation rate per day (multiply by this)
pub const EVAPORATION_RATE: f32 = 0.95;

/// Maximum bonds per KU (to limit memory)
pub const MAX_BONDS_PER_KU: usize = 100;

/// Damping factor for centrality computation (PageRank-like)
pub const CENTRALITY_DAMPING: f32 = 0.85;

/// Number of iterations for centrality computation
pub const CENTRALITY_ITERATIONS: usize = 10;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Why two KUs are bonded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BondReason {
    /// Retrieved in the same session
    CoRetrieval,
    /// Both cited by the same KU
    CoCitation,
    /// Explicit relation (from RelationType bonds)
    ExplicitRelation,
}

/// A synaptic bond between two KUs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapticBond {
    /// CID of the bonded KU (the "other" end)
    pub target_cid: [u8; 32],
    /// Bond strength [0.0, 1.0]
    pub strength: f32,
    /// How many times reinforced
    pub reinforcement_count: u32,
    /// Why this bond exists
    pub reason: BondReason,
    /// Last reinforcement timestamp
    pub last_reinforced: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Synaptic Map — Per-KU bond tracker
// ═══════════════════════════════════════════════════════════════════════════

/// Tracks all synaptic bonds for a single KU.
#[derive(Debug, Clone, Default)]
pub struct SynapticMap {
    /// target_cid → bond
    bonds: HashMap<[u8; 32], SynapticBond>,
}

impl SynapticMap {
    pub fn new() -> Self {
        Self { bonds: HashMap::new() }
    }

    /// Reinforce or create a bond.
    pub fn reinforce(&mut self, target_cid: [u8; 32], reason: BondReason, timestamp: u64) {
        match self.bonds.get_mut(&target_cid) {
            Some(bond) => {
                bond.strength = (bond.strength + REINFORCE_INCREMENT).min(MAX_BOND_STRENGTH);
                bond.reinforcement_count += 1;
                bond.last_reinforced = bond.last_reinforced.max(timestamp);
            }
            None => {
                if self.bonds.len() >= MAX_BONDS_PER_KU {
                    self.evict_weakest();
                }
                let initial_strength = match reason {
                    BondReason::CoRetrieval => INITIAL_CO_RETRIEVAL_STRENGTH,
                    BondReason::CoCitation => INITIAL_CO_CITATION_STRENGTH,
                    BondReason::ExplicitRelation => 0.5, // Explicit = strong start
                };
                self.bonds.insert(target_cid, SynapticBond {
                    target_cid,
                    strength: initial_strength,
                    reinforcement_count: 1,
                    reason,
                    last_reinforced: timestamp,
                });
            }
        }
    }

    /// Evaporate all bond strengths (call periodically, e.g., daily).
    pub fn evaporate(&mut self) {
        self.bonds.retain(|_, bond| {
            bond.strength *= EVAPORATION_RATE;
            bond.strength >= MIN_BOND_STRENGTH
        });
    }

    /// Evict weakest bond to make room.
    fn evict_weakest(&mut self) {
        if let Some((&weakest_cid, _)) = self.bonds.iter()
            .min_by(|a, b| a.1.strength.partial_cmp(&b.1.strength).unwrap_or(std::cmp::Ordering::Equal))
        {
            self.bonds.remove(&weakest_cid);
        }
    }

    /// Get all bonds sorted by strength (strongest first).
    pub fn sorted_bonds(&self) -> Vec<&SynapticBond> {
        let mut bonds: Vec<_> = self.bonds.values().collect();
        bonds.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        bonds
    }

    /// Total bond strength (sum of all bonds).
    pub fn total_strength(&self) -> f32 {
        self.bonds.values().map(|b| b.strength).sum()
    }

    /// Number of active bonds.
    pub fn bond_count(&self) -> usize {
        self.bonds.len()
    }

    /// Get a specific bond.
    pub fn get_bond(&self, target_cid: &[u8; 32]) -> Option<&SynapticBond> {
        self.bonds.get(target_cid)
    }

    /// Merge with remote synaptic map (max-strength wins per bond).
    pub fn merge(&mut self, other: &SynapticMap) {
        for (cid, remote_bond) in &other.bonds {
            match self.bonds.get_mut(cid) {
                Some(local_bond) => {
                    // Max strength wins (CRDT-like)
                    if remote_bond.strength > local_bond.strength {
                        local_bond.strength = remote_bond.strength;
                    }
                    local_bond.reinforcement_count = local_bond.reinforcement_count
                        .max(remote_bond.reinforcement_count);
                    local_bond.last_reinforced = local_bond.last_reinforced
                        .max(remote_bond.last_reinforced);
                }
                None => {
                    if self.bonds.len() < MAX_BONDS_PER_KU {
                        self.bonds.insert(*cid, remote_bond.clone());
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Centrality Calculator
// ═══════════════════════════════════════════════════════════════════════════

/// Computes synaptic centrality for a set of KUs.
///
/// Uses a simplified PageRank-like algorithm:
/// Each KU distributes its score along bonds weighted by strength.
/// KUs with many strong bonds to other central KUs get high centrality.
pub struct CentralityCalculator;

impl CentralityCalculator {
    /// Compute centrality scores for a set of KUs.
    ///
    /// Input: CID → SynapticMap
    /// Output: CID → centrality [0.0, 1.0]
    ///
    /// Uses power iteration (10 rounds) which is sufficient
    /// for local neighborhood (not full network).
    pub fn compute(maps: &HashMap<[u8; 32], SynapticMap>) -> HashMap<[u8; 32], f32> {
        let n = maps.len();
        if n == 0 {
            return HashMap::new();
        }

        // Initialize uniform scores
        let initial_score = 1.0 / n as f32;
        let mut scores: HashMap<[u8; 32], f32> = maps.keys()
            .map(|&cid| (cid, initial_score))
            .collect();

        // Power iteration
        for _ in 0..CENTRALITY_ITERATIONS {
            let mut new_scores: HashMap<[u8; 32], f32> = maps.keys()
                .map(|&cid| (cid, (1.0 - CENTRALITY_DAMPING) / n as f32))
                .collect();

            for (&source_cid, map) in maps {
                let source_score = scores.get(&source_cid).copied().unwrap_or(0.0);
                let total_str = map.total_strength();
                if total_str == 0.0 {
                    continue;
                }

                for bond in map.bonds.values() {
                    if let Some(target_score) = new_scores.get_mut(&bond.target_cid) {
                        let contribution = CENTRALITY_DAMPING * source_score
                            * (bond.strength / total_str);
                        *target_score += contribution;
                    }
                }
            }

            scores = new_scores;
        }

        // Normalize to [0.0, 1.0]
        let max_score = scores.values().cloned().fold(0.0_f32, f32::max);
        if max_score > 0.0 {
            for score in scores.values_mut() {
                *score /= max_score;
            }
        }

        scores
    }

    /// Convert centrality score to u16 [0, 10000] for TrustSection.
    pub fn centrality_to_u16(centrality: f32) -> u16 {
        (centrality * 10000.0).clamp(0.0, 10000.0) as u16
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;

    fn test_cid(id: u8) -> [u8; 32] {
        let mut cid = [0u8; 32];
        cid[0] = id;
        cid
    }

    #[test]
    fn test_new_map_is_empty() {
        let map = SynapticMap::new();
        assert_eq!(map.bond_count(), 0);
        assert_eq!(map.total_strength(), 0.0);
    }

    #[test]
    fn test_reinforce_creates_bond() {
        let mut map = SynapticMap::new();
        map.reinforce(test_cid(2), BondReason::CoRetrieval, T0);

        assert_eq!(map.bond_count(), 1);
        let bond = map.get_bond(&test_cid(2)).unwrap();
        assert!((bond.strength - INITIAL_CO_RETRIEVAL_STRENGTH).abs() < 0.001);
        assert_eq!(bond.reinforcement_count, 1);
    }

    #[test]
    fn test_reinforce_strengthens_bond() {
        let mut map = SynapticMap::new();
        map.reinforce(test_cid(2), BondReason::CoRetrieval, T0);
        map.reinforce(test_cid(2), BondReason::CoRetrieval, T0 + 100);

        let bond = map.get_bond(&test_cid(2)).unwrap();
        let expected = INITIAL_CO_RETRIEVAL_STRENGTH + REINFORCE_INCREMENT;
        assert!((bond.strength - expected).abs() < 0.001);
        assert_eq!(bond.reinforcement_count, 2);
    }

    #[test]
    fn test_strength_capped_at_max() {
        let mut map = SynapticMap::new();
        for i in 0..100 {
            map.reinforce(test_cid(2), BondReason::CoRetrieval, T0 + i);
        }

        let bond = map.get_bond(&test_cid(2)).unwrap();
        assert!(bond.strength <= MAX_BOND_STRENGTH, "Strength capped: {}", bond.strength);
    }

    #[test]
    fn test_co_citation_stronger_initial() {
        let mut map = SynapticMap::new();
        map.reinforce(test_cid(2), BondReason::CoCitation, T0);

        let bond = map.get_bond(&test_cid(2)).unwrap();
        assert!(bond.strength > INITIAL_CO_RETRIEVAL_STRENGTH,
            "Co-citation starts stronger than co-retrieval");
    }

    #[test]
    fn test_evaporate_reduces_strength() {
        let mut map = SynapticMap::new();
        map.reinforce(test_cid(2), BondReason::CoRetrieval, T0);

        let before = map.get_bond(&test_cid(2)).unwrap().strength;
        map.evaporate();
        let after = map.get_bond(&test_cid(2)).unwrap().strength;

        assert!(after < before, "Evaporation reduces strength");
        assert!((after - before * EVAPORATION_RATE).abs() < 0.001);
    }

    #[test]
    fn test_evaporate_removes_dead_bonds() {
        let mut map = SynapticMap::new();
        map.reinforce(test_cid(2), BondReason::CoRetrieval, T0);

        // Evaporate many times until below threshold
        for _ in 0..200 {
            map.evaporate();
        }

        assert_eq!(map.bond_count(), 0, "Dead bonds removed after many evaporations");
    }

    #[test]
    fn test_sorted_bonds() {
        let mut map = SynapticMap::new();
        map.reinforce(test_cid(1), BondReason::CoRetrieval, T0); // weakest
        map.reinforce(test_cid(2), BondReason::ExplicitRelation, T0); // strongest (0.5)
        map.reinforce(test_cid(3), BondReason::CoCitation, T0); // medium

        let sorted = map.sorted_bonds();
        assert!(sorted[0].strength >= sorted[1].strength);
        assert!(sorted[1].strength >= sorted[2].strength);
    }

    #[test]
    fn test_max_bonds_evicts_weakest() {
        let mut map = SynapticMap::new();
        // Fill to max
        for i in 0..MAX_BONDS_PER_KU as u8 {
            map.reinforce(test_cid(i), BondReason::CoRetrieval, T0);
        }
        assert_eq!(map.bond_count(), MAX_BONDS_PER_KU);

        // One more should evict weakest
        map.reinforce(test_cid(255), BondReason::ExplicitRelation, T0);
        assert_eq!(map.bond_count(), MAX_BONDS_PER_KU);
    }

    #[test]
    fn test_merge_synaptic_maps() {
        let mut map1 = SynapticMap::new();
        map1.reinforce(test_cid(2), BondReason::CoRetrieval, T0);

        let mut map2 = SynapticMap::new();
        map2.reinforce(test_cid(3), BondReason::CoCitation, T0);

        map1.merge(&map2);
        assert_eq!(map1.bond_count(), 2, "Merged: both bonds present");
    }

    #[test]
    fn test_merge_max_strength_wins() {
        let mut map1 = SynapticMap::new();
        map1.reinforce(test_cid(2), BondReason::CoRetrieval, T0);

        let mut map2 = SynapticMap::new();
        // Reinforce multiple times to make stronger
        for i in 0..5 {
            map2.reinforce(test_cid(2), BondReason::CoRetrieval, T0 + i);
        }

        let remote_strength = map2.get_bond(&test_cid(2)).unwrap().strength;
        map1.merge(&map2);

        let merged_strength = map1.get_bond(&test_cid(2)).unwrap().strength;
        assert!((merged_strength - remote_strength).abs() < 0.001,
            "Max strength wins in merge");
    }

    #[test]
    fn test_centrality_empty() {
        let maps: HashMap<[u8; 32], SynapticMap> = HashMap::new();
        let scores = CentralityCalculator::compute(&maps);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_centrality_star_topology() {
        let mut maps: HashMap<[u8; 32], SynapticMap> = HashMap::new();

        // Center node (1) bonded to all others
        let mut center_map = SynapticMap::new();
        for i in 2..=5 {
            center_map.reinforce(test_cid(i), BondReason::CoCitation, T0);
        }
        maps.insert(test_cid(1), center_map);

        // Leaf nodes bonded only to center
        for i in 2..=5u8 {
            let mut leaf_map = SynapticMap::new();
            leaf_map.reinforce(test_cid(1), BondReason::CoCitation, T0);
            maps.insert(test_cid(i), leaf_map);
        }

        let scores = CentralityCalculator::compute(&maps);
        let center_score = scores.get(&test_cid(1)).copied().unwrap_or(0.0);

        // Center should have highest centrality
        for i in 2..=5 {
            let leaf_score = scores.get(&test_cid(i)).copied().unwrap_or(0.0);
            assert!(center_score >= leaf_score,
                "Center ({}) ≥ Leaf {} ({})", center_score, i, leaf_score);
        }
    }

    #[test]
    fn test_centrality_to_u16() {
        assert_eq!(CentralityCalculator::centrality_to_u16(0.0), 0);
        assert_eq!(CentralityCalculator::centrality_to_u16(1.0), 10000);
        assert_eq!(CentralityCalculator::centrality_to_u16(0.5), 5000);
    }
}
