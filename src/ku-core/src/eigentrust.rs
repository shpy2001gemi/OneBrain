//! # EigenTrust — Node Reputation Scoring
//!
//! PoK v2: Computes node-level trust based on the quality of KUs
//! they produce. Nodes that consistently create high-PoMV KUs
//! earn higher reputation.
//!
//! ## Why Node-Level Trust?
//! - PoMV scores individual KUs, but we also need to assess SOURCE reliability
//! - High-trust nodes' new KUs get faster propagation (but NOT higher scores)
//! - EigenTrust is the established algorithm for decentralized reputation
//!
//! ## Algorithm:
//! - Each node starts with trust = 1/N (uniform)
//! - Trust is updated iteratively based on:
//!   - Average PoMV of KUs they published
//!   - Whether their KUs got quarantined (immune system)
//!   - Diversity of their contributions
//! - Converges after ~10 iterations

use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Damping factor (like PageRank)
pub const EIGENTRUST_DAMPING: f64 = 0.85;

/// Pre-trust weight for new/unknown nodes
pub const PRE_TRUST: f64 = 0.01;

/// Number of iterations for convergence
pub const EIGENTRUST_ITERATIONS: usize = 10;

/// Penalty multiplier for quarantined KUs
pub const QUARANTINE_PENALTY: f64 = 0.5;

/// Minimum trust score (never goes to absolute zero)
pub const MIN_TRUST: f64 = 0.001;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Summary statistics for a node's KU production
#[derive(Debug, Clone, Default)]
pub struct NodeProfile {
    /// Average PoMV score of this node's KUs [0.0, 1.0]
    pub avg_pomv: f64,
    /// Number of KUs published
    pub ku_count: usize,
    /// Number of quarantined KUs (immune flagged)
    pub quarantined_count: usize,
    /// Number of distinct niches contributed to
    pub niche_diversity: usize,
    /// Total metabolic rate across all their KUs
    pub total_metabolic_rate: f64,
}

/// Per-node trust assessment
#[derive(Debug, Clone)]
pub struct NodeTrust {
    pub node_id: u64,
    pub trust_score: f64,
    pub profile: NodeProfile,
}

// ═══════════════════════════════════════════════════════════════════════════
// EigenTrust Calculator
// ═══════════════════════════════════════════════════════════════════════════

/// Computes node-level trust scores based on KU quality.
pub struct EigenTrustCalculator;

impl EigenTrustCalculator {
    /// Compute local trust value for a single node based on its profile.
    ///
    /// Returns [0.0, 1.0]:
    /// - High avg_pomv + many KUs + low quarantine + diverse → high trust
    /// - Low avg_pomv or many quarantined → low trust
    pub fn local_trust(profile: &NodeProfile) -> f64 {
        if profile.ku_count == 0 {
            return PRE_TRUST; // New node = minimal pre-trust
        }

        // Base: average PoMV quality
        let mut trust = profile.avg_pomv;

        // Penalty for quarantined KUs
        let quarantine_ratio = profile.quarantined_count as f64 / profile.ku_count as f64;
        trust *= 1.0 - quarantine_ratio * QUARANTINE_PENALTY;

        // Bonus for diversity (diminishing returns)
        let diversity_bonus = (profile.niche_diversity as f64).sqrt() / 10.0;
        trust = (trust + diversity_bonus).min(1.0);

        trust.max(MIN_TRUST)
    }

    /// Compute global trust scores for a network of nodes.
    ///
    /// Uses simplified EigenTrust iteration:
    /// t(i)^{k+1} = α × Σ_j(c_ij × t(j)^k) + (1-α) × p(i)
    ///
    /// Where c_ij = node j's trust in node i (based on local_trust).
    /// p(i) = pre-trusted value (uniform).
    pub fn compute_global(profiles: &HashMap<u64, NodeProfile>) -> HashMap<u64, f64> {
        let n = profiles.len();
        if n == 0 {
            return HashMap::new();
        }

        // Pre-trusted: uniform distribution
        let pre_trust_val = 1.0 / n as f64;

        // Initial: local trust normalized
        let mut scores: HashMap<u64, f64> = profiles
            .iter()
            .map(|(&node_id, profile)| (node_id, Self::local_trust(profile)))
            .collect();

        // Normalize
        Self::normalize(&mut scores);

        // Power iteration
        for _ in 0..EIGENTRUST_ITERATIONS {
            let mut new_scores: HashMap<u64, f64> = HashMap::new();

            for &node_i in profiles.keys() {
                let local = Self::local_trust(&profiles[&node_i]);
                let weighted_sum: f64 = profiles
                    .iter()
                    .map(|(&node_j, _)| {
                        let c_ij = local; // Simplified: use own local trust as weight
                        let t_j = scores.get(&node_j).copied().unwrap_or(pre_trust_val);
                        c_ij * t_j
                    })
                    .sum();

                let new_trust =
                    EIGENTRUST_DAMPING * weighted_sum + (1.0 - EIGENTRUST_DAMPING) * pre_trust_val;

                new_scores.insert(node_i, new_trust.max(MIN_TRUST));
            }

            Self::normalize(&mut new_scores);
            scores = new_scores;
        }

        scores
    }

