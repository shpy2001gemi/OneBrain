//! # Sync Protocol — Delta-State CRDT Exchange
//!
//! Implements the OBP sync protocol for eventual consistency:
//! - Delta exchange based on vector clocks
//! - Anti-entropy gossip (periodic + triggered)
//! - Sync request/response message framing

use std::collections::HashMap;

use ku_core::crdt::VectorClock;
use crate::identity::NodeId;

// ═══════════════════════════════════════════════════════════════════════════
// Sync Messages
// ═══════════════════════════════════════════════════════════════════════════

/// A sync request from one node to another.
#[derive(Debug, Clone)]
pub struct SyncRequest {
    /// Sender's node ID.
    pub sender: NodeId,
    /// Sender's vector clock (so receiver knows what we already have).
    pub clock: VectorClock,
    /// Specific CIDs we're requesting (empty = full sync).
    pub requested_cids: Vec<[u8; 32]>,
}

/// A sync response containing deltas.
#[derive(Debug, Clone)]
pub struct SyncResponse {
    /// Responder's node ID.
    pub sender: NodeId,
    /// Responder's current vector clock.
    pub clock: VectorClock,
    /// KU deltas (CID → wire-encoded bytes).
    pub deltas: Vec<SyncDelta>,
}

/// A single delta entry (a KU that the receiver doesn't have).
#[derive(Debug, Clone)]
pub struct SyncDelta {
    /// Content ID (BLAKE3 hash).
    pub cid: [u8; 32],
    /// Wire-encoded KU bytes.
    pub data: Vec<u8>,
    /// The vector clock at the time this KU was created/updated.
    pub version: VectorClock,
}

