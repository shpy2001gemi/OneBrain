//! Checked projection to the existing SEM types. No canonical identity is minted
//! here; the KU service owns semantic-content normalization and object preparation.
use super::*;
use ku_core::foundation::semantic::*;
use ku_core::foundation::{NormalizedText, ObjectReference};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::ToPrimitive;
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
pub(crate) fn array(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn offset(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}
fn inside(s: &Value, c: &Value) -> bool {
    offset(&c["start"]) <= offset(&s["start"])
        && offset(&s["start"]) < offset(&s["end"])
        && offset(&s["end"]) <= offset(&c["end"])
}
fn index<'a>(rows: &'a Value, field: &str) -> Result<BTreeMap<&'a str, &'a Value>> {
    let mut result = BTreeMap::new();
    for row in array(rows) {
        require(
            result.insert(text(&row[field]), row).is_none(),
            "duplicate_id",
        )?;
    }
    Ok(result)
}

pub(crate) fn exact_number(value: &str) -> Result<ExactRatio> {
    require(value.len() <= 64, "unsupported_number")?;
    let regex = regex::Regex::new(r"\A-?(?:0|[1-9][0-9]*)(?:\.[0-9]+|/[1-9][0-9]*)?\z")
        .expect("static regex");
    require(regex.is_match(value), "unsupported_number")?;
    let (numerator, denominator) = if let Some((left, right)) = value.split_once('/') {
        (left.to_owned(), right.to_owned())
    } else if let Some((left, right)) = value.split_once('.') {
        (
            format!("{left}{right}"),
            format!("1{}", "0".repeat(right.len())),
        )
    } else {
        (value.to_owned(), "1".into())
    };
    big_ratio(&numerator, &denominator)
}
fn big_ratio(n: &str, d: &str) -> Result<ExactRatio> {
    let n = n
        .parse::<BigInt>()
        .map_err(|_| ExtractionError("unsupported_number"))?;
    let d = d
        .parse::<BigInt>()
        .map_err(|_| ExtractionError("unsupported_number"))?;
    require(d > BigInt::from(0), "unsupported_number")?;
    let gcd = n.gcd(&d);
    let n = (n / &gcd)
        .to_i64()
        .ok_or(ExtractionError("exact_number_overflow"))?;
    let d = (d / gcd)
        .to_u64()
        .ok_or(ExtractionError("exact_number_overflow"))?;
    ExactRatio::new(n, d).map_err(|_| ExtractionError("exact_number_overflow"))
}
fn ratio(v: &Value) -> Result<ExactRatio> {
    let r = big_ratio(text(&v["numerator"]), text(&v["denominator"]))?;
    require(
        r.numerator().to_string() == text(&v["numerator"])
            && r.denominator().to_string() == text(&v["denominator"]),
        "noncanonical_ratio",
    )?;
    Ok(r)
}
fn unit(option: &Value) -> Result<UnitRef> {
    let u = option.get("unit").ok_or(ExtractionError("not_unit"))?;
    let mut exponents = [0; 7];
    for (i, e) in array(&u["dimension"]).iter().enumerate() {
        exponents[i] = e.as_i64().unwrap_or(0) as i8;
    }
    let scale = ratio(&u["scale"])?;
    require(scale.numerator() > 0, "unit_scale")?;
    Ok(UnitRef {
        unit: ConceptCcid::from_bytes(unhex(text(&option["ccid"]))?),
        dimension: DimensionVector::new(exponents),
        scale_to_base: scale,
        offset_to_base: ratio(&u["offset"])?,
    })
}

