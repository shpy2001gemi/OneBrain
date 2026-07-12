//! # Ollama Backend
//!
//! Full implementation of [`ModelBackend`] and [`EmbeddingProvider`] traits
//! for the [Ollama](https://ollama.ai/) local LLM runtime.
//!
//! Communicates with Ollama's REST API using non-streaming mode.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::AiError;
use crate::traits::{EmbeddingProvider, ModelBackend};
use crate::types::*;

/// Ollama backend implementation.
///
/// Connects to a locally running Ollama instance via its REST API.
pub struct OllamaBackend {
    client: Client,
    base_url: String,
    llm_model: String,
    embedding_model: String,
    timeout: Duration,
    /// Embedding vector dimensions (default: 768 for nomic-embed-text).
    embedding_dimensions: usize,
}

impl OllamaBackend {
    /// Create a new Ollama backend.
    ///
    /// # Arguments
    /// * `base_url` — Ollama API base URL (e.g. "http://localhost:11434").
    /// * `llm_model` — Model name for chat completions (e.g. "qwen2.5:3b").
    /// * `embedding_model` — Model name for embeddings (e.g. "nomic-embed-text").
    /// * `timeout_secs` — Request timeout in seconds.
    pub fn new(
        base_url: impl Into<String>,
        llm_model: impl Into<String>,
        embedding_model: impl Into<String>,
        timeout_secs: u64,
    ) -> Result<Self, AiError> {
        let timeout = Duration::from_secs(timeout_secs);
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(AiError::HttpError)?;

        Ok(Self {
            client,
            base_url: base_url.into(),
            llm_model: llm_model.into(),
            embedding_model: embedding_model.into(),
            timeout,
            embedding_dimensions: 768,
        })
    }

    /// Set the embedding dimensions.
    ///
    /// Different embedding models produce different dimensionalities.
    /// Default is 768 (nomic-embed-text). Other common values:
    /// - 384 (all-MiniLM-L6-v2)
    /// - 1024 (mxbai-embed-large)
    /// - 1536 (text-embedding-3-small)
    pub fn with_embedding_dimensions(mut self, dims: usize) -> Self {
        self.embedding_dimensions = dims;
        self
    }

    /// Build the full URL for an API endpoint.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Convert our ChatMessage format to Ollama's API format.
    fn to_ollama_messages(messages: &[ChatMessage]) -> Vec<OllamaChatMessage> {
        messages
            .iter()
            .map(|m| OllamaChatMessage {
                role: match m.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
            })
            .collect()
    }

    /// Build Ollama options from our InferenceOptions.
    fn to_ollama_options(options: &InferenceOptions) -> OllamaOptions {
        OllamaOptions {
            temperature: Some(options.temperature),
            num_predict: options.max_tokens.map(|t| t as i64),
            top_p: options.top_p,
            seed: options.seed.map(|s| s as i64),
            stop: if options.stop.is_empty() {
                None
            } else {
                Some(options.stop.clone())
            },
        }
    }

    /// Parse Ollama tool calls from the response.
    fn parse_tool_calls(
        tool_calls: Option<Vec<OllamaToolCall>>,
    ) -> Vec<ToolCallResponse> {
        tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, tc)| ToolCallResponse {
                id: format!("call_{}", i),
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect()
    }
}

// ─── Ollama API Request/Response Structs ────────────────────────────────

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    /// Disable thinking/reasoning mode for qwen3 models.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaChatResponseMessage>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaModelTag>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaModelTag {
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Serialize)]
struct OllamaShowRequest {
    model: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaShowResponse {
    #[serde(default)]
    modelfile: String,
    #[serde(default)]
    parameters: String,
    details: Option<OllamaShowDetails>,
}

#[derive(Debug, Deserialize)]
struct OllamaShowDetails {
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    parameter_size: Option<String>,
    #[serde(default)]
    quantization_level: Option<String>,
}

// ─── ModelBackend Implementation ────────────────────────────────────────

#[async_trait]
impl ModelBackend for OllamaBackend {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        options: &InferenceOptions,
    ) -> Result<ChatResponse, AiError> {
        let request = OllamaChatRequest {
            model: self.llm_model.clone(),
            messages: Self::to_ollama_messages(messages),
            stream: false,
            think: Some(false),
            options: Some(Self::to_ollama_options(options)),
            tools: None,
            format: None,
        };

        let resp = self
            .client
            .post(self.url("/api/chat"))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout(self.timeout.as_secs())
                } else if e.is_connect() {
                    AiError::BackendUnavailable(format!("cannot connect to Ollama at {}: {}", self.base_url, e))
                } else {
                    AiError::HttpError(e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::InferenceError(format!(
                "Ollama returned HTTP {}: {}",
                status, body
            )));
        }

