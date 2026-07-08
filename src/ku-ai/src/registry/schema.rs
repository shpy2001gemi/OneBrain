//! # Model Registry Schema
//!
//! Data structures for the curated model catalog, including model entries,
//! types, features, and tier requirements.

use serde::{Deserialize, Serialize};

use crate::device::DeviceTier;

/// The complete model catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCatalog {
    /// Catalog schema version.
    pub version: String,
    /// List of available models.
    pub models: Vec<ModelEntry>,
}

/// A single model entry in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Unique model identifier (e.g. "qwen2.5-3b-q4").
    pub id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// HuggingFace repository (for GGUF download).
    pub hf_repo: String,
    /// HuggingFace filename within the repo.
    pub hf_filename: String,
    /// Ollama model tag (e.g. "qwen2.5:3b").
    pub ollama_name: String,
    /// Model file size in bytes.
    pub size_bytes: u64,
    /// SHA-256 checksum of the model file.
    pub sha256: String,
    /// Minimum device tier required to load this model.
    pub min_tier: DeviceTier,
    /// Type of model (LLM or embedding).
    pub model_type: ModelType,
    /// Feature flags for this model.
    pub features: ModelFeatures,
}

/// Type of model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelType {
    /// Large Language Model for text generation.
    Llm,
    /// Embedding model for vector representations.
    Embedding,
}

/// Feature flags describing model capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFeatures {
    /// Whether the model supports tool/function calling.
    pub tool_calling: bool,
    /// Whether the model supports multiple languages.
    pub multilingual: bool,
    /// Maximum context length in tokens.
    pub context_length: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_serde() {
        let llm = ModelType::Llm;
        let json = serde_json::to_string(&llm).unwrap();
        assert_eq!(json, r#""llm""#);

        let embedding = ModelType::Embedding;
        let json = serde_json::to_string(&embedding).unwrap();
        assert_eq!(json, r#""embedding""#);
    }

    #[test]
    fn test_model_entry_deserialization() {
        let json = r#"{
            "id": "test-model",
            "display_name": "Test Model",
            "hf_repo": "org/model",
            "hf_filename": "model.gguf",
            "ollama_name": "test:latest",
            "size_bytes": 1000000,
            "sha256": "abc123",
            "min_tier": "T1",
            "model_type": "llm",
            "features": {
                "tool_calling": true,
                "multilingual": false,
                "context_length": 8192
            }
        }"#;
        let entry: ModelEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, "test-model");
        assert_eq!(entry.min_tier, DeviceTier::T1);
        assert_eq!(entry.model_type, ModelType::Llm);
        assert!(entry.features.tool_calling);
    }

    #[test]
    fn test_model_catalog_roundtrip() {
        let catalog = ModelCatalog {
            version: "1.0".to_string(),
            models: vec![],
        };
        let json = serde_json::to_string(&catalog).unwrap();
        let back: ModelCatalog = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, "1.0");
        assert!(back.models.is_empty());
    }
}
