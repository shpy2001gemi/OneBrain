use super::super::tests::{all_lanes_config, dependencies};
use super::*;

async fn start(path: &Path) -> VNextProductRuntime {
    VNextProductRuntime::start(
        path,
        "127.0.0.1:0".parse().unwrap(),
        &all_lanes_config(),
        dependencies(45),
        None,
    )
    .await
    .unwrap()
}
fn command(name: &str, key: u8, generation: u64) -> Request {
    Request::new(
        name,
        json!({"idempotency_key":hex(&[key;32]),"expected_generation":generation}),
        true,
    )
    .unwrap()
}
fn authorize_host(runtime: &VNextProductRuntime) -> (Control, String) {
    let host = runtime.obp_host();
    (
        host.control([1; 32]).unwrap(),
        host.management(
            [1; 32],
            vec![
                "network_kill".into(),
                "network_reenable".into(),
                "configure".into(),
            ],
            Duration::from_secs(300),
        )
        .unwrap(),
    )
}

#[test]
fn strict_transport_never_accepts_duplicate_authority_or_unbounded_json() {
    for fixture in contract::inventory()["fixtures"].as_array().unwrap() {
        assert_eq!(
            contract::validate(fixture["dto"].as_str().unwrap(), &fixture["value"]).is_ok(),
            fixture["valid"].as_bool().unwrap(),
            "{}",
            fixture["name"]
        );
    }
    for bytes in [
        br#"{"x":1,"x":2}"#.as_slice(),
        br#"{"payload":{"a":1,"a":2}}"#,
        br#"{"x":1e999}"#,
        b"[]",
        b"\xff",
    ] {
        assert!(contract::parse(bytes).is_err());
    }
    let bytes = format!("{{\"x\":{}0{}}}", "[".repeat(17), "]".repeat(17));
    assert!(contract::parse(bytes.as_bytes()).is_err());
    assert!(Request::new(
        "network_kill",
        json!({"idempotency_key":hex(&[1;32]),"expected_generation":1,"authorized":true}),
        true
    )
    .is_err());
    assert!(Request::new(
        "network_kill",
        json!({"idempotency_key":hex(&[1;32]),"expected_generation":true}),
        true
    )
    .is_err());
    assert!(Request::new("source_list", json!({"limit":1,"continuation":null}), false).is_err());
}

#[tokio::test]
async fn real_owner_kill_replay_restart_reconcile_and_stale_authority() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let (control, token) = authorize_host(&runtime);
    let gen = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                None,
                command("network_kill", 1, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "forbidden"
    );
    assert!(services.obp_reconcile([1; 32], &session, [1; 32]).is_err());
    let result = services
        .obp_command(
            [1; 32],
            &session,
            &control,
            Some(&token),
            command("network_kill", 1, gen),
        )
        .await
        .unwrap();
    assert_eq!(result["state"], "completed");
    assert_eq!(result["result"]["kill_switch"], true);
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_kill", 1, gen)
            )
            .await
            .unwrap(),
        result
    );
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_reenable", 1, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "idempotency_conflict"
    );
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_reenable", 2, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "generation_conflict"
    );
    let metadata = services.obp_reconcile([1; 32], &session, [1; 32]).unwrap();
    assert!(metadata.get("result").is_none());
    assert!(services.obp_reconcile([2; 32], &session, [1; 32]).is_err());
    runtime.shutdown().await;
    drop(runtime);
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let next = services.obp_session().unwrap();
    assert_ne!(next.process_generation, session.process_generation);
    assert_eq!(next.dataset_generation, session.dataset_generation);
    assert!(services.obp_reconcile([1; 32], &session, [1; 32]).is_err());
    assert_eq!(
        services.obp_reconcile([1; 32], &next, [1; 32]).unwrap(),
        metadata
    );
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &next,
                &control,
                Some(&token),
                command("network_kill", 1, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "forbidden"
    );
    let (control, token) = authorize_host(&runtime);
    let gen = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    let result = services
        .obp_command(
            [1; 32],
            &next,
            &control,
            Some(&token),
            command("network_reenable", 2, gen),
        )
        .await
        .unwrap();
    assert_eq!(result["state"], "completed");
    assert_eq!(result["result"]["kill_switch"], false);
    control.revoke();
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &next,
                &control,
                Some(&token),
                command("network_reenable", 2, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "forbidden"
    );
    runtime.shutdown().await;
}

