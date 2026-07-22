//! Disclosure policy, local taint audit and fail-closed sanitizers.

use std::collections::BTreeSet;

use ku_core::foundation::schema_registry::OBJECT_KIND_SANITIZED_PUBLIC_PROBLEM;
use ku_core::foundation::{
    canonicalize_set_by_key, encode_canonical, CanonicalValue, ConceptCcid, DisclosureClass,
    KnowledgeObjectEnvelope, ObjectKind, ObjectReference, ResourceProfile, SchemaVersion,
};

use crate::vnext_query::{CoarseRouteToken, CoarseRouteTokenClass, MIN_ROUTE_TOKEN_SUPPORT};

pub const DISCLOSURE_PROFILE_MAJOR: u64 = 1;
pub const DISCLOSURE_PROFILE_MINOR: u64 = 0;
pub const SANITIZED_PUBLIC_PROBLEM_KIND: ObjectKind =
    ObjectKind(OBJECT_KIND_SANITIZED_PUBLIC_PROBLEM);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum DisclosureMode {
    LocalOnly = 0,
    RouteMinimal = 1,
    NegotiatedEncrypted = 2,
    PublicProblem = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentKind {
    Explicit,
    Standing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisclosureConsent {
    pub kind: ConsentKind,
    pub policy_ref: ObjectReference,
    pub mode: DisclosureMode,
    pub purpose: ConceptCcid,
    pub scope_commitment: [u8; 32],
    pub consent_commitment: [u8; 32],
    pub not_before: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisclosurePolicy {
    pub policy_ref: ObjectReference,
    pub default_mode: DisclosureMode,
    pub enabled_nonlocal_modes: Vec<DisclosureMode>,
    pub minimum_route_support: u64,
}

impl DisclosurePolicy {
    pub fn private_default(policy_ref: ObjectReference) -> Self {
        Self {
            policy_ref,
            default_mode: DisclosureMode::LocalOnly,
            enabled_nonlocal_modes: vec![],
            minimum_route_support: MIN_ROUTE_TOKEN_SUPPORT,
        }
    }

    pub fn authorize(
        &self,
        requested: DisclosureMode,
        consent: Option<&DisclosureConsent>,
        local_tick: u64,
    ) -> Result<(), DisclosureError> {
        if self.default_mode != DisclosureMode::LocalOnly
            || self.minimum_route_support < MIN_ROUTE_TOKEN_SUPPORT
            || has_duplicates(&self.enabled_nonlocal_modes)
        {
            return Err(DisclosureError::InvalidPolicy);
        }
        if requested == DisclosureMode::LocalOnly {
            return Ok(());
        }
        if !self.enabled_nonlocal_modes.contains(&requested) {
            return Err(DisclosureError::ModeDisabled);
        }
        let consent = consent.ok_or(DisclosureError::ConsentRequired)?;
        if consent.policy_ref != self.policy_ref
            || consent.mode != requested
            || consent.scope_commitment == [0; 32]
            || consent.consent_commitment == [0; 32]
            || consent.not_before >= consent.expires_at
            || local_tick < consent.not_before
            || local_tick >= consent.expires_at
        {
            return Err(DisclosureError::ConsentScopeMismatch);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaintLabel {
    RawText,
    StableReceptorId,
    StableAssemblyId,
    StableNeedId,
    StableUserId,
    StableNodeId,
    PrivateSourceReference,
    ExactNumberOrRange,
    RareConcept,
    ExactGraphConjunction,
    LocationOrTime,
    Hypothesis,
    AcceptanceTest,
    PrivateContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PrivateNeedMaterial {
    pub raw_text: Vec<u8>,
    pub stable_receptor_ids: Vec<[u8; 32]>,
    pub stable_assembly_ids: Vec<[u8; 32]>,
    pub stable_need_ids: Vec<[u8; 32]>,
    pub stable_user_ids: Vec<[u8; 32]>,
    pub stable_node_ids: Vec<[u8; 32]>,
    pub private_source_refs: Vec<ObjectReference>,
    pub exact_literals: Vec<Vec<u8>>,
    pub contains_location_or_time: bool,
    pub contains_hypothesis: bool,
    pub contains_acceptance_test: bool,
    pub contains_private_context: bool,
}

impl PrivateNeedMaterial {
    fn taints(&self, exact_conjunction_width: u16, rare: bool) -> BTreeSet<TaintLabel> {
        let mut labels = BTreeSet::new();
        if !self.raw_text.is_empty() {
            labels.insert(TaintLabel::RawText);
        }
        if !self.stable_receptor_ids.is_empty() {
            labels.insert(TaintLabel::StableReceptorId);
        }
        if !self.stable_assembly_ids.is_empty() {
            labels.insert(TaintLabel::StableAssemblyId);
        }
        if !self.stable_need_ids.is_empty() {
            labels.insert(TaintLabel::StableNeedId);
        }
        if !self.stable_user_ids.is_empty() {
            labels.insert(TaintLabel::StableUserId);
        }
        if !self.stable_node_ids.is_empty() {
            labels.insert(TaintLabel::StableNodeId);
        }
        if !self.private_source_refs.is_empty() {
            labels.insert(TaintLabel::PrivateSourceReference);
        }
        if !self.exact_literals.is_empty() {
            labels.insert(TaintLabel::ExactNumberOrRange);
        }
        if exact_conjunction_width > 1 {
            labels.insert(TaintLabel::ExactGraphConjunction);
        }
        if rare {
            labels.insert(TaintLabel::RareConcept);
        }
        if self.contains_location_or_time {
            labels.insert(TaintLabel::LocationOrTime);
        }
        if self.contains_hypothesis {
            labels.insert(TaintLabel::Hypothesis);
        }
        if self.contains_acceptance_test {
            labels.insert(TaintLabel::AcceptanceTest);
        }
        if self.contains_private_context {
            labels.insert(TaintLabel::PrivateContext);
        }
        labels
    }

    fn local_commitment(&self) -> Result<[u8; 32], DisclosureError> {
        let reference_values = self
            .private_source_refs
            .iter()
            .map(|reference| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(reference.reference_kind)),
                    (1, CanonicalValue::Bytes(reference.cid.to_vec())),
                ])
            })
            .collect::<Vec<_>>();
        let bytes = encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(self.raw_text.clone())),
                (1, bytes32_array(&self.stable_receptor_ids)),
                (2, bytes32_array(&self.stable_assembly_ids)),
                (3, bytes32_array(&self.stable_need_ids)),
                (4, bytes32_array(&self.stable_user_ids)),
                (5, bytes32_array(&self.stable_node_ids)),
                (6, CanonicalValue::Array(reference_values)),
                (
                    7,
                    CanonicalValue::Array(
                        self.exact_literals
                            .iter()
                            .cloned()
                            .map(CanonicalValue::Bytes)
                            .collect(),
                    ),
                ),
                (8, CanonicalValue::Bool(self.contains_location_or_time)),
                (9, CanonicalValue::Bool(self.contains_hypothesis)),
                (10, CanonicalValue::Bool(self.contains_acceptance_test)),
                (11, CanonicalValue::Bool(self.contains_private_context)),
            ]),
            ResourceProfile::ObjectV1,
        )?;
        Ok(domain_commitment(
            b"onebrain:vnext:private-disclosure-audit-input:1\0",
            &bytes,
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaintAudit {
    pub private_input_commitment: [u8; 32],
    pub observed: Vec<TaintLabel>,
    pub discharged_by_projection: Vec<TaintLabel>,
    pub remaining: Vec<TaintLabel>,
    pub generalized: bool,
}

impl TaintAudit {
    pub const fn is_local_private_state(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneralizedRouteToken {
    pub token: CoarseRouteToken,
    pub estimated_support: u64,
    pub ontology_distance: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDisclosureCandidate {
    pub token: CoarseRouteToken,
    pub estimated_support: u64,
    pub exact_conjunction_width: u16,
    pub generalizations: Vec<GeneralizedRouteToken>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SanitizedRouteProjection {
    pub token: CoarseRouteToken,
    pub estimated_support: u64,
    pub audit: TaintAudit,
}

impl SanitizedRouteProjection {
    pub const fn contains_stable_identity_raw_text_or_exact_conjunction(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteSanitization {
    Ready(SanitizedRouteProjection),
    Suppressed(TaintAudit),
}

pub struct DisclosureSanitizer;

impl DisclosureSanitizer {
    pub fn sanitize_route_minimal(
        policy: &DisclosurePolicy,
        consent: Option<&DisclosureConsent>,
        local_tick: u64,
        private: &PrivateNeedMaterial,
        candidate: &RouteDisclosureCandidate,
    ) -> Result<RouteSanitization, DisclosureError> {
        policy.authorize(DisclosureMode::RouteMinimal, consent, local_tick)?;
        if candidate.token.allowlisted_code == 0 || candidate.exact_conjunction_width == 0 {
            return Err(DisclosureError::InvalidRouteCandidate);
        }
        let rare = candidate.estimated_support < policy.minimum_route_support;
        let observed = private.taints(candidate.exact_conjunction_width, rare);
        let selected = if !rare {
            Some(GeneralizedRouteToken {
                token: candidate.token,
                estimated_support: candidate.estimated_support,
                ontology_distance: 0,
            })
        } else {
            let mut eligible = candidate
                .generalizations
                .iter()
                .copied()
                .filter(|generalized| {
                    generalized.token.allowlisted_code != 0
                        && generalized.estimated_support >= policy.minimum_route_support
                        && generalized.ontology_distance > 0
                })
                .collect::<Vec<_>>();
            eligible.sort_by_key(|generalized| {
                (
                    generalized.ontology_distance,
                    token_class_code(generalized.token.class),
                    generalized.token.allowlisted_code,
                )
            });
            eligible.into_iter().next()
        };
        let private_input_commitment = private.local_commitment()?;
        let Some(selected) = selected else {
            return Ok(RouteSanitization::Suppressed(TaintAudit {
                private_input_commitment,
                observed: observed.iter().copied().collect(),
                discharged_by_projection: vec![],
                remaining: observed.into_iter().collect(),
                generalized: false,
            }));
        };
        Ok(RouteSanitization::Ready(SanitizedRouteProjection {
            token: selected.token,
            estimated_support: selected.estimated_support,
            audit: TaintAudit {
                private_input_commitment,
                observed: observed.iter().copied().collect(),
                discharged_by_projection: observed.into_iter().collect(),
                remaining: vec![],
                generalized: selected.ontology_distance > 0,
            },
        }))
    }

    pub fn sanitize_public_problem(
        policy: &DisclosurePolicy,
        consent: &DisclosureConsent,
        local_tick: u64,
        private: &PrivateNeedMaterial,
        draft: &PublicProblemDraft,
    ) -> Result<SanitizedPublicProblem, DisclosureError> {
        policy.authorize(DisclosureMode::PublicProblem, Some(consent), local_tick)?;
        let problem_class = supported_concept(&draft.problem_class, policy.minimum_route_support)?;
        let roles = draft
            .roles
            .iter()
            .map(|candidate| supported_concept(candidate, policy.minimum_route_support))
            .collect::<Result<Vec<_>, _>>()?;
        if roles.is_empty() || draft.constraint_classes.is_empty() {
            return Err(DisclosureError::InvalidPublicProblem);
        }
        let problem = SanitizedPublicProblem {
            problem_class,
            roles,
            constraint_classes: draft.constraint_classes.clone(),
            policy_ref: policy.policy_ref.clone(),
            consent_commitment: consent.consent_commitment,
            limitations: draft.limitations.clone(),
        };
        problem.canonical_payload()?;
        // Taint inventory is evaluated and committed locally, but is never a
        // field or reference of the public object.
        let _local_audit_commitment = private.local_commitment()?;
        Ok(problem)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportedConceptCandidate {
    pub exact: ConceptCcid,
    pub estimated_support: u64,
    pub ancestors: Vec<(ConceptCcid, u64, u16)>,
}

fn supported_concept(
    candidate: &SupportedConceptCandidate,
    minimum_support: u64,
) -> Result<ConceptCcid, DisclosureError> {
    if candidate.estimated_support >= minimum_support {
        return Ok(candidate.exact);
    }
    let mut ancestors = candidate
        .ancestors
        .iter()
        .copied()
        .filter(|(_, support, distance)| *support >= minimum_support && *distance > 0)
        .collect::<Vec<_>>();
    ancestors.sort_by_key(|(concept, _, distance)| (*distance, *concept.as_bytes()));
    ancestors
        .first()
        .map(|(concept, _, _)| *concept)
        .ok_or(DisclosureError::RareValueNotGeneralizable)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublicConstraintClass {
    pub dimension_class: u16,
    pub operator_family: u16,
    pub value_bucket: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicProblemDraft {
    pub problem_class: SupportedConceptCandidate,
    pub roles: Vec<SupportedConceptCandidate>,
    pub constraint_classes: Vec<PublicConstraintClass>,
    pub limitations: Vec<ConceptCcid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SanitizedPublicProblem {
    pub problem_class: ConceptCcid,
    pub roles: Vec<ConceptCcid>,
    pub constraint_classes: Vec<PublicConstraintClass>,
    pub policy_ref: ObjectReference,
    pub consent_commitment: [u8; 32],
    pub limitations: Vec<ConceptCcid>,
}

impl SanitizedPublicProblem {
    pub fn canonical_payload(&self) -> Result<CanonicalValue, DisclosureError> {
        if self.roles.is_empty()
            || self.constraint_classes.is_empty()
            || self.consent_commitment == [0; 32]
            || has_duplicates(&self.roles)
            || has_duplicates(&self.constraint_classes)
            || has_duplicates(&self.limitations)
        {
            return Err(DisclosureError::InvalidPublicProblem);
        }
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(DISCLOSURE_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(DISCLOSURE_PROFILE_MINOR)),
            (
                2,
                CanonicalValue::Bytes(self.problem_class.as_bytes().to_vec()),
            ),
            (3, canonical_ccid_set(&self.roles)?),
            (
                4,
                CanonicalValue::Array(canonicalize_set_by_key(
                    self.constraint_classes
                        .iter()
                        .map(|constraint| {
                            let value = CanonicalValue::Map(vec![
                                (
                                    0,
                                    CanonicalValue::Unsigned(u64::from(constraint.dimension_class)),
                                ),
                                (
                                    1,
                                    CanonicalValue::Unsigned(u64::from(constraint.operator_family)),
                                ),
                                (
                                    2,
                                    CanonicalValue::Unsigned(u64::from(constraint.value_bucket)),
                                ),
                            ]);
                            (value.clone(), value)
                        })
                        .collect(),
                    ResourceProfile::ObjectV1,
                )?),
            ),
            (
                5,
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(self.policy_ref.reference_kind)),
                    (1, CanonicalValue::Bytes(self.policy_ref.cid.to_vec())),
                ]),
            ),
            (6, CanonicalValue::Bytes(self.consent_commitment.to_vec())),
            (7, canonical_ccid_set(&self.limitations)?),
        ]))
    }

    pub fn to_public_object(&self) -> Result<KnowledgeObjectEnvelope, DisclosureError> {
        let mut object = KnowledgeObjectEnvelope::new(
            SANITIZED_PUBLIC_PROBLEM_KIND,
            SchemaVersion::new(DISCLOSURE_PROFILE_MAJOR, DISCLOSURE_PROFILE_MINOR),
            DisclosureClass::Public,
            self.canonical_payload()?,
        );
        object.references = vec![self.policy_ref.clone()];
        Ok(object)
    }

    pub const fn contains_private_source_reference_or_raw_text(&self) -> bool {
        false
    }
}

fn bytes32_array(values: &[[u8; 32]]) -> CanonicalValue {
    CanonicalValue::Array(
        values
            .iter()
            .map(|value| CanonicalValue::Bytes(value.to_vec()))
            .collect(),
    )
}

fn canonical_ccid_set(values: &[ConceptCcid]) -> Result<CanonicalValue, DisclosureError> {
    let values = values
        .iter()
        .map(|concept| CanonicalValue::Bytes(concept.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        values,
        ResourceProfile::ObjectV1,
    )?))
}

fn token_class_code(class: CoarseRouteTokenClass) -> u8 {
    match class {
        CoarseRouteTokenClass::ObjectClass => 0,
        CoarseRouteTokenClass::CapabilityClass => 1,
        CoarseRouteTokenClass::DimensionClass => 2,
        CoarseRouteTokenClass::OperatorFamily => 3,
        CoarseRouteTokenClass::CoarseRole => 4,
    }
}

fn has_duplicates<T: Ord + Copy>(values: &[T]) -> bool {
    values.iter().copied().collect::<BTreeSet<_>>().len() != values.len()
}

fn domain_commitment(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisclosureError {
    Canonical(ku_core::foundation::CanonicalError),
    Object(ku_core::foundation::ObjectError),
    InvalidPolicy,
    ModeDisabled,
    ConsentRequired,
    ConsentScopeMismatch,
    InvalidRouteCandidate,
    RareValueNotGeneralizable,
    InvalidPublicProblem,
}

impl From<ku_core::foundation::CanonicalError> for DisclosureError {
    fn from(error: ku_core::foundation::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<ku_core::foundation::ObjectError> for DisclosureError {
    fn from(error: ku_core::foundation::ObjectError) -> Self {
        Self::Object(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vnext_query::{DisclosureCompiler, QueryRun, RouteSketchEntropy};
    use ku_core::foundation::public_knowledge_exchange_fixture_v1;

    fn reference(byte: u8) -> ObjectReference {
        ObjectReference::new(0, [byte; 32])
    }

    fn policy() -> DisclosurePolicy {
        DisclosurePolicy {
            policy_ref: reference(1),
            default_mode: DisclosureMode::LocalOnly,
            enabled_nonlocal_modes: vec![
                DisclosureMode::RouteMinimal,
                DisclosureMode::NegotiatedEncrypted,
                DisclosureMode::PublicProblem,
            ],
            minimum_route_support: 64,
        }
    }

    fn consent(mode: DisclosureMode) -> DisclosureConsent {
        DisclosureConsent {
            kind: ConsentKind::Explicit,
            policy_ref: reference(1),
            mode,
            purpose: ConceptCcid::from_bytes([2; 16]),
            scope_commitment: [3; 32],
            consent_commitment: [4; 32],
            not_before: 10,
            expires_at: 100,
        }
    }

    fn private() -> PrivateNeedMaterial {
        PrivateNeedMaterial {
            raw_text: b"secret anti-gravity material acceptance test".to_vec(),
            stable_receptor_ids: vec![[11; 32]],
            stable_assembly_ids: vec![[12; 32]],
            stable_need_ids: vec![[13; 32]],
            stable_user_ids: vec![[14; 32]],
            stable_node_ids: vec![[15; 32]],
            private_source_refs: vec![reference(16)],
            exact_literals: vec![b"123.456..123.457".to_vec()],
            contains_location_or_time: true,
            contains_hypothesis: true,
            contains_acceptance_test: true,
            contains_private_context: true,
        }
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty()
            && haystack
                .windows(needle.len())
                .any(|window| window == needle)
    }

    #[test]
    fn default_is_local_only_and_nonlocal_modes_require_scoped_consent() {
        let private = DisclosurePolicy::private_default(reference(1));
        assert_eq!(private.default_mode, DisclosureMode::LocalOnly);
        assert!(private
            .authorize(DisclosureMode::LocalOnly, None, 0)
            .is_ok());
        assert_eq!(
            private.authorize(DisclosureMode::RouteMinimal, None, 20),
            Err(DisclosureError::ModeDisabled)
        );
        assert_eq!(
            policy().authorize(DisclosureMode::RouteMinimal, None, 20),
            Err(DisclosureError::ConsentRequired)
        );
        assert!(policy()
            .authorize(
                DisclosureMode::RouteMinimal,
                Some(&consent(DisclosureMode::RouteMinimal)),
                20,
            )
            .is_ok());
    }

    #[test]
    fn rare_route_token_is_generalized_or_suppressed_never_leaked() {
        let candidate = RouteDisclosureCandidate {
            token: CoarseRouteToken {
                class: CoarseRouteTokenClass::CoarseRole,
                allowlisted_code: 600,
            },
            estimated_support: 5,
            exact_conjunction_width: 7,
            generalizations: vec![GeneralizedRouteToken {
                token: CoarseRouteToken {
                    class: CoarseRouteTokenClass::CoarseRole,
                    allowlisted_code: 60,
                },
                estimated_support: 80,
                ontology_distance: 2,
            }],
        };
        let consent = consent(DisclosureMode::RouteMinimal);
        let ready = DisclosureSanitizer::sanitize_route_minimal(
            &policy(),
            Some(&consent),
            20,
            &private(),
            &candidate,
        )
        .unwrap();
        let RouteSanitization::Ready(ready) = ready else {
            panic!("expected generalized token")
        };
        assert_eq!(ready.token.allowlisted_code, 60);
        assert!(ready.audit.generalized);
        assert!(ready.audit.remaining.is_empty());

        let mut no_generalization = candidate;
        no_generalization.generalizations.clear();
        assert!(matches!(
            DisclosureSanitizer::sanitize_route_minimal(
                &policy(),
                Some(&consent),
                20,
                &private(),
                &no_generalization,
            )
            .unwrap(),
            RouteSanitization::Suppressed(_)
        ));
    }

    #[test]
    fn route_network_bytes_exclude_all_private_material_and_stable_ids() {
        let private = private();
        let consent = consent(DisclosureMode::RouteMinimal);
        let candidate = RouteDisclosureCandidate {
            token: CoarseRouteToken {
                class: CoarseRouteTokenClass::CapabilityClass,
                allowlisted_code: 7,
            },
            estimated_support: 100,
            exact_conjunction_width: 9,
            generalizations: vec![],
        };
        let RouteSanitization::Ready(ready) = DisclosureSanitizer::sanitize_route_minimal(
            &policy(),
            Some(&consent),
            20,
            &private,
            &candidate,
        )
        .unwrap() else {
            panic!("expected route token")
        };
        let selector = public_knowledge_exchange_fixture_v1();
        let run = QueryRun::new(
            [30; 32],
            ku_core::foundation::ObjectCid::from_bytes([31; 32]),
            selector,
        )
        .unwrap();
        let sketch = DisclosureCompiler::default()
            .compile_route_minimal(
                &run,
                ready.token,
                ready.estimated_support,
                1,
                10,
                2,
                1,
                RouteSketchEntropy {
                    sketch_id: [32; 32],
                    one_time_reply_capability: [33; 32],
                    replay_nonce: [34; 32],
                    commitment_salt: [35; 32],
                },
            )
            .unwrap();
        let bytes = sketch.network_bytes().unwrap();
        assert!(!contains(&bytes, &private.raw_text));
        for id in private
            .stable_receptor_ids
            .iter()
            .chain(&private.stable_assembly_ids)
            .chain(&private.stable_need_ids)
            .chain(&private.stable_user_ids)
            .chain(&private.stable_node_ids)
        {
            assert!(!contains(&bytes, id));
        }
        assert!(!contains(&bytes, &private.private_source_refs[0].cid));
        assert!(!contains(&bytes, &private.exact_literals[0]));
        assert!(!ready.contains_stable_identity_raw_text_or_exact_conjunction());
    }

    #[test]
    fn public_problem_object_contains_no_private_refs_raw_text_or_rare_ccid() {
        let private = private();
        let consent = consent(DisclosureMode::PublicProblem);
        let rare = ConceptCcid::from_bytes([50; 16]);
        let ancestor = ConceptCcid::from_bytes([51; 16]);
        let sanitized = DisclosureSanitizer::sanitize_public_problem(
            &policy(),
            &consent,
            20,
            &private,
            &PublicProblemDraft {
                problem_class: SupportedConceptCandidate {
                    exact: rare,
                    estimated_support: 3,
                    ancestors: vec![(ancestor, 100, 1)],
                },
                roles: vec![SupportedConceptCandidate {
                    exact: ConceptCcid::from_bytes([52; 16]),
                    estimated_support: 100,
                    ancestors: vec![],
                }],
                constraint_classes: vec![PublicConstraintClass {
                    dimension_class: 1,
                    operator_family: 2,
                    value_bucket: 3,
                }],
                limitations: vec![],
            },
        )
        .unwrap();
        assert_eq!(sanitized.problem_class, ancestor);
        let object = sanitized.to_public_object().unwrap();
        let (bytes, _) = object.encode(ResourceProfile::ObjectV1).unwrap();
        assert_eq!(object.disclosure, DisclosureClass::Public);
        assert!(!contains(&bytes, rare.as_bytes()));
        assert!(!contains(&bytes, &private.raw_text));
        assert!(!contains(&bytes, &private.private_source_refs[0].cid));
        assert!(!sanitized.contains_private_source_reference_or_raw_text());
    }
}
