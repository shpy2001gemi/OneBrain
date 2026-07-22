//! # PoMV — Proof-of-Metabolic-Value Aggregator
//!
//! The FINAL scoring engine: combines all 6 PoK v2 signals
//! into a single PoMV score for each KU.
//!
//! ## Weighted Formula:
//! ```text
//! PoMV = w₁×Metabolism + w₂×Prediction + w₃×Entropy +
//!        w₄×Survival + w₅×Synaptic + w₆×NicheFitness
//! ```
//!
//! ## Public name: Proof-of-Knowledge (PoK)
//! ## Internal name: Proof-of-Metabolic-Value (PoMV)

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Signal Weights (tunable)
// ═══════════════════════════════════════════════════════════════════════════

/// Default weight configuration
pub const DEFAULT_WEIGHTS: PomvWeights = PomvWeights {
    metabolism: 0.35,
    prediction: 0.15,
    entropy: 0.10,
    survival: 0.10,
    synaptic: 0.15,
    niche_fitness: 0.15,
};

/// Configurable signal weights (must sum to 1.0)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PomvWeights {
    pub metabolism: f32,
    pub prediction: f32,
    pub entropy: f32,
    pub survival: f32,
    pub synaptic: f32,
    pub niche_fitness: f32,
}

impl PomvWeights {
    /// Validate that weights sum to approximately 1.0
    pub fn is_valid(&self) -> bool {
        let sum = self.metabolism
            + self.prediction
            + self.entropy
            + self.survival
            + self.synaptic
            + self.niche_fitness;
        (sum - 1.0).abs() < 0.01
    }
}

impl Default for PomvWeights {
    fn default() -> Self {
        DEFAULT_WEIGHTS
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Signal Input
// ═══════════════════════════════════════════════════════════════════════════

/// All 6 PoMV signals for a single KU, normalized to [0.0, 1.0]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PomvSignals {
    /// Metabolic rate (from metabolism.rs)
    pub metabolism: f32,
    /// Prediction accuracy (from prediction.rs)
    pub prediction: f32,
    /// Entropy/novelty with decay (from entropy.rs)
    pub entropy: f32,
    /// Anti-fragile survival score (from immune.rs)
    pub survival: f32,
    /// Synaptic centrality (from synaptic.rs)
    pub synaptic: f32,
    /// Ecological niche fitness (from ecosystem.rs)
    pub niche_fitness: f32,
}

impl PomvSignals {
    /// Create from u16 values (as stored in TrustSection)
    pub fn from_u16(
        metabolism: u16,
        prediction: u16,
        entropy: u16,
        survival: u16,
        synaptic: u16,
        niche_fitness: u16,
    ) -> Self {
        Self {
            metabolism: metabolism as f32 / 10000.0,
            prediction: prediction as f32 / 10000.0,
            entropy: entropy as f32 / 10000.0,
            survival: survival as f32 / 10000.0,
            synaptic: synaptic as f32 / 10000.0,
            niche_fitness: niche_fitness as f32 / 10000.0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PoMV Score
// ═══════════════════════════════════════════════════════════════════════════

/// Computed PoMV score with breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomvScore {
    /// Final weighted score [0.0, 1.0]
    pub total: f32,
    /// Individual weighted contributions
    pub contributions: PomvContributions,
    /// Weights used for computation
    pub weights: PomvWeights,
}

/// Breakdown of how each signal contributed to the score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomvContributions {
    pub metabolism: f32,
    pub prediction: f32,
    pub entropy: f32,
    pub survival: f32,
    pub synaptic: f32,
    pub niche_fitness: f32,
}

// ═══════════════════════════════════════════════════════════════════════════
// PoMV Calculator
// ═══════════════════════════════════════════════════════════════════════════

/// Stateless PoMV calculator.
pub struct PomvCalculator;

impl PomvCalculator {
    /// Compute the PoMV score for a KU.
    pub fn compute(signals: &PomvSignals, weights: &PomvWeights) -> PomvScore {
        let contributions = PomvContributions {
            metabolism: weights.metabolism * signals.metabolism,
            prediction: weights.prediction * signals.prediction,
            entropy: weights.entropy * signals.entropy,
            survival: weights.survival * signals.survival,
            synaptic: weights.synaptic * signals.synaptic,
            niche_fitness: weights.niche_fitness * signals.niche_fitness,
        };

        let total = contributions.metabolism
            + contributions.prediction
            + contributions.entropy
            + contributions.survival
            + contributions.synaptic
            + contributions.niche_fitness;

        PomvScore {
            total: total.clamp(0.0, 1.0),
            contributions,
            weights: *weights,
        }
    }

