//! Private structured meaning. No canonical lowering or factual verification.
use super::*;
use review_draft::{quote_positions, Validation};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const PROFILE: &str = "ku-semantic-selection/2.0";
pub const DRAFT_PROFILE: &str = "ku-semantic-draft/2.0";
pub const DRAFT_SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v2/draft.schema.json");
pub const SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v2/selection.schema.json");
pub const REPAIR_SCHEMA: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v2/repair.schema.json");
pub const PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v2/selection.vi.txt");
pub const REPAIR_PROMPT: &str =
    include_str!("../../../../docs/specs/vnext/ku-semantic-selection-v2/repair.vi.txt");
const STRUCTURES: &[&str] = &["alternatives", "ellipses", "references", "comparisons"];
const ROLES: &[&str] = &[
    "arguments",
    "frequency",
    "negation",
    "condition",
    "time",
    "location",
    "modality",
    "approximation",
];

pub fn commitment() -> Result<String> {
    hash(&json!([
        PROFILE,
        DRAFT_PROFILE,
        DRAFT_SCHEMA,
        SCHEMA,
        REPAIR_SCHEMA,
        PROMPT,
        REPAIR_PROMPT,
        include_str!("semantic_selection_v2.rs"),
        include_str!("schema.rs"),
        include_str!("compiler.rs"),
        semantic_selection::commitment()?
    ]))
}
fn items<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v[key].as_array().into_iter().flatten()
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
/// Preserve the reviewed property order through dynamic narrowing. Some grammar
/// decoders use this order to guide generation, including optional fields.
fn ordered_schema(raw: &str, changed: &Value) -> Result<String> {
    use serde::Deserialize;
    use serde_json::value::RawValue;
    #[derive(Deserialize)]
    struct Pairs(#[serde(deserialize_with = "pairs")] Vec<(String, Box<RawValue>)>);
    fn pairs<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Vec<(String, Box<RawValue>)>, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Vec<(String, Box<RawValue>)>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("schema object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(pair) = a.next_entry()? {
                    out.push(pair);
                }
                Ok(out)
            }
        }
        d.deserialize_map(Visitor)
    }
    if !changed.is_object() {
        return Ok(changed.to_string());
    }
    let Pairs(pairs) = serde_json::from_str(raw).map_err(|_| ExtractionError("schema"))?;
    let mut fields = Vec::new();
    let mut seen = BTreeSet::new();
    for (key, v) in pairs {
        if let Some(child) = changed.get(&key) {
            fields.push(format!(
                "{}:{}",
                json!(key),
                ordered_schema(v.get(), child)?
            ));
            seen.insert(key);
        }
    }
    for (key, value) in changed.as_object().unwrap() {
        if !seen.contains(key) {
            fields.push(format!("{}:{}", json!(key), value));
        }
    }
    Ok(format!("{{{}}}", fields.join(",")))
}
fn legacy(selection: &Value) -> Value {
    let mut v = selection.clone();
    for c in v["claims"].as_array_mut().unwrap() {
        for key in STRUCTURES {
            c.as_object_mut().unwrap().remove(*key);
        }
    }
    v
}
fn span(
    source: &str,
    selector: &Value,
    default: &str,
    b: &mut WorkBudget,
) -> Result<Option<(usize, usize)>> {
    b.charge(source.len())?;
    let scope = selector["within"].as_str().unwrap_or(default);
    let scopes = quote_positions(source, scope);
    if scopes.len() != 1 {
        return Ok(None);
    }
    let found = quote_positions(scope, text(&selector["quote"]));
    let parents = quote_positions(source, default);
    Ok(
        if found.len() == 1
            && parents.len() == 1
            && contains(
                parents[0],
                (scopes[0].0 + found[0].0, scopes[0].0 + found[0].1),
            )
        {
            Some((scopes[0].0 + found[0].0, scopes[0].0 + found[0].1))
        } else {
            None
        },
    )
}
fn issue(v: &mut Validation, i: usize, field: &str, code: &str) {
    v.issues.push(format!("selection[{i}].{field}: {code}"));
}
fn mark(covered: &mut BTreeSet<usize>, at: (usize, usize)) {
    covered.extend(at.0..at.1);
}
fn contains(outer: (usize, usize), inner: (usize, usize)) -> bool {
    outer.0 <= inner.0 && inner.1 <= outer.1
}

