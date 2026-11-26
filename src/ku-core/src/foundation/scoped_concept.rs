//! P1 Scoped Novel-Concept Definitions & Registry Absorption (`P1-KU-LIFECYCLE-001` §2.3).
//!
//! Allows partitioned or domain-specialized authors to coin namespace-bound
//! `ScopedConceptCcid`s backed by canonical `ConceptDefinitionFrame` objects,
//! enforce explicit `ObjectReference` dependencies before KU acceptance, and
//! later establish `ConceptEquivalence` mappings to standard Registry CCIDs
//! without rewriting historical KUs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::canonical::{
    canonicalize_set_by_key, CanonicalError, CanonicalValue, NormalizedText, ResourceProfile,
};
use super::content_id::{ObjectCid, SemanticContentCid};
use super::mapping::{
    CorrespondenceKind, MappingError, MappingKernel, MappingTermLocator, MappingTransform,
    TermCorrespondence,
};
use super::object::{DisclosureClass, KnowledgeObjectEnvelope, ObjectError, ObjectReference};
use super::semantic::{
    ComparisonOperator, ConceptCcid, ConstraintExpression, LiteralValue, SemanticError,
    SemanticFrameSet, StatementFrame, StatementId, StatementQualifiers, TermRef, TypedConstraint,
};
use super::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};

pub const DEFINES_SCOPED_CONCEPT_DOMAIN: &[u8] = b"onebrain:vnext:tier0:defines-scoped-concept:1\0";
pub const SCOPED_CCID_DOMAIN: &[u8] = b"onebrain:vnext:scoped-ccid:1\0";

/// Reserved Tier-0 predicate `blake3("onebrain:vnext:tier0:defines-scoped-concept:1\0")[..16]`.
pub const DEFINES_SCOPED_CONCEPT_CCID: ConceptCcid = ConceptCcid::from_bytes([
    144, 187, 250, 122, 207, 222, 184, 88, 105, 173, 32, 146, 99, 12, 174, 189,
]);

/// Non-critical `KnowledgeObjectEnvelope.extensions` tag (`"SCOPED01"`).
pub const SCOPED_CONCEPT_EXTENSION_KEY: u64 = 0x5343_4f50_4544_3031;

pub const MAX_SCOPED_CONCEPT_BINDINGS: usize = 1_024;
pub const MAX_BROADER_CONCEPTS: usize = 64;

