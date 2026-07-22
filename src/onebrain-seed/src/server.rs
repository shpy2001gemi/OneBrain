use onebrain_protocol::*;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::registry::PeerRegistry;
use crate::relay::RelayService;

struct SeedState {
    registry: PeerRegistry,
    relay: RelayService,
    name: String,
}

pub async fn run_seed_server(
    bind_addr: SocketAddr,
    name: &str,
    max_peers: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(bind_addr).await?;
    println!("  ✓ Listening on {}", listener.local_addr()?);
    println!();

    let state = Arc::new(Mutex::new(SeedState {
        registry: PeerRegistry::new(max_peers),
        relay: RelayService::new(),
        name: name.to_string(),
    }));

    // Spawn cleanup task (remove stale peers every 60s)
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut s = cleanup_state.lock().await;
            let removed = s.registry.cleanup_stale(PEER_TIMEOUT_SECS);
            for peer_id in &removed {
                s.relay.remove_connection(peer_id);
            }
            if !removed.is_empty() {
                println!(
                    "  [cleanup] Removed {} stale peer(s). Active: {}",
                    removed.len(),
                    s.registry.peer_count()
                );
            }
        }
    });

    // Spawn stats printer every 30s
    let stats_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let s = stats_state.lock().await;
            println!(
                "  [stats] Peers: {} registered, {} connected",
                s.registry.peer_count(),
                s.relay.connection_count()
            );
        }
    });

    println!("  Waiting for peer connections...\n");

    loop {
        let (stream, addr) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(handle_client(stream, addr, state));
    }
}

async fn handle_client(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    state: Arc<Mutex<SeedState>>,
) {
    let stream = Arc::new(Mutex::new(stream));
    let mut peer_id_opt: Option<String> = None;

    loop {
        // Read message
        let msg: SeedMessage = {
            let mut guard = stream.lock().await;
            match recv_message(&mut *guard).await {
                Ok(m) => m,
                Err(_) => break, // connection closed
            }
        };

        match msg {
            SeedMessage::Register {
                peer_id,
                name,
                listen_port: _,
                internal_addr,
                upnp_addr,
                ku_count,
            } => {
                let mut s = state.lock().await;
                let registered = s.registry.register(
                    peer_id.clone(),
                    name.clone(),
                    addr,
                    internal_addr,
                    upnp_addr,
                    ku_count,
                );
                if registered {
                    s.relay.register_connection(peer_id.clone(), stream.clone());
                    peer_id_opt = Some(peer_id.clone());
                    let short_id = if peer_id.len() >= 8 {
                        &peer_id[..8]
                    } else {
                        &peer_id
                    };
                    println!("  [+] {} ({}) registered from {}", name, short_id, addr);

                    let response = SeedMessage::Registered {
                        your_external_addr: addr,
                        seed_name: s.name.clone(),
                    };
                    let mut guard = stream.lock().await;
                    let _ = send_message(&mut *guard, &response).await;
                } else {
                    let mut guard = stream.lock().await;
                    let _ = send_message(
                        &mut *guard,
                        &SeedMessage::SeedError {
                            message: "Registry full".to_string(),
                        },
                    )
                    .await;
                }
            }

            SeedMessage::Heartbeat { peer_id, ku_count } => {
                let mut s = state.lock().await;
                s.registry.heartbeat(&peer_id, ku_count);
                let mut guard = stream.lock().await;
                let _ = send_message(&mut *guard, &SeedMessage::HeartbeatAck).await;
            }

            SeedMessage::GetPeers => {
                let s = state.lock().await;
                let peers = s.registry.get_peer_list();
                let mut guard = stream.lock().await;
                let _ = send_message(&mut *guard, &SeedMessage::PeerList { peers }).await;
            }

            SeedMessage::RelayToPeer {
                to_peer_id,
                payload,
            } => {
                let s = state.lock().await;
                if let Some(pid) = &peer_id_opt {
                    let from_name = s
                        .registry
                        .get_peer(pid)
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    let _ = s
                        .relay
                        .relay_to(&to_peer_id, pid, &from_name, payload)
                        .await;
                }
            }

            SeedMessage::Disconnect { peer_id } => {
                let mut s = state.lock().await;
                let short_id = if peer_id.len() >= 8 {
                    &peer_id[..8]
                } else {
                    &peer_id
                };
                println!("  [-] {} disconnected", short_id);
                s.registry.remove(&peer_id);
                s.relay.remove_connection(&peer_id);
                break;
            }

            _ => {} // ignore client-bound messages
        }
    }

    // Cleanup on disconnect
    if let Some(pid) = peer_id_opt {
        let mut s = state.lock().await;
        s.registry.remove(&pid);
        s.relay.remove_connection(&pid);
        let short_id = if pid.len() >= 8 { &pid[..8] } else { &pid };
        println!("  [-] {} connection lost", short_id);
    }
}
