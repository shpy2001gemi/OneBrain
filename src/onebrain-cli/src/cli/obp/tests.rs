use super::*;
use clap::Parser;

fn parse(argv: &[&str]) -> Result<ObpArgs, clap::Error> {
    let args = crate::CliArgs::try_parse_from(
        ["onebrain", "obp"].into_iter().chain(argv.iter().copied()),
    )?;
    match args.command.unwrap() {
        crate::Commands::Obp(a) => Ok(a),
        _ => unreachable!(),
    }
}
fn wrap(data: Value) -> Value {
    json!({"ok":true,"profile":onebrain_api::vnext_api::VNEXT_PRODUCT_PROFILE,"data":data,
        "meta":{"lifecycle":"disabled","coverage":"local_only","limitations":[],"continuation":null}})
}
fn status() -> Value {
    let safe = contract::inventory()["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "safe_default")
        .unwrap();
    wrap(
        json!({"available":true,"session":{"process_generation":"11".repeat(32),"dataset_generation":"22".repeat(32)},"status":safe["value"]}),
    )
}
fn write(path: &Path, v: &Value) {
    std::fs::write(path, serde_json::to_vec(v).unwrap()).unwrap();
}
fn accept(key: &str) -> Result<String, String> {
    Ok(format!("{key}\n"))
}

#[test]
fn all_registered_commands_map_exactly_and_reject_extra_operations() {
    let dir = tempfile::tempdir().unwrap();
    let context = dir.path().join("context.json");
    let payload = dir.path().join("payload.json");
    write(&context, &status());
    for op in contract::inventory()["operations"].as_array().unwrap() {
        let name = op["name"].as_str().unwrap();
        if name == "status" {
            assert!(parse(&["status"])
                .unwrap()
                .command
                .prepare()
                .unwrap()
                .body
                .is_none());
            continue;
        }
        // Build request fields from the accepted DTO inventory, not CLI defaults.
        let required = &contract::inventory()["dtos"][op["request"].as_str().unwrap()]["required"];
        let mut v = json!({});
        for (field, ty) in required.as_object().unwrap() {
            let ty = ty.as_str().unwrap();
            let t = &contract::inventory()["types"][ty];
            v[field] = match t["kind"].as_str().unwrap() {
                "hex" => json!("33".repeat(32)),
                "boolean" => json!(false),
                "integer" => json!(1),
                "enum" => t["values"][0].clone(),
                _ => panic!("unexpected request type"),
            };
        }
        write(&payload, &v);
        let command = name.replace('_', "-");
        let argv = [
            command.as_str(),
            "--context-file",
            context.to_str().unwrap(),
            "--payload-file",
            payload.to_str().unwrap(),
        ];
        let p = parse(&argv).unwrap().command.prepare().unwrap();
        assert_eq!(p.name, name);
        assert_eq!(p.route == "commands", op["effect"] != "none");
        assert_eq!(p.management, op["access"] == "host_management");
        assert_eq!(p.body.unwrap()["payload"], v);
        assert!(parse(&[argv.as_slice(), &["--yes"]].concat()).is_err());
    }
    for unsupported in [
        "source-remove",
        "intent-cancel",
        "peers",
        "outbox-list",
        "connect",
    ] {
        assert!(parse(&[unsupported]).is_err());
    }
    assert!(parse(&[
        "reconcile",
        "--context-file",
        "c",
        "--idempotency-key",
        "ABC"
    ])
    .is_err());
}

#[test]
fn input_files_are_closed_bounded_and_session_fenced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("request.json");
    for raw in ["{\"limit\":1,\"limit\":2}", "{\"x\":1e999}", "[]"] {
        std::fs::write(&path, raw).unwrap();
        assert!(read_json(&path).is_err());
    }
    let f = std::fs::File::create(&path).unwrap();
    f.set_len(contract::MAX_BYTES as u64 + 1).unwrap();
    assert!(read_json(&path).is_err());
    for v in [
        json!({"limit":257}),
        json!({"limit":1,"continuation":null}),
        json!({"limit":1,"continuation":"raw"}),
        json!({"limit":1,"url":"http://example.com"}),
    ] {
        assert!(contract::Request::new("source_list", v, false).is_err());
    }
    let mut s = status();
    assert!(session(&s).is_ok());
    s["data"]["session"]["principal"] = json!("33".repeat(32));
    assert!(session(&s).is_err());
    assert!(session(&wrap(json!({"available":false,"compiled":true}))).is_err());
}

