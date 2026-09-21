use super::*;
use clap::Parser;

fn parse(argv: &[&str]) -> Result<KuArgs, clap::Error> {
    let args =
        crate::CliArgs::try_parse_from(["onebrain", "ku"].into_iter().chain(argv.iter().copied()))?;
    match args.command.unwrap() {
        crate::Commands::Ku(args) => Ok(args),
        _ => unreachable!(),
    }
}

#[test]
fn all_operations_parse_and_reject_bypasses_and_untyped_ids() {
    for command in ["prepare", "save", "revise", "export"] {
        assert!(parse(&[command, "--payload-file", "request.json"]).is_ok());
        assert!(parse(&[command, "--payload-file", "request.json", "--yes"]).is_err());
    }
    for command in ["preview", "cancel", "reconcile", "status"] {
        assert!(parse(&[command, "--operation-id", &"ab".repeat(32)]).is_ok());
        assert!(parse(&[command, "--operation-id", "abbreviated"]).is_err());
        assert!(parse(&[command, "--operation-id", &"AB".repeat(32)]).is_err());
    }
    for argv in [
        vec!["reserve"],
        vec!["status"],
        vec!["list"],
        vec!["search", "--query", "water"],
    ] {
        assert!(parse(&argv).is_ok());
    }
    assert!(parse(&["get", "--object-cid", &"ab".repeat(32)]).is_ok());
    assert!(parse(&["list", "--limit", "257"]).is_err());
    assert!(parse(&["list", "--max-bytes", "1"]).is_err());
    assert!(parse(&["list", "--continuation", "invented"])
        .unwrap()
        .command
        .request()
        .is_err());
    assert!(parse(&["search", "--query", &"x".repeat(4097)])
        .unwrap()
        .command
        .request()
        .is_err());
}

#[test]
fn dto_rejects_duplicate_null_unknown_and_oversize_inputs() {
    for bytes in [
        br#"{"limit":1,"limit":2}"#.as_slice(),
        br#"{"limit":1,"continuation":null}"#,
        br#"{"limit":1,"authorized":true}"#,
    ] {
        assert!(dto::<KuListV1>(bytes).is_err());
    }
    assert!(dto::<KuGetV1>(br#"{"semantic_content_cid":"not-an-object-id"}"#).is_err());
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file()
        .set_len(MAX_KU_PAYLOAD_BYTES as u64 + 1)
        .unwrap();
    assert!(file_dto::<KuPrepareV1>(&PayloadFile {
        payload_file: file.path().into()
    })
    .is_err());
    let id = "ab".repeat(32);
    assert!(verify_cancel(&id, &(id.clone() + "\r\n")).is_ok());
    for typed in ["yes", "", "ab"] {
        assert!(verify_cancel(&id, typed).is_err());
    }
}

#[test]
fn local_transport_rejects_credential_and_remote_destinations() {
    for url in [
        "http://example.com",
        "http://secret@127.0.0.1",
        "http://localhost/?token=secret",
        "file:///tmp/token",
        "http://127.0.0.1/base",
    ] {
        let args = parse(&["--api-url", url, "--api-token", "private-token", "status"]).unwrap();
        assert!(Client::new(&args).is_err());
    }
    for url in ["http://127.0.0.1:4280", "http://[::1]:4280"] {
        assert!(Client::new(
            &parse(&["--api-url", url, "--api-token", "private-token", "status"]).unwrap()
        )
        .is_ok());
    }
}

#[tokio::test]
async fn lost_oversize_redirect_and_failure_replies_never_replay() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (status, body, expected) in [
        ("200 OK", "x".repeat(32769), "exceeds --max-bytes"),
        ("200 OK", "not-json".into(), "outcome unknown"),
        ("302 Found", "{}".into(), "outcome unknown"),
        ("409 Conflict", json!({"ok":false,"profile":onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE,"error":{"code":"conflict","discriminator":onebrain_base_contract::BaseErrorCodeV1::UnknownOutcome.discriminator(),"failure":{"code":"UnknownOutcome","retryable":true,"reconcile_before_retry":true,"limitations":["unknown_outcome"]}}}).to_string(), "KU API failure"),
        ("200 OK", json!({"echo":"private-token"}).to_string(), "reflected credentials"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let host = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8;4096];
            let read = stream.read(&mut request).await.unwrap();
            assert!(String::from_utf8_lossy(&request[..read]).starts_with("POST /api/vnext/ku/operations"));
            let reply = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nLocation: http://example.test/private\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = stream.write_all(reply.as_bytes()).await;
            drop(stream);
            assert!(tokio::time::timeout(Duration::from_millis(100), listener.accept()).await.is_err(), "no retry or redirect is permitted");
        });
        let args = parse(&["--api-url", &url, "--api-token", "private-token", "--max-bytes", "32768", "status"]).unwrap();
        let error = Client::new(&args).unwrap().call(Method::POST, "/api/vnext/ku/operations", Some(json!({"request":{"operation":"save"}}))).await.unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains("private-token"));
        host.await.unwrap();
    }
}

