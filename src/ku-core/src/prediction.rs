//! # Prediction Registry — Accuracy Tracking
//!
//! PoK v2 Signal #2: Tracks whether knowledge makes accurate predictions.
//! Knowledge that helps predict outcomes is objectively valuable.
//!
//! ## Design:
//! - Each KU can register predictions (implicit from content type)
//! - Predictions are resolved over time by observation
//! - Prediction score = correct / total × confidence
//! - Experience/Narrative KUs use NoResolution (founder Q2)
//!
//! ## Resolution Methods:
//! - Fact: "Water boils at 100°C" → TemporalConsistency (still true?)
//! - Procedure: "Do X to achieve Y" → UsageOutcome (users report success?)
//! - Hypothesis: "A causes B" → CrossReference (new evidence confirms?)
//! - Experience: "Đà Lạt đẹp" → NoResolution (pure metabolism)

use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// How a prediction should be resolved
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionMethod {
    /// Fact — still true over time? Auto-check periodically
    TemporalConsistency,
    /// Procedure — users report success after following steps
    UsageOutcome,
    /// Hypothesis — new evidence confirms/refutes
    CrossReference,
    /// Experience/Narrative — NO resolution, pure metabolism
    /// (Founder Q2: "không thể đúng sai, thuộc về trải nghiệm")
    NoResolution,
}

/// A registered prediction for a KU
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prediction {
    /// Hash of the prediction content (BLAKE3)
    pub predicate_hash: [u8; 32],
    /// When should this be resolved? (epoch seconds, 0 = no deadline)
    pub deadline: u64,
    /// How to resolve
    pub resolution_method: ResolutionMethod,
    /// When registered
    pub registered_at: u64,
}

/// Outcome of a resolved prediction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PredictionOutcome {
    /// Prediction was correct / procedure worked
    Confirmed,
    /// Prediction was wrong / procedure failed
    Refuted,
    /// Partial — some aspects correct, some wrong
    Partial { confidence: u16 }, // [0, 10000]
    /// Not enough data to resolve
    Inconclusive,
}

/// A resolved prediction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    /// Which prediction this resolves
    pub predicate_hash: [u8; 32],
    /// The outcome
    pub outcome: PredictionOutcome,
    /// Who resolved it (node_id)
    pub resolver_node: u64,
    /// When resolved
    pub resolved_at: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Prediction Registry
// ═══════════════════════════════════════════════════════════════════════════

/// Per-KU prediction tracker.
///
/// Tracks registered predictions and their resolutions.
/// Computes a prediction accuracy score [0.0, 1.0].
#[derive(Debug, Clone)]
pub struct PredictionRegistry {
    /// All registered predictions
    predictions: Vec<Prediction>,
    /// All resolutions received
    resolutions: Vec<Resolution>,
}

