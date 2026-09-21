//! Shared sparse meaning selections -> complete private drafts. No model I/O.
use super::*;
use review_draft::{quote_positions, Validation};
use serde_json::{json, Value};

pub const PROFILE: &str = "ku-semantic-selection/1.0";
pub const SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v1/selection.schema.json");
pub const REPAIR_SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v1/repair.schema.json");
pub const PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v1/selection.vi.txt");
pub const REPAIR_PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v1/repair.vi.txt");
const ARRAYS: &[&str] = &[
    "arguments",
    "frequency",
    "negation",
    "condition",
    "time",
    "location",
    "modality",
    "approximation",
];
const PREPOSITIONS: &[&str] = &["ở", "tại", "với", "bằng", "at", "with", "by"];

pub fn commitment() -> Result<String> {
    hash(&json!([
        PROFILE,
        SCHEMA,
        REPAIR_SCHEMA,
        PROMPT,
        REPAIR_PROMPT,
        include_str!("semantic_selection.rs"),
        review_draft::commitment()?
    ]))
}

/// Only fields owned by this host-bound repair are available to the decoder.
/// Keep the nested property order of the pinned schema (including quantities).
pub fn repair_wire_schema(fields: &Value, choices: &Value) -> Result<String> {
    use serde_json::value::RawValue;
    use std::collections::BTreeMap;
    let fields = fields
        .as_array()
        .ok_or(ExtractionError("selection_repair_scope"))?;
    require(
        !fields.is_empty() && fields.len() <= 4,
        "selection_repair_scope",
    )?;
    let root: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(REPAIR_SCHEMA).map_err(|_| ExtractionError("schema"))?;
    let props: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(root["properties"].get()).map_err(|_| ExtractionError("schema"))?;
    let patch: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(props["patch"].get()).map_err(|_| ExtractionError("schema"))?;
    let allowed: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(patch["properties"].get()).map_err(|_| ExtractionError("schema"))?;
    let mut emitted = Vec::new();
    for field in fields {
        let name = field
            .as_str()
            .ok_or(ExtractionError("selection_repair_scope"))?;
        let schema = allowed
            .get(name)
            .ok_or(ExtractionError("selection_repair_scope"))?;
        let mut value: Value =
            serde_json::from_str(schema.get()).map_err(|_| ExtractionError("schema"))?;
        let narrowed = if name == "arguments" && choices["arguments"].is_array() {
            if choices["arguments"].as_array().unwrap().is_empty() {
                value["maxItems"] = json!(0);
            } else {
                value["items"]["enum"] = choices["arguments"].clone();
            }
            value.to_string()
        } else if name == "links" && choices["to"].is_array() {
            if !choices["to"].as_array().unwrap().is_empty() {
                value["items"]["properties"]["to"]["enum"] = choices["to"].clone();
            }
            // Preserve the original to/kind/via generation order.
            format!(
                r#"{{"type":"array","maxItems":{},"items":{{"type":"object","properties":{{"to":{},"kind":{},"via":{}}},"required":{},"additionalProperties":false}}}}"#,
                if choices["to"].as_array().unwrap().is_empty() {
                    json!(0)
                } else {
                    value["maxItems"].clone()
                },
                value["items"]["properties"]["to"],
                value["items"]["properties"]["kind"],
                value["items"]["properties"]["via"],
                value["items"]["required"]
            )
        } else {
            schema.get().to_owned()
        };
        emitted.push(format!("{}:{}", json!(name), narrowed));
    }
    Ok(format!(
        r#"{{"type":"object","properties":{{"patch":{{"type":"object","properties":{{{}}},"required":[],"additionalProperties":false}},"unresolved":{}}},"required":["patch"],"additionalProperties":false}}"#,
        emitted.join(","),
        props["unresolved"].get()
    ))
}
fn numbers() -> Result<regex::Regex> {
    // Consume unsupported lexical forms as a whole so validation cannot silently
    // reinterpret 1e3, 1,000 or .5 as multiple smaller supported numbers.
    regex::Regex::new(r"[+-]?(?:[0-9]+|\.[0-9]+)(?:[.,/][0-9]+)*(?:[eE][+-]?[0-9]+)?")
        .map_err(|_| ExtractionError("number_pattern"))
}
fn items<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v[key].as_array().into_iter().flatten()
}
fn quotes<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    items(v, key).filter_map(Value::as_str)
}
fn positions(source: &str, q: &str, budget: &mut WorkBudget) -> Result<Vec<(usize, usize)>> {
    budget.charge(source.len())?;
    Ok(quote_positions(source, q))
}