pub fn node_count(selection: &Value) -> usize {
    items(selection, "claims")
        .map(|c| {
            STRUCTURES
                .iter()
                .map(|k| items(c, k).count())
                .sum::<usize>()
                + items(c, "alternatives")
                    .map(|g| items(g, "branches").count())
                    .sum::<usize>()
                + items(c, "arguments").count()
                + 1
        })
        .sum()
}

pub fn numeric_candidates(source: &str, b: &mut WorkBudget) -> Result<Value> {
    let candidates = semantic_selection::numeric_candidates(source, b)?;
    Ok(json!(candidates
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| compiler::exact_number(text(&c["number"])).is_ok())
        .collect::<Vec<_>>()))
}

/// Dynamic decoder restriction is also checked during assembly, at every length.
pub fn wire_schema(source: &str, fields: Option<&Value>) -> Result<String> {
    let mut root: Value = serde_json::from_str(if fields.is_some() {
        REPAIR_SCHEMA
    } else {
        SCHEMA
    })
    .map_err(|_| ExtractionError("schema"))?;
    let props = if fields.is_some() {
        &mut root["properties"]["patch"]["properties"]
    } else {
        &mut root["properties"]["claims"]["items"]["properties"]
    };
    if let Some(fields) = fields {
        let fields = fields
            .as_array()
            .ok_or(ExtractionError("selection_repair_scope"))?;
        require(
            !fields.is_empty() && fields.len() <= 4,
            "selection_repair_scope",
        )?;
        require(
            fields
                .iter()
                .all(|f| f.as_str().is_some_and(|s| props.get(s).is_some())),
            "selection_repair_scope",
        )?;
        props
            .as_object_mut()
            .unwrap()
            .retain(|k, _| fields.contains(&json!(k)));
    }
    let mut b = WorkBudget::new(
        1_000_000,
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(false)),
    )?;
    if props.get("quantities").is_some()
        && numeric_candidates(source, &mut b)?
            .as_array()
            .unwrap()
            .is_empty()
    {
        props["quantities"]["maxItems"] = json!(0);
    }
    ordered_schema(
        if fields.is_some() {
            REPAIR_SCHEMA
        } else {
            SCHEMA
        },
        &root,
    )
}

pub fn repair_wire_schema(source: &str, fields: &Value, choices: &Value) -> Result<String> {
    let mut wire: Value = serde_json::from_str(&wire_schema(source, Some(fields))?)
        .map_err(|_| ExtractionError("schema"))?;
    if wire["properties"]["patch"]["properties"]
        .get("predicate")
        .is_some()
    {
        let options = choices["predicate"]
            .as_array()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        require(!options.is_empty(), "selection_repair_choice")?;
        wire["properties"]["patch"]["properties"]["predicate"]["enum"] = json!(options);
    }
    for (key, choice) in [("arguments", "arguments"), ("links", "to")] {
        if wire["properties"]["patch"]["properties"].get(key).is_some() {
            let options = choices[choice]
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            let target = &mut wire["properties"]["patch"]["properties"][key];
            if options.is_empty() {
                target["maxItems"] = json!(0);
            } else if key == "arguments" {
                target["items"]["enum"] = json!(options);
            } else {
                target["items"]["properties"]["to"]["enum"] = json!(options);
            }
        }
    }
    ordered_schema(REPAIR_SCHEMA, &wire)
}

