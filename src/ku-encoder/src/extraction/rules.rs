//! Explicit no-LLM source form: @ku1 ("subject") [predicate] ("object")
//! Each required unit must match completely. Arbitrary prose abstains.
use super::*;
use serde_json::{json, Value};

pub(crate) fn candidate(context: &Value, budget: &mut WorkBudget) -> Result<Value> {
    check(context, "Context", budget)?;
    let raw = context["source_text"]
        .as_str()
        .ok_or(ExtractionError("source_text"))?;
    let grammar=regex::Regex::new(r#"\A@ku1 \("(?P<left>[^"\r\n]+)"\) \[(?P<predicate>[^\[\]\r\n]+)\] \("(?P<right>[^"\r\n]+)"\)\z"#).expect("static source grammar");
    let mut concepts = Vec::new();
    let mut statements = Vec::new();
    let mut coverage = Vec::new();
    for (i, u) in context["required_units"]
        .as_array()
        .ok_or(ExtractionError("coverage_set"))?
        .iter()
        .enumerate()
    {
        let start = u["span"]["start"]
            .as_u64()
            .ok_or(ExtractionError("span_bounds"))? as usize;
        let end = u["span"]["end"]
            .as_u64()
            .ok_or(ExtractionError("span_bounds"))? as usize;
        let text = raw.get(start..end).ok_or(ExtractionError("span_bounds"))?;
        budget.charge(text.len())?;
        if let Some(captures) = grammar.captures(text) {
            let span = |name: &str| {
                let m = captures.name(name).expect("fixed capture");
                json!({"start":start+m.start(),"end":start+m.end(),"quote":m.as_str()})
            };
            let p = format!("p{i}");
            let s = format!("s{i}");
            let evidence = span("predicate");
            concepts.push(json!({"key":p,"label":evidence["quote"],"evidence":evidence}));
            statements.push(json!({"key":s,"predicate":p,"evidence":[u["span"]],"arguments":[{"kind":"text","value":span("left")["quote"],"evidence":span("left")},{"kind":"text","value":span("right")["quote"],"evidence":span("right")}],"negation":{"value":false,"evidence":[]},"modality":{"value":"asserted","evidence":[]}}));
            coverage.push(
                json!({"unit":u["key"],"status":"represented","statements":[s],"reason":"none"}),
            );
        } else {
            coverage.push(json!({"unit":u["key"],"status":"unsupported","statements":[],"reason":"unsupported_semantics"}));
        }
    }
    let candidate = json!({"profile":"ku-extraction/1.0","attempt_id":context["attempt_id"],"context_sha256":hash(context)?,"concepts":concepts,"statements":statements,"coverage":coverage});
    check(&candidate, "Candidate", budget)?;
    Ok(candidate)
}

/// This adapter cannot run inference. Its method bodies enforce that invariant
/// even if called directly instead of through the no-LLM workflow branch.
pub struct NoLlmProvider {
    manifest: Value,
}
impl NoLlmProvider {
    pub fn new() -> Self {
        let binding = ExtractionWorkflow::bundle_hash();
        Self {
            manifest: json!({"profile":"ku-extraction-provider/1.0","provider_id":"ku1-explicit-text-rule/1","backend_build_sha256":binding,"mode":"rules","tools_enabled":false,"max_context_tokens":0,"peak_bytes_reservation":16777216,"schema_bundle_sha256":binding,"supported_schema_keywords":[],"temperature_milli":0}),
        }
    }
}
impl Default for NoLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}
#[async_trait::async_trait]
impl ExtractionProvider for NoLlmProvider {
    fn manifest(&self) -> &Value {
        &self.manifest
    }
    fn input_tokens(&self, _: &ProviderRequest) -> Result<u32> {
        Err(ExtractionError("no_llm_provider"))
    }
    async fn extract(&self, _: ProviderRequest) -> Result<Vec<u8>> {
        Err(ExtractionError("no_llm_provider"))
    }
}
