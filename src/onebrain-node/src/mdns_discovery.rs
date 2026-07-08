//! mDNS LAN peer discovery.
//!
//! Discovers other OneBrain nodes on the local network.
//! Best-effort: if mDNS is not available, we rely on seed nodes
//! for cross-network peer discovery.

use std::net::SocketAddr;

/// Result of an mDNS discovery attempt.
pub struct MdnsResult {
    /// Whether this node was registered on the LAN.
    pub registered: bool,
    /// Peers discovered via mDNS on the local network.
    pub discovered_peers: Vec<SocketAddr>,
    /// Human-readable status message.
    pub message: String,
}

/// Try to register on LAN and discover peers via mDNS.
///
/// This is a best-effort stub. The `mdns-sd` crate may not compile
/// on Windows GNU toolchain, so we start with a no-op implementation.
/// Seed nodes handle cross-network discovery.
pub async fn try_mdns_discovery(_name: &str, _port: u16) -> MdnsResult {
    // TODO: Use mdns-sd crate for LAN discovery when available.
    // Stub for now — mDNS crate may not compile on Windows GNU.
    // Seed nodes handle cross-network discovery.

    MdnsResult {
        registered: false,
        discovered_peers: vec![],
        message: "mDNS: Not available (using seed discovery instead)".to_string(),
    }
}
