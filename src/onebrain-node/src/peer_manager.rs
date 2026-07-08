//! Peer manager — tracks connected peers.
//!
//! Simple in-memory list of known peers. In production, this would
//! integrate with ku-net's SWIM membership and Kademlia DHT.

use std::net::SocketAddr;
use crate::network::PeerInfo;

/// Manages the list of connected peers.
#[derive(Debug, Default)]
pub struct PeerManager {
    peers: Vec<PeerInfo>,
}

impl PeerManager {
    /// Create an empty peer manager.
    pub fn new() -> Self {
        Self { peers: Vec::new() }
    }

    /// Add a peer (deduplicates by address).
    pub fn add_peer(&mut self, info: PeerInfo) {
        // Remove existing entry for this address (update)
        self.peers.retain(|p| p.addr != info.addr);
        self.peers.push(info);
    }

    /// Remove a peer by address.
    pub fn remove_peer(&mut self, addr: &SocketAddr) {
        self.peers.retain(|p| &p.addr != addr);
    }

    /// Number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get the list of peers.
    pub fn peer_list(&self) -> &[PeerInfo] {
        &self.peers
    }

    /// Get all known peer addresses.
    pub fn known_addrs(&self) -> Vec<SocketAddr> {
        self.peers.iter().map(|p| p.addr).collect()
    }
}
