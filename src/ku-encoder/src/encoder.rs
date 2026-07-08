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

use ku_core::ku_tool_executor::{KuToolExecutor, EncodingStats};
use ku_core::ku_tools::{ToolCall as CoreToolCall, ToolResult as CoreToolResult, tool_definitions};
use ku_core::text_parser::ConceptDict;
use ku_ai::traits::ModelBackend;
use ku_ai::types::{
    ChatOrToolResponse, InferenceOptions, ToolCallResponse, ToolDefinition,
};

use crate::error::EncoderError;
use crate::prompt::PromptBuilder;

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
    pub fn new(
        backend: Box<dyn ModelBackend>,
        dict: ConceptDict,
        config: EncoderConfig,
    ) -> Self {
        Self { backend, dict, config }
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
        let response = self.backend.chat_with_tools(&messages, &tool_defs, &options).await?;

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

        // 5. Execute tool calls via KuToolExecutor
        let mut executor = KuToolExecutor::new(self.dict.clone());
        let mut results: Vec<CoreToolResult> = Vec::new();

        for tc in &tool_calls {
            let core_call = CoreToolCall {
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            };
            let result = executor.execute(&core_call);
            results.push(result);
        }

        // 6. Finalize and get wire bytes
        let wire_bytes = executor.finalize_all();

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
        let concepts_used: Vec<String> = tool_calls.iter()
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
    fn calculate_confidence(
        &self,
        stats: &EncodingStats,
        _results: &[CoreToolResult],
    ) -> f32 {
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
        let encoder = AiEncoder::new(
            Box::new(mock),
            default_dict(),
            EncoderConfig::default(),
        );

        let result = encoder.encode("Water boils at 100°C").await.unwrap();
        assert!(!result.wire_bytes.is_empty(), "Should produce wire bytes");
        assert!(result.confidence > 0.0, "Confidence should be positive");
        assert_eq!(result.gene_type.as_deref(), Some("fact"));
        assert_eq!(result.source_text, "Water boils at 100°C");
    }

    #[tokio::test]
    async fn test_encode_no_tool_calls() {
        let mock = MockBackend::new().with_chat_response("I cannot encode this.");
        let encoder = AiEncoder::new(
            Box::new(mock),
            default_dict(),
            EncoderConfig::default(),
        );

        let result = encoder.encode("something").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), EncoderError::NoToolCalls));
    }

    #[tokio::test]
    async fn test_encode_produces_valid_wire_bytes() {
        let mock = MockBackend::new().with_tool_response(make_fact_tool_calls());
        let encoder = AiEncoder::new(
            Box::new(mock),
            default_dict(),
            EncoderConfig::default(),
        );

        let result = encoder.encode("Water boils at 100°C").await.unwrap();
        for bytes in &result.wire_bytes {
            assert!(bytes.len() >= 5, "Wire bytes should have at least header+CRC");
            // Should be valid CoreDna (first byte is CORE_DNA_MAGIC = 0x4B)
            assert_eq!(bytes[0], 0x4B, "First byte should be CoreDna magic");
        }
    }

    #[test]
    fn test_convert_tool_definitions() {
        let mock = MockBackend::new();
        let encoder = AiEncoder::new(
            Box::new(mock),
            default_dict(),
            EncoderConfig::default(),
        );

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
        let encoder = AiEncoder::new(
            Box::new(mock),
            default_dict(),
            EncoderConfig::default(),
        );

        let stats = EncodingStats::default();
        let confidence = encoder.calculate_confidence(&stats, &[]);
        assert!((confidence - 0.0).abs() < f32::EPSILON);
    }
}
