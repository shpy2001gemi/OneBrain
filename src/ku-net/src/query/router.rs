//! # Query Router — Phase D1+D2
//!
//! 6-layer scope escalation for distributed KQL queries:
//!
//! ```text
//! L0: LOCAL      → Search own KuStorage
//! L1: NEIGHBORS  → 1-hop SWIM peers
//! L2: CLUSTER    → Super-peer routing
//! L3: DHT        → Kademlia concept lookup
//! L4: SEMANTIC   → Stigmergy pheromone routing
//! L5: GLOBAL     → Random walk + TTL flooding
//! ```
//!
//! The router decides which layers to query and in what order.
//! For SCOPE AUTO, it starts local and escalates outward.

use crate::identity::NodeId;
use crate::dht::DhtNode;
use crate::stigmergy::PheromoneTable;
use super::index::ConceptIndex;
use super::messages::{QueryScope, QueryForwardMsg, QueryId};

// ═══════════════════════════════════════════════════════════════════════════
// Query Router
// ═══════════════════════════════════════════════════════════════════════════

/// Routing decision: which nodes to forward a query to.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Target nodes to forward the query to.
    pub targets: Vec<RoutingTarget>,
    /// The scope level that produced these targets.
    pub scope: QueryScope,
    /// Whether to escalate to the next scope if results are insufficient.
    pub should_escalate: bool,
}

/// A single routing target.
#[derive(Debug, Clone)]
pub struct RoutingTarget {
    /// The target node.
    pub node_id: NodeId,
    /// Confidence that this node has relevant results [0.0, 1.0].
    pub confidence: f32,
    /// How this target was selected.
    pub source: TargetSource,
}

/// How a routing target was discovered.
#[derive(Debug, Clone, PartialEq)]
pub enum TargetSource {
    /// Direct neighbor (SWIM member list).
    Neighbor,
    /// Super-peer redirect.
    SuperPeer,
    /// DHT closest-node lookup.
    DhtLookup,
    /// Stigmergy pheromone trail.
    Pheromone,
    /// Random walk (last resort).
    RandomWalk,
    /// VacuumFilter match.
    VacuumFilter,
}

/// The query router handles scope-based routing decisions.
pub struct QueryRouter {
    /// Our node ID.
    my_id: NodeId,
    /// Known neighbor nodes (from SWIM).
    neighbors: Vec<NodeId>,
    /// Maximum neighbors to query at each scope.
    max_fanout: usize,
    /// Queries currently in flight (for dedup).
    inflight: Vec<QueryId>,
    /// Maximum concurrent queries.
    max_inflight: usize,
}

impl QueryRouter {
    /// Create a new query router.
    pub fn new(my_id: NodeId) -> Self {
        Self {
            my_id,
            neighbors: Vec::new(),
            max_fanout: 5,
            inflight: Vec::new(),
            max_inflight: 64,
        }
    }

    /// Update the neighbor list (called when SWIM membership changes).
    pub fn update_neighbors(&mut self, neighbors: Vec<NodeId>) {
        self.neighbors = neighbors;
    }

    /// Add a neighbor.
    pub fn add_neighbor(&mut self, node_id: NodeId) {
        if !self.neighbors.contains(&node_id) {
            self.neighbors.push(node_id);
        }
    }

    /// Remove a neighbor.
    pub fn remove_neighbor(&mut self, node_id: &NodeId) {
        self.neighbors.retain(|n| n != node_id);
    }

    /// Number of known neighbors.
    pub fn neighbor_count(&self) -> usize {
        self.neighbors.len()
    }

    /// Route a query based on its scope.
    ///
    /// Returns a RoutingDecision with target nodes and whether to escalate.
    pub fn route(
        &mut self,
        query: &QueryForwardMsg,
        concept_index: &ConceptIndex,
        dht: &DhtNode,
        pheromone: &PheromoneTable,
    ) -> RoutingDecision {
        // Dedup: reject if already inflight
        if self.inflight.contains(&query.query_id) {
            return RoutingDecision {
                targets: Vec::new(),
                scope: query.scope,
                should_escalate: false,
            };
        }

        if self.inflight.len() < self.max_inflight {
            self.inflight.push(query.query_id);
        }

        match query.scope {
            QueryScope::Local => self.route_local(),
            QueryScope::Neighbors => self.route_neighbors(query),
            QueryScope::Cluster => self.route_cluster(query),
            QueryScope::Dht => self.route_dht(query, concept_index, dht),
            QueryScope::Semantic => self.route_semantic(query, pheromone),
            QueryScope::Global => self.route_global(query),
        }
    }

    /// Auto-escalate: determine the next scope to try.
    pub fn next_scope(current: QueryScope) -> Option<QueryScope> {
        match current {
            QueryScope::Local => Some(QueryScope::Neighbors),
            QueryScope::Neighbors => Some(QueryScope::Cluster),
            QueryScope::Cluster => Some(QueryScope::Dht),
            QueryScope::Dht => Some(QueryScope::Semantic),
            QueryScope::Semantic => Some(QueryScope::Global),
            QueryScope::Global => None, // No more scopes
        }
    }

