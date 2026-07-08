//! # Model Registry
//!
//! Manages the curated model catalog, loaded at compile time from the
//! embedded `registry.json` file. Provides lookup by ID and Ollama name.

pub mod schema;
pub mod selector;

pub use schema::{ModelCatalog, ModelEntry, ModelFeatures, ModelType};
pub use selector::ModelSelector;

use crate::error::AiError;

/// Embedded model catalog JSON, included at compile time.
const REGISTRY_JSON: &str = include_str!("../../registry.json");

/// In-memory model registry backed by the embedded catalog.
pub struct ModelRegistry {
    /// The loaded model catalog.
    pub catalog: ModelCatalog,
}

impl ModelRegistry {
    /// Load the registry from the embedded JSON catalog.
    ///
    /// This is a compile-time embedded resource, so it never fails at runtime
    /// unless the JSON was malformed at build time.
    pub fn load() -> Result<Self, AiError> {
        let catalog: ModelCatalog =
            serde_json::from_str(REGISTRY_JSON).map_err(AiError::JsonError)?;
        Ok(Self { catalog })
    }

    /// Find a model entry by its unique ID.
    pub fn find_by_id(&self, id: &str) -> Option<&ModelEntry> {
        self.catalog.models.iter().find(|m| m.id == id)
    }

    /// Find a model entry by its Ollama model name/tag.
    pub fn find_by_ollama_name(&self, name: &str) -> Option<&ModelEntry> {
        self.catalog.models.iter().find(|m| m.ollama_name == name)
    }

    /// List all models in the catalog.
    pub fn list_all(&self) -> &[ModelEntry] {
        &self.catalog.models
    }

    /// Return the number of models in the catalog.
    pub fn len(&self) -> usize {
        self.catalog.models.len()
    }

    /// Check if the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.catalog.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_loads_successfully() {
        let registry = ModelRegistry::load().expect("registry should load");
        assert!(!registry.is_empty(), "catalog should not be empty");
    }

    #[test]
    fn test_registry_has_expected_models() {
        let registry = ModelRegistry::load().unwrap();
        assert!(registry.len() >= 8, "should have at least 8 models");
    }

    #[test]
    fn test_find_by_id() {
        let registry = ModelRegistry::load().unwrap();
        let model = registry.find_by_id("qwen2.5-3b-q4");
        assert!(model.is_some());
        assert_eq!(model.unwrap().ollama_name, "qwen2.5:3b");
    }

    #[test]
    fn test_find_by_ollama_name() {
        let registry = ModelRegistry::load().unwrap();
        let model = registry.find_by_ollama_name("nomic-embed-text");
        assert!(model.is_some());
        assert_eq!(model.unwrap().model_type, ModelType::Embedding);
    }

    #[test]
    fn test_find_nonexistent_returns_none() {
        let registry = ModelRegistry::load().unwrap();
        assert!(registry.find_by_id("nonexistent-model").is_none());
        assert!(registry.find_by_ollama_name("nonexistent:latest").is_none());
    }

    #[test]
    fn test_list_all() {
        let registry = ModelRegistry::load().unwrap();
        let all = registry.list_all();
        assert_eq!(all.len(), registry.len());
    }
}
