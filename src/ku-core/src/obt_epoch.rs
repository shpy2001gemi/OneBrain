//! OBT Epoch Boundary Settlement
//!
//! Manages epoch lifecycle: boundary computation, settlement aggregation,
//! and transition between epochs.
//!
//! Epoch = 1 hour (3,600 seconds). Epoch 0 starts at Unix epoch.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::obt_constants::OBT_EPOCH_DURATION_S;
use crate::obt_minting::compute_epoch_emission;

// ═══════════════════════════════════════════════════════════════════════════
// Free functions
// ═══════════════════════════════════════════════════════════════════════════

/// Compute `(start_ts, end_ts)` for a given epoch number.
pub fn compute_epoch_boundaries(epoch: u64) -> (u64, u64) {
    let start = epoch * OBT_EPOCH_DURATION_S;
    (start, start + OBT_EPOCH_DURATION_S)
}

/// Get epoch number from a Unix timestamp.
pub fn epoch_from_timestamp(ts: u64) -> u64 {
    ts / OBT_EPOCH_DURATION_S
}

/// Check if a timestamp falls within a given epoch (half-open `[start, end)`).
pub fn is_in_epoch(ts: u64, epoch: u64) -> bool {
    let (start, end) = compute_epoch_boundaries(epoch);
    ts >= start && ts < end
}

// ═══════════════════════════════════════════════════════════════════════════
// EpochSummary
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochSummary {
    pub epoch: u64,
    pub start_ts: u64,
    pub end_ts: u64,
    pub active_nodes: u32,
    pub total_emission: u64,
    pub total_minted: u64,
    pub avg_pomv_score: f64,
    pub storage_challenges_issued: u32,
    pub storage_challenges_passed: u32,
    pub forks_detected: u32,
    pub penalties_applied: u32,
}

impl EpochSummary {
    /// Create a default summary for the given epoch with zeroed counters.
    pub fn new(epoch: u64) -> Self {
        let (start_ts, end_ts) = compute_epoch_boundaries(epoch);
        Self {
            epoch,
            start_ts,
            end_ts,
            active_nodes: 0,
            total_emission: 0,
            total_minted: 0,
            avg_pomv_score: 0.0,
            storage_challenges_issued: 0,
            storage_challenges_passed: 0,
            forks_detected: 0,
            penalties_applied: 0,
        }
    }

    /// Fraction of storage challenges that passed.
    ///
    /// Returns `0.0` when no challenges were issued.
    pub fn challenge_pass_rate(&self) -> f64 {
        if self.storage_challenges_issued == 0 {
            return 0.0;
        }
        self.storage_challenges_passed as f64 / self.storage_challenges_issued as f64
    }

