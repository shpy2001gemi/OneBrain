//! Pre-finality challenge window, non-grindable VRF audit selection, and `FinalizedMintCertificate` (`§5.1`).
//!
//! Approved Track A and Track B claims create non-spendable, non-transferable
//! `PendingMintRecord`s. If an authorized `ChallengeDispute` arrives before the
//! challenge window completes, the mint transitions to `MintLifecycleState::VoidedOrFrozen`
//! with zero clawback required.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, VerifyingKey};
use ku_core::foundation::{
    ActorId, CanonicalError, CanonicalErrorKind, CanonicalValue, EventCid, FeedAuthorityDecision,
    FeedEventSigner, FeedId, KeyStateReducer, ObjectCid, PermitCid, ProvenFeedEventSigner,
    ResourceProfile, ValidatedFeedInception,
};
use thiserror::Error;

use crate::claims::{ClaimEvaluationOutcome, ContributorRole};
use crate::codec::{
    decode_value, domain_digest, encode_actor, encode_event_cid, encode_feed, encode_object_cid,
    encode_permit_cid, encode_value, expect_actor, expect_array, expect_bytes_exact,
    expect_event_cid, expect_feed, expect_map, expect_object_cid, expect_permit_cid, expect_text,
    expect_u16, expect_u64, req_field,
};
use crate::nullifier::ScopedNullifier;
use crate::policy::RewardPolicyV1;
use crate::units::ObtAmount;

/// Domain prefix for deterministic non-grindable challenge auditor VRF selection (`§5.1.2`).
pub const AUDIT_VRF_DOMAIN: &[u8] = b"onebrain:vnext:obt-audit-vrf:1\0";

/// Domain prefix for `PendingMintRecord` CID derivation.
pub const PENDING_MINT_DOMAIN: &[u8] = b"onebrain:vnext:obt-pending-mint:1\0";

/// Domain prefix for `FinalizedMintCertificate` CID derivation.
pub const FINALIZED_MINT_DOMAIN: &[u8] = b"onebrain:vnext:obt-finalized-mint:1\0";

const AUDIT_COSIG_DOMAIN: &[u8] = b"onebrain:vnext:obt-mint-audit-cosig:1\0";

/// Deterministic non-grindable auditor selection over active `(ActorId, PermitCid)` evaluators.
///
/// Uses `"onebrain:vnext:obt-audit-vrf:1\0"` binding commit-reveal `epoch_entropy_reveal`,
/// `claim_cid`, `ActorId`, and `PermitCid` — never `BLAKE3(ku_cid || epoch)`.
/// Deduplicates by `ActorId` so a single principal with multiple permits cannot multiply its weight.
pub fn select_challenge_auditors(
    epoch_entropy_reveal: &[u8; 32],
    claim_cid: &[u8; 32],
    active_evaluators: &[(ActorId, PermitCid)],
    k: usize,
) -> Vec<(ActorId, PermitCid)> {
    if k == 0 || active_evaluators.is_empty() {
        return Vec::new();
    }

    // Canonical deduplication by ActorId (choosing smallest PermitCid bytes on tie)
    // so slice order or permit-stuffing never alters the committee.
    let mut unique_by_actor: BTreeMap<ActorId, PermitCid> = BTreeMap::new();
    for &(actor, permit) in active_evaluators {
        unique_by_actor
            .entry(actor)
            .and_modify(|existing| {
                if permit.as_bytes() < existing.as_bytes() {
                    *existing = permit;
                }
            })
            .or_insert(permit);
    }

    let mut scored: Vec<([u8; 32], ActorId, PermitCid)> = unique_by_actor
        .into_iter()
        .map(|(actor, permit)| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(AUDIT_VRF_DOMAIN);
            hasher.update(epoch_entropy_reveal);
            hasher.update(claim_cid);
            hasher.update(actor.as_bytes());
            hasher.update(permit.as_bytes());
            let ticket = *hasher.finalize().as_bytes();
            (ticket, actor, permit)
        })
        .collect();

    scored.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.as_bytes().cmp(right.2.as_bytes()))
    });

    scored
        .into_iter()
        .take(k)
        .map(|(_, actor, permit)| (actor, permit))
        .collect()
}

