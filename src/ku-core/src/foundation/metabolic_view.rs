//! Policy/frontier-relative metabolic evidence views.
//!
//! Signed exercise evidence is cumulative and never time-decayed here. A
//! separate recent-activity projection may fade by per-feed event distance.
//! Local exposure telemetry cannot enter either evidence set.

use std::collections::{BTreeMap, BTreeSet};

use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalValue, ResourceProfile,
};
use super::content_id::EventCid;
use super::identity::FeedId;
use super::object::ObjectReference;
use super::use_evidence::{AssessedExerciseEvidence, ExerciseAuthority, ExerciseEvidence, UseMode};

pub const METABOLIC_VIEW_MAJOR: u64 = 1;
pub const METABOLIC_VIEW_MINOR: u64 = 0;
pub const MAX_ACCEPTED_EVIDENCE_POLICIES: usize = 64;
pub const MAX_METABOLIC_EVIDENCE_RECORDS: usize = 1_000_000;
pub const MAX_EXPOSURE_OBSERVATIONS: usize = 1_000_000;
const ACTIVITY_WEIGHT_SCALE: u64 = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetabolicViewPolicy {
    pub policy_ref: ObjectReference,
    pub accepted_evidence_policies: Vec<ObjectReference>,
    /// Positive horizon measured in later accepted events from the same Feed.
    pub recent_event_horizon: u64,
}

impl MetabolicViewPolicy {
    /// Validate and canonicalize a locally configured policy before it enters
    /// an allow-listed runtime registry.
    pub fn validated(&self) -> Result<Self, MetabolicViewError> {
        self.normalized()
    }

    fn normalized(&self) -> Result<Self, MetabolicViewError> {
        if self.policy_ref.cid == [0; 32]
            || self.accepted_evidence_policies.is_empty()
            || self.accepted_evidence_policies.len() > MAX_ACCEPTED_EVIDENCE_POLICIES
            || self.recent_event_horizon == 0
            || self
                .accepted_evidence_policies
                .iter()
                .any(|reference| reference.cid == [0; 32])
        {
            return Err(MetabolicViewError::InvalidPolicy);
        }
        let mut accepted = self.accepted_evidence_policies.clone();
        accepted.sort_by_key(|reference| (reference.reference_kind, reference.cid));
        accepted.dedup();
        if accepted.len() != self.accepted_evidence_policies.len() {
            return Err(MetabolicViewError::InvalidPolicy);
        }
        Ok(Self {
            policy_ref: self.policy_ref.clone(),
            accepted_evidence_policies: accepted,
            recent_event_horizon: self.recent_event_horizon,
        })
    }

    fn accepts(&self, evidence_policy: &ObjectReference) -> bool {
        self.accepted_evidence_policies
            .binary_search_by_key(
                &(evidence_policy.reference_kind, evidence_policy.cid),
                |reference| (reference.reference_kind, reference.cid),
            )
            .is_ok()
    }

