//! AI-assisted KU encoder.
//!
//! Takes natural language text, sends it to a local LLM with tool-calling,
//! and converts the AI's tool calls into CoreDna binary via KuToolExecutor.
//!
//! # Pipeline
//! ```text
//! Text → PromptBuilder → LLM (chat_with_tools) → ToolCallResponse[]
//!      → KuToolExecutor.execute() → finalize_all() → Vec<Vec<u8>>
//! ```
//!
//! # Example
//! ```rust,ignore
//! use ku_encoder::{AiEncoder, EncoderConfig};
//! use ku_ai::backend::MockBackend;
//! use ku_core::text_parser::default_dict;
//!
//! let backend = MockBackend::new().with_tool_response(tool_calls);
//! let encoder = AiEncoder::new(Box::new(backend), default_dict(), EncoderConfig::default());
//! let result = encoder.encode("Water boils at 100°C").await?;
//! ```

use ku_ai::traits::ModelBackend;
use ku_ai::types::{ChatOrToolResponse, InferenceOptions, ToolCallResponse, ToolDefinition};
use ku_core::ku_tool_executor::{EncodingStats, KuToolExecutor};
use ku_core::ku_tools::{tool_definitions, ToolCall as CoreToolCall, ToolResult as CoreToolResult};
use ku_core::text_parser::ConceptDict;

use crate::error::EncoderError;
use crate::prompt::PromptBuilder;

/// Debug logging macro — only emits in debug builds.
/// Silenced in release builds to avoid polluting stderr.
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        eprintln!($($arg)*)
    };
}

/// Result of a successful AI encoding.
#[derive(Debug, Clone)]
pub struct EncodingResult {
    /// Encoded CoreDna wire bytes (one per KU).
    pub wire_bytes: Vec<Vec<u8>>,
    /// Gene type detected ("fact", "procedure", "experience", etc.).
    pub gene_type: Option<String>,
    /// Concepts used during encoding.
    pub concepts_used: Vec<String>,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
    /// Encoding statistics from KuToolExecutor.
    pub stats: EncodingStats,
    /// Original text that was encoded.
    pub source_text: String,
}

/// Configuration for the AI encoder.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// Sampling temperature for the AI model (lower = more deterministic).
    pub temperature: f32,
    /// Maximum number of retries before falling back to rule-based encoding.
    pub max_retries: u32,
    /// Minimum confidence score to accept an encoding result.
    pub min_confidence: f32,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            temperature: 0.1,
            max_retries: 2,
            min_confidence: 0.60,
        }
    }
}

/// AI-powered KU encoder.
///
/// Bridges a `ModelBackend` (from ku-ai) to `KuToolExecutor` (from ku-core),
/// converting natural language text into compact CoreDna binary.
pub struct AiEncoder {
    /// The AI model backend for chat + tool calling.
    backend: Box<dyn ModelBackend>,
    /// Concept dictionary shared with the executor.
    dict: ConceptDict,
    /// Encoder configuration.
    config: EncoderConfig,
}

impl AiEncoder {
    /// Create a new AI encoder with the given backend, dictionary, and config.
    pub fn new(backend: Box<dyn ModelBackend>, dict: ConceptDict, config: EncoderConfig) -> Self {
        Self {
            backend,
            dict,
            config,
        }
    }

