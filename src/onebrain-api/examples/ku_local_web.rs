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

const REGISTRY_SETUP_ERROR: &str = "ku_registry_unavailable: local KU host cannot start. \
    Set registry_root to the Registry root containing releases/ and activation state/ and \
    registry_public_key to its independently trusted public key. An unsigned concepts.obr \
    is insufficient. Follow 'Run the local MVP' in \
    docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md; \
    retain the existing data_dir and Vault key when retrying.";

#[derive(Debug)]
enum InputReadError {
    Unreadable,
    TooLarge,
}

fn read_bounded(path: &std::path::Path, maximum: u64) -> Result<Vec<u8>, InputReadError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| InputReadError::Unreadable)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| InputReadError::Unreadable)?;
    if bytes.len() as u64 > maximum {
        return Err(InputReadError::TooLarge);
    }
    Ok(bytes)
}

fn load_config(path: &std::path::Path) -> Result<Config, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path, 65536).map_err(|error| match error {
        InputReadError::Unreadable => {
            "ku_host_config_unreadable: cannot read the host JSON config. \
            Check the command's config path and local read permissions; relative paths resolve \
            against the launch working directory."
        }
        InputReadError::TooLarge => {
            "ku_host_config_too_large: host JSON config exceeds 65536 bytes. \
            Keep secret and canonical source bytes in their referenced files, outside the config."
        }
    })?;
    // serde's detailed error may contain operator-supplied values or field names.
    // Only its category and numeric position are safe to include in diagnostics.
    // Windows PowerShell's UTF-8 writer prefixes JSON with a BOM. Accept one
    // leading UTF-8 BOM after enforcing the original on-disk byte limit; keep
    // all other JSON/schema checks and operator-owned file bytes unchanged.
    let json = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    serde_json::from_slice(json).map_err(|error| {
        let guidance = match error.classify() {
            serde_json::error::Category::Data => {
                "check required fields and their types: data_dir, \
                registry_root, registry_public_key, vault_key_file, api_token_file, web_dir, port. \
                Unknown fields are rejected; sources and ollama are optional"
            }
            _ => "use complete UTF-8 JSON; escape Windows backslashes or use forward slashes",
        };
        format!(
            "ku_host_config_invalid: {guidance} (line {}, column {}). Follow 'Run the local MVP' \
            in docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md; \
            retain the existing data_dir and Vault key when retrying.",
            error.line(),
            error.column()
        )
        .into()
    })
}

fn load_api_token(path: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path, 1024).map_err(|error| match error {
        InputReadError::Unreadable => {
            "ku_api_token_unreadable: cannot read api_token_file. \
            Set it to an existing locally readable token file; relative paths resolve against \
            the launch working directory. Keep the token in that private file, outside Git."
        }
        InputReadError::TooLarge => {
            "ku_api_token_too_large: api_token_file exceeds 1024 bytes \
            including surrounding whitespace. Supply a plain UTF-8 token file, not JSON, \
            a key file or a canonical source file."
        }
    })?;
    let invalid = "ku_api_token_invalid: api_token_file must contain a UTF-8 token of \
        32..1024 ASCII letters, digits, hyphen or underscore after trimming surrounding \
        whitespace. Supply only the token, without quotes or JSON; keep the same data_dir \
        and Vault key when correcting this file.";
    let token = String::from_utf8(bytes)
        .map_err(|_| invalid)?
        .trim()
        .to_owned();
    if token.len() < 32
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(invalid.into());
    }
    Ok(token)
}

