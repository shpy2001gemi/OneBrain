//! Signed outcome observations and benefit/attribution evidence.
//!
//! These artifacts separate observed change from causal attribution and from
//! reward authorization. Conflicting observations remain explicit branches.

use std::collections::{BTreeMap, BTreeSet};

use super::authority::FeedAuthorityDecision;
use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalValue, ResourceProfile,
};
use super::content_id::{EventCid, ObjectCid};
use super::event::{EventType, ValidatedKnowledgeEvent};
use super::feed::ValidatedFeedInception;
use super::identity::{ActorId, FeedId};
use super::key_state::KeyStateReducer;
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};
use super::schema_registry::{
    EVENT_TYPE_BENEFIT_EVIDENCE, EVENT_TYPE_OUTCOME_OBSERVATION, OBJECT_KIND_BENEFIT_EVIDENCE,
    OBJECT_KIND_OUTCOME_OBSERVATION,
};
use super::semantic::ConceptCcid;
use super::use_evidence::ExerciseEvidence;

pub const OUTCOME_EVIDENCE_MAJOR: u64 = 1;
pub const OUTCOME_EVIDENCE_MINOR: u64 = 0;
pub const OUTCOME_OBSERVATION_KIND: ObjectKind = ObjectKind(OBJECT_KIND_OUTCOME_OBSERVATION);
pub const BENEFIT_EVIDENCE_KIND: ObjectKind = ObjectKind(OBJECT_KIND_BENEFIT_EVIDENCE);
pub const OUTCOME_OBSERVATION_EVENT_TYPE: EventType = EventType(EVENT_TYPE_OUTCOME_OBSERVATION);
pub const BENEFIT_EVIDENCE_EVENT_TYPE: EventType = EventType(EVENT_TYPE_BENEFIT_EVIDENCE);
pub const MAX_OUTCOME_REFERENCES: usize = 4_096;
pub const MAX_OUTCOME_LIMITATIONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum OutcomeValence {
    Beneficial = 0,
    Harmful = 1,
    Mixed = 2,
    NoObservedChange = 3,
    Unknown = 4,
}

impl OutcomeValence {
    const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Beneficial),
            1 => Some(Self::Harmful),
            2 => Some(Self::Mixed),
            3 => Some(Self::NoObservedChange),
            4 => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum EvidenceLimitation {
    MissingBaseline = 0,
    MissingCounterfactual = 1,
    AttributionUnknown = 2,
    UnwitnessedObservation = 3,
    SelfReported = 4,
    ConfoundingFactors = 5,
    PartialObservation = 6,
    PrivateContextUnavailable = 7,
}

impl EvidenceLimitation {
    const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::MissingBaseline),
            1 => Some(Self::MissingCounterfactual),
            2 => Some(Self::AttributionUnknown),
            3 => Some(Self::UnwitnessedObservation),
            4 => Some(Self::SelfReported),
            5 => Some(Self::ConfoundingFactors),
            6 => Some(Self::PartialObservation),
            7 => Some(Self::PrivateContextUnavailable),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffectedPrincipal {
    Actor(ActorId),
    Commitment([u8; 32]),
}

