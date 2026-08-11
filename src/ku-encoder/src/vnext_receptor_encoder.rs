//! Local, provenance-preserving encoder for vNext Receptor definitions.
//!
//! The encoder is deliberately deterministic and model-agnostic. An AI may
//! propose a draft, but only already-resolved CCIDs and immutable object
//! references cross this boundary. Missing fields become explicit limitations;
//! they are never filled with guessed identifiers.

use std::collections::BTreeSet;

use ku_core::foundation::{
    ConceptCcid, DisclosureClass, KnowledgeObjectEnvelope, ObjectCid, ObjectError, ObjectReference,
    ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorDefinition, ReceptorError,
    ReceptorOrigin, ResourceProfile, SourceSpan, StatementLocator, TypedConstraint,
};

/// Whether the extraction stage inspected all constraints represented by its
/// source. This is an encoding-coverage statement, not epistemic truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintCoverage {
    CompleteForSource,
    Partial,
    Unknown,
}

/// Typed limitations retained beside the encoded object in the local trace.
/// They intentionally do not become fields in the immutable Receptor schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EncodingLimitation {
    MissingRole,
    MissingAcceptancePolicy,
    MissingDeclaredSourceSpan,
    MissingOriginInputs,
    SourceSpanUnavailable,
    ExpectedTypesUnresolved,
    ConstraintCoveragePartial,
    ConstraintCoverageUnknown,
    DisclosureDowngradedToLocal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceptorOriginKind {
    Declared,
    Derived,
    Emergent,
}

/// Provenance supplied by the extraction stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceptorOriginDraft {
    Declared {
        source: StatementLocator,
        source_span: Option<SourceSpan>,
    },
    Derived {
        derivation_rule: ObjectReference,
        inputs: Vec<ObjectReference>,
        evidence_spans: Vec<SourceSpan>,
    },
    Emergent {
        detector: ObjectReference,
        observations: Vec<ObjectReference>,
        evidence_spans: Vec<SourceSpan>,
    },
}

impl ReceptorOriginDraft {
    pub const fn kind(&self) -> ReceptorOriginKind {
        match self {
            Self::Declared { .. } => ReceptorOriginKind::Declared,
            Self::Derived { .. } => ReceptorOriginKind::Derived,
            Self::Emergent { .. } => ReceptorOriginKind::Emergent,
        }
    }
}

/// A typed draft. Optional fields are intentional: the encoder can report an
/// incomplete extraction without fabricating a valid-looking definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorEncodingDraft {
    pub role: Option<ConceptCcid>,
    pub expected_types: Vec<ConceptCcid>,
    pub hard_constraints: Vec<TypedConstraint>,
    pub cardinality: ReceptorCardinality,
    pub origin: ReceptorOriginDraft,
    pub acceptance: Option<ReceptorAcceptanceProfile>,
    pub constraint_coverage: ConstraintCoverage,
    /// `None` is the privacy-preserving default. Public output is accepted only
    /// for a declared receptor; derived and emergent output stays local.
    pub requested_disclosure: Option<DisclosureClass>,
    /// Limitations detected by an upstream local extractor.
    pub declared_limitations: Vec<EncodingLimitation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceptorEncodingTrace {
    origin_kind: ReceptorOriginKind,
    source_spans: Vec<SourceSpan>,
    limitations: Vec<EncodingLimitation>,
}

impl ReceptorEncodingTrace {
    pub const fn origin_kind(&self) -> ReceptorOriginKind {
        self.origin_kind
    }

    pub fn source_spans(&self) -> &[SourceSpan] {
        &self.source_spans
    }

    pub fn limitations(&self) -> &[EncodingLimitation] {
        &self.limitations
    }

