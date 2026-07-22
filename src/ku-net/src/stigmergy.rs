//! # Stigmergy Routing — SPEC B §6
//!
//! Bio-inspired pheromone-based content routing.
//! Nodes reinforce paths that successfully delivered queries/content,
//! creating "trails" for future queries on the same topics.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::constants::*;
use crate::identity::NodeId;

// ─── Types ─────────────────────────────────────────────────────────────────

/// Topic identifier (BLAKE3 hash of topic label).
pub type TopicId = [u8; 32];

/// Entry in the pheromone table for a specific topic.
#[derive(Debug, Clone)]
pub struct PheromoneEntry {
    /// The topic this entry is for.
    pub topic_id: TopicId,
    /// Next-hop candidates with pheromone strength.
    /// Higher strength = more likely to be chosen.
    pub next_hops: Vec<PheromoneHop>,
    /// When this entry was last reinforced.
    pub last_reinforced: Instant,
}

/// A single next-hop candidate with pheromone strength.
#[derive(Debug, Clone)]
pub struct PheromoneHop {
    /// The peer to route through.
    pub node_id: NodeId,
    /// Pheromone strength [0.0, 1.0]. Decays over time.
    pub strength: f32,
    /// Number of successful deliveries via this hop.
    pub success_count: u32,
    /// Number of failed deliveries via this hop.
    pub failure_count: u32,
}

// ─── Pheromone Table ───────────────────────────────────────────────────────

/// Pheromone routing table for stigmergy-based content discovery.
///
/// Each topic has a list of next-hops ranked by pheromone strength.
/// Successful queries reinforce paths; failed queries weaken them.
/// All pheromones decay over time (evaporation).
pub struct PheromoneTable {
    /// Topic → pheromone entry mapping.
    entries: HashMap<TopicId, PheromoneEntry>,
    /// Maximum number of entries.
    #[allow(dead_code)]
    capacity: usize,
    /// Pheromone decay rate per hour (τ = 0.95).
    decay_rate: f32,
}

