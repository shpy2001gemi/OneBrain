//! Receptor definition and private claim-envelope objects.

use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalValue, ResourceProfile,
};
use super::content_id::ReservedDomain;
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectKind, ObjectReference, SchemaVersion,
};
use super::schema_registry::{
    OBJECT_KIND_RECEPTOR_CLAIM_ENVELOPE, OBJECT_KIND_RECEPTOR_DEFINITION,
};
use super::semantic::{
    ConceptCcid, SemanticError, SemanticFrameSet, StatementFrame, StatementId, StatementQualifiers,
    TermRef, TypedConstraint,
};

pub const RECEPTOR_DEFINITION_KIND: ObjectKind = ObjectKind(OBJECT_KIND_RECEPTOR_DEFINITION);
pub const RECEPTOR_CLAIM_KIND: ObjectKind = ObjectKind(OBJECT_KIND_RECEPTOR_CLAIM_ENVELOPE);
pub const RECEPTOR_PROFILE_MAJOR: u64 = 1;
pub const RECEPTOR_PROFILE_MINOR: u64 = 0;
pub const MAX_EXPECTED_TYPES: usize = 1_024;
pub const MAX_ORIGIN_INPUTS: usize = 4_096;
pub const MAX_CLAIM_EVIDENCE: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementLocator {
    pub object: ObjectReference,
    pub statement_index: u32,
}

impl StatementLocator {
    pub(crate) fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.object.to_value()),
            (1, CanonicalValue::Unsigned(self.statement_index as u64)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceptorOrigin {
    Declared {
        source: StatementLocator,
    },
    Derived {
        derivation_rule: ObjectReference,
        inputs: Vec<ObjectReference>,
    },
    Emergent {
        detector: ObjectReference,
        observations: Vec<ObjectReference>,
    },
}

impl ReceptorOrigin {
    fn to_value(&self) -> Result<CanonicalValue, ReceptorError> {
        match self {
            Self::Declared { source } => Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, source.to_value()),
            ])),
            Self::Derived {
                derivation_rule,
                inputs,
            } => Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, derivation_rule.to_value()),
                (2, canonical_reference_set(inputs, MAX_ORIGIN_INPUTS)?),
            ])),
            Self::Emergent {
                detector,
                observations,
            } => Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, detector.to_value()),
                (2, canonical_reference_set(observations, MAX_ORIGIN_INPUTS)?),
            ])),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceptorCardinality {
    pub minimum: u32,
    pub maximum: Option<u32>,
}

impl ReceptorCardinality {
    pub fn new(minimum: u32, maximum: Option<u32>) -> Result<Self, ReceptorError> {
        if maximum.is_some_and(|maximum| maximum < minimum) {
            return Err(ReceptorError::Cardinality);
        }
        Ok(Self { minimum, maximum })
    }