#[tokio::test]
async fn configuration_and_pages_stay_local_and_generation_bound() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let (control, token) = authorize_host(&runtime);
    let gen = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    let request=Request::new("configure",json!({"idempotency_key":hex(&[9;32]),"expected_generation":gen,"outbound_first_requested":true,"advertise_reachability":false}),true).unwrap();
    let result = services
        .obp_command([1; 32], &session, &control, Some(&token), request)
        .await
        .unwrap();
    assert_eq!(result["result"]["requested"], true);
    assert_eq!(result["result"]["active"], false);
    assert_eq!(result["result"]["limitations"], json!(["restart_required"]));
    for op in ["source_list", "reservation_list"] {
        let page = services
            .obp_query(
                [1; 32],
                &session,
                Request::new(op, json!({"limit":1}), false).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page["items"], json!([]));
        assert_eq!(page["claims_global_completion"], false);
    }
    assert!(services
        .obp_query(
            [1; 32],
            &session,
            Request::new("intent_status", json!({"intent_id":hex(&[7;32])}), false).unwrap()
        )
        .await
        .is_err());
    runtime.shutdown().await;
    drop(runtime);
    let mut runtime = start(dir.path()).await;
    let status = runtime.services().obp_status().await.unwrap();
    assert_eq!(status["requested"], true);
    assert_eq!(status["active"], false);
    runtime.shutdown().await;
}

#[test]
fn restart_conservatively_preserves_incomplete_and_conflicting_operations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commands.redb");
    let store = store::Store::open(&path).unwrap();
    let payload = command("network_kill", 1, 1).payload;
    assert!(store
        .admit([1; 32], "network_kill", &payload)
        .unwrap()
        .is_none());
    drop(store);
    let store = store::Store::open(&path).unwrap();
    let old = store
        .admit([1; 32], "network_kill", &payload)
        .unwrap()
        .unwrap();
    assert_eq!(old["state"], "reconcile_required");
    assert_eq!(old["reconcile_before_retry"], true);
    assert_eq!(
        store
            .admit([2; 32], "network_kill", &payload)
            .unwrap_err()
            .reason,
        "idempotency_conflict"
    );
    assert_eq!(
        store
            .admit([1; 32], "network_reenable", &payload)
            .unwrap_err()
            .reason,
        "idempotency_conflict"
    );
}

#[tokio::test]
async fn expiry_revocation_and_principal_never_create_command_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let (control, token) = authorize_host(&runtime);
    let gen = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    assert_eq!(
        services
            .obp_command(
                [2; 32],
                &session,
                &control,
                Some(&token),
                command("network_kill", 1, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "forbidden"
    );
    runtime.obp_host().revoke_management(&token).unwrap();
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_kill", 1, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "forbidden"
    );
    let token = runtime
        .obp_host()
        .management(
            [1; 32],
            vec!["network_kill".into()],
            Duration::from_millis(1),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_kill", 1, gen)
            )
            .await
            .unwrap_err()
            .reason,
        "forbidden"
    );
    assert!(services.obp_reconcile([1; 32], &session, [1; 32]).is_err());
    runtime.shutdown().await;
}

#[tokio::test]
async fn crash_child() {
    let Ok(path) = std::env::var("OBP_API_TEST_DIRECTORY") else {
        return;
    };
    let runtime = start(Path::new(&path)).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let (control, token) = authorize_host(&runtime);
    let gen = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    let _ = services
        .obp_command(
            [1; 32],
            &session,
            &control,
            Some(&token),
            command("network_kill", 1, gen),
        )
        .await;
    panic!("crash point was not reached");
}