/// Derive a provisional 128-bit `ConceptCcid` from the normalized `SemanticContentCid`
/// of its `ConceptDefinitionFrame` (`blake3("onebrain:vnext:scoped-ccid:1\0" || def_semantic_cid)[..16]`).
pub fn derive_scoped_concept_ccid(def_semantic_cid: &SemanticContentCid) -> ConceptCcid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SCOPED_CCID_DOMAIN);
    hasher.update(def_semantic_cid.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    ConceptCcid::from_bytes(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConceptOrigin {
    StandardRegistry,
    ScopedDefinition(ObjectCid),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopedDependencyStatus {
    Satisfied,
    DeferredMissingDependency { missing: Vec<ObjectCid> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopedConceptError {
    Canonical(CanonicalError),
    Semantic(SemanticError),
    Object(ObjectError),
    EmptyDefinition,
    WrongDefinitionPredicate(ConceptCcid),
    InvalidDefinitionArguments,
    EmptyLabelOrSummary,
    TooManyBroaderConcepts,
    TooManyScopedBindings,
    DuplicateScopedConceptBinding(ConceptCcid),
    DuplicateExtensionKey,
    MalformedExtension,
    UnreferencedDefinitionObject {
        scoped_ccid: ConceptCcid,
        definition_object: ObjectCid,
    },
    UnusedScopedConceptBinding(ConceptCcid),
}

impl fmt::Display for ScopedConceptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonical(err) => write!(formatter, "scoped concept canonical error: {err}"),
            Self::Semantic(err) => write!(formatter, "scoped concept semantic error: {err}"),
            Self::Object(err) => write!(formatter, "scoped concept object error: {err}"),
            Self::EmptyDefinition => formatter.write_str("concept definition frame is empty"),
            Self::WrongDefinitionPredicate(ccid) => {
                write!(formatter, "expected DEFINES_SCOPED_CONCEPT predicate, found {ccid}")
            }
            Self::InvalidDefinitionArguments => {
                formatter.write_str("concept definition frame requires NFC label and summary")
            }
            Self::EmptyLabelOrSummary => {
                formatter.write_str("concept definition label and summary must be non-empty")
            }
            Self::TooManyBroaderConcepts => {
                formatter.write_str("concept definition exceeds maximum broader concepts")
            }
            Self::TooManyScopedBindings => {
                formatter.write_str("scoped concept extension exceeds maximum bindings")
            }
            Self::DuplicateScopedConceptBinding(ccid) => {
                write!(formatter, "duplicate scoped concept binding for {ccid}")
            }
            Self::DuplicateExtensionKey => {
                formatter.write_str("duplicate SCOPED_CONCEPT_EXTENSION_KEY on envelope")
            }
            Self::MalformedExtension => {
                formatter.write_str("malformed SCOPED_CONCEPT_EXTENSION_KEY payload")
            }
            Self::UnreferencedDefinitionObject {
                scoped_ccid,
                definition_object,
            } => write!(
                formatter,
                "scoped concept {scoped_ccid} defining object {definition_object} is missing from envelope.references"
            ),
            Self::UnusedScopedConceptBinding(ccid) => {
                write!(formatter, "scoped concept binding {ccid} does not occur in KU semantics")
            }
        }
    }
}

impl std::error::Error for ScopedConceptError {}

impl From<CanonicalError> for ScopedConceptError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<SemanticError> for ScopedConceptError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<ObjectError> for ScopedConceptError {
    fn from(error: ObjectError) -> Self {
        Self::Object(error)
    }
}

/// Canonical builder and validator for novel scoped concept definitions (`P1` §2.3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptDefinitionFrame {
    pub canonical_label: NormalizedText,
    pub definition_summary: NormalizedText,
    pub broader_concepts: Vec<ConceptCcid>,
    pub constraints: Vec<TypedConstraint>,
}

impl ConceptDefinitionFrame {
    pub fn new(
        canonical_label: impl Into<String>,
        definition_summary: impl Into<String>,
        mut broader_concepts: Vec<ConceptCcid>,
        constraints: Vec<TypedConstraint>,
    ) -> Result<Self, ScopedConceptError> {
        let canonical_label = NormalizedText::new(canonical_label)?;
        let definition_summary = NormalizedText::new(definition_summary)?;
        if canonical_label.as_str().trim().is_empty()
            || definition_summary.as_str().trim().is_empty()
        {
            return Err(ScopedConceptError::EmptyLabelOrSummary);
        }
        broader_concepts.sort();
        broader_concepts.dedup();
        if broader_concepts.len() > MAX_BROADER_CONCEPTS {
            return Err(ScopedConceptError::TooManyBroaderConcepts);
        }
        let frame = Self {
            canonical_label,
            definition_summary,
            broader_concepts,
            constraints,
        };
        let sfs = frame.to_semantic_frame_set()?;
        let _ = normalize_semantic_content(&sfs, SEMANTIC_CONTENT_PROFILE)?;
        Ok(frame)
    }

    pub fn to_semantic_frame_set(&self) -> Result<SemanticFrameSet, ScopedConceptError> {
        if self.canonical_label.as_str().trim().is_empty()
            || self.definition_summary.as_str().trim().is_empty()
        {
            return Err(ScopedConceptError::EmptyLabelOrSummary);
        }
        if self.broader_concepts.len() > MAX_BROADER_CONCEPTS {
            return Err(ScopedConceptError::TooManyBroaderConcepts);
        }
        let mut broader = self.broader_concepts.clone();
        broader.sort();
        broader.dedup();
        let mut arguments = Vec::with_capacity(2 + broader.len());
        arguments.push(TermRef::Literal(LiteralValue::Text(
            self.canonical_label.clone(),
        )));
        arguments.push(TermRef::Literal(LiteralValue::Text(
            self.definition_summary.clone(),
        )));
        for parent in broader {
            arguments.push(TermRef::Concept(parent));
        }
        let sfs = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: DEFINES_SCOPED_CONCEPT_CCID,
                arguments,
                constraints: self.constraints.clone(),
                qualifiers: StatementQualifiers::default(),
            }],
        };
        sfs.alpha_normalized().map_err(Into::into)
    }

    pub fn from_semantic_frame_set(frames: &SemanticFrameSet) -> Result<Self, ScopedConceptError> {
        let normalized = frames.alpha_normalized()?;
        let Some(first) = normalized.statements.first() else {
            return Err(ScopedConceptError::EmptyDefinition);
        };
        if normalized.statements.len() != 1 {
            return Err(ScopedConceptError::InvalidDefinitionArguments);
        }
        if first.operator_or_predicate != DEFINES_SCOPED_CONCEPT_CCID {
            return Err(ScopedConceptError::WrongDefinitionPredicate(
                first.operator_or_predicate,
            ));
        }
        if first.arguments.len() < 2 {
            return Err(ScopedConceptError::InvalidDefinitionArguments);
        }
        let TermRef::Literal(LiteralValue::Text(canonical_label)) = &first.arguments[0] else {
            return Err(ScopedConceptError::InvalidDefinitionArguments);
        };
        let TermRef::Literal(LiteralValue::Text(definition_summary)) = &first.arguments[1] else {
            return Err(ScopedConceptError::InvalidDefinitionArguments);
        };
        if canonical_label.as_str().trim().is_empty()
            || definition_summary.as_str().trim().is_empty()
        {
            return Err(ScopedConceptError::EmptyLabelOrSummary);
        }
        let mut broader_concepts = Vec::with_capacity(first.arguments.len() - 2);
        for arg in &first.arguments[2..] {
            match arg {
                TermRef::Concept(ccid) => broader_concepts.push(*ccid),
                _ => return Err(ScopedConceptError::InvalidDefinitionArguments),
            }
        }
        if broader_concepts.len() > MAX_BROADER_CONCEPTS {
            return Err(ScopedConceptError::TooManyBroaderConcepts);
        }
        let mut sorted = broader_concepts.clone();
        sorted.sort();
        sorted.dedup();
        if sorted != broader_concepts {
            return Err(ScopedConceptError::InvalidDefinitionArguments);
        }
        Ok(Self {
            canonical_label: canonical_label.clone(),
            definition_summary: definition_summary.clone(),
            broader_concepts,
            constraints: first.constraints.clone(),
        })
    }

    /// Validate a `SemanticFrameSet` as a `ConceptDefinitionFrame` and derive its
    /// deterministic `(ScopedConceptCcid, SemanticContentCid)`.
    pub fn validate_and_derive(
        frames: &SemanticFrameSet,
    ) -> Result<(ConceptCcid, SemanticContentCid), ScopedConceptError> {
        let parsed = Self::from_semantic_frame_set(frames)?;
        let canonical_frames = parsed.to_semantic_frame_set()?;
        let normalized = normalize_semantic_content(&canonical_frames, SEMANTIC_CONTENT_PROFILE)?;
        let scoped_ccid = derive_scoped_concept_ccid(&normalized.cid);
        Ok((scoped_ccid, normalized.cid))
    }

    pub fn derive_scoped_ccid(
        &self,
    ) -> Result<(ConceptCcid, SemanticContentCid), ScopedConceptError> {
        let sfs = self.to_semantic_frame_set()?;
        let normalized = normalize_semantic_content(&sfs, SEMANTIC_CONTENT_PROFILE)?;
        Ok((derive_scoped_concept_ccid(&normalized.cid), normalized.cid))
    }

    /// Encode this definition into a `KnowledgeObjectEnvelope` and compute its
    /// `(ObjectCid, ScopedConceptCcid, SemanticContentCid)`.
    pub fn to_knowledge_object(
        &self,
        disclosure: DisclosureClass,
    ) -> Result<
        (
            KnowledgeObjectEnvelope,
            ObjectCid,
            ConceptCcid,
            SemanticContentCid,
        ),
        ScopedConceptError,
    > {
        let sfs = self.to_semantic_frame_set()?;
        let normalized = normalize_semantic_content(&sfs, SEMANTIC_CONTENT_PROFILE)?;
        let scoped_ccid = derive_scoped_concept_ccid(&normalized.cid);
        let object = normalized.semantic.to_knowledge_object(disclosure)?;
        let (_, object_cid) = object.encode(ResourceProfile::ObjectV1)?;
        Ok((object, object_cid, scoped_ccid, normalized.cid))
    }
}

