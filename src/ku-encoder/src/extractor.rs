//! # Extractor — AI-powered SPO triple extraction.
//!
//! Calls the AI backend with a compact prompt to extract structured
//! SPO triples from natural language paragraphs. Uses `chat()` (plain
//! text output with JSON parsing), NOT `chat_with_tools()`.
//!
//! # Pipeline position
//! ```text
//! BƯỚC 2: extract_paragraph(paragraph, anchors) → Vec<SpoTriple>
//! ```

use ku_ai::traits::ModelBackend;
use ku_ai::types::{ChatMessage, InferenceOptions};
use ku_core::ku_system_prompt::generate_extraction_prompt;

use crate::error::EncoderError;
use crate::prescan::{format_anchors_for_prompt, override_corrected, verify_anchors};
use crate::types::{Anchor, SpoTriple, VerifyResult};

/// Debug logging macro — only emits in debug builds.
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        eprintln!($($arg)*)
    };
}

// ============================================================================
// SpoExtractor
// ============================================================================

/// AI-powered SPO triple extractor.
///
/// Uses `ModelBackend::chat()` to extract structured knowledge from text.
/// The AI outputs a JSON array of SPO triples which is then parsed.
pub struct SpoExtractor<'a> {
    /// The AI model backend.
    backend: &'a dyn ModelBackend,
    /// Sampling temperature (lower = more deterministic).
    temperature: f32,
    /// Maximum retry attempts on parse failure.
    max_retries: u32,
}

impl<'a> SpoExtractor<'a> {
    /// Create a new extractor with the given backend.
    pub fn new(backend: &'a dyn ModelBackend) -> Self {
        Self {
            backend,
            temperature: 0.1,
            max_retries: 2,
        }
    }