/// Provenance of a `PendingMintRecord`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingMintSource {
    WorkClaim {
        claim_cid: [u8; 32],
    },
    BenefitClaim {
        claim_cid: [u8; 32],
        contributor_role: ContributorRole,
        share_bps: u16,
        payout_index: u32,
    },
}

impl PendingMintSource {
    pub const fn claim_cid(&self) -> [u8; 32] {
        match self {
            Self::WorkClaim { claim_cid } | Self::BenefitClaim { claim_cid, .. } => *claim_cid,
        }
    }

    pub(crate) fn to_canonical_value(self) -> CanonicalValue {
        match self {
            Self::WorkClaim { claim_cid } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Bytes(claim_cid.to_vec())),
            ]),
            Self::BenefitClaim {
                claim_cid,
                contributor_role,
                share_bps,
                payout_index,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(claim_cid.to_vec())),
                (2, CanonicalValue::Unsigned(contributor_role as u64)),
                (3, CanonicalValue::Unsigned(u64::from(share_bps))),
                (4, CanonicalValue::Unsigned(u64::from(payout_index))),
            ]),
        }
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2, 3, 4])?;
        match expect_u64(req_field(entries, 0)?)? {
            0 => Ok(Self::WorkClaim {
                claim_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
            }),
            1 => {
                let role = ContributorRole::from_code(expect_u64(req_field(entries, 2)?)?)
                    .ok_or_else(|| CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0))?;
                Ok(Self::BenefitClaim {
                    claim_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
                    contributor_role: role,
                    share_bps: expect_u16(req_field(entries, 3)?)?,
                    payout_index: u32::try_from(expect_u64(req_field(entries, 4)?)?)
                        .map_err(|_| CanonicalError::new(CanonicalErrorKind::LimitItems, 0))?,
                })
            }
            _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
        }
    }
}

/// Authorized challenge dispute voiding or freezing a `PendingMintRecord` before finalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChallengeDispute {
    HarmfulOutcomeObserved {
        outcome_event_cid: EventCid,
        outcome_case_id: [u8; 32],
    },
    AccountEquivocationWarrant {
        warrant_cid: [u8; 32],
        equivocating_actor: ActorId,
        seq: u64,
    },
    InvalidWorkProof {
        proof_cid: [u8; 32],
        reason_code: String,
    },
}

impl ChallengeDispute {
    pub(crate) fn to_canonical_value(&self) -> CanonicalValue {
        match self {
            Self::HarmfulOutcomeObserved {
                outcome_event_cid,
                outcome_case_id,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, encode_event_cid(*outcome_event_cid)),
                (2, CanonicalValue::Bytes(outcome_case_id.to_vec())),
            ]),
            Self::AccountEquivocationWarrant {
                warrant_cid,
                equivocating_actor,
                seq,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(warrant_cid.to_vec())),
                (2, encode_actor(*equivocating_actor)),
                (3, CanonicalValue::Unsigned(*seq)),
            ]),
            Self::InvalidWorkProof {
                proof_cid,
                reason_code,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, CanonicalValue::Bytes(proof_cid.to_vec())),
                (2, CanonicalValue::Text(reason_code.clone())),
            ]),
        }
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2, 3])?;
        match expect_u64(req_field(entries, 0)?)? {
            0 => Ok(Self::HarmfulOutcomeObserved {
                outcome_event_cid: expect_event_cid(req_field(entries, 1)?)?,
                outcome_case_id: expect_bytes_exact::<32>(req_field(entries, 2)?)?,
            }),
            1 => Ok(Self::AccountEquivocationWarrant {
                warrant_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
                equivocating_actor: expect_actor(req_field(entries, 2)?)?,
                seq: expect_u64(req_field(entries, 3)?)?,
            }),
            2 => Ok(Self::InvalidWorkProof {
                proof_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
                reason_code: expect_text(req_field(entries, 2)?, 128)?,
            }),
            _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
        }
    }
}

/// Lifecycle state of a `PendingMintRecord` (`§5.1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MintLifecycleState {
    PendingInChallengeWindow {
        opened_at_frontier_seq: u64,
        closes_at_frontier_seq: u64,
    },
    VoidedOrFrozen {
        dispute: ChallengeDispute,
        voided_at_frontier_seq: u64,
    },
    Finalized {
        certificate_cid: [u8; 32],
    },
}

