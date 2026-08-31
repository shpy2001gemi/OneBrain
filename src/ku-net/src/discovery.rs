//! # Peer Discovery — 6-Layer Bootstrap — SPEC A §6
//!
//! Multi-layer peer discovery ensuring any single layer is sufficient
//! for joining the network. No central server required.

use crate::identity::NodeId;
use crate::messages::NetworkAddress;
use std::time::Instant;

// ─── Constants (SPEC A §6.2) ──────────────────────────────────────────────

/// mDNS service type for local OBP discovery.
pub const MDNS_SERVICE_TYPE: &str = "_obp._udp.local";
/// mDNS service type for personal mesh discovery (SPEC B §11.4).
pub const MDNS_MESH_SERVICE_TYPE: &str = "_obp-mesh._tcp.local";
/// Well-known HTTP endpoint for peer discovery.
pub const WELL_KNOWN_PATH: &str = "/.well-known/obp-peers";
/// Maximum bootstrap seeds from any single source.
pub const MAX_SEEDS_PER_SOURCE: usize = 20;
/// Minimum peers needed to consider bootstrap successful.
pub const MIN_BOOTSTRAP_PEERS: usize = 3;
/// Maximum time for bootstrap attempt before trying next layer (seconds).
pub const BOOTSTRAP_LAYER_TIMEOUT_S: u64 = 30;

// ─── Bootstrap Layers (SPEC A §6.1) ─────────────────────────────────────

/// The 6 bootstrap layers, tried in cascade order.
///
/// SPEC A §6.1: Any SINGLE layer is sufficient for network entry.
/// Layers are tried top-to-bottom; first success wins.
///
/// ```text
/// Priority  Layer         Method                          Needs Internet?
/// 1         Social        QR code / NFC / BLE exchange    NO
/// 2         Local         mDNS / BLE broadcast            NO
/// 3         HTTP          Well-known URL endpoint         YES
/// 4         DHT           Bootstrap nodes from config     YES
/// 5         DNS           TXT records at known domains    YES
/// 6         Hardcoded     Compiled-in fallback peers      YES
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BootstrapLayer {
    /// QR code scan, NFC tap, or BLE proximity exchange.
    Social = 0,
    /// mDNS `_obp._udp.local` broadcast on LAN.
    Local = 1,
    /// HTTP GET `/.well-known/obp-peers` from seed URLs.
    Http = 2,
    /// DHT bootstrap from known node addresses.
    Dht = 3,
    /// DNS TXT record lookup at known domains.
    Dns = 4,
    /// Hardcoded fallback peers compiled into binary.
    Hardcoded = 5,
}

impl BootstrapLayer {
    /// Whether this layer requires internet connectivity.
    pub fn requires_internet(&self) -> bool {
        !matches!(self, Self::Social | Self::Local)
    }

    /// Display name for logging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Social => "Social (QR/NFC/BLE)",
            Self::Local => "Local (mDNS/BLE)",
            Self::Http => "HTTP (well-known)",
            Self::Dht => "DHT (bootstrap nodes)",
            Self::Dns => "DNS (TXT records)",
            Self::Hardcoded => "Hardcoded (fallback)",
        }
    }
}

// ─── Bootstrap State Machine (SPEC A §6.3) ───────────────────────────────

/// Bootstrap state machine states.
///
/// ```text
/// NotStarted → Discovering → Joining → Connected
///                   ↓
///               Failed (retry next layer)
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapState {
    /// Initial state, not yet started.
    NotStarted,
    /// Actively discovering peers via current layer.
    Discovering {
        layer: BootstrapLayer,
        started_at: Instant,
    },
    /// Found peers, joining the network (SWIM join + DHT insert).
    Joining {
        peers: Vec<DiscoveredPeer>,
        started_at: Instant,
    },
    /// Successfully connected to the network.
    Connected {
        via_layer: BootstrapLayer,
        peer_count: usize,
    },
    /// All layers exhausted, bootstrap failed.
    Failed { attempts: Vec<BootstrapLayer> },
}

/// A peer discovered during bootstrap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    pub node_id: Option<NodeId>, // May not be known yet
    pub address: NetworkAddress,
    pub source: BootstrapLayer,
    pub discovered_at: Instant,
}

// ─── Bootstrap Engine ────────────────────────────────────────────────────

/// 6-layer bootstrap engine (SPEC A §6.5 Algorithm 6).
///
/// Cascades through layers until MIN_BOOTSTRAP_PEERS are found.
/// Each layer has a timeout; if exceeded, try next layer.
pub struct BootstrapEngine {
    pub state: BootstrapState,
    /// Discovered peers across all layers.
    pub discovered: Vec<DiscoveredPeer>,
    /// Which layers have been attempted.
    pub attempted_layers: Vec<BootstrapLayer>,
    /// Social seeds (from QR scan, NFC, etc.) — pre-configured.
    pub social_seeds: Vec<NetworkAddress>,
    /// HTTP seed URLs for well-known endpoint.
    pub http_seeds: Vec<String>,
    /// DHT bootstrap node addresses.
    pub dht_seeds: Vec<NetworkAddress>,
    /// DNS seed domains.
    pub dns_seeds: Vec<String>,
    /// Hardcoded fallback peers.
    pub hardcoded_seeds: Vec<NetworkAddress>,
}

