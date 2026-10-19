//! Trusted local KU custody inputs shared by the opt-in Web host and Desktop.
//! No path, key, Registry authority or model grant is accepted from a WebView.
use ku_core::foundation::VaultKey;
use onebrain_base_contract::{
    ku::{InputMode, KuPrepareV1},
    BaseErrorCodeV1, ResourceBudgetV1,
};
use onebrain_node::concept_registry_runtime::ConceptRegistryGenerationManager;
use onebrain_node::concept_registry_runtime::ConceptRegistryReaderLease;
use onebrain_node::extraction::{ExtractionProvider, ManagedOllamaProvider};
use onebrain_node::ku_manual::ManualKuInputs;
use onebrain_node::ku_ollama::OllamaKuInputs;
use onebrain_node::ku_product::{
    CanonicalPublicReader, KuInputProvider, KuPublicReader, KuResolvedInput, KuRuntimeConfig,
    KuServices,
};
#[cfg(feature = "vnext-network-runtime")]
pub use onebrain_node::ku_public_share::{
    inspect_accepted_public_ku, inspect_outbound_intent, saved_ku_view, PublicKuShareCoordinator,
};
use onebrain_node::{BaseServiceError, NodeConfig, OneBrainNode};
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

/// Optional opt-in loopback `VNextNetworkRuntime` configuration for a normal
/// local KU host. Omitted by default so local hosts remain offline unless the
/// operator explicitly enables loopback networking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KuHostNetworkConfig {
    pub network_opt_in: bool,
    pub bind_addr: std::net::SocketAddr,
}

impl KuHostNetworkConfig {
    /// Enforce explicit opt-in and loopback-only binding before touching
    /// Registry, custody inputs, or dataset directories.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.network_opt_in || !self.bind_addr.ip().is_loopback() {
            return Err("ku_network_config_invalid");
        }
        Ok(())
    }
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
    public: Arc<dyn KuPublicReader>,
    key: [u8; 32],
    registry: Option<Arc<ConceptRegistryGenerationManager>>,
    reason: &'static str,
) -> (KuRuntimeConfig, Option<&'static str>) {
    (
        KuRuntimeConfig {
            vault_key: VaultKey::from_bytes(key),
            registry,
            inputs: Arc::new(ReadOnlyInputs),
            public: Some(public),
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
    let public: Arc<dyn KuPublicReader> =
        CanonicalPublicReader::for_data_dir(&node_config.data_dir);
    if config.sources.len() > 64 {
        return Ok(read_only(public, key, None, "ku_host_source_limit"));
    }
    let mut registry_config = node_config.clone();
    registry_config.concept_registry_release_root = Some(config.registry_root.clone());
    registry_config.concept_registry_release_public_key = Some(config.registry_public_key.clone());
    registry_config.concept_registry_mode = onebrain_node::ConceptRegistryMode::Required;
    let registry = match ConceptRegistryGenerationManager::open(registry_config) {
        Ok(registry) => Arc::new(registry),
        Err(_) => return Ok(read_only(public, key, None, "ku_registry_unavailable")),
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
        Err(reason) => return Ok(read_only(public, key, Some(registry), reason)),
    };
    let manual = match ManualKuInputs::new([0; 32], registry.clone(), sources) {
        Ok(manual) => Arc::new(manual),
        Err(_) => {
            return Ok(read_only(
                public,
                key,
                Some(registry),
                "ku_source_admission_failed",
            ))
        }
    };
    let mut providers: Vec<(String, Arc<dyn ExtractionProvider>, u64)> = Vec::new();
    let mut model_issue = None;
    if let Some(ollama) = &config.ollama {
        if ollama.models.is_empty() || ollama.models.len() > 8 {
            return Ok(read_only(public, key, Some(registry), "ku_model_limit"));
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
        Err(_) => {
            return Ok(read_only(
                public,
                key,
                Some(registry),
                "ku_custody_install_failed",
            ))
        }
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
            public: Some(public),
        },
        model_issue,
    ))
}

/// Acquire an authenticated `KuServices` handle from a normal host's
/// `OneBrainNode` using the host's configured API token.
pub fn host_ku_services(node: &OneBrainNode, api_token: &str) -> Result<KuServices, &'static str> {
    node.ku_services([0; 32], api_token.trim().as_bytes())
        .map_err(|_| "ku_authorization_unavailable")
}

