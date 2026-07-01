//! # Encoding Job — DHT-based Encoding Job Board
//!
//! Defines the data structures for the encoding job board that runs on the
//! DHT (Distributed Hash Table). Allows AI verifiers to discover pending
//! encodings and claim verification work.
//!
//! ## Key Design
//! - Owner posts `EncodingJob` to DHT → AI verifiers browse and claim
//! - Claim Token mechanism prevents stampede (too many AIs on same job)
//! - Owner is gatekeeper for their own job (no central coordinator)
//!
//! ## DHT Key Design
//! ```text
//! key = BLAKE3("encoding-job:" || raw_hash)
//! ```

use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════════════════
// Encoding Job (posted on DHT)
// ═══════════════════════════════════════════════════════════════════════════

/// An encoding job posted on the DHT for AI verifiers to discover.
///
/// This represents a pending KU that needs encoding or verification.
/// Posted by the owner node, visible to all AI verifiers in the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingJob {
    /// BLAKE3 hash of the compressed raw text — used as DHT key.
    pub raw_hash: [u8; 32],

    /// Owner node ID (the knowledge contributor's node).
    pub owner_node: u64,

    /// Current encoding status (Raw, Self, Part).
    pub status: u8, // EncodingStatus as u8 for wire compatibility

    /// Number of verifiers that have already claimed this job.
    pub claimed_count: u8,

    /// Number of verifiers needed for consensus (dynamic, capped at 3).
    pub needed_count: u8,

    /// Size of the raw text in bytes (influences reward).
    pub raw_size_bytes: u32,

    /// OBT reward offered per verifier.
    pub reward_per_verifier: u64,

    /// Timestamp when this job was posted.
    pub posted_at: u64,

    /// Whether an initial SELF encoding exists (for verifiers to compare against).
    pub has_self_encoding: bool,
}

impl EncodingJob {
    /// DHT key prefix for encoding jobs.
    pub const KEY_PREFIX: &'static [u8] = b"encoding-job:";

    /// Compute the DHT key for this job.
    pub fn dht_key(&self) -> [u8; 32] {
        Self::compute_dht_key(&self.raw_hash)
    }

    /// Compute DHT key from raw hash.
    pub fn compute_dht_key(raw_hash: &[u8; 32]) -> [u8; 32] {
        let mut input = Vec::with_capacity(Self::KEY_PREFIX.len() + 32);
        input.extend_from_slice(Self::KEY_PREFIX);
        input.extend_from_slice(raw_hash);
        blake3::hash(&input).into()
    }

    /// Whether this job still has open slots for verifiers.
    pub fn has_open_slots(&self) -> bool {
        self.claimed_count < self.needed_count
    }

    /// Whether this job is completed (enough verifiers have claimed).
    pub fn is_slots_full(&self) -> bool {
        self.claimed_count >= self.needed_count
    }

    /// Remaining slots for verifiers.
    pub fn remaining_slots(&self) -> u8 {
        self.needed_count.saturating_sub(self.claimed_count)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Claim Protocol
// ═══════════════════════════════════════════════════════════════════════════

/// Request from an AI verifier to claim a verification slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRequest {
    /// BLAKE3 hash of the raw text being claimed.
    pub raw_hash: [u8; 32],

    /// Node ID of the verifier requesting the claim.
    pub verifier_node: u64,

    /// Random nonce for deduplication.
    pub claim_nonce: u64,

    /// Timestamp of the claim request.
    pub timestamp: u64,
}

/// Response from the owner node to a claim request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClaimResponse {
    /// Claim accepted — verifier receives raw text and optional SELF encoding.
    Accepted {
        /// Compressed raw text (zstd).
        raw_text_compressed: Vec<u8>,
        /// The owner's initial CoreDna encoding (if status >= SELF).
        self_encoding_bytes: Option<Vec<u8>>,
        /// Claim token — proof of accepted claim.
        claim_token: [u8; 32],
    },
    /// Claim rejected.
    Rejected {
        /// Reason for rejection.
        reason: ClaimRejectReason,
    },
}

/// Reasons a claim can be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimRejectReason {
    /// Job has already reached FULL status.
    AlreadyFull,
    /// All verification slots are taken.
    SlotsFull,
    /// Claim request came too soon after a previous one (rate limit).
    TooSoon,
    /// This node has already claimed this job.
    AlreadyClaimed,
    /// Job not found on this owner node.
    NotFound,
}

impl std::fmt::Display for ClaimRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyFull => write!(f, "Job already finalized (FULL)"),
            Self::SlotsFull => write!(f, "All verification slots are taken"),
            Self::TooSoon => write!(f, "Rate limited — try again later"),
            Self::AlreadyClaimed => write!(f, "You have already claimed this job"),
            Self::NotFound => write!(f, "Job not found"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Verification Submission
// ═══════════════════════════════════════════════════════════════════════════

/// Submission of a verification result back to the owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationSubmission {
    /// Claim token received during claim acceptance.
    pub claim_token: [u8; 32],

    /// The verifier's CoreDna encoding (serialized bytes).
    pub encoding_bytes: Vec<u8>,

    /// Node ID of the verifier.
    pub verifier_node: u64,

    /// Whether the verifier agrees with the existing SELF encoding.
    pub agrees_with_self: bool,

    /// Timestamp of submission.
    pub timestamp: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job() -> EncodingJob {
        EncodingJob {
            raw_hash: [0xAA; 32],
            owner_node: 1,
            status: 0x01, // Self
            claimed_count: 0,
            needed_count: 3,
            raw_size_bytes: 2048,
            reward_per_verifier: 5,
            posted_at: 1000,
            has_self_encoding: true,
        }
    }

    #[test]
    fn test_job_open_slots() {
        let mut job = make_job();
        assert!(job.has_open_slots());
        assert_eq!(job.remaining_slots(), 3);

        job.claimed_count = 3;
        assert!(!job.has_open_slots());
        assert!(job.is_slots_full());
        assert_eq!(job.remaining_slots(), 0);
    }

    #[test]
    fn test_dht_key_deterministic() {
        let hash = [0xBB; 32];
        let key1 = EncodingJob::compute_dht_key(&hash);
        let key2 = EncodingJob::compute_dht_key(&hash);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_dht_key_different_for_different_hashes() {
        let hash_a = [0xAA; 32];
        let hash_b = [0xBB; 32];
        let key_a = EncodingJob::compute_dht_key(&hash_a);
        let key_b = EncodingJob::compute_dht_key(&hash_b);
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn test_claim_reject_display() {
        assert_eq!(
            format!("{}", ClaimRejectReason::SlotsFull),
            "All verification slots are taken"
        );
    }
}