/// Encode `Vec<(ConceptCcid, ObjectCid)>` under `SCOPED_CONCEPT_EXTENSION_KEY`.
pub fn encode_scoped_concept_extension(
    bindings: &[(ConceptCcid, ObjectCid)],
) -> Result<(u64, CanonicalValue), ScopedConceptError> {
    if bindings.is_empty() || bindings.len() > MAX_SCOPED_CONCEPT_BINDINGS {
        return Err(ScopedConceptError::TooManyScopedBindings);
    }
    let mut by_ccid = BTreeMap::new();
    for &(scoped_ccid, def_cid) in bindings {
        if def_cid.as_bytes() == &[0u8; 32] {
            return Err(ScopedConceptError::MalformedExtension);
        }
        if let Some(existing) = by_ccid.insert(scoped_ccid, def_cid) {
            if existing != def_cid {
                return Err(ScopedConceptError::DuplicateScopedConceptBinding(
                    scoped_ccid,
                ));
            }
            return Err(ScopedConceptError::DuplicateScopedConceptBinding(
                scoped_ccid,
            ));
        }
    }
    let items = by_ccid
        .into_iter()
        .map(|(ccid, obj_cid)| {
            let entry = CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(ccid.as_bytes().to_vec())),
                (1, CanonicalValue::Bytes(obj_cid.as_bytes().to_vec())),
            ]);
            (entry.clone(), entry)
        })
        .collect();
    let canonical_items = canonicalize_set_by_key(items, ResourceProfile::ObjectV1)?;
    Ok((
        SCOPED_CONCEPT_EXTENSION_KEY,
        CanonicalValue::Array(canonical_items),
    ))
}

