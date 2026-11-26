//! CAP-honest `ActorId` `AccountBlockV1` chains, `R4` authority freshness, equivocation warrants,
//! and domain-checkpoint finality (`§5.2–5.3`).
//!
//! Offline or partitioned islands never incur slashing, jail, or balance decay (`NEG-P5-002`).
//! Conflicting blocks at the same `(ActorId, seq)` deterministically produce an
//! `AccountEquivocationWarrant` freezing both conflicting branches and unfinalized downstream
//! `ReceiveTransfer`s while leaving already `FinalInDomain` blocks untouched.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, VerifyingKey};
use ku_core::foundation::{
    signature_message, ActorId, AuthorityFreshnessObservation, AuthorityScope, CanonicalError,
    CanonicalErrorKind, CanonicalValue, EventCid, FeedAuthorityDecision, FeedEventSigner, FeedId,
    KeyStateReducer, ObjectCid, ProvenFeedEventSigner, ReservedDomain, ResourceProfile,
    RevocationAction, RevocationCheck, RevocationFreshnessDecision, RevocationFreshnessEvaluator,
    RevocationFreshnessProfile, RevocationRisk, ValidatedFeedInception,
};
use thiserror::Error;

use crate::codec::{
    decode_value, domain_digest, encode_actor, encode_event_cid, encode_feed, encode_object_cid,
    encode_u128, encode_value, expect_actor, expect_array, expect_bytes_exact, expect_event_cid,
    expect_feed, expect_map, expect_object_cid, expect_u128, expect_u64, map_lookup, req_field,
};
use crate::mint_pipeline::{FinalizedMintCertificate, MintPipelineError};
use crate::nullifier::ScopedNullifier;
use crate::units::{ObtAmount, ObtMathError};

/// Domain prefix for `AccountBlockV1` CID derivation.
pub const ACCOUNT_BLOCK_DOMAIN: &[u8] = b"onebrain:vnext:obt-account-block:1\0";

/// Domain prefix for `AccountEquivocationWarrant` CID derivation.
pub const ACCOUNT_WARRANT_DOMAIN: &[u8] = b"onebrain:vnext:obt-equivocation-warrant:1\0";

/// Domain prefix for `SettlementDomainCheckpoint` validator signing and CID derivation.
pub const SETTLEMENT_CHECKPOINT_DOMAIN: &[u8] = b"onebrain:vnext:obt-settlement-checkpoint:1\0";

/// Delegated authority effect tag for ledger block signing (`§3.2`).
pub const EFFECT_OBT_LEDGER_SIGN: &[u8] = b"EFFECT_OBT_LEDGER_SIGN";

/// Mandatory UI disclosure badge for `SettlementState::ProvisionalPartitionLocal` (`§5.2`).
pub const PROVISIONAL_SETTLEMENT_UI_BADGE: &str =
    "[Provisional — unquorumed partition state, subject to double-spend fork risk]";

/// Terrestrial `R4` live authority freshness window in seconds (`REV-001` `R4 = 60s`).
pub const R4_LIVE_FRESHNESS_MAX_AGE_SECS: u64 = 60;

/// Typed operation carried inside an `AccountBlockV1` (`§5.2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountBlockOp {
    OpenAccount {
        initial_policy_cid: ObjectCid,
    },
    CreditFinalizedMint {
        certificate: Box<FinalizedMintCertificate>,
    },
    SendTransfer {
        recipient: ActorId,
        amount: ObtAmount,
        expires_at_frontier_seq: Option<u64>,
        memo_commitment: Option<[u8; 32]>,
    },
    ReceiveTransfer {
        send_block_cid: [u8; 32],
        sender: ActorId,
        amount: ObtAmount,
    },
    CancelExpiredSend {
        send_block_cid: [u8; 32],
        refund_amount: ObtAmount,
        observed_frontier_seq: u64,
    },
}

impl AccountBlockOp {
    pub(crate) fn to_canonical_value(&self) -> CanonicalValue {
        match self {
            Self::OpenAccount { initial_policy_cid } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, encode_object_cid(*initial_policy_cid)),
            ]),
            Self::CreditFinalizedMint { certificate } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, certificate.to_canonical_value()),
            ]),
            Self::SendTransfer {
                recipient,
                amount,
                expires_at_frontier_seq,
                memo_commitment,
            } => {
                let mut fields = vec![
                    (0, CanonicalValue::Unsigned(2)),
                    (1, encode_actor(*recipient)),
                    (2, amount.to_canonical_value()),
                ];
                if let Some(exp) = expires_at_frontier_seq {
                    fields.push((3, CanonicalValue::Unsigned(*exp)));
                }
                if let Some(memo) = memo_commitment {
                    fields.push((4, CanonicalValue::Bytes(memo.to_vec())));
                }
                CanonicalValue::Map(fields)
            }
            Self::ReceiveTransfer {
                send_block_cid,
                sender,
                amount,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(3)),
                (1, CanonicalValue::Bytes(send_block_cid.to_vec())),
                (2, encode_actor(*sender)),
                (3, amount.to_canonical_value()),
            ]),
            Self::CancelExpiredSend {
                send_block_cid,
                refund_amount,
                observed_frontier_seq,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(4)),
                (1, CanonicalValue::Bytes(send_block_cid.to_vec())),
                (2, refund_amount.to_canonical_value()),
                (3, CanonicalValue::Unsigned(*observed_frontier_seq)),
            ]),
        }
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2, 3, 4])?;
        match expect_u64(req_field(entries, 0)?)? {
            0 => Ok(Self::OpenAccount {
                initial_policy_cid: expect_object_cid(req_field(entries, 1)?)?,
            }),
            1 => {
                let cert_bytes = encode_value(req_field(entries, 1)?, ResourceProfile::ControlV1)?;
                Ok(Self::CreditFinalizedMint {
                    certificate: Box::new(FinalizedMintCertificate::decode_canonical_bytes(
                        &cert_bytes,
                    )?),
                })
            }
            2 => Ok(Self::SendTransfer {
                recipient: expect_actor(req_field(entries, 1)?)?,
                amount: ObtAmount::from_canonical_value(req_field(entries, 2)?)?,
                expires_at_frontier_seq: map_lookup(entries, 3).map(expect_u64).transpose()?,
                memo_commitment: map_lookup(entries, 4)
                    .map(expect_bytes_exact::<32>)
                    .transpose()?,
            }),
            3 => Ok(Self::ReceiveTransfer {
                send_block_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
                sender: expect_actor(req_field(entries, 2)?)?,
                amount: ObtAmount::from_canonical_value(req_field(entries, 3)?)?,
            }),
            4 => Ok(Self::CancelExpiredSend {
                send_block_cid: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
                refund_amount: ObtAmount::from_canonical_value(req_field(entries, 2)?)?,
                observed_frontier_seq: expect_u64(req_field(entries, 3)?)?,
            }),
            _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
        }
    }
}

