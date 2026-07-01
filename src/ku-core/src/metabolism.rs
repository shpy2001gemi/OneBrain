//! # KU Metabolism Tracker — PoK v2 Core Engine
//!
//! Tracks usage signals for each Knowledge Unit using CRDTs.
//! Every query, retrieval, citation, and interaction is recorded
//! as a monotonically increasing counter — observable, objective,
//! no voting needed.
//!
//! ## Design Philosophy (Founder decisions, 06/2026):
//! - Knowledge value = usage, NOT correctness
//! - No clawback — G-Counters only increase
//! - Refutation is POSITIVE (someone engaged)
//! - Corroborate/Challenge are optional signals
//! - Everything works LOCAL — each node tracks its own view

use crate::crdt::{GCounter, LWWRegister};
use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Weight for query hit velocity
pub const ALPHA_QUERY: f64 = 0.25;
/// Weight for retrieval depth (dwell time)
pub const ALPHA_RETRIEVAL: f64 = 0.20;
/// Weight for citation freshness
pub const ALPHA_CITATION: f64 = 0.25;
/// Weight for derivative novelty
pub const ALPHA_DERIVATIVE: f64 = 0.15;
/// Weight for downstream cascade
pub const ALPHA_DOWNSTREAM: f64 = 0.15;

/// Minimum metabolic rate to be considered "alive"
pub const METABOLISM_ALIVE_THRESHOLD: f64 = 0.001;

/// Default half-life for metabolic decay (30 days in seconds)
pub const DEFAULT_HALF_LIFE_SECS: u64 = 30 * 24 * 3600;

/// Ln(2) constant for exponential decay
const LN2: f64 = 0.693147180559945;

// ═══════════════════════════════════════════════════════════════════════════
// Metabolism Event Types
// ═══════════════════════════════════════════════════════════════════════════

/// Events that affect a KU's metabolism
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetabolismEvent {
    /// KU appeared in a query result
    QueryHit,
    /// KU was retrieved and read (with dwell time in ms)
    Retrieval { dwell_ms: u64 },
    /// Another KU cited this one
    Citation,
    /// A new KU was derived/inspired by this one
    Derivative,
    /// A downstream KU (that cited this) was also used
    DownstreamUsage,
    /// Someone explicitly corroborated (optional signal)
    Corroboration,
    /// Someone challenged/refuted (POSITIVE engagement!)
    Refutation,
}

// ═══════════════════════════════════════════════════════════════════════════
// KU Metabolism — CRDT-based usage tracker
// ═══════════════════════════════════════════════════════════════════════════

/// Per-KU metabolism tracker using grow-only CRDTs.
///
/// Each signal is tracked by a separate GCounter, enabling:
/// - Fully decentralized: each node increments its own slot
/// - Eventually consistent: GCounter merge = per-node max
/// - No clawback: counters only go up
/// - Observable: pure usage data, no subjective judgment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KUMetabolism {
    // === Consumption signals ===
    /// Number of times this KU appeared in query results
    pub query_hits: GCounter,
    /// Number of times this KU was fully retrieved/read
    pub retrieval_count: GCounter,
    /// Total dwell time across all reads (milliseconds)
    pub dwell_time_ms: GCounter,

    // === Transformation signals ===
    /// Number of other KUs that cite this one
    pub citation_count: GCounter,
    /// Number of KUs derived/inspired by this one
    pub derivative_count: GCounter,
    /// Number of refutations received (positive engagement!)
    pub refutation_count: GCounter,
    /// Number of explicit corroborations (optional signal)
    pub corroboration_count: GCounter,

    // === Excretion signals ===
    /// Usage of downstream KUs that cite this one
    pub downstream_usage: GCounter,

    // === Timestamps ===
    /// When this KU was created (epoch seconds, immutable)
    pub created_at: u64,
    /// Last activity timestamp (LWW — latest wins across nodes)
    pub last_activity: LWWRegister<u64>,

    // === Diversity tracking ===
    /// Number of unique nodes that have interacted
    pub unique_nodes: GCounter,
}

