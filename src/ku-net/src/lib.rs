//! # ku-net — OBP Network Protocol
//!
//! Implements the OneBrain Protocol (OBP) network layer:
//! - **Layer 0**: Cryptographic identity (NodeID, DID, Ed25519)
//! - **Layer 1**: Message framing and wire formats
//! - **Layer 2**: SWIM membership and node fitness
//! - **Layer 3**: Peer discovery (6-layer bootstrap)
//! - **Layer 4**: S/Kademlia DHT routing
//! - **Layer 5**: Stigmergy (pheromone) routing
//! - **Layer 6**: Vacuum probabilistic filters
//! - **Layer 7**: Topic-based pub/sub
//! - **Layer 8**: QUIC transport (feature-gated)
//!
//! ## Design Principles
//! - NO central servers — network self-sustains among nodes
//! - Internet is OPTIMIZATION, not REQUIREMENT
//! - Scale target: 100 BILLION+ nodes
//! - Mobile-first: <0.5% battery/day
//!
//! ## Spec References
//! - SPEC A: Identity & Transport (`02a_spec_identity_transport.md`)
//! - SPEC B: Overlay & Routing (`02b_spec_overlay_routing.md`)
//! - SPEC D: Message Catalog (`02d_message_catalog.md`)

pub mod error;
pub mod constants;
pub mod identity;
pub mod messages;
pub mod membership;
pub mod discovery;
#[cfg(feature = "quic")]
pub mod transport;
pub mod dht;
pub mod stigmergy;
pub mod vacuum;
pub mod pubsub;
pub mod query;
pub mod sync;
pub mod metabolism_gossip;
pub mod encoding_job;        // ★ v6 NEW: DHT-based encoding job board
pub mod encoding_gossip;     // ★ v6 NEW: Encoding consensus network protocol
pub mod encoding_stigmergy;  // ★ v6 NEW: Pheromone-based job load balancing
pub mod obt_transfer;        // ★ OBT: Token transfer protocol messages
pub mod obt_gossip;          // ★ OBT: Gossip protocol (fork warrants, mint relay, epoch summary)
pub mod graph_gossip;        // ★ OBKG: Graph gossip (FedR deltas, graph stats, dream reports)
#[cfg(feature = "persist")]
pub mod dht_store;           // ★ Phase 3: DHT persistence (redb-backed)
pub mod replication;         // ★ Phase 4: R=7 tier-aware replication manager

#[cfg(test)]
mod tests;
