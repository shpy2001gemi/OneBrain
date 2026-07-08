//! # AI Trait Abstractions
//!
//! Async trait interfaces for model backends and embedding providers.
//! These traits decouple the AI layer from any specific backend implementation.

use async_trait::async_trait;
use crate::error::AiError;
use crate::types::*;

/// Trait for a model backend that supports chat, structured output, and tool calling.
///
/// Implementations include [`OllamaBackend`](crate::backend::OllamaBackend) for local Ollama
/// and [`MockBackend`](crate::backend::MockBackend) for testing.
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Send a chat completion request and receive a response.
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &InferenceOptions,
    ) -> Result<ChatResponse, AiError>;

    /// Send a chat request with a JSON Schema for structured output.
    ///
    /// The response is guaranteed to conform to the provided schema.
    async fn chat_structured(
        &self,
        messages: &[ChatMessage],
        schema: &serde_json::Value,
        options: &InferenceOptions,
    ) -> Result<serde_json::Value, AiError>;

    /// Send a chat request with tool definitions, allowing the model to request tool calls.
    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        options: &InferenceOptions,
    ) -> Result<ChatOrToolResponse, AiError>;

    /// Check whether the backend is healthy and reachable.
    async fn health_check(&self) -> Result<BackendStatus, AiError>;

    /// Retrieve metadata about the currently loaded model.
    async fn model_info(&self) -> Result<ModelInfo, AiError>;

    /// Return the human-readable name of this backend (e.g. "ollama", "mock").
    fn backend_name(&self) -> &str;
}

/// Trait for producing vector embeddings from text.
///
/// Used for semantic search, knowledge graph embeddings, and similarity computation.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a single text string into a dense vector.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError>;

    /// Embed a batch of text strings into dense vectors.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError>;

    /// Return the dimensionality of the embedding vectors.
    fn dimensions(&self) -> usize;

    /// Return the name of the embedding model.
    fn model_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify that the traits are object-safe (can be used as dyn trait objects)
    #[test]
    fn test_model_backend_is_object_safe() {
        fn _assert_object_safe(_: &dyn ModelBackend) {}
    }

    #[test]
    fn test_embedding_provider_is_object_safe() {
        fn _assert_object_safe(_: &dyn EmbeddingProvider) {}
    }
}
