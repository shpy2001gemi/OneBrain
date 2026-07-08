//! # Shared AI Types
//!
//! Core data structures used across the AI layer: messages, responses,
//! tool definitions, inference options, and model metadata.

use serde::{Deserialize, Serialize};

// ─── Role ───────────────────────────────────────────────────────────────

/// Role of a participant in a chat conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System instruction message.
    System,
    /// User input message.
    User,
    /// Assistant (model) response.
    Assistant,
    /// Tool result message.
    Tool,
}

// ─── ChatMessage ────────────────────────────────────────────────────────

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of the message sender.
    pub role: Role,
    /// Text content of the message.
    pub content: String,
    /// Optional tool call ID (for tool result messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Create a system instruction message.
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_call_id: None }
    }

    /// Create a user input message.
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_call_id: None }
    }

    /// Create an assistant response message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None }
    }

    /// Create a tool result message with the given call ID.
    pub fn tool(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: content.into(), tool_call_id: Some(call_id.into()) }
    }
}

// ─── ChatResponse ───────────────────────────────────────────────────────

/// Response from a chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Text content of the response.
    pub content: String,
    /// Any tool calls requested by the model.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallResponse>,
    /// Token usage statistics, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageStats>,
}

// ─── ToolCallResponse ───────────────────────────────────────────────────

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    /// Unique identifier for this tool call.
    pub id: String,
    /// Name of the tool/function to call.
    pub name: String,
    /// Arguments to pass to the tool, as a JSON value.
    pub arguments: serde_json::Value,
}

// ─── ChatOrToolResponse ─────────────────────────────────────────────────

/// Discriminated response: either a text chat reply or tool call requests.
#[derive(Debug, Clone)]
pub enum ChatOrToolResponse {
    /// Plain text chat response.
    Chat(String),
    /// One or more tool calls requested by the model.
    ToolCalls(Vec<ToolCallResponse>),
}

// ─── ToolDefinition ─────────────────────────────────────────────────────

/// Definition of a tool that can be called by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool type — always "function" for function-calling.
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition including name, description, and parameter schema.
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// Create a new tool definition with the given name, description, and JSON Schema parameters.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

/// Function definition within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Name of the function.
    pub name: String,
    /// Human-readable description of what the function does.
    pub description: String,
    /// JSON Schema describing the function's parameters.
    pub parameters: serde_json::Value,
}

// ─── InferenceOptions ───────────────────────────────────────────────────

/// Options controlling model inference behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceOptions {
    /// Sampling temperature (0.0 = deterministic, higher = more creative).
    pub temperature: f32,
    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Nucleus sampling probability threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Random seed for reproducible generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Stop sequences that terminate generation.
    #[serde(default)]
    pub stop: Vec<String>,
}

impl Default for InferenceOptions {
    fn default() -> Self {
        Self {
            temperature: 0.1,
            max_tokens: None,
            top_p: None,
            seed: None,
            stop: Vec::new(),
        }
    }
}

// ─── ModelInfo ───────────────────────────────────────────────────────────

/// Metadata about a loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model name or identifier.
    pub name: String,
    /// Size of the model on disk in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Quantization level (e.g. "Q4_K_M", "Q8_0").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// Approximate parameter count (e.g. "3B", "7B").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<String>,
    /// Maximum context length in tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    /// Model family (e.g. "qwen2.5", "llama3.2").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

// ─── BackendStatus ──────────────────────────────────────────────────────

/// Health status of an AI backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    /// Whether the backend is reachable and operational.
    pub available: bool,
    /// Name of the currently loaded model, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_loaded: Option<String>,
    /// Error message if the backend is unhealthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ─── UsageStats ─────────────────────────────────────────────────────────

/// Token usage statistics for an inference request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct UsageStats {
    /// Number of tokens in the prompt.
    pub prompt_tokens: u32,
    /// Number of tokens generated.
    pub completion_tokens: u32,
    /// Total tokens (prompt + completion).
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_constructors() {
        let sys = ChatMessage::system("You are helpful.");
        assert_eq!(sys.role, Role::System);
        assert!(sys.tool_call_id.is_none());

        let user = ChatMessage::user("Hello");
        assert_eq!(user.role, Role::User);

        let asst = ChatMessage::assistant("Hi there!");
        assert_eq!(asst.role, Role::Assistant);

        let tool = ChatMessage::tool("call_123", r#"{"result": 42}"#);
        assert_eq!(tool.role, Role::Tool);
        assert_eq!(tool.tool_call_id.as_deref(), Some("call_123"));
    }

    #[test]
    fn test_role_serde_roundtrip() {
        let role = Role::Assistant;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, r#""assistant""#);
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Role::Assistant);
    }

    #[test]
    fn test_inference_options_default() {
        let opts = InferenceOptions::default();
        assert!((opts.temperature - 0.1).abs() < f32::EPSILON);
        assert!(opts.max_tokens.is_none());
        assert!(opts.top_p.is_none());
        assert!(opts.seed.is_none());
        assert!(opts.stop.is_empty());
    }

    #[test]
    fn test_tool_definition_new() {
        let tool = ToolDefinition::new(
            "get_weather",
            "Get current weather",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"]
            }),
        );
        assert_eq!(tool.tool_type, "function");
        assert_eq!(tool.function.name, "get_weather");
    }

    #[test]
    fn test_chat_message_serde_roundtrip() {
        let msg = ChatMessage::user("Hello, world!");
        let json = serde_json::to_string(&msg).unwrap();
        let back: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, Role::User);
        assert_eq!(back.content, "Hello, world!");
    }

    #[test]
    fn test_backend_status_available() {
        let status = BackendStatus {
            available: true,
            model_loaded: Some("qwen2.5:3b".to_string()),
            error: None,
        };
        assert!(status.available);
        assert_eq!(status.model_loaded.as_deref(), Some("qwen2.5:3b"));
    }

    #[test]
    fn test_usage_stats() {
        let usage = UsageStats {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        assert_eq!(usage.total_tokens, usage.prompt_tokens + usage.completion_tokens);
    }
}
