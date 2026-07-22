//! # KU Tool Executor
//!
//! Stateful executor that processes tool calls from a local AI model
//! and builds CoreDna knowledge units. The AI sends JSON tool calls,
//! the executor validates and converts them into compact binary instructions.
//!
//! ## Architecture
//! ```text
//! AI model (any local LLM)
//!     │ ToolCall (JSON)
//!     ▼
//! KuToolExecutor
//!     │ CoreDna instructions
//!     ▼
//! core_dna::encode() → compact binary
//! ```
//!
//! ## Usage
//! ```rust,ignore
//! let mut executor = KuToolExecutor::new(dict);
//!
//! // AI sends tool calls
//! executor.execute(&ToolCall { name: "new_ku".into(), arguments: json!({"gene_type": "fact"}) });
//! executor.execute(&ToolCall { name: "add_part_of".into(), arguments: json!({"part": 601, "whole": 600}) });
//! executor.execute(&ToolCall { name: "set_certainty".into(), arguments: json!({"level": 9500}) });
//! executor.execute(&ToolCall { name: "finalize".into(), arguments: json!({}) });
//!
//! // Get encoded KUs
//! let wire_bytes = executor.finalize_all();
//! ```

use crate::core_dna::{CoreDna, Instruction, NumericValue};
use crate::ku_tools::{resolve_gene_type, resolve_unit, ToolCall, ToolResult};
use crate::text_parser::ConceptDict;
use crate::types::ConceptId;

// ============================================================================
// Executor State
// ============================================================================

/// A KU being built by the executor.
struct KuBuilder {
    gene_type: u8,
    instructions: Vec<Instruction>,
}

/// Encoding statistics for reporting.
#[derive(Debug, Clone, Default)]
pub struct EncodingStats {
    pub total_kus: usize,
    pub total_instructions: usize,
    pub total_wire_bytes: usize,
    pub concepts_created: usize,
    pub concepts_looked_up: usize,
    pub tool_calls_processed: usize,
    pub tool_calls_failed: usize,
}

/// The main executor that processes AI tool calls and builds CoreDna KUs.
pub struct KuToolExecutor {
    /// Shared concept dictionary (grows as AI creates new concepts)
    dict: ConceptDict,
    /// Current KU being built (None if no KU started)
    current: Option<KuBuilder>,
    /// Completed KUs (finalized)
    completed: Vec<CoreDna>,
    /// Encoding statistics
    stats: EncodingStats,
}

impl KuToolExecutor {
    /// Create a new executor with the given concept dictionary.
    pub fn new(dict: ConceptDict) -> Self {
        Self {
            dict,
            current: None,
            completed: Vec::new(),
            stats: EncodingStats::default(),
        }
    }

    /// Create with default dictionary.
    pub fn with_default_dict() -> Self {
        Self::new(crate::text_parser::default_dict())
    }

    /// Execute a single tool call. Returns a result message for the AI's context.
    pub fn execute(&mut self, call: &ToolCall) -> ToolResult {
        self.stats.tool_calls_processed += 1;

        let result = match call.name.as_str() {
            "new_ku" => self.exec_new_ku(&call.arguments),
            "finalize" => self.exec_finalize(),
            "lookup" => self.exec_lookup(&call.arguments),
            "lookup_or_create" => self.exec_lookup_or_create(&call.arguments),
            "add_triple" => self.exec_add_triple(&call.arguments),
            "add_part_of" => self.exec_add_part_of(&call.arguments),
            "add_quality" => self.exec_add_quality(&call.arguments),
            "add_quantity" => self.exec_add_quantity(&call.arguments),
            "add_tolerance" => self.exec_add_tolerance(&call.arguments),
            "add_enum_val" => self.exec_add_enum_val(&call.arguments),
            "add_causal" => self.exec_add_causal(&call.arguments),
            "add_located" => self.exec_add_located(&call.arguments),
            "add_step" => self.exec_add_step(&call.arguments),
            "set_certainty" => self.exec_set_certainty(&call.arguments),
            "set_difficulty" => self.exec_set_difficulty(&call.arguments),
            _ => {
                self.stats.tool_calls_failed += 1;
                ToolResult::err(format!("Unknown tool: '{}'. Available: new_ku, finalize, lookup, lookup_or_create, add_triple, add_part_of, add_quality, add_quantity, add_tolerance, add_enum_val, add_causal, add_located, add_step, set_certainty, set_difficulty", call.name))
            }
        };

        if !result.success {
            self.stats.tool_calls_failed += 1;
        }

        result
    }

