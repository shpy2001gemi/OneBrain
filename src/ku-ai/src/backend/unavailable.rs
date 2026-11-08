//! # Unavailable Backend
//!
//! A [`ModelBackend`] that honestly reports that no LLM host adapter is
//! compiled into the current build (ARCH-09). It never fabricates a response:
//! every call returns [`AiError::BackendUnavailable`] with the given reason.
//! Runtime-core builds without the Ollama HTTP adapter use it so chat surfaces
//! fail explicitly instead of silently using a mock.

use async_trait::async_trait;

use crate::error::AiError;
use crate::traits::ModelBackend;
use crate::types::*;

/// Backend placeholder for builds that contain no LLM host adapter.
#[derive(Debug, Clone)]
pub struct UnavailableBackend {
    reason: String,
}

impl UnavailableBackend {
    /// Create a placeholder that reports `reason` on every call.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// The reason reported on every call.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    fn unavailable(&self) -> AiError {
        AiError::BackendUnavailable(self.reason.clone())
    }
}

#[async_trait]
impl ModelBackend for UnavailableBackend {
    async fn chat(
        &self,
        _messages: &[ChatMessage],
        _options: &InferenceOptions,
    ) -> Result<ChatResponse, AiError> {
        Err(self.unavailable())
    }

    async fn chat_structured(
        &self,
        _messages: &[ChatMessage],
        _schema: &serde_json::Value,
        _options: &InferenceOptions,
    ) -> Result<serde_json::Value, AiError> {
        Err(self.unavailable())
    }

    async fn chat_with_tools(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolDefinition],
        _options: &InferenceOptions,
    ) -> Result<ChatOrToolResponse, AiError> {
        Err(self.unavailable())
    }

    async fn health_check(&self) -> Result<BackendStatus, AiError> {
        Err(self.unavailable())
    }

    async fn model_info(&self) -> Result<ModelInfo, AiError> {
        Err(self.unavailable())
    }

    fn backend_name(&self) -> &str {
        "unavailable"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REASON: &str = "no LLM host adapter";

    fn is_reason(error: AiError) -> bool {
        matches!(error, AiError::BackendUnavailable(ref reason) if reason == REASON)
    }

    #[tokio::test]
    async fn every_call_reports_the_reason_and_never_fabricates_output() {
        let backend = UnavailableBackend::new(REASON);
        assert_eq!(backend.backend_name(), "unavailable");
        assert_eq!(backend.reason(), REASON);
        let messages = [ChatMessage::user("hello")];
        let options = InferenceOptions::default();
        let schema = serde_json::json!({});
        let chat = backend.chat(&messages, &options).await;
        let structured = backend.chat_structured(&messages, &schema, &options).await;
        let tools = backend.chat_with_tools(&messages, &[], &options).await;
        assert!(is_reason(chat.unwrap_err()));
        assert!(is_reason(structured.unwrap_err()));
        assert!(is_reason(tools.unwrap_err()));
        assert!(is_reason(backend.health_check().await.unwrap_err()));
        assert!(is_reason(backend.model_info().await.unwrap_err()));
    }
}
