//! P1 `D-011`: Deterministic `ku-semantic-selection/2.0` (`ku-semantic-draft/2.0`) -> `SemanticFrameSet` lowering (`ku-review-lowering/2.0`).
//!
//! Leaves `review_lowering.rs` (`ku-review-lowering/1.0`) untouched while adding deterministic,
//! model-free, fail-closed lowering of reviewed `ku-semantic-draft/2.0` artifacts:
//! - `alternatives`: requires `exclusivity` in `{"inclusive", "exclusive"}` (`"unspecified"` ->
//!   `saveable: false` blocker); lowers branches deterministically with commutative sorting
//!   when `"inclusive"` and dependency-free.
//! - `ellipses`: binds elided role to the exact resolved `TermRef` of `borrowed` role
//!   (`saveable: false` if unresolved or self-referential).
//! - `references`: validates acyclic intra-draft coreference DAG, binds shared
//!   `TermRef::Variable { id: VariableId(k), ... }`, and invokes
//!   `SemanticFrameSet::alpha_normalized()` so alpha-equivalent phrasings yield identical
//!   `SemanticContentCid`.
//! - `comparisons`: `target_kind == "explicit"` lowers to exact rational `QuantityLiteral`
//!   (SI normalized via host `UnitTransform`) or `TermRef::Concept` inside `TypedConstraint`
//!   (`ConstraintExpression::Compare`); `"unspecified"` / `"implicit_candidate"` stays
//!   `saveable: false`.
//! - `qualifiers`: maps `negation`/`polarity`, `condition`, `time`, `location`, `frequency`,
//!   `modality`, and `approximation` deterministically onto `StatementQualifiers` /
//!   `TypedConstraint`.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::canonical::{encode_canonical, ResourceProfile};
use ku_core::foundation::content_id::SemanticContentCid;
use ku_core::foundation::object::ObjectReference;
use ku_core::foundation::semantic::{
    ComparisonOperator, ConceptCcid, ConstraintExpression, DimensionVector, ExactRatio,
    LiteralValue, Modality, QuantityLiteral, SemanticFrameSet, SourceSpan, StatementFrame,
    StatementId, StatementQualifiers, TermRef, TypedConstraint, UnitRef, VariableId,
};
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use serde_json::{json, Value};

use super::review_draft::quote_positions;
use super::review_lowering::{LookupOption, LoweringBinding, Role, UnitTransform, Unresolved};
use super::*;

pub const LOWERING_SCHEMA_V2: &str = "ku-review-lowering/2.0";

/// `blake3("onebrain:vnext:tier0:inclusive-or:1\0")[..16]`
pub const DISJUNCTION_INCLUSIVE_OR_CCID: ConceptCcid = ConceptCcid::from_bytes([
    244, 217, 124, 64, 142, 152, 83, 153, 138, 93, 160, 171, 32, 26, 16, 245,
]);

/// `blake3("onebrain:vnext:tier0:exclusive-xor:1\0")[..16]`
pub const DISJUNCTION_EXCLUSIVE_XOR_CCID: ConceptCcid = ConceptCcid::from_bytes([
    212, 180, 95, 114, 200, 239, 216, 75, 232, 115, 161, 17, 228, 155, 233, 128,
]);

/// `blake3("onebrain:vnext:tier0:qualifier-frequency:1\0")[..16]`
pub const QUALIFIER_FREQUENCY_CCID: ConceptCcid = ConceptCcid::from_bytes([
    66, 159, 183, 157, 70, 176, 82, 71, 32, 145, 45, 49, 137, 183, 49, 190,
]);

/// `blake3("onebrain:vnext:tier0:qualifier-approximation:1\0")[..16]`
pub const QUALIFIER_APPROXIMATION_CCID: ConceptCcid = ConceptCcid::from_bytes([
    7, 161, 227, 66, 245, 164, 100, 115, 124, 139, 95, 22, 212, 221, 224, 120,
]);

