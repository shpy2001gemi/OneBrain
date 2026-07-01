//! # Query Messages — Phase C3
//!
//! Wire format for distributed KQL queries.
//! Uses existing MessageType 0x50-0x52 (QueryForward, QueryResponse, QueryCancel).

use serde::{Serialize, Deserialize};
use crate::identity::NodeId;

// ═══════════════════════════════════════════════════════════════════════════
// Query Wire Messages
// ═══════════════════════════════════════════════════════════════════════════

/// Unique query identifier for tracking distributed queries.
pub type QueryId = [u8; 16];

/// Scope level for query execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum QueryScope {
    Local      = 0,
    Neighbors  = 1,
    Cluster    = 2,
    Dht        = 3,
    Semantic   = 4,
    Global     = 5,
}

/// A query forwarded to another node (MessageType 0x50 QueryForward).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryForwardMsg {
    /// Unique query ID for dedup and response matching.
    pub query_id: QueryId,
    /// The originator of the query.
    pub origin: NodeId,
    /// KQL query string.
    pub kql: String,
    /// Current scope level.
    pub scope: QueryScope,
    /// Remaining TTL (decremented at each hop).
    pub ttl: u8,
    /// Maximum results wanted.
    pub max_results: u32,
    /// Concept IDs extracted from the query (for routing hints).
    pub concept_hints: Vec<u64>,
    /// Nodes already visited (loop prevention).
    pub visited: Vec<NodeId>,
}

/// A response to a forwarded query (MessageType 0x51 QueryResponse).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResponseMsg {
    /// Matching query ID.
    pub query_id: QueryId,
    /// The responding node.
    pub responder: NodeId,
    /// Serialized KU results (Core DNA wire bytes).
    pub results_payload: Vec<u8>,
    /// Number of results in this response.
    pub result_count: u32,
    /// Total results available at the responder (may be more than sent).
    pub total_available: u32,
    /// The scope at which results were found.
    pub scope_found: QueryScope,
}

/// Cancel a running distributed query (MessageType 0x52 QueryCancel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCancelMsg {
    /// Query to cancel.
    pub query_id: QueryId,
    /// Who is cancelling.
    pub origin: NodeId,
    /// Reason code: 0=enough_results, 1=timeout, 2=user_cancel.
    pub reason: u8,
}

/// Watch registration forwarded to neighbors (MessageType 0x41 WatchRegister).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRegisterMsg {
    /// Unique watch ID.
    pub watch_id: u64,
    /// The originator node.
    pub origin: NodeId,
    /// KQL FIND pattern (the inner query of WATCH).
    pub kql_pattern: String,
    /// Event filter: 0=CREATE, 1=UPDATE, 2=DEPRECATE, 3=ANY.
    pub event_filter: u8,
    /// TTL for propagation.
    pub ttl: u8,
}

/// Notification when a watch matches (MessageType 0x40 WatchNotify).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchNotifyMsg {
    /// The matching watch ID.
    pub watch_id: u64,
    /// Node that detected the match.
    pub notifier: NodeId,
    /// Serialized matching KU (Core DNA wire bytes).
    pub ku_payload: Vec<u8>,
    /// What event triggered: 0=CREATE, 1=UPDATE, 2=DEPRECATE.
    pub event_type: u8,
}

impl QueryForwardMsg {
    /// Create a new query forward message.
    pub fn new(kql: String, origin: NodeId, scope: QueryScope, max_results: u32) -> Self {
        let mut query_id = [0u8; 16];
        let hash = blake3::hash(kql.as_bytes());
        query_id.copy_from_slice(&hash.as_bytes()[..16]);
        // Mix in origin to make unique per node
        for (i, b) in origin.0[..16].iter().enumerate() {
            query_id[i] ^= b;
        }

        Self {
            query_id,
            origin,
            kql,
            scope,
            ttl: match scope {
                QueryScope::Local => 0,
                QueryScope::Neighbors => 1,
                QueryScope::Cluster => 3,
                QueryScope::Dht => 8,
                QueryScope::Semantic => 5,
                QueryScope::Global => 12,
            },
            max_results,
            concept_hints: Vec::new(),
            visited: vec![origin],
        }
    }

    /// Check if we've already visited a node (loop prevention).
    pub fn has_visited(&self, node_id: &NodeId) -> bool {
        self.visited.contains(node_id)
    }

    /// Add a node to the visited set and decrement TTL.
    pub fn forward_through(&mut self, node_id: NodeId) {
        self.visited.push(node_id);
        self.ttl = self.ttl.saturating_sub(1);
    }

    /// Whether this query can still be forwarded.
    pub fn can_forward(&self) -> bool {
        self.ttl > 0
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
        let proof = generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL);
        proof.node_id
    }

    #[test]
    fn test_query_forward_creation() {
        let origin = make_node_id();
        let msg = QueryForwardMsg::new(
            "FIND (k:KU) WHERE k.trust_score > 5000".to_string(),
            origin,
            QueryScope::Neighbors,
            10,
        );

        assert_eq!(msg.scope, QueryScope::Neighbors);
        assert_eq!(msg.ttl, 1);
        assert_eq!(msg.max_results, 10);
        assert!(msg.has_visited(&origin));
        assert!(msg.can_forward());
    }

    #[test]
    fn test_query_forward_ttl_decrement() {
        let origin = make_node_id();
        let mut msg = QueryForwardMsg::new(
            "FIND (k:KU)".to_string(),
            origin,
            QueryScope::Neighbors,
            5,
        );

        assert_eq!(msg.ttl, 1);
        let hop = make_node_id();
        msg.forward_through(hop);
        assert_eq!(msg.ttl, 0);
        assert!(!msg.can_forward());
        assert!(msg.has_visited(&hop));
    }

    #[test]
    fn test_query_scope_ordering() {
        assert!(QueryScope::Local < QueryScope::Neighbors);
        assert!(QueryScope::Neighbors < QueryScope::Cluster);
        assert!(QueryScope::Cluster < QueryScope::Dht);
        assert!(QueryScope::Dht < QueryScope::Semantic);
        assert!(QueryScope::Semantic < QueryScope::Global);
    }

    #[test]
    fn test_dht_scope_ttl() {
        let origin = make_node_id();
        let msg = QueryForwardMsg::new(
            "FIND (k:KU)".to_string(),
            origin,
            QueryScope::Dht,
            10,
        );
        assert_eq!(msg.ttl, 8);
    }

    #[test]
    fn test_query_id_unique_per_origin() {
        let origin1 = make_node_id();
        let origin2 = make_node_id();

        let msg1 = QueryForwardMsg::new("FIND (k:KU)".to_string(), origin1, QueryScope::Local, 5);
        let msg2 = QueryForwardMsg::new("FIND (k:KU)".to_string(), origin2, QueryScope::Local, 5);

        // Same KQL but different origins → different query IDs
        assert_ne!(msg1.query_id, msg2.query_id);
    }
}
