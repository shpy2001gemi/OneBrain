//! Signed per-feed checkpoints and proof-gated suppression authority.
//!
//! A valid signature authenticates the producer; it does not by itself allow
//! an implementation to hide or delete older events. Suppression becomes
//! eligible only after exact key-state, history, consistency and reducer-effect
//! proofs have been checked at one named local frontier.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::authority::FeedAuthorityDecision;
use super::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::{signature_message, CheckpointCid, EventCid, ManifestCid, ReservedDomain};
use super::event::ValidatedKnowledgeEvent;
use super::feed::ValidatedFeedInception;
use super::identity::FeedId;
use super::key_state::KeyStateCheckpointProof;
use super::schema_registry::SCHEMA_FEED_CHECKPOINT;

pub const CHECKPOINT_PROFILE_MAJOR: u64 = 1;
pub const CHECKPOINT_PROFILE_MINOR: u64 = 0;
pub const MAX_CHECKPOINT_LEAVES: usize = 65_536;
pub const MAX_CHECKPOINT_PROOF_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedCheckpointBody {
    pub feed_id: FeedId,
    pub covered_sequence: u64,
    pub covered_root: [u8; 32],
    pub state_cid: ManifestCid,
    pub reducer_version: [u8; 32],
    pub last_event_cid: EventCid,
    pub previous_checkpoint_cid: Option<CheckpointCid>,
    pub retirement_floor_root: [u8; 32],
    pub key_state_frontier: EventCid,
    pub key_state_root: Option<[u8; 32]>,
    pub archive_manifest_ref: Option<ManifestCid>,
    pub nonce: [u8; 32],
}

impl FeedCheckpointBody {
    #[allow(clippy::too_many_arguments)]
    pub fn from_witness(
        witness: &CheckpointHistoryWitness,
        reducer_version: [u8; 32],
        previous_checkpoint_cid: Option<CheckpointCid>,
        retirement_floor_root: [u8; 32],
        key_state: &KeyStateCheckpointProof,
        archive_manifest_ref: Option<ManifestCid>,
        nonce: [u8; 32],
    ) -> Result<Self, CheckpointError> {
        if previous_checkpoint_cid != witness.previous.as_ref().map(|base| base.checkpoint_cid)
            || key_state.subject_feed != witness.feed_id
        {
            return Err(CheckpointError::InvalidHistory);
        }
        let last = witness
            .leaves
            .last()
            .ok_or(CheckpointError::InvalidHistory)?;
        let body = Self {
            feed_id: witness.feed_id,
            covered_sequence: last.sequence,
            covered_root: witness.root,
            state_cid: last.state_after,
            reducer_version,
            last_event_cid: last.event_cid,
            previous_checkpoint_cid,
            retirement_floor_root,
            key_state_frontier: key_state.frontier,
            key_state_root: Some(key_state.state_root),
            archive_manifest_ref,
            nonce,
        };
        body.validate()?;
        Ok(body)
    }

    fn validate(&self) -> Result<(), CheckpointError> {
        if self.covered_root == [0; 32]
            || self.state_cid.as_bytes() == &[0; 32]
            || self.reducer_version == [0; 32]
            || self.last_event_cid.as_bytes() == &[0; 32]
            || self.retirement_floor_root == [0; 32]
            || self.key_state_frontier.as_bytes() == &[0; 32]
            || self.key_state_root == Some([0; 32])
            || self.nonce == [0; 32]
        {
            return Err(CheckpointError::InvalidField);
        }
        Ok(())
    }

