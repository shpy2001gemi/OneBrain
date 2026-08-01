//! Desktop configuration — persisted as TOML in the user's config directory.

use onebrain_node::{ConceptRegistryMode, NodeConfig};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Desktop-specific configuration that wraps and extends [`NodeConfig`].
///
/// Persisted as `config.toml` in `<config_dir>/OneBrain/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopConfig {
    /// Display name for this node.
    pub node_name: String,
    /// Persistent data directory.
    pub data_dir: PathBuf,
    /// Ollama API base URL.
    pub ollama_url: String,
    /// Ollama model name.
    pub model: String,
    /// P2P listen port.
    pub port: u16,
    /// REST/WebSocket API port.
    pub api_port: u16,
    /// Seed peer addresses (as strings for TOML friendliness).
    pub seeds: Vec<String>,
    /// Explicit compiled Concept Registry path, if configured.
    #[serde(default)]
    pub concept_registry_path: Option<PathBuf>,
    /// Startup policy for the external Concept Registry.
    #[serde(default)]
    pub concept_registry_mode: ConceptRegistryMode,
    /// Maximum number of label resolutions retained in memory.
    #[serde(default = "default_registry_cache_capacity")]
    pub concept_registry_cache_capacity: usize,
    /// Root containing signed immutable registry releases and activation state.
    #[serde(default)]
    pub concept_registry_release_root: Option<PathBuf>,
    /// Pinned Ed25519 release-signing public key (64 lowercase hex digits).
    #[serde(default)]
    pub concept_registry_release_public_key: Option<String>,
    /// Whether to start the P2P network automatically on launch.
    pub auto_start: bool,
    /// Set to `true` after the first-run wizard completes.
    pub first_run_done: bool,
}

impl DesktopConfig {
    /// OS-specific config directory: `<config_dir>/OneBrain/`.
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OneBrain")
    }

    /// Path to the config file: `<config_dir>/OneBrain/config.toml`.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Default data directory: `<data_dir>/OneBrain/`.
    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("OneBrain")
    }

    /// Load from the TOML file on disk, returning `None` if missing or invalid.
    pub fn load() -> Option<Self> {
        let path = Self::config_path();
        let content = std::fs::read_to_string(&path).ok()?;
        toml::from_str(&content).ok()
    }

    /// Save this configuration to the TOML file.
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(Self::config_path(), content)?;
        Ok(())
    }

    /// Convert to the [`NodeConfig`] used by `onebrain-node`.
    pub fn to_node_config(&self) -> NodeConfig {
        let seeds: Vec<SocketAddr> = self.seeds.iter().filter_map(|s| s.parse().ok()).collect();

        NodeConfig {
            name: self.node_name.clone(),
            port: self.port,
            data_dir: self.data_dir.clone(),
            ollama_url: self.ollama_url.clone(),
            model: self.model.clone(),
            seeds,
            concept_registry_path: self.concept_registry_path.clone(),
            concept_registry_mode: self.concept_registry_mode,
            concept_registry_cache_capacity: self.concept_registry_cache_capacity,
            concept_registry_release_root: self.concept_registry_release_root.clone(),
            concept_registry_release_public_key: self.concept_registry_release_public_key.clone(),
            vnext: Default::default(),
        }
    }
}

impl Default for DesktopConfig {
    fn default() -> Self {
        let node_name = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "OneBrain".to_string());

        Self {
            node_name,
            data_dir: Self::default_data_dir(),
            ollama_url: "http://localhost:11434".to_string(),
            model: "qwen3:8b".to_string(),
            port: 4242,
            api_port: 4280,
            seeds: Vec::new(),
            concept_registry_path: None,
            concept_registry_mode: ConceptRegistryMode::Optional,
            concept_registry_cache_capacity: default_registry_cache_capacity(),
            concept_registry_release_root: None,
            concept_registry_release_public_key: None,
            auto_start: true,
            first_run_done: false,
        }
    }
}

fn default_registry_cache_capacity() -> usize {
    4096
}