fn link_target(
    source: &str,
    claims: &[Value],
    to: &str,
    budget: &mut WorkBudget,
) -> Result<Option<usize>> {
    let found = positions(source, to, budget)?;
    let mut targets = Vec::new();
    if found.len() == 1 {
        for (target, claim) in claims.iter().enumerate() {
            let subject = claim["subject"].as_str().unwrap();
            let predicate = claim["predicate"].as_str().unwrap();
            let ev = positions(source, claim["evidence"].as_str().unwrap(), budget)?;
            if !subject.is_empty()
                && to.contains(subject)
                && to.contains(predicate)
                && ev.len() == 1
                && ((found[0].0 <= ev[0].0 && ev[0].1 <= found[0].1)
                    || (ev[0].0 <= found[0].0 && found[0].1 <= ev[0].1))
            {
                targets.push(target);
            }
        }
    }
    Ok(if targets.len() == 1 {
        Some(targets[0])
    } else {
        None
    })
}

/// Finite mechanical alternatives, never a selected semantic answer.
pub fn repair_choices(
    source: &str,
    selection: &Value,
    target: usize,
    fields: &[String],
    validation: &Validation,
    budget: &mut WorkBudget,
) -> Result<Value> {
    let mut choices = json!({});
    if fields.iter().any(|s| s == "arguments") {
        let mut args: std::collections::BTreeSet<String> =
            quotes(&selection["claims"][target], "arguments")
                .map(str::to_owned)
                .collect();
        for q in &validation.uncovered {
            if !PREPOSITIONS.contains(&q.as_str()) {
                continue;
            }
            for arg in quotes(&selection["claims"][target], "arguments") {
                let expanded = format!("{q} {arg}");
                if positions(source, &expanded, budget)?.len() == 1 {
                    args.insert(expanded);
                }
            }
        }
        choices["arguments"] = json!(args);
    }
    if fields.iter().any(|s| s == "links") {
        let (draft, _) = assemble(source, selection, budget)?;
        let claims = draft["statements"].as_array().unwrap();
        let mut targets = std::collections::BTreeSet::new();
        for (i, claim) in claims.iter().enumerate() {
            let evidence = claim["evidence"].as_str().unwrap();
            if i != target
                && evidence.chars().count() <= 2048
                && link_target(source, claims, evidence, budget)? == Some(i)
            {
                targets.insert(evidence.to_owned());
            }
        }
        choices["to"] = json!(targets);
    }
    Ok(choices)
}

/// Syntax hints only. No unit interpretation, claim segmentation or IDs.
pub fn numeric_candidates(source: &str, budget: &mut WorkBudget) -> Result<Value> {
    require(source.len() <= 8192, "source_bound")?;
    budget.charge(source.len())?;
    let mut out = Vec::new();
    for found in numbers()?.find_iter(source) {
        require(out.len() < 64, "number_bound")?;
        let tail = &source[found.end()..];
        let suffix: String = tail
            .chars()
            .take_while(|c| c.is_alphabetic() || *c == '°')
            .collect();
        out.push(json!({"number":found.as_str(),"adjacent_text":suffix}));
    }
    Ok(json!(out))
}

