//! Private structured meaning. No canonical lowering or factual verification.
use super::*;
use review_draft::{quote_positions, Validation};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

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
/// Lexical OR cue: exposes missing structure, never assigns an operator.
fn or_cue() -> Result<regex::Regex> {
    regex::Regex::new(r"(?i)(?:^|\s)(?:hoặc|or)(?:\s|$)").map_err(|_| ExtractionError("schema"))
}
/// Known conjunction/contrast cues are a negative guard inside an OR group only.
fn incompatible_cue(group: &Value) -> bool {
    matches!(
        text(&group["cue"]).to_lowercase().as_str(),
        "và" | "and" | "nhưng" | "but"
    )
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
    if let Some(alt) = wire["properties"]["patch"]["properties"].get_mut("alternatives") {
        // D-060 / D-061: same group count; surface and borrowed text only from host-derived choices.
        let groups = choices["alternatives"]["groups"]
            .as_u64()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        let quotes = choices["alternatives"]["borrowed"]
            .as_array()
            .ok_or(ExtractionError("selection_repair_choice"))?
            .clone();
        // D-064: only listed incompatible-cue groups may be omitted.
        let drops = choices["alternatives"]["drop"]
            .as_array()
            .map_or(0, |d| d.len() as u64);
        alt["minItems"] = json!(groups.saturating_sub(drops));
        alt["maxItems"] = json!(groups);
        // D-073: flagged unbound group quotes rebind only to listed bound arguments.
        if let Some(group_quotes) = choices["alternatives"].get("quote") {
            let group_quotes = group_quotes
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!group_quotes.is_empty(), "selection_repair_choice")?;
            alt["items"]["properties"]["quote"]["enum"] = json!(group_quotes);
        }
        // D-071: flagged unresolved cues narrow only to listed local token runs.
        if let Some(cues) = choices["alternatives"].get("cue") {
            let cues = cues
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!cues.is_empty(), "selection_repair_choice")?;
            alt["items"]["properties"]["cue"]["enum"] = json!(cues);
        }
        let branch = &mut alt["items"]["properties"]["branches"]["items"];
        if let Some(surfaces) = choices["alternatives"].get("surfaces") {
            let surfaces = surfaces
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!surfaces.is_empty(), "selection_repair_choice")?;
            branch["properties"]["surface"]["properties"]["quote"]["enum"] = json!(surfaces);
        }
        if quotes.is_empty() {
            branch["properties"]
                .as_object_mut()
                .ok_or(ExtractionError("schema"))?
                .remove("borrowed");
        } else {
            branch["properties"]["borrowed"]["properties"]["quote"]["enum"] = json!(quotes);
        }
    }
    if let Some(cmp) = wire["properties"]["patch"]["properties"].get_mut("comparisons") {
        // D-065: no comparison added or removed; every value from host choices.
        let c = &choices["comparisons"];
        let count = c["count"]
            .as_u64()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        cmp["minItems"] = json!(count);
        cmp["maxItems"] = json!(count);
        let props = &mut cmp["items"]["properties"];
        for (key, choice) in [("property", "property"), ("target_kind", "target_kind")] {
            let options = c[choice]
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!options.is_empty(), "selection_repair_choice")?;
            props[key]["enum"] = json!(options);
        }
        if let Some(cues) = c.get("cue") {
            let options = cues
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!options.is_empty(), "selection_repair_choice")?;
            props["cue"]["enum"] = json!(options);
        }
        let targets = c["targets"]
            .as_array()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        if targets.is_empty() {
            props
                .as_object_mut()
                .ok_or(ExtractionError("schema"))?
                .remove("target");
        } else {
            props["target"]["properties"]["quote"]["enum"] = json!(targets);
        }
    }
    if let Some(ell) = wire["properties"]["patch"]["properties"].get_mut("ellipses") {
        // D-066: only flagged ellipses may be omitted; values from host choices.
        let c = &choices["ellipses"];
        let count = c["count"]
            .as_u64()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        let drops = c["repairable"].as_array().map_or(0, |r| r.len() as u64);
        ell["minItems"] = json!(count.saturating_sub(drops));
        ell["maxItems"] = json!(count);
        for (key, choice) in [("surface", "surfaces"), ("borrowed", "borrowed")] {
            let options = c[choice]
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!options.is_empty(), "selection_repair_choice")?;
            ell["items"]["properties"][key]["properties"]["quote"]["enum"] = json!(options);
        }
    }
    if let Some(refs) = wire["properties"]["patch"]["properties"].get_mut("references") {
        // D-072: no reference added or removed; every quote/kind from host choices.
        let c = &choices["references"];
        let count = c["count"]
            .as_u64()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        refs["minItems"] = json!(count);
        refs["maxItems"] = json!(count);
        let props = &mut refs["items"]["properties"];
        for (key, choice) in [("via", "via"), ("to", "to")] {
            let options = c[choice]
                .as_array()
                .ok_or(ExtractionError("selection_repair_choice"))?;
            require(!options.is_empty(), "selection_repair_choice")?;
            props[key]["properties"]["quote"]["enum"] = json!(options);
        }
        let kinds = c["target_kind"]
            .as_array()
            .ok_or(ExtractionError("selection_repair_choice"))?;
        require(!kinds.is_empty(), "selection_repair_choice")?;
        props["target_kind"]["enum"] = json!(kinds);
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
    let cue = or_cue()?;
    let comparison_cue = regex::Regex::new(r"(?i)(?:^|\s)(?:hơn|than)(?:\s|$)")
        .map_err(|_| ExtractionError("schema"))?;
    for (i, c) in items(selection, "claims").enumerate() {
        let scope = c["anchor"].as_str().unwrap_or(source);
        // Lexical cues expose missing structure; they never assign a semantic
        // operator or resolve its scope. False positives remain reviewable.
        for arg in items(c, "arguments") {
            if cue.is_match(text(arg)) && !items(c, "alternatives").any(|g| g["quote"] == *arg) {
                issue(&mut v, i, "alternatives", "unrepresented_choice_cue");
            }
        }
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
            if incompatible_cue(group) {
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
    let numeric =
        regex::Regex::new(r"[+-]?(?:[0-9]+|\.[0-9]+)(?:[.,/][0-9]+)*(?:[eE][+-]?[0-9]+)?")
            .map_err(|_| ExtractionError("number_pattern"))?;
    for (i, c) in items(selection, "claims").enumerate() {
        for q in items(c, "quantities") {
            let candidates = semantic_selection::numeric_candidates(text(&q["quote"]), b)?;
            if candidates.as_array().unwrap().len() != 1
                || compiler::exact_number(text(&candidates[0]["number"])).is_err()
            {
                issue(&mut v, i, "quantities", "unsupported_number_form");
            }
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
        if ["borrowed_outside_other_branch", "borrowed_ambiguous"]
            .iter()
            .any(|code| {
                v.issues
                    .contains(&format!("selection[{i}].alternatives: {code}"))
            })
            && !borrowed_choices(source, claim, b)?.is_empty()
        {
            return Ok(Some((i, vec!["alternatives".into()])));
        }
        if v.issues.contains(&format!(
            "selection[{i}].alternatives: branch_scope_or_order"
        )) && !branch_scope_choices(source, claim, b)?.is_empty()
        {
            return Ok(Some((i, vec!["alternatives".into()])));
        }
        if v.issues
            .contains(&format!("selection[{i}].alternatives: incompatible_or_cue"))
            && !incompatible_cue_drops(source, selection, i, b)?.is_empty()
        {
            return Ok(Some((i, vec!["alternatives".into()])));
        }
        if v.issues
            .contains(&format!("selection[{i}].alternatives: cue_ambiguous"))
            && !alternative_cue_choices(source, claim, b)?.is_empty()
        {
            return Ok(Some((i, vec!["alternatives".into()])));
        }
        if v.issues
            .contains(&format!("selection[{i}].alternatives: argument_binding"))
            && !argument_binding_choices(source, selection, i, b)?.is_empty()
        {
            return Ok(Some((i, vec!["alternatives".into()])));
        }
        if v.issues
            .contains(&format!("selection[{i}].references: cue_ambiguous"))
            && !reference_cue_choices(source, claim, b)?.is_empty()
        {
            return Ok(Some((i, vec!["references".into()])));
        }
        if [
            "property_binding",
            "unexpected_target",
            "target_absent_or_ambiguous",
            "explicit_target_scope",
            "cue_ambiguous",
        ]
        .iter()
        .any(|code| {
            v.issues
                .contains(&format!("selection[{i}].comparisons: {code}"))
        }) && !comparison_choices(source, selection, i, b)?.is_empty()
        {
            return Ok(Some((i, vec!["comparisons".into()])));
        }
        if ["role_binding", "self_reference", "absent_or_ambiguous"]
            .iter()
            .any(|code| {
                v.issues
                    .contains(&format!("selection[{i}].ellipses: {code}"))
            })
            && !ellipsis_choices(source, claim, b)?.is_empty()
        {
            return Ok(Some((i, vec!["ellipses".into()])));
        }
    }
    Ok(None)
}

/// D-066: ellipses flagged `role_binding` (surface is neither the claim subject
/// nor one of its arguments). Each may only be omitted, or have its surface
/// rebound to an exact bound subject/argument that is a contiguous
/// whitespace-bounded token extension of the emitted surface and resolves
/// uniquely in the claim scope outside a uniquely resolved borrowed span.
/// Borrowed text is fixed; no role, borrowed text or new ellipsis is invented.
/// D-068: a bound ellipsis flagged `self_reference` (its surface resolves to the
/// very span it claims to borrow) may only be omitted; the bound role stays an
/// explicit quote and no surface or borrowed text is substituted.
/// D-074 (slice 1): a bound ellipsis flagged `absent_or_ambiguous` (`surface` or
/// `borrowed` fails unique resolution while its bound role quote itself resolves
/// uniquely in the claim scope) may likewise only be omitted, keeping the bound
/// subject/argument as an explicit quote.
fn ellipsis_choices(
    source: &str,
    claim: &Value,
    b: &mut WorkBudget,
) -> Result<BTreeMap<usize, BTreeSet<String>>> {
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut roles = vec![&claim["subject"]];
    roles.extend(items(claim, "arguments"));
    let mut out = BTreeMap::new();
    for (e, ellipsis) in items(claim, "ellipses").enumerate() {
        let surface = text(&ellipsis["surface"]["quote"]);
        let borrowed = span(source, &ellipsis["borrowed"], source, b)?;
        if roles.iter().any(|r| text(r) == surface) {
            let at = span(source, &ellipsis["surface"], scope, b)?;
            if (at.is_some() && at == borrowed)
                || ((at.is_none() || borrowed.is_none())
                    && span(source, &json!({ "quote": surface }), scope, b)?.is_some())
            {
                out.insert(e, BTreeSet::new());
            }
            continue;
        }
        let mut options = BTreeSet::new();
        if let Some(from) = borrowed {
            for role in &roles {
                let role = text(role);
                if token_extension(surface, role)
                    && span(source, &json!({ "quote": role }), scope, b)?
                        .is_some_and(|at| at != from)
                {
                    options.insert(role.to_owned());
                }
            }
        }
        out.insert(e, options);
    }
    Ok(out)
}

/// D-065 / D-067 / D-069 / D-070 finite choices for one flagged comparison.
struct ComparisonRepair {
    rebind: bool,
    /// D-070: whitespace-bounded token runs of an unresolved `cue` that resolve
    /// uniquely in the claim scope.
    cues: BTreeSet<String>,
    omit_target: bool,
    /// D-067 / D-069: an unresolved or out-of-scope explicit target may only
    /// become `target_kind: unspecified` with the target omitted.
    downgrade: bool,
}

/// True when the shorter string occurs in the longer one bounded by whitespace
/// or the string edges, so one is a contiguous token extension of the other.
fn token_extension(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.trim().is_empty() || short == long {
        return false;
    }
    long.match_indices(short).any(|(at, _)| {
        let end = at + short.len();
        long[..at]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
            && long[end..].chars().next().is_none_or(char::is_whitespace)
    })
}

/// D-065: comparisons flagged `property_binding` whose property and the claim
/// predicate are contiguous whitespace-bounded token extensions of one another
/// may only be rebound to the exact claim predicate; comparisons flagged
/// `unexpected_target` (a target with `target_kind: unspecified`) may only omit
/// that stray target. D-067: comparisons flagged `target_absent_or_ambiguous`
/// (an explicit or implicit-candidate target that does not resolve uniquely in
/// its scope) may only be downgraded to `target_kind: unspecified` with that
/// target omitted, leaving `target_unspecified` visible. No target is invented,
/// substituted or marked explicit, and unrelated properties stay unrouted so
/// futile repairs never dispatch. D-069: an explicit target flagged
/// `explicit_target_scope` (it resolves, but outside the claim's evidence span
/// or inside a term owned by another claim) gets the same single downgrade.
/// D-070: comparisons flagged `cue_ambiguous` (`cue` does not resolve uniquely
/// in the claim scope) may only narrow `cue` to a contiguous whitespace-bounded
/// token run of the emitted `cue` itself that resolves uniquely in the claim
/// scope; comparisons without a valid run get no cue choice so unrepairable
/// cues never dispatch.
fn comparison_choices(
    source: &str,
    selection: &Value,
    i: usize,
    b: &mut WorkBudget,
) -> Result<BTreeMap<usize, ComparisonRepair>> {
    let mut out = BTreeMap::new();
    let Some(claim) = selection["claims"].as_array().and_then(|c| c.get(i)) else {
        return Ok(out);
    };
    let predicate = text(&claim["predicate"]);
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut draft = None;
    for (j, cmp) in items(claim, "comparisons").enumerate() {
        let rebind = cmp["property"] != claim["predicate"]
            && token_extension(text(&cmp["property"]), predicate);
        let mut cues = BTreeSet::new();
        let cue = text(&cmp["cue"]);
        if span(source, &json!({ "quote": cue }), scope, b)?.is_none() {
            let tokens: Vec<(usize, usize)> = cue
                .split_whitespace()
                .take(16)
                .map(|t| {
                    let start = t.as_ptr() as usize - cue.as_ptr() as usize;
                    (start, start + t.len())
                })
                .collect();
            for s in 0..tokens.len() {
                for e in s..tokens.len().min(s + 8) {
                    let candidate = &cue[tokens[s].0..tokens[e].1];
                    if candidate != cue
                        && candidate.len() <= 128
                        && cues.len() < 64
                        && span(source, &json!({ "quote": candidate }), scope, b)?.is_some()
                    {
                        cues.insert(candidate.to_owned());
                    }
                }
            }
        }
        let kind = text(&cmp["target_kind"]);
        let omit_target = kind == "unspecified" && cmp.get("target").is_some();
        let downgrade = kind != "unspecified"
            && match span(
                source,
                &cmp["target"],
                if kind == "explicit" { scope } else { source },
                b,
            )? {
                None => true,
                Some(at) => {
                    kind == "explicit"
                        && explicit_out_of_scope(source, selection, i, at, &mut draft, b)?
                }
            };
        if rebind || !cues.is_empty() || omit_target || downgrade {
            out.insert(
                j,
                ComparisonRepair {
                    rebind,
                    cues,
                    omit_target,
                    downgrade,
                },
            );
        }
    }
    Ok(out)
}

/// D-069: mirrors the `assemble` check behind `explicit_target_scope` for one
/// resolved explicit target of claim `i`: it lies outside the claim's evidence
/// span, or inside a term (resolved role or branch) owned by another claim. The
/// host-assembled draft is built at most once per call and only when needed.
fn explicit_out_of_scope(
    source: &str,
    selection: &Value,
    i: usize,
    at: (usize, usize),
    draft: &mut Option<Value>,
    b: &mut WorkBudget,
) -> Result<bool> {
    if draft.is_none() {
        *draft = Some(assemble(source, selection, b)?.0);
    }
    let statements: Vec<Value> = draft
        .as_ref()
        .map(|d| items(d, "statements").cloned().collect())
        .unwrap_or_default();
    let Some(own) = statements.get(i) else {
        return Ok(false);
    };
    if !span(source, &json!({ "quote": own["evidence"] }), source, b)?
        .is_some_and(|s| contains(s, at))
    {
        return Ok(true);
    }
    let pair = |s: &Value| Some((s.get(0)?.as_u64()? as usize, s.get(1)?.as_u64()? as usize));
    Ok(statements
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != i)
        .any(|(_, st)| {
            items(st, "evidence_spans")
                .filter_map(|e| Some((e["start"].as_u64()? as usize, e["end"].as_u64()? as usize)))
                .chain(
                    items(st, "alternatives")
                        .flat_map(|g| items(g, "branches"))
                        .filter_map(|br| pair(&br["span"])),
                )
                .any(|t| contains(t, at))
        }))
}

/// D-064: OR groups flagged `incompatible_or_cue` (a known conjunction or
/// contrast cue) whose only finite repair is omitting the whole group. The claim
/// keeps the exact group quote as its bound argument; no conjunction or contrast
/// operator is asserted. Groups whose quote also carries an OR cue, whose quote
/// is not a bound argument, which do not resolve uniquely, or which contain a
/// resolved reference target stay unrouted so futile repairs never dispatch.
fn incompatible_cue_drops(
    source: &str,
    selection: &Value,
    target: usize,
    b: &mut WorkBudget,
) -> Result<BTreeSet<usize>> {
    let mut out = BTreeSet::new();
    let Some(claim) = selection["claims"].as_array().and_then(|c| c.get(target)) else {
        return Ok(out);
    };
    let or = or_cue()?;
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut referenced = Vec::new();
    for c in items(selection, "claims") {
        for r in items(c, "references") {
            if let Some(at) = span(source, &r["to"], source, b)? {
                referenced.push(at);
            }
        }
    }
    for (g, group) in items(claim, "alternatives").enumerate() {
        if !incompatible_cue(group)
            || or.is_match(text(&group["quote"]))
            || !items(claim, "arguments").any(|q| *q == group["quote"])
        {
            continue;
        }
        let Some(group_span) = span(source, &json!({ "quote": group["quote"] }), scope, b)? else {
            continue;
        };
        if referenced.iter().any(|at| contains(group_span, *at)) {
            continue;
        }
        out.insert(g);
    }
    Ok(out)
}

fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// D-071: OR groups flagged `cue_ambiguous` (`cue` does not resolve uniquely
/// inside `group.quote`) may only narrow `cue` to a contiguous
/// whitespace-bounded token run of the emitted `cue` itself that resolves
/// uniquely inside `group.quote` and overlaps no located branch span. Groups
/// whose quote does not resolve, or without a valid run (an absent cue or a
/// repeated single-token cue), get no choice so unrepairable cues never dispatch.
fn alternative_cue_choices(
    source: &str,
    claim: &Value,
    b: &mut WorkBudget,
) -> Result<BTreeMap<usize, BTreeSet<String>>> {
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut out = BTreeMap::new();
    for (g, group) in items(claim, "alternatives").enumerate() {
        let Some(group_span) = span(source, &json!({ "quote": group["quote"] }), scope, b)? else {
            continue;
        };
        let cue = text(&group["cue"]);
        let within = |q: &str| json!({ "quote": q, "within": group["quote"] });
        if span(source, &within(cue), scope, b)?.is_some() {
            continue;
        }
        let quote = text(&group["quote"]);
        let mut located = Vec::new();
        for branch in items(group, "branches") {
            if let Some(at) =
                span(source, &branch["surface"], quote, b)?.filter(|at| contains(group_span, *at))
            {
                located.push(at);
            }
        }
        let tokens: Vec<(usize, usize)> = cue
            .split_whitespace()
            .take(16)
            .map(|t| {
                let start = t.as_ptr() as usize - cue.as_ptr() as usize;
                (start, start + t.len())
            })
            .collect();
        let mut options = BTreeSet::new();
        for s in 0..tokens.len() {
            for e in s..tokens.len().min(s + 8) {
                let candidate = &cue[tokens[s].0..tokens[e].1];
                if candidate != cue
                    && candidate.len() <= 128
                    && options.len() < 64
                    && span(source, &within(candidate), scope, b)?
                        .is_some_and(|c| located.iter().all(|at| !overlaps(c, *at)))
                {
                    options.insert(candidate.to_owned());
                }
            }
        }
        if !options.is_empty() {
            out.insert(g, options);
        }
    }
    Ok(out)
}

/// D-073: OR groups flagged `argument_binding` (`group.quote` equals none of
/// the claim's `arguments`) whose emitted `quote`, `cue`, ordered disjoint
/// `branches` and any `borrowed` reconstructions already resolve cleanly in
/// the claim scope may rebind `group.quote` only to an exact bound `argument`
/// of the same claim that is a contiguous whitespace-bounded token extension
/// of the emitted `group.quote`, is not already owned by another group of the
/// claim, and preserves unique `cue`/branch/`borrowed` resolution with no new
/// issue or coverage loss. Unrepairable groups get no choice so futile repairs
/// never dispatch.
fn argument_binding_choices(
    source: &str,
    selection: &Value,
    target: usize,
    b: &mut WorkBudget,
) -> Result<BTreeMap<usize, BTreeSet<String>>> {
    let mut out = BTreeMap::new();
    let Some(claim) = selection["claims"].as_array().and_then(|c| c.get(target)) else {
        return Ok(out);
    };
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut baseline: Option<Validation> = None;
    let positions = |v: &Validation| -> BTreeSet<usize> {
        v.uncovered
            .iter()
            .flat_map(|q| quote_positions(source, q))
            .flat_map(|(s, e)| s..e)
            .collect()
    };
    for (g, group) in items(claim, "alternatives").enumerate() {
        let emitted = text(&group["quote"]);
        if items(claim, "arguments").any(|q| text(q) == emitted) || incompatible_cue(group) {
            continue;
        }
        let Some(group_span) = span(source, &json!({ "quote": emitted }), scope, b)? else {
            continue;
        };
        let Some(emitted_cue) = span(
            source,
            &json!({ "quote": group["cue"], "within": emitted }),
            scope,
            b,
        )?
        else {
            continue;
        };
        let mut prior_emitted = group_span.0;
        let mut emitted_branches = Vec::new();
        let mut clean_emitted = true;
        for branch in items(group, "branches") {
            let Some(at) = span(source, &branch["surface"], emitted, b)?.filter(|at| {
                contains(group_span, *at) && at.0 >= prior_emitted && !overlaps(*at, emitted_cue)
            }) else {
                clean_emitted = false;
                break;
            };
            prior_emitted = at.1;
            emitted_branches.push(at);
        }
        if !clean_emitted {
            continue;
        }
        for (j, branch) in items(group, "branches").enumerate() {
            if let Some(borrowed) = branch.get("borrowed") {
                let valid = span(source, borrowed, emitted, b)?.is_some_and(|at| {
                    emitted_branches
                        .iter()
                        .enumerate()
                        .any(|(k, s)| k != j && contains(*s, at))
                });
                if !valid {
                    clean_emitted = false;
                    break;
                }
            }
        }
        if !clean_emitted {
            continue;
        }
        let mut options = BTreeSet::new();
        for arg_val in items(claim, "arguments") {
            let arg = text(arg_val);
            if !token_extension(emitted, arg)
                || items(claim, "alternatives")
                    .enumerate()
                    .any(|(k, other)| k != g && text(&other["quote"]) == arg)
            {
                continue;
            }
            let Some(arg_span) = span(source, &json!({ "quote": arg }), scope, b)? else {
                continue;
            };
            let Some(cue_span) = span(
                source,
                &json!({ "quote": group["cue"], "within": arg }),
                scope,
                b,
            )?
            else {
                continue;
            };
            let mut prior_end = arg_span.0;
            let mut branch_spans = Vec::new();
            let mut ok = true;
            for branch in items(group, "branches") {
                let Some(at) = span(source, &branch["surface"], arg, b)?.filter(|at| {
                    contains(arg_span, *at) && at.0 >= prior_end && !overlaps(*at, cue_span)
                }) else {
                    ok = false;
                    break;
                };
                prior_end = at.1;
                branch_spans.push(at);
            }
            if !ok {
                continue;
            }
            for (j, branch) in items(group, "branches").enumerate() {
                if let Some(borrowed) = branch.get("borrowed") {
                    let valid = span(source, borrowed, arg, b)?.is_some_and(|at| {
                        branch_spans
                            .iter()
                            .enumerate()
                            .any(|(k, s)| k != j && contains(*s, at))
                    });
                    if !valid {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let before = match &baseline {
                Some(v) => v,
                None => baseline.insert(assemble(source, selection, b)?.1),
            };
            let mut trial = selection.clone();
            trial["claims"][target]["alternatives"][g]["quote"] = json!(arg);
            let (_, after) = assemble(source, &trial, b)?;
            if after.issues.iter().all(|s| before.issues.contains(s))
                && positions(&after).is_subset(&positions(before))
            {
                options.insert(arg.to_owned());
            }
        }
        if !options.is_empty() {
            out.insert(g, options);
        }
    }
    Ok(out)
}

/// D-072: references flagged `cue_ambiguous` (`via` does not resolve uniquely
/// in the claim scope) may only narrow `via.quote` to a contiguous
/// whitespace-bounded token run of the emitted `via.quote` itself that resolves
/// uniquely in the same scope (any emitted `via.within` kept) and overlaps no
/// resolved `to` span. References without a valid run (an absent cue or a
/// repeated single-token cue) get no choice so unrepairable cues never dispatch.
fn reference_cue_choices(
    source: &str,
    claim: &Value,
    b: &mut WorkBudget,
) -> Result<BTreeMap<usize, BTreeSet<String>>> {
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut out = BTreeMap::new();
    for (j, r) in items(claim, "references").enumerate() {
        if span(source, &r["via"], scope, b)?.is_some() {
            continue;
        }
        let target = span(source, &r["to"], source, b)?;
        let cue = text(&r["via"]["quote"]);
        let tokens: Vec<(usize, usize)> = cue
            .split_whitespace()
            .take(16)
            .map(|t| {
                let start = t.as_ptr() as usize - cue.as_ptr() as usize;
                (start, start + t.len())
            })
            .collect();
        let mut options = BTreeSet::new();
        for s in 0..tokens.len() {
            for e in s..tokens.len().min(s + 8) {
                let candidate = &cue[tokens[s].0..tokens[e].1];
                if candidate == cue || candidate.len() > 128 || options.len() >= 64 {
                    continue;
                }
                let mut via = r["via"].clone();
                via["quote"] = json!(candidate);
                if span(source, &via, scope, b)?
                    .is_some_and(|c| target.is_none_or(|t| !overlaps(c, t)))
                {
                    options.insert(candidate.to_owned());
                }
            }
        }
        if !options.is_empty() {
            out.insert(j, options);
        }
    }
    Ok(out)
}

struct GroupScopeRepair {
    group: usize,
    group_span: (usize, usize),
    cue_span: Option<(usize, usize)>,
    surfaces: BTreeMap<usize, BTreeSet<String>>,
    keepable: BTreeSet<usize>,
}

/// D-061: groups that raised `branch_scope_or_order` because disjoint resolved
/// branches were emitted out of source byte order, or because a branch surface
/// failed resolution inside `group.quote`, overlapped `cue`, or overlapped
/// another branch. Surface-flagged branches get finite whitespace-bounded token
/// runs inside `group.quote` outside `cue_span` (and outside fixed sibling
/// spans); unrepairable groups are omitted so futile repairs never dispatch.
fn branch_scope_choices(
    source: &str,
    claim: &Value,
    b: &mut WorkBudget,
) -> Result<Vec<GroupScopeRepair>> {
    let scope = claim["anchor"].as_str().unwrap_or(source);
    let mut out = Vec::new();
    for (g, group) in items(claim, "alternatives").enumerate() {
        let Some(group_span) = span(source, &json!({ "quote": group["quote"] }), scope, b)? else {
            continue;
        };
        let quote = text(&group["quote"]);
        let cue_span = span(
            source,
            &json!({ "quote": group["cue"], "within": group["quote"] }),
            scope,
            b,
        )?;
        let branches: Vec<_> = items(group, "branches").collect();
        let mut spans = Vec::with_capacity(branches.len());
        let mut prior_end = group_span.0;
        let mut failed = false;
        for branch in &branches {
            let at =
                span(source, &branch["surface"], quote, b)?.filter(|at| contains(group_span, *at));
            if let Some(at) = at.filter(|at| at.0 >= prior_end) {
                prior_end = at.1;
            } else {
                failed = true;
            }
            spans.push(at);
        }
        if !failed {
            continue;
        }
        let mut bad = BTreeSet::new();
        let mut keepable = BTreeSet::new();
        for (j, s) in spans.iter().enumerate() {
            let Some(at) = *s else {
                bad.insert(j);
                continue;
            };
            let hits_cue = cue_span.is_some_and(|cs| overlaps(at, cs));
            let hits_other = spans
                .iter()
                .enumerate()
                .any(|(k, other)| k != j && other.is_some_and(|o| overlaps(at, o)));
            if hits_cue || hits_other {
                bad.insert(j);
                if !hits_cue {
                    keepable.insert(j);
                }
            }
        }
        if bad.is_empty() {
            out.push(GroupScopeRepair {
                group: g,
                group_span,
                cue_span,
                surfaces: BTreeMap::new(),
                keepable,
            });
            continue;
        }
        let Some(cs) = cue_span else {
            continue;
        };
        let fixed: Vec<(usize, usize)> = spans
            .iter()
            .enumerate()
            .filter(|(m, _)| !bad.contains(m))
            .filter_map(|(_, s)| *s)
            .collect();
        let tokens: Vec<(usize, usize)> = quote
            .split_whitespace()
            .take(32)
            .map(|t| {
                let start = t.as_ptr() as usize - quote.as_ptr() as usize;
                (start, start + t.len())
            })
            .collect();
        let mut options = BTreeSet::new();
        for s in 0..tokens.len() {
            for e in s..tokens.len().min(s + 8) {
                let candidate = &quote[tokens[s].0..tokens[e].1];
                if options.len() < 64
                    && span(source, &json!({ "quote": candidate }), quote, b)?.is_some_and(|c| {
                        contains(group_span, c)
                            && !overlaps(c, cs)
                            && fixed.iter().all(|f| !overlaps(c, *f))
                    })
                {
                    options.insert(candidate.to_owned());
                }
            }
        }
        if options.is_empty() {
            continue;
        }
        let surfaces = bad.into_iter().map(|j| (j, options.clone())).collect();
        out.push(GroupScopeRepair {
            group: g,
            group_span,
            cue_span,
            surfaces,
            keepable,
        });
    }
    Ok(out)
}

fn has_bijection(n: usize, edge: impl Fn(usize, usize) -> bool) -> bool {
    fn dfs(
        new_j: usize,
        n: usize,
        edge: &impl Fn(usize, usize) -> bool,
        seen: &mut [bool],
        matched_old: &mut [Option<usize>],
    ) -> bool {
        for old_j in 0..n {
            if !edge(old_j, new_j) || seen[old_j] {
                continue;
            }
            seen[old_j] = true;
            if matched_old[old_j].is_none_or(|prev_new| dfs(prev_new, n, edge, seen, matched_old)) {
                matched_old[old_j] = Some(new_j);
                return true;
            }
        }
        false
    }
    let mut matched_old = vec![None; n];
    for new_j in 0..n {
        let mut seen = vec![false; n];
        if !dfs(new_j, n, &edge, &mut seen, &mut matched_old) {
            return false;
        }
    }
    true
}

/// D-060: branches whose borrowed text is not taken from another branch of the
/// group: it resolves in no other branch, or not uniquely inside the group at
/// all (`borrowed_ambiguous`). Each may drop `borrowed` (explicit surface only) or take one
/// whitespace-bounded token run of another branch surface that resolves to a
/// unique position inside that branch, mirroring the assembly check. The model
/// chooses; the host neither picks a meaning nor accepts anything else.
fn borrowed_choices(
    source: &str,
    claim: &Value,
    b: &mut WorkBudget,
) -> Result<BTreeMap<(usize, usize), BTreeSet<String>>> {
    let mut out = BTreeMap::new();
    for (g, group) in items(claim, "alternatives").enumerate() {
        let quote = text(&group["quote"]);
        let mut spans = Vec::new();
        for branch in items(group, "branches") {
            spans.push(span(source, &branch["surface"], quote, b)?);
        }
        for (j, branch) in items(group, "branches").enumerate() {
            let Some(borrowed) = branch.get("borrowed") else {
                continue;
            };
            // Unresolved inside the group (`borrowed_ambiguous`) is flagged too.
            if let Some(at) = span(source, borrowed, quote, b)? {
                if spans
                    .iter()
                    .enumerate()
                    .any(|(k, s)| k != j && s.is_some_and(|s| contains(s, at)))
                {
                    continue;
                }
            }
            let mut options = BTreeSet::new();
            for (k, other) in items(group, "branches").enumerate() {
                let Some(other_span) = spans[k].filter(|_| k != j) else {
                    continue;
                };
                let surface = text(&other["surface"]["quote"]);
                let tokens: Vec<(usize, usize)> = surface
                    .split_whitespace()
                    .take(16)
                    .map(|t| {
                        let start = t.as_ptr() as usize - surface.as_ptr() as usize;
                        (start, start + t.len())
                    })
                    .collect();
                for s in 0..tokens.len() {
                    for e in s..tokens.len().min(s + 8) {
                        let candidate = &surface[tokens[s].0..tokens[e].1];
                        if options.len() < 64
                            && span(source, &json!({ "quote": candidate }), quote, b)?
                                .is_some_and(|c| contains(other_span, c))
                        {
                            options.insert(candidate.to_owned());
                        }
                    }
                }
            }
            out.insert((g, j), options);
        }
    }
    Ok(out)
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
    if fields.iter().any(|f| f == "alternatives") {
        let claim = selection["claims"]
            .as_array()
            .and_then(|c| c.get(target))
            .ok_or(ExtractionError("selection_repair_target"))?;
        let repairable = borrowed_choices(source, claim, b)?;
        let scope_repairs = branch_scope_choices(source, claim, b)?;
        let scope_by_group: BTreeMap<usize, &GroupScopeRepair> =
            scope_repairs.iter().map(|sr| (sr.group, sr)).collect();
        let mut quotes = BTreeSet::new();
        for (g, group) in items(claim, "alternatives").enumerate() {
            let quote = text(&group["quote"]);
            for (j, branch) in items(group, "branches").enumerate() {
                if !repairable.contains_key(&(g, j)) {
                    if let Some(q) = branch["borrowed"]["quote"].as_str() {
                        quotes.insert(q.to_owned());
                    }
                } else if let Some(sr) = scope_by_group.get(&g).filter(|sr| !sr.surfaces.is_empty())
                {
                    if let Some(borrowed) = branch.get("borrowed") {
                        if span(source, borrowed, quote, b)?.is_some_and(|at| {
                            contains(sr.group_span, at)
                                && sr.cue_span.is_none_or(|cs| !overlaps(at, cs))
                        }) {
                            if let Some(q) = borrowed["quote"].as_str() {
                                quotes.insert(q.to_owned());
                            }
                        }
                    }
                }
            }
        }
        let mut listed = Vec::new();
        for ((g, j), options) in &repairable {
            quotes.extend(options.iter().cloned());
            listed.push(json!({"group":g,"branch":j,"borrowed":options,"may_omit":true}));
        }
        let mut alt_choices = json!({
            "groups": items(claim, "alternatives").count(),
            "repairable": listed,
            "borrowed": quotes,
        });
        if !scope_repairs.is_empty() {
            let mut surfaces = BTreeSet::new();
            for (g, group) in items(claim, "alternatives").enumerate() {
                for (j, branch) in items(group, "branches").enumerate() {
                    let keep_existing = match scope_by_group.get(&g) {
                        None => true,
                        Some(sr) => !sr.surfaces.contains_key(&j) || sr.keepable.contains(&j),
                    };
                    if keep_existing {
                        if let Some(q) = branch["surface"]["quote"].as_str() {
                            surfaces.insert(q.to_owned());
                        }
                    }
                }
            }
            let mut scope_listed = Vec::new();
            for sr in &scope_repairs {
                let mut branch_list = Vec::new();
                for (j, opts) in &sr.surfaces {
                    surfaces.extend(opts.iter().cloned());
                    branch_list.push(json!({
                        "branch": j,
                        "surface": opts,
                        "may_keep": sr.keepable.contains(j),
                    }));
                }
                scope_listed.push(json!({
                    "group": sr.group,
                    "reorder": true,
                    "branches": branch_list,
                }));
            }
            alt_choices["scope_or_order"] = json!(scope_listed);
            alt_choices["surfaces"] = json!(surfaces);
        }
        let drops = incompatible_cue_drops(source, selection, target, b)?;
        if !drops.is_empty() {
            // D-064: whole flagged groups may be omitted; kept surfaces stay as emitted.
            alt_choices["drop"] = json!(drops);
            if alt_choices.get("surfaces").is_none() {
                let surfaces: BTreeSet<String> = items(claim, "alternatives")
                    .flat_map(|g| items(g, "branches"))
                    .filter_map(|br| br["surface"]["quote"].as_str().map(str::to_owned))
                    .collect();
                alt_choices["surfaces"] = json!(surfaces);
            }
        }
        let cue_repairs = alternative_cue_choices(source, claim, b)?;
        if !cue_repairs.is_empty() {
            // D-071: unflagged group cues stay as emitted; flagged unresolved cues
            // list only their local token runs.
            let mut cues: BTreeSet<String> = items(claim, "alternatives")
                .enumerate()
                .filter(|(g, _)| !cue_repairs.contains_key(g))
                .map(|(_, group)| text(&group["cue"]).to_owned())
                .collect();
            let mut cue_listed = Vec::new();
            for (g, options) in &cue_repairs {
                cues.extend(options.iter().cloned());
                cue_listed.push(json!({ "group": g, "cue": options }));
            }
            alt_choices["cue_narrowing"] = json!(cue_listed);
            alt_choices["cue"] = json!(cues);
        }
        let quote_repairs = argument_binding_choices(source, selection, target, b)?;
        if !quote_repairs.is_empty() {
            // D-073: unflagged group quotes stay as emitted; flagged unbound group
            // quotes list only their valid bound-argument rebinds.
            let mut group_quotes: BTreeSet<String> = items(claim, "alternatives")
                .enumerate()
                .filter(|(g, _)| !quote_repairs.contains_key(g))
                .map(|(_, group)| text(&group["quote"]).to_owned())
                .collect();
            let mut quote_listed = Vec::new();
            for (g, options) in &quote_repairs {
                group_quotes.extend(options.iter().cloned());
                quote_listed.push(json!({ "group": g, "quote": options }));
            }
            alt_choices["argument_binding"] = json!(quote_listed);
            alt_choices["quote"] = json!(group_quotes);
        }
        choices["alternatives"] = alt_choices;
    }
    if fields.iter().any(|f| f == "comparisons") {
        let claim = selection["claims"]
            .as_array()
            .and_then(|c| c.get(target))
            .ok_or(ExtractionError("selection_repair_target"))?;
        // D-065: same comparison count; property/kind/target only from existing values
        // plus the exact claim predicate for flagged property bindings.
        // D-067: plus `unspecified` for flagged unresolved targets.
        // D-069: and for flagged out-of-scope explicit targets.
        // D-070: plus narrowed token runs of flagged unresolved cues.
        let repairs = comparison_choices(source, selection, target, b)?;
        let has_cue_repair = repairs.values().any(|r| !r.cues.is_empty());
        let mut properties = BTreeSet::new();
        let mut cues = BTreeSet::new();
        let mut kinds = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for (j, cmp) in items(claim, "comparisons").enumerate() {
            properties.insert(text(&cmp["property"]).to_owned());
            if has_cue_repair && repairs.get(&j).is_none_or(|r| r.cues.is_empty()) {
                cues.insert(text(&cmp["cue"]).to_owned());
            }
            kinds.insert(text(&cmp["target_kind"]).to_owned());
            if let Some(q) = cmp["target"]["quote"].as_str() {
                targets.insert(q.to_owned());
            }
        }
        let mut listed = Vec::new();
        for (j, r) in &repairs {
            let mut entry = json!({ "comparison": j });
            if r.rebind {
                properties.insert(text(&claim["predicate"]).to_owned());
                entry["property"] = claim["predicate"].clone();
            }
            if !r.cues.is_empty() {
                cues.extend(r.cues.iter().cloned());
                entry["cue"] = json!(r.cues);
            }
            if r.downgrade {
                kinds.insert("unspecified".to_owned());
                entry["target_kind"] = json!("unspecified");
            }
            if r.omit_target || (r.downgrade && claim["comparisons"][*j].get("target").is_some()) {
                entry["omit_target"] = json!(true);
            }
            listed.push(entry);
        }
        let mut cmp_choices = json!({
            "count": items(claim, "comparisons").count(),
            "property": properties,
            "target_kind": kinds,
            "targets": targets,
            "repairable": listed,
        });
        if has_cue_repair {
            cmp_choices["cue"] = json!(cues);
        }
        choices["comparisons"] = cmp_choices;
    }
    if fields.iter().any(|f| f == "ellipses") {
        let claim = selection["claims"]
            .as_array()
            .and_then(|c| c.get(target))
            .ok_or(ExtractionError("selection_repair_target"))?;
        // D-066: borrowed text fixed; flagged surfaces only rebind to a bound
        // token-extended subject/argument, or the flagged ellipsis is omitted.
        // D-068: a self-referencing ellipsis lists no surface: omission only.
        let repairs = ellipsis_choices(source, claim, b)?;
        let mut surfaces = BTreeSet::new();
        let mut borrowed = BTreeSet::new();
        for ellipsis in items(claim, "ellipses") {
            if let Some(q) = ellipsis["surface"]["quote"].as_str() {
                surfaces.insert(q.to_owned());
            }
            if let Some(q) = ellipsis["borrowed"]["quote"].as_str() {
                borrowed.insert(q.to_owned());
            }
        }
        let mut listed = Vec::new();
        for (e, options) in &repairs {
            surfaces.extend(options.iter().cloned());
            listed.push(json!({ "ellipsis": e, "surface": options, "may_omit": true }));
        }
        choices["ellipses"] = json!({
            "count": items(claim, "ellipses").count(),
            "surfaces": surfaces,
            "borrowed": borrowed,
            "repairable": listed,
        });
    }
    if fields.iter().any(|f| f == "references") {
        let claim = selection["claims"]
            .as_array()
            .and_then(|c| c.get(target))
            .ok_or(ExtractionError("selection_repair_target"))?;
        // D-072: same reference count; `to`/`target_kind` fixed; unflagged `via`
        // quotes stay as emitted and flagged ones list only local token runs.
        let repairs = reference_cue_choices(source, claim, b)?;
        let mut via = BTreeSet::new();
        let mut to = BTreeSet::new();
        let mut kinds = BTreeSet::new();
        for (j, r) in items(claim, "references").enumerate() {
            if !repairs.contains_key(&j) {
                via.insert(text(&r["via"]["quote"]).to_owned());
            }
            to.insert(text(&r["to"]["quote"]).to_owned());
            kinds.insert(text(&r["target_kind"]).to_owned());
        }
        let mut listed = Vec::new();
        for (j, options) in &repairs {
            via.extend(options.iter().cloned());
            listed.push(json!({ "reference": j, "via": options }));
        }
        choices["references"] = json!({
            "count": items(claim, "references").count(),
            "via": via,
            "to": to,
            "target_kind": kinds,
            "repairable": listed,
        });
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
    // D-067: the only issue a repair may introduce is the honest
    // `target_unspecified` of a downgraded comparison.
    let mut allowed_new = BTreeSet::new();
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
        let repairable = borrowed_choices(source, old, b)?;
        let scope_repairs = branch_scope_choices(source, old, b)?;
        let scope_by_group: BTreeMap<usize, &GroupScopeRepair> =
            scope_repairs.iter().map(|sr| (sr.group, sr)).collect();
        let cue_repairs = alternative_cue_choices(source, old, b)?;
        let quote_repairs = argument_binding_choices(source, base, target, b)?;
        for (key, value) in patch {
            let previous = old[key]
                .as_array()
                .ok_or(ExtractionError("selection_repair_scope"))?;
            let next = value.as_array().unwrap();
            if key == "alternatives" && next.len() < previous.len() {
                // D-064: only whole incompatible-cue groups may be omitted; every
                // kept group stays byte-identical and in its original order.
                let drops = incompatible_cue_drops(source, base, target, b)?;
                let mut kept = next.iter().peekable();
                for (g, before) in previous.iter().enumerate() {
                    if kept.peek() == Some(&before) {
                        kept.next();
                    } else {
                        require(drops.contains(&g), "selection_repair_choice")?;
                    }
                }
                require(kept.next().is_none(), "selection_repair_structure_loss")?;
                result["claims"][target][key] = value.clone();
                continue;
            }
            if key == "ellipses" {
                // D-066: unflagged ellipses stay byte-identical and in order; a
                // flagged one may only be omitted or rebound to a listed surface
                // with its borrowed text unchanged.
                let flagged = ellipsis_choices(source, old, b)?;
                let mut kept = next.iter().peekable();
                for (e, before) in previous.iter().enumerate() {
                    let options = flagged.get(&e);
                    match kept.peek() {
                        Some(after) if **after == *before => {
                            kept.next();
                        }
                        Some(after)
                            if options.is_some_and(|o| {
                                after["borrowed"] == before["borrowed"]
                                    && after["surface"].as_object().is_some_and(|s| s.len() == 1)
                                    && after["surface"]["quote"]
                                        .as_str()
                                        .is_some_and(|q| o.contains(q))
                            }) =>
                        {
                            kept.next();
                        }
                        _ => require(options.is_some(), "selection_repair_choice")?,
                    }
                }
                require(kept.next().is_none(), "selection_repair_structure_loss")?;
                result["claims"][target][key] = value.clone();
                continue;
            }
            require(
                previous.len() == next.len(),
                "selection_repair_structure_loss",
            )?;
            if key == "comparisons" {
                // D-065: cue fixed unless D-070 narrows a flagged unresolved cue
                // to a listed local token run; a flagged property may only become
                // the exact claim predicate and a flagged stray target may only be
                // omitted. D-067: kind fixed unless a flagged unresolved target is
                // downgraded to `unspecified` together with omitting it (D-069:
                // likewise a flagged out-of-scope explicit target).
                let repairs = comparison_choices(source, base, target, b)?;
                for (j, (before, after)) in previous.iter().zip(next).enumerate() {
                    let r = repairs.get(&j);
                    if let Some(cues) = r.map(|r| &r.cues).filter(|c| !c.is_empty()) {
                        require(
                            after["cue"].as_str().is_some_and(|q| cues.contains(q)),
                            "selection_repair_choice",
                        )?;
                    } else {
                        require(
                            before["cue"] == after["cue"],
                            "selection_repair_structure_loss",
                        )?;
                    }
                    let downgraded = before["target_kind"] != after["target_kind"];
                    require(
                        !downgraded
                            || (r.is_some_and(|r| r.downgrade)
                                && after["target_kind"] == "unspecified"
                                && after.get("target").is_none()),
                        "selection_repair_choice",
                    )?;
                    if downgraded {
                        allowed_new.insert(format!(
                            "selection[{target}].comparisons: target_unspecified"
                        ));
                    }
                    require(
                        before["property"] == after["property"]
                            || (r.is_some_and(|r| r.rebind)
                                && after["property"] == old["predicate"]),
                        "selection_repair_choice",
                    )?;
                    require(
                        before.get("target") == after.get("target")
                            || downgraded
                            || (r.is_some_and(|r| r.omit_target) && after.get("target").is_none()),
                        "selection_repair_choice",
                    )?;
                }
                result["claims"][target][key] = value.clone();
                continue;
            }
            if key == "references" {
                // D-072: `to`, `target_kind` and any `via.within` stay fixed; an
                // unflagged `via` stays byte-identical and a flagged unresolved
                // `via.quote` may only narrow to one of its listed token runs.
                let flagged = reference_cue_choices(source, old, b)?;
                for (j, (before, after)) in previous.iter().zip(next).enumerate() {
                    let mut restored = after.clone();
                    restored["via"] = before["via"].clone();
                    require(restored == *before, "selection_repair_structure_loss")?;
                    if let Some(options) = flagged.get(&j) {
                        let mut expected = before["via"].clone();
                        expected["quote"] = after["via"]["quote"].clone();
                        require(
                            after["via"] == expected
                                && after["via"]["quote"]
                                    .as_str()
                                    .is_some_and(|q| options.contains(q)),
                            "selection_repair_choice",
                        )?;
                    } else {
                        require(
                            before["via"] == after["via"],
                            "selection_repair_structure_loss",
                        )?;
                    }
                }
                result["claims"][target][key] = value.clone();
                continue;
            }
            for (g, (before, after)) in previous.iter().zip(next).enumerate() {
                if key == "alternatives" {
                    // D-073: quote fixed unless a flagged unbound group quote
                    // rebinds to one of its listed bound arguments.
                    if let Some(quotes) = quote_repairs.get(&g) {
                        require(
                            after["quote"].as_str().is_some_and(|q| quotes.contains(q)),
                            "selection_repair_choice",
                        )?;
                    } else {
                        require(
                            before["quote"] == after["quote"],
                            "selection_repair_structure_loss",
                        )?;
                    }
                    // D-071: cue fixed unless a flagged unresolved cue narrows to
                    // one of its listed local token runs.
                    if let Some(cues) = cue_repairs.get(&g) {
                        require(
                            after["cue"].as_str().is_some_and(|q| cues.contains(q)),
                            "selection_repair_choice",
                        )?;
                    } else {
                        require(
                            before["cue"] == after["cue"],
                            "selection_repair_structure_loss",
                        )?;
                    }
                }
                let retained: &[&str] = match key.as_str() {
                    "alternatives" => &["exclusivity"],
                    "comparisons" => &["property", "cue", "target_kind"],
                    "references" => &["via", "target_kind"],
                    _ => return Err(ExtractionError("selection_repair_scope")),
                };
                require(
                    retained.iter().all(|k| before[*k] == after[*k]),
                    "selection_repair_structure_loss",
                )?;
                if key == "alternatives" {
                    let a: Vec<_> = items(before, "branches").collect();
                    let z: Vec<_> = items(after, "branches").collect();
                    if let Some(sr) = scope_by_group.get(&g) {
                        require(a.len() == z.len(), "selection_repair_structure_loss")?;
                        require(
                            has_bijection(a.len(), |old_j, new_j| {
                                let x = a[old_j];
                                let y = z[new_j];
                                let surface_ok = if x["surface"] == y["surface"] {
                                    !sr.surfaces.contains_key(&old_j)
                                        || sr.keepable.contains(&old_j)
                                } else if let Some(opts) = sr.surfaces.get(&old_j) {
                                    y["surface"].as_object().is_some_and(|o| o.len() == 1)
                                        && y["surface"]["quote"]
                                            .as_str()
                                            .is_some_and(|q| opts.contains(q))
                                } else {
                                    false
                                };
                                let borrowed_ok = if x.get("borrowed") == y.get("borrowed") {
                                    true
                                } else if let Some(opts) = repairable.get(&(g, old_j)) {
                                    match y.get("borrowed") {
                                        None => true,
                                        Some(v) => {
                                            v.as_object().is_some_and(|o| o.len() == 1)
                                                && v["quote"]
                                                    .as_str()
                                                    .is_some_and(|q| opts.contains(q))
                                        }
                                    }
                                } else {
                                    false
                                };
                                surface_ok && borrowed_ok
                            }),
                            "selection_repair_choice",
                        )?;
                        let quote = text(&after["quote"]);
                        let mut prior_end = sr.group_span.0;
                        for y in &z {
                            let at = span(source, &y["surface"], quote, b)?.filter(|at| {
                                contains(sr.group_span, *at)
                                    && sr.cue_span.is_none_or(|cs| !overlaps(*at, cs))
                                    && at.0 >= prior_end
                            });
                            let Some(at) = at else {
                                return Err(ExtractionError("selection_repair_revalidation"));
                            };
                            prior_end = at.1;
                        }
                    } else {
                        require(
                            a.len() == z.len()
                                && a.iter().zip(&z).all(|(x, y)| x["surface"] == y["surface"]),
                            "selection_repair_structure_loss",
                        )?;
                        // Unflagged reconstructions stay byte-identical; flagged ones
                        // may only be dropped or replaced by a host-derived choice.
                        for (j, (x, y)) in a.iter().zip(&z).enumerate() {
                            if x.get("borrowed") == y.get("borrowed") {
                                continue;
                            }
                            let options = repairable
                                .get(&(g, j))
                                .ok_or(ExtractionError("selection_repair_structure_loss"))?;
                            require(
                                match y.get("borrowed") {
                                    None => true,
                                    Some(v) => {
                                        v.as_object().is_some_and(|o| o.len() == 1)
                                            && v["quote"]
                                                .as_str()
                                                .is_some_and(|q| options.contains(q))
                                    }
                                },
                                "selection_repair_choice",
                            )?;
                        }
                    }
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
        after
            .issues
            .iter()
            .all(|s| before.issues.contains(s) || allowed_new.contains(s)),
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
