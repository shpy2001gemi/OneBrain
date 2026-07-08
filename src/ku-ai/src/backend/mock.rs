//! # Mock Backend
//!
//! A deterministic mock implementation of [`ModelBackend`] and [`EmbeddingProvider`]
//! for testing. Returns canned responses in sequence.

use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::error::AiError;
use crate::traits::{EmbeddingProvider, ModelBackend};
use crate::types::*;

/// Mock backend for testing AI layer components without a real LLM.
///
/// Stores a queue of responses and returns them in order.
/// When the queue is exhausted, wraps around to the beginning.
pub struct MockBackend {
    responses: Mutex<Vec<ChatOrToolResponse>>,
    index: AtomicUsize,
    embedding_dims: usize,
}

impl MockBackend {
    /// Create a new empty mock backend with 768-dimension embeddings.
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(Vec::new()),
            index: AtomicUsize::new(0),
            embedding_dims: 768,
        }
    }

    /// Add a raw response to the queue.
    pub fn with_response(self, response: ChatOrToolResponse) -> Self {
        if let Ok(mut responses) = self.responses.lock() {
            responses.push(response);
        }
        self
    }

    /// Add a simple text chat response to the queue.
    pub fn with_chat_response(self, text: impl Into<String>) -> Self {
        self.with_response(ChatOrToolResponse::Chat(text.into()))
    }

    /// Add a tool call response to the queue.
    pub fn with_tool_response(self, tool_calls: Vec<ToolCallResponse>) -> Self {
        self.with_response(ChatOrToolResponse::ToolCalls(tool_calls))
    }

    /// Get the next response from the queue.
    fn next_response(&self) -> ChatOrToolResponse {
        let responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        if responses.is_empty() {
            return ChatOrToolResponse::Chat("mock response".to_string());
        }
        let idx = self.index.fetch_add(1, Ordering::Relaxed) % responses.len();
        responses[idx].clone()
    }

    /// Generate a deterministic embedding vector for testing.
    fn deterministic_embedding(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.embedding_dims];
        // Use a simple hash-based approach for deterministic but varied vectors
        let hash = text.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(b as u64)
        });
        for (i, v) in vec.iter_mut().enumerate() {
            let seed = hash.wrapping_add(i as u64);
            // Map to [-1, 1] range
            *v = ((seed % 2000) as f32 / 1000.0) - 1.0;
        }
        // Normalize to unit vector
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vec.iter_mut() {
                *v /= norm;
            }
        }
        vec
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelBackend for MockBackend {
    async fn chat(
        &self,
        _messages: &[ChatMessage],
        _options: &InferenceOptions,
    ) -> Result<ChatResponse, AiError> {
        let response = self.next_response();
        match response {
            ChatOrToolResponse::Chat(text) => Ok(ChatResponse {
                content: text,
                tool_calls: Vec::new(),
                usage: Some(UsageStats {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                }),
            }),
            ChatOrToolResponse::ToolCalls(calls) => Ok(ChatResponse {
                content: String::new(),
                tool_calls: calls,
                usage: Some(UsageStats {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                }),
            }),
        }
    }

    async fn chat_structured(
        &self,
        _messages: &[ChatMessage],
        _schema: &serde_json::Value,
        _options: &InferenceOptions,
    ) -> Result<serde_json::Value, AiError> {
        let response = self.next_response();
        match response {
            ChatOrToolResponse::Chat(text) => {
                serde_json::from_str(&text).map_err(|_| {
                    // If the mock response isn't valid JSON, return a default object
                    AiError::InferenceError("mock response is not valid JSON".to_string())
                })
            }
            ChatOrToolResponse::ToolCalls(_) => {
                Ok(serde_json::json!({"mock": true}))
            }
        }
    }

    async fn chat_with_tools(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        _options: &InferenceOptions,
    ) -> Result<ChatOrToolResponse, AiError> {
        Ok(self.next_response())
    }

    async fn health_check(&self) -> Result<BackendStatus, AiError> {
        Ok(BackendStatus {
            available: true,
            model_loaded: Some("mock-model".to_string()),
            error: None,
        })
    }

    async fn model_info(&self) -> Result<ModelInfo, AiError> {
        Ok(ModelInfo {
            name: "mock-model".to_string(),
            size_bytes: Some(0),
            quantization: Some("mock".to_string()),
            parameter_count: Some("0B".to_string()),
            context_length: Some(8192),
            family: Some("mock".to_string()),
        })
    }

    fn backend_name(&self) -> &str {
        "mock"
    }
}