    /// Encode natural language text into CoreDna binary.
    ///
    /// Performs the full pipeline:
    /// 1. Build system + user messages via `PromptBuilder`
    /// 2. Convert ku-core `ToolDef`s to ku-ai `ToolDefinition`s
    /// 3. Call the AI backend with tool definitions
    /// 4. Execute returned tool calls via `KuToolExecutor`
    /// 5. Finalize and return wire bytes with confidence score
    pub async fn encode(&self, text: &str) -> Result<EncodingResult, EncoderError> {
        // 1. Build messages
        let messages = PromptBuilder::build_encoding_messages(text, &self.dict);

        // 2. Convert ku-core ToolDef to ku-ai ToolDefinition
        let tool_defs = self.convert_tool_definitions();

        // 3. Call AI with tools
        let options = InferenceOptions {
            temperature: self.config.temperature,
            ..Default::default()
        };
        let response = self
            .backend
            .chat_with_tools(&messages, &tool_defs, &options)
            .await?;

        // 4. Extract tool calls from response
        let tool_calls = match response {
            ChatOrToolResponse::ToolCalls(calls) => calls,
            ChatOrToolResponse::Chat(text) => {
                // Try to parse tool calls from text (some models embed JSON in content)
                self.try_parse_tool_calls_from_text(&text)?
            }
        };

        if tool_calls.is_empty() {
            return Err(EncoderError::NoToolCalls);
        }

        // DEBUG: Log what tool calls the AI generated
        debug_log!("[ENCODER DEBUG] Got {} tool calls:", tool_calls.len());
        for (i, tc) in tool_calls.iter().enumerate() {
            debug_log!("  [{}] {} => {}", i, tc.name, tc.arguments);
        }

        // 5. Auto-fix: inject missing new_ku at the start if model forgot it
        let has_new_ku = tool_calls.iter().any(|tc| tc.name == "new_ku");
        let has_finalize = tool_calls.iter().any(|tc| tc.name == "finalize");

        let mut executor = KuToolExecutor::new(self.dict.clone());
        let mut results: Vec<CoreToolResult> = Vec::new();

        if !has_new_ku {
            debug_log!("[ENCODER FIX] Auto-injecting new_ku(gene_type=fact)");
            let auto_new = CoreToolCall {
                name: "new_ku".into(),
                arguments: serde_json::json!({"gene_type": "fact"}),
            };
            let r = executor.execute(&auto_new);
            debug_log!(
                "  [EXEC] new_ku(auto) => success={}, msg={}",
                r.success,
                r.message
            );
            results.push(r);
        }

        // Execute model's tool calls
        for tc in &tool_calls {
            let core_call = CoreToolCall {
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            };
            let result = executor.execute(&core_call);
            debug_log!(
                "  [EXEC] {} => success={}, msg={}",
                tc.name,
                result.success,
                result.message
            );
            results.push(result);
        }

        // Auto-fix: inject finalize if model forgot it
        if !has_finalize {
            debug_log!("[ENCODER FIX] Auto-injecting finalize");
            let auto_fin = CoreToolCall {
                name: "finalize".into(),
                arguments: serde_json::json!({}),
            };
            let r = executor.execute(&auto_fin);
            debug_log!(
                "  [EXEC] finalize(auto) => success={}, msg={}",
                r.success,
                r.message
            );
            results.push(r);
        }

        // 6. Finalize and get wire bytes
        let wire_bytes = executor.finalize_all();
        debug_log!(
            "[ENCODER DEBUG] finalize_all => {} KUs produced",
            wire_bytes.len()
        );

        if wire_bytes.is_empty() {
            return Err(EncoderError::ToolExecution(
                "No KUs produced after executing tool calls".into(),
            ));
        }

        // 7. Calculate confidence
        let stats = executor.stats().clone();
        let confidence = self.calculate_confidence(&stats, &results);

        // 8. Extract gene type from tool calls
        let gene_type = tool_calls
            .iter()
            .find(|tc| tc.name == "new_ku")
            .and_then(|tc| tc.arguments.get("gene_type"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract concept names from lookup/lookup_or_create tool calls
        let concepts_used: Vec<String> = tool_calls
            .iter()
            .filter(|tc| tc.name == "lookup" || tc.name == "lookup_or_create")
            .filter_map(|tc| tc.arguments.get("word").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();

        Ok(EncodingResult {
            wire_bytes,
            gene_type,
            concepts_used,
            confidence,
            stats,
            source_text: text.to_string(),
        })
    }

    /// Get a reference to the encoder configuration.
    pub fn config(&self) -> &EncoderConfig {
        &self.config
    }

    /// Get a reference to the concept dictionary.
    pub fn dict(&self) -> &ConceptDict {
        &self.dict
    }
}

/// Extract concept ID references from an instruction (for validation).
fn instruction_concept_refs(instr: &ku_core::core_dna::Instruction) -> Vec<u64> {
    use ku_core::core_dna::Instruction;
    match instr {
        Instruction::Triple { s, p, o } => vec![*s, *p, *o],
        Instruction::Quality { s, q } => vec![*s, *q],
        Instruction::Quantity { s, unit, .. } => vec![*s, *unit],
        Instruction::PartOf { part, whole } => vec![*part, *whole],
        Instruction::Located { s, location } => vec![*s, *location],
        Instruction::Temporal { s, time } => vec![*s, *time],
        Instruction::Causal { cause, effect } => vec![*cause, *effect],
        Instruction::Simulates { s, model } => vec![*s, *model],
        Instruction::Condition { cond, result } => vec![*cond, *result],
        Instruction::Agent { actor, action } => vec![*actor, *action],
        Instruction::Tool { action, instrument } => vec![*action, *instrument],
        Instruction::Label { key, value } => vec![*key, *value],
        Instruction::Step { action, target, .. } => vec![*action, *target],
        Instruction::Precond { concept } => vec![*concept],
        Instruction::Effect { concept } => vec![*concept],
        Instruction::EnumVal { s, values } => {
            let mut refs = vec![*s];
            refs.extend(values.iter());
            refs
        }
        Instruction::Constraint { source, target, .. } => vec![*source, *target],
        Instruction::Range { s, .. } => vec![*s],
        Instruction::Tolerance { s, .. } => vec![*s],
        Instruction::Member { label, .. } => vec![*label],
        // No concept refs:
        Instruction::Certainty { .. }
        | Instruction::Difficulty { .. }
        | Instruction::CidRef { .. }
        | Instruction::TextRef { .. }
        | Instruction::Formula { .. }
        | Instruction::MediaRef { .. }
        | Instruction::Affect { .. }
        | Instruction::Sequence { .. }
        | Instruction::Witness { .. }
        | Instruction::CompositeHdr { .. }
        | Instruction::End => vec![],
    }
}

impl AiEncoder {
    // ========================================================================
    // encode_v2 — new pipeline (JSON extraction, no tool-calling)
    // ========================================================================

    /// Encode text using the v2 pipeline.
    ///
    /// Pipeline: prescan → split → extract(JSON) → verify → analyze → resolve → build
    ///
    /// Unlike `encode()` (v1), this method:
    /// - Does NOT use tool-calling — AI outputs JSON triples directly
    /// - Uses deterministic code for analysis, resolution, and building
    /// - Supports anchor protection for formulas and novel terms
    ///
    /// # Arguments
    /// * `text` — natural language text to encode
    /// * `registry` — the ConceptRegistry for name → CCID resolution
    pub async fn encode_v2(
        &self,
        text: &str,
        registry: &dyn ku_core::concept_registry::ConceptLookup,
    ) -> Result<EncodingResult, EncoderError> {
        use crate::analyzer;
        use crate::builder::KuBuilder;
        use crate::concept_resolver::ConceptResolver;
        use crate::extractor::SpoExtractor;
        use crate::prescan::prescan_anchors;
        use crate::splitter::split_paragraphs;
        use crate::types::SpoTriple;

        debug_log!(
            "[ENCODE_V2] Starting pipeline for text ({} bytes)",
            text.len()
        );

        // STEP 0: Pre-scan anchors (detect formulas, numbers, math)
        let anchors = prescan_anchors(text);
        if !anchors.is_empty() {
            debug_log!(
                "[ENCODE_V2] Pre-scanned {} anchor(s): {:?}",
                anchors.len(),
                anchors.iter().map(|a| a.as_str()).collect::<Vec<_>>()
            );
        }

        // STEP 1: Split into paragraphs
        let paragraphs = split_paragraphs(text);
        let total_paragraphs = paragraphs.len();
        debug_log!("[ENCODE_V2] Split into {} paragraph(s)", total_paragraphs);

        // STEP 2: Extract SPO triples per paragraph
        let extractor = SpoExtractor::new(self.backend.as_ref())
            .with_temperature(self.config.temperature)
            .with_max_retries(self.config.max_retries);

        let mut all_triples: Vec<SpoTriple> = Vec::new();
        let mut dropped_paragraphs: Vec<String> = Vec::new();

        for (i, paragraph) in paragraphs.iter().enumerate() {
            debug_log!(
                "[ENCODE_V2] Extracting paragraph {}/{}",
                i + 1,
                total_paragraphs
            );
            match extractor.extract(paragraph, &anchors).await {
                Ok(triples) => {
                    debug_log!("[ENCODE_V2]   → {} triples extracted", triples.len());
                    all_triples.extend(triples);
                }
                Err(e) => {
                    debug_log!("[ENCODE_V2]   → extraction failed: {}", e);
                    dropped_paragraphs.push(paragraph.clone());
                }
            }
        }

        // If >50% paragraphs dropped, return error
        if total_paragraphs > 0 && dropped_paragraphs.len() * 2 > total_paragraphs {
            return Err(EncoderError::ToolExecution(format!(
                "Too many paragraphs failed extraction: {}/{} dropped",
                dropped_paragraphs.len(),
                total_paragraphs
            )));
        }

        if all_triples.is_empty() {
            return Err(EncoderError::NoTriples);
        }

        debug_log!("[ENCODE_V2] Total: {} triples extracted", all_triples.len());

        // STEP 3: Analyze (map role → Op, certainty → u16)
        let analyzed = analyzer::analyze(all_triples.clone());

        // STEP 4: Resolve concepts (name → CCID)
        let mut resolver = ConceptResolver::new(registry);
        let resolved = resolver
            .resolve_all(analyzed)
            .map_err(|error| EncoderError::ConceptRegistry(error.to_string()))?;
        let _total_concepts = resolved.len();

        // Log any resolution warnings (fuzzy/ambiguous matches)
        let warnings = resolver.take_warnings();
        if !warnings.is_empty() {
            debug_log!("[ENCODE_V2] {} resolution warning(s):", warnings.len());
            for w in &warnings {
                debug_log!(
                    "  {:?}: \"{}\" → \"{}\" ({} candidates)",
                    w.warning_type,
                    w.input_name,
                    w.chosen_canonical,
                    w.candidate_count
                );
            }
        }

        // STEP 5: Build CoreDna units (1 triple = 1 KU)
        let built =
            KuBuilder::build(resolved).map_err(|e| EncoderError::CoreDnaError(e.to_string()))?;

        // STEP 6: Validate each KU (concept table consistency)
        let mut valid_results: Vec<(ku_core::core_dna::CoreDna, Vec<u8>)> = Vec::new();
        let mut validation_failures = 0usize;

        for (dna, wire_bytes) in built {
            // Quick structural validation: decode wire bytes and check concept table
            match ku_core::core_dna::decode_core_dna(&wire_bytes) {
                Ok(decoded) => {
                    // Verify concept table consistency
                    let mut valid = true;
                    let local_ids: std::collections::HashSet<u64> =
                        decoded.concept_table.iter().map(|e| e.local_id).collect();

                    for instr in &decoded.instructions {
                        let refs = instruction_concept_refs(instr);
                        for r in refs {
                            if r >= 16512 && !local_ids.contains(&r) {
                                debug_log!("[ENCODE_V2] Validation fail: instruction refs concept {} not in table", r);
                                valid = false;
                                break;
                            }
                        }
                        if !valid {
                            break;
                        }
                    }

                    if valid {
                        valid_results.push((dna, wire_bytes));
                    } else {
                        validation_failures += 1;
                    }
                }
                Err(e) => {
                    debug_log!("[ENCODE_V2] Wire decode fail: {}", e);
                    validation_failures += 1;
                }
            }
        }

        if validation_failures > 0 {
            debug_log!(
                "[ENCODE_V2] {} KU(s) failed validation, {} passed",
                validation_failures,
                valid_results.len()
            );
        }

        let wire_bytes: Vec<Vec<u8>> = valid_results
            .iter()
            .map(|(_, bytes)| bytes.clone())
            .collect();
        let ku_count = wire_bytes.len();

        debug_log!(
            "[ENCODE_V2] Built {} KU(s), total {} bytes",
            ku_count,
            wire_bytes.iter().map(|b| b.len()).sum::<usize>()
        );

        // Determine gene type from majority of triples
        let gene_type_str = match analyzer::determine_gene_type(&all_triples) {
            analyzer::GENE_PROCEDURE => "procedure",
            analyzer::GENE_EXPERIENCE => "experience",
            _ => "fact",
        };

        // Extract concept names used
        let concepts_used: Vec<String> = all_triples
            .iter()
            .flat_map(|t| vec![t.s_en.clone(), t.o_en.clone()])
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Calculate real confidence (not hardcoded)
        let paragraph_success_rate = if total_paragraphs > 0 {
            (total_paragraphs - dropped_paragraphs.len()) as f32 / total_paragraphs as f32
        } else {
            0.0
        };
        let validation_success_rate = if ku_count + validation_failures > 0 {
            ku_count as f32 / (ku_count + validation_failures) as f32
        } else {
            0.0
        };
        let confidence = if ku_count > 0 {
            (paragraph_success_rate * 0.5 + validation_success_rate * 0.5).clamp(0.0, 1.0)
        } else {
            0.0
        };

        Ok(EncodingResult {
            wire_bytes,
            gene_type: Some(gene_type_str.to_string()),
            concepts_used,
            confidence,
            stats: ku_core::ku_tool_executor::EncodingStats {
                total_kus: ku_count,
                total_instructions: all_triples.len(),
                ..Default::default()
            },
            source_text: text.to_string(),
        })
    }

    /// Convert ku-core tool definitions to ku-ai format.
    ///
    /// Maps each `ToolDef { name, description, parameters }` from ku-core
    /// into a `ToolDefinition` compatible with the ku-ai backend API.
    fn convert_tool_definitions(&self) -> Vec<ToolDefinition> {
        tool_definitions()
            .into_iter()
            .map(|td| ToolDefinition::new(td.name, td.description, td.parameters.clone()))
            .collect()
    }

    /// Try to parse tool calls from text content (for models that embed JSON in chat).
    ///
    /// Some smaller models may return tool calls as JSON in the chat content
    /// rather than via the structured tool_calls field.
    fn try_parse_tool_calls_from_text(
        &self,
        text: &str,
    ) -> Result<Vec<ToolCallResponse>, EncoderError> {
        // Try parsing as JSON array of tool calls
        if let Ok(calls) = serde_json::from_str::<Vec<ToolCallResponse>>(text) {
            return Ok(calls);
        }
        Err(EncoderError::NoToolCalls)
    }

    /// Calculate encoding confidence based on execution statistics.
    ///
    /// Factors:
    /// - **Success rate** (70% weight): proportion of tool calls that succeeded.
    /// - **Instruction count** (30% weight): whether multiple instructions were produced.
    fn calculate_confidence(&self, stats: &EncodingStats, _results: &[CoreToolResult]) -> f32 {
        if stats.total_kus == 0 {
            return 0.0;
        }

        let success_rate = if stats.tool_calls_processed > 0 {
            1.0 - (stats.tool_calls_failed as f32 / stats.tool_calls_processed as f32)
        } else {
            0.0
        };

        let has_concepts = if stats.total_instructions > 1 {
            1.0
        } else {
            0.5
        };

        (success_rate * 0.7 + has_concepts * 0.3).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_ai::backend::MockBackend;
    use ku_ai::types::ToolCallResponse;
    use ku_core::text_parser::default_dict;

    /// Helper to create tool call responses that simulate a simple KU encoding.
    fn make_fact_tool_calls() -> Vec<ToolCallResponse> {
        vec![
            ToolCallResponse {
                id: "call_1".into(),
                name: "new_ku".into(),
                arguments: serde_json::json!({"gene_type": "fact"}),
            },
            ToolCallResponse {
                id: "call_2".into(),
                name: "lookup_or_create".into(),
                arguments: serde_json::json!({"word": "water"}),
            },
            ToolCallResponse {
                id: "call_3".into(),
                name: "lookup_or_create".into(),
                arguments: serde_json::json!({"word": "boiling"}),
            },
            ToolCallResponse {
                id: "call_4".into(),
                name: "add_quality".into(),
                arguments: serde_json::json!({"subject": 1000, "quality": 1001}),
            },
            ToolCallResponse {
                id: "call_5".into(),
                name: "set_certainty".into(),
                arguments: serde_json::json!({"level": 9500}),
            },
            ToolCallResponse {
                id: "call_6".into(),
                name: "finalize".into(),
                arguments: serde_json::json!({}),
            },
        ]
    }

    #[tokio::test]
    async fn test_encode_with_mock_backend() {
        let mock = MockBackend::new().with_tool_response(make_fact_tool_calls());
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let result = encoder.encode("Water boils at 100°C").await.unwrap();
        assert!(!result.wire_bytes.is_empty(), "Should produce wire bytes");
        assert!(result.confidence > 0.0, "Confidence should be positive");
        assert_eq!(result.gene_type.as_deref(), Some("fact"));
        assert_eq!(result.source_text, "Water boils at 100°C");
    }

    #[tokio::test]
    async fn test_encode_no_tool_calls() {
        let mock = MockBackend::new().with_chat_response("I cannot encode this.");
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let result = encoder.encode("something").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EncoderError::NoToolCalls));
    }

    #[tokio::test]
    async fn test_encode_produces_valid_wire_bytes() {
        let mock = MockBackend::new().with_tool_response(make_fact_tool_calls());
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let result = encoder.encode("Water boils at 100°C").await.unwrap();
        for bytes in &result.wire_bytes {
            assert!(
                bytes.len() >= 5,
                "Wire bytes should have at least header+CRC"
            );
            // Should be valid CoreDna (first byte is CORE_DNA_MAGIC = 0x4B)
            assert_eq!(bytes[0], 0x4B, "First byte should be CoreDna magic");
        }
    }

    #[test]
    fn test_convert_tool_definitions() {
        let mock = MockBackend::new();
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let defs = encoder.convert_tool_definitions();
        assert_eq!(defs.len(), 15, "Should have 15 tool definitions");
        assert_eq!(defs[0].function.name, "new_ku");
    }

    #[test]
    fn test_encoder_config_default() {
        let config = EncoderConfig::default();
        assert!((config.temperature - 0.1).abs() < f32::EPSILON);
        assert_eq!(config.max_retries, 2);
        assert!((config.min_confidence - 0.60).abs() < f32::EPSILON);
    }

    #[test]
    fn test_calculate_confidence_no_kus() {
        let mock = MockBackend::new();
        let encoder = AiEncoder::new(Box::new(mock), default_dict(), EncoderConfig::default());

        let stats = EncodingStats::default();
        let confidence = encoder.calculate_confidence(&stats, &[]);
        assert!((confidence - 0.0).abs() < f32::EPSILON);
    }
}