impl MintLifecycleState {
    pub(crate) fn to_canonical_value(&self) -> CanonicalValue {
        match self {
            Self::PendingInChallengeWindow {
                opened_at_frontier_seq,
                closes_at_frontier_seq,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Unsigned(*opened_at_frontier_seq)),
                (2, CanonicalValue::Unsigned(*closes_at_frontier_seq)),
            ]),
            Self::VoidedOrFrozen {
                dispute,
                voided_at_frontier_seq,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, dispute.to_canonical_value()),
                (2, CanonicalValue::Unsigned(*voided_at_frontier_seq)),
            ]),
            Self::Finalized { certificate_cid } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, CanonicalValue::Bytes(certificate_cid.to_vec())),
            ]),
        }
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2])?;
        match expect_u64(req_field(entries, 0)?)? {
            0 => Ok(Self::PendingInChallengeWindow {
                opened_at_frontier_seq: expect_u64(req_field(entries, 1)?)?,
                closes_at_frontier_seq: expect_u64(req_field(entries, 2)?)?,
            }),
            1 => Ok(Self::VoidedOrFrozen {
                dispute: ChallengeDispute::from_canonical_value(req_field(entries, 1)?)?,
                voided_at_frontier_seq: expect_u64(req_field(entries, 2)?)?,
            }),
            2 => Ok(Self::Finalized {
                certificate_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
            }),
            _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
        }
    }
}

/// Signed auditor co-signature over a `PendingMintRecord` after VRF committee selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditCoSignature {
    pub auditor_actor: ActorId,
    pub auditor_permit: PermitCid,
    pub auditor_feed: FeedId,
    pub auditor_public_key: [u8; 32],
    pub pending_mint_cid: [u8; 32],
    pub signature: [u8; 64],
}

impl AuditCoSignature {
    pub fn signing_message(
        pending_mint_cid: &[u8; 32],
        claim_cid: &[u8; 32],
        epoch_entropy_reveal: &[u8; 32],
    ) -> Vec<u8> {
        let mut msg = Vec::with_capacity(AUDIT_COSIG_DOMAIN.len() + 96);
        msg.extend_from_slice(AUDIT_COSIG_DOMAIN);
        msg.extend_from_slice(pending_mint_cid);
        msg.extend_from_slice(claim_cid);
        msg.extend_from_slice(epoch_entropy_reveal);
        msg
    }

    pub fn sign(
        auditor_actor: ActorId,
        auditor_permit: PermitCid,
        feed: &ValidatedFeedInception,
        signer: &dyn FeedEventSigner,
        pending_mint_cid: [u8; 32],
        claim_cid: [u8; 32],
        epoch_entropy_reveal: [u8; 32],
    ) -> Result<Self, MintPipelineError> {
        let proven = ProvenFeedEventSigner::prove_for_public_key(
            signer,
            feed.signed.inception.feed_public_key,
        )
        .map_err(|_| MintPipelineError::InvalidAuditorSignature)?;
        let msg = Self::signing_message(&pending_mint_cid, &claim_cid, &epoch_entropy_reveal);
        let signature = proven
            .sign_feed_event(&msg)
            .map_err(|_| MintPipelineError::InvalidAuditorSignature)?;
        Ok(Self {
            auditor_actor,
            auditor_permit,
            auditor_feed: feed.feed_id,
            auditor_public_key: feed.signed.inception.feed_public_key,
            pending_mint_cid,
            signature,
        })
    }

    pub fn verify_crypto(
        &self,
        claim_cid: &[u8; 32],
        epoch_entropy_reveal: &[u8; 32],
    ) -> Result<(), MintPipelineError> {
        let vk = VerifyingKey::from_bytes(&self.auditor_public_key)
            .map_err(|_| MintPipelineError::InvalidAuditorSignature)?;
        let msg = Self::signing_message(&self.pending_mint_cid, claim_cid, epoch_entropy_reveal);
        vk.verify_strict(&msg, &Signature::from_bytes(&self.signature))
            .map_err(|_| MintPipelineError::InvalidAuditorSignature)
    }

