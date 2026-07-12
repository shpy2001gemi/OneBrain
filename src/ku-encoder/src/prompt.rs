//! Prompt builder for AI-assisted KU encoding.
//!
//! Wraps ku-core's `generate_system_prompt()` and constructs the full
//! message sequence (system + user) for LLM tool-calling.
//!
//! # Usage
//! ```rust,ignore
//! use ku_encoder::prompt::PromptBuilder;
//! use ku_core::text_parser::default_dict;
//!
//! let dict = default_dict();
//! let messages = PromptBuilder::build_encoding_messages("Water boils at 100°C", &dict);
//! assert_eq!(messages.len(), 2); // system + user
//! ```

use ku_core::text_parser::ConceptDict;
use ku_core::ku_system_prompt::generate_system_prompt;
use ku_core::ku_tools::tool_definitions_json;
use ku_ai::types::ChatMessage;

/// Builds prompts for AI-assisted KU encoding.
///
/// Combines ku-core's system prompt generator with ku-ai's `ChatMessage`
/// types to produce ready-to-send message sequences for the LLM.
pub struct PromptBuilder;

impl PromptBuilder {
    /// Build the system prompt string using ku-core's generator.
    ///
    /// Embeds the concept dictionary (first 50 entries) and tool definitions
    /// into a comprehensive system instruction for the Knowledge Encoder AI.
    pub fn build_system_prompt(dict: &ConceptDict) -> String {
        let tool_defs = tool_definitions_json();
        generate_system_prompt(dict, &tool_defs)
    }

    /// Build the complete message sequence for encoding a text.
    ///
    /// Returns `[system_message, user_message]` ready for the AI backend.
    pub fn build_encoding_messages(text: &str, dict: &ConceptDict) -> Vec<ChatMessage> {
        let system = Self::build_system_prompt(dict);
        vec![
            ChatMessage::system(system),
            ChatMessage::user(format!(
                "/no_think\nPlease encode the following knowledge into Knowledge Units \
                 using the available tools:\n\n{}",
                text
            )),
        ]
    }

    /// Build messages for a multi-turn encoding conversation.
    ///
    /// Appends tool result messages to the base conversation, enabling
    /// the AI to see previous tool execution results and continue encoding.
    pub fn build_continuation_messages(
        base_messages: &[ChatMessage],
        tool_results: &[(String, String)], // (tool_call_id, result_content)
    ) -> Vec<ChatMessage> {
        let mut messages = base_messages.to_vec();
        for (id, content) in tool_results {
            messages.push(ChatMessage::tool(id.clone(), content.clone()));
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::text_parser::default_dict;

    #[test]
    fn test_build_system_prompt_not_empty() {
        let dict = default_dict();
        let prompt = PromptBuilder::build_system_prompt(&dict);
        assert!(!prompt.is_empty());
        // Should mention Knowledge Unit / KU
        assert!(
            prompt.contains("Knowledge") || prompt.contains("KU"),
            "System prompt should mention Knowledge Units"
        );
    }

    #[test]
    fn test_build_encoding_messages_structure() {
        let dict = default_dict();
        let messages = PromptBuilder::build_encoding_messages("Water boils at 100°C", &dict);

        assert_eq!(messages.len(), 2, "Should have system + user messages");
        assert_eq!(messages[0].role, ku_ai::types::Role::System);
        assert_eq!(messages[1].role, ku_ai::types::Role::User);
        assert!(
            messages[1].content.contains("Water boils"),
            "User message should contain the input text"
        );
    }

    #[test]
    fn test_build_continuation_messages() {
        let dict = default_dict();
        let base = PromptBuilder::build_encoding_messages("test", &dict);
        let results = vec![
            ("call_1".to_string(), "OK: concept found".to_string()),
            ("call_2".to_string(), "OK: triple added".to_string()),
        ];

        let cont = PromptBuilder::build_continuation_messages(&base, &results);
        assert_eq!(cont.len(), 4, "base(2) + 2 tool results");
        assert_eq!(cont[2].role, ku_ai::types::Role::Tool);
        assert_eq!(cont[2].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(cont[3].content, "OK: triple added");
    }

    #[test]
    fn test_build_encoding_messages_preserves_text() {
        let dict = default_dict();
        let input = "Tên lửa có thân bằng hợp kim nhôm-liti";
        let messages = PromptBuilder::build_encoding_messages(input, &dict);
        assert!(
            messages[1].content.contains(input),
            "User message must contain the original text verbatim"
        );
    }
}
