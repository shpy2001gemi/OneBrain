//! # OBKG Graph Gossip — Network Layer
//!
//! Implements the OBKG (OneBrain Knowledge Graph) gossip protocol messages
//! for federated learning delta sync, graph statistics exchange, and dream
//! report propagation across the OBP network.
//!
//! ## Message Types (0xB0–0xB3)
//! | Code | Message            | Direction                     |
//! |------|--------------------|-------------------------------|
//! | 0xB0 | FedRDeltaPush      | Trainer → DHT neighbors       |
//! | 0xB1 | FedRDeltaPull      | Learner → Trainer             |
//! | 0xB2 | GraphStatsMessage  | Node → Gossip ring (periodic) |
//! | 0xB3 | DreamReportMessage | Node → Neighbors (post-dream) |
//!
//! ## FedR (Federated Representation) Flow
//! 1. Node trains local relation embeddings over an epoch
//! 2. Node computes deltas (diff from previous epoch weights)
//! 3. Node pushes `FedRDeltaPush` to DHT neighbors
//! 4. Receiving node merges deltas into local model
//! 5. Nodes that missed epochs send `FedRDeltaPull` to catch up
//!
//! ## Reference
//! See `docs/specs/obkg/` for full OBKG protocol specification.

use std::collections::HashMap;

use serde::{Serialize, Deserialize};

use crate::constants::*;

// ═══════════════════════════════════════════════════════════════════════════
// §1 — Graph Gossip Message Types
// ═══════════════════════════════════════════════════════════════════════════

/// Federated Representation delta push (0xB0).
///
/// Sent by a node after completing a local training epoch to share
/// embedding weight deltas with DHT neighbors for federated learning.
///
/// The `deltas` map uses relation-type ID as key, with value being a pair
/// of quantized 32-dimensional vectors representing (head_delta, tail_delta).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FedRDeltaPush {
    /// Sender's Ed25519 public key (peer identifier).
    pub peer_id: [u8; 32],
    /// Training epoch number this delta corresponds to.
    pub epoch: u64,
    /// Number of triples used in this training epoch.
    pub triple_count: u64,
    /// Quantized embedding deltas per relation type.
    /// Key: relation type ID (u8), Value: (head_delta, tail_delta) as i8 vectors.
    pub deltas: HashMap<u8, ([i8; 32], [i8; 32])>,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
    /// Ed25519 signature over BLAKE3(peer_id ‖ epoch ‖ triple_count ‖ deltas_cbor).
    pub signature: Vec<u8>,
}

/// Federated Representation delta pull request (0xB1).
///
/// Sent by a node that missed one or more epochs and wants to catch up
/// by requesting deltas from a peer that has them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FedRDeltaPull {
    /// Requester's Ed25519 public key.
    pub requester_id: [u8; 32],
    /// Minimum epoch to fetch deltas from (inclusive).
    pub min_epoch: u64,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// Graph statistics message (0xB2).
///
/// Periodically broadcast to the gossip ring so neighbors can build
/// a global view of the knowledge graph's health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStatsMessage {
    /// Sender's Ed25519 public key.
    pub peer_id: [u8; 32],
    /// Total bonds (edges) in the local OBKG fragment.
    pub total_bonds: u64,
    /// Bonds with weight above the active threshold.
    pub active_bonds: u64,
    /// Bonds that have been weakened (below reinforcement threshold).
    pub weakened: u64,
    /// Bonds marked as deprecated (pending pruning).
    pub deprecated: u64,
    /// Total KUs (Knowledge Units) held locally.
    pub ku_count: u64,
    /// Current FedR epoch number.
    pub fedr_epoch: u64,
    /// Timestamp of the last dream consolidation cycle.
    pub last_dream_at: u64,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// Dream report message (0xB3).
