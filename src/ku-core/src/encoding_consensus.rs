//! # Encoding Consensus — Distributed Encoding Verification
//!
//! Implements the Distributed Encoding Consensus mechanism where multiple
//! AI nodes collaboratively verify and encode raw knowledge into CoreDna.
//!
//! ## Encoding Status Lifecycle
//! ```text
//! RAW → SELF → PART → FULL
//! ```
//!
//! - **RAW**: Raw text submitted, no AI has encoded yet
//! - **SELF**: Owner's local AI has produced an initial CoreDna encoding
//! - **PART**: Some verifier AIs have confirmed, but below consensus threshold
//! - **FULL**: Consensus reached — final KU is immutable, intermediates deleted
//!
//! ## Design Decisions
//! - Verify threshold: dynamic by network size, **capped at 3**
//! - 2-phase verify: (A) AI decomposition + (B) tool encoding round-trip
//! - FULL = immutable — new raw text = new KU
//! - Rewards paid in OBT tokens, proportional to raw text size
//! - No timeout — pending encodings kept until enough verifiers participate
//!
//! ## Reference
//! See `docs/specs/ENCODING_CONSENSUS_SPEC.md` for full design.

use crate::core_dna::{CoreDna, encode_core_dna, decode_core_dna};
use crate::error::KuError;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Encoding Status
// ═══════════════════════════════════════════════════════════════════════════

/// Encoding verification status — tracks how well-verified the encoding is.
///
/// This is separate from `EpistemicStatus` (which tracks knowledge quality).
/// EncodingStatus tracks encoding quality: "Was the raw text encoded correctly?"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EncodingStatus {
    /// Raw text submitted, no AI encoding yet.
    /// The contributor has knowledge but no AI to encode it.
    Raw = 0x00,

    /// Owner's local AI has produced an initial CoreDna encoding.
    /// May be inaccurate if the AI is weak or tampered with.
    Self_ = 0x01,

    /// Some verifier AIs have confirmed, but below consensus threshold.
    /// Partially verified — encoding is likely correct but not guaranteed.
    Part = 0x02,

    /// Consensus reached — final KU. Immutable.
    /// All intermediate data (raw text, alternate encodings) is deleted.
    Full = 0x03,
}

impl Default for EncodingStatus {
    fn default() -> Self {
        EncodingStatus::Self_
    }
}

impl EncodingStatus {
    /// Parse from u8 wire value.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::Raw),
            0x01 => Some(Self::Self_),
            0x02 => Some(Self::Part),
            0x03 => Some(Self::Full),
            _ => None,
        }
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Raw => "Raw",
            Self::Self_ => "Self",
            Self::Part => "Part",
            Self::Full => "Full",
        }
    }

    /// Whether this KU's encoding is finalized and immutable.
    pub fn is_finalized(&self) -> bool {
        *self == Self::Full
    }

    /// Whether this KU still needs verification from other AIs.
    pub fn needs_verification(&self) -> bool {
        matches!(self, Self::Raw | Self::Self_ | Self::Part)
    }
}

impl std::fmt::Display for EncodingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Encoding Submission
// ═══════════════════════════════════════════════════════════════════════════

/// A single encoding attempt submitted by an AI verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingSubmission {
    /// Which AI node produced this encoding.
    pub encoder_node_id: u64,

    /// The proposed CoreDna encoding as wire bytes.
    /// Stored as bytes because CoreDna doesn't impl Serialize.
    pub core_dna_bytes: Vec<u8>,

    /// BLAKE3 hash of the raw text — proves the encoder read the original.
    pub raw_text_hash: [u8; 32],

    /// When this submission was created.
    pub timestamp: u64,

    /// Whether this is the first encoding (SELF) or a verification encoding.
    pub is_first_encoder: bool,

    /// Time spent by AI to produce this encoding (milliseconds).
    /// Used by OBT Gate 4 to reject suspiciously fast auto-generated encodings.
    /// Minimum threshold: 100ms (see `obt_anti_gaming::MIN_ENCODING_TIME_MS`).
    pub encoding_time_ms: u64,
}

impl EncodingSubmission {
    /// Create from a CoreDna (encodes to bytes).
    pub fn from_core_dna(
        dna: &CoreDna,
        encoder_node_id: u64,
        raw_text_hash: [u8; 32],
        timestamp: u64,
        is_first_encoder: bool,
        encoding_time_ms: u64,
    ) -> Result<Self, KuError> {
        let bytes = encode_core_dna(dna)?;
        Ok(Self {
            encoder_node_id,
            core_dna_bytes: bytes,
            raw_text_hash,
            timestamp,
            is_first_encoder,
            encoding_time_ms,
        })
    }

