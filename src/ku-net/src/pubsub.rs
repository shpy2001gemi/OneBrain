//! # Topic-Based Pub/Sub — SPEC B §7
//!
//! Publish-subscribe system for real-time KU propagation.
//! Nodes subscribe to topics (domain_codes) and receive
//! KU pushes matching their interest vector.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::identity::NodeId;

// ─── Types ─────────────────────────────────────────────────────────────────

/// Topic identifier — 16-bit domain code from SPEC C.
pub type DomainCode = u16;

/// Reserved domain code for encoding job announcements (Hybrid DHT+PubSub).
pub const ENCODING_JOBS_TOPIC: u16 = 0xFFFF;

/// Subscription entry for a topic.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// The subscriber node.
    pub node_id: NodeId,
    /// When the subscription was created.
    pub subscribed_at: Instant,
    /// TTL: number of hops to propagate (0 = local only).
    pub ttl: u8,
}

/// Interest vector: compact representation of a node's topic interests.
///
/// The 16-byte topic vector (from SWIM membership) is a Bloom filter
/// of the node's subscribed domain codes.
#[derive(Debug, Clone, Default)]
pub struct InterestVector {
    /// Raw 128-bit interest vector.
    pub bits: [u8; 16],
}

// ─── Pub/Sub Manager ───────────────────────────────────────────────────────

/// Manages topic subscriptions and message routing.
pub struct PubSubManager {
    /// Topic → set of subscribers.
    subscriptions: HashMap<DomainCode, Vec<Subscription>>,
    /// Our own subscriptions (topics we're interested in).
    my_topics: HashSet<DomainCode>,
    /// Our interest vector (summary of my_topics).
    my_interest: InterestVector,
    /// Maximum subscriptions per topic.
    max_subs_per_topic: usize,
}

impl PubSubManager {
    /// Create a new pub/sub manager.
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
            my_topics: HashSet::new(),
            my_interest: InterestVector::default(),
            max_subs_per_topic: 100,
        }
    }

    /// Subscribe to a topic.
    pub fn subscribe(&mut self, topic: DomainCode) {
        self.my_topics.insert(topic);
        self.rebuild_interest_vector();
    }

    /// Unsubscribe from a topic.
    pub fn unsubscribe(&mut self, topic: DomainCode) {
        self.my_topics.remove(&topic);
        self.rebuild_interest_vector();
    }

    /// Register a remote node's subscription.
    pub fn add_subscriber(&mut self, topic: DomainCode, subscriber: Subscription) {
        let subs = self.subscriptions.entry(topic).or_default();
        // Don't duplicate
        if !subs.iter().any(|s| s.node_id == subscriber.node_id)
            && subs.len() < self.max_subs_per_topic
        {
            subs.push(subscriber);
        }
    }

    /// Remove a subscriber from a topic.
    pub fn remove_subscriber(&mut self, topic: DomainCode, node_id: &NodeId) {
        if let Some(subs) = self.subscriptions.get_mut(&topic) {
            subs.retain(|s| s.node_id != *node_id);
        }
    }

    /// Remove a node from all topic subscriptions (e.g., node left).
    pub fn remove_node(&mut self, node_id: &NodeId) {
        for subs in self.subscriptions.values_mut() {
            subs.retain(|s| s.node_id != *node_id);
        }
    }

    /// Find subscribers for a given topic (for message routing).
    pub fn subscribers_for(&self, topic: DomainCode) -> Vec<&Subscription> {
        self.subscriptions
            .get(&topic)
            .map(|subs| subs.iter().collect())
            .unwrap_or_default()
    }

    /// Check if we're interested in a topic.
    pub fn is_interested(&self, topic: DomainCode) -> bool {
        self.my_topics.contains(&topic)
    }

    /// Check if a node's interest vector matches any of our topics.
    ///
    /// Uses the Bloom filter representation for fast checking.
    pub fn interests_overlap(a: &InterestVector, b: &InterestVector) -> bool {
        for i in 0..16 {
            if a.bits[i] & b.bits[i] != 0 {
                return true;
            }
        }
        false
    }

    /// Get our current interest vector.
    pub fn interest_vector(&self) -> &InterestVector {
        &self.my_interest
    }

    /// Number of topics we're subscribed to.
    pub fn my_topic_count(&self) -> usize {
        self.my_topics.len()
    }

    /// Total number of remote subscriptions we're tracking.
    pub fn total_subscriptions(&self) -> usize {
        self.subscriptions.values().map(|v| v.len()).sum()
    }

    /// Broadcast an encoding job announcement to all subscribers of the encoding topic.
    ///
    /// Returns the list of subscriber `NodeId`s that should receive the announcement.
    /// The caller is responsible for actually sending `job_bytes` to each node.
    pub fn broadcast_encoding_job(&self, _job_bytes: &[u8]) -> Vec<NodeId> {
        self.subscribers_for(ENCODING_JOBS_TOPIC)
            .iter()
            .map(|s| s.node_id)
            .collect()
    }

    // ─── Internal ──────────────────────────────────────────────────────────

    fn rebuild_interest_vector(&mut self) {
        self.my_interest = InterestVector::default();
        for &topic in &self.my_topics {
            // Set bits based on topic hash (mini Bloom filter)
            let h1 = (topic as usize) % 128;
            let h2 = ((topic as usize).wrapping_mul(7) + 13) % 128;
            let h3 = ((topic as usize).wrapping_mul(11) + 37) % 128;

            self.my_interest.bits[h1 / 8] |= 1 << (h1 % 8);
            self.my_interest.bits[h2 / 8] |= 1 << (h2 % 8);
            self.my_interest.bits[h3 / 8] |= 1 << (h3 % 8);
        }
    }
}