        let ollama_resp: OllamaChatResponse = resp.json().await.map_err(AiError::HttpError)?;
        let msg = ollama_resp.message.unwrap_or(OllamaChatResponseMessage {
            content: String::new(),
            tool_calls: None,
        });

        let prompt_tokens = ollama_resp.prompt_eval_count.unwrap_or(0);
        let completion_tokens = ollama_resp.eval_count.unwrap_or(0);

        Ok(ChatResponse {
            content: msg.content,
            tool_calls: Self::parse_tool_calls(msg.tool_calls),
            usage: Some(UsageStats {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }),
        })
    }

    async fn chat_structured(
        &self,
        messages: &[ChatMessage],
        schema: &serde_json::Value,
        options: &InferenceOptions,
    ) -> Result<serde_json::Value, AiError> {
        let request = OllamaChatRequest {
            model: self.llm_model.clone(),
            messages: Self::to_ollama_messages(messages),
            stream: false,
            think: Some(false),
            options: Some(Self::to_ollama_options(options)),
            tools: None,
            format: Some(schema.clone()),
        };

        let resp = self
            .client
            .post(self.url("/api/chat"))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout(self.timeout.as_secs())
                } else if e.is_connect() {
                    AiError::BackendUnavailable(format!("cannot connect to Ollama at {}: {}", self.base_url, e))
                } else {
                    AiError::HttpError(e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::InferenceError(format!(
                "Ollama structured output returned HTTP {}: {}",
                status, body
            )));
        }

        let ollama_resp: OllamaChatResponse = resp.json().await.map_err(AiError::HttpError)?;
        let content = ollama_resp
            .message
            .map(|m| m.content)
            .unwrap_or_default();

        serde_json::from_str(&content).map_err(|e| {
            AiError::InferenceError(format!(
                "failed to parse structured output as JSON: {} — raw: {}",
                e, content
            ))
        })
    }

    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        options: &InferenceOptions,
    ) -> Result<ChatOrToolResponse, AiError> {
        let tools_json: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| serde_json::to_value(t).unwrap_or_default())
            .collect();

        let request = OllamaChatRequest {
            model: self.llm_model.clone(),
            messages: Self::to_ollama_messages(messages),
            stream: false,
            think: Some(false),
            options: Some(Self::to_ollama_options(options)),
            tools: Some(tools_json),
            format: None,
        };

        let resp = self
            .client
            .post(self.url("/api/chat"))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout(self.timeout.as_secs())
                } else if e.is_connect() {
                    AiError::BackendUnavailable(format!("cannot connect to Ollama at {}: {}", self.base_url, e))
                } else {
                    AiError::HttpError(e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::ToolCallingError(format!(
                "Ollama tool call returned HTTP {}: {}",
                status, body
            )));
        }

        let ollama_resp: OllamaChatResponse = resp.json().await.map_err(AiError::HttpError)?;
        let msg = ollama_resp.message.unwrap_or(OllamaChatResponseMessage {
            content: String::new(),
            tool_calls: None,
        });

        let tool_calls = Self::parse_tool_calls(msg.tool_calls);
        if tool_calls.is_empty() {
            Ok(ChatOrToolResponse::Chat(msg.content))
        } else {
            Ok(ChatOrToolResponse::ToolCalls(tool_calls))
        }
    }

    async fn health_check(&self) -> Result<BackendStatus, AiError> {
        let resp = self
            .client
            .get(self.url("/api/tags"))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let tags: OllamaTagsResponse = r.json().await.unwrap_or(OllamaTagsResponse {
                    models: None,
                });
                let model_loaded = tags
                    .models
                    .as_ref()
                    .and_then(|models| {
                        models
                            .iter()
                            .find(|m| m.name.starts_with(&self.llm_model))
                            .map(|m| m.name.clone())
                    });
                Ok(BackendStatus {
                    available: true,
                    model_loaded,
                    error: None,
                })
            }
            Ok(r) => Ok(BackendStatus {
                available: false,
                model_loaded: None,
                error: Some(format!("HTTP {}", r.status())),
            }),
            Err(e) => Ok(BackendStatus {
                available: false,
                model_loaded: None,
                error: Some(e.to_string()),
            }),
        }
    }

    async fn model_info(&self) -> Result<ModelInfo, AiError> {
        let request = OllamaShowRequest {
            model: self.llm_model.clone(),
        };

        let resp = self
            .client
            .post(self.url("/api/show"))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    AiError::BackendUnavailable(format!("cannot connect to Ollama: {}", e))
                } else {
                    AiError::HttpError(e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::ModelNotFound(format!(
                "model '{}' not found (HTTP {}): {}",
                self.llm_model, status, body
            )));
        }

        let show: OllamaShowResponse = resp.json().await.map_err(AiError::HttpError)?;
        let details = show.details.unwrap_or(OllamaShowDetails {
            family: None,
            parameter_size: None,
            quantization_level: None,
        });

        Ok(ModelInfo {
            name: self.llm_model.clone(),
            size_bytes: None,
            quantization: details.quantization_level,
            parameter_count: details.parameter_size,
            context_length: None,
            family: details.family,
        })
    }

    fn backend_name(&self) -> &str {
        "ollama"
    }
}

