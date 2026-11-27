//! Pillar 6 `D-013` Delegated Encode & Two-Phase Commit-Before-Reveal Blind Verify Coordinator.
//!
//! Enforces:
//! - `DelegatedEncodeJobV1` bounded leases (`DelegatedJobLeaseWireV1` `lease_ttl_ms <= 300_000` ms)
//!   with Ed25519 signature verification and automatic expiry reclamation.
//! - Two-phase commit-before-reveal `BlindVerifyJobV1` (`FID-002` / `D-014`):
//!   `BlindVerifyCommitWireV1` MUST be acknowledged (`commitment_ack_digest`) BEFORE
//!   `BlindVerifyRevealWireV1` is accepted, producing a verified `BlindCommitRevealProof`
//!   and post-commit `RewardEvidenceNoticeV2` (`AcceptedWorkNotice`).

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use ku_core::foundation::{
    signature_message, ActorId, EventCid, FeedId, NodeId, ObjectCid, PermitCid, ReservedDomain,
};
use onebrain_obt::{AcceptedWorkKind, BlindCommitRevealProof, DelegatedWorkJobProof};
use onebrain_protocol::pillar_wire::{
    BlindVerifyCommitWireV1, BlindVerifyRevealWireV1, DelegatedJobLeaseWireV1,
    MAX_DELEGATED_JOB_LEASE_TTL_MS,
};
use thiserror::Error;

use crate::vnext_reward_firewall::{AcceptedWorkNotice, RewardEvidenceNoticeV2};

const DELEGATED_LEASE_SIG_DOMAIN: &[u8] = b"onebrain:vnext:d013-delegated-lease:1\0";
const BLIND_COMMIT_SIG_DOMAIN: &[u8] = b"onebrain:vnext:d013-blind-commit:1\0";
const BLIND_COMMIT_ACK_DOMAIN: &[u8] = b"onebrain:vnext:d013-blind-commit-ack:1\0";
const BLIND_REVEAL_SIG_DOMAIN: &[u8] = b"onebrain:vnext:d013-blind-reveal:1\0";

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DelegatedJobError {
    #[error("delegated job_id or required CIDs must be non-zero")]
    InvalidJobParameters,
    #[error("delegated job lease_ttl_ms {0} is zero or exceeds MAX_DELEGATED_JOB_LEASE_TTL_MS (300_000 ms)")]
    InvalidLeaseTtl(u32),
    #[error("delegated job lease is still active for another worker until {expires_at_ms} ms")]
    JobAlreadyLeased { expires_at_ms: u64 },
    #[error("delegated job lease expired at {expires_at_ms} ms (now {now_ms} ms)")]
    LeaseExpired { expires_at_ms: u64, now_ms: u64 },
    #[error("delegated job signature verification failed")]
    InvalidSignature,
    #[error("delegated job {0:?} not found")]
    UnknownJob([u8; 32]),
    #[error("blind verify reveal rejected: commit must precede reveal (FID-002)")]
    CommitMustPrecedeReveal,
    #[error("blind verify commitment_ack_digest mismatch")]
    CommitmentAckMismatch,
    #[error("blind verify reveal digest/salt does not match committed hash")]
    RevealHashMismatch,
    #[error("blind verify job already revealed")]
    AlreadyRevealed,
}

/// Registered `D-013` delegated encode job specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegatedEncodeJobV1 {
    pub job_id: [u8; 32],
    pub requester_actor_id: ActorId,
    pub requester_public_key: [u8; 32],
    pub permit_cid: PermitCid,
    pub source_artifact_cid: ObjectCid,
    pub compiler_profile_cid: ObjectCid,
    pub active_lease: Option<ActiveDelegatedLease>,
    pub completed_work_record: Option<ObjectCid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveDelegatedLease {
    pub worker_node_id: NodeId,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub wire: DelegatedJobLeaseWireV1,
}

impl ActiveDelegatedLease {
    pub const fn is_expired_at(&self, now_unix_ms: u64) -> bool {
        now_unix_ms >= self.expires_at_unix_ms
    }
}