    fn to_value(&self) -> Result<CanonicalValue, MetabolicViewError> {
        let members = self
            .accepted_evidence_policies
            .iter()
            .map(ObjectReference::to_value)
            .map(|value| (value.clone(), value))
            .collect();
        Ok(CanonicalValue::Map(vec![
            (0, self.policy_ref.to_value()),
            (
                1,
                CanonicalValue::Array(canonicalize_set_by_key(members, ResourceProfile::ObjectV1)?),
            ),
            (2, CanonicalValue::Unsigned(self.recent_event_horizon)),
        ]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetabolicEvidenceFrontier {
    authority_frontier: [u8; 32],
    accepted_feed_positions: BTreeMap<FeedId, u64>,
}

impl MetabolicEvidenceFrontier {
    pub fn new(
        authority_frontier: [u8; 32],
        positions: impl IntoIterator<Item = (FeedId, u64)>,
    ) -> Result<Self, MetabolicViewError> {
        if authority_frontier == [0; 32] {
            return Err(MetabolicViewError::InvalidFrontier);
        }
        let mut accepted_feed_positions = BTreeMap::<FeedId, u64>::new();
        for (feed, position) in positions {
            accepted_feed_positions
                .entry(feed)
                .and_modify(|existing| *existing = (*existing).max(position))
                .or_insert(position);
        }
        Ok(Self {
            authority_frontier,
            accepted_feed_positions,
        })
    }

    pub const fn authority_frontier(&self) -> &[u8; 32] {
        &self.authority_frontier
    }

    pub fn accepted_position(&self, feed: FeedId) -> Option<u64> {
        self.accepted_feed_positions.get(&feed).copied()
    }

    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.authority_frontier.to_vec())),
            (
                1,
                CanonicalValue::Array(
                    self.accepted_feed_positions
                        .iter()
                        .map(|(feed, position)| {
                            CanonicalValue::Array(vec![
                                CanonicalValue::Bytes(feed.as_bytes().to_vec()),
                                CanonicalValue::Unsigned(*position),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum MetabolicViewLimitation {
    LocalFrontierOnly = 0,
    RecentActivityUsesPerFeedEventDistance = 1,
    AuthorityUnresolved = 2,
    UnauthorizedEvidenceExcluded = 3,
    EvidenceBeyondFrontier = 4,
    EvidencePolicyExcluded = 5,
    LocalEvidenceRetentionBound = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentExerciseKind {
    Use(UseMode),
    DerivationInput,
    DerivationOutput,
    DerivationInputAndOutput,
}

impl RecentExerciseKind {
    const fn code(self) -> u64 {
        match self {
            Self::Use(mode) => mode as u64,
            Self::DerivationInput => 100,
            Self::DerivationOutput => 101,
            Self::DerivationInputAndOutput => 102,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentExerciseActivity {
    pub event_cid: EventCid,
    pub author_feed: FeedId,
    pub author_sequence: u64,
    pub later_events_in_author_feed: u64,
    /// A display/projection weight only; it is not cumulative value or reward.
    pub relative_weight_millionths: u32,
    pub kind: RecentExerciseKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetabolicEvidenceView {
    pub schema_major: u64,
    pub schema_minor: u64,
    pub target: ObjectReference,
    pub policy: MetabolicViewPolicy,
    pub frontier: MetabolicEvidenceFrontier,
    pub revision: u64,
    pub previous_view_root: Option<[u8; 32]>,
    pub view_root: [u8; 32],
    pub evidence_root: [u8; 32],
    pub frontier_root: [u8; 32],
    pub cumulative_event_ids: Vec<EventCid>,
    pub recent_activity: Vec<RecentExerciseActivity>,
    pub limitations: Vec<MetabolicViewLimitation>,
}

impl MetabolicEvidenceView {
    pub const fn is_globally_complete(&self) -> bool {
        false
    }

    pub const fn establishes_benefit(&self) -> bool {
        false
    }

    pub const fn is_reward_instruction(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetabolicRecordOutcome {
    Added,
    ExactReplay,
    Reassessed,
    CapacityReached,
}

#[derive(Clone, Copy, Debug)]
struct RevisionHead {
    revision: u64,
    root: [u8; 32],
    previous_root: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ViewLineageKey {
    target_kind: u64,
    target_cid: [u8; 32],
    policy_kind: u64,
    policy_cid: [u8; 32],
}

pub struct MetabolicEvidenceReducer {
    max_records: usize,
    records: BTreeMap<[u8; 32], AssessedExerciseEvidence>,
    dropped_for_capacity: u64,
    heads: BTreeMap<ViewLineageKey, RevisionHead>,
}

impl MetabolicEvidenceReducer {
    pub fn new(max_records: usize) -> Result<Self, MetabolicViewError> {
        if max_records == 0 || max_records > MAX_METABOLIC_EVIDENCE_RECORDS {
            return Err(MetabolicViewError::InvalidCapacity);
        }
        Ok(Self {
            max_records,
            records: BTreeMap::new(),
            dropped_for_capacity: 0,
            heads: BTreeMap::new(),
        })
    }

    pub fn record(&mut self, evidence: AssessedExerciseEvidence) -> MetabolicRecordOutcome {
        let key = evidence.evidence.event_cid().into_bytes();
        match self.records.get(&key) {
            Some(existing) if existing == &evidence => MetabolicRecordOutcome::ExactReplay,
            Some(_) => {
                self.records.insert(key, evidence);
                MetabolicRecordOutcome::Reassessed
            }
            None if self.records.len() == self.max_records => {
                self.dropped_for_capacity = self.dropped_for_capacity.saturating_add(1);
                MetabolicRecordOutcome::CapacityReached
            }
            None => {
                self.records.insert(key, evidence);
                MetabolicRecordOutcome::Added
            }
        }
    }

    pub fn materialize(
        &mut self,
        target: ObjectReference,
        policy: &MetabolicViewPolicy,
        frontier: &MetabolicEvidenceFrontier,
    ) -> Result<MetabolicEvidenceView, MetabolicViewError> {
        if target.cid == [0; 32] {
            return Err(MetabolicViewError::InvalidTarget);
        }
        let policy = policy.normalized()?;
        let mut limitations = BTreeSet::from([
            MetabolicViewLimitation::LocalFrontierOnly,
            MetabolicViewLimitation::RecentActivityUsesPerFeedEventDistance,
        ]);
        if self.dropped_for_capacity > 0 {
            limitations.insert(MetabolicViewLimitation::LocalEvidenceRetentionBound);
        }
        let mut cumulative_event_ids = Vec::new();
        let mut recent_activity = Vec::new();
        for record in self.records.values() {
            let Some((kind, evidence_policy)) = evidence_target(&record.evidence, &target) else {
                continue;
            };
            if !policy.accepts(evidence_policy) {
                limitations.insert(MetabolicViewLimitation::EvidencePolicyExcluded);
                continue;
            }
            match record.authority {
                ExerciseAuthority::Unauthorized => {
                    limitations.insert(MetabolicViewLimitation::UnauthorizedEvidenceExcluded);
                    continue;
                }
                ExerciseAuthority::Unresolved => {
                    limitations.insert(MetabolicViewLimitation::AuthorityUnresolved);
                    continue;
                }
                ExerciseAuthority::Authorized => {}
            }
            let feed = record.evidence.author_feed();
            let sequence = record.evidence.author_sequence();
            let Some(frontier_position) = frontier.accepted_position(feed) else {
                limitations.insert(MetabolicViewLimitation::EvidenceBeyondFrontier);
                continue;
            };
            let Some(distance) = frontier_position.checked_sub(sequence) else {
                limitations.insert(MetabolicViewLimitation::EvidenceBeyondFrontier);
                continue;
            };
            cumulative_event_ids.push(record.evidence.event_cid());
            if distance < policy.recent_event_horizon {
                let remaining = policy.recent_event_horizon - distance;
                let weight = (u128::from(remaining) * u128::from(ACTIVITY_WEIGHT_SCALE)
                    / u128::from(policy.recent_event_horizon)) as u32;
                recent_activity.push(RecentExerciseActivity {
                    event_cid: record.evidence.event_cid(),
                    author_feed: feed,
                    author_sequence: sequence,
                    later_events_in_author_feed: distance,
                    relative_weight_millionths: weight,
                    kind,
                });
            }
        }
        cumulative_event_ids.sort_by_key(|cid| cid.into_bytes());
        recent_activity.sort_by_key(|activity| activity.event_cid.into_bytes());
        let limitations = limitations.into_iter().collect::<Vec<_>>();
        let evidence_root = digest_value(
            b"exercise-evidence-set",
            &CanonicalValue::Array(
                cumulative_event_ids
                    .iter()
                    .map(|cid| CanonicalValue::Bytes(cid.as_bytes().to_vec()))
                    .collect(),
            ),
        )?;
        let frontier_root = digest_value(b"accepted-frontier", &frontier.to_value())?;
        let projection = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(METABOLIC_VIEW_MAJOR)),
            (1, CanonicalValue::Unsigned(METABOLIC_VIEW_MINOR)),
            (2, target.to_value()),
            (3, policy.to_value()?),
            (4, CanonicalValue::Bytes(frontier_root.to_vec())),
            (5, CanonicalValue::Bytes(evidence_root.to_vec())),
            (
                6,
                CanonicalValue::Array(
                    recent_activity
                        .iter()
                        .map(|activity| {
                            CanonicalValue::Map(vec![
                                (
                                    0,
                                    CanonicalValue::Bytes(activity.event_cid.as_bytes().to_vec()),
                                ),
                                (
                                    1,
                                    CanonicalValue::Unsigned(activity.later_events_in_author_feed),
                                ),
                                (
                                    2,
                                    CanonicalValue::Unsigned(u64::from(
                                        activity.relative_weight_millionths,
                                    )),
                                ),
                                (3, CanonicalValue::Unsigned(activity.kind.code())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                7,
                CanonicalValue::Array(
                    limitations
                        .iter()
                        .map(|limitation| CanonicalValue::Unsigned(*limitation as u64))
                        .collect(),
                ),
            ),
        ]);
        let view_root = digest_value(b"metabolic-evidence-view", &projection)?;
        let lineage = ViewLineageKey {
            target_kind: target.reference_kind,
            target_cid: target.cid,
            policy_kind: policy.policy_ref.reference_kind,
            policy_cid: policy.policy_ref.cid,
        };
        let head = match self.heads.get(&lineage).copied() {
            Some(existing) if existing.root == view_root => existing,
            Some(existing) => RevisionHead {
                revision: existing.revision.saturating_add(1),
                root: view_root,
                previous_root: Some(existing.root),
            },
            None => RevisionHead {
                revision: 1,
                root: view_root,
                previous_root: None,
            },
        };
        self.heads.insert(lineage, head);
        Ok(MetabolicEvidenceView {
            schema_major: METABOLIC_VIEW_MAJOR,
            schema_minor: METABOLIC_VIEW_MINOR,
            target,
            policy,
            frontier: frontier.clone(),
            revision: head.revision,
            previous_view_root: head.previous_root,
            view_root,
            evidence_root,
            frontier_root,
            cumulative_event_ids,
            recent_activity,
            limitations,
        })
    }
}

fn evidence_target<'a>(
    evidence: &'a ExerciseEvidence,
    target: &ObjectReference,
) -> Option<(RecentExerciseKind, &'a ObjectReference)> {
    match evidence {
        ExerciseEvidence::Use(event) => event
            .payload()
            .subjects
            .iter()
            .any(|subject| subject == target)
            .then_some((
                RecentExerciseKind::Use(event.payload().mode),
                &event.payload().use_policy,
            )),
        ExerciseEvidence::Derivation(event) => {
            let input = event
                .payload()
                .inputs
                .iter()
                .any(|candidate| &candidate.input == target);
            let output = &event.payload().output == target;
            let kind = match (input, output) {
                (true, true) => RecentExerciseKind::DerivationInputAndOutput,
                (true, false) => RecentExerciseKind::DerivationInput,
                (false, true) => RecentExerciseKind::DerivationOutput,
                (false, false) => return None,
            };
            Some((kind, &event.payload().derivation_policy))
        }
    }
}

fn digest_value(label: &[u8], value: &CanonicalValue) -> Result<[u8; 32], MetabolicViewError> {
    let bytes = encode_canonical(value, ResourceProfile::ObjectV1)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:derived-metabolic-view:1\0");
    hasher.update(label);
    hasher.update(&[0]);
    hasher.update(&bytes);
    Ok(*hasher.finalize().as_bytes())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExposureKind {
    QueryHit,
    Retrieval,
    Presented,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExposureObservation {
    pub observation_id: [u8; 32],
    pub target: ObjectReference,
    pub kind: ExposureKind,
    pub local_sequence: u64,
    pub private_context_commitment: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExposureRecordOutcome {
    Added,
    ExactReplay,
    ConflictingIdRejected,
    CapacityReached,
}

pub struct ExposureTelemetry {
    max_observations: usize,
    observations: BTreeMap<[u8; 32], ExposureObservation>,
}

impl ExposureTelemetry {
    pub fn new(max_observations: usize) -> Result<Self, MetabolicViewError> {
        if max_observations == 0 || max_observations > MAX_EXPOSURE_OBSERVATIONS {
            return Err(MetabolicViewError::InvalidCapacity);
        }
        Ok(Self {
            max_observations,
            observations: BTreeMap::new(),
        })
    }

    pub const fn is_local_private(&self) -> bool {
        true
    }

    pub const fn counts_as_use(&self) -> bool {
        false
    }

    pub fn record(&mut self, observation: ExposureObservation) -> ExposureRecordOutcome {
        if observation.observation_id == [0; 32]
            || observation.target.cid == [0; 32]
            || observation.private_context_commitment == [0; 32]
        {
            return ExposureRecordOutcome::ConflictingIdRejected;
        }
        match self.observations.get(&observation.observation_id) {
            Some(existing) if existing == &observation => ExposureRecordOutcome::ExactReplay,
            Some(_) => ExposureRecordOutcome::ConflictingIdRejected,
            None if self.observations.len() == self.max_observations => {
                ExposureRecordOutcome::CapacityReached
            }
            None => {
                self.observations
                    .insert(observation.observation_id, observation);
                ExposureRecordOutcome::Added
            }
        }
    }

    pub fn local_count_for(&self, target: &ObjectReference) -> usize {
        self.observations
            .values()
            .filter(|observation| &observation.target == target)
            .count()
    }
}

#[derive(Debug)]
pub enum MetabolicViewError {
    Canonical(super::canonical::CanonicalError),
    InvalidCapacity,
    InvalidPolicy,
    InvalidFrontier,
    InvalidTarget,
}

impl From<super::canonical::CanonicalError> for MetabolicViewError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object, DeviceId,
        DisclosureClass, FeedInception, KnowledgeEventEnvelope, KnownObjectKind,
        NamespaceCommitment, SignedFeedInception, UseEvidencePayload, ValidatedUseEvidenceEvent,
        USE_EVIDENCE_EVENT_TYPE, USE_EVIDENCE_KIND,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn author() -> (SigningKey, crate::foundation::ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[31; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"metabolic-view-test", [32; 32]).unwrap(),
            0,
            DeviceId::from_bytes([33; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        (
            key,
            decode_feed_inception(&signed.encode().unwrap()).unwrap(),
        )
    }

    fn assessed_use(
        target: ObjectReference,
        sequence: u64,
        nonce: u8,
        mode: UseMode,
        evidence_policy: ObjectReference,
    ) -> AssessedExerciseEvidence {
        let payload = UseEvidencePayload {
            subjects: vec![target],
            mode,
            actor_class: crate::foundation::ConceptCcid::from_bytes([34; 16]),
            task_context_commitment: [nonce; 32],
            causal_role: crate::foundation::ConceptCcid::from_bytes([35; 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: evidence_policy,
            observed_frontier: [36; 32],
        };
        let object = payload
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap();
        let (bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let object = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(USE_EVIDENCE_KIND, 1)],
            &[],
        )
        .unwrap();
        let (key, feed) = author();
        let mut event = KnowledgeEventEnvelope::new(
            USE_EVIDENCE_EVENT_TYPE,
            feed.feed_id,
            sequence,
            DisclosureClass::LocalOnly,
            [nonce.wrapping_add(100); 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let bytes = event.sign(&feed, &key).unwrap().encode().unwrap().0;
        let event = decode_knowledge_event(&bytes, &feed, &[USE_EVIDENCE_EVENT_TYPE]).unwrap();
        AssessedExerciseEvidence {
            evidence: ExerciseEvidence::Use(
                ValidatedUseEvidenceEvent::bind(&event, &object).unwrap(),
            ),
            authority: ExerciseAuthority::Authorized,
        }
    }

    fn policy(evidence_policy: ObjectReference, horizon: u64) -> MetabolicViewPolicy {
        MetabolicViewPolicy {
            policy_ref: reference(80),
            accepted_evidence_policies: vec![evidence_policy],
            recent_event_horizon: horizon,
        }
    }

    fn frontier(feed: FeedId, sequence: u64) -> MetabolicEvidenceFrontier {
        MetabolicEvidenceFrontier::new([81; 32], [(feed, sequence)]).unwrap()
    }

    #[test]
    fn query_hit_retrieval_and_exposure_stay_outside_use_evidence() {
        let target = reference(1);
        let evidence_policy = reference(2);
        let (_, feed) = author();
        let mut reducer = MetabolicEvidenceReducer::new(10).unwrap();
        let before = reducer
            .materialize(
                target.clone(),
                &policy(evidence_policy.clone(), 10),
                &frontier(feed.feed_id, 0),
            )
            .unwrap();
        let mut telemetry = ExposureTelemetry::new(10).unwrap();
        for (index, kind) in [
            ExposureKind::QueryHit,
            ExposureKind::Retrieval,
            ExposureKind::Presented,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                telemetry.record(ExposureObservation {
                    observation_id: [index as u8 + 1; 32],
                    target: target.clone(),
                    kind,
                    local_sequence: index as u64,
                    private_context_commitment: [90; 32],
                }),
                ExposureRecordOutcome::Added
            );
        }
        let after = reducer
            .materialize(
                target.clone(),
                &policy(evidence_policy, 10),
                &frontier(feed.feed_id, 0),
            )
            .unwrap();
        assert!(telemetry.is_local_private());
        assert!(!telemetry.counts_as_use());
        assert_eq!(telemetry.local_count_for(&target), 3);
        assert!(after.cumulative_event_ids.is_empty());
        assert_eq!(before.view_root, after.view_root);
        assert_eq!(before.revision, after.revision);
    }

    #[test]
    fn one_event_seen_through_many_bridges_is_counted_once() {
        let target = reference(3);
        let evidence_policy = reference(4);
        let evidence = assessed_use(
            target.clone(),
            0,
            5,
            UseMode::Application,
            evidence_policy.clone(),
        );
        let feed = evidence.evidence.author_feed();
        let mut reducer = MetabolicEvidenceReducer::new(10).unwrap();
        assert_eq!(
            reducer.record(evidence.clone()),
            MetabolicRecordOutcome::Added
        );
        for _bridge in 0..20 {
            assert_eq!(
                reducer.record(evidence.clone()),
                MetabolicRecordOutcome::ExactReplay
            );
        }
        let view = reducer
            .materialize(target, &policy(evidence_policy, 10), &frontier(feed, 0))
            .unwrap();
        assert_eq!(view.cumulative_event_ids.len(), 1);
        assert_eq!(view.recent_activity.len(), 1);
    }

    #[test]
    fn late_reunion_evidence_creates_a_linked_revision() {
        let target = reference(6);
        let evidence_policy = reference(7);
        let later = assessed_use(
            target.clone(),
            1,
            8,
            UseMode::Transformation,
            evidence_policy.clone(),
        );
        let feed = later.evidence.author_feed();
        let mut reducer = MetabolicEvidenceReducer::new(10).unwrap();
        reducer.record(later);
        let first = reducer
            .materialize(
                target.clone(),
                &policy(evidence_policy.clone(), 10),
                &frontier(feed, 1),
            )
            .unwrap();
        let earlier = assessed_use(
            target.clone(),
            0,
            9,
            UseMode::Discovery,
            evidence_policy.clone(),
        );
        reducer.record(earlier);
        let reunited = reducer
            .materialize(target, &policy(evidence_policy, 10), &frontier(feed, 1))
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(reunited.revision, 2);
        assert_eq!(reunited.previous_view_root, Some(first.view_root));
        assert_eq!(reunited.cumulative_event_ids.len(), 2);
    }

    #[test]
    fn recent_activity_decays_by_feed_events_but_cumulative_root_does_not() {
        let target = reference(10);
        let evidence_policy = reference(11);
        let evidence = assessed_use(
            target.clone(),
            0,
            12,
            UseMode::Application,
            evidence_policy.clone(),
        );
        let feed = evidence.evidence.author_feed();
        let mut reducer = MetabolicEvidenceReducer::new(10).unwrap();
        reducer.record(evidence);
        let fresh = reducer
            .materialize(
                target.clone(),
                &policy(evidence_policy.clone(), 10),
                &frontier(feed, 0),
            )
            .unwrap();
        let fading = reducer
            .materialize(
                target.clone(),
                &policy(evidence_policy.clone(), 10),
                &frontier(feed, 4),
            )
            .unwrap();
        let old = reducer
            .materialize(target, &policy(evidence_policy, 10), &frontier(feed, 10))
            .unwrap();
        assert_eq!(
            fresh.recent_activity[0].relative_weight_millionths,
            1_000_000
        );
        assert_eq!(
            fading.recent_activity[0].relative_weight_millionths,
            600_000
        );
        assert!(old.recent_activity.is_empty());
        assert_eq!(fresh.evidence_root, fading.evidence_root);
        assert_eq!(fresh.evidence_root, old.evidence_root);
        assert_eq!(old.cumulative_event_ids.len(), 1);
    }

    #[test]
    fn geography_node_tier_and_arrival_order_are_not_projection_inputs() {
        let target = reference(13);
        let evidence_policy = reference(14);
        let first = assessed_use(
            target.clone(),
            0,
            15,
            UseMode::Transfer,
            evidence_policy.clone(),
        );
        let second = assessed_use(
            target.clone(),
            1,
            16,
            UseMode::Epistemic,
            evidence_policy.clone(),
        );
        let feed = first.evidence.author_feed();
        let mut region_a_tier_0 = MetabolicEvidenceReducer::new(10).unwrap();
        let mut region_b_tier_6 = MetabolicEvidenceReducer::new(10).unwrap();
        for evidence in [first.clone(), second.clone()] {
            region_a_tier_0.record(evidence);
        }
        for evidence in [second, first] {
            region_b_tier_6.record(evidence);
        }
        let left = region_a_tier_0
            .materialize(
                target.clone(),
                &policy(evidence_policy.clone(), 10),
                &frontier(feed, 1),
            )
            .unwrap();
        let right = region_b_tier_6
            .materialize(target, &policy(evidence_policy, 10), &frontier(feed, 1))
            .unwrap();
        assert_eq!(left.view_root, right.view_root);
        assert_eq!(left.cumulative_event_ids, right.cumulative_event_ids);
    }

    #[test]
    fn opposing_knowledge_can_accumulate_use_without_truth_or_benefit_claim() {
        let target = reference(17);
        let evidence_policy = reference(18);
        let evidence = assessed_use(
            target.clone(),
            0,
            19,
            UseMode::ComparedOrOpposed,
            evidence_policy.clone(),
        );
        let feed = evidence.evidence.author_feed();
        let mut reducer = MetabolicEvidenceReducer::new(10).unwrap();
        reducer.record(evidence);
        let view = reducer
            .materialize(target, &policy(evidence_policy, 10), &frontier(feed, 0))
            .unwrap();
        assert_eq!(view.cumulative_event_ids.len(), 1);
        assert_eq!(
            view.recent_activity[0].kind,
            RecentExerciseKind::Use(UseMode::ComparedOrOpposed)
        );
        assert!(!view.establishes_benefit());
        assert!(!view.is_reward_instruction());
        assert!(!view.is_globally_complete());
    }
}