impl KUMetabolism {
    /// Create a new metabolism tracker for a KU.
    pub fn new(created_at: u64) -> Self {
        Self {
            query_hits: GCounter::new(),
            retrieval_count: GCounter::new(),
            dwell_time_ms: GCounter::new(),
            citation_count: GCounter::new(),
            derivative_count: GCounter::new(),
            refutation_count: GCounter::new(),
            corroboration_count: GCounter::new(),
            downstream_usage: GCounter::new(),
            created_at,
            last_activity: LWWRegister::new(created_at, created_at, 0),
            unique_nodes: GCounter::new(),
        }
    }

    /// Record a metabolism event from a specific node.
    pub fn record_event(&mut self, node_id: u64, event: MetabolismEvent, timestamp: u64) {
        // Track unique node interaction
        if self.unique_nodes.node_count(node_id) == 0 {
            // u64 overflow is unrealistic for node counts; safe to ignore.
            let _ = self.unique_nodes.increment(node_id);
        }

        // Update last activity
        self.last_activity.set(timestamp, timestamp, node_id);

        // Route to appropriate counter
        // Note: GCounter::increment returns Result for overflow protection.
        // u64::MAX overflow is unrealistic for per-event counters.
        match event {
            MetabolismEvent::QueryHit => {
                let _ = self.query_hits.increment(node_id);
            }
            MetabolismEvent::Retrieval { dwell_ms } => {
                let _ = self.retrieval_count.increment(node_id);
                let _ = self.dwell_time_ms.increment_by(node_id, dwell_ms);
            }
            MetabolismEvent::Citation => {
                let _ = self.citation_count.increment(node_id);
            }
            MetabolismEvent::Derivative => {
                let _ = self.derivative_count.increment(node_id);
            }
            MetabolismEvent::DownstreamUsage => {
                let _ = self.downstream_usage.increment(node_id);
            }
            MetabolismEvent::Corroboration => {
                let _ = self.corroboration_count.increment(node_id);
            }
            MetabolismEvent::Refutation => {
                // Refutation is POSITIVE — someone engaged!
                let _ = self.refutation_count.increment(node_id);
            }
        }
    }

    /// Compute the current metabolic rate with temporal decay.
    ///
    /// Formula: rate = raw_signal × decay(age, half_life)
    ///
    /// Where raw_signal = weighted sum of:
    ///   α₁ × query_velocity + α₂ × retrieval_depth +
    ///   α₃ × citation_freshness + α₄ × derivative_novelty +
    ///   α₅ × downstream_cascade
    pub fn metabolic_rate(&self, now: u64, half_life_secs: u64) -> f64 {
        let age_secs = now.saturating_sub(self.created_at);
        let half_life = if half_life_secs == 0 { DEFAULT_HALF_LIFE_SECS } else { half_life_secs };

        // Exponential decay: e^(-ln2 × age / half_life)
        let decay = (-LN2 * age_secs as f64 / half_life as f64).exp();

        // Raw signal components (normalized by node diversity)
        let diversity = (self.unique_nodes.value() as f64).max(1.0);
        let query_vel = self.query_hits.value() as f64 / diversity.sqrt();
        let retrieval_depth = self.retrieval_count.value() as f64
            * self.avg_dwell_seconds();
        let citation_fresh = self.citation_count.value() as f64;
        let derivative_nov = self.derivative_count.value() as f64;
        let downstream = self.downstream_usage.value() as f64;

        let raw = ALPHA_QUERY * query_vel
            + ALPHA_RETRIEVAL * retrieval_depth
            + ALPHA_CITATION * citation_fresh
            + ALPHA_DERIVATIVE * derivative_nov
            + ALPHA_DOWNSTREAM * downstream;

        raw * decay
    }

    /// Average dwell time per retrieval (in seconds).
    pub fn avg_dwell_seconds(&self) -> f64 {
        let retrievals = self.retrieval_count.value();
        if retrievals == 0 {
            return 0.0;
        }
        (self.dwell_time_ms.value() as f64 / retrievals as f64) / 1000.0
    }

    /// Total engagement score (all interactions).
    pub fn total_engagement(&self) -> u64 {
        self.query_hits.value()
            + self.retrieval_count.value()
            + self.citation_count.value()
            + self.derivative_count.value()
            + self.refutation_count.value()
            + self.corroboration_count.value()
            + self.downstream_usage.value()
    }

    /// Node diversity — how many unique nodes interacted.
    pub fn node_diversity(&self) -> usize {
        self.unique_nodes.value() as usize
    }

