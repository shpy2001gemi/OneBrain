//! Seed node client — connects to n1.onebrain.live / n2.onebrain.live
//! for peer discovery and relay.
//!
//! The seed client registers with a seed node, receives its external
//! address, queries the peer list, and can relay messages to other
//! peers via the seed when direct connections are not possible.

use onebrain_protocol::*;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Client that connects to a seed node for discovery and relay.
pub struct SeedClient {
    seed_domains: Vec<String>,
    seed_port: u16,
    peer_id: String,
    node_name: String,
    listen_port: u16,
    stream: Option<Arc<Mutex<TcpStream>>>,
    external_addr: Option<SocketAddr>,
    connected_seed: Option<String>,
}

impl SeedClient {
    /// Create a new seed client with the given node identity.
    pub fn new(peer_id: String, node_name: String, listen_port: u16) -> Self {
        Self {
            seed_domains: SEED_DOMAINS.iter().map(|s| s.to_string()).collect(),
            seed_port: SEED_PORT,
            peer_id,
            node_name,
            listen_port,
            stream: None,
            external_addr: None,
            connected_seed: None,
        }
    }

    /// Connect to the first available seed node.
    ///
    /// Tries each seed domain in order, resolving DNS and attempting TCP
    /// connection. On success, registers with the seed and receives our
    /// external address.
    pub async fn connect(&mut self) -> Result<SocketAddr, String> {
        for domain in &self.seed_domains.clone() {
            let addr_str = format!("{}:{}", domain, self.seed_port);
            println!("  Trying seed: {}...", addr_str);

            // Resolve DNS
            let addrs: Vec<SocketAddr> = match tokio::net::lookup_host(&addr_str).await {
                Ok(a) => a.collect(),
                Err(e) => {
                    println!("    ✗ DNS failed: {}", e);
                    continue;
                }
            };

            for addr in addrs {
                match TcpStream::connect(addr).await {
                    Ok(mut stream) => {
                        // Register with seed
                        let register_msg = SeedMessage::Register {
                            peer_id: self.peer_id.clone(),
                            name: self.node_name.clone(),
                            listen_port: self.listen_port,
                            internal_addr: None, // TODO: detect local IP
                            upnp_addr: None,     // TODO: from UPnP result
                            ku_count: 0,
                        };

                        if let Err(e) = send_message(&mut stream, &register_msg).await {
                            println!("    ✗ Register failed: {}", e);
                            continue;
                        }

                        // Wait for Registered response
                        match recv_message::<SeedMessage>(&mut stream).await {
                            Ok(SeedMessage::Registered {
                                your_external_addr,
                                seed_name,
                            }) => {
                                println!("  ✓ Connected to seed: {} ({})", seed_name, domain);
                                println!("  ✓ External address: {}", your_external_addr);
                                self.external_addr = Some(your_external_addr);
                                self.connected_seed = Some(format!("{} ({})", seed_name, domain));
                                self.stream = Some(Arc::new(Mutex::new(stream)));
                                return Ok(your_external_addr);
                            }
                            Ok(SeedMessage::SeedError { message }) => {
                                println!("    ✗ Seed rejected: {}", message);
                                continue;
                            }
                            Ok(_) => {
                                println!("    ✗ Unexpected response");
                                continue;
                            }
                            Err(e) => {
                                println!("    ✗ Read failed: {}", e);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        println!("    ✗ Connect failed: {}", e);
                        continue;
                    }
                }
            }
        }

        Err("Could not connect to any seed node".to_string())
    }

    /// Get list of online peers from seed.
    pub async fn get_peers(&self) -> Result<Vec<PeerSummary>, String> {
        let stream = self.stream.as_ref().ok_or("Not connected to seed")?;
        let mut guard = stream.lock().await;

        send_message(&mut *guard, &SeedMessage::GetPeers)
            .await
            .map_err(|e| format!("Send failed: {}", e))?;

        match recv_message::<SeedMessage>(&mut *guard).await {
            Ok(SeedMessage::PeerList { peers }) => Ok(peers),
            Ok(_) => Err("Unexpected response".to_string()),
            Err(e) => Err(format!("Read failed: {}", e)),
        }
    }

    /// Relay a PeerMessage to another peer via the seed.
    pub async fn relay_to(&self, to_peer_id: &str, msg: &PeerMessage) -> Result<(), String> {
        let stream = self.stream.as_ref().ok_or("Not connected to seed")?;
        let payload = serde_json::to_vec(msg).map_err(|e| format!("Serialize failed: {}", e))?;

        let relay_msg = SeedMessage::RelayToPeer {
            to_peer_id: to_peer_id.to_string(),
            payload,
        };

        let mut guard = stream.lock().await;
        send_message(&mut *guard, &relay_msg)
            .await
            .map_err(|e| format!("Relay failed: {}", e))
    }

    /// Run background loop: heartbeat + receive relayed messages.
    ///
    /// Spawns a heartbeat task that periodically pings the seed to
    /// keep the registration alive. Relayed messages from other peers
    /// will be received interleaved with heartbeat acks.
    pub async fn run_background(
        stream: Arc<Mutex<TcpStream>>,
        peer_id: String,
        _event_tx: tokio::sync::mpsc::Sender<crate::network::NodeEvent>,
    ) {
        let hb_peer_id = peer_id;

        // Heartbeat task
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
            loop {
                interval.tick().await;
                let msg = SeedMessage::Heartbeat {
                    peer_id: hb_peer_id.clone(),
                    ku_count: 0, // TODO: get actual count from shared state
                };
                let mut guard = stream.lock().await;
                if send_message(&mut *guard, &msg).await.is_err() {
                    break; // connection lost
                }
                // Read HeartbeatAck
                match recv_message::<SeedMessage>(&mut *guard).await {
                    Ok(SeedMessage::HeartbeatAck) => {}
                    Ok(SeedMessage::RelayedMessage {
                        from_peer_id: _,
                        from_name: _,
                        payload: _,
                    }) => {
                        // TODO: dispatch relayed message to event_tx
                    }
                    _ => {
                        break; // unexpected or error → disconnect
                    }
                }
            }
        });
    }

    /// Get the external address (if connected to seed).
    pub fn external_addr(&self) -> Option<SocketAddr> {
        self.external_addr
    }

    /// Whether we're connected to a seed node.
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Get the seed connection info string.
    pub fn connected_seed_info(&self) -> Option<&str> {
        self.connected_seed.as_deref()
    }

    /// Get a clone of the underlying TCP stream (for background tasks).
    pub fn stream(&self) -> Option<Arc<Mutex<TcpStream>>> {
        self.stream.clone()
    }
}
