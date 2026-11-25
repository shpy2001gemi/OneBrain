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
    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("ku-private-canary-token.txt");
    let valid_token = "ku-client-token-file-0123456789-abcdef";
    std::fs::write(&token_file, format!("{valid_token}\n")).unwrap();
    let from_file = parse(&[
        "--api-url",
        "http://127.0.0.1:4280",
        "--api-token-file",
        token_file.to_str().unwrap(),
        "status",
    ])
    .unwrap();
    let client = Client::new(&from_file).unwrap();
    assert_eq!(client.token, valid_token);
    assert!(parse(&[
        "--api-token",
        "private-token",
        "--api-token-file",
        token_file.to_str().unwrap(),
        "status",
    ])
    .is_err());
    std::fs::write(&token_file, b"short-private-canary").unwrap();
    let err = Client::new(&from_file).err().unwrap();
    assert_eq!(err, crate::API_TOKEN_INVALID);
    assert!(!err.contains("canary"));
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

#[test]
fn share_commands_parse_exact_payloads_and_reject_bypasses() {
    let hex = |b: &str| b.repeat(32);
    let (a, n, s, ns) = (hex("ab"), hex("cd"), hex("02"), hex("03"));
    let prepare = [
        "share",
        "prepare",
        "--object-cid",
        &a,
        "--expected-node-id",
        &n,
        "--address",
        "127.0.0.1:4433",
        "--selector",
        &s,
        "--namespace",
        &ns,
    ];
    let KuCommand::Share(share) = parse(&prepare).unwrap().command else {
        panic!("share")
    };
    assert_eq!(
        share.request(),
        (
            "share_prepare",
            json!({"object_cid":a,"expected_node_id":n,"address":"127.0.0.1:4433","selector":s,"namespace":ns})
        )
    );
    // Missing/typo'd/uppercase IDs and invented bypass flags fail before any request.
    assert!(parse(&prepare[..10]).is_err());
    let mut upper = prepare.to_vec();
    let upper_cid = hex("AB");
    upper[3] = upper_cid.as_str();
    assert!(parse(&upper).is_err());
    let mut bad_addr = prepare.to_vec();
    bad_addr[7] = "not-an-address";
    assert!(parse(&bad_addr).is_err());
    for argv in [
        vec!["share", "confirm", "--prepared-id", &a, "--yes"],
        vec!["share", "confirm", "--prepared-id", &a, "--confirm-public"],
        vec!["share", "confirm", "--prepared-id", "abbreviated"],
        vec!["share", "list", "--limit", "0"],
        vec!["share", "inspect", "--object-cid", &a],
    ] {
        assert!(parse(&argv).is_err(), "{argv:?}");
    }
    let KuCommand::Share(confirm) = parse(&["share", "confirm", "--prepared-id", &a])
        .unwrap()
        .command
    else {
        panic!("share")
    };
    assert_eq!(
        confirm.request(),
        (
            "share_confirm",
            json!({"prepared_id":a,"confirm_public":true})
        )
    );
    let KuCommand::Share(list) = parse(&["share", "list"]).unwrap().command else {
        panic!("share")
    };
    assert_eq!(list.request(), ("share_list", json!({})));
    let KuCommand::Share(list) = parse(&["share", "list", "--query", "water", "--limit", "5"])
        .unwrap()
        .command
    else {
        panic!("share")
    };
    assert_eq!(
        list.request(),
        ("share_list", json!({"query":"water","limit":5}))
    );
    for (argv, action) in [
        (vec!["share", "intent", "--intent-id", &a], "share_intent"),
        (
            vec!["share", "inspect", "--object-cid", &a, "--selector", &s],
            "share_inspect",
        ),
    ] {
        let KuCommand::Share(cmd) = parse(&argv).unwrap().command else {
            panic!("share")
        };
        assert_eq!(cmd.request().0, action);
    }
    assert!(parse(&prepare).unwrap().command.request().is_err());
}