struct Context<'a> {
    value: &'a Value,
    units: BTreeMap<&'a str, &'a Value>,
    options: BTreeMap<&'a str, &'a Value>,
}
impl<'a> Context<'a> {
    fn new(value: &'a Value, b: &mut WorkBudget) -> Result<Self> {
        check(value, "Context", b)?;
        let raw = text(&value["source_text"]);
        require(raw.len() <= 786432, "source_bytes")?;
        let windows = index(&value["windows"], "key")?;
        require(
            windows.values().any(|w| w["role"] == "focus"),
            "missing_focus",
        )?;
        for w in windows.values() {
            require(offset(&w["start"]) < offset(&w["end"]), "window_bounds")?;
            require(
                raw.get(offset(&w["start"])..offset(&w["end"])).is_some(),
                "window_bounds",
            )?;
        }
        let result = Self {
            value,
            units: index(&value["required_units"], "key")?,
            options: index(&value["options"], "key")?,
        };
        for u in result.units.values() {
            result.span(&u["span"], b)?;
            require(
                windows
                    .values()
                    .any(|w| w["role"] == "focus" && inside(&u["span"], w)),
                "unit_outside_focus",
            )?;
        }
        for o in result.options.values() {
            result.span(&o["mention"], b)?;
            require(o["lookup_label"] == o["mention"]["quote"], "lookup_label")?;
            if o.get("unit").is_some() {
                unit(o)?;
            }
        }
        Ok(result)
    }
    fn span(&self, s: &Value, b: &mut WorkBudget) -> Result<()> {
        b.charge(text(&s["quote"]).len() + 1)?;
        require(offset(&s["start"]) < offset(&s["end"]), "span_bounds")?;
        let quote = text(&self.value["source_text"]).get(offset(&s["start"])..offset(&s["end"]));
        require(quote == Some(text(&s["quote"])), "span_quote")?;
        require(
            array(&self.value["windows"]).iter().any(|w| inside(s, w)),
            "outside_context",
        )
    }
}

