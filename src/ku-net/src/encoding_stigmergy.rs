//! # Encoding Stigmergy — Pheromone-based Load Balancing for Encoding Jobs
//!
//! Implements stigmergy (indirect coordination through environmental signals)
//! to help AI verifiers choose which encoding jobs to work on.
//!
//! ## Design Principle
//! Jobs that wait longer and have fewer workers become more "attractive",
//! like pheromone trails that strengthen over time. This naturally distributes
//! work across all pending KUs without any central coordinator.
//!
//! ## Attractiveness Formula
//! ```text
//! attractiveness = (α × wait_time_hours) + (β × remaining_slots) + (γ × reward)
//!                  ────────────────────────────────────────────────────────────
//!                  (1 + activity_level)
//! ```

use serde::{Serialize, Deserialize};

// Import centralized constants from constants.rs
use crate::constants::{
    ENCODING_PHEROMONE_ALPHA_WAIT as ALPHA_WAIT,
    ENCODING_PHEROMONE_BETA_SLOTS as BETA_SLOTS,
    ENCODING_PHEROMONE_GAMMA_REWARD as GAMMA_REWARD,
    ENCODING_PHEROMONE_EVAPORATION as EVAPORATION_RATE,
};

// ═══════════════════════════════════════════════════════════════════════════
// Job Pheromone
// ═══════════════════════════════════════════════════════════════════════════

/// Pheromone signal for an encoding job — indicates how "attractive" it is.
///
/// Higher attractiveness = more likely a verifier should choose this job.
/// Activity pheromone evaporates over time, making neglected jobs stand out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPheromone {
    /// BLAKE3 hash of the raw text — identifies the job.
    pub raw_hash: [u8; 32],

    /// Current activity level (increases when verifiers claim, decays over time).
    pub activity_level: f32,

    /// How long the job has been waiting (in seconds since posted).
    pub waiting_seconds: u64,

    /// Remaining verifier slots.
    pub remaining_slots: u8,

    /// OBT reward per verifier.
    pub reward: u64,

    /// Size of the raw text (larger = more work = should pay more).
    pub raw_size_bytes: u32,

    /// Last time this pheromone was updated.
    pub last_updated: u64,
}

impl JobPheromone {
    /// Create from an encoding job's metadata.
    pub fn from_job(
        raw_hash: [u8; 32],
        remaining_slots: u8,
        reward: u64,
        raw_size_bytes: u32,
        posted_at: u64,
        now: u64,
    ) -> Self {
        Self {
            raw_hash,
            activity_level: 0.0,
            waiting_seconds: now.saturating_sub(posted_at),
            remaining_slots,
            reward,
            raw_size_bytes,
            last_updated: now,
        }
    }

    /// Update pheromone state: evaporate activity, update wait time.
    pub fn tick(&mut self, now: u64) {
        let elapsed_hours = (now.saturating_sub(self.last_updated)) as f32 / 3600.0;
        self.activity_level *= (1.0 - EVAPORATION_RATE).powf(elapsed_hours);
        self.waiting_seconds = now.saturating_sub(self.last_updated) + self.waiting_seconds;
        self.last_updated = now;
    }

    /// Record that a verifier has claimed this job (increases activity).
    pub fn record_claim(&mut self) {
        self.activity_level += 1.0;
        if self.remaining_slots > 0 {
            self.remaining_slots -= 1;
        }
    }

    /// Compute attractiveness score.
    ///
    /// Jobs that wait longer, have more open slots, and offer higher rewards
    /// are more attractive. High activity (many recent claims) dampens score
    /// to prevent stampede.
    pub fn attractiveness(&self) -> f32 {
        let wait_hours = self.waiting_seconds as f32 / 3600.0;
        let slots = self.remaining_slots as f32;
        let reward_normalized = (self.reward as f32).min(100.0) / 100.0;

        let numerator =
            ALPHA_WAIT * wait_hours.min(168.0) / 168.0  // Normalize to 1 week max
            + BETA_SLOTS * slots / 3.0                   // Normalize to max 3 slots
            + GAMMA_REWARD * reward_normalized;

        numerator / (1.0 + self.activity_level)
    }
}

