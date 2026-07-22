use onebrain_protocol::{send_message, SeedMessage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Manages active TCP connections for relaying messages.
pub struct RelayService {
    /// peer_id → active TcpStream (maintained via heartbeat)
    connections: HashMap<String, Arc<Mutex<TcpStream>>>,
}

impl RelayService {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Register a connection for a peer
    pub fn register_connection(&mut self, peer_id: String, stream: Arc<Mutex<TcpStream>>) {
        self.connections.insert(peer_id, stream);
    }

    /// Remove a connection
    pub fn remove_connection(&mut self, peer_id: &str) {
        self.connections.remove(peer_id);
    }

    /// Relay a message to a specific peer
    pub async fn relay_to(
        &self,
        to_peer_id: &str,
        from_peer_id: &str,
        from_name: &str,
        payload: Vec<u8>,
    ) -> Result<(), String> {
        let stream = self
            .connections
            .get(to_peer_id)
            .ok_or_else(|| format!("Peer {} not connected", to_peer_id))?;

        let msg = SeedMessage::RelayedMessage {
            from_peer_id: from_peer_id.to_string(),
            from_name: from_name.to_string(),
            payload,
        };

        let mut guard = stream.lock().await;
        send_message(&mut *guard, &msg)
            .await
            .map_err(|e| format!("Relay failed: {}", e))
    }

    pub fn has_connection(&self, peer_id: &str) -> bool {
        self.connections.contains_key(peer_id)
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
}
