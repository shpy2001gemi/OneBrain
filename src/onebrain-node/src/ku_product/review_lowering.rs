//! ARCH-07: explicit review → canonical draft lowering for simple review-draft
//! claims (concept–predicate–concept and concept–predicate–quantity).
//!
//! The model never grants authority here: terms resolve only by exact label in
//! the node's verified Registry lease, units only through the host table below.
//! Anything unresolved returns `saveable:false` and no preparation. A lowered
//! draft still goes through the ordinary prepare → preview → save steps.
use super::*;
use crate::ku_manual::{ManualEditorResponse, ReviewLowerInput};
use ku_core::concept_registry::{ConceptCategory, ResolveResult};
use ku_encoder::extraction::review_lowering::{
    self as lowering, LookupOption, Role, UnitTransform,
};
use ku_encoder::extraction::{ExtractionError, WorkBudget};
use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;

const MAX_LOWERED: usize = 256;

pub(super) struct LoweredDraft {
    principal: [u8; 32],
    input: ReviewLowerInput,
    preparation: KuPrepareV1,
    semantic: SemanticFrameSet,
    source: Vec<u8>,
    bindings: Vec<KuConceptBinding>,
}

/// Implementation commitment carried by lowered preparations. It binds the
/// shared lowering/compiler source, not any model or provider.
fn implementation() -> Option<[u8; 32]> {
    static VALUE: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    *VALUE.get_or_init(|| {
        lowering::commitment()
            .ok()
            .map(|c| blake3::derive_key("onebrain:ku-review-lowering:1", c.as_bytes()))
    })
}
pub(super) fn is_lowering_commitment(commitment: [u8; 32]) -> bool {
    implementation() == Some(commitment)
}

/// Minimal host-owned exact unit transforms keyed by the Registry unit
/// concept's canonical name. Units outside this table stay unresolved.
fn unit_transform(canonical: &str) -> Option<UnitTransform> {
    const L: [i8; 7] = [1, 0, 0, 0, 0, 0, 0];
    const M: [i8; 7] = [0, 1, 0, 0, 0, 0, 0];
    const T: [i8; 7] = [0, 0, 1, 0, 0, 0, 0];
    const I: [i8; 7] = [0, 0, 0, 1, 0, 0, 0];
    const K: [i8; 7] = [0, 0, 0, 0, 1, 0, 0];
    const N: [i8; 7] = [0, 0, 0, 0, 0, 1, 0];
    const J: [i8; 7] = [0, 0, 0, 0, 0, 0, 1];
    const V: [i8; 7] = [3, 0, 0, 0, 0, 0, 0];
    let (dimension, scale, offset) = match canonical.to_lowercase().as_str() {
        "metre" | "meter" => (L, (1, 1), (0, 1)),
        "kilometre" | "kilometer" => (L, (1000, 1), (0, 1)),
        "centimetre" | "centimeter" => (L, (1, 100), (0, 1)),
        "millimetre" | "millimeter" => (L, (1, 1000), (0, 1)),
        "kilogram" => (M, (1, 1), (0, 1)),
        "gram" => (M, (1, 1000), (0, 1)),
        "tonne" => (M, (1000, 1), (0, 1)),
        "second" => (T, (1, 1), (0, 1)),
        "minute" => (T, (60, 1), (0, 1)),
        "hour" => (T, (3600, 1), (0, 1)),
        "ampere" => (I, (1, 1), (0, 1)),
        "kelvin" => (K, (1, 1), (0, 1)),
        "degree celsius" => (K, (1, 1), (27315, 100)),
        "mole" => (N, (1, 1), (0, 1)),
        "candela" => (J, (1, 1), (0, 1)),
        "litre" | "liter" => (V, (1, 1000), (0, 1)),
        _ => return None,
    };
    Some(UnitTransform {
        dimension,
        scale,
        offset,
    })
}

fn lowering_error(error: ExtractionError) -> BaseServiceError {
    match error.0 {
        "registry_unavailable" => unavailable(),
        "resource" | "deadline" | "registry_ambiguous_limit" => resource(),
        "canceled" => conflict(),
        code => BaseServiceError::new(BaseErrorCodeV1::InvalidRequest, code),
    }
}

impl KuStore {
    fn review_intake(
        &self,
        principal: [u8; 32],
        operation: [u8; 32],
    ) -> Result<crate::ku_ollama::TextIntakeRecord, BaseServiceError> {
        let tx = self.journal.begin_read().map_err(|_| corrupt())?;
        let table = tx.open_table(TEXT_INTAKE).map_err(|_| not_found())?;
        let value = table
            .get(operation.as_slice())
            .map_err(|_| corrupt())?
            .ok_or_else(not_found)?;
        let record = self.decode_intake(operation, value.value())?;
        if record.principal != principal {
            return Err(not_found());
        }
        Ok(record)
    }

