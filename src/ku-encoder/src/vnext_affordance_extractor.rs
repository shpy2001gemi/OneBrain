//! Deterministic local projection of vNext Knowledge Affordances.
//!
//! This module has no model or embedding dependency. Derived affordances are
//! constructed only by copying a typed evidence snapshot, so the projection
//! cannot silently add a role, input, semantic frame, or abstraction pattern.

use ku_core::foundation::{
    AcceptedInput, AffordanceError, AffordanceOrigin, AffordanceSemantics, ConceptCcid,
    DisclosureClass, KnowledgeAffordance, KnowledgeObjectEnvelope, ObjectCid, ObjectError,
    ObjectReference, ResourceProfile, SemanticFrameSet, StatementLocator,
};

pub const RULE_BASED_AFFORDANCE_EXTRACTOR_PROFILE: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffordanceEvidenceKind {
    KnowledgeUnit,
    Assembly,
    Capability,
}

/// Exact evidence from one immutable source. The rule-based extractor copies
/// these fields; it never accepts a separately supplied derived output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffordanceEvidenceSnapshot {
    pub evidence_kind: AffordanceEvidenceKind,
    pub source: ObjectReference,
    pub offered_roles: Vec<ConceptCcid>,
    pub accepted_inputs: Vec<AcceptedInput>,
    pub semantics: AffordanceSemantics,
    pub abstraction_patterns: Vec<SemanticFrameSet>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplicitAffordanceDraft {
    pub sources: Vec<ObjectReference>,
    pub offered_roles: Vec<ConceptCcid>,
    pub accepted_inputs: Vec<AcceptedInput>,
    pub semantics: AffordanceSemantics,
    pub abstraction_patterns: Vec<SemanticFrameSet>,
    pub author_claims: Vec<StatementLocator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffordanceExtractionDraft {
    Explicit(ExplicitAffordanceDraft),
    Derived {
        evidence: AffordanceEvidenceSnapshot,
        /// Other immutable KU/Assembly/Capability objects that affected the
        /// projection. The evidence source is inserted automatically.
        contextual_inputs: Vec<ObjectReference>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AffordanceExtractionTrace {
    profile_version: u64,
    evidence_kind: Option<AffordanceEvidenceKind>,
    derivation_engine: Option<ObjectReference>,
    derivation_rule: Option<ObjectReference>,
    derivation_rule_version: Option<u64>,
    projected_source: Option<ObjectReference>,
}

impl AffordanceExtractionTrace {
    pub const fn profile_version(&self) -> u64 {
        self.profile_version
    }

    pub const fn evidence_kind(&self) -> Option<AffordanceEvidenceKind> {
        self.evidence_kind
    }

    pub const fn derivation_engine(&self) -> Option<&ObjectReference> {
        self.derivation_engine.as_ref()
    }

    pub const fn derivation_rule(&self) -> Option<&ObjectReference> {
        self.derivation_rule.as_ref()
    }

    pub const fn derivation_rule_version(&self) -> Option<u64> {
        self.derivation_rule_version
    }

    pub const fn projected_source(&self) -> Option<&ObjectReference> {
        self.projected_source.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedAffordance {
    affordance: KnowledgeAffordance,
    trace: AffordanceExtractionTrace,
    object: KnowledgeObjectEnvelope,
    bytes: Vec<u8>,
    cid: ObjectCid,
}

impl ExtractedAffordance {
    pub const fn affordance(&self) -> &KnowledgeAffordance {
        &self.affordance
    }

    pub const fn trace(&self) -> &AffordanceExtractionTrace {
        &self.trace
    }

    pub const fn disclosure(&self) -> DisclosureClass {
        self.object.disclosure
    }

    pub const fn object(&self) -> &KnowledgeObjectEnvelope {
        &self.object
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub const fn cid(&self) -> ObjectCid {
        self.cid
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AffordanceExtractionError {
    MissingAuthorClaim,
    AuthorClaimOutsideSources,
    MissingDerivedEvidenceRole,
    MissingDerivationInput,
    InvalidRuleVersion,
    Affordance(AffordanceError),
    Object(ObjectError),
}

impl std::fmt::Display for AffordanceExtractionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "AFFORDANCE_EXTRACTION: {self:?}")
    }
}

impl std::error::Error for AffordanceExtractionError {}

/// Offline rule-based extractor. The immutable rule reference is the semantic
/// version identity; `derivation_rule_version` is retained as an audit label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleBasedAffordanceExtractor {
    derivation_engine: ObjectReference,
    derivation_rule: ObjectReference,
    derivation_rule_version: u64,
}

impl RuleBasedAffordanceExtractor {
    pub fn new(
        derivation_engine: ObjectReference,
        derivation_rule: ObjectReference,
        derivation_rule_version: u64,
    ) -> Result<Self, AffordanceExtractionError> {
        if derivation_rule_version == 0 {
            return Err(AffordanceExtractionError::InvalidRuleVersion);
        }
        Ok(Self {
            derivation_engine,
            derivation_rule,
            derivation_rule_version,
        })
    }

    pub fn extract(
        &self,
        draft: AffordanceExtractionDraft,
    ) -> Result<ExtractedAffordance, AffordanceExtractionError> {
        let (affordance, trace) = match draft {
            AffordanceExtractionDraft::Explicit(draft) => self.explicit(draft)?,
            AffordanceExtractionDraft::Derived {
                evidence,
                contextual_inputs,
            } => self.derived(evidence, contextual_inputs)?,
        };

        // Extraction is local. Publication is a separate policy operation.
        let object = affordance
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .map_err(AffordanceExtractionError::Affordance)?;
        let (bytes, cid) = object
            .encode(ResourceProfile::ObjectV1)
            .map_err(AffordanceExtractionError::Object)?;
        Ok(ExtractedAffordance {
            affordance,
            trace,
            object,
            bytes,
            cid,
        })
    }

    fn explicit(
        &self,
        draft: ExplicitAffordanceDraft,
    ) -> Result<(KnowledgeAffordance, AffordanceExtractionTrace), AffordanceExtractionError> {
        if draft.author_claims.is_empty() {
            return Err(AffordanceExtractionError::MissingAuthorClaim);
        }
        if draft
            .author_claims
            .iter()
            .any(|claim| !draft.sources.contains(&claim.object))
        {
            return Err(AffordanceExtractionError::AuthorClaimOutsideSources);
        }
        Ok((
            KnowledgeAffordance {
                sources: draft.sources,
                offered_roles: draft.offered_roles,
                accepted_inputs: draft.accepted_inputs,
                semantics: draft.semantics,
                abstraction_patterns: draft.abstraction_patterns,
                origin: AffordanceOrigin::Explicit {
                    claims: draft.author_claims,
                },
            },
            AffordanceExtractionTrace {
                profile_version: RULE_BASED_AFFORDANCE_EXTRACTOR_PROFILE,
                evidence_kind: None,
                derivation_engine: None,
                derivation_rule: None,
                derivation_rule_version: None,
                projected_source: None,
            },
        ))
    }

    fn derived(
        &self,
        evidence: AffordanceEvidenceSnapshot,
        mut contextual_inputs: Vec<ObjectReference>,
    ) -> Result<(KnowledgeAffordance, AffordanceExtractionTrace), AffordanceExtractionError> {
        if evidence.offered_roles.is_empty() {
            return Err(AffordanceExtractionError::MissingDerivedEvidenceRole);
        }
        contextual_inputs.push(evidence.source.clone());
        canonicalize_references(&mut contextual_inputs);
        if contextual_inputs.is_empty() {
            return Err(AffordanceExtractionError::MissingDerivationInput);
        }

        // The projection is intentionally a move of exact evidence fields.
        // There is no second "derived output" argument that could exceed them.
        let source = evidence.source.clone();
        let evidence_kind = evidence.evidence_kind;
        Ok((
            KnowledgeAffordance {
                sources: vec![evidence.source],
                offered_roles: evidence.offered_roles,
                accepted_inputs: evidence.accepted_inputs,
                semantics: evidence.semantics,
                abstraction_patterns: evidence.abstraction_patterns,
                origin: AffordanceOrigin::Derived {
                    derivation_engine: self.derivation_engine.clone(),
                    derivation_rule: self.derivation_rule.clone(),
                    inputs: contextual_inputs,
                },
            },
            AffordanceExtractionTrace {
                profile_version: RULE_BASED_AFFORDANCE_EXTRACTOR_PROFILE,
                evidence_kind: Some(evidence_kind),
                derivation_engine: Some(self.derivation_engine.clone()),
                derivation_rule: Some(self.derivation_rule.clone()),
                derivation_rule_version: Some(self.derivation_rule_version),
                projected_source: Some(source),
            },
        ))
    }
}

fn canonicalize_references(values: &mut Vec<ObjectReference>) {
    values.sort_by(|left, right| {
        (left.reference_kind, left.cid).cmp(&(right.reference_kind, right.cid))
    });
    values.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::{
        ReceptorSlotId, StatementFrame, StatementId, StatementQualifiers, TermRef,
    };

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn frames(marker: u8) -> SemanticFrameSet {
        SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(marker as u32),
                operator_or_predicate: concept(marker),
                arguments: vec![TermRef::Receptor {
                    slot: ReceptorSlotId(0),
                    expected_type: Some(concept(marker + 1)),
                }],
                constraints: Vec::new(),
                qualifiers: StatementQualifiers::default(),
            }],
        }
    }

    fn semantics() -> AffordanceSemantics {
        AffordanceSemantics {
            preconditions: frames(10),
            outputs: frames(20),
            effects: frames(30),
            properties: frames(40),
            invariants: frames(50),
            operating_conditions: frames(60),
            limits: frames(70),
        }
    }

    fn evidence(kind: AffordanceEvidenceKind) -> AffordanceEvidenceSnapshot {
        AffordanceEvidenceSnapshot {
            evidence_kind: kind,
            source: reference(1),
            offered_roles: vec![concept(2)],
            accepted_inputs: vec![AcceptedInput {
                receptor_definition: reference(3),
                role: concept(4),
                required: true,
            }],
            semantics: semantics(),
            abstraction_patterns: vec![frames(80)],
        }
    }

    fn extractor() -> RuleBasedAffordanceExtractor {
        RuleBasedAffordanceExtractor::new(reference(90), reference(91), 7).unwrap()
    }

    #[test]
    fn derived_projection_copies_exact_evidence_and_stays_private() {
        let evidence = evidence(AffordanceEvidenceKind::KnowledgeUnit);
        let expected_roles = evidence.offered_roles.clone();
        let expected_inputs = evidence.accepted_inputs.clone();
        let expected_semantics = evidence.semantics.clone();
        let value = extractor()
            .extract(AffordanceExtractionDraft::Derived {
                evidence,
                contextual_inputs: vec![reference(8)],
            })
            .unwrap();
        assert_eq!(value.disclosure(), DisclosureClass::LocalOnly);
        assert_eq!(value.affordance().offered_roles, expected_roles);
        assert_eq!(value.affordance().accepted_inputs, expected_inputs);
        assert_eq!(value.affordance().semantics, expected_semantics);
        assert_eq!(
            value.trace().evidence_kind(),
            Some(AffordanceEvidenceKind::KnowledgeUnit)
        );
        assert_eq!(value.trace().derivation_rule_version(), Some(7));
    }

    #[test]
    fn rebuild_and_input_order_produce_the_same_object() {
        let first = extractor()
            .extract(AffordanceExtractionDraft::Derived {
                evidence: evidence(AffordanceEvidenceKind::Assembly),
                contextual_inputs: vec![reference(8), reference(7)],
            })
            .unwrap();
        let rebuilt = extractor()
            .extract(AffordanceExtractionDraft::Derived {
                evidence: evidence(AffordanceEvidenceKind::Assembly),
                contextual_inputs: vec![reference(7), reference(8), reference(7)],
            })
            .unwrap();
        assert_eq!(first.cid(), rebuilt.cid());
        assert_eq!(first.bytes(), rebuilt.bytes());
    }

    #[test]
    fn immutable_rule_reference_is_identity_bearing() {
        let first = extractor()
            .extract(AffordanceExtractionDraft::Derived {
                evidence: evidence(AffordanceEvidenceKind::Capability),
                contextual_inputs: Vec::new(),
            })
            .unwrap();
        let next_rule = RuleBasedAffordanceExtractor::new(reference(90), reference(92), 8)
            .unwrap()
            .extract(AffordanceExtractionDraft::Derived {
                evidence: evidence(AffordanceEvidenceKind::Capability),
                contextual_inputs: Vec::new(),
            })
            .unwrap();
        assert_ne!(first.cid(), next_rule.cid());
    }

    #[test]
    fn derived_projection_requires_a_role_present_in_evidence() {
        let mut no_role = evidence(AffordanceEvidenceKind::KnowledgeUnit);
        no_role.offered_roles.clear();
        assert_eq!(
            extractor()
                .extract(AffordanceExtractionDraft::Derived {
                    evidence: no_role,
                    contextual_inputs: Vec::new(),
                })
                .unwrap_err(),
            AffordanceExtractionError::MissingDerivedEvidenceRole
        );
    }

    #[test]
    fn explicit_affordance_requires_claims_inside_its_sources() {
        let missing = ExplicitAffordanceDraft {
            sources: vec![reference(1)],
            offered_roles: vec![concept(2)],
            accepted_inputs: Vec::new(),
            semantics: semantics(),
            abstraction_patterns: Vec::new(),
            author_claims: Vec::new(),
        };
        assert_eq!(
            extractor()
                .extract(AffordanceExtractionDraft::Explicit(missing))
                .unwrap_err(),
            AffordanceExtractionError::MissingAuthorClaim
        );

        let outside = ExplicitAffordanceDraft {
            sources: vec![reference(1)],
            offered_roles: vec![concept(2)],
            accepted_inputs: Vec::new(),
            semantics: semantics(),
            abstraction_patterns: Vec::new(),
            author_claims: vec![StatementLocator {
                object: reference(9),
                statement_index: 0,
            }],
        };
        assert_eq!(
            extractor()
                .extract(AffordanceExtractionDraft::Explicit(outside))
                .unwrap_err(),
            AffordanceExtractionError::AuthorClaimOutsideSources
        );
    }

    #[test]
    fn explicit_author_affordance_is_encoded_without_ai_runtime() {
        let value = extractor()
            .extract(AffordanceExtractionDraft::Explicit(
                ExplicitAffordanceDraft {
                    sources: vec![reference(1)],
                    offered_roles: vec![concept(2)],
                    accepted_inputs: Vec::new(),
                    semantics: semantics(),
                    abstraction_patterns: Vec::new(),
                    author_claims: vec![StatementLocator {
                        object: reference(1),
                        statement_index: 0,
                    }],
                },
            ))
            .unwrap();
        assert_eq!(value.disclosure(), DisclosureClass::LocalOnly);
        assert_eq!(value.trace().evidence_kind(), None);
        assert!(!value.bytes().is_empty());
    }
}