    fn to_value(self) -> CanonicalValue {
        let mut fields = vec![(0, CanonicalValue::Unsigned(self.minimum as u64))];
        if let Some(maximum) = self.maximum {
            fields.push((1, CanonicalValue::Unsigned(maximum as u64)));
        }
        CanonicalValue::Map(fields)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum UnknownConstraintPolicy {
    RejectBinding = 0,
    KeepUnresolved = 1,
    AllowWithExplicitWaiver = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorAcceptanceProfile {
    pub policy: ObjectReference,
    pub required_evidence_kinds: Vec<ConceptCcid>,
    pub unknown_constraint_policy: UnknownConstraintPolicy,
}

impl ReceptorAcceptanceProfile {
    fn to_value(&self) -> Result<CanonicalValue, ReceptorError> {
        let evidence = canonical_ccid_set(&self.required_evidence_kinds, MAX_EXPECTED_TYPES)?;
        Ok(CanonicalValue::Map(vec![
            (0, self.policy.to_value()),
            (1, evidence),
            (
                2,
                CanonicalValue::Unsigned(self.unknown_constraint_policy as u64),
            ),
        ]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorDefinition {
    pub role: ConceptCcid,
    pub expected_types: Vec<ConceptCcid>,
    pub hard_constraints: Vec<TypedConstraint>,
    pub cardinality: ReceptorCardinality,
    pub origin: ReceptorOrigin,
    pub acceptance: ReceptorAcceptanceProfile,
}

impl ReceptorDefinition {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, ReceptorError> {
        let expected_types = canonical_ccid_set(&self.expected_types, MAX_EXPECTED_TYPES)?;
        let constraints = normalized_constraints(self.role, &self.hard_constraints)?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(RECEPTOR_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(RECEPTOR_PROFILE_MINOR)),
            (2, CanonicalValue::Bytes(self.role.as_bytes().to_vec())),
            (3, expected_types),
            (
                4,
                CanonicalValue::Array(constraints.iter().map(TypedConstraint::to_value).collect()),
            ),
            (5, self.cardinality.to_value()),
            (6, self.origin.to_value()?),
            (7, self.acceptance.to_value()?),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, ReceptorError> {
        Ok(KnowledgeObjectEnvelope::new(
            RECEPTOR_DEFINITION_KIND,
            SchemaVersion::new(RECEPTOR_PROFILE_MAJOR, RECEPTOR_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceptorClaimValue {
    Concept(ConceptCcid),
    Literal(super::semantic::LiteralValue),
    KnowledgeObject(ObjectReference),
}

impl ReceptorClaimValue {
    fn to_term(&self) -> TermRef {
        match self {
            Self::Concept(concept) => TermRef::Concept(*concept),
            Self::Literal(literal) => TermRef::Literal(literal.clone()),
            Self::KnowledgeObject(reference) => TermRef::KnowledgeObject(reference.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorClaimEnvelope {
    pub definition: ObjectReference,
    pub candidate: ReceptorClaimValue,
    pub evidence: Vec<ObjectReference>,
    pub disclosure: DisclosureClass,
}

impl ReceptorClaimEnvelope {
    pub fn new_private(
        definition: ObjectReference,
        candidate: ReceptorClaimValue,
        evidence: Vec<ObjectReference>,
    ) -> Self {
        Self {
            definition,
            candidate,
            evidence,
            disclosure: DisclosureClass::LocalOnly,
        }
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, ReceptorError> {
        if !matches!(
            self.disclosure,
            DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
        ) {
            return Err(ReceptorError::ClaimMustUsePrivateStorage);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(RECEPTOR_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(RECEPTOR_PROFILE_MINOR)),
            (2, self.definition.to_value()),
            (3, self.candidate.to_term().to_value()),
            (
                4,
                canonical_reference_set(&self.evidence, MAX_CLAIM_EVIDENCE)?,
            ),
        ]))
    }

    pub fn to_knowledge_object(&self) -> Result<KnowledgeObjectEnvelope, ReceptorError> {
        Ok(KnowledgeObjectEnvelope::new(
            RECEPTOR_CLAIM_KIND,
            SchemaVersion::new(RECEPTOR_PROFILE_MAJOR, RECEPTOR_PROFILE_MINOR),
            self.disclosure,
            self.canonical_payload()?,
        ))
    }

    /// Explicit policy-gated operation. Ordinary construction/encoding creates
    /// no network-visible commitment.
    pub fn randomized_commitment(
        &self,
        opening: [u8; 32],
        disclosure_policy: ObjectReference,
    ) -> Result<ReceptorClaimCommitment, ReceptorError> {
        let commitment_preimage = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(1)),
            (1, CanonicalValue::Bytes(opening.to_vec())),
            (2, disclosure_policy.to_value()),
            (3, self.canonical_payload()?),
        ]);
        let bytes = encode_canonical(&commitment_preimage, ResourceProfile::ObjectV1)
            .map_err(SemanticError::from)?;
        Ok(ReceptorClaimCommitment {
            commitment: ReservedDomain::Object.digest(&bytes),
            disclosure_policy,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorClaimCommitment {
    pub commitment: [u8; 32],
    pub disclosure_policy: ObjectReference,
}

fn normalized_constraints(
    role: ConceptCcid,
    constraints: &[TypedConstraint],
) -> Result<Vec<TypedConstraint>, ReceptorError> {
    let frame = SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: role,
            arguments: Vec::new(),
            constraints: constraints.to_vec(),
            qualifiers: StatementQualifiers::default(),
        }],
    }
    .alpha_normalized()?;
    Ok(frame.statements.into_iter().next().unwrap().constraints)
}

fn canonical_ccid_set(
    values: &[ConceptCcid],
    limit: usize,
) -> Result<CanonicalValue, ReceptorError> {
    if values.len() > limit {
        return Err(ReceptorError::Limit);
    }
    let members = values
        .iter()
        .map(|value| {
            let value = CanonicalValue::Bytes(value.as_bytes().to_vec());
            (value.clone(), value)
        })
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

fn canonical_reference_set(
    values: &[ObjectReference],
    limit: usize,
) -> Result<CanonicalValue, ReceptorError> {
    if values.len() > limit {
        return Err(ReceptorError::Limit);
    }
    let members = values
        .iter()
        .map(|reference| {
            let value = reference.to_value();
            (value.clone(), value)
        })
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceptorError {
    Semantic(SemanticError),
    Cardinality,
    Limit,
    ClaimMustUsePrivateStorage,
}

impl From<SemanticError> for ReceptorError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<super::canonical::CanonicalError> for ReceptorError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Semantic(SemanticError::Canonical(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        InMemoryVerifiedBackend, KnownObjectKind, PrivateVault, PutVerifiedOutcome, VaultKey,
    };

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn definition(expected_types: Vec<ConceptCcid>) -> ReceptorDefinition {
        ReceptorDefinition {
            role: concept(1),
            expected_types,
            hard_constraints: Vec::new(),
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin: ReceptorOrigin::Declared {
                source: StatementLocator {
                    object: reference(2),
                    statement_index: 0,
                },
            },
            acceptance: ReceptorAcceptanceProfile {
                policy: reference(3),
                required_evidence_kinds: vec![concept(4)],
                unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
            },
        }
    }

    #[test]
    fn set_order_does_not_change_definition_object_cid() {
        let left = definition(vec![concept(8), concept(7)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let right = definition(vec![concept(7), concept(8)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn origin_kinds_have_distinct_canonical_payloads() {
        let declared = definition(vec![concept(7)]).canonical_payload().unwrap();
        let mut derived = definition(vec![concept(7)]);
        derived.origin = ReceptorOrigin::Derived {
            derivation_rule: reference(5),
            inputs: vec![reference(6)],
        };
        let mut emergent = definition(vec![concept(7)]);
        emergent.origin = ReceptorOrigin::Emergent {
            detector: reference(5),
            observations: vec![reference(6)],
        };
        assert_ne!(declared, derived.canonical_payload().unwrap());
        assert_ne!(
            derived.canonical_payload().unwrap(),
            emergent.canonical_payload().unwrap()
        );
    }

    #[test]
    fn private_claim_round_trips_only_through_private_vault() {
        let claim = ReceptorClaimEnvelope::new_private(
            reference(1),
            ReceptorClaimValue::Concept(concept(2)),
            vec![reference(3)],
        );
        let object = claim.to_knowledge_object().unwrap();
        let (bytes, cid) = object.encode(ResourceProfile::ObjectV1).unwrap();
        let vault = PrivateVault::new(
            InMemoryVerifiedBackend::default(),
            VaultKey::from_bytes([9; 32]),
        );
        assert_eq!(
            vault
                .put_verified_object(
                    cid,
                    &bytes,
                    ResourceProfile::ObjectV1,
                    &[KnownObjectKind::new(RECEPTOR_CLAIM_KIND, 1)],
                    &[],
                )
                .unwrap(),
            PutVerifiedOutcome::Stored
        );
        assert_eq!(vault.get_object(cid).unwrap().unwrap(), bytes);
    }

    #[test]
    fn encoding_private_claim_does_not_create_a_commitment() {
        let claim = ReceptorClaimEnvelope::new_private(
            reference(1),
            ReceptorClaimValue::Concept(concept(2)),
            Vec::new(),
        );
        let payload = claim.canonical_payload().unwrap();
        let CanonicalValue::Map(fields) = payload else {
            panic!("claim payload must be map");
        };
        assert_eq!(fields.len(), 5);
    }

    #[test]
    fn explicit_randomized_commitment_is_binding_and_hiding() {
        let claim = ReceptorClaimEnvelope::new_private(
            reference(1),
            ReceptorClaimValue::Concept(concept(2)),
            Vec::new(),
        );
        let policy = reference(8);
        let left = claim
            .randomized_commitment([1; 32], policy.clone())
            .unwrap();
        let same = claim
            .randomized_commitment([1; 32], policy.clone())
            .unwrap();
        let hidden = claim.randomized_commitment([2; 32], policy).unwrap();
        assert_eq!(left.commitment, same.commitment);
        assert_ne!(left.commitment, hidden.commitment);
    }

    #[test]
    fn public_claim_envelope_is_rejected() {
        let mut claim = ReceptorClaimEnvelope::new_private(
            reference(1),
            ReceptorClaimValue::Concept(concept(2)),
            Vec::new(),
        );
        claim.disclosure = DisclosureClass::Public;
        assert_eq!(
            claim.canonical_payload().unwrap_err(),
            ReceptorError::ClaimMustUsePrivateStorage
        );
    }
}