///
/// Broadcast after a node completes a dream consolidation cycle,
/// reporting what the dreaming process accomplished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamReportMessage {
    /// Sender's Ed25519 public key.
    pub peer_id: [u8; 32],
    /// Number of bonds reinforced during the dream.
    pub bonds_reinforced: u64,
    /// Total weight added across all reinforced bonds.
    pub total_weight_added: f64,
    /// Number of new associative bonds created.
    pub associations_created: u64,
    /// Number of weak/deprecated bonds pruned.
    pub bonds_pruned: u64,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// §2 — CBOR Serialization
// ═══════════════════════════════════════════════════════════════════════════

impl FedRDeltaPush {
    /// Serialize to CBOR bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .expect("FedRDeltaPush CBOR serialization should not fail");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_cbor(data: &[u8]) -> Result<Self, String> {
        ciborium::from_reader(data)
            .map_err(|e| format!("FedRDeltaPush CBOR decode error: {e}"))
    }
}

impl FedRDeltaPull {
    /// Serialize to CBOR bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .expect("FedRDeltaPull CBOR serialization should not fail");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_cbor(data: &[u8]) -> Result<Self, String> {
        ciborium::from_reader(data)
            .map_err(|e| format!("FedRDeltaPull CBOR decode error: {e}"))
    }
}

impl GraphStatsMessage {
    /// Serialize to CBOR bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .expect("GraphStatsMessage CBOR serialization should not fail");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_cbor(data: &[u8]) -> Result<Self, String> {
        ciborium::from_reader(data)
            .map_err(|e| format!("GraphStatsMessage CBOR decode error: {e}"))
    }
}

impl DreamReportMessage {
    /// Serialize to CBOR bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf)
            .expect("DreamReportMessage CBOR serialization should not fail");
        buf
    }

    /// Deserialize from CBOR bytes.
    pub fn from_cbor(data: &[u8]) -> Result<Self, String> {
        ciborium::from_reader(data)
            .map_err(|e| format!("DreamReportMessage CBOR decode error: {e}"))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §3 — Dispatch Enum & Router
// ═══════════════════════════════════════════════════════════════════════════

/// Unified enum for all graph gossip message types.
#[derive(Debug, Clone)]
pub enum GraphGossipMessage {
    /// Federated delta push (0xB0).
    FedRPush(FedRDeltaPush),
    /// Federated delta pull (0xB1).
    FedRPull(FedRDeltaPull),
    /// Graph statistics (0xB2).
    Stats(GraphStatsMessage),
    /// Dream report (0xB3).
    Dream(DreamReportMessage),
}

/// Dispatch a raw graph gossip message by type code.
///
/// Reads `msg_type` to determine which struct to decode from `payload`,
/// returning the appropriate `GraphGossipMessage` variant.
pub fn dispatch_graph_message(msg_type: u8, payload: &[u8]) -> Result<GraphGossipMessage, String> {
    match msg_type {
        MSG_FEDR_DELTA_PUSH => {
            let msg = FedRDeltaPush::from_cbor(payload)?;
            Ok(GraphGossipMessage::FedRPush(msg))
        }
        MSG_FEDR_DELTA_PULL => {
            let msg = FedRDeltaPull::from_cbor(payload)?;
            Ok(GraphGossipMessage::FedRPull(msg))
        }
        MSG_GRAPH_STATS => {
            let msg = GraphStatsMessage::from_cbor(payload)?;
            Ok(GraphGossipMessage::Stats(msg))
        }
        MSG_DREAM_REPORT => {
            let msg = DreamReportMessage::from_cbor(payload)?;
            Ok(GraphGossipMessage::Dream(msg))
        }
        _ => Err(format!("Unknown graph gossip message type: 0x{msg_type:02X}")),
    }
}

/// Identify a graph gossip message type from its wire code.
pub fn graph_gossip_message_name(code: u8) -> Option<&'static str> {
    match code {
        MSG_FEDR_DELTA_PUSH => Some("FedRDeltaPush"),
        MSG_FEDR_DELTA_PULL => Some("FedRDeltaPull"),
        MSG_GRAPH_STATS     => Some("GraphStatsMessage"),
        MSG_DREAM_REPORT    => Some("DreamReportMessage"),
        _ => None,
    }
}

