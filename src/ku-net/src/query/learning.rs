//! # Pheromone Learning — Phase F2
//!
//! Reinforcement learning loop for the stigmergy-based query router.
//!
//! When a query succeeds via a particular path (scope/node), we
//! reinforce the pheromone trail. When it fails, we weaken it.
//! This allows the network to learn the best routes for specific
//! knowledge domains over time.
//!
//! ## Feedback Loop
//! ```text
//! Query Result → Score → Reinforce/Weaken → Stigmergy Table → Next Query
//! ```

use std::collections::HashMap;

use crate::identity::NodeId;
use ku_core::foundation::ConceptCcid;

use super::messages::QueryScope;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A record of a query's outcome for learning.
#[derive(Debug, Clone)]
pub struct QueryOutcome {
    /// The concept that was queried.
    pub concept: ConceptCcid,
    /// Which scope resolved the query.
    pub resolved_at: QueryScope,
    /// Node that provided the best result (if any).
    pub provider: Option<NodeId>,
    /// Quality of results [0.0, 1.0].
    pub quality: f64,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Number of results returned.
    pub result_count: usize,
}

/// Statistics for a concept-scope pair.
#[derive(Debug, Clone)]
pub struct RouteStats {
    /// Total queries routed this way.
    pub total_queries: u64,
    /// Successful queries (returned results).
    pub successes: u64,
    /// Average quality of results.
    pub avg_quality: f64,
    /// Average latency in ms.
    pub avg_latency_ms: f64,
    /// Current pheromone strength [0.0, 1.0].
    pub pheromone: f64,
}

impl RouteStats {
    fn new() -> Self {
        Self {
            total_queries: 0,
            successes: 0,
            avg_quality: 0.0,
            avg_latency_ms: 0.0,
            pheromone: 0.5, // Neutral start
        }
    }