    /// Mark a query as completed (remove from inflight).
    pub fn complete_query(&mut self, query_id: &QueryId) {
        self.inflight.retain(|id| id != query_id);
    }

    /// Number of queries currently in flight.
    pub fn inflight_count(&self) -> usize {
        self.inflight.len()
    }

    // ─── Layer Routing ─────────────────────────────────────────────────────

    fn route_local(&self) -> RoutingDecision {
        // Local: no forwarding needed, execute on self
        RoutingDecision {
            targets: vec![RoutingTarget {
                node_id: self.my_id,
                confidence: 1.0,
                source: TargetSource::Neighbor,
            }],
            scope: QueryScope::Local,
            should_escalate: true, // Escalate if not enough results
        }
    }

    fn route_neighbors(&self, query: &QueryForwardMsg) -> RoutingDecision {
        let targets: Vec<RoutingTarget> = self.neighbors.iter()
            .filter(|n| !query.has_visited(n))
            .take(self.max_fanout)
            .map(|n| RoutingTarget {
                node_id: *n,
                confidence: 0.5, // Unknown confidence for plain neighbors
                source: TargetSource::Neighbor,
            })
            .collect();

        RoutingDecision {
            targets,
            scope: QueryScope::Neighbors,
            should_escalate: true,
        }
    }

    fn route_cluster(&self, query: &QueryForwardMsg) -> RoutingDecision {
        // Cluster routing: use super-peers (highest-tier neighbors)
        // For now, just pick more neighbors with SuperPeer source
        let targets: Vec<RoutingTarget> = self.neighbors.iter()
            .filter(|n| !query.has_visited(n))
            .take(self.max_fanout)
            .map(|n| RoutingTarget {
                node_id: *n,
                confidence: 0.6,
                source: TargetSource::SuperPeer,
            })
            .collect();

        RoutingDecision {
            targets,
            scope: QueryScope::Cluster,
            should_escalate: true,
        }
    }

    fn route_dht(
        &self,
        query: &QueryForwardMsg,
        concept_index: &ConceptIndex,
        dht: &DhtNode,
    ) -> RoutingDecision {
        let mut targets = Vec::new();

        // Use concept hints to look up DHT keys
        for &concept_id in &query.concept_hints {
            let key = ConceptIndex::concept_to_key(concept_id);
            let closest = dht.find_closest_nodes(&key);
            for entry in closest.iter().take(3) {
                if !query.has_visited(&entry.node_id) && entry.node_id != self.my_id {
                    targets.push(RoutingTarget {
                        node_id: entry.node_id,
                        confidence: 0.7,
                        source: TargetSource::DhtLookup,
                    });
                }
            }
        }

        // If no concept hints, use all concepts from the index
        if targets.is_empty() {
            for &concept_id in concept_index.concepts().iter().take(3) {
                let key = ConceptIndex::concept_to_key(concept_id);
                let closest = dht.find_closest_nodes(&key);
                for entry in closest.iter().take(2) {
                    if !query.has_visited(&entry.node_id) && entry.node_id != self.my_id {
                        targets.push(RoutingTarget {
                            node_id: entry.node_id,
                            confidence: 0.4,
                            source: TargetSource::DhtLookup,
                        });
                    }
                }
            }
        }

        // Dedup targets
        targets.dedup_by(|a, b| a.node_id == b.node_id);
        targets.truncate(self.max_fanout);

        RoutingDecision {
            targets,
            scope: QueryScope::Dht,
            should_escalate: true,
        }
    }

    fn route_semantic(
        &self,
        query: &QueryForwardMsg,
        pheromone: &PheromoneTable,
    ) -> RoutingDecision {
        let mut targets = Vec::new();
        let visited: Vec<NodeId> = query.visited.clone();

        // Use pheromone trails for each concept hint
        for &concept_id in &query.concept_hints {
            let topic_key = ConceptIndex::concept_to_key(concept_id);
            let hops = pheromone.route_query(&topic_key, &visited);

            for node_id in hops.into_iter().take(3) {
                if node_id != self.my_id {
                    targets.push(RoutingTarget {
                        node_id,
                        confidence: 0.8, // Pheromone trails have high confidence
                        source: TargetSource::Pheromone,
                    });
                }
            }
        }

        targets.dedup_by(|a, b| a.node_id == b.node_id);
        targets.truncate(self.max_fanout);

        RoutingDecision {
            targets,
            scope: QueryScope::Semantic,
            should_escalate: true,
        }
    }