    pub fn has_known_limitations(&self) -> bool {
        !self.limitations.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncompleteReceptorEncoding {
    trace: ReceptorEncodingTrace,
}

impl IncompleteReceptorEncoding {
    pub const fn trace(&self) -> &ReceptorEncodingTrace {
        &self.trace
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedReceptor {
    definition: ReceptorDefinition,
    disclosure: DisclosureClass,
    trace: ReceptorEncodingTrace,
    object: KnowledgeObjectEnvelope,
    bytes: Vec<u8>,
    cid: ObjectCid,
}

impl EncodedReceptor {
    pub const fn definition(&self) -> &ReceptorDefinition {
        &self.definition
    }

    pub const fn disclosure(&self) -> DisclosureClass {
        self.disclosure
    }

    pub const fn trace(&self) -> &ReceptorEncodingTrace {
        &self.trace
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
pub enum ReceptorEncodingOutcome {
    Encoded(Box<EncodedReceptor>),
    Incomplete(IncompleteReceptorEncoding),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceptorEncodingError {
    EmptySourceSpan,
    SourceSpanOutsideOrigin,
    RouteMinimalIsNotObjectStorage,
    Receptor(ReceptorError),
    Object(ObjectError),
}

impl std::fmt::Display for ReceptorEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "RECEPTOR_ENCODING: {self:?}")
    }
}

impl std::error::Error for ReceptorEncodingError {}

/// Deterministic boundary between local extraction and immutable KU objects.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReceptorEncoder;

impl ReceptorEncoder {
    pub fn encode(
        &self,
        draft: ReceptorEncodingDraft,
    ) -> Result<ReceptorEncodingOutcome, ReceptorEncodingError> {
        if draft.requested_disclosure == Some(DisclosureClass::RouteMinimal) {
            return Err(ReceptorEncodingError::RouteMinimalIsNotObjectStorage);
        }

        let mut limitations: BTreeSet<_> = draft.declared_limitations.into_iter().collect();
        if draft.role.is_none() {
            limitations.insert(EncodingLimitation::MissingRole);
        }
        if draft.acceptance.is_none() {
            limitations.insert(EncodingLimitation::MissingAcceptancePolicy);
        }
        if draft.expected_types.is_empty() {
            limitations.insert(EncodingLimitation::ExpectedTypesUnresolved);
        }
        match draft.constraint_coverage {
            ConstraintCoverage::CompleteForSource => {}
            ConstraintCoverage::Partial => {
                limitations.insert(EncodingLimitation::ConstraintCoveragePartial);
            }
            ConstraintCoverage::Unknown => {
                limitations.insert(EncodingLimitation::ConstraintCoverageUnknown);
            }
        }

        let (origin, mut source_spans, origin_incomplete) =
            validate_origin(draft.origin, &mut limitations)?;
        canonicalize_spans(&mut source_spans);

        let origin_kind = match &origin {
            ReceptorOrigin::Declared { .. } => ReceptorOriginKind::Declared,
            ReceptorOrigin::Derived { .. } => ReceptorOriginKind::Derived,
            ReceptorOrigin::Emergent { .. } => ReceptorOriginKind::Emergent,
        };
        let disclosure =
            disclosure_for_origin(origin_kind, draft.requested_disclosure, &mut limitations);
        let trace = ReceptorEncodingTrace {
            origin_kind,
            source_spans,
            limitations: limitations.iter().copied().collect(),
        };

        let Some(role) = draft.role else {
            return Ok(ReceptorEncodingOutcome::Incomplete(
                IncompleteReceptorEncoding { trace },
            ));
        };
        let Some(acceptance) = draft.acceptance else {
            return Ok(ReceptorEncodingOutcome::Incomplete(
                IncompleteReceptorEncoding { trace },
            ));
        };
        if origin_incomplete {
            return Ok(ReceptorEncodingOutcome::Incomplete(
                IncompleteReceptorEncoding { trace },
            ));
        }

        let definition = ReceptorDefinition {
            role,
            expected_types: draft.expected_types,
            hard_constraints: draft.hard_constraints,
            cardinality: draft.cardinality,
            origin,
            acceptance,
        };
        let object = definition
            .to_knowledge_object(disclosure)
            .map_err(ReceptorEncodingError::Receptor)?;
        let (bytes, cid) = object
            .encode(ResourceProfile::ObjectV1)
            .map_err(ReceptorEncodingError::Object)?;
        Ok(ReceptorEncodingOutcome::Encoded(Box::new(
            EncodedReceptor {
                definition,
                disclosure,
                trace,
                object,
                bytes,
                cid,
            },
        )))
    }
}

fn validate_origin(
    draft: ReceptorOriginDraft,
    limitations: &mut BTreeSet<EncodingLimitation>,
) -> Result<(ReceptorOrigin, Vec<SourceSpan>, bool), ReceptorEncodingError> {
    match draft {
        ReceptorOriginDraft::Declared {
            source,
            source_span,
        } => {
            let Some(span) = source_span else {
                limitations.insert(EncodingLimitation::MissingDeclaredSourceSpan);
                return Ok((ReceptorOrigin::Declared { source }, Vec::new(), true));
            };
            validate_span(&span)?;
            if span.source != source.object {
                return Err(ReceptorEncodingError::SourceSpanOutsideOrigin);
            }
            Ok((ReceptorOrigin::Declared { source }, vec![span], false))
        }
        ReceptorOriginDraft::Derived {
            derivation_rule,
            inputs,
            evidence_spans,
        } => {
            let incomplete = inputs.is_empty();
            if incomplete {
                limitations.insert(EncodingLimitation::MissingOriginInputs);
            }
            validate_evidence_spans(&evidence_spans, &inputs, limitations)?;
            Ok((
                ReceptorOrigin::Derived {
                    derivation_rule,
                    inputs,
                },
                evidence_spans,
                incomplete,
            ))
        }
        ReceptorOriginDraft::Emergent {
            detector,
            observations,
            evidence_spans,
        } => {
            let incomplete = observations.is_empty();
            if incomplete {
                limitations.insert(EncodingLimitation::MissingOriginInputs);
            }
            validate_evidence_spans(&evidence_spans, &observations, limitations)?;
            Ok((
                ReceptorOrigin::Emergent {
                    detector,
                    observations,
                },
                evidence_spans,
                incomplete,
            ))
        }
    }
}

fn validate_evidence_spans(
    spans: &[SourceSpan],
    origin_inputs: &[ObjectReference],
    limitations: &mut BTreeSet<EncodingLimitation>,
) -> Result<(), ReceptorEncodingError> {
    if spans.is_empty() {
        limitations.insert(EncodingLimitation::SourceSpanUnavailable);
    }
    for span in spans {
        validate_span(span)?;
        if !origin_inputs.contains(&span.source) {
            return Err(ReceptorEncodingError::SourceSpanOutsideOrigin);
        }
    }
    Ok(())
}

fn validate_span(span: &SourceSpan) -> Result<(), ReceptorEncodingError> {
    if span.start >= span.end {
        Err(ReceptorEncodingError::EmptySourceSpan)
    } else {
        Ok(())
    }
}

fn canonicalize_spans(spans: &mut Vec<SourceSpan>) {
    spans.sort_by(|left, right| {
        (
            left.source.reference_kind,
            left.source.cid,
            left.start,
            left.end,
        )
            .cmp(&(
                right.source.reference_kind,
                right.source.cid,
                right.start,
                right.end,
            ))
    });
    spans.dedup();
}

fn disclosure_for_origin(
    origin: ReceptorOriginKind,
    requested: Option<DisclosureClass>,
    limitations: &mut BTreeSet<EncodingLimitation>,
) -> DisclosureClass {
    match origin {
        ReceptorOriginKind::Declared => requested.unwrap_or(DisclosureClass::LocalOnly),
        ReceptorOriginKind::Derived | ReceptorOriginKind::Emergent => {
            if requested.is_some_and(|value| value != DisclosureClass::LocalOnly) {
                limitations.insert(EncodingLimitation::DisclosureDowngradedToLocal);
            }
            DisclosureClass::LocalOnly
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::{
        decode_knowledge_object, KnownObjectKind, ObjectSemantics, UnknownConstraintPolicy,
        RECEPTOR_DEFINITION_KIND,
    };
    use serde::Deserialize;

    fn concept(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn acceptance() -> ReceptorAcceptanceProfile {
        ReceptorAcceptanceProfile {
            policy: reference(90),
            required_evidence_kinds: vec![concept(91)],
            unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
        }
    }

    fn draft(origin: ReceptorOriginDraft) -> ReceptorEncodingDraft {
        ReceptorEncodingDraft {
            role: Some(concept(1)),
            expected_types: vec![concept(2)],
            hard_constraints: Vec::new(),
            cardinality: ReceptorCardinality::new(1, Some(1)).unwrap(),
            origin,
            acceptance: Some(acceptance()),
            constraint_coverage: ConstraintCoverage::CompleteForSource,
            requested_disclosure: None,
            declared_limitations: Vec::new(),
        }
    }

    fn encoded(draft: ReceptorEncodingDraft) -> EncodedReceptor {
        match ReceptorEncoder.encode(draft).unwrap() {
            ReceptorEncodingOutcome::Encoded(value) => *value,
            ReceptorEncodingOutcome::Incomplete(_) => panic!("expected encoded receptor"),
        }
    }

    #[derive(Deserialize)]
    struct Corpus {
        profile: String,
        cases: Vec<CorpusCase>,
    }

    #[derive(Deserialize)]
    struct CorpusCase {
        name: String,
        origin: String,
        requested_disclosure: String,
        expected_disclosure: String,
    }

    #[test]
    fn frozen_corpus_round_trips_as_known_receptor_objects() {
        let corpus: Corpus = serde_json::from_str(include_str!(
            "../../test-vectors/vnext/ai/receptor-encoding-v1.json"
        ))
        .unwrap();
        assert_eq!(corpus.profile, "ReceptorEncodingCorpus/1");
        for case in corpus.cases {
            let source = reference(10);
            let origin = match case.origin.as_str() {
                "declared" => ReceptorOriginDraft::Declared {
                    source: StatementLocator {
                        object: source.clone(),
                        statement_index: 4,
                    },
                    source_span: Some(SourceSpan {
                        source: source.clone(),
                        start: 12,
                        end: 37,
                    }),
                },
                "derived" => ReceptorOriginDraft::Derived {
                    derivation_rule: reference(11),
                    inputs: vec![source.clone()],
                    evidence_spans: vec![SourceSpan {
                        source: source.clone(),
                        start: 12,
                        end: 37,
                    }],
                },
                "emergent" => ReceptorOriginDraft::Emergent {
                    detector: reference(12),
                    observations: vec![source.clone()],
                    evidence_spans: vec![SourceSpan {
                        source,
                        start: 12,
                        end: 37,
                    }],
                },
                other => panic!("unknown corpus origin: {other}"),
            };
            let requested = match case.requested_disclosure.as_str() {
                "default" => None,
                "public" => Some(DisclosureClass::Public),
                other => panic!("unknown requested disclosure: {other}"),
            };
            let mut value = draft(origin);
            value.requested_disclosure = requested;
            let value = encoded(value);
            let expected = match case.expected_disclosure.as_str() {
                "local-only" => DisclosureClass::LocalOnly,
                "public" => DisclosureClass::Public,
                other => panic!("unknown expected disclosure: {other}"),
            };
            assert_eq!(value.disclosure(), expected, "{}", case.name);
            let decoded = decode_knowledge_object(
                value.bytes(),
                ResourceProfile::ObjectV1,
                &[KnownObjectKind::new(RECEPTOR_DEFINITION_KIND, 1)],
                &[],
            )
            .unwrap();
            assert_eq!(decoded.cid(), value.cid(), "{}", case.name);
            assert!(matches!(decoded.semantics(), ObjectSemantics::Known(_)));
        }
    }

    #[test]
    fn omission_returns_incomplete_without_fabricating_role() {
        let source = reference(10);
        let mut value = draft(ReceptorOriginDraft::Declared {
            source: StatementLocator {
                object: source.clone(),
                statement_index: 0,
            },
            source_span: Some(SourceSpan {
                source,
                start: 0,
                end: 4,
            }),
        });
        value.role = None;
        let ReceptorEncodingOutcome::Incomplete(incomplete) =
            ReceptorEncoder.encode(value).unwrap()
        else {
            panic!("missing role must not produce an object")
        };
        assert!(incomplete
            .trace()
            .limitations()
            .contains(&EncodingLimitation::MissingRole));
    }

    #[test]
    fn missing_declared_span_is_incomplete() {
        let value = draft(ReceptorOriginDraft::Declared {
            source: StatementLocator {
                object: reference(10),
                statement_index: 0,
            },
            source_span: None,
        });
        let ReceptorEncodingOutcome::Incomplete(incomplete) =
            ReceptorEncoder.encode(value).unwrap()
        else {
            panic!("missing declared span must not produce an object")
        };
        assert!(incomplete
            .trace()
            .limitations()
            .contains(&EncodingLimitation::MissingDeclaredSourceSpan));
    }

    #[test]
    fn adversarial_empty_or_unrelated_span_is_rejected() {
        let source = reference(10);
        let empty = draft(ReceptorOriginDraft::Declared {
            source: StatementLocator {
                object: source.clone(),
                statement_index: 0,
            },
            source_span: Some(SourceSpan {
                source: source.clone(),
                start: 4,
                end: 4,
            }),
        });
        assert_eq!(
            ReceptorEncoder.encode(empty).unwrap_err(),
            ReceptorEncodingError::EmptySourceSpan
        );

        let unrelated = draft(ReceptorOriginDraft::Derived {
            derivation_rule: reference(11),
            inputs: vec![source],
            evidence_spans: vec![SourceSpan {
                source: reference(99),
                start: 0,
                end: 4,
            }],
        });
        assert_eq!(
            ReceptorEncoder.encode(unrelated).unwrap_err(),
            ReceptorEncodingError::SourceSpanOutsideOrigin
        );
    }

    #[test]
    fn derived_and_emergent_are_private_even_when_public_is_requested() {
        for origin in [
            ReceptorOriginDraft::Derived {
                derivation_rule: reference(11),
                inputs: vec![reference(10)],
                evidence_spans: Vec::new(),
            },
            ReceptorOriginDraft::Emergent {
                detector: reference(12),
                observations: vec![reference(10)],
                evidence_spans: Vec::new(),
            },
        ] {
            let mut value = draft(origin);
            value.requested_disclosure = Some(DisclosureClass::Public);
            let value = encoded(value);
            assert_eq!(value.disclosure(), DisclosureClass::LocalOnly);
            assert!(value
                .trace()
                .limitations()
                .contains(&EncodingLimitation::DisclosureDowngradedToLocal));
            assert!(value
                .trace()
                .limitations()
                .contains(&EncodingLimitation::SourceSpanUnavailable));
        }
    }

    #[test]
    fn limitations_are_deduplicated_and_never_claim_silent_completeness() {
        let source = reference(10);
        let mut value = draft(ReceptorOriginDraft::Declared {
            source: StatementLocator {
                object: source.clone(),
                statement_index: 0,
            },
            source_span: Some(SourceSpan {
                source,
                start: 0,
                end: 4,
            }),
        });
        value.expected_types.clear();
        value.constraint_coverage = ConstraintCoverage::Partial;
        value.declared_limitations = vec![
            EncodingLimitation::ExpectedTypesUnresolved,
            EncodingLimitation::ExpectedTypesUnresolved,
        ];
        let value = encoded(value);
        assert!(value.trace().has_known_limitations());
        assert_eq!(
            value
                .trace()
                .limitations()
                .iter()
                .filter(|item| **item == EncodingLimitation::ExpectedTypesUnresolved)
                .count(),
            1
        );
        assert!(value
            .trace()
            .limitations()
            .contains(&EncodingLimitation::ConstraintCoveragePartial));
    }
}