/// Canonical `ActorId`-custodied ledger block (`§5.2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountBlockV1 {
    pub account: ActorId,
    pub seq: u64,
    pub prev_block_cid: Option<[u8; 32]>,
    pub op: AccountBlockOp,
    pub balance_after: u128,
    pub nullifier: Option<ScopedNullifier>,
    pub signer_feed: FeedId,
    pub signer_public_key: [u8; 32],
    pub authority_frontier: EventCid,
    pub signature: [u8; 64],
}

impl AccountBlockV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        account: ActorId,
        seq: u64,
        prev_block_cid: Option<[u8; 32]>,
        op: AccountBlockOp,
        balance_after: u128,
        nullifier: Option<ScopedNullifier>,
        feed: &ValidatedFeedInception,
        authority_frontier: EventCid,
        signer: &dyn FeedEventSigner,
    ) -> Result<Self, AccountChainError> {
        let proven = ProvenFeedEventSigner::prove_for_public_key(
            signer,
            feed.signed.inception.feed_public_key,
        )
        .map_err(|_| AccountChainError::SignatureInvalid)?;
        let mut block = Self {
            account,
            seq,
            prev_block_cid,
            op,
            balance_after,
            nullifier,
            signer_feed: feed.feed_id,
            signer_public_key: feed.signed.inception.feed_public_key,
            authority_frontier,
            signature: [0u8; 64],
        };
        let msg = block.signing_preimage()?;
        block.signature = proven
            .sign_feed_event(&msg)
            .map_err(|_| AccountChainError::SignatureInvalid)?;
        Ok(block)
    }

    pub fn unsigned_canonical_value(&self) -> CanonicalValue {
        let mut fields = vec![
            (0, encode_actor(self.account)),
            (1, CanonicalValue::Unsigned(self.seq)),
            (3, self.op.to_canonical_value()),
            (4, encode_u128(self.balance_after)),
            (6, encode_feed(self.signer_feed)),
            (7, CanonicalValue::Bytes(self.signer_public_key.to_vec())),
            (8, encode_event_cid(self.authority_frontier)),
        ];
        if let Some(prev) = self.prev_block_cid {
            fields.push((2, CanonicalValue::Bytes(prev.to_vec())));
        }
        if let Some(nullifier) = self.nullifier {
            fields.push((5, nullifier.to_canonical_value()));
        }
        fields.sort_by_key(|(k, _)| *k);
        CanonicalValue::Map(fields)
    }

    pub fn signing_preimage(&self) -> Result<Vec<u8>, AccountChainError> {
        let unsigned = encode_value(&self.unsigned_canonical_value(), ResourceProfile::ControlV1)?;
        let mut domain_tagged = Vec::with_capacity(
            ACCOUNT_BLOCK_DOMAIN.len() + EFFECT_OBT_LEDGER_SIGN.len() + unsigned.len(),
        );
        domain_tagged.extend_from_slice(ACCOUNT_BLOCK_DOMAIN);
        domain_tagged.extend_from_slice(EFFECT_OBT_LEDGER_SIGN);
        domain_tagged.extend_from_slice(&unsigned);
        signature_message(ReservedDomain::Event, &domain_tagged)
            .map_err(|_| AccountChainError::SignatureInvalid)
    }

    pub fn verify_crypto(&self) -> Result<(), AccountChainError> {
        let vk = VerifyingKey::from_bytes(&self.signer_public_key)
            .map_err(|_| AccountChainError::SignatureInvalid)?;
        let msg = self.signing_preimage()?;
        vk.verify_strict(&msg, &Signature::from_bytes(&self.signature))
            .map_err(|_| AccountChainError::SignatureInvalid)
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.unsigned_canonical_value()),
            (1, CanonicalValue::Bytes(self.signature.to_vec())),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ControlV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let outer = decode_value(bytes, ResourceProfile::ControlV1)?;
        let outer_entries = expect_map(&outer, &[0, 1])?;
        let inner = expect_map(req_field(outer_entries, 0)?, &[0, 1, 2, 3, 4, 5, 6, 7, 8])?;
        let signature = expect_bytes_exact::<64>(req_field(outer_entries, 1)?)?;
        let block = Self {
            account: expect_actor(req_field(inner, 0)?)?,
            seq: expect_u64(req_field(inner, 1)?)?,
            prev_block_cid: map_lookup(inner, 2)
                .map(expect_bytes_exact::<32>)
                .transpose()?,
            op: AccountBlockOp::from_canonical_value(req_field(inner, 3)?)?,
            balance_after: expect_u128(req_field(inner, 4)?)?,
            nullifier: map_lookup(inner, 5)
                .map(ScopedNullifier::from_canonical_value)
                .transpose()?,
            signer_feed: expect_feed(req_field(inner, 6)?)?,
            signer_public_key: expect_bytes_exact::<32>(req_field(inner, 7)?)?,
            authority_frontier: expect_event_cid(req_field(inner, 8)?)?,
            signature,
        };
        block
            .verify_crypto()
            .map_err(|_| CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0))?;
        Ok(block)
    }

    pub fn block_cid(&self) -> [u8; 32] {
        let bytes = self
            .encode_canonical_bytes()
            .expect("AccountBlockV1 fits ControlV1 limits");
        domain_digest(ACCOUNT_BLOCK_DOMAIN, &bytes)
    }
}

/// Self-verifying proof that two distinct valid signed blocks exist at `(actor, seq)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountEquivocationWarrant {
    pub warrant_cid: [u8; 32],
    pub actor: ActorId,
    pub seq: u64,
    pub block_a_cid: [u8; 32],
    pub block_b_cid: [u8; 32],
    pub block_a_bytes: Vec<u8>,
    pub block_b_bytes: Vec<u8>,
}

impl AccountEquivocationWarrant {
    pub fn from_conflicting_blocks(
        left: &AccountBlockV1,
        right: &AccountBlockV1,
    ) -> Result<Self, AccountChainError> {
        left.verify_crypto()?;
        right.verify_crypto()?;
        if left.account != right.account || left.seq != right.seq {
            return Err(AccountChainError::InvalidEquivocationWarrant);
        }
        let left_cid = left.block_cid();
        let right_cid = right.block_cid();
        if left_cid == right_cid {
            return Err(AccountChainError::InvalidEquivocationWarrant);
        }
        let (a, a_cid, b, b_cid) = if left_cid < right_cid {
            (left, left_cid, right, right_cid)
        } else {
            (right, right_cid, left, left_cid)
        };
        let block_a_bytes = a.encode_canonical_bytes()?;
        let block_b_bytes = b.encode_canonical_bytes()?;

        let mut preimage = Vec::with_capacity(104);
        preimage.extend_from_slice(a.account.as_bytes());
        preimage.extend_from_slice(&a.seq.to_be_bytes());
        preimage.extend_from_slice(&a_cid);
        preimage.extend_from_slice(&b_cid);
        let warrant_cid = domain_digest(ACCOUNT_WARRANT_DOMAIN, &preimage);

        Ok(Self {
            warrant_cid,
            actor: a.account,
            seq: a.seq,
            block_a_cid: a_cid,
            block_b_cid: b_cid,
            block_a_bytes,
            block_b_bytes,
        })
    }