    /// Decode the stored CoreDna bytes.
    pub fn decode_core_dna(&self) -> Result<CoreDna, KuError> {
        decode_core_dna(&self.core_dna_bytes)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Consensus Configuration
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for the encoding consensus mechanism.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Minimum number of verifiers needed for FULL status.
    /// Dynamic by network size, capped at 3.
    pub min_verifiers: usize,

    /// Similarity threshold for two encodings to "agree" (0.0-1.0).
    /// Default: 0.8 — two encodings agree if similarity > 0.8.
    pub agreement_threshold: f32,

    /// Minimum similarity for an encoding to be considered valid (0.0-1.0).
    pub similarity_threshold: f32,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            min_verifiers: 3,
            agreement_threshold: 0.8,
            similarity_threshold: 0.6,
        }
    }
}

/// Compute the required number of verifiers based on network size.
///
/// Dynamic but capped — encoding is just "indexing", attackers gain nothing
/// from encoding incorrectly. 3 independent AIs is sufficient cross-verification.
pub fn compute_needed_verifiers(network_size: usize) -> usize {
    match network_size {
        0..=5 => 1,    // Very small network: 1 verifier is enough
        6..=20 => 2,   // Small network: 2 verifiers
        _ => 3,        // Medium/large: capped at 3
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Encoding Consensus Record
// ═══════════════════════════════════════════════════════════════════════════

/// Tracks the consensus state for a pending KU encoding.
///
/// This record lives on the owner node and participating verifier nodes.
/// It is NOT broadcast to the entire network — only the final KU (FULL) is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingConsensus {
    /// The original raw text (compressed with zstd in practice).
    /// Cleared when status reaches FULL.
    pub raw_text: String,

    /// BLAKE3 hash of raw_text — permanent identifier for this encoding task.
    pub raw_hash: [u8; 32],

    /// Node ID of the knowledge contributor.
    pub contributor_id: u64,

    /// Current encoding status.
    pub status: EncodingStatus,

    /// All encoding submissions received.
    pub submissions: Vec<EncodingSubmission>,

    /// Pairwise agreement scores between submissions.
    /// Key: (node_a, node_b), Value: similarity score.
    pub agreement_matrix: HashMap<(u64, u64), f32>,

    /// When this encoding task was created.
    pub created_at: u64,

    /// When consensus was reached (FULL).
    pub finalized_at: Option<u64>,

    /// CID of the final KU (set when FULL).
    pub final_cid: Option<[u8; 32]>,

    /// Consensus configuration.
    pub config: ConsensusConfig,
}

impl EncodingConsensus {
    /// Create a new encoding consensus record for raw text.
    pub fn new_raw(raw_text: String, contributor_id: u64, now: u64) -> Self {
        let raw_hash = blake3::hash(raw_text.as_bytes()).into();
        Self {
            raw_text,
            raw_hash,
            contributor_id,
            status: EncodingStatus::Raw,
            submissions: Vec::new(),
            agreement_matrix: HashMap::new(),
            created_at: now,
            finalized_at: None,
            final_cid: None,
            config: ConsensusConfig::default(),
        }
    }

    /// Create with an initial SELF encoding from the owner's AI.
    pub fn new_self(
        raw_text: String,
        contributor_id: u64,
        initial_dna: CoreDna,
        now: u64,
    ) -> Self {
        let raw_hash = blake3::hash(raw_text.as_bytes()).into();
        let submission = EncodingSubmission::from_core_dna(
            &initial_dna, contributor_id, raw_hash, now, true, 500,
        ).expect("initial encoding should be valid");
        Self {
            raw_text,
            raw_hash,
            contributor_id,
            status: EncodingStatus::Self_,
            submissions: vec![submission],
            agreement_matrix: HashMap::new(),
            created_at: now,
            finalized_at: None,
            final_cid: None,
            config: ConsensusConfig::default(),
        }
    }

    /// Submit a verification encoding from another AI.
    ///
    /// Returns the new status after processing this submission.
    pub fn submit_verification(
        &mut self,
        submission: EncodingSubmission,
        similarity_fn: impl Fn(&[u8], &[u8]) -> f32,
    ) -> EncodingStatus {
        let new_node = submission.encoder_node_id;

        // Compute pairwise similarities with existing submissions
        for existing in &self.submissions {
            let sim = similarity_fn(&existing.core_dna_bytes, &submission.core_dna_bytes);
            self.agreement_matrix.insert(
                (existing.encoder_node_id, new_node),
                sim,
            );
            self.agreement_matrix.insert(
                (new_node, existing.encoder_node_id),
                sim,
            );
        }

        self.submissions.push(submission);

        // Update status based on submission count
        if self.status == EncodingStatus::Raw {
            self.status = EncodingStatus::Self_;
        } else if self.status == EncodingStatus::Self_ && self.submissions.len() >= 2 {
            self.status = EncodingStatus::Part;
        }

        self.status
    }

    /// Try to finalize — check if consensus threshold is reached.
    ///
    /// Returns `Some(best_index)` if consensus is reached, with the index
    /// of the winning submission. Returns `None` if not enough agreement.
    pub fn try_finalize(&mut self, now: u64) -> Option<usize> {
        let n = self.submissions.len();
        if n < self.config.min_verifiers {
            return None; // Not enough verifiers yet
        }

        // Find the submission with the highest selection score
        let mut best_idx = 0;
        let mut best_score = f32::MIN;

        for i in 0..n {
            let score = self.compute_selection_score(i);
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }

        // Check that the best submission has enough agreement
        let agreement_count = self.count_agreements(best_idx);
        let agreement_ratio = agreement_count as f32 / n as f32;

        if agreement_ratio >= self.config.agreement_threshold {
            self.status = EncodingStatus::Full;
            self.finalized_at = Some(now);
            // CID will be set by the caller after creating KuRuntime
            Some(best_idx)
        } else {
            None
        }
    }

    /// Compute the weighted selection score for a submission.
    ///
    /// Score = 0.50 * agreement + 0.30 * detail + 0.20 * reputation
    fn compute_selection_score(&self, idx: usize) -> f32 {
        const W_AGREEMENT: f32 = 0.50;
        const W_DETAIL: f32 = 0.30;
        const W_REPUTATION: f32 = 0.20;

        let n = self.submissions.len();
        let submission = &self.submissions[idx];

        // Factor 1: Agreement — how many other submissions agree with this one?
        let agreement = self.count_agreements(idx) as f32 / n.max(1) as f32;

        // Factor 2: Detail — more bytes = more detailed encoding
        let max_bytes = self.submissions.iter()
            .map(|s| s.core_dna_bytes.len())
            .max()
            .unwrap_or(1);
        let detail = submission.core_dna_bytes.len() as f32
            / max_bytes.max(1) as f32;

        // Factor 3: Reputation — placeholder, defaults to 0.5
        // TODO: integrate with node reputation system
        let reputation = if submission.is_first_encoder { 0.6 } else { 0.5 };

        W_AGREEMENT * agreement + W_DETAIL * detail + W_REPUTATION * reputation
    }

    /// Count how many other submissions agree with submission at `idx`.
    fn count_agreements(&self, idx: usize) -> usize {
        let node = self.submissions[idx].encoder_node_id;
        let mut count = 1; // Agrees with itself

        for (i, other) in self.submissions.iter().enumerate() {
            if i == idx { continue; }
            let key = (node, other.encoder_node_id);
            if let Some(&sim) = self.agreement_matrix.get(&key) {
                if sim >= self.config.agreement_threshold {
                    count += 1;
                }
            }
        }

        count
    }

    /// Get the best encoding's CoreDna bytes (after finalization).
    pub fn final_encoding_bytes(&self) -> Option<&[u8]> {
        if self.status != EncodingStatus::Full {
            return None;
        }
        // After finalization, the best submission is the one with highest score
        let mut best_idx = 0;
        let mut best_score = f32::MIN;
        for i in 0..self.submissions.len() {
            let score = self.compute_selection_score(i);
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }
        self.submissions.get(best_idx).map(|s| s.core_dna_bytes.as_slice())
    }

    /// Get the best encoding's CoreDna (after finalization, decoded).
    pub fn final_encoding(&self) -> Option<CoreDna> {
        self.final_encoding_bytes()
            .and_then(|bytes| decode_core_dna(bytes).ok())
    }

    /// Clean up intermediate data after reaching FULL status.
    ///
    /// Removes raw text and keeps only the winning submission.
    pub fn cleanup(&mut self) {
        if self.status == EncodingStatus::Full {
            self.raw_text.clear();
            self.agreement_matrix.clear();
            // Keep only the winning submission
            if let Some(best_bytes) = self.final_encoding_bytes().map(|b| b.to_vec()) {
                self.submissions.retain(|s| s.core_dna_bytes == best_bytes);
            }
        }
    }

    /// Number of unique verifier nodes that have submitted.
    ///
    /// Used by `obt_anti_gaming::gate_2_encoding_consensus()` which requires
    /// `verifier_count >= ENCODING_CONSENSUS_MIN_VERIFIERS` (3).
    pub fn verifier_count(&self) -> u32 {
        self.submissions.len() as u32
    }

    /// Average encoding time across all submissions (milliseconds).
    ///
    /// Used by `obt_anti_gaming::gate_4_complexity()` which requires
    /// `encoding_time_ms >= MIN_ENCODING_TIME_MS` (100ms).
    pub fn avg_encoding_time_ms(&self) -> u64 {
        if self.submissions.is_empty() {
            return 0;
        }
        let total: u64 = self.submissions.iter()
            .map(|s| s.encoding_time_ms)
            .sum();
        total / self.submissions.len() as u64
    }

    /// Check if this encoding is a duplicate of an existing KU.
    ///
    /// Uses BLAKE3 content-addressed identity: if the same raw text was
    /// already encoded (same raw_hash exists in the known set), it's a duplicate.
    pub fn is_duplicate(&self, known_raw_hashes: &std::collections::HashSet<[u8; 32]>) -> bool {
        known_raw_hashes.contains(&self.raw_hash)
    }

    /// Number of additional verifiers needed to potentially reach FULL.
    pub fn verifiers_needed(&self) -> usize {
        self.config.min_verifiers.saturating_sub(self.submissions.len())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_dna::{CoreDna, CoreDnaHeader, Instruction};

    fn make_dna(gene_type: u8, instructions: Vec<Instruction>) -> CoreDna {
        CoreDna {
            header: CoreDnaHeader {
                version: 2,
                gene_type,
                has_concept_table: false,
            },
            concept_table: Vec::new(),
            instructions,
        }
    }

    fn simple_fact_dna() -> CoreDna {
        make_dna(0, vec![
            Instruction::Triple { s: 1, p: 2, o: 3 },
            Instruction::Certainty { level: 9000 },
            Instruction::End,
        ])
    }

    fn similar_fact_dna() -> CoreDna {
        make_dna(0, vec![
            Instruction::Triple { s: 1, p: 2, o: 3 },
            Instruction::Quality { s: 1, q: 4 },
            Instruction::Certainty { level: 8500 },
            Instruction::End,
        ])
    }

    /// Bytes-based similarity: decode both and compare gene_type
    fn test_similarity(a: &[u8], b: &[u8]) -> f32 {
        let dna_a = decode_core_dna(a);
        let dna_b = decode_core_dna(b);
        match (dna_a, dna_b) {
            (Ok(a), Ok(b)) => {
                if a.header.gene_type == b.header.gene_type { 0.9 } else { 0.0 }
            },
            _ => 0.0,
        }
    }

    #[test]
    fn test_encoding_status_basics() {
        assert_eq!(EncodingStatus::default(), EncodingStatus::Self_);
        assert!(EncodingStatus::Full.is_finalized());
        assert!(!EncodingStatus::Part.is_finalized());
        assert!(EncodingStatus::Raw.needs_verification());
        assert!(!EncodingStatus::Full.needs_verification());
        assert_eq!(EncodingStatus::from_u8(0x02), Some(EncodingStatus::Part));
        assert_eq!(EncodingStatus::from_u8(0xFF), None);
        assert_eq!(format!("{}", EncodingStatus::Full), "Full");
    }

    #[test]
    fn test_compute_needed_verifiers() {
        assert_eq!(compute_needed_verifiers(1), 1);
        assert_eq!(compute_needed_verifiers(5), 1);
        assert_eq!(compute_needed_verifiers(10), 2);
        assert_eq!(compute_needed_verifiers(20), 2);
        assert_eq!(compute_needed_verifiers(100), 3);
        assert_eq!(compute_needed_verifiers(1_000_000), 3); // Capped!
    }

    #[test]
    fn test_new_raw() {
        let c = EncodingConsensus::new_raw("Test knowledge".into(), 42, 1000);
        assert_eq!(c.status, EncodingStatus::Raw);
        assert_eq!(c.contributor_id, 42);
        assert_eq!(c.verifier_count(), 0);
        assert!(c.verifiers_needed() > 0);
    }

    #[test]
    fn test_new_self() {
        let dna = simple_fact_dna();
        let c = EncodingConsensus::new_self("Test".into(), 42, dna, 1000);
        assert_eq!(c.status, EncodingStatus::Self_);
        assert_eq!(c.verifier_count(), 1);
    }

    #[test]
    fn test_submit_verification_transitions() {
        let dna = simple_fact_dna();
        let mut c = EncodingConsensus::new_self("Test".into(), 1, dna.clone(), 1000);
        assert_eq!(c.status, EncodingStatus::Self_);

        // Second submission → PART
        let sub2 = EncodingSubmission::from_core_dna(
            &similar_fact_dna(), 2, c.raw_hash, 1100, false, 500,
        ).unwrap();
        let status = c.submit_verification(sub2, test_similarity);
        assert_eq!(status, EncodingStatus::Part);
        assert_eq!(c.verifier_count(), 2);
    }

    #[test]
    fn test_consensus_finalization() {
        let dna = simple_fact_dna();
        let mut c = EncodingConsensus::new_self("Test".into(), 1, dna.clone(), 1000);
        c.config.min_verifiers = 3;

        // Add 2 more verifiers
        for node_id in 2..=3 {
            let sub = EncodingSubmission::from_core_dna(
                &simple_fact_dna(), node_id, c.raw_hash,
                1000 + node_id * 100, false, 500,
            ).unwrap();
            c.submit_verification(sub, test_similarity);
        }

        assert_eq!(c.verifier_count(), 3);
        let result = c.try_finalize(2000);
        assert!(result.is_some());
        assert_eq!(c.status, EncodingStatus::Full);
        assert!(c.finalized_at.is_some());
    }

    #[test]
    fn test_consensus_not_enough_verifiers() {
        let dna = simple_fact_dna();
        let mut c = EncodingConsensus::new_self("Test".into(), 1, dna, 1000);
        c.config.min_verifiers = 3;

        // Only 1 verifier (the self encoder) — should not finalize
        assert!(c.try_finalize(2000).is_none());
        assert_eq!(c.status, EncodingStatus::Self_);
    }

    #[test]
    fn test_cleanup_after_full() {
        let dna = simple_fact_dna();
        let mut c = EncodingConsensus::new_self("Important knowledge".into(), 1, dna, 1000);
        c.config.min_verifiers = 1;

        let _ = c.try_finalize(2000);
        assert_eq!(c.status, EncodingStatus::Full);

        c.cleanup();
        assert!(c.raw_text.is_empty()); // Raw text deleted
        assert!(c.agreement_matrix.is_empty()); // Matrix deleted
        assert_eq!(c.submissions.len(), 1); // Only winner kept
    }

    #[test]
    fn test_full_is_immutable() {
        // FULL status means the encoding is final
        let status = EncodingStatus::Full;
        assert!(status.is_finalized());
        assert!(!status.needs_verification());
    }

    #[test]
    fn test_verifier_count() {
        let mut ec = EncodingConsensus::new_raw("test knowledge".to_string(), 1, 1000);
        assert_eq!(ec.verifier_count(), 0);
        
        // Create a test CoreDna and submit
        let dna = simple_fact_dna();
        let sub = EncodingSubmission::from_core_dna(
            &dna, 100, ec.raw_hash, 1001, true, 500,
        ).unwrap();
        ec.submissions.push(sub);
        assert_eq!(ec.verifier_count(), 1);
    }

    #[test]
    fn test_avg_encoding_time() {
        let mut ec = EncodingConsensus::new_raw("test knowledge".to_string(), 1, 1000);
        assert_eq!(ec.avg_encoding_time_ms(), 0);
        
        let dna = simple_fact_dna();
        let sub1 = EncodingSubmission::from_core_dna(
            &dna, 100, ec.raw_hash, 1001, true, 200,
        ).unwrap();
        let sub2 = EncodingSubmission::from_core_dna(
            &dna, 101, ec.raw_hash, 1002, false, 400,
        ).unwrap();
        ec.submissions.push(sub1);
        ec.submissions.push(sub2);
        assert_eq!(ec.avg_encoding_time_ms(), 300); // (200+400)/2
    }

    #[test]
    fn test_is_duplicate() {
        let ec = EncodingConsensus::new_raw("test knowledge".to_string(), 1, 1000);
        let mut known = std::collections::HashSet::new();
        assert!(!ec.is_duplicate(&known));
        
        known.insert(ec.raw_hash);
        assert!(ec.is_duplicate(&known));
    }
}