/// Two-phase commit-before-reveal `BlindVerifyJobV1` state (`FID-002`, `D-013`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlindVerifyJobV1 {
    pub job_id: [u8; 32],
    pub source_span_cid: ObjectCid,
    pub verifier_feed_id: FeedId,
    pub verifier_public_key: [u8; 32],
    pub phase: BlindVerifyPhase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlindVerifyPhase {
    AwaitingCommit,
    Committed {
        attempt_semantic_cid_salted_hash: [u8; 32],
        commitment_ack_digest: [u8; 32],
        correlation_declaration_bytes: Vec<u8>,
    },
    Revealed {
        proof: BlindCommitRevealProof,
        encoding_attempt_bytes: Vec<u8>,
        fidelity_attestation_bytes: Vec<u8>,
    },
}

/// Signing preimage helper for `DelegatedJobLeaseWireV1`.
pub fn delegated_lease_signing_preimage(
    job_id: [u8; 32],
    worker_node_id: NodeId,
    permit_cid: PermitCid,
    lease_ttl_ms: u32,
    issued_at_unix_ms: u64,
) -> Vec<u8> {
    let mut raw = Vec::with_capacity(DELEGATED_LEASE_SIG_DOMAIN.len() + 108);
    raw.extend_from_slice(DELEGATED_LEASE_SIG_DOMAIN);
    raw.extend_from_slice(&job_id);
    raw.extend_from_slice(worker_node_id.as_bytes());
    raw.extend_from_slice(permit_cid.as_bytes());
    raw.extend_from_slice(&lease_ttl_ms.to_be_bytes());
    raw.extend_from_slice(&issued_at_unix_ms.to_be_bytes());
    signature_message(ReservedDomain::Event, &raw).expect("non-empty domain message")
}

/// Signing preimage helper for `BlindVerifyCommitWireV1`.
pub fn blind_commit_signing_preimage(
    job_id: [u8; 32],
    verifier_feed_id: FeedId,
    source_span_cid: ObjectCid,
    attempt_semantic_cid_salted_hash: [u8; 32],
    correlation_declaration_bytes: &[u8],
) -> Vec<u8> {
    let mut raw = Vec::with_capacity(
        BLIND_COMMIT_SIG_DOMAIN.len() + 136 + correlation_declaration_bytes.len(),
    );
    raw.extend_from_slice(BLIND_COMMIT_SIG_DOMAIN);
    raw.extend_from_slice(&job_id);
    raw.extend_from_slice(verifier_feed_id.as_bytes());
    raw.extend_from_slice(source_span_cid.as_bytes());
    raw.extend_from_slice(&attempt_semantic_cid_salted_hash);
    raw.extend_from_slice(&(correlation_declaration_bytes.len() as u64).to_be_bytes());
    raw.extend_from_slice(correlation_declaration_bytes);
    signature_message(ReservedDomain::Event, &raw).expect("non-empty domain message")
}

/// Signing preimage helper for `BlindVerifyRevealWireV1`.
pub fn blind_reveal_signing_preimage(
    job_id: [u8; 32],
    commitment_ack_digest: [u8; 32],
    reveal_salt: [u8; 32],
    encoding_attempt_bytes: &[u8],
    fidelity_attestation_bytes: &[u8],
) -> Vec<u8> {
    let mut raw = Vec::with_capacity(
        BLIND_REVEAL_SIG_DOMAIN.len()
            + 112
            + encoding_attempt_bytes.len()
            + fidelity_attestation_bytes.len(),
    );
    raw.extend_from_slice(BLIND_REVEAL_SIG_DOMAIN);
    raw.extend_from_slice(&job_id);
    raw.extend_from_slice(&commitment_ack_digest);
    raw.extend_from_slice(&reveal_salt);
    raw.extend_from_slice(&(encoding_attempt_bytes.len() as u64).to_be_bytes());
    raw.extend_from_slice(encoding_attempt_bytes);
    raw.extend_from_slice(&(fidelity_attestation_bytes.len() as u64).to_be_bytes());
    raw.extend_from_slice(fidelity_attestation_bytes);
    signature_message(ReservedDomain::Event, &raw).expect("non-empty domain message")
}