/// Preserve sparse raw choices separately; never silently repair their meaning.
pub fn assemble(
    source: &str,
    selection: &Value,
    budget: &mut WorkBudget,
) -> Result<(Value, Validation)> {
    require(source.len() <= 8192, "source_bound")?;
    schema::check_embedded(
        selection,
        &serde_json::from_str(SCHEMA).map_err(|_| ExtractionError("schema"))?,
        budget,
    )?;
    let mut claims = Vec::new();
    let mut issues = Vec::new();
    let numeric = numbers()?;
    for (i, choice) in items(selection, "claims").enumerate() {
        let mut claim = json!({"subject":choice["subject"],"predicate":choice["predicate"],"numbers":[],"relations":[]});
        let mut chosen = vec![
            choice["subject"].as_str().unwrap(),
            choice["predicate"].as_str().unwrap(),
        ];
        for key in ARRAYS {
            claim[key] = choice.get(key).cloned().unwrap_or_else(|| json!([]));
            chosen.extend(quotes(choice, key));
        }
        for q in items(choice, "quantities") {
            let text = q["quote"].as_str().unwrap();
            let unit = q["unit"].as_str().unwrap_or("");
            let entity = q["counted_entity"].as_str().unwrap_or("");
            chosen.extend([text, unit, entity]);
            budget.charge(text.len())?;
            let matches: Vec<_> = numeric.find_iter(text).collect();
            if matches.len() != 1 || !text.contains(unit) || !text.contains(entity) {
                issues.push(format!(
                    "selection[{i}].quantities: number_or_annotation_scope"
                ));
                continue;
            }
            if unit.is_empty() == entity.is_empty() {
                issues.push(format!("selection[{i}].quantities: unresolved_kind"));
            }
            claim["numbers"].as_array_mut().unwrap().push(json!({"value_quote":matches[0].as_str(),"unit_quote":unit,"counted_entity_quote":entity}));
        }
        // Ambiguous repeated quotes never choose a nearest occurrence.
        let anchor = choice["anchor"].as_str();
        let mut scope = source;
        let mut scope_start = 0;
        if let Some(a) = anchor {
            let found = positions(source, a, budget)?;
            if found.len() == 1 {
                scope = a;
                scope_start = found[0].0;
            } else {
                issues.push(format!("selection[{i}].anchor: absent_or_ambiguous"));
            }
        }
        let (mut start, mut end) = (scope.len(), 0);
        let mut unambiguous = true;
        for q in chosen.into_iter().filter(|s| !s.is_empty()) {
            let found = positions(scope, q, budget)?;
            if found.len() != 1 {
                unambiguous = false;
                issues.push(format!(
                    "selection[{i}].anchor: role_quote_absent_or_ambiguous"
                ));
            } else {
                start = start.min(found[0].0);
                end = end.max(found[0].1);
            }
        }
        let evidence = if unambiguous && start < end {
            &source[scope_start + start..scope_start + end]
        } else {
            scope
        };
        claim["evidence"] = json!(evidence);
        // Numeric syntax in an argument still needs a declared semantic kind.
        for arg in quotes(choice, "arguments") {
            budget.charge(arg.len())?;
            for number in numeric.find_iter(arg) {
                if !items(&claim, "numbers").any(|n| n["value_quote"] == number.as_str()) {
                    issues.push(format!("selection[{i}].quantities: missing_choice"));
                }
            }
        }
        claims.push(claim);
    }
    for (i, choice) in items(selection, "claims").enumerate() {
        for link in items(choice, "links") {
            let to = link["to"].as_str().unwrap();
            let target = link_target(source, &claims, to, budget)?;
            if target.is_none() || target == Some(i) {
                issues.push(format!(
                    "selection[{i}].links: absent_ambiguous_or_self_target"
                ));
            } else {
                claims[i]["relations"].as_array_mut().unwrap().push(
                    json!({"statement":target.unwrap(),"kind":link["kind"],"quote":link["via"]}),
                );
            }
        }
    }
    let draft = json!({"statements":claims,"unresolved":selection.get("unresolved").cloned().unwrap_or_else(||json!([]))});
    let mut validation = review_draft::validate(source, &draft, budget)?;
    validation.issues.extend(issues);
    validation.issues.sort();
    validation.issues.dedup();
    validation.issues.truncate(16);
    Ok((draft, validation))
}

/// Return only a mechanically identifiable repair target. Unknown omissions stay visible.
pub fn repair_target(
    source: &str,
    selection: &Value,
    validation: &Validation,
    budget: &mut WorkBudget,
) -> Result<Option<(usize, Vec<String>)>> {
    for (i, claim) in items(selection, "claims").enumerate() {
        for field in ["anchor", "quantities", "links"] {
            if validation
                .issues
                .iter()
                .any(|s| s.starts_with(&format!("selection[{i}].{field}:")))
            {
                return Ok(Some((i, vec![field.into()])));
            }
        }
        if validation
            .issues
            .iter()
            .any(|s| s == &format!("statements[{i}].predicate: duplicated_qualifier"))
        {
            return Ok(Some((i, vec!["predicate".into()])));
        }
        for q in &validation.uncovered {
            if !PREPOSITIONS.contains(&q.as_str()) {
                continue;
            }
            for arg in quotes(claim, "arguments") {
                let expanded = format!("{q} {arg}");
                if positions(source, &expanded, budget)?.len() == 1 {
                    // Only dispatch if no other claim has the same target argument.
                    if items(selection, "claims")
                        .filter(|c| quotes(c, "arguments").any(|a| a == arg))
                        .count()
                        == 1
                    {
                        return Ok(Some((i, vec!["arguments".into()])));
                    }
                }
            }
        }
    }
    // Lexical cues propose a field to the LLM; they never assign its value.
    // Only uncovered exact cues inside one uniquely anchored claim are eligible.
    for quote in &validation.uncovered {
        let field = match quote.as_str() {
            "thường" | "thường xuyên" | "đôi khi" | "luôn" | "hiếm khi" | "usually" | "often"
            | "sometimes" | "always" | "rarely" => "frequency",
            "không" | "chưa" | "chẳng" | "not" | "never" => "negation",
            _ => continue,
        };
        let found = positions(source, quote, budget)?;
        if found.len() != 1 {
            continue;
        }
        let (draft, _) = assemble(source, selection, budget)?;
        let mut targets = Vec::new();
        for (i, claim) in items(&draft, "statements").enumerate() {
            let ev = positions(source, claim["evidence"].as_str().unwrap(), budget)?;
            if ev.len() == 1 && ev[0].0 <= found[0].0 && found[0].1 <= ev[0].1 {
                targets.push(i);
            }
        }
        if targets.len() == 1 {
            return Ok(Some((targets[0], vec![field.into()])));
        }
    }
    Ok(None)
}