    /// Execute a batch of tool calls (multi-turn conversation).
    pub fn execute_batch(&mut self, calls: &[ToolCall]) -> Vec<ToolResult> {
        calls.iter().map(|c| self.execute(c)).collect()
    }

    /// Get all completed CoreDna objects.
    pub fn completed_dnas(&self) -> &[CoreDna] {
        &self.completed
    }

    /// Encode all completed KUs to wire bytes. Also finalizes any in-progress KU.
    pub fn finalize_all(&mut self) -> Vec<Vec<u8>> {
        // Auto-finalize current KU if any
        if self.current.is_some() {
            let _ = self.exec_finalize();
        }

        self.completed
            .iter()
            .filter_map(|dna| dna.encode().ok())
            .collect()
    }

    /// Get encoding statistics.
    pub fn stats(&self) -> &EncodingStats {
        &self.stats
    }

    /// Get a reference to the concept dictionary.
    pub fn dict(&self) -> &ConceptDict {
        &self.dict
    }

    /// Get a mutable reference to the concept dictionary.
    pub fn dict_mut(&mut self) -> &mut ConceptDict {
        &mut self.dict
    }

    // ========================================================================
    // Tool Implementations
    // ========================================================================

    fn exec_new_ku(&mut self, args: &serde_json::Value) -> ToolResult {
        // Auto-finalize previous KU if any
        if self.current.is_some() {
            let _ = self.exec_finalize();
        }

        let gene_str = args
            .get("gene_type")
            .and_then(|v| v.as_str())
            .unwrap_or("fact");

        match resolve_gene_type(gene_str) {
            Some(gene_type) => {
                self.current = Some(KuBuilder {
                    gene_type,
                    instructions: Vec::new(),
                });
                ToolResult::ok(format!("Started new KU (gene_type={}). Add instructions then call finalize.", gene_str))
            }
            None => ToolResult::err(format!("Unknown gene_type '{}'. Use: fact, procedure, experience, hypothesis, testimony, formal, composite", gene_str)),
        }
    }

    fn exec_finalize(&mut self) -> ToolResult {
        match self.current.take() {
            Some(builder) => {
                let n_instr = builder.instructions.len();
                let dna = CoreDna::new(builder.gene_type, builder.instructions);

                match dna.encode() {
                    Ok(wire) => {
                        let wire_len = wire.len();
                        self.stats.total_kus += 1;
                        self.stats.total_instructions += n_instr;
                        self.stats.total_wire_bytes += wire_len;
                        self.completed.push(dna);

                        ToolResult::ok_with_data(
                            format!(
                                "KU #{} finalized: {} instructions → {} bytes.",
                                self.stats.total_kus, n_instr, wire_len
                            ),
                            serde_json::json!({
                                "ku_index": self.stats.total_kus - 1,
                                "instructions": n_instr,
                                "wire_bytes": wire_len,
                            }),
                        )
                    }
                    Err(e) => ToolResult::err(format!("Encode error: {}", e)),
                }
            }
            None => ToolResult::err("No KU in progress. Call new_ku first."),
        }
    }

    fn exec_lookup(&mut self, args: &serde_json::Value) -> ToolResult {
        let word = match args.get("word").and_then(|v| v.as_str()) {
            Some(w) => w,
            None => return ToolResult::err("Missing 'word' parameter"),
        };

        let id = self.dict.lookup(word);
        self.stats.concepts_looked_up += 1;

        if id == crate::text_parser::UNKNOWN_CONCEPT {
            ToolResult::ok_with_data(
                format!(
                    "'{}' not found in dictionary. Use lookup_or_create to register it.",
                    word
                ),
                serde_json::json!({ "concept_id": 0, "found": false }),
            )
        } else {
            ToolResult::ok_with_data(
                format!("'{}' = ConceptId {}", word, id),
                serde_json::json!({ "concept_id": id, "found": true }),
            )
        }
    }

