//! Node configuration.
//!
//! Defines the runtime configuration for an OneBrain node instance,
//! including network port, data paths, Ollama settings, and seed peers.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::vnext_config::VNextFeatureConfig;

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
    /// vNext features and independent emergency kill switches.
    #[serde(default)]
    pub vnext: VNextFeatureConfig,
}

impl NodeConfig {
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
        self.data_dir.join("concepts.obr")
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
            vnext: VNextFeatureConfig::default(),
        }
    }
}