impl Default for PubSubManager {
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

    #[test]
    fn test_subscribe_unsubscribe() {
        let mut ps = PubSubManager::new();

        ps.subscribe(100); // Physics
        ps.subscribe(200); // Chemistry
        assert_eq!(ps.my_topic_count(), 2);
        assert!(ps.is_interested(100));

        ps.unsubscribe(100);
        assert_eq!(ps.my_topic_count(), 1);
        assert!(!ps.is_interested(100));
        assert!(ps.is_interested(200));
    }

    #[test]
    fn test_interest_vector_non_zero() {
        let mut ps = PubSubManager::new();
        ps.subscribe(42);

        let iv = ps.interest_vector();
        let has_bits = iv.bits.iter().any(|&b| b != 0);
        assert!(has_bits, "Interest vector should have bits set");
    }

    #[test]
    fn test_interests_overlap() {
        let mut ps_a = PubSubManager::new();
        ps_a.subscribe(100);

        let mut ps_b = PubSubManager::new();
        ps_b.subscribe(100); // Same topic

        assert!(PubSubManager::interests_overlap(
            ps_a.interest_vector(),
            ps_b.interest_vector()
        ));

        // Different topics may or may not overlap (Bloom filter)
        let mut ps_c = PubSubManager::new();
        ps_c.subscribe(60000);
        // Not guaranteed to be disjoint due to Bloom filter collisions
    }

    #[test]
    fn test_add_and_find_subscribers() {
        let mut ps = PubSubManager::new();
        let node_a = make_node_id();
        let node_b = make_node_id();

        ps.add_subscriber(
            100,
            Subscription {
                node_id: node_a,
                subscribed_at: Instant::now(),
                ttl: 3,
            },
        );
        ps.add_subscriber(
            100,
            Subscription {
                node_id: node_b,
                subscribed_at: Instant::now(),
                ttl: 2,
            },
        );

        let subs = ps.subscribers_for(100);
        assert_eq!(subs.len(), 2);
        assert_eq!(ps.total_subscriptions(), 2);
    }

    #[test]
    fn test_remove_node_from_all_topics() {
        let mut ps = PubSubManager::new();
        let node = make_node_id();

        ps.add_subscriber(
            100,
            Subscription {
                node_id: node,
                subscribed_at: Instant::now(),
                ttl: 3,
            },
        );
        ps.add_subscriber(
            200,
            Subscription {
                node_id: node,
                subscribed_at: Instant::now(),
                ttl: 3,
            },
        );

        assert_eq!(ps.total_subscriptions(), 2);

        ps.remove_node(&node);
        assert_eq!(ps.total_subscriptions(), 0);
    }

    #[test]
    fn test_no_duplicate_subscribers() {
        let mut ps = PubSubManager::new();
        let node = make_node_id();

        ps.add_subscriber(
            100,
            Subscription {
                node_id: node,
                subscribed_at: Instant::now(),
                ttl: 3,
            },
        );
        ps.add_subscriber(
            100,
            Subscription {
                node_id: node,
                subscribed_at: Instant::now(),
                ttl: 5,
            },
        );

        assert_eq!(ps.subscribers_for(100).len(), 1, "No duplicates");
    }
}