/// Decide whether an AI verifier should claim a given job.
///
/// Returns an attractiveness score in [0.0, 1.0+].
/// Higher scores indicate the job is a better candidate for this verifier.
///
/// # Arguments
/// * `pheromone` — The job's pheromone signal
/// * `verifier_load` — How many jobs this verifier is currently working on
/// * `max_concurrent` — Maximum concurrent jobs a verifier should take
pub fn should_claim(
    pheromone: &JobPheromone,
    verifier_load: usize,
    max_concurrent: usize,
) -> f32 {
    if verifier_load >= max_concurrent {
        return 0.0; // Verifier is fully loaded
    }

    if pheromone.remaining_slots == 0 {
        return 0.0; // No slots available
    }

    let base = pheromone.attractiveness();

    // Load penalty: reduce attractiveness if verifier is busy
    let load_factor = 1.0 - (verifier_load as f32 / max_concurrent as f32);

    base * load_factor
}

/// Sort jobs by attractiveness (highest first).
pub fn rank_jobs(pheromones: &mut [JobPheromone]) {
    pheromones.sort_by(|a, b| {
        b.attractiveness()
            .partial_cmp(&a.attractiveness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pheromone(wait_secs: u64, slots: u8, reward: u64) -> JobPheromone {
        JobPheromone {
            raw_hash: [0xAA; 32],
            activity_level: 0.0,
            waiting_seconds: wait_secs,
            remaining_slots: slots,
            reward,
            raw_size_bytes: 2048,
            last_updated: 1000,
        }
    }

    #[test]
    fn test_attractiveness_increases_with_wait() {
        let short_wait = make_pheromone(3600, 3, 5);      // 1 hour
        let long_wait = make_pheromone(24 * 3600, 3, 5);   // 24 hours

        assert!(
            long_wait.attractiveness() > short_wait.attractiveness(),
            "Longer waiting should increase attractiveness"
        );
    }

    #[test]
    fn test_attractiveness_increases_with_reward() {
        let low_reward = make_pheromone(3600, 3, 1);
        let high_reward = make_pheromone(3600, 3, 50);

        assert!(
            high_reward.attractiveness() > low_reward.attractiveness(),
            "Higher reward should increase attractiveness"
        );
    }

    #[test]
    fn test_activity_dampens_attractiveness() {
        let mut quiet = make_pheromone(3600, 3, 5);
        let mut busy = make_pheromone(3600, 3, 5);
        busy.activity_level = 5.0;

        assert!(
            quiet.attractiveness() > busy.attractiveness(),
            "High activity should dampen attractiveness"
        );
    }

    #[test]
    fn test_should_claim_respects_load() {
        let p = make_pheromone(3600, 3, 5);

        let score_idle = should_claim(&p, 0, 3);
        let score_busy = should_claim(&p, 2, 3);
        let score_full = should_claim(&p, 3, 3);

        assert!(score_idle > score_busy);
        assert_eq!(score_full, 0.0);
    }

    #[test]
    fn test_should_claim_no_slots() {
        let p = make_pheromone(3600, 0, 5);
        assert_eq!(should_claim(&p, 0, 3), 0.0);
    }

    #[test]
    fn test_rank_jobs() {
        let mut jobs = vec![
            make_pheromone(3600, 1, 5),      // Low attractiveness
            make_pheromone(24 * 3600, 3, 50), // High attractiveness
            make_pheromone(7200, 2, 10),      // Medium
        ];

        rank_jobs(&mut jobs);

        assert!(
            jobs[0].attractiveness() >= jobs[1].attractiveness(),
            "Jobs should be sorted by attractiveness (highest first)"
        );
        assert!(
            jobs[1].attractiveness() >= jobs[2].attractiveness(),
            "Jobs should be sorted by attractiveness (highest first)"
        );
    }

    #[test]
    fn test_evaporation() {
        let mut p = make_pheromone(0, 3, 5);
        p.activity_level = 5.0;

        // 10 hours later
        p.tick(p.last_updated + 10 * 3600);

        assert!(
            p.activity_level < 5.0,
            "Activity should decay over time, got {}",
            p.activity_level
        );
    }

    #[test]
    fn test_record_claim() {
        let mut p = make_pheromone(3600, 3, 5);
        p.record_claim();
        assert_eq!(p.remaining_slots, 2);
        assert!(p.activity_level > 0.0);
    }
}