#[tokio::test]
async fn share_confirm_requires_exact_typed_prepared_id_before_any_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let id = "ab".repeat(32);
    let args = parse(&[
        "--api-url",
        &url,
        "--api-token",
        "private-token",
        "share",
        "confirm",
        "--prepared-id",
        &id,
    ])
    .unwrap();
    for typed in ["yes", "", "ab", "y\n"] {
        let error = run_with_confirmation(&args, |_| Ok(typed.into()))
            .await
            .unwrap_err();
        assert!(error.contains("nothing was confirmed"), "{error}");
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), listener.accept())
            .await
            .is_err(),
        "a rejected confirmation must not contact the host"
    );
    // The exact typed prepared_id sends confirm_public=true to the registered editor action.
    let session =
        json!({"process_generation":"11".repeat(32),"dataset_generation":"22".repeat(32)});
    let envelope = |payload: Value| {
        json!({"ok":true,"profile":onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE,
            "data":{"session":session,"payload":payload,"model_qualified":false},"meta":{}})
        .to_string()
    };
    let status_reply = envelope(
        json!({"lifecycle":"active","coverage":"local_only","limitations":[],
        "registry_ready":true,"local_encoder_ready":false,"remote_encoding_enabled":false,"direct_issuance_enabled":false}),
    );
    let confirm_reply = envelope(
        json!({"event":"share_confirmed","intent_id":"33".repeat(32),"authorizes_reward":false}),
    );
    let expected_id = id.clone();
    let host = tokio::spawn(async move {
        let mut requests = Vec::new();
        for reply in [status_reply, confirm_reply] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let read = stream.read(&mut buf).await.unwrap();
                raw.extend_from_slice(&buf[..read]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if raw.len() >= end + 4 + length || read == 0 {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8_lossy(&raw).to_string());
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    let confirmed = run_with_confirmation(&args, |shown| {
        assert_eq!(shown, expected_id);
        Ok(format!("{shown}\r\n"))
    })
    .await
    .unwrap();
    assert_eq!(confirmed["data"]["payload"]["event"], "share_confirmed");
    let requests = host.await.unwrap();
    assert!(requests[0].starts_with("GET /api/vnext/ku/status"));
    assert!(requests[1].starts_with("POST /api/vnext/ku/editor"));
    let body: Value = serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body["request"],
        json!({"action":"share_confirm","payload":{"prepared_id":id,"confirm_public":true}})
    );
    assert_eq!(body["session"], session);
}

#[test]
fn share_responses_must_match_event_and_never_claim_reward_or_sending_preview() {
    let session =
        json!({"process_generation":"11".repeat(32),"dataset_generation":"22".repeat(32)});
    let env = |payload: Value| {
        json!({"ok":true,"profile":onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE,
            "data":{"session":session,"payload":payload,"model_qualified":false},"meta":{}})
    };
    assert!(validate_response(
        "share_prepare",
        &env(json!({"event":"share_prepared","enqueued":false,"payload_sent":false}))
    )
    .is_ok());
    for (op, payload) in [
        (
            "share_prepare",
            json!({"event":"share_prepared","enqueued":true,"payload_sent":false}),
        ),
        ("share_prepare", json!({"event":"share_prepared"})),
        (
            "share_confirm",
            json!({"event":"share_confirmed","authorizes_reward":true}),
        ),
        ("share_intent", json!({"event":"public_object"})),
        ("share_list", json!({"event":"intent"})),
    ] {
        assert!(
            validate_response(op, &env(payload.clone())).is_err(),
            "{op} {payload}"
        );
    }
    for (op, event) in [
        ("share_confirm", "share_confirmed"),
        ("share_intent", "intent"),
        ("share_inspect", "public_object"),
        ("share_list", "public_objects"),
    ] {
        assert!(
            validate_response(op, &env(json!({"event":event,"authorizes_reward":false}))).is_ok()
        );
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
