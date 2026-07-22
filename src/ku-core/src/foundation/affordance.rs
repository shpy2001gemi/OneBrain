//! Explicit, provenance-preserving Knowledge Affordance objects.

use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectKind, ObjectReference, SchemaVersion,
};
use super::receptor::StatementLocator;
use super::schema_registry::OBJECT_KIND_KNOWLEDGE_AFFORDANCE;
use super::semantic::{ConceptCcid, SemanticError, SemanticFrameSet};

pub const KNOWLEDGE_AFFORDANCE_KIND: ObjectKind = ObjectKind(OBJECT_KIND_KNOWLEDGE_AFFORDANCE);
pub const AFFORDANCE_PROFILE_MAJOR: u64 = 1;
pub const AFFORDANCE_PROFILE_MINOR: u64 = 0;
pub const MAX_AFFORDANCE_MEMBERS: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedInput {
    pub receptor_definition: ObjectReference,
    pub role: ConceptCcid,
    pub required: bool,
}

impl AcceptedInput {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.receptor_definition.to_value()),
            (1, CanonicalValue::Bytes(self.role.as_bytes().to_vec())),
            (2, CanonicalValue::Bool(self.required)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffordanceOrigin {
    Explicit {
        claims: Vec<StatementLocator>,
    },
    Derived {
        derivation_engine: ObjectReference,
        derivation_rule: ObjectReference,
        inputs: Vec<ObjectReference>,
    },
}

impl AffordanceOrigin {
    fn to_value(&self) -> Result<CanonicalValue, AffordanceError> {
        match self {
            Self::Explicit { claims } => {
                if claims.len() > MAX_AFFORDANCE_MEMBERS {
                    return Err(AffordanceError::Limit);
                }
                let members = claims
                    .iter()
                    .map(|claim| {
                        let value = claim.to_value();
                        (value.clone(), value)
                    })
                    .collect();
                Ok(CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(0)),
                    (
                        1,
                        CanonicalValue::Array(canonicalize_set_by_key(
                            members,
                            ResourceProfile::ObjectV1,
                        )?),
                    ),
                ]))
            }
            Self::Derived {
                derivation_engine,
                derivation_rule,
                inputs,
            } => Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, derivation_engine.to_value()),
                (2, derivation_rule.to_value()),
                (3, canonical_reference_set(inputs)?),
            ])),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffordanceSemantics {
    pub preconditions: SemanticFrameSet,
    pub outputs: SemanticFrameSet,
    pub effects: SemanticFrameSet,
    pub properties: SemanticFrameSet,
    pub invariants: SemanticFrameSet,
    pub operating_conditions: SemanticFrameSet,
    pub limits: SemanticFrameSet,
}

