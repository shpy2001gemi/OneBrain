use onebrain_protocol::PeerSummary;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

pub struct PeerRecord {
    pub peer_id: String,
    pub name: String,
    pub external_addr: SocketAddr,
    pub internal_addr: Option<SocketAddr>,
    pub upnp_addr: Option<SocketAddr>,
    pub ku_count: u64,
    pub last_seen: Instant,
}

pub struct PeerRegistry {
    peers: HashMap<String, PeerRecord>, // peer_id → record
    max_peers: usize,
}

impl PeerRegistry {
    pub fn new(max_peers: usize) -> Self {
        Self {
            peers: HashMap::new(),
            max_peers,
        }
    }

    pub fn register(
        &mut self,
        peer_id: String,
        name: String,
        external_addr: SocketAddr,
        internal_addr: Option<SocketAddr>,
        upnp_addr: Option<SocketAddr>,
        ku_count: u64,
    ) -> bool {
        if self.peers.len() >= self.max_peers && !self.peers.contains_key(&peer_id) {
            return false; // full
        }
        self.peers.insert(
            peer_id.clone(),
            PeerRecord {
                peer_id,
                name,
                external_addr,
                internal_addr,
                upnp_addr,
                ku_count,
                last_seen: Instant::now(),
            },
        );
        true
    }

    pub fn heartbeat(&mut self, peer_id: &str, ku_count: u64) -> bool {
        if let Some(record) = self.peers.get_mut(peer_id) {
            record.last_seen = Instant::now();
            record.ku_count = ku_count;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, peer_id: &str) {
        self.peers.remove(peer_id);
    }

    /// Remove peers that haven't sent heartbeat in timeout_secs
    pub fn cleanup_stale(&mut self, timeout_secs: u64) -> Vec<String> {
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let now = Instant::now();
        let stale: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, r)| now.duration_since(r.last_seen) > timeout)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &stale {
            self.peers.remove(id);
        }
        stale
    }

    pub fn get_peer_list(&self) -> Vec<PeerSummary> {
        self.peers
            .values()
            .map(|r| PeerSummary {
                peer_id: r.peer_id.clone(),
                name: r.name.clone(),
                external_addr: r.external_addr,
                upnp_addr: r.upnp_addr,
                ku_count: r.ku_count,
            })
            .collect()
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn get_peer(&self, peer_id: &str) -> Option<&PeerRecord> {
        self.peers.get(peer_id)
    }
}