    pub(crate) fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, encode_actor(self.auditor_actor)),
            (1, encode_permit_cid(self.auditor_permit)),
            (2, encode_feed(self.auditor_feed)),
            (3, CanonicalValue::Bytes(self.auditor_public_key.to_vec())),
            (4, CanonicalValue::Bytes(self.pending_mint_cid.to_vec())),
            (5, CanonicalValue::Bytes(self.signature.to_vec())),
        ])
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2, 3, 4, 5])?;
        Ok(Self {
            auditor_actor: expect_actor(req_field(entries, 0)?)?,
            auditor_permit: expect_permit_cid(req_field(entries, 1)?)?,
            auditor_feed: expect_feed(req_field(entries, 2)?)?,
            auditor_public_key: expect_bytes_exact::<32>(req_field(entries, 3)?)?,
            pending_mint_cid: expect_bytes_exact::<32>(req_field(entries, 4)?)?,
            signature: expect_bytes_exact::<64>(req_field(entries, 5)?)?,
        })
    }
}

/// Non-spendable, non-transferable pre-finality mint claim (`§5.1.1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingMintRecord {
    pub pending_mint_cid: [u8; 32],
    pub source: PendingMintSource,
    pub beneficiary: ActorId,
    pub amount: ObtAmount,
    pub claim_nullifier: ScopedNullifier,
    pub credit_nullifier: ScopedNullifier,
    pub policy_cid: ObjectCid,
    pub opened_at_frontier_seq: u64,
    pub closes_at_frontier_seq: u64,
    pub required_audit_quorum: u16,
    pub state: MintLifecycleState,
}

impl PendingMintRecord {
    pub fn new(
        source: PendingMintSource,
        beneficiary: ActorId,
        amount: ObtAmount,
        claim_nullifier: ScopedNullifier,
        credit_nullifier: ScopedNullifier,
        policy: &RewardPolicyV1,
        opened_at_frontier_seq: u64,
    ) -> Self {
        let closes_at_frontier_seq =
            opened_at_frontier_seq.saturating_add(policy.challenge_window_frontier_span);
        let policy_cid = policy.policy_cid();
        let identity_value = CanonicalValue::Map(vec![
            (0, source.to_canonical_value()),
            (1, encode_actor(beneficiary)),
            (2, amount.to_canonical_value()),
            (3, claim_nullifier.to_canonical_value()),
            (4, credit_nullifier.to_canonical_value()),
            (5, encode_object_cid(policy_cid)),
            (6, CanonicalValue::Unsigned(opened_at_frontier_seq)),
            (7, CanonicalValue::Unsigned(closes_at_frontier_seq)),
            (
                8,
                CanonicalValue::Unsigned(u64::from(policy.required_audit_quorum)),
            ),
        ]);
        let bytes = encode_value(&identity_value, ResourceProfile::ControlV1)
            .expect("PendingMintRecord identity fits ControlV1");
        let pending_mint_cid = domain_digest(PENDING_MINT_DOMAIN, &bytes);
        Self {
            pending_mint_cid,
            source,
            beneficiary,
            amount,
            claim_nullifier,
            credit_nullifier,
            policy_cid,
            opened_at_frontier_seq,
            closes_at_frontier_seq,
            required_audit_quorum: policy.required_audit_quorum,
            state: MintLifecycleState::PendingInChallengeWindow {
                opened_at_frontier_seq,
                closes_at_frontier_seq,
            },
        }
    }

    /// `PendingMintRecord` is strictly non-spendable (`§5.1.1`).
    pub const fn is_spendable(&self) -> bool {
        false
    }