const QUALIFIER_KEYS: &[&str] = &[
    "frequency",
    "negation",
    "condition",
    "time",
    "location",
    "modality",
    "approximation",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringBlockerV2 {
    pub claim_index: usize,
    pub field: &'static str,
    pub code: &'static str,
}

pub struct LoweredDraftV2 {
    pub schema: &'static str,
    pub saveable: bool,
    pub semantic: Option<SemanticFrameSet>,
    pub semantic_content_cid: Option<SemanticContentCid>,
    pub bindings: Vec<LoweringBinding>,
    pub unresolved: Vec<Unresolved>,
    pub blockers: Vec<LoweringBlockerV2>,
}

pub fn commitment_v2() -> Result<String> {
    hash(&json!([
        LOWERING_SCHEMA_V2,
        include_str!("review_lowering_v2.rs"),
        include_str!("compiler.rs"),
        include_str!("schema.rs"),
    ]))
}

#[derive(Clone, Debug)]
struct NumberSpec {
    value_quote: String,
    unit_quote: String,
    counted_entity_quote: String,
}

#[derive(Clone, Debug)]
struct EllipsisSpec {
    surface_span: (usize, usize),
    surface_quote: String,
    borrowed_span: (usize, usize),
    borrowed_quote: String,
}

#[derive(Clone, Debug)]
struct ReferenceSpec {
    via_span: (usize, usize),
    via_quote: String,
    target_kind: String,
    target_claim: usize,
}

#[derive(Default)]
struct LoweringStateV2 {
    bindings: Vec<LoweringBinding>,
    unresolved: Vec<Unresolved>,
    blockers: Vec<LoweringBlockerV2>,
    next_var_id: u32,
    next_stmt_id: u32,
}

impl LoweringStateV2 {
    fn alloc_var(&mut self) -> VariableId {
        let id = VariableId(self.next_var_id);
        self.next_var_id = self.next_var_id.saturating_add(1);
        id
    }

    fn alloc_stmt(&mut self) -> StatementId {
        let id = StatementId(self.next_stmt_id);
        self.next_stmt_id = self.next_stmt_id.saturating_add(1);
        id
    }

    fn push_blocker(&mut self, claim_index: usize, field: &'static str, code: &'static str) {
        let b = LoweringBlockerV2 {
            claim_index,
            field,
            code,
        };
        if !self.blockers.contains(&b) {
            self.blockers.push(b);
        }
    }

    fn resolve_concept(
        &mut self,
        label: &str,
        role: Role,
        lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
        budget: &mut WorkBudget,
    ) -> Result<Option<ConceptCcid>> {
        let trimmed = label.trim();
        if trimmed.is_empty() {
            self.unresolved.push(Unresolved {
                role,
                label: label.to_string(),
                candidates: 0,
            });
            return Ok(None);
        }
        budget.charge(trimmed.len() + 256)?;
        let found = lookup(trimmed, role)?;
        require(found.len() <= 64, "registry_ambiguous_limit")?;
        if found.len() == 1 && found[0].unit.is_none() {
            let ccid = found[0].ccid;
            self.bindings.push(LoweringBinding {
                label: trimmed.to_string(),
                ccid,
            });
            Ok(Some(ConceptCcid::from_bytes(ccid)))
        } else {
            self.unresolved.push(Unresolved {
                role,
                label: trimmed.to_string(),
                candidates: found.len(),
            });
            Ok(None)
        }
    }

    fn resolve_quantity(
        &mut self,
        value_quote: &str,
        unit_quote: &str,
        lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
        budget: &mut WorkBudget,
    ) -> Result<Option<QuantityLiteral>> {
        let value = compiler::exact_number(value_quote.trim())?;
        let trimmed_unit = unit_quote.trim();
        if trimmed_unit.is_empty() {
            self.unresolved.push(Unresolved {
                role: Role::Unit,
                label: unit_quote.to_string(),
                candidates: 0,
            });
            return Ok(None);
        }
        budget.charge(trimmed_unit.len() + 256)?;
        let found = lookup(trimmed_unit, Role::Unit)?;
        require(found.len() <= 64, "registry_ambiguous_limit")?;
        if found.len() == 1 {
            if let Some(transform) = found[0].unit {
                let qty = build_quantity_literal(value, found[0].ccid, transform)?;
                self.bindings.push(LoweringBinding {
                    label: trimmed_unit.to_string(),
                    ccid: found[0].ccid,
                });
                return Ok(Some(qty));
            }
        }
        self.unresolved.push(Unresolved {
            role: Role::Unit,
            label: trimmed_unit.to_string(),
            candidates: found.len(),
        });
        Ok(None)
    }
}

/// Lowers a single reviewed `ku-semantic-draft/2.0` statement object by wrapping it
/// in a 1-statement `ku-semantic-draft/2.0` document.
pub fn lower_claim_v2(
    source: &str,
    source_ref: [u8; 32],
    registry_root: [u8; 32],
    claim: &Value,
    lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
    budget: &mut WorkBudget,
) -> Result<LoweredDraftV2> {
    let mut wrapped = claim.clone();
    if wrapped.is_object() {
        if wrapped.get("id").is_none() {
            wrapped["id"] = json!("claim-0");
        }
        if wrapped.get("evidence_spans").is_none() {
            wrapped["evidence_spans"] = json!([]);
        }
    }
    let draft = json!({
        "profile": "ku-semantic-draft/2.0",
        "semantic_nodes": 1,
        "unresolved": [],
        "statements": [wrapped],
    });
    lower_draft_v2(source, source_ref, registry_root, &draft, lookup, budget)
}

/// Deterministic lowering of a reviewed `ku-semantic-draft/2.0` JSON artifact
/// into an alpha-normalized `SemanticFrameSet` (`LOWERING_SCHEMA_V2`).
pub fn lower_draft_v2(
    source: &str,
    source_ref: [u8; 32],
    _registry_root: [u8; 32],
    draft: &Value,
    lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
    budget: &mut WorkBudget,
) -> Result<LoweredDraftV2> {
    require(!source.is_empty() && source.len() <= 8192, "source_bound")?;
    require(source_ref != [0; 32], "source_ref")?;
    let source_obj_ref = ObjectReference::new(0, source_ref);
    require(draft.is_object(), "review_draft_unsupported")?;
    require(
        draft["profile"].as_str() == Some("ku-semantic-draft/2.0"),
        "review_draft_profile",
    )?;
    let statements_json = draft["statements"]
        .as_array()
        .ok_or(ExtractionError("review_draft_unsupported"))?;
    require(
        !statements_json.is_empty() && statements_json.len() <= 64,
        "review_draft_unsupported",
    )?;
    budget.charge(source.len().saturating_mul(2) + statements_json.len() * 256)?;

    // Reserve statement IDs 0..statements_json.len() for the primary claims so cross-claim
    // condition relations (`relations[].statement`) map deterministically before auxiliary frames;
    // start fresh lowering variables above 1000 (`alpha_normalized()` canonicalizes them to 0..k).
    let mut state = LoweringStateV2 {
        next_stmt_id: statements_json.len() as u32,
        next_var_id: 1_000,
        ..Default::default()
    };

    if draft["unresolved"]
        .as_array()
        .is_some_and(|arr| !arr.is_empty())
    {
        state.push_blocker(0, "unresolved", "draft_unresolved");
    }

    // Step 1: Locate every claim's evidence span in `source`.
    let mut claim_spans: Vec<(usize, usize)> = Vec::with_capacity(statements_json.len());
    for stmt in statements_json {
        let evidence = stmt["evidence"].as_str().unwrap_or("");
        require(!evidence.is_empty(), "review_claim_unsupported")?;
        let found = quote_positions(source, evidence);
        require(found.len() == 1, "review_claim_evidence")?;
        claim_spans.push(found[0]);
    }

    // Collect addressable spans across all claims so `references` and `ellipses`
    // can validate targets and acyclicity deterministically.
    // Map span `(start, end)` -> list of `(claim_idx, kind)`.
    let mut addressable_spans: BTreeMap<(usize, usize), Vec<(usize, &'static str)>> =
        BTreeMap::new();
    for (i, stmt) in statements_json.iter().enumerate() {
        let cspan = claim_spans[i];
        addressable_spans
            .entry(cspan)
            .or_default()
            .push((i, "claim"));
        let evidence_str = &source[cspan.0..cspan.1];

        let subj = stmt["subject"].as_str().unwrap_or("");
        if !subj.is_empty() {
            if let Ok(sp) = locate_unique_in_window(evidence_str, cspan.0, subj) {
                addressable_spans.entry(sp).or_default().push((i, "term"));
            }
        }
        for arg in string_array(&stmt["arguments"]) {
            if let Ok(sp) = locate_unique_in_window(evidence_str, cspan.0, arg) {
                addressable_spans.entry(sp).or_default().push((i, "term"));
            }
        }
        if let Some(alts) = stmt["alternatives"].as_array() {
            for group in alts {
                let gquote = group["quote"].as_str().unwrap_or("");
                if let Ok(gsp) = locate_unique_in_window(evidence_str, cspan.0, gquote) {
                    addressable_spans.entry(gsp).or_default().push((i, "group"));
                    let g_text = &source[gsp.0..gsp.1];
                    if let Some(branches) = group["branches"].as_array() {
                        for br in branches {
                            if let Some(bsp) = resolve_selector_span(
                                source,
                                &br["surface"],
                                g_text,
                                gsp.0,
                                br.get("span"),
                            ) {
                                addressable_spans.entry(bsp).or_default().push((i, "term"));
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 2: Parse & validate `references` (intra-draft coreference DAG) and `ellipses`.
    let mut parsed_references: Vec<Vec<ReferenceSpec>> = vec![Vec::new(); statements_json.len()];
    let mut ref_claim_edges: Vec<(usize, usize)> = Vec::new();
    let mut ref_span_parent: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();

    for (i, stmt) in statements_json.iter().enumerate() {
        let cspan = claim_spans[i];
        let ctext = &source[cspan.0..cspan.1];
        if let Some(refs) = stmt["references"].as_array() {
            for r in refs {
                let kind = r["target_kind"].as_str().unwrap_or("");
                if !matches!(kind, "term" | "group" | "claim") {
                    state.push_blocker(i, "references", "invalid_target_kind");
                    continue;
                }
                let via_quote = r["via"]["quote"].as_str().unwrap_or("").to_string();
                let via_opt =
                    resolve_selector_span(source, &r["via"], ctext, cspan.0, r.get("via_span"));
                let to_opt =
                    resolve_selector_span(source, &r["to"], source, 0, r.get("target_span"));

                let (Some(via_span), Some(to_span)) = (via_opt, to_opt) else {
                    state.push_blocker(i, "references", "target_unresolved");
                    continue;
                };
                if via_span == to_span {
                    state.push_blocker(i, "references", "self_reference");
                    continue;
                }
                let matching_targets: Vec<usize> = addressable_spans
                    .get(&to_span)
                    .into_iter()
                    .flatten()
                    .filter(|(_, k)| *k == kind)
                    .map(|(c_idx, _)| *c_idx)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();

                if matching_targets.len() != 1 {
                    state.push_blocker(i, "references", "target_unresolved");
                    continue;
                }
                let target_claim = matching_targets[0];
                if target_claim == i {
                    state.push_blocker(i, "references", "self_reference");
                    continue;
                }
                ref_claim_edges.push((i, target_claim));
                if kind == "term" || kind == "group" {
                    ref_span_parent.insert(via_span, to_span);
                }
                parsed_references[i].push(ReferenceSpec {
                    via_span,
                    via_quote,
                    target_kind: kind.to_string(),
                    target_claim,
                });
            }
        }
    }

    // Validate DAG acyclicity across claim-level and span-level coreferences.
    if has_directed_cycle(statements_json.len(), &ref_claim_edges)
        || has_span_cycle(&ref_span_parent)
    {
        for &(from_claim, _) in &ref_claim_edges {
            state.push_blocker(from_claim, "references", "cycle");
        }
        if state.blockers.iter().all(|b| b.code != "cycle") {
            state.push_blocker(0, "references", "cycle");
        }
    }

    // Parse `ellipses` per claim and check for self-reference / cycles.
    let mut parsed_ellipses: Vec<Vec<EllipsisSpec>> = vec![Vec::new(); statements_json.len()];
    let mut ellipsis_span_parent: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    for (i, stmt) in statements_json.iter().enumerate() {
        let cspan = claim_spans[i];
        let ctext = &source[cspan.0..cspan.1];
        if let Some(ells) = stmt["ellipses"].as_array() {
            for ell in ells {
                let s_quote = ell["surface"]["quote"].as_str().unwrap_or("").to_string();
                let b_quote = ell["borrowed"]["quote"].as_str().unwrap_or("").to_string();
                let s_opt =
                    resolve_selector_span(source, &ell["surface"], ctext, cspan.0, ell.get("span"));
                let b_opt = resolve_selector_span(
                    source,
                    &ell["borrowed"],
                    source,
                    0,
                    ell.get("borrowed_span"),
                );
                let (Some(s_span), Some(b_span)) = (s_opt, b_opt) else {
                    state.push_blocker(i, "ellipses", "unresolved_borrowed_role");
                    continue;
                };
                if s_span == b_span {
                    state.push_blocker(i, "ellipses", "self_reference");
                    continue;
                }
                ellipsis_span_parent.insert(s_span, b_span);
                parsed_ellipses[i].push(EllipsisSpec {
                    surface_span: s_span,
                    surface_quote: s_quote,
                    borrowed_span: b_span,
                    borrowed_quote: b_quote,
                });
            }
        }
    }
    if has_span_cycle(&ellipsis_span_parent) {
        state.push_blocker(0, "ellipses", "cycle");
    }

    // Identify root antecedent spans for `term` / `group` coreferences and allocate one
    // shared `VariableId` per root antecedent when DAG is acyclic.
    let mut root_antecedent_vars: BTreeMap<(usize, usize), VariableId> = BTreeMap::new();
    let mut span_to_root_antecedent: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    if !state
        .blockers
        .iter()
        .any(|b| b.field == "references" && b.code == "cycle")
    {
        for &via_span in ref_span_parent.keys() {
            let mut cur = via_span;
            let mut guard = 0usize;
            while let Some(&next) = ref_span_parent.get(&cur) {
                cur = next;
                guard += 1;
                if guard > 128 {
                    break;
                }
            }
            let root = cur;
            root_antecedent_vars
                .entry(root)
                .or_insert_with(|| state.alloc_var());
            span_to_root_antecedent.insert(via_span, root);
            span_to_root_antecedent.insert(root, root);
        }
    }

    // Step 3: Determine topological claim order so antecedents and borrowed roles resolve
    // before dependent claims when possible (falling back to 0..N if any blocker exists).
    let claim_order = topological_claim_order(
        statements_json.len(),
        &ref_claim_edges,
        &parsed_ellipses,
        &claim_spans,
    );

    let mut resolved_span_terms: BTreeMap<(usize, usize), TermRef> = BTreeMap::new();
    let mut primary_frames: BTreeMap<usize, StatementFrame> = BTreeMap::new();
    let mut aux_frames: Vec<StatementFrame> = Vec::new();

    for &i in &claim_order {
        let stmt = &statements_json[i];
        let cspan = claim_spans[i];
        let ctext = &source[cspan.0..cspan.1];

        let predicate_quote = stmt["predicate"].as_str().unwrap_or("");
        require(!predicate_quote.is_empty(), "review_claim_unsupported")?;
        let _predicate_span = locate_unique_in_window(ctext, cspan.0, predicate_quote)?;

        // Check qualifier predicate-overlap guard across all qualifier arrays.
        for qkey in QUALIFIER_KEYS {
            for qval in string_array(&stmt[qkey]) {
                if predicate_quote.contains(qval) || qval.contains(predicate_quote) {
                    state.push_blocker(i, qkey, "predicate_overlap");
                }
            }
        }

        let pred_ccid = state.resolve_concept(predicate_quote, Role::Predicate, lookup, budget)?;

        let numbers = parse_numbers(&stmt["numbers"])?;

        // Build lookup map of alternatives keyed by their `quote` inside this claim.
        let mut alt_by_quote: BTreeMap<String, &Value> = BTreeMap::new();
        if let Some(alts) = stmt["alternatives"].as_array() {
            for group in alts {
                let gq = group["quote"].as_str().unwrap_or("").to_string();
                alt_by_quote.insert(gq, group);
            }
        }

        // Build lookup map of ellipses in this claim keyed by `surface_span` and `surface_quote`.
        let claim_ellipses = &parsed_ellipses[i];
        let claim_refs = &parsed_references[i];

        // Resolve subject term (may be coreferent, elided, or standard concept).
        let subject_quote = stmt["subject"].as_str().unwrap_or("");
        let mut arguments: Vec<TermRef> = Vec::new();

        if !subject_quote.is_empty() {
            let subj_span = locate_unique_in_window(ctext, cspan.0, subject_quote)?;
            if let Some(term) = resolve_role_term(
                i,
                source,
                &source_obj_ref,
                subj_span,
                subject_quote,
                Role::Subject,
                &numbers,
                &alt_by_quote,
                claim_ellipses,
                claim_refs,
                &span_to_root_antecedent,
                &root_antecedent_vars,
                &mut resolved_span_terms,
                &mut aux_frames,
                &mut state,
                lookup,
                budget,
            )? {
                arguments.push(term);
            }
        } else if !claim_ellipses.is_empty() {
            // Elided subject reconstructed via first ellipsis when subject string is empty.
            let ell = &claim_ellipses[0];
            if let Some(borrowed_term) = resolved_span_terms.get(&ell.borrowed_span).cloned() {
                resolved_span_terms.insert(ell.surface_span, borrowed_term.clone());
                arguments.push(borrowed_term);
            } else {
                state.push_blocker(i, "ellipses", "unresolved_borrowed_role");
            }
        } else {
            return Err(ExtractionError("review_claim_unsupported"));
        }

        // Resolve claim arguments (`arguments` array or standalone `numbers` quantity when `arguments` is empty).
        let raw_args = string_array(&stmt["arguments"]);
        let comparisons_json = stmt["comparisons"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        if raw_args.is_empty() && !numbers.is_empty() && comparisons_json.is_empty() {
            for num in &numbers {
                if !num.counted_entity_quote.is_empty() {
                    state.push_blocker(i, "numbers", "counted_entity_unlowered");
                    continue;
                }
                let val_sp = locate_unique_in_window(ctext, cspan.0, &num.value_quote)?;
                let _unit_sp = locate_unique_in_window(ctext, cspan.0, &num.unit_quote)?;
                if let Some(qty) =
                    state.resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
                {
                    let term = TermRef::Literal(LiteralValue::Quantity(qty));
                    resolved_span_terms.insert(val_sp, term.clone());
                    arguments.push(term);
                }
            }
        } else {
            for arg_quote in &raw_args {
                let arg_span = locate_unique_in_window(ctext, cspan.0, arg_quote)?;
                if let Some(term) = resolve_role_term(
                    i,
                    source,
                    &source_obj_ref,
                    arg_span,
                    arg_quote,
                    Role::Object,
                    &numbers,
                    &alt_by_quote,
                    claim_ellipses,
                    claim_refs,
                    &span_to_root_antecedent,
                    &root_antecedent_vars,
                    &mut resolved_span_terms,
                    &mut aux_frames,
                    &mut state,
                    lookup,
                    budget,
                )? {
                    arguments.push(term);
                }
            }
        }

        // Also check any `alternatives` groups not bound to `arguments` (e.g. subject or unbound).
        if let Some(alts) = stmt["alternatives"].as_array() {
            for group in alts {
                let gq = group["quote"].as_str().unwrap_or("");
                let bound_to_arg_or_subj = gq == subject_quote || raw_args.contains(&gq);
                if !bound_to_arg_or_subj {
                    state.push_blocker(i, "alternatives", "argument_binding");
                }
                if group["exclusivity"].as_str() == Some("unspecified") {
                    state.push_blocker(i, "alternatives", "exclusivity_unspecified");
                }
            }
        }

        // Step 4: Lower `comparisons` onto `TypedConstraint` (`ConstraintExpression::Compare`).
        let mut constraints: Vec<TypedConstraint> = Vec::new();
        let left_subject_term = arguments.first().cloned();
        for cmp in comparisons_json {
            let prop = cmp["property"].as_str().unwrap_or("");
            if prop != predicate_quote {
                state.push_blocker(i, "comparisons", "property_binding");
            }
            let target_kind = cmp["target_kind"].as_str().unwrap_or("");
            match target_kind {
                "unspecified" => {
                    state.push_blocker(i, "comparisons", "target_unspecified");
                    continue;
                }
                "implicit_candidate" => {
                    state.push_blocker(i, "comparisons", "implicit_target_unassessed");
                    continue;
                }
                "explicit" => {}
                _ => {
                    state.push_blocker(i, "comparisons", "invalid_target_kind");
                    continue;
                }
            }
            let cue = cmp["cue"].as_str().unwrap_or("");
            let Some(operator) = parse_comparison_operator(cue) else {
                state.push_blocker(i, "comparisons", "unsupported_comparison_cue");
                continue;
            };
            let target_quote = cmp["target"]["quote"].as_str().unwrap_or("");
            if target_quote.is_empty() {
                state.push_blocker(i, "comparisons", "target_unresolved");
                continue;
            }
            let target_span_opt = resolve_selector_span(
                source,
                &cmp["target"],
                ctext,
                cspan.0,
                cmp.get("target_span"),
            );
            let Some(target_span) = target_span_opt else {
                state.push_blocker(i, "comparisons", "target_unresolved");
                continue;
            };
            let right_term_opt = if let Some(num) = find_matching_number(&numbers, target_quote) {
                if !num.counted_entity_quote.is_empty() {
                    state.push_blocker(i, "comparisons", "counted_entity_unlowered");
                    None
                } else {
                    state
                        .resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
                        .map(|q| TermRef::Literal(LiteralValue::Quantity(q)))
                }
            } else if let Some(existing) = resolved_span_terms.get(&target_span).cloned() {
                Some(existing)
            } else {
                state
                    .resolve_concept(target_quote, Role::Object, lookup, budget)?
                    .map(TermRef::Concept)
            };

            if let (Some(left), Some(right)) = (left_subject_term.clone(), right_term_opt) {
                resolved_span_terms.insert(target_span, right.clone());
                constraints.push(TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left,
                        operator,
                        right,
                    },
                    required: true,
                });
            }
        }

        // Step 5: Lower `qualifiers` (`negation`, `modality`, `condition`, `time`, `location`, `frequency`, `approximation`).
        let mut qualifiers = StatementQualifiers {
            negated: !string_array(&stmt["negation"]).is_empty(),
            modality: Modality::Asserted,
            condition: None,
            time: None,
            location: None,
            perspective: None,
            tolerance: None,
            source_spans: vec![SourceSpan {
                source: source_obj_ref.clone(),
                start: cspan.0 as u64,
                end: cspan.1 as u64,
            }],
        };

        // Modality
        let modalities = string_array(&stmt["modality"]);
        if modalities.len() > 1 {
            state.push_blocker(i, "modality", "multiple_modalities_ambiguous");
        } else if let Some(&m_str) = modalities.first() {
            match parse_modality_cue(m_str) {
                Some(m) => qualifiers.modality = m,
                None => state.push_blocker(i, "modality", "unsupported_modality"),
            }
        }

        // Condition: check cross-statement `relations` (`kind == "condition"`) or standalone `condition` qualifier.
        let mut condition_stmt_id: Option<StatementId> = None;
        if let Some(rels) = stmt["relations"].as_array() {
            for rel in rels {
                if rel["kind"].as_str() == Some("condition") {
                    let target_idx = rel["statement"].as_u64().unwrap_or(u64::MAX) as usize;
                    if target_idx >= statements_json.len() || target_idx == i {
                        state.push_blocker(i, "condition", "invalid_condition_relation");
                    } else {
                        condition_stmt_id = Some(StatementId(target_idx as u32));
                    }
                }
            }
        }
        // Also check `references` with `target_kind == "claim"` if `condition` is present.
        if condition_stmt_id.is_none() && !string_array(&stmt["condition"]).is_empty() {
            if let Some(claim_ref) = claim_refs.iter().find(|r| r.target_kind == "claim") {
                condition_stmt_id = Some(StatementId(claim_ref.target_claim as u32));
            }
        }
        let cond_quotes = string_array(&stmt["condition"]);
        if condition_stmt_id.is_none() && !cond_quotes.is_empty() {
            if cond_quotes.len() > 1 {
                state.push_blocker(i, "condition", "multiple_conditions_ambiguous");
            } else {
                let cq = cond_quotes[0];
                let c_loc = locate_unique_in_window(ctext, cspan.0, cq)?;
                if let Some(cond_ccid) =
                    state.resolve_concept(cq, Role::Predicate, lookup, budget)?
                {
                    let aux_id = state.alloc_stmt();
                    let cond_args = match left_subject_term.clone() {
                        Some(subj_t) => vec![subj_t],
                        None => vec![TermRef::Concept(cond_ccid)],
                    };
                    aux_frames.push(StatementFrame {
                        statement_id: aux_id,
                        operator_or_predicate: cond_ccid,
                        arguments: cond_args,
                        constraints: Vec::new(),
                        qualifiers: StatementQualifiers {
                            source_spans: vec![SourceSpan {
                                source: source_obj_ref.clone(),
                                start: c_loc.0 as u64,
                                end: c_loc.1 as u64,
                            }],
                            ..StatementQualifiers::default()
                        },
                    });
                    condition_stmt_id = Some(aux_id);
                }
            }
        }
        qualifiers.condition = condition_stmt_id;

        // Time
        let time_quotes = string_array(&stmt["time"]);
        if time_quotes.len() > 1 {
            state.push_blocker(i, "time", "multiple_time_qualifiers");
        } else if let Some(&tq) = time_quotes.first() {
            let _ = locate_unique_in_window(ctext, cspan.0, tq)?;
            if let Some(num) = find_matching_number(&numbers, tq) {
                if let Some(qty) =
                    state.resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
                {
                    qualifiers.time = Some(TermRef::Literal(LiteralValue::Quantity(qty)));
                }
            } else if let Some(ccid) = state.resolve_concept(tq, Role::Object, lookup, budget)? {
                qualifiers.time = Some(TermRef::Concept(ccid));
            }
        }

        // Location
        let loc_quotes = string_array(&stmt["location"]);
        if loc_quotes.len() > 1 {
            state.push_blocker(i, "location", "multiple_location_qualifiers");
        } else if let Some(&lq) = loc_quotes.first() {
            let _ = locate_unique_in_window(ctext, cspan.0, lq)?;
            if let Some(ccid) = state.resolve_concept(lq, Role::Object, lookup, budget)? {
                qualifiers.location = Some(TermRef::Concept(ccid));
            }
        }

        // Frequency -> TypedConstraint(Compare(Concept(QUALIFIER_FREQUENCY_CCID), Equal, term))
        for fq in string_array(&stmt["frequency"]) {
            let _ = locate_unique_in_window(ctext, cspan.0, fq)?;
            let freq_term_opt = if let Some(num) = find_matching_number(&numbers, fq) {
                state
                    .resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
                    .map(|q| TermRef::Literal(LiteralValue::Quantity(q)))
            } else {
                state
                    .resolve_concept(fq, Role::Object, lookup, budget)?
                    .map(TermRef::Concept)
            };
            if let Some(freq_term) = freq_term_opt {
                constraints.push(TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: TermRef::Concept(QUALIFIER_FREQUENCY_CCID),
                        operator: ComparisonOperator::Equal,
                        right: freq_term,
                    },
                    required: true,
                });
            }
        }

        // Approximation -> tolerance or TypedConstraint(Compare(Concept(QUALIFIER_APPROXIMATION_CCID), Equal, term))
        for aq in string_array(&stmt["approximation"]) {
            let _ = locate_unique_in_window(ctext, cspan.0, aq)?;
            if let Some(num) = find_matching_number(&numbers, aq) {
                if let Some(qty) =
                    state.resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
                {
                    qualifiers.tolerance = Some(qty);
                }
            } else if let Some(ccid) = state.resolve_concept(aq, Role::Object, lookup, budget)? {
                constraints.push(TypedConstraint {
                    expression: ConstraintExpression::Compare {
                        left: TermRef::Concept(QUALIFIER_APPROXIMATION_CCID),
                        operator: ComparisonOperator::Equal,
                        right: TermRef::Concept(ccid),
                    },
                    required: true,
                });
            }
        }

        sort_constraints_canonically(&mut constraints)?;

        if let Some(pred) = pred_ccid {
            if !arguments.is_empty() {
                primary_frames.insert(
                    i,
                    StatementFrame {
                        statement_id: StatementId(i as u32),
                        operator_or_predicate: pred,
                        arguments,
                        constraints,
                        qualifiers,
                    },
                );
            }
        }
    }

    // Validate condition DAG acyclicity across assembled statements.
    let mut cond_edges = Vec::new();
    for (&idx, frame) in &primary_frames {
        if let Some(cond_id) = frame.qualifiers.condition {
            if (cond_id.0 as usize) < statements_json.len() {
                cond_edges.push((idx, cond_id.0 as usize));
            }
        }
    }
    if has_directed_cycle(statements_json.len(), &cond_edges) {
        state.push_blocker(0, "condition", "condition_cycle");
    }

    let saveable = state.blockers.is_empty()
        && state.unresolved.is_empty()
        && primary_frames.len() == statements_json.len();

    if !saveable {
        return Ok(LoweredDraftV2 {
            schema: LOWERING_SCHEMA_V2,
            saveable: false,
            semantic: None,
            semantic_content_cid: None,
            bindings: state.bindings,
            unresolved: state.unresolved,
            blockers: state.blockers,
        });
    }

    // Emit statements in deterministic order: primary frames 0..N followed by auxiliary frames.
    let mut all_statements: Vec<StatementFrame> =
        Vec::with_capacity(primary_frames.len() + aux_frames.len());
    for idx in 0..statements_json.len() {
        if let Some(frame) = primary_frames.remove(&idx) {
            all_statements.push(frame);
        }
    }
    all_statements.extend(aux_frames);

    let raw_sfs = SemanticFrameSet {
        statements: all_statements,
    };
    let alpha_sfs = raw_sfs
        .alpha_normalized()
        .map_err(|_| ExtractionError("semantic_validation"))?;
    let normalized = normalize_semantic_content(&alpha_sfs, SEMANTIC_CONTENT_PROFILE)
        .map_err(|_| ExtractionError("semantic_validation"))?;

    Ok(LoweredDraftV2 {
        schema: LOWERING_SCHEMA_V2,
        saveable: true,
        semantic: Some(alpha_sfs),
        semantic_content_cid: Some(normalized.cid),
        bindings: state.bindings,
        unresolved: state.unresolved,
        blockers: state.blockers,
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_role_term(
    claim_idx: usize,
    source: &str,
    source_obj_ref: &ObjectReference,
    role_span: (usize, usize),
    role_quote: &str,
    role_kind: Role,
    numbers: &[NumberSpec],
    alt_by_quote: &BTreeMap<String, &Value>,
    claim_ellipses: &[EllipsisSpec],
    claim_refs: &[ReferenceSpec],
    span_to_root_antecedent: &BTreeMap<(usize, usize), (usize, usize)>,
    root_antecedent_vars: &BTreeMap<(usize, usize), VariableId>,
    resolved_span_terms: &mut BTreeMap<(usize, usize), TermRef>,
    aux_frames: &mut Vec<StatementFrame>,
    state: &mut LoweringStateV2,
    lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
    budget: &mut WorkBudget,
) -> Result<Option<TermRef>> {
    // 1. Check if this role is an elided role reconstructed via `ellipses`.
    if let Some(ell) = claim_ellipses
        .iter()
        .find(|e| e.surface_span == role_span || e.surface_quote == role_quote)
    {
        if let Some(borrowed_term) = resolved_span_terms.get(&ell.borrowed_span).cloned() {
            resolved_span_terms.insert(role_span, borrowed_term.clone());
            return Ok(Some(borrowed_term));
        } else if let Some(ccid) =
            state.resolve_concept(&ell.borrowed_quote, role_kind, lookup, budget)?
        {
            let term = if let Some(&root_sp) = span_to_root_antecedent.get(&ell.borrowed_span) {
                let var_id = root_antecedent_vars[&root_sp];
                TermRef::Variable {
                    id: var_id,
                    type_constraint: Some(ccid),
                }
            } else {
                TermRef::Concept(ccid)
            };
            resolved_span_terms.insert(ell.borrowed_span, term.clone());
            resolved_span_terms.insert(role_span, term.clone());
            return Ok(Some(term));
        } else {
            state.push_blocker(claim_idx, "ellipses", "unresolved_borrowed_role");
            return Ok(None);
        }
    }

    // 2. Check if this role is an outgoing coreference anaphor (`via` in `references`).
    if let Some(cref) = claim_refs.iter().find(|r| {
        (r.target_kind == "term" || r.target_kind == "group")
            && (r.via_span == role_span || r.via_quote == role_quote)
    }) {
        if let Some(&root_sp) = span_to_root_antecedent.get(&cref.via_span) {
            if let Some(root_term) = resolved_span_terms.get(&root_sp).cloned() {
                let shared_var_term = match root_term {
                    TermRef::Variable { .. } => root_term,
                    TermRef::Concept(c) => TermRef::Variable {
                        id: root_antecedent_vars[&root_sp],
                        type_constraint: Some(c),
                    },
                    _ => TermRef::Variable {
                        id: root_antecedent_vars[&root_sp],
                        type_constraint: None,
                    },
                };
                resolved_span_terms.insert(role_span, shared_var_term.clone());
                return Ok(Some(shared_var_term));
            }
        }
        state.push_blocker(claim_idx, "references", "target_unresolved");
        return Ok(None);
    }

    // 3. Check if this role is an `alternatives` group!
    if let Some(&group) = alt_by_quote.get(role_quote) {
        let exclusivity = group["exclusivity"].as_str().unwrap_or("");
        let disj_op = match exclusivity {
            "inclusive" => DISJUNCTION_INCLUSIVE_OR_CCID,
            "exclusive" => DISJUNCTION_EXCLUSIVE_XOR_CCID,
            _ => {
                state.push_blocker(claim_idx, "alternatives", "exclusivity_unspecified");
                return Ok(None);
            }
        };
        let Some(branches) = group["branches"].as_array() else {
            state.push_blocker(claim_idx, "alternatives", "invalid_branches");
            return Ok(None);
        };
        if !(2..=16).contains(&branches.len()) {
            state.push_blocker(claim_idx, "alternatives", "branch_count");
            return Ok(None);
        }

        let group_text = &source[role_span.0..role_span.1];
        let mut branch_terms: Vec<TermRef> = Vec::with_capacity(branches.len());
        let mut has_inter_branch_dependency = false;

        for br in branches {
            let surf_q = br["surface"]["quote"].as_str().unwrap_or("");
            let br_span_opt = resolve_selector_span(
                source,
                &br["surface"],
                group_text,
                role_span.0,
                br.get("span"),
            );
            let effective_label = if let Some(borrowed_obj) = br.get("borrowed") {
                has_inter_branch_dependency = true;
                let bq = borrowed_obj["quote"].as_str().unwrap_or("");
                format!("{surf_q} {bq}")
            } else {
                surf_q.to_string()
            };

            let term_opt = if let Some(num) = find_matching_number(numbers, &effective_label) {
                state
                    .resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
                    .map(|q| TermRef::Literal(LiteralValue::Quantity(q)))
            } else {
                let primary = state.resolve_concept(&effective_label, role_kind, lookup, budget)?;
                primary.map(TermRef::Concept)
            };

            if let Some(mut bterm) = term_opt {
                if let Some(bsp) = br_span_opt {
                    if let Some(&root_sp) = span_to_root_antecedent.get(&bsp) {
                        if root_sp == bsp {
                            let expected = match &bterm {
                                TermRef::Concept(c) => Some(*c),
                                _ => None,
                            };
                            bterm = TermRef::Variable {
                                id: root_antecedent_vars[&root_sp],
                                type_constraint: expected,
                            };
                        }
                    }
                    resolved_span_terms.insert(bsp, bterm.clone());
                }
                branch_terms.push(bterm);
            }
        }

        if branch_terms.len() != branches.len() {
            return Ok(None);
        }

        // Commutative canonical sorting when `"inclusive"` and dependency-free!
        if exclusivity == "inclusive" && !has_inter_branch_dependency {
            sort_terms_canonically(&mut branch_terms)?;
        }

        let group_var = if let Some(&root_sp) = span_to_root_antecedent.get(&role_span) {
            root_antecedent_vars[&root_sp]
        } else {
            state.alloc_var()
        };
        let group_term = TermRef::Variable {
            id: group_var,
            type_constraint: None,
        };
        let mut disj_args = Vec::with_capacity(1 + branch_terms.len());
        disj_args.push(group_term.clone());
        disj_args.extend(branch_terms);

        let aux_id = state.alloc_stmt();
        aux_frames.push(StatementFrame {
            statement_id: aux_id,
            operator_or_predicate: disj_op,
            arguments: disj_args,
            constraints: Vec::new(),
            qualifiers: StatementQualifiers {
                source_spans: vec![SourceSpan {
                    source: source_obj_ref.clone(),
                    start: role_span.0 as u64,
                    end: role_span.1 as u64,
                }],
                ..StatementQualifiers::default()
            },
        });
        resolved_span_terms.insert(role_span, group_term.clone());
        return Ok(Some(group_term));
    }

    // 4. Check if this role is a quantity literal in `numbers`.
    if let Some(num) = find_matching_number(numbers, role_quote) {
        if !num.counted_entity_quote.is_empty() {
            state.push_blocker(claim_idx, "numbers", "counted_entity_unlowered");
            return Ok(None);
        }
        if let Some(qty) =
            state.resolve_quantity(&num.value_quote, &num.unit_quote, lookup, budget)?
        {
            let term = TermRef::Literal(LiteralValue::Quantity(qty));
            resolved_span_terms.insert(role_span, term.clone());
            return Ok(Some(term));
        }
        return Ok(None);
    }

    // 5. Standard concept resolution; promote to `TermRef::Variable` if this span is a
    // coreference root antecedent so anaphoric references share the exact same `VariableId`!
    if let Some(ccid) = state.resolve_concept(role_quote, role_kind, lookup, budget)? {
        let term = if let Some(&root_sp) = span_to_root_antecedent.get(&role_span) {
            if root_sp == role_span {
                TermRef::Variable {
                    id: root_antecedent_vars[&root_sp],
                    type_constraint: Some(ccid),
                }
            } else {
                TermRef::Concept(ccid)
            }
        } else {
            TermRef::Concept(ccid)
        };
        resolved_span_terms.insert(role_span, term.clone());
        Ok(Some(term))
    } else {
        Ok(None)
    }
}

fn build_quantity_literal(
    value: ExactRatio,
    unit_ccid: [u8; 16],
    transform: UnitTransform,
) -> Result<QuantityLiteral> {
    require(
        transform.scale.1 > 0 && transform.offset.1 > 0,
        "unit_transform",
    )?;
    let scale = ExactRatio::new(transform.scale.0, transform.scale.1)
        .map_err(|_| ExtractionError("unit_transform"))?;
    let offset = ExactRatio::new(transform.offset.0, transform.offset.1)
        .map_err(|_| ExtractionError("unit_transform"))?;
    let qty = QuantityLiteral {
        value,
        source_unit: UnitRef {
            unit: ConceptCcid::from_bytes(unit_ccid),
            dimension: DimensionVector::new(transform.dimension),
            scale_to_base: scale,
            offset_to_base: offset,
        },
    };
    // Verify SI base conversion does not overflow (`base = source * scale + offset`).
    let _ = qty
        .to_base_value()
        .map_err(|_| ExtractionError("number_overflow"))?;
    Ok(qty)
}

fn parse_numbers(v: &Value) -> Result<Vec<NumberSpec>> {
    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for item in arr {
            out.push(NumberSpec {
                value_quote: item["value_quote"].as_str().unwrap_or("").to_string(),
                unit_quote: item["unit_quote"].as_str().unwrap_or("").to_string(),
                counted_entity_quote: item["counted_entity_quote"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }
    Ok(out)
}

fn find_matching_number<'a>(numbers: &'a [NumberSpec], quote: &str) -> Option<&'a NumberSpec> {
    let trimmed = quote.trim();
    numbers.iter().find(|n| {
        !n.unit_quote.is_empty()
            && trimmed.contains(n.value_quote.as_str())
            && trimmed.contains(n.unit_quote.as_str())
    })
}

fn parse_comparison_operator(cue: &str) -> Option<ComparisonOperator> {
    let norm = cue.trim().to_lowercase();
    match norm.as_str() {
        ">=" | "≥" | "at least" | "no less than" | "ít nhất" => {
            Some(ComparisonOperator::GreaterThanOrEqual)
        }
        "<=" | "≤" | "at most" | "no more than" | "tối đa" => {
            Some(ComparisonOperator::LessThanOrEqual)
        }
        ">" | "greater than" | "more than" | "higher than" | "longer than" | "larger than"
        | "faster than" | "heavier than" | "warmer than" | "than" | "hơn" | "lớn hơn"
        | "cao hơn" | "dài hơn" | "nhiều hơn" | "nặng hơn" => {
            Some(ComparisonOperator::GreaterThan)
        }
        "<" | "less than" | "fewer than" | "lower than" | "shorter than" | "smaller than"
        | "slower than" | "lighter than" | "cooler than" | "kém" | "nhỏ hơn" | "thấp hơn"
        | "ít hơn" | "ngắn hơn" => Some(ComparisonOperator::LessThan),
        "!=" | "≠" | "different from" | "khác" => Some(ComparisonOperator::NotEqual),
        "=" | "==" | "equal to" | "equals" | "as" | "bằng" | "như" => {
            Some(ComparisonOperator::Equal)
        }
        _ => None,
    }
}

fn parse_modality_cue(cue: &str) -> Option<Modality> {
    let norm = cue.trim().to_lowercase();
    match norm.as_str() {
        "asserted" => Some(Modality::Asserted),
        "observed" | "quan sát" => Some(Modality::Observed),
        "reported" | "theo" => Some(Modality::Reported),
        "possible" | "possibly" | "may" | "might" | "can" | "could" | "có thể" => {
            Some(Modality::Possible)
        }
        "necessary" | "obligatory" | "must" | "shall" | "required" | "phải" | "bắt buộc"
        | "cần phải" => Some(Modality::Necessary),
        "desired" | "should" | "nên" => Some(Modality::Desired),
        _ => None,
    }
}

fn sort_terms_canonically(terms: &mut [TermRef]) -> Result<()> {
    let mut keyed: Vec<(Vec<u8>, TermRef)> = Vec::with_capacity(terms.len());
    for term in terms.iter() {
        let probe = TypedConstraint {
            expression: ConstraintExpression::Compare {
                left: term.clone(),
                operator: ComparisonOperator::Equal,
                right: term.clone(),
            },
            required: true,
        };
        let bytes = encode_canonical(&probe.canonical_value(), ResourceProfile::ObjectV1)
            .map_err(|_| ExtractionError("semantic_validation"))?;
        keyed.push((bytes, term.clone()));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (slot, (_, term)) in terms.iter_mut().zip(keyed) {
        *slot = term;
    }
    Ok(())
}

fn sort_constraints_canonically(constraints: &mut Vec<TypedConstraint>) -> Result<()> {
    let mut keyed: Vec<(Vec<u8>, TypedConstraint)> = Vec::with_capacity(constraints.len());
    for c in constraints.drain(..) {
        let bytes = encode_canonical(&c.canonical_value(), ResourceProfile::ObjectV1)
            .map_err(|_| ExtractionError("semantic_validation"))?;
        keyed.push((bytes, c));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|a, b| a.0 == b.0);
    *constraints = keyed.into_iter().map(|(_, c)| c).collect();
    Ok(())
}

fn string_array(v: &Value) -> Vec<&str> {
    v.as_array()
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn locate_unique_in_window(
    window_text: &str,
    window_offset: usize,
    quote: &str,
) -> Result<(usize, usize)> {
    let found = quote_positions(window_text, quote);
    require(found.len() == 1, "review_claim_quote_ambiguous")?;
    Ok((window_offset + found[0].0, window_offset + found[0].1))
}

fn resolve_selector_span(
    source: &str,
    selector: &Value,
    default_scope: &str,
    default_offset: usize,
    explicit_span: Option<&Value>,
) -> Option<(usize, usize)> {
    if let Some(arr) = explicit_span.and_then(Value::as_array) {
        if arr.len() == 2 {
            if let (Some(s), Some(e)) = (arr[0].as_u64(), arr[1].as_u64()) {
                let (s, e) = (s as usize, e as usize);
                if s < e && source.get(s..e) == selector["quote"].as_str() {
                    return Some((s, e));
                }
            }
        }
    }
    let quote = selector["quote"].as_str()?;
    if let Some(within) = selector["within"].as_str() {
        let scopes = quote_positions(source, within);
        if scopes.len() != 1 {
            return None;
        }
        let local = quote_positions(within, quote);
        if local.len() == 1 {
            return Some((scopes[0].0 + local[0].0, scopes[0].0 + local[0].1));
        }
        None
    } else {
        let local = quote_positions(default_scope, quote);
        if local.len() == 1 {
            Some((default_offset + local[0].0, default_offset + local[0].1))
        } else {
            None
        }
    }
}

fn has_directed_cycle(node_count: usize, edges: &[(usize, usize)]) -> bool {
    let mut color = vec![0u8; node_count];
    fn dfs(u: usize, edges: &[(usize, usize)], color: &mut [u8]) -> bool {
        color[u] = 1;
        for &(from, to) in edges {
            if from == u {
                if color[to] == 1 {
                    return true;
                }
                if color[to] == 0 && dfs(to, edges, color) {
                    return true;
                }
            }
        }
        color[u] = 2;
        false
    }
    for u in 0..node_count {
        if color[u] == 0 && dfs(u, edges, &mut color) {
            return true;
        }
    }
    false
}

fn has_span_cycle(parent_map: &BTreeMap<(usize, usize), (usize, usize)>) -> bool {
    for &start in parent_map.keys() {
        let mut seen = BTreeSet::new();
        let mut cur = start;
        while let Some(&next) = parent_map.get(&cur) {
            if !seen.insert(cur) {
                return true;
            }
            if next == start {
                return true;
            }
            cur = next;
        }
    }
    false
}

fn topological_claim_order(
    n: usize,
    ref_edges: &[(usize, usize)],
    ellipses: &[Vec<EllipsisSpec>],
    claim_spans: &[(usize, usize)],
) -> Vec<usize> {
    // Dependency: if claim `i` references or borrows from claim `j`, resolve `j` BEFORE `i`.
    let mut deps: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for &(from_claim, to_claim) in ref_edges {
        if from_claim < n && to_claim < n && from_claim != to_claim {
            deps[from_claim].insert(to_claim);
        }
    }
    for (i, ells) in ellipses.iter().enumerate() {
        for ell in ells {
            for (j, &(cs, ce)) in claim_spans.iter().enumerate() {
                if j != i && cs <= ell.borrowed_span.0 && ell.borrowed_span.1 <= ce {
                    deps[i].insert(j);
                }
            }
        }
    }

    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    while order.len() < n {
        let next_opt = (0..n).find(|&i| !visited[i] && deps[i].iter().all(|&d| visited[d]));
        let idx = next_opt.unwrap_or_else(|| (0..n).find(|&i| !visited[i]).unwrap());
        visited[idx] = true;
        order.push(idx);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Duration;

    fn budget() -> WorkBudget {
        WorkBudget::new(
            1_000_000,
            Duration::from_secs(30),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }

    fn registry() -> BTreeMap<&'static str, LookupOption> {
        let concept = |n: u8| LookupOption {
            ccid: [n; 16],
            unit: None,
        };
        BTreeMap::from([
            ("copper", concept(1)),
            ("conducts", concept(2)),
            ("electricity", concept(3)),
            ("heat", concept(4)),
            ("resists", concept(5)),
            ("corrosion", concept(6)),
            ("aluminum", concept(7)),
            ("bridge", concept(8)),
            ("length", concept(9)),
            ("laboratory", concept(10)),
            ("daily", concept(11)),
            ("approximately", concept(12)),
            ("heated", concept(13)),
            ("winter", concept(14)),
            (
                "m",
                LookupOption {
                    ccid: [20; 16],
                    unit: Some(UnitTransform {
                        dimension: [1, 0, 0, 0, 0, 0, 0],
                        scale: (1, 1),
                        offset: (0, 1),
                    }),
                },
            ),
            (
                "km",
                LookupOption {
                    ccid: [21; 16],
                    unit: Some(UnitTransform {
                        dimension: [1, 0, 0, 0, 0, 0, 0],
                        scale: (1000, 1),
                        offset: (0, 1),
                    }),
                },
            ),
        ])
    }

    fn run_lower_draft(source: &str, draft: &Value) -> LoweredDraftV2 {
        let reg = registry();
        let mut lookup = |label: &str, role: Role| -> Result<Vec<LookupOption>> {
            Ok(reg
                .get(label.to_lowercase().as_str())
                .filter(|o| (role == Role::Unit) == o.unit.is_some())
                .cloned()
                .into_iter()
                .collect())
        };
        lower_draft_v2(source, [9; 32], [8; 32], draft, &mut lookup, &mut budget()).unwrap()
    }

    fn base_stmt(subject: &str, predicate: &str, args: Value, evidence: &str) -> Value {
        json!({
            "id": "claim-0",
            "subject": subject,
            "predicate": predicate,
            "arguments": args,
            "frequency": [],
            "negation": [],
            "condition": [],
            "time": [],
            "location": [],
            "modality": [],
            "approximation": [],
            "numbers": [],
            "relations": [],
            "evidence": evidence,
            "alternatives": [],
            "ellipses": [],
            "references": [],
            "comparisons": [],
            "evidence_spans": []
        })
    }

    #[test]
    fn tier0_structural_ccids_match_exact_blake3_domains() {
        let check = |domain: &[u8], expected: ConceptCcid| {
            let h = blake3::hash(domain);
            assert_eq!(expected.as_bytes(), &h.as_bytes()[..16]);
        };
        check(
            b"onebrain:vnext:tier0:inclusive-or:1\0",
            DISJUNCTION_INCLUSIVE_OR_CCID,
        );
        check(
            b"onebrain:vnext:tier0:exclusive-xor:1\0",
            DISJUNCTION_EXCLUSIVE_XOR_CCID,
        );
        check(
            b"onebrain:vnext:tier0:qualifier-frequency:1\0",
            QUALIFIER_FREQUENCY_CCID,
        );
        check(
            b"onebrain:vnext:tier0:qualifier-approximation:1\0",
            QUALIFIER_APPROXIMATION_CCID,
        );
    }

    #[test]
    fn alternatives_block_unspecified_and_commutatively_sort_inclusive_or() {
        let src_a = "Copper conducts heat or electricity.";
        let mut s_unspec = base_stmt(
            "Copper",
            "conducts",
            json!(["heat or electricity"]),
            "Copper conducts heat or electricity",
        );
        s_unspec["alternatives"] = json!([{
            "quote": "heat or electricity",
            "cue": "or",
            "exclusivity": "unspecified",
            "branches": [
                {"surface": {"quote": "heat"}},
                {"surface": {"quote": "electricity"}}
            ]
        }]);
        let reg = registry();
        let mut lookup = |label: &str, role: Role| -> Result<Vec<LookupOption>> {
            Ok(reg
                .get(label.to_lowercase().as_str())
                .filter(|o| (role == Role::Unit) == o.unit.is_some())
                .cloned()
                .into_iter()
                .collect())
        };
        let blocked = lower_claim_v2(
            src_a,
            [1; 32],
            [2; 32],
            &s_unspec,
            &mut lookup,
            &mut budget(),
        )
        .unwrap();
        assert!(!blocked.saveable);
        assert!(blocked.semantic.is_none());
        assert!(blocked
            .blockers
            .iter()
            .any(|b| b.code == "exclusivity_unspecified"));

        // Inclusive OR: "heat or electricity" vs "electricity or heat" -> identical SemanticContentCid!
        let mut s_inc_1 = s_unspec.clone();
        s_inc_1["alternatives"][0]["exclusivity"] = json!("inclusive");
        let out_1 = lower_claim_v2(
            src_a,
            [1; 32],
            [2; 32],
            &s_inc_1,
            &mut lookup,
            &mut budget(),
        )
        .unwrap();
        assert!(out_1.saveable);

        let src_b = "Copper conducts electricity or heat.";
        let mut s_inc_2 = base_stmt(
            "Copper",
            "conducts",
            json!(["electricity or heat"]),
            "Copper conducts electricity or heat",
        );
        s_inc_2["alternatives"] = json!([{
            "quote": "electricity or heat",
            "cue": "or",
            "exclusivity": "inclusive",
            "branches": [
                {"surface": {"quote": "electricity"}},
                {"surface": {"quote": "heat"}}
            ]
        }]);
        let out_2 = lower_claim_v2(
            src_b,
            [1; 32],
            [2; 32],
            &s_inc_2,
            &mut lookup,
            &mut budget(),
        )
        .unwrap();
        assert!(out_2.saveable);
        assert_eq!(out_1.semantic_content_cid, out_2.semantic_content_cid);

        // Exclusive XOR produces a distinct SemanticContentCid from Inclusive OR.
        let mut s_exc = s_inc_1.clone();
        s_exc["alternatives"][0]["exclusivity"] = json!("exclusive");
        let out_exc =
            lower_claim_v2(src_a, [1; 32], [2; 32], &s_exc, &mut lookup, &mut budget()).unwrap();
        assert!(out_exc.saveable);
        assert_ne!(out_exc.semantic_content_cid, out_1.semantic_content_cid);
    }

    #[test]
    fn references_validate_dag_and_alpha_equivalent_phrasings_produce_identical_semantic_cid() {
        // Phrasing 1: "It" refers to "Copper"
        let src_1 = "Copper conducts electricity. It resists corrosion.";
        let mut c0_1 = base_stmt(
            "Copper",
            "conducts",
            json!(["electricity"]),
            "Copper conducts electricity",
        );
        c0_1["id"] = json!("claim-0");
        let mut c1_1 = base_stmt(
            "It",
            "resists",
            json!(["corrosion"]),
            "It resists corrosion",
        );
        c1_1["id"] = json!("claim-1");
        c1_1["references"] = json!([{
            "via": {"quote": "It", "within": "It resists corrosion"},
            "to": {"quote": "Copper", "within": "Copper conducts electricity"},
            "target_kind": "term"
        }]);
        let draft_1 = json!({
            "profile": "ku-semantic-draft/2.0",
            "semantic_nodes": 6,
            "unresolved": [],
            "statements": [c0_1.clone(), c1_1]
        });

        // Phrasing 2: "That metal" refers to "Copper"
        let src_2 = "Copper conducts electricity. That metal resists corrosion.";
        let mut c1_2 = base_stmt(
            "That metal",
            "resists",
            json!(["corrosion"]),
            "That metal resists corrosion",
        );
        c1_2["id"] = json!("claim-1");
        c1_2["references"] = json!([{
            "via": {"quote": "That metal", "within": "That metal resists corrosion"},
            "to": {"quote": "Copper", "within": "Copper conducts electricity"},
            "target_kind": "term"
        }]);
        let draft_2 = json!({
            "profile": "ku-semantic-draft/2.0",
            "semantic_nodes": 6,
            "unresolved": [],
            "statements": [c0_1, c1_2]
        });

        let out_1 = run_lower_draft(src_1, &draft_1);
        let out_2 = run_lower_draft(src_2, &draft_2);
        assert!(out_1.saveable);
        assert!(out_2.saveable);
        assert_eq!(out_1.semantic_content_cid, out_2.semantic_content_cid);

        // Cyclic coreference across two claims fails closed with `cycle` blocker!
        let src_cyc = "He conducts electricity. She resists corrosion.";
        let mut cyc_0 = base_stmt(
            "He",
            "conducts",
            json!(["electricity"]),
            "He conducts electricity",
        );
        cyc_0["references"] = json!([{
            "via": {"quote": "He", "within": "He conducts electricity"},
            "to": {"quote": "She", "within": "She resists corrosion"},
            "target_kind": "term"
        }]);
        let mut cyc_1 = base_stmt(
            "She",
            "resists",
            json!(["corrosion"]),
            "She resists corrosion",
        );
        cyc_1["id"] = json!("claim-1");
        cyc_1["references"] = json!([{
            "via": {"quote": "She", "within": "She resists corrosion"},
            "to": {"quote": "He", "within": "He conducts electricity"},
            "target_kind": "term"
        }]);
        let cyc_draft = json!({
            "profile": "ku-semantic-draft/2.0",
            "semantic_nodes": 6,
            "unresolved": [],
            "statements": [cyc_0, cyc_1]
        });
        let cyc_out = run_lower_draft(src_cyc, &cyc_draft);
        assert!(!cyc_out.saveable);
        assert!(cyc_out.blockers.iter().any(|b| b.code == "cycle"));
    }

    #[test]
    fn ellipses_comparisons_and_qualifiers_lower_deterministically_and_block_incomplete() {
        // Ellipsis: second clause borrows object "electricity" from first clause.
        let src = "Copper conducts electricity, and aluminum conducts too.";
        let c0 = base_stmt(
            "Copper",
            "conducts",
            json!(["electricity"]),
            "Copper conducts electricity",
        );
        let mut c1 = base_stmt(
            "aluminum",
            "conducts",
            json!(["too"]),
            "aluminum conducts too",
        );
        c1["id"] = json!("claim-1");
        c1["ellipses"] = json!([{
            "surface": {"quote": "too", "within": "aluminum conducts too"},
            "borrowed": {"quote": "electricity", "within": "Copper conducts electricity"}
        }]);
        let draft = json!({
            "profile": "ku-semantic-draft/2.0",
            "semantic_nodes": 6,
            "unresolved": [],
            "statements": [c0, c1]
        });
        let out = run_lower_draft(src, &draft);
        assert!(out.saveable);
        let sfs = out.semantic.unwrap();
        assert_eq!(sfs.statements.len(), 2);
        assert_eq!(
            sfs.statements[1].arguments[1],
            TermRef::Concept(ConceptCcid::from_bytes([3; 16]))
        );

        // Explicit SI-normalized comparison + negation + modality + time + location + frequency + approximation:
        let src_cmp =
            "Bridge may not length more than 2 km daily in winter at laboratory approximately.";
        let mut sc = base_stmt("Bridge", "length", json!([]), src_cmp);
        sc["negation"] = json!(["not"]);
        sc["modality"] = json!(["may"]);
        sc["time"] = json!(["winter"]);
        sc["location"] = json!(["laboratory"]);
        sc["frequency"] = json!(["daily"]);
        sc["approximation"] = json!(["approximately"]);
        sc["numbers"] = json!([{
            "value_quote": "2",
            "unit_quote": "km",
            "counted_entity_quote": ""
        }]);
        sc["comparisons"] = json!([{
            "property": "length",
            "cue": "more than",
            "target_kind": "explicit",
            "target": {"quote": "2 km", "within": src_cmp}
        }]);
        let cmp_draft = json!({
            "profile": "ku-semantic-draft/2.0",
            "semantic_nodes": 5,
            "unresolved": [],
            "statements": [sc.clone()]
        });
        let cmp_out = run_lower_draft(src_cmp, &cmp_draft);
        assert!(cmp_out.saveable);
        let stmt = &cmp_out.semantic.unwrap().statements[0];
        assert!(stmt.qualifiers.negated);
        assert_eq!(stmt.qualifiers.modality, Modality::Possible);
        assert_eq!(
            stmt.qualifiers.time,
            Some(TermRef::Concept(ConceptCcid::from_bytes([14; 16])))
        );
        assert_eq!(
            stmt.qualifiers.location,
            Some(TermRef::Concept(ConceptCcid::from_bytes([10; 16])))
        );
        assert!(stmt.constraints.iter().any(|c| matches!(
            &c.expression,
            ConstraintExpression::Compare {
                operator: ComparisonOperator::GreaterThan,
                right: TermRef::Literal(LiteralValue::Quantity(q)),
                ..
            } if q.to_base_value().unwrap() == ExactRatio::new(2000, 1).unwrap()
        )));

        // Implicit candidate comparison stays `saveable: false`.
        let mut sc_imp = sc;
        sc_imp["comparisons"][0]["target_kind"] = json!("implicit_candidate");
        let imp_out = run_lower_draft(
            src_cmp,
            &json!({
                "profile": "ku-semantic-draft/2.0",
                "semantic_nodes": 5,
                "unresolved": [],
                "statements": [sc_imp]
            }),
        );
        assert!(!imp_out.saveable);
        assert!(imp_out
            .blockers
            .iter()
            .any(|b| b.code == "implicit_target_unassessed"));
    }
}
