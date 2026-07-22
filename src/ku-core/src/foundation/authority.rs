//! Frontier-relative feed delegation, rotation, and revocation decisions.
//!
//! This module deliberately has no global validity oracle. Missing authority
//! evidence is unresolved and may become usable after a disconnected partition
//! receives more events. Only evidence already accepted into the caller's local
//! frontier can authorize or quarantine a feed.

use super::content_id::EventCid;
use super::feed::{FeedInception, ValidatedFeedInception};
use super::identity::{ActorId, DeviceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelegationGrant {
    pub actor: ActorId,
    pub device: DeviceId,
    pub delegation_ref: EventCid,
    pub namespace_commitment: Option<super::feed::NamespaceCommitment>,
    pub first_generation: u64,
    pub last_generation: u64,
    pub proof: EventCid,
}

impl DelegationGrant {
    fn covers(&self, feed: &FeedInception) -> bool {
        self.device == feed.owner_device
            && Some(*self.delegation_ref.as_bytes()) == feed.actor_delegation_ref
            && self
                .namespace_commitment
                .is_none_or(|namespace| namespace == feed.namespace_commitment)
            && (self.first_generation..=self.last_generation).contains(&feed.generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedRevocation {
    pub actor: ActorId,
    pub device: DeviceId,
    pub delegation_ref: EventCid,
    pub revoked_from_generation: u64,
    pub proof: EventCid,
}

impl AcceptedRevocation {
    fn covers(&self, feed: &FeedInception) -> bool {
        self.device == feed.owner_device
            && Some(*self.delegation_ref.as_bytes()) == feed.actor_delegation_ref
            && feed.generation >= self.revoked_from_generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnresolvedAuthorityReason {
    MissingDelegationReference,
    MissingAcceptedGrant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedAuthorityDecision {
    AuthorizedRelative {
        actor: ActorId,
        grant: EventCid,
        frontier: EventCid,
    },
    StaleOrUnresolved {
        reason: UnresolvedAuthorityReason,
        frontier: EventCid,
    },
    QuarantinedRevokedRelative {
        actor: ActorId,
        revocation: EventCid,
        frontier: EventCid,
    },
}

impl FeedAuthorityDecision {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::AuthorizedRelative { .. } => "AUTHORIZED_RELATIVE",
            Self::StaleOrUnresolved { .. } => "STALE_OR_UNRESOLVED",
            Self::QuarantinedRevokedRelative { .. } => "QUARANTINED_REVOKED_RELATIVE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuccessorStructureError {
    MissingOrWrongPredecessor,
    GenerationNotIncremented,
    OwnerDeviceChanged,
    MissingPreRotationCommitment,
    PreRotationCommitmentMismatch,
}

impl SuccessorStructureError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::MissingOrWrongPredecessor => "SUCCESSOR_PREDECESSOR_MISMATCH",
            Self::GenerationNotIncremented => "SUCCESSOR_GENERATION_MISMATCH",
            Self::OwnerDeviceChanged => "SUCCESSOR_OWNER_DEVICE_MISMATCH",
            Self::MissingPreRotationCommitment => "SUCCESSOR_COMMITMENT_MISSING",
            Self::PreRotationCommitmentMismatch => "SUCCESSOR_COMMITMENT_MISMATCH",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedSuccessorDecision {
    AuthorizedRelative(FeedAuthorityDecision),
    StaleOrUnresolved(FeedAuthorityDecision),
    QuarantinedRevokedRelative(FeedAuthorityDecision),
    RejectedStructural(SuccessorStructureError),
}

impl FeedSuccessorDecision {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::AuthorizedRelative(decision)
            | Self::StaleOrUnresolved(decision)
            | Self::QuarantinedRevokedRelative(decision) => decision.code(),
            Self::RejectedStructural(error) => error.code(),
        }
    }
}

/// An immutable local projection of authority evidence accepted at `frontier`.
pub struct FeedAuthorityView<'a> {
    pub frontier: EventCid,
    pub grants: &'a [DelegationGrant],
    pub revocations: &'a [AcceptedRevocation],
}

impl FeedAuthorityView<'_> {
    pub fn evaluate(&self, feed: &ValidatedFeedInception) -> FeedAuthorityDecision {
        let inception = &feed.signed.inception;
        let Some(delegation_ref) = inception.actor_delegation_ref else {
            return FeedAuthorityDecision::StaleOrUnresolved {
                reason: UnresolvedAuthorityReason::MissingDelegationReference,
                frontier: self.frontier,
            };
        };

        if let Some(revocation) = self.revocations.iter().find(|proof| {
            proof.delegation_ref.as_bytes() == &delegation_ref && proof.covers(inception)
        }) {
            return FeedAuthorityDecision::QuarantinedRevokedRelative {
                actor: revocation.actor,
                revocation: revocation.proof,
                frontier: self.frontier,
            };
        }

        if let Some(grant) = self.grants.iter().find(|proof| {
            proof.delegation_ref.as_bytes() == &delegation_ref && proof.covers(inception)
        }) {
            return FeedAuthorityDecision::AuthorizedRelative {
                actor: grant.actor,
                grant: grant.proof,
                frontier: self.frontier,
            };
        }

        FeedAuthorityDecision::StaleOrUnresolved {
            reason: UnresolvedAuthorityReason::MissingAcceptedGrant,
            frontier: self.frontier,
        }
    }

    pub fn evaluate_successor(
        &self,
        predecessor: &ValidatedFeedInception,
        successor: &ValidatedFeedInception,
    ) -> FeedSuccessorDecision {
        if let Err(error) = validate_successor_structure(predecessor, successor) {
            return FeedSuccessorDecision::RejectedStructural(error);
        }
        match self.evaluate(successor) {
            decision @ FeedAuthorityDecision::AuthorizedRelative { .. } => {
                FeedSuccessorDecision::AuthorizedRelative(decision)
            }
            decision @ FeedAuthorityDecision::StaleOrUnresolved { .. } => {
                FeedSuccessorDecision::StaleOrUnresolved(decision)
            }
            decision @ FeedAuthorityDecision::QuarantinedRevokedRelative { .. } => {
                FeedSuccessorDecision::QuarantinedRevokedRelative(decision)
            }
        }
    }
}

pub fn validate_successor_structure(
    predecessor: &ValidatedFeedInception,
    successor: &ValidatedFeedInception,
) -> Result<(), SuccessorStructureError> {
    let previous = &predecessor.signed.inception;
    let next = &successor.signed.inception;
    if next.predecessor_feed != Some(predecessor.feed_id) {
        return Err(SuccessorStructureError::MissingOrWrongPredecessor);
    }
    if previous.generation.checked_add(1) != Some(next.generation) {
        return Err(SuccessorStructureError::GenerationNotIncremented);
    }
    if previous.owner_device != next.owner_device {
        return Err(SuccessorStructureError::OwnerDeviceChanged);
    }
    let claimed = previous
        .pre_rotation_commitment
        .ok_or(SuccessorStructureError::MissingPreRotationCommitment)?;
    let expected = FeedInception::successor_commitment(next)
        .map_err(|_| SuccessorStructureError::PreRotationCommitmentMismatch)?;
    if claimed != expected {
        return Err(SuccessorStructureError::PreRotationCommitmentMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{decode_feed_inception, NamespaceCommitment};

    fn validated(feed: FeedInception, key: &SigningKey) -> ValidatedFeedInception {
        let signed = feed.sign(key).unwrap();
        decode_feed_inception(&signed.encode().unwrap()).unwrap()
    }

    fn feed_pair() -> (ValidatedFeedInception, ValidatedFeedInception, EventCid) {
        let previous_key = SigningKey::from_bytes(&[1; 32]);
        let next_key = SigningKey::from_bytes(&[2; 32]);
        let device = DeviceId::from_bytes([3; 32]);
        let delegation = EventCid::from_bytes([4; 32]);
        let namespace = NamespaceCommitment::derive(b"authority-test", [5; 32]).unwrap();
        let mut next =
            FeedInception::new(*next_key.verifying_key().as_bytes(), namespace, 1, device);
        let mut previous = FeedInception::new(
            *previous_key.verifying_key().as_bytes(),
            namespace,
            0,
            device,
        );
        next.predecessor_feed = Some(previous.feed_id().unwrap());
        next.actor_delegation_ref = Some(*delegation.as_bytes());
        previous.commit_to_successor(&next).unwrap();
        (
            validated(previous, &previous_key),
            validated(next, &next_key),
            delegation,
        )
    }

    #[test]
    fn missing_authority_is_unresolved_not_globally_invalid() {
        let (_, successor, _) = feed_pair();
        let view = FeedAuthorityView {
            frontier: EventCid::from_bytes([9; 32]),
            grants: &[],
            revocations: &[],
        };
        assert_eq!(view.evaluate(&successor).code(), "STALE_OR_UNRESOLVED");
    }

    #[test]
    fn accepted_grant_authorizes_relative_to_the_frontier() {
        let (predecessor, successor, delegation) = feed_pair();
        let grant = DelegationGrant {
            actor: ActorId::from_bytes([6; 32]),
            device: successor.signed.inception.owner_device,
            delegation_ref: delegation,
            namespace_commitment: None,
            first_generation: 0,
            last_generation: 5,
            proof: EventCid::from_bytes([7; 32]),
        };
        let view = FeedAuthorityView {
            frontier: EventCid::from_bytes([9; 32]),
            grants: &[grant],
            revocations: &[],
        };
        assert_eq!(
            view.evaluate_successor(&predecessor, &successor).code(),
            "AUTHORIZED_RELATIVE"
        );
    }

    #[test]
    fn accepted_revocation_quarantines_only_covered_generations() {
        let (predecessor, successor, delegation) = feed_pair();
        let actor = ActorId::from_bytes([6; 32]);
        let grant = DelegationGrant {
            actor,
            device: successor.signed.inception.owner_device,
            delegation_ref: delegation,
            namespace_commitment: None,
            first_generation: 0,
            last_generation: 5,
            proof: EventCid::from_bytes([7; 32]),
        };
        let revocation = AcceptedRevocation {
            actor,
            device: successor.signed.inception.owner_device,
            delegation_ref: delegation,
            revoked_from_generation: 1,
            proof: EventCid::from_bytes([8; 32]),
        };
        let view = FeedAuthorityView {
            frontier: EventCid::from_bytes([9; 32]),
            grants: &[grant],
            revocations: &[revocation],
        };
        assert_eq!(
            view.evaluate_successor(&predecessor, &successor).code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
    }

    #[test]
    fn malformed_rotation_is_rejected_before_authority_evaluation() {
        let (_, successor, delegation) = feed_pair();
        let key = SigningKey::from_bytes(&[10; 32]);
        let mut bad = successor.signed.inception.clone();
        bad.feed_public_key = *key.verifying_key().as_bytes();
        bad.predecessor_feed = Some(successor.feed_id);
        bad.generation = 3;
        let bad = validated(bad, &key);
        let grant = DelegationGrant {
            actor: ActorId::from_bytes([6; 32]),
            device: bad.signed.inception.owner_device,
            delegation_ref: delegation,
            namespace_commitment: None,
            first_generation: 0,
            last_generation: 5,
            proof: EventCid::from_bytes([7; 32]),
        };
        let view = FeedAuthorityView {
            frontier: EventCid::from_bytes([9; 32]),
            grants: &[grant],
            revocations: &[],
        };
        assert_eq!(
            view.evaluate_successor(&successor, &bad).code(),
            "SUCCESSOR_GENERATION_MISMATCH"
        );
    }

    #[test]
    fn unrelated_revocation_does_not_poison_a_valid_grant() {
        let (_, successor, delegation) = feed_pair();
        let actor = ActorId::from_bytes([6; 32]);
        let grant = DelegationGrant {
            actor,
            device: successor.signed.inception.owner_device,
            delegation_ref: delegation,
            namespace_commitment: None,
            first_generation: 0,
            last_generation: 5,
            proof: EventCid::from_bytes([7; 32]),
        };
        let unrelated = AcceptedRevocation {
            actor,
            device: DeviceId::from_bytes([99; 32]),
            delegation_ref: delegation,
            revoked_from_generation: 0,
            proof: EventCid::from_bytes([8; 32]),
        };
        let view = FeedAuthorityView {
            frontier: EventCid::from_bytes([9; 32]),
            grants: &[grant],
            revocations: &[unrelated],
        };
        assert_eq!(view.evaluate(&successor).code(), "AUTHORIZED_RELATIVE");
    }
}