#[cfg(feature = "base-v1")]
#[path = "fixture.rs"]
mod fixture;
#[cfg(feature = "base-v1")]
#[path = "../../../../onebrain-api/src/ku_api/tests/registry_fixture.rs"]
mod registry_fixture;

#[cfg(feature = "base-v1")]
#[tokio::test]
async fn api_backed_cli_workflow_preserves_private_save_paging_and_reconcile() {
    use ku_core::foundation::VaultKey;
    use onebrain_node::{ku_product::KuRuntimeConfig, NodeConfig, OneBrainNode};
    use std::sync::{atomic::Ordering, Arc};
    let dir = tempfile::tempdir().unwrap();
    let registry = registry_fixture::registry(dir.path());
    let root = registry
        .reader_lease()
        .status()
        .release_aggregate_root
        .clone()
        .unwrap();
    let inputs = Arc::new(fixture::Inputs::new());
    let config = NodeConfig {
        data_dir: dir.path().join("node"),
        concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    std::fs::create_dir_all(&config.data_dir).unwrap();
    let mut node = OneBrainNode::new(config).await.unwrap();
    let token = "ku-cli-test-token";
    let mut runtime = onebrain_api::base_runtime_config_for_api_token(token);
    runtime.ku = Some(KuRuntimeConfig {
        vault_key: VaultKey::from_bytes([8; 32]),
        registry: Some(registry),
        inputs: inputs.clone(),
        public: None,
    });
    node.install_base_runtime(runtime).unwrap();
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let server = onebrain_api::ApiServer::new(node, token.into(), port);
    let task = tokio::spawn(async move {
        server.start().await.unwrap();
    });
    let url = format!("http://127.0.0.1:{port}");
    let args = |command: &[&str]| {
        let mut argv = vec!["--api-url", url.as_str(), "--api-token", token];
        argv.extend_from_slice(command);
        parse(&argv).unwrap()
    };
    let mut ready = false;
    for _ in 0..100 {
        if run(&args(&["status"])).await.is_ok() {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ready);
    let reserved = run(&args(&["reserve"])).await.unwrap();
    let op = reserved["data"]["payload"]["operation_id"]
        .as_str()
        .unwrap();
    let prepare = json!({"operation_id":op,"idempotency_key":op,"input_mode":"resolved_semantic_draft","source_refs":[inputs.cid],"registry_release_root":root,"semantic_profile":"ku-semantic-content/1.0","implementation_commitment":"04".repeat(32),"destination":"LOCAL_ONLY","draft_ref":"01".repeat(32)});
    let path = dir.path().join("request.json");
    let path_str = path.to_str().unwrap();
    std::fs::write(&path, prepare.to_string()).unwrap();
    let prepared = run(&args(&["prepare", "--payload-file", path_str]))
        .await
        .unwrap();
    let replay = run(&args(&["prepare", "--payload-file", path_str]))
        .await
        .unwrap();
    assert_eq!(prepared["data"]["payload"], replay["data"]["payload"]);
    assert_eq!(inputs.calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepared["data"]["payload"]["validity"], "ready");
    let preview = run(&args(&["preview", "--operation-id", op]))
        .await
        .unwrap();
    assert_eq!(prepared["data"]["payload"], preview["data"]["payload"]);
    assert_eq!(
        run(&args(&["list"])).await.unwrap()["data"]["payload"]["items"],
        json!([])
    );
    let ids = prepared["data"]["payload"]["object_cids"].clone();
    std::fs::write(
        &path,
        json!({"operation_id":op,"idempotency_key":op,"object_cids":ids}).to_string(),
    )
    .unwrap();
    let saved = run(&args(&["save", "--payload-file", path_str]))
        .await
        .unwrap();
    assert_eq!(saved["data"]["payload"]["state"], "committed");
    assert_eq!(saved["data"]["payload"]["published"], false);
    assert_eq!(saved["data"]["payload"]["authorizes_reward"], false);
    assert_eq!(
        run(&args(&["save", "--payload-file", path_str]))
            .await
            .unwrap()["data"]["payload"],
        saved["data"]["payload"]
    );
    assert_eq!(
        run(&args(&["reconcile", "--operation-id", op]))
            .await
            .unwrap()["data"]["payload"],
        saved["data"]["payload"]
    );
    assert_eq!(
        run(&args(&["status", "--operation-id", op])).await.unwrap()["data"]["payload"]["receipt"],
        saved["data"]["payload"]
    );
    let first = run(&args(&["list", "--limit", "1"])).await.unwrap();
    assert_eq!(first["meta"]["coverage"], "partial");
    let continuation = first["data"]["payload"]["continuation"].as_str().unwrap();
    let second = run(&args(&[
        "list",
        "--limit",
        "1",
        "--continuation",
        continuation,
    ]))
    .await
    .unwrap();
    assert_ne!(
        first["data"]["payload"]["items"],
        second["data"]["payload"]["items"]
    );
    assert!(run(&args(&[
        "search",
        "--query",
        "water",
        "--continuation",
        continuation
    ]))
    .await
    .is_err());
    let found = run(&args(&["search", "--query", "water"])).await.unwrap();
    assert_eq!(
        found["data"]["payload"]["items"].as_array().unwrap().len(),
        2
    );
    let get = run(&args(&["get", "--object-cid", ids[0].as_str().unwrap()]))
        .await
        .unwrap();
    assert_eq!(
        get["data"]["payload"]["canonical_bytes"],
        prepared["data"]["payload"]["artifacts"][0]["canonical_preview"]
    );
    let new = run(&args(&["reserve"])).await.unwrap();
    let next = new["data"]["payload"]["operation_id"].as_str().unwrap();
    let mut successor = prepare.clone();
    successor["operation_id"] = json!(next);
    successor["idempotency_key"] = json!(next);
    successor["draft_ref"] = json!("02".repeat(32));
    std::fs::write(&path, json!({"preparation":successor,"predecessor_object_cid":ids[0],"expected_revision_frontier":found["data"]["payload"]["snapshot_frontier"]}).to_string()).unwrap();
    let revised = run(&args(&["revise", "--payload-file", path_str]))
        .await
        .unwrap();
    assert_ne!(revised["data"]["payload"]["object_cids"], ids);
    let cancel = args(&["cancel", "--operation-id", next]);
    assert!(run_with_confirmation(&cancel, |_| Ok("yes".into()))
        .await
        .is_err());
    let canceled = run_with_confirmation(&cancel, |_| Ok(next.into()))
        .await
        .unwrap();
    assert_eq!(canceled["data"]["payload"]["state"], "canceled");
    std::fs::write(
        &path,
        json!({"object_cids":ids,"mode":"canonical_public_exchange"}).to_string(),
    )
    .unwrap();
    assert!(run(&args(&["export", "--payload-file", path_str]))
        .await
        .is_err());
    task.abort();
    let _ = task.await;
}