#[tokio::test]
async fn real_process_exit_at_four_command_boundaries_never_blind_replays() {
    for stage in [
        "before_admission",
        "after_admission",
        "after_effect",
        "after_result_commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "vnext_product_runtime::obp::tests::crash_child",
                "--nocapture",
            ])
            .env("OBP_API_TEST_DIRECTORY", dir.path())
            .env("OBP_API_TEST_CRASH", stage)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{stage}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut runtime = start(dir.path()).await;
        let services = runtime.services();
        let session = services.obp_session().unwrap();
        let (control, token) = authorize_host(&runtime);
        let status = services.obp_status().await.unwrap();
        let outcome = services.obp_reconcile([1; 32], &session, [1; 32]);
        match stage {
            "before_admission" => {
                assert!(outcome.is_err());
                assert_eq!(status["kill_switch"], false);
            }
            "after_admission" | "after_effect" => {
                assert_eq!(outcome.unwrap()["state"], "reconcile_required");
                assert_eq!(status["kill_switch"], stage == "after_effect");
                let old = services
                    .lease()
                    .unwrap()
                    .core
                    .obp
                    .store
                    .read()
                    .unwrap()
                    .records[&hex(&[1; 32])]
                    .payload
                    .clone();
                let replay = services
                    .obp_command(
                        [1; 32],
                        &session,
                        &control,
                        Some(&token),
                        Request::new("network_kill", old, true).unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(replay["state"], "reconcile_required");
                assert_eq!(
                    services.obp_status().await.unwrap()["generation"],
                    status["generation"]
                );
            }
            "after_result_commit" => {
                assert_eq!(outcome.unwrap()["state"], "completed");
                assert_eq!(status["kill_switch"], true);
            }
            _ => unreachable!(),
        }
        runtime.shutdown().await;
    }
}

struct EmptyPorts;

#[tokio::test]
async fn bounded_admission_and_expired_snapshots_fail_before_effect() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let (control, token) = authorize_host(&runtime);
    let lease = services.lease().unwrap();
    let generation = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    let busy = lease.core.obp.serial.lock().await;
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_kill", 90, generation)
            )
            .await
            .unwrap_err()
            .reason,
        "admission_limit"
    );
    drop(busy);
    assert!(services.obp_reconcile([1; 32], &session, [90; 32]).is_err());
    services
        .obp_query(
            [1; 32],
            &session,
            Request::new("source_list", json!({"limit":1}), false).unwrap(),
        )
        .await
        .unwrap();
    let key = {
        let mut snapshots = lease.core.obp.snapshots.lock().unwrap();
        let (key, snapshot) = snapshots.iter_mut().next().unwrap();
        snapshot.expires = Instant::now() - Duration::from_secs(1);
        *key
    };
    let mut bytes = key.to_vec();
    bytes.extend_from_slice(&0u64.to_be_bytes());
    let tag = blake3::keyed_hash(&lease.core.obp.cursor_key, &bytes);
    bytes.extend_from_slice(tag.as_bytes());
    let token_page = format!("obc1.{}", URL_SAFE_NO_PAD.encode(bytes));
    assert_eq!(
        services
            .obp_query(
                [1; 32],
                &session,
                Request::new(
                    "source_list",
                    json!({"limit":1,"continuation":token_page}),
                    false
                )
                .unwrap()
            )
            .await
            .unwrap_err()
            .reason,
        "snapshot_expired"
    );
    lease.core.obp.store.mutate(|state|{for n in 0..4096u64 {let mut key=[0;32];key[..8].copy_from_slice(&n.to_be_bytes());let key=hex(&key);state.records.insert(key.clone(),store::Record{principal:[1;32],operation:"network_kill".into(),payload:json!({"idempotency_key":key,"expected_generation":generation}),outcome:json!({"idempotency_key":key,"operation":"network_kill","state":"admitted","reconcile_before_retry":true})});}Ok(())}).unwrap();
    assert_eq!(
        services
            .obp_command(
                [1; 32],
                &session,
                &control,
                Some(&token),
                command("network_kill", 90, generation)
            )
            .await
            .unwrap_err()
            .reason,
        "admission_limit"
    );
    assert_eq!(
        services.obp_status().await.unwrap()["generation"],
        generation
    );
    drop(lease);
    runtime.shutdown().await;
}

