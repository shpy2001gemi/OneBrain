//! # Model Selector
//!
//! Selects the best model for a given device tier from the model catalog.
//! Filters by model type and minimum tier requirement, then picks the
//! most capable model that fits.

use crate::device::DeviceTier;
use crate::registry::schema::{ModelCatalog, ModelEntry, ModelType};

/// Selects appropriate models from the catalog based on device capabilities.
pub struct ModelSelector;

impl ModelSelector {
    /// Select the best LLM model for the given device tier.
    ///
    /// Returns the model with the highest `min_tier` that is ≤ the device tier.
    /// This ensures we pick the most capable model the hardware can run.
    pub fn select_llm(catalog: &ModelCatalog, tier: DeviceTier) -> Option<&ModelEntry> {
        Self::select_by_type(catalog, tier, ModelType::Llm)
    }

    /// Select the best embedding model for the given device tier.
    ///
    /// Returns the embedding model with the highest `min_tier` that is ≤ the device tier.
    pub fn select_embedding(catalog: &ModelCatalog, tier: DeviceTier) -> Option<&ModelEntry> {
        Self::select_by_type(catalog, tier, ModelType::Embedding)
    }

    /// Select the best model of a given type for the device tier.
    fn select_by_type(
        catalog: &ModelCatalog,
        tier: DeviceTier,
        model_type: ModelType,
    ) -> Option<&ModelEntry> {
        catalog
            .models
            .iter()
            .filter(|m| m.model_type == model_type && m.min_tier <= tier)
            .max_by_key(|m| m.min_tier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::schema::{ModelFeatures, ModelEntry};

    fn make_entry(
        id: &str,
        ollama_name: &str,
        min_tier: DeviceTier,
        model_type: ModelType,
    ) -> ModelEntry {
        ModelEntry {
            id: id.to_string(),
            display_name: id.to_string(),
            hf_repo: String::new(),
            hf_filename: String::new(),
            ollama_name: ollama_name.to_string(),
            size_bytes: 0,
            sha256: String::new(),
            min_tier,
            model_type,
            features: ModelFeatures {
                tool_calling: true,
                multilingual: false,
                context_length: 8192,
            },
        }
    }

    fn test_catalog() -> ModelCatalog {
        ModelCatalog {
            version: "1.0".to_string(),
            models: vec![
                make_entry("tiny-llm", "tiny:0.5b", DeviceTier::T0, ModelType::Llm),
                make_entry("small-llm", "small:3b", DeviceTier::T2, ModelType::Llm),
                make_entry("medium-llm", "medium:7b", DeviceTier::T3, ModelType::Llm),
                make_entry("large-llm", "large:14b", DeviceTier::T4, ModelType::Llm),
                make_entry("embed-small", "embed:small", DeviceTier::T0, ModelType::Embedding),
                make_entry("embed-large", "embed:large", DeviceTier::T3, ModelType::Embedding),
            ],
        }
    }

    #[test]
    fn test_select_llm_t0_gets_tiny() {
        let catalog = test_catalog();
        let model = ModelSelector::select_llm(&catalog, DeviceTier::T0);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "tiny-llm");
    }

    #[test]
    fn test_select_llm_t2_gets_small() {
        let catalog = test_catalog();
        let model = ModelSelector::select_llm(&catalog, DeviceTier::T2);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "small-llm");
    }

    #[test]
    fn test_select_llm_t3_gets_medium() {
        let catalog = test_catalog();
        let model = ModelSelector::select_llm(&catalog, DeviceTier::T3);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "medium-llm");
    }

    #[test]
    fn test_select_llm_t4_gets_large() {
        let catalog = test_catalog();
        let model = ModelSelector::select_llm(&catalog, DeviceTier::T4);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "large-llm");
    }

    #[test]
    fn test_select_llm_t6_gets_largest() {
        let catalog = test_catalog();
        let model = ModelSelector::select_llm(&catalog, DeviceTier::T6);
        assert!(model.is_some());
        // T6 can run all models, picks highest min_tier = T4 (large-llm)
        assert_eq!(model.unwrap().id, "large-llm");
    }

    #[test]
    fn test_select_embedding_t0() {
        let catalog = test_catalog();
        let model = ModelSelector::select_embedding(&catalog, DeviceTier::T0);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "embed-small");
    }

    #[test]
    fn test_select_embedding_t3() {
        let catalog = test_catalog();
        let model = ModelSelector::select_embedding(&catalog, DeviceTier::T3);
        assert!(model.is_some());
        assert_eq!(model.unwrap().id, "embed-large");
    }

    #[test]
    fn test_empty_catalog_returns_none() {
        let catalog = ModelCatalog {
            version: "1.0".to_string(),
            models: vec![],
        };
        assert!(ModelSelector::select_llm(&catalog, DeviceTier::T6).is_none());
        assert!(ModelSelector::select_embedding(&catalog, DeviceTier::T6).is_none());
    }
}