impl AffectedPrincipal {
    fn to_value(&self) -> CanonicalValue {
        match self {
            Self::Actor(actor) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, CanonicalValue::Bytes(actor.as_bytes().to_vec())),
            ]),
            Self::Commitment(commitment) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, CanonicalValue::Bytes(commitment.to_vec())),
            ]),
        }
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, OutcomeEvidenceError> {
        let map = value_map(value, "affected.principal")?;
        let bytes = value_bytes32(map, 1, "affected.principal.value")?;
        match value_unsigned(map, 0, "affected.principal.kind")? {
            0 => Ok(Self::Actor(ActorId::from_bytes(bytes))),
            1 if bytes != [0; 32] => Ok(Self::Commitment(bytes)),
            _ => Err(OutcomeEvidenceError::InvalidField(
                "affected.principal.kind",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffectedScope {
    pub principal: Option<AffectedPrincipal>,
    pub assembly: Option<ObjectReference>,
}

impl AffectedScope {
    fn validate(&self) -> Result<(), OutcomeEvidenceError> {
        if self.principal.is_none() && self.assembly.is_none() {
            return Err(OutcomeEvidenceError::InvalidField("affected.empty"));
        }
        if self
            .assembly
            .as_ref()
            .is_some_and(|value| value.cid == [0; 32])
        {
            return Err(OutcomeEvidenceError::InvalidField("affected.assembly"));
        }
        Ok(())
    }

    fn to_value(&self) -> Result<CanonicalValue, OutcomeEvidenceError> {
        self.validate()?;
        let mut fields = Vec::new();
        if let Some(principal) = &self.principal {
            fields.push((0, principal.to_value()));
        }
        if let Some(assembly) = &self.assembly {
            fields.push((1, assembly.to_value()));
        }
        Ok(CanonicalValue::Map(fields))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, OutcomeEvidenceError> {
        let map = value_map(value, "affected")?;
        let scope = Self {
            principal: value_optional(map, 0)
                .map(AffectedPrincipal::from_value)
                .transpose()?,
            assembly: value_optional(map, 1)
                .map(ObjectReference::from_value)
                .transpose()?,
        };
        scope.validate()?;
        Ok(scope)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeObservationPayload {
    pub task_context_commitment: [u8; 32],
    pub outcome_class: ConceptCcid,
    pub valence: OutcomeValence,
    pub affected: AffectedScope,
    pub measurement_evidence: Vec<ObjectReference>,
    pub baseline: Option<ObjectReference>,
    pub limitations: Vec<EvidenceLimitation>,
    pub observation_policy: ObjectReference,
    pub observed_frontier: [u8; 32],
}

impl OutcomeObservationPayload {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, OutcomeEvidenceError> {
        self.validate()?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(OUTCOME_EVIDENCE_MAJOR)),
            (1, CanonicalValue::Unsigned(OUTCOME_EVIDENCE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.task_context_commitment.to_vec()),
            ),
            (
                3,
                CanonicalValue::Bytes(self.outcome_class.as_bytes().to_vec()),
            ),
            (4, CanonicalValue::Unsigned(self.valence as u64)),
            (5, self.affected.to_value()?),
            (6, reference_set(&self.measurement_evidence)?),
            (8, limitation_set(&self.limitations)?),
            (9, self.observation_policy.to_value()),
            (10, CanonicalValue::Bytes(self.observed_frontier.to_vec())),
        ];
        if let Some(baseline) = &self.baseline {
            fields.push((7, baseline.to_value()));
        }
        fields.sort_by_key(|(key, _)| *key);
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, OutcomeEvidenceError> {
        let mut object = KnowledgeObjectEnvelope::new(
            OUTCOME_OBSERVATION_KIND,
            SchemaVersion::new(OUTCOME_EVIDENCE_MAJOR, OUTCOME_EVIDENCE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = self.measurement_evidence.clone();
        object.references.extend(self.baseline.iter().cloned());
        object.references.push(self.observation_policy.clone());
        object
            .references
            .extend(self.affected.assembly.iter().cloned());
        Ok(object)
    }

    fn validate(&self) -> Result<(), OutcomeEvidenceError> {
        if self.task_context_commitment == [0; 32]
            || self.observed_frontier == [0; 32]
            || self.observation_policy.cid == [0; 32]
        {
            return Err(OutcomeEvidenceError::InvalidField(
                "observation.commitment_policy_frontier",
            ));
        }
        self.affected.validate()?;
        validate_reference_limit(&self.measurement_evidence)?;
        validate_limitations(&self.limitations)?;
        let limitations = self.limitations.iter().copied().collect::<BTreeSet<_>>();
        if self.measurement_evidence.is_empty()
            && !limitations.contains(&EvidenceLimitation::UnwitnessedObservation)
        {
            return Err(OutcomeEvidenceError::MissingRequiredLimitation(
                EvidenceLimitation::UnwitnessedObservation,
            ));
        }
        if self.baseline.is_none() && !limitations.contains(&EvidenceLimitation::MissingBaseline) {
            return Err(OutcomeEvidenceError::MissingRequiredLimitation(
                EvidenceLimitation::MissingBaseline,
            ));
        }
        Ok(())
    }

    fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, OutcomeEvidenceError> {
        let envelope = known_envelope(object, OUTCOME_OBSERVATION_KIND)?;
        let map = value_map(&envelope.payload, "outcome.payload")?;
        validate_version(map)?;
        let payload = Self {
            task_context_commitment: value_bytes32(map, 2, "outcome.task")?,
            outcome_class: ConceptCcid::from_bytes(value_bytes16(map, 3, "outcome.class")?),
            valence: OutcomeValence::from_code(value_unsigned(map, 4, "outcome.valence")?)
                .ok_or(OutcomeEvidenceError::InvalidField("outcome.valence"))?,
            affected: AffectedScope::from_value(value_required(map, 5, "outcome.affected")?)?,
            measurement_evidence: parse_reference_set(map, 6, "outcome.measurement")?,
            baseline: value_optional(map, 7)
                .map(ObjectReference::from_value)
                .transpose()?,
            limitations: parse_limitations(map, 8)?,
            observation_policy: ObjectReference::from_value(value_required(
                map,
                9,
                "outcome.policy",
            )?)?,
            observed_frontier: value_bytes32(map, 10, "outcome.frontier")?,
        };
        payload.validate()?;
        if payload.canonical_payload()? != envelope.payload {
            return Err(OutcomeEvidenceError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum AttributionStatus {
    Supported = 0,
    Opposed = 1,
    Contested = 2,
    Unknown = 3,
}

impl AttributionStatus {
    const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Supported),
            1 => Some(Self::Opposed),
            2 => Some(Self::Contested),
            3 => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenefitEvidencePayload {
    pub task_context_commitment: [u8; 32],
    pub outcome_observations: Vec<ObjectReference>,
    pub use_events: Vec<EventCid>,
    pub knowledge_subjects: Vec<ObjectReference>,
    pub assessed_valence: OutcomeValence,
    pub attribution: AttributionStatus,
    pub causal_evidence: Vec<ObjectReference>,
    pub counterfactual_evidence: Vec<ObjectReference>,
    pub limitations: Vec<EvidenceLimitation>,
    pub benefit_policy: ObjectReference,
    pub assessed_frontier: [u8; 32],
}

impl BenefitEvidencePayload {
    pub const fn requires_outcome_observation() -> bool {
        true
    }

    pub const fn is_reward_instruction(&self) -> bool {
        false
    }

    pub const fn establishes_truth(&self) -> bool {
        false
    }

    pub const fn establishes_attributed_benefit(&self) -> bool {
        matches!(self.attribution, AttributionStatus::Supported)
            && matches!(self.assessed_valence, OutcomeValence::Beneficial)
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, OutcomeEvidenceError> {
        self.validate()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(OUTCOME_EVIDENCE_MAJOR)),
            (1, CanonicalValue::Unsigned(OUTCOME_EVIDENCE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.task_context_commitment.to_vec()),
            ),
            (3, reference_set(&self.outcome_observations)?),
            (4, event_set(&self.use_events)?),
            (5, reference_set(&self.knowledge_subjects)?),
            (6, CanonicalValue::Unsigned(self.assessed_valence as u64)),
            (7, CanonicalValue::Unsigned(self.attribution as u64)),
            (8, reference_set(&self.causal_evidence)?),
            (9, reference_set(&self.counterfactual_evidence)?),
            (10, limitation_set(&self.limitations)?),
            (11, self.benefit_policy.to_value()),
            (12, CanonicalValue::Bytes(self.assessed_frontier.to_vec())),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, OutcomeEvidenceError> {
        let mut object = KnowledgeObjectEnvelope::new(
            BENEFIT_EVIDENCE_KIND,
            SchemaVersion::new(OUTCOME_EVIDENCE_MAJOR, OUTCOME_EVIDENCE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = self.outcome_observations.clone();
        object.references.extend(self.knowledge_subjects.clone());
        object.references.extend(self.causal_evidence.clone());
        object
            .references
            .extend(self.counterfactual_evidence.clone());
        object.references.push(self.benefit_policy.clone());
        Ok(object)
    }

    pub fn resolve_outcomes(
        &self,
        outcomes: &[ValidatedOutcomeObservationEvent],
    ) -> Result<(), OutcomeEvidenceError> {
        let available = outcomes
            .iter()
            .map(|outcome| ObjectReference::new(0, outcome.payload_object_cid().into_bytes()))
            .collect::<Vec<_>>();
        if self
            .outcome_observations
            .iter()
            .all(|required| available.contains(required))
        {
            Ok(())
        } else {
            Err(OutcomeEvidenceError::UnresolvedOutcomeReference)
        }
    }

    fn validate(&self) -> Result<(), OutcomeEvidenceError> {
        if self.task_context_commitment == [0; 32]
            || self.assessed_frontier == [0; 32]
            || self.benefit_policy.cid == [0; 32]
        {
            return Err(OutcomeEvidenceError::InvalidField(
                "benefit.commitment_policy_frontier",
            ));
        }
        if self.outcome_observations.is_empty() {
            return Err(OutcomeEvidenceError::OutcomeObservationRequired);
        }
        for references in [
            &self.outcome_observations,
            &self.knowledge_subjects,
            &self.causal_evidence,
            &self.counterfactual_evidence,
        ] {
            validate_reference_limit(references)?;
        }
        if self.use_events.len() > MAX_OUTCOME_REFERENCES {
            return Err(OutcomeEvidenceError::Limit);
        }
        validate_limitations(&self.limitations)?;
        let limitations = self.limitations.iter().copied().collect::<BTreeSet<_>>();
        if self.counterfactual_evidence.is_empty()
            && !limitations.contains(&EvidenceLimitation::MissingCounterfactual)
        {
            return Err(OutcomeEvidenceError::MissingRequiredLimitation(
                EvidenceLimitation::MissingCounterfactual,
            ));
        }
        match self.attribution {
            AttributionStatus::Unknown => {
                if !limitations.contains(&EvidenceLimitation::AttributionUnknown) {
                    return Err(OutcomeEvidenceError::MissingRequiredLimitation(
                        EvidenceLimitation::AttributionUnknown,
                    ));
                }
            }
            AttributionStatus::Supported
            | AttributionStatus::Opposed
            | AttributionStatus::Contested => {
                if self.knowledge_subjects.is_empty() || self.causal_evidence.is_empty() {
                    return Err(OutcomeEvidenceError::AttributionEvidenceRequired);
                }
            }
        }
        Ok(())
    }

    fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, OutcomeEvidenceError> {
        let envelope = known_envelope(object, BENEFIT_EVIDENCE_KIND)?;
        let map = value_map(&envelope.payload, "benefit.payload")?;
        validate_version(map)?;
        let payload = Self {
            task_context_commitment: value_bytes32(map, 2, "benefit.task")?,
            outcome_observations: parse_reference_set(map, 3, "benefit.outcomes")?,
            use_events: parse_event_set(map, 4, "benefit.use_events")?,
            knowledge_subjects: parse_reference_set(map, 5, "benefit.subjects")?,
            assessed_valence: OutcomeValence::from_code(value_unsigned(map, 6, "benefit.valence")?)
                .ok_or(OutcomeEvidenceError::InvalidField("benefit.valence"))?,
            attribution: AttributionStatus::from_code(value_unsigned(
                map,
                7,
                "benefit.attribution",
            )?)
            .ok_or(OutcomeEvidenceError::InvalidField("benefit.attribution"))?,
            causal_evidence: parse_reference_set(map, 8, "benefit.causal")?,
            counterfactual_evidence: parse_reference_set(map, 9, "benefit.counterfactual")?,
            limitations: parse_limitations(map, 10)?,
            benefit_policy: ObjectReference::from_value(value_required(
                map,
                11,
                "benefit.policy",
            )?)?,
            assessed_frontier: value_bytes32(map, 12, "benefit.frontier")?,
        };
        payload.validate()?;
        if payload.canonical_payload()? != envelope.payload {
            return Err(OutcomeEvidenceError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedOutcomeObservationEvent {
    event_cid: EventCid,
    payload_object: ObjectCid,
    author_feed: FeedId,
    author_sequence: u64,
    payload: OutcomeObservationPayload,
    case_id: OutcomeCaseId,
}

impl ValidatedOutcomeObservationEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, OutcomeEvidenceError> {
        bind_event(event, payload_object, OUTCOME_OBSERVATION_EVENT_TYPE)?;
        let payload = OutcomeObservationPayload::from_object(payload_object)?;
        let case_id = OutcomeCaseId::derive(&payload)?;
        Ok(Self {
            event_cid: event.cid(),
            payload_object: payload_object.cid(),
            author_feed: event.signed.event.author_feed,
            author_sequence: event.signed.event.author_sequence,
            payload,
            case_id,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn payload_object_cid(&self) -> ObjectCid {
        self.payload_object
    }

    pub const fn author_feed(&self) -> FeedId {
        self.author_feed
    }

    pub const fn author_sequence(&self) -> u64 {
        self.author_sequence
    }

    pub const fn payload(&self) -> &OutcomeObservationPayload {
        &self.payload
    }

    pub const fn case_id(&self) -> OutcomeCaseId {
        self.case_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedBenefitEvidenceEvent {
    event_cid: EventCid,
    payload_object: ObjectCid,
    author_feed: FeedId,
    author_sequence: u64,
    payload: BenefitEvidencePayload,
}

impl ValidatedBenefitEvidenceEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, OutcomeEvidenceError> {
        bind_event(event, payload_object, BENEFIT_EVIDENCE_EVENT_TYPE)?;
        Ok(Self {
            event_cid: event.cid(),
            payload_object: payload_object.cid(),
            author_feed: event.signed.event.author_feed,
            author_sequence: event.signed.event.author_sequence,
            payload: BenefitEvidencePayload::from_object(payload_object)?,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn payload_object_cid(&self) -> ObjectCid {
        self.payload_object
    }

    pub const fn author_feed(&self) -> FeedId {
        self.author_feed
    }

    pub const fn author_sequence(&self) -> u64 {
        self.author_sequence
    }

    pub const fn payload(&self) -> &BenefitEvidencePayload {
        &self.payload
    }
}

fn bind_event(
    event: &ValidatedKnowledgeEvent,
    payload: &ValidatedKnowledgeObject,
    expected_type: EventType,
) -> Result<(), OutcomeEvidenceError> {
    if event.signed.event.event_type != expected_type {
        return Err(OutcomeEvidenceError::WrongEventType);
    }
    if event.signed.event.disclosure != payload.disclosure() {
        return Err(OutcomeEvidenceError::DisclosureMismatch);
    }
    let expected = ObjectReference::new(0, payload.cid().into_bytes());
    if event.signed.event.payload_refs != [expected] {
        return Err(OutcomeEvidenceError::PayloadReferenceMismatch);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OutcomeCaseId([u8; 32]);

impl OutcomeCaseId {
    fn derive(payload: &OutcomeObservationPayload) -> Result<Self, OutcomeEvidenceError> {
        let value = CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Bytes(payload.task_context_commitment.to_vec()),
            ),
            (
                1,
                CanonicalValue::Bytes(payload.outcome_class.as_bytes().to_vec()),
            ),
            (2, payload.affected.to_value()?),
            (3, payload.observation_policy.to_value()),
        ]);
        let bytes = encode_canonical(&value, ResourceProfile::ObjectV1)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:outcome-case:1\0");
        hasher.update(&bytes);
        Ok(Self(*hasher.finalize().as_bytes()))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeAuthority {
    Authorized,
    Unauthorized,
    Unresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssessedOutcomeObservation {
    pub observation: ValidatedOutcomeObservationEvent,
    pub authority: OutcomeAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeRecordResult {
    Added,
    ExactReplay,
    Reassessed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeBranchRecord {
    pub event_cid: EventCid,
    pub valence: OutcomeValence,
    pub authority: OutcomeAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeBranchView {
    pub case_id: OutcomeCaseId,
    pub records: Vec<OutcomeBranchRecord>,
    pub authorized_conflict: bool,
}

impl OutcomeBranchView {
    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct OutcomeEvidenceReducer {
    records: BTreeMap<OutcomeCaseId, BTreeMap<[u8; 32], AssessedOutcomeObservation>>,
}

impl OutcomeEvidenceReducer {
    pub fn record(&mut self, record: AssessedOutcomeObservation) -> OutcomeRecordResult {
        let case = record.observation.case_id();
        let event = record.observation.event_cid().into_bytes();
        let branch = self.records.entry(case).or_default();
        match branch.get(&event) {
            Some(existing) if existing == &record => OutcomeRecordResult::ExactReplay,
            Some(_) => {
                branch.insert(event, record);
                OutcomeRecordResult::Reassessed
            }
            None => {
                branch.insert(event, record);
                OutcomeRecordResult::Added
            }
        }
    }

    pub fn branch(&self, case_id: OutcomeCaseId) -> OutcomeBranchView {
        let records = self
            .records
            .get(&case_id)
            .into_iter()
            .flat_map(|records| records.values())
            .map(|record| OutcomeBranchRecord {
                event_cid: record.observation.event_cid(),
                valence: record.observation.payload().valence,
                authority: record.authority,
            })
            .collect::<Vec<_>>();
        let authorized_valences = records
            .iter()
            .filter(|record| record.authority == OutcomeAuthority::Authorized)
            .map(|record| record.valence as u64)
            .collect::<BTreeSet<_>>();
        OutcomeBranchView {
            case_id,
            authorized_conflict: authorized_valences.len() > 1,
            records,
        }
    }
}

/// Valence-neutral closure state over an `OutcomeCaseId`'s multi-principal
/// causal and attribution chain (`P4` §2–§3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum BenefitClosureState {
    SupportedUncontested = 0,
    ContestedOrMixed = 1,
    DeferredIncomplete = 2,
    ObservedNonBeneficial = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssessedOutcomeBranch {
    pub event_cid: EventCid,
    pub payload_object_cid: ObjectCid,
    pub author_feed: FeedId,
    pub author_decision: FeedAuthorityDecision,
    pub valence: OutcomeValence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssessedBenefitBranch {
    pub event_cid: EventCid,
    pub payload_object_cid: ObjectCid,
    pub author_feed: FeedId,
    pub author_decision: FeedAuthorityDecision,
    pub assessed_valence: OutcomeValence,
    pub attribution: AttributionStatus,
    pub outcome_authors_authorized: bool,
    pub exercise_authors_authorized: bool,
    pub ku_authors_authorized: bool,
    pub causal_chain_complete: bool,
}

/// Multi-principal, valence-neutral `BenefitClosureView` over one `OutcomeCaseId`.
///
/// Explicitly carries non-reward and non-truth guard invariants: never collapses
/// concurrent observations or attributions into a single winner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenefitClosureView {
    pub case_id: OutcomeCaseId,
    pub state: BenefitClosureState,
    pub outcome_branches: Vec<AssessedOutcomeBranch>,
    pub benefit_branches: Vec<AssessedBenefitBranch>,
}

impl BenefitClosureView {
    pub const fn is_reward_instruction(&self) -> bool {
        false
    }

    pub const fn establishes_truth(&self) -> bool {
        false
    }

    pub const fn collapses_to_single_winner(&self) -> bool {
        false
    }
}

/// Deterministic multi-principal reducer evaluating `FeedAuthorityDecision`
/// independently across KU authors, Use/Derivation authors, Outcome authors,
/// and Benefit authors (`P4` §3).
#[derive(Default)]
pub struct BenefitClosureReducer {
    feeds: BTreeMap<FeedId, ValidatedFeedInception>,
    ku_authors: BTreeMap<(u64, [u8; 32]), FeedId>,
    exercises: BTreeMap<[u8; 32], ExerciseEvidence>,
    outcomes_by_case: BTreeMap<OutcomeCaseId, BTreeMap<[u8; 32], ValidatedOutcomeObservationEvent>>,
    outcomes_by_object: BTreeMap<(u64, [u8; 32]), (OutcomeCaseId, [u8; 32])>,
    benefits: BTreeMap<[u8; 32], ValidatedBenefitEvidenceEvent>,
}

impl BenefitClosureReducer {
    pub fn register_feed(&mut self, inception: ValidatedFeedInception) {
        self.feeds.insert(inception.feed_id, inception);
    }

    pub fn register_ku_author(&mut self, ku_ref: ObjectReference, author_feed: FeedId) {
        self.ku_authors
            .insert((ku_ref.reference_kind, ku_ref.cid), author_feed);
    }

    pub fn record_exercise(&mut self, exercise: ExerciseEvidence) -> OutcomeRecordResult {
        let key = exercise.event_cid().into_bytes();
        match self.exercises.get(&key) {
            Some(existing) if existing == &exercise => OutcomeRecordResult::ExactReplay,
            Some(_) => {
                self.exercises.insert(key, exercise);
                OutcomeRecordResult::Reassessed
            }
            None => {
                self.exercises.insert(key, exercise);
                OutcomeRecordResult::Added
            }
        }
    }

    pub fn record_outcome(
        &mut self,
        outcome: ValidatedOutcomeObservationEvent,
    ) -> OutcomeRecordResult {
        let case_id = outcome.case_id();
        let event_key = outcome.event_cid().into_bytes();
        let obj_key = (0u64, outcome.payload_object_cid().into_bytes());
        self.outcomes_by_object
            .insert(obj_key, (case_id, event_key));
        let branch = self.outcomes_by_case.entry(case_id).or_default();
        match branch.get(&event_key) {
            Some(existing) if existing == &outcome => OutcomeRecordResult::ExactReplay,
            Some(_) => {
                branch.insert(event_key, outcome);
                OutcomeRecordResult::Reassessed
            }
            None => {
                branch.insert(event_key, outcome);
                OutcomeRecordResult::Added
            }
        }
    }

    pub fn record_benefit(
        &mut self,
        benefit: ValidatedBenefitEvidenceEvent,
    ) -> OutcomeRecordResult {
        let key = benefit.event_cid().into_bytes();
        match self.benefits.get(&key) {
            Some(existing) if existing == &benefit => OutcomeRecordResult::ExactReplay,
            Some(_) => {
                self.benefits.insert(key, benefit);
                OutcomeRecordResult::Reassessed
            }
            None => {
                self.benefits.insert(key, benefit);
                OutcomeRecordResult::Added
            }
        }
    }

    fn evaluate_feed_authority(
        &self,
        feed_id: FeedId,
        key_state: &KeyStateReducer,
    ) -> FeedAuthorityDecision {
        match self.feeds.get(&feed_id) {
            Some(feed) => key_state.evaluate(feed),
            None => FeedAuthorityDecision::StaleOrUnresolved {
                reason: super::authority::UnresolvedAuthorityReason::MissingAcceptedGrant,
                frontier: key_state.frontier(),
            },
        }
    }

    /// Evaluates independent per-author authority across KU, Use/Derivation,
    /// Outcome, and Benefit principals for `case_id`, retaining all concurrent
    /// branches sorted by `EventCid` without single-winner collapse.
    pub fn evaluate_case(
        &self,
        case_id: OutcomeCaseId,
        key_state: &KeyStateReducer,
    ) -> BenefitClosureView {
        let mut outcome_branches = Vec::new();
        let mut case_outcome_objects: BTreeMap<(u64, [u8; 32]), (FeedAuthorityDecision, [u8; 32])> =
            BTreeMap::new();

        if let Some(case_outcomes) = self.outcomes_by_case.get(&case_id) {
            for outcome in case_outcomes.values() {
                let decision = self.evaluate_feed_authority(outcome.author_feed(), key_state);
                case_outcome_objects.insert(
                    (0u64, outcome.payload_object_cid().into_bytes()),
                    (decision, outcome.payload().task_context_commitment),
                );
                outcome_branches.push(AssessedOutcomeBranch {
                    event_cid: outcome.event_cid(),
                    payload_object_cid: outcome.payload_object_cid(),
                    author_feed: outcome.author_feed(),
                    author_decision: decision,
                    valence: outcome.payload().valence,
                });
            }
        }

        let mut benefit_branches = Vec::new();
        for benefit in self.benefits.values() {
            let payload = benefit.payload();
            let touches_case = payload.outcome_observations.iter().any(|oref| {
                self.outcomes_by_object
                    .get(&(oref.reference_kind, oref.cid))
                    .is_some_and(|(cid_case, _)| *cid_case == case_id)
            });
            if !touches_case {
                continue;
            }

            let author_decision = self.evaluate_feed_authority(benefit.author_feed(), key_state);

            let mut all_outcomes_present_and_matching_task =
                !payload.outcome_observations.is_empty();
            let mut outcome_authors_authorized = true;
            for oref in &payload.outcome_observations {
                match self
                    .outcomes_by_object
                    .get(&(oref.reference_kind, oref.cid))
                {
                    Some((o_case, o_event_key)) => {
                        let Some(obs) = self
                            .outcomes_by_case
                            .get(o_case)
                            .and_then(|m| m.get(o_event_key))
                        else {
                            all_outcomes_present_and_matching_task = false;
                            outcome_authors_authorized = false;
                            continue;
                        };
                        if obs.payload().task_context_commitment != payload.task_context_commitment
                        {
                            all_outcomes_present_and_matching_task = false;
                        }
                        if !matches!(
                            self.evaluate_feed_authority(obs.author_feed(), key_state),
                            FeedAuthorityDecision::AuthorizedRelative { .. }
                        ) {
                            outcome_authors_authorized = false;
                        }
                    }
                    None => {
                        all_outcomes_present_and_matching_task = false;
                        outcome_authors_authorized = false;
                    }
                }
            }

            let mut all_exercises_present_and_matching_task = !payload.use_events.is_empty();
            let mut exercise_authors_authorized = !payload.use_events.is_empty();
            for ucid in &payload.use_events {
                match self.exercises.get(ucid.as_bytes()) {
                    Some(ex) => {
                        let ex_task = match ex {
                            ExerciseEvidence::Use(u) => u.payload().task_context_commitment,
                            ExerciseEvidence::Derivation(d) => d.payload().task_context_commitment,
                        };
                        if ex_task != payload.task_context_commitment {
                            all_exercises_present_and_matching_task = false;
                        }
                        if !matches!(
                            self.evaluate_feed_authority(ex.author_feed(), key_state),
                            FeedAuthorityDecision::AuthorizedRelative { .. }
                        ) {
                            exercise_authors_authorized = false;
                        }
                    }
                    None => {
                        all_exercises_present_and_matching_task = false;
                        exercise_authors_authorized = false;
                    }
                }
            }

            let mut all_kus_present = !payload.knowledge_subjects.is_empty();
            let mut ku_authors_authorized = !payload.knowledge_subjects.is_empty();
            for kref in &payload.knowledge_subjects {
                match self.ku_authors.get(&(kref.reference_kind, kref.cid)) {
                    Some(ku_feed) => {
                        if !matches!(
                            self.evaluate_feed_authority(*ku_feed, key_state),
                            FeedAuthorityDecision::AuthorizedRelative { .. }
                        ) {
                            ku_authors_authorized = false;
                        }
                    }
                    None => {
                        all_kus_present = false;
                        ku_authors_authorized = false;
                    }
                }
            }

            let causal_chain_complete = matches!(
                author_decision,
                FeedAuthorityDecision::AuthorizedRelative { .. }
            ) && all_outcomes_present_and_matching_task
                && outcome_authors_authorized
                && all_exercises_present_and_matching_task
                && exercise_authors_authorized
                && all_kus_present
                && ku_authors_authorized;

            benefit_branches.push(AssessedBenefitBranch {
                event_cid: benefit.event_cid(),
                payload_object_cid: benefit.payload_object_cid(),
                author_feed: benefit.author_feed(),
                author_decision,
                assessed_valence: payload.assessed_valence,
                attribution: payload.attribution,
                outcome_authors_authorized,
                exercise_authors_authorized,
                ku_authors_authorized,
                causal_chain_complete,
            });
        }

        let state = compute_benefit_closure_state(&outcome_branches, &benefit_branches);
        BenefitClosureView {
            case_id,
            state,
            outcome_branches,
            benefit_branches,
        }
    }
}

fn compute_benefit_closure_state(
    outcomes: &[AssessedOutcomeBranch],
    benefits: &[AssessedBenefitBranch],
) -> BenefitClosureState {
    let auth_outcomes: Vec<&AssessedOutcomeBranch> = outcomes
        .iter()
        .filter(|o| {
            matches!(
                o.author_decision,
                FeedAuthorityDecision::AuthorizedRelative { .. }
            )
        })
        .collect();
    if auth_outcomes.is_empty() {
        return BenefitClosureState::DeferredIncomplete;
    }

    let outcome_valences: BTreeSet<u64> = auth_outcomes.iter().map(|o| o.valence as u64).collect();
    if outcome_valences.len() > 1
        || auth_outcomes
            .iter()
            .any(|o| o.valence == OutcomeValence::Mixed)
    {
        return BenefitClosureState::ContestedOrMixed;
    }

    let valid_benefits: Vec<&AssessedBenefitBranch> = benefits
        .iter()
        .filter(|b| b.causal_chain_complete)
        .collect();

    // Check for any contested/mixed/conflicting valid benefit branches.
    let benefit_valences: BTreeSet<u64> = valid_benefits
        .iter()
        .map(|b| b.assessed_valence as u64)
        .collect();
    let has_supported = valid_benefits
        .iter()
        .any(|b| b.attribution == AttributionStatus::Supported);
    let has_opposed_or_contested = valid_benefits.iter().any(|b| {
        matches!(
            b.attribution,
            AttributionStatus::Opposed | AttributionStatus::Contested
        ) || b.assessed_valence == OutcomeValence::Mixed
    });

    if benefit_valences.len() > 1 || (has_supported && has_opposed_or_contested) {
        return BenefitClosureState::ContestedOrMixed;
    }
    if valid_benefits.iter().any(|b| {
        b.attribution == AttributionStatus::Contested || b.assessed_valence == OutcomeValence::Mixed
    }) {
        return BenefitClosureState::ContestedOrMixed;
    }

    let sole_outcome_valence = auth_outcomes[0].valence;
    if valid_benefits.is_empty() {
        // If there are unresolved/incomplete benefit claims or beneficial outcome awaiting attribution:
        if matches!(
            sole_outcome_valence,
            OutcomeValence::Harmful | OutcomeValence::NoObservedChange
        ) && benefits.is_empty()
        {
            return BenefitClosureState::ObservedNonBeneficial;
        }
        return BenefitClosureState::DeferredIncomplete;
    }

    if valid_benefits.iter().all(|b| {
        b.attribution == AttributionStatus::Supported
            && b.assessed_valence == OutcomeValence::Beneficial
    }) && sole_outcome_valence == OutcomeValence::Beneficial
    {
        return BenefitClosureState::SupportedUncontested;
    }

    if valid_benefits.iter().all(|b| {
        matches!(
            b.assessed_valence,
            OutcomeValence::Harmful | OutcomeValence::NoObservedChange
        ) || b.attribution == AttributionStatus::Opposed
    }) && matches!(
        sole_outcome_valence,
        OutcomeValence::Harmful | OutcomeValence::NoObservedChange
    ) {
        return BenefitClosureState::ObservedNonBeneficial;
    }

    BenefitClosureState::DeferredIncomplete
}

fn known_envelope(
    object: &ValidatedKnowledgeObject,
    expected: ObjectKind,
) -> Result<&KnowledgeObjectEnvelope, OutcomeEvidenceError> {
    match object.semantics() {
        ObjectSemantics::Known(envelope)
            if envelope.kind == expected
                && envelope.kind_version.major == OUTCOME_EVIDENCE_MAJOR =>
        {
            Ok(envelope)
        }
        _ => Err(OutcomeEvidenceError::WrongPayloadKind),
    }
}

fn validate_version(map: &[(u64, CanonicalValue)]) -> Result<(), OutcomeEvidenceError> {
    if value_unsigned(map, 0, "version.major")? != OUTCOME_EVIDENCE_MAJOR
        || value_unsigned(map, 1, "version.minor")? != OUTCOME_EVIDENCE_MINOR
    {
        Err(OutcomeEvidenceError::InvalidField("version"))
    } else {
        Ok(())
    }
}

fn validate_reference_limit(values: &[ObjectReference]) -> Result<(), OutcomeEvidenceError> {
    if values.len() > MAX_OUTCOME_REFERENCES {
        Err(OutcomeEvidenceError::Limit)
    } else {
        Ok(())
    }
}

fn validate_limitations(values: &[EvidenceLimitation]) -> Result<(), OutcomeEvidenceError> {
    if values.len() > MAX_OUTCOME_LIMITATIONS
        || values.iter().copied().collect::<BTreeSet<_>>().len() != values.len()
    {
        Err(OutcomeEvidenceError::Limit)
    } else {
        Ok(())
    }
}

fn reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, OutcomeEvidenceError> {
    validate_reference_limit(values)?;
    let members = values
        .iter()
        .map(ObjectReference::to_value)
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn event_set(values: &[EventCid]) -> Result<CanonicalValue, OutcomeEvidenceError> {
    if values.len() > MAX_OUTCOME_REFERENCES {
        return Err(OutcomeEvidenceError::Limit);
    }
    let members = values
        .iter()
        .map(|cid| CanonicalValue::Bytes(cid.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn limitation_set(values: &[EvidenceLimitation]) -> Result<CanonicalValue, OutcomeEvidenceError> {
    validate_limitations(values)?;
    let members = values
        .iter()
        .map(|limitation| CanonicalValue::Unsigned(*limitation as u64))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn parse_reference_set(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Vec<ObjectReference>, OutcomeEvidenceError> {
    value_array(map, key, field)?
        .iter()
        .map(ObjectReference::from_value)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

fn parse_event_set(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Vec<EventCid>, OutcomeEvidenceError> {
    value_array(map, key, field)?
        .iter()
        .map(|value| match value {
            CanonicalValue::Bytes(bytes) if bytes.len() == 32 => {
                let mut cid = [0; 32];
                cid.copy_from_slice(bytes);
                Ok(EventCid::from_bytes(cid))
            }
            _ => Err(OutcomeEvidenceError::InvalidField(field)),
        })
        .collect()
}

fn parse_limitations(
    map: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<Vec<EvidenceLimitation>, OutcomeEvidenceError> {
    value_array(map, key, "limitations")?
        .iter()
        .map(|value| match value {
            CanonicalValue::Unsigned(code) => EvidenceLimitation::from_code(*code)
                .ok_or(OutcomeEvidenceError::InvalidField("limitations")),
            _ => Err(OutcomeEvidenceError::InvalidField("limitations")),
        })
        .collect()
}

fn value_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], OutcomeEvidenceError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(OutcomeEvidenceError::InvalidField(field)),
    }
}

fn value_optional(map: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
}

fn value_required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, OutcomeEvidenceError> {
    value_optional(map, key).ok_or(OutcomeEvidenceError::InvalidField(field))
}

fn value_unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, OutcomeEvidenceError> {
    match value_required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(OutcomeEvidenceError::InvalidField(field)),
    }
}

fn value_array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], OutcomeEvidenceError> {
    match value_required(map, key, field)? {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(OutcomeEvidenceError::InvalidField(field)),
    }
}

fn value_bytes16(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 16], OutcomeEvidenceError> {
    let bytes = value_bytes(map, key, field)?;
    if bytes.len() != 16 {
        return Err(OutcomeEvidenceError::InvalidField(field));
    }
    let mut value = [0; 16];
    value.copy_from_slice(bytes);
    Ok(value)
}

fn value_bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], OutcomeEvidenceError> {
    let bytes = value_bytes(map, key, field)?;
    if bytes.len() != 32 {
        return Err(OutcomeEvidenceError::InvalidField(field));
    }
    let mut value = [0; 32];
    value.copy_from_slice(bytes);
    Ok(value)
}

fn value_bytes<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], OutcomeEvidenceError> {
    match value_required(map, key, field)? {
        CanonicalValue::Bytes(bytes) => Ok(bytes),
        _ => Err(OutcomeEvidenceError::InvalidField(field)),
    }
}

#[derive(Debug)]
pub enum OutcomeEvidenceError {
    Canonical(super::canonical::CanonicalError),
    Object(ObjectError),
    InvalidField(&'static str),
    Limit,
    MissingRequiredLimitation(EvidenceLimitation),
    OutcomeObservationRequired,
    AttributionEvidenceRequired,
    UnresolvedOutcomeReference,
    NonCanonicalPayload,
    WrongPayloadKind,
    WrongEventType,
    PayloadReferenceMismatch,
    DisclosureMismatch,
}

impl From<super::canonical::CanonicalError> for OutcomeEvidenceError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for OutcomeEvidenceError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;

    use super::*;
    use crate::foundation::{
        decode_feed_inception, decode_knowledge_event, decode_knowledge_object, DeviceId,
        FeedInception, KnowledgeEventEnvelope, KnownObjectKind, NamespaceCommitment,
        SignedFeedInception, UseMode,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn author() -> (SigningKey, crate::foundation::ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[41; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"outcome-evidence-test", [42; 32]).unwrap(),
            0,
            DeviceId::from_bytes([43; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        (
            key,
            decode_feed_inception(&signed.encode().unwrap()).unwrap(),
        )
    }

    fn signed_object_event(
        object: KnowledgeObjectEnvelope,
        kind: ObjectKind,
        event_type: EventType,
        sequence: u64,
        nonce: u8,
    ) -> (ValidatedKnowledgeEvent, ValidatedKnowledgeObject) {
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let object = decode_knowledge_object(
            &object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(kind, 1)],
            &[],
        )
        .unwrap();
        let (key, feed) = author();
        let mut event = KnowledgeEventEnvelope::new(
            event_type,
            feed.feed_id,
            sequence,
            DisclosureClass::LocalOnly,
            [nonce; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let bytes = event.sign(&feed, &key).unwrap().encode().unwrap().0;
        let event = decode_knowledge_event(
            &bytes,
            &feed,
            &[OUTCOME_OBSERVATION_EVENT_TYPE, BENEFIT_EVIDENCE_EVENT_TYPE],
        )
        .unwrap();
        (event, object)
    }

    fn outcome(valence: OutcomeValence, measurement: u8) -> OutcomeObservationPayload {
        OutcomeObservationPayload {
            task_context_commitment: [50; 32],
            outcome_class: ConceptCcid::from_bytes([51; 16]),
            valence,
            affected: AffectedScope {
                principal: Some(AffectedPrincipal::Commitment([52; 32])),
                assembly: Some(reference(53)),
            },
            measurement_evidence: vec![reference(measurement)],
            baseline: Some(reference(54)),
            limitations: vec![EvidenceLimitation::PartialObservation],
            observation_policy: reference(55),
            observed_frontier: [56; 32],
        }
    }

    fn validated_outcome(
        valence: OutcomeValence,
        measurement: u8,
        sequence: u64,
        nonce: u8,
    ) -> ValidatedOutcomeObservationEvent {
        let (event, object) = signed_object_event(
            outcome(valence, measurement)
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
            OUTCOME_OBSERVATION_KIND,
            OUTCOME_OBSERVATION_EVENT_TYPE,
            sequence,
            nonce,
        );
        ValidatedOutcomeObservationEvent::bind(&event, &object).unwrap()
    }

    fn unknown_benefit(outcome_ref: ObjectReference) -> BenefitEvidencePayload {
        BenefitEvidencePayload {
            task_context_commitment: [50; 32],
            outcome_observations: vec![outcome_ref],
            use_events: vec![EventCid::from_bytes([61; 32])],
            knowledge_subjects: vec![reference(62)],
            assessed_valence: OutcomeValence::Unknown,
            attribution: AttributionStatus::Unknown,
            causal_evidence: Vec::new(),
            counterfactual_evidence: Vec::new(),
            limitations: vec![
                EvidenceLimitation::AttributionUnknown,
                EvidenceLimitation::MissingCounterfactual,
            ],
            benefit_policy: reference(63),
            assessed_frontier: [64; 32],
        }
    }

    #[test]
    fn signed_outcome_round_trips_exact_context_and_affected_scope() {
        let validated = validated_outcome(OutcomeValence::Beneficial, 57, 0, 58);
        assert_eq!(validated.payload().task_context_commitment, [50; 32]);
        assert_eq!(validated.payload().valence, OutcomeValence::Beneficial);
        assert!(validated.payload().affected.principal.is_some());
        assert_eq!(validated.payload().affected.assembly, Some(reference(53)));
    }

    #[test]
    fn use_event_alone_cannot_construct_benefit_evidence() {
        let payload = BenefitEvidencePayload {
            outcome_observations: Vec::new(),
            ..unknown_benefit(reference(65))
        };
        assert!(BenefitEvidencePayload::requires_outcome_observation());
        assert!(matches!(
            payload.canonical_payload().unwrap_err(),
            OutcomeEvidenceError::OutcomeObservationRequired
        ));
    }

    #[test]
    fn missing_attribution_remains_unknown_and_never_becomes_reward() {
        let outcome = validated_outcome(OutcomeValence::Beneficial, 66, 0, 67);
        let payload = unknown_benefit(ObjectReference::new(
            0,
            outcome.payload_object_cid().into_bytes(),
        ));
        payload.resolve_outcomes(&[outcome]).unwrap();
        assert_eq!(payload.attribution, AttributionStatus::Unknown);
        assert!(!payload.establishes_attributed_benefit());
        assert!(!payload.establishes_truth());
        assert!(!payload.is_reward_instruction());
    }

    #[test]
    fn signed_benefit_binds_outcome_use_policy_frontier_and_limitations() {
        let outcome = validated_outcome(OutcomeValence::Mixed, 68, 0, 69);
        let payload = unknown_benefit(ObjectReference::new(
            0,
            outcome.payload_object_cid().into_bytes(),
        ));
        let (event, object) = signed_object_event(
            payload
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
            BENEFIT_EVIDENCE_KIND,
            BENEFIT_EVIDENCE_EVENT_TYPE,
            1,
            70,
        );
        let validated = ValidatedBenefitEvidenceEvent::bind(&event, &object).unwrap();
        assert_eq!(
            validated.payload().canonical_payload().unwrap(),
            payload.canonical_payload().unwrap()
        );
        validated.payload().resolve_outcomes(&[outcome]).unwrap();
    }

    #[test]
    fn opposing_or_refutation_use_is_not_rejected_by_benefit_contract() {
        let mode = UseMode::ComparedOrOpposed;
        let outcome = validated_outcome(OutcomeValence::Beneficial, 71, 0, 72);
        let payload = unknown_benefit(ObjectReference::new(
            0,
            outcome.payload_object_cid().into_bytes(),
        ));
        assert_eq!(mode, UseMode::ComparedOrOpposed);
        assert!(payload.canonical_payload().is_ok());
        assert_eq!(payload.use_events.len(), 1);
    }

    #[test]
    fn contradictory_outcomes_remain_branches_independent_of_arrival_order() {
        let beneficial = validated_outcome(OutcomeValence::Beneficial, 73, 0, 74);
        let harmful = validated_outcome(OutcomeValence::Harmful, 75, 1, 76);
        assert_eq!(beneficial.case_id(), harmful.case_id());
        let first = AssessedOutcomeObservation {
            observation: beneficial,
            authority: OutcomeAuthority::Authorized,
        };
        let second = AssessedOutcomeObservation {
            observation: harmful,
            authority: OutcomeAuthority::Authorized,
        };
        let mut left = OutcomeEvidenceReducer::default();
        let mut right = OutcomeEvidenceReducer::default();
        left.record(first.clone());
        left.record(second.clone());
        right.record(second);
        right.record(first.clone());
        let left = left.branch(first.observation.case_id());
        let right = right.branch(first.observation.case_id());
        assert_eq!(left.records, right.records);
        assert_eq!(left.records.len(), 2);
        assert!(left.authorized_conflict);
        assert!(!left.is_globally_complete());
    }

    #[test]
    fn benefit_closure_reducer_evaluates_each_principal_independently_and_preserves_branches() {
        use crate::foundation::{
            actor_id_from_root_key, ActorDelegation, ActorRevocation, ActorRootDelegation,
            KeyStateReducer, UseEvidencePayload, ValidatedUseEvidenceEvent,
            USE_EVIDENCE_EVENT_TYPE, USE_EVIDENCE_KIND,
        };

        let make_principal = |seed: u8| {
            let sk = SigningKey::from_bytes(&[seed; 32]);
            let actor = actor_id_from_root_key(*sk.verifying_key().as_bytes()).unwrap();
            let dev = DeviceId::from_bytes([seed.wrapping_add(2); 32]);
            let ns =
                NamespaceCommitment::derive(b"p4-closure", [seed.wrapping_add(1); 32]).unwrap();
            let mut inc = FeedInception::new(*sk.verifying_key().as_bytes(), ns, 0, dev);
            let provisional_feed_id = inc.feed_id().unwrap();
            let root_del = crate::foundation::decode_actor_root_delegation(
                &ActorRootDelegation::new(
                    *sk.verifying_key().as_bytes(),
                    provisional_feed_id,
                    dev,
                    None,
                    0,
                    10,
                )
                .unwrap()
                .sign(&sk)
                .unwrap()
                .encode()
                .unwrap(),
            )
            .unwrap();
            inc.actor_delegation_ref = Some(root_del.cid.into_bytes());
            let feed = decode_feed_inception(&inc.sign(&sk).unwrap().encode().unwrap()).unwrap();
            (sk, actor, feed, root_del)
        };

        let (_ku_sk, _ku_actor, ku_feed, ku_root) = make_principal(10);
        let (use_sk, _use_actor, use_feed, use_root) = make_principal(20);
        let (out_sk, _out_actor, out_feed, out_root) = make_principal(30);
        let (ben_root_sk, ben_actor, ben_root_feed, ben_root) = make_principal(40);

        // Create a child device feed for the benefit author so we can test revocation isolation.
        let ben_child_sk = SigningKey::from_bytes(&[45; 32]);
        let ben_child_dev = DeviceId::from_bytes([46; 32]);
        let ben_child_ns = NamespaceCommitment::derive(b"p4-ben-child", [47; 32]).unwrap();
        let mut ben_child_inc = FeedInception::new(
            *ben_child_sk.verifying_key().as_bytes(),
            ben_child_ns,
            0,
            ben_child_dev,
        );
        let ben_child_feed_id = ben_child_inc.feed_id().unwrap();
        let ben_child_del = crate::foundation::decode_actor_delegation(
            &ActorDelegation::new(
                ben_actor,
                ben_root.cid,
                ben_root_feed.feed_id,
                ben_child_feed_id,
                ben_child_dev,
                Some(ben_child_ns),
                0,
                5,
            )
            .unwrap()
            .sign(&ben_root_feed, &ben_root_sk)
            .unwrap()
            .encode()
            .unwrap(),
            &ben_root_feed,
        )
        .unwrap();
        ben_child_inc.actor_delegation_ref = Some(ben_child_del.cid.into_bytes());
        let ben_child_feed =
            decode_feed_inception(&ben_child_inc.sign(&ben_child_sk).unwrap().encode().unwrap())
                .unwrap();

        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([90; 32]));
        key_state.accept_root(ku_root.scoped_delegation());
        key_state.accept_root(use_root.scoped_delegation());
        key_state.accept_root(out_root.scoped_delegation());
        key_state.accept_root(ben_root.scoped_delegation());
        key_state.submit_child(ben_child_del.scoped_delegation());

        let ku_ref = reference(62);
        let sign_on = |obj: KnowledgeObjectEnvelope,
                       kind: ObjectKind,
                       ev_type: EventType,
                       feed: &ValidatedFeedInception,
                       sk: &SigningKey,
                       seq: u64,
                       nonce: u8| {
            let (obj_bytes, obj_cid) = obj.encode(ResourceProfile::ObjectV1).unwrap();
            let v_obj = decode_knowledge_object(
                &obj_bytes,
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(kind, 1)],
                &[],
            )
            .unwrap();
            let mut ev = KnowledgeEventEnvelope::new(
                ev_type,
                feed.feed_id,
                seq,
                DisclosureClass::LocalOnly,
                [nonce; 32],
            );
            ev.payload_refs = vec![ObjectReference::new(0, obj_cid.into_bytes())];
            let (ev_bytes, _) = ev.sign(feed, sk).unwrap().encode().unwrap();
            let v_ev = decode_knowledge_event(&ev_bytes, feed, &[ev_type]).unwrap();
            (v_ev, v_obj)
        };

        // Use event signed by use_feed
        let use_payload = UseEvidencePayload {
            subjects: vec![ku_ref.clone()],
            mode: UseMode::Application,
            actor_class: ConceptCcid::from_bytes([80; 16]),
            task_context_commitment: [50; 32],
            causal_role: ConceptCcid::from_bytes([81; 16]),
            assembly: None,
            mapping: None,
            outcome_observation: None,
            use_policy: reference(82),
            observed_frontier: [56; 32],
        };
        let (u_ev, u_obj) = sign_on(
            use_payload
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
            USE_EVIDENCE_KIND,
            USE_EVIDENCE_EVENT_TYPE,
            &use_feed,
            &use_sk,
            1,
            83,
        );
        let v_use = ValidatedUseEvidenceEvent::bind(&u_ev, &u_obj).unwrap();

        // Outcome observation signed by out_feed
        let (o_ev, o_obj) = sign_on(
            outcome(OutcomeValence::Beneficial, 84)
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
            OUTCOME_OBSERVATION_KIND,
            OUTCOME_OBSERVATION_EVENT_TYPE,
            &out_feed,
            &out_sk,
            1,
            85,
        );
        let v_out = ValidatedOutcomeObservationEvent::bind(&o_ev, &o_obj).unwrap();

        // Benefit evidence signed by ben_child_feed
        let ben_payload = BenefitEvidencePayload {
            task_context_commitment: [50; 32],
            outcome_observations: vec![ObjectReference::new(
                0,
                v_out.payload_object_cid().into_bytes(),
            )],
            use_events: vec![v_use.event_cid()],
            knowledge_subjects: vec![ku_ref.clone()],
            assessed_valence: OutcomeValence::Beneficial,
            attribution: AttributionStatus::Supported,
            causal_evidence: vec![reference(86)],
            counterfactual_evidence: vec![reference(87)],
            limitations: vec![],
            benefit_policy: reference(63),
            assessed_frontier: [64; 32],
        };
        let (b_ev, b_obj) = sign_on(
            ben_payload
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
            BENEFIT_EVIDENCE_KIND,
            BENEFIT_EVIDENCE_EVENT_TYPE,
            &ben_child_feed,
            &ben_child_sk,
            1,
            88,
        );
        let v_ben = ValidatedBenefitEvidenceEvent::bind(&b_ev, &b_obj).unwrap();

        let mut closure = BenefitClosureReducer::default();
        closure.register_feed(ku_feed);
        closure.register_feed(use_feed);
        closure.register_feed(out_feed.clone());
        closure.register_feed(ben_child_feed);
        closure.register_ku_author(ku_ref, ku_root.signed.delegation.subject_feed);
        closure.record_exercise(ExerciseEvidence::Use(v_use));
        closure.record_outcome(v_out.clone());
        closure.record_benefit(v_ben);

        let view = closure.evaluate_case(v_out.case_id(), &key_state);
        assert_eq!(view.state, BenefitClosureState::SupportedUncontested);
        assert!(!view.is_reward_instruction());
        assert!(!view.establishes_truth());
        assert!(!view.collapses_to_single_winner());

        // Revoking ONLY the benefit child feed degrades closure to DeferredIncomplete
        // while keeping the outcome author AuthorizedRelative!
        let rev = crate::foundation::decode_actor_revocation(
            &ActorRevocation::new(
                ben_actor,
                ben_child_del.cid,
                ben_child_dev,
                0,
                ben_root.cid,
                ben_root_feed.feed_id,
            )
            .unwrap()
            .sign(&ben_root_feed, &ben_root_sk)
            .unwrap()
            .encode()
            .unwrap(),
            &ben_root_feed,
        )
        .unwrap();
        key_state.submit_revocation(rev.scoped_revocation());

        let after_rev = closure.evaluate_case(v_out.case_id(), &key_state);
        assert_eq!(after_rev.state, BenefitClosureState::DeferredIncomplete);
        assert!(matches!(
            after_rev.outcome_branches[0].author_decision,
            FeedAuthorityDecision::AuthorizedRelative { .. }
        ));
        assert!(matches!(
            after_rev.benefit_branches[0].author_decision,
            FeedAuthorityDecision::QuarantinedRevokedRelative { .. }
        ));

        // Concurrent opposing outcome from authorized out_feed produces ContestedOrMixed
        // and retains BOTH outcome branches without single-winner collapse.
        let (o2_ev, o2_obj) = sign_on(
            outcome(OutcomeValence::Harmful, 89)
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
            OUTCOME_OBSERVATION_KIND,
            OUTCOME_OBSERVATION_EVENT_TYPE,
            &out_feed,
            &out_sk,
            2,
            90,
        );
        let v_out2 = ValidatedOutcomeObservationEvent::bind(&o2_ev, &o2_obj).unwrap();
        closure.record_outcome(v_out2);
        let contested = closure.evaluate_case(v_out.case_id(), &key_state);
        assert_eq!(contested.state, BenefitClosureState::ContestedOrMixed);
        assert_eq!(contested.outcome_branches.len(), 2);
    }
}