    /// `PendingMintRecord` is strictly non-transferable (`§5.1.1`).
    pub const fn is_transferable(&self) -> bool {
        false
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.pending_mint_cid.to_vec())),
            (1, self.source.to_canonical_value()),
            (2, encode_actor(self.beneficiary)),
            (3, self.amount.to_canonical_value()),
            (4, self.claim_nullifier.to_canonical_value()),
            (5, self.credit_nullifier.to_canonical_value()),
            (6, encode_object_cid(self.policy_cid)),
            (7, CanonicalValue::Unsigned(self.opened_at_frontier_seq)),
            (8, CanonicalValue::Unsigned(self.closes_at_frontier_seq)),
            (
                9,
                CanonicalValue::Unsigned(u64::from(self.required_audit_quorum)),
            ),
            (10, self.state.to_canonical_value()),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ControlV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_value(bytes, ResourceProfile::ControlV1)?;
        let entries = expect_map(&value, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10])?;
        Ok(Self {
            pending_mint_cid: expect_bytes_exact::<32>(req_field(entries, 0)?)?,
            source: PendingMintSource::from_canonical_value(req_field(entries, 1)?)?,
            beneficiary: expect_actor(req_field(entries, 2)?)?,
            amount: ObtAmount::from_canonical_value(req_field(entries, 3)?)?,
            claim_nullifier: ScopedNullifier::from_canonical_value(req_field(entries, 4)?)?,
            credit_nullifier: ScopedNullifier::from_canonical_value(req_field(entries, 5)?)?,
            policy_cid: expect_object_cid(req_field(entries, 6)?)?,
            opened_at_frontier_seq: expect_u64(req_field(entries, 7)?)?,
            closes_at_frontier_seq: expect_u64(req_field(entries, 8)?)?,
            required_audit_quorum: expect_u16(req_field(entries, 9)?)?,
            state: MintLifecycleState::from_canonical_value(req_field(entries, 10)?)?,
        })
    }
}

/// Certificate emitted once a `PendingMintRecord` completes its challenge window uncontested
/// and collects the required VRF-selected auditor co-signatures (`§5.1.3`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizedMintCertificate {
    pub certificate_cid: [u8; 32],
    pub pending_mint_cid: [u8; 32],
    pub claim_cid: [u8; 32],
    pub beneficiary: ActorId,
    pub amount: ObtAmount,
    pub claim_nullifier: ScopedNullifier,
    pub credit_nullifier: ScopedNullifier,
    pub policy_cid: ObjectCid,
    pub epoch_entropy_reveal: [u8; 32],
    pub finalized_at_frontier_seq: u64,
    pub audit_cosignatures: Vec<AuditCoSignature>,
}

impl FinalizedMintCertificate {
    pub fn verify_self_contained(&self) -> Result<(), MintPipelineError> {
        if self.amount.is_zero()
            || self.epoch_entropy_reveal == [0u8; 32]
            || self.audit_cosignatures.is_empty()
        {
            return Err(MintPipelineError::InvalidCertificate);
        }
        let mut seen_auditors = BTreeSet::new();
        for cosig in &self.audit_cosignatures {
            if cosig.pending_mint_cid != self.pending_mint_cid
                || !seen_auditors.insert(cosig.auditor_actor)
            {
                return Err(MintPipelineError::InvalidAuditorSignature);
            }
            cosig.verify_crypto(&self.claim_cid, &self.epoch_entropy_reveal)?;
        }
        let expected = self.compute_cid();
        if self.certificate_cid != expected {
            return Err(MintPipelineError::InvalidCertificate);
        }
        Ok(())
    }

