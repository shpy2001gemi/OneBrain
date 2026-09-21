//! Private quoted drafts and checked field edits. No Registry or KU authority.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const PROFILE: &str = "ku-review-draft/1.0";
const DRAFT_SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-review-draft-v1/draft.schema.json");
const REVIEW_SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-review-draft-v1/review.schema.json");
const DRAFT_PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-review-draft-v1/draft.vi.txt");
const REVIEW_PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-review-draft-v1/review.vi.txt");
const ROLES: &[&str] = &[
    "frequency",
    "negation",
    "condition",
    "time",
    "location",
    "modality",
    "approximation",
];
const MAX_CALLS: u32 = 32;
const NUMBER_SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-review-draft-v1/numbers.schema.json");
const NUMBER_PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-review-draft-v1/numbers.vi.txt");
const MAX_TIME_MS: u64 = 1_800_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Draft,
    Numbers,
    Supplement,
    Review,
    Select,
    RepairSelection,
}
pub struct TaskRequest {
    pub kind: TaskKind,
    pub data: Value,
    pub deadline: Duration,
}
impl TaskRequest {
    fn selection_v2(&self) -> bool {
        self.data["SELECTION_PROFILE"] == semantic_selection_v2::PROFILE
    }
    pub fn schema(&self) -> &'static str {
        if self.selection_v2() {
            return if self.kind == TaskKind::RepairSelection { semantic_selection_v2::REPAIR_SCHEMA } else { semantic_selection_v2::SCHEMA };
        }
        match self.kind {
            TaskKind::Draft | TaskKind::Supplement => DRAFT_SCHEMA,
            TaskKind::Numbers => NUMBER_SCHEMA,
            TaskKind::Review => REVIEW_SCHEMA,
            TaskKind::Select => semantic_selection::SCHEMA,
            TaskKind::RepairSelection => semantic_selection::REPAIR_SCHEMA,
        }
    }
    pub fn system_prompt(&self) -> &'static str {
        if self.selection_v2() {
            return if self.kind == TaskKind::RepairSelection { semantic_selection_v2::REPAIR_PROMPT } else { semantic_selection_v2::PROMPT };
        }
        match self.kind {
            TaskKind::Draft | TaskKind::Supplement => DRAFT_PROMPT,
            TaskKind::Numbers => NUMBER_PROMPT,
            TaskKind::Review => REVIEW_PROMPT,
            TaskKind::Select => semantic_selection::PROMPT,
            TaskKind::RepairSelection => semantic_selection::REPAIR_PROMPT,
        }
    }
    pub fn prompt(&self) -> Result<String> {
        let system = self.system_prompt();
        let user =
            serde_json::to_string(&self.data).map_err(|_| ExtractionError("invalid_json"))?;
        require(user.len() < 131072, "payload_bytes")?;
        Ok(format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"))
    }
    /// Constrain quotes on short windows without prescribing any semantic role.
    pub fn wire_schema(&self) -> Result<String> {
        if self.selection_v2() {
            if self.kind == TaskKind::RepairSelection {
                return semantic_selection_v2::repair_wire_schema(
                    self.data["SOURCE"].as_str().ok_or(ExtractionError("source_binding"))?,
                    &self.data["FIELDS"], &self.data["CHOICES"]);
            }
            return semantic_selection_v2::wire_schema(
                self.data["SOURCE"].as_str().ok_or(ExtractionError("source_binding"))?,
                if self.kind == TaskKind::RepairSelection { Some(&self.data["FIELDS"]) } else { None },
            );
        }
        let base = if self.kind == TaskKind::RepairSelection {
            semantic_selection::repair_wire_schema(&self.data["FIELDS"], &self.data["CHOICES"])?
        } else {
            self.schema().into()
        };
        let source = self.data["SOURCE"]
            .as_str()
            .ok_or(ExtractionError("source_binding"))?;
        let tokens = regex::Regex::new(r"[\p{L}\p{N}_]+|[^\p{L}\p{N}_\s]")
            .map_err(|_| ExtractionError("schema"))?;
        let tokens: Vec<_> = tokens.find_iter(source).collect();
        if tokens.len() > 48 {
            return Ok(base);
        }
        let mut quotes = BTreeSet::new();
        for (i, a) in tokens.iter().enumerate() {
            for b in &tokens[i..] {
                quotes.insert(source[a.start()..b.end()].to_owned());
            }
        }
        let parts = regex::Regex::new(r"-?[0-9]+(?:\.[0-9]+|/[1-9][0-9]*)?|[\p{L}]+")
            .map_err(|_| ExtractionError("schema"))?;
        quotes.extend(parts.find_iter(source).map(|m| m.as_str().to_owned()));
        if quotes.iter().map(String::len).sum::<usize>() > 16_384 {
            return Ok(base);
        }
        let mut before = BTreeSet::from([String::new()]);
        for claim in self.data["DRAFT"]["statements"]
            .as_array()
            .into_iter()
            .flatten()
        {
            for field in ["subject", "predicate"] {
                if let Some(q) = claim[field].as_str() {
                    before.insert(q.into());
                }
            }
            for field in std::iter::once("arguments").chain(ROLES.iter().copied()) {
                before.extend(strings(&claim[field]).map(str::to_owned));
            }
        }
        // Use a raw ordered schema DOM through the existing JSON text: edits only
        // append enum constraints; reconstruct property order from the source.
        fn visit(
            schema: &mut Value,
            name: &str,
            quotes: &BTreeSet<String>,
            before: &BTreeSet<String>,
            claims: usize,
            numeric: &BTreeSet<String>,
        ) {
            if name == "numbers" && schema["type"] == "array" && numeric.is_empty() {
                schema["maxItems"] = json!(0);
            }
            if name == "statement" && schema["type"] == "integer" && claims > 0 {
                schema["maximum"] = json!(claims - 1);
            }
            let quote_field = matches!(
                name,
                "subject"
                    | "predicate"
                    | "evidence"
                    | "quote"
                    | "after"
                    | "value_quote"
                    | "unit_quote"
                    | "counted_entity_quote"
                    | "arguments"
                    | "missing"
                    | "before"
                    | "to"
                    | "via"
                    | "anchor"
                    | "unit"
                    | "counted_entity"
            ) || ROLES.contains(&name);
            if schema["type"] == "string" && quote_field {
                let mut values = if name == "before" {
                    before.clone()
                } else {
                    quotes.clone()
                };
                if schema["minLength"].as_u64().unwrap_or(0) == 0 {
                    values.insert(String::new());
                }
                let max = schema["maxLength"].as_u64().unwrap_or(8192) as usize;
                let existing = schema.get("enum").and_then(Value::as_array).cloned();
                schema["enum"] = json!(values
                    .into_iter()
                    .filter(|s| s.chars().count() <= max)
                    .filter(|s| existing.as_ref().map_or(true, |a| a.contains(&json!(s))))
                    .collect::<Vec<_>>());
            }
            if name == "value_quote" && !numeric.is_empty() {
                schema["enum"] = json!(numeric);
            }
            if let Some(properties) = schema["properties"].as_object_mut() {
                for (key, value) in properties {
                    visit(value, key, quotes, before, claims, numeric);
                }
            }
            if let Some(items) = schema.get_mut("items") {
                visit(items, name, quotes, before, claims, numeric);
            }
        }
        let original: Value = serde_json::from_str(&base).map_err(|_| ExtractionError("schema"))?;
        let mut changed = original.clone();
        let numeric: BTreeSet<String> = regex::Regex::new(r"-?[0-9]+(?:\.[0-9]+|/[1-9][0-9]*)?")
            .map_err(|_| ExtractionError("schema"))?
            .find_iter(source)
            .map(|m| m.as_str().to_owned())
            .collect();
        visit(
            &mut changed,
            "",
            &quotes,
            &before,
            self.data["DRAFT"]["statements"]
                .as_array()
                .map_or(0, Vec::len),
            &numeric,
        );
        // Ollama grammar property order is preserved by replacing exact property
        // schemas at their locations in an order-preserving serializer below.
        fn ordered(raw: &serde_json::value::RawValue, changed: &Value) -> Result<String> {
            if changed.is_object() {
                #[derive(Deserialize)]
                struct Pairs(
                    #[serde(deserialize_with = "pairs")]
                    Vec<(String, Box<serde_json::value::RawValue>)>,
                );
                fn pairs<'de, D: serde::Deserializer<'de>>(
                    d: D,
                ) -> std::result::Result<Vec<(String, Box<serde_json::value::RawValue>)>, D::Error>
                {
                    struct Visitor;
                    impl<'de> serde::de::Visitor<'de> for Visitor {
                        type Value = Vec<(String, Box<serde_json::value::RawValue>)>;
                        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                            f.write_str("schema object")
                        }
                        fn visit_map<A: serde::de::MapAccess<'de>>(
                            self,
                            mut a: A,
                        ) -> std::result::Result<Self::Value, A::Error> {
                            let mut v = Vec::new();
                            while let Some(pair) = a.next_entry()? {
                                v.push(pair);
                            }
                            Ok(v)
                        }
                    }
                    d.deserialize_map(Visitor)
                }
                let Pairs(pairs) =
                    serde_json::from_str(raw.get()).map_err(|_| ExtractionError("schema"))?;
                let mut fields = Vec::new();
                let mut keys = BTreeSet::new();
                for (k, v) in pairs {
                    keys.insert(k.clone());
                    fields.push(format!("{}:{}", json!(k), ordered(&v, &changed[&k])?));
                }
                for (k, v) in changed.as_object().unwrap() {
                    if !keys.contains(k) {
                        fields.push(format!("{}:{}", json!(k), v));
                    }
                }
                Ok(format!("{{{}}}", fields.join(",")))
            } else {
                Ok(changed.to_string())
            }
        }
        let raw: Box<serde_json::value::RawValue> =
            serde_json::from_str(&base).map_err(|_| ExtractionError("schema"))?;
        ordered(&raw, &changed)
    }
}
pub fn commitment() -> Result<String> {
    hash(&json!([
        PROFILE,
        DRAFT_SCHEMA,
        REVIEW_SCHEMA,
        DRAFT_PROMPT,
        REVIEW_PROMPT,
        NUMBER_SCHEMA,
        NUMBER_PROMPT,
        include_str!("review_draft.rs"),
        MAX_CALLS,
        MAX_TIME_MS
    ]))
}
fn strings(v: &Value) -> impl Iterator<Item = &str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str)
}
pub fn quote_positions(source: &str, quote: &str) -> Vec<(usize, usize)> {
    if quote.is_empty() {
        return vec![];
    }
    source
        .char_indices()
        .filter_map(|(i, _)| {
            source[i..]
                .starts_with(quote)
                .then_some((i, i + quote.len()))
        })
        .collect()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    pub issues: Vec<String>,
    pub uncovered: Vec<String>,
}
impl Validation {
    pub fn clear(&self) -> bool {
        self.issues.is_empty() && self.uncovered.is_empty()
    }
}
pub fn validate(source: &str, draft: &Value, budget: &mut WorkBudget) -> Result<Validation> {
    let schema: Value =
        serde_json::from_str(DRAFT_SCHEMA).map_err(|_| ExtractionError("schema"))?;
    schema::check_embedded(draft, &schema, budget)?;
    require(source.len() <= 8192, "source_bound")?;
    let mut out = Validation::default();
    let mut covered = vec![false; source.len()];
    let claims = draft["statements"]
        .as_array()
        .ok_or(ExtractionError("schema"))?;
    for (i, claim) in claims.iter().enumerate() {
        let evidence = claim["evidence"].as_str().unwrap();
        budget.charge(source.len())?;
        let spans = quote_positions(source, evidence);
        if spans.len() != 1 {
            out.issues
                .push(format!("statements[{i}].evidence: absent_or_ambiguous"));
        }
        let mut quotes = vec![
            claim["subject"].as_str().unwrap(),
            claim["predicate"].as_str().unwrap(),
        ];
        if quotes[0].is_empty() {
            out.issues
                .push(format!("statements[{i}].subject: unresolved"));
        }
        quotes.extend(strings(&claim["arguments"]));
        for role in ROLES {
            for q in strings(&claim[*role]) {
                if matches!(*role, "frequency" | "negation") && quotes[1].contains(q) {
                    out.issues
                        .push(format!("statements[{i}].predicate: duplicated_qualifier"));
                }
                quotes.push(q);
            }
        }
        for (n, number) in claim["numbers"].as_array().unwrap().iter().enumerate() {
            for key in ["value_quote", "unit_quote", "counted_entity_quote"] {
                quotes.push(number[key].as_str().unwrap());
            }
            if compiler::exact_number(number["value_quote"].as_str().unwrap()).is_err() {
                out.issues
                    .push(format!("statements[{i}].numbers[{n}]: unsupported_number"));
            }
            if !number["unit_quote"].as_str().unwrap().is_empty()
                && !number["counted_entity_quote"].as_str().unwrap().is_empty()
            {
                out.issues.push(format!(
                    "statements[{i}].numbers[{n}]: ambiguous_quantity_kind"
                ));
            }
        }
        for q in quotes.into_iter().filter(|q| !q.is_empty()) {
            budget.charge(evidence.len())?;
            let local = quote_positions(evidence, q);
            if local.is_empty() {
                out.issues
                    .push(format!("statements[{i}]: quote_outside_evidence"));
            }
            if spans.len() == 1 {
                // Multiple occurrences remain covered but are never assigned a unique identity.
                for (a, b) in local {
                    covered[spans[0].0 + a..spans[0].0 + b].fill(true);
                }
            }
        }
        for relation in claim["relations"].as_array().unwrap() {
            let target = relation["statement"].as_u64().unwrap() as usize;
            if target >= claims.len() || target == i {
                out.issues
                    .push(format!("statements[{i}].relations: invalid_target"));
            }
            budget.charge(source.len())?;
            let spans = quote_positions(source, relation["quote"].as_str().unwrap());
            if spans.len() != 1 {
                out.issues.push(format!(
                    "statements[{i}].relations: absent_or_ambiguous_quote"
                ));
            } else {
                covered[spans[0].0..spans[0].1].fill(true);
            }
        }
    }
    for unresolved in draft["unresolved"].as_array().unwrap() {
        budget.charge(source.len())?;
        if quote_positions(source, unresolved["quote"].as_str().unwrap()).is_empty() {
            out.issues.push("unresolved: absent_quote".into());
        }
    }
    let mut start = 0;
    for (i, c) in source.char_indices() {
        if covered[i..i + c.len_utf8()].iter().all(|b| *b) {
            let part = source[start..i].trim();
            if part.chars().any(char::is_alphanumeric) {
                out.uncovered.push(part.into());
            }
            start = i + c.len_utf8();
        }
    }
    let part = source[start..].trim();
    if part.chars().any(char::is_alphanumeric) {
        out.uncovered.push(part.into());
    }
    out.issues.truncate(16);
    out.uncovered.truncate(16);
    Ok(out)
}
pub fn apply_review(
    source: &str,
    draft: &Value,
    review: &Value,
    budget: &mut WorkBudget,
) -> Result<Value> {
    let schema = serde_json::from_str(REVIEW_SCHEMA).map_err(|_| ExtractionError("schema"))?;
    schema::check_embedded(review, &schema, budget)?;
    let before_validation = validate(source, draft, budget)?;
    let mut result = draft.clone();
    let mut used = BTreeSet::new();
    for edit in review["edits"].as_array().unwrap() {
        let i = edit["statement"].as_u64().unwrap() as usize;
        let field = edit["field"].as_str().unwrap();
        let before = edit["before"].as_str().unwrap();
        let after = edit["after"].as_str().unwrap();
        let claim = result["statements"]
            .as_array_mut()
            .unwrap()
            .get_mut(i)
            .ok_or(ExtractionError("edit_statement"))?;
        // Empty-to-empty array edits have no effect. They grant no authority
        // to replace a value or to accept an otherwise invalid draft.
        if before.is_empty() && after.is_empty() && !matches!(field, "subject" | "predicate") {
            continue;
        }
        if before == after {
            let present = if matches!(field, "subject" | "predicate") {
                claim[field] == before && !before.is_empty()
            } else {
                strings(&claim[field]).filter(|q| *q == before).count() == 1
            };
            require(present, "edit_precondition")?;
            continue;
        }
        require(used.insert((i, field, before)), "edit_conflict")?;
        budget.charge(source.len())?;
        require(
            after.is_empty() || claim["evidence"].as_str().unwrap().contains(after),
            "edit_quote",
        )?;
        if matches!(field, "subject" | "predicate") {
            require(
                claim[field] == before && !after.is_empty(),
                "edit_precondition",
            )?;
            claim[field] = json!(after);
        } else {
            let values = claim[field]
                .as_array_mut()
                .ok_or(ExtractionError("edit_field"))?;
            if before.is_empty() {
                require(
                    !after.is_empty() && !values.contains(&json!(after)),
                    "edit_precondition",
                )?;
                values.push(json!(after));
            } else {
                let positions: Vec<_> = values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| (v == before).then_some(i))
                    .collect();
                require(positions.len() == 1, "edit_precondition")?;
                if after.is_empty() {
                    values.remove(positions[0]);
                } else {
                    values[positions[0]] = json!(after);
                }
            }
        }
    }
    for quote in strings(&review["missing"]).chain(
        review["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["quote"].as_str().unwrap()),
    ) {
        budget.charge(source.len())?;
        require(!quote_positions(source, quote).is_empty(), "review_quote")?;
    }
    let checked = validate(source, &result, budget)?;
    let positions = |quotes: &[String]| -> BTreeSet<usize> {
        quotes
            .iter()
            .flat_map(|q| quote_positions(source, q))
            .flat_map(|(a, b)| {
                source[a..b]
                    .char_indices()
                    .filter(|(_, c)| c.is_alphanumeric())
                    .map(move |(i, _)| a + i)
            })
            .collect()
    };
    require(
        checked.issues.is_empty()
            && positions(&checked.uncovered).is_subset(&positions(&before_validation.uncovered)),
        "edit_revalidation",
    )?;
    Ok(result)
}

