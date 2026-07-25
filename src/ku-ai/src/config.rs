//! # AI Configuration
//!
//! TOML-based configuration for the AI layer, including backend selection,
//! model preferences, encoding parameters, and mediator settings.
//! Supports tier-aware defaults and persistent storage.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::device::DeviceTier;
use crate::error::AiError;

/// Top-level AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Whether the AI layer is enabled.
    pub enabled: bool,
    /// Active backend name (e.g. "ollama").
    pub backend: String,
    /// Ollama-specific configuration.
    pub ollama: OllamaConfig,
    /// Model selection preferences.
    pub models: ModelsConfig,
    /// Encoding inference parameters.
    pub encoding: EncodingConfig,
    /// Mediator behavior configuration.
    pub mediator: MediatorConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: "ollama".to_string(),
            ollama: OllamaConfig::default(),
            models: ModelsConfig::default(),
            encoding: EncodingConfig::default(),
            mediator: MediatorConfig::default(),
        }
    }
}

/// Ollama backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Ollama API base URL.
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            timeout_secs: 300,
        }
    }
}

/// Model selection preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// Override for the active LLM model name.
    pub active_llm: Option<String>,
    /// Override for the active embedding model name.
    pub active_embedding: Option<String>,
    /// Whether to automatically download missing models.
    pub auto_download: bool,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            active_llm: None,
            active_embedding: None,
            auto_download: true,
        }
    }
}

/// Encoding inference parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingConfig {
    /// Temperature for encoding inference (lower = more deterministic).
    pub temperature: f32,
    /// Maximum retries for failed encoding attempts.
    pub max_retries: u32,
    /// Minimum confidence threshold to accept an encoding.
    pub min_confidence: f64,
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            temperature: 0.1,
            max_retries: 2,
            min_confidence: 0.60,
        }
    }
}

/// Mediator behavior configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediatorConfig {
    /// Knowledge detection mode: "reactive" or "proactive".
    pub knowledge_detection: String,
    /// Maximum conversation history messages to retain.
    pub max_history: usize,
    /// Whether the graph agent is enabled.
    pub graph_agent_enabled: bool,
}

impl Default for MediatorConfig {
    fn default() -> Self {
        Self {
            knowledge_detection: "reactive".to_string(),
            max_history: 20,
            graph_agent_enabled: true,
        }
    }
}

impl AiConfig {
    /// Load configuration from a TOML file.
    ///
    /// Returns an error if the file doesn't exist or contains invalid TOML.
    pub fn load(path: &Path) -> Result<Self, AiError> {
        let content = std::fs::read_to_string(path).map_err(AiError::IoError)?;
        toml::from_str(&content).map_err(|e| AiError::ConfigError(e.to_string()))
    }

    /// Save configuration to a TOML file.
    ///
    /// Creates parent directories if they don't exist.
    pub fn save(&self, path: &Path) -> Result<(), AiError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(AiError::IoError)?;
        }
        let content =
            toml::to_string_pretty(self).map_err(|e| AiError::ConfigError(e.to_string()))?;
        std::fs::write(path, content).map_err(AiError::IoError)
    }

    /// Generate a default configuration tuned for the given device tier.
    ///
    /// Selects appropriate models based on available hardware:
    /// - T0–T1: Small models (qwen2.5:0.5b, nomic-embed-text)
    /// - T2–T3: Medium models (qwen2.5:3b, nomic-embed-text)
    /// - T4–T5: Large models (qwen2.5:14b, nomic-embed-text)
    /// - T6: Server models (qwen2.5:14b, nomic-embed-text)
    pub fn default_for_tier(tier: DeviceTier) -> Self {
        let (llm, embedding) = match tier {
            DeviceTier::T0 | DeviceTier::T1 => ("qwen2.5:0.5b", "nomic-embed-text"),
            DeviceTier::T2 | DeviceTier::T3 => ("qwen2.5:3b", "nomic-embed-text"),
            DeviceTier::T4 | DeviceTier::T5 => ("qwen2.5:14b", "nomic-embed-text"),
            DeviceTier::T6 => ("qwen2.5:14b", "nomic-embed-text"),
        };

        Self {
            models: ModelsConfig {
                active_llm: Some(llm.to_string()),
                active_embedding: Some(embedding.to_string()),
                auto_download: true,
            },
            ..Default::default()
        }
    }

    /// Return the default configuration file path.
    ///
    /// Uses the `directories` crate to find the user's config directory:
    /// - **Windows**: `%APPDATA%/OneBrain/ai_config.toml`
    /// - **macOS**: `~/Library/Application Support/OneBrain/ai_config.toml`
    /// - **Linux**: `~/.config/OneBrain/ai_config.toml`
    pub fn default_config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "OneBrain", "OneBrain")
            .map(|dirs| dirs.config_dir().join("ai_config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AiConfig::default();
        assert!(config.enabled);
        assert_eq!(config.backend, "ollama");
        assert_eq!(config.ollama.base_url, "http://localhost:11434");
        assert_eq!(config.ollama.timeout_secs, 300);
        assert!(config.models.auto_download);
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = AiConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("serialization should succeed");
        let back: AiConfig = toml::from_str(&toml_str).expect("deserialization should succeed");
        assert_eq!(back.backend, "ollama");
        assert_eq!(back.ollama.timeout_secs, 300);
        assert!((back.encoding.temperature - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn test_default_for_tier_t0() {
        let config = AiConfig::default_for_tier(DeviceTier::T0);
        assert_eq!(config.models.active_llm.as_deref(), Some("qwen2.5:0.5b"));
    }

    #[test]
    fn test_default_for_tier_t3() {
        let config = AiConfig::default_for_tier(DeviceTier::T3);
        assert_eq!(config.models.active_llm.as_deref(), Some("qwen2.5:3b"));
    }

    #[test]
    fn test_default_for_tier_t4() {
        let config = AiConfig::default_for_tier(DeviceTier::T4);
        assert_eq!(config.models.active_llm.as_deref(), Some("qwen2.5:14b"));
    }

    #[test]
    fn test_default_for_tier_t6() {
        let config = AiConfig::default_for_tier(DeviceTier::T6);
        assert_eq!(config.models.active_llm.as_deref(), Some("qwen2.5:14b"));
    }

    #[test]
    fn test_default_config_path_returns_some() {
        // Should return a valid path on any desktop OS
        let path = AiConfig::default_config_path();
        assert!(path.is_some(), "should find a config directory");
        let p = path.unwrap();
        assert!(p.to_string_lossy().contains("ai_config.toml"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("ku_ai_test_config");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_config.toml");

        let config = AiConfig::default_for_tier(DeviceTier::T3);
        config.save(&path).expect("save should succeed");

        let loaded = AiConfig::load(&path).expect("load should succeed");
        assert_eq!(loaded.models.active_llm.as_deref(), Some("qwen2.5:3b"));
        assert_eq!(loaded.backend, "ollama");

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
