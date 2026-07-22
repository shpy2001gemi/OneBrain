//! # KU Tool Definitions
//!
//! Defines the 15 tools that a local AI model can call to encode knowledge
//! into CoreDna instructions. Compatible with OpenAI function calling format,
//! Ollama tools API, llama.cpp tool mode, and any JSON Schema-based tool interface.
//!
//! The AI runtime is **pluggable** — this module only exports tool schemas.
//! Any local model (Gemma 4, Qwen 2.5, Phi-3, Llama 3.1) can use these tools
//! as long as it supports function calling or structured output.

use serde::{Deserialize, Serialize};

// ============================================================================
// Tool Definition Types
// ============================================================================

/// A single tool definition with JSON Schema parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    /// Tool name (snake_case, e.g. "add_triple")
    pub name: &'static str,
    /// Human-readable description for the AI
    pub description: &'static str,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

/// A tool call from the AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name
    pub name: String,
    /// Arguments as JSON object
    pub arguments: serde_json::Value,
}

/// Result returned to the AI after executing a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the call succeeded
    pub success: bool,
    /// Human-readable message for AI context window
    pub message: String,
    /// Structured return data (e.g., ConceptId from lookup)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ToolResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: None,
        }
    }

    pub fn ok_with_data(message: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

// ============================================================================
// Tool Definitions — 15 tools for knowledge encoding
// ============================================================================

/// Returns all tool definitions.
pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        // ── Session management ──
        ToolDef {
            name: "new_ku",
            description: "Start a new Knowledge Unit. Must be called before adding instructions. \
                          Gene types: 'fact' (default), 'procedure' (steps), 'experience' (sensory/emotional).",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "gene_type": {
                        "type": "string",
                        "enum": ["fact", "procedure", "experience", "hypothesis", "testimony", "formal", "composite"],
                        "description": "Type of knowledge: fact (declarations), procedure (steps), experience (sensory/emotional)"
                    }
                },
                "required": ["gene_type"]
            }),
        },
        ToolDef {
            name: "finalize",
            description: "Finalize the current KU, encode it to compact binary, and prepare for the next one. \
                          Always call this when done with a KU before starting a new one.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },

        // ── Concept lookup ──
        ToolDef {
            name: "lookup",
            description: "Look up a word/phrase in the ConceptDict to get its ConceptId. \
                          Returns the ID if found, or 0 (unknown) if not. \
                          Always try lookup before lookup_or_create.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "word": {
                        "type": "string",
                        "description": "Word or phrase to look up (case-insensitive)"
                    }
                },
                "required": ["word"]
            }),
        },
        ToolDef {
            name: "lookup_or_create",
            description: "Look up a word in ConceptDict; if not found, auto-assign a new ConceptId. \
                          Use this for domain-specific terms not in the base dictionary.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "word": {
                        "type": "string",
                        "description": "Word or phrase to look up or register"
                    }
                },
                "required": ["word"]
            }),
        },

        // ── Relationship instructions ──
        ToolDef {
            name: "add_triple",
            description: "Add a Subject-Predicate-Object triple. Use for: 'X is Y', 'X has property P'. \
                          Pass concept NAMES as strings (e.g. subject=\"water\", predicate=\"is_a\", object=\"liquid\").",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Subject concept name" },
                    "predicate": { "type": "string", "description": "Predicate/relationship concept name" },
                    "object": { "type": "string", "description": "Object concept name" }
                },
                "required": ["subject", "predicate", "object"]
            }),
        },
        ToolDef {
            name: "add_part_of",
            description: "Declare that 'part' is a component of 'whole'. Use for: 'X contains Y'. \
                          Pass concept NAMES as strings.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "part": { "type": "string", "description": "Part concept name" },
                    "whole": { "type": "string", "description": "Whole concept name" }
                },
                "required": ["part", "whole"]
            }),
        },
        ToolDef {
            name: "add_quality",
            description: "Assign a quality/property to a subject. Use for: 'X is lightweight'. \
                          Pass concept NAMES as strings.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Subject concept name" },
                    "quality": { "type": "string", "description": "Quality concept name" }
                },
                "required": ["subject", "quality"]
            }),
        },
        ToolDef {
            name: "add_quantity",
            description: "Assign a numeric measurement to a subject. Use for: 'temperature = 100°C'. \
                          Pass subject as concept NAME string.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Subject concept name" },
                    "value": { "type": "number", "description": "Numeric value (float)" },
                    "unit": { "type": "string", "description": "Unit: degree, meter, second, kg, percent, cm, km, ms, min, hour, dimensionless" }
                },
                "required": ["subject", "value", "unit"]
            }),
        },
        ToolDef {
            name: "add_tolerance",
            description: "Add a tolerance/margin to a measurement. Use for: '± 0.5°'. \
                          Pass subject as concept NAME string.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Subject concept name" },
                    "value": { "type": "number", "description": "Nominal value" },
                    "delta": { "type": "number", "description": "Tolerance delta (±)" }
                },
                "required": ["subject", "value", "delta"]
            }),
        },
        ToolDef {
            name: "add_enum_val",
            description: "Declare a set of possible values/options. \
                          Pass subject as concept NAME string, values as array of integers.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Subject concept name" },
                    "values": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "List of possible value ConceptIds"
                    }
                },
                "required": ["subject", "values"]
            }),
        },
        ToolDef {
            name: "add_causal",
            description: "Declare a cause-effect relationship. Use for: 'heat → boiling'. \
                          Pass concept NAMES as strings.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "cause": { "type": "string", "description": "Cause concept name" },
                    "effect": { "type": "string", "description": "Effect concept name" }
                },
                "required": ["cause", "effect"]
            }),
        },
        ToolDef {
            name: "add_located",
            description: "Declare spatial location. Use for: 'payload is at nose'. \
                          Pass concept NAMES as strings.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Subject concept name" },
                    "location": { "type": "string", "description": "Location concept name" }
                },
                "required": ["subject", "location"]
            }),
        },

        // ── Procedure instructions ──
        ToolDef {
            name: "add_step",
            description: "Add a procedure step. Steps are ordered by 'ord'. \
                          Pass action and target as concept NAME strings.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "ord": { "type": "integer", "description": "Step order (0-based)" },
                    "action": { "type": "string", "description": "Action concept name (verb)" },
                    "target": { "type": "string", "description": "Target concept name" }
                },
                "required": ["ord", "action", "target"]
            }),
        },

        // ── Metadata instructions ──
        ToolDef {
            name: "set_certainty",
            description: "Set confidence level for the current KU. Scale: 0-10000 \
                          (10000 = axiomatic truth, 9500 = established fact, 9000 = high confidence, \
                          7000 = moderate, 5000 = uncertain, <3000 = speculation).",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "level": { "type": "integer", "minimum": 0, "maximum": 10000, "description": "Certainty level 0-10000" }
                },
                "required": ["level"]
            }),
        },
        ToolDef {
            name: "set_difficulty",
            description: "Set complexity/difficulty level for a procedure KU. Scale: 0-5.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "level": { "type": "integer", "minimum": 0, "maximum": 5, "description": "Difficulty 0-5" }
                },
                "required": ["level"]
            }),
        },
    ]
}