impl PredictionRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            predictions: Vec::new(),
            resolutions: Vec::new(),
        }
    }

    /// Register a new prediction.
    pub fn register_prediction(&mut self, prediction: Prediction) {
        // Don't register duplicates
        if !self.predictions.iter().any(|p| p.predicate_hash == prediction.predicate_hash) {
            self.predictions.push(prediction);
        }
    }

    /// Submit a resolution for a prediction.
    pub fn resolve(&mut self, resolution: Resolution) {
        // Only resolve if prediction exists
        if self.predictions.iter().any(|p| p.predicate_hash == resolution.predicate_hash) {
            // Don't add duplicate resolutions from same node
            if !self.resolutions.iter().any(|r|
                r.predicate_hash == resolution.predicate_hash
                && r.resolver_node == resolution.resolver_node
            ) {
                self.resolutions.push(resolution);
            }
        }
    }

    /// Compute prediction accuracy score.
    ///
    /// Score = weighted_correct / total_resolved
    ///
    /// - Confirmed = 1.0
    /// - Refuted = 0.0
    /// - Partial = confidence / 10000
    /// - Inconclusive = not counted
    /// - NoResolution predictions = not counted
    ///
    /// Returns [0.0, 1.0], or 0.5 (neutral) if no resolvable predictions.
    pub fn prediction_score(&self) -> f64 {
        let resolvable: Vec<_> = self.predictions.iter()
            .filter(|p| p.resolution_method != ResolutionMethod::NoResolution)
            .collect();

        if resolvable.is_empty() {
            return 0.5; // Neutral — no resolvable predictions (e.g., Experience KU)
        }

        let mut total_weight = 0.0_f64;
        let mut weighted_correct = 0.0_f64;

        for pred in &resolvable {
            // Find all resolutions for this prediction
            let pred_resolutions: Vec<_> = self.resolutions.iter()
                .filter(|r| r.predicate_hash == pred.predicate_hash)
                .collect();

            if pred_resolutions.is_empty() {
                continue; // Not yet resolved
            }

            // Average outcome across resolvers (diversity of opinion)
            let mut outcome_sum = 0.0_f64;
            let mut resolver_count = 0;

            for res in &pred_resolutions {
                match &res.outcome {
                    PredictionOutcome::Confirmed => {
                        outcome_sum += 1.0;
                        resolver_count += 1;
                    }
                    PredictionOutcome::Refuted => {
                        outcome_sum += 0.0;
                        resolver_count += 1;
                    }
                    PredictionOutcome::Partial { confidence } => {
                        outcome_sum += *confidence as f64 / 10000.0;
                        resolver_count += 1;
                    }
                    PredictionOutcome::Inconclusive => {
                        // Skip inconclusive
                    }
                }
            }

            if resolver_count > 0 {
                let avg_outcome = outcome_sum / resolver_count as f64;
                // More resolvers = higher confidence in the resolution
                let weight = (resolver_count as f64).sqrt();
                weighted_correct += avg_outcome * weight;
                total_weight += weight;
            }
        }

        if total_weight == 0.0 {
            return 0.5; // No decisive resolutions yet
        }

        weighted_correct / total_weight
    }

    /// Number of registered predictions.
    pub fn prediction_count(&self) -> usize {
        self.predictions.len()
    }

    /// Number of resolved predictions.
    pub fn resolution_count(&self) -> usize {
        // Count unique resolved predicates
        let mut resolved_hashes = std::collections::HashSet::new();
        for r in &self.resolutions {
            if !matches!(r.outcome, PredictionOutcome::Inconclusive) {
                resolved_hashes.insert(r.predicate_hash);
            }
        }
        resolved_hashes.len()
    }

    /// Number of resolvable predictions (excluding NoResolution).
    pub fn resolvable_count(&self) -> usize {
        self.predictions.iter()
            .filter(|p| p.resolution_method != ResolutionMethod::NoResolution)
            .count()
    }

    /// Convert prediction score to u16 [0, 10000] for TrustSection.
    pub fn score_to_u16(&self) -> u16 {
        (self.prediction_score() * 10000.0).clamp(0.0, 10000.0) as u16
    }

    /// Merge with another registry (from remote node).
    ///
    /// Simple union — dedup by predicate_hash + resolver_node.
    pub fn merge(&mut self, other: &PredictionRegistry) {
        for pred in &other.predictions {
            if !self.predictions.iter().any(|p| p.predicate_hash == pred.predicate_hash) {
                self.predictions.push(pred.clone());
            }
        }
        for res in &other.resolutions {
            if !self.resolutions.iter().any(|r|
                r.predicate_hash == res.predicate_hash
                && r.resolver_node == res.resolver_node
            ) {
                self.resolutions.push(res.clone());
            }
        }
    }
}