/// Bind an active in-process `VNextNetworkRuntime` to the normal host's
/// canonical public reader for `data_dir` while opt-in networking is running.
#[cfg(feature = "vnext-network-runtime")]
pub fn attach_live_public_reader(
    data_dir: &Path,
    runtime: &Arc<onebrain_node::vnext_network_runtime::VNextNetworkRuntime>,
) {
    CanonicalPublicReader::register_live_runtime_for_data_dir(data_dir, runtime);
}

/// Start an opt-in loopback `VNextNetworkRuntime` for `data_dir` and bind it to
/// the normal host's canonical public reader when `network` is configured.
#[cfg(feature = "vnext-network-runtime")]
pub async fn start_host_loopback_network_runtime(
    data_dir: &Path,
    network: &KuHostNetworkConfig,
) -> Result<Arc<onebrain_node::vnext_network_runtime::VNextNetworkRuntime>, &'static str> {
    network.validate()?;
    let runtime = Arc::new(
        onebrain_node::vnext_network_runtime::VNextNetworkRuntime::start(
            data_dir,
            network.bind_addr,
            onebrain_node::vnext_config::VNextNetworkPolicy::default(),
        )
        .await
        .map_err(|_| "ku_network_startup_failed")?,
    );
    attach_live_public_reader(data_dir, &runtime);
    Ok(runtime)
}

/// Prepare a D-045 Public KU share directly from a normal host's authenticated
/// `OneBrainNode` without exposing the private `ObjectCID` or adding routes.
#[cfg(feature = "vnext-network-runtime")]
#[allow(clippy::too_many_arguments)]
pub async fn prepare_host_public_ku_share(
    node: &OneBrainNode,
    api_token: &str,
    share: &mut PublicKuShareCoordinator,
    object_cid: &str,
    expected_node_id: &str,
    address: std::net::SocketAddr,
    selector: &str,
    namespace: &str,
    now: u64,
) -> Result<serde_json::Value, &'static str> {
    let services = host_ku_services(node, api_token)?;
    share
        .prepare_with_services(
            &services,
            object_cid,
            expected_node_id,
            address,
            selector,
            namespace,
            now,
        )
        .await
}

/// Confirm a prepared D-045 Public KU share after rechecking current authorized
/// access through the normal host's authenticated `OneBrainNode`.
#[cfg(feature = "vnext-network-runtime")]
pub async fn confirm_host_public_ku_share(
    node: &OneBrainNode,
    api_token: &str,
    share: &mut PublicKuShareCoordinator,
    prepared_id: &str,
    confirm_public: bool,
    runtime: &onebrain_node::vnext_network_runtime::VNextNetworkRuntime,
    now: u64,
) -> Result<serde_json::Value, &'static str> {
    let services = host_ku_services(node, api_token)?;
    share
        .confirm_with_services(&services, prepared_id, confirm_public, runtime, now)
        .await
}