    fn exec_lookup_or_create(&mut self, args: &serde_json::Value) -> ToolResult {
        let word = match args.get("word").and_then(|v| v.as_str()) {
            Some(w) => w,
            None => return ToolResult::err("Missing 'word' parameter"),
        };

        let existing = self.dict.lookup(word);
        let is_new = existing == crate::text_parser::UNKNOWN_CONCEPT;
        let id = self.dict.lookup_or_create(word);

        if is_new {
            self.stats.concepts_created += 1;
        }
        self.stats.concepts_looked_up += 1;

        ToolResult::ok_with_data(
            format!(
                "'{}' = ConceptId {} {}",
                word,
                id,
                if is_new { "(NEW)" } else { "(existing)" }
            ),
            serde_json::json!({ "concept_id": id, "created": is_new }),
        )
    }

    // ── Instruction adders (require active KU) ──

    fn require_active_ku(&mut self) -> Result<&mut KuBuilder, ToolResult> {
        match self.current.as_mut() {
            Some(builder) => Ok(builder),
            None => Err(ToolResult::err("No KU in progress. Call new_ku first.")),
        }
    }

    fn get_concept_id(
        &mut self,
        args: &serde_json::Value,
        key: &str,
    ) -> Result<ConceptId, ToolResult> {
        match args.get(key) {
            // If the model sends a string name, auto-resolve via lookup_or_create
            Some(serde_json::Value::String(word)) => {
                let id = self.dict.lookup_or_create(word);
                eprintln!("  [AUTO-RESOLVE] {} '{}' → ConceptId {}", key, word, id);
                Ok(id)
            }
            // If the model sends an integer ID, use it directly
            Some(serde_json::Value::Number(n)) => n.as_u64().ok_or_else(|| {
                ToolResult::err(format!(
                    "Invalid '{}': expected positive integer or string concept name",
                    key
                ))
            }),
            _ => Err(ToolResult::err(format!(
                "Missing '{}' (expected integer ConceptId or string concept name)",
                key
            ))),
        }
    }

