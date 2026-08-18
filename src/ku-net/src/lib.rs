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

pub mod constants;
pub mod dht;
#[cfg(feature = "persist")]
pub mod dht_store; // ★ Phase 3: DHT persistence (redb-backed)
pub mod discovery;
pub mod encoding_gossip; // ★ v6 NEW: Encoding consensus network protocol
pub mod encoding_job; // ★ v6 NEW: DHT-based encoding job board
pub mod encoding_stigmergy; // ★ v6 NEW: Pheromone-based job load balancing
pub mod error;
pub mod graph_gossip; // ★ OBKG: Graph gossip (FedR deltas, graph stats, dream reports)
pub mod identity;
pub mod membership;
pub mod messages;
pub mod metabolism_gossip;
pub mod obt_gossip; // ★ OBT: Gossip protocol (fork warrants, mint relay, epoch summary)
pub mod obt_transfer; // ★ OBT: Token transfer protocol messages
pub mod pubsub;
pub mod query;
pub mod registry_gossip;
pub mod replication; // ★ Phase 4: R=7 tier-aware replication manager
pub mod stigmergy;
pub mod sync;
#[cfg(feature = "quic")]
pub mod transport;
pub mod vacuum; // ★ v7 NEW: ConceptRegistry delta gossip + bloom anti-entropy

pub mod vnext_bridge_merge;
pub mod vnext_candidates;
pub mod vnext_carrier;
pub mod vnext_carrier_adapter;
#[cfg(feature = "dr-m5-chaos-harness")]
pub mod vnext_chaos;
pub mod vnext_inventory_forest;
pub mod vnext_provider_view;
#[cfg(feature = "quic")]
pub mod vnext_quic_session;
pub mod vnext_reachability;
pub mod vnext_reachability_crypto;
pub mod vnext_reachability_resolver;
pub mod vnext_reconciliation;
pub mod vnext_reconciliation_journal;
pub mod vnext_relay_discovery;
pub mod vnext_resource_gate;
pub mod vnext_route_plan;
pub mod vnext_session;

#[cfg(test)]
mod tests;