/// Returns all tool definitions as a JSON string for embedding in AI prompts.
pub fn tool_definitions_json() -> String {
    let defs = tool_definitions();
    serde_json::to_string_pretty(&defs).unwrap_or_default()
}

/// Returns tool definitions in OpenAI-compatible function calling format.
pub fn tool_definitions_openai_format() -> serde_json::Value {
    let defs = tool_definitions();
    let tools: Vec<serde_json::Value> = defs
        .iter()
        .map(|d| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": d.name,
                    "description": d.description,
                    "parameters": d.parameters,
                }
            })
        })
        .collect();
    serde_json::json!(tools)
}

// ============================================================================
// Unit name → ConceptId mapping (for add_quantity tool)
// ============================================================================

/// Resolve unit name string to well-known ConceptId.
pub fn resolve_unit(unit: &str) -> crate::types::ConceptId {
    match unit.to_lowercase().as_str() {
        "degree" | "°" | "deg" => crate::text_parser::UNIT_DEGREE,
        "meter" | "m" => crate::text_parser::UNIT_METER,
        "second" | "s" => crate::text_parser::UNIT_SECOND,
        "kg" | "kilogram" => crate::text_parser::UNIT_KILOGRAM,
        "percent" | "%" => crate::text_parser::UNIT_PERCENT,
        "cm" | "centimeter" => crate::text_parser::UNIT_CENTIMETER,
        "km" | "kilometer" => crate::text_parser::UNIT_KILOMETER,
        "ms" | "millisecond" => crate::text_parser::UNIT_MILLISECOND,
        "min" | "minute" => crate::text_parser::UNIT_MINUTE,
        "hour" | "h" => crate::text_parser::UNIT_HOUR,
        _ => crate::text_parser::UNIT_DIMENSIONLESS,
    }
}

