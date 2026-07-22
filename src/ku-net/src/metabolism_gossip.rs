//! # Metabolism Gossip — Network Handler for PoK v2
//!
//! Handles the 3 metabolism message types:
//! - 0x86 MetabolismUpdate: Receive CRDT delta from peer
//! - 0x87 MetabolismQuery: Peer requests metabolism for a CID
//! - 0x89 MetabolismResponse: Response to a query
//!
//! ## Protocol:
//! 1. Periodic: node picks random peers, sends MetabolismUpdate with
//!    deltas for recently-changed KUs
//! 2. On-demand: node sends MetabolismQuery for specific CID,
//!    peer responds with MetabolismResponse
//!
//! ## CRDT Safety:
//! All merges use GCounter.merge() — idempotent, commutative, monotonic.

use crate::messages::MessageType;
use ku_core::metabolism::KUMetabolism;
use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Wire Types
// ═══════════════════════════════════════════════════════════════════════════

/// MetabolismUpdate payload (0x86)
///
/// Sent periodically via gossip. Contains CRDT deltas for one or more KUs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetabolismUpdateMsg {
    /// Sender's node ID
    pub sender: u64,
    /// Per-CID metabolism snapshots (max 20 per message to stay under MTU)
    pub updates: Vec<MetabolismDelta>,
    /// Sender's local timestamp
    pub timestamp: u64,
}

/// A single KU's metabolism delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetabolismDelta {
    /// CID of the KU
    pub cid: [u8; 32],
    /// Full metabolism snapshot (GCounters serialize compactly)
    pub metabolism: KUMetabolism,
}

/// MetabolismQuery payload (0x87)
///
/// Request metabolism data for specific CIDs from a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetabolismQueryMsg {
    /// Requester's node ID
    pub requester: u64,
    /// CIDs to query
    pub cids: Vec<[u8; 32]>,
    /// Request ID for correlation
    pub request_id: u64,
}

/// MetabolismResponse payload (0x89)
///
/// Response to a MetabolismQuery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetabolismResponseMsg {
    /// Responder's node ID
    pub responder: u64,
    /// Requested metabolism data (only for CIDs the responder knows about)
    pub data: Vec<MetabolismDelta>,
    /// Correlating request ID
    pub request_id: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Gossip Handler
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum KU deltas per gossip message
pub const MAX_DELTAS_PER_MESSAGE: usize = 20;

/// Handler for metabolism gossip messages.
///
/// Stateless — operates on a provided metabolism store.
/// Each node runs one handler instance.
pub struct MetabolismGossipHandler;

impl MetabolismGossipHandler {
    /// Handle incoming MetabolismUpdate (0x86).
    ///
    /// Merges each delta into the local metabolism store.
    /// Returns number of KUs merged.
    pub fn handle_update(
        store: &mut ku_core::metabolism_store::MetabolismStore,
        msg: &MetabolismUpdateMsg,
    ) -> usize {
        let mut merged = 0;
        for delta in &msg.updates {
            store.merge_remote(delta.cid, &delta.metabolism);
            merged += 1;
        }
        merged
    }

    /// Handle incoming MetabolismQuery (0x87).
    ///
    /// Returns a MetabolismResponse with data for known CIDs.
    pub fn handle_query(
        store: &ku_core::metabolism_store::MetabolismStore,
        msg: &MetabolismQueryMsg,
    ) -> MetabolismResponseMsg {
        let mut data = Vec::new();

        for cid in &msg.cids {
            if let Some(metabolism) = store.get(cid) {
                data.push(MetabolismDelta {
                    cid: *cid,
                    metabolism: metabolism.clone(),
                });
            }
        }

        MetabolismResponseMsg {
            responder: 0, // Caller fills in actual node ID
            data,
            request_id: msg.request_id,
        }
    }

    /// Handle incoming MetabolismResponse (0x89).
    ///
    /// Merges response data into local store.
    /// Returns number of KUs merged.
    pub fn handle_response(
        store: &mut ku_core::metabolism_store::MetabolismStore,
        msg: &MetabolismResponseMsg,
    ) -> usize {
        let mut merged = 0;
        for delta in &msg.data {
            store.merge_remote(delta.cid, &delta.metabolism);
            merged += 1;
        }
        merged
    }

    /// Prepare a gossip update message with the top N most active KUs.
    ///
    /// Call this periodically (e.g., every 30s) and send to random peers.
    pub fn prepare_update(
        store: &ku_core::metabolism_store::MetabolismStore,
        sender: u64,
        now: u64,
        max_deltas: usize,
    ) -> MetabolismUpdateMsg {
        let top = store.top_active(max_deltas.min(MAX_DELTAS_PER_MESSAGE), now);

        let updates: Vec<MetabolismDelta> = top
            .iter()
            .filter_map(|(cid, _rate)| {
                store.get(cid).map(|m| MetabolismDelta {
                    cid: *cid,
                    metabolism: m.clone(),
                })
            })
            .collect();

        MetabolismUpdateMsg {
            sender,
            updates,
            timestamp: now,
        }
    }

    /// Prepare a query for specific CIDs.
    pub fn prepare_query(
        requester: u64,
        cids: Vec<[u8; 32]>,
        request_id: u64,
    ) -> MetabolismQueryMsg {
        MetabolismQueryMsg {
            requester,
            cids,
            request_id,
        }
    }