    pub(crate) fn review_lower(
        &self,
        principal: [u8; 32],
        input: ReviewLowerInput,
        budget: &ResourceBudgetV1,
    ) -> Result<ManualEditorResponse, BaseServiceError> {
        if !input.reviewed {
            return Err(BaseServiceError::new(
                BaseErrorCodeV1::InvalidRequest,
                "explicit_review_required",
            ));
        }
        if input.operation_id == input.review_operation_id {
            return Err(invalid());
        }
        let (source, claim) = self.review_claim(
            principal,
            input.review_operation_id.0,
            input.window as usize,
            input.statement as usize,
        )?;
        let intake = self.review_intake(principal, input.review_operation_id.0)?;
        if intake.input.text != source {
            return Err(corrupt());
        }
        let source_ref = *intake.preparation.source_refs.first().ok_or_else(corrupt)?;
        // Custody must hold the source for prepare/save access checks.
        self.inputs.restore_text(intake.clone())?;
        let lease = self
            .registry
            .as_ref()
            .ok_or_else(unavailable)?
            .reader_lease();
        let root: [u8; 32] = decode_hex(
            lease
                .status()
                .release_aggregate_root
                .as_deref()
                .ok_or_else(unavailable)?,
        )
        .map_err(|_| unavailable())?;
        let mut work = WorkBudget::new(
            budget.max_work_units.min(1_000_000),
            Duration::from_secs(30),
            Arc::new(AtomicBool::new(false)),
        )
        .map_err(|_| resource())?;
        let mut lookup = |label: &str, role: Role| -> Result<Vec<LookupOption>, ExtractionError> {
            let found = match lease
                .resolve_checked(label)
                .map_err(|_| ExtractionError("registry_unavailable"))?
            {
                ResolveResult::Found(c) => vec![c],
                ResolveResult::Ambiguous(c) => c,
                // Fuzzy matches are never exact-label authority.
                ResolveResult::NotFound | ResolveResult::Fuzzy(_) => vec![],
            };
            Ok(found
                .into_iter()
                .filter(|c| (c.category == ConceptCategory::Unit) == (role == Role::Unit))
                .map(|c| LookupOption {
                    ccid: c.ccid,
                    unit: (role == Role::Unit)
                        .then(|| unit_transform(&c.canonical_name))
                        .flatten(),
                })
                .collect())
        };
        let lowered =
            lowering::lower_claim(&source, source_ref.0, root, &claim, &mut lookup, &mut work)
                .map_err(lowering_error)?;
        let limitations = json!([
            "review_lowering_simple_claims_only",
            "semantic_fidelity_unassessed",
            "experimental_model_unqualified",
            "local_only"
        ]);
        let unresolved: Vec<_> = lowered
            .unresolved
            .iter()
            .map(|u| json!({"role": u.role.as_str(), "label": u.label, "candidates": u.candidates}))
            .collect();
        let Some(semantic) = lowered.semantic else {
            return Ok(ManualEditorResponse::ReviewLowering {
                review_lowering: json!({"saveable": false, "unresolved": unresolved, "limitations": limitations}),
            });
        };
        let bytes = semantic.canonical_bytes().map_err(|_| invalid())?;
        // Deterministic: re-lowering after a restart reproduces the preparation.
        let mut reference = blake3::Hasher::new_derive_key("onebrain:ku:review-lowering-draft:1");
        reference.update(&self.dataset.0);
        reference.update(&input.review_operation_id.0);
        reference.update(&input.window.to_le_bytes());
        reference.update(&input.statement.to_le_bytes());
        reference.update(&bytes);
        let preparation = KuPrepareV1 {
            operation_id: input.operation_id,
            idempotency_key: input.idempotency_key,
            input_mode: InputMode::ResolvedSemanticDraft,
            source_refs: vec![source_ref],
            registry_release_root: ReleaseRoot(root),
            semantic_profile: SEMANTIC_CONTENT_PROFILE.into(),
            implementation_commitment: ImplementationCommitment(
                implementation().ok_or_else(unavailable)?,
            ),
            destination: Disclosure::LOCALONLY,
            draft_ref: Some(ObjectCID(*reference.finalize().as_bytes())),
        };
        preparation.validate().map_err(|_| invalid())?;
        let encoded = serde_json::to_value(&preparation).map_err(|_| invalid())?;
        if encoded.to_string().len() as u64 > budget.max_bytes {
            return Err(resource());
        }
        {
            let mut drafts = self.lowered.lock().map_err(|_| corrupt())?;
            if let Some(old) = drafts.get(&input.operation_id.0) {
                if old.principal != principal
                    || old.input != input
                    || serde_json::to_vec(&old.preparation).ok()
                        != serde_json::to_vec(&preparation).ok()
                {
                    return Err(conflict());
                }
            } else if drafts.len() >= MAX_LOWERED {
                return Err(resource());
            }
            drafts.insert(
                input.operation_id.0,
                LoweredDraft {
                    principal,
                    input,
                    preparation,
                    semantic,
                    source: intake.source,
                    bindings: lowered
                        .bindings
                        .into_iter()
                        .map(|b| KuConceptBinding {
                            label: b.label,
                            selected: Some(b.ccid),
                        })
                        .collect(),
                },
            );
        }
        Ok(ManualEditorResponse::ReviewLowering {
            review_lowering: json!({"saveable": true, "preparation": encoded, "unresolved": [], "limitations": limitations}),
        })
    }

    /// Prepare-time input for a lowered draft. The ordinary prepare path still
    /// revalidates sources, bindings, frames and output identity.
    pub(super) fn resolve_lowered(
        &self,
        principal: [u8; 32],
        request: &KuPrepareV1,
    ) -> Result<KuResolvedInput, BaseServiceError> {
        let drafts = self.lowered.lock().map_err(|_| corrupt())?;
        let draft = drafts.get(&request.operation_id.0).ok_or_else(|| {
            BaseServiceError::new(BaseErrorCodeV1::NotFound, "review_lowering_expired")
        })?;
        if draft.principal != principal {
            return Err(not_found());
        }
        if serde_json::to_vec(&draft.preparation).ok() != serde_json::to_vec(request).ok() {
            return Err(invalid());
        }
        Ok(KuResolvedInput {
            drafts: vec![draft.semantic.clone()],
            source_objects: vec![draft.source.clone()],
            bindings: draft.bindings.clone(),
            needs_resolution: false,
            extraction_budget: None,
        })
    }
}