#[async_trait]
impl EmbeddingProvider for MockBackend {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        Ok(self.deterministic_embedding(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        Ok(texts
            .iter()
            .map(|t| self.deterministic_embedding(t))
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.embedding_dims
    }

    fn model_name(&self) -> &str {
        "mock-embed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_backend_default_response() {
        let mock = MockBackend::new();
        let msgs = vec![ChatMessage::user("test")];
        let resp = mock.chat(&msgs, &InferenceOptions::default()).await.unwrap();
        assert_eq!(resp.content, "mock response");
    }

    #[tokio::test]
    async fn test_mock_backend_canned_responses() {
        let mock = MockBackend::new()
            .with_chat_response("first")
            .with_chat_response("second");

        let msgs = vec![ChatMessage::user("test")];
        let r1 = mock.chat(&msgs, &InferenceOptions::default()).await.unwrap();
        assert_eq!(r1.content, "first");

        let r2 = mock.chat(&msgs, &InferenceOptions::default()).await.unwrap();
        assert_eq!(r2.content, "second");

        // Should wrap around
        let r3 = mock.chat(&msgs, &InferenceOptions::default()).await.unwrap();
        assert_eq!(r3.content, "first");
    }

    #[tokio::test]
    async fn test_mock_backend_tool_response() {
        let tool_calls = vec![ToolCallResponse {
            id: "call_1".to_string(),
            name: "test_tool".to_string(),
            arguments: serde_json::json!({"key": "value"}),
        }];

        let mock = MockBackend::new().with_tool_response(tool_calls);
        let msgs = vec![ChatMessage::user("use tools")];
        let tools = vec![ToolDefinition::new("test_tool", "A test tool", serde_json::json!({}))];

        let resp = mock
            .chat_with_tools(&msgs, &tools, &InferenceOptions::default())
            .await
            .unwrap();

        match resp {
            ChatOrToolResponse::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "test_tool");
            }
            ChatOrToolResponse::Chat(_) => panic!("expected tool calls"),
        }
    }

    #[tokio::test]
    async fn test_mock_backend_health_check() {
        let mock = MockBackend::new();
        let status = mock.health_check().await.unwrap();
        assert!(status.available);
    }

    #[tokio::test]
    async fn test_mock_backend_model_info() {
        let mock = MockBackend::new();
        let info = mock.model_info().await.unwrap();
        assert_eq!(info.name, "mock-model");
    }

    #[tokio::test]
    async fn test_mock_embedding_deterministic() {
        let mock = MockBackend::new();
        let e1 = mock.embed("hello").await.unwrap();
        let e2 = mock.embed("hello").await.unwrap();
        assert_eq!(e1, e2, "same input should produce same embedding");

        let e3 = mock.embed("world").await.unwrap();
        assert_ne!(e1, e3, "different input should produce different embedding");
    }

    #[tokio::test]
    async fn test_mock_embedding_dimensions() {
        let mock = MockBackend::new();
        let embedding = mock.embed("test").await.unwrap();
        assert_eq!(embedding.len(), 768);
    }

    #[tokio::test]
    async fn test_mock_embedding_batch() {
        let mock = MockBackend::new();
        let texts = vec!["hello".to_string(), "world".to_string()];
        let embeddings = mock.embed_batch(&texts).await.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 768);
    }

    #[test]
    fn test_mock_backend_name() {
        let mock = MockBackend::new();
        assert_eq!(mock.backend_name(), "mock");
        assert_eq!(mock.model_name(), "mock-embed");
    }
}