    /// `total_minted / total_emission`.
    ///
    /// Returns `0.0` when emission is zero.
    pub fn mint_utilization(&self) -> f64 {
        if self.total_emission == 0 {
            return 0.0;
        }
        self.total_minted as f64 / self.total_emission as f64
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Event types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintEvent {
    pub node_id: u64,
    pub amount: u64, // milliOBT
    pub source: MintEventSource,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MintEventSource {
    Pomv,           // R1
    EncoderReward,  // R2
    VerifierReward, // R3
    StorageReward,  // R4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResult {
    pub node_id: u64,
    pub passed: bool,
    pub timestamp: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// EpochAccumulator
// ═══════════════════════════════════════════════════════════════════════════

/// Collects events during an epoch and can be finalised into an [`EpochSummary`].
#[derive(Debug, Clone)]
pub struct EpochAccumulator {
    pub epoch: u64,
    pub active_node_ids: HashSet<u64>,
    pub mint_events: Vec<MintEvent>,
    pub challenge_results: Vec<ChallengeResult>,
    pub fork_count: u32,
    pub penalty_count: u32,
    pub pomv_scores: Vec<f64>,
}

impl EpochAccumulator {
    /// Create an empty accumulator for the given epoch.
    pub fn new(epoch: u64) -> Self {
        Self {
            epoch,
            active_node_ids: HashSet::new(),
            mint_events: Vec::new(),
            challenge_results: Vec::new(),
            fork_count: 0,
            penalty_count: 0,
            pomv_scores: Vec::new(),
        }
    }

    /// Record that a node was active during this epoch.
    pub fn record_active_node(&mut self, node_id: u64) {
        self.active_node_ids.insert(node_id);
    }

    /// Record a minting event.
    pub fn record_mint(&mut self, event: MintEvent) {
        self.mint_events.push(event);
    }

    /// Record a storage challenge result.
    pub fn record_challenge(&mut self, result: ChallengeResult) {
        self.challenge_results.push(result);
    }

    /// Increment the fork counter.
    pub fn record_fork(&mut self) {
        self.fork_count += 1;
    }

    /// Increment the penalty counter.
    pub fn record_penalty(&mut self) {
        self.penalty_count += 1;
    }

    /// Record a PoMV score observation.
    pub fn record_pomv(&mut self, score: f64) {
        self.pomv_scores.push(score);
    }

    /// Consume the accumulator and produce an immutable [`EpochSummary`].
    pub fn finalize(self) -> EpochSummary {
        let active_nodes = self.active_node_ids.len() as u32;

        let total_minted: u64 = self.mint_events.iter().map(|e| e.amount).sum();

        let avg_pomv_score = if self.pomv_scores.is_empty() {
            0.0
        } else {
            self.pomv_scores.iter().sum::<f64>() / self.pomv_scores.len() as f64
        };

        let total_emission = compute_epoch_emission(active_nodes, avg_pomv_score);

        let storage_challenges_issued = self.challenge_results.len() as u32;
        let storage_challenges_passed = self
            .challenge_results
            .iter()
            .filter(|c| c.passed)
            .count() as u32;

        let (start_ts, end_ts) = compute_epoch_boundaries(self.epoch);

        EpochSummary {
            epoch: self.epoch,
            start_ts,
            end_ts,
            active_nodes,
            total_emission,
            total_minted,
            avg_pomv_score,
            storage_challenges_issued,
            storage_challenges_passed,
            forks_detected: self.fork_count,
            penalties_applied: self.penalty_count,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Epoch boundary helpers ─────────────────────────────────────────

    #[test]
    fn test_epoch_boundary_computation() {
        assert_eq!(compute_epoch_boundaries(0), (0, 3_600));
        assert_eq!(compute_epoch_boundaries(1), (3_600, 7_200));
        assert_eq!(compute_epoch_boundaries(100), (360_000, 363_600));
    }

    #[test]
    fn test_epoch_from_timestamp() {
        assert_eq!(epoch_from_timestamp(0), 0);
        assert_eq!(epoch_from_timestamp(3_599), 0);
        assert_eq!(epoch_from_timestamp(3_600), 1);
        assert_eq!(epoch_from_timestamp(7_199), 1);
        assert_eq!(epoch_from_timestamp(7_200), 2);
    }

    #[test]
    fn test_is_in_epoch_inside() {
        assert!(is_in_epoch(100, 0));
        assert!(is_in_epoch(3_599, 0));
        assert!(is_in_epoch(3_600, 1));
    }

    #[test]
    fn test_is_in_epoch_outside() {
        assert!(!is_in_epoch(3_600, 0)); // boundary: start of next epoch
        assert!(!is_in_epoch(7_200, 1));
        assert!(!is_in_epoch(0, 1));
    }

    #[test]
    fn test_is_in_epoch_boundary() {
        // Start is inclusive, end is exclusive
        assert!(is_in_epoch(0, 0));       // start of epoch 0
        assert!(!is_in_epoch(3_600, 0));  // end of epoch 0 (exclusive)
        assert!(is_in_epoch(3_600, 1));   // start of epoch 1
    }

    // ── EpochSummary ──────────────────────────────────────────────────

    #[test]
    fn test_epoch_summary_new_defaults() {
        let s = EpochSummary::new(5);
        assert_eq!(s.epoch, 5);
        assert_eq!(s.start_ts, 18_000);
        assert_eq!(s.end_ts, 21_600);
        assert_eq!(s.active_nodes, 0);
        assert_eq!(s.total_emission, 0);
        assert_eq!(s.total_minted, 0);
        assert!((s.avg_pomv_score - 0.0).abs() < f64::EPSILON);
        assert_eq!(s.storage_challenges_issued, 0);
        assert_eq!(s.storage_challenges_passed, 0);
        assert_eq!(s.forks_detected, 0);
        assert_eq!(s.penalties_applied, 0);
    }

    #[test]
    fn test_challenge_pass_rate_zero_challenges() {
        let s = EpochSummary::new(0);
        assert!((s.challenge_pass_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_challenge_pass_rate_some() {
        let mut s = EpochSummary::new(0);
        s.storage_challenges_issued = 10;
        s.storage_challenges_passed = 7;
        assert!((s.challenge_pass_rate() - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_challenge_pass_rate_all_pass() {
        let mut s = EpochSummary::new(0);
        s.storage_challenges_issued = 5;
        s.storage_challenges_passed = 5;
        assert!((s.challenge_pass_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mint_utilization() {
        let mut s = EpochSummary::new(0);
        s.total_emission = 1_000;
        s.total_minted = 500;
        assert!((s.mint_utilization() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_mint_utilization_zero_emission() {
        let s = EpochSummary::new(0);
        assert!((s.mint_utilization() - 0.0).abs() < f64::EPSILON);
    }

    // ── EpochAccumulator ──────────────────────────────────────────────

    #[test]
    fn test_accumulator_records_events() {
        let mut acc = EpochAccumulator::new(1);
        acc.record_active_node(10);
        acc.record_active_node(20);
        acc.record_fork();
        acc.record_penalty();
        acc.record_pomv(0.8);

        acc.record_mint(MintEvent {
            node_id: 10,
            amount: 500,
            source: MintEventSource::Pomv,
            timestamp: 4_000,
        });

        acc.record_challenge(ChallengeResult {
            node_id: 10,
            passed: true,
            timestamp: 4_100,
        });

        assert_eq!(acc.active_node_ids.len(), 2);
        assert_eq!(acc.mint_events.len(), 1);
        assert_eq!(acc.challenge_results.len(), 1);
        assert_eq!(acc.fork_count, 1);
        assert_eq!(acc.penalty_count, 1);
        assert_eq!(acc.pomv_scores.len(), 1);
    }

    #[test]
    fn test_finalize_produces_correct_summary() {
        let mut acc = EpochAccumulator::new(2);
        acc.record_active_node(1);
        acc.record_active_node(2);
        acc.record_active_node(3);

        acc.record_pomv(0.6);
        acc.record_pomv(0.8);
        // avg = 0.7

        acc.record_mint(MintEvent {
            node_id: 1,
            amount: 100,
            source: MintEventSource::Pomv,
            timestamp: 7_300,
        });
        acc.record_mint(MintEvent {
            node_id: 2,
            amount: 200,
            source: MintEventSource::EncoderReward,
            timestamp: 7_400,
        });

        acc.record_challenge(ChallengeResult { node_id: 1, passed: true, timestamp: 7_500 });
        acc.record_challenge(ChallengeResult { node_id: 2, passed: false, timestamp: 7_600 });
        acc.record_challenge(ChallengeResult { node_id: 3, passed: true, timestamp: 7_700 });

        acc.record_fork();
        acc.record_fork();
        acc.record_penalty();

        let summary = acc.finalize();

        assert_eq!(summary.epoch, 2);
        assert_eq!(summary.active_nodes, 3);
        assert_eq!(summary.total_minted, 300);
        assert!((summary.avg_pomv_score - 0.7).abs() < f64::EPSILON);
        assert_eq!(summary.storage_challenges_issued, 3);
        assert_eq!(summary.storage_challenges_passed, 2);
        assert_eq!(summary.forks_detected, 2);
        assert_eq!(summary.penalties_applied, 1);

        // Emission should match compute_epoch_emission(3, 0.7)
        let expected_emission = compute_epoch_emission(3, 0.7);
        assert_eq!(summary.total_emission, expected_emission);
    }

    #[test]
    fn test_finalize_with_no_events() {
        let acc = EpochAccumulator::new(0);
        let summary = acc.finalize();

        assert_eq!(summary.active_nodes, 0);
        assert_eq!(summary.total_emission, 0); // 0 nodes, 0 pomv → 0
        assert_eq!(summary.total_minted, 0);
        assert!((summary.avg_pomv_score - 0.0).abs() < f64::EPSILON);
        assert_eq!(summary.storage_challenges_issued, 0);
        assert_eq!(summary.storage_challenges_passed, 0);
        assert_eq!(summary.forks_detected, 0);
        assert_eq!(summary.penalties_applied, 0);
    }

    #[test]
    fn test_multiple_mint_events_summed() {
        let mut acc = EpochAccumulator::new(0);
        acc.record_active_node(1);
        acc.record_pomv(1.0);

        for i in 0..10 {
            acc.record_mint(MintEvent {
                node_id: 1,
                amount: 100,
                source: MintEventSource::StorageReward,
                timestamp: i * 100,
            });
        }

        let summary = acc.finalize();
        assert_eq!(summary.total_minted, 1_000);
    }

    #[test]
    fn test_active_node_dedup() {
        let mut acc = EpochAccumulator::new(0);
        acc.record_active_node(42);
        acc.record_active_node(42);
        acc.record_active_node(42);
        acc.record_active_node(99);
        acc.record_pomv(0.5);

        let summary = acc.finalize();
        assert_eq!(summary.active_nodes, 2);
    }

    #[test]
    fn test_pomv_average() {
        let mut acc = EpochAccumulator::new(0);
        acc.record_active_node(1);
        acc.record_pomv(0.2);
        acc.record_pomv(0.4);
        acc.record_pomv(0.6);
        acc.record_pomv(0.8);
        // avg = (0.2 + 0.4 + 0.6 + 0.8) / 4 = 0.5

        let summary = acc.finalize();
        assert!((summary.avg_pomv_score - 0.5).abs() < f64::EPSILON);
    }
}