fn runtime_warning(reason: &'static str) -> &'static str {
    // These read errors are source failures only after prepare_ku_runtime has
    // returned a runtime. Fatal Vault-key errors were handled separately.
    // Project existing shared codes without rereading or exposing custody inputs.
    match reason {
        "ku_host_input_unavailable" => {
            "ku_host_input_unavailable: cannot read sources[].canonical_file. Check each \
            configured source file and local read permissions; relative paths resolve against \
            the launch working directory. Source editing/encoding is unavailable for this \
            launch. Retry with the same data_dir and original Vault key; saved KU access \
            still requires successful authenticated reads. Follow 'Resolve source-input setup \
            failures' in docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md."
        }
        "ku_host_input_exceeds_limit" => {
            "ku_host_input_exceeds_limit: a sources[].canonical_file exceeds 65536 bytes. \
            Supply the intended complete canonical LOCAL_ONLY Text SourceArtifact within \
            that limit; do not truncate or edit canonical bytes. Source editing/encoding is \
            unavailable for this launch. Retry with the same data_dir and original Vault key; \
            saved KU access still requires successful authenticated reads. Follow 'Resolve \
            source-input setup failures' in docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md."
        }
        "ku_source_admission_failed" => {
            "ku_source_admission_failed: check sources[].canonical_file and sources[].label. \
            Each file must be a valid binary canonical LOCAL_ONLY Text SourceArtifact, \
            not raw text, JSON, Base64 or a KU object. Labels must contain 1..128 UTF-8 bytes \
            and source object IDs must be distinct. Source editing/encoding is unavailable \
            for this launch. Retry with the same data_dir and original Vault key; saved KU \
            access still requires successful authenticated reads. Follow 'Resolve source-input \
            setup failures' in docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md."
        }
        "ku_host_source_limit" => {
            "ku_host_source_limit: sources has more than 64 entries. Explicitly choose at most \
            64 operator-admitted sources; source editing/encoding is unavailable for this \
            launch. Retry with the same data_dir and original Vault key; saved KU access \
            still requires successful authenticated reads. Follow 'Resolve source-input setup \
            failures' in docs/handoffs/2026-09-ku-obp-productization/outputs/KU_WEB_001_IMPLEMENTATION.md."
        }
        // Do not translate an unfamiliar fallback code into a saved-read guarantee.
        other => other,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: ku_local_web <trusted-host-config.json>")?;
    let config = load_config(std::path::Path::new(&path))?;
    run(config).await
}

async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let token = load_api_token(&config.api_token_file)?;
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
    )
    .map_err(|reason| match reason {
        // The shared helper's fatal errors currently come only from Vault key
        // loading. Keep its codes and Desktop behavior intact; add operator
        // guidance at this example's boundary without rereading private inputs.
        "ku_host_input_unavailable" => {
            "ku_vault_key_unreadable: cannot read vault_key_file. Set it to an existing \
            locally readable private Vault key file; relative paths resolve against the \
            launch working directory. For an existing dataset, restore access to its \
            original key; do not generate a replacement key or change data_dir."
        }
        "ku_host_input_exceeds_limit" => {
            "ku_vault_key_too_large: vault_key_file exceeds 32 bytes. Supply the original \
            32-byte binary Vault key, not hex, Base64, JSON or token text; no whitespace \
            is trimmed. Keep the same data_dir and recover its original key through \
            your local secret-management process; do not truncate or replace the key."
        }
        "ku_vault_key_invalid" => {
            "ku_vault_key_invalid: vault_key_file must contain exactly 32 binary bytes. \
            No text decoding or whitespace trimming is performed. Keep the same data_dir \
            and restore its original key through your local secret-management process; \
            do not pad, generate or replace the key."
        }
        other => other,
    })?;
    // This host requires a verified Registry at node startup. The shared custody
    // helper's read-only fallback cannot make an unavailable Registry startable.
    if model_issue == Some("ku_registry_unavailable") {
        return Err(REGISTRY_SETUP_ERROR.into());
    }
    std::fs::create_dir_all(&node_config.data_dir)?;
    let mut node = OneBrainNode::new(node_config).await?;
    let mut base = base_runtime_config_for_api_token(&token);
    base.ku = Some(ku);
    node.install_base_runtime(base)?;
    if let Some(reason) = model_issue {
        eprintln!("{}", runtime_warning(reason));
    }
    println!(
        "Local KU: http://127.0.0.1:{}/ku — AI unqualified; no publication",
        config.port
    );
    ApiServer::new(node, token, config.port)
        .with_web_dir(config.web_dir)
        .start()
        .await
}