    pub(crate) fn compute_cid(&self) -> [u8; 32] {
        let cosigs = self
            .audit_cosignatures
            .iter()
            .map(AuditCoSignature::to_canonical_value)
            .collect();
        let body = CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.pending_mint_cid.to_vec())),
            (1, CanonicalValue::Bytes(self.claim_cid.to_vec())),
            (2, encode_actor(self.beneficiary)),
            (3, self.amount.to_canonical_value()),
            (4, self.claim_nullifier.to_canonical_value()),
            (5, self.credit_nullifier.to_canonical_value()),
            (6, encode_object_cid(self.policy_cid)),
            (7, CanonicalValue::Bytes(self.epoch_entropy_reveal.to_vec())),
            (8, CanonicalValue::Unsigned(self.finalized_at_frontier_seq)),
            (9, CanonicalValue::Array(cosigs)),
        ]);
        let bytes = encode_value(&body, ResourceProfile::ControlV1)
            .expect("FinalizedMintCertificate fits ControlV1");
        domain_digest(FINALIZED_MINT_DOMAIN, &bytes)
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        let cosigs = self
            .audit_cosignatures
            .iter()
            .map(AuditCoSignature::to_canonical_value)
            .collect();
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.certificate_cid.to_vec())),
            (1, CanonicalValue::Bytes(self.pending_mint_cid.to_vec())),
            (2, CanonicalValue::Bytes(self.claim_cid.to_vec())),
            (3, encode_actor(self.beneficiary)),
            (4, self.amount.to_canonical_value()),
            (5, self.claim_nullifier.to_canonical_value()),
            (6, self.credit_nullifier.to_canonical_value()),
            (7, encode_object_cid(self.policy_cid)),
            (8, CanonicalValue::Bytes(self.epoch_entropy_reveal.to_vec())),
            (9, CanonicalValue::Unsigned(self.finalized_at_frontier_seq)),
            (10, CanonicalValue::Array(cosigs)),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ControlV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_value(bytes, ResourceProfile::ControlV1)?;
        let entries = expect_map(&value, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10])?;
        let audit_cosignatures = expect_array(req_field(entries, 10)?)?
            .iter()
            .map(AuditCoSignature::from_canonical_value)
            .collect::<Result<Vec<_>, _>>()?;
        let cert = Self {
            certificate_cid: expect_bytes_exact::<32>(req_field(entries, 0)?)?,
            pending_mint_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
            claim_cid: expect_bytes_exact::<32>(req_field(entries, 2)?)?,
            beneficiary: expect_actor(req_field(entries, 3)?)?,
            amount: ObtAmount::from_canonical_value(req_field(entries, 4)?)?,
            claim_nullifier: ScopedNullifier::from_canonical_value(req_field(entries, 5)?)?,
            credit_nullifier: ScopedNullifier::from_canonical_value(req_field(entries, 6)?)?,
            policy_cid: expect_object_cid(req_field(entries, 7)?)?,
            epoch_entropy_reveal: expect_bytes_exact::<32>(req_field(entries, 8)?)?,
            finalized_at_frontier_seq: expect_u64(req_field(entries, 9)?)?,
            audit_cosignatures,
        };
        if cert.compute_cid() != cert.certificate_cid {
            return Err(CanonicalError::new(CanonicalErrorKind::ReencodeMismatch, 0));
        }
        Ok(cert)
    }
}

/// Errors returned by `MintPipeline`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum MintPipelineError {
    #[error("OBT_MINT_UNKNOWN_PENDING: pending mint not found")]
    UnknownPendingMint,
    #[error("OBT_MINT_WINDOW_STILL_OPEN: challenge window open until frontier {closes_at}, current {current}")]
    ChallengeWindowStillOpen { current: u64, closes_at: u64 },
    #[error("OBT_MINT_ALREADY_FINALIZED: pending mint already finalized")]
    AlreadyFinalized,
    #[error("OBT_MINT_VOIDED_OR_FROZEN: pending mint was voided or frozen by dispute")]
    AlreadyVoidedOrFrozen,
    #[error("OBT_MINT_UNAUTHORIZED_CHALLENGER: challenger feed is not AuthorizedRelative")]
    UnauthorizedChallenger,
    #[error("OBT_MINT_ZERO_ENTROPY: epoch_entropy_reveal must be non-zero")]
    ZeroEntropyReveal,
    #[error("OBT_MINT_AUDITOR_NOT_SELECTED: auditor {0} not in VRF-selected committee")]
    AuditorNotSelected(ActorId),
    #[error("OBT_MINT_AUDITOR_AUTHORITY_INVALID: auditor feed is not AuthorizedRelative")]
    InvalidAuditorAuthority,
    #[error("OBT_MINT_AUDITOR_SIGNATURE_INVALID: auditor signature invalid")]
    InvalidAuditorSignature,
    #[error("OBT_MINT_INSUFFICIENT_AUDIT_QUORUM: got {got} valid cosignatures, need {required}")]
    InsufficientAuditQuorum { got: usize, required: u16 },
    #[error("OBT_MINT_INVALID_CERTIFICATE: FinalizedMintCertificate failed verification")]
    InvalidCertificate,
}