    /// Is this KU still metabolically active?
    pub fn is_alive(&self, now: u64, half_life_secs: u64) -> bool {
        self.metabolic_rate(now, half_life_secs) > METABOLISM_ALIVE_THRESHOLD
    }

    /// Merge with another KUMetabolism (from a remote node).
    ///
    /// All GCounters merge via per-node max — automatic convergence.
    pub fn merge(&mut self, other: &KUMetabolism) {
        self.query_hits.merge(&other.query_hits);
        self.retrieval_count.merge(&other.retrieval_count);
        self.dwell_time_ms.merge(&other.dwell_time_ms);
        self.citation_count.merge(&other.citation_count);
        self.derivative_count.merge(&other.derivative_count);
        self.refutation_count.merge(&other.refutation_count);
        self.corroboration_count.merge(&other.corroboration_count);
        self.downstream_usage.merge(&other.downstream_usage);
        self.unique_nodes.merge(&other.unique_nodes);
        self.last_activity.merge(&other.last_activity);
        // created_at is immutable — keep earliest
        self.created_at = self.created_at.min(other.created_at);
    }

    /// Convert metabolic rate to u16 [0, 10000] for TrustSection storage.
    ///
    /// Uses sigmoid normalization: 10000 × (1 - e^(-rate/10))
    pub fn rate_to_u16(&self, now: u64, half_life_secs: u64) -> u16 {
        let rate = self.metabolic_rate(now, half_life_secs);
        let normalized = 1.0 - (-rate / 10.0).exp();
        (normalized * 10000.0).min(10000.0) as u16
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const NODE_A: u64 = 1;
    const NODE_B: u64 = 2;
    const NODE_C: u64 = 3;
    const T0: u64 = 1_000_000;

    #[test]
    fn test_new_metabolism_starts_at_zero() {
        let m = KUMetabolism::new(T0);
        assert_eq!(m.total_engagement(), 0);
        assert_eq!(m.node_diversity(), 0);
        assert_eq!(m.metabolic_rate(T0, DEFAULT_HALF_LIFE_SECS), 0.0);
    }

    #[test]
    fn test_query_hit_increases_rate() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);

