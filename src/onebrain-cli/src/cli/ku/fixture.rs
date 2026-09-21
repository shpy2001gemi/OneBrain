//! Isolated custody fixture adapted from the API lifecycle tests.
use super::*;
use ku_core::foundation::semantic::{
    LiteralValue, SemanticFrameSet, SourceSpan, StatementFrame, StatementId, StatementQualifiers,
    TermRef,
};
use ku_core::foundation::{
    NormalizedText, ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact,
    SourceArtifactKind,
};
use onebrain_base_contract::{BaseErrorCodeV1, ResourceBudgetV1};
use onebrain_node::concept_registry_runtime::ConceptRegistryReaderLease;
use onebrain_node::ku_product::{KuConceptBinding, KuInputProvider, KuResolvedInput};
use onebrain_node::BaseServiceError;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub(super) struct Inputs {
    source: Vec<u8>,
    pub(super) cid: SourceArtifactCID,
    pub(super) calls: AtomicUsize,
    revoked: AtomicBool,
    unresolved: AtomicBool,
    wait: AtomicBool,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
impl Inputs {
    pub(super) fn new() -> Self {
        let reference = |n| ObjectReference::new(1, [n; 32]);
        let source = SourceArtifact {
            source_kind: SourceArtifactKind::Text,
            raw_bytes: b"Exact PRIVATE water source".to_vec(),
            media_type_commitment: [2; 32],
            capture_adapter: reference(1),
            capture_sequence: 1,
            governance: ObservationGovernance {
                consent_policy: reference(2),
                consent_receipt: reference(3),
                revocation_policy: reference(4),
                retention_policy: reference(5),
                capture_scope_commitment: [3; 32],
                authorization_assessment_commitment: [4; 32],
                assessed_frontier: [5; 32],
            },
        };
        let (source, cid) = source
            .to_private_object()
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        Self {
            source,
            cid: SourceArtifactCID(cid.into_bytes()),
            calls: AtomicUsize::new(0),
            revoked: AtomicBool::new(false),
            unresolved: AtomicBool::new(false),
            wait: AtomicBool::new(false),
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        }
    }
}
impl KuInputProvider for Inputs {
    fn implementation(&self, mode: InputMode) -> Option<[u8; 32]> {
        // Advertising a fixture AI implementation must not enable the REST AI lane.
        Some(match mode {
            InputMode::ResolvedSemanticDraft => [4; 32],
            InputMode::LocalRule => [5; 32],
            InputMode::LocalAi => [6; 32],
        })
    }
    fn check_access(
        &self,
        principal: [u8; 32],
        sources: &[[u8; 32]],
    ) -> Result<(), BaseServiceError> {
        if principal == [0; 32] && sources == [self.cid.0] && !self.revoked.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(BaseServiceError::new(
                BaseErrorCodeV1::NotFound,
                "ku_not_found",
            ))
        }
    }
    fn resolve(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
        _: &ConceptRegistryReaderLease,
        _: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError> {
        self.check_access(
            principal,
            &request.source_refs.iter().map(|s| s.0).collect::<Vec<_>>(),
        )?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let version = request.draft_ref.map(|id| id.0[0]).unwrap_or(1);
        let drafts = (0..2)
            .map(|i| SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(90 + i),
                    operator_or_predicate: ku_core::foundation::ConceptCcid::from_bytes([7; 16]),
                    arguments: vec![TermRef::Literal(LiteralValue::Text(
                        NormalizedText::new(format!("water version {version} item {i}")).unwrap(),
                    ))],
                    constraints: vec![],
                    qualifiers: StatementQualifiers {
                        source_spans: vec![SourceSpan {
                            source: ObjectReference::new(1, self.cid.0),
                            start: 0,
                            end: 5,
                        }],
                        ..Default::default()
                    },
                }],
            })
            .collect();
        Ok(KuResolvedInput {
            drafts,
            source_objects: vec![self.source.clone()],
            bindings: vec![KuConceptBinding {
                label: "water".into(),
                selected: if self.unresolved.load(Ordering::SeqCst) {
                    None
                } else {
                    Some([7; 16])
                },
            }],
            needs_resolution: false,
            extraction_budget: None,
        })
    }
    fn resolve_async<'a>(
        &'a self,
        principal: [u8; 32],
        request: &'a KuPrepareV1,
        registry: &'a ConceptRegistryReaderLease,
        budget: &'a ResourceBudgetV1,
        _: onebrain_node::ku_product::KuExtractionExecution<'a>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<KuResolvedInput, BaseServiceError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            if self.wait.load(Ordering::SeqCst) {
                self.entered.notify_one();
                self.resume.notified().await;
            }
            self.resolve(principal, request, registry, budget)
        })
    }
}
