//! ARCH-09: LLM host adapter selection for the chat mediator.
//!
//! The runtime core depends only on the `ku_ai::ModelBackend` trait. The local
//! Ollama HTTP adapter (reqwest) is a host concern compiled with the
//! `ollama-host` feature, which the API/CLI/Desktop hosts enable. A core build
//! without it gets an [`UnavailableBackend`](ku_ai::UnavailableBackend) that
//! reports [`NO_LLM_HOST_ADAPTER`] on every call instead of fabricating chat
//! output or silently falling back to a mock.

use ku_ai::traits::ModelBackend;

use crate::error::NodeError;

/// Reason reported by chat backends in builds without an LLM host adapter.
pub const NO_LLM_HOST_ADAPTER: &str = "no LLM host adapter is compiled into this build; \
     enable the onebrain-node `ollama-host` feature (the API/CLI/Desktop hosts do)";

/// Embedding model paired with the configured chat model (unchanged default).
const EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Whether this build contains the local Ollama host adapter.
pub const fn has_llm_host_adapter() -> bool {
    cfg!(feature = "ollama-host")
}

/// Build the chat/encoder backend for `model` at `ollama_url`.
#[cfg(feature = "ollama-host")]
pub fn model_backend(
    ollama_url: &str,
    model: &str,
    timeout_secs: u64,
) -> Result<Box<dyn ModelBackend>, NodeError> {
    let backend = ku_ai::OllamaBackend::new(ollama_url, model, EMBEDDING_MODEL, timeout_secs)
        .map_err(NodeError::Ai)?;
    Ok(Box::new(backend))
}

/// Build the chat/encoder backend; without `ollama-host` it is unavailable.
#[cfg(not(feature = "ollama-host"))]
pub fn model_backend(
    ollama_url: &str,
    model: &str,
    timeout_secs: u64,
) -> Result<Box<dyn ModelBackend>, NodeError> {
    let _ = (ollama_url, model, timeout_secs, EMBEDDING_MODEL);
    Ok(Box::new(ku_ai::UnavailableBackend::new(
        NO_LLM_HOST_ADAPTER,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_matches_compiled_host_adapter() {
        let backend = model_backend("http://127.0.0.1:11434", "qwen2.5:3b", 5)
            .expect("backend construction does not contact the host");
        if has_llm_host_adapter() {
            assert_eq!(backend.backend_name(), "ollama");
        } else {
            assert_eq!(backend.backend_name(), "unavailable");
        }
    }

    #[cfg(not(feature = "ollama-host"))]
    #[tokio::test]
    async fn core_build_chat_reports_missing_host_adapter() {
        let backend = model_backend("http://127.0.0.1:11434", "qwen2.5:3b", 5).unwrap();
        let error = backend
            .chat(
                &[ku_ai::ChatMessage::user("hello")],
                &ku_ai::InferenceOptions::default(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, ku_ai::AiError::BackendUnavailable(ref reason) if reason == NO_LLM_HOST_ADAPTER)
        );
    }
}