    /// Set the sampling temperature.
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    /// Set the maximum number of retries.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Extract SPO triples from a paragraph of text.
    ///
    /// Includes anchor verification: if AI modifies protected terms,
    /// the extractor will attempt to override or retry.
    ///
    /// # Arguments
    /// * `paragraph` — the text to extract knowledge from
    /// * `anchors` — pre-scanned terms that must be preserved
    ///
    /// # Returns
    /// A vector of verified SPO triples, or an error.
    pub async fn extract(
        &self,
        paragraph: &str,
        anchors: &[Anchor],
    ) -> Result<Vec<SpoTriple>, EncoderError> {
        let anchor_instruction = format_anchors_for_prompt(anchors);

        // Build prompt
        let (system_prompt, user_template) =
            generate_extraction_prompt(anchor_instruction.as_deref());
        let user_message = user_template.replace("{TEXT}", paragraph);

        let messages = vec![
            ChatMessage::system(&system_prompt),
            ChatMessage::user(&user_message),
        ];

        let options = InferenceOptions {
            temperature: self.temperature,
            ..Default::default()
        };

        // Try extraction with retries
        let mut last_error = None;
        for attempt in 0..=self.max_retries {
            match self.try_extract(&messages, &options).await {
                Ok(mut triples) => {
                    // Verify anchors survived AI processing
                    if !anchors.is_empty() {
                        let verify = verify_anchors(anchors, &triples);
                        if verify != VerifyResult::Ok {
                            // Try to override corrected anchors
                            let overrides = override_corrected(anchors, &mut triples);
                            if overrides > 0 {
                                debug_log!(
                                    "[EXTRACTOR] Overrode {} AI-corrected anchor(s)",
                                    overrides
                                );
                            }

                            // Verify again after override
                            let verify2 = verify_anchors(anchors, &triples);
                            if verify2 != VerifyResult::Ok && attempt < self.max_retries {
                                debug_log!(
                                    "[EXTRACTOR] Anchor verification failed after override, \
                                     retrying (attempt {}/{}): {:?}",
                                    attempt + 1,
                                    self.max_retries,
                                    verify2
                                );
                                last_error = Some(EncoderError::AnchorVerificationFailed(format!(
                                    "{:?}",
                                    verify2
                                )));
                                continue;
                            }
                        }
                    }

                    if triples.is_empty() && attempt < self.max_retries {
                        debug_log!(
                            "[EXTRACTOR] AI returned empty triples, retrying ({}/{})",
                            attempt + 1,
                            self.max_retries
                        );
                        last_error = Some(EncoderError::NoTriples);
                        continue;
                    }

                    return Ok(triples);
                }
                Err(e) => {
                    debug_log!(
                        "[EXTRACTOR] Parse error on attempt {}/{}: {}",
                        attempt + 1,
                        self.max_retries,
                        e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(EncoderError::NoTriples))
    }

    /// Single attempt to extract and parse triples.
    async fn try_extract(
        &self,
        messages: &[ChatMessage],
        options: &InferenceOptions,
    ) -> Result<Vec<SpoTriple>, EncoderError> {
        let response = self.backend.chat(messages, options).await?;
        let content = response.content.trim();

        if content.is_empty() {
            return Err(EncoderError::NoTriples);
        }

        // Try to parse as JSON array
        parse_triples_json(content)
    }
}

// ============================================================================
// JSON parsing helpers
// ============================================================================

/// Parse AI response text into a vector of SpoTriples.
///
/// Handles common AI output quirks:
/// - Markdown code fences (```json ... ```) anywhere in text (not just at start)
/// - Bare JSON array `[...]` embedded in conversational text
/// - Leading/trailing whitespace
/// - Single object vs array
pub fn parse_triples_json(text: &str) -> Result<Vec<SpoTriple>, EncoderError> {
    let text = text.trim();

    // Strategy 1: Find ```json ... ``` or ``` ... ``` code fence anywhere in text
    if let Some(json_str) = extract_code_fence(text) {
        if let Ok(triples) = serde_json::from_str::<Vec<SpoTriple>>(json_str) {
            return Ok(triples);
        }
        if let Ok(triple) = serde_json::from_str::<SpoTriple>(json_str) {
            return Ok(vec![triple]);
        }
    }

    // Strategy 2: Find bare JSON array [...] in text
    if let Some(json_str) = extract_json_array(text) {
        if let Ok(triples) = serde_json::from_str::<Vec<SpoTriple>>(json_str) {
            return Ok(triples);
        }
    }

    // Strategy 3: Try parsing the entire text as-is
    if let Ok(triples) = serde_json::from_str::<Vec<SpoTriple>>(text) {
        return Ok(triples);
    }
    if let Ok(triple) = serde_json::from_str::<SpoTriple>(text) {
        return Ok(vec![triple]);
    }

    Err(EncoderError::JsonParseFailed(format!(
        "Could not parse AI output as SpoTriple array or object. First 200 chars: {}",
        &text[..text.len().min(200)]
    )))
}

/// Extract content from a markdown code fence anywhere in text.
///
/// Handles: ````json\n...\n```` or ````\n...\n````
fn extract_code_fence(text: &str) -> Option<&str> {
    // Find opening fence
    let fence_start = text.find("```")?;
    let after_fence = &text[fence_start + 3..];

    // Skip optional language tag (e.g., "json")
    let content_start = after_fence.find('\n')? + 1;
    let content = &after_fence[content_start..];

    // Find closing fence
    let fence_end = content.find("```")?;
    Some(content[..fence_end].trim())
}

/// Extract a JSON array `[...]` from text, handling nested brackets.
fn extract_json_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;

    for i in start..bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_array() {
        let json = r#"[
            {"s":"bàn","s_en":"desk","p":"có","o":"chân","o_en":"leg","qty":4,"role":"part","c":"usually"},
            {"s":"bàn","s_en":"desk","p":"làm bằng","o":"gỗ","o_en":"wood","role":"material","c":"usually"}
        ]"#;
        let triples = parse_triples_json(json).unwrap();
        assert_eq!(triples.len(), 2);
        assert_eq!(triples[0].s, "bàn");
        assert_eq!(triples[0].qty, Some(4.0));
        assert_eq!(triples[1].role, "material");
    }

    #[test]
    fn test_parse_json_single_object() {
        let json = r#"{"s":"water","s_en":"water","p":"boils at","o":"100°C","o_en":"100°C","role":"property","c":"always"}"#;
        let triples = parse_triples_json(json).unwrap();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].s_en, "water");
    }

    #[test]
    fn test_parse_json_with_code_fence() {
        let json = "```json\n[{\"s\":\"x\",\"s_en\":\"x\",\"p\":\"is\",\"o\":\"y\",\"o_en\":\"y\",\"role\":\"relation\",\"c\":\"always\"}]\n```";
        let triples = parse_triples_json(json).unwrap();
        assert_eq!(triples.len(), 1);
    }

    #[test]
    fn test_parse_json_invalid() {
        let result = parse_triples_json("This is not JSON at all");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_json_empty_array() {
        let triples = parse_triples_json("[]").unwrap();
        assert!(triples.is_empty());
    }

    #[test]
    fn test_parse_json_with_formula() {
        let json = r#"[{"s":"H8O","s_en":"H8O","p":"expressed as","o":"H₈O","o_en":"H₈O","role":"formula","notation":"chemical","c":"always"}]"#;
        let triples = parse_triples_json(json).unwrap();
        assert_eq!(triples[0].role, "formula");
        assert_eq!(triples[0].notation, Some("chemical".to_string()));
    }

    #[test]
    fn test_parse_json_text_before_code_fence() {
        // LLM outputs conversational text before the code fence
        let text = "Here is the extracted data:\n```json\n[{\"s\":\"water\",\"s_en\":\"water\",\"p\":\"is\",\"o\":\"liquid\",\"o_en\":\"liquid\",\"role\":\"relation\",\"c\":\"always\"}]\n```";
        let triples = parse_triples_json(text).unwrap();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].s, "water");
    }

    #[test]
    fn test_parse_json_bare_array_in_text() {
        // LLM outputs the array embedded in conversational text
        let text = "I found the following triples: [{\"s\":\"cat\",\"s_en\":\"cat\",\"p\":\"is\",\"o\":\"animal\",\"o_en\":\"animal\",\"role\":\"relation\",\"c\":\"always\"}] hope this helps!";
        let triples = parse_triples_json(text).unwrap();
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].s, "cat");
    }

    #[test]
    fn test_parse_json_code_fence_no_lang() {
        // Code fence without language tag
        let text = "```\n[{\"s\":\"x\",\"s_en\":\"x\",\"p\":\"is\",\"o\":\"y\",\"o_en\":\"y\",\"role\":\"relation\",\"c\":\"always\"}]\n```";
        let triples = parse_triples_json(text).unwrap();
        assert_eq!(triples.len(), 1);
    }
}
