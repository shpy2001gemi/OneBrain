//! Immutable frontier assembly manifests with stable receptor placements.

use std::collections::BTreeSet;

use super::canonical::{canonicalize_set_by_key, CanonicalValue, ResourceProfile};
use super::object::{
    DisclosureClass, KnowledgeObjectEnvelope, ObjectKind, ObjectReference, SchemaVersion,
};
use super::receptor::ReceptorCardinality;
use super::schema_registry::OBJECT_KIND_ASSEMBLY_MANIFEST;
use super::semantic::{SemanticError, SemanticFrameSet};

pub const ASSEMBLY_MANIFEST_KIND: ObjectKind = ObjectKind(OBJECT_KIND_ASSEMBLY_MANIFEST);
pub const ASSEMBLY_PROFILE_MAJOR: u64 = 1;
pub const ASSEMBLY_PROFILE_MINOR: u64 = 0;
pub const MAX_ASSEMBLY_PLACEMENTS: usize = 16_384;
pub const MAX_ASSEMBLY_SOURCES: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssemblyLineageId([u8; 32]);

impl AssemblyLineageId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlacementId([u8; 32]);

impl PlacementId {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorPlacement {
    pub placement_id: PlacementId,
    pub receptor_definition: ObjectReference,
    pub cardinality: ReceptorCardinality,
    pub required: bool,
    pub local_context: SemanticFrameSet,
    pub resolution_policy_override: Option<ObjectReference>,
}

impl ReceptorPlacement {
    fn to_value(&self) -> Result<CanonicalValue, AssemblyError> {
        let mut fields = vec![
            (
                0,
                CanonicalValue::Bytes(self.placement_id.as_bytes().to_vec()),
            ),
            (1, self.receptor_definition.to_value()),
            (
                2,
                CanonicalValue::Map({
                    let mut cardinality =
                        vec![(0, CanonicalValue::Unsigned(self.cardinality.minimum as u64))];
                    if let Some(maximum) = self.cardinality.maximum {
                        cardinality.push((1, CanonicalValue::Unsigned(maximum as u64)));
                    }
                    cardinality
                }),
            ),
            (3, CanonicalValue::Bool(self.required)),
            (4, self.local_context.canonical_value()?),
        ];
        if let Some(policy) = &self.resolution_policy_override {
            fields.push((5, policy.to_value()));
        }
        Ok(CanonicalValue::Map(fields))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontierAssemblyManifest {
    pub lineage: AssemblyLineageId,
    pub revision: u64,
    pub predecessor: Option<ObjectReference>,
    pub source_objects: Vec<ObjectReference>,
    pub placements: Vec<ReceptorPlacement>,
    pub default_resolution_policy: ObjectReference,
}

impl FrontierAssemblyManifest {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, AssemblyError> {
        if self.revision == 0 && self.predecessor.is_some() {
            return Err(AssemblyError::UnexpectedPredecessor);
        }
        if self.revision > 0 && self.predecessor.is_none() {
            return Err(AssemblyError::MissingPredecessor);
        }
        if self.placements.len() > MAX_ASSEMBLY_PLACEMENTS
            || self.source_objects.len() > MAX_ASSEMBLY_SOURCES
        {
            return Err(AssemblyError::Limit);
        }
        let unique: BTreeSet<_> = self
            .placements
            .iter()
            .map(|placement| placement.placement_id)
            .collect();
        if unique.len() != self.placements.len() {
            return Err(AssemblyError::DuplicatePlacement);
        }

        let source_values = self
            .source_objects
            .iter()
            .map(ObjectReference::to_value)
            .collect::<Vec<_>>();
        let placement_values = self
            .placements
            .iter()
            .map(ReceptorPlacement::to_value)
            .collect::<Result<Vec<_>, _>>()?;
        let mut fields = vec![
            (0, CanonicalValue::Unsigned(ASSEMBLY_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(ASSEMBLY_PROFILE_MINOR)),
            (2, CanonicalValue::Bytes(self.lineage.0.to_vec())),
            (3, CanonicalValue::Unsigned(self.revision)),
            (4, canonical_set(source_values)?),
            (5, canonical_set(placement_values)?),
            (6, self.default_resolution_policy.to_value()),
        ];
        if let Some(predecessor) = &self.predecessor {
            fields.push((7, predecessor.to_value()));
        }
        Ok(CanonicalValue::Map(fields))
    }

    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<KnowledgeObjectEnvelope, AssemblyError> {
        Ok(KnowledgeObjectEnvelope::new(
            ASSEMBLY_MANIFEST_KIND,
            SchemaVersion::new(ASSEMBLY_PROFILE_MAJOR, ASSEMBLY_PROFILE_MINOR),
            disclosure,
            self.canonical_payload()?,
        ))
    }

    pub fn placement(&self, id: PlacementId) -> Option<&ReceptorPlacement> {
        self.placements
            .iter()
            .find(|placement| placement.placement_id == id)
    }
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, AssemblyError> {
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
pub enum AssemblyError {
    Semantic(SemanticError),
    Limit,
    DuplicatePlacement,
    MissingPredecessor,
    UnexpectedPredecessor,
}

impl From<SemanticError> for AssemblyError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<super::canonical::CanonicalError> for AssemblyError {
    fn from(error: super::canonical::CanonicalError) -> Self {
        Self::Semantic(SemanticError::Canonical(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn empty_frames() -> SemanticFrameSet {
        SemanticFrameSet {
            statements: Vec::new(),
        }
    }

    fn placement(id: u8, definition: u8) -> ReceptorPlacement {
        ReceptorPlacement {
            placement_id: PlacementId::from_bytes([id; 32]),
            receptor_definition: reference(definition),
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            required: true,
            local_context: empty_frames(),
            resolution_policy_override: None,
        }
    }

    fn manifest(placements: Vec<ReceptorPlacement>) -> FrontierAssemblyManifest {
        FrontierAssemblyManifest {
            lineage: AssemblyLineageId::from_bytes([1; 32]),
            revision: 0,
            predecessor: None,
            source_objects: vec![reference(2)],
            placements,
            default_resolution_policy: reference(3),
        }
    }

    #[test]
    fn same_definition_at_two_placements_has_distinct_stable_identity() {
        let manifest = manifest(vec![placement(10, 9), placement(11, 9)]);
        assert_eq!(
            manifest.placements[0].receptor_definition,
            manifest.placements[1].receptor_definition
        );
        assert_ne!(
            manifest.placements[0].placement_id,
            manifest.placements[1].placement_id
        );
        assert!(manifest
            .placement(PlacementId::from_bytes([10; 32]))
            .is_some());
        assert!(manifest
            .placement(PlacementId::from_bytes([11; 32]))
            .is_some());
    }

    #[test]
    fn placement_insertion_order_does_not_change_manifest_cid() {
        let left = manifest(vec![placement(10, 9), placement(11, 9)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let right = manifest(vec![placement(11, 9), placement(10, 9)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn duplicate_placement_and_invalid_revision_chain_are_rejected() {
        let duplicate = manifest(vec![placement(10, 9), placement(10, 8)]);
        assert_eq!(
            duplicate.canonical_payload().unwrap_err(),
            AssemblyError::DuplicatePlacement
        );

        let mut revision = manifest(Vec::new());
        revision.revision = 1;
        assert_eq!(
            revision.canonical_payload().unwrap_err(),
            AssemblyError::MissingPredecessor
        );
    }

    #[test]
    fn changing_placement_changes_manifest_identity_without_changing_definition() {
        let left = manifest(vec![placement(10, 9)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let right = manifest(vec![placement(11, 9)])
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        assert_ne!(left.1, right.1);
    }
}