    /// Normalize scores so they sum to 1.0
    fn normalize(scores: &mut HashMap<u64, f64>) {
        let total: f64 = scores.values().sum();
        if total > 0.0 {
            for score in scores.values_mut() {
                *score /= total;
            }
        }
    }

    /// Convert node trust to u16 [0, 10000].
    ///
    /// Since normalized scores can be very small (1/N),
    /// we scale relative to the maximum.
    pub fn trust_to_u16(score: f64, max_score: f64) -> u16 {
        if max_score <= 0.0 {
            return 5000; // Neutral
        }
        let relative = (score / max_score).clamp(0.0, 1.0);
        (relative * 10000.0) as u16
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_node_pre_trust() {
        let profile = NodeProfile::default();
        let trust = EigenTrustCalculator::local_trust(&profile);
        assert!(
            (trust - PRE_TRUST).abs() < 0.001,
            "New node = pre-trust: {}",
            trust
        );
    }

    #[test]
    fn test_good_node_high_trust() {
        let profile = NodeProfile {
            avg_pomv: 0.8,
            ku_count: 50,
            quarantined_count: 0,
            niche_diversity: 5,
            total_metabolic_rate: 100.0,
        };
        let trust = EigenTrustCalculator::local_trust(&profile);
        assert!(trust > 0.7, "Good node = high trust: {}", trust);
    }

    #[test]
    fn test_spammer_low_trust() {
        let profile = NodeProfile {
            avg_pomv: 0.1,
            ku_count: 100,
            quarantined_count: 80, // 80% quarantined!
            niche_diversity: 1,
            total_metabolic_rate: 0.5,
        };
        let trust = EigenTrustCalculator::local_trust(&profile);
        assert!(trust < 0.2, "Spammer = low trust: {}", trust);
    }

    #[test]
    fn test_quarantine_penalty() {
        let clean = NodeProfile {
            avg_pomv: 0.5,
            ku_count: 10,
            quarantined_count: 0,
            niche_diversity: 3,
            total_metabolic_rate: 5.0,
        };
        let flagged = NodeProfile {
            quarantined_count: 5,
            ..clean.clone()
        };

        let clean_trust = EigenTrustCalculator::local_trust(&clean);
        let flagged_trust = EigenTrustCalculator::local_trust(&flagged);

        assert!(
            clean_trust > flagged_trust,
            "Clean ({}) > Flagged ({})",
            clean_trust,
            flagged_trust
        );
    }

    #[test]
    fn test_diversity_bonus() {
        let narrow = NodeProfile {
            avg_pomv: 0.5,
            ku_count: 10,
            quarantined_count: 0,
            niche_diversity: 1,
            total_metabolic_rate: 5.0,
        };
        let diverse = NodeProfile {
            niche_diversity: 10,
            ..narrow.clone()
        };

        let narrow_trust = EigenTrustCalculator::local_trust(&narrow);
        let diverse_trust = EigenTrustCalculator::local_trust(&diverse);

        assert!(
            diverse_trust > narrow_trust,
            "Diverse ({}) > Narrow ({})",
            diverse_trust,
            narrow_trust
        );
    }

    #[test]
    fn test_global_trust_empty() {
        let profiles: HashMap<u64, NodeProfile> = HashMap::new();
        let scores = EigenTrustCalculator::compute_global(&profiles);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_global_trust_uniform() {
        let mut profiles = HashMap::new();
        for i in 1..=5 {
            profiles.insert(
                i,
                NodeProfile {
                    avg_pomv: 0.5,
                    ku_count: 10,
                    quarantined_count: 0,
                    niche_diversity: 3,
                    total_metabolic_rate: 5.0,
                },
            );
        }

        let scores = EigenTrustCalculator::compute_global(&profiles);
        assert_eq!(scores.len(), 5);

        // All equal profiles → approximately equal trust
        let values: Vec<f64> = scores.values().cloned().collect();
        let max_diff = values
            .iter()
            .cloned()
            .fold(0.0_f64, |acc, v| acc.max((v - values[0]).abs()));
        assert!(
            max_diff < 0.01,
            "Equal profiles → equal trust, max_diff = {}",
            max_diff
        );
    }

    #[test]
    fn test_global_trust_sums_to_one() {
        let mut profiles = HashMap::new();
        profiles.insert(
            1,
            NodeProfile {
                avg_pomv: 0.9,
                ku_count: 50,
                quarantined_count: 0,
                niche_diversity: 8,
                total_metabolic_rate: 100.0,
            },
        );
        profiles.insert(
            2,
            NodeProfile {
                avg_pomv: 0.1,
                ku_count: 5,
                quarantined_count: 3,
                niche_diversity: 1,
                total_metabolic_rate: 0.5,
            },
        );

        let scores = EigenTrustCalculator::compute_global(&profiles);
        let sum: f64 = scores.values().sum();
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Global trust sums to 1.0: {}",
            sum
        );
    }

    #[test]
    fn test_trust_to_u16() {
        assert_eq!(EigenTrustCalculator::trust_to_u16(0.5, 1.0), 5000);
        assert_eq!(EigenTrustCalculator::trust_to_u16(1.0, 1.0), 10000);
        assert_eq!(EigenTrustCalculator::trust_to_u16(0.0, 1.0), 0);
    }
}