    pub fn verify(&self) -> Result<(AccountBlockV1, AccountBlockV1), AccountChainError> {
        let block_a = AccountBlockV1::decode_canonical_bytes(&self.block_a_bytes)?;
        let block_b = AccountBlockV1::decode_canonical_bytes(&self.block_b_bytes)?;
        let rebuilt = Self::from_conflicting_blocks(&block_a, &block_b)?;
        if &rebuilt != self {
            return Err(AccountChainError::InvalidEquivocationWarrant);
        }
        Ok((block_a, block_b))
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.warrant_cid.to_vec())),
            (1, encode_actor(self.actor)),
            (2, CanonicalValue::Unsigned(self.seq)),
            (3, CanonicalValue::Bytes(self.block_a_cid.to_vec())),
            (4, CanonicalValue::Bytes(self.block_b_cid.to_vec())),
            (5, CanonicalValue::Bytes(self.block_a_bytes.clone())),
            (6, CanonicalValue::Bytes(self.block_b_bytes.clone())),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ObjectV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_value(bytes, ResourceProfile::ObjectV1)?;
        let entries = expect_map(&value, &[0, 1, 2, 3, 4, 5, 6])?;
        let CanonicalValue::Bytes(block_a_bytes) = req_field(entries, 5)?.clone() else {
            return Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0));
        };
        let CanonicalValue::Bytes(block_b_bytes) = req_field(entries, 6)?.clone() else {
            return Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0));
        };
        let warrant = Self {
            warrant_cid: expect_bytes_exact::<32>(req_field(entries, 0)?)?,
            actor: expect_actor(req_field(entries, 1)?)?,
            seq: expect_u64(req_field(entries, 2)?)?,
            block_a_cid: expect_bytes_exact::<32>(req_field(entries, 3)?)?,
            block_b_cid: expect_bytes_exact::<32>(req_field(entries, 4)?)?,
            block_a_bytes,
            block_b_bytes,
        };
        warrant
            .verify()
            .map_err(|_| CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0))?;
        Ok(warrant)
    }
}

/// Validator entry in a named `SettlementDomainManifest` (`§5.2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementDomainValidator {
    pub validator_actor: ActorId,
    pub validator_feed: FeedId,
    pub public_key: [u8; 32],
    pub weight: u64,
}

/// Named closed quorum settlement domain manifest (`>= 2f + 1` threshold weight).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementDomainManifest {
    pub domain_id: [u8; 32],
    pub fault_weight_f: u64,
    pub validators: Vec<SettlementDomainValidator>,
}

impl SettlementDomainManifest {
    pub fn quorum_weight(&self) -> u64 {
        self.fault_weight_f.saturating_mul(2).saturating_add(1)
    }

    pub fn validate(&self) -> Result<(), AccountChainError> {
        if self.domain_id == [0u8; 32] || self.validators.is_empty() {
            return Err(AccountChainError::InvalidSettlementManifest);
        }
        let mut seen = BTreeSet::new();
        let mut total_weight: u64 = 0;
        for v in &self.validators {
            if v.weight == 0 || !seen.insert(v.validator_actor) {
                return Err(AccountChainError::InvalidSettlementManifest);
            }
            total_weight = total_weight
                .checked_add(v.weight)
                .ok_or(AccountChainError::InvalidSettlementManifest)?;
        }
        let min_bft_total = self
            .fault_weight_f
            .checked_mul(3)
            .and_then(|v| v.checked_add(1))
            .ok_or(AccountChainError::InvalidSettlementManifest)?;
        if total_weight < min_bft_total || total_weight < self.quorum_weight() {
            return Err(AccountChainError::InvalidSettlementManifest);
        }
        Ok(())
    }
}

/// Validator co-signature on a `SettlementDomainCheckpoint`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorCheckpointSig {
    pub validator_actor: ActorId,
    pub validator_feed: FeedId,
    pub signature: [u8; 64],
}

impl ValidatorCheckpointSig {
    pub(crate) fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, encode_actor(self.validator_actor)),
            (1, encode_feed(self.validator_feed)),
            (2, CanonicalValue::Bytes(self.signature.to_vec())),
        ])
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2])?;
        Ok(Self {
            validator_actor: expect_actor(req_field(entries, 0)?)?,
            validator_feed: expect_feed(req_field(entries, 1)?)?,
            signature: expect_bytes_exact::<64>(req_field(entries, 2)?)?,
        })
    }
}

/// Signed threshold checkpoint establishing `SettlementState::FinalInDomain` (`§5.2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettlementDomainCheckpoint {
    pub domain_id: [u8; 32],
    pub checkpoint_seq: u64,
    /// Sorted `(ActorId, finalized_up_to_seq, head_block_cid)` entries.
    pub finalized_heads: Vec<(ActorId, u64, [u8; 32])>,
    pub nullifier_set_root: [u8; 32],
    pub validator_signatures: Vec<ValidatorCheckpointSig>,
}

