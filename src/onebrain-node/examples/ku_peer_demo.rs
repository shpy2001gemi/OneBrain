//! INT-KU-OBP-001: isolated, opt-in loopback topology using the existing runtime.
//! JSONL on local stdin is trusted operator control; D-045 sharing is opt-in.
use base64::{engine::general_purpose::STANDARD, Engine};
use ku_core::foundation::NodeId;
use ku_core::foundation::{NamespaceCommitment, ObjectCid, SelectorCid};
use onebrain_base_contract::ku::KuViewV1;
use onebrain_base_contract::ku_payload::{decode_hex, hex};
use onebrain_node::ku_public_share::{
    inspect_public_ku, prepare_public_ku, PreparedPublicKu, PUBLIC_CONSEQUENCE,
};
use onebrain_node::vnext_config::VNextNetworkPolicy;
use onebrain_node::vnext_network_runtime::VNextNetworkRuntime;
use onebrain_protocol::ReconcileManifestKind;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::BufRead,
    io::Read,
    net::SocketAddr,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    data_dir: PathBuf,
    bind_addr: SocketAddr,
    network_opt_in: bool,
    #[serde(default)]
    local_ku_host: Option<LocalKuHost>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalKuHost {
    port: u16,
    api_token_file: PathBuf,
}

fn now() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

async fn saved_view(host: &LocalKuHost, cid: &str) -> Result<KuViewV1, &'static str> {
    if host.port == 0 || decode_hex::<32>(cid).is_err() {
        return Err("invalid_local_ku_host");
    }
    let mut token = Vec::new();
    std::fs::File::open(&host.api_token_file)
        .map_err(|_| "ku_authorization_unavailable")?
        .take(1025)
        .read_to_end(&mut token)
        .map_err(|_| "ku_authorization_unavailable")?;
    if token.len() > 1024 {
        return Err("ku_authorization_unavailable");
    }
    let token = String::from_utf8(token).map_err(|_| "ku_authorization_unavailable")?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "ku_host_unavailable")?;
    let base = format!("http://127.0.0.1:{}/api/vnext/ku", host.port);
    async fn read(mut response: reqwest::Response) -> Result<serde_json::Value, &'static str> {
        if !response.status().is_success() {
            return Err("saved_ku_not_accessible");
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "ku_host_unavailable")? {
            if bytes.len() + chunk.len() > 1048576 {
                return Err("ku_response_limit");
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "invalid_ku_response")?;
        if value["ok"] != true {
            return Err("saved_ku_not_accessible");
        }
        Ok(value)
    }
    let status = read(
        client
            .get(format!("{base}/status"))
            .bearer_auth(token.trim())
            .send()
            .await
            .map_err(|_| "ku_host_unavailable")?,
    )
    .await?;
    let request = json!({"session":status["data"]["session"],"budget":{"max_items":256,"max_bytes":1048576,"max_work_units":1000000},"request":{"operation":"get","payload":{"object_cid":cid}}});
    let response = read(
        client
            .post(format!("{base}/operations"))
            .bearer_auth(token.trim())
            .header("Content-Type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .map_err(|_| "ku_host_unavailable")?,
    )
    .await?;
    serde_json::from_value(response["data"]["payload"].clone()).map_err(|_| "invalid_ku_response")
}

#[derive(Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Status,
    Connect {
        expected_node_id: String,
        address: SocketAddr,
    },
    Shutdown,
    PrepareShare {
        object_cid: String,
        expected_node_id: String,
        address: SocketAddr,
        selector: String,
        namespace: String,
    },
    ConfirmShare {
        prepared_id: String,
        confirm_public: bool,
    },
    Inspect {
        object_cid: String,
        selector: String,
    },
    Intent {
        intent_id: String,
    },
}

