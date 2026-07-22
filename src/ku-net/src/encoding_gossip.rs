//! # Encoding Gossip — Network Protocol for Encoding Consensus
//!
//! Handles network communication for the encoding consensus protocol:
//! - Job announcement broadcasting
//! - Claim request/response handling
//! - Submission distribution
//! - FULL announcement + cleanup
//!
//! This module defines the protocol logic; actual transport is delegated
//! to the transport layer (QUIC/UDP).

use crate::encoding_job::{
    ClaimRejectReason, ClaimRequest, ClaimResponse, EncodingJob, VerificationSubmission,
};
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Encoding Protocol Messages
// ═══════════════════════════════════════════════════════════════════════════

/// Protocol message types for encoding consensus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncodingMessage {
    /// Owner announces a new encoding job on the DHT.
    JobAnnounce(EncodingJob),

    /// Owner updates the job status (e.g., claimed_count change).
    JobUpdate {
        raw_hash: [u8; 32],
        claimed_count: u8,
        status: u8,
    },

    /// Verifier requests to claim a slot on a job.
    Claim(ClaimRequest),

    /// Owner responds to a claim request.
    ClaimResult(ClaimResponse),

    /// Verifier submits their encoding result.
    Submission(VerificationSubmission),

    /// Owner announces that consensus is reached (FULL).
    ConsensusReached {
        raw_hash: [u8; 32],
        final_cid: [u8; 32],
        final_encoding_bytes: Vec<u8>,
    },

    /// Owner cleans up — job is done, remove from DHT.
    JobComplete { raw_hash: [u8; 32] },
}

// ═══════════════════════════════════════════════════════════════════════════
// Job Manager (Owner side)
// ═══════════════════════════════════════════════════════════════════════════

/// Manages encoding jobs on the owner's side.
///
/// Tracks which verifiers have claimed slots and handles the claim protocol.
pub struct OwnerJobManager {
    /// Active jobs by raw_hash.
    pub jobs: std::collections::HashMap<[u8; 32], OwnerJobState>,
}

/// State of a job being managed by the owner.
pub struct OwnerJobState {
    /// The encoding job metadata.
    pub job: EncodingJob,

    /// Node IDs that have claimed slots.
    pub claimed_nodes: Vec<u64>,

    /// Compressed raw text to send to claimants.
    pub raw_text_compressed: Vec<u8>,

    /// SELF encoding bytes (if available).
    pub self_encoding_bytes: Option<Vec<u8>>,
}

impl OwnerJobManager {
    /// Create a new job manager.
    pub fn new() -> Self {
        Self {
            jobs: std::collections::HashMap::new(),
        }
    }

    /// Post a new encoding job.
    pub fn post_job(
        &mut self,
        job: EncodingJob,
        raw_text_compressed: Vec<u8>,
        self_encoding_bytes: Option<Vec<u8>>,
    ) -> EncodingMessage {
        let raw_hash = job.raw_hash;
        let state = OwnerJobState {
            job: job.clone(),
            claimed_nodes: Vec::new(),
            raw_text_compressed,
            self_encoding_bytes,
        };
        self.jobs.insert(raw_hash, state);
        EncodingMessage::JobAnnounce(job)
    }

    /// Handle a claim request from a verifier.
    pub fn handle_claim(&mut self, request: &ClaimRequest) -> ClaimResponse {
        let state = match self.jobs.get_mut(&request.raw_hash) {
            Some(s) => s,
            None => {
                return ClaimResponse::Rejected {
                    reason: ClaimRejectReason::NotFound,
                }
            }
        };

        // Check if already finalized
        if state.job.status == 0x03 {
            // Full
            return ClaimResponse::Rejected {
                reason: ClaimRejectReason::AlreadyFull,
            };
        }

        // Check if this node already claimed
        if state.claimed_nodes.contains(&request.verifier_node) {
            return ClaimResponse::Rejected {
                reason: ClaimRejectReason::AlreadyClaimed,
            };
        }

        // Check if slots are full
        if state.job.is_slots_full() {
            return ClaimResponse::Rejected {
                reason: ClaimRejectReason::SlotsFull,
            };
        }

        // Accept the claim
        state.claimed_nodes.push(request.verifier_node);
        state.job.claimed_count += 1;

        // Generate claim token
        let mut token_input = Vec::new();
        token_input.extend_from_slice(&request.raw_hash);
        token_input.extend_from_slice(&request.verifier_node.to_le_bytes());
        token_input.extend_from_slice(&request.claim_nonce.to_le_bytes());
        let claim_token: [u8; 32] = blake3::hash(&token_input).into();

        ClaimResponse::Accepted {
            raw_text_compressed: state.raw_text_compressed.clone(),
            self_encoding_bytes: state.self_encoding_bytes.clone(),
            claim_token,
        }
    }