#[cfg(test)]
#[path = "../src/ku_api/tests/registry_fixture.rs"]
mod registry_fixture;

#[cfg(test)]
mod tests {
    use super::*;

    async fn ku_request(
        router: &axum::Router,
        token: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        use tower::ServiceExt;
        let request = axum::http::Request::builder()
            .method(if body.is_some() { "POST" } else { "GET" })
            .uri(path)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(
                body.map(|value| value.to_string()).unwrap_or_default(),
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        assert_eq!(response.headers()["Cache-Control"], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 1048576)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn empty_catalog_and_source_failures_preserve_saved_reads_after_restart() {
        use axum::http::StatusCode;
        use ku_core::foundation::{
            ObjectReference, ObservationGovernance, ResourceProfile, SourceArtifact,
            SourceArtifactKind,
        };
        use serde_json::json;

        // Reuse the existing synthetic signed/activated fixture, never operator inputs.
        let root = tempfile::tempdir().unwrap();
        drop(registry_fixture::registry(root.path()));
        let key = [0x51; 32];
        let key_path = root.path().join("vault.bin");
        std::fs::write(&key_path, key).unwrap();
        let reference = |n| ObjectReference::new(1, [n; 32]);
        let (source, source_cid) = SourceArtifact {
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
        let source_path = root.path().join("private-source-canary.canonical");
        std::fs::write(&source_path, &source).unwrap();
        let retained_source_path = root.path().join("retained-source.canonical");
        std::fs::write(&retained_source_path, &source).unwrap();
        let config = NodeConfig {
            data_dir: root.path().join("dataset"),
            concept_registry_mode: ConceptRegistryMode::Required,
            concept_registry_release_root: Some(root.path().join("registry")),
            concept_registry_release_public_key: Some(onebrain_base_contract::ku_payload::hex(
                ed25519_dalek::SigningKey::from_bytes(&[0x42; 32])
                    .verifying_key()
                    .as_bytes(),
            )),
            ..Default::default()
        };
        let inputs = KuHostInputsConfig {
            registry_root: config.concept_registry_release_root.clone().unwrap(),
            registry_public_key: config.concept_registry_release_public_key.clone().unwrap(),
            vault_key_file: key_path.clone(),
            sources: vec![Source {
                label: "private-label-canary".into(),
                canonical_file: source_path.clone(),
            }],
            ollama: None,
        };
        let token = "synthetic-token-canary-1234567890123456";
        let start = |ku| {
            let config = config.clone();
            async move {
                std::fs::create_dir_all(&config.data_dir).unwrap();
                let mut node = OneBrainNode::new(config).await.unwrap();
                let mut runtime = base_runtime_config_for_api_token(token);
                runtime.ku = Some(ku);
                node.install_base_runtime(runtime).unwrap();
                ApiServer::new(node, token.into(), 0).build_router()
            }
        };
        let (ku, issue) = prepare_ku_runtime(&config, &inputs).unwrap();
        assert!(issue.is_none());
        let router = start(ku).await;
        let (_, status) = ku_request(&router, token, "/api/vnext/ku/status", None).await;
        let session = status["data"]["session"].clone();
        let (_, reserved) = ku_request(
            &router,
            token,
            "/api/vnext/ku/reservations",
            Some(json!({"session":session})),
        )
        .await;
        let op = reserved["data"]["payload"]["operation_id"].clone();
        let budget = json!({"max_items":256,"max_bytes":1048576,"max_work_units":1000000});
        let editor = |action, payload| json!({"session":session,"budget":budget,"request":{"action":action,"payload":payload}});
        let operation = |session: &serde_json::Value, name, payload| json!({"session":session,"budget":budget,"request":{"operation":name,"payload":payload}});
        let (code, draft) = ku_request(&router, token, "/api/vnext/ku/editor", Some(editor("draft", json!({
            "operation_id":op,"idempotency_key":op,"source_ref":onebrain_base_contract::ku_payload::hex(&source_cid.into_bytes()),
            "predicate_label":"water","selected_ccid":"07".repeat(16),"argument_text":"synthetic saved assertion"
        })))).await;
        assert_eq!(code, StatusCode::OK, "{draft}");
        let (code, preview) = ku_request(
            &router,
            token,
            "/api/vnext/ku/operations",
            Some(operation(
                &session,
                "prepare",
                draft["data"]["payload"].clone(),
            )),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{preview}");
        assert_eq!(preview["data"]["payload"]["validity"], "ready");
        let ids = preview["data"]["payload"]["object_cids"].clone();
        let canonical = preview["data"]["payload"]["artifacts"][0]["canonical_preview"].clone();
        let (code, receipt) = ku_request(
            &router,
            token,
            "/api/vnext/ku/operations",
            Some(operation(
                &session,
                "save",
                json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}),
            )),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{receipt}");
        assert_eq!(receipt["data"]["payload"]["state"], "committed");
        assert_eq!(receipt["data"]["payload"]["published"], false);
        drop(router);

        // Both optional-config forms must be a usable empty catalog, not fallback.
        for explicit_empty in [false, true] {
            let mut json_config = json!({
                "data_dir":config.data_dir,"registry_root":inputs.registry_root,
                "registry_public_key":inputs.registry_public_key,"vault_key_file":key_path,
                "api_token_file":root.path().join("unused-token.txt"),
                "web_dir":root.path().join("unused-web"),"port":0
            });
            if explicit_empty {
                json_config["sources"] = json!([]);
            }
            // Compose a PowerShell-style UTF-8 config for the saved-read restart.
            let config_path = root.path().join("composed-host.json");
            let mut config_bytes = vec![0xef, 0xbb, 0xbf];
            config_bytes.extend(serde_json::to_vec(&json_config).unwrap());
            std::fs::write(&config_path, &config_bytes).unwrap();
            let empty = load_config(&config_path).unwrap();
            assert_eq!(std::fs::read(&config_path).unwrap(), config_bytes);
            let empty_inputs = KuHostInputsConfig {
                sources: empty.sources,
                ..inputs.clone()
            };
            let (ku, issue) = prepare_ku_runtime(&config, &empty_inputs).unwrap();
            assert!(issue.is_none());
            let router = start(ku).await;
            let (code, status) = ku_request(&router, token, "/api/vnext/ku/status", None).await;
            assert_eq!(code, StatusCode::OK);
            let refreshed = status["data"]["session"].clone();
            let catalog_request = json!({"session":refreshed,"budget":budget,"request":{"action":"catalog","payload":{}}});
            let (code, catalog) = ku_request(
                &router,
                token,
                "/api/vnext/ku/editor",
                Some(catalog_request.clone()),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{catalog}");
            assert_eq!(catalog["data"]["payload"]["sources"], json!([]));
            assert_eq!(
                ku_request(
                    &router,
                    "wrong-token",
                    "/api/vnext/ku/editor",
                    Some(catalog_request)
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
            let (code, view) = ku_request(
                &router,
                token,
                "/api/vnext/ku/operations",
                Some(operation(&refreshed, "get", json!({"object_cid":ids[0]}))),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{view}");
            assert_eq!(view["data"]["payload"]["canonical_bytes"], canonical);
            assert_eq!(view["data"]["payload"]["disclosure_class"], "LOCAL_ONLY");
            let (code, recovered) = ku_request(
                &router,
                token,
                "/api/vnext/ku/operations",
                Some(operation(
                    &refreshed,
                    "reconcile",
                    json!({"operation_id":op}),
                )),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{recovered}");
            assert_eq!(recovered["data"]["payload"], receipt["data"]["payload"]);
            assert_eq!(std::fs::read(&key_path).unwrap(), key);
            assert_eq!(std::fs::read(&source_path).unwrap(), source);
            drop(router);
        }

        let mut fallback_inputs = inputs.clone();
        // One usable file followed by a failed file must not admit a partial catalog.
        fallback_inputs.sources.insert(
            0,
            Source {
                label: "retained-source".into(),
                canonical_file: retained_source_path.clone(),
            },
        );

        for (bytes, expected, source_limit) in [
            (None, "ku_host_input_unavailable", false),
            (
                Some(b"raw-private-source-canary".to_vec()),
                "ku_source_admission_failed",
                false,
            ),
            (
                Some(vec![b'x'; 65537]),
                "ku_host_input_exceeds_limit",
                false,
            ),
            (None, "ku_host_source_limit", true),
        ] {
            if let Some(bytes) = &bytes {
                std::fs::write(&source_path, bytes).unwrap();
            } else {
                std::fs::remove_file(&source_path).unwrap();
            }
            let mut failed_inputs = fallback_inputs.clone();
            if source_limit {
                failed_inputs.sources = vec![inputs.sources[0].clone(); 65];
            }
            let (ku, issue) = prepare_ku_runtime(&config, &failed_inputs).unwrap();
            assert_eq!(issue, Some(expected));
            let warning = runtime_warning(issue.unwrap());
            assert!(warning.starts_with(expected));
            assert!(warning.contains(if source_limit {
                "64"
            } else {
                "sources[].canonical_file"
            }));
            assert!(warning.contains("same data_dir and original Vault key"));
            assert!(!warning.contains("canary"));
            assert!(!warning.contains(&root.path().display().to_string()));
            assert!(!warning.contains("remains readable"));
            let router = start(ku).await;
            let (code, status) = ku_request(&router, token, "/api/vnext/ku/status", None).await;
            assert_eq!(code, StatusCode::OK);
            let refreshed = status["data"]["session"].clone();
            assert_eq!(
                refreshed["dataset_generation"],
                session["dataset_generation"]
            );
            assert_ne!(
                refreshed["process_generation"],
                session["process_generation"]
            );
            let get = operation(&refreshed, "get", json!({"object_cid":ids[0]}));
            let (code, view) = ku_request(
                &router,
                token,
                "/api/vnext/ku/operations",
                Some(get.clone()),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{view}");
            assert_eq!(view["data"]["payload"]["canonical_bytes"], canonical);
            assert_eq!(view["data"]["payload"]["disclosure_class"], "LOCAL_ONLY");
            assert_eq!(
                ku_request(
                    &router,
                    "wrong-token",
                    "/api/vnext/ku/operations",
                    Some(get)
                )
                .await
                .0,
                StatusCode::FORBIDDEN
            );
            let (code, denied) = ku_request(&router, token, "/api/vnext/ku/editor", Some(json!({"session":refreshed,"budget":budget,"request":{"action":"catalog","payload":{}}}))).await;
            assert!(!code.is_success(), "{denied}");
            assert!(!denied.to_string().contains("canary"));
            let (code, page) = ku_request(
                &router,
                token,
                "/api/vnext/ku/operations",
                Some(operation(&refreshed, "list", json!({"limit":20}))),
            )
            .await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(
                page["data"]["payload"]["items"].as_array().unwrap().len(),
                1
            );
            let (code, recovered) = ku_request(
                &router,
                token,
                "/api/vnext/ku/operations",
                Some(operation(
                    &refreshed,
                    "reconcile",
                    json!({"operation_id":op}),
                )),
            )
            .await;
            assert_eq!(code, StatusCode::OK, "{recovered}");
            assert_eq!(recovered["data"]["payload"], receipt["data"]["payload"]);
            assert_eq!(std::fs::read(&key_path).unwrap(), key);
            assert_eq!(std::fs::read(&retained_source_path).unwrap(), source);
            if let Some(bytes) = bytes {
                assert_eq!(std::fs::read(&source_path).unwrap(), bytes);
            } else {
                assert!(!source_path.exists());
            }
            drop(router);
        }
    }

    #[test]
    fn config_diagnostics_hide_input_values_and_preserve_valid_schema() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("private-path-canary.json");
        let valid = serde_json::json!({
            "data_dir": "private-value-canary",
            "registry_root": "registry",
            "registry_public_key": "trusted-key",
            "vault_key_file": "vault.bin",
            "api_token_file": "token.txt",
            "web_dir": "web",
            "port": 4280
        });
        let error = load_config(&path).err().unwrap().to_string();
        assert!(error.starts_with("ku_host_config_unreadable:"));
        assert!(error.contains("working directory"));
        assert!(!error.contains("canary"));

        let mut wrong_type = valid.clone();
        wrong_type["port"] = serde_json::json!("private-value-canary");
        let mut unknown = valid.clone();
        unknown["private-field-canary"] = serde_json::json!("private-value-canary");
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("api_token_file");
        for value in [wrong_type, unknown, missing] {
            let bytes = serde_json::to_vec(&value).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            let error = load_config(&path).err().unwrap().to_string();
            assert!(error.starts_with("ku_host_config_invalid:"));
            assert!(error.contains("required fields"));
            assert!(error.contains("api_token_file"));
            assert!(!error.contains("canary"));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        std::fs::write(&path, br#"{"data_dir":"unterminated-private-value-canary"#).unwrap();
        let error = load_config(&path).err().unwrap().to_string();
        assert!(error.contains("complete UTF-8 JSON"));
        assert!(!error.contains("canary"));

        let mut bytes = serde_json::to_vec(&valid).unwrap();
        bytes.resize(65536, b' ');
        std::fs::write(&path, &bytes).unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.port, 4280);
        assert!(config.sources.is_empty());
        assert!(config.ollama.is_none());
        bytes.push(b' ');
        std::fs::write(&path, &bytes).unwrap();
        assert!(load_config(&path)
            .err()
            .unwrap()
            .to_string()
            .starts_with("ku_host_config_too_large:"));
    }

    #[test]
    fn powershell_utf8_bom_config_preserves_schema_bounds_and_input() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("host.json");
        let mut value = serde_json::json!({
            "data_dir":"dataset", "registry_root":"registry",
            "registry_public_key":"trusted-key", "vault_key_file":"vault.key",
            "api_token_file":"api-token.txt", "web_dir":"web", "port":4280,
            "sources":[{"label":"Admitted source", "canonical_file":"source.canonical"}]
        });
        let encode = |value: &serde_json::Value| {
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend(serde_json::to_vec(value).unwrap());
            bytes
        };
        let mut bytes = encode(&value);
        std::fs::write(&path, &bytes).unwrap();
        let config = load_config(&path).unwrap();
        assert_eq!(config.port, 4280);
        assert_eq!(
            config.sources[0].canonical_file,
            PathBuf::from("source.canonical")
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        bytes.resize(65536, b' ');
        std::fs::write(&path, &bytes).unwrap();
        assert!(load_config(&path).is_ok());
        bytes.push(b' ');
        std::fs::write(&path, &bytes).unwrap();
        assert!(load_config(&path)
            .err()
            .unwrap()
            .to_string()
            .starts_with("ku_host_config_too_large:"));

        value["private-field-canary"] = serde_json::json!("private-value-canary");
        for bytes in [
            encode(&value),
            [vec![0xef, 0xbb, 0xbf], encode(&value)].concat(),
            vec![0xff, 0xfe, b'{', 0],
        ] {
            std::fs::write(&path, &bytes).unwrap();
            let error = load_config(&path).err().unwrap().to_string();
            assert!(error.starts_with("ku_host_config_invalid:"));
            assert!(!error.contains("canary"));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[tokio::test]
    async fn token_setup_failures_leave_new_and_existing_custody_untouched() {
        let root = tempfile::tempdir().unwrap();
        let token_path = root.path().join("private-token-path-canary.txt");
        let key_path = root.path().join("vault.bin");
        let key = [0x51; 32];
        std::fs::write(&key_path, key).unwrap();
        let data_dir = root.path().join("dataset");
        let registry_root = root.path().join("registry");
        let config = || Config {
            data_dir: data_dir.clone(),
            registry_root: registry_root.clone(),
            registry_public_key: "unused-private-value-canary".into(),
            vault_key_file: key_path.clone(),
            api_token_file: token_path.clone(),
            sources: Vec::new(),
            ollama: None,
            web_dir: root.path().join("web"),
            port: 0,
        };
        let error = run(config()).await.unwrap_err().to_string();
        assert!(error.starts_with("ku_api_token_unreadable:"));
        assert!(error.contains("api_token_file"));
        assert!(!error.contains("canary"));
        assert!(!data_dir.exists());
        assert!(!registry_root.exists());

        std::fs::create_dir(&data_dir).unwrap();
        let saved_path = data_dir.join("saved.bin");
        std::fs::write(&saved_path, b"saved private bytes").unwrap();
        for (bytes, code) in [
            (b"short-private-canary".to_vec(), "ku_api_token_invalid:"),
            (vec![0xff; 32], "ku_api_token_invalid:"),
            (
                format!("\"{}\"", "private-canary-".repeat(3)).into_bytes(),
                "ku_api_token_invalid:",
            ),
            (vec![b'a'; 1025], "ku_api_token_too_large:"),
        ] {
            std::fs::write(&token_path, &bytes).unwrap();
            let error = run(config()).await.unwrap_err().to_string();
            assert!(error.starts_with(code));
            assert!(error.contains("api_token_file"));
            assert!(!error.contains("canary"));
            assert!(!error.contains(&root.path().display().to_string()));
            assert!(!registry_root.exists());
            assert_eq!(std::fs::read(&key_path).unwrap(), key);
            assert_eq!(std::fs::read(&saved_path).unwrap(), b"saved private bytes");
            assert_eq!(std::fs::read(&token_path).unwrap(), bytes);
            assert_eq!(std::fs::read_dir(&data_dir).unwrap().count(), 1);
        }

        let token = "synthetic-private-token-canary-1234567890";
        std::fs::write(&token_path, format!(" \r\n{token}\t ")).unwrap();
        assert_eq!(load_api_token(&token_path).unwrap(), token);
        std::fs::write(&token_path, vec![b'a'; 1024]).unwrap();
        assert_eq!(load_api_token(&token_path).unwrap().len(), 1024);
    }

    #[tokio::test]
    async fn vault_key_setup_failures_preserve_custody_and_stop_before_registry_or_dataset() {
        let root = tempfile::tempdir().unwrap();
        let token_path = root.path().join("private-token-canary.txt");
        let key_path = root.path().join("private-key-canary.bin");
        let token = "synthetic-token-canary-1234567890123456";
        std::fs::write(&token_path, token).unwrap();
        let data_dir = root.path().join("dataset");
        let registry_root = root.path().join("registry");
        let saved_path = data_dir.join("saved-private-canary.bin");
        let config = || Config {
            data_dir: data_dir.clone(),
            registry_root: registry_root.clone(),
            registry_public_key: "unused-private-key-canary".into(),
            vault_key_file: key_path.clone(),
            api_token_file: token_path.clone(),
            sources: Vec::new(),
            ollama: None,
            web_dir: root.path().join("web"),
            port: 0,
        };

        for existing in [false, true] {
            if existing {
                std::fs::create_dir(&data_dir).unwrap();
                std::fs::write(&saved_path, b"synthetic saved private bytes").unwrap();
            }
            for (bytes, code) in [
                (None, "ku_vault_key_unreadable:"),
                (Some(Vec::new()), "ku_vault_key_invalid:"),
                (Some(vec![0x51; 31]), "ku_vault_key_invalid:"),
                (Some(vec![0x51; 33]), "ku_vault_key_too_large:"),
                (
                    Some(b"private-key-canary-".repeat(4)),
                    "ku_vault_key_too_large:",
                ),
            ] {
                if let Some(bytes) = &bytes {
                    std::fs::write(&key_path, bytes).unwrap();
                } else if key_path.exists() {
                    std::fs::remove_file(&key_path).unwrap();
                }
                let error = run(config()).await.unwrap_err().to_string();
                assert!(error.starts_with(code));
                assert!(error.contains("vault_key_file"));
                assert!(error.contains("original key"));
                assert!(!error.contains("canary"));
                assert!(!error.contains(&root.path().display().to_string()));
                assert!(!registry_root.exists());
                assert_eq!(std::fs::read(&token_path).unwrap(), token.as_bytes());
                if let Some(bytes) = bytes {
                    assert_eq!(std::fs::read(&key_path).unwrap(), bytes);
                } else {
                    assert!(!key_path.exists());
                }
                if existing {
                    assert_eq!(
                        std::fs::read(&saved_path).unwrap(),
                        b"synthetic saved private bytes"
                    );
                    assert_eq!(std::fs::read_dir(&data_dir).unwrap().count(), 1);
                } else {
                    assert!(!data_dir.exists());
                }
            }
        }

        // Exact binary length is the existing format contract, not key-origin
        // verification: even bytes that resemble text must remain accepted.
        for key in [vec![0x51; 32], vec![0xff; 32], vec![b' '; 32]] {
            std::fs::write(&key_path, &key).unwrap();
            assert_eq!(
                run(config()).await.unwrap_err().to_string(),
                REGISTRY_SETUP_ERROR
            );
            assert_eq!(std::fs::read(&key_path).unwrap(), key);
            assert_eq!(
                std::fs::read(&saved_path).unwrap(),
                b"synthetic saved private bytes"
            );
            assert!(!registry_root.exists());
        }
    }

    #[tokio::test]
    async fn registry_setup_failure_preserves_custody_and_does_not_create_a_dataset() {
        // Synthetic, test-only custody; no real Registry, keys or source are opened.
        let root = tempfile::tempdir().unwrap();
        let token = "synthetic-private-token-canary-1234567890";
        let key = [0x51; 32];
        let token_path = root.path().join("private-token-canary.txt");
        let key_path = root.path().join("private-key-canary.bin");
        std::fs::write(&token_path, token).unwrap();
        std::fs::write(&key_path, key).unwrap();
        let data_dir = root.path().join("dataset");
        let registry_root = root.path().join("registry");
        let public_key = onebrain_base_contract::ku_payload::hex(
            ed25519_dalek::SigningKey::from_bytes(&[0x42; 32])
                .verifying_key()
                .as_bytes(),
        );
        let config = || Config {
            data_dir: data_dir.clone(),
            registry_root: registry_root.clone(),
            registry_public_key: public_key.clone(),
            vault_key_file: key_path.clone(),
            api_token_file: token_path.clone(),
            sources: Vec::new(),
            ollama: None,
            web_dir: root.path().join("web"),
            port: 0,
        };

        let error = run(config()).await.unwrap_err().to_string();
        assert!(error.starts_with("ku_registry_unavailable: local KU host cannot start."));
        assert!(error.contains("registry_root"));
        assert!(error.contains("registry_public_key"));
        assert!(error.contains("releases/ and activation state/"));
        assert!(!error.contains(token));
        assert!(!error.contains("canary"));
        assert!(!error.contains(&root.path().display().to_string()));
        assert!(!data_dir.exists());
        assert!(!registry_root.exists());

        // An unsigned file is not an activated release; failed retries must leave
        // an existing dataset and operator custody inputs untouched.
        std::fs::create_dir(&data_dir).unwrap();
        let saved_path = data_dir.join("saved-private-canary");
        std::fs::write(&saved_path, b"synthetic saved bytes").unwrap();
        std::fs::create_dir(&registry_root).unwrap();
        let unsigned_path = registry_root.join("concepts.obr");
        std::fs::write(&unsigned_path, b"unsigned test-only placeholder").unwrap();
        assert_eq!(run(config()).await.unwrap_err().to_string(), error);
        assert_eq!(
            std::fs::read(&saved_path).unwrap(),
            b"synthetic saved bytes"
        );
        assert_eq!(std::fs::read(&key_path).unwrap(), key);
        assert_eq!(std::fs::read(&token_path).unwrap(), token.as_bytes());
        assert_eq!(
            std::fs::read(&unsigned_path).unwrap(),
            b"unsigned test-only placeholder"
        );
        assert_eq!(std::fs::read_dir(&data_dir).unwrap().count(), 1);
        assert_eq!(std::fs::read_dir(&registry_root).unwrap().count(), 1);
    }
}
