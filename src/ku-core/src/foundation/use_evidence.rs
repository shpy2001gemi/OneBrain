//! Signed use/derivation evidence path for PoMV vNext.
//!
//! These records attest that an actor exercised knowledge in a causal role.
//! They do not establish proposition truth, benefit, reward, or ranking.

use std::collections::BTreeMap;

use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::content_id::{EventCid, MappingKernelCid};
use super::event::{EventType, ValidatedKnowledgeEvent};
use super::identity::FeedId;
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};
use super::schema_registry::{
    EVENT_TYPE_DERIVATION_EVIDENCE, EVENT_TYPE_USE_EVIDENCE, OBJECT_KIND_DERIVATION_EVIDENCE,
    OBJECT_KIND_USE_EVIDENCE,
};
use super::semantic::ConceptCcid;

pub const USE_EVIDENCE_KIND: ObjectKind = ObjectKind(OBJECT_KIND_USE_EVIDENCE);
pub const DERIVATION_EVIDENCE_KIND: ObjectKind = ObjectKind(OBJECT_KIND_DERIVATION_EVIDENCE);
pub const USE_EVIDENCE_EVENT_TYPE: EventType = EventType(EVENT_TYPE_USE_EVIDENCE);
pub const DERIVATION_EVIDENCE_EVENT_TYPE: EventType = EventType(EVENT_TYPE_DERIVATION_EVIDENCE);
pub const USE_EVIDENCE_PROFILE_MAJOR: u64 = 1;
pub const USE_EVIDENCE_PROFILE_MINOR: u64 = 0;
pub const MAX_USE_SUBJECTS: usize = 16_384;
pub const MAX_DERIVATION_INPUTS: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum UseMode {
    Application = 0,
    Transformation = 1,
    Epistemic = 2,
    Transfer = 3,
    Discovery = 4,
    ReceptorDiscovered = 5,
    CandidateEvaluated = 6,
    ConstraintClarified = 7,
    GapPartiallyFilled = 8,
    AssemblyUsed = 9,
    AnalogicalTransfer = 10,
    ComparedOrOpposed = 11,
    CapabilityResultUsed = 12,
}

impl UseMode {
    /// Exposure-only signals are intentionally absent.
    pub const ALL: [Self; 13] = [
        Self::Application,
        Self::Transformation,
        Self::Epistemic,
        Self::Transfer,
        Self::Discovery,
        Self::ReceptorDiscovered,
        Self::CandidateEvaluated,
        Self::ConstraintClarified,
        Self::GapPartiallyFilled,
        Self::AssemblyUsed,
        Self::AnalogicalTransfer,
        Self::ComparedOrOpposed,
        Self::CapabilityResultUsed,
    ];