/// The node-bound task selects the target; model output cannot redirect it.
pub fn apply_repair(
    source: &str,
    base: &Value,
    target: usize,
    fields: &[String],
    reply: &Value,
    budget: &mut WorkBudget,
) -> Result<(Value, Value, Validation)> {
    schema::check_embedded(
        reply,
        &serde_json::from_str(REPAIR_SCHEMA).map_err(|_| ExtractionError("schema"))?,
        budget,
    )?;
    let patch = reply["patch"].as_object().unwrap();
    require(!patch.is_empty(), "selection_repair_no_change")?;
    require(
        patch.keys().all(|key| fields.contains(key)),
        "selection_repair_scope",
    )?;
    require(
        reply
            .get("unresolved")
            .map_or(true, |v| v.as_array().unwrap().is_empty()),
        "selection_repair_unresolved",
    )?;
    let mut changed = base.clone();
    let claim = changed["claims"]
        .as_array_mut()
        .and_then(|v| v.get_mut(target))
        .ok_or(ExtractionError("selection_repair_target"))?;
    for (key, value) in patch {
        claim[key] = value.clone();
    }
    require(changed != *base, "selection_repair_no_change")?;
    // A numeric annotation cannot stand in for a removed relation argument.
    // This repair operation may extend an argument quote, never silently erase it.
    if patch.contains_key("arguments") {
        for old in quotes(&base["claims"][target], "arguments") {
            require(
                quotes(&changed["claims"][target], "arguments").any(|new| new.contains(old)),
                "selection_repair_argument_loss",
            )?;
        }
    }
    let (before, old_validation) = assemble(source, base, budget)?;
    let choices = repair_choices(source, base, target, fields, &old_validation, budget)?;
    for arg in quotes(&reply["patch"], "arguments") {
        require(
            choices["arguments"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(arg))),
            "selection_repair_choice",
        )?;
    }
    for link in items(&reply["patch"], "links") {
        require(
            choices["to"]
                .as_array()
                .is_some_and(|a| a.contains(&link["to"])),
            "selection_repair_choice",
        )?;
    }
    let (draft, validation) = assemble(source, &changed, budget)?;
    require(
        draft != before
            || validation.issues != old_validation.issues
            || validation.uncovered != old_validation.uncovered,
        "selection_repair_no_change",
    )?;
    require(
        validation
            .issues
            .iter()
            .all(|s| old_validation.issues.contains(s)),
        "selection_repair_revalidation",
    )?;
    // Compare actual source coverage positions, not just whether a quote got shorter.
    let missing =
        |quotes: &[String], budget: &mut WorkBudget| -> Result<std::collections::BTreeSet<usize>> {
            let mut out = std::collections::BTreeSet::new();
            for q in quotes {
                for (a, b) in positions(source, q, budget)? {
                    out.extend(a..b);
                }
            }
            Ok(out)
        };
    require(
        missing(&validation.uncovered, budget)?
            .is_subset(&missing(&old_validation.uncovered, budget)?),
        "selection_repair_coverage",
    )?;
    for (old, new) in items(&before, "statements").zip(items(&draft, "statements")) {
        let mut remaining: Vec<_> = items(new, "numbers")
            .map(|n| n["value_quote"].clone())
            .collect();
        for n in items(old, "numbers") {
            let at = remaining
                .iter()
                .position(|v| *v == n["value_quote"])
                .ok_or(ExtractionError("selection_repair_quantity_loss"))?;
            remaining.remove(at);
        }
    }
    Ok((changed, draft, validation))
}

#[cfg(test)]
mod tests;