/// Host-only compiler input. Construction is kept within the workflow module:
/// arbitrary context/resolution JSON must never be treated as authenticated.
pub(crate) fn compile(
    context: &Value,
    candidate: &Value,
    resolution: &Value,
    b: &mut WorkBudget,
) -> Result<Option<SemanticFrameSet>> {
    let context = Context::new(context, b)?;
    check(candidate, "Candidate", b)?;
    check(resolution, "Resolution", b)?;
    b.charge(
        serde_json::to_vec(context.value)
            .map_err(|_| ExtractionError("invalid_json"))?
            .len(),
    )?;
    let digest = hash(context.value)?;
    for v in [candidate, resolution] {
        require(
            v["attempt_id"] == context.value["attempt_id"] && text(&v["context_sha256"]) == digest,
            "context_binding",
        )?;
    }
    let concepts = index(&candidate["concepts"], "key")?;
    let statements = index(&candidate["statements"], "key")?;
    let coverage = index(&candidate["coverage"], "unit")?;
    let bindings = index(&resolution["bindings"], "concept")?;
    require(coverage.keys().eq(context.units.keys()), "coverage_set")?;
    require(
        bindings.keys().all(|k| concepts.contains_key(k)),
        "extraneous_binding",
    )?;
    let mut resolved = BTreeMap::new();
    let mut ready = true;
    for (key, c) in &concepts {
        context.span(&c["evidence"], b)?;
        require(c["label"] == c["evidence"]["quote"], "concept_label")?;
        if let Some(proposal) = c.get("option_proposal") {
            let p = context
                .options
                .get(text(proposal))
                .ok_or(ExtractionError("unknown_option_proposal"))?;
            require(p["mention"] == c["evidence"], "option_mention")?;
        }
        let Some(binding) = bindings.get(key) else {
            ready = false;
            continue;
        };
        let option = *context
            .options
            .get(text(&binding["option"]))
            .ok_or(ExtractionError("unknown_option"))?;
        require(option["mention"] == c["evidence"], "option_mention")?;
        if binding["selection"] == "exact_label" {
            b.charge(context.options.len())?;
            require(
                context
                    .options
                    .values()
                    .filter(|o| o["mention"] == c["evidence"])
                    .count()
                    == 1
                    && option["lookup_label"] == c["label"],
                "ambiguous_exact_label",
            )?;
        } else if binding["selection"] == "model_proposal" {
            ready = false;
        }
        resolved.insert(*key, option);
    }
    let ids: BTreeMap<_, _> = array(&candidate["statements"])
        .iter()
        .enumerate()
        .map(|(i, s)| (text(&s["key"]), i as u32))
        .collect();
    let mut compiler = Compiler {
        context,
        concepts,
        statements,
        resolved,
        ids,
        used: BTreeSet::new(),
        edges: BTreeMap::new(),
        budget: b,
    };
    let mut frames = Vec::new();
    for s in array(&candidate["statements"]) {
        frames.push(compiler.statement(s)?);
    }
    require(
        compiler
            .used
            .iter()
            .copied()
            .eq(compiler.concepts.keys().copied()),
        "unused_concept",
    )?;
    let mut roots = BTreeSet::new();
    for (key, c) in &coverage {
        let refs = array(&c["statements"]);
        require(
            refs.iter().map(text).collect::<BTreeSet<_>>().len() == refs.len(),
            "duplicate_coverage_statement",
        )?;
        if c["status"] == "represented" {
            require(
                c["reason"] == "none" && !refs.is_empty(),
                "represented_coverage",
            )?;
        } else {
            require(c["reason"] != "none", "unresolved_reason")?;
            ready = false;
        }
        for r in refs {
            let r = text(r);
            let s = compiler
                .statements
                .get(r)
                .ok_or(ExtractionError("unknown_statement"))?;
            require(
                array(&s["evidence"])
                    .iter()
                    .any(|e| inside(e, &compiler.context.units[key]["span"])),
                "coverage_scope",
            )?;
            roots.insert(r);
        }
    }
    fn walk<'a>(
        key: &'a str,
        edges: &BTreeMap<&'a str, BTreeSet<&'a str>>,
        active: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
        b: &mut WorkBudget,
    ) -> Result<()> {
        b.charge(1)?;
        require(!active.contains(key), "cyclic_statement")?;
        if visited.contains(key) {
            return Ok(());
        }
        active.insert(key);
        if let Some(children) = edges.get(key) {
            for c in children {
                walk(c, edges, active, visited, b)?;
            }
        }
        active.remove(key);
        visited.insert(key);
        Ok(())
    }
    let mut visited = BTreeSet::new();
    for root in roots {
        walk(
            root,
            &compiler.edges,
            &mut BTreeSet::new(),
            &mut visited,
            compiler.budget,
        )?;
    }
    require(
        visited
            .iter()
            .copied()
            .eq(compiler.statements.keys().copied()),
        "orphan_statement",
    )?;
    if !ready || frames.is_empty() {
        return Ok(None);
    }
    let sem = SemanticFrameSet { statements: frames };
    sem.canonical_bytes()
        .map_err(|_| ExtractionError("invalid_sem"))?;
    Ok(Some(sem))
}

