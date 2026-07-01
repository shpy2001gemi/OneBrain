//! # Concept Index — Phase C1+C2
//!
//! Maps concept IDs to DHT keys and populates VacuumFilters
//! so peers can discover which nodes hold which knowledge.

use crate::dht::DhtNode;
use crate::vacuum::VacuumFilter;

// ═══════════════════════════════════════════════════════════════════════════
// Concept Index
// ═══════════════════════════════════════════════════════════════════════════

/// Maps concept IDs to DHT-compatible 32-byte keys.
///
/// A concept ID (u64) is hashed via BLAKE3 to produce a 32-byte DHT key.
/// This allows looking up which nodes hold KUs about a given concept.
pub struct ConceptIndex {
    /// Local concept IDs that this node holds KUs for.
    local_concepts: Vec<u64>,
    /// VacuumFilter advertising our concepts to neighbors.
    filter: VacuumFilter,
    /// Maximum concepts to index.
    capacity: usize,
}

impl ConceptIndex {
    /// Create a new concept index with given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            local_concepts: Vec::with_capacity(capacity),
            filter: VacuumFilter::with_defaults(capacity.max(64)),
            capacity,
        }
    }

    /// Convert a concept ID to a 32-byte DHT key.
    pub fn concept_to_key(concept_id: u64) -> [u8; 32] {
        let bytes = concept_id.to_be_bytes();
        *blake3::hash(&bytes).as_bytes()
    }

    /// Register a concept as locally available.
    ///
    /// Adds the concept to the local index and VacuumFilter.
    pub fn register_concept(&mut self, concept_id: u64) {
        if self.local_concepts.len() >= self.capacity {
            return;
        }
        if !self.local_concepts.contains(&concept_id) {
            self.local_concepts.push(concept_id);
            // Insert into VacuumFilter using the concept key
            let key = Self::concept_to_key(concept_id);
            self.filter.insert(&key);
        }
    }

    /// Register multiple concepts from a KU's codons.
    pub fn register_concepts(&mut self, concept_ids: &[u64]) {
        for &id in concept_ids {
            self.register_concept(id);
        }
    }

    /// Check if a concept is locally available.
    pub fn has_concept(&self, concept_id: u64) -> bool {
        self.local_concepts.contains(&concept_id)
    }

    /// Check the VacuumFilter (probabilistic) for a concept.
    ///
    /// Returns true if the concept *might* be locally available.
    /// False positives are possible; false negatives are not.
    pub fn might_have_concept(&self, concept_id: u64) -> bool {
        let key = Self::concept_to_key(concept_id);
        self.filter.contains(&key)
    }

    /// Publish concept keys to the DHT.
    ///
    /// For each local concept, stores a pointer (our node's wire bytes)
    /// in the DHT at the concept's key position.
    pub fn publish_to_dht(&self, dht: &mut DhtNode, node_info: &[u8]) -> usize {
        let mut published = 0;
        for &concept_id in &self.local_concepts {
            let key = Self::concept_to_key(concept_id);
            if dht.store(key, node_info.to_vec()).is_ok() {
                published += 1;
            }
        }
        published
    }

    /// Get the VacuumFilter for sharing with neighbors.
    pub fn filter(&self) -> &VacuumFilter {
        &self.filter
    }

    /// Number of indexed concepts.
    pub fn count(&self) -> usize {
        self.local_concepts.len()
    }

    /// Get all indexed concept IDs.
    pub fn concepts(&self) -> &[u64] {
        &self.local_concepts
    }
}

impl Default for ConceptIndex {
    fn default() -> Self {
        Self::new(10_000)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concept_to_key_deterministic() {
        let k1 = ConceptIndex::concept_to_key(42);
        let k2 = ConceptIndex::concept_to_key(42);
        assert_eq!(k1, k2);

        let k3 = ConceptIndex::concept_to_key(43);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_register_and_lookup() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concept(100);
        idx.register_concept(200);
        idx.register_concept(300);

        assert!(idx.has_concept(100));
        assert!(idx.has_concept(200));
        assert!(!idx.has_concept(400));
        assert_eq!(idx.count(), 3);
    }

    #[test]
    fn test_vacuum_filter_populated() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concept(42);
        idx.register_concept(99);

        // VacuumFilter should contain registered concepts
        assert!(idx.might_have_concept(42));
        assert!(idx.might_have_concept(99));
        // Unregistered concept — most likely false
        // (VacuumFilter has false positives, so we can't assert false definitively)
    }

    #[test]
    fn test_register_bulk() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concepts(&[1, 2, 3, 4, 5]);
        assert_eq!(idx.count(), 5);
        assert!(idx.has_concept(3));
    }

    #[test]
    fn test_no_duplicate_registration() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concept(42);
        idx.register_concept(42);
        idx.register_concept(42);
        assert_eq!(idx.count(), 1);
    }

    #[test]
    fn test_capacity_limit() {
        let mut idx = ConceptIndex::new(3);
        idx.register_concept(1);
        idx.register_concept(2);
        idx.register_concept(3);
        idx.register_concept(4); // Should be ignored
        assert_eq!(idx.count(), 3);
        assert!(!idx.has_concept(4));
    }

    #[test]
    fn test_publish_to_dht() {
        use crate::identity::{generate_node_id, KeyPair, PUZZLE_C_SMALL};

        let kp = KeyPair::generate();
        let proof = generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL);
        let mut dht = DhtNode::new(proof.node_id);

        let mut idx = ConceptIndex::new(100);
        idx.register_concept(42);
        idx.register_concept(99);

        let node_info = b"node:192.168.1.1:4242";
        let count = idx.publish_to_dht(&mut dht, node_info);
        assert_eq!(count, 2);
        assert_eq!(dht.storage_count(), 2);

        // Verify we can look up by concept key
        let key = ConceptIndex::concept_to_key(42);
        assert!(dht.has(&key));
    }
}