impl AffordanceSemantics {
    fn to_value(&self) -> Result<CanonicalValue, AffordanceError> {
        Ok(CanonicalValue::Map(vec![
            (0, self.preconditions.canonical_value()?),
            (1, self.outputs.canonical_value()?),
            (2, self.effects.canonical_value()?),
            (3, self.properties.canonical_value()?),
            (4, self.invariants.canonical_value()?),
            (5, self.operating_conditions.canonical_value()?),
            (6, self.limits.canonical_value()?),
        ]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeAffordance {
    pub sources: Vec<ObjectReference>,
    pub offered_roles: Vec<ConceptCcid>,
    pub accepted_inputs: Vec<AcceptedInput>,
    pub semantics: AffordanceSemantics,
    pub abstraction_patterns: Vec<SemanticFrameSet>,
    pub origin: AffordanceOrigin,
}

impl KnowledgeAffordance {
    /// Exact declared support only. Embeddings/rankers cannot add a role here.
    pub fn supports_role(&self, role: ConceptCcid) -> bool {
        self.offered_roles.contains(&role)
    }

    pub fn canonical_payload(&self) -> Result<CanonicalValue, AffordanceError> {
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(AFFORDANCE_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(AFFORDANCE_PROFILE_MINOR)),
            (2, canonical_reference_set(&self.sources)?),
            (3, canonical_ccid_set(&self.offered_roles)?),
            (4, canonical_input_set(&self.accepted_inputs)?),
            (5, self.semantics.to_value()?),
            (6, canonical_pattern_set(&self.abstraction_patterns)?),
            (7, self.origin.to_value()?),
        ]))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, AffordanceError> {
        Ok(KnowledgeObjectEnvelope::new(
            KNOWLEDGE_AFFORDANCE_KIND,
            SchemaVersion::new(AFFORDANCE_PROFILE_MAJOR, AFFORDANCE_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        ))
    }
}

fn canonical_reference_set(values: &[ObjectReference]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(values.iter().map(ObjectReference::to_value).collect())
}

fn canonical_ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(
        values
            .iter()
            .map(|value| CanonicalValue::Bytes(value.as_bytes().to_vec()))
            .collect(),
    )
}

fn canonical_input_set(values: &[AcceptedInput]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(values.iter().map(AcceptedInput::to_value).collect())
}

fn canonical_pattern_set(values: &[SemanticFrameSet]) -> Result<CanonicalValue, AffordanceError> {
    canonical_set(
        values
            .iter()
            .map(SemanticFrameSet::canonical_value)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, AffordanceError> {
    if values.len() > MAX_AFFORDANCE_MEMBERS {
        return Err(AffordanceError::Limit);
    }
    let members = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ObjectV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffordanceError {
    Semantic(SemanticError),
    Limit,
}

impl From<SemanticError> for AffordanceError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<super::canonical::CanonicalError> for AffordanceError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Semantic(SemanticError::Canonical(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        ConstraintExpression, StatementFrame, StatementId, StatementQualifiers, TermRef,
        TypedConstraint,
    };

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn frames(marker: u8) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(marker as u32),
                operator_or_predicate: concept(marker),
                arguments: vec![TermRef::Concept(concept(marker + 1))],
                constraints: vec![TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: TermRef::Concept(concept(marker + 1)),
                        operator: super::super::semantic::ComparisonOperator::Equal,
                        right: TermRef::Concept(concept(marker + 2)),
                    },
                    required: true,
                }],
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn affordance(sources: Vec<ObjectReference>, roles: Vec<ConceptCcid>) -> KnowledgeAffordance {
        KnowledgeAffordance {
            sources,
            offered_roles: roles,
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(9),
                role: concept(8),
                required: true,
            }],
            semantics: AffordanceSemantics {
                preconditions: frames(10),
                outputs: frames(20),
                effects: frames(30),
                properties: frames(40),
                invariants: frames(50),
                operating_conditions: frames(60),
                limits: frames(70),
            },
            abstraction_patterns: vec![frames(80)],
            origin: AffordanceOrigin::Explicit {
                claims: vec![StatementLocator {
                    object: reference(1),
                    statement_index: 0,
                }],
            },
        }
    }

    #[test]
    fn set_insertion_order_does_not_change_affordance_cid() {
        let left = affordance(
            vec![reference(2), reference(1)],
            vec![concept(2), concept(1)],
        )
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        let right = affordance(
            vec![reference(1), reference(2)],
            vec![concept(1), concept(2)],
        )
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn derived_origin_retains_engine_rule_and_inputs_in_identity() {
        let explicit = affordance(vec![reference(1)], vec![concept(1)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let mut derived = affordance(vec![reference(1)], vec![concept(1)]);
        derived.origin = AffordanceOrigin::Derived {
            derivation_engine: reference(6),
            derivation_rule: reference(7),
            inputs: vec![reference(1)],
        };
        let derived = derived
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        assert_ne!(explicit.1, derived.1);
    }

    #[test]
    fn only_explicitly_offered_role_is_supported() {
        let affordance = affordance(vec![reference(1)], vec![concept(1)]);
        assert!(affordance.supports_role(concept(1)));
        assert!(!affordance.supports_role(concept(2)));
    }

    #[test]
    fn duplicate_source_is_rejected_instead_of_inflating_provenance() {
        let error = affordance(vec![reference(1), reference(1)], vec![concept(1)])
            .canonical_payload()
            .unwrap_err();
        assert!(matches!(error, AffordanceError::Semantic(_)));
    }
}