impl BootstrapEngine {
    /// Create a new bootstrap engine with default seeds.
    pub fn new() -> Self {
        BootstrapEngine {
            state: BootstrapState::NotStarted,
            discovered: Vec::new(),
            attempted_layers: Vec::new(),
            social_seeds: Vec::new(),
            http_seeds: Vec::new(),
            dht_seeds: Vec::new(),
            dns_seeds: Vec::new(),
            hardcoded_seeds: Vec::new(),
        }
    }

    /// Start bootstrap process.
    ///
    /// Returns the first layer to try.
    pub fn start(&mut self) -> BootstrapLayer {
        let layer = self.next_layer().unwrap_or(BootstrapLayer::Hardcoded);
        self.state = BootstrapState::Discovering {
            layer,
            started_at: Instant::now(),
        };
        self.attempted_layers.push(layer);
        layer
    }

    /// Report discovered peers from current layer.
    pub fn report_discovered(&mut self, peers: Vec<DiscoveredPeer>) {
        self.discovered.extend(peers);

        if self.discovered.len() >= MIN_BOOTSTRAP_PEERS {
            // Enough peers found — transition to Joining
            let layer = match &self.state {
                BootstrapState::Discovering { layer, .. } => *layer,
                _ => BootstrapLayer::Hardcoded,
            };
            self.state = BootstrapState::Joining {
                peers: self.discovered.clone(),
                started_at: Instant::now(),
            };
            let _ = layer; // used after joining completes
        }
    }

    /// Mark current layer as failed, try next.
    pub fn layer_failed(&mut self) -> Option<BootstrapLayer> {
        if let Some(next) = self.next_layer() {
            self.state = BootstrapState::Discovering {
                layer: next,
                started_at: Instant::now(),
            };
            self.attempted_layers.push(next);
            Some(next)
        } else {
            self.state = BootstrapState::Failed {
                attempts: self.attempted_layers.clone(),
            };
            None
        }
    }

    /// Mark bootstrap as successful.
    pub fn mark_connected(&mut self, via_layer: BootstrapLayer, peer_count: usize) {
        self.state = BootstrapState::Connected {
            via_layer,
            peer_count,
        };
    }

    /// Get the next untried layer (in priority order).
    fn next_layer(&self) -> Option<BootstrapLayer> {
        let all_layers = [
            BootstrapLayer::Social,
            BootstrapLayer::Local,
            BootstrapLayer::Http,
            BootstrapLayer::Dht,
            BootstrapLayer::Dns,
            BootstrapLayer::Hardcoded,
        ];
        all_layers
            .into_iter()
            .find(|l| !self.attempted_layers.contains(l))
    }

    /// Check if bootstrap has timed out on current layer.
    pub fn check_timeout(&self) -> bool {
        if let BootstrapState::Discovering { started_at, .. } = &self.state {
            started_at.elapsed().as_secs() >= BOOTSTRAP_LAYER_TIMEOUT_S
        } else {
            false
        }
    }

    /// Get seeds for a specific layer.
    pub fn seeds_for_layer(&self, layer: BootstrapLayer) -> Vec<NetworkAddress> {
        match layer {
            BootstrapLayer::Social => self.social_seeds.clone(),
            BootstrapLayer::Local => Vec::new(), // mDNS — no pre-configured seeds
            BootstrapLayer::Http => Vec::new(),  // URLs, not addresses
            BootstrapLayer::Dht => self.dht_seeds.clone(),
            BootstrapLayer::Dns => Vec::new(), // Domain names, not addresses
            BootstrapLayer::Hardcoded => self.hardcoded_seeds.clone(),
        }
    }

    /// Whether bootstrap is complete.
    pub fn is_connected(&self) -> bool {
        matches!(self.state, BootstrapState::Connected { .. })
    }
}

impl Default for BootstrapEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ─── PEX Protocol (SPEC A §6.7) ──────────────────────────────────────────

/// Peer Exchange (PEX) — ongoing peer discovery after bootstrap.
///
/// Nodes periodically exchange small batches of known peers
/// to maintain connectivity and discover new nodes.
pub struct PexState {
    /// Known peers eligible for exchange.
    pub known_peers: Vec<PexPeerInfo>,
    /// Maximum peers to send in one PEX round.
    pub max_exchange: usize,
}

/// Peer info shared via PEX.
#[derive(Debug, Clone)]
pub struct PexPeerInfo {
    pub node_id: NodeId,
    pub address: NetworkAddress,
    pub tier: u8,
    pub fitness: u16, // 0–10000 fixed-point
    pub last_verified: Instant,
}

impl PexState {
    pub fn new() -> Self {
        PexState {
            known_peers: Vec::new(),
            max_exchange: 10,
        }
    }

    /// Select peers for exchange (mix of random + high-fitness).
    pub fn select_for_exchange(&self) -> Vec<&PexPeerInfo> {
        let mut selected: Vec<&PexPeerInfo> = self.known_peers.iter().collect();
        // Sort by fitness (higher first), then take max_exchange
        selected.sort_by_key(|peer| std::cmp::Reverse(peer.fitness));
        selected.truncate(self.max_exchange);
        selected
    }

    /// Add or update peer from received PEX data.
    pub fn receive_peer(&mut self, peer: PexPeerInfo) {
        if let Some(existing) = self
            .known_peers
            .iter_mut()
            .find(|p| p.node_id == peer.node_id)
        {
            // Update if newer
            if peer.last_verified > existing.last_verified {
                *existing = peer;
            }
        } else {
            self.known_peers.push(peer);
        }
    }
}

impl Default for PexState {
    fn default() -> Self {
        Self::new()
    }
}