/// Decode `Vec<(ConceptCcid, ObjectCid)>` from `KnowledgeObjectEnvelope.extensions`.
/// Returns an empty `Vec` when the non-critical extension is absent.
pub fn decode_scoped_concept_extension(
    envelope: &KnowledgeObjectEnvelope,
) -> Result<Vec<(ConceptCcid, ObjectCid)>, ScopedConceptError> {
    let matches: Vec<_> = envelope
        .extensions
        .iter()
        .filter(|(key, _)| *key == SCOPED_CONCEPT_EXTENSION_KEY)
        .collect();
    if matches.is_empty() {
        return Ok(Vec::new());
    }
    if matches.len() > 1 {
        return Err(ScopedConceptError::DuplicateExtensionKey);
    }
    let value = &matches[0].1;
    let CanonicalValue::Array(entries) = value else {
        return Err(ScopedConceptError::MalformedExtension);
    };
    if entries.is_empty() || entries.len() > MAX_SCOPED_CONCEPT_BINDINGS {
        return Err(ScopedConceptError::MalformedExtension);
    }
    let mut parsed = Vec::with_capacity(entries.len());
    let mut seen = BTreeSet::new();
    for item in entries {
        let CanonicalValue::Map(fields) = item else {
            return Err(ScopedConceptError::MalformedExtension);
        };
        if fields.len() != 2 || fields[0].0 != 0 || fields[1].0 != 1 {
            return Err(ScopedConceptError::MalformedExtension);
        }
        let CanonicalValue::Bytes(ccid_bytes) = &fields[0].1 else {
            return Err(ScopedConceptError::MalformedExtension);
        };
        let CanonicalValue::Bytes(cid_bytes) = &fields[1].1 else {
            return Err(ScopedConceptError::MalformedExtension);
        };
        let ccid_arr: [u8; 16] = ccid_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ScopedConceptError::MalformedExtension)?;
        let cid_arr: [u8; 32] = cid_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ScopedConceptError::MalformedExtension)?;
        if cid_arr == [0u8; 32] {
            return Err(ScopedConceptError::MalformedExtension);
        }
        let ccid = ConceptCcid::from_bytes(ccid_arr);
        if !seen.insert(ccid) {
            return Err(ScopedConceptError::DuplicateScopedConceptBinding(ccid));
        }
        parsed.push((ccid, ObjectCid::from_bytes(cid_arr)));
    }
    let (_, reencoded) = encode_scoped_concept_extension(&parsed)?;
    if &reencoded != value {
        return Err(ScopedConceptError::MalformedExtension);
    }
    Ok(parsed)
}

