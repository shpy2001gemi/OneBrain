//! Encoding-fidelity evidence contracts.
//!
//! Fidelity asks whether a named encoding represents a named source with the
//! stated gene/concept/span limitations. It never decides proposition truth,
//! realized value or whether a KU may be preserved, queried or used.

use std::collections::{BTreeMap, BTreeSet};

use super::canonical::{canonicalize_set_by_key, CanonicalError, CanonicalValue, ResourceProfile};
use super::content_id::{EventCid, ObjectCid};
use super::event::{EventType, ValidatedKnowledgeEvent};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectKind, ObjectReference,
    ObjectSemantics, SchemaVersion, ValidatedKnowledgeObject,
};
use super::schema_registry::{
    EVENT_TYPE_ENCODING_FIDELITY_ATTESTATION, OBJECT_KIND_ENCODING_ATTEMPT,
    OBJECT_KIND_ENCODING_FIDELITY_ATTESTATION, OBJECT_KIND_FIDELITY_POLICY,
};
use super::semantic::ConceptCcid;

pub const FIDELITY_PROFILE_MAJOR: u64 = 1;
pub const FIDELITY_PROFILE_MINOR: u64 = 0;
pub const MAX_FIDELITY_MEMBERS: usize = 4_096;
pub const ENCODING_ATTEMPT_KIND: ObjectKind = ObjectKind(OBJECT_KIND_ENCODING_ATTEMPT);
pub const FIDELITY_POLICY_KIND: ObjectKind = ObjectKind(OBJECT_KIND_FIDELITY_POLICY);
pub const ENCODING_FIDELITY_ATTESTATION_KIND: ObjectKind =
    ObjectKind(OBJECT_KIND_ENCODING_FIDELITY_ATTESTATION);