#[cfg(test)]
#[path = "ku_api/tests/registry_fixture.rs"]
mod registry_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ku_core::foundation::semantic::SemanticFrameSet;
    use ku_core::foundation::{
        decode_knowledge_object, DisclosureClass, KnownObjectKind, ObjectKind, ObjectReference,
        ObjectSemantics, ObservationGovernance, ResourceProfile, SourceArtifact,
        SourceArtifactKind, StoredRecordKind,
    };
    use onebrain_base_contract::ku::{
        ArtifactDisclosure, ArtifactValidity, ExportMode, KuExportV1, KuGetV1, KuListV1,
        KuRequestV1, KuResponseV1, KuSaveV1, ObjectCID, SourceArtifactCID,
    };
    #[cfg(feature = "vnext-network-runtime")]
    use onebrain_base_contract::ku_payload::decode_hex;
    use onebrain_base_contract::ku_payload::hex;
    use onebrain_node::canonical_exchange::{
        read_canonical_exchange, write_canonical_exchange, BaseExchangeEntryV1,
    };
    use onebrain_node::ku_manual::{ManualDraft, ManualEditorRequest, ManualEditorResponse};
    use onebrain_node::ConceptRegistryMode;

    fn budget() -> ResourceBudgetV1 {
        ResourceBudgetV1::try_new(256, 1_032_192, 1_000_000).unwrap()
    }

    async fn setup_normal_host(
        root: &Path,
        name: &str,
        token: &str,
    ) -> (OneBrainNode, ObjectCID, PathBuf) {
        let host_dir = root.join(name);
        std::fs::create_dir_all(&host_dir).unwrap();
        drop(registry_fixture::registry(&host_dir));
        let key_path = host_dir.join("vault.bin");
        std::fs::write(&key_path, [0x51; 32]).unwrap();
        let reference = |n| ObjectReference::new(1, [n; 32]);
        let (source_bytes, source_cid) = SourceArtifact {
            source_kind: SourceArtifactKind::Text,
            raw_bytes: b"synthetic-private-source-canary".to_vec(),
            media_type_commitment: [2; 32],
            capture_adapter: reference(1),
            capture_sequence: 1,
            governance: ObservationGovernance {
                consent_policy: reference(2),
                consent_receipt: reference(3),
                revocation_policy: reference(4),
                retention_policy: reference(5),
                capture_scope_commitment: [3; 32],
                authorization_assessment_commitment: [4; 32],
                assessed_frontier: [5; 32],
            },
        }
        .to_private_object()
        .unwrap()
        .encode(ResourceProfile::ObjectV1)
        .unwrap();
        let source_path = host_dir.join("source.canonical");
        std::fs::write(&source_path, &source_bytes).unwrap();
        let data_dir = host_dir.join("dataset");
        let node_config = NodeConfig {
            data_dir: data_dir.clone(),
            concept_registry_mode: ConceptRegistryMode::Required,
            concept_registry_release_root: Some(host_dir.join("registry")),
            concept_registry_release_public_key: Some(hex(ed25519_dalek::SigningKey::from_bytes(
                &[0x42; 32],
            )
            .verifying_key()
            .as_bytes())),
            ..Default::default()
        };
        let inputs = KuHostInputsConfig {
            registry_root: node_config.concept_registry_release_root.clone().unwrap(),
            registry_public_key: node_config
                .concept_registry_release_public_key
                .clone()
                .unwrap(),
            vault_key_file: key_path,
            sources: vec![Source {
                label: "private-source-label".into(),
                canonical_file: source_path,
            }],
            ollama: None,
        };
        let (ku, issue) = prepare_ku_runtime(&node_config, &inputs).unwrap();
        assert!(issue.is_none());
        std::fs::create_dir_all(&data_dir).unwrap();
        let mut node = OneBrainNode::new(node_config).await.unwrap();
        let mut base = crate::base_runtime_config_for_api_token(token);
        base.ku = Some(ku);
        node.install_base_runtime(base).unwrap();
        (node, ObjectCID(source_cid.into_bytes()), data_dir)
    }

    async fn save_manual_private_ku(
        node: &OneBrainNode,
        token: &str,
        source_cid: ObjectCID,
    ) -> (ObjectCID, [u8; 32], Vec<u8>) {
        let services = host_ku_services(node, token).unwrap();
        let op = services.reserve().await.unwrap();
        let ManualEditorResponse::Draft(prepare_req) = services
            .editor(
                ManualEditorRequest::Draft(ManualDraft {
                    operation_id: op,
                    idempotency_key: onebrain_base_contract::ku::IdempotencyKey(op.0),
                    source_ref: SourceArtifactCID(source_cid.0),
                    predicate_label: "water".into(),
                    selected_ccid: Some(hex(&[7; 16])),
                    argument_text: "Water is a liquid.".into(),
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        let KuResponseV1::Prepare(prepared) = services
            .invoke(KuRequestV1::Prepare(prepare_req), budget())
            .await
            .unwrap()
        else {
            panic!()
        };
        let private_cid = prepared.object_cids[0];
        let sem_cid = prepared.artifacts[0].semantic_content_cid.0;
        let preview_bytes = STANDARD
            .decode(&prepared.artifacts[0].canonical_preview)
            .unwrap();
        let KuResponseV1::Save(receipt) = services
            .invoke(
                KuRequestV1::Save(KuSaveV1 {
                    operation_id: op,
                    idempotency_key: onebrain_base_contract::ku::IdempotencyKey(op.0),
                    object_cids: vec![private_cid],
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(!receipt.published);
        (private_cid, sem_cid, preview_bytes)
    }

    #[tokio::test]
    async fn normal_host_reads_and_exports_accepted_public_ku_without_exposing_private_store() {
        let temp = tempfile::tempdir().unwrap();
        let token = "synthetic-normal-host-token-1234567890";
        let (mut node, source_cid, _) = setup_normal_host(temp.path(), "host", token).await;
        assert!(!node.vnext_status().features.obp_rp);
        let (private_cid, sem_cid, private_bytes) =
            save_manual_private_ku(&node, token, source_cid).await;
        let services = host_ku_services(&node, token).unwrap();

        // Private LOCAL_ONLY KUs must fail closed on public export.
        let err = services
            .invoke(
                KuRequestV1::Export(KuExportV1 {
                    mode: ExportMode::CanonicalPublicExchange,
                    object_cids: vec![private_cid],
                }),
                budget(),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, BaseErrorCodeV1::Conflict);

        // Admit a DisclosureClass::Public object with the same semantic kernel.
        let decoded = decode_knowledge_object(
            &private_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(ObjectKind(2), 1)],
            &[],
        )
        .unwrap();
        let ObjectSemantics::Known(envelope) = decoded.semantics() else {
            panic!()
        };
        let semantic = SemanticFrameSet::from_canonical_value(&envelope.payload).unwrap();
        let (public_bytes, public_cid_raw) = semantic
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        let public_cid = ObjectCID(public_cid_raw.into_bytes());
        assert_ne!(public_cid, private_cid);
        let exchange_path = temp.path().join("public.obx");
        write_canonical_exchange(
            &[BaseExchangeEntryV1::VNextPublic {
                kind: StoredRecordKind::Object,
                cid: public_cid.0,
                canonical_bytes: public_bytes.clone(),
            }],
            std::fs::File::create(&exchange_path).unwrap(),
        )
        .unwrap();
        let imported = node.import_canonical_exchange(&exchange_path).unwrap();
        assert_eq!(imported.imported, 1);

        let KuResponseV1::Get(view) = services
            .invoke(
                KuRequestV1::Get(KuGetV1 {
                    object_cid: public_cid,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(view.object_cid, public_cid);
        assert_eq!(view.disclosure_class, ArtifactDisclosure::PUBLIC);
        assert_eq!(view.artifact_validity, ArtifactValidity::AcceptedKnown);
        assert_eq!(view.semantic_content_cid.unwrap().0, sem_cid);
        assert!(!view.executable);
        assert!(view.limitations.iter().any(|l| l == "fidelity_unassessed"));

        let KuResponseV1::Export(exported) = services
            .invoke(
                KuRequestV1::Export(KuExportV1 {
                    mode: ExportMode::CanonicalPublicExchange,
                    object_cids: vec![public_cid],
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        let exchange_bytes = STANDARD.decode(exported.public_records.unwrap()).unwrap();
        assert_eq!(
            read_canonical_exchange(exchange_bytes.as_slice())
                .unwrap()
                .len(),
            1
        );

        // Private list/search remains scoped to the principal's private owned store only.
        let KuResponseV1::List(page) = services
            .invoke(
                KuRequestV1::List(KuListV1 {
                    limit: 16,
                    continuation: None,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].object_cid, private_cid);

        let local_only_share = services
            .editor(
                ManualEditorRequest::ShareInspect(onebrain_node::ku_manual::PublicShareInspect {
                    object_cid: hex(&public_cid.0),
                    selector: "02".repeat(32),
                }),
                budget(),
            )
            .await
            .err()
            .expect("local-only host without active network runtime must reject share_inspect");
        assert_eq!(local_only_share.code, BaseErrorCodeV1::CapabilityDisabled);
        assert_eq!(
            local_only_share.reason,
            "ku_public_share_network_unavailable"
        );
    }

    #[cfg(feature = "vnext-network-runtime")]
    #[tokio::test]
    async fn normal_host_public_ku_share_seam_enforces_consent_and_serves_registered_public_reads()
    {
        use onebrain_node::vnext_config::VNextNetworkPolicy;
        use onebrain_node::vnext_network_runtime::VNextNetworkRuntime;
        use std::time::Duration;

        let temp = tempfile::tempdir().unwrap();
        let token = "synthetic-normal-host-token-1234567890";
        let (sender_node, source_cid, sender_dir) =
            setup_normal_host(temp.path(), "sender", token).await;
        let (receiver_node, _, receiver_dir) =
            setup_normal_host(temp.path(), "receiver", token).await;
        let (private_cid, sem_cid, _) =
            save_manual_private_ku(&sender_node, token, source_cid).await;
        let private_cid_hex = hex(&private_cid.0);
        let sem_cid_hex = hex(&sem_cid);

        let sender_net = Arc::new(
            VNextNetworkRuntime::start(
                &sender_dir,
                "127.0.0.1:0".parse().unwrap(),
                VNextNetworkPolicy::default(),
            )
            .await
            .unwrap(),
        );
        let receiver_net = Arc::new(
            VNextNetworkRuntime::start(
                &receiver_dir,
                "127.0.0.1:0".parse().unwrap(),
                VNextNetworkPolicy::default(),
            )
            .await
            .unwrap(),
        );
        attach_live_public_reader(&receiver_dir, &receiver_net);

        let sender_peer_hex = hex(&sender_net.status().principal);
        let receiver_peer_hex = hex(&receiver_net.status().principal);
        let receiver_addr = receiver_net.status().listen_addr;
        let selector_hex = "02".repeat(32);
        let namespace_hex = "03".repeat(32);
        let mut share = PublicKuShareCoordinator::new();

        let preview = prepare_host_public_ku_share(
            &sender_node,
            token,
            &mut share,
            &private_cid_hex,
            &receiver_peer_hex,
            receiver_addr,
            &selector_hex,
            &namespace_hex,
            100,
        )
        .await
        .unwrap();
        assert_eq!(preview["event"], "share_prepared");
        assert_eq!(preview["enqueued"], false);
        assert_eq!(preview["payload_sent"], false);
        let prepared_id = preview["prepared_id"].as_str().unwrap().to_owned();
        let public_cid_hex = preview["public_object_cid"].as_str().unwrap().to_owned();
        assert_ne!(public_cid_hex, private_cid_hex);
        assert_eq!(preview["semantic_content_cid"], sem_cid_hex);
        assert!(!preview.to_string().contains(&private_cid_hex));

        // Explicit confirmation is required before enqueueing any outbound intent.
        assert_eq!(
            confirm_host_public_ku_share(
                &sender_node,
                token,
                &mut share,
                &prepared_id,
                false,
                &sender_net,
                101,
            )
            .await,
            Err("explicit_public_confirmation_required")
        );

        let confirmed = confirm_host_public_ku_share(
            &sender_node,
            token,
            &mut share,
            &prepared_id,
            true,
            &sender_net,
            101,
        )
        .await
        .unwrap();
        assert_eq!(confirmed["event"], "share_confirmed");
        assert_eq!(confirmed["public_object_cid"], public_cid_hex);
        assert_eq!(confirmed["authorizes_reward"], false);
        let intent_id = confirmed["intent_id"].as_str().unwrap().to_owned();

        // Single-use prepared capability cannot be confirmed a second time.
        assert_eq!(
            confirm_host_public_ku_share(
                &sender_node,
                token,
                &mut share,
                &prepared_id,
                true,
                &sender_net,
                102,
            )
            .await,
            Err("prepared_share_missing")
        );

        let inspected_default_intent = inspect_outbound_intent(&sender_net, &intent_id).unwrap();
        assert_eq!(
            inspected_default_intent["public_object_cid"],
            public_cid_hex
        );
        assert_eq!(inspected_default_intent["claims_network_completion"], false);

        // Wait for loopback delivery to the receiver runtime.
        let mut accepted = None;
        for _ in 0..50 {
            if let Ok(value) =
                inspect_accepted_public_ku(&receiver_net, &public_cid_hex, &selector_hex)
            {
                accepted = Some(value);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let accepted = accepted.expect("receiver should accept confirmed public KU");
        assert_eq!(accepted["semantic"]["semantic_content_cid"], sem_cid_hex);
        assert_eq!(
            accepted["authenticated_source_peers"],
            serde_json::json!([sender_peer_hex])
        );
        assert_eq!(
            accepted["provenance"],
            "transport/selector only; original source withheld and unverifiable"
        );
        assert_eq!(accepted["fidelity"], "unassessed");
        assert_eq!(accepted["authorizes_reward"], false);

        // Receiver normal host serves the accepted Public KU through registered get/export.
        let receiver_services = host_ku_services(&receiver_node, token).unwrap();
        let public_cid = ObjectCID(decode_hex::<32>(&public_cid_hex).unwrap());
        let KuResponseV1::Get(view) = receiver_services
            .invoke(
                KuRequestV1::Get(KuGetV1 {
                    object_cid: public_cid,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(view.object_cid, public_cid);
        assert_eq!(view.disclosure_class, ArtifactDisclosure::PUBLIC);
        assert_eq!(view.artifact_validity, ArtifactValidity::AcceptedKnown);
        assert_eq!(view.semantic_content_cid.unwrap().0, sem_cid);
        assert!(!view.executable);

        let KuResponseV1::Export(exported) = receiver_services
            .invoke(
                KuRequestV1::Export(KuExportV1 {
                    mode: ExportMode::CanonicalPublicExchange,
                    object_cids: vec![public_cid],
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(exported.public_records.is_some());

        // Receiver private catalog remains empty; sender private KU remains LOCAL_ONLY.
        let KuResponseV1::List(receiver_page) = receiver_services
            .invoke(
                KuRequestV1::List(KuListV1 {
                    limit: 16,
                    continuation: None,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert!(receiver_page.items.is_empty());

        let sender_services = host_ku_services(&sender_node, token).unwrap();
        let KuResponseV1::Get(sender_private) = sender_services
            .invoke(
                KuRequestV1::Get(KuGetV1 {
                    object_cid: private_cid,
                }),
                budget(),
            )
            .await
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            sender_private.disclosure_class,
            ArtifactDisclosure::LOCALONLY
        );
    }

    #[cfg(feature = "vnext-network-runtime")]
    #[tokio::test]
    async fn normal_host_http_editor_public_ku_share_enforces_consent_network_gate_and_serves_registered_reads(
    ) {
        use crate::ApiServer;
        use axum::body::{to_bytes, Body};
        use axum::http::{header, Method, Request as HttpRequest, StatusCode};
        use onebrain_node::vnext_config::VNextNetworkPolicy;
        use onebrain_node::vnext_network_runtime::VNextNetworkRuntime;
        use serde_json::{json, Value};
        use std::time::Duration;
        use tower::ServiceExt;

        async fn http_json(
            router: axum::Router,
            method: Method,
            uri: &str,
            token: &str,
            body: Option<Value>,
        ) -> (StatusCode, Value) {
            let mut builder = HttpRequest::builder()
                .method(method)
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"));
            let req = if let Some(payload) = body {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                builder
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap()
            } else {
                builder.body(Body::empty()).unwrap()
            };
            let resp = router.oneshot(req).await.unwrap();
            let status = resp.status();
            let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
            (status, serde_json::from_slice(&bytes).unwrap())
        }

        fn editor_envelope(session: &Value, action: &str, payload: Value) -> Value {
            json!({
                "session": session,
                "budget": {
                    "max_items": 32,
                    "max_bytes": 65536,
                    "max_work_units": 4096
                },
                "request": {
                    "action": action,
                    "payload": payload
                }
            })
        }

        fn operation_envelope(session: &Value, operation: &str, payload: Value) -> Value {
            json!({
                "session": session,
                "budget": {
                    "max_items": 256,
                    "max_bytes": 1048576,
                    "max_work_units": 1000000
                },
                "request": {
                    "operation": operation,
                    "payload": payload
                }
            })
        }

        let temp = tempfile::tempdir().unwrap();
        let token = "synthetic-normal-host-token-1234567890";
        let (sender_node, source_cid, sender_dir) =
            setup_normal_host(temp.path(), "http-sender", token).await;
        let (receiver_node, _, receiver_dir) =
            setup_normal_host(temp.path(), "http-receiver", token).await;
        let (private_cid, sem_cid, _) =
            save_manual_private_ku(&sender_node, token, source_cid).await;
        let private_cid_hex = hex(&private_cid.0);
        let sem_cid_hex = hex(&sem_cid);

        let sender_router = ApiServer::new(sender_node, token.into(), 0).build_router();
        let receiver_router = ApiServer::new(receiver_node, token.into(), 0).build_router();

        let (status, sender_status) = http_json(
            sender_router.clone(),
            Method::GET,
            "/api/vnext/ku/status",
            token,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let sender_session = sender_status["data"]["session"].clone();

        let (status, receiver_status) = http_json(
            receiver_router.clone(),
            Method::GET,
            "/api/vnext/ku/status",
            token,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let receiver_session = receiver_status["data"]["session"].clone();

        let sender_net = Arc::new(
            VNextNetworkRuntime::start(
                &sender_dir,
                "127.0.0.1:0".parse().unwrap(),
                VNextNetworkPolicy::default(),
            )
            .await
            .unwrap(),
        );
        let receiver_net = Arc::new(
            VNextNetworkRuntime::start(
                &receiver_dir,
                "127.0.0.1:0".parse().unwrap(),
                VNextNetworkPolicy::default(),
            )
            .await
            .unwrap(),
        );
        let sender_peer_hex = hex(&sender_net.status().principal);
        let receiver_peer_hex = hex(&receiver_net.status().principal);
        let receiver_addr = receiver_net.status().listen_addr.to_string();
        let selector_hex = "02".repeat(32);
        let namespace_hex = "03".repeat(32);

        // Prepare works over registered POST /api/vnext/ku/editor without enabling network yet.
        let (status, prepared_resp) = http_json(
            sender_router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            token,
            Some(editor_envelope(
                &sender_session,
                "share_prepare",
                json!({
                    "object_cid": private_cid_hex,
                    "expected_node_id": receiver_peer_hex,
                    "address": receiver_addr,
                    "selector": selector_hex,
                    "namespace": namespace_hex
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{prepared_resp}");
        let preview = &prepared_resp["data"]["payload"];
        assert_eq!(preview["event"], "share_prepared");
        assert_eq!(preview["enqueued"], false);
        assert_eq!(preview["payload_sent"], false);
        let prepared_id = preview["prepared_id"].as_str().unwrap().to_owned();
        let public_cid_hex = preview["public_object_cid"].as_str().unwrap().to_owned();
        assert_ne!(public_cid_hex, private_cid_hex);
        assert_eq!(preview["semantic_content_cid"], sem_cid_hex);
        assert!(!preview.to_string().contains(&private_cid_hex));
        assert!(!preview
            .to_string()
            .contains("synthetic-private-source-canary"));

        // Explicit confirmation is required before network or outbox checks.
        let (status, unconfirmed_resp) = http_json(
            sender_router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            token,
            Some(editor_envelope(
                &sender_session,
                "share_confirm",
                json!({
                    "prepared_id": prepared_id,
                    "confirm_public": false
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{unconfirmed_resp}");
        assert_eq!(
            unconfirmed_resp["error"]["message"],
            "explicit_public_confirmation_required"
        );

        // Confirming before an opt-in network runtime is attached fails closed with capability_disabled
        // and does not consume the prepared capability.
        let (status, disabled_resp) = http_json(
            sender_router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            token,
            Some(editor_envelope(
                &sender_session,
                "share_confirm",
                json!({
                    "prepared_id": prepared_id,
                    "confirm_public": true
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{disabled_resp}");
        assert_eq!(disabled_resp["error"]["code"], "capability_disabled");
        assert_eq!(
            disabled_resp["error"]["message"],
            "ku_public_share_network_unavailable"
        );

        // Attach the opt-in runtimes and confirm the prepared share.
        attach_live_public_reader(&sender_dir, &sender_net);
        attach_live_public_reader(&receiver_dir, &receiver_net);

        let (status, confirmed_resp) = http_json(
            sender_router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            token,
            Some(editor_envelope(
                &sender_session,
                "share_confirm",
                json!({
                    "prepared_id": prepared_id,
                    "confirm_public": true
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{confirmed_resp}");
        let confirmed = &confirmed_resp["data"]["payload"];
        assert_eq!(confirmed["event"], "share_confirmed");
        assert_eq!(confirmed["public_object_cid"], public_cid_hex);
        assert_eq!(confirmed["authorizes_reward"], false);
        let intent_id = confirmed["intent_id"].as_str().unwrap().to_owned();

        // Single-use prepared capability cannot be replayed.
        let (status, replay_resp) = http_json(
            sender_router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            token,
            Some(editor_envelope(
                &sender_session,
                "share_confirm",
                json!({
                    "prepared_id": prepared_id,
                    "confirm_public": true
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{replay_resp}");
        assert_eq!(replay_resp["error"]["message"], "prepared_share_missing");

        // Sender inspects the durable outbound intent via POST /api/vnext/ku/editor.
        let (status, intent_resp) = http_json(
            sender_router.clone(),
            Method::POST,
            "/api/vnext/ku/editor",
            token,
            Some(editor_envelope(
                &sender_session,
                "share_intent",
                json!({
                    "intent_id": intent_id
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{intent_resp}");
        let intent_payload = &intent_resp["data"]["payload"];
        assert_eq!(intent_payload["event"], "intent");
        assert_eq!(intent_payload["public_object_cid"], public_cid_hex);
        assert_eq!(intent_payload["claims_network_completion"], false);

        // Receiver inspects the accepted Public KU and transport provenance via POST /api/vnext/ku/editor.
        let mut accepted = None;
        for _ in 0..50 {
            let (status, inspect_resp) = http_json(
                receiver_router.clone(),
                Method::POST,
                "/api/vnext/ku/editor",
                token,
                Some(editor_envelope(
                    &receiver_session,
                    "share_inspect",
                    json!({
                        "object_cid": public_cid_hex,
                        "selector": selector_hex
                    }),
                )),
            )
            .await;
            if status == StatusCode::OK {
                accepted = Some(inspect_resp["data"]["payload"].clone());
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let accepted = accepted.expect("receiver should accept and inspect confirmed Public KU");
        assert_eq!(accepted["event"], "public_object");
        assert_eq!(accepted["semantic"]["semantic_content_cid"], sem_cid_hex);
        assert_eq!(
            accepted["authenticated_source_peers"],
            json!([sender_peer_hex])
        );
        assert_eq!(
            accepted["provenance"],
            "transport/selector only; original source withheld and unverifiable"
        );
        assert_eq!(accepted["fidelity"], "unassessed");
        assert_eq!(accepted["authorizes_reward"], false);

        // Receiver also serves the accepted Public KU through registered POST /api/vnext/ku/operations.
        let (status, get_resp) = http_json(
            receiver_router.clone(),
            Method::POST,
            "/api/vnext/ku/operations",
            token,
            Some(operation_envelope(
                &receiver_session,
                "get",
                json!({ "object_cid": public_cid_hex }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{get_resp}");
        let view = &get_resp["data"]["payload"];
        assert_eq!(view["object_cid"], public_cid_hex);
        assert_eq!(view["disclosure_class"], "PUBLIC");
        assert_eq!(view["artifact_validity"], "accepted_known");
        assert_eq!(view["semantic_content_cid"], sem_cid_hex);
        assert_eq!(view["executable"], false);

        let (status, export_resp) = http_json(
            receiver_router.clone(),
            Method::POST,
            "/api/vnext/ku/operations",
            token,
            Some(operation_envelope(
                &receiver_session,
                "export",
                json!({
                    "mode": "canonical_public_exchange",
                    "object_cids": [public_cid_hex]
                }),
            )),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{export_resp}");
        assert!(export_resp["data"]["payload"]["public_records"].is_string());
    }
}
