//! Semantic MappingKernel and provenance-bearing MappingEnvelope.

use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalValue, ResourceProfile,
};
use super::content_id::{EventCid, MappingKernelCid, ReservedDomain};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectKind, ObjectReference, SchemaVersion,
};
use super::schema_registry::OBJECT_KIND_MAPPING_ENVELOPE;
use super::semantic::{
    ConceptCcid, ConstraintEvaluation, DimensionVector, ExactRatio, SemanticError,
    SemanticFrameSet, TypedConstraint,
};

pub const MAPPING_ENVELOPE_KIND: ObjectKind = ObjectKind(OBJECT_KIND_MAPPING_ENVELOPE);
pub const MAPPING_PROFILE_MAJOR: u64 = 1;
pub const MAPPING_PROFILE_MINOR: u64 = 0;
pub const MAX_MAPPING_MEMBERS: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum MappingSide {
    Source = 0,
    Target = 1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingTermLocator {
    pub object: ObjectReference,
    pub statement_index: u32,
    pub argument_index: Option<u32>,
}

impl MappingTermLocator {
    fn to_value(&self) -> CanonicalValue {
        let mut fields = vec![
            (0, self.object.to_value()),
            (1, CanonicalValue::Unsigned(self.statement_index as u64)),
        ];
        if let Some(argument) = self.argument_index {
            fields.push((2, CanonicalValue::Unsigned(argument as u64)));
        }
        CanonicalValue::Map(fields)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum CorrespondenceKind {
    Equivalent = 0,
    Broader = 1,
    Narrower = 2,
    Analogous = 3,
    CausalRole = 4,
    StructuralRole = 5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappingTransform {
    Identity,
    AffineUnit {
        source_dimension: DimensionVector,
        target_dimension: DimensionVector,
        scale: ExactRatio,
        offset: ExactRatio,
    },
    ExplicitRule {
        rule: ObjectReference,
    },
}

impl MappingTransform {
    fn to_value(&self) -> CanonicalValue {
        match self {
            Self::Identity => CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(0))]),
            Self::AffineUnit {
                source_dimension,
                target_dimension,
                scale,
                offset,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, source_dimension.to_value()),
                (2, target_dimension.to_value()),
                (3, scale.to_value()),
                (4, offset.to_value()),
            ]),
            Self::ExplicitRule { rule } => {
                CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(2)), (1, rule.to_value())])
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermCorrespondence {
    pub source: MappingTermLocator,
    pub target: MappingTermLocator,
    pub kind: CorrespondenceKind,
    pub transform: MappingTransform,
}

impl TermCorrespondence {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.source.to_value()),
            (1, self.target.to_value()),
            (2, CanonicalValue::Unsigned(self.kind as u64)),
            (3, self.transform.to_value()),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingConstraintRegion {
    pub constraint: TypedConstraint,
    pub evaluation: ConstraintEvaluation,
}