struct Compiler<'a, 'b> {
    context: Context<'a>,
    concepts: BTreeMap<&'a str, &'a Value>,
    statements: BTreeMap<&'a str, &'a Value>,
    resolved: BTreeMap<&'a str, &'a Value>,
    ids: BTreeMap<&'a str, u32>,
    used: BTreeSet<&'a str>,
    edges: BTreeMap<&'a str, BTreeSet<&'a str>>,
    budget: &'b mut WorkBudget,
}
impl<'a> Compiler<'a, '_> {
    fn concept(&mut self, key: &'a str, evidence: Option<&Value>) -> Result<ConceptCcid> {
        let c = self
            .concepts
            .get(key)
            .ok_or(ExtractionError("unknown_concept"))?;
        self.used.insert(key);
        if let Some(e) = evidence {
            require(*e == c["evidence"], "concept_mention")?;
        }
        // Placeholder is never returned: any unresolved binding makes the entire
        // compilation return None after every other structural check has run.
        Ok(ConceptCcid::from_bytes(match self.resolved.get(key) {
            Some(o) => unhex(text(&o["ccid"]))?,
            None => [0; 16],
        }))
    }
    fn reference(&mut self, key: &'a str, parent: &'a str) -> Result<StatementId> {
        let id = *self
            .ids
            .get(key)
            .ok_or(ExtractionError("unknown_statement"))?;
        self.edges.entry(parent).or_default().insert(key);
        Ok(StatementId(id))
    }
    fn scope(&mut self, e: &Value, parent: &str, code: &'static str) -> Result<()> {
        self.context.span(e, self.budget)?;
        require(
            array(&self.statements[parent]["evidence"])
                .iter()
                .any(|s| inside(e, s)),
            code,
        )
    }
    fn term(&mut self, v: &'a Value, parent: &'a str) -> Result<TermRef> {
        let kind = text(&v["kind"]);
        if kind == "quantity" {
            self.scope(&v["number"], parent, "term_scope")?;
            self.scope(&v["unit_evidence"], parent, "term_scope")?;
            let key = text(&v["unit"]);
            self.concept(key, Some(&v["unit_evidence"]))?;
            let value = exact_number(text(&v["number"]["quote"]))?;
            let u = match self.resolved.get(key) {
                Some(o) => unit(o)?,
                None => UnitRef::coherent(
                    ConceptCcid::from_bytes([0; 16]),
                    DimensionVector::DIMENSIONLESS,
                ),
            };
            let quantity = QuantityLiteral {
                value,
                source_unit: u,
            };
            quantity
                .to_base_value()
                .map_err(|_| ExtractionError("exact_number_overflow"))?;
            return Ok(TermRef::Literal(LiteralValue::Quantity(quantity)));
        }
        self.scope(&v["evidence"], parent, "term_scope")?;
        Ok(match kind {
            "concept" => TermRef::Concept(self.concept(text(&v["concept"]), Some(&v["evidence"]))?),
            "statement" => TermRef::Statement(self.reference(text(&v["statement"]), parent)?),
            "text" => {
                require(v["value"] == v["evidence"]["quote"], "literal_quote")?;
                TermRef::Literal(LiteralValue::Text(
                    NormalizedText::new(text(&v["value"]))
                        .map_err(|_| ExtractionError("non_nfc"))?,
                ))
            }
            "boolean" => {
                let expected = match text(&v["evidence"]["quote"]) {
                    "true" | "đúng" => true,
                    "false" | "sai" => false,
                    _ => return Err(ExtractionError("boolean_lexeme")),
                };
                require(v["value"].as_bool() == Some(expected), "boolean_lexeme")?;
                TermRef::Literal(LiteralValue::Boolean(expected))
            }
            _ => return Err(ExtractionError("unsupported_term")),
        })
    }
    fn statement(&mut self, s: &'a Value) -> Result<StatementFrame> {
        self.budget.charge(1)?;
        let key = text(&s["key"]);
        let mut seen = BTreeSet::new();
        for e in array(&s["evidence"]) {
            require(
                seen.insert((offset(&e["start"]), offset(&e["end"]))),
                "duplicate_source_span",
            )?;
            self.context.span(e, self.budget)?;
        }
        let predicate_key = text(&s["predicate"]);
        let predicate = self.concept(predicate_key, None)?;
        require(
            array(&s["evidence"])
                .iter()
                .any(|e| inside(&self.concepts[predicate_key]["evidence"], e)),
            "predicate_scope",
        )?;
        for name in ["negation", "modality"] {
            let q = &s[name];
            if (name == "negation" && q["value"] == true)
                || (name == "modality" && q["value"] != "asserted")
            {
                require(
                    !array(&q["evidence"]).is_empty(),
                    "missing_qualifier_evidence",
                )?;
            }
            for e in array(&q["evidence"]) {
                self.scope(e, key, "qualifier_scope")?;
            }
        }
        let modality = match text(&s["modality"]["value"]) {
            "asserted" => Modality::Asserted,
            "observed" => Modality::Observed,
            "reported" => Modality::Reported,
            "possible" => Modality::Possible,
            "necessary" => Modality::Necessary,
            "desired" => Modality::Desired,
            _ => return Err(ExtractionError("enum")),
        };
        let mut qualifiers = StatementQualifiers {
            negated: s["negation"]["value"].as_bool().unwrap_or(false),
            modality,
            ..Default::default()
        };
        for e in array(&s["evidence"]) {
            qualifiers.source_spans.push(SourceSpan {
                source: ObjectReference::new(1, unhex(text(&self.context.value["source_ref"]))?),
                start: offset(&e["start"]) as u64,
                end: offset(&e["end"]) as u64,
            });
        }
        if let Some(c) = s.get("condition") {
            self.scope(&c["evidence"], key, "qualifier_scope")?;
            qualifiers.condition = Some(self.reference(text(&c["statement"]), key)?);
        }
        if let Some(v) = s.get("time") {
            qualifiers.time = Some(self.term(v, key)?);
        }
        if let Some(v) = s.get("location") {
            qualifiers.location = Some(self.term(v, key)?);
        }
        if let Some(v) = s.get("perspective") {
            qualifiers.perspective = Some(self.term(v, key)?);
        }
        if let Some(v) = s.get("tolerance") {
            let TermRef::Literal(LiteralValue::Quantity(q)) = self.term(v, key)? else {
                return Err(ExtractionError("not_quantity"));
            };
            qualifiers.tolerance = Some(q);
        }
        let arguments = array(&s["arguments"])
            .iter()
            .map(|v| self.term(v, key))
            .collect::<Result<Vec<_>>>()?;
        Ok(StatementFrame {
            statement_id: StatementId(self.ids[key]),
            operator_or_predicate: predicate,
            arguments,
            constraints: vec![],
            qualifiers,
        })
    }
}

