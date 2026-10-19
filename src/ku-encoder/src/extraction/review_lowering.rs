//! ARCH-07: lower one explicitly reviewed, simple review-draft claim to a SEM
//! draft through the shared checked compiler (`compiler::compile`).
//!
//! Supported shapes only: concept–predicate–concept and
//! concept–predicate–quantity (number plus unit), with no qualifiers or
//! structured meaning. The host supplies verified Registry lookups and unit
//! transforms; the model is never consulted and grants no authority. A term
//! without exactly one CCID (or a unit without a host transform) leaves the
//! result unresolved, so the caller must keep it unsaveable.
use super::*;
use ku_core::foundation::semantic::SemanticFrameSet;
use review_draft::quote_positions;
use serde_json::{json, Value};

pub const PROFILE: &str = "ku-review-lowering/1.0";
const QUALIFIERS: &[&str] = &[
    "frequency",
    "negation",
    "condition",
    "time",
    "location",
    "modality",
    "approximation",
    "relations",
    "alternatives",
    "ellipses",
    "references",
    "comparisons",
];

/// Host-owned exact transform `base = source * scale + offset` in SI base order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitTransform {
    pub dimension: [i8; 7],
    pub scale: (i64, u64),
    pub offset: (i64, u64),
}

/// One verified Registry candidate for a label. `unit` is set only for unit
/// concepts the host can transform exactly.
#[derive(Clone, Debug)]
pub struct LookupOption {
    pub ccid: [u8; 16],
    pub unit: Option<UnitTransform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Subject,
    Predicate,
    Object,
    Unit,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Subject => "subject",
            Role::Predicate => "predicate",
            Role::Object => "object",
            Role::Unit => "unit",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoweringBinding {
    pub label: String,
    pub ccid: [u8; 16],
}
#[derive(Clone, Debug)]
pub struct Unresolved {
    pub role: Role,
    pub label: String,
    pub candidates: usize,
}
pub struct Lowered {
    /// `Some` only when every term resolved and the compiler accepted it.
    pub semantic: Option<SemanticFrameSet>,
    pub bindings: Vec<LoweringBinding>,
    pub unresolved: Vec<Unresolved>,
}

pub fn commitment() -> Result<String> {
    hash(&json!([
        PROFILE,
        include_str!("review_lowering.rs"),
        include_str!("compiler.rs"),
        include_str!("schema.rs")
    ]))
}

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn ratio((n, d): (i64, u64)) -> Result<Value> {
    require(d > 0, "unit_transform")?;
    let g = gcd(n.unsigned_abs(), d).max(1);
    let n = n / g as i64;
    Ok(json!({"numerator": n.to_string(), "denominator": (d / g).to_string()}))
}

struct Builder<'a> {
    source: &'a str,
    root: [u8; 32],
    concepts: Vec<Value>,
    options: Vec<Value>,
    bindings: Vec<Value>,
    lowered: Vec<LoweringBinding>,
    unresolved: Vec<Unresolved>,
}
impl Builder<'_> {
    fn span(&self, (s, e): (usize, usize)) -> Value {
        json!({"start": s, "end": e, "quote": &self.source[s..e]})
    }
    fn concept(
        &mut self,
        key: &str,
        role: Role,
        at: (usize, usize),
        lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
        b: &mut WorkBudget,
    ) -> Result<()> {
        let label = &self.source[at.0..at.1];
        b.charge(label.len() + 1024)?;
        let found = lookup(label, role)?;
        require(found.len() <= 64, "registry_ambiguous_limit")?;
        require(
            self.options.len() + found.len() <= 256,
            "registry_ambiguous_limit",
        )?;
        let mention = self.span(at);
        let mut first = None;
        for option in &found {
            let key = format!("o{}", self.options.len());
            let mut value = json!({"key": key, "ccid": hex(&option.ccid), "lookup_label": label, "mention": mention});
            if let Some(u) = option.unit {
                value["unit"] = json!({"dimension": u.dimension, "scale": ratio(u.scale)?, "offset": ratio(u.offset)?});
            }
            first.get_or_insert_with(|| value.clone());
            self.options.push(value);
        }
        self.concepts
            .push(json!({"key": key, "label": label, "evidence": mention}));
        let usable = found.len() == 1 && (role == Role::Unit) == found[0].unit.is_some();
        if let (true, Some(option)) = (usable, first) {
            let provenance = hash(&json!({"root": hex(&self.root), "option": option}))?;
            self.bindings.push(json!({"concept": key, "option": option["key"], "selection": "exact_label", "provenance_sha256": provenance}));
            self.lowered.push(LoweringBinding {
                label: label.into(),
                ccid: found[0].ccid,
            });
        } else {
            self.unresolved.push(Unresolved {
                role,
                label: label.into(),
                candidates: found.len(),
            });
        }
        Ok(())
    }
}