impl MappingConstraintRegion {
    fn to_value(&self) -> CanonicalValue {
        let evaluation = match self.evaluation {
            ConstraintEvaluation::Satisfied => 0,
            ConstraintEvaluation::Violated => 1,
            ConstraintEvaluation::Unknown => 2,
        };
        CanonicalValue::Map(vec![
            (0, self.constraint.to_value()),
            (1, CanonicalValue::Unsigned(evaluation)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmappedRegion {
    pub side: MappingSide,
    pub locator: MappingTermLocator,
    pub reason: ConceptCcid,
}

impl UnmappedRegion {
    fn to_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.side as u64)),
            (1, self.locator.to_value()),
            (2, CanonicalValue::Bytes(self.reason.as_bytes().to_vec())),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingKernel {
    pub source_objects: Vec<ObjectReference>,
    pub target_objects: Vec<ObjectReference>,
    pub correspondences: Vec<TermCorrespondence>,
    pub assumptions: SemanticFrameSet,
    pub constraint_regions: Vec<MappingConstraintRegion>,
    pub unmapped_regions: Vec<UnmappedRegion>,
}

impl MappingKernel {
    pub fn canonical_value(&self) -> Result<CanonicalValue, MappingError> {
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(MAPPING_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(MAPPING_PROFILE_MINOR)),
            (2, canonical_set(reference_values(&self.source_objects))?),
            (3, canonical_set(reference_values(&self.target_objects))?),
            (
                4,
                canonical_set(
                    self.correspondences
                        .iter()
                        .map(TermCorrespondence::to_value)
                        .collect(),
                )?,
            ),
            (5, self.assumptions.canonical_value()?),
            (
                6,
                canonical_set(
                    self.constraint_regions
                        .iter()
                        .map(MappingConstraintRegion::to_value)
                        .collect(),
                )?,
            ),
            (
                7,
                canonical_set(
                    self.unmapped_regions
                        .iter()
                        .map(UnmappedRegion::to_value)
                        .collect(),
                )?,
            ),
        ]))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, MappingError> {
        encode_canonical(&self.canonical_value()?, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub fn cid(&self) -> Result<MappingKernelCid, MappingError> {
        MappingKernelCid::compute(ReservedDomain::MappingKernel, &self.canonical_bytes()?)
            .map_err(|_| MappingError::Domain)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingEnvelope {
    pub kernel: MappingKernelCid,
    pub generator: ObjectReference,
    pub derivation_rule: Option<ObjectReference>,
    pub evidence: Vec<ObjectReference>,
    pub source_event: Option<EventCid>,
}

impl MappingEnvelope {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, MappingError> {
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(MAPPING_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(MAPPING_PROFILE_MINOR)),
            (2, CanonicalValue::Bytes(self.kernel.as_bytes().to_vec())),
            (3, self.generator.to_value()),
            (5, canonical_set(reference_values(&self.evidence))?),
        ];
        if let Some(rule) = &self.derivation_rule {
            fields.push((4, rule.to_value()));
        }
        if let Some(event) = self.source_event {
            fields.push((6, CanonicalValue::Bytes(event.as_bytes().to_vec())));
        }
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, MappingError> {
        Ok(KnowledgeObjectEnvelope::new(
            MAPPING_ENVELOPE_KIND,
            SchemaVersion::new(MAPPING_PROFILE_MAJOR, MAPPING_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        ))
    }
}

fn reference_values(references: &[ObjectReference]) -> Vec<CanonicalValue> {
    references.iter().map(ObjectReference::to_value).collect()
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, MappingError> {
    if values.len() > MAX_MAPPING_MEMBERS {
        return Err(MappingError::Limit);
    }
    let values = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappingError {
    Semantic(SemanticError),
    Limit,
    Domain,
}

impl From<SemanticError> for MappingError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<super::canonical::CanonicalError> for MappingError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Semantic(SemanticError::Canonical(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        ComparisonOperator, ConstraintExpression, StatementFrame, StatementId, StatementQualifiers,
        TermRef,
    };

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn locator(object: u8, statement: u32, argument: u32) -> MappingTermLocator {
        MappingTermLocator {
            object: reference(object),
            statement_index: statement,
            argument_index: Some(argument),
        }
    }

    fn assumptions() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(99),
                operator_or_predicate: concept(1),
                arguments: vec![TermRef::Concept(concept(2))],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn kernel(correspondences: Vec<TermCorrespondence>) -> MappingKernel {
        MappingKernel {
            source_objects: vec![reference(1)],
            target_objects: vec![reference(2)],
            correspondences,
            assumptions: assumptions(),
            constraint_regions: vec![MappingConstraintRegion {
                constraint: TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: TermRef::Concept(concept(3)),
                        operator: ComparisonOperator::Equal,
                        right: TermRef::Concept(concept(4)),
                    },
                    required: true,
                },
                evaluation: ConstraintEvaluation::Unknown,
            }],
            unmapped_regions: vec![UnmappedRegion {
                side: MappingSide::Source,
                locator: locator(1, 0, 2),
                reason: concept(9),
            }],
        }
    }

    fn correspondence(source_arg: u32, target_arg: u32) -> TermCorrespondence {
        TermCorrespondence {
            source: locator(1, 0, source_arg),
            target: locator(2, 0, target_arg),
            kind: CorrespondenceKind::Analogous,
            transform: MappingTransform::Identity,
        }
    }

    #[test]
    fn correspondence_insertion_order_does_not_change_kernel_id() {
        let left = kernel(vec![correspondence(0, 1), correspondence(2, 3)]);
        let right = kernel(vec![correspondence(2, 3), correspondence(0, 1)]);
        assert_eq!(left.cid().unwrap(), right.cid().unwrap());
    }

    #[test]
    fn generator_and_evidence_change_envelope_not_kernel() {
        let kernel = kernel(vec![correspondence(0, 1)]).cid().unwrap();
        let left = MappingEnvelope {
            kernel,
            generator: reference(10),
            derivation_rule: Some(reference(11)),
            evidence: vec![reference(12)],
            source_event: None,
        }
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        let right = MappingEnvelope {
            kernel,
            generator: reference(20),
            derivation_rule: Some(reference(11)),
            evidence: vec![reference(13)],
            source_event: None,
        }
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        assert_ne!(left.1, right.1);
        assert!(left.0.windows(32).any(|window| window == kernel.as_bytes()));
        assert!(right
            .0
            .windows(32)
            .any(|window| window == kernel.as_bytes()));
    }

    #[test]
    fn unknown_violated_and_unmapped_regions_are_identity_bearing() {
        let unknown = kernel(vec![correspondence(0, 1)]);
        let mut violated = unknown.clone();
        violated.constraint_regions[0].evaluation = ConstraintEvaluation::Violated;
        let mut fully_mapped = unknown.clone();
        fully_mapped.unmapped_regions.clear();
        assert_ne!(unknown.cid().unwrap(), violated.cid().unwrap());
        assert_ne!(unknown.cid().unwrap(), fully_mapped.cid().unwrap());
    }

    #[test]
    fn affine_transform_preserves_dimensions_scale_and_offset() {
        let mut transformed = correspondence(0, 1);
        transformed.transform = MappingTransform::AffineUnit {
            source_dimension: DimensionVector::TEMPERATURE,
            target_dimension: DimensionVector::TEMPERATURE,
            scale: ExactRatio::integer(1),
            offset: ExactRatio::new(27_315, 100).unwrap(),
        };
        assert_ne!(
            kernel(vec![correspondence(0, 1)]).cid().unwrap(),
            kernel(vec![transformed]).cid().unwrap()
        );
    }

    #[test]
    fn duplicate_correspondence_is_rejected() {
        let duplicate = correspondence(0, 1);
        assert!(kernel(vec![duplicate.clone(), duplicate]).cid().is_err());
    }
}