/// Compute the canonical reveal output digest over `(encoding_attempt_bytes, fidelity_attestation_bytes)`
/// bound into `BlindCommitRevealProof::compute_commit_digest(reveal_output_digest, reveal_salt)`.
pub fn compute_blind_reveal_output_digest(
    encoding_attempt_bytes: &[u8],
    fidelity_attestation_bytes: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:d013-blind-reveal-output:1\0");
    hasher.update(&(encoding_attempt_bytes.len() as u64).to_be_bytes());
    hasher.update(encoding_attempt_bytes);
    hasher.update(&(fidelity_attestation_bytes.len() as u64).to_be_bytes());
    hasher.update(fidelity_attestation_bytes);
    *hasher.finalize().as_bytes()
}

/// Host coordinator for `D-013` delegated encode leases and blind verification jobs.
#[derive(Default)]
pub struct DelegatedJobCoordinator {
    encode_jobs: BTreeMap<[u8; 32], DelegatedEncodeJobV1>,
    blind_verify_jobs: BTreeMap<[u8; 32], BlindVerifyJobV1>,
}

impl DelegatedJobCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_encode_job(
        &mut self,
        job_id: [u8; 32],
        requester_actor_id: ActorId,
        requester_public_key: [u8; 32],
        permit_cid: PermitCid,
        source_artifact_cid: ObjectCid,
        compiler_profile_cid: ObjectCid,
    ) -> Result<(), DelegatedJobError> {
        if job_id == [0; 32]
            || requester_actor_id.as_bytes() == &[0; 32]
            || requester_public_key == [0; 32]
            || permit_cid.as_bytes() == &[0; 32]
            || source_artifact_cid.as_bytes() == &[0; 32]
            || compiler_profile_cid.as_bytes() == &[0; 32]
        {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        self.encode_jobs.insert(
            job_id,
            DelegatedEncodeJobV1 {
                job_id,
                requester_actor_id,
                requester_public_key,
                permit_cid,
                source_artifact_cid,
                compiler_profile_cid,
                active_lease: None,
                completed_work_record: None,
            },
        );
        Ok(())
    }

    /// Claim or reclaim an expired `DelegatedEncodeJobV1` lease (`<= 300_000 ms`).
    pub fn claim_encode_lease(
        &mut self,
        lease: DelegatedJobLeaseWireV1,
        now_unix_ms: u64,
    ) -> Result<ActiveDelegatedLease, DelegatedJobError> {
        if lease.lease_ttl_ms == 0 || lease.lease_ttl_ms > MAX_DELEGATED_JOB_LEASE_TTL_MS {
            return Err(DelegatedJobError::InvalidLeaseTtl(lease.lease_ttl_ms));
        }
        if lease.worker_node_id.as_bytes() == &[0; 32] {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        let job = self
            .encode_jobs
            .get_mut(&lease.job_id)
            .ok_or(DelegatedJobError::UnknownJob(lease.job_id))?;
        if job.permit_cid != lease.permit_cid {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        if let Some(existing) = &job.active_lease {
            if !existing.is_expired_at(now_unix_ms)
                && existing.worker_node_id != lease.worker_node_id
            {
                return Err(DelegatedJobError::JobAlreadyLeased {
                    expires_at_ms: existing.expires_at_unix_ms,
                });
            }
        }
        let vk = VerifyingKey::from_bytes(&job.requester_public_key)
            .map_err(|_| DelegatedJobError::InvalidSignature)?;
        let preimage = delegated_lease_signing_preimage(
            lease.job_id,
            lease.worker_node_id,
            lease.permit_cid,
            lease.lease_ttl_ms,
            lease.issued_at_unix_ms,
        );
        vk.verify(
            &preimage,
            &Signature::from_bytes(&lease.requester_signature),
        )
        .map_err(|_| DelegatedJobError::InvalidSignature)?;

        let expires_at_unix_ms = lease
            .issued_at_unix_ms
            .saturating_add(u64::from(lease.lease_ttl_ms));
        if now_unix_ms >= expires_at_unix_ms {
            return Err(DelegatedJobError::LeaseExpired {
                expires_at_ms: expires_at_unix_ms,
                now_ms: now_unix_ms,
            });
        }
        let active = ActiveDelegatedLease {
            worker_node_id: lease.worker_node_id,
            issued_at_unix_ms: lease.issued_at_unix_ms,
            expires_at_unix_ms,
            wire: lease,
        };
        job.active_lease = Some(active.clone());
        Ok(active)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_encode_job(
        &mut self,
        job_id: [u8; 32],
        worker_node_id: NodeId,
        now_unix_ms: u64,
        work_record_cid: ObjectCid,
        attestation_event_cid: EventCid,
        evidence_frontier_root: [u8; 32],
        policy_cid: ObjectCid,
    ) -> Result<(DelegatedWorkJobProof, RewardEvidenceNoticeV2), DelegatedJobError> {
        if work_record_cid.as_bytes() == &[0; 32]
            || evidence_frontier_root == [0; 32]
            || policy_cid.as_bytes() == &[0; 32]
        {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        let job = self
            .encode_jobs
            .get_mut(&job_id)
            .ok_or(DelegatedJobError::UnknownJob(job_id))?;
        let lease = job
            .active_lease
            .as_ref()
            .ok_or(DelegatedJobError::InvalidJobParameters)?;
        if lease.worker_node_id != worker_node_id {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        if lease.is_expired_at(now_unix_ms) {
            return Err(DelegatedJobError::LeaseExpired {
                expires_at_ms: lease.expires_at_unix_ms,
                now_ms: now_unix_ms,
            });
        }
        job.completed_work_record = Some(work_record_cid);
        let proof = DelegatedWorkJobProof {
            job_id: job.job_id,
            requester_actor_id: job.requester_actor_id,
            permit_cid: job.permit_cid,
            source_artifact_cid: job.source_artifact_cid,
            compiler_profile_cid: job.compiler_profile_cid,
        };
        let notice = RewardEvidenceNoticeV2::AcceptedWork(AcceptedWorkNotice {
            work_record_cid,
            attestation_event_cid,
            work_kind: AcceptedWorkKind::DelegatedEncode,
            evidence_frontier_root,
            policy_cid,
        });
        Ok((proof, notice))
    }

    // ─── Two-Phase Commit-Before-Reveal `BlindVerifyJobV1` ───────────────────

    pub fn register_blind_verify_job(
        &mut self,
        job_id: [u8; 32],
        source_span_cid: ObjectCid,
        verifier_feed_id: FeedId,
        verifier_public_key: [u8; 32],
    ) -> Result<(), DelegatedJobError> {
        if job_id == [0; 32]
            || source_span_cid.as_bytes() == &[0; 32]
            || verifier_feed_id.as_bytes() == &[0; 32]
            || verifier_public_key == [0; 32]
        {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        self.blind_verify_jobs.insert(
            job_id,
            BlindVerifyJobV1 {
                job_id,
                source_span_cid,
                verifier_feed_id,
                verifier_public_key,
                phase: BlindVerifyPhase::AwaitingCommit,
            },
        );
        Ok(())
    }

    /// Phase 1 (`FID-002`): accept `BlindVerifyCommitWireV1` before target/peer outputs are
    /// disclosed, returning the deterministic `commitment_ack_digest`.
    pub fn submit_blind_verify_commit(
        &mut self,
        commit: BlindVerifyCommitWireV1,
    ) -> Result<[u8; 32], DelegatedJobError> {
        if commit.attempt_semantic_cid_salted_hash == [0; 32]
            || commit.correlation_declaration_bytes.is_empty()
        {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        let job = self
            .blind_verify_jobs
            .get_mut(&commit.job_id)
            .ok_or(DelegatedJobError::UnknownJob(commit.job_id))?;
        if job.verifier_feed_id != commit.verifier_feed_id
            || job.source_span_cid != commit.source_span_cid
        {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        if matches!(job.phase, BlindVerifyPhase::Revealed { .. }) {
            return Err(DelegatedJobError::AlreadyRevealed);
        }
        let vk = VerifyingKey::from_bytes(&job.verifier_public_key)
            .map_err(|_| DelegatedJobError::InvalidSignature)?;
        let preimage = blind_commit_signing_preimage(
            commit.job_id,
            commit.verifier_feed_id,
            commit.source_span_cid,
            commit.attempt_semantic_cid_salted_hash,
            &commit.correlation_declaration_bytes,
        );
        vk.verify(
            &preimage,
            &Signature::from_bytes(&commit.verifier_signature),
        )
        .map_err(|_| DelegatedJobError::InvalidSignature)?;

        let mut ack_hasher = blake3::Hasher::new();
        ack_hasher.update(BLIND_COMMIT_ACK_DOMAIN);
        ack_hasher.update(&commit.job_id);
        ack_hasher.update(commit.verifier_feed_id.as_bytes());
        ack_hasher.update(&commit.attempt_semantic_cid_salted_hash);
        let commitment_ack_digest = *ack_hasher.finalize().as_bytes();

        job.phase = BlindVerifyPhase::Committed {
            attempt_semantic_cid_salted_hash: commit.attempt_semantic_cid_salted_hash,
            commitment_ack_digest,
            correlation_declaration_bytes: commit.correlation_declaration_bytes,
        };
        Ok(commitment_ack_digest)
    }

    /// Phase 2 (`FID-002`): verify `BlindVerifyRevealWireV1` against the prior Phase 1 commit
    /// (`CommitMustPrecedeReveal` if absent!) and emit post-commit `AcceptedWorkNotice`.
    pub fn submit_blind_verify_reveal(
        &mut self,
        reveal: BlindVerifyRevealWireV1,
        attempt_object_cid: ObjectCid,
        attestation_event_cid: EventCid,
        evidence_frontier_root: [u8; 32],
        policy_cid: ObjectCid,
    ) -> Result<(BlindCommitRevealProof, RewardEvidenceNoticeV2), DelegatedJobError> {
        if attempt_object_cid.as_bytes() == &[0; 32]
            || evidence_frontier_root == [0; 32]
            || policy_cid.as_bytes() == &[0; 32]
            || reveal.reveal_salt == [0; 32]
            || reveal.encoding_attempt_bytes.is_empty()
            || reveal.fidelity_attestation_bytes.is_empty()
        {
            return Err(DelegatedJobError::InvalidJobParameters);
        }
        let job = self
            .blind_verify_jobs
            .get_mut(&reveal.job_id)
            .ok_or(DelegatedJobError::UnknownJob(reveal.job_id))?;
        let (committed_hash, expected_ack) = match &job.phase {
            BlindVerifyPhase::AwaitingCommit => {
                return Err(DelegatedJobError::CommitMustPrecedeReveal)
            }
            BlindVerifyPhase::Revealed { .. } => return Err(DelegatedJobError::AlreadyRevealed),
            BlindVerifyPhase::Committed {
                attempt_semantic_cid_salted_hash,
                commitment_ack_digest,
                ..
            } => (*attempt_semantic_cid_salted_hash, *commitment_ack_digest),
        };
        if reveal.commitment_ack_digest != expected_ack {
            return Err(DelegatedJobError::CommitmentAckMismatch);
        }
        let vk = VerifyingKey::from_bytes(&job.verifier_public_key)
            .map_err(|_| DelegatedJobError::InvalidSignature)?;
        let preimage = blind_reveal_signing_preimage(
            reveal.job_id,
            reveal.commitment_ack_digest,
            reveal.reveal_salt,
            &reveal.encoding_attempt_bytes,
            &reveal.fidelity_attestation_bytes,
        );
        vk.verify(
            &preimage,
            &Signature::from_bytes(&reveal.verifier_signature),
        )
        .map_err(|_| DelegatedJobError::InvalidSignature)?;

        let reveal_output_digest = compute_blind_reveal_output_digest(
            &reveal.encoding_attempt_bytes,
            &reveal.fidelity_attestation_bytes,
        );
        let proof = BlindCommitRevealProof::new_committed(
            attempt_object_cid,
            reveal_output_digest,
            reveal.reveal_salt,
        );
        if proof.attempt_commit_digest != committed_hash || proof.validate().is_err() {
            return Err(DelegatedJobError::RevealHashMismatch);
        }

        job.phase = BlindVerifyPhase::Revealed {
            proof: proof.clone(),
            encoding_attempt_bytes: reveal.encoding_attempt_bytes,
            fidelity_attestation_bytes: reveal.fidelity_attestation_bytes,
        };
        let notice = RewardEvidenceNoticeV2::AcceptedWork(AcceptedWorkNotice {
            work_record_cid: attempt_object_cid,
            attestation_event_cid,
            work_kind: AcceptedWorkKind::BlindFidelityVerify,
            evidence_frontier_root,
            policy_cid,
        });
        Ok((proof, notice))
    }

    pub fn blind_verify_job(&self, job_id: &[u8; 32]) -> Option<&BlindVerifyJobV1> {
        self.blind_verify_jobs.get(job_id)
    }
}