impl Default for PredictionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_000_000;
    const NODE_A: u64 = 1;
    const NODE_B: u64 = 2;

    fn make_hash(id: u8) -> [u8; 32] {
        let mut h = [0u8; 32];
        h[0] = id;
        h
    }

    fn make_prediction(id: u8, method: ResolutionMethod) -> Prediction {
        Prediction {
            predicate_hash: make_hash(id),
            deadline: T0 + 86400,
            resolution_method: method,
            registered_at: T0,
        }
    }

    fn make_resolution(id: u8, outcome: PredictionOutcome, node: u64) -> Resolution {
        Resolution {
            predicate_hash: make_hash(id),
            outcome,
            resolver_node: node,
            resolved_at: T0 + 1000,
        }
    }

    #[test]
    fn test_empty_registry() {
        let reg = PredictionRegistry::new();
        assert_eq!(reg.prediction_count(), 0);
        assert_eq!(reg.prediction_score(), 0.5); // Neutral
    }

    #[test]
    fn test_register_prediction() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        assert_eq!(reg.prediction_count(), 1);
        assert_eq!(reg.resolvable_count(), 1);
    }

    #[test]
    fn test_no_duplicate_predictions() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        assert_eq!(reg.prediction_count(), 1);
    }

    #[test]
    fn test_resolve_confirmed() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        reg.resolve(make_resolution(1, PredictionOutcome::Confirmed, NODE_A));

        let score = reg.prediction_score();
        assert!((score - 1.0).abs() < 0.01, "Confirmed = score 1.0: {}", score);
    }

    #[test]
    fn test_resolve_refuted() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::UsageOutcome));
        reg.resolve(make_resolution(1, PredictionOutcome::Refuted, NODE_A));

        let score = reg.prediction_score();
        assert!(score < 0.01, "Refuted = score ≈ 0.0: {}", score);
    }

    #[test]
    fn test_resolve_partial() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::CrossReference));
        reg.resolve(make_resolution(1, PredictionOutcome::Partial { confidence: 7000 }, NODE_A));

        let score = reg.prediction_score();
        assert!((score - 0.7).abs() < 0.01, "70% partial = score 0.7: {}", score);
    }

    #[test]
    fn test_experience_ku_no_resolution() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::NoResolution));
        assert_eq!(reg.resolvable_count(), 0);
        assert_eq!(reg.prediction_score(), 0.5, "Experience KU = neutral 0.5");
    }

    #[test]
    fn test_multiple_resolvers() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));

        // Two nodes confirm
        reg.resolve(make_resolution(1, PredictionOutcome::Confirmed, NODE_A));
        reg.resolve(make_resolution(1, PredictionOutcome::Confirmed, NODE_B));

        let score = reg.prediction_score();
        assert!((score - 1.0).abs() < 0.01, "Both confirm = 1.0: {}", score);
    }

    #[test]
    fn test_mixed_resolutions() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        reg.register_prediction(make_prediction(2, ResolutionMethod::UsageOutcome));

        reg.resolve(make_resolution(1, PredictionOutcome::Confirmed, NODE_A));
        reg.resolve(make_resolution(2, PredictionOutcome::Refuted, NODE_A));

        let score = reg.prediction_score();
        assert!((score - 0.5).abs() < 0.1, "1 confirmed + 1 refuted ≈ 0.5: {}", score);
    }

    #[test]
    fn test_merge_registries() {
        let mut reg1 = PredictionRegistry::new();
        reg1.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));

        let mut reg2 = PredictionRegistry::new();
        reg2.register_prediction(make_prediction(2, ResolutionMethod::UsageOutcome));
        reg2.resolve(make_resolution(2, PredictionOutcome::Confirmed, NODE_B));

        reg1.merge(&reg2);
        assert_eq!(reg1.prediction_count(), 2);
        assert_eq!(reg1.resolution_count(), 1);
    }

    #[test]
    fn test_score_to_u16() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        reg.resolve(make_resolution(1, PredictionOutcome::Confirmed, NODE_A));

        assert_eq!(reg.score_to_u16(), 10000);
    }

    #[test]
    fn test_unresolved_stays_neutral() {
        let mut reg = PredictionRegistry::new();
        reg.register_prediction(make_prediction(1, ResolutionMethod::TemporalConsistency));
        // No resolution yet
        assert_eq!(reg.prediction_score(), 0.5, "Unresolved = neutral");
    }
}
