//! Private SourceArtifact and signed ObservationEvent contracts.
//!
//! Raw observations are immutable local objects intended for the encrypted
//! Private Vault. They are not public KU claims and have no publication path.

use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::content_id::{EventCid, ObjectCid};
use super::event::{EventType, ValidatedKnowledgeEvent};
use super::identity::FeedId;
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};
use super::schema_registry::{
    EVENT_TYPE_OBSERVATION, OBJECT_KIND_OBSERVATION_EVENT_PAYLOAD, OBJECT_KIND_SOURCE_ARTIFACT,
};
use super::semantic::{ConceptCcid, SourceSpan};

pub const SOURCE_ARTIFACT_KIND: ObjectKind = ObjectKind(OBJECT_KIND_SOURCE_ARTIFACT);
pub const OBSERVATION_EVENT_PAYLOAD_KIND: ObjectKind =
    ObjectKind(OBJECT_KIND_OBSERVATION_EVENT_PAYLOAD);
pub const OBSERVATION_EVENT_TYPE: EventType = EventType(EVENT_TYPE_OBSERVATION);
pub const OBSERVATION_PROFILE_MAJOR: u64 = 1;
pub const OBSERVATION_PROFILE_MINOR: u64 = 0;
pub const MAX_RAW_OBSERVATION_BYTES: usize = 786_432;
pub const MAX_OBSERVATION_SPANS: usize = 16_384;
pub const MAX_OBSERVATION_LIMITATIONS: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum SourceArtifactKind {
    Text = 0,
    File = 1,
    Sensor = 2,
}

impl SourceArtifactKind {
    fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Text),
            1 => Some(Self::File),
            2 => Some(Self::Sensor),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationGovernance {
    pub consent_policy: ObjectReference,
    pub consent_receipt: ObjectReference,
    pub revocation_policy: ObjectReference,
    pub retention_policy: ObjectReference,
    pub capture_scope_commitment: [u8; 32],
    pub authorization_assessment_commitment: [u8; 32],
    pub assessed_frontier: [u8; 32],
}

