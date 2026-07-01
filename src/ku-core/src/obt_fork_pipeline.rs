//! OBT Fork Detection → Penalty Pipeline
//!
//! When a fork is detected (two blocks with same sequence for same account),
//! a ForkWarrant is created and processed through this pipeline.
//!
//! Lifecycle: Detected → Verified → PenaltyApplied → (optionally Disputed → Resolved)
//!
//! See `docs/specs/obt/08_PENALTY.md`.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarrantStatus {
    Detected,
    Verified,
    PenaltyApplied,
    Disputed,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkWarrantRecord {
    pub warrant_hash: [u8; 32],
    pub offender: [u8; 32],
    pub block_a_hash: [u8; 32],
    pub block_b_hash: [u8; 32],
    pub sequence: u64,
    pub detected_by: [u8; 32],
    pub detected_at: u64,
    pub status: WarrantStatus,
    pub witnesses: Vec<[u8; 32]>,
    pub penalty_tier: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PipelineError {
    WarrantNotFound,
    InvalidTransition {
        from: WarrantStatus,
        to: WarrantStatus,
    },
    DuplicateWitness,
    AlreadyProcessed,
}

#[derive(Debug, Clone)]
pub struct PenaltyAction {
    pub offender: [u8; 32],
    pub tier: u8,
    pub trust_factor: f64, // trust is multiplied by this
    pub jail_days: Option<u32>,
}

/// Minimum witnesses required before a warrant can be verified.
pub const MIN_WARRANT_WITNESSES: usize = 3;

// ═══════════════════════════════════════════════════════════════════════════
// Pipeline
// ═══════════════════════════════════════════════════════════════════════════

pub struct ForkPipeline {
    pending: Vec<ForkWarrantRecord>,
    processed: Vec<ForkWarrantRecord>,
}

impl ForkPipeline {
    /// Create an empty pipeline.
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            processed: Vec::new(),
        }
    }

    /// Submit a new fork warrant.
    ///
    /// Computes `warrant_hash = BLAKE3(offender ‖ block_a ‖ block_b ‖ sequence.to_le_bytes())`
    /// and creates a record with `Detected` status in the pending queue.
    pub fn submit_warrant(
        &mut self,
        offender: [u8; 32],
        block_a_hash: [u8; 32],
        block_b_hash: [u8; 32],
        sequence: u64,
        detected_by: [u8; 32],
        timestamp: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&offender);
        hasher.update(&block_a_hash);
        hasher.update(&block_b_hash);
        hasher.update(&sequence.to_le_bytes());
        let warrant_hash: [u8; 32] = *hasher.finalize().as_bytes();

        let record = ForkWarrantRecord {
            warrant_hash,
            offender,
            block_a_hash,
            block_b_hash,
            sequence,
            detected_by,
            detected_at: timestamp,
            status: WarrantStatus::Detected,
            witnesses: Vec::new(),
            penalty_tier: None,
        };
        self.pending.push(record);
        warrant_hash
    }

    /// Add a witness attestation to a pending warrant.
    ///
    /// The warrant must be in `Detected` status and the witness must not
    /// already be listed.
    pub fn add_witness(
        &mut self,
        warrant_hash: [u8; 32],
        witness: [u8; 32],
    ) -> Result<(), PipelineError> {
        let record = self
            .pending
            .iter_mut()
            .find(|r| r.warrant_hash == warrant_hash)
            .ok_or(PipelineError::WarrantNotFound)?;

        if record.status != WarrantStatus::Detected {
            return Err(PipelineError::InvalidTransition {
                from: record.status,
                to: WarrantStatus::Detected,
            });
        }

        if record.witnesses.contains(&witness) {
            return Err(PipelineError::DuplicateWitness);
        }

        record.witnesses.push(witness);
        Ok(())
    }

    /// Check if a warrant has accumulated enough witnesses for verification.
    ///
    /// If `witnesses.len() >= min_witnesses`, transitions status to `Verified`
    /// and returns `Ok(true)`. Otherwise returns `Ok(false)`.
    pub fn check_verification(
        &mut self,
        warrant_hash: [u8; 32],
        min_witnesses: usize,
    ) -> Result<bool, PipelineError> {
        let record = self
            .pending
            .iter_mut()
            .find(|r| r.warrant_hash == warrant_hash)
            .ok_or(PipelineError::WarrantNotFound)?;

        if record.status != WarrantStatus::Detected {
            return Err(PipelineError::InvalidTransition {
                from: record.status,
                to: WarrantStatus::Verified,
            });
        }

        if record.witnesses.len() >= min_witnesses {
            record.status = WarrantStatus::Verified;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Determine the penalty tier for an offender based on prior offenses.
    ///
    /// | Prior PenaltyApplied | Tier |
    /// |----------------------|------|
    /// | 0                    | 2    |
    /// | 1                    | 3    |
    /// | 2+                   | 4    |
    pub fn determine_penalty(&self, offender: &[u8; 32]) -> u8 {
        let prior = self
            .processed
            .iter()
            .filter(|r| {
                r.offender == *offender && r.status == WarrantStatus::PenaltyApplied
            })
            .count();
        match prior {
            0 => 2,
            1 => 3,
            _ => 4,
        }
    }

    /// Apply penalty to a verified warrant.
    ///
    /// Transitions `Verified → PenaltyApplied`, moves the warrant from
    /// pending to processed, and returns the [`PenaltyAction`].
    pub fn apply_penalty(
        &mut self,
        warrant_hash: [u8; 32],
    ) -> Result<PenaltyAction, PipelineError> {
        let idx = self
            .pending
            .iter()
            .position(|r| r.warrant_hash == warrant_hash)
            .ok_or(PipelineError::WarrantNotFound)?;

        if self.pending[idx].status != WarrantStatus::Verified {
            return Err(PipelineError::InvalidTransition {
                from: self.pending[idx].status,
                to: WarrantStatus::PenaltyApplied,
            });
        }

        let tier = self.determine_penalty(&self.pending[idx].offender);

        let (trust_factor, jail_days) = match tier {
            2 => (0.7, None),
            3 => (0.2, Some(7)),
            4 => (0.001, Some(180)),
            _ => unreachable!("determine_penalty only returns 2, 3, or 4"),
        };

        let mut record = self.pending.remove(idx);
        record.status = WarrantStatus::PenaltyApplied;
        record.penalty_tier = Some(tier);

        let action = PenaltyAction {
            offender: record.offender,
            tier,
            trust_factor,
            jail_days,
        };

        self.processed.push(record);
        Ok(action)
    }

    /// File an appeal against a warrant that has been penalised.
    ///
    /// Transitions `PenaltyApplied → Disputed`.
    pub fn file_appeal(
        &mut self,
        warrant_hash: [u8; 32],
    ) -> Result<(), PipelineError> {
        let record = self
            .processed
            .iter_mut()
            .find(|r| r.warrant_hash == warrant_hash)
            .ok_or(PipelineError::WarrantNotFound)?;

        if record.status != WarrantStatus::PenaltyApplied {
            return Err(PipelineError::InvalidTransition {
                from: record.status,
                to: WarrantStatus::Disputed,
            });
        }

        record.status = WarrantStatus::Disputed;
        Ok(())
    }

    /// Return all warrants (pending + processed) for a given offender.
    pub fn warrants_for(&self, offender: &[u8; 32]) -> Vec<&ForkWarrantRecord> {
        self.pending
            .iter()
            .chain(self.processed.iter())
            .filter(|r| r.offender == *offender)
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers ────────────────────────────────────────────────────────────

    fn id(n: u8) -> [u8; 32] {
        let mut arr = [0u8; 32];
        arr[0] = n;
        arr
    }

    fn submit_default(pipe: &mut ForkPipeline) -> [u8; 32] {
        pipe.submit_warrant(id(1), id(10), id(11), 42, id(99), 1_000)
    }

    fn add_n_witnesses(pipe: &mut ForkPipeline, hash: [u8; 32], n: u8) {
        for i in 0..n {
            // Witnesses start at id(200) to avoid collision with other ids
            pipe.add_witness(hash, id(200 + i)).unwrap();
        }
    }

    fn verify_warrant(pipe: &mut ForkPipeline, hash: [u8; 32]) {
        add_n_witnesses(pipe, hash, MIN_WARRANT_WITNESSES as u8);
        assert!(pipe.check_verification(hash, MIN_WARRANT_WITNESSES).unwrap());
    }

    // Tests ─────────────────────────────────────────────────────────────

    #[test]
    fn test_submit_and_retrieve_warrant() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);

        let warrants = pipe.warrants_for(&id(1));
        assert_eq!(warrants.len(), 1);
        assert_eq!(warrants[0].warrant_hash, hash);
        assert_eq!(warrants[0].offender, id(1));
        assert_eq!(warrants[0].sequence, 42);
        assert_eq!(warrants[0].status, WarrantStatus::Detected);
    }

    #[test]
    fn test_add_witnesses_no_duplicates() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);

        pipe.add_witness(hash, id(50)).unwrap();
        pipe.add_witness(hash, id(51)).unwrap();
        assert_eq!(pipe.warrants_for(&id(1))[0].witnesses.len(), 2);

        // Duplicate should fail
        let err = pipe.add_witness(hash, id(50)).unwrap_err();
        assert_eq!(err, PipelineError::DuplicateWitness);
    }

    #[test]
    fn test_verification_with_enough_witnesses() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);

        add_n_witnesses(&mut pipe, hash, MIN_WARRANT_WITNESSES as u8);
        let verified = pipe.check_verification(hash, MIN_WARRANT_WITNESSES).unwrap();
        assert!(verified);
        assert_eq!(
            pipe.warrants_for(&id(1))[0].status,
            WarrantStatus::Verified
        );
    }

    #[test]
    fn test_verification_fails_with_too_few() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);

        // Only 1 witness (need 3)
        pipe.add_witness(hash, id(50)).unwrap();
        let verified = pipe.check_verification(hash, MIN_WARRANT_WITNESSES).unwrap();
        assert!(!verified);
        assert_eq!(
            pipe.warrants_for(&id(1))[0].status,
            WarrantStatus::Detected
        );
    }

    #[test]
    fn test_first_offense_tier2() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);
        verify_warrant(&mut pipe, hash);

        let action = pipe.apply_penalty(hash).unwrap();
        assert_eq!(action.tier, 2);
        assert!((action.trust_factor - 0.7).abs() < f64::EPSILON);
        assert_eq!(action.jail_days, None);
    }

    #[test]
    fn test_second_offense_tier3() {
        let mut pipe = ForkPipeline::new();

        // First offense
        let h1 = pipe.submit_warrant(id(1), id(10), id(11), 42, id(99), 1_000);
        verify_warrant(&mut pipe, h1);
        pipe.apply_penalty(h1).unwrap();

        // Second offense (different sequence)
        let h2 = pipe.submit_warrant(id(1), id(20), id(21), 43, id(99), 2_000);
        verify_warrant(&mut pipe, h2);

        let action = pipe.apply_penalty(h2).unwrap();
        assert_eq!(action.tier, 3);
        assert!((action.trust_factor - 0.2).abs() < f64::EPSILON);
        assert_eq!(action.jail_days, Some(7));
    }

    #[test]
    fn test_third_offense_tier4() {
        let mut pipe = ForkPipeline::new();

        // First + second offense
        for seq in 42..=43 {
            let h = pipe.submit_warrant(id(1), id(10 + seq as u8), id(11 + seq as u8), seq, id(99), seq * 1_000);
            verify_warrant(&mut pipe, h);
            pipe.apply_penalty(h).unwrap();
        }

        // Third offense
        let h3 = pipe.submit_warrant(id(1), id(30), id(31), 44, id(99), 3_000);
        verify_warrant(&mut pipe, h3);

        let action = pipe.apply_penalty(h3).unwrap();
        assert_eq!(action.tier, 4);
        assert!((action.trust_factor - 0.001).abs() < f64::EPSILON);
        assert_eq!(action.jail_days, Some(180));
    }

    #[test]
    fn test_invalid_transition_rejected() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);

        // Try to apply penalty on Detected warrant (must be Verified)
        let err = pipe.apply_penalty(hash).unwrap_err();
        assert!(matches!(err, PipelineError::InvalidTransition { .. }));
    }

    #[test]
    fn test_file_appeal() {
        let mut pipe = ForkPipeline::new();
        let hash = submit_default(&mut pipe);
        verify_warrant(&mut pipe, hash);
        pipe.apply_penalty(hash).unwrap();

        pipe.file_appeal(hash).unwrap();
        let warrants = pipe.warrants_for(&id(1));
        assert_eq!(warrants[0].status, WarrantStatus::Disputed);
    }

    #[test]
    fn test_warrants_for_returns_correct_records() {
        let mut pipe = ForkPipeline::new();

        // Submit warrants for two different offenders
        let h1 = pipe.submit_warrant(id(1), id(10), id(11), 42, id(99), 1_000);
        let _h2 = pipe.submit_warrant(id(2), id(20), id(21), 43, id(99), 2_000);
        let h3 = pipe.submit_warrant(id(1), id(30), id(31), 44, id(99), 3_000);

        // Process one of offender-1's warrants
        verify_warrant(&mut pipe, h1);
        pipe.apply_penalty(h1).unwrap();

        // Should still find both warrants for offender 1 (one pending, one processed)
        let warrants = pipe.warrants_for(&id(1));
        assert_eq!(warrants.len(), 2);

        // Only one for offender 2
        assert_eq!(pipe.warrants_for(&id(2)).len(), 1);

        // h3 should still be pending/Detected
        let h3_record = warrants.iter().find(|r| r.warrant_hash == h3).unwrap();
        assert_eq!(h3_record.status, WarrantStatus::Detected);
    }

    #[test]
    fn test_warrant_hash_deterministic() {
        let mut pipe = ForkPipeline::new();
        let h1 = pipe.submit_warrant(id(1), id(10), id(11), 42, id(99), 1_000);
        let h2 = pipe.submit_warrant(id(1), id(10), id(11), 42, id(99), 9_999);

        // Same (offender, block_a, block_b, sequence) → same hash (timestamp not in hash)
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_pipeline_handles_multiple_warrants() {
        let mut pipe = ForkPipeline::new();

        let mut hashes = Vec::new();
        for i in 0u8..5 {
            let h = pipe.submit_warrant(
                id(i),
                id(100 + i),
                id(150 + i),
                i as u64,
                id(99),
                (i as u64 + 1) * 1_000,
            );
            hashes.push(h);
        }

        // Verify and penalise first 3
        for &h in &hashes[..3] {
            verify_warrant(&mut pipe, h);
            pipe.apply_penalty(h).unwrap();
        }

        // 2 still pending, 3 processed
        assert_eq!(pipe.warrants_for(&id(0)).len(), 1);
        assert_eq!(pipe.warrants_for(&id(3)).len(), 1);
        assert_eq!(
            pipe.warrants_for(&id(3))[0].status,
            WarrantStatus::Detected
        );
    }
}
