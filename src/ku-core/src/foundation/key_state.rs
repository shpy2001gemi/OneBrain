//! Scoped delegation/key-state reducer at one accepted event frontier.

use std::collections::BTreeMap;

use super::authority::{
    AcceptedRevocation, DelegationGrant, FeedAuthorityDecision, FeedAuthorityView,
    FeedSuccessorDecision,
};
use super::content_id::EventCid;
use super::feed::ValidatedFeedInception;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStateApplyOutcome {
    Accepted,
    AlreadyPresent,
    StaleOrUnresolved,
    RejectedAttenuation,
    RejectedAuthority,
    ConflictingClaim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStateRecordStatus {
    Accepted,
    StaleOrUnresolved,
    RejectedAttenuation,
    RejectedAuthority,
    Unknown,
}

/// Exact, frontier-scoped commitment used by checkpoint validation.
///
/// This proof is produced from the reducer's complete accepted/pending state;
/// callers cannot manufacture an `AUTHORIZED_RELATIVE` result from a root
/// alone. It remains a local-frontier proof and says nothing about a newer
/// revocation that has not reached this node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyStateCheckpointProof {
    pub subject_feed: super::identity::FeedId,
    pub frontier: EventCid,
    pub state_root: [u8; 32],
    pub decision: FeedAuthorityDecision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopedDelegation {
    pub grant: DelegationGrant,
    pub parent_delegation_ref: Option<EventCid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopedRevocation {
    pub revocation: AcceptedRevocation,
    pub authorized_by: EventCid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rejection {
    Attenuation,
    Authority,
}

pub struct KeyStateReducer {
    frontier: EventCid,
    accepted: BTreeMap<[u8; 32], ScopedDelegation>,
    pending: BTreeMap<[u8; 32], ScopedDelegation>,
    rejected: BTreeMap<[u8; 32], Rejection>,
    revocations: BTreeMap<[u8; 32], ScopedRevocation>,
    pending_revocations: BTreeMap<[u8; 32], ScopedRevocation>,
}

impl KeyStateReducer {
    pub fn new(frontier: EventCid) -> Self {
        Self {
            frontier,
            accepted: BTreeMap::new(),
            pending: BTreeMap::new(),
            rejected: BTreeMap::new(),
            revocations: BTreeMap::new(),
            pending_revocations: BTreeMap::new(),
        }
    }

    pub const fn frontier(&self) -> EventCid {
        self.frontier
    }

    pub fn advance_frontier(&mut self, frontier: EventCid) {
        self.frontier = frontier;
    }

    /// Trust-root admission is explicit. A child cannot promote itself to root.
    pub fn accept_root(&mut self, root: ScopedDelegation) -> KeyStateApplyOutcome {
        if root.parent_delegation_ref.is_some() {
            return KeyStateApplyOutcome::RejectedAuthority;
        }
        if root.grant.first_generation > root.grant.last_generation {
            return KeyStateApplyOutcome::RejectedAttenuation;
        }
        let id = *root.grant.delegation_ref.as_bytes();
        match self.existing_delegation(id) {
            Some(existing) if existing == root => return KeyStateApplyOutcome::AlreadyPresent,
            Some(_) => return KeyStateApplyOutcome::ConflictingClaim,
            None => {}
        }
        self.accepted.insert(id, root);
        self.reconcile_pending();
        KeyStateApplyOutcome::Accepted
    }

    pub fn submit_child(&mut self, child: ScopedDelegation) -> KeyStateApplyOutcome {
        let Some(_) = child.parent_delegation_ref else {
            return KeyStateApplyOutcome::RejectedAuthority;
        };
        let id = *child.grant.delegation_ref.as_bytes();
        match self.existing_delegation(id) {
            Some(existing) if existing == child => return KeyStateApplyOutcome::AlreadyPresent,
            Some(_) => return KeyStateApplyOutcome::ConflictingClaim,
            None => {}
        }
        match self.assess_child(&child) {
            ChildAssessment::Accepted => {
                self.accepted.insert(id, child);
                self.reconcile_pending();
                KeyStateApplyOutcome::Accepted
            }
            ChildAssessment::MissingParent => {
                self.pending.insert(id, child);
                KeyStateApplyOutcome::StaleOrUnresolved
            }
            ChildAssessment::RejectedAttenuation => {
                self.rejected.insert(id, Rejection::Attenuation);
                KeyStateApplyOutcome::RejectedAttenuation
            }
            ChildAssessment::RejectedAuthority => {
                self.rejected.insert(id, Rejection::Authority);
                KeyStateApplyOutcome::RejectedAuthority
            }
        }
    }

    pub fn submit_revocation(&mut self, revocation: ScopedRevocation) -> KeyStateApplyOutcome {
        let id = *revocation.revocation.proof.as_bytes();
        if let Some(existing) = self
            .revocations
            .get(&id)
            .or(self.pending_revocations.get(&id))
        {
            return if *existing == revocation {
                KeyStateApplyOutcome::AlreadyPresent
            } else {
                KeyStateApplyOutcome::ConflictingClaim
            };
        }
        match self.assess_revocation(&revocation) {
            RevocationAssessment::Accepted => {
                self.revocations.insert(id, revocation);
                KeyStateApplyOutcome::Accepted
            }
            RevocationAssessment::MissingEvidence => {
                self.pending_revocations.insert(id, revocation);
                KeyStateApplyOutcome::StaleOrUnresolved
            }
            RevocationAssessment::RejectedAuthority => KeyStateApplyOutcome::RejectedAuthority,
        }
    }

    pub fn delegation_status(&self, delegation_ref: EventCid) -> KeyStateRecordStatus {
        let id = delegation_ref.as_bytes();
        if self.accepted.contains_key(id) {
            KeyStateRecordStatus::Accepted
        } else if self.pending.contains_key(id) {
            KeyStateRecordStatus::StaleOrUnresolved
        } else {
            match self.rejected.get(id) {
                Some(Rejection::Attenuation) => KeyStateRecordStatus::RejectedAttenuation,
                Some(Rejection::Authority) => KeyStateRecordStatus::RejectedAuthority,
                None => KeyStateRecordStatus::Unknown,
            }
        }
    }

    /// Return one already-admitted delegation projection. This is used by the
    /// wire authority validator to bind a child/revocation signature to the
    /// exact FeedId authorized by its parent, never to a caller-supplied key.
    pub fn accepted_delegation(&self, delegation_ref: EventCid) -> Option<ScopedDelegation> {
        self.accepted.get(delegation_ref.as_bytes()).copied()
    }

    pub fn evaluate(&self, feed: &ValidatedFeedInception) -> FeedAuthorityDecision {
        let grants: Vec<_> = self.accepted.values().map(|node| node.grant).collect();
        let revocations = self.expanded_revocations();
        FeedAuthorityView {
            frontier: self.frontier,
            grants: &grants,
            revocations: &revocations,
        }
        .evaluate(feed)
    }

    /// Commit the exact reducer maps and assess one feed at the same frontier.
    pub fn checkpoint_proof(&self, feed: &ValidatedFeedInception) -> KeyStateCheckpointProof {
        KeyStateCheckpointProof {
            subject_feed: feed.feed_id,
            frontier: self.frontier,
            state_root: self.checkpoint_root(),
            decision: self.evaluate(feed),
        }
    }

    /// Deterministic commitment to every high-water-relevant key-state map.
    pub fn checkpoint_root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:key-state-checkpoint:1\0");
        hasher.update(self.frontier.as_bytes());
        hash_delegation_map(&mut hasher, 0, &self.accepted);
        hash_delegation_map(&mut hasher, 1, &self.pending);

        hasher.update(&[2]);
        hasher.update(&(self.rejected.len() as u64).to_be_bytes());
        for (id, rejection) in &self.rejected {
            hasher.update(id);
            hasher.update(&[match rejection {
                Rejection::Attenuation => 0,
                Rejection::Authority => 1,
            }]);
        }

        hash_revocation_map(&mut hasher, 3, &self.revocations);
        hash_revocation_map(&mut hasher, 4, &self.pending_revocations);
        *hasher.finalize().as_bytes()
    }

    pub fn evaluate_successor(
        &self,
        predecessor: &ValidatedFeedInception,
        successor: &ValidatedFeedInception,
    ) -> FeedSuccessorDecision {
        let grants: Vec<_> = self.accepted.values().map(|node| node.grant).collect();
        let revocations = self.expanded_revocations();
        FeedAuthorityView {
            frontier: self.frontier,
            grants: &grants,
            revocations: &revocations,
        }
        .evaluate_successor(predecessor, successor)
    }

    fn existing_delegation(&self, id: [u8; 32]) -> Option<ScopedDelegation> {
        self.accepted.get(&id).or(self.pending.get(&id)).copied()
    }

    fn assess_child(&self, child: &ScopedDelegation) -> ChildAssessment {
        if child.grant.first_generation > child.grant.last_generation {
            return ChildAssessment::RejectedAttenuation;
        }
        let parent_id = child
            .parent_delegation_ref
            .expect("child assessment requires a parent");
        let Some(parent) = self.accepted.get(parent_id.as_bytes()) else {
            return ChildAssessment::MissingParent;
        };
        if parent.grant.actor != child.grant.actor {
            return ChildAssessment::RejectedAuthority;
        }
        if child.grant.first_generation < parent.grant.first_generation
            || child.grant.last_generation > parent.grant.last_generation
        {
            return ChildAssessment::RejectedAttenuation;
        }
        if let Some(parent_namespace) = parent.grant.namespace_commitment {
            if child.grant.namespace_commitment != Some(parent_namespace) {
                return ChildAssessment::RejectedAttenuation;
            }
        }
        ChildAssessment::Accepted
    }

    fn assess_revocation(&self, revocation: &ScopedRevocation) -> RevocationAssessment {
        let target_id = revocation.revocation.delegation_ref;
        let Some(target) = self.accepted.get(target_id.as_bytes()) else {
            return RevocationAssessment::MissingEvidence;
        };
        let Some(authorizer) = self.accepted.get(revocation.authorized_by.as_bytes()) else {
            return RevocationAssessment::MissingEvidence;
        };
        if target.grant.actor != revocation.revocation.actor
            || target.grant.device != revocation.revocation.device
            || authorizer.grant.actor != target.grant.actor
            || !self.is_ancestor_or_self(revocation.authorized_by, target_id)
        {
            return RevocationAssessment::RejectedAuthority;
        }
        RevocationAssessment::Accepted
    }

    fn is_ancestor_or_self(&self, ancestor: EventCid, mut node: EventCid) -> bool {
        loop {
            if ancestor == node {
                return true;
            }
            let Some(current) = self.accepted.get(node.as_bytes()) else {
                return false;
            };
            let Some(parent) = current.parent_delegation_ref else {
                return false;
            };
            node = parent;
        }
    }

    fn reconcile_pending(&mut self) {
        loop {
            let pending_ids: Vec<_> = self.pending.keys().copied().collect();
            let mut changed = false;
            for id in pending_ids {
                let child = self.pending[&id];
                match self.assess_child(&child) {
                    ChildAssessment::Accepted => {
                        self.pending.remove(&id);
                        self.accepted.insert(id, child);
                        changed = true;
                    }
                    ChildAssessment::RejectedAttenuation => {
                        self.pending.remove(&id);
                        self.rejected.insert(id, Rejection::Attenuation);
                        changed = true;
                    }
                    ChildAssessment::RejectedAuthority => {
                        self.pending.remove(&id);
                        self.rejected.insert(id, Rejection::Authority);
                        changed = true;
                    }
                    ChildAssessment::MissingParent => {}
                }
            }

            let pending_revocations: Vec<_> = self.pending_revocations.keys().copied().collect();
            for id in pending_revocations {
                let revocation = self.pending_revocations[&id];
                match self.assess_revocation(&revocation) {
                    RevocationAssessment::Accepted => {
                        self.pending_revocations.remove(&id);
                        self.revocations.insert(id, revocation);
                        changed = true;
                    }
                    RevocationAssessment::RejectedAuthority => {
                        self.pending_revocations.remove(&id);
                        changed = true;
                    }
                    RevocationAssessment::MissingEvidence => {}
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn expanded_revocations(&self) -> Vec<AcceptedRevocation> {
        let mut expanded = Vec::new();
        for scoped in self.revocations.values() {
            expanded.push(scoped.revocation);
            for node in self.accepted.values() {
                if node.grant.delegation_ref != scoped.revocation.delegation_ref
                    && self.is_ancestor_or_self(
                        scoped.revocation.delegation_ref,
                        node.grant.delegation_ref,
                    )
                {
                    expanded.push(AcceptedRevocation {
                        actor: node.grant.actor,
                        device: node.grant.device,
                        delegation_ref: node.grant.delegation_ref,
                        revoked_from_generation: scoped.revocation.revoked_from_generation,
                        proof: scoped.revocation.proof,
                    });
                }
            }
        }
        expanded
    }
}

fn hash_delegation_map(
    hasher: &mut blake3::Hasher,
    lane: u8,
    values: &BTreeMap<[u8; 32], ScopedDelegation>,
) {
    hasher.update(&[lane]);
    hasher.update(&(values.len() as u64).to_be_bytes());
    for (id, scoped) in values {
        let grant = scoped.grant;
        hasher.update(id);
        hasher.update(grant.actor.as_bytes());
        hasher.update(grant.device.as_bytes());
        hasher.update(grant.subject_feed.as_bytes());
        hasher.update(grant.delegation_ref.as_bytes());
        match grant.namespace_commitment {
            Some(namespace) => {
                hasher.update(&[1]);
                hasher.update(namespace.as_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }
        hasher.update(&grant.first_generation.to_be_bytes());
        hasher.update(&grant.last_generation.to_be_bytes());
        hasher.update(grant.proof.as_bytes());
        match scoped.parent_delegation_ref {
            Some(parent) => {
                hasher.update(&[1]);
                hasher.update(parent.as_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }
    }
}

fn hash_revocation_map(
    hasher: &mut blake3::Hasher,
    lane: u8,
    values: &BTreeMap<[u8; 32], ScopedRevocation>,
) {
    hasher.update(&[lane]);
    hasher.update(&(values.len() as u64).to_be_bytes());
    for (id, scoped) in values {
        let revocation = scoped.revocation;
        hasher.update(id);
        hasher.update(revocation.actor.as_bytes());
        hasher.update(revocation.device.as_bytes());
        hasher.update(revocation.delegation_ref.as_bytes());
        hasher.update(&revocation.revoked_from_generation.to_be_bytes());
        hasher.update(revocation.proof.as_bytes());
        hasher.update(scoped.authorized_by.as_bytes());
    }
}

#[derive(Clone, Copy)]
enum ChildAssessment {
    Accepted,
    MissingParent,
    RejectedAttenuation,
    RejectedAuthority,
}

#[derive(Clone, Copy)]
enum RevocationAssessment {
    Accepted,
    MissingEvidence,
    RejectedAuthority,
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, ActorId, DeviceId, FeedId, FeedInception, NamespaceCommitment,
    };

    fn grant(
        id: u8,
        parent: Option<EventCid>,
        actor: ActorId,
        device: DeviceId,
        namespace: Option<NamespaceCommitment>,
        first: u64,
        last: u64,
    ) -> ScopedDelegation {
        ScopedDelegation {
            grant: DelegationGrant {
                actor,
                device,
                subject_feed: FeedId::from_bytes([id; 32]),
                delegation_ref: EventCid::from_bytes([id; 32]),
                namespace_commitment: namespace,
                first_generation: first,
                last_generation: last,
                proof: EventCid::from_bytes([id; 32]),
            },
            parent_delegation_ref: parent,
        }
    }

    fn feed(
        key_byte: u8,
        device: DeviceId,
        namespace: NamespaceCommitment,
        generation: u64,
        delegation: EventCid,
    ) -> ValidatedFeedInception {
        let key = SigningKey::from_bytes(&[key_byte; 32]);
        let mut feed = FeedInception::new(
            *key.verifying_key().as_bytes(),
            namespace,
            generation,
            device,
        );
        feed.actor_delegation_ref = Some(*delegation.as_bytes());
        let signed = feed.sign(&key).unwrap();
        decode_feed_inception(&signed.encode().unwrap()).unwrap()
    }

    #[test]
    fn child_arriving_before_parent_stays_unresolved_then_reconciles() {
        let actor = ActorId::from_bytes([1; 32]);
        let device = DeviceId::from_bytes([2; 32]);
        let namespace = NamespaceCommitment::derive(b"key-state", [3; 32]).unwrap();
        let root = grant(10, None, actor, device, None, 0, 9);
        let mut child = grant(
            11,
            Some(root.grant.delegation_ref),
            actor,
            device,
            Some(namespace),
            1,
            3,
        );
        let candidate = feed(4, device, namespace, 1, child.grant.delegation_ref);
        child.grant.subject_feed = candidate.feed_id;
        let mut reducer = KeyStateReducer::new(EventCid::from_bytes([9; 32]));
        assert_eq!(
            reducer.submit_child(child),
            KeyStateApplyOutcome::StaleOrUnresolved
        );
        assert_eq!(reducer.evaluate(&candidate).code(), "STALE_OR_UNRESOLVED");
        assert_eq!(reducer.accept_root(root), KeyStateApplyOutcome::Accepted);
        assert_eq!(
            reducer.delegation_status(child.grant.delegation_ref),
            KeyStateRecordStatus::Accepted
        );
        assert_eq!(reducer.evaluate(&candidate).code(), "AUTHORIZED_RELATIVE");
    }

    #[test]
    fn child_cannot_widen_generation_or_namespace_scope() {
        let actor = ActorId::from_bytes([1; 32]);
        let device = DeviceId::from_bytes([2; 32]);
        let namespace = NamespaceCommitment::derive(b"key-state", [3; 32]).unwrap();
        let root = grant(10, None, actor, device, Some(namespace), 2, 4);
        let mut reducer = KeyStateReducer::new(EventCid::from_bytes([9; 32]));
        reducer.accept_root(root);
        let wide_generation = grant(
            11,
            Some(root.grant.delegation_ref),
            actor,
            device,
            Some(namespace),
            1,
            4,
        );
        let wide_namespace = grant(
            12,
            Some(root.grant.delegation_ref),
            actor,
            device,
            None,
            2,
            4,
        );
        assert_eq!(
            reducer.submit_child(wide_generation),
            KeyStateApplyOutcome::RejectedAttenuation
        );
        assert_eq!(
            reducer.submit_child(wide_namespace),
            KeyStateApplyOutcome::RejectedAttenuation
        );
    }

    #[test]
    fn ancestor_revocation_cascades_but_only_after_acceptance() {
        let actor = ActorId::from_bytes([1; 32]);
        let device = DeviceId::from_bytes([2; 32]);
        let namespace = NamespaceCommitment::derive(b"key-state", [3; 32]).unwrap();
        let root = grant(10, None, actor, device, None, 0, 9);
        let mut child = grant(
            11,
            Some(root.grant.delegation_ref),
            actor,
            device,
            Some(namespace),
            1,
            3,
        );
        let candidate = feed(4, device, namespace, 2, child.grant.delegation_ref);
        child.grant.subject_feed = candidate.feed_id;
        let mut reducer = KeyStateReducer::new(EventCid::from_bytes([9; 32]));
        reducer.accept_root(root);
        reducer.submit_child(child);
        assert_eq!(reducer.evaluate(&candidate).code(), "AUTHORIZED_RELATIVE");

        let revocation = ScopedRevocation {
            revocation: AcceptedRevocation {
                actor,
                device,
                delegation_ref: root.grant.delegation_ref,
                revoked_from_generation: 2,
                proof: EventCid::from_bytes([20; 32]),
            },
            authorized_by: root.grant.delegation_ref,
        };
        assert_eq!(
            reducer.submit_revocation(revocation),
            KeyStateApplyOutcome::Accepted
        );
        assert_eq!(
            reducer.evaluate(&candidate).code(),
            "QUARANTINED_REVOKED_RELATIVE"
        );
    }

    #[test]
    fn unauthorized_revocation_does_not_become_fresh_authority() {
        let actor = ActorId::from_bytes([1; 32]);
        let device = DeviceId::from_bytes([2; 32]);
        let root = grant(10, None, actor, device, None, 0, 9);
        let mut reducer = KeyStateReducer::new(EventCid::from_bytes([9; 32]));
        reducer.accept_root(root);
        let revocation = ScopedRevocation {
            revocation: AcceptedRevocation {
                actor,
                device,
                delegation_ref: root.grant.delegation_ref,
                revoked_from_generation: 0,
                proof: EventCid::from_bytes([20; 32]),
            },
            authorized_by: EventCid::from_bytes([99; 32]),
        };
        assert_eq!(
            reducer.submit_revocation(revocation),
            KeyStateApplyOutcome::StaleOrUnresolved
        );
    }

    #[test]
    fn structurally_committed_rotation_remains_authorized_in_scope() {
        let actor = ActorId::from_bytes([1; 32]);
        let device = DeviceId::from_bytes([2; 32]);
        let namespace = NamespaceCommitment::derive(b"key-state", [3; 32]).unwrap();
        let mut root = grant(10, None, actor, device, Some(namespace), 0, 3);
        let previous_key = SigningKey::from_bytes(&[4; 32]);
        let next_key = SigningKey::from_bytes(&[5; 32]);
        let mut previous = FeedInception::new(
            *previous_key.verifying_key().as_bytes(),
            namespace,
            0,
            device,
        );
        let mut next =
            FeedInception::new(*next_key.verifying_key().as_bytes(), namespace, 1, device);
        previous.actor_delegation_ref = Some(*root.grant.delegation_ref.as_bytes());
        next.actor_delegation_ref = Some(*root.grant.delegation_ref.as_bytes());
        next.predecessor_feed = Some(previous.feed_id().unwrap());
        previous.commit_to_successor(&next).unwrap();
        let previous =
            decode_feed_inception(&previous.sign(&previous_key).unwrap().encode().unwrap())
                .unwrap();
        let next = decode_feed_inception(&next.sign(&next_key).unwrap().encode().unwrap()).unwrap();
        root.grant.subject_feed = previous.feed_id;

        let mut reducer = KeyStateReducer::new(EventCid::from_bytes([9; 32]));
        reducer.accept_root(root);
        assert_eq!(
            reducer.evaluate_successor(&previous, &next).code(),
            "AUTHORIZED_RELATIVE"
        );
    }
}