#[tokio::test]
async fn retry_preserves_existing_pending_bytes_and_counters() {
    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let (control, _) = authorize_host(&runtime);
    let lease = services.lease().unwrap();
    let network = lease.core.network().unwrap();
    let intent = crate::vnext_outbox::OutboundTransferIntent::new(
        NodeId::from_bytes([77; 32]),
        "127.0.0.1:9".parse().unwrap(),
        SelectorCid::from_bytes([2; 32]),
        NamespaceCommitment::from_bytes([3; 32]),
        DisclosureClass::Public,
        onebrain_protocol::ReconcileManifestKind::Object,
        b"local pending intent".to_vec(),
    )
    .unwrap();
    network.enqueue_outbound(&intent).unwrap();
    let before = network.outbound_intent(&intent.id).unwrap().unwrap();
    let query = Request::new("intent_status", json!({"intent_id":hex(&intent.id)}), false).unwrap();
    assert_eq!(
        services.obp_query([1; 32], &session, query).await.unwrap()["state"],
        "pending"
    );
    let generation = services.obp_status().await.unwrap()["generation"].clone();
    let request=Request::new("intent_retry",json!({"idempotency_key":hex(&[64;32]),"expected_generation":generation,"intent_id":hex(&intent.id)}),true).unwrap();
    let result = services
        .obp_command([1; 32], &session, &control, None, request.clone())
        .await
        .unwrap();
    assert_eq!(result["state"], "completed");
    assert_eq!(
        services
            .obp_command([1; 32], &session, &control, None, request)
            .await
            .unwrap(),
        result
    );
    let after = network.outbound_intent(&intent.id).unwrap().unwrap();
    assert_eq!(before.canonical_bytes, after.canonical_bytes);
    assert_eq!(before.transport_attempts, after.transport_attempts);
    assert_eq!(before.validation_retries, after.validation_retries);
    drop(network);
    drop(lease);
    runtime.shutdown().await;
}
impl crate::vnext_reachability_manager::CandidateGatherer for EmptyPorts {
    fn gather(
        &self,
        epoch: crate::vnext_reachability_manager::NetworkEpoch,
    ) -> ku_net::vnext_relay_discovery::ReachabilityFuture<
        '_,
        std::result::Result<
            crate::vnext_reachability_manager::GatheredCandidates,
            crate::vnext_reachability_manager::ReachabilityError,
        >,
    > {
        Box::pin(async move {
            use crate::vnext_reachability_manager::*;
            Ok(GatheredCandidates {
                private: PrivateCandidateSet::local(vec![], epoch)?,
                public: vec![],
                direct: vec![],
                relay: vec![],
                epoch,
                observed_at: 1,
            })
        })
    }
}
impl crate::vnext_reachability_manager::AdvertisementPublisher for EmptyPorts {
    fn publish<'a>(
        &'a self,
        _: &'a onebrain_protocol::ReachabilityAdvertisementV1,
    ) -> ku_net::vnext_relay_discovery::ReachabilityFuture<
        'a,
        std::result::Result<(), crate::vnext_reachability_manager::ReachabilityError>,
    > {
        Box::pin(async { panic!("no advertisement consent") })
    }
}
impl crate::vnext_outbound_product::DiscoveryRecordSource for EmptyPorts {
    fn fetch(
        &self,
        _: ku_net::vnext_relay_discovery::SourceBudget,
    ) -> ku_net::vnext_relay_discovery::ReachabilityFuture<
        '_,
        std::result::Result<Vec<Vec<u8>>, ku_net::vnext_relay_discovery::RelayDiscoveryLimitation>,
    > {
        Box::pin(async { Ok(vec![]) })
    }
}