pub fn assemble(
    source: &str,
    selection: &Value,
    b: &mut WorkBudget,
) -> Result<(Value, Validation)> {
    require(source.len() <= 8192, "source_bound")?;
    schema::check_embedded(
        selection,
        &serde_json::from_str(SCHEMA).map_err(|_| ExtractionError("schema"))?,
        b,
    )?;
    require(node_count(selection) <= 256, "semantic_node_bound")?;
    let (mut draft, mut v) = semantic_selection::assemble(source, &legacy(selection), b)?;
    draft["profile"] = json!(DRAFT_PROFILE);
    draft["semantic_nodes"] = json!(node_count(selection));
    let mut covered = BTreeSet::new();
    // Addressable objects have host-created identities, never model-assigned IDs.
    let mut targets: Vec<(usize, String, String, (usize, usize))> = Vec::new();
    for (i, c) in items(selection, "claims").enumerate() {
        let scope = c["anchor"].as_str().unwrap_or(source);
        // Lexical cues expose missing structure; they never assign a semantic
        // operator or resolve its scope. False positives remain reviewable.
        let cue = regex::Regex::new(r"(?i)(?:^|\s)(?:hoặc|or)(?:\s|$)")
            .map_err(|_| ExtractionError("schema"))?;
        for arg in items(c, "arguments") {
            if cue.is_match(text(arg)) && !items(c, "alternatives").any(|g| g["quote"] == *arg) {
                issue(&mut v, i, "alternatives", "unrepresented_choice_cue");
            }
        }
        let comparison_cue = regex::Regex::new(r"(?i)(?:^|\s)(?:hơn|than)(?:\s|$)")
            .map_err(|_| ExtractionError("schema"))?;
        if comparison_cue.is_match(text(&c["predicate"]))
            && items(c, "comparisons").next().is_none()
        {
            issue(&mut v, i, "comparisons", "unrepresented_comparison_cue");
        }
        for key in &ROLES[1..] {
            for q in items(c, key) {
                if text(&c["predicate"]).contains(text(q))
                    || text(q).contains(text(&c["predicate"]))
                {
                    issue(&mut v, i, key, "predicate_overlap");
                }
            }
        }
        let out = &mut draft["statements"][i];
        out["id"] = json!(format!("claim-{i}"));
        let mut evidence = Vec::new();
        let mut roles = vec![("subject", &c["subject"]), ("predicate", &c["predicate"])];
        for key in ROLES {
            roles.extend(items(c, key).map(|q| (*key, q)));
        }
        for (n, (role, q)) in roles.into_iter().enumerate() {
            if text(q).is_empty() {
                continue;
            }
            if let Some(at) = span(source, &json!({"quote":q}), scope, b)? {
                evidence.push(json!({"id":format!("claim-{i}-role-{n}"),"role":role,"quote":q,"start":at.0,"end":at.1}));
                // A group's surface is not a substitute for its internal structure.
                if !items(c, "alternatives").any(|g| g["quote"] == *q) {
                    mark(&mut covered, at);
                }
                targets.push((i, "term".into(), format!("claim-{i}-role-{n}"), at));
            } else {
                issue(&mut v, i, role, "absent_or_ambiguous");
            }
        }
        out["evidence_spans"] = json!(evidence);
        if let Some(at) = span(source, &json!({"quote":out["evidence"]}), source, b)? {
            targets.push((i, "claim".into(), format!("claim-{i}"), at));
        }
        for key in STRUCTURES {
            out[*key] = c.get(*key).cloned().unwrap_or_else(|| json!([]));
        }
        for (g, group) in items(c, "alternatives").enumerate() {
            let group_span = span(source, &json!({"quote":group["quote"]}), scope, b)?;
            let Some(group_span) = group_span else {
                issue(&mut v, i, "alternatives", "group_ambiguous");
                continue;
            };
            if !items(c, "arguments").any(|q| *q == group["quote"]) {
                issue(&mut v, i, "alternatives", "argument_binding");
            }
            if group["exclusivity"] != "unspecified" {
                issue(&mut v, i, "alternatives", "exclusivity_unassessed");
            }
            if matches!(
                text(&group["cue"]).to_lowercase().as_str(),
                "và" | "and" | "nhưng" | "but"
            ) {
                issue(&mut v, i, "alternatives", "incompatible_or_cue");
            }
            let id = format!("claim-{i}-group-{g}");
            out["alternatives"][g]["id"] = json!(id);
            out["alternatives"][g]["operator"] = json!("or");
            out["alternatives"][g]["origin"] = json!("explicit");
            out["alternatives"][g]["span"] = json!([group_span.0, group_span.1]);
            targets.push((i, "group".into(), id, group_span));
            if let Some(at) = span(
                source,
                &json!({"quote":group["cue"],"within":group["quote"]}),
                scope,
                b,
            )? {
                mark(&mut covered, at);
                out["alternatives"][g]["cue_span"] = json!([at.0, at.1]);
            } else {
                issue(&mut v, i, "alternatives", "cue_ambiguous");
            }
            let mut prior_end = group_span.0;
            for (j, branch) in items(group, "branches").enumerate() {
                let at = span(source, &branch["surface"], text(&group["quote"]), b)?;
                let branch_out = &mut out["alternatives"][g]["branches"][j];
                branch_out["origin"] = json!(if branch.get("borrowed").is_some() {
                    "reconstructed"
                } else {
                    "explicit"
                });
                if let Some(at) = at.filter(|at| contains(group_span, *at) && at.0 >= prior_end) {
                    prior_end = at.1;
                    mark(&mut covered, at);
                    branch_out["span"] = json!([at.0, at.1]);
                    branch_out["id"] = json!(format!("claim-{i}-group-{g}-branch-{j}"));
                    targets.push((
                        i,
                        "term".into(),
                        format!("claim-{i}-group-{g}-branch-{j}"),
                        at,
                    ));
                } else {
                    issue(&mut v, i, "alternatives", "branch_scope_or_order");
                }
                if let Some(borrowed) = branch.get("borrowed") {
                    if let Some(at) = span(source, borrowed, text(&group["quote"]), b)? {
                        branch_out["borrowed_span"] = json!([at.0, at.1]);
                        let mut other_branch = false;
                        for (k, other) in items(group, "branches").enumerate() {
                            if k != j
                                && span(source, &other["surface"], text(&group["quote"]), b)?
                                    .is_some_and(|s| contains(s, at))
                            {
                                other_branch = true;
                            }
                        }
                        if !other_branch {
                            issue(&mut v, i, "alternatives", "borrowed_outside_other_branch");
                        }
                    } else {
                        issue(&mut v, i, "alternatives", "borrowed_ambiguous");
                    }
                }
            }
        }
        for (e, ellipsis) in items(c, "ellipses").enumerate() {
            let at = span(source, &ellipsis["surface"], scope, b)?;
            let borrowed = span(source, &ellipsis["borrowed"], source, b)?;
            let role = text(&ellipsis["surface"]["quote"]);
            if !(c["subject"] == role || items(c, "arguments").any(|q| *q == role)) {
                issue(&mut v, i, "ellipses", "role_binding");
            }
            if let (Some(at), Some(from)) = (at, borrowed) {
                if at == from {
                    issue(&mut v, i, "ellipses", "self_reference");
                }
                out["ellipses"][e]["origin"] = json!("reconstructed");
                out["ellipses"][e]["span"] = json!([at.0, at.1]);
                out["ellipses"][e]["borrowed_span"] = json!([from.0, from.1]);
            } else {
                issue(&mut v, i, "ellipses", "absent_or_ambiguous");
            }
        }
        for (j, cmp) in items(c, "comparisons").enumerate() {
            if cmp["property"] != c["predicate"] {
                issue(&mut v, i, "comparisons", "property_binding");
            }
            if let Some(at) = span(source, &json!({"quote":cmp["cue"]}), scope, b)? {
                mark(&mut covered, at);
                out["comparisons"][j]["cue_span"] = json!([at.0, at.1]);
            } else {
                issue(&mut v, i, "comparisons", "cue_ambiguous");
            }
            let kind = text(&cmp["target_kind"]);
            if kind == "unspecified" {
                if cmp.get("target").is_some() {
                    issue(&mut v, i, "comparisons", "unexpected_target");
                }
                issue(&mut v, i, "comparisons", "target_unspecified");
            } else if let Some(at) = span(
                source,
                &cmp["target"],
                if kind == "explicit" { scope } else { source },
                b,
            )? {
                out["comparisons"][j]["target_span"] = json!([at.0, at.1]);
                if kind == "explicit" {
                    let claim_span = span(source, &json!({"quote":out["evidence"]}), source, b)?;
                    if !claim_span.is_some_and(|s| contains(s, at))
                        || targets
                            .iter()
                            .any(|t| t.0 != i && t.1 == "term" && contains(t.3, at))
                    {
                        issue(&mut v, i, "comparisons", "explicit_target_scope");
                    } else {
                        mark(&mut covered, at);
                    }
                }
            } else {
                issue(&mut v, i, "comparisons", "target_absent_or_ambiguous");
            }
            out["comparisons"][j]["origin"] = json!(if kind == "implicit_candidate" {
                "inferred"
            } else {
                "explicit"
            });
            if kind == "implicit_candidate" {
                issue(&mut v, i, "comparisons", "implicit_target_unassessed");
            }
        }
        for link in items(c, "links") {
            if let Some(at) = span(source, &json!({"quote":link["via"]}), source, b)? {
                mark(&mut covered, at);
            }
        }
    }
    let mut edges = Vec::new();
    for (i, c) in items(selection, "claims").enumerate() {
        let scope = c["anchor"].as_str().unwrap_or(source);
        // Check after all claim roles exist, independent of model claim order.
        for cmp in items(c, "comparisons").filter(|c| c["target_kind"] == "explicit") {
            if let Some(at) = span(source, &cmp["target"], scope, b)? {
                if targets
                    .iter()
                    .any(|t| t.0 != i && t.1 == "term" && contains(t.3, at))
                {
                    issue(&mut v, i, "comparisons", "explicit_target_scope");
                }
            }
        }
        for (j, r) in items(c, "references").enumerate() {
            if let Some(at) = span(source, &r["via"], scope, b)? {
                mark(&mut covered, at);
                draft["statements"][i]["references"][j]["via_span"] = json!([at.0, at.1]);
            } else {
                issue(&mut v, i, "references", "cue_ambiguous");
            }
            let at = span(source, &r["to"], source, b)?;
            let matches: Vec<_> = targets
                .iter()
                .filter(|t| t.1 == text(&r["target_kind"]) && Some(t.3) == at)
                .collect();
            if matches.len() == 1 && matches[0].0 != i {
                draft["statements"][i]["references"][j]["target_id"] = json!(matches[0].2);
                draft["statements"][i]["references"][j]["status"] = json!("proposed");
                draft["statements"][i]["references"][j]["target_span"] =
                    json!([matches[0].3 .0, matches[0].3 .1]);
                edges.push((i, matches[0].0));
            } else {
                issue(&mut v, i, "references", "target_absent_ambiguous_or_self");
            }
        }
    }
    for &(start, next) in &edges {
        let mut queue = vec![next];
        let mut seen = BTreeSet::new();
        while let Some(at) = queue.pop() {
            b.charge(1)?;
            if at == start {
                issue(&mut v, start, "references", "cycle");
                break;
            }
            if seen.insert(at) {
                queue.extend(edges.iter().filter(|e| e.0 == at).map(|e| e.1));
            }
        }
    }
    // Full numeric tokens only: never accept a digit fragment of 1e3 or 1,000.
    let words = regex::Regex::new(r"[\p{L}]+").map_err(|_| ExtractionError("schema"))?;
    // Suspicion only, not a number parser or semantic assertion. Other languages
    // remain the extractor/reviewer's responsibility; never infer no numbers.
    if words.find_iter(source).any(|m| {
        matches!(
            m.as_str().to_lowercase().as_str(),
            "một"
                | "hai"
                | "ba"
                | "bốn"
                | "tư"
                | "năm"
                | "sáu"
                | "bảy"
                | "tám"
                | "chín"
                | "mười"
                | "trăm"
                | "nghìn"
                | "triệu"
                | "one"
                | "two"
                | "three"
                | "four"
                | "five"
                | "six"
                | "seven"
                | "eight"
                | "nine"
                | "ten"
                | "hundred"
                | "thousand"
        )
    }) {
        v.issues
            .push("numeric: possible_number_word_unassessed".into());
    }
    let numbers = semantic_selection::numeric_candidates(source, b)?;
    for n in numbers.as_array().unwrap() {
        if compiler::exact_number(text(&n["number"])).is_err() {
            v.issues.push("numeric: unsupported_number_form".into());
        }
    }
    for (i, c) in items(selection, "claims").enumerate() {
        for q in items(c, "quantities") {
            let candidates = semantic_selection::numeric_candidates(text(&q["quote"]), b)?;
            if candidates.as_array().unwrap().len() != 1
                || compiler::exact_number(text(&candidates[0]["number"])).is_err()
            {
                issue(&mut v, i, "quantities", "unsupported_number_form");
            }
            let numeric =
                regex::Regex::new(r"[+-]?(?:[0-9]+|\.[0-9]+)(?:[.,/][0-9]+)*(?:[eE][+-]?[0-9]+)?")
                    .map_err(|_| ExtractionError("number_pattern"))?;
            let scope = c["anchor"].as_str().unwrap_or(source);
            if let Some(qspan) = span(source, &json!({"quote":q["quote"]}), scope, b)? {
                if !numeric.find_iter(source).any(|m| {
                    contains(qspan, (m.start(), m.end()))
                        && compiler::exact_number(m.as_str()).is_ok()
                }) {
                    issue(&mut v, i, "quantities", "source_numeric_token");
                }
            }
        }
    }
    if items(selection, "unresolved").next().is_some() {
        v.issues.push("semantic: unresolved".into());
    }
    v.uncovered.clear();
    let mut start = None;
    for (i, ch) in source.char_indices() {
        let missing = !(i..i + ch.len_utf8()).all(|n| covered.contains(&n));
        if missing {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            let q = source[s..i].trim();
            if q.chars().any(char::is_alphanumeric) {
                v.uncovered.push(q.into());
            }
        }
    }
    if let Some(s) = start {
        let q = source[s..].trim();
        if q.chars().any(char::is_alphanumeric) {
            v.uncovered.push(q.into());
        }
    }
    v.issues.sort();
    v.issues.dedup();
    v.issues.truncate(16);
    v.uncovered.truncate(16);
    schema::check_embedded(
        &draft,
        &serde_json::from_str(DRAFT_SCHEMA).map_err(|_| ExtractionError("schema"))?,
        b,
    )?;
    Ok((draft, v))
}