    /// Get the message type for each handler.
    pub fn message_type_update() -> MessageType {
        MessageType::MetabolismUpdate
    }
    pub fn message_type_query() -> MessageType {
        MessageType::MetabolismQuery
    }
    pub fn message_type_response() -> MessageType {
        MessageType::MetabolismResponse
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::metabolism::{KUMetabolism, MetabolismEvent};
    use ku_core::metabolism_store::MetabolismStore;

    const T0: u64 = 1_000_000;
    const NODE_A: u64 = 1;
    const NODE_B: u64 = 2;

    fn test_cid(id: u8) -> [u8; 32] {
        let mut cid = [0u8; 32];
        cid[0] = id;
        cid
    }

    #[test]
    fn test_handle_update_merges() {
        let mut store = MetabolismStore::new(NODE_A);
        store.record_event(test_cid(1), MetabolismEvent::QueryHit, T0);

        // Remote node has different events
        let mut remote_m = KUMetabolism::new(T0);
        remote_m.record_event(NODE_B, MetabolismEvent::Citation, T0 + 100);
        remote_m.record_event(NODE_B, MetabolismEvent::QueryHit, T0 + 200);

        let msg = MetabolismUpdateMsg {
            sender: NODE_B,
            updates: vec![MetabolismDelta {
                cid: test_cid(1),
                metabolism: remote_m,
            }],
            timestamp: T0 + 300,
        };

        let merged = MetabolismGossipHandler::handle_update(&mut store, &msg);
        assert_eq!(merged, 1);

        // Local should now have merged data
        let m = store.get(&test_cid(1)).unwrap();
        assert!(
            m.total_engagement() >= 3,
            "Should have local + remote: {}",
            m.total_engagement()
        );
    }

    #[test]
    fn test_handle_query_response_cycle() {
        let mut store_a = MetabolismStore::new(NODE_A);
        store_a.record_event(test_cid(1), MetabolismEvent::QueryHit, T0);
        store_a.record_event(test_cid(1), MetabolismEvent::Citation, T0 + 100);
        store_a.record_event(test_cid(2), MetabolismEvent::QueryHit, T0 + 200);

        // Node B queries for CID 1 and CID 3 (doesn't exist)
        let query =
            MetabolismGossipHandler::prepare_query(NODE_B, vec![test_cid(1), test_cid(3)], 42);

        // Node A handles query
        let response = MetabolismGossipHandler::handle_query(&store_a, &query);

        assert_eq!(response.request_id, 42);
        assert_eq!(response.data.len(), 1, "Only CID 1 found, CID 3 unknown");
        assert_eq!(response.data[0].cid, test_cid(1));

        // Node B merges response
        let mut store_b = MetabolismStore::new(NODE_B);
        let merged = MetabolismGossipHandler::handle_response(&mut store_b, &response);
        assert_eq!(merged, 1);
        assert!(store_b.get(&test_cid(1)).is_some());
    }

    #[test]
    fn test_prepare_update() {
        let mut store = MetabolismStore::new(NODE_A);

        // Add some KUs
        for i in 0..5 {
            store.record_event(test_cid(i), MetabolismEvent::QueryHit, T0 + i as u64 * 100);
        }

        let msg = MetabolismGossipHandler::prepare_update(&store, NODE_A, T0 + 1000, 3);
        assert!(msg.updates.len() <= 3, "Max 3 deltas");
        assert_eq!(msg.sender, NODE_A);
    }

    #[test]
    fn test_update_idempotent() {
        let mut store = MetabolismStore::new(NODE_A);

        let mut remote_m = KUMetabolism::new(T0);
        remote_m.record_event(NODE_B, MetabolismEvent::QueryHit, T0);

        let msg = MetabolismUpdateMsg {
            sender: NODE_B,
            updates: vec![MetabolismDelta {
                cid: test_cid(1),
                metabolism: remote_m.clone(),
            }],
            timestamp: T0,
        };

        // Apply same update twice
        MetabolismGossipHandler::handle_update(&mut store, &msg);
        let after_first = store.get(&test_cid(1)).unwrap().total_engagement();

        MetabolismGossipHandler::handle_update(&mut store, &msg);
        let after_second = store.get(&test_cid(1)).unwrap().total_engagement();

        assert_eq!(after_first, after_second, "Idempotent: CRDT merge is safe");
    }

    #[test]
    fn test_message_types() {
        assert_eq!(MetabolismGossipHandler::message_type_update() as u8, 0x86);
        assert_eq!(MetabolismGossipHandler::message_type_query() as u8, 0x87);
        assert_eq!(MetabolismGossipHandler::message_type_response() as u8, 0x89);
    }

    #[test]
    fn test_max_deltas_capped() {
        let mut store = MetabolismStore::new(NODE_A);
        for i in 0..50 {
            store.record_event(test_cid(i), MetabolismEvent::QueryHit, T0 + i as u64);
        }

        let msg = MetabolismGossipHandler::prepare_update(&store, NODE_A, T0 + 1000, 100);
        assert!(
            msg.updates.len() <= MAX_DELTAS_PER_MESSAGE,
            "Capped at {}: got {}",
            MAX_DELTAS_PER_MESSAGE,
            msg.updates.len()
        );
    }
}