    fn route_global(&self, query: &QueryForwardMsg) -> RoutingDecision {
        // Global: random walk — pick random unvisited neighbors
        let targets: Vec<RoutingTarget> = self.neighbors.iter()
            .filter(|n| !query.has_visited(n))
            .take(self.max_fanout)
            .map(|n| RoutingTarget {
                node_id: *n,
                confidence: 0.2, // Low confidence, broad search
                source: TargetSource::RandomWalk,
            })
            .collect();

        RoutingDecision {
            targets,
            scope: QueryScope::Global,
            should_escalate: false, // No more scopes after global
        }
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

    fn setup_router() -> (QueryRouter, NodeId) {
        let my_id = make_node_id();
        let mut router = QueryRouter::new(my_id);
        // Add some neighbors
        for _ in 0..5 {
            router.add_neighbor(make_node_id());
        }
        (router, my_id)
    }

    #[test]
    fn test_route_local() {
        let (mut router, my_id) = setup_router();
        let concept_index = ConceptIndex::new(100);
        let dht = DhtNode::new(my_id);
        let pheromone = PheromoneTable::new();

        let msg = QueryForwardMsg::new(
            "FIND (k:KU)".to_string(),
            my_id,
            QueryScope::Local,
            10,
        );

        let decision = router.route(&msg, &concept_index, &dht, &pheromone);
        assert_eq!(decision.scope, QueryScope::Local);
        assert_eq!(decision.targets.len(), 1);
        assert_eq!(decision.targets[0].node_id, my_id);
        assert!(decision.should_escalate);
    }

    #[test]
    fn test_route_neighbors() {
        let (mut router, my_id) = setup_router();
        let concept_index = ConceptIndex::new(100);
        let dht = DhtNode::new(my_id);
        let pheromone = PheromoneTable::new();

        let msg = QueryForwardMsg::new(
            "FIND (k:KU)".to_string(),
            my_id,
            QueryScope::Neighbors,
            10,
        );

        let decision = router.route(&msg, &concept_index, &dht, &pheromone);
        assert_eq!(decision.scope, QueryScope::Neighbors);
        assert!(!decision.targets.is_empty());
        assert!(decision.targets.len() <= 5);

        for target in &decision.targets {
            assert_eq!(target.source, TargetSource::Neighbor);
        }
    }

    #[test]
    fn test_scope_escalation() {
        assert_eq!(QueryRouter::next_scope(QueryScope::Local), Some(QueryScope::Neighbors));
        assert_eq!(QueryRouter::next_scope(QueryScope::Neighbors), Some(QueryScope::Cluster));
        assert_eq!(QueryRouter::next_scope(QueryScope::Cluster), Some(QueryScope::Dht));
        assert_eq!(QueryRouter::next_scope(QueryScope::Dht), Some(QueryScope::Semantic));
        assert_eq!(QueryRouter::next_scope(QueryScope::Semantic), Some(QueryScope::Global));
        assert_eq!(QueryRouter::next_scope(QueryScope::Global), None);
    }

    #[test]
    fn test_dedup_inflight() {
        let (mut router, my_id) = setup_router();
        let concept_index = ConceptIndex::new(100);
        let dht = DhtNode::new(my_id);
        let pheromone = PheromoneTable::new();

        let msg = QueryForwardMsg::new(
            "FIND (k:KU)".to_string(),
            my_id,
            QueryScope::Neighbors,
            10,
        );

        // First route — should produce targets
        let d1 = router.route(&msg, &concept_index, &dht, &pheromone);
        assert!(!d1.targets.is_empty());
        assert_eq!(router.inflight_count(), 1);

        // Second route with same query — should produce empty (dedup)
        let d2 = router.route(&msg, &concept_index, &dht, &pheromone);
        assert!(d2.targets.is_empty());

        // Complete the query
        router.complete_query(&msg.query_id);
        assert_eq!(router.inflight_count(), 0);
    }

    #[test]
    fn test_neighbor_management() {
        let my_id = make_node_id();
        let mut router = QueryRouter::new(my_id);

        let n1 = make_node_id();
        let n2 = make_node_id();

        router.add_neighbor(n1);
        router.add_neighbor(n2);
        assert_eq!(router.neighbor_count(), 2);

        // No duplicate
        router.add_neighbor(n1);
        assert_eq!(router.neighbor_count(), 2);

        router.remove_neighbor(&n1);
        assert_eq!(router.neighbor_count(), 1);
    }

    #[test]
    fn test_skip_visited_nodes() {
        let (mut router, my_id) = setup_router();
        let concept_index = ConceptIndex::new(100);
        let dht = DhtNode::new(my_id);
        let pheromone = PheromoneTable::new();

        let mut msg = QueryForwardMsg::new(
            "FIND (k:KU)".to_string(),
            my_id,
            QueryScope::Neighbors,
            10,
        );

        // Mark all neighbors as visited
        for n in &router.neighbors.clone() {
            msg.forward_through(*n);
        }

        let decision = router.route(&msg, &concept_index, &dht, &pheromone);
        // All neighbors visited → no targets
        assert!(decision.targets.is_empty());
    }
}