/// Acknowledgment of received deltas.
#[derive(Debug, Clone)]
pub struct SyncAck {
    /// Sender of the ACK.
    pub sender: NodeId,
    /// Updated clock after applying deltas.
    pub clock: VectorClock,
    /// CIDs successfully received.
    pub received_cids: Vec<[u8; 32]>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Sync State (per-peer)
// ═══════════════════════════════════════════════════════════════════════════

/// Tracks sync state with a specific peer.
#[derive(Debug, Clone)]
pub struct PeerSyncState {
    /// The peer's last known vector clock.
    pub last_clock: VectorClock,
    /// Number of successful syncs.
    pub sync_count: u64,
    /// Number of deltas sent to this peer.
    pub deltas_sent: u64,
    /// Number of deltas received from this peer.
    pub deltas_received: u64,
}

impl PeerSyncState {
    fn new() -> Self {
        Self {
            last_clock: VectorClock::new(),
            sync_count: 0,
            deltas_sent: 0,
            deltas_received: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Sync Manager
// ═══════════════════════════════════════════════════════════════════════════

/// Manages synchronization state across all peers.
pub struct SyncManager {
    /// Our node ID.
    my_id: NodeId,
    /// Our node's numeric ID (for vector clock).
    my_numeric_id: u64,
    /// Our current vector clock.
    pub clock: VectorClock,
    /// Per-peer sync state.
    peers: HashMap<NodeId, PeerSyncState>,
    /// Local KU store: CID → wire bytes.
    local_store: HashMap<[u8; 32], Vec<u8>>,
    /// CID → version clock.
    versions: HashMap<[u8; 32], VectorClock>,
}

impl SyncManager {
    /// Create a new sync manager.
    pub fn new(my_id: NodeId) -> Self {
        let my_numeric_id = u64::from_be_bytes(my_id.0[0..8].try_into().unwrap());
        Self {
            my_id,
            my_numeric_id,
            clock: VectorClock::new(),
            peers: HashMap::new(),
            local_store: HashMap::new(),
            versions: HashMap::new(),
        }
    }

    /// Store a KU locally and tick our clock.
    pub fn store_local(&mut self, cid: [u8; 32], data: Vec<u8>) {
        self.clock.tick(self.my_numeric_id);
        let version = self.clock.clone();
        self.local_store.insert(cid, data);
        self.versions.insert(cid, version);
    }

    /// Generate a sync request for a specific peer.
    pub fn create_sync_request(&self) -> SyncRequest {
        SyncRequest {
            sender: self.my_id,
            clock: self.clock.clone(),
            requested_cids: Vec::new(),
        }
    }

    /// Handle an incoming sync request — compute deltas to send back.
    pub fn handle_sync_request(&self, request: &SyncRequest) -> SyncResponse {
        let mut deltas = Vec::new();

        // Find all KUs that the requester doesn't have
        // (their clock < our version for each KU)
        for (cid, version) in &self.versions {
            // If requester's clock doesn't cover this version,
            // they need this delta
            if !request.clock.covers(version) {
                if let Some(data) = self.local_store.get(cid) {
                    deltas.push(SyncDelta {
                        cid: *cid,
                        data: data.clone(),
                        version: version.clone(),
                    });
                }
            }
        }

        SyncResponse {
            sender: self.my_id,
            clock: self.clock.clone(),
            deltas,
        }
    }

    /// Apply received deltas from a sync response.
    pub fn apply_sync_response(&mut self, response: &SyncResponse) -> Vec<[u8; 32]> {
        let mut applied = Vec::new();

        for delta in &response.deltas {
            // Check if this is newer than what we have
            let dominated = self.versions.get(&delta.cid)
                .map(|our_ver| our_ver.dominates(&delta.version))
                .unwrap_or(false);

            if !dominated {
                self.local_store.insert(delta.cid, delta.data.clone());
                // Merge version clocks
                let entry = self.versions.entry(delta.cid)
                    .or_insert_with(VectorClock::new);
                entry.merge(&delta.version);
                applied.push(delta.cid);
            }
        }

        // Update peer state
        let peer_state = self.peers.entry(response.sender)
            .or_insert_with(PeerSyncState::new);
        peer_state.last_clock.merge(&response.clock);
        peer_state.sync_count += 1;
        peer_state.deltas_received += applied.len() as u64;

        // Merge clocks
        self.clock.merge(&response.clock);

        applied
    }

    /// Generate ACK for received deltas.
    pub fn create_sync_ack(&self, received: &[[u8; 32]]) -> SyncAck {
        SyncAck {
            sender: self.my_id,
            clock: self.clock.clone(),
            received_cids: received.to_vec(),
        }
    }

    /// Number of locally stored KUs.
    pub fn local_count(&self) -> usize {
        self.local_store.len()
    }

    /// Number of tracked peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Get sync stats for a peer.
    pub fn peer_stats(&self, peer_id: &NodeId) -> Option<&PeerSyncState> {
        self.peers.get(peer_id)
    }

    /// Check if we have a specific CID.
    pub fn has_cid(&self, cid: &[u8; 32]) -> bool {
        self.local_store.contains_key(cid)
    }

    /// Get data for a CID.
    pub fn get_data(&self, cid: &[u8; 32]) -> Option<&Vec<u8>> {
        self.local_store.get(cid)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{generate_node_id, KeyPair, PUZZLE_C_SMALL};

    fn make_node_id() -> NodeId {
        let kp = KeyPair::generate();
        generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL).node_id
    }

    #[test]
    fn test_store_local_ticks_clock() {
        let node_id = make_node_id();
        let mut mgr = SyncManager::new(node_id);

        mgr.store_local([0xAA; 32], b"data1".to_vec());
        mgr.store_local([0xBB; 32], b"data2".to_vec());

        assert_eq!(mgr.local_count(), 2);
        // Clock should have ticked twice
        let numeric_id = u64::from_be_bytes(node_id.0[0..8].try_into().unwrap());
        assert_eq!(mgr.clock.get(numeric_id), 2);
    }

    #[test]
    fn test_sync_request_response() {
        let id_a = make_node_id();
        let id_b = make_node_id();

        let mut node_a = SyncManager::new(id_a);
        let mut node_b = SyncManager::new(id_b);

        // Node A has data
        node_a.store_local([0x11; 32], b"ku_1".to_vec());
        node_a.store_local([0x22; 32], b"ku_2".to_vec());

        // Node B requests sync
        let request = node_b.create_sync_request();

        // Node A responds with deltas
        let response = node_a.handle_sync_request(&request);
        assert_eq!(response.deltas.len(), 2, "Should send 2 deltas");

        // Node B applies
        let applied = node_b.apply_sync_response(&response);
        assert_eq!(applied.len(), 2);
        assert!(node_b.has_cid(&[0x11; 32]));
        assert!(node_b.has_cid(&[0x22; 32]));
    }

    #[test]
    fn test_sync_incremental() {
        let id_a = make_node_id();
        let id_b = make_node_id();

        let mut node_a = SyncManager::new(id_a);
        let mut node_b = SyncManager::new(id_b);

        // Round 1: A has 1 KU
        node_a.store_local([0x11; 32], b"ku_1".to_vec());

        let req1 = node_b.create_sync_request();
        let resp1 = node_a.handle_sync_request(&req1);
        assert_eq!(resp1.deltas.len(), 1);
        node_b.apply_sync_response(&resp1);

        // Round 2: A adds 1 more KU
        node_a.store_local([0x22; 32], b"ku_2".to_vec());

        let req2 = node_b.create_sync_request();
        let resp2 = node_a.handle_sync_request(&req2);
        // Should only send the new KU (incremental)
        assert_eq!(resp2.deltas.len(), 1, "Should only send new delta");
    }

    #[test]
    fn test_sync_bidirectional() {
        let id_a = make_node_id();
        let id_b = make_node_id();

        let mut node_a = SyncManager::new(id_a);
        let mut node_b = SyncManager::new(id_b);

        // Each node has different data
        node_a.store_local([0xAA; 32], b"from_a".to_vec());
        node_b.store_local([0xBB; 32], b"from_b".to_vec());

        // A → B
        let req = node_a.create_sync_request();
        let resp = node_b.handle_sync_request(&req);
        node_a.apply_sync_response(&resp);

        // B → A
        let req = node_b.create_sync_request();
        let resp = node_a.handle_sync_request(&req);
        node_b.apply_sync_response(&resp);

        // Both should now have both KUs
        assert!(node_a.has_cid(&[0xAA; 32]));
        assert!(node_a.has_cid(&[0xBB; 32]));
        assert!(node_b.has_cid(&[0xAA; 32]));
        assert!(node_b.has_cid(&[0xBB; 32]));
    }

    #[test]
    fn test_sync_idempotent() {
        let id_a = make_node_id();
        let id_b = make_node_id();

        let mut node_a = SyncManager::new(id_a);
        let mut node_b = SyncManager::new(id_b);

        node_a.store_local([0x11; 32], b"data".to_vec());

        // Sync twice
        let req = node_b.create_sync_request();
        let resp = node_a.handle_sync_request(&req);
        node_b.apply_sync_response(&resp);

        let req2 = node_b.create_sync_request();
        let resp2 = node_a.handle_sync_request(&req2);
        // Second sync should have 0 deltas (already synced)
        assert_eq!(resp2.deltas.len(), 0, "Should be 0 after full sync");
    }

    #[test]
    fn test_peer_stats() {
        let id_a = make_node_id();
        let id_b = make_node_id();

        let mut node_a = SyncManager::new(id_a);
        let mut node_b = SyncManager::new(id_b);

        node_a.store_local([0x11; 32], b"data".to_vec());

        let req = node_b.create_sync_request();
        let resp = node_a.handle_sync_request(&req);
        node_b.apply_sync_response(&resp);

        let stats = node_b.peer_stats(&id_a).unwrap();
        assert_eq!(stats.sync_count, 1);
        assert_eq!(stats.deltas_received, 1);
    }
}
