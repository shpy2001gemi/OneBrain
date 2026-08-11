//! Node configuration.
//!
//! Defines the runtime configuration for an OneBrain node instance,
//! including network port, data paths, Ollama settings, and seed peers.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use crate::vnext_config::VNextFeatureConfig;

/// Controls whether the node may fall back to the legacy encoder when the
/// external Concept Registry is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConceptRegistryMode {
    /// Registry load failures stop node initialization before side effects.
    Required,
    /// Registry load failures are exposed in status and encoder v1 is used.
    #[default]
    Optional,
    /// Do not attempt to open the registry; encoder v1 is selected explicitly.
    Disabled,
}

impl std::fmt::Display for ConceptRegistryMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Disabled => "disabled",
        })
    }
}

impl FromStr for ConceptRegistryMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "required" => Ok(Self::Required),
            "optional" => Ok(Self::Optional),
            "disabled" => Ok(Self::Disabled),
            _ => Err(format!(
                "invalid concept registry mode '{value}'; expected required, optional, or disabled"
            )),
        }
    }
}

/// Configuration for an OneBrain node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node display name.
    pub name: String,
    /// Port to listen on.
    pub port: u16,
    /// Data directory for persistent storage.
    pub data_dir: PathBuf,
    /// Ollama API URL.
    pub ollama_url: String,
    /// Ollama model name.
    pub model: String,
    /// Seed peer addresses for initial discovery.
    pub seeds: Vec<SocketAddr>,
    /// Explicit Concept Registry path. When absent, use data_dir/concepts.obr.
    #[serde(default)]
    pub concept_registry_path: Option<PathBuf>,
    /// Whether registry failures are fatal, optional, or deliberately disabled.
    #[serde(default)]
    pub concept_registry_mode: ConceptRegistryMode,
    /// Maximum number of on-demand label resolutions retained in memory.
    #[serde(default = "default_concept_registry_cache_capacity")]
    pub concept_registry_cache_capacity: usize,
    /// Root containing signed immutable Concept Registry releases and state.
    /// When set, `concept_registry_path` must be absent.
    #[serde(default)]
    pub concept_registry_release_root: Option<PathBuf>,
    /// Pinned Ed25519 release-signing public key as 64 lowercase hex digits.
    #[serde(default)]
    pub concept_registry_release_public_key: Option<String>,
    /// vNext features and independent emergency kill switches.
    #[serde(default)]
    pub vnext: VNextFeatureConfig,
}

impl NodeConfig {
    /// Non-switched Base control plane and generation root. Base-owned stores
    /// are resolved beneath its selected generation, never joined directly to
    /// `data_dir`.
    pub fn base_dataset_root(&self) -> PathBuf {
        self.data_dir.join("base")
    }

    /// Path to the redb storage file.
    pub fn storage_path(&self) -> PathBuf {
        self.data_dir.join("ku.redb")
    }

    /// Path to the node identity file.
    pub fn identity_path(&self) -> PathBuf {
        self.data_dir.join("identity.json")
    }

    /// Path to the retriever index file.
    pub fn retriever_path(&self) -> PathBuf {
        self.data_dir.join("retriever_index.json")
    }

    /// Path to the user profile file.
    pub fn profile_path(&self) -> PathBuf {
        self.data_dir.join("user_profile.json")
    }

    /// Path to the remembered peers file (peer memory).
    pub fn peer_memory_path(&self) -> PathBuf {
        self.data_dir.join("known_peers.json")
    }

    /// Path to the blob storage redb file.
    pub fn blob_storage_path(&self) -> PathBuf {
        self.data_dir.join("ku.blob.redb")
    }

    /// Path to the ConceptRegistry OBR file.
    ///
    /// Used by encode_v2 for concept name → CCID resolution.
    pub fn obr_path(&self) -> PathBuf {
        self.concept_registry_path
            .clone()
            .unwrap_or_else(|| self.data_dir.join("concepts.obr"))
    }

    /// Default seed node domains for peer discovery.
    pub fn default_seed_domains() -> Vec<String> {
        vec![
            "n1.onebrain.live".to_string(),
            "n2.onebrain.live".to_string(),
        ]
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            name: "OneBrain".to_string(),
            port: 4242,
            data_dir: PathBuf::from("./onebrain_data"),
            ollama_url: "http://localhost:11434".to_string(),
            model: "qwen3:8b".to_string(),
            seeds: Vec::new(),
            concept_registry_path: None,
            concept_registry_mode: ConceptRegistryMode::Optional,
            concept_registry_cache_capacity: default_concept_registry_cache_capacity(),
            concept_registry_release_root: None,
            concept_registry_release_public_key: None,
            vnext: VNextFeatureConfig::default(),
        }
    }
}

fn default_concept_registry_cache_capacity() -> usize {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_defaults_remain_backward_compatible() {
        let config = NodeConfig::default();
        assert_eq!(config.concept_registry_mode, ConceptRegistryMode::Optional);
        assert_eq!(
            config.obr_path(),
            PathBuf::from("./onebrain_data").join("concepts.obr")
        );
    }

    #[test]
    fn explicit_registry_path_is_not_relative_to_the_working_directory() {
        let config = NodeConfig {
            concept_registry_path: Some(PathBuf::from("D:/registry/concepts.obr")),
            ..NodeConfig::default()
        };
        assert_eq!(config.obr_path(), PathBuf::from("D:/registry/concepts.obr"));
    }

    #[test]
    fn legacy_serialized_config_gets_optional_registry_defaults() {
        let config: NodeConfig = serde_json::from_str(
            r#"{
                "name":"test",
                "port":4242,
                "data_dir":"./data",
                "ollama_url":"http://localhost:11434",
                "model":"test-model",
                "seeds":[]
            }"#,
        )
        .unwrap();
        assert_eq!(config.concept_registry_mode, ConceptRegistryMode::Optional);
        assert_eq!(config.obr_path(), PathBuf::from("./data/concepts.obr"));
    }

    #[test]
    fn registry_mode_parses_cli_values_strictly() {
        assert_eq!(
            "required".parse(),
            Ok::<ConceptRegistryMode, String>(ConceptRegistryMode::Required)
        );
        assert_eq!(
            "OPTIONAL".parse(),
            Ok::<ConceptRegistryMode, String>(ConceptRegistryMode::Optional)
        );
        assert!("fallback-silently".parse::<ConceptRegistryMode>().is_err());
    }
}