/// Pre-finality mint state machine (`PendingMint -> ChallengeWindow -> VRF Audit -> FinalizedMint`).
#[derive(Clone, Debug, Default)]
pub struct MintPipeline {
    pub pending_mints: BTreeMap<[u8; 32], PendingMintRecord>,
    pub finalized_mints: BTreeMap<[u8; 32], FinalizedMintCertificate>,
}

impl MintPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage non-spendable `PendingMintRecord`(s) from an approved Track A or Track B outcome.
    pub fn stage_approved_claim(
        &mut self,
        outcome: &ClaimEvaluationOutcome,
        policy: &RewardPolicyV1,
        opened_at_frontier_seq: u64,
    ) -> Vec<PendingMintRecord> {
        let mut created = Vec::new();
        match outcome {
            ClaimEvaluationOutcome::WorkApproved {
                claim_cid,
                nullifier,
                beneficiary,
                amount,
                ..
            } => {
                let record = PendingMintRecord::new(
                    PendingMintSource::WorkClaim {
                        claim_cid: *claim_cid,
                    },
                    *beneficiary,
                    *amount,
                    *nullifier,
                    *nullifier,
                    policy,
                    opened_at_frontier_seq,
                );
                self.pending_mints
                    .entry(record.pending_mint_cid)
                    .or_insert_with(|| record.clone());
                created.push(record);
            }
            ClaimEvaluationOutcome::BenefitApproved {
                claim_cid,
                nullifier,
                distribution,
                ..
            } => {
                for (idx, payout) in distribution.payouts.iter().enumerate() {
                    if payout.amount.is_zero() {
                        continue;
                    }
                    let payout_index = idx as u32;
                    let credit_nullifier = ScopedNullifier::for_benefit_recipient(
                        *nullifier,
                        payout.role as u8,
                        payout.beneficiary,
                        payout_index,
                    );
                    let record = PendingMintRecord::new(
                        PendingMintSource::BenefitClaim {
                            claim_cid: *claim_cid,
                            contributor_role: payout.role,
                            share_bps: payout.share_bps,
                            payout_index,
                        },
                        payout.beneficiary,
                        payout.amount,
                        *nullifier,
                        credit_nullifier,
                        policy,
                        opened_at_frontier_seq,
                    );
                    self.pending_mints
                        .entry(record.pending_mint_cid)
                        .or_insert_with(|| record.clone());
                    created.push(record);
                }
            }
        }
        created
    }

    /// Submit an authorized `ChallengeDispute` before finalization, transitioning the
    /// `PendingMintRecord` to `MintLifecycleState::VoidedOrFrozen` (zero clawback needed).
    pub fn submit_challenge_dispute(
        &mut self,
        pending_mint_cid: [u8; 32],
        dispute: ChallengeDispute,
        challenger_feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        current_frontier_seq: u64,
    ) -> Result<&PendingMintRecord, MintPipelineError> {
        if !matches!(
            key_state.evaluate(challenger_feed),
            FeedAuthorityDecision::AuthorizedRelative { .. }
        ) {
            return Err(MintPipelineError::UnauthorizedChallenger);
        }
        let record = self
            .pending_mints
            .get_mut(&pending_mint_cid)
            .ok_or(MintPipelineError::UnknownPendingMint)?;
        match record.state {
            MintLifecycleState::Finalized { .. } => Err(MintPipelineError::AlreadyFinalized),
            MintLifecycleState::VoidedOrFrozen { .. } => {
                Err(MintPipelineError::AlreadyVoidedOrFrozen)
            }
            MintLifecycleState::PendingInChallengeWindow { .. } => {
                record.state = MintLifecycleState::VoidedOrFrozen {
                    dispute,
                    voided_at_frontier_seq: current_frontier_seq,
                };
                Ok(record)
            }
        }
    }

    /// Finalize an uncontested `PendingMintRecord` once `current_frontier_seq >= closes_at_frontier_seq`
    /// and VRF-selected auditor co-signatures satisfy `required_audit_quorum`.
    #[allow(clippy::too_many_arguments)]
    pub fn finalize_mint(
        &mut self,
        pending_mint_cid: [u8; 32],
        epoch_entropy_reveal: [u8; 32],
        active_evaluators: &[(ActorId, PermitCid)],
        committee_size: usize,
        cosignatures: Vec<AuditCoSignature>,
        auditor_feeds: &BTreeMap<FeedId, ValidatedFeedInception>,
        key_state: &KeyStateReducer,
        current_frontier_seq: u64,
    ) -> Result<FinalizedMintCertificate, MintPipelineError> {
        if epoch_entropy_reveal == [0u8; 32] {
            return Err(MintPipelineError::ZeroEntropyReveal);
        }
        let record = self
            .pending_mints
            .get(&pending_mint_cid)
            .ok_or(MintPipelineError::UnknownPendingMint)?
            .clone();

        match record.state {
            MintLifecycleState::VoidedOrFrozen { .. } => {
                return Err(MintPipelineError::AlreadyVoidedOrFrozen);
            }
            MintLifecycleState::Finalized { .. } => {
                return Err(MintPipelineError::AlreadyFinalized);
            }
            MintLifecycleState::PendingInChallengeWindow {
                closes_at_frontier_seq,
                ..
            } => {
                if current_frontier_seq < closes_at_frontier_seq {
                    return Err(MintPipelineError::ChallengeWindowStillOpen {
                        current: current_frontier_seq,
                        closes_at: closes_at_frontier_seq,
                    });
                }
            }
        }

        let claim_cid = record.source.claim_cid();
        let selected = select_challenge_auditors(
            &epoch_entropy_reveal,
            &claim_cid,
            active_evaluators,
            committee_size.max(usize::from(record.required_audit_quorum)),
        );
        let selected_set: BTreeSet<(ActorId, [u8; 32])> = selected
            .into_iter()
            .map(|(a, p)| (a, *p.as_bytes()))
            .collect();

        let mut valid_cosigs = Vec::new();
        let mut seen_actors = BTreeSet::new();
        for cosig in cosignatures {
            if cosig.pending_mint_cid != pending_mint_cid {
                return Err(MintPipelineError::InvalidAuditorSignature);
            }
            if !selected_set.contains(&(cosig.auditor_actor, *cosig.auditor_permit.as_bytes())) {
                return Err(MintPipelineError::AuditorNotSelected(cosig.auditor_actor));
            }
            if !seen_actors.insert(cosig.auditor_actor) {
                continue;
            }
            let feed = auditor_feeds
                .get(&cosig.auditor_feed)
                .ok_or(MintPipelineError::InvalidAuditorAuthority)?;
            if feed.signed.inception.feed_public_key != cosig.auditor_public_key {
                return Err(MintPipelineError::InvalidAuditorSignature);
            }
            match key_state.evaluate(feed) {
                FeedAuthorityDecision::AuthorizedRelative { actor, .. }
                    if actor == cosig.auditor_actor => {}
                _ => return Err(MintPipelineError::InvalidAuditorAuthority),
            }
            cosig.verify_crypto(&claim_cid, &epoch_entropy_reveal)?;
            valid_cosigs.push(cosig);
        }

        valid_cosigs.sort_by_key(|c| c.auditor_actor);
        if valid_cosigs.len() < usize::from(record.required_audit_quorum) {
            return Err(MintPipelineError::InsufficientAuditQuorum {
                got: valid_cosigs.len(),
                required: record.required_audit_quorum,
            });
        }

        let mut cert = FinalizedMintCertificate {
            certificate_cid: [0u8; 32],
            pending_mint_cid,
            claim_cid,
            beneficiary: record.beneficiary,
            amount: record.amount,
            claim_nullifier: record.claim_nullifier,
            credit_nullifier: record.credit_nullifier,
            policy_cid: record.policy_cid,
            epoch_entropy_reveal,
            finalized_at_frontier_seq: current_frontier_seq,
            audit_cosignatures: valid_cosigs,
        };
        cert.certificate_cid = cert.compute_cid();

        if let Some(slot) = self.pending_mints.get_mut(&pending_mint_cid) {
            slot.state = MintLifecycleState::Finalized {
                certificate_cid: cert.certificate_cid,
            };
        }
        self.finalized_mints
            .insert(cert.certificate_cid, cert.clone());
        Ok(cert)
    }
}
