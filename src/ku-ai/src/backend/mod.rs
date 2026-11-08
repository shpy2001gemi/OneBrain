//! # Backend Module
//!
//! Re-exports backend implementations and provides a factory function
//! for creating backends from configuration.

pub mod mock;
#[cfg(feature = "ollama")]
pub mod ollama;
pub mod unavailable;

pub use mock::MockBackend;
#[cfg(feature = "ollama")]
pub use ollama::OllamaBackend;
pub use unavailable::UnavailableBackend;

use crate::config::AiConfig;
use crate::error::AiError;

/// Create a backend from the given configuration.
///
/// Currently supports:
/// - `"ollama"` — creates an `OllamaBackend` connected to the configured URL
///   (requires the `ollama` cargo feature; otherwise an explicit config error).
/// - `"mock"` — creates a [`MockBackend`] for testing.
///
/// Returns an error for unknown backend names.
pub fn create_backend(config: &AiConfig) -> Result<Box<dyn crate::traits::ModelBackend>, AiError> {
    match config.backend.as_str() {
        #[cfg(feature = "ollama")]
        "ollama" => {
            let llm = config.models.active_llm.as_deref().unwrap_or("qwen2.5:3b");
            let embed = config
                .models
                .active_embedding
                .as_deref()
                .unwrap_or("nomic-embed-text");
            let backend = OllamaBackend::new(
                &config.ollama.base_url,
                llm,
                embed,
                config.ollama.timeout_secs,
            )?;
            Ok(Box::new(backend))
        }
        #[cfg(not(feature = "ollama"))]
        "ollama" => Err(AiError::ConfigError(
            "the ollama backend is not compiled into this build (enable the ku-ai `ollama` feature)"
                .to_string(),
        )),
        "mock" => Ok(Box::new(MockBackend::new())),
        other => Err(AiError::ConfigError(format!(
            "unknown backend: '{}' (supported: ollama, mock)",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "ollama")]
    #[test]
    fn test_create_backend_ollama() {
        let config = AiConfig::default();
        let backend = create_backend(&config);
        assert!(backend.is_ok());
        assert_eq!(backend.unwrap().backend_name(), "ollama");
    }

    #[cfg(not(feature = "ollama"))]
    #[test]
    fn test_create_backend_ollama_without_feature_is_explicit_error() {
        let config = AiConfig::default();
        assert!(matches!(
            create_backend(&config),
            Err(AiError::ConfigError(message)) if message.contains("not compiled")
        ));
    }

    #[test]
    fn test_create_backend_mock() {
        let config = AiConfig {
            backend: "mock".to_string(),
            ..AiConfig::default()
        };
        let backend = create_backend(&config);
        assert!(backend.is_ok());
        assert_eq!(backend.unwrap().backend_name(), "mock");
    }

    #[test]
    fn test_create_backend_unknown() {
        let config = AiConfig {
            backend: "unknown".to_string(),
            ..AiConfig::default()
        };
        let backend = create_backend(&config);
        assert!(backend.is_err());
    }
}