// Only a scheduling check. An apparently complete number remains unverified;
// a second model call is useful when a value or its kind was left unresolved.
fn needs_number_task(source: &str, draft: &Value) -> Result<bool> {
    let numeric = regex::Regex::new(r"-?[0-9]+(?:\.[0-9]+|/[1-9][0-9]*)?")
        .map_err(|_| ExtractionError("schema"))?;
    let mut required: Vec<_> = numeric.find_iter(source).map(|m| m.as_str()).collect();
    if required.is_empty() {
        return Ok(false);
    }
    let mut found = Vec::new();
    for claim in draft["statements"]
        .as_array()
        .ok_or(ExtractionError("schema"))?
    {
        for n in claim["numbers"]
            .as_array()
            .ok_or(ExtractionError("schema"))?
        {
            let value = n["value_quote"].as_str().ok_or(ExtractionError("schema"))?;
            let unit = n["unit_quote"].as_str().ok_or(ExtractionError("schema"))?;
            let entity = n["counted_entity_quote"]
                .as_str()
                .ok_or(ExtractionError("schema"))?;
            if unit.is_empty() == entity.is_empty() {
                return Ok(true);
            }
            found.push(value);
        }
    }
    required.sort_unstable();
    found.sort_unstable();
    Ok(required != found)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub draft: Value,
    pub producer: String,
    pub task: TaskKind,
    pub validation: Validation,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selection_proposals: Vec<Value>,
    #[serde(default)]
    pub numbers_done: bool,
    #[serde(default)]
    pub supplement_done: bool,
    pub start: usize,
    pub end: usize,
    pub revisions: Vec<Revision>,
    pub reviews: Vec<Value>,
    pub state: String,
    pub calls: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftJob {
    pub source_sha256: String,
    pub profile: String,
    pub commitment: String,
    pub state: String,
    pub windows: Vec<Window>,
    pub calls: u32,
    pub reserved_tokens: u64,
    pub charged_ms: u64,
    pub active: Option<ActiveCall>,
    pub issues: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveCall {
    pub window: usize,
    pub kind: TaskKind,
    pub reserved_ms: u64,
    pub binding: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_scope: Option<Value>,
}
impl DraftJob {
    pub fn new(source: &str) -> Result<Self> {
        require(
            !source.trim().is_empty() && source.len() <= 8192,
            "source_bound",
        )?;
        let mut windows = Vec::new();
        let mut start = 0;
        while start < source.len() {
            let limit = (start + 1200).min(source.len());
            let mut end = limit;
            while !source.is_char_boundary(end) {
                end -= 1;
            }
            if end < source.len() {
                // Focus windows do not claim semantic independence; context is sent separately.
                if let Some((i, _)) = source[start..end].char_indices().rev().find(|(i, c)| {
                    *i > 300 && (c.is_whitespace() || matches!(c, '.' | '!' | '?' | ';'))
                }) {
                    end = start + i + source[start + i..].chars().next().unwrap().len_utf8();
                }
            }
            windows.push(Window {
                selection: None,
                selection_proposals: vec![],
                numbers_done: false,
                supplement_done: false,
                start,
                end,
                revisions: vec![],
                reviews: vec![],
                state: "queued".into(),
                calls: 0,
            });
            start = end;
        }
        require(windows.len() <= 16, "window_bound")?;
        Ok(Self {
            source_sha256: hash(&json!(source))?,
            profile: PROFILE.into(),
            commitment: commitment()?,
            state: "queued".into(),
            windows,
            calls: 0,
            reserved_tokens: 0,
            charged_ms: 0,
            active: None,
            issues: vec![],
        })
    }
    pub fn new_selection(source: &str) -> Result<Self> {
        let mut job = Self::new(source)?;
        job.profile = semantic_selection::PROFILE.into();
        job.commitment = semantic_selection::commitment()?;
        Ok(job)
    }
    pub fn new_selection_v2(source: &str) -> Result<Self> {
        let mut job = Self::new(source)?;
        job.profile = semantic_selection_v2::PROFILE.into();
        job.commitment = semantic_selection_v2::commitment()?;
        Ok(job)
    }
    pub fn expected_commitment(&self) -> Result<String> {
        match self.profile.as_str() {
            PROFILE => commitment(),
            semantic_selection::PROFILE => semantic_selection::commitment(),
            semantic_selection_v2::PROFILE => semantic_selection_v2::commitment(),
            _ => Err(ExtractionError("draft_profile")),
        }
    }
    pub fn next(&self, source: &str) -> Result<Option<(usize, TaskRequest)>> {
        require(
            self.source_sha256 == hash(&json!(source))?,
            "source_binding",
        )?;
        require(
            self.commitment == self.expected_commitment()?,
            "draft_profile",
        )?;
        require(self.active.is_none(), "draft_inflight")?;
        if matches!(
            self.state.as_str(),
            "canceled" | "failed" | "needs_review" | "draft_ready" | "draft_extracted"
        ) {
            return Ok(None);
        }
        require(
            self.calls < MAX_CALLS && self.charged_ms < MAX_TIME_MS,
            "draft_budget",
        )?;
        let Some((i, w)) = self
            .windows
            .iter()
            .enumerate()
            .find(|(_, w)| matches!(w.state.as_str(), "queued" | "reviewing" | "repairing"))
        else {
            return Ok(None);
        };
        require(w.calls < 5, "draft_budget")?;
        require(
            w.start < w.end && source.is_char_boundary(w.start) && source.is_char_boundary(w.end),
            "source_binding",
        )?;
        let focus = source
            .get(w.start..w.end)
            .ok_or(ExtractionError("source_binding"))?;
        if self.profile == semantic_selection::PROFILE || self.profile == semantic_selection_v2::PROFILE {
            let mut budget = WorkBudget::new(
                1_000_000,
                Duration::from_secs(30),
                Arc::new(AtomicBool::new(false)),
            )?;
            let mut data = json!({"SOURCE":focus});
            if self.profile == semantic_selection_v2::PROFILE { data["SELECTION_PROFILE"] = json!(self.profile); }
            let kind = if let Some(selection) = &w.selection {
                let validation = &w
                    .revisions
                    .last()
                    .ok_or(ExtractionError("draft_missing"))?
                    .validation;
                let (target, fields) =
                    selection_repair_target(&self.profile, focus, selection, validation, &mut budget)?
                        .ok_or(ExtractionError("selection_repair_target"))?;
                data["TARGET"] = selection["claims"][target].clone();
                data["FIELDS"] = json!(fields);
                data["CHOICES"] = selection_repair_choices(
                    &self.profile,
                    focus,
                    selection,
                    target,
                    &fields,
                    validation,
                    &mut budget,
                )?;
                data["HOST_ISSUES"] = json!(validation.issues);
                data["UNCOVERED_ROLE_TEXT"] = json!(validation.uncovered);
                TaskKind::RepairSelection
            } else {
                data["NUMERIC_CANDIDATES"] =
                    if self.profile == semantic_selection_v2::PROFILE { semantic_selection_v2::numeric_candidates(focus, &mut budget)? } else { semantic_selection::numeric_candidates(focus, &mut budget)? };
                TaskKind::Select
            };
            if self.windows.len() > 1 {
                data["CONTEXT_ONLY"] = json!(source);
                data["SCOPE_RULE"] = json!("Chỉ tách SOURCE. CONTEXT_ONLY không phải nguồn để lấy thêm mệnh đề. Liên hệ vượt SOURCE phải ghi unresolved.");
            }
            return Ok(Some((
                i,
                TaskRequest {
                    kind,
                    data,
                    deadline: Duration::from_millis((MAX_TIME_MS - self.charged_ms).min(600_000)),
                },
            )));
        }
        let kind = if w.revisions.is_empty() {
            TaskKind::Draft
        } else if !w.numbers_done && needs_number_task(focus, &w.revisions.last().unwrap().draft)? {
            TaskKind::Numbers
        } else if !w.supplement_done
            && w.reviews
                .last()
                .is_some_and(|r| r["missing"].as_array().is_some_and(|a| !a.is_empty()))
        {
            TaskKind::Supplement
        } else {
            TaskKind::Review
        };
        let mut data = json!({"SOURCE":focus});
        if self.windows.len() > 1 {
            data["CONTEXT_ONLY"] = json!(source);
            data["SCOPE_RULE"]=json!("Chỉ trích xuất SOURCE. CONTEXT_ONLY giúp hiểu chủ thể/liên hệ; không lấy mệnh đề độc lập ngoài SOURCE. Liên hệ vượt SOURCE chưa biểu diễn được phải ghi unresolved.");
        }
        if let Some(revision) = w.revisions.last() {
            data["DRAFT"] = revision.draft.clone();
            data["HOST_ISSUES"] = json!(revision.validation.issues);
            data["UNCOVERED_ROLE_TEXT"] = json!(revision.validation.uncovered);
        }
        if kind == TaskKind::Supplement {
            data["MISSING"] = w.reviews.last().unwrap()["missing"].clone();
            data["TASK_RULE"]=json!("Chỉ trả các mệnh đề trong MISSING chưa có trong DRAFT. Giữ SOURCE làm ngữ cảnh đầy đủ. Không lặp lại mệnh đề đã có, không thay đổi mệnh đề cũ. Nếu không thể tách an toàn, trả unresolved với nguyên văn cần xem lại.");
        }
        let deadline = Duration::from_millis((MAX_TIME_MS - self.charged_ms).min(600_000));
        Ok(Some((
            i,
            TaskRequest {
                kind,
                data,
                deadline,
            },
        )))
    }
    /// Persist this reservation before making the call. Crash charges its full time.
    pub fn reserve(
        &mut self,
        window: usize,
        request: &TaskRequest,
        input_tokens: u32,
    ) -> Result<()> {
        require(
            self.active.is_none() && input_tokens <= 6144,
            "draft_admission",
        )?;
        require(
            self.calls < MAX_CALLS && window < self.windows.len(),
            "draft_budget",
        )?;
        let reserved_ms = request.deadline.as_millis() as u64;
        require(
            reserved_ms > 0
                && reserved_ms <= 600_000
                && self.charged_ms.saturating_add(reserved_ms) <= MAX_TIME_MS
                && self.windows[window].calls < 5,
            "draft_budget",
        )?;
        let binding = hash(&request.data)?;
        self.calls += 1;
        self.windows[window].calls += 1;
        self.reserved_tokens += input_tokens as u64 + 2048;
        self.charged_ms += reserved_ms;
        self.state = match request.kind {
            TaskKind::Draft | TaskKind::Select => "extracting",
            TaskKind::Numbers | TaskKind::Supplement | TaskKind::RepairSelection => "repairing",
            TaskKind::Review => "reviewing",
        }
        .into();
        self.active = Some(ActiveCall {
            window,
            kind: request.kind,
            reserved_ms,
            binding,
            repair_scope: if request.kind == TaskKind::RepairSelection {
                Some(request.data.clone())
            } else {
                None
            },
        });
        Ok(())
    }
    pub fn finish(
        &mut self,
        source: &str,
        raw: Result<Vec<u8>>,
        elapsed_ms: u64,
        producer: &str,
        budget: &mut WorkBudget,
    ) -> Result<()> {
        require(
            self.source_sha256 == hash(&json!(source))?,
            "source_binding",
        )?;
        let active = self
            .active
            .take()
            .ok_or(ExtractionError("draft_inflight"))?;
        self.charged_ms = self
            .charged_ms
            .saturating_sub(active.reserved_ms)
            .saturating_add(elapsed_ms.min(active.reserved_ms));
        let other_claims: usize = self
            .windows
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != active.window)
            .filter_map(|(_, window)| window.revisions.last())
            .map(|revision| revision.draft["statements"].as_array().map_or(0, Vec::len))
            .sum();
        let other_nodes: usize = self.windows.iter().enumerate()
            .filter(|(i,_)| *i != active.window)
            .filter_map(|(_,w)| w.revisions.last())
            .map(|r| r.draft["semantic_nodes"].as_u64().unwrap_or(0) as usize).sum();
        let w = &mut self.windows[active.window];
        let focus = source
            .get(w.start..w.end)
            .ok_or(ExtractionError("source_binding"))?;
        let process = (|| -> Result<()> {
            let raw = raw?;
            let output = parse(&raw, budget)?;
            match active.kind {
                TaskKind::Select | TaskKind::RepairSelection => {
                    require(self.profile == semantic_selection::PROFILE || self.profile == semantic_selection_v2::PROFILE, "draft_profile")?;
                    w.selection_proposals.push(output.clone());
                    let (selection, draft, validation) = if active.kind == TaskKind::Select {
                        require(w.selection.is_none(), "selection_already_recorded")?;
                        let (draft, validation) =
                            selection_assemble(&self.profile, focus, &output, budget)?;
                        (output, draft, validation)
                    } else {
                        let scope = active
                            .repair_scope
                            .as_ref()
                            .ok_or(ExtractionError("selection_repair_scope"))?;
                        require(hash(scope)? == active.binding, "selection_repair_binding")?;
                        let base = w
                            .selection
                            .as_ref()
                            .ok_or(ExtractionError("draft_missing"))?;
                        let old = w.revisions.last().ok_or(ExtractionError("draft_missing"))?;
                        let (target, fields) = selection_repair_target(
                            &self.profile,
                            focus,
                            base,
                            &old.validation,
                            budget,
                        )?
                        .ok_or(ExtractionError("selection_repair_target"))?;
                        require(
                            scope["SOURCE"] == focus
                                && scope["TARGET"] == base["claims"][target]
                                && scope["FIELDS"] == json!(fields),
                            "selection_repair_binding",
                        )?;
                        selection_apply_repair(
                            &self.profile, focus, base, target, &fields, &output, budget,
                        )?
                    };
                    require(
                        other_claims + draft["statements"].as_array().unwrap().len() <= 64,
                        "claim_bound",
                    )?;
                    require(other_nodes + draft["semantic_nodes"].as_u64().unwrap_or(0) as usize <= 256,"semantic_node_bound")?;
                    w.state = if validation.clear()
                        && draft["unresolved"].as_array().unwrap().is_empty()
                    {
                        "draft_extracted"
                    } else if w.calls < 3
                        && selection_repair_target(
                            &self.profile,
                            focus,
                            &selection,
                            &validation,
                            budget,
                        )?
                        .is_some()
                    {
                        "repairing"
                    } else {
                        "needs_review"
                    }
                    .into();
                    w.selection = Some(selection);
                    w.revisions.push(Revision {
                        draft,
                        producer: producer.into(),
                        task: active.kind,
                        validation,
                    });
                }
                TaskKind::Draft => {
                    let validation = validate(focus, &output, budget)?;
                    require(
                        other_claims + output["statements"].as_array().unwrap().len() <= 64,
                        "claim_bound",
                    )?;
                    w.revisions.push(Revision {
                        draft: output,
                        producer: producer.into(),
                        task: active.kind,
                        validation,
                    });
                    w.state = "reviewing".into();
                }
                TaskKind::Numbers => {
                    let schema = serde_json::from_str(NUMBER_SCHEMA)
                        .map_err(|_| ExtractionError("schema"))?;
                    schema::check_embedded(&output, &schema, budget)?;
                    let old = &w
                        .revisions
                        .last()
                        .ok_or(ExtractionError("draft_missing"))?
                        .draft;
                    let mut changed = old.clone();
                    for claim in changed["statements"].as_array_mut().unwrap() {
                        claim["numbers"] = json!([]);
                    }
                    for number in output["numbers"].as_array().unwrap() {
                        let i = number["statement"].as_u64().unwrap() as usize;
                        let claim = changed["statements"]
                            .as_array_mut()
                            .unwrap()
                            .get_mut(i)
                            .ok_or(ExtractionError("number_statement"))?;
                        let mut n = number.clone();
                        n.as_object_mut().unwrap().remove("statement");
                        claim["numbers"].as_array_mut().unwrap().push(n);
                    }
                    let validation = validate(focus, &changed, budget)?;
                    let previous = validate(focus, old, budget)?;
                    require(
                        validation
                            .issues
                            .iter()
                            .all(|issue| previous.issues.contains(issue)),
                        "number_revalidation",
                    )?;
                    for (before, after) in old["statements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .zip(changed["statements"].as_array().unwrap())
                    {
                        for n in before["numbers"].as_array().unwrap() {
                            require(
                                after["numbers"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .any(|v| v["value_quote"] == n["value_quote"]),
                                "number_omission",
                            )?;
                        }
                    }
                    w.revisions.push(Revision {
                        draft: changed,
                        producer: producer.into(),
                        task: active.kind,
                        validation,
                    });
                    w.numbers_done = true;
                    w.state = "reviewing".into();
                }
                TaskKind::Supplement => {
                    validate(focus, &output, budget)?;
                    let mut changed = w
                        .revisions
                        .last()
                        .ok_or(ExtractionError("draft_missing"))?
                        .draft
                        .clone();
                    let old_evidence: Vec<_> = changed["statements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c["evidence"].clone())
                        .collect();
                    let additions = output["statements"].as_array().unwrap();
                    require(
                        other_claims + old_evidence.len() + additions.len() <= 64,
                        "claim_bound",
                    )?;
                    for claim in additions {
                        require(
                            !old_evidence.contains(&claim["evidence"]),
                            "duplicate_claim",
                        )?;
                        let mut claim = claim.clone();
                        for relation in claim["relations"].as_array_mut().unwrap() {
                            let index = relation["statement"].as_u64().unwrap() as usize;
                            require(index < additions.len(), "supplement_reference")?;
                            relation["statement"] = json!(index + old_evidence.len());
                        }
                        changed["statements"].as_array_mut().unwrap().push(claim);
                    }
                    changed["unresolved"]
                        .as_array_mut()
                        .unwrap()
                        .extend(output["unresolved"].as_array().unwrap().iter().cloned());
                    let validation = validate(focus, &changed, budget)?;
                    require(validation.issues.is_empty(), "supplement_revalidation")?;
                    w.revisions.push(Revision {
                        draft: changed,
                        producer: producer.into(),
                        task: active.kind,
                        validation,
                    });
                    w.supplement_done = true;
                    w.state = "reviewing".into();
                }
                TaskKind::Review => {
                    let schema = serde_json::from_str(REVIEW_SCHEMA)
                        .map_err(|_| ExtractionError("schema"))?;
                    schema::check_embedded(&output, &schema, budget)?;
                    // Keep the proposed review even when its edit preconditions
                    // fail; it is evidence of a proposal, never an applied edit.
                    w.reviews.push(output.clone());
                    let old = w.revisions.last().ok_or(ExtractionError("draft_missing"))?;
                    let changed = apply_review(focus, &old.draft, &output, budget)?;
                    let validation = validate(focus, &changed, budget)?;
                    let same = changed == old.draft;
                    if !same {
                        w.revisions.push(Revision {
                            draft: changed,
                            producer: producer.into(),
                            task: active.kind,
                            validation,
                        });
                        w.state = "repairing".into();
                    } else {
                        w.state = if validation.clear()
                            && output["missing"].as_array().unwrap().is_empty()
                            && output["unresolved"].as_array().unwrap().is_empty()
                            && old.draft["unresolved"].as_array().unwrap().is_empty()
                        {
                            "draft_ready"
                        } else {
                            "needs_review"
                        }
                        .into();
                    }
                    if !w.supplement_done && !output["missing"].as_array().unwrap().is_empty() {
                        w.state = "repairing".into();
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = process {
            w.state = "needs_review".into();
            self.issues.push(e.0.into());
        }
        if w.calls >= 5 && matches!(w.state.as_str(), "reviewing" | "repairing") {
            w.state = "needs_review".into();
        }
        self.state = if self.windows.iter().all(|w| w.state == "draft_extracted") {
            "draft_extracted"
        } else if self.windows.iter().all(|w| w.state == "draft_ready") {
            "draft_ready"
        } else if self
            .windows
            .iter()
            .any(|w| matches!(w.state.as_str(), "queued" | "reviewing" | "repairing"))
        {
            "queued"
        } else {
            "needs_review"
        }
        .into();
        if self.calls >= MAX_CALLS || self.charged_ms >= MAX_TIME_MS {
            self.state = "needs_review".into();
            self.issues.push("draft_budget".into());
        }
        self.issues.truncate(16);
        Ok(())
    }
    pub fn interrupt(&mut self) {
        self.active = None;
        if !matches!(
            self.state.as_str(),
            "draft_ready" | "draft_extracted" | "needs_review" | "canceled" | "failed"
        ) {
            self.state = "interrupted".into();
        }
    }
}

fn selection_assemble(profile: &str, source: &str, v: &Value, b: &mut WorkBudget) -> Result<(Value,Validation)> {
    if profile == semantic_selection_v2::PROFILE { semantic_selection_v2::assemble(source,v,b) }
    else { semantic_selection::assemble(source,v,b) }
}
fn selection_repair_target(profile: &str, source: &str, v: &Value, validation: &Validation, b: &mut WorkBudget) -> Result<Option<(usize,Vec<String>)>> {
    if profile == semantic_selection_v2::PROFILE { semantic_selection_v2::repair_target(source,v,validation,b) }
    else { semantic_selection::repair_target(source,v,validation,b) }
}
fn selection_repair_choices(profile: &str, source: &str, v: &Value, target: usize, fields: &[String], validation: &Validation, b: &mut WorkBudget) -> Result<Value> {
    if profile == semantic_selection_v2::PROFILE { semantic_selection_v2::repair_choices(source,v,target,fields,validation,b) }
    else { semantic_selection::repair_choices(source,v,target,fields,validation,b) }
}
fn selection_apply_repair(profile: &str, source: &str, v: &Value, target: usize, fields: &[String], reply: &Value, b: &mut WorkBudget) -> Result<(Value,Value,Validation)> {
    if profile == semantic_selection_v2::PROFILE { semantic_selection_v2::apply_repair(source,v,target,fields,reply,b) }
    else { semantic_selection::apply_repair(source,v,target,fields,reply,b) }
}
#[cfg(test)]
mod tests;