    fn to_value(&self) -> Result<CanonicalValue, CheckpointError> {
        self.validate()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.feed_id.as_bytes().to_vec())),
            (1, CanonicalValue::Unsigned(self.covered_sequence)),
            (2, CanonicalValue::Bytes(self.covered_root.to_vec())),
            (3, CanonicalValue::Bytes(self.state_cid.as_bytes().to_vec())),
            (4, CanonicalValue::Bytes(self.reducer_version.to_vec())),
            (
                5,
                CanonicalValue::Bytes(self.last_event_cid.as_bytes().to_vec()),
            ),
            (
                6,
                optional_digest(self.previous_checkpoint_cid.map(|v| *v.as_bytes())),
            ),
            (
                7,
                CanonicalValue::Bytes(self.retirement_floor_root.to_vec()),
            ),
            (
                8,
                CanonicalValue::Bytes(self.key_state_frontier.as_bytes().to_vec()),
            ),
            (9, optional_digest(self.key_state_root)),
            (
                10,
                optional_digest(self.archive_manifest_ref.map(|v| *v.as_bytes())),
            ),
            (11, CanonicalValue::Bytes(self.nonce.to_vec())),
        ]))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, CheckpointError> {
        let fields = map(value)?;
        if fields.len() != 12 {
            return Err(CheckpointError::InvalidField);
        }
        let body = Self {
            feed_id: FeedId::from_bytes(bytes32(fields, 0)?),
            covered_sequence: unsigned(fields, 1)?,
            covered_root: bytes32(fields, 2)?,
            state_cid: ManifestCid::from_bytes(bytes32(fields, 3)?),
            reducer_version: bytes32(fields, 4)?,
            last_event_cid: EventCid::from_bytes(bytes32(fields, 5)?),
            previous_checkpoint_cid: optional_bytes32(fields, 6)?.map(CheckpointCid::from_bytes),
            retirement_floor_root: bytes32(fields, 7)?,
            key_state_frontier: EventCid::from_bytes(bytes32(fields, 8)?),
            key_state_root: optional_bytes32(fields, 9)?,
            archive_manifest_ref: optional_bytes32(fields, 10)?.map(ManifestCid::from_bytes),
            nonce: bytes32(fields, 11)?,
        };
        body.validate()?;
        if body.to_value()? != *value {
            return Err(CheckpointError::NonCanonical);
        }
        Ok(body)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedFeedCheckpoint {
    pub body: FeedCheckpointBody,
    pub signer_feed: FeedId,
    pub signature: [u8; 64],
}

impl SignedFeedCheckpoint {
    pub fn sign(
        body: FeedCheckpointBody,
        signer: &ValidatedFeedInception,
        key: &SigningKey,
    ) -> Result<Self, CheckpointError> {
        body.validate()?;
        if signer.feed_id != body.feed_id
            || key.verifying_key().as_bytes() != &signer.signed.inception.feed_public_key
        {
            return Err(CheckpointError::SignerMismatch);
        }
        let unsigned = unsigned_bytes(&body, signer.feed_id)?;
        let message = signature_message(ReservedDomain::Checkpoint, &unsigned)
            .map_err(|_| CheckpointError::SignatureDomain)?;
        Ok(Self {
            body,
            signer_feed: signer.feed_id,
            signature: key.sign(&message).to_bytes(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, CheckpointError> {
        encode_record(&self.body, self.signer_feed, Some(self.signature))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedFeedCheckpoint {
    pub checkpoint_cid: CheckpointCid,
    pub signed: SignedFeedCheckpoint,
    original_bytes: Vec<u8>,
}

impl ValidatedFeedCheckpoint {
    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }

    /// Signature validation alone never grants suppression or deletion.
    pub const fn has_suppression_authority_without_proofs(&self) -> bool {
        false
    }
}

pub fn decode_feed_checkpoint(
    bytes: &[u8],
    signer: &ValidatedFeedInception,
) -> Result<ValidatedFeedCheckpoint, CheckpointError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value)?;
    if root.len() != 6
        || unsigned(root, 0)? != SCHEMA_FEED_CHECKPOINT
        || unsigned(root, 1)? != CHECKPOINT_PROFILE_MAJOR
        || unsigned(root, 2)? > CHECKPOINT_PROFILE_MINOR
    {
        return Err(CheckpointError::UnsupportedVersion);
    }
    let body = FeedCheckpointBody::from_value(required(root, 3)?)?;
    let signer_feed = FeedId::from_bytes(bytes32(root, 4)?);
    let signature = bytes64(root, 5)?;
    if signer_feed != signer.feed_id || body.feed_id != signer.feed_id {
        return Err(CheckpointError::SignerMismatch);
    }
    let unsigned = unsigned_bytes(&body, signer_feed)?;
    let message = signature_message(ReservedDomain::Checkpoint, &unsigned)
        .map_err(|_| CheckpointError::SignatureDomain)?;
    let key = VerifyingKey::from_bytes(&signer.signed.inception.feed_public_key)
        .map_err(|_| CheckpointError::SignatureInvalid)?;
    key.verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| CheckpointError::SignatureInvalid)?;
    let signed = SignedFeedCheckpoint {
        body,
        signer_feed,
        signature,
    };
    if signed.encode()? != bytes {
        return Err(CheckpointError::NonCanonical);
    }
    Ok(ValidatedFeedCheckpoint {
        checkpoint_cid: CheckpointCid::compute(ReservedDomain::Checkpoint, bytes)
            .map_err(|_| CheckpointError::SignatureDomain)?,
        signed,
        original_bytes: bytes.to_vec(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointLeaf {
    feed_id: FeedId,
    sequence: u64,
    event_cid: EventCid,
    causal_parents: Vec<EventCid>,
    state_before: ManifestCid,
    state_after: ManifestCid,
    effect_root: [u8; 32],
}

impl CheckpointLeaf {
    pub fn from_validated_event(
        event: &ValidatedKnowledgeEvent,
        state_before: ManifestCid,
        state_after: ManifestCid,
        effect_root: [u8; 32],
    ) -> Result<Self, CheckpointError> {
        if state_before.as_bytes() == &[0; 32]
            || state_after.as_bytes() == &[0; 32]
            || effect_root == [0; 32]
        {
            return Err(CheckpointError::InvalidField);
        }
        Ok(Self {
            feed_id: event.signed.event.author_feed,
            sequence: event.signed.event.author_sequence,
            event_cid: event.cid(),
            causal_parents: event.signed.event.causal_parents.clone(),
            state_before,
            state_after,
            effect_root,
        })
    }

    pub const fn feed_id(&self) -> FeedId {
        self.feed_id
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub fn causal_parents(&self) -> &[EventCid] {
        &self.causal_parents
    }

    pub const fn state_before(&self) -> ManifestCid {
        self.state_before
    }

    pub const fn state_after(&self) -> ManifestCid {
        self.state_after
    }

    pub const fn effect_root(&self) -> [u8; 32] {
        self.effect_root
    }

    fn value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.feed_id.as_bytes().to_vec())),
            (1, CanonicalValue::Unsigned(self.sequence)),
            (2, CanonicalValue::Bytes(self.event_cid.as_bytes().to_vec())),
            (
                3,
                CanonicalValue::Array(
                    self.causal_parents
                        .iter()
                        .map(|parent| CanonicalValue::Bytes(parent.as_bytes().to_vec()))
                        .collect(),
                ),
            ),
            (
                4,
                CanonicalValue::Bytes(self.state_before.as_bytes().to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(self.state_after.as_bytes().to_vec()),
            ),
            (6, CanonicalValue::Bytes(self.effect_root.to_vec())),
        ])
    }

    fn hash(&self) -> Result<[u8; 32], CheckpointError> {
        let bytes = encode_canonical(&self.value(), ResourceProfile::ControlV1)?;
        Ok(domain_hash(b"checkpoint-leaf/1", &[&bytes]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointHistoryWitness {
    feed_id: FeedId,
    base_sequence: u64,
    previous: Option<CheckpointBase>,
    leaves: Vec<CheckpointLeaf>,
    root: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CheckpointBase {
    checkpoint_cid: CheckpointCid,
    covered_sequence: u64,
    covered_root: [u8; 32],
    last_event_cid: EventCid,
    state_cid: ManifestCid,
}

impl CheckpointHistoryWitness {
    pub fn new(leaves: Vec<CheckpointLeaf>) -> Result<Self, CheckpointError> {
        Self::build(leaves, 0, None)
    }

    pub fn extend(
        previous: &ValidatedFeedCheckpoint,
        leaves: Vec<CheckpointLeaf>,
    ) -> Result<Self, CheckpointError> {
        let base_sequence = previous
            .signed
            .body
            .covered_sequence
            .checked_add(1)
            .ok_or(CheckpointError::ResourceLimit)?;
        Self::build(
            leaves,
            base_sequence,
            Some(CheckpointBase {
                checkpoint_cid: previous.checkpoint_cid,
                covered_sequence: previous.signed.body.covered_sequence,
                covered_root: previous.signed.body.covered_root,
                last_event_cid: previous.signed.body.last_event_cid,
                state_cid: previous.signed.body.state_cid,
            }),
        )
    }

    fn build(
        leaves: Vec<CheckpointLeaf>,
        base_sequence: u64,
        previous: Option<CheckpointBase>,
    ) -> Result<Self, CheckpointError> {
        if leaves.is_empty() || leaves.len() > MAX_CHECKPOINT_LEAVES {
            return Err(CheckpointError::InvalidHistory);
        }
        let feed_id = leaves[0].feed_id;
        for (index, leaf) in leaves.iter().enumerate() {
            let offset = u64::try_from(index).map_err(|_| CheckpointError::ResourceLimit)?;
            let sequence = base_sequence
                .checked_add(offset)
                .ok_or(CheckpointError::ResourceLimit)?;
            if leaf.feed_id != feed_id || leaf.sequence != sequence {
                return Err(CheckpointError::InvalidHistory);
            }
            let expected_predecessor = if index == 0 {
                previous.map(|base| base.last_event_cid)
            } else {
                Some(leaves[index - 1].event_cid)
            };
            if let Some(predecessor) = expected_predecessor {
                if !leaf.causal_parents.contains(&predecessor) {
                    return Err(CheckpointError::InvalidHistory);
                }
            }
            let expected_state = if index == 0 {
                previous.map(|base| base.state_cid)
            } else {
                Some(leaves[index - 1].state_after)
            };
            if expected_state.is_some_and(|state| state != leaf.state_before) {
                return Err(CheckpointError::InvalidEffectChain);
            }
        }
        let chunk_root = merkle_root(&leaves)?;
        let last_sequence = leaves
            .last()
            .map(|leaf| leaf.sequence)
            .ok_or(CheckpointError::InvalidHistory)?;
        let root = match previous {
            Some(base) => checkpoint_extension_root(base, base_sequence, last_sequence, chunk_root),
            None => chunk_root,
        };
        Ok(Self {
            feed_id,
            base_sequence,
            previous,
            leaves,
            root,
        })
    }

    pub const fn feed_id(&self) -> FeedId {
        self.feed_id
    }

    pub const fn root(&self) -> [u8; 32] {
        self.root
    }

    pub const fn base_sequence(&self) -> u64 {
        self.base_sequence
    }

    pub fn leaves(&self) -> &[CheckpointLeaf] {
        &self.leaves
    }

    pub fn inclusion_proof(&self, sequence: u64) -> Result<MerkleInclusionProof, CheckpointError> {
        let relative = sequence
            .checked_sub(self.base_sequence)
            .ok_or(CheckpointError::InvalidProof)?;
        let index = usize::try_from(relative).map_err(|_| CheckpointError::InvalidProof)?;
        if index >= self.leaves.len() {
            return Err(CheckpointError::InvalidProof);
        }
        let mut level = self
            .leaves
            .iter()
            .map(CheckpointLeaf::hash)
            .collect::<Result<Vec<_>, _>>()?;
        let mut cursor = index;
        let mut siblings = Vec::new();
        while level.len() > 1 {
            let sibling_index = if cursor % 2 == 0 {
                (cursor + 1).min(level.len() - 1)
            } else {
                cursor - 1
            };
            siblings.push(MerkleSibling {
                digest: level[sibling_index],
                sibling_on_left: cursor % 2 == 1,
            });
            level = next_merkle_level(&level);
            cursor /= 2;
        }
        Ok(MerkleInclusionProof {
            leaf: self.leaves[index].clone(),
            base_sequence: self.base_sequence,
            leaf_count: self.leaves.len() as u64,
            siblings,
            extension: self.previous.map(|base| CheckpointExtensionBinding {
                previous_checkpoint_cid: base.checkpoint_cid,
                previous_covered_sequence: base.covered_sequence,
                previous_covered_root: base.covered_root,
            }),
        })
    }

    pub fn validate_consistency(
        &self,
        previous: &ValidatedFeedCheckpoint,
        current: &ValidatedFeedCheckpoint,
    ) -> Result<ValidatedCheckpointConsistency, CheckpointError> {
        let Some(base) = self.previous else {
            return Err(CheckpointError::InvalidConsistencyProof);
        };
        if base.checkpoint_cid != previous.checkpoint_cid
            || base.covered_sequence != previous.signed.body.covered_sequence
            || base.covered_root != previous.signed.body.covered_root
            || base.last_event_cid != previous.signed.body.last_event_cid
            || base.state_cid != previous.signed.body.state_cid
            || current.signed.body.previous_checkpoint_cid != Some(previous.checkpoint_cid)
            || previous.signed.body.feed_id != current.signed.body.feed_id
            || self.base_sequence
                != previous
                    .signed
                    .body
                    .covered_sequence
                    .checked_add(1)
                    .ok_or(CheckpointError::InvalidConsistencyProof)?
        {
            return Err(CheckpointError::InvalidConsistencyProof);
        }
        if !self.matches_checkpoint(current) {
            return Err(CheckpointError::InvalidConsistencyProof);
        }
        Ok(ValidatedCheckpointConsistency {
            previous: previous.checkpoint_cid,
            current: current.checkpoint_cid,
            appended_leaf_count: self.leaves.len(),
        })
    }

    fn matches_checkpoint(&self, checkpoint: &ValidatedFeedCheckpoint) -> bool {
        let body = &checkpoint.signed.body;
        let Some(last) = self.leaves.last() else {
            return false;
        };
        body.feed_id == self.feed_id
            && body.covered_sequence == last.sequence
            && body.covered_root == self.root
            && body.last_event_cid == last.event_cid
            && body.state_cid == last.state_after
            && body.previous_checkpoint_cid
                == self.previous.as_ref().map(|base| base.checkpoint_cid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerkleSibling {
    pub digest: [u8; 32],
    pub sibling_on_left: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerkleInclusionProof {
    pub leaf: CheckpointLeaf,
    pub base_sequence: u64,
    pub leaf_count: u64,
    pub siblings: Vec<MerkleSibling>,
    pub extension: Option<CheckpointExtensionBinding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointExtensionBinding {
    pub previous_checkpoint_cid: CheckpointCid,
    pub previous_covered_sequence: u64,
    pub previous_covered_root: [u8; 32],
}

impl MerkleInclusionProof {
    pub fn validate(&self, checkpoint: &ValidatedFeedCheckpoint) -> Result<(), CheckpointError> {
        let Some(relative_index) = self.leaf.sequence.checked_sub(self.base_sequence) else {
            return Err(CheckpointError::InvalidProof);
        };
        let Some(last_sequence) = self
            .base_sequence
            .checked_add(self.leaf_count.saturating_sub(1))
        else {
            return Err(CheckpointError::InvalidProof);
        };
        if self.leaf_count == 0
            || self.leaf_count > MAX_CHECKPOINT_LEAVES as u64
            || relative_index >= self.leaf_count
            || self.siblings.len() > MAX_CHECKPOINT_PROOF_DEPTH
            || self.leaf.feed_id != checkpoint.signed.body.feed_id
            || last_sequence != checkpoint.signed.body.covered_sequence
        {
            return Err(CheckpointError::InvalidProof);
        }
        let mut expected_levels = 0usize;
        let mut width = self.leaf_count as usize;
        while width > 1 {
            expected_levels += 1;
            width = width.div_ceil(2);
        }
        if self.siblings.len() != expected_levels {
            return Err(CheckpointError::InvalidProof);
        }
        let mut digest = self.leaf.hash()?;
        for sibling in &self.siblings {
            digest = if sibling.sibling_on_left {
                node_hash(sibling.digest, digest)
            } else {
                node_hash(digest, sibling.digest)
            };
        }
        let root = match self.extension {
            Some(binding) => {
                if checkpoint.signed.body.previous_checkpoint_cid
                    != Some(binding.previous_checkpoint_cid)
                    || binding.previous_covered_sequence.checked_add(1) != Some(self.base_sequence)
                {
                    return Err(CheckpointError::InvalidProof);
                }
                checkpoint_extension_root(
                    CheckpointBase {
                        checkpoint_cid: binding.previous_checkpoint_cid,
                        covered_sequence: binding.previous_covered_sequence,
                        covered_root: binding.previous_covered_root,
                        last_event_cid: EventCid::from_bytes([0; 32]),
                        state_cid: ManifestCid::from_bytes([0; 32]),
                    },
                    self.base_sequence,
                    last_sequence,
                    digest,
                )
            }
            None => {
                if self.base_sequence != 0
                    || checkpoint.signed.body.previous_checkpoint_cid.is_some()
                {
                    return Err(CheckpointError::InvalidProof);
                }
                digest
            }
        };
        if root != checkpoint.signed.body.covered_root {
            return Err(CheckpointError::InvalidProof);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedCheckpointConsistency {
    pub previous: CheckpointCid,
    pub current: CheckpointCid,
    pub appended_leaf_count: usize,
}

pub trait CheckpointEffectVerifier {
    fn verify_effect(&self, reducer_version: [u8; 32], leaf: &CheckpointLeaf) -> bool;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointSuppressionAssessment {
    AuthorizedRelative(ValidatedCheckpointSuppression),
    UnresolvedMissingHistory,
    UnresolvedPreviousCheckpoint,
    UnresolvedKeyState,
    UnresolvedMissingEffectVerifier,
    QuarantinedRevokedRelative,
    RejectedHistoryProof,
    RejectedReducerEffect,
}

impl CheckpointSuppressionAssessment {
    pub const fn permits_suppression(&self) -> bool {
        matches!(self, Self::AuthorizedRelative(_))
    }

    pub const fn permits_payload_deletion(&self) -> bool {
        false
    }

    pub const fn authority(&self) -> Option<&ValidatedCheckpointSuppression> {
        match self {
            Self::AuthorizedRelative(authority) => Some(authority),
            _ => None,
        }
    }
}

/// Unforgeable outside this module; authorizes only checkpoint read-path
/// suppression and never payload deletion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedCheckpointSuppression {
    checkpoint_cid: CheckpointCid,
    covered_sequence: u64,
}

impl ValidatedCheckpointSuppression {
    pub const fn checkpoint_cid(&self) -> CheckpointCid {
        self.checkpoint_cid
    }

    pub const fn covered_sequence(&self) -> u64 {
        self.covered_sequence
    }
}

pub fn assess_checkpoint_suppression(
    checkpoint: &ValidatedFeedCheckpoint,
    key_state: &KeyStateCheckpointProof,
    witness: Option<&CheckpointHistoryWitness>,
    effect_verifier: Option<&dyn CheckpointEffectVerifier>,
) -> CheckpointSuppressionAssessment {
    assess_checkpoint_suppression_inner(checkpoint, key_state, witness, effect_verifier, None)
}

pub fn assess_checkpoint_extension_suppression(
    checkpoint: &ValidatedFeedCheckpoint,
    key_state: &KeyStateCheckpointProof,
    witness: Option<&CheckpointHistoryWitness>,
    effect_verifier: Option<&dyn CheckpointEffectVerifier>,
    previous: &ValidatedFeedCheckpoint,
    previous_authority: &ValidatedCheckpointSuppression,
) -> CheckpointSuppressionAssessment {
    assess_checkpoint_suppression_inner(
        checkpoint,
        key_state,
        witness,
        effect_verifier,
        Some((previous, previous_authority)),
    )
}

fn assess_checkpoint_suppression_inner(
    checkpoint: &ValidatedFeedCheckpoint,
    key_state: &KeyStateCheckpointProof,
    witness: Option<&CheckpointHistoryWitness>,
    effect_verifier: Option<&dyn CheckpointEffectVerifier>,
    previous: Option<(&ValidatedFeedCheckpoint, &ValidatedCheckpointSuppression)>,
) -> CheckpointSuppressionAssessment {
    let body = &checkpoint.signed.body;
    if key_state.subject_feed != body.feed_id
        || body.key_state_root != Some(key_state.state_root)
        || body.key_state_frontier != key_state.frontier
    {
        return CheckpointSuppressionAssessment::UnresolvedKeyState;
    }
    match key_state.decision {
        FeedAuthorityDecision::AuthorizedRelative { frontier, .. }
            if frontier == body.key_state_frontier => {}
        FeedAuthorityDecision::AuthorizedRelative { .. }
        | FeedAuthorityDecision::StaleOrUnresolved { .. } => {
            return CheckpointSuppressionAssessment::UnresolvedKeyState;
        }
        FeedAuthorityDecision::QuarantinedRevokedRelative { .. } => {
            return CheckpointSuppressionAssessment::QuarantinedRevokedRelative;
        }
    }
    let Some(witness) = witness else {
        return CheckpointSuppressionAssessment::UnresolvedMissingHistory;
    };
    if !witness.matches_checkpoint(checkpoint) {
        return CheckpointSuppressionAssessment::RejectedHistoryProof;
    }
    match (witness.previous, previous) {
        (Some(_), None) => {
            return CheckpointSuppressionAssessment::UnresolvedPreviousCheckpoint;
        }
        (Some(_), Some((previous, authority))) => {
            if authority.checkpoint_cid != previous.checkpoint_cid
                || witness.validate_consistency(previous, checkpoint).is_err()
            {
                return CheckpointSuppressionAssessment::RejectedHistoryProof;
            }
        }
        (None, Some(_)) => return CheckpointSuppressionAssessment::RejectedHistoryProof,
        (None, None) => {}
    }
    let Some(verifier) = effect_verifier else {
        return CheckpointSuppressionAssessment::UnresolvedMissingEffectVerifier;
    };
    if witness
        .leaves
        .iter()
        .any(|leaf| !verifier.verify_effect(body.reducer_version, leaf))
    {
        return CheckpointSuppressionAssessment::RejectedReducerEffect;
    }
    CheckpointSuppressionAssessment::AuthorizedRelative(ValidatedCheckpointSuppression {
        checkpoint_cid: checkpoint.checkpoint_cid,
        covered_sequence: checkpoint.signed.body.covered_sequence,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointApplyOutcome {
    Added,
    ExactReplay,
    ParallelSameRootRetained,
    ConflictObserved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointConflictProof {
    pub feed_id: FeedId,
    pub covered_sequence: u64,
    pub checkpoint_cids: Vec<CheckpointCid>,
    pub covered_roots: Vec<[u8; 32]>,
}

#[derive(Default)]
pub struct CheckpointRegister {
    positions: BTreeMap<(FeedId, u64), BTreeMap<[u8; 32], ValidatedFeedCheckpoint>>,
}

impl CheckpointRegister {
    pub fn apply(&mut self, checkpoint: ValidatedFeedCheckpoint) -> CheckpointApplyOutcome {
        let key = (
            checkpoint.signed.body.feed_id,
            checkpoint.signed.body.covered_sequence,
        );
        let cid = checkpoint.checkpoint_cid.into_bytes();
        let position = self.positions.entry(key).or_default();
        if position.contains_key(&cid) {
            return CheckpointApplyOutcome::ExactReplay;
        }
        let existing_roots: BTreeSet<_> = position
            .values()
            .map(|item| item.signed.body.covered_root)
            .collect();
        let same_root = existing_roots.contains(&checkpoint.signed.body.covered_root);
        let had_record = !position.is_empty();
        position.insert(cid, checkpoint);
        if existing_roots.is_empty() {
            CheckpointApplyOutcome::Added
        } else if same_root && existing_roots.len() == 1 {
            CheckpointApplyOutcome::ParallelSameRootRetained
        } else if had_record {
            CheckpointApplyOutcome::ConflictObserved
        } else {
            CheckpointApplyOutcome::Added
        }
    }

    pub fn conflict_proofs(&self) -> Vec<CheckpointConflictProof> {
        self.positions
            .iter()
            .filter_map(|(&(feed_id, covered_sequence), records)| {
                let roots: BTreeSet<_> = records
                    .values()
                    .map(|item| item.signed.body.covered_root)
                    .collect();
                (roots.len() > 1).then(|| CheckpointConflictProof {
                    feed_id,
                    covered_sequence,
                    checkpoint_cids: records
                        .keys()
                        .copied()
                        .map(CheckpointCid::from_bytes)
                        .collect(),
                    covered_roots: roots.into_iter().collect(),
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Canonical(CanonicalError),
    UnsupportedVersion,
    InvalidField,
    NonCanonical,
    SignerMismatch,
    SignatureInvalid,
    SignatureDomain,
    InvalidHistory,
    InvalidEffectChain,
    InvalidProof,
    InvalidConsistencyProof,
    ResourceLimit,
}

impl CheckpointError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Canonical(_) => "CHECKPOINT_CANONICAL",
            Self::UnsupportedVersion => "CHECKPOINT_VERSION",
            Self::InvalidField => "CHECKPOINT_FIELD",
            Self::NonCanonical => "CHECKPOINT_NON_CANONICAL",
            Self::SignerMismatch => "CHECKPOINT_SIGNER_MISMATCH",
            Self::SignatureInvalid => "CHECKPOINT_SIGNATURE",
            Self::SignatureDomain => "CHECKPOINT_SIGNATURE_DOMAIN",
            Self::InvalidHistory => "CHECKPOINT_HISTORY",
            Self::InvalidEffectChain => "CHECKPOINT_EFFECT_CHAIN",
            Self::InvalidProof => "CHECKPOINT_INCLUSION_PROOF",
            Self::InvalidConsistencyProof => "CHECKPOINT_CONSISTENCY_PROOF",
            Self::ResourceLimit => "CHECKPOINT_RESOURCE_LIMIT",
        }
    }
}

impl From<CanonicalError> for CheckpointError {
    fn from(value: CanonicalError) -> Self {
        Self::Canonical(value)
    }
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for CheckpointError {}

fn unsigned_bytes(
    body: &FeedCheckpointBody,
    signer_feed: FeedId,
) -> Result<Vec<u8>, CheckpointError> {
    encode_record(body, signer_feed, None)
}

fn encode_record(
    body: &FeedCheckpointBody,
    signer_feed: FeedId,
    signature: Option<[u8; 64]>,
) -> Result<Vec<u8>, CheckpointError> {
    let mut fields = vec![
        (0, CanonicalValue::Unsigned(SCHEMA_FEED_CHECKPOINT)),
        (1, CanonicalValue::Unsigned(CHECKPOINT_PROFILE_MAJOR)),
        (2, CanonicalValue::Unsigned(CHECKPOINT_PROFILE_MINOR)),
        (3, body.to_value()?),
        (4, CanonicalValue::Bytes(signer_feed.as_bytes().to_vec())),
    ];
    if let Some(signature) = signature {
        fields.push((5, CanonicalValue::Bytes(signature.to_vec())));
    }
    Ok(encode_canonical(
        &CanonicalValue::Map(fields),
        ResourceProfile::ControlV1,
    )?)
}

fn optional_digest(value: Option<[u8; 32]>) -> CanonicalValue {
    value
        .map(|digest| CanonicalValue::Bytes(digest.to_vec()))
        .unwrap_or(CanonicalValue::Null)
}

fn merkle_root(leaves: &[CheckpointLeaf]) -> Result<[u8; 32], CheckpointError> {
    if leaves.is_empty() || leaves.len() > MAX_CHECKPOINT_LEAVES {
        return Err(CheckpointError::InvalidHistory);
    }
    let mut level = leaves
        .iter()
        .map(CheckpointLeaf::hash)
        .collect::<Result<Vec<_>, _>>()?;
    while level.len() > 1 {
        level = next_merkle_level(&level);
    }
    Ok(level[0])
}

fn next_merkle_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    level
        .chunks(2)
        .map(|pair| node_hash(pair[0], pair.get(1).copied().unwrap_or(pair[0])))
        .collect()
}

fn node_hash(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    domain_hash(b"checkpoint-node/1", &[&left, &right])
}

fn checkpoint_extension_root(
    previous: CheckpointBase,
    base_sequence: u64,
    last_sequence: u64,
    chunk_root: [u8; 32],
) -> [u8; 32] {
    domain_hash(
        b"checkpoint-extension/1",
        &[
            previous.checkpoint_cid.as_bytes(),
            &previous.covered_sequence.to_be_bytes(),
            &previous.covered_root,
            &base_sequence.to_be_bytes(),
            &last_sequence.to_be_bytes(),
            &chunk_root,
        ],
    )
}

fn domain_hash(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:");
    hasher.update(domain);
    hasher.update(&[0]);
    for part in parts {
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

fn map(value: &CanonicalValue) -> Result<&[(u64, CanonicalValue)], CheckpointError> {
    match value {
        CanonicalValue::Map(values) => Ok(values),
        _ => Err(CheckpointError::InvalidField),
    }
}

fn required(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, CheckpointError> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(CheckpointError::InvalidField)
}

fn unsigned(fields: &[(u64, CanonicalValue)], key: u64) -> Result<u64, CheckpointError> {
    match required(fields, key)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(CheckpointError::InvalidField),
    }
}

fn fixed_bytes<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], CheckpointError> {
    match value {
        CanonicalValue::Bytes(bytes) if bytes.len() == N => {
            let mut output = [0; N];
            output.copy_from_slice(bytes);
            Ok(output)
        }
        _ => Err(CheckpointError::InvalidField),
    }
}

fn bytes32(fields: &[(u64, CanonicalValue)], key: u64) -> Result<[u8; 32], CheckpointError> {
    fixed_bytes(required(fields, key)?)
}

fn bytes64(fields: &[(u64, CanonicalValue)], key: u64) -> Result<[u8; 64], CheckpointError> {
    fixed_bytes(required(fields, key)?)
}

fn optional_bytes32(
    fields: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<Option<[u8; 32]>, CheckpointError> {
    match required(fields, key)? {
        CanonicalValue::Null => Ok(None),
        value => fixed_bytes(value).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, decode_knowledge_event, ActorId, DelegationGrant, DeviceId,
        DisclosureClass, EventType, FeedInception, KeyStateReducer, KnowledgeEventEnvelope,
        NamespaceCommitment, ScopedDelegation,
    };

    const EVENT: EventType = EventType(1);
    const REDUCER: [u8; 32] = [70; 32];

    struct ExactEffects;

    impl CheckpointEffectVerifier for ExactEffects {
        fn verify_effect(&self, reducer_version: [u8; 32], leaf: &CheckpointLeaf) -> bool {
            reducer_version == REDUCER && leaf.effect_root == [leaf.sequence as u8 + 40; 32]
        }
    }

    fn setup() -> (
        SigningKey,
        ValidatedFeedInception,
        KeyStateReducer,
        CheckpointHistoryWitness,
    ) {
        let key = SigningKey::from_bytes(&[11; 32]);
        let delegation = EventCid::from_bytes([12; 32]);
        let device = DeviceId::from_bytes([13; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"checkpoint-tests", [14; 32]).unwrap(),
            0,
            device,
        );
        inception.actor_delegation_ref = Some(*delegation.as_bytes());
        let signed = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let frontier = EventCid::from_bytes([15; 32]);
        let mut key_state = KeyStateReducer::new(frontier);
        key_state.accept_root(ScopedDelegation {
            grant: DelegationGrant {
                actor: ActorId::from_bytes([16; 32]),
                device,
                subject_feed: feed.feed_id,
                delegation_ref: delegation,
                namespace_commitment: Some(feed.signed.inception.namespace_commitment),
                first_generation: 0,
                last_generation: 2,
                proof: EventCid::from_bytes([17; 32]),
            },
            parent_delegation_ref: None,
        });

        let mut leaves = Vec::new();
        let mut parent = None;
        for sequence in 0..3u64 {
            let mut event = KnowledgeEventEnvelope::new(
                EVENT,
                feed.feed_id,
                sequence,
                DisclosureClass::Public,
                [sequence as u8 + 1; 32],
            );
            event.causal_parents = parent.into_iter().collect();
            let (bytes, _) = event.sign(&feed, &key).unwrap().encode().unwrap();
            let validated = decode_knowledge_event(&bytes, &feed, &[EVENT]).unwrap();
            parent = Some(validated.cid());
            leaves.push(
                CheckpointLeaf::from_validated_event(
                    &validated,
                    ManifestCid::from_bytes([sequence as u8 + 20; 32]),
                    ManifestCid::from_bytes([sequence as u8 + 21; 32]),
                    [sequence as u8 + 40; 32],
                )
                .unwrap(),
            );
        }
        (
            key,
            feed,
            key_state,
            CheckpointHistoryWitness::new(leaves).unwrap(),
        )
    }

    fn checkpoint(
        key: &SigningKey,
        feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        witness: &CheckpointHistoryWitness,
        previous: Option<CheckpointCid>,
        nonce: u8,
    ) -> ValidatedFeedCheckpoint {
        let body = FeedCheckpointBody::from_witness(
            witness,
            REDUCER,
            previous,
            [90; 32],
            &key_state.checkpoint_proof(feed),
            None,
            [nonce; 32],
        )
        .unwrap();
        let signed = SignedFeedCheckpoint::sign(body, feed, key).unwrap();
        decode_feed_checkpoint(&signed.encode().unwrap(), feed).unwrap()
    }

    #[test]
    fn signature_is_not_suppression_authority_without_all_proofs() {
        let (key, feed, state, witness) = setup();
        let checkpoint = checkpoint(&key, &feed, &state, &witness, None, 91);
        assert!(!checkpoint.has_suppression_authority_without_proofs());
        assert_eq!(
            assess_checkpoint_suppression(
                &checkpoint,
                &state.checkpoint_proof(&feed),
                None,
                Some(&ExactEffects),
            ),
            CheckpointSuppressionAssessment::UnresolvedMissingHistory
        );
        assert_eq!(
            assess_checkpoint_suppression(
                &checkpoint,
                &state.checkpoint_proof(&feed),
                Some(&witness),
                None,
            ),
            CheckpointSuppressionAssessment::UnresolvedMissingEffectVerifier
        );
        let authorized = assess_checkpoint_suppression(
            &checkpoint,
            &state.checkpoint_proof(&feed),
            Some(&witness),
            Some(&ExactEffects),
        );
        assert!(authorized.permits_suppression());
        assert_eq!(
            authorized.authority().unwrap().checkpoint_cid(),
            checkpoint.checkpoint_cid
        );
        assert!(!authorized.permits_payload_deletion());
    }

    #[test]
    fn inclusion_and_effect_are_bound_to_exact_event_and_root() {
        let (key, feed, state, witness) = setup();
        let checkpoint = checkpoint(&key, &feed, &state, &witness, None, 92);
        let proof = witness.inclusion_proof(1).unwrap();
        proof.validate(&checkpoint).unwrap();
        let mut tampered = proof;
        tampered.siblings[0].digest[0] ^= 1;
        assert_eq!(
            tampered.validate(&checkpoint).unwrap_err().code(),
            "CHECKPOINT_INCLUSION_PROOF"
        );
        let mut unlinked = witness.leaves.clone();
        unlinked[1].causal_parents.clear();
        assert_eq!(
            CheckpointHistoryWitness::new(unlinked).unwrap_err(),
            CheckpointError::InvalidHistory
        );
    }

    #[test]
    fn append_consistency_binds_previous_checkpoint_and_prefix_root() {
        let (key, feed, state, witness) = setup();
        let first_witness = CheckpointHistoryWitness::new(witness.leaves[..2].to_vec()).unwrap();
        let first = checkpoint(&key, &feed, &state, &first_witness, None, 93);
        let extension_witness =
            CheckpointHistoryWitness::extend(&first, witness.leaves[2..].to_vec()).unwrap();
        let second = checkpoint(
            &key,
            &feed,
            &state,
            &extension_witness,
            Some(first.checkpoint_cid),
            94,
        );
        let proof = extension_witness
            .validate_consistency(&first, &second)
            .unwrap();
        assert_eq!(proof.appended_leaf_count, 1);
        extension_witness
            .inclusion_proof(2)
            .unwrap()
            .validate(&second)
            .unwrap();
        assert_eq!(
            assess_checkpoint_suppression(
                &second,
                &state.checkpoint_proof(&feed),
                Some(&extension_witness),
                Some(&ExactEffects),
            ),
            CheckpointSuppressionAssessment::UnresolvedPreviousCheckpoint
        );
        let first_assessment = assess_checkpoint_suppression(
            &first,
            &state.checkpoint_proof(&feed),
            Some(&first_witness),
            Some(&ExactEffects),
        );
        let second_assessment = assess_checkpoint_extension_suppression(
            &second,
            &state.checkpoint_proof(&feed),
            Some(&extension_witness),
            Some(&ExactEffects),
            &first,
            first_assessment.authority().unwrap(),
        );
        assert!(second_assessment.permits_suppression());
    }

    #[test]
    fn same_position_different_root_is_conflict_without_arrival_winner() {
        let (key, feed, state, witness) = setup();
        let left = checkpoint(&key, &feed, &state, &witness, None, 95);
        let mut other_leaves = witness.leaves.clone();
        other_leaves[2].effect_root = [99; 32];
        let other_witness = CheckpointHistoryWitness::new(other_leaves).unwrap();
        let right = checkpoint(&key, &feed, &state, &other_witness, None, 96);
        let mut register = CheckpointRegister::default();
        assert_eq!(register.apply(left), CheckpointApplyOutcome::Added);
        assert_eq!(
            register.apply(right),
            CheckpointApplyOutcome::ConflictObserved
        );
        let conflicts = register.conflict_proofs();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].checkpoint_cids.len(), 2);
        assert_eq!(conflicts[0].covered_roots.len(), 2);
    }

    #[test]
    fn foreign_or_stale_key_state_never_authorizes_suppression() {
        let (key, feed, state, witness) = setup();
        let checkpoint = checkpoint(&key, &feed, &state, &witness, None, 97);
        let foreign = KeyStateCheckpointProof {
            subject_feed: feed.feed_id,
            frontier: EventCid::from_bytes([111; 32]),
            state_root: [112; 32],
            decision: state.checkpoint_proof(&feed).decision,
        };
        assert_eq!(
            assess_checkpoint_suppression(
                &checkpoint,
                &foreign,
                Some(&witness),
                Some(&ExactEffects),
            ),
            CheckpointSuppressionAssessment::UnresolvedKeyState
        );
        let mut wrong_subject = state.checkpoint_proof(&feed);
        wrong_subject.subject_feed = FeedId::from_bytes([113; 32]);
        assert_eq!(
            assess_checkpoint_suppression(
                &checkpoint,
                &wrong_subject,
                Some(&witness),
                Some(&ExactEffects),
            ),
            CheckpointSuppressionAssessment::UnresolvedKeyState
        );
    }
}