#[test]
fn destinations_and_reflected_credentials_fail_closed() {
    for url in [
        "http://example.com",
        "http://localhost",
        "http://secret@127.0.0.1",
        "http://127.0.0.1/?token=x",
        "http://127.0.0.1/path",
        "ftp://127.0.0.1",
    ] {
        assert!(Client::new(
            &parse(&["--api-url", url, "--api-token", "SECRET", "status"]).unwrap(),
            None
        )
        .is_err());
    }
    let c = Client::new(
        &parse(&["--api-token", "SECRET", "status"]).unwrap(),
        Some("MANAGEMENT".into()),
    )
    .unwrap();
    assert!(c.has_secret(&json!({"nested":[{"message":"reflect SECRET"}]})));
    assert!(c.has_secret(&json!({"MANAGEMENT":"x"})));
    assert!(!c.has_secret(&status()));
}

#[test]
fn transport_outcome_fixtures_and_expected_peer_are_enforced() {
    let transport: Value = serde_json::from_str(include_str!(
        "../../../../test-vectors/vnext/obp-local-api-v1.json"
    ))
    .unwrap();
    for f in transport["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == "outcome")
    {
        let data = f["value"].clone();
        let name = data["operation"].as_str().unwrap_or("refresh").to_owned();
        assert_eq!(
            validate_response(&name, &wrap(data), None).is_ok(),
            f["valid"].as_bool().unwrap(),
            "{}",
            f["name"]
        );
    }
    for f in contract::inventory()["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["dto"] == "ObpRouteV1" || f["dto"] == "ObpIntentV1")
    {
        let name = if f["dto"] == "ObpRouteV1" {
            "route_status"
        } else {
            "intent_status"
        };
        assert_eq!(
            validate_response(name, &wrap(f["value"].clone()), None).is_ok(),
            f["valid"].as_bool().unwrap()
        );
    }
    let route = contract::inventory()["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["dto"] == "ObpRouteV1" && f["valid"] == true)
        .unwrap()["value"]
        .clone();
    assert!(check_target(
        "route_request",
        &route,
        Some(&json!({"payload":{"expected_peer":"ff".repeat(32)}}))
    )
    .is_err());
    let unknown = wrap(
        json!({"idempotency_key":"33".repeat(32),"operation":"refresh","state":"reconcile_required","reconcile_before_retry":true}),
    );
    assert!(validate_response(
        "reconcile",
        &unknown,
        Some(&json!({"idempotency_key":"44".repeat(32)}))
    )
    .is_err());
}

#[test]
fn every_registered_error_preserves_retry_and_reconciliation_semantics() {
    let transport: Value = serde_json::from_str(include_str!(
        "../../../../test-vectors/vnext/obp-local-api-v1.json"
    ))
    .unwrap();
    for policy in transport["errors"].as_array().unwrap() {
        let mut v = wrap(json!({}));
        v.as_object_mut().unwrap().remove("data");
        v["ok"] = json!(false);
        v["error"] = json!({"code":policy["code"],"message":policy["reason"],"retryable":policy["retryable"],
            "limitations":[policy["reason"]],"obp":{"reason":policy["reason"],"outcome":policy["outcome"],"reconcile_before_retry":policy["reconcile_before_retry"]}});
        assert!(validate_error(&v).is_ok(), "{}", policy["reason"]);
        v["error"]["obp"]["reconcile_before_retry"] =
            json!(!policy["reconcile_before_retry"].as_bool().unwrap());
        assert!(validate_error(&v).is_err());
    }
}

#[tokio::test]
async fn transport_gap_does_not_replay_and_reflection_redirect_overflow_are_rejected() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for mode in ["gap", "reflection", "redirect", "overflow"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).await.unwrap();
            let text = String::from_utf8_lossy(&buffer[..n]);
            assert!(text.contains("Bearer PRIVATE_TOKEN"));
            assert!(!text
                .to_ascii_lowercase()
                .contains("x-onebrain-obp-management"));
            match mode {
                "gap" => {}
                "reflection" => {
                    let body = "{\"leak\":\"PRIVATE_TOKEN\"}";
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                }
                "redirect" => {
                    stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/\r\nContent-Length: 2\r\n\r\n{}").await.unwrap();
                }
                _ => {
                    let body = " ".repeat(contract::MAX_BYTES + 1);
                    let _ = stream
                        .write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        )
                        .await;
                }
            }
            drop(stream);
            assert!(
                tokio::time::timeout(Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let error = run(
            &parse(&["--api-url", &url, "--api-token", "PRIVATE_TOKEN", "status"]).unwrap(),
            Some("PRIVATE_MANAGEMENT".into()),
            accept,
        )
        .await
        .unwrap_err();
        assert!(!error.contains("PRIVATE_TOKEN"));
        assert!(error.contains("reconcile"));
        task.await.unwrap();
    }
}

#[tokio::test]
async fn real_api_node_workflow_preserves_authority_replay_and_session_context() {
    use ku_core::foundation::{MetabolicViewPolicy, ObjectReference};
    use onebrain_node::*;
    let dir = tempfile::tempdir().unwrap();
    let mut config = NodeConfig {
        port: 0,
        data_dir: dir.path().into(),
        concept_registry_mode: ConceptRegistryMode::Disabled,
        ..Default::default()
    };
    config.vnext.enabled.object_event_v1 = true;
    config.vnext.enabled.obp_rp = true;
    let mut node = OneBrainNode::new(config).await.unwrap();
    let policies = LocalPolicyRegistry::new([(
        LocalPolicyVersion::new(1).unwrap(),
        MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [3; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [4; 32])],
            recent_event_horizon: 64,
        },
    )])
    .unwrap();
    node.set_vnext_identity_signer(std::sync::Arc::new(ed25519_dalek::SigningKey::from_bytes(
        &[7; 32],
    )));
    node.set_vnext_product_dependencies(VNextProductRuntimeDependencies::new(
        ku_kql::vnext_private_need::LocalNeedVaultKey::from_bytes([5; 32]),
        policies,
    ))
    .unwrap();
    node.start_network().await.unwrap();
    let host = node.obp_host().unwrap();
    let control = host.control([1; 32]).unwrap();
    let management = host
        .management(
            [1; 32],
            vec![
                "network_kill".into(),
                "network_reenable".into(),
                "configure".into(),
            ],
            Duration::from_secs(300),
        )
        .unwrap();
    let shared = std::sync::Arc::new(tokio::sync::Mutex::new(node));
    let server =
        onebrain_api::ApiServer::with_shared_node(shared.clone(), "PRIVATE_TOKEN".into(), 0)
            .with_obp_binding(onebrain_api::obp_api::Binding::new([1; 32], Some(control)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let serving = tokio::spawn(async move {
        axum::serve(listener, server.build_router()).await.unwrap();
    });
    let args = |argv: &[&str]| {
        parse(
            &[
                &["--api-url", url.as_str(), "--api-token", "PRIVATE_TOKEN"],
                argv,
            ]
            .concat(),
        )
        .unwrap()
    };
    let s = run(&args(&["status"]), None, accept).await.unwrap();
    assert_eq!(s["data"]["status"]["requested"], false);
    let generation = s["data"]["status"]["generation"].clone();
    let context = dir.path().join("context.json");
    write(&context, &s);
    let payload = dir.path().join("payload.json");
    let command = |name| {
        args(&[
            name,
            "--context-file",
            context.to_str().unwrap(),
            "--payload-file",
            payload.to_str().unwrap(),
        ])
    };
    write(&payload, &json!({"limit":1}));
    for name in ["source-list", "reservation-list"] {
        let page = run(&command(name), None, accept).await.unwrap();
        assert_eq!(page["data"]["items"], json!([]));
    }
    // Missing host intake/runtime never falls back to addresses or a new owner.
    write(
        &payload,
        &json!({"idempotency_key":"66".repeat(32),"expected_generation":generation}),
    );
    assert!(run(&command("refresh"), None, accept).await.is_err());
    for (name, request) in [
        ("route-status", json!({"route_id":"55".repeat(32)})),
        ("intent-status", json!({"intent_id":"55".repeat(32)})),
        (
            "route-request",
            json!({"idempotency_key":"55".repeat(32),"expected_generation":generation,"expected_peer":"77".repeat(32)}),
        ),
        (
            "intent-retry",
            json!({"idempotency_key":"55".repeat(32),"expected_generation":generation,"intent_id":"77".repeat(32)}),
        ),
        (
            "source-admit",
            json!({"idempotency_key":"55".repeat(32),"expected_generation":generation,"input_ref":"77".repeat(32),"kind":"manual_invitation"}),
        ),
        (
            "source-set-enabled",
            json!({"idempotency_key":"55".repeat(32),"expected_generation":generation,"source_id":"77".repeat(32),"enabled":false}),
        ),
    ] {
        write(&payload, &request);
        assert!(run(&command(name), Some(management.clone()), accept)
            .await
            .is_err());
    }
    write(
        &payload,
        &json!({"idempotency_key":"88".repeat(32),"expected_generation":generation,"outbound_first_requested":false,"advertise_reachability":false}),
    );
    let configured = run(&command("configure"), Some(management.clone()), accept)
        .await
        .unwrap();
    assert_eq!(configured["data"]["state"], "completed");
    assert_eq!(configured["data"]["result"]["requested"], false);
    let key = "33".repeat(32);
    let kill = json!({"idempotency_key":key,"expected_generation":generation});
    write(&payload, &kill);
    assert!(run(&command("network-kill"), None, accept)
        .await
        .unwrap_err()
        .contains("host-scoped"));
    assert!(
        run(&command("network-kill"), Some(management.clone()), |_| Ok(
            "yes".into()
        ))
        .await
        .is_err()
    );
    assert_eq!(
        run(&args(&["status"]), None, accept).await.unwrap()["data"]["status"]["generation"],
        generation
    );
    assert!(run(
        &command("network-kill"),
        Some("wrong-capability".into()),
        accept
    )
    .await
    .is_err());
    let killed = run(&command("network-kill"), Some(management.clone()), accept)
        .await
        .unwrap();
    assert_eq!(killed["data"]["state"], "completed");
    assert_eq!(killed["data"]["result"]["kill_switch"], true);
    assert_eq!(
        run(&command("network-kill"), Some(management.clone()), accept)
            .await
            .unwrap()["data"],
        killed["data"]
    );
    let reconcile = args(&[
        "reconcile",
        "--context-file",
        context.to_str().unwrap(),
        "--idempotency-key",
        &key,
    ]);
    let outcome = run(&reconcile, None, accept).await.unwrap();
    assert_eq!(outcome["data"]["state"], "completed");
    assert!(outcome["data"].get("result").is_none());
    write(
        &payload,
        &json!({"idempotency_key":key,"expected_generation":999}),
    );
    assert!(
        run(&command("network-kill"), Some(management.clone()), accept)
            .await
            .is_err()
    );
    write(
        &payload,
        &json!({"idempotency_key":"44".repeat(32),"expected_generation":generation}),
    );
    assert!(run(
        &command("network-reenable"),
        Some(management.clone()),
        accept
    )
    .await
    .is_err());
    let mut stale = s.clone();
    stale["data"]["session"]["dataset_generation"] = json!("ff".repeat(32));
    write(&context, &stale);
    assert!(run(&reconcile, None, accept).await.is_err());
    write(&context, &s);
    write(&payload, &kill);
    host.revoke_management(&management).unwrap();
    assert!(run(&command("network-kill"), Some(management), accept)
        .await
        .is_err());
    assert_eq!(
        run(&reconcile, None, accept).await.unwrap()["data"]["state"],
        "completed"
    );
    let current = run(&args(&["status"]), None, accept).await.unwrap();
    let fresh_grant = host
        .management(
            [1; 32],
            vec!["network_reenable".into()],
            Duration::from_secs(300),
        )
        .unwrap();
    write(
        &payload,
        &json!({"idempotency_key":"99".repeat(32),"expected_generation":current["data"]["status"]["generation"]}),
    );
    let enabled = run(&command("network-reenable"), Some(fresh_grant), accept)
        .await
        .unwrap();
    assert_eq!(enabled["data"]["result"]["kill_switch"], false);
    assert_eq!(enabled["data"]["result"]["requested"], false);
    serving.abort();
    let _ = serving.await;
    shared.lock().await.shutdown_network().await;
}

#[tokio::test]
async fn lost_command_reply_is_unknown_and_never_automatically_replayed() {
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = vec![0; 4096];
        let n = stream.read(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes[..n]);
        assert!(text.starts_with("POST /api/vnext/obp/commands "));
        assert!(text
            .to_ascii_lowercase()
            .contains("x-onebrain-obp-management: private_management"));
        drop(stream);
        assert!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err()
        );
    });
    let dir = tempfile::tempdir().unwrap();
    let context = dir.path().join("context.json");
    write(&context, &status());
    let payload = dir.path().join("payload.json");
    write(
        &payload,
        &json!({"idempotency_key":"33".repeat(32),"expected_generation":1}),
    );
    let args = parse(&[
        "--api-url",
        &url,
        "--api-token",
        "PRIVATE_TOKEN",
        "network-kill",
        "--context-file",
        context.to_str().unwrap(),
        "--payload-file",
        payload.to_str().unwrap(),
    ])
    .unwrap();
    assert_eq!(
        run(&args, Some("PRIVATE_MANAGEMENT".into()), accept)
            .await
            .unwrap_err(),
        UNKNOWN
    );
    task.await.unwrap();
}