// ─── EmbeddingProvider Implementation ───────────────────────────────────

#[async_trait]
impl EmbeddingProvider for OllamaBackend {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, AiError> {
        let results = self.embed_batch(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| AiError::InferenceError("empty embedding response".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AiError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = OllamaEmbedRequest {
            model: self.embedding_model.clone(),
            input: texts.to_vec(),
        };

        let resp = self
            .client
            .post(self.url("/api/embed"))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AiError::Timeout(self.timeout.as_secs())
                } else if e.is_connect() {
                    AiError::BackendUnavailable(format!("cannot connect to Ollama: {}", e))
                } else {
                    AiError::HttpError(e)
                }
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AiError::InferenceError(format!(
                "Ollama embed returned HTTP {}: {}",
                status, body
            )));
        }

        let embed_resp: OllamaEmbedResponse = resp.json().await.map_err(AiError::HttpError)?;
        Ok(embed_resp.embeddings)
    }

    fn dimensions(&self) -> usize {
        self.embedding_dimensions
    }

    fn model_name(&self) -> &str {
        &self.embedding_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_backend_creation() {
        let backend = OllamaBackend::new(
            "http://localhost:11434",
            "qwen2.5:3b",
            "nomic-embed-text",
            120,
        );
        assert!(backend.is_ok());
        let backend = backend.unwrap();
        assert_eq!(backend.backend_name(), "ollama");
        assert_eq!(backend.dimensions(), 768);
        assert_eq!(backend.model_name(), "nomic-embed-text");
    }

    #[test]
    fn test_custom_embedding_dimensions() {
        let backend = OllamaBackend::new(
            "http://localhost:11434",
            "qwen2.5:3b",
            "mxbai-embed-large",
            120,
        )
        .unwrap()
        .with_embedding_dimensions(1024);
        assert_eq!(backend.dimensions(), 1024);
    }

    #[test]
    fn test_to_ollama_messages() {
        let messages = vec![
            ChatMessage::system("Be helpful"),
            ChatMessage::user("Hello"),
        ];
        let ollama_msgs = OllamaBackend::to_ollama_messages(&messages);
        assert_eq!(ollama_msgs.len(), 2);
        assert_eq!(ollama_msgs[0].role, "system");
        assert_eq!(ollama_msgs[1].role, "user");
    }

    #[test]
    fn test_to_ollama_options_default() {
        let opts = InferenceOptions::default();
        let ollama_opts = OllamaBackend::to_ollama_options(&opts);
        assert!((ollama_opts.temperature.unwrap() - 0.1).abs() < f32::EPSILON);
        assert!(ollama_opts.num_predict.is_none());
    }

    #[test]
    fn test_parse_tool_calls_empty() {
        let calls = OllamaBackend::parse_tool_calls(None);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_with_data() {
        let calls = OllamaBackend::parse_tool_calls(Some(vec![OllamaToolCall {
            function: OllamaToolCallFunction {
                name: "get_weather".to_string(),
                arguments: serde_json::json!({"city": "Tokyo"}),
            },
        }]));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].id, "call_0");
    }

    #[test]
    fn test_url_construction() {
        let backend = OllamaBackend::new(
            "http://localhost:11434",
            "test",
            "test",
            60,
        )
        .unwrap();
        assert_eq!(backend.url("/api/chat"), "http://localhost:11434/api/chat");
    }
}