pub const ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE: EventType =
    EventType(EVENT_TYPE_ENCODING_FIDELITY_ATTESTATION);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum EncodingAttemptRole {
    Publisher = 0,
    ExternalBlind = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodingAttempt {
    pub role: EncodingAttemptRole,
    pub source_artifact: ObjectReference,
    /// External blind attempts cannot carry the candidate they will later
    /// compare against. FID-002 owns the commit-before-reveal transcript.
    pub candidate_encoding: Option<ObjectReference>,
    pub output_commitment: [u8; 32],
    pub pipeline_model_tool_commitments: Vec<[u8; 32]>,
    pub source_acquisition_or_derivation_commitment: [u8; 32],
    pub execution_record_ref: ObjectReference,
    pub blind_session_commitment: Option<[u8; 32]>,
    pub challenge_nonce_commitment: Option<[u8; 32]>,
}

impl EncodingAttempt {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, FidelityError> {
        self.validate()?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(FIDELITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(FIDELITY_PROFILE_MINOR)),
            (2, CanonicalValue::Unsigned(self.role as u64)),
            (3, self.source_artifact.to_value()),
            (5, CanonicalValue::Bytes(self.output_commitment.to_vec())),
            (
                6,
                canonical_bytes32_set(&self.pipeline_model_tool_commitments)?,
            ),
            (
                7,
                CanonicalValue::Bytes(self.source_acquisition_or_derivation_commitment.to_vec()),
            ),
            (8, self.execution_record_ref.to_value()),
        ];
        if let Some(candidate) = &self.candidate_encoding {
            fields.push((4, candidate.to_value()));
        }
        if let Some(commitment) = self.blind_session_commitment {
            fields.push((9, CanonicalValue::Bytes(commitment.to_vec())));
        }
        if let Some(commitment) = self.challenge_nonce_commitment {
            fields.push((10, CanonicalValue::Bytes(commitment.to_vec())));
        }
        fields.sort_by_key(|(key, _)| *key);
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, FidelityError> {
        let mut object = KnowledgeObjectEnvelope::new(
            ENCODING_ATTEMPT_KIND,
            SchemaVersion::new(FIDELITY_PROFILE_MAJOR, FIDELITY_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = vec![
            self.source_artifact.clone(),
            self.execution_record_ref.clone(),
        ];
        object.references.extend(self.candidate_encoding.clone());
        Ok(object)
    }

    pub const fn establishes_truth(&self) -> bool {
        false
    }

    pub const fn establishes_independence(&self) -> bool {
        false
    }

    fn validate(&self) -> Result<(), FidelityError> {
        if self.output_commitment == [0; 32]
            || self.source_acquisition_or_derivation_commitment == [0; 32]
            || self.pipeline_model_tool_commitments.is_empty()
            || self.pipeline_model_tool_commitments.len() > MAX_FIDELITY_MEMBERS
            || has_duplicates(&self.pipeline_model_tool_commitments)
        {
            return Err(FidelityError::InvalidField("encoding_attempt"));
        }
        match self.role {
            EncodingAttemptRole::Publisher if self.candidate_encoding.is_none() => {
                Err(FidelityError::InvalidField("publisher_candidate"))
            }
            EncodingAttemptRole::ExternalBlind
                if self.candidate_encoding.is_some()
                    || self.blind_session_commitment.is_none()
                    || self.challenge_nonce_commitment.is_none() =>
            {
                Err(FidelityError::InvalidField("blind_attempt_boundary"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum CorrelationDimension {
    AdministrativePrincipal = 0,
    DeviceOrFeed = 1,
    PipelineModelLineage = 2,
    PromptTemplate = 3,
    Preprocessing = 4,
    SourceAcquisitionOrDerivation = 5,
    ExecutionEnvironment = 6,
    BlindSession = 7,
    ChallengeNonce = 8,
}

impl CorrelationDimension {
    fn from_code(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::AdministrativePrincipal),
            1 => Some(Self::DeviceOrFeed),
            2 => Some(Self::PipelineModelLineage),
            3 => Some(Self::PromptTemplate),
            4 => Some(Self::Preprocessing),
            5 => Some(Self::SourceAcquisitionOrDerivation),
            6 => Some(Self::ExecutionEnvironment),
            7 => Some(Self::BlindSession),
            8 => Some(Self::ChallengeNonce),
            _ => None,
        }
    }
}

/// Strengths are categorical, not a scalar confidence ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum EvidenceStrength {
    Unknown = 0,
    SelfClaimed = 1,
    CryptoBound = 2,
    ExternallyAttested = 3,
    EmpiricallyEstimated = 4,
}

impl EvidenceStrength {
    fn from_code(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::Unknown),
            1 => Some(Self::SelfClaimed),
            2 => Some(Self::CryptoBound),
            3 => Some(Self::ExternallyAttested),
            4 => Some(Self::EmpiricallyEstimated),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorrelationDimensionEvidence {
    pub dimension: CorrelationDimension,
    pub value_commitment: Option<[u8; 32]>,
    pub strength: EvidenceStrength,
    pub evidence_refs: Vec<ObjectReference>,
}

impl CorrelationDimensionEvidence {
    fn canonical_value(&self) -> Result<CanonicalValue, FidelityError> {
        if self.evidence_refs.len() > MAX_FIDELITY_MEMBERS
            || (self.strength != EvidenceStrength::Unknown && self.value_commitment.is_none())
        {
            return Err(FidelityError::InvalidField("correlation_dimension"));
        }
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(self.dimension as u64)),
            (1, CanonicalValue::Unsigned(self.strength as u64)),
            (3, canonical_reference_set(&self.evidence_refs)?),
        ];
        if let Some(value) = self.value_commitment {
            if value == [0; 32] {
                return Err(FidelityError::InvalidField("correlation_commitment"));
            }
            fields.push((2, CanonicalValue::Bytes(value.to_vec())));
            fields.sort_by_key(|(key, _)| *key);
        }
        Ok(CanonicalValue::Map(fields))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, FidelityError> {
        let map = value_map(value, "correlation_dimension")?;
        let dimension =
            CorrelationDimension::from_code(value_unsigned(map, 0, "correlation.dimension")?)
                .ok_or(FidelityError::InvalidField("correlation.dimension"))?;
        let strength = EvidenceStrength::from_code(value_unsigned(map, 1, "correlation.strength")?)
            .ok_or(FidelityError::InvalidField("correlation.strength"))?;
        let value_commitment = value_optional(map, 2)
            .map(|value| value_bytes32_direct(value, "correlation.commitment"))
            .transpose()?;
        let evidence_refs = value_array(map, 3, "correlation.refs")?
            .iter()
            .map(ObjectReference::from_value)
            .collect::<Result<Vec<_>, _>>()?;
        let dimension = Self {
            dimension,
            value_commitment,
            strength,
            evidence_refs,
        };
        if dimension.canonical_value()? != *value {
            return Err(FidelityError::NonCanonicalPayload);
        }
        Ok(dimension)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorrelationEvidence {
    pub dimensions: Vec<CorrelationDimensionEvidence>,
}

impl CorrelationEvidence {
    pub fn canonical_value(&self) -> Result<CanonicalValue, FidelityError> {
        if self.dimensions.is_empty()
            || self.dimensions.len() > MAX_FIDELITY_MEMBERS
            || has_duplicates(
                &self
                    .dimensions
                    .iter()
                    .map(|item| item.dimension)
                    .collect::<Vec<_>>(),
            )
        {
            return Err(FidelityError::InvalidField("correlation_evidence"));
        }
        let values = self
            .dimensions
            .iter()
            .map(CorrelationDimensionEvidence::canonical_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CanonicalValue::Array(canonicalize_set_by_key(
            values
                .into_iter()
                .map(|value| (value.clone(), value))
                .collect(),
            ResourceProfile::ObjectV1,
        )?))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, FidelityError> {
        let CanonicalValue::Array(values) = value else {
            return Err(FidelityError::InvalidField("correlation_evidence"));
        };
        let evidence = Self {
            dimensions: values
                .iter()
                .map(CorrelationDimensionEvidence::from_value)
                .collect::<Result<Vec<_>, _>>()?,
        };
        if evidence.canonical_value()? != *value {
            return Err(FidelityError::NonCanonicalPayload);
        }
        Ok(evidence)
    }

    pub fn dimension(
        &self,
        dimension: CorrelationDimension,
    ) -> Option<&CorrelationDimensionEvidence> {
        self.dimensions
            .iter()
            .find(|evidence| evidence.dimension == dimension)
    }

    pub const fn contains_independent_boolean_or_score(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum FidelityCheckKind {
    SourceSpanAlignment = 0,
    GeneSelection = 1,
    ConceptSelection = 2,
}

impl FidelityCheckKind {
    fn from_code(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::SourceSpanAlignment),
            1 => Some(Self::GeneSelection),
            2 => Some(Self::ConceptSelection),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum FidelityCheckStatus {
    ConsistentWithSource = 0,
    HardEncodingMismatch = 1,
    Unresolved = 2,
    NotApplicable = 3,
}

impl FidelityCheckStatus {
    fn from_code(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::ConsistentWithSource),
            1 => Some(Self::HardEncodingMismatch),
            2 => Some(Self::Unresolved),
            3 => Some(Self::NotApplicable),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityCheck {
    pub kind: FidelityCheckKind,
    pub status: FidelityCheckStatus,
    pub checked_region_commitment: [u8; 32],
    pub evidence_ref: Option<ObjectReference>,
}

impl FidelityCheck {
    fn canonical_value(&self) -> Result<CanonicalValue, FidelityError> {
        if self.checked_region_commitment == [0; 32] {
            return Err(FidelityError::InvalidField("checked_region"));
        }
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(self.kind as u64)),
            (1, CanonicalValue::Unsigned(self.status as u64)),
            (
                2,
                CanonicalValue::Bytes(self.checked_region_commitment.to_vec()),
            ),
        ];
        if let Some(reference) = &self.evidence_ref {
            fields.push((3, reference.to_value()));
        }
        Ok(CanonicalValue::Map(fields))
    }

    fn from_value(value: &CanonicalValue) -> Result<Self, FidelityError> {
        let map = value_map(value, "fidelity_check")?;
        let check = Self {
            kind: FidelityCheckKind::from_code(value_unsigned(map, 0, "check.kind")?)
                .ok_or(FidelityError::InvalidField("check.kind"))?,
            status: FidelityCheckStatus::from_code(value_unsigned(map, 1, "check.status")?)
                .ok_or(FidelityError::InvalidField("check.status"))?,
            checked_region_commitment: value_bytes32(map, 2, "check.region")?,
            evidence_ref: value_optional(map, 3)
                .map(ObjectReference::from_value)
                .transpose()?,
        };
        if check.canonical_value()? != *value {
            return Err(FidelityError::NonCanonicalPayload);
        }
        Ok(check)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodingFidelityAttestation {
    pub source_artifact: ObjectReference,
    pub candidate_encoding: ObjectReference,
    pub blind_attempt_output_commitment: [u8; 32],
    pub attempt_ref: ObjectReference,
    pub execution_record_ref: ObjectReference,
    pub correlation_evidence: CorrelationEvidence,
    pub checks: Vec<FidelityCheck>,
    pub limitations: Vec<ConceptCcid>,
    pub policy_ref: ObjectReference,
}

impl EncodingFidelityAttestation {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, FidelityError> {
        self.validate()?;
        let checks = self
            .checks
            .iter()
            .map(FidelityCheck::canonical_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(FIDELITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(FIDELITY_PROFILE_MINOR)),
            (2, self.source_artifact.to_value()),
            (3, self.candidate_encoding.to_value()),
            (
                4,
                CanonicalValue::Bytes(self.blind_attempt_output_commitment.to_vec()),
            ),
            (5, self.attempt_ref.to_value()),
            (6, self.execution_record_ref.to_value()),
            (7, self.correlation_evidence.canonical_value()?),
            (
                8,
                CanonicalValue::Array(canonicalize_set_by_key(
                    checks
                        .into_iter()
                        .map(|value| (value.clone(), value))
                        .collect(),
                    ResourceProfile::ObjectV1,
                )?),
            ),
            (9, canonical_ccid_set(&self.limitations)?),
            (10, self.policy_ref.to_value()),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, FidelityError> {
        let mut object = KnowledgeObjectEnvelope::new(
            ENCODING_FIDELITY_ATTESTATION_KIND,
            SchemaVersion::new(FIDELITY_PROFILE_MAJOR, FIDELITY_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        );
        object.references = vec![
            self.source_artifact.clone(),
            self.candidate_encoding.clone(),
            self.attempt_ref.clone(),
            self.execution_record_ref.clone(),
            self.policy_ref.clone(),
        ];
        object.references.extend(
            self.correlation_evidence
                .dimensions
                .iter()
                .flat_map(|dimension| dimension.evidence_refs.iter().cloned()),
        );
        object.references.extend(
            self.checks
                .iter()
                .filter_map(|check| check.evidence_ref.clone()),
        );
        Ok(object)
    }

    fn from_object(object: &ValidatedKnowledgeObject) -> Result<Self, FidelityError> {
        let envelope = known_envelope(object, ENCODING_FIDELITY_ATTESTATION_KIND)?;
        let map = value_map(&envelope.payload, "fidelity_attestation")?;
        let attestation = Self {
            source_artifact: ObjectReference::from_value(value_required(
                map,
                2,
                "attestation.source",
            )?)?,
            candidate_encoding: ObjectReference::from_value(value_required(
                map,
                3,
                "attestation.encoding",
            )?)?,
            blind_attempt_output_commitment: value_bytes32(map, 4, "attestation.output")?,
            attempt_ref: ObjectReference::from_value(value_required(
                map,
                5,
                "attestation.attempt",
            )?)?,
            execution_record_ref: ObjectReference::from_value(value_required(
                map,
                6,
                "attestation.execution",
            )?)?,
            correlation_evidence: CorrelationEvidence::from_value(value_required(
                map,
                7,
                "attestation.correlation",
            )?)?,
            checks: value_array(map, 8, "attestation.checks")?
                .iter()
                .map(FidelityCheck::from_value)
                .collect::<Result<Vec<_>, _>>()?,
            limitations: value_array(map, 9, "attestation.limitations")?
                .iter()
                .map(|value| {
                    value_bytes16_direct(value, "attestation.limitation")
                        .map(ConceptCcid::from_bytes)
                })
                .collect::<Result<Vec<_>, _>>()?,
            policy_ref: ObjectReference::from_value(value_required(
                map,
                10,
                "attestation.policy",
            )?)?,
        };
        attestation.validate()?;
        if attestation.canonical_payload()? != envelope.payload {
            return Err(FidelityError::NonCanonicalPayload);
        }
        Ok(attestation)
    }

    fn validate(&self) -> Result<(), FidelityError> {
        if self.blind_attempt_output_commitment == [0; 32]
            || self.checks.is_empty()
            || self.checks.len() > MAX_FIDELITY_MEMBERS
            || self.limitations.len() > MAX_FIDELITY_MEMBERS
            || has_duplicates(
                &self
                    .checks
                    .iter()
                    .map(|check| check.kind)
                    .collect::<Vec<_>>(),
            )
            || has_duplicates(&self.limitations)
        {
            return Err(FidelityError::InvalidField("fidelity_attestation"));
        }
        self.correlation_evidence.canonical_value()?;
        Ok(())
    }

    pub fn has_hard_encoding_mismatch(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == FidelityCheckStatus::HardEncodingMismatch)
    }

    pub const fn establishes_proposition_truth(&self) -> bool {
        false
    }

    pub const fn classifies_knowledge_as_wrong(&self) -> bool {
        false
    }

    pub const fn blocks_preserve_publish_query_or_use(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityPolicy {
    pub publisher_attempt_required: bool,
    pub minimum_external_blind_attempts: u16,
    pub minimum_evidenced_distinct_external_groups: u16,
    pub required_distinct_dimensions: Vec<CorrelationDimension>,
    pub accepted_strengths_by_dimension: Vec<(CorrelationDimension, Vec<EvidenceStrength>)>,
    pub required_checks: Vec<FidelityCheckKind>,
}

impl FidelityPolicy {
    pub fn default_v1() -> Self {
        let accepted = vec![
            EvidenceStrength::CryptoBound,
            EvidenceStrength::ExternallyAttested,
            EvidenceStrength::EmpiricallyEstimated,
        ];
        Self {
            publisher_attempt_required: true,
            minimum_external_blind_attempts: 2,
            minimum_evidenced_distinct_external_groups: 2,
            required_distinct_dimensions: vec![
                CorrelationDimension::AdministrativePrincipal,
                CorrelationDimension::PipelineModelLineage,
            ],
            accepted_strengths_by_dimension: vec![
                (
                    CorrelationDimension::AdministrativePrincipal,
                    accepted.clone(),
                ),
                (CorrelationDimension::PipelineModelLineage, accepted),
            ],
            required_checks: vec![
                FidelityCheckKind::SourceSpanAlignment,
                FidelityCheckKind::GeneSelection,
                FidelityCheckKind::ConceptSelection,
            ],
        }
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, FidelityError> {
        self.validate()?;
        let strength_rules = self
            .accepted_strengths_by_dimension
            .iter()
            .map(|(dimension, strengths)| {
                Ok(CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(*dimension as u64)),
                    (
                        1,
                        canonical_unsigned_set(strengths.iter().map(|strength| *strength as u64))?,
                    ),
                ]))
            })
            .collect::<Result<Vec<_>, FidelityError>>()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(FIDELITY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(FIDELITY_PROFILE_MINOR)),
            (2, CanonicalValue::Bool(self.publisher_attempt_required)),
            (
                3,
                CanonicalValue::Unsigned(u64::from(self.minimum_external_blind_attempts)),
            ),
            (
                4,
                canonical_unsigned_set(
                    self.required_distinct_dimensions
                        .iter()
                        .map(|dimension| *dimension as u64),
                )?,
            ),
            (
                5,
                CanonicalValue::Array(canonicalize_set_by_key(
                    strength_rules
                        .into_iter()
                        .map(|value| (value.clone(), value))
                        .collect(),
                    ResourceProfile::ObjectV1,
                )?),
            ),
            (
                6,
                canonical_unsigned_set(self.required_checks.iter().map(|check| *check as u64))?,
            ),
            (
                7,
                CanonicalValue::Unsigned(u64::from(
                    self.minimum_evidenced_distinct_external_groups,
                )),
            ),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, FidelityError> {
        Ok(KnowledgeObjectEnvelope::new(
            FIDELITY_POLICY_KIND,
            SchemaVersion::new(FIDELITY_PROFILE_MAJOR, FIDELITY_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        ))
    }

    /// Returns a policy-derived grouping key only when every required
    /// dimension has an accepted evidence category and a non-zero commitment.
    /// The existence of a key is not a boolean claim of cognitive independence.
    pub fn evidenced_group_key(
        &self,
        evidence: &CorrelationEvidence,
    ) -> Result<Option<[u8; 32]>, FidelityError> {
        self.validate()?;
        evidence.canonical_value()?;
        let accepted: BTreeMap<_, _> = self
            .accepted_strengths_by_dimension
            .iter()
            .map(|(dimension, strengths)| (*dimension, strengths))
            .collect();
        let mut selected = Vec::new();
        for dimension in &self.required_distinct_dimensions {
            let Some(item) = evidence.dimension(*dimension) else {
                return Ok(None);
            };
            let Some(commitment) = item.value_commitment else {
                return Ok(None);
            };
            if !accepted
                .get(dimension)
                .is_some_and(|strengths| strengths.contains(&item.strength))
            {
                return Ok(None);
            }
            selected.push(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(*dimension as u64)),
                (1, CanonicalValue::Bytes(commitment.to_vec())),
                (2, CanonicalValue::Unsigned(item.strength as u64)),
            ]));
        }
        let bytes = super::canonical::encode_canonical(
            &CanonicalValue::Array(selected),
            ResourceProfile::ObjectV1,
        )?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:fidelity-correlation-group:1\0");
        hasher.update(&bytes);
        Ok(Some(*hasher.finalize().as_bytes()))
    }

    pub const fn uses_node_count_or_self_claim_as_independence(&self) -> bool {
        false
    }

    fn validate(&self) -> Result<(), FidelityError> {
        if self.minimum_external_blind_attempts < 2
            || self.minimum_evidenced_distinct_external_groups < 2
            || self.required_distinct_dimensions.is_empty()
            || self.required_checks.is_empty()
            || has_duplicates(&self.required_distinct_dimensions)
            || has_duplicates(&self.required_checks)
            || self.accepted_strengths_by_dimension.len() != self.required_distinct_dimensions.len()
        {
            return Err(FidelityError::InvalidField("fidelity_policy"));
        }
        for dimension in &self.required_distinct_dimensions {
            let Some((_, strengths)) = self
                .accepted_strengths_by_dimension
                .iter()
                .find(|(candidate, _)| candidate == dimension)
            else {
                return Err(FidelityError::InvalidField("fidelity_strength_rule"));
            };
            if strengths.is_empty()
                || has_duplicate_strengths(strengths)
                || strengths.contains(&EvidenceStrength::Unknown)
                || strengths.contains(&EvidenceStrength::SelfClaimed)
            {
                return Err(FidelityError::InvalidField("fidelity_strength_rule"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedEncodingFidelityAttestationEvent {
    event_cid: EventCid,
    payload_object_cid: ObjectCid,
    attestation: EncodingFidelityAttestation,
}

impl ValidatedEncodingFidelityAttestationEvent {
    pub fn bind(
        event: &ValidatedKnowledgeEvent,
        payload_object: &ValidatedKnowledgeObject,
    ) -> Result<Self, FidelityError> {
        if event.signed.event.event_type != ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE {
            return Err(FidelityError::WrongEventType);
        }
        if event.signed.event.disclosure != payload_object.disclosure() {
            return Err(FidelityError::DisclosureMismatch);
        }
        let expected = ObjectReference::new(0, payload_object.cid().into_bytes());
        if event.signed.event.payload_refs != [expected] {
            return Err(FidelityError::PayloadReferenceMismatch);
        }
        Ok(Self {
            event_cid: event.cid(),
            payload_object_cid: payload_object.cid(),
            attestation: EncodingFidelityAttestation::from_object(payload_object)?,
        })
    }

    pub const fn event_cid(&self) -> EventCid {
        self.event_cid
    }

    pub const fn payload_object_cid(&self) -> ObjectCid {
        self.payload_object_cid
    }

    pub const fn attestation(&self) -> &EncodingFidelityAttestation {
        &self.attestation
    }

    pub const fn establishes_proposition_truth(&self) -> bool {
        false
    }
}

fn known_envelope(
    object: &ValidatedKnowledgeObject,
    expected: ObjectKind,
) -> Result<&KnowledgeObjectEnvelope, FidelityError> {
    match object.semantics() {
        ObjectSemantics::Known(envelope)
            if envelope.kind == expected
                && envelope.kind_version.major == FIDELITY_PROFILE_MAJOR =>
        {
            Ok(envelope)
        }
        _ => Err(FidelityError::WrongPayloadKind),
    }
}

fn canonical_reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, FidelityError> {
    if values.len() > MAX_FIDELITY_MEMBERS {
        return Err(FidelityError::Limit);
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

fn canonical_bytes32_set(values: &[[u8; 32]]) -> Result<CanonicalValue, FidelityError> {
    if values.len() > MAX_FIDELITY_MEMBERS {
        return Err(FidelityError::Limit);
    }
    let values = values
        .iter()
        .map(|value| CanonicalValue::Bytes(value.to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

fn canonical_ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, FidelityError> {
    if values.len() > MAX_FIDELITY_MEMBERS {
        return Err(FidelityError::Limit);
    }
    let values = values
        .iter()
        .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

fn canonical_unsigned_set(
    values: impl Iterator<Item = u64>,
) -> Result<CanonicalValue, FidelityError> {
    let values = values
        .map(CanonicalValue::Unsigned)
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

fn has_duplicates<T: Ord + Copy>(values: &[T]) -> bool {
    values.iter().copied().collect::<BTreeSet<_>>().len() != values.len()
}

fn has_duplicate_strengths(values: &[EvidenceStrength]) -> bool {
    values
        .iter()
        .map(|strength| *strength as u64)
        .collect::<BTreeSet<_>>()
        .len()
        != values.len()
}

fn value_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], FidelityError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(FidelityError::InvalidField(field)),
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
) -> Result<&'a CanonicalValue, FidelityError> {
    value_optional(map, key).ok_or(FidelityError::InvalidField(field))
}

fn value_unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, FidelityError> {
    match value_required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(FidelityError::InvalidField(field)),
    }
}

fn value_array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], FidelityError> {
    match value_required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(FidelityError::InvalidField(field)),
    }
}

fn value_bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], FidelityError> {
    value_bytes32_direct(value_required(map, key, field)?, field)
}

fn value_bytes32_direct(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 32], FidelityError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(FidelityError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| FidelityError::InvalidField(field))
}

fn value_bytes16_direct(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 16], FidelityError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(FidelityError::InvalidField(field));
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| FidelityError::InvalidField(field))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FidelityError {
    Canonical(CanonicalError),
    Object(ObjectError),
    InvalidField(&'static str),
    Limit,
    NonCanonicalPayload,
    WrongPayloadKind,
    WrongEventType,
    DisclosureMismatch,
    PayloadReferenceMismatch,
}

impl From<CanonicalError> for FidelityError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ObjectError> for FidelityError {
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

    fn dimension(
        kind: CorrelationDimension,
        commitment: u8,
        strength: EvidenceStrength,
    ) -> CorrelationDimensionEvidence {
        CorrelationDimensionEvidence {
            dimension: kind,
            value_commitment: Some([commitment; 32]),
            strength,
            evidence_refs: vec![],
        }
    }

    fn correlation(admin: u8, pipeline: u8, device: u8) -> CorrelationEvidence {
        CorrelationEvidence {
            dimensions: vec![
                dimension(
                    CorrelationDimension::AdministrativePrincipal,
                    admin,
                    EvidenceStrength::CryptoBound,
                ),
                dimension(
                    CorrelationDimension::PipelineModelLineage,
                    pipeline,
                    EvidenceStrength::ExternallyAttested,
                ),
                dimension(
                    CorrelationDimension::DeviceOrFeed,
                    device,
                    EvidenceStrength::CryptoBound,
                ),
            ],
        }
    }

    fn attestation(correlation_evidence: CorrelationEvidence) -> EncodingFidelityAttestation {
        EncodingFidelityAttestation {
            source_artifact: reference(1),
            candidate_encoding: reference(2),
            blind_attempt_output_commitment: [3; 32],
            attempt_ref: reference(4),
            execution_record_ref: reference(5),
            correlation_evidence,
            checks: vec![
                FidelityCheck {
                    kind: FidelityCheckKind::SourceSpanAlignment,
                    status: FidelityCheckStatus::ConsistentWithSource,
                    checked_region_commitment: [6; 32],
                    evidence_ref: None,
                },
                FidelityCheck {
                    kind: FidelityCheckKind::GeneSelection,
                    status: FidelityCheckStatus::HardEncodingMismatch,
                    checked_region_commitment: [7; 32],
                    evidence_ref: Some(reference(8)),
                },
                FidelityCheck {
                    kind: FidelityCheckKind::ConceptSelection,
                    status: FidelityCheckStatus::Unresolved,
                    checked_region_commitment: [9; 32],
                    evidence_ref: None,
                },
            ],
            limitations: vec![ConceptCcid::from_bytes([10; 16])],
            policy_ref: reference(11),
        }
    }

    #[test]
    fn hundred_sybil_device_ids_on_one_admin_pipeline_form_one_group() {
        let policy = FidelityPolicy::default_v1();
        assert!(policy.publisher_attempt_required);
        assert_eq!(policy.minimum_external_blind_attempts, 2);
        assert_eq!(policy.minimum_evidenced_distinct_external_groups, 2);
        let groups = (1..=100)
            .map(|device| {
                policy
                    .evidenced_group_key(&correlation(1, 2, device))
                    .unwrap()
                    .unwrap()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(groups.len(), 1);
        assert!(!policy.uses_node_count_or_self_claim_as_independence());
    }

    #[test]
    fn two_evidenced_distinct_admin_and_pipeline_pairs_have_distinct_group_keys() {
        let policy = FidelityPolicy::default_v1();
        let first = policy
            .evidenced_group_key(&correlation(1, 2, 3))
            .unwrap()
            .unwrap();
        let second = policy
            .evidenced_group_key(&correlation(4, 5, 3))
            .unwrap()
            .unwrap();
        assert_ne!(first, second);
        let self_claimed = CorrelationEvidence {
            dimensions: vec![
                dimension(
                    CorrelationDimension::AdministrativePrincipal,
                    4,
                    EvidenceStrength::SelfClaimed,
                ),
                dimension(
                    CorrelationDimension::PipelineModelLineage,
                    5,
                    EvidenceStrength::SelfClaimed,
                ),
            ],
        };
        assert_eq!(policy.evidenced_group_key(&self_claimed).unwrap(), None);
    }

    #[test]
    fn blind_attempt_shape_cannot_carry_revealed_candidate() {
        let blind = EncodingAttempt {
            role: EncodingAttemptRole::ExternalBlind,
            source_artifact: reference(1),
            candidate_encoding: None,
            output_commitment: [2; 32],
            pipeline_model_tool_commitments: vec![[3; 32]],
            source_acquisition_or_derivation_commitment: [4; 32],
            execution_record_ref: reference(5),
            blind_session_commitment: Some([6; 32]),
            challenge_nonce_commitment: Some([7; 32]),
        };
        assert!(blind.canonical_payload().is_ok());
        let mut leaked = blind;
        leaked.candidate_encoding = Some(reference(8));
        assert!(leaked.canonical_payload().is_err());
    }

    #[test]
    fn signed_attestation_binds_exact_object_but_never_votes_truth() {
        let key = SigningKey::from_bytes(&[30; 32]);
        let inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"fidelity", [31; 32]).unwrap(),
            0,
            DeviceId::from_bytes([32; 32]),
        );
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let payload = attestation(correlation(1, 2, 3));
        let object = payload
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap();
        let (object_bytes, object_cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let validated_object = decode_knowledge_object(
            &object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(
                ENCODING_FIDELITY_ATTESTATION_KIND,
                FIDELITY_PROFILE_MAJOR,
            )],
            &[],
        )
        .unwrap();
        let mut event = KnowledgeEventEnvelope::new(
            ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE,
            feed.feed_id,
            1,
            DisclosureClass::Public,
            [33; 32],
        );
        event.payload_refs = vec![ObjectReference::new(0, object_cid.into_bytes())];
        let signed_event = event.sign(&feed, &key).unwrap();
        let (event_bytes, _) = signed_event.encode().unwrap();
        let validated_event = decode_knowledge_event(
            &event_bytes,
            &feed,
            &[ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE],
        )
        .unwrap();
        let bound =
            ValidatedEncodingFidelityAttestationEvent::bind(&validated_event, &validated_object)
                .unwrap();
        assert!(bound.attestation().has_hard_encoding_mismatch());
        assert!(!bound.establishes_proposition_truth());
        assert!(!bound.attestation().classifies_knowledge_as_wrong());
        assert!(!bound.attestation().blocks_preserve_publish_query_or_use());
    }
}