impl ObservationGovernance {
    pub fn validate(&self) -> Result<(), ObservationError> {
        if self.consent_policy.cid == [0; 32]
            || self.consent_receipt.cid == [0; 32]
            || self.revocation_policy.cid == [0; 32]
            || self.retention_policy.cid == [0; 32]
            || self.capture_scope_commitment == [0; 32]
            || self.authorization_assessment_commitment == [0; 32]
            || self.assessed_frontier == [0; 32]
        {
            Err(ObservationError::InvalidGovernance)
        } else {
            Ok(())
        }
    }

    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.consent_policy.to_value()),
            (1, self.consent_receipt.to_value()),
            (2, self.revocation_policy.to_value()),
            (3, self.retention_policy.to_value()),
            (
                4,
                CanonicalValue::Bytes(self.capture_scope_commitment.to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(self.authorization_assessment_commitment.to_vec()),
            ),
            (6, CanonicalValue::Bytes(self.assessed_frontier.to_vec())),
        ])
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, ObservationError> {
        let map = value_map(value, "governance")?;
        let governance = Self {
            consent_policy: ObjectReference::from_value(value_required(map, 0, "consent_policy")?)?,
            consent_receipt: ObjectReference::from_value(value_required(
                map,
                1,
                "consent_receipt",
            )?)?,
            revocation_policy: ObjectReference::from_value(value_required(
                map,
                2,
                "revocation_policy",
            )?)?,
            retention_policy: ObjectReference::from_value(value_required(
                map,
                3,
                "retention_policy",
            )?)?,
            capture_scope_commitment: value_bytes32(map, 4, "capture_scope")?,
            authorization_assessment_commitment: value_bytes32(map, 5, "authorization_assessment")?,
            assessed_frontier: value_bytes32(map, 6, "assessed_frontier")?,
        };
        governance.validate()?;
        Ok(governance)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceArtifact {
    pub source_kind: SourceArtifactKind,
    pub raw_bytes: Vec<u8>,
    pub media_type_commitment: [u8; 32],
    pub capture_adapter: ObjectReference,
    pub capture_sequence: u64,
    pub governance: ObservationGovernance,
}

impl SourceArtifact {
    pub fn validate(&self) -> Result<(), ObservationError> {
        self.governance.validate()?;
        if self.raw_bytes.is_empty()
            || self.raw_bytes.len() > MAX_RAW_OBSERVATION_BYTES
            || self.media_type_commitment == [0; 32]
            || self.capture_adapter.cid == [0; 32]
        {
            return Err(ObservationError::InvalidSourceArtifact);
        }
        Ok(())
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, ObservationError> {
        self.validate()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(OBSERVATION_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(OBSERVATION_PROFILE_MINOR)),
            (2, CanonicalValue::Unsigned(self.source_kind as u64)),
            (3, CanonicalValue::Bytes(self.raw_bytes.clone())),
            (
                4,
                CanonicalValue::Bytes(self.media_type_commitment.to_vec()),
            ),
            (5, self.capture_adapter.to_value()),
            (6, CanonicalValue::Unsigned(self.capture_sequence)),
            (7, self.governance.to_value()),
        ]))
    }

    pub fn to_private_object(&self) -> Result<KnowledgeObjectEnvelope, ObservationError> {
        let mut object = KnowledgeObjectEnvelope::new(
            SOURCE_ARTIFACT_KIND,
            SchemaVersion::new(OBSERVATION_PROFILE_MAJOR, OBSERVATION_PROFILE_MINOR),
            DisclosureClass::LocalOnly,
            self.canonical_payload()?,
        );
        object.references = vec![
            self.capture_adapter.clone(),
            self.governance.consent_policy.clone(),
            self.governance.consent_receipt.clone(),
            self.governance.revocation_policy.clone(),
            self.governance.retention_policy.clone(),
        ];
        Ok(object)
    }

    pub fn from_validated(object: &ValidatedKnowledgeObject) -> Result<Self, ObservationError> {
        let envelope = known_envelope(object, SOURCE_ARTIFACT_KIND)?;
        if envelope.disclosure != DisclosureClass::LocalOnly {
            return Err(ObservationError::MustRemainLocalOnly);
        }
        let map = value_map(&envelope.payload, "source_artifact")?;
        require_version(map)?;
        let artifact = Self {
            source_kind: SourceArtifactKind::from_code(value_unsigned(map, 2, "source_kind")?)
                .ok_or(ObservationError::InvalidField("source_kind"))?,
            raw_bytes: value_bytes(map, 3, "raw_bytes")?.to_vec(),
            media_type_commitment: value_bytes32(map, 4, "media_type")?,
            capture_adapter: ObjectReference::from_value(value_required(
                map,
                5,
                "capture_adapter",
            )?)?,
            capture_sequence: value_unsigned(map, 6, "capture_sequence")?,
            governance: ObservationGovernance::from_value(value_required(map, 7, "governance")?)?,
        };
        artifact.validate()?;
        if artifact.canonical_payload()? != envelope.payload {
            return Err(ObservationError::NonCanonicalPayload);
        }
        Ok(artifact)
    }

    pub const fn has_publication_path(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationEventPayload {
    pub source_artifact: ObjectReference,
    pub observation_kind: ConceptCcid,
    pub source_spans: Vec<SourceSpan>,
    pub extractor: ObjectReference,
    pub limitations: Vec<ConceptCcid>,
    pub governance: ObservationGovernance,
    pub observed_frontier: [u8; 32],
}

impl ObservationEventPayload {
    pub fn validate(&self) -> Result<(), ObservationError> {
        self.governance.validate()?;
        if self.source_artifact.reference_kind != SOURCE_ARTIFACT_KIND.0
            || self.source_artifact.cid == [0; 32]
            || self.extractor.cid == [0; 32]
            || self.observed_frontier == [0; 32]
            || self.source_spans.is_empty()
            || self.source_spans.len() > MAX_OBSERVATION_SPANS
            || self.limitations.len() > MAX_OBSERVATION_LIMITATIONS
            || self
                .source_spans
                .iter()
                .any(|span| span.source != self.source_artifact || span.start >= span.end)
        {
            return Err(ObservationError::InvalidObservation);
        }
        canonical_spans(&self.source_spans)?;
        canonical_concepts(&self.limitations)?;
        Ok(())
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, ObservationError> {
        self.validate()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(OBSERVATION_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(OBSERVATION_PROFILE_MINOR)),
            (2, self.source_artifact.to_value()),
            (
                3,
                CanonicalValue::Bytes(self.observation_kind.as_bytes().to_vec()),
            ),
            (4, canonical_spans(&self.source_spans)?),
            (5, self.extractor.to_value()),
            (6, canonical_concepts(&self.limitations)?),
            (7, self.governance.to_value()),
            (8, CanonicalValue::Bytes(self.observed_frontier.to_vec())),
        ]))
    }

    pub fn to_private_object(&self) -> Result<KnowledgeObjectEnvelope, ObservationError> {
        let mut object = KnowledgeObjectEnvelope::new(
            OBSERVATION_EVENT_PAYLOAD_KIND,
            SchemaVersion::new(OBSERVATION_PROFILE_MAJOR, OBSERVATION_PROFILE_MINOR),
            DisclosureClass::LocalOnly,
            self.canonical_payload()?,
        );
        object.references = vec![
            self.source_artifact.clone(),
            self.extractor.clone(),
            self.governance.consent_policy.clone(),
            self.governance.consent_receipt.clone(),
            self.governance.revocation_policy.clone(),
            self.governance.retention_policy.clone(),
        ];
        Ok(object)
    }

    fn from_validated(object: &ValidatedKnowledgeObject) -> Result<Self, ObservationError> {
        let envelope = known_envelope(object, OBSERVATION_EVENT_PAYLOAD_KIND)?;
        if envelope.disclosure != DisclosureClass::LocalOnly {
            return Err(ObservationError::MustRemainLocalOnly);
        }
        let map = value_map(&envelope.payload, "observation")?;
        require_version(map)?;
        let payload = Self {
            source_artifact: ObjectReference::from_value(value_required(
                map,
                2,
                "source_artifact",
            )?)?,
            observation_kind: ConceptCcid::from_bytes(value_bytes16(map, 3, "observation_kind")?),
            source_spans: parse_spans(value_required(map, 4, "source_spans")?)?,
            extractor: ObjectReference::from_value(value_required(map, 5, "extractor")?)?,
            limitations: parse_concepts(value_required(map, 6, "limitations")?)?,
            governance: ObservationGovernance::from_value(value_required(map, 7, "governance")?)?,
            observed_frontier: value_bytes32(map, 8, "observed_frontier")?,
        };
        payload.validate()?;
        if payload.canonical_payload()? != envelope.payload {
            return Err(ObservationError::NonCanonicalPayload);
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedObservationEvent {
    event_cid: EventCid,
    author_feed: FeedId,
    author_sequence: u64,
    payload_object: ObjectCid,
    payload: ObservationEventPayload,
}

impl ValidatedObservationEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, ObservationError> {
        if event.signed.event.event_type != OBSERVATION_EVENT_TYPE {
            return Err(ObservationError::WrongEventType);
        }
        if event.signed.event.disclosure != DisclosureClass::LocalOnly
            || payload_object.disclosure() != DisclosureClass::LocalOnly
        {
            return Err(ObservationError::MustRemainLocalOnly);
        }
        let expected = ObjectReference::new(0, payload_object.cid().into_bytes());
        if event.signed.event.payload_refs != [expected] {
            return Err(ObservationError::PayloadReferenceMismatch);
        }
        Ok(Self {
            event_cid: event.cid(),
            author_feed: event.signed.event.author_feed,
            author_sequence: event.signed.event.author_sequence,
            payload_object: payload_object.cid(),
            payload: ObservationEventPayload::from_validated(payload_object)?,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn author_feed(&self) -> FeedId {
        self.author_feed
    }

    pub const fn author_sequence(&self) -> u64 {
        self.author_sequence
    }

    pub const fn payload_object_cid(&self) -> ObjectCid {
        self.payload_object
    }

    pub const fn payload(&self) -> &ObservationEventPayload {
        &self.payload
    }

    pub const fn is_use_or_benefit_evidence(&self) -> bool {
        false
    }
}

fn known_envelope(
    object: &ValidatedKnowledgeObject,
    expected: ObjectKind,
) -> Result<&KnowledgeObjectEnvelope, ObservationError> {
    match object.semantics() {
        ObjectSemantics::Known(envelope)
            if envelope.kind == expected
                && envelope.kind_version.major == OBSERVATION_PROFILE_MAJOR =>
        {
            Ok(envelope)
        }
        _ => Err(ObservationError::WrongObjectKind),
    }
}

fn canonical_spans(spans: &[SourceSpan]) -> Result<CanonicalValue, ObservationError> {
    let members = spans
        .iter()
        .map(|span| {
            CanonicalValue::Map(vec![
                (0, span.source.to_value()),
                (1, CanonicalValue::Unsigned(span.start)),
                (2, CanonicalValue::Unsigned(span.end)),
            ])
        })
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn parse_spans(value: &CanonicalValue) -> Result<Vec<SourceSpan>, ObservationError> {
    let values = value_array(value, "source_spans")?;
    values
        .iter()
        .map(|value| {
            let map = value_map(value, "source_span")?;
            Ok(SourceSpan {
                source: ObjectReference::from_value(value_required(map, 0, "span.source")?)?,
                start: value_unsigned(map, 1, "span.start")?,
                end: value_unsigned(map, 2, "span.end")?,
            })
        })
        .collect()
}

fn canonical_concepts(concepts: &[ConceptCcid]) -> Result<CanonicalValue, ObservationError> {
    let members = concepts
        .iter()
        .map(|concept| CanonicalValue::Bytes(concept.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn parse_concepts(value: &CanonicalValue) -> Result<Vec<ConceptCcid>, ObservationError> {
    value_array(value, "limitations")?
        .iter()
        .map(|value| match value {
            CanonicalValue::Bytes(bytes) if bytes.len() == 16 => {
                let mut output = [0; 16];
                output.copy_from_slice(bytes);
                Ok(ConceptCcid::from_bytes(output))
            }
            _ => Err(ObservationError::InvalidField("limitation")),
        })
        .collect()
}

fn require_version(map: &[(u64, CanonicalValue)]) -> Result<(), ObservationError> {
    if value_unsigned(map, 0, "major")? != OBSERVATION_PROFILE_MAJOR
        || value_unsigned(map, 1, "minor")? != OBSERVATION_PROFILE_MINOR
    {
        Err(ObservationError::UnsupportedVersion)
    } else {
        Ok(())
    }
}

fn value_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ObservationError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(ObservationError::InvalidField(field)),
    }
}

fn value_array<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ObservationError> {
    match value {
        CanonicalValue::Array(values) => Ok(values),
        _ => Err(ObservationError::InvalidField(field)),
    }
}

fn value_required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ObservationError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ObservationError::InvalidField(field))
}

fn value_unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ObservationError> {
    match value_required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ObservationError::InvalidField(field)),
    }
}

fn value_bytes<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], ObservationError> {
    match value_required(map, key, field)? {
        CanonicalValue::Bytes(bytes) => Ok(bytes),
        _ => Err(ObservationError::InvalidField(field)),
    }
}

fn value_bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ObservationError> {
    let bytes = value_bytes(map, key, field)?;
    if bytes.len() != 32 {
        return Err(ObservationError::InvalidField(field));
    }
    let mut output = [0; 32];
    output.copy_from_slice(bytes);
    Ok(output)
}

fn value_bytes16(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 16], ObservationError> {
    let bytes = value_bytes(map, key, field)?;
    if bytes.len() != 16 {
        return Err(ObservationError::InvalidField(field));
    }
    let mut output = [0; 16];
    output.copy_from_slice(bytes);
    Ok(output)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObservationError {
    Canonical(super::canonical::CanonicalError),
    Object(ObjectError),
    InvalidGovernance,
    InvalidSourceArtifact,
    InvalidObservation,
    InvalidField(&'static str),
    MustRemainLocalOnly,
    UnsupportedVersion,
    WrongObjectKind,
    WrongEventType,
    PayloadReferenceMismatch,
    NonCanonicalPayload,
}

impl From<super::canonical::CanonicalError> for ObservationError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for ObservationError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}
