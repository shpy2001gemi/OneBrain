//! Trusted local KU custody inputs shared by the opt-in Web host and Desktop.
//! No path, key, Registry authority or model grant is accepted from a WebView.
use ku_core::foundation::VaultKey;
use ku_encoder::extraction::{ExtractionProvider, ManagedOllamaProvider};
use onebrain_base_contract::{
    ku::{InputMode, KuPrepareV1},
    BaseErrorCodeV1, ResourceBudgetV1,
};
use onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager;
use onebrain_node::concept_registry_runtime::ConceptRegistryReaderLease;
use onebrain_node::ku_manual::ManualKuInputs;
use onebrain_node::ku_ollama::OllamaKuInputs;
use onebrain_node::ku_product::{KuInputProvider, KuResolvedInput, KuRuntimeConfig};
use onebrain_node::{BaseServiceError, NodeConfig};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KuHostInputsConfig {
    pub registry_root: PathBuf,
    pub registry_public_key: String,
    pub vault_key_file: PathBuf,
    #[serde(default)]
    pub sources: Vec<Source>,
    #[serde(default)]
    pub ollama: Option<Ollama>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub label: String,
    pub canonical_file: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ollama {
    pub executable: PathBuf,
    pub models_dir: PathBuf,
    pub models: Vec<String>,
    pub memory_limit_bytes: u64,
}

struct ReadOnlyInputs;
impl KuInputProvider for ReadOnlyInputs {
    fn implementation(&self, _: InputMode) -> Option<[u8; 32]> {
        None
    }
    fn check_access(&self, _: [u8; 32], _: &[[u8; 32]]) -> Result<(), BaseServiceError> {
        Err(BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "ku_host_input_unavailable",
        ))
    }
    fn resolve(
        &self,
        _: [u8; 32],
        _: &KuPrepareV1,
        _: &ConceptRegistryReaderLease,
        _: &ResourceBudgetV1,
    ) -> Result<KuResolvedInput, BaseServiceError> {
        Err(BaseServiceError::new(
            BaseErrorCodeV1::DependencyUnavailable,
            "ku_host_input_unavailable",
        ))
    }
}

fn read_only(
    key: [u8; 32],
    registry: Option<Arc<ConceptRegistryGenerationManager>>,
    reason: &'static str,
) -> (KuRuntimeConfig, Option<&'static str>) {
    (
        KuRuntimeConfig {
            vault_key: VaultKey::from_bytes(key),
            registry,
            inputs: Arc::new(ReadOnlyInputs),
            public: None,
        },
        Some(reason),
    )
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "ku_host_input_unavailable")?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "ku_host_input_unavailable")?;
    if bytes.len() as u64 > maximum {
        return Err("ku_host_input_exceeds_limit");
    }
    Ok(bytes)
}

/// Prepare one node-owned KU runtime from operator files. Failures contain no
/// private path or source data so Desktop may show the code in its UI.
pub fn prepare_ku_runtime(
    node_config: &NodeConfig,
    config: &KuHostInputsConfig,
) -> Result<(KuRuntimeConfig, Option<&'static str>), &'static str> {
    let key: [u8; 32] = read_bounded(&config.vault_key_file, 32)?
        .try_into()
        .map_err(|_| "ku_vault_key_invalid")?;
    if config.sources.len() > 64 {
        return Ok(read_only(key, None, "ku_host_source_limit"));
    }
    let mut registry_config = node_config.clone();
    registry_config.concept_registry_release_root = Some(config.registry_root.clone());
    registry_config.concept_registry_release_public_key = Some(config.registry_public_key.clone());
    registry_config.concept_registry_mode = onebrain_node::ConceptRegistryMode::Required;
    let registry = match ConceptRegistryGenerationManager::open(registry_config) {
        Ok(registry) => Arc::new(registry),
        Err(_) => return Ok(read_only(key, None, "ku_registry_unavailable")),
    };
    let sources = config
        .sources
        .iter()
        .map(|source| {
            Ok((
                source.label.clone(),
                read_bounded(&source.canonical_file, 65536)?,
            ))
        })
        .collect::<Result<Vec<_>, &'static str>>();
    let sources = match sources {
        Ok(sources) => sources,
        Err(reason) => return Ok(read_only(key, Some(registry), reason)),
    };
    let manual = match ManualKuInputs::new([0; 32], registry.clone(), sources) {
        Ok(manual) => Arc::new(manual),
        Err(_) => return Ok(read_only(key, Some(registry), "ku_source_admission_failed")),
    };
    let mut providers: Vec<(String, Arc<dyn ExtractionProvider>, u64)> = Vec::new();
    let mut model_issue = None;
    if let Some(ollama) = &config.ollama {
        if ollama.models.is_empty() || ollama.models.len() > 8 {
            return Ok(read_only(key, Some(registry), "ku_model_limit"));
        }
        let gate = Arc::new(tokio::sync::Semaphore::new(1));
        for name in &ollama.models {
            match ManagedOllamaProvider::open(
                ollama.executable.clone(),
                ollama.models_dir.clone(),
                name,
                ollama.memory_limit_bytes,
                gate.clone(),
            ) {
                Ok(provider) => {
                    providers.push((name.clone(), Arc::new(provider), ollama.memory_limit_bytes))
                }
                Err(_) => model_issue = Some("ku_experimental_model_unavailable"),
            }
        }
    }
    let inputs = match OllamaKuInputs::new([0; 32], manual, registry.clone(), providers) {
        Ok(inputs) => inputs,
        Err(_) => return Ok(read_only(key, Some(registry), "ku_custody_install_failed")),
    };
    let inputs: Arc<dyn KuInputProvider> =
        Arc::new(if std::env::var("KU_SELECTION").as_deref() == Ok("2") {
            inputs.with_selection_v2()
        } else {
            inputs
        });
    Ok((
        KuRuntimeConfig {
            vault_key: VaultKey::from_bytes(key),
            registry: Some(registry),
            inputs,
            public: None,
        },
        model_issue,
    ))
}