fn status(runtime: &VNextNetworkRuntime, sharing: bool) -> Result<(), Box<dyn std::error::Error>> {
    let status = runtime.status();
    println!(
        "{}",
        json!({
            "event": "status",
            "node_id": hex(&status.principal),
            "listen_addr": status.listen_addr,
            "authenticated_sessions": status.authenticated_sessions,
            "accepted_records": status.accepted_records,
            "authenticated_routes": runtime.authenticated_route_count()?,
            "claims_network_completion": false,
            "ku_share_host_configured": sharing
        })
    );
    Ok(())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: ku_peer_demo <isolated-demo-config.json>")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err("config exceeds limit".into());
    }
    let config: Config = serde_json::from_slice(&bytes)?;
    if !config.network_opt_in || !config.bind_addr.ip().is_loopback() {
        return Err("explicit network_opt_in and loopback binding required".into());
    }
    let mut runtime = VNextNetworkRuntime::start(
        &config.data_dir,
        config.bind_addr,
        VNextNetworkPolicy::default(),
    )
    .await?;
    status(&runtime, config.local_ku_host.is_some())?;
    let mut prepared: BTreeMap<String, (String, PreparedPublicKu)> = BTreeMap::new();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut result = Ok(());
    loop {
        let mut line = Vec::new();
        let count = input.by_ref().take(4097).read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        if line.len() > 4096 {
            result = Err("operator command exceeds limit".into());
            break;
        }
        let command = match serde_json::from_slice::<Command>(&line) {
            Ok(command) => command,
            Err(_) => {
                println!("{}", json!({"event":"error","code":"invalid_command"}));
                continue;
            }
        };
        match command {
            Command::Status => status(&runtime, config.local_ku_host.is_some())?,
            Command::Shutdown => break,
            Command::PrepareShare {
                object_cid,
                expected_node_id,
                address,
                selector,
                namespace,
            } => {
                let current_time = now()?;
                prepared.retain(|_, (_, p)| p.expires_at() > current_time);
                let result = async {
                    if prepared.len() >= 16 {
                        return Err("prepared_limit");
                    }
                    let host = config
                        .local_ku_host
                        .as_ref()
                        .ok_or("ku_host_not_configured")?;
                    let view = saved_view(host, &object_cid).await?;
                    let peer = NodeId::from_bytes(
                        decode_hex::<32>(&expected_node_id).map_err(|_| "invalid_peer")?,
                    );
                    let selector_id = SelectorCid::from_bytes(
                        decode_hex::<32>(&selector).map_err(|_| "invalid_selector")?,
                    );
                    let namespace_id = NamespaceCommitment::from_bytes(
                        decode_hex::<32>(&namespace).map_err(|_| "invalid_namespace")?,
                    );
                    let proposal = prepare_public_ku(
                        &view,
                        peer,
                        address,
                        selector_id,
                        namespace_id,
                        current_time,
                    )?;
                    let mut nonce = [0; 32];
                    getrandom::fill(&mut nonce).map_err(|_| "consent_randomness_unavailable")?;
                    let prepared_id = hex(&nonce);
                    let preview = json!({
                        "event":"share_prepared", "prepared_id":prepared_id,
                        "intent_id":hex(&proposal.intent_id()),
                        "public_object_cid":hex(&proposal.public_cid()),
                        "semantic_content_cid":hex(&proposal.semantic_cid()),
                        "canonical_base64":STANDARD.encode(proposal.preview()),
                        "semantic":inspect_public_ku(proposal.preview())?,
                        "expected_node_id":expected_node_id, "address":address,
                        "selector":selector, "namespace":namespace,
                        "expires_at":proposal.expires_at(), "consequence":PUBLIC_CONSEQUENCE,
                        "enqueued":false, "payload_sent":false,
                        "support":"one predicate and one text literal only"
                    });
                    prepared.insert(prepared_id, (object_cid, proposal));
                    Ok(preview)
                }
                .await;
                match result {
                    Ok(preview) => println!("{preview}"),
                    Err(code) => println!("{}", json!({"event":"error","code":code})),
                }
            }
            Command::ConfirmShare {
                prepared_id,
                confirm_public,
            } => {
                let result = async {
                    if !confirm_public { return Err("explicit_public_confirmation_required"); }
                    let (cid, _) = prepared.get(&prepared_id).ok_or("prepared_share_missing")?;
                    let view = saved_view(config.local_ku_host.as_ref().ok_or("ku_host_not_configured")?, cid).await?;
                    let (_, proposal) = prepared.remove(&prepared_id).ok_or("prepared_share_missing")?;
                    let intent = proposal.confirm(&view, now().map_err(|_| "invalid_clock")?)?;
                    let outcome = runtime.enqueue_outbound(&intent).map_err(|_| "outbox_unavailable_reprepare_same_intent")?;
                    Ok(json!({"event":"share_confirmed","intent_id":hex(&intent.id),"public_object_cid":hex(&intent.cid),"outbox":format!("{outcome:?}"),"delivery":"pending; inspect intent for acknowledgement","authorizes_reward":false}))
                }.await;
                match result {
                    Ok(result) => println!("{result}"),
                    Err(code) => println!("{}", json!({"event":"error","code":code})),
                }
            }
            Command::Inspect {
                object_cid,
                selector,
            } => {
                let result = (|| -> Result<_, &'static str> {
                    let cid = ObjectCid::from_bytes(
                        decode_hex::<32>(&object_cid).map_err(|_| "invalid_public_identity")?,
                    );
                    let selector = SelectorCid::from_bytes(
                        decode_hex::<32>(&selector).map_err(|_| "invalid_selector")?,
                    );
                    let bytes = runtime
                        .public_object(cid)
                        .map_err(|_| "public_store_unavailable")?
                        .ok_or("public_object_not_found")?;
                    let semantic = inspect_public_ku(&bytes)?;
                    let peers: Vec<_> = runtime
                        .record_source_peers(
                            ReconcileManifestKind::Object,
                            cid.into_bytes(),
                            selector,
                        )
                        .map_err(|_| "provenance_unavailable")?
                        .iter()
                        .map(|id| hex(id.as_bytes()))
                        .collect();
                    Ok(
                        json!({"event":"public_object","object_cid":object_cid,"canonical_base64":STANDARD.encode(bytes),"semantic":semantic,"authenticated_source_peers":peers,"provenance":"transport/selector only; original source withheld and unverifiable","fidelity":"unassessed","authorizes_reward":false}),
                    )
                })();
                match result {
                    Ok(result) => println!("{result}"),
                    Err(code) => println!("{}", json!({"event":"error","code":code})),
                }
            }
            Command::Intent { intent_id } => {
                let result = (|| -> Result<_, &'static str> {
                    let id = decode_hex::<32>(&intent_id).map_err(|_| "invalid_intent_identity")?;
                    let intent = runtime
                        .outbound_intent(&id)
                        .map_err(|_| "outbox_unavailable")?
                        .ok_or("intent_not_found")?;
                    Ok(
                        json!({"event":"intent","intent_id":intent_id,"public_object_cid":hex(&intent.cid),"state":format!("{:?}",intent.state),"terminal_sequence":intent.terminal_sequence,"claims_network_completion":false}),
                    )
                })();
                match result {
                    Ok(result) => println!("{result}"),
                    Err(code) => println!("{}", json!({"event":"error","code":code})),
                }
            }
            Command::Connect {
                expected_node_id,
                address,
            } => {
                if !address.ip().is_loopback() || address.port() == 0 {
                    println!("{}", json!({"event":"error","code":"invalid_peer"}));
                    continue;
                }
                let peer = match decode_hex::<32>(&expected_node_id) {
                    Ok(peer) if peer != [0; 32] => NodeId::from_bytes(peer),
                    _ => {
                        println!("{}", json!({"event":"error","code":"invalid_peer"}));
                        continue;
                    }
                };
                match tokio::time::timeout(
                    Duration::from_secs(5),
                    runtime.connect_expected(peer, address),
                )
                .await
                {
                    Ok(Ok(connection)) => {
                        println!(
                            "{}",
                            json!({"event":"connected","expected_node_id":hex(peer.as_bytes()),"authenticated":true,"payload_sent":false})
                        );
                        connection.close();
                    }
                    _ => println!(
                        "{}",
                        json!({"event":"error","code":"peer_authentication_failed","payload_sent":false})
                    ),
                }
            }
        }
    }
    runtime.shutdown().await;
    result
}
