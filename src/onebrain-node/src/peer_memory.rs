//! Peer memory — remembers peers across restarts.
//!
//! Persists known peer info to a JSON file so the node can attempt
//! to reconnect to previously-seen peers on startup. Keeps at most
//! 100 peers, sorted by most recently seen.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use std::time::SystemTime;

/// A peer remembered from a previous session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberedPeer {
    /// Unique peer identifier (32 hex chars).
    pub peer_id: String,
    /// Display name of the peer.
    pub name: String,
    /// Last known network address.
    pub last_addr: SocketAddr,
    /// When this peer was last seen online.
    pub last_seen: SystemTime,
}

/// In-memory + on-disk store of known peers.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PeerMemory {
    /// List of known peers, most-recently-seen first.
    pub known_peers: Vec<RememberedPeer>,
}

impl PeerMemory {
    /// Load peer memory from a JSON file, or return an empty store.
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    /// Save peer memory to a JSON file (best-effort).
    pub fn save(&self, path: &Path) {
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }

    /// Remember a peer (update if already known, otherwise add).
    pub fn remember(&mut self, peer_id: String, name: String, addr: SocketAddr) {
        // Update existing or add new
        if let Some(existing) = self.known_peers.iter_mut().find(|p| p.peer_id == peer_id) {
            existing.name = name;
            existing.last_addr = addr;
            existing.last_seen = SystemTime::now();
        } else {
            self.known_peers.push(RememberedPeer {
                peer_id,
                name,
                last_addr: addr,
                last_seen: SystemTime::now(),
            });
        }
        // Keep max 100 peers, most recently seen first
        if self.known_peers.len() > 100 {
            self.known_peers
                .sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
            self.known_peers.truncate(100);
        }
    }

    /// Number of remembered peers.
    pub fn peer_count(&self) -> usize {
        self.known_peers.len()
    }
}
