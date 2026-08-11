//! # Backend Module
//!
//! Re-exports backend implementations and provides a factory function
//! for creating backends from configuration.

pub mod mock;
pub mod ollama;

pub use mock::MockBackend;
pub use ollama::OllamaBackend;

use crate::config::AiConfig;
use crate::error::AiError;

/// Create a backend from the given configuration.
///
/// Currently supports:
/// - `"ollama"` — creates an [`OllamaBackend`] connected to the configured URL.
/// - `"mock"` — creates a [`MockBackend`] for testing.
///
/// Returns an error for unknown backend names.
pub fn create_backend(config: &AiConfig) -> Result<Box<dyn crate::traits::ModelBackend>, AiError> {
    match config.backend.as_str() {
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

    #[test]
    fn test_create_backend_ollama() {
        let config = AiConfig::default();
        let backend = create_backend(&config);
        assert!(backend.is_ok());
        assert_eq!(backend.unwrap().backend_name(), "ollama");
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