// ============================================================================
// Gene type string → u8 mapping
// ============================================================================

/// Resolve gene type name to u8 code.
pub fn resolve_gene_type(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "fact" => Some(0),
        "procedure" => Some(1),
        "experience" => Some(2),
        "creative" => Some(3),
        "media" | "media_experience" => Some(4),
        "testimony" => Some(5),
        "formal" => Some(6),
        "hypothesis" => Some(7),
        "narrative" => Some(8),
        "sensory" => Some(9),
        "composite" => Some(10),
        _ => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_count() {
        let defs = tool_definitions();
        assert_eq!(defs.len(), 15, "Should have 15 tools");
        println!("  ✓ 15 tool definitions");
    }

    #[test]
    fn test_tool_names_unique() {
        let defs = tool_definitions();
        let mut names: Vec<&str> = defs.iter().map(|d| d.name).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), original_len, "Tool names must be unique");
        println!("  ✓ All tool names unique");
    }

    #[test]
    fn test_tool_definitions_json() {
        let json = tool_definitions_json();
        assert!(!json.is_empty());
        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        println!("  ✓ JSON output valid ({} bytes)", json.len());
    }

    #[test]
    fn test_openai_format() {
        let openai = tool_definitions_openai_format();
        assert!(openai.is_array());
        let arr = openai.as_array().unwrap();
        assert_eq!(arr.len(), 15);
        // Check structure
        for tool in arr {
            assert_eq!(tool["type"], "function");
            assert!(tool["function"]["name"].is_string());
            assert!(tool["function"]["parameters"].is_object());
        }
        println!("  ✓ OpenAI format valid");
    }

    #[test]
    fn test_resolve_unit() {
        assert_eq!(resolve_unit("degree"), crate::text_parser::UNIT_DEGREE);
        assert_eq!(resolve_unit("m"), crate::text_parser::UNIT_METER);
        assert_eq!(resolve_unit("kg"), crate::text_parser::UNIT_KILOGRAM);
        assert_eq!(
            resolve_unit("unknown"),
            crate::text_parser::UNIT_DIMENSIONLESS
        );
        println!("  ✓ Unit resolution correct");
    }

    #[test]
    fn test_resolve_gene_type() {
        assert_eq!(resolve_gene_type("fact"), Some(0));
        assert_eq!(resolve_gene_type("procedure"), Some(1));
        assert_eq!(resolve_gene_type("experience"), Some(2));
        assert_eq!(resolve_gene_type("composite"), Some(10));
        assert_eq!(resolve_gene_type("invalid"), None);
        println!("  ✓ Gene type resolution correct");
    }

    #[test]
    fn test_tool_call_serde() {
        let call = ToolCall {
            name: "add_triple".into(),
            arguments: serde_json::json!({
                "subject": 500,
                "predicate": 501,
                "object": 502,
            }),
        };
        let json = serde_json::to_string(&call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "add_triple");
        assert_eq!(parsed.arguments["subject"], 500);
        println!("  ✓ ToolCall serialization roundtrip");
    }

    #[test]
    fn test_tool_result_serde() {
        let result = ToolResult::ok_with_data(
            "Found concept 'rocket' = 600",
            serde_json::json!({ "concept_id": 600 }),
        );
        assert!(result.success);
        assert!(result.data.is_some());

        let err = ToolResult::err("Unknown tool: xyz");
        assert!(!err.success);
        assert!(err.data.is_none());
        println!("  ✓ ToolResult serialization");
    }
}