    /// Success rate [0.0, 1.0].
    pub fn success_rate(&self) -> f64 {
        if self.total_queries == 0 {
            0.0
        } else {
            self.successes as f64 / self.total_queries as f64
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Pheromone Learner
// ═══════════════════════════════════════════════════════════════════════════

/// Learns optimal query routes from outcome feedback.
pub struct PheromoneLearner {
    /// Stats per (concept, scope) pair.
    route_stats: HashMap<(ConceptCcid, u8), RouteStats>,
    /// Learning rate for pheromone updates [0.0, 1.0].
    learning_rate: f64,
    /// Evaporation rate per time unit [0.0, 1.0].
    evaporation_rate: f64,
    /// Minimum pheromone level (prevents route starvation).
    min_pheromone: f64,
    /// Maximum pheromone level.
    max_pheromone: f64,
}

impl PheromoneLearner {
    /// Create with default parameters.
    pub fn new() -> Self {
        Self {
            route_stats: HashMap::new(),
            learning_rate: 0.1,
            evaporation_rate: 0.01,
            min_pheromone: 0.05,
            max_pheromone: 0.95,
        }
    }

    /// Create with custom parameters.
    pub fn with_params(
        learning_rate: f64,
        evaporation_rate: f64,
        min_pheromone: f64,
        max_pheromone: f64,
    ) -> Self {
        Self {
            route_stats: HashMap::new(),
            learning_rate,
            evaporation_rate,
            min_pheromone,
            max_pheromone,
        }
    }

    /// Record a query outcome and update pheromones.
    pub fn record_outcome(&mut self, outcome: &QueryOutcome) {
        let key = (outcome.concept, outcome.resolved_at as u8);
        let stats = self.route_stats.entry(key).or_insert_with(RouteStats::new);

        stats.total_queries += 1;

        if outcome.result_count > 0 {
            stats.successes += 1;
        }

        // Update running averages
        let n = stats.total_queries as f64;
        stats.avg_quality = stats.avg_quality * (n - 1.0) / n + outcome.quality / n;
        stats.avg_latency_ms = stats.avg_latency_ms * (n - 1.0) / n + outcome.latency_ms as f64 / n;

        // Update pheromone based on outcome quality
        let delta = if outcome.quality > 0.5 {
            // Positive reinforcement
            self.learning_rate * outcome.quality
        } else {
            // Negative reinforcement
            -self.learning_rate * (1.0 - outcome.quality) * 0.5
        };

        stats.pheromone = (stats.pheromone + delta).clamp(self.min_pheromone, self.max_pheromone);
    }

    /// Get the recommended scope for a concept based on learned routes.
    ///
    /// Returns scopes sorted by pheromone strength (best first).
    pub fn recommend_scopes(&self, concept: ConceptCcid) -> Vec<(QueryScope, f64)> {
        let mut scopes: Vec<(QueryScope, f64)> = [
            QueryScope::Local,
            QueryScope::Neighbors,
            QueryScope::Cluster,
            QueryScope::Dht,
            QueryScope::Semantic,
            QueryScope::Global,
        ]
        .iter()
        .map(|&scope| {
            let key = (concept, scope as u8);
            let pheromone = self
                .route_stats
                .get(&key)
                .map(|s| s.pheromone)
                .unwrap_or(0.5); // Default neutral
            (scope, pheromone)
        })
        .collect();

        scopes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scopes
    }

    /// Apply evaporation to all pheromone trails.
    ///
    /// Call this periodically (e.g., every minute) to allow
    /// the system to forget old routes and adapt to changes.
    pub fn evaporate(&mut self) {
        for stats in self.route_stats.values_mut() {
            // Decay toward neutral (0.5)
            let decay = (stats.pheromone - 0.5) * self.evaporation_rate;
            stats.pheromone =
                (stats.pheromone - decay).clamp(self.min_pheromone, self.max_pheromone);
        }
    }

    /// Get stats for a concept-scope pair.
    pub fn get_stats(&self, concept: ConceptCcid, scope: QueryScope) -> Option<&RouteStats> {
        self.route_stats.get(&(concept, scope as u8))
    }

    /// Number of tracked routes.
    pub fn route_count(&self) -> usize {
        self.route_stats.len()
    }

    /// Overall success rate across all routes.
    pub fn global_success_rate(&self) -> f64 {
        let total: u64 = self.route_stats.values().map(|s| s.total_queries).sum();
        let successes: u64 = self.route_stats.values().map(|s| s.successes).sum();
        if total == 0 {
            0.0
        } else {
            successes as f64 / total as f64
        }
    }
}

impl Default for PheromoneLearner {
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

    fn concept(value: u128) -> ConceptCcid {
        ConceptCcid::from_bytes(value.to_be_bytes())
    }

    fn success_outcome(value: u128, scope: QueryScope) -> QueryOutcome {
        QueryOutcome {
            concept: concept(value),
            resolved_at: scope,
            provider: None,
            quality: 0.9,
            latency_ms: 50,
            result_count: 5,
        }
    }

    fn failure_outcome(value: u128, scope: QueryScope) -> QueryOutcome {
        QueryOutcome {
            concept: concept(value),
            resolved_at: scope,
            provider: None,
            quality: 0.0,
            latency_ms: 500,
            result_count: 0,
        }
    }

    #[test]
    fn test_positive_reinforcement() {
        let mut learner = PheromoneLearner::new();
        let initial = 0.5_f64; // Default pheromone

        learner.record_outcome(&success_outcome(42, QueryScope::Dht));

        let stats = learner.get_stats(concept(42), QueryScope::Dht).unwrap();
        assert!(
            stats.pheromone > initial,
            "Success should increase pheromone"
        );
        assert_eq!(stats.successes, 1);
        assert_eq!(stats.total_queries, 1);
    }

    #[test]
    fn test_negative_reinforcement() {
        let mut learner = PheromoneLearner::new();

        learner.record_outcome(&failure_outcome(42, QueryScope::Global));

        let stats = learner.get_stats(concept(42), QueryScope::Global).unwrap();
        assert!(stats.pheromone < 0.5, "Failure should decrease pheromone");
    }

    #[test]
    fn test_recommend_scopes() {
        let mut learner = PheromoneLearner::new();

        // DHT is good for concept 42
        for _ in 0..5 {
            learner.record_outcome(&success_outcome(42, QueryScope::Dht));
        }
        // Global is bad
        for _ in 0..5 {
            learner.record_outcome(&failure_outcome(42, QueryScope::Global));
        }

        let recs = learner.recommend_scopes(concept(42));
        // DHT should be recommended first
        assert_eq!(recs[0].0, QueryScope::Dht);
        assert!(recs[0].1 > recs.last().unwrap().1);
    }

    #[test]
    fn test_evaporation() {
        let mut learner = PheromoneLearner::new();

        // Build up strong pheromone
        for _ in 0..10 {
            learner.record_outcome(&success_outcome(42, QueryScope::Dht));
        }

        let before = learner
            .get_stats(concept(42), QueryScope::Dht)
            .unwrap()
            .pheromone;

        // Evaporate
        for _ in 0..100 {
            learner.evaporate();
        }

        let after = learner
            .get_stats(concept(42), QueryScope::Dht)
            .unwrap()
            .pheromone;
        assert!(
            after < before,
            "Evaporation should decay pheromone toward neutral"
        );
    }

    #[test]
    fn test_pheromone_bounds() {
        let mut learner = PheromoneLearner::new();

        // Many successes should not exceed max
        for _ in 0..100 {
            learner.record_outcome(&success_outcome(1, QueryScope::Local));
        }
        let stats = learner.get_stats(concept(1), QueryScope::Local).unwrap();
        assert!(stats.pheromone <= 0.95, "Pheromone should not exceed max");

        // Many failures should not go below min
        for _ in 0..100 {
            learner.record_outcome(&failure_outcome(2, QueryScope::Global));
        }
        let stats = learner.get_stats(concept(2), QueryScope::Global).unwrap();
        assert!(stats.pheromone >= 0.05, "Pheromone should not go below min");
    }

    #[test]
    fn test_running_averages() {
        let mut learner = PheromoneLearner::new();

        learner.record_outcome(&QueryOutcome {
            concept: concept(1),
            resolved_at: QueryScope::Local,
            provider: None,
            quality: 0.8,
            latency_ms: 100,
            result_count: 3,
        });

        learner.record_outcome(&QueryOutcome {
            concept: concept(1),
            resolved_at: QueryScope::Local,
            provider: None,
            quality: 0.6,
            latency_ms: 200,
            result_count: 2,
        });

        let stats = learner.get_stats(concept(1), QueryScope::Local).unwrap();
        assert!(
            (stats.avg_quality - 0.7).abs() < 0.01,
            "Avg quality should be ~0.7"
        );
        assert!(
            (stats.avg_latency_ms - 150.0).abs() < 1.0,
            "Avg latency should be ~150ms"
        );
    }

    #[test]
    fn test_global_success_rate() {
        let mut learner = PheromoneLearner::new();

        learner.record_outcome(&success_outcome(1, QueryScope::Local));
        learner.record_outcome(&success_outcome(2, QueryScope::Dht));
        learner.record_outcome(&failure_outcome(3, QueryScope::Global));

        let rate = learner.global_success_rate();
        assert!((rate - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_success_rate() {
        let mut learner = PheromoneLearner::new();

        learner.record_outcome(&success_outcome(1, QueryScope::Local));
        learner.record_outcome(&failure_outcome(1, QueryScope::Local));

        let stats = learner.get_stats(concept(1), QueryScope::Local).unwrap();
        assert_eq!(stats.success_rate(), 0.5);
    }
}