        let rate = m.metabolic_rate(T0 + 1, DEFAULT_HALF_LIFE_SECS);
        assert!(rate > 0.0, "Query hit should increase metabolic rate");
        assert_eq!(m.query_hits.value(), 1);
    }

    #[test]
    fn test_retrieval_with_dwell_time() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::Retrieval { dwell_ms: 5000 }, T0 + 1);

        assert_eq!(m.retrieval_count.value(), 1);
        assert_eq!(m.dwell_time_ms.value(), 5000);
        assert!((m.avg_dwell_seconds() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_citation_increases_rate() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::Citation, T0 + 1);
        m.record_event(NODE_B, MetabolismEvent::Citation, T0 + 2);
        m.record_event(NODE_C, MetabolismEvent::Citation, T0 + 3);

        assert_eq!(m.citation_count.value(), 3);
        let rate = m.metabolic_rate(T0 + 3, DEFAULT_HALF_LIFE_SECS);
        assert!(rate > 0.5, "3 citations should give significant rate: {}", rate);
    }

    #[test]
    fn test_refutation_is_positive() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::Refutation, T0 + 1);

        assert_eq!(m.refutation_count.value(), 1);
        assert_eq!(m.total_engagement(), 1, "Refutation counts as engagement");
    }

    #[test]
    fn test_metabolic_rate_decays_over_time() {
        let mut m = KUMetabolism::new(T0);
        // Add some activity
        for i in 0..10 {
            m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + i);
        }

        let rate_fresh = m.metabolic_rate(T0 + 10, DEFAULT_HALF_LIFE_SECS);
        let rate_after_half_life = m.metabolic_rate(T0 + DEFAULT_HALF_LIFE_SECS, DEFAULT_HALF_LIFE_SECS);

        // After one half-life, rate should be approximately halved
        let ratio = rate_after_half_life / rate_fresh;
        assert!(
            (ratio - 0.5).abs() < 0.05,
            "Rate should halve after one half-life: ratio = {}",
            ratio
        );
    }

    #[test]
    fn test_node_diversity_tracking() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 2); // same node
        m.record_event(NODE_B, MetabolismEvent::QueryHit, T0 + 3);
        m.record_event(NODE_C, MetabolismEvent::Citation, T0 + 4);

        assert_eq!(m.node_diversity(), 3, "3 unique nodes");
        assert_eq!(m.query_hits.value(), 3, "3 total queries");
    }

    #[test]
    fn test_merge_two_metabolisms() {
        let mut m1 = KUMetabolism::new(T0);
        m1.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);
        m1.record_event(NODE_A, MetabolismEvent::Citation, T0 + 2);

        let mut m2 = KUMetabolism::new(T0);
        m2.record_event(NODE_B, MetabolismEvent::QueryHit, T0 + 3);
        m2.record_event(NODE_B, MetabolismEvent::Retrieval { dwell_ms: 3000 }, T0 + 4);

        m1.merge(&m2);

        assert_eq!(m1.query_hits.value(), 2, "Merged queries: 1+1");
        assert_eq!(m1.citation_count.value(), 1, "Citations preserved");
        assert_eq!(m1.retrieval_count.value(), 1, "Retrievals merged");
        assert_eq!(m1.dwell_time_ms.value(), 3000, "Dwell merged");
        assert_eq!(m1.node_diversity(), 2, "2 unique nodes after merge");
    }

    #[test]
    fn test_merge_idempotent() {
        let mut m1 = KUMetabolism::new(T0);
        m1.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);

        let m1_clone = m1.clone();
        m1.merge(&m1_clone);

        assert_eq!(m1.query_hits.value(), 1, "Merge with self is idempotent");
    }

    #[test]
    fn test_is_alive_with_no_activity() {
        let m = KUMetabolism::new(T0);
        assert!(!m.is_alive(T0 + 1, DEFAULT_HALF_LIFE_SECS), "No activity = not alive");
    }

    #[test]
    fn test_is_alive_with_activity() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);
        assert!(m.is_alive(T0 + 1, DEFAULT_HALF_LIFE_SECS), "Activity = alive");
    }

    #[test]
    fn test_rate_to_u16_normalization() {
        let mut m = KUMetabolism::new(T0);
        // Add lots of activity
        for i in 0..100 {
            m.record_event(NODE_A + (i % 10), MetabolismEvent::QueryHit, T0 + i);
            m.record_event(NODE_A + (i % 10), MetabolismEvent::Citation, T0 + i);
        }

        let u16_val = m.rate_to_u16(T0 + 100, DEFAULT_HALF_LIFE_SECS);
        assert!(u16_val > 0, "Active KU should have non-zero u16 rate");
        assert!(u16_val <= 10000, "Rate should be capped at 10000");
    }

    #[test]
    fn test_avg_dwell_no_retrievals() {
        let m = KUMetabolism::new(T0);
        assert_eq!(m.avg_dwell_seconds(), 0.0, "No retrievals = 0 dwell");
    }

    #[test]
    fn test_total_engagement_counts_all() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0);
        m.record_event(NODE_A, MetabolismEvent::Retrieval { dwell_ms: 1000 }, T0);
        m.record_event(NODE_A, MetabolismEvent::Citation, T0);
        m.record_event(NODE_A, MetabolismEvent::Derivative, T0);
        m.record_event(NODE_A, MetabolismEvent::Refutation, T0);
        m.record_event(NODE_A, MetabolismEvent::Corroboration, T0);
        m.record_event(NODE_A, MetabolismEvent::DownstreamUsage, T0);

        assert_eq!(m.total_engagement(), 7, "All 7 event types counted");
    }

    #[test]
    fn test_merge_keeps_earliest_created_at() {
        let mut m1 = KUMetabolism::new(T0 + 100);
        let m2 = KUMetabolism::new(T0);

        m1.merge(&m2);
        assert_eq!(m1.created_at, T0, "Merge keeps earliest created_at");
    }

    #[test]
    fn test_zero_metabolism_after_very_long_time() {
        let mut m = KUMetabolism::new(T0);
        m.record_event(NODE_A, MetabolismEvent::QueryHit, T0 + 1);

        // After 10 half-lives, rate should be ~0
        let far_future = T0 + 10 * DEFAULT_HALF_LIFE_SECS;
        let rate = m.metabolic_rate(far_future, DEFAULT_HALF_LIFE_SECS);
        assert!(rate < 0.001, "After 10 half-lives, rate ≈ 0: {}", rate);
        assert!(!m.is_alive(far_future, DEFAULT_HALF_LIFE_SECS));
    }
}