impl SettlementDomainCheckpoint {
    pub fn unsigned_canonical_value(&self) -> CanonicalValue {
        let mut heads = self.finalized_heads.clone();
        heads.sort_by_key(|(actor, _, _)| *actor);
        let head_values = heads
            .into_iter()
            .map(|(actor, seq, cid)| {
                CanonicalValue::Array(vec![
                    encode_actor(actor),
                    CanonicalValue::Unsigned(seq),
                    CanonicalValue::Bytes(cid.to_vec()),
                ])
            })
            .collect();
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.domain_id.to_vec())),
            (1, CanonicalValue::Unsigned(self.checkpoint_seq)),
            (2, CanonicalValue::Array(head_values)),
            (3, CanonicalValue::Bytes(self.nullifier_set_root.to_vec())),
        ])
    }

    pub fn signing_message(&self) -> Result<Vec<u8>, AccountChainError> {
        let unsigned = encode_value(
            &self.unsigned_canonical_value(),
            ResourceProfile::ManifestV1,
        )?;
        let mut msg = Vec::with_capacity(SETTLEMENT_CHECKPOINT_DOMAIN.len() + unsigned.len());
        msg.extend_from_slice(SETTLEMENT_CHECKPOINT_DOMAIN);
        msg.extend_from_slice(&unsigned);
        Ok(msg)
    }

    pub fn sign_validator(
        &self,
        validator_actor: ActorId,
        feed: &ValidatedFeedInception,
        signer: &dyn FeedEventSigner,
    ) -> Result<ValidatorCheckpointSig, AccountChainError> {
        let proven = ProvenFeedEventSigner::prove_for_public_key(
            signer,
            feed.signed.inception.feed_public_key,
        )
        .map_err(|_| AccountChainError::CheckpointSignatureInvalid)?;
        let msg = self.signing_message()?;
        let signature = proven
            .sign_feed_event(&msg)
            .map_err(|_| AccountChainError::CheckpointSignatureInvalid)?;
        Ok(ValidatorCheckpointSig {
            validator_actor,
            validator_feed: feed.feed_id,
            signature,
        })
    }

    pub fn verify_against_manifest(
        &self,
        manifest: &SettlementDomainManifest,
    ) -> Result<u64, AccountChainError> {
        manifest.validate()?;
        if self.domain_id != manifest.domain_id {
            return Err(AccountChainError::CheckpointDomainMismatch);
        }
        let msg = self.signing_message()?;
        let validators: BTreeMap<ActorId, &SettlementDomainValidator> = manifest
            .validators
            .iter()
            .map(|v| (v.validator_actor, v))
            .collect();

        let mut seen = BTreeSet::new();
        let mut signed_weight: u64 = 0;
        for sig in &self.validator_signatures {
            if !seen.insert(sig.validator_actor) {
                continue;
            }
            let v = validators
                .get(&sig.validator_actor)
                .ok_or(AccountChainError::CheckpointSignatureInvalid)?;
            if v.validator_feed != sig.validator_feed {
                return Err(AccountChainError::CheckpointSignatureInvalid);
            }
            let vk = VerifyingKey::from_bytes(&v.public_key)
                .map_err(|_| AccountChainError::CheckpointSignatureInvalid)?;
            vk.verify_strict(&msg, &Signature::from_bytes(&sig.signature))
                .map_err(|_| AccountChainError::CheckpointSignatureInvalid)?;
            signed_weight = signed_weight.saturating_add(v.weight);
        }
        let required = manifest.quorum_weight();
        if signed_weight < required {
            return Err(AccountChainError::InsufficientCheckpointQuorum {
                signed_weight,
                required_weight: required,
            });
        }
        Ok(signed_weight)
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        let sigs = self
            .validator_signatures
            .iter()
            .map(ValidatorCheckpointSig::to_canonical_value)
            .collect();
        CanonicalValue::Map(vec![
            (0, self.unsigned_canonical_value()),
            (1, CanonicalValue::Array(sigs)),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ManifestV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let outer = decode_value(bytes, ResourceProfile::ManifestV1)?;
        let outer_entries = expect_map(&outer, &[0, 1])?;
        let inner = expect_map(req_field(outer_entries, 0)?, &[0, 1, 2, 3])?;
        let mut finalized_heads = Vec::new();
        for row in expect_array(req_field(inner, 2)?)? {
            let triple = expect_array(row)?;
            if triple.len() != 3 {
                return Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0));
            }
            finalized_heads.push((
                expect_actor(&triple[0])?,
                expect_u64(&triple[1])?,
                expect_bytes_exact::<32>(&triple[2])?,
            ));
        }
        let validator_signatures = expect_array(req_field(outer_entries, 1)?)?
            .iter()
            .map(ValidatorCheckpointSig::from_canonical_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            domain_id: expect_bytes_exact::<32>(req_field(inner, 0)?)?,
            checkpoint_seq: expect_u64(req_field(inner, 1)?)?,
            finalized_heads,
            nullifier_set_root: expect_bytes_exact::<32>(req_field(inner, 3)?)?,
            validator_signatures,
        })
    }
}

/// Explicit CAP-honest settlement state for an `AccountBlockV1` (`§5.2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettlementState {
    ProvisionalPartitionLocal,
    FinalInDomain {
        domain_id: [u8; 32],
        checkpoint: SettlementDomainCheckpoint,
    },
    FrozenEquivocated {
        warrant: AccountEquivocationWarrant,
    },
}

impl SettlementState {
    pub const fn ui_badge(&self) -> Option<&'static str> {
        match self {
            Self::ProvisionalPartitionLocal => Some(PROVISIONAL_SETTLEMENT_UI_BADGE),
            Self::FinalInDomain { .. } | Self::FrozenEquivocated { .. } => None,
        }
    }

    pub const fn is_final_in_domain(&self) -> bool {
        matches!(self, Self::FinalInDomain { .. })
    }

    pub const fn is_frozen(&self) -> bool {
        matches!(self, Self::FrozenEquivocated { .. })
    }

    pub(crate) fn to_canonical_value(&self) -> CanonicalValue {
        match self {
            Self::ProvisionalPartitionLocal => {
                CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(0))])
            }
            Self::FinalInDomain {
                domain_id,
                checkpoint,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(domain_id.to_vec())),
                (2, checkpoint.to_canonical_value()),
            ]),
            Self::FrozenEquivocated { warrant } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, warrant.to_canonical_value()),
            ]),
        }
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, CanonicalError> {
        let entries = expect_map(value, &[0, 1, 2])?;
        match expect_u64(req_field(entries, 0)?)? {
            0 => Ok(Self::ProvisionalPartitionLocal),
            1 => {
                let cp_bytes = encode_value(req_field(entries, 2)?, ResourceProfile::ManifestV1)?;
                Ok(Self::FinalInDomain {
                    domain_id: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
                    checkpoint: SettlementDomainCheckpoint::decode_canonical_bytes(&cp_bytes)?,
                })
            }
            2 => {
                let w_bytes = encode_value(req_field(entries, 1)?, ResourceProfile::ObjectV1)?;
                Ok(Self::FrozenEquivocated {
                    warrant: AccountEquivocationWarrant::decode_canonical_bytes(&w_bytes)?,
                })
            }
            _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
        }
    }
}

/// Outcome of submitting an `AccountBlockV1` to `AccountChainEngine`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockInsertOutcome {
    AcceptedProvisional { block_cid: [u8; 32] },
    AlreadyPresent { block_cid: [u8; 32] },
    EquivocationFrozen { warrant: AccountEquivocationWarrant },
}