#[cfg(test)]
pub(crate) fn logical(sem: &SemanticFrameSet) -> Value {
    fn r(r: ExactRatio) -> Value {
        json!({"numerator":r.numerator().to_string(),"denominator":r.denominator().to_string()})
    }
    fn q(q: &QuantityLiteral) -> Value {
        json!({"kind":"quantity","value":r(q.value),"unit":{"ccid":q.source_unit.unit.to_hex(),"dimension":q.source_unit.dimension.exponents(),"scale":r(q.source_unit.scale_to_base),"offset":r(q.source_unit.offset_to_base)}})
    }
    fn t(t: &TermRef) -> Value {
        match t {
            TermRef::Concept(c) => json!({"kind":"concept","ccid":c.to_hex()}),
            TermRef::Statement(s) => json!({"kind":"statement","id":s.0}),
            TermRef::Literal(LiteralValue::Text(s)) => json!({"kind":"text","value":s.as_str()}),
            TermRef::Literal(LiteralValue::Boolean(b)) => json!({"kind":"boolean","value":b}),
            TermRef::Literal(LiteralValue::Quantity(v)) => q(v),
            _ => panic!("outside extraction surface"),
        }
    }
    json!({"major":1,"minor":0,"statements":sem.statements.iter().map(|s|{
        let quals=&s.qualifiers;
        let mut v=json!({"negated":quals.negated,"modality":match quals.modality {Modality::Asserted=>"asserted",Modality::Observed=>"observed",Modality::Reported=>"reported",Modality::Possible=>"possible",Modality::Necessary=>"necessary",Modality::Desired=>"desired"},"source_spans":quals.source_spans.iter().map(|s|json!({"source":hex(&s.source.cid),"start":s.start,"end":s.end})).collect::<Vec<_>>()});
        if let Some(c)=quals.condition {v["condition"]=c.0.into();}
        for (key,value) in [("time",&quals.time),("location",&quals.location),("perspective",&quals.perspective)] {if let Some(value)=value {v[key]=t(value);}}
        if let Some(value)=&quals.tolerance {v["tolerance"]=q(value);}
        json!({"id":s.statement_id.0,"predicate":s.operator_or_predicate.to_hex(),"arguments":s.arguments.iter().map(t).collect::<Vec<_>>(),"constraints":[],"qualifiers":v})
    }).collect::<Vec<_>>()})
}