    fn exec_add_triple(&mut self, args: &serde_json::Value) -> ToolResult {
        let s = match self.get_concept_id(args, "subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let p = match self.get_concept_id(args, "predicate") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let o = match self.get_concept_id(args, "object") {
            Ok(v) => v,
            Err(e) => return e,
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Triple { s, p, o });
                ToolResult::ok(format!("Added Triple({}, {}, {})", s, p, o))
            }
            Err(e) => e,
        }
    }

    fn exec_add_part_of(&mut self, args: &serde_json::Value) -> ToolResult {
        let part = match self.get_concept_id(args, "part") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let whole = match self.get_concept_id(args, "whole") {
            Ok(v) => v,
            Err(e) => return e,
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder
                    .instructions
                    .push(Instruction::PartOf { part, whole });
                ToolResult::ok(format!("Added PartOf({} ⊂ {})", part, whole))
            }
            Err(e) => e,
        }
    }

    fn exec_add_quality(&mut self, args: &serde_json::Value) -> ToolResult {
        let s = match self.get_concept_id(args, "subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let q = match self.get_concept_id(args, "quality") {
            Ok(v) => v,
            Err(e) => return e,
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Quality { s, q });
                ToolResult::ok(format!("Added Quality({} → {})", s, q))
            }
            Err(e) => e,
        }
    }

    fn exec_add_quantity(&mut self, args: &serde_json::Value) -> ToolResult {
        let s = match self.get_concept_id(args, "subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let value = match args.get("value").and_then(|v| v.as_f64()) {
            Some(v) => v,
            None => return ToolResult::err("Missing 'value' (expected number)"),
        };
        let unit_str = args
            .get("unit")
            .and_then(|v| v.as_str())
            .unwrap_or("dimensionless");
        let unit = resolve_unit(unit_str);

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Quantity {
                    s,
                    value: NumericValue::F32(value as f32),
                    unit,
                });
                ToolResult::ok(format!("Added Quantity({} = {} {})", s, value, unit_str))
            }
            Err(e) => e,
        }
    }

    fn exec_add_tolerance(&mut self, args: &serde_json::Value) -> ToolResult {
        let s = match self.get_concept_id(args, "subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let value = match args.get("value").and_then(|v| v.as_f64()) {
            Some(v) => v,
            None => return ToolResult::err("Missing 'value' (expected number)"),
        };
        let delta = match args.get("delta").and_then(|v| v.as_f64()) {
            Some(v) => v,
            None => return ToolResult::err("Missing 'delta' (expected number)"),
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Tolerance {
                    s,
                    value: NumericValue::F32(value as f32),
                    delta: NumericValue::F32(delta as f32),
                });
                ToolResult::ok(format!("Added Tolerance({} = {} ± {})", s, value, delta))
            }
            Err(e) => e,
        }
    }

    fn exec_add_enum_val(&mut self, args: &serde_json::Value) -> ToolResult {
        let s = match self.get_concept_id(args, "subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let values: Vec<ConceptId> = match args.get("values").and_then(|v| v.as_array()) {
            Some(arr) => {
                let mut ids = Vec::new();
                for v in arr {
                    match v.as_u64() {
                        Some(id) => ids.push(id),
                        None => {
                            return ToolResult::err(
                                "Each value in 'values' must be an integer ConceptId",
                            )
                        }
                    }
                }
                ids
            }
            None => return ToolResult::err("Missing 'values' (expected array of ConceptIds)"),
        };

        match self.require_active_ku() {
            Ok(builder) => {
                let n = values.len();
                builder
                    .instructions
                    .push(Instruction::EnumVal { s, values });
                ToolResult::ok(format!("Added EnumVal({} → {} options)", s, n))
            }
            Err(e) => e,
        }
    }

    fn exec_add_causal(&mut self, args: &serde_json::Value) -> ToolResult {
        let cause = match self.get_concept_id(args, "cause") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let effect = match self.get_concept_id(args, "effect") {
            Ok(v) => v,
            Err(e) => return e,
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder
                    .instructions
                    .push(Instruction::Causal { cause, effect });
                ToolResult::ok(format!("Added Causal({} → {})", cause, effect))
            }
            Err(e) => e,
        }
    }

    fn exec_add_located(&mut self, args: &serde_json::Value) -> ToolResult {
        let s = match self.get_concept_id(args, "subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let location = match self.get_concept_id(args, "location") {
            Ok(v) => v,
            Err(e) => return e,
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder
                    .instructions
                    .push(Instruction::Located { s, location });
                ToolResult::ok(format!("Added Located({} @ {})", s, location))
            }
            Err(e) => e,
        }
    }

    fn exec_add_step(&mut self, args: &serde_json::Value) -> ToolResult {
        let ord = match args.get("ord").and_then(|v| v.as_u64()) {
            Some(v) => v as u8,
            None => return ToolResult::err("Missing 'ord' (expected integer 0-255)"),
        };
        let action = match self.get_concept_id(args, "action") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let target = match self.get_concept_id(args, "target") {
            Ok(v) => v,
            Err(e) => return e,
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Step {
                    ord,
                    action,
                    target,
                });
                ToolResult::ok(format!("Added Step(#{}: {} → {})", ord, action, target))
            }
            Err(e) => e,
        }
    }

    fn exec_set_certainty(&mut self, args: &serde_json::Value) -> ToolResult {
        let level = match args.get("level").and_then(|v| v.as_u64()) {
            Some(v) if v <= 10000 => v as u16,
            Some(_) => return ToolResult::err("'level' must be 0-10000"),
            None => return ToolResult::err("Missing 'level' (expected integer 0-10000)"),
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Certainty { level });
                ToolResult::ok(format!("Set Certainty({})", level))
            }
            Err(e) => e,
        }
    }

    fn exec_set_difficulty(&mut self, args: &serde_json::Value) -> ToolResult {
        let level = match args.get("level").and_then(|v| v.as_u64()) {
            Some(v) if v <= 5 => v as u8,
            Some(_) => return ToolResult::err("'level' must be 0-5"),
            None => return ToolResult::err("Missing 'level' (expected integer 0-5)"),
        };

        match self.require_active_ku() {
            Ok(builder) => {
                builder.instructions.push(Instruction::Difficulty { level });
                ToolResult::ok(format!("Set Difficulty({})", level))
            }
            Err(e) => e,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            name: name.into(),
            arguments: args,
        }
    }

    #[test]
    fn test_basic_fact_workflow() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Executor — Basic Fact Workflow");
        println!("══════════════════════════════════════════════════");

        let mut exec = KuToolExecutor::with_default_dict();

        // Simulate AI tool calls
        let results = exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "fact"})),
            make_call("lookup_or_create", json!({"word": "water"})),
            make_call("lookup_or_create", json!({"word": "temperature"})),
            make_call("lookup_or_create", json!({"word": "boiling"})),
            make_call("add_quality", json!({"subject": 1000, "quality": 1002})),
            make_call(
                "add_quantity",
                json!({"subject": 1000, "value": 100.0, "unit": "degree"}),
            ),
            make_call("set_certainty", json!({"level": 10000})),
            make_call("finalize", json!({})),
        ]);

        for r in &results {
            println!("  {} {}", if r.success { "✓" } else { "✗" }, r.message);
        }

        assert!(results.iter().all(|r| r.success));
        assert_eq!(exec.stats().total_kus, 1);
        assert_eq!(exec.stats().total_instructions, 3); // quality + quantity + certainty

        let wires = exec.finalize_all();
        assert_eq!(wires.len(), 1);
        println!("  Wire: {} bytes", wires[0].len());
        println!("  Basic fact workflow: PASSED ✓");
    }

    #[test]
    fn test_rocket_body_encoding() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Executor — Rocket Body (simulated AI)");
        println!("══════════════════════════════════════════════════");

        let mut exec = KuToolExecutor::with_default_dict();

        // Phase 1: AI looks up concepts
        let calls = vec![
            make_call("lookup_or_create", json!({"word": "tên lửa"})), // 1000
            make_call("lookup_or_create", json!({"word": "thân"})),    // 1001
            make_call("lookup_or_create", json!({"word": "vỏ"})),      // 1002
            make_call("lookup_or_create", json!({"word": "hợp kim nhôm-liti"})), // 1003
            make_call("lookup_or_create", json!({"word": "titan"})),   // 1004
            make_call("lookup_or_create", json!({"word": "carbon composite"})), // 1005
            make_call("lookup_or_create", json!({"word": "nhẹ"})),     // 1006
            make_call("lookup_or_create", json!({"word": "bền"})),     // 1007
            make_call("lookup_or_create", json!({"word": "áp lực lớn"})), // 1008
            make_call("lookup_or_create", json!({"word": "vật liệu"})), // 1009
        ];
        let results = exec.execute_batch(&calls);
        assert!(results.iter().all(|r| r.success));

        // Extract concept IDs from results
        let ids: Vec<u64> = results
            .iter()
            .filter_map(|r| r.data.as_ref())
            .filter_map(|d| d.get("concept_id"))
            .filter_map(|v| v.as_u64())
            .collect();

        println!("  Concepts created: {:?}", ids);

        // Phase 2: AI builds KU
        let rocket = ids[0];
        let body = ids[1];
        let shell = ids[2];
        let al_li = ids[3];
        let titan = ids[4];
        let carbon = ids[5];
        let lightweight = ids[6];
        let strong = ids[7];
        let high_press = ids[8];
        let material = ids[9];

        let results2 = exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "fact"})),
            make_call("add_part_of", json!({"part": body, "whole": rocket})),
            make_call("add_part_of", json!({"part": shell, "whole": rocket})),
            make_call(
                "add_triple",
                json!({"subject": body, "predicate": material, "object": al_li}),
            ),
            make_call(
                "add_enum_val",
                json!({"subject": material, "values": [al_li, titan, carbon]}),
            ),
            make_call(
                "add_quality",
                json!({"subject": body, "quality": lightweight}),
            ),
            make_call("add_quality", json!({"subject": body, "quality": strong})),
            make_call("add_causal", json!({"cause": high_press, "effect": strong})),
            make_call("set_certainty", json!({"level": 9500})),
            make_call("finalize", json!({})),
        ]);

        for r in &results2 {
            println!("  {} {}", if r.success { "✓" } else { "✗" }, r.message);
        }

        assert!(results2.iter().all(|r| r.success));

        let wires = exec.finalize_all();
        assert_eq!(wires.len(), 1);
        println!("  Wire: {} bytes (vs ~200+ bytes text)", wires[0].len());
        println!("  Concepts created: {}", exec.stats().concepts_created);
        println!("  Rocket body encoding: PASSED ✓");
    }

    #[test]
    fn test_multi_ku_workflow() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Executor — Multi-KU (auto-finalize)");
        println!("══════════════════════════════════════════════════");

        let mut exec = KuToolExecutor::with_default_dict();

        // KU 1: Fact
        exec.execute(&make_call("new_ku", json!({"gene_type": "fact"})));
        exec.execute(&make_call(
            "add_quality",
            json!({"subject": 100, "quality": 200}),
        ));
        exec.execute(&make_call("set_certainty", json!({"level": 9000})));

        // KU 2: Procedure (auto-finalizes KU 1)
        exec.execute(&make_call("new_ku", json!({"gene_type": "procedure"})));
        exec.execute(&make_call(
            "add_step",
            json!({"ord": 0, "action": 300, "target": 400}),
        ));
        exec.execute(&make_call(
            "add_step",
            json!({"ord": 1, "action": 301, "target": 401}),
        ));
        exec.execute(&make_call("set_difficulty", json!({"level": 3})));

        // Finalize all (auto-finalizes KU 2)
        let wires = exec.finalize_all();
        assert_eq!(wires.len(), 2, "Should have 2 KUs");
        assert_eq!(exec.stats().total_kus, 2);

        let total: usize = wires.iter().map(|w| w.len()).sum();
        println!("  KU 1: {} bytes", wires[0].len());
        println!("  KU 2: {} bytes", wires[1].len());
        println!("  Total: {} bytes", total);
        println!("  Multi-KU workflow: PASSED ✓");
    }

    #[test]
    fn test_error_handling() {
        println!("\n══════════════════════════════════════════════════");
        println!("  TEST: Executor — Error Handling");
        println!("══════════════════════════════════════════════════");

        let mut exec = KuToolExecutor::with_default_dict();

        // Error: add instruction without new_ku
        let r = exec.execute(&make_call(
            "add_quality",
            json!({"subject": 1, "quality": 2}),
        ));
        assert!(!r.success);
        println!("  ✓ No active KU: {}", r.message);

        // Error: unknown tool
        let r = exec.execute(&make_call("unknown_tool", json!({})));
        assert!(!r.success);
        println!("  ✓ Unknown tool: {}", r.message);

        // Error: missing parameters
        exec.execute(&make_call("new_ku", json!({"gene_type": "fact"})));
        let r = exec.execute(&make_call("add_triple", json!({"subject": 1})));
        assert!(!r.success);
        println!("  ✓ Missing param: {}", r.message);

        // Error: invalid gene type
        let r = exec.execute(&make_call("new_ku", json!({"gene_type": "invalid"})));
        assert!(!r.success);
        println!("  ✓ Invalid gene type: {}", r.message);

        // Error: certainty out of range
        exec.execute(&make_call("new_ku", json!({"gene_type": "fact"})));
        let r = exec.execute(&make_call("set_certainty", json!({"level": 99999})));
        assert!(!r.success);
        println!("  ✓ Out of range: {}", r.message);

        println!("  Error handling: PASSED ✓");
    }

    #[test]
    fn test_full_rocket_5_kus() {
        println!("\n══════════════════════════════════════════════════════════════");
        println!("  TEST: 🚀 Executor — Full Rocket (5 KUs, simulated AI)");
        println!("══════════════════════════════════════════════════════════════");

        let mut exec = KuToolExecutor::with_default_dict();

        // Lookup all concepts first (AI would do this in first pass)
        let concepts = [
            "tên lửa",
            "thân",
            "vỏ",
            "hợp kim nhôm-liti",
            "titan",
            "carbon composite",
            "động cơ",
            "nhiên liệu",
            "hydro lỏng",
            "oxy lỏng",
            "buồng đốt",
            "lực đẩy",
            "bơm",
            "nhiên liệu rắn",
            "đơn giản",
            "tin cậy",
            "dẫn đường",
            "điều khiển",
            "imu",
            "con quay hồi chuyển",
            "máy tính bay",
            "quỹ đạo",
            "thrust vectoring",
            "khoang tải trọng",
            "vệ tinh",
            "đầu đạn",
            "thiết bị nghiên cứu",
            "nhẹ",
            "bền",
            "áp lực lớn",
            "vật liệu",
            "đầu tên lửa",
        ];

        let mut ids = std::collections::HashMap::new();
        for word in &concepts {
            let r = exec.execute(&make_call("lookup_or_create", json!({"word": word})));
            let id = r.data.as_ref().unwrap()["concept_id"].as_u64().unwrap();
            ids.insert(*word, id);
        }

        let c = |word: &str| -> u64 { *ids.get(word).unwrap() };

        // KU 1: Body & Shell
        exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "fact"})),
            make_call("add_part_of", json!({"part": c("thân"), "whole": c("tên lửa")})),
            make_call("add_part_of", json!({"part": c("vỏ"), "whole": c("tên lửa")})),
            make_call("add_triple", json!({"subject": c("thân"), "predicate": c("vật liệu"), "object": c("hợp kim nhôm-liti")})),
            make_call("add_enum_val", json!({"subject": c("vật liệu"), "values": [c("hợp kim nhôm-liti"), c("titan"), c("carbon composite")]})),
            make_call("add_quality", json!({"subject": c("thân"), "quality": c("nhẹ")})),
            make_call("add_quality", json!({"subject": c("thân"), "quality": c("bền")})),
            make_call("add_causal", json!({"cause": c("áp lực lớn"), "effect": c("bền")})),
            make_call("set_certainty", json!({"level": 9500})),
            make_call("finalize", json!({})),
        ]);

        // KU 2: Liquid Fuel Engine
        exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "procedure"})),
            make_call(
                "add_part_of",
                json!({"part": c("động cơ"), "whole": c("tên lửa")}),
            ),
            make_call(
                "add_step",
                json!({"ord": 0, "action": c("bơm"), "target": c("nhiên liệu")}),
            ),
            make_call(
                "add_step",
                json!({"ord": 1, "action": c("bơm"), "target": c("hydro lỏng")}),
            ),
            make_call(
                "add_step",
                json!({"ord": 2, "action": c("bơm"), "target": c("oxy lỏng")}),
            ),
            make_call(
                "add_step",
                json!({"ord": 3, "action": c("buồng đốt"), "target": c("lực đẩy")}),
            ),
            make_call("set_difficulty", json!({"level": 4})),
            make_call("finalize", json!({})),
        ]);

        // KU 3: Solid Fuel
        exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "fact"})),
            make_call("add_triple", json!({"subject": c("nhiên liệu rắn"), "predicate": c("vật liệu"), "object": c("nhiên liệu")})),
            make_call("add_quality", json!({"subject": c("nhiên liệu rắn"), "quality": c("đơn giản")})),
            make_call("add_quality", json!({"subject": c("nhiên liệu rắn"), "quality": c("tin cậy")})),
            make_call("set_certainty", json!({"level": 9000})),
            make_call("finalize", json!({})),
        ]);

        // KU 4: Guidance & Control
        exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "fact"})),
            make_call("add_part_of", json!({"part": c("dẫn đường"), "whole": c("tên lửa")})),
            make_call("add_part_of", json!({"part": c("điều khiển"), "whole": c("tên lửa")})),
            make_call("add_enum_val", json!({"subject": c("dẫn đường"), "values": [c("imu"), c("con quay hồi chuyển"), c("máy tính bay")]})),
            make_call("add_causal", json!({"cause": c("thrust vectoring"), "effect": c("quỹ đạo")})),
            make_call("set_certainty", json!({"level": 9500})),
            make_call("finalize", json!({})),
        ]);

        // KU 5: Payload Bay
        exec.execute_batch(&[
            make_call("new_ku", json!({"gene_type": "fact"})),
            make_call("add_part_of", json!({"part": c("khoang tải trọng"), "whole": c("tên lửa")})),
            make_call("add_located", json!({"subject": c("khoang tải trọng"), "location": c("đầu tên lửa")})),
            make_call("add_enum_val", json!({"subject": c("khoang tải trọng"), "values": [c("vệ tinh"), c("thiết bị nghiên cứu"), c("đầu đạn")]})),
            make_call("set_certainty", json!({"level": 9000})),
            make_call("finalize", json!({})),
        ]);

        // Summary
        let wires = exec.finalize_all();
        let stats = exec.stats();

        println!("\n  📊 Results:");
        for (i, w) in wires.iter().enumerate() {
            println!("    KU #{}: {} bytes", i + 1, w.len());
        }
        let total: usize = wires.iter().map(|w| w.len()).sum();
        println!("  ─────────────────────────");
        println!("  Total:      {} bytes", total);
        println!("  Text:       1078 bytes");
        println!("  Ratio:      {:.1}x smaller", 1078.0 / total as f64);
        println!("  KUs:        {}", stats.total_kus);
        println!("  Instructions: {}", stats.total_instructions);
        println!("  Concepts:   {} created", stats.concepts_created);

        assert_eq!(wires.len(), 5, "Should produce 5 KUs");
        assert!(
            total < 1078,
            "Should be smaller than text ({} vs 1078)",
            total
        );

        println!("\n  🚀 Full rocket encoding: PASSED ✓");
    }
}