    /// Remove a completed job.
    pub fn complete_job(&mut self, raw_hash: &[u8; 32]) -> Option<EncodingMessage> {
        self.jobs
            .remove(raw_hash)
            .map(|_| EncodingMessage::JobComplete {
                raw_hash: *raw_hash,
            })
    }

    /// Number of active jobs.
    pub fn active_job_count(&self) -> usize {
        self.jobs.len()
    }
}

impl Default for OwnerJobManager {
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

    fn make_job() -> EncodingJob {
        EncodingJob {
            raw_hash: [0xAA; 32],
            owner_node: 1,
            status: 0x01,
            claimed_count: 0,
            needed_count: 3,
            raw_size_bytes: 2048,
            reward_per_verifier: 5,
            posted_at: 1000,
            has_self_encoding: true,
        }
    }

    #[test]
    fn test_post_and_claim() {
        let mut mgr = OwnerJobManager::new();
        let job = make_job();
        let _ = mgr.post_job(job, vec![1, 2, 3], Some(vec![4, 5]));

        assert_eq!(mgr.active_job_count(), 1);

        let claim = ClaimRequest {
            raw_hash: [0xAA; 32],
            verifier_node: 42,
            claim_nonce: 123,
            timestamp: 1100,
        };

        match mgr.handle_claim(&claim) {
            ClaimResponse::Accepted {
                raw_text_compressed,
                ..
            } => {
                assert_eq!(raw_text_compressed, vec![1, 2, 3]);
            }
            ClaimResponse::Rejected { reason } => {
                panic!("Expected accepted, got rejected: {:?}", reason);
            }
        }
    }

    #[test]
    fn test_duplicate_claim_rejected() {
        let mut mgr = OwnerJobManager::new();
        mgr.post_job(make_job(), vec![], None);

        let claim = ClaimRequest {
            raw_hash: [0xAA; 32],
            verifier_node: 42,
            claim_nonce: 1,
            timestamp: 1100,
        };

        let _ = mgr.handle_claim(&claim); // First: accept
        let result = mgr.handle_claim(&claim); // Second: reject

        match result {
            ClaimResponse::Rejected { reason } => {
                assert_eq!(reason, ClaimRejectReason::AlreadyClaimed);
            }
            _ => panic!("Expected rejection for duplicate claim"),
        }
    }

    #[test]
    fn test_slots_full_rejected() {
        let mut mgr = OwnerJobManager::new();
        let mut job = make_job();
        job.needed_count = 1;
        mgr.post_job(job, vec![], None);

        // First claim fills the only slot
        let claim1 = ClaimRequest {
            raw_hash: [0xAA; 32],
            verifier_node: 1,
            claim_nonce: 1,
            timestamp: 1100,
        };
        let _ = mgr.handle_claim(&claim1);

        // Second claim should be rejected
        let claim2 = ClaimRequest {
            raw_hash: [0xAA; 32],
            verifier_node: 2,
            claim_nonce: 2,
            timestamp: 1200,
        };
        match mgr.handle_claim(&claim2) {
            ClaimResponse::Rejected { reason } => {
                assert_eq!(reason, ClaimRejectReason::SlotsFull);
            }
            _ => panic!("Expected slots full rejection"),
        }
    }
}