/// Lower one review-draft claim (`ReviewClaim` JSON). Errors use a fixed
/// vocabulary and never echo source text or model output.
pub fn lower_claim(
    source: &str,
    source_ref: [u8; 32],
    registry_root: [u8; 32],
    claim: &Value,
    lookup: &mut dyn FnMut(&str, Role) -> Result<Vec<LookupOption>>,
    b: &mut WorkBudget,
) -> Result<Lowered> {
    require(!source.is_empty() && source.len() <= 8192, "source_bound")?;
    require(claim.is_object(), "review_claim_unsupported")?;
    for key in QUALIFIERS {
        require(
            claim
                .get(*key)
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
            "review_claim_unsupported",
        )?;
    }
    let subject = claim["subject"].as_str().unwrap_or("");
    let predicate = claim["predicate"].as_str().unwrap_or("");
    let evidence = claim["evidence"].as_str().unwrap_or("");
    let arguments = strings(&claim["arguments"]);
    let numbers = claim["numbers"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    require(
        !subject.is_empty() && !predicate.is_empty() && !evidence.is_empty(),
        "review_claim_unsupported",
    )?;
    b.charge(source.len() + evidence.len() * 4)?;
    let found = quote_positions(source, evidence);
    require(found.len() == 1, "review_claim_evidence")?;
    let (es, ee) = found[0];
    let locate = |quote: &str| -> Result<(usize, usize)> {
        let local = quote_positions(evidence, quote);
        require(local.len() == 1, "review_claim_quote_ambiguous")?;
        Ok((es + local[0].0, es + local[0].1))
    };
    let mut builder = Builder {
        source,
        root: registry_root,
        concepts: vec![],
        options: vec![],
        bindings: vec![],
        lowered: vec![],
        unresolved: vec![],
    };
    let subject_at = locate(subject)?;
    let predicate_at = locate(predicate)?;
    builder.concept("subject", Role::Subject, subject_at, lookup, b)?;
    builder.concept("predicate", Role::Predicate, predicate_at, lookup, b)?;
    let mut args = vec![
        json!({"kind": "concept", "concept": "subject", "evidence": builder.span(subject_at)}),
    ];
    match numbers {
        [] => {
            require(
                arguments.len() == 1 && !arguments[0].is_empty(),
                "review_claim_unsupported",
            )?;
            let at = locate(arguments[0])?;
            builder.concept("object", Role::Object, at, lookup, b)?;
            args.push(
                json!({"kind": "concept", "concept": "object", "evidence": builder.span(at)}),
            );
        }
        [number] => {
            let value = number["value_quote"].as_str().unwrap_or("");
            let unit = number["unit_quote"].as_str().unwrap_or("");
            let counted = number["counted_entity_quote"].as_str().unwrap_or("");
            require(
                !unit.is_empty() && counted.is_empty() && arguments.len() <= 1,
                "review_claim_unsupported",
            )?;
            require(
                arguments
                    .first()
                    .is_none_or(|a| a.contains(value) && a.contains(unit)),
                "review_claim_unsupported",
            )?;
            compiler::exact_number(value)?;
            let number_at = locate(value)?;
            let unit_at = locate(unit)?;
            builder.concept("unit", Role::Unit, unit_at, lookup, b)?;
            args.push(json!({"kind": "quantity", "number": builder.span(number_at), "unit": "unit", "unit_evidence": builder.span(unit_at)}));
        }
        _ => return Err(ExtractionError("review_claim_unsupported")),
    }
    let claim_span = builder.span((es, ee));
    let attempt =
        hash(&json!({"profile": PROFILE, "source_ref": hex(&source_ref), "claim": claim}))?;
    let context = json!({"profile": "ku-extraction/1.0", "attempt_id": attempt, "source_ref": hex(&source_ref),
        "source_text": source, "registry_root": hex(&registry_root), "resource_profile": "standard",
        "windows": [{"key": "focus", "start": 0, "end": source.len(), "role": "focus"}],
        "required_units": [{"key": "claim", "span": claim_span}], "options": builder.options});
    let digest = hash(&context)?;
    let candidate = json!({"profile": "ku-extraction/1.0", "attempt_id": attempt, "context_sha256": digest,
        "concepts": builder.concepts,
        "statements": [{"key": "s0", "evidence": [claim_span], "predicate": "predicate", "arguments": args,
            "negation": {"value": false, "evidence": []}, "modality": {"value": "asserted", "evidence": []}}],
        "coverage": [{"unit": "claim", "status": "represented", "statements": ["s0"], "reason": "none"}]});
    let resolution =
        json!({"attempt_id": attempt, "context_sha256": digest, "bindings": builder.bindings});
    let semantic = compiler::compile(&context, &candidate, &resolution, b)?;
    require(
        semantic.is_some() == builder.unresolved.is_empty(),
        "review_lowering_incomplete",
    )?;
    Ok(Lowered {
        semantic,
        bindings: builder.lowered,
        unresolved: builder.unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::semantic::{LiteralValue, TermRef};
    use std::collections::BTreeMap;

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
            ("cây cầu", concept(4)),
            ("dài", concept(5)),
            (
                "mét",
                LookupOption {
                    ccid: [6; 16],
                    unit: Some(UnitTransform {
                        dimension: [1, 0, 0, 0, 0, 0, 0],
                        scale: (1, 1),
                        offset: (0, 1),
                    }),
                },
            ),
        ])
    }
    fn lower(source: &str, claim: Value) -> Result<Lowered> {
        let registry = registry();
        let mut lookup = |label: &str, role: Role| -> Result<Vec<LookupOption>> {
            Ok(registry
                .get(label.to_lowercase().as_str())
                .filter(|o| (role == Role::Unit) == o.unit.is_some())
                .cloned()
                .into_iter()
                .collect())
        };
        lower_claim(source, [9; 32], [8; 32], &claim, &mut lookup, &mut budget())
    }
    fn claim(
        subject: &str,
        predicate: &str,
        args: &[&str],
        numbers: Value,
        evidence: &str,
    ) -> Value {
        json!({"subject":subject,"predicate":predicate,"arguments":args,"frequency":[],"negation":[],"condition":[],"time":[],
            "location":[],"modality":[],"approximation":[],"numbers":numbers,"relations":[],"evidence":evidence})
    }

    #[test]
    fn english_concept_predicate_concept_lowers_with_spans() {
        let source = "Copper conducts electricity.";
        let out = lower(
            source,
            claim(
                "Copper",
                "conducts",
                &["electricity"],
                json!([]),
                "Copper conducts electricity",
            ),
        )
        .unwrap();
        assert!(out.unresolved.is_empty());
        let sem = out.semantic.unwrap();
        assert_eq!(sem.statements.len(), 1);
        let s = &sem.statements[0];
        assert_eq!(s.operator_or_predicate.as_bytes(), &[2; 16]);
        assert!(
            matches!(&s.arguments[..], [TermRef::Concept(a), TermRef::Concept(b)] if a.as_bytes()==&[1;16] && b.as_bytes()==&[3;16])
        );
        assert_eq!(s.qualifiers.source_spans[0].end, 27);
        assert_eq!(out.bindings.len(), 3);
    }

    #[test]
    fn vietnamese_quantity_lowers_with_exact_unit() {
        let source = "Cây cầu dài 120 mét.";
        let out = lower(
            source,
            claim(
                "Cây cầu",
                "dài",
                &[],
                json!([{"value_quote":"120","unit_quote":"mét","counted_entity_quote":""}]),
                "Cây cầu dài 120 mét",
            ),
        )
        .unwrap();
        let sem = out.semantic.unwrap();
        let TermRef::Literal(LiteralValue::Quantity(q)) = &sem.statements[0].arguments[1] else {
            panic!("quantity argument expected");
        };
        assert_eq!(q.value.numerator(), 120);
        assert_eq!(q.source_unit.unit.as_bytes(), &[6; 16]);
        assert_eq!(out.bindings.len(), 3);
    }

    #[test]
    fn unresolved_terms_and_unsupported_shapes_never_produce_sem() {
        let source = "Copper conducts heat.";
        let out = lower(
            source,
            claim(
                "Copper",
                "conducts",
                &["heat"],
                json!([]),
                "Copper conducts heat",
            ),
        )
        .unwrap();
        assert!(out.semantic.is_none());
        assert_eq!(out.unresolved.len(), 1);
        assert_eq!(out.unresolved[0].role, Role::Object);
        // A unit concept without a host transform stays unresolved.
        let out = lower(
            "Cây cầu dài 120 dặm.",
            claim(
                "Cây cầu",
                "dài",
                &[],
                json!([{"value_quote":"120","unit_quote":"dặm","counted_entity_quote":""}]),
                "Cây cầu dài 120 dặm",
            ),
        )
        .unwrap();
        assert!(out.semantic.is_none());
        assert_eq!(out.unresolved[0].role, Role::Unit);
        let mut negated = claim(
            "Copper",
            "conducts",
            &["heat"],
            json!([]),
            "Copper conducts heat",
        );
        negated["negation"] = json!(["not"]);
        assert_eq!(
            lower(source, negated).err(),
            Some(ExtractionError("review_claim_unsupported"))
        );
        let two = claim(
            "Copper",
            "conducts",
            &["heat", "Copper"],
            json!([]),
            "Copper conducts heat",
        );
        assert_eq!(
            lower(source, two).err(),
            Some(ExtractionError("review_claim_unsupported"))
        );
        let absent = claim(
            "Copper",
            "conducts",
            &["heat"],
            json!([]),
            "Iron conducts heat",
        );
        assert_eq!(
            lower(source, absent).err(),
            Some(ExtractionError("review_claim_evidence"))
        );
    }
}