    /// Compute with default weights.
    pub fn compute_default(signals: &PomvSignals) -> PomvScore {
        Self::compute(signals, &DEFAULT_WEIGHTS)
    }

    /// Convert PoMV score to OBToken reward amount.
    ///
    /// Simple linear: reward = pomv_score × max_reward_per_epoch
    pub fn to_reward(pomv_score: f32, max_reward_per_epoch: f64) -> f64 {
        pomv_score as f64 * max_reward_per_epoch
    }

    /// Rank a set of KUs by PoMV score (highest first).
    pub fn rank(scores: &mut [(usize, PomvScore)]) {
        scores.sort_by(|a, b| {
            b.1.total
                .partial_cmp(&a.1.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_weights_valid() {
        assert!(
            DEFAULT_WEIGHTS.is_valid(),
            "Default weights must sum to 1.0"
        );
    }

    #[test]
    fn test_zero_signals_zero_score() {
        let signals = PomvSignals::default();
        let score = PomvCalculator::compute_default(&signals);
        assert_eq!(score.total, 0.0);
    }

    #[test]
    fn test_max_signals_max_score() {
        let signals = PomvSignals {
            metabolism: 1.0,
            prediction: 1.0,
            entropy: 1.0,
            survival: 1.0,
            synaptic: 1.0,
            niche_fitness: 1.0,
        };
        let score = PomvCalculator::compute_default(&signals);
        assert!(
            (score.total - 1.0).abs() < 0.01,
            "All max = score 1.0: {}",
            score.total
        );
    }

    #[test]
    fn test_metabolism_dominant() {
        let signals = PomvSignals {
            metabolism: 1.0,
            prediction: 0.0,
            entropy: 0.0,
            survival: 0.0,
            synaptic: 0.0,
            niche_fitness: 0.0,
        };
        let score = PomvCalculator::compute_default(&signals);
        assert!((score.total - DEFAULT_WEIGHTS.metabolism).abs() < 0.001);
        assert!(score.contributions.metabolism > score.contributions.prediction);
    }

    #[test]
    fn test_contributions_sum_to_total() {
        let signals = PomvSignals {
            metabolism: 0.8,
            prediction: 0.6,
            entropy: 0.3,
            survival: 0.2,
            synaptic: 0.7,
            niche_fitness: 0.5,
        };
        let score = PomvCalculator::compute_default(&signals);
        let sum = score.contributions.metabolism
            + score.contributions.prediction
            + score.contributions.entropy
            + score.contributions.survival
            + score.contributions.synaptic
            + score.contributions.niche_fitness;
        assert!((sum - score.total).abs() < 0.001);
    }

    #[test]
    fn test_from_u16() {
        let signals = PomvSignals::from_u16(5000, 7000, 3000, 1000, 8000, 4000);
        assert!((signals.metabolism - 0.5).abs() < 0.001);
        assert!((signals.prediction - 0.7).abs() < 0.001);
        assert!((signals.synaptic - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_to_reward() {
        let reward = PomvCalculator::to_reward(0.5, 100.0);
        assert!((reward - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_rank_descending() {
        let s1 = PomvCalculator::compute_default(&PomvSignals {
            metabolism: 0.1,
            ..Default::default()
        });
        let s2 = PomvCalculator::compute_default(&PomvSignals {
            metabolism: 0.9,
            ..Default::default()
        });
        let s3 = PomvCalculator::compute_default(&PomvSignals {
            metabolism: 0.5,
            ..Default::default()
        });

        let mut ranked = vec![(0, s1), (1, s2), (2, s3)];
        PomvCalculator::rank(&mut ranked);

        assert_eq!(ranked[0].0, 1, "Highest metabolism first");
        assert_eq!(ranked[1].0, 2, "Medium second");
        assert_eq!(ranked[2].0, 0, "Lowest last");
    }

    #[test]
    fn test_custom_weights() {
        let weights = PomvWeights {
            metabolism: 1.0,
            prediction: 0.0,
            entropy: 0.0,
            survival: 0.0,
            synaptic: 0.0,
            niche_fitness: 0.0,
        };
        let signals = PomvSignals {
            metabolism: 0.5,
            prediction: 1.0, // Should be ignored
            ..Default::default()
        };
        let score = PomvCalculator::compute(&signals, &weights);
        assert!((score.total - 0.5).abs() < 0.001, "Only metabolism counted");
    }
}
