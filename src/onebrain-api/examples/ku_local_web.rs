//! Explicit local host integration; reads operator-owned inputs, never test fixtures.
use onebrain_api::{
    base_runtime_config_for_api_token,
    ku_host::{prepare_ku_runtime, KuHostInputsConfig, Ollama, Source},
    ApiServer,
};
use onebrain_node::{ConceptRegistryMode, NodeConfig, OneBrainNode};
use serde::Deserialize;
use std::path::PathBuf;

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
    let node_config = NodeConfig {
        data_dir: config.data_dir,
        concept_registry_mode: ConceptRegistryMode::Required,
        concept_registry_release_root: Some(config.registry_root.clone()),
        concept_registry_release_public_key: Some(config.registry_public_key.clone()),
        ..Default::default()
    };
    let (ku, model_issue) = prepare_ku_runtime(
        &node_config,
        &KuHostInputsConfig {
            registry_root: config.registry_root,
            registry_public_key: config.registry_public_key,
            vault_key_file: config.vault_key_file,
            sources: config.sources,
            ollama: config.ollama,
        },
    )?;
    if let Some(reason) = model_issue {
        eprintln!("{reason}; existing private KU remains readable");
    }
    std::fs::create_dir_all(&node_config.data_dir)?;
    let mut node = OneBrainNode::new(node_config).await?;
    let mut base = base_runtime_config_for_api_token(&token);
    base.ku = Some(ku);
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