/// Errors returned by `AccountChainEngine`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AccountChainError {
    #[error("OBT_CHAIN_CANONICAL: {0}")]
    Canonical(#[from] CanonicalError),
    #[error("OBT_CHAIN_MATH: {0}")]
    Math(#[from] ObtMathError),
    #[error("OBT_CHAIN_MINT_CERT: {0}")]
    MintCertificate(#[from] MintPipelineError),
    #[error("OBT_CHAIN_SIGNATURE_INVALID: block signature or feed public key invalid")]
    SignatureInvalid,
    #[error(
        "OBT_CHAIN_UNAUTHORIZED_FEED: signer feed is not AuthorizedRelative for account ActorId"
    )]
    UnauthorizedFeedForActor,
    #[error("OBT_CHAIN_R4_FRESHNESS_STALE: R4 authority freshness check (>60s or revoked) rejected signing")]
    StaleOrDeniedR4Freshness,
    #[error("OBT_CHAIN_ACCOUNT_FROZEN: account {0} has an active equivocation freeze")]
    AccountFrozen(ActorId),
    #[error("OBT_CHAIN_SEQUENCE_INVALID: expected seq {expected}, got {got}")]
    InvalidSequence { expected: u64, got: u64 },
    #[error("OBT_CHAIN_PREV_BLOCK_MISMATCH: prev_block_cid does not match head at seq - 1")]
    PrevBlockMismatch,
    #[error("OBT_CHAIN_BALANCE_MISMATCH: expected balance_after {expected}, got {got}")]
    BalanceMismatch { expected: u128, got: u128 },
    #[error("OBT_CHAIN_NULLIFIER_MISMATCH_OR_REPLAY: block nullifier missing, mismatched, or already consumed")]
    NullifierInvalidOrReplayed,
    #[error("OBT_CHAIN_INVALID_TRANSFER: transfer amount must be > 0 and recipient != sender")]
    InvalidTransferParameters,
    #[error("OBT_CHAIN_UNKNOWN_SEND_BLOCK: referenced SendTransfer block not found or mismatched")]
    UnknownOrMismatchedSendBlock,
    #[error("OBT_CHAIN_PROVISIONAL_OPT_IN_REQUIRED: sender SendTransfer is ProvisionalPartitionLocal and recipient did not opt in")]
    ProvisionalOptInRequired,
    #[error("OBT_CHAIN_FROZEN_SEND_BLOCK: referenced SendTransfer belongs to an equivocated frozen branch")]
    SendBlockFrozenEquivocated,
    #[error("OBT_CHAIN_SEND_NOT_EXPIRED_OR_ALREADY_SETTLED: CancelExpiredSend not permitted")]
    CancelSendNotPermitted,
    #[error("OBT_CHAIN_INVALID_WARRANT: AccountEquivocationWarrant verification failed")]
    InvalidEquivocationWarrant,
    #[error("OBT_CHAIN_INVALID_MANIFEST: SettlementDomainManifest invalid")]
    InvalidSettlementManifest,
    #[error("OBT_CHAIN_CHECKPOINT_DOMAIN_MISMATCH: checkpoint domain_id mismatch")]
    CheckpointDomainMismatch,
    #[error("OBT_CHAIN_CHECKPOINT_SIG_INVALID: validator checkpoint signature invalid")]
    CheckpointSignatureInvalid,
    #[error("OBT_CHAIN_CHECKPOINT_QUORUM_INSUFFICIENT: signed weight {signed_weight} < required {required_weight}")]
    InsufficientCheckpointQuorum {
        signed_weight: u64,
        required_weight: u64,
    },
    #[error("OBT_CHAIN_CHECKPOINT_MISSING_BLOCK: checkpoint references unknown local block head")]
    CheckpointReferencesUnknownBlock,
}

/// Stored block + settlement state entry inside `AccountChainEngine` and `RedbObtStore`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackedAccountBlock {
    pub block_cid: [u8; 32],
    pub block: AccountBlockV1,
    pub settlement: SettlementState,
}

impl TrackedAccountBlock {
    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.block_cid.to_vec())),
            (1, self.block.to_canonical_value()),
            (2, self.settlement.to_canonical_value()),
        ]);
        encode_value(&value, ResourceProfile::ManifestV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_value(bytes, ResourceProfile::ManifestV1)?;
        let entries = expect_map(&value, &[0, 1, 2])?;
        let block_bytes = encode_value(req_field(entries, 1)?, ResourceProfile::ControlV1)?;
        let block = AccountBlockV1::decode_canonical_bytes(&block_bytes)?;
        let block_cid = expect_bytes_exact::<32>(req_field(entries, 0)?)?;
        if block.block_cid() != block_cid {
            return Err(CanonicalError::new(CanonicalErrorKind::ReencodeMismatch, 0));
        }
        let settlement = SettlementState::from_canonical_value(req_field(entries, 2)?)?;
        Ok(Self {
            block_cid,
            block,
            settlement,
        })
    }
}

/// CAP-honest `ActorId` account-chain ledger engine (`§5.2–5.3`).
#[derive(Clone, Debug, Default)]
pub struct AccountChainEngine {
    /// `(ActorId, seq)` -> all valid signed branches observed at that slot.
    pub chains: BTreeMap<(ActorId, u64), Vec<TrackedAccountBlock>>,
    pub blocks_by_cid: BTreeMap<[u8; 32], (ActorId, u64)>,
    pub consumed_nullifiers: BTreeSet<ScopedNullifier>,
    pub cancelled_sends: BTreeSet<[u8; 32]>,
    pub warrants: BTreeMap<[u8; 32], AccountEquivocationWarrant>,
    pub checkpoints: BTreeMap<([u8; 32], u64), SettlementDomainCheckpoint>,
    pub domain_manifests: BTreeMap<[u8; 32], SettlementDomainManifest>,
}

