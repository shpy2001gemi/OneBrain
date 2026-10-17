//! INT-KU-OBP-001: isolated, opt-in loopback topology using the existing runtime.
//! JSONL on local stdin is trusted operator control; D-045 sharing is opt-in.
use ku_core::foundation::NodeId;
use onebrain_base_contract::ku::KuViewV1;
use onebrain_base_contract::ku_payload::{decode_hex, hex};
use onebrain_node::ku_public_share::{
    inspect_accepted_public_ku, inspect_outbound_intent, PublicKuShareCoordinator,
};
use onebrain_node::vnext_config::VNextNetworkPolicy;
use onebrain_node::vnext_network_runtime::VNextNetworkRuntime;
use serde::Deserialize;
use serde_json::json;
use std::{
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
    let mut share = PublicKuShareCoordinator::new();
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
                let result = async {
                    share.check_capacity(current_time)?;
                    let host = config
                        .local_ku_host
                        .as_ref()
                        .ok_or("ku_host_not_configured")?;
                    let view = saved_view(host, &object_cid).await?;
                    share.prepare_view(
                        &object_cid,
                        &view,
                        &expected_node_id,
                        address,
                        &selector,
                        &namespace,
                        current_time,
                    )
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
                    let cid = share
                        .prepared_object_cid(&prepared_id, confirm_public)?
                        .to_owned();
                    let host = config
                        .local_ku_host
                        .as_ref()
                        .ok_or("ku_host_not_configured")?;
                    let view = saved_view(host, &cid).await?;
                    let current_time = now().map_err(|_| "invalid_clock")?;
                    share.confirm_view(&prepared_id, confirm_public, &view, &runtime, current_time)
                }
                .await;
                match result {
                    Ok(result) => println!("{result}"),
                    Err(code) => println!("{}", json!({"event":"error","code":code})),
                }
            }
            Command::Inspect {
                object_cid,
                selector,
            } => match inspect_accepted_public_ku(&runtime, &object_cid, &selector) {
                Ok(result) => println!("{result}"),
                Err(code) => println!("{}", json!({"event":"error","code":code})),
            },
            Command::Intent { intent_id } => match inspect_outbound_intent(&runtime, &intent_id) {
                Ok(result) => println!("{result}"),
                Err(code) => println!("{}", json!({"event":"error","code":code})),
            },
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