impl PheromoneTable {
    /// Create a new pheromone table with default parameters.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            capacity: MAX_PHEROMONE_ENTRIES,
            decay_rate: PHEROMONE_DECAY,
        }
    }

    /// Reinforce a path: increase pheromone for topic → next_hop.
    ///
    /// Called when a query via `next_hop` for `topic` was successful.
    pub fn reinforce(&mut self, topic: &TopicId, via: &NodeId, success: bool) {
        let entry = self
            .entries
            .entry(*topic)
            .or_insert_with(|| PheromoneEntry {
                topic_id: *topic,
                next_hops: Vec::new(),
                last_reinforced: Instant::now(),
            });

        entry.last_reinforced = Instant::now();

        // Find or create hop entry
        if let Some(hop) = entry.next_hops.iter_mut().find(|h| h.node_id == *via) {
            if success {
                hop.strength = (hop.strength + 0.1).min(1.0);
                hop.success_count += 1;
            } else {
                hop.strength = (hop.strength - 0.2).max(0.0);
                hop.failure_count += 1;
            }
        } else if success {
            // New hop — only add on success
            entry.next_hops.push(PheromoneHop {
                node_id: *via,
                strength: 0.3, // Initial pheromone
                success_count: 1,
                failure_count: 0,
            });
        }

        // Sort by strength descending
        entry
            .next_hops
            .sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap());

        // Limit next_hops to top 10
        entry.next_hops.truncate(10);
    }

    /// Apply time-based evaporation to all pheromones.
    ///
    /// Should be called periodically (e.g., every hour).
    /// Removes entries with zero-strength hops.
    pub fn evaporate(&mut self, elapsed: Duration) {
        let hours = elapsed.as_secs_f32() / 3600.0;
        let decay = self.decay_rate.powf(hours);

        let mut to_remove = Vec::new();

        for (topic_id, entry) in &mut self.entries {
            for hop in &mut entry.next_hops {
                hop.strength *= decay;
            }
            // Remove dead hops (strength < 0.01)
            entry.next_hops.retain(|h| h.strength >= 0.01);

            if entry.next_hops.is_empty() {
                to_remove.push(*topic_id);
            }
        }

        for topic_id in to_remove {
            self.entries.remove(&topic_id);
        }
    }

    /// Get the best next-hop for a topic.
    pub fn best_next_hop(&self, topic: &TopicId) -> Option<NodeId> {
        self.entries
            .get(topic)
            .and_then(|e| e.next_hops.first())
            .map(|h| h.node_id)
    }

    /// Get ranked next-hops for a topic, excluding specified nodes.
    ///
    /// Used for query routing: exclude nodes that have already failed.
    pub fn route_query(&self, topic: &TopicId, exclude: &[NodeId]) -> Vec<NodeId> {
        self.entries
            .get(topic)
            .map(|e| {
                e.next_hops
                    .iter()
                    .filter(|h| !exclude.contains(&h.node_id))
                    .filter(|h| h.strength >= 0.05)
                    .map(|h| h.node_id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Number of tracked topics.
    pub fn topic_count(&self) -> usize {
        self.entries.len()
    }

    /// Get pheromone entry for a topic (for inspection).
    pub fn get_entry(&self, topic: &TopicId) -> Option<&PheromoneEntry> {
        self.entries.get(topic)
    }
}

impl Default for PheromoneTable {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{generate_node_id, KeyPair, PUZZLE_C_SMALL};

    fn make_node_id() -> NodeId {
        let kp = KeyPair::generate();
        generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL).node_id
    }

    fn make_topic(label: &str) -> TopicId {
        *blake3::hash(label.as_bytes()).as_bytes()
    }

    #[test]
    fn test_reinforce_new_topic() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("physics");
        let via = make_node_id();

        table.reinforce(&topic, &via, true);

        assert_eq!(table.topic_count(), 1);
        let entry = table.get_entry(&topic).unwrap();
        assert_eq!(entry.next_hops.len(), 1);
        assert_eq!(entry.next_hops[0].strength, 0.3);
        assert_eq!(entry.next_hops[0].success_count, 1);
    }

    #[test]
    fn test_reinforce_increases_strength() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("chemistry");
        let via = make_node_id();

        // Reinforce 5 times
        for _ in 0..5 {
            table.reinforce(&topic, &via, true);
        }

        let entry = table.get_entry(&topic).unwrap();
        assert!(entry.next_hops[0].strength > 0.3);
        assert_eq!(entry.next_hops[0].success_count, 5);
    }

    #[test]
    fn test_failure_decreases_strength() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("biology");
        let via = make_node_id();

        // Initial success
        table.reinforce(&topic, &via, true);
        let initial = table.get_entry(&topic).unwrap().next_hops[0].strength;

        // Failure
        table.reinforce(&topic, &via, false);
        let after_fail = table.get_entry(&topic).unwrap().next_hops[0].strength;
        assert!(after_fail < initial);
    }

    #[test]
    fn test_best_next_hop() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("math");
        let node_a = make_node_id();
        let node_b = make_node_id();

        // Node A: 1 success
        table.reinforce(&topic, &node_a, true);
        // Node B: 3 successes (stronger)
        for _ in 0..3 {
            table.reinforce(&topic, &node_b, true);
        }

        let best = table.best_next_hop(&topic).unwrap();
        assert_eq!(best, node_b, "Node B should be best (more reinforcements)");
    }

    #[test]
    fn test_route_query_excludes_nodes() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("history");
        let node_a = make_node_id();
        let node_b = make_node_id();

        table.reinforce(&topic, &node_a, true);
        table.reinforce(&topic, &node_b, true);

        // Exclude node_a
        let routes = table.route_query(&topic, &[node_a]);
        assert!(!routes.contains(&node_a));
        assert!(routes.contains(&node_b));
    }

    #[test]
    fn test_evaporate_reduces_strength() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("art");
        let via = make_node_id();

        table.reinforce(&topic, &via, true);
        let before = table.get_entry(&topic).unwrap().next_hops[0].strength;

        // Evaporate 10 hours
        table.evaporate(Duration::from_secs(10 * 3600));
        let after = table.get_entry(&topic).unwrap().next_hops[0].strength;

        assert!(after < before, "Strength should decrease after evaporation");
    }

    #[test]
    fn test_evaporate_removes_dead_entries() {
        let mut table = PheromoneTable::new();
        let topic = make_topic("ancient");
        let via = make_node_id();

        table.reinforce(&topic, &via, true);

        // Evaporate 1000 hours — should kill entry
        table.evaporate(Duration::from_secs(1000 * 3600));
        assert_eq!(table.topic_count(), 0, "Dead topics should be removed");
    }
}