#[tokio::test]
async fn admitted_sources_refresh_toggle_and_paging_use_existing_owner() {
    let dir = tempfile::tempdir().unwrap();
    let (grant, receiver) = watch::channel(true);
    let ports = OutboundFirstDependencies::new(
        Arc::new(EmptyPorts),
        Arc::new(EmptyPorts),
        Arc::new(ku_net::vnext_reachability_crypto::SystemPublicEndpointResolver::new(4).unwrap()),
        receiver,
    );
    let mut runtime = VNextProductRuntime::start(
        dir.path(),
        "127.0.0.1:0".parse().unwrap(),
        &all_lanes_config(),
        dependencies(45).with_outbound_first(ports),
        None,
    )
    .await
    .unwrap();
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let host = runtime.obp_host();
    let control = host.control([1; 32]).unwrap();
    let token = host
        .management(
            [1; 32],
            vec!["source_admit".into(), "source_set_enabled".into()],
            Duration::from_secs(300),
        )
        .unwrap();
    let gen = services.obp_status().await.unwrap()["generation"]
        .as_u64()
        .unwrap();
    for marker in [8u8, 9] {
        let input = host
            .input(
                [1; 32],
                DiscoveryInput::Rendezvous {
                    relay: NodeId::from_bytes([marker; 32]),
                    source: Arc::new(EmptyPorts),
                },
            )
            .unwrap();
        let r=Request::new("source_admit",json!({"idempotency_key":hex(&[marker;32]),"expected_generation":gen,"input_ref":input,"kind":"rendezvous"}),true).unwrap();
        let result = services
            .obp_command([1; 32], &session, &control, Some(&token), r.clone())
            .await
            .unwrap();
        assert_eq!(result["state"], "completed");
        assert_eq!(result["result"]["source_id"], hex(&[marker; 32]));
        assert_eq!(
            services
                .obp_command([1; 32], &session, &control, Some(&token), r)
                .await
                .unwrap(),
            result
        );
    }
    let first = services
        .obp_query(
            [1; 32],
            &session,
            Request::new("source_list", json!({"limit":1}), false).unwrap(),
        )
        .await
        .unwrap();
    let continuation = first["continuation"].as_str().unwrap();
    let query = Request::new(
        "source_list",
        json!({"limit":1,"continuation":continuation}),
        false,
    )
    .unwrap();
    assert_eq!(
        services
            .obp_query([2; 32], &session, query.clone())
            .await
            .unwrap_err()
            .reason,
        "generation_conflict"
    );
    let second = services.obp_query([1; 32], &session, query).await.unwrap();
    assert_ne!(
        first["items"][0]["source_id"],
        second["items"][0]["source_id"]
    );
    assert_eq!(first["snapshot_frontier"], second["snapshot_frontier"]);
    let mut bytes = URL_SAFE_NO_PAD
        .decode(continuation.strip_prefix("obc1.").unwrap())
        .unwrap();
    bytes[39] ^= 1;
    let tampered = format!("obc1.{}", URL_SAFE_NO_PAD.encode(bytes));
    assert!(services
        .obp_query(
            [1; 32],
            &session,
            Request::new(
                "source_list",
                json!({"limit":1,"continuation":tampered}),
                false
            )
            .unwrap()
        )
        .await
        .is_err());
    let r=Request::new("source_set_enabled",json!({"idempotency_key":hex(&[10;32]),"expected_generation":gen,"source_id":hex(&[8;32]),"enabled":false}),true).unwrap();
    let result = services
        .obp_command([1; 32], &session, &control, Some(&token), r)
        .await
        .unwrap();
    assert_eq!(result["result"]["state"], "disabled");
    let result = services
        .obp_command(
            [1; 32],
            &session,
            &control,
            None,
            command("refresh", 11, gen),
        )
        .await
        .unwrap();
    assert_eq!(result["state"], "completed");
    let route=Request::new("route_request",json!({"idempotency_key":hex(&[12;32]),"expected_generation":gen,"expected_peer":hex(&[44;32])}),true).unwrap();
    let result = services
        .obp_command([1; 32], &session, &control, None, route)
        .await
        .unwrap();
    assert_eq!(result["result"]["state"], "path_limited");
    assert!(result["result"].get("authenticated_peer").is_none());
    let query = Request::new(
        "route_status",
        json!({"route_id":result["result"]["route_id"]}),
        false,
    )
    .unwrap();
    assert_eq!(
        services.obp_query([1; 32], &session, query).await.unwrap()["state"],
        "path_limited"
    );
    runtime.shutdown().await;
    drop(runtime);
    drop(grant);
    let mut runtime = start(dir.path()).await;
    let services = runtime.services();
    let session = services.obp_session().unwrap();
    let page = services
        .obp_query(
            [1; 32],
            &session,
            Request::new("source_list", json!({"limit":256}), false).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["items"][0]["state"], "disabled");
    assert_eq!(page["items"][1]["state"], "unavailable");
    runtime.shutdown().await;
}
