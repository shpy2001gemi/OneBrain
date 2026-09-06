//! Explicit local host integration; reads operator-owned inputs, never test fixtures.
use ku_core::foundation::VaultKey;
use ku_encoder::extraction::{ExtractionProvider, ManagedOllamaProvider};
use onebrain_api::{base_runtime_config_for_api_token, ApiServer};
use onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager;
use onebrain_node::ku_manual::ManualKuInputs;
use onebrain_node::ku_ollama::OllamaKuInputs;
use onebrain_node::ku_product::KuInputProvider;
use onebrain_node::ku_product::KuRuntimeConfig;
use onebrain_node::{ConceptRegistryMode, NodeConfig, OneBrainNode};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    data_dir: PathBuf,
    registry_root: PathBuf,
    registry_public_key: String,
    vault_key_file: PathBuf,
    api_token_file: PathBuf,
    #[serde(default)]
    sources: Vec<Source>,
    #[serde(default)]
    ollama: Option<Ollama>,
    web_dir: PathBuf,
    port: u16,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ollama {
    executable: PathBuf,
    models_dir: PathBuf,
    models: Vec<String>,
    memory_limit_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    label: String,
    canonical_file: PathBuf,
}
fn read_bounded(
    path: &std::path::Path,
    maximum: u64,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err("host input exceeds limit".into());
    }
    Ok(bytes)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: ku_local_web <trusted-host-config.json>")?;
    let config: Config =
        serde_json::from_slice(&read_bounded(std::path::Path::new(&path), 65536)?)?;
    if config.sources.len() > 64 {
        return Err("host supports at most 64 manual sources".into());
    }
    let token = String::from_utf8(read_bounded(&config.api_token_file, 1024)?)?
        .trim()
        .to_owned();
    if token.len() < 32
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(
            "token must contain 32..1024 ASCII letters, digits, hyphen or underscore".into(),
        );
    }
    let key: [u8; 32] = read_bounded(&config.vault_key_file, 32)?
        .try_into()
        .map_err(|_| "Vault key must be exactly 32 raw bytes")?;
    let node_config = NodeConfig {
        data_dir: config.data_dir,
        concept_registry_mode: ConceptRegistryMode::Required,
        concept_registry_release_root: Some(config.registry_root),
        concept_registry_release_public_key: Some(config.registry_public_key),
        ..Default::default()
    };
    let registry = Arc::new(ConceptRegistryGenerationManager::open(node_config.clone())?);
    let sources = config
        .sources
        .into_iter()
        .map(|s| Ok((s.label, read_bounded(&s.canonical_file, 65536)?)))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let inputs = Arc::new(
        ManualKuInputs::new([0; 32], registry.clone(), sources)
            .map_err(|_| "invalid host source admission")?,
    );
    let providers = if let Some(ollama) = config.ollama {
        if ollama.models.is_empty() || ollama.models.len() > 8 {
            return Err("admit 1..8 installed models".into());
        }
        let gate = Arc::new(tokio::sync::Semaphore::new(1));
        println!("Verifying installed Ollama artifacts; model quality remains unqualified.");
        let mut providers: Vec<(String, Arc<dyn ExtractionProvider>, u64)> = Vec::new();
        for name in ollama.models {
            let provider = ManagedOllamaProvider::open(
                ollama.executable.clone(),
                ollama.models_dir.clone(),
                &name,
                ollama.memory_limit_bytes,
                gate.clone(),
            );
            match provider {
                Ok(provider) => {
                    providers.push((name, Arc::new(provider), ollama.memory_limit_bytes))
                }
                Err(error) => eprintln!(
                    "Experimental model unavailable ({}); existing private KU remains readable.",
                    error.0
                ),
            }
        }
        providers
    } else {
        Vec::new()
    };
    let inputs: Arc<dyn KuInputProvider> = Arc::new(
        OllamaKuInputs::new([0; 32], inputs, registry.clone(), providers)
            .map_err(|_| "local text custody installation failed")?,
    );
    std::fs::create_dir_all(&node_config.data_dir)?;
    let mut node = OneBrainNode::new(node_config).await?;
    let mut base = base_runtime_config_for_api_token(&token);
    base.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes(key),
        registry: Some(registry),
        inputs,
        public: None,
    });
    node.install_base_runtime(base)?;
    println!(
        "Local KU: http://127.0.0.1:{}/ku — AI unqualified; no publication",
        config.port
    );
    ApiServer::new(node, token, config.port)
        .with_web_dir(config.web_dir)
        .start()
        .await
}