    const fn from_code(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::Application),
            1 => Some(Self::Transformation),
            2 => Some(Self::Epistemic),
            3 => Some(Self::Transfer),
            4 => Some(Self::Discovery),
            5 => Some(Self::ReceptorDiscovered),
            6 => Some(Self::CandidateEvaluated),
            7 => Some(Self::ConstraintClarified),
            8 => Some(Self::GapPartiallyFilled),
            9 => Some(Self::AssemblyUsed),
            10 => Some(Self::AnalogicalTransfer),
            11 => Some(Self::ComparedOrOpposed),
            12 => Some(Self::CapabilityResultUsed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseEvidencePayload {
    pub subjects: Vec<ObjectReference>,
    pub mode: UseMode,
    pub actor_class: ConceptCcid,
    pub task_context_commitment: [u8; 32],
    pub causal_role: ConceptCcid,
    pub assembly: Option<ObjectReference>,
    pub mapping: Option<MappingKernelCid>,
    /// A reference to separately modeled outcome evidence. Its presence still
    /// does not make this UseEvent a benefit assessment.
    pub outcome_observation: Option<ObjectReference>,
    pub use_policy: ObjectReference,
    pub observed_frontier: [u8; 32],
}

impl UseEvidencePayload {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, UseEvidenceError> {
        self.validate()?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(USE_EVIDENCE_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(USE_EVIDENCE_PROFILE_MINOR)),
            (
                2,
                canonical_reference_set(&self.subjects, MAX_USE_SUBJECTS)?,
            ),
            (3, CanonicalValue::Unsigned(self.mode as u64)),
            (
                4,
                CanonicalValue::Bytes(self.actor_class.as_bytes().to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(self.task_context_commitment.to_vec()),
            ),
            (
                6,
                CanonicalValue::Bytes(self.causal_role.as_bytes().to_vec()),
            ),
            (10, self.use_policy.to_value()),
            (11, CanonicalValue::Bytes(self.observed_frontier.to_vec())),
        ];
        if let Some(assembly) = &self.assembly {
            fields.push((7, assembly.to_value()));
        }
        if let Some(mapping) = self.mapping {
            fields.push((8, CanonicalValue::Bytes(mapping.as_bytes().to_vec())));
        }
        if let Some(outcome) = &self.outcome_observation {
            fields.push((9, outcome.to_value()));
        }
        fields.sort_by_key(|(key, _)| *key);
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, UseEvidenceError> {
        let mut object = KnowledgeObjectEnvelope::new(
            USE_EVIDENCE_KIND,
            SchemaVersion::new(USE_EVIDENCE_PROFILE_MAJOR, USE_EVIDENCE_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = self.subjects.clone();
        object.references.push(self.use_policy.clone());
        object.references.extend(self.assembly.iter().cloned());
        object
            .references
            .extend(self.outcome_observation.iter().cloned());
        Ok(object)
    }

    fn validate(&self) -> Result<(), UseEvidenceError> {
        if self.subjects.is_empty() || self.subjects.len() > MAX_USE_SUBJECTS {
            return Err(UseEvidenceError::Limit);
        }
        if self.task_context_commitment == [0; 32] || self.observed_frontier == [0; 32] {
            return Err(UseEvidenceError::InvalidField("commitment_or_frontier"));
        }
        Ok(())
    }

    fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, UseEvidenceError> {
        let envelope = known_envelope(object, USE_EVIDENCE_KIND)?;
        let map = value_map(&envelope.payload, "use.payload")?;
        let payload = Self {
            subjects: value_array(map, 2, "use.subjects")?
                .iter()
                .map(ObjectReference::from_value)
                .collect::<Result<_, _>>()?,
            mode: UseMode::from_code(value_unsigned(map, 3, "use.mode")?)
                .ok_or(UseEvidenceError::InvalidField("use.mode"))?,
            actor_class: ConceptCcid::from_bytes(value_bytes16(map, 4, "use.actor_class")?),
            task_context_commitment: value_bytes32(map, 5, "use.task")?,
            causal_role: ConceptCcid::from_bytes(value_bytes16(map, 6, "use.causal_role")?),
            assembly: value_optional(map, 7)
                .map(ObjectReference::from_value)
                .transpose()?,
            mapping: value_optional(map, 8)
                .map(|_| value_bytes32(map, 8, "use.mapping").map(MappingKernelCid::from_bytes))
                .transpose()?,
            outcome_observation: value_optional(map, 9)
                .map(ObjectReference::from_value)
                .transpose()?,
            use_policy: ObjectReference::from_value(value_required(map, 10, "use.policy")?)?,
            observed_frontier: value_bytes32(map, 11, "use.frontier")?,
        };
        payload.validate()?;
        if payload.canonical_payload()? != envelope.payload {
            return Err(UseEvidenceError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivationInput {
    pub input: ObjectReference,
    pub causal_role: ConceptCcid,
}

impl DerivationInput {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.input.to_value()),
            (
                1,
                CanonicalValue::Bytes(self.causal_role.as_bytes().to_vec()),
            ),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivationEvidencePayload {
    pub inputs: Vec<DerivationInput>,
    pub output: ObjectReference,
    pub derivation_rule: ObjectReference,
    pub task_context_commitment: [u8; 32],
    pub derivation_policy: ObjectReference,
    pub observed_frontier: [u8; 32],
}

impl DerivationEvidencePayload {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, UseEvidenceError> {
        self.validate()?;
        let members = self
            .inputs
            .iter()
            .map(DerivationInput::to_value)
            .map(|value| (value.clone(), value))
            .collect();
        let inputs = canonicalize_set_by_key(members, ResourceProfile::ObjectV1)?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(USE_EVIDENCE_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(USE_EVIDENCE_PROFILE_MINOR)),
            (2, CanonicalValue::Array(inputs)),
            (3, self.output.to_value()),
            (4, self.derivation_rule.to_value()),
            (
                5,
                CanonicalValue::Bytes(self.task_context_commitment.to_vec()),
            ),
            (6, self.derivation_policy.to_value()),
            (7, CanonicalValue::Bytes(self.observed_frontier.to_vec())),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, UseEvidenceError> {
        let mut object = KnowledgeObjectEnvelope::new(
            DERIVATION_EVIDENCE_KIND,
            SchemaVersion::new(USE_EVIDENCE_PROFILE_MAJOR, USE_EVIDENCE_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = self
            .inputs
            .iter()
            .map(|input| input.input.clone())
            .collect();
        object.references.push(self.output.clone());
        object.references.push(self.derivation_rule.clone());
        object.references.push(self.derivation_policy.clone());
        Ok(object)
    }

    fn validate(&self) -> Result<(), UseEvidenceError> {
        if self.inputs.is_empty() || self.inputs.len() > MAX_DERIVATION_INPUTS {
            return Err(UseEvidenceError::Limit);
        }
        if self.task_context_commitment == [0; 32] || self.observed_frontier == [0; 32] {
            return Err(UseEvidenceError::InvalidField("commitment_or_frontier"));
        }
        Ok(())
    }

    fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, UseEvidenceError> {
        let envelope = known_envelope(object, DERIVATION_EVIDENCE_KIND)?;
        let map = value_map(&envelope.payload, "derivation.payload")?;
        let inputs = value_array(map, 2, "derivation.inputs")?
            .iter()
            .map(|value| {
                let input = value_map(value, "derivation.input")?;
                Ok(DerivationInput {
                    input: ObjectReference::from_value(value_required(
                        input,
                        0,
                        "derivation.input.ref",
                    )?)?,
                    causal_role: ConceptCcid::from_bytes(value_bytes16(
                        input,
                        1,
                        "derivation.input.role",
                    )?),
                })
            })
            .collect::<Result<Vec<_>, UseEvidenceError>>()?;
        let payload = Self {
            inputs,
            output: ObjectReference::from_value(value_required(map, 3, "derivation.output")?)?,
            derivation_rule: ObjectReference::from_value(value_required(
                map,
                4,
                "derivation.rule",
            )?)?,
            task_context_commitment: value_bytes32(map, 5, "derivation.task")?,
            derivation_policy: ObjectReference::from_value(value_required(
                map,
                6,
                "derivation.policy",
            )?)?,
            observed_frontier: value_bytes32(map, 7, "derivation.frontier")?,
        };
        payload.validate()?;
        if payload.canonical_payload()? != envelope.payload {
            return Err(UseEvidenceError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedUseEvidenceEvent {
    event_cid: EventCid,
    author_feed: FeedId,
    author_sequence: u64,
    payload_object: super::content_id::ObjectCid,
    payload: UseEvidencePayload,
}

impl ValidatedUseEvidenceEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, UseEvidenceError> {
        bind_event(event, payload_object, USE_EVIDENCE_EVENT_TYPE)?;
        Ok(Self {
            event_cid: event.cid(),
            author_feed: event.signed.event.author_feed,
            author_sequence: event.signed.event.author_sequence,
            payload_object: payload_object.cid(),
            payload: UseEvidencePayload::from_object(payload_object)?,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn payload_object_cid(&self) -> super::content_id::ObjectCid {
        self.payload_object
    }

    pub const fn author_feed(&self) -> FeedId {
        self.author_feed
    }

    pub const fn author_sequence(&self) -> u64 {
        self.author_sequence
    }

    pub const fn payload(&self) -> &UseEvidencePayload {
        &self.payload
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedDerivationEvidenceEvent {
    event_cid: EventCid,
    author_feed: FeedId,
    author_sequence: u64,
    payload_object: super::content_id::ObjectCid,
    payload: DerivationEvidencePayload,
}

impl ValidatedDerivationEvidenceEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, UseEvidenceError> {
        bind_event(event, payload_object, DERIVATION_EVIDENCE_EVENT_TYPE)?;
        Ok(Self {
            event_cid: event.cid(),
            author_feed: event.signed.event.author_feed,
            author_sequence: event.signed.event.author_sequence,
            payload_object: payload_object.cid(),
            payload: DerivationEvidencePayload::from_object(payload_object)?,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn payload_object_cid(&self) -> super::content_id::ObjectCid {
        self.payload_object
    }

    pub const fn author_feed(&self) -> FeedId {
        self.author_feed
    }

    pub const fn author_sequence(&self) -> u64 {
        self.author_sequence
    }

    pub const fn payload(&self) -> &DerivationEvidencePayload {
        &self.payload
    }
}

fn bind_event(
    event: &ValidatedKnowledgeEvent,
    payload: &ValidatedKnowledgeObject,
    expected_type: EventType,
) -> Result<(), UseEvidenceError> {
    if event.signed.event.event_type != expected_type {
        return Err(UseEvidenceError::WrongEventType);
    }
    if event.signed.event.disclosure != payload.disclosure() {
        return Err(UseEvidenceError::DisclosureMismatch);
    }
    let expected = ObjectReference::new(0, payload.cid().into_bytes());
    if event.signed.event.payload_refs != [expected] {
        return Err(UseEvidenceError::PayloadReferenceMismatch);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExerciseEvidence {
    Use(ValidatedUseEvidenceEvent),
    Derivation(ValidatedDerivationEvidenceEvent),
}

impl ExerciseEvidence {
    pub const fn event_cid(&self) -> EventCid {
        match self {
            Self::Use(event) => event.event_cid,
            Self::Derivation(event) => event.event_cid,
        }
    }

    pub const fn author_feed(&self) -> FeedId {
        match self {
            Self::Use(event) => event.author_feed,
            Self::Derivation(event) => event.author_feed,
        }
    }

    pub const fn author_sequence(&self) -> u64 {
        match self {
            Self::Use(event) => event.author_sequence,
            Self::Derivation(event) => event.author_sequence,
        }
    }

    pub const fn establishes_benefit(&self) -> bool {
        false
    }

    pub const fn establishes_truth(&self) -> bool {
        false
    }

    pub const fn is_reward_instruction(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExerciseAuthority {
    Authorized,
    Unauthorized,
    Unresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssessedExerciseEvidence {
    pub evidence: ExerciseEvidence,
    pub authority: ExerciseAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExerciseRecordOutcome {
    Added,
    ExactReplay,
    Reassessed,
}

#[derive(Default)]
pub struct ExerciseEvidencePath {
    records: BTreeMap<[u8; 32], AssessedExerciseEvidence>,
}

impl ExerciseEvidencePath {
    pub fn record(&mut self, evidence: AssessedExerciseEvidence) -> ExerciseRecordOutcome {
        let key = evidence.evidence.event_cid().into_bytes();
        match self.records.get(&key) {
            Some(existing) if existing == &evidence => ExerciseRecordOutcome::ExactReplay,
            Some(_) => {
                self.records.insert(key, evidence);
                ExerciseRecordOutcome::Reassessed
            }
            None => {
                self.records.insert(key, evidence);
                ExerciseRecordOutcome::Added
            }
        }
    }

    pub fn authorized(&self) -> Vec<&ExerciseEvidence> {
        self.records
            .values()
            .filter(|record| record.authority == ExerciseAuthority::Authorized)
            .map(|record| &record.evidence)
            .collect()
    }

    pub fn unresolved_event_ids(&self) -> Vec<EventCid> {
        self.records
            .values()
            .filter(|record| record.authority == ExerciseAuthority::Unresolved)
            .map(|record| record.evidence.event_cid())
            .collect()
    }

    pub fn unique_event_count(&self) -> usize {
        self.records.len()
    }
}

fn known_envelope(
    object: &ValidatedKnowledgeObject,
    expected: ObjectKind,
) -> Result<&KnowledgeObjectEnvelope, UseEvidenceError> {
    match object.semantics() {
        ObjectSemantics::Known(envelope)
            if envelope.kind == expected
                && envelope.kind_version.major == USE_EVIDENCE_PROFILE_MAJOR =>
        {
            Ok(envelope)
        }
        _ => Err(UseEvidenceError::WrongPayloadKind),
    }
}

fn canonical_reference_set(
    values: &[ObjectReference],
    limit: usize,
) -> Result<CanonicalValue, UseEvidenceError> {
    if values.len() > limit {
        return Err(UseEvidenceError::Limit);
    }
    let values = values
        .iter()
        .map(ObjectReference::to_value)
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

fn value_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], UseEvidenceError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(UseEvidenceError::InvalidField(field)),
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
) -> Result<&'a CanonicalValue, UseEvidenceError> {
    value_optional(map, key).ok_or(UseEvidenceError::InvalidField(field))
}

fn value_unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, UseEvidenceError> {
    match value_required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(UseEvidenceError::InvalidField(field)),
    }
}

fn value_array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], UseEvidenceError> {
    match value_required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(UseEvidenceError::InvalidField(field)),
    }
}

fn value_bytes16(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 16], UseEvidenceError> {
    let bytes = value_bytes(map, key, field)?;
    if bytes.len() != 16 {
        return Err(UseEvidenceError::InvalidField(field));
    }
    let mut value = [0; 16];
    value.copy_from_slice(bytes);
    Ok(value)
}

fn value_bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], UseEvidenceError> {
    let bytes = value_bytes(map, key, field)?;
    if bytes.len() != 32 {
        return Err(UseEvidenceError::InvalidField(field));
    }
    let mut value = [0; 32];
    value.copy_from_slice(bytes);
    Ok(value)
}

fn value_bytes<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], UseEvidenceError> {
    match value_required(map, key, field)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(UseEvidenceError::InvalidField(field)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UseEvidenceError {
    Canonical(super::canonical::CanonicalError),
    Object(ObjectError),
    InvalidField(&'static str),
    Limit,
    NonCanonicalPayload,
    WrongPayloadKind,
    WrongEventType,
    PayloadReferenceMismatch,
    DisclosureMismatch,
}

impl From<super::canonical::CanonicalError> for UseEvidenceError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for UseEvidenceError {
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
        SignedFeedInception,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn author() -> (SigningKey, crate::foundation::ValidatedFeedInception) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"use-evidence-test", [8; 32]).unwrap(),
            0,
            DeviceId::from_bytes([9; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        (
            key,
            decode_feed_inception(&signed.encode().unwrap()).unwrap(),
        )
    }

    fn signed_event(
        event_type: EventType,
        kind: ObjectKind,
        object: KnowledgeObjectEnvelope,
    ) -> (ValidatedKnowledgeEvent, ValidatedKnowledgeObject) {
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let object = decode_knowledge_object(
            &object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(kind, 1)],
            &[],
        )
        .unwrap();
        let (key, author) = author();
        let mut event = KnowledgeEventEnvelope::new(
            event_type,
            author.feed_id,
            0,
            DisclosureClass::LocalOnly,
            [30; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let bytes = event.sign(&author, &key).unwrap().encode().unwrap().0;
        let event = decode_knowledge_event(
            &bytes,
            &author,
            &[USE_EVIDENCE_EVENT_TYPE, DERIVATION_EVIDENCE_EVENT_TYPE],
        )
        .unwrap();
        (event, object)
    }

    fn use_payload() -> UseEvidencePayload {
        UseEvidencePayload {
            subjects: vec![reference(1)],
            mode: UseMode::ComparedOrOpposed,
            actor_class: concept(2),
            task_context_commitment: [3; 32],
            causal_role: concept(4),
            assembly: Some(reference(5)),
            mapping: Some(MappingKernelCid::from_bytes([6; 32])),
            outcome_observation: None,
            use_policy: reference(7),
            observed_frontier: [8; 32],
        }
    }

    #[test]
    fn signed_use_event_is_deduplicated_by_event_cid() {
        let (event, object) = signed_event(
            USE_EVIDENCE_EVENT_TYPE,
            USE_EVIDENCE_KIND,
            use_payload()
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
        );
        let event = ValidatedUseEvidenceEvent::bind(&event, &object).unwrap();
        let evidence = ExerciseEvidence::Use(event);
        let mut path = ExerciseEvidencePath::default();
        let record = AssessedExerciseEvidence {
            evidence: evidence.clone(),
            authority: ExerciseAuthority::Authorized,
        };
        assert_eq!(path.record(record.clone()), ExerciseRecordOutcome::Added);
        assert_eq!(path.record(record), ExerciseRecordOutcome::ExactReplay);
        assert_eq!(path.unique_event_count(), 1);
        assert_eq!(path.authorized().len(), 1);
        assert!(!evidence.establishes_benefit());
        assert!(!evidence.establishes_truth());
        assert!(!evidence.is_reward_instruction());
    }

    #[test]
    fn authority_is_assessed_separately_from_signature() {
        let (event, object) = signed_event(
            USE_EVIDENCE_EVENT_TYPE,
            USE_EVIDENCE_KIND,
            use_payload()
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
        );
        let evidence =
            ExerciseEvidence::Use(ValidatedUseEvidenceEvent::bind(&event, &object).unwrap());
        let mut path = ExerciseEvidencePath::default();
        path.record(AssessedExerciseEvidence {
            evidence: evidence.clone(),
            authority: ExerciseAuthority::Unauthorized,
        });
        assert!(path.authorized().is_empty());
        assert_eq!(
            path.record(AssessedExerciseEvidence {
                evidence,
                authority: ExerciseAuthority::Authorized,
            }),
            ExerciseRecordOutcome::Reassessed
        );
        assert_eq!(path.authorized().len(), 1);
    }

    #[test]
    fn derivation_event_retains_exact_inputs_output_and_roles() {
        let payload = DerivationEvidencePayload {
            inputs: vec![
                DerivationInput {
                    input: reference(1),
                    causal_role: concept(10),
                },
                DerivationInput {
                    input: reference(2),
                    causal_role: concept(11),
                },
            ],
            output: reference(3),
            derivation_rule: reference(4),
            task_context_commitment: [5; 32],
            derivation_policy: reference(6),
            observed_frontier: [7; 32],
        };
        let (event, object) = signed_event(
            DERIVATION_EVIDENCE_EVENT_TYPE,
            DERIVATION_EVIDENCE_KIND,
            payload
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
        );
        let event = ValidatedDerivationEvidenceEvent::bind(&event, &object).unwrap();
        assert_eq!(event.payload(), &payload);
        let evidence = ExerciseEvidence::Derivation(event);
        assert!(!evidence.establishes_benefit());
        assert!(!evidence.establishes_truth());
    }

    #[test]
    fn query_hit_retrieval_and_exposure_are_not_use_modes() {
        assert_eq!(UseMode::ALL.len(), 13);
        let debug = format!("{:?}", UseMode::ALL);
        for forbidden in ["QueryHit", "Retrieval", "Exposure"] {
            assert!(!debug.contains(forbidden));
        }
    }

    #[test]
    fn event_must_bind_exact_payload_kind_and_disclosure() {
        let (event, object) = signed_event(
            USE_EVIDENCE_EVENT_TYPE,
            USE_EVIDENCE_KIND,
            use_payload()
                .to_knowledge_object(DisclosureClass::LocalOnly)
                .unwrap(),
        );
        assert_eq!(
            ValidatedDerivationEvidenceEvent::bind(&event, &object).unwrap_err(),
            UseEvidenceError::WrongEventType
        );
    }
}