impl AccountChainEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_domain_manifest(
        &mut self,
        manifest: SettlementDomainManifest,
    ) -> Result<(), AccountChainError> {
        manifest.validate()?;
        self.domain_manifests.insert(manifest.domain_id, manifest);
        Ok(())
    }

    /// Return the active non-frozen head of `account`, if any.
    pub fn active_head(&self, account: ActorId) -> Option<&TrackedAccountBlock> {
        let mut highest: Option<&TrackedAccountBlock> = None;
        for ((acct, _seq), branches) in self.chains.range((account, 0)..=(account, u64::MAX)) {
            if *acct != account {
                break;
            }
            if let Some(final_branch) = branches.iter().find(|b| b.settlement.is_final_in_domain())
            {
                highest = Some(final_branch);
            } else if branches.len() == 1 && !branches[0].settlement.is_frozen() {
                highest = Some(&branches[0]);
            } else {
                break;
            }
        }
        highest
    }

    /// Total settled (`FinalInDomain`) balance for `account`.
    pub fn settled_balance(&self, account: ActorId) -> ObtAmount {
        let mut last_settled = 0u128;
        for ((acct, _), branches) in self.chains.range((account, 0)..=(account, u64::MAX)) {
            if *acct != account {
                break;
            }
            if let Some(final_branch) = branches.iter().find(|b| b.settlement.is_final_in_domain())
            {
                last_settled = final_branch.block.balance_after;
            }
        }
        ObtAmount(last_settled)
    }

    /// Latest provisional or settled balance for `account` (`None` if unfrozen chain is empty).
    pub fn available_provisional_balance(
        &self,
        account: ActorId,
    ) -> Option<(ObtAmount, &'static str)> {
        let head = self.active_head(account)?;
        let badge = head.settlement.ui_badge().unwrap_or("");
        Some((ObtAmount(head.block.balance_after), badge))
    }

    pub fn has_equivocation(&self, account: ActorId) -> bool {
        self.warrants.values().any(|w| w.actor == account)
    }

    /// Verify Ed25519 signature, `KeyStateReducer` `AuthorizedRelative` for `block.account`,
    /// and `REV-001` `R4` (`60s` live freshness) via `RevocationFreshnessEvaluator`.
    pub fn verify_r4_authority(
        block: &AccountBlockV1,
        feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        authority_observed_at_secs: u64,
        current_local_secs: u64,
    ) -> Result<(), AccountChainError> {
        if feed.feed_id != block.signer_feed
            || feed.signed.inception.feed_public_key != block.signer_public_key
        {
            return Err(AccountChainError::SignatureInvalid);
        }
        block.verify_crypto()?;

        match key_state.evaluate(feed) {
            FeedAuthorityDecision::AuthorizedRelative { actor, .. } if actor == block.account => {}
            _ => return Err(AccountChainError::UnauthorizedFeedForActor),
        }

        let observation =
            AuthorityFreshnessObservation::from_feed(key_state, feed, authority_observed_at_secs);
        let check = RevocationCheck {
            action: RevocationAction::IrreversibleExternalEffect,
            risk: RevocationRisk::R4IrreversibleAuthority,
            task_commitment: None,
            required_scopes: vec![AuthorityScope::feed(feed.feed_id)],
        };
        let decision = RevocationFreshnessEvaluator::evaluate(
            &RevocationFreshnessProfile::terrestrial_interactive(),
            &check,
            &[observation],
            current_local_secs,
            None,
        )
        .map_err(|_| AccountChainError::StaleOrDeniedR4Freshness)?;

        match decision {
            RevocationFreshnessDecision::AuthorizedRelativeFresh { .. } => Ok(()),
            _ => Err(AccountChainError::StaleOrDeniedR4Freshness),
        }
    }

    /// Submit a live `AccountBlockV1` (rejecting blocks extending or receiving from already-frozen branches).
    pub fn submit_block(
        &mut self,
        block: AccountBlockV1,
        feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        authority_observed_at_secs: u64,
        current_local_secs: u64,
        recipient_accepts_provisional: bool,
    ) -> Result<BlockInsertOutcome, AccountChainError> {
        self.submit_block_inner(
            block,
            feed,
            key_state,
            authority_observed_at_secs,
            current_local_secs,
            recipient_accepts_provisional,
            false,
        )
    }

    /// Reconcile a synced `AccountBlockV1` over `obp/obt-ledger/1`, preserving valid historical
    /// downstream blocks on equivocated branches directly in `SettlementState::FrozenEquivocated`
    /// so peers converge to identical audit state regardless of fork arrival order.
    pub fn ingest_synced_block(
        &mut self,
        block: AccountBlockV1,
        feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        authority_observed_at_secs: u64,
        current_local_secs: u64,
        recipient_accepts_provisional: bool,
    ) -> Result<BlockInsertOutcome, AccountChainError> {
        self.submit_block_inner(
            block,
            feed,
            key_state,
            authority_observed_at_secs,
            current_local_secs,
            recipient_accepts_provisional,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_block_inner(
        &mut self,
        block: AccountBlockV1,
        feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        authority_observed_at_secs: u64,
        current_local_secs: u64,
        recipient_accepts_provisional: bool,
        allow_frozen_sync_audit: bool,
    ) -> Result<BlockInsertOutcome, AccountChainError> {
        Self::verify_r4_authority(
            &block,
            feed,
            key_state,
            authority_observed_at_secs,
            current_local_secs,
        )?;

        let block_cid = block.block_cid();
        if self.blocks_by_cid.contains_key(&block_cid) {
            return Ok(BlockInsertOutcome::AlreadyPresent { block_cid });
        }

        // Check if another distinct valid signed block already exists at `(block.account, block.seq)`!
        if let Some(existing_branches) = self.chains.get(&(block.account, block.seq)).cloned() {
            let first = &existing_branches[0];
            let warrant =
                AccountEquivocationWarrant::from_conflicting_blocks(&first.block, &block)?;
            self.record_equivocation_and_freeze(block, block_cid, warrant.clone());
            return Ok(BlockInsertOutcome::EquivocationFrozen { warrant });
        }

        // Reject extending a chain that already has a frozen equivocated slot before `block.seq`
        // unless reconciling historical frozen branches during sync.
        if !allow_frozen_sync_audit && self.has_unfinalized_freeze(block.account) {
            return Err(AccountChainError::AccountFrozen(block.account));
        }

        let mut inherited_freeze: Option<AccountEquivocationWarrant> = None;

        // Validate predecessor & balance transition.
        let prev_balance = if block.seq == 0 {
            if block.prev_block_cid.is_some() {
                return Err(AccountChainError::PrevBlockMismatch);
            }
            if !matches!(block.op, AccountBlockOp::OpenAccount { .. }) {
                return Err(AccountChainError::InvalidSequence {
                    expected: 0,
                    got: block.seq,
                });
            }
            0u128
        } else {
            let prev_seq = block.seq - 1;
            let Some(prev_branches) = self.chains.get(&(block.account, prev_seq)) else {
                return Err(AccountChainError::InvalidSequence {
                    expected: self
                        .active_head(block.account)
                        .map_or(0, |h| h.block.seq + 1),
                    got: block.seq,
                });
            };
            let matched_prev = prev_branches
                .iter()
                .find(|b| Some(b.block_cid) == block.prev_block_cid);
            let prev = match matched_prev {
                Some(p) => {
                    if let SettlementState::FrozenEquivocated { warrant } = &p.settlement {
                        if !allow_frozen_sync_audit {
                            return Err(AccountChainError::AccountFrozen(block.account));
                        }
                        inherited_freeze = Some(warrant.clone());
                    }
                    p
                }
                None => {
                    if prev_branches.iter().any(|b| b.settlement.is_frozen()) {
                        return Err(AccountChainError::AccountFrozen(block.account));
                    }
                    return Err(AccountChainError::PrevBlockMismatch);
                }
            };
            prev.block.balance_after
        };

        let op_freeze = self.validate_and_apply_op(
            &block,
            prev_balance,
            recipient_accepts_provisional,
            allow_frozen_sync_audit,
        )?;
        let effective_freeze = inherited_freeze.or(op_freeze);

        let settlement = match &effective_freeze {
            Some(warrant) => SettlementState::FrozenEquivocated {
                warrant: warrant.clone(),
            },
            None => SettlementState::ProvisionalPartitionLocal,
        };
        let tracked = TrackedAccountBlock {
            block_cid,
            block: block.clone(),
            settlement,
        };
        self.chains
            .entry((block.account, block.seq))
            .or_default()
            .push(tracked);
        self.blocks_by_cid
            .insert(block_cid, (block.account, block.seq));

        if let Some(warrant) = effective_freeze {
            self.freeze_from_warrant(&warrant);
            Ok(BlockInsertOutcome::EquivocationFrozen { warrant })
        } else {
            Ok(BlockInsertOutcome::AcceptedProvisional { block_cid })
        }
    }

    fn validate_and_apply_op(
        &mut self,
        block: &AccountBlockV1,
        prev_balance: u128,
        recipient_accepts_provisional: bool,
        allow_frozen_sync_audit: bool,
    ) -> Result<Option<AccountEquivocationWarrant>, AccountChainError> {
        match &block.op {
            AccountBlockOp::OpenAccount { initial_policy_cid } => {
                if *initial_policy_cid.as_bytes() == [0u8; 32] || block.nullifier.is_some() {
                    return Err(AccountChainError::NullifierInvalidOrReplayed);
                }
                if block.balance_after != 0 {
                    return Err(AccountChainError::BalanceMismatch {
                        expected: 0,
                        got: block.balance_after,
                    });
                }
                Ok(None)
            }
            AccountBlockOp::CreditFinalizedMint { certificate } => {
                certificate.verify_self_contained()?;
                if certificate.beneficiary != block.account {
                    return Err(AccountChainError::UnauthorizedFeedForActor);
                }
                if block.nullifier != Some(certificate.credit_nullifier)
                    || self
                        .consumed_nullifiers
                        .contains(&certificate.credit_nullifier)
                {
                    return Err(AccountChainError::NullifierInvalidOrReplayed);
                }
                let expected = ObtAmount(prev_balance)
                    .checked_add(certificate.amount)?
                    .quanta();
                if block.balance_after != expected {
                    return Err(AccountChainError::BalanceMismatch {
                        expected,
                        got: block.balance_after,
                    });
                }
                self.consumed_nullifiers
                    .insert(certificate.credit_nullifier);
                Ok(None)
            }
            AccountBlockOp::SendTransfer {
                recipient, amount, ..
            } => {
                if amount.is_zero() || *recipient == block.account || block.nullifier.is_some() {
                    return Err(AccountChainError::InvalidTransferParameters);
                }
                let expected = ObtAmount(prev_balance).checked_sub(*amount)?.quanta();
                if block.balance_after != expected {
                    return Err(AccountChainError::BalanceMismatch {
                        expected,
                        got: block.balance_after,
                    });
                }
                Ok(None)
            }
            AccountBlockOp::ReceiveTransfer {
                send_block_cid,
                sender,
                amount,
            } => {
                let expected_nullifier =
                    ScopedNullifier::for_receive_transfer(*send_block_cid, block.account);
                if block.nullifier != Some(expected_nullifier)
                    || self.consumed_nullifiers.contains(&expected_nullifier)
                    || self.cancelled_sends.contains(send_block_cid)
                {
                    return Err(AccountChainError::NullifierInvalidOrReplayed);
                }
                let send_tracked = self
                    .get_tracked_block(send_block_cid)
                    .ok_or(AccountChainError::UnknownOrMismatchedSendBlock)?;
                let mut upstream_freeze: Option<AccountEquivocationWarrant> = None;
                if let SettlementState::FrozenEquivocated { warrant } = &send_tracked.settlement {
                    if !allow_frozen_sync_audit {
                        return Err(AccountChainError::SendBlockFrozenEquivocated);
                    }
                    upstream_freeze = Some(warrant.clone());
                }
                if matches!(
                    send_tracked.settlement,
                    SettlementState::ProvisionalPartitionLocal
                ) && !recipient_accepts_provisional
                {
                    return Err(AccountChainError::ProvisionalOptInRequired);
                }
                let AccountBlockOp::SendTransfer {
                    recipient: send_recipient,
                    amount: send_amount,
                    ..
                } = &send_tracked.block.op
                else {
                    return Err(AccountChainError::UnknownOrMismatchedSendBlock);
                };
                if send_tracked.block.account != *sender
                    || *send_recipient != block.account
                    || *send_amount != *amount
                {
                    return Err(AccountChainError::UnknownOrMismatchedSendBlock);
                }
                let expected = ObtAmount(prev_balance).checked_add(*amount)?.quanta();
                if block.balance_after != expected {
                    return Err(AccountChainError::BalanceMismatch {
                        expected,
                        got: block.balance_after,
                    });
                }
                self.consumed_nullifiers.insert(expected_nullifier);
                Ok(upstream_freeze)
            }
            AccountBlockOp::CancelExpiredSend {
                send_block_cid,
                refund_amount,
                observed_frontier_seq,
            } => {
                if self.cancelled_sends.contains(send_block_cid) {
                    return Err(AccountChainError::CancelSendNotPermitted);
                }
                let send_tracked = self
                    .get_tracked_block(send_block_cid)
                    .ok_or(AccountChainError::UnknownOrMismatchedSendBlock)?;
                let AccountBlockOp::SendTransfer {
                    recipient,
                    amount,
                    expires_at_frontier_seq: Some(exp),
                    ..
                } = &send_tracked.block.op
                else {
                    return Err(AccountChainError::CancelSendNotPermitted);
                };
                let recv_nullifier =
                    ScopedNullifier::for_receive_transfer(*send_block_cid, *recipient);
                if send_tracked.block.account != block.account
                    || *amount != *refund_amount
                    || *observed_frontier_seq < *exp
                    || self.consumed_nullifiers.contains(&recv_nullifier)
                {
                    return Err(AccountChainError::CancelSendNotPermitted);
                }
                let expected = ObtAmount(prev_balance)
                    .checked_add(*refund_amount)?
                    .quanta();
                if block.balance_after != expected {
                    return Err(AccountChainError::BalanceMismatch {
                        expected,
                        got: block.balance_after,
                    });
                }
                self.cancelled_sends.insert(*send_block_cid);
                Ok(None)
            }
        }
    }

    pub fn get_tracked_block(&self, block_cid: &[u8; 32]) -> Option<&TrackedAccountBlock> {
        let &(actor, seq) = self.blocks_by_cid.get(block_cid)?;
        self.chains
            .get(&(actor, seq))?
            .iter()
            .find(|b| &b.block_cid == block_cid)
    }

    fn has_unfinalized_freeze(&self, actor: ActorId) -> bool {
        for ((acct, _), branches) in self.chains.range((actor, 0)..=(actor, u64::MAX)) {
            if *acct != actor {
                break;
            }
            let has_final = branches.iter().any(|b| b.settlement.is_final_in_domain());
            if !has_final && branches.iter().any(|b| b.settlement.is_frozen()) {
                return true;
            }
        }
        false
    }

    /// Record an `AccountEquivocationWarrant` and cascade-freeze conflicting unfinalized branches
    /// plus unfinalized downstream `ReceiveTransfer` blocks while leaving `FinalInDomain` untouched!
    pub fn ingest_equivocation_warrant(
        &mut self,
        warrant: AccountEquivocationWarrant,
    ) -> Result<(), AccountChainError> {
        let (block_a, block_b) = warrant.verify()?;
        let cid_a = block_a.block_cid();
        let cid_b = block_b.block_cid();
        for (blk, cid) in [(block_a, cid_a), (block_b, cid_b)] {
            if let std::collections::btree_map::Entry::Vacant(slot) = self.blocks_by_cid.entry(cid)
            {
                slot.insert((blk.account, blk.seq));
                self.chains
                    .entry((blk.account, blk.seq))
                    .or_default()
                    .push(TrackedAccountBlock {
                        block_cid: cid,
                        block: blk,
                        settlement: SettlementState::FrozenEquivocated {
                            warrant: warrant.clone(),
                        },
                    });
            }
        }
        self.freeze_from_warrant(&warrant);
        self.warrants.insert(warrant.warrant_cid, warrant);
        Ok(())
    }

    fn record_equivocation_and_freeze(
        &mut self,
        incoming: AccountBlockV1,
        incoming_cid: [u8; 32],
        warrant: AccountEquivocationWarrant,
    ) {
        self.blocks_by_cid
            .insert(incoming_cid, (incoming.account, incoming.seq));
        self.chains
            .entry((incoming.account, incoming.seq))
            .or_default()
            .push(TrackedAccountBlock {
                block_cid: incoming_cid,
                block: incoming,
                settlement: SettlementState::FrozenEquivocated {
                    warrant: warrant.clone(),
                },
            });
        self.freeze_from_warrant(&warrant);
        self.warrants.insert(warrant.warrant_cid, warrant);
    }

    fn freeze_from_warrant(&mut self, warrant: &AccountEquivocationWarrant) {
        // Check if `(warrant.actor, warrant.seq)` already has a `FinalInDomain` block:
        // per §5.2, already `FinalInDomain` blocks remain completely untouched!
        let mut frozen_send_cids = BTreeSet::new();

        for ((acct, seq), branches) in self
            .chains
            .range_mut((warrant.actor, warrant.seq)..=(warrant.actor, u64::MAX))
        {
            if *acct != warrant.actor {
                break;
            }
            let _ = seq;
            for b in branches.iter_mut() {
                if !b.settlement.is_final_in_domain() {
                    b.settlement = SettlementState::FrozenEquivocated {
                        warrant: warrant.clone(),
                    };
                    if matches!(b.block.op, AccountBlockOp::SendTransfer { .. }) {
                        frozen_send_cids.insert(b.block_cid);
                    }
                }
            }
        }

        // Propagate freeze to any unfinalized downstream `ReceiveTransfer` (and subsequent
        // unfinalized blocks on that recipient chain) citing a frozen `SendTransfer`.
        loop {
            let mut newly_frozen_recipient_from: Option<(ActorId, u64)> = None;
            for ((acct, seq), branches) in &self.chains {
                for b in branches {
                    if b.settlement.is_final_in_domain() || b.settlement.is_frozen() {
                        continue;
                    }
                    if let AccountBlockOp::ReceiveTransfer { send_block_cid, .. } = &b.block.op {
                        if frozen_send_cids.contains(send_block_cid) {
                            newly_frozen_recipient_from = Some((*acct, *seq));
                            break;
                        }
                    }
                }
                if newly_frozen_recipient_from.is_some() {
                    break;
                }
            }

            let Some((recipient_actor, start_seq)) = newly_frozen_recipient_from else {
                break;
            };
            for ((acct, _), branches) in self
                .chains
                .range_mut((recipient_actor, start_seq)..=(recipient_actor, u64::MAX))
            {
                if *acct != recipient_actor {
                    break;
                }
                for b in branches.iter_mut() {
                    if !b.settlement.is_final_in_domain() {
                        b.settlement = SettlementState::FrozenEquivocated {
                            warrant: warrant.clone(),
                        };
                        if matches!(b.block.op, AccountBlockOp::SendTransfer { .. }) {
                            frozen_send_cids.insert(b.block_cid);
                        }
                    }
                }
            }
        }
    }

    /// Verify a `SettlementDomainCheckpoint` (`>= 2f + 1` weight) and promote covered
    /// ancestor chains to `SettlementState::FinalInDomain`.
    pub fn apply_domain_checkpoint(
        &mut self,
        checkpoint: SettlementDomainCheckpoint,
    ) -> Result<(), AccountChainError> {
        let manifest = self
            .domain_manifests
            .get(&checkpoint.domain_id)
            .ok_or(AccountChainError::CheckpointDomainMismatch)?
            .clone();
        checkpoint.verify_against_manifest(&manifest)?;

        // First validate that every head cited in `finalized_heads` exists locally and matches `(actor, seq)`
        // and is not frozen.
        let mut chains_to_promote: Vec<Vec<(ActorId, u64, [u8; 32])>> = Vec::new();
        for &(actor, head_seq, head_cid) in &checkpoint.finalized_heads {
            let mut cursor_cid = head_cid;
            let mut cursor_seq = head_seq;
            let mut lineage = Vec::new();
            loop {
                let tracked = self
                    .get_tracked_block(&cursor_cid)
                    .ok_or(AccountChainError::CheckpointReferencesUnknownBlock)?;
                if tracked.block.account != actor
                    || tracked.block.seq != cursor_seq
                    || tracked.settlement.is_frozen()
                {
                    return Err(AccountChainError::CheckpointReferencesUnknownBlock);
                }
                lineage.push((actor, cursor_seq, cursor_cid));
                if cursor_seq == 0 {
                    break;
                }
                cursor_seq -= 1;
                cursor_cid = tracked
                    .block
                    .prev_block_cid
                    .ok_or(AccountChainError::PrevBlockMismatch)?;
            }
            chains_to_promote.push(lineage);
        }

        for lineage in chains_to_promote {
            for (actor, seq, cid) in lineage {
                if let Some(branches) = self.chains.get_mut(&(actor, seq)) {
                    for b in branches.iter_mut() {
                        if b.block_cid == cid {
                            b.settlement = SettlementState::FinalInDomain {
                                domain_id: checkpoint.domain_id,
                                checkpoint: checkpoint.clone(),
                            };
                        }
                    }
                }
            }
        }

        self.checkpoints.insert(
            (checkpoint.domain_id, checkpoint.checkpoint_seq),
            checkpoint,
        );
        Ok(())
    }
}