/// Attach `SCOPED_CONCEPT_EXTENSION_KEY` and ensure every defining `ObjectCid` is
/// listed in `envelope.references` per `P1` §2.3.2.
pub fn attach_scoped_concept_bindings(
    envelope: &mut KnowledgeObjectEnvelope,
    bindings: &[(ConceptCcid, ObjectCid)],
) -> Result<(), ScopedConceptError> {
    if envelope
        .extensions
        .iter()
        .any(|(key, _)| *key == SCOPED_CONCEPT_EXTENSION_KEY)
    {
        return Err(ScopedConceptError::DuplicateExtensionKey);
    }
    let (ext_key, ext_val) = encode_scoped_concept_extension(bindings)?;
    envelope.extensions.push((ext_key, ext_val));
    envelope.extensions.sort_by_key(|(key, _)| *key);

    for &(_, def_cid) in bindings {
        let obj_ref = ObjectReference::new(0, def_cid.into_bytes());
        if !envelope.references.contains(&obj_ref) {
            envelope.references.push(obj_ref);
        }
    }
    Ok(())
}

/// Validate that every scoped concept binding on a KU is referenced in both
/// `semantic` and `envelope.references`, returning `DeferredMissingDependency`
/// if any defining `ObjectCid` has not yet arrived in `available_objects`.
pub fn verify_scoped_concept_dependencies(
    envelope: &KnowledgeObjectEnvelope,
    semantic: &SemanticFrameSet,
    available_objects: &[ObjectCid],
) -> Result<ScopedDependencyStatus, ScopedConceptError> {
    let bindings = decode_scoped_concept_extension(envelope)?;
    if bindings.is_empty() {
        return Ok(ScopedDependencyStatus::Satisfied);
    }
    let used_ccids = semantic.concept_ccids()?;
    let mut missing = BTreeSet::new();
    for (scoped_ccid, def_cid) in bindings {
        if !used_ccids.contains(&scoped_ccid) {
            return Err(ScopedConceptError::UnusedScopedConceptBinding(scoped_ccid));
        }
        let expected_ref = ObjectReference::new(0, def_cid.into_bytes());
        if !envelope.references.contains(&expected_ref) {
            return Err(ScopedConceptError::UnreferencedDefinitionObject {
                scoped_ccid,
                definition_object: def_cid,
            });
        }
        if !available_objects.contains(&def_cid) {
            missing.insert(def_cid.into_bytes());
        }
    }
    if missing.is_empty() {
        Ok(ScopedDependencyStatus::Satisfied)
    } else {
        Ok(ScopedDependencyStatus::DeferredMissingDependency {
            missing: missing.into_iter().map(ObjectCid::from_bytes).collect(),
        })
    }
}