/// Check if a message type code is a graph gossip protocol message.
pub fn is_graph_gossip_message(code: u8) -> bool {
    (0xB0..=0xB3).contains(&code)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper builders ──────────────────────────────────────────────────

    fn sample_fedr_push() -> FedRDeltaPush {
        let mut deltas = HashMap::new();
        deltas.insert(0, ([1i8; 32], [-1i8; 32]));
        deltas.insert(1, ([2i8; 32], [-2i8; 32]));
        FedRDeltaPush {
            peer_id: [0xAAu8; 32],
            epoch: 42,
            triple_count: 1000,
            deltas,
            timestamp: 1_700_000_000_000,
            signature: vec![0u8; 64],
        }
    }

    fn sample_fedr_pull() -> FedRDeltaPull {
        FedRDeltaPull {
            requester_id: [0xBBu8; 32],
            min_epoch: 10,
            timestamp: 1_700_000_000_000,
        }
    }

    fn sample_graph_stats() -> GraphStatsMessage {
        GraphStatsMessage {
            peer_id: [0xCCu8; 32],
            total_bonds: 50_000,
            active_bonds: 42_000,
            weakened: 5_000,
            deprecated: 3_000,
            ku_count: 1_200,
            fedr_epoch: 42,
            last_dream_at: 1_699_999_000_000,
            timestamp: 1_700_000_000_000,
        }
    }

    fn sample_dream_report() -> DreamReportMessage {
        DreamReportMessage {
            peer_id: [0xDDu8; 32],
            bonds_reinforced: 500,
            total_weight_added: 12.75,
            associations_created: 30,
            bonds_pruned: 120,
            timestamp: 1_700_000_000_000,
        }
    }

    // ── CBOR roundtrip tests ─────────────────────────────────────────────

    #[test]
    fn test_fedr_push_cbor_roundtrip() {
        let original = sample_fedr_push();
        let bytes = original.to_cbor();
        let decoded = FedRDeltaPush::from_cbor(&bytes).unwrap();
        assert_eq!(decoded.peer_id, original.peer_id);
        assert_eq!(decoded.epoch, original.epoch);
        assert_eq!(decoded.triple_count, original.triple_count);
        assert_eq!(decoded.deltas.len(), 2);
        assert_eq!(decoded.deltas[&0].0, [1i8; 32]);
        assert_eq!(decoded.deltas[&0].1, [-1i8; 32]);
        assert_eq!(decoded.timestamp, original.timestamp);
        assert_eq!(decoded.signature.len(), 64);
    }

    #[test]
    fn test_fedr_pull_cbor_roundtrip() {
        let original = sample_fedr_pull();
        let bytes = original.to_cbor();
        let decoded = FedRDeltaPull::from_cbor(&bytes).unwrap();
        assert_eq!(decoded.requester_id, original.requester_id);
        assert_eq!(decoded.min_epoch, original.min_epoch);
        assert_eq!(decoded.timestamp, original.timestamp);
    }

    #[test]
    fn test_graph_stats_cbor_roundtrip() {
        let original = sample_graph_stats();
        let bytes = original.to_cbor();
        let decoded = GraphStatsMessage::from_cbor(&bytes).unwrap();
        assert_eq!(decoded.peer_id, original.peer_id);
        assert_eq!(decoded.total_bonds, 50_000);
        assert_eq!(decoded.active_bonds, 42_000);
        assert_eq!(decoded.weakened, 5_000);
        assert_eq!(decoded.deprecated, 3_000);
        assert_eq!(decoded.ku_count, 1_200);
        assert_eq!(decoded.fedr_epoch, 42);
        assert_eq!(decoded.last_dream_at, original.last_dream_at);
    }

    #[test]
    fn test_dream_report_cbor_roundtrip() {
        let original = sample_dream_report();
        let bytes = original.to_cbor();
        let decoded = DreamReportMessage::from_cbor(&bytes).unwrap();
        assert_eq!(decoded.peer_id, original.peer_id);
        assert_eq!(decoded.bonds_reinforced, 500);
        assert_eq!((decoded.total_weight_added - 12.75).abs() < f64::EPSILON, true);
        assert_eq!(decoded.associations_created, 30);
        assert_eq!(decoded.bonds_pruned, 120);
    }

    // ── Dispatch tests ───────────────────────────────────────────────────

    #[test]
    fn test_dispatch_fedr_push() {
        let push = sample_fedr_push();
        let bytes = push.to_cbor();
        let msg = dispatch_graph_message(MSG_FEDR_DELTA_PUSH, &bytes).unwrap();
        match msg {
            GraphGossipMessage::FedRPush(m) => assert_eq!(m.epoch, 42),
            _ => panic!("Expected FedRPush variant"),
        }
    }

    #[test]
    fn test_dispatch_fedr_pull() {
        let pull = sample_fedr_pull();
        let bytes = pull.to_cbor();
        let msg = dispatch_graph_message(MSG_FEDR_DELTA_PULL, &bytes).unwrap();
        match msg {
            GraphGossipMessage::FedRPull(m) => assert_eq!(m.min_epoch, 10),
            _ => panic!("Expected FedRPull variant"),
        }
    }

    #[test]
    fn test_dispatch_graph_stats() {
        let stats = sample_graph_stats();
        let bytes = stats.to_cbor();
        let msg = dispatch_graph_message(MSG_GRAPH_STATS, &bytes).unwrap();
        match msg {
            GraphGossipMessage::Stats(m) => assert_eq!(m.total_bonds, 50_000),
            _ => panic!("Expected Stats variant"),
        }
    }

    #[test]
    fn test_dispatch_dream_report() {
        let dream = sample_dream_report();
        let bytes = dream.to_cbor();
        let msg = dispatch_graph_message(MSG_DREAM_REPORT, &bytes).unwrap();
        match msg {
            GraphGossipMessage::Dream(m) => assert_eq!(m.bonds_pruned, 120),
            _ => panic!("Expected Dream variant"),
        }
    }

    #[test]
    fn test_dispatch_unknown_type() {
        let result = dispatch_graph_message(0xFF, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown graph gossip message type"));
    }

    #[test]
    fn test_dispatch_invalid_payload() {
        let result = dispatch_graph_message(MSG_FEDR_DELTA_PUSH, &[0xFF, 0x00]);
        assert!(result.is_err());
    }

    // ── Constant & helper tests ──────────────────────────────────────────

    #[test]
    fn test_message_type_codes() {
        assert_eq!(MSG_FEDR_DELTA_PUSH, 0xB0);
        assert_eq!(MSG_FEDR_DELTA_PULL, 0xB1);
        assert_eq!(MSG_GRAPH_STATS, 0xB2);
        assert_eq!(MSG_DREAM_REPORT, 0xB3);
    }

    #[test]
    fn test_is_graph_gossip_message() {
        assert!(is_graph_gossip_message(0xB0));
        assert!(is_graph_gossip_message(0xB3));
        assert!(!is_graph_gossip_message(0xA0)); // OBT range
        assert!(!is_graph_gossip_message(0xB4)); // Just above
        assert!(!is_graph_gossip_message(0xAF)); // Just below
    }

    #[test]
    fn test_graph_gossip_message_name() {
        assert_eq!(graph_gossip_message_name(0xB0), Some("FedRDeltaPush"));
        assert_eq!(graph_gossip_message_name(0xB1), Some("FedRDeltaPull"));
        assert_eq!(graph_gossip_message_name(0xB2), Some("GraphStatsMessage"));
        assert_eq!(graph_gossip_message_name(0xB3), Some("DreamReportMessage"));
        assert_eq!(graph_gossip_message_name(0xFF), None);
    }
}