pub fn repair_target(
    source: &str,
    selection: &Value,
    v: &Validation,
    b: &mut WorkBudget,
) -> Result<Option<(usize, Vec<String>)>> {
    // Only mechanically bounded legacy fields are automatically repairable.
    // Structural semantic ambiguity remains visible for review.
    if let Some(target) = semantic_selection::repair_target(source, &legacy(selection), v, b)? {
        return Ok(Some(target));
    }
    for (i, claim) in items(selection, "claims").enumerate() {
        if ROLES[1..].iter().any(|key| {
            v.issues
                .contains(&format!("selection[{i}].{key}: predicate_overlap"))
        }) && predicate_choices(claim).len() > 1
        {
            return Ok(Some((i, vec!["predicate".into()])));
        }
    }
    Ok(None)
}

// Only trim an already selected qualifier at a whitespace boundary. These are
// exact substrings, not suggested new meanings or joined discontiguous tokens.
fn predicate_choices(claim: &Value) -> BTreeSet<String> {
    let predicate = text(&claim["predicate"]);
    let mut choices = BTreeSet::from([predicate.to_owned()]);
    for key in &ROLES[1..] {
        for q in items(claim, key) {
            let qualifier = text(q);
            if qualifier.is_empty() {
                continue;
            }
            for (rest, boundary) in [
                (predicate.strip_prefix(qualifier), true),
                (predicate.strip_suffix(qualifier), false),
            ] {
                if let Some(rest) = rest {
                    let edge = if boundary {
                        rest.chars().next()
                    } else {
                        rest.chars().next_back()
                    };
                    if edge.is_some_and(char::is_whitespace) && !rest.trim().is_empty() {
                        choices.insert(rest.trim().to_owned());
                    }
                }
            }
        }
    }
    choices
}
pub fn repair_choices(
    source: &str,
    selection: &Value,
    target: usize,
    fields: &[String],
    v: &Validation,
    b: &mut WorkBudget,
) -> Result<Value> {
    let mut choices =
        semantic_selection::repair_choices(source, &legacy(selection), target, fields, v, b)?;
    if fields.iter().any(|f| f == "predicate") {
        let claim = selection["claims"]
            .as_array()
            .and_then(|c| c.get(target))
            .ok_or(ExtractionError("selection_repair_target"))?;
        choices["predicate"] = json!(predicate_choices(claim));
    }
    Ok(choices)
}
pub fn apply_repair(
    source: &str,
    base: &Value,
    target: usize,
    fields: &[String],
    reply: &Value,
    b: &mut WorkBudget,
) -> Result<(Value, Value, Validation)> {
    let mut result = base.clone();
    if fields.iter().any(|f| STRUCTURES.contains(&f.as_str())) {
        schema::check_embedded(
            reply,
            &serde_json::from_str(REPAIR_SCHEMA).map_err(|_| ExtractionError("schema"))?,
            b,
        )?;
        let patch = reply["patch"].as_object().unwrap();
        require(
            !patch.is_empty()
                && patch
                    .keys()
                    .all(|k| fields.contains(k) && STRUCTURES.contains(&k.as_str())),
            "selection_repair_scope",
        )?;
        require(
            items(reply, "unresolved").next().is_none(),
            "selection_repair_unresolved",
        )?;
        let old = base["claims"]
            .as_array()
            .and_then(|a| a.get(target))
            .ok_or(ExtractionError("selection_repair_target"))?;
        for (key, value) in patch {
            let previous = old[key]
                .as_array()
                .ok_or(ExtractionError("selection_repair_scope"))?;
            let next = value.as_array().unwrap();
            require(
                previous.len() == next.len(),
                "selection_repair_structure_loss",
            )?;
            for (before, after) in previous.iter().zip(next) {
                let retained: &[&str] = match key.as_str() {
                    "alternatives" => &["quote", "cue", "exclusivity"],
                    "comparisons" => &["property", "cue", "target_kind"],
                    "references" => &["via", "target_kind"],
                    "ellipses" => &["surface"],
                    _ => return Err(ExtractionError("selection_repair_scope")),
                };
                require(
                    retained.iter().all(|k| before[*k] == after[*k]),
                    "selection_repair_structure_loss",
                )?;
                if key == "alternatives" {
                    let a: Vec<_> = items(before, "branches").collect();
                    let z: Vec<_> = items(after, "branches").collect();
                    require(
                        a.len() == z.len()
                            && a.iter().zip(z).all(|(x, y)| {
                                x["surface"]["quote"] == y["surface"]["quote"]
                                    && x.get("borrowed").is_some() == y.get("borrowed").is_some()
                            }),
                        "selection_repair_structure_loss",
                    )?;
                }
            }
            result["claims"][target][key] = value.clone();
        }
    } else {
        // Legacy bounded choices cannot rewrite structured meaning.
        if let Some(predicate) = reply["patch"].get("predicate") {
            let claim = base["claims"]
                .as_array()
                .and_then(|c| c.get(target))
                .ok_or(ExtractionError("selection_repair_target"))?;
            require(
                predicate
                    .as_str()
                    .is_some_and(|p| predicate_choices(claim).contains(p)),
                "selection_repair_choice",
            )?;
        }
        let (changed, _, _) =
            semantic_selection::apply_repair(source, &legacy(base), target, fields, reply, b)?;
        for key in fields {
            if let Some(value) = changed["claims"][target].get(key) {
                result["claims"][target][key] = value.clone();
            }
        }
    }
    require(result != *base, "selection_repair_no_change")?;
    let (before_draft, before) = assemble(source, base, b)?;
    let (draft, after) = assemble(source, &result, b)?;
    require(
        draft != before_draft
            || after.issues != before.issues
            || after.uncovered != before.uncovered,
        "selection_repair_no_change",
    )?;
    require(
        after.issues.iter().all(|s| before.issues.contains(s)),
        "selection_repair_revalidation",
    )?;
    let positions = |v: &Validation| -> BTreeSet<usize> {
        v.uncovered
            .iter()
            .flat_map(|q| quote_positions(source, q))
            .flat_map(|(s, e)| s..e)
            .collect()
    };
    require(
        positions(&after).is_subset(&positions(&before)),
        "selection_repair_coverage",
    )?;
    Ok((result, draft, after))
}

#[cfg(test)]
mod tests;