/// Build a canonical `MappingKernel` (`KU-005`) establishing exact
/// `ConceptEquivalence(scoped_ccid <-> standard_ccid)` (`P1` §2.3.3).
pub fn build_concept_equivalence_mapping(
    scoped_def_obj: ObjectReference,
    scoped_ccid: ConceptCcid,
    standard_ku_or_def: ObjectReference,
    standard_ccid: ConceptCcid,
) -> Result<MappingKernel, MappingError> {
    if scoped_ccid == standard_ccid
        || scoped_def_obj == standard_ku_or_def
        || scoped_def_obj.cid == [0u8; 32]
        || standard_ku_or_def.cid == [0u8; 32]
    {
        return Err(MappingError::Semantic(SemanticError::InvalidField(
            "concept_equivalence.endpoints",
        )));
    }
    let assumptions = SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: DEFINES_SCOPED_CONCEPT_CCID,
            arguments: vec![
                TermRef::Concept(scoped_ccid),
                TermRef::Concept(standard_ccid),
            ],
            constraints: vec![TypedConstraint {
                expression: ConstraintExpression::Compare {
                    left: TermRef::Concept(scoped_ccid),
                    operator: ComparisonOperator::Equal,
                    right: TermRef::Concept(standard_ccid),
                },
                required: true,
            }],
            qualifiers: StatementQualifiers::default(),
        }],
    };
    let kernel = MappingKernel {
        source_objects: vec![scoped_def_obj.clone()],
        target_objects: vec![standard_ku_or_def.clone()],
        correspondences: vec![TermCorrespondence {
            source: MappingTermLocator {
                object: scoped_def_obj,
                statement_index: 0,
                argument_index: Some(0),
            },
            target: MappingTermLocator {
                object: standard_ku_or_def,
                statement_index: 0,
                argument_index: Some(0),
            },
            kind: CorrespondenceKind::Equivalent,
            transform: MappingTransform::Identity,
        }],
        assumptions,
        constraint_regions: Vec::new(),
        unmapped_regions: Vec::new(),
    };
    let _ = kernel.cid()?;
    Ok(kernel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::{
        decode_knowledge_object, KnownObjectKind, SEMANTIC_KERNEL_OBJECT_KIND,
    };

    #[test]
    fn defines_scoped_concept_constant_matches_domain_blake3() {
        let digest = blake3::hash(DEFINES_SCOPED_CONCEPT_DOMAIN);
        assert_eq!(
            DEFINES_SCOPED_CONCEPT_CCID.as_bytes(),
            &digest.as_bytes()[..16]
        );
    }

    #[test]
    fn definition_frame_derives_deterministic_scoped_ccid_and_dependencies() {
        let def = ConceptDefinitionFrame::new(
            "exciton-condensate-phase-alpha",
            "Metastable bilayer exciton condensate observed below 4 K",
            vec![ConceptCcid::from_bytes([7; 16])],
            Vec::new(),
        )
        .unwrap();
        let (def_obj, def_obj_cid, scoped_ccid, sem_cid) =
            def.to_knowledge_object(DisclosureClass::Public).unwrap();
        assert_eq!(scoped_ccid, derive_scoped_concept_ccid(&sem_cid));
        let _ = def_obj.encode(ResourceProfile::ObjectV1).unwrap();

        // Build a KU referencing `scoped_ccid` and attach extension + references.
        let ku_sfs = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: ConceptCcid::from_bytes([2; 16]),
                arguments: vec![TermRef::Concept(scoped_ccid)],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        };
        let mut ku_obj = ku_sfs.to_knowledge_object(DisclosureClass::Public).unwrap();
        attach_scoped_concept_bindings(&mut ku_obj, &[(scoped_ccid, def_obj_cid)]).unwrap();
        let (ku_bytes, _) = ku_obj.encode(ResourceProfile::ObjectV1).unwrap();
        let decoded = decode_knowledge_object(
            &ku_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(SEMANTIC_KERNEL_OBJECT_KIND, 1)],
            &[],
        )
        .unwrap();
        let crate::foundation::ObjectSemantics::Known(decoded_env) = decoded.semantics() else {
            panic!("expected known semantic kernel");
        };
        assert_eq!(
            decode_scoped_concept_extension(decoded_env).unwrap(),
            vec![(scoped_ccid, def_obj_cid)]
        );

        // Missing definition object yields DeferredMissingDependency; once present, Satisfied.
        assert_eq!(
            verify_scoped_concept_dependencies(decoded_env, &ku_sfs, &[]).unwrap(),
            ScopedDependencyStatus::DeferredMissingDependency {
                missing: vec![def_obj_cid]
            }
        );
        let available = [def_obj_cid];
        assert_eq!(
            verify_scoped_concept_dependencies(decoded_env, &ku_sfs, &available).unwrap(),
            ScopedDependencyStatus::Satisfied
        );

        // Omitting the defining ObjectCid from `references` fails closed.
        let mut missing_ref = decoded_env.clone();
        missing_ref.references.clear();
        assert!(matches!(
            verify_scoped_concept_dependencies(&missing_ref, &ku_sfs, &available),
            Err(ScopedConceptError::UnreferencedDefinitionObject { .. })
        ));
    }

    #[test]
    fn concept_equivalence_mapping_uses_equivalent_identity_correspondence() {
        let scoped_ccid = ConceptCcid::from_bytes([0x11; 16]);
        let standard_ccid = ConceptCcid::from_bytes([0x22; 16]);
        let kernel = build_concept_equivalence_mapping(
            ObjectReference::new(0, [0xAA; 32]),
            scoped_ccid,
            ObjectReference::new(0, [0xBB; 32]),
            standard_ccid,
        )
        .unwrap();
        assert_eq!(kernel.correspondences.len(), 1);
        assert_eq!(
            kernel.correspondences[0].kind,
            CorrespondenceKind::Equivalent
        );
        assert_eq!(
            kernel.correspondences[0].transform,
            MappingTransform::Identity
        );
        assert!(kernel.cid().is_ok());
    }
}
