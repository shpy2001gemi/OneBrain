//! # Concept Index — Phase C1+C2
//!
//! Maps concept IDs to DHT keys and populates VacuumFilters
//! so peers can discover which nodes hold which knowledge.

use crate::dht::DhtNode;
use crate::vacuum::VacuumFilter;
use ku_core::foundation::ConceptCcid;

// ═══════════════════════════════════════════════════════════════════════════
// Concept Index
// ═══════════════════════════════════════════════════════════════════════════

/// Maps global concept CCIDs to DHT-compatible 32-byte keys.
///
/// The full 16-byte CCID is domain-separated and hashed via BLAKE3.
/// This allows looking up which nodes hold KUs about a given concept.
pub struct ConceptIndex {
    /// Global concept CCIDs that this node holds KUs for.
    local_concepts: Vec<ConceptCcid>,
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

    /// Convert a global concept CCID to a domain-separated 32-byte DHT key.
    pub fn concept_to_key(concept: ConceptCcid) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:concept-dht-key/1\0");
        hasher.update(concept.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Register a concept as locally available.
    ///
    /// Adds the concept to the local index and VacuumFilter.
    pub fn register_concept(&mut self, concept: ConceptCcid) {
        if self.local_concepts.len() >= self.capacity {
            return;
        }
        if !self.local_concepts.contains(&concept) {
            self.local_concepts.push(concept);
            // Insert into VacuumFilter using the concept key
            let key = Self::concept_to_key(concept);
            self.filter.insert(&key);
        }
    }

    /// Register multiple global concepts from a KU's Concept Table.
    pub fn register_concepts(&mut self, concepts: &[ConceptCcid]) {
        for &concept in concepts {
            self.register_concept(concept);
        }
    }

    /// Check if a concept is locally available.
    pub fn has_concept(&self, concept: ConceptCcid) -> bool {
        self.local_concepts.contains(&concept)
    }

    /// Check the VacuumFilter (probabilistic) for a concept.
    ///
    /// Returns true if the concept *might* be locally available.
    /// False positives are possible; false negatives are not.
    pub fn might_have_concept(&self, concept: ConceptCcid) -> bool {
        let key = Self::concept_to_key(concept);
        self.filter.contains(&key)
    }

    /// Publish concept keys to the DHT.
    ///
    /// For each local concept, stores a pointer (our node's wire bytes)
    /// in the DHT at the concept's key position.
    pub fn publish_to_dht(&self, dht: &mut DhtNode, node_info: &[u8]) -> usize {
        let mut published = 0;
        for &concept in &self.local_concepts {
            let key = Self::concept_to_key(concept);
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

    /// Get all indexed global concept identities.
    pub fn concepts(&self) -> &[ConceptCcid] {
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

    fn concept(value: u128) -> ConceptCcid {
        ConceptCcid::from_bytes(value.to_be_bytes())
    }

    #[test]
    fn test_concept_to_key_deterministic() {
        let k1 = ConceptIndex::concept_to_key(concept(42));
        let k2 = ConceptIndex::concept_to_key(concept(42));
        assert_eq!(k1, k2);

        let k3 = ConceptIndex::concept_to_key(concept(43));
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_register_and_lookup() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concept(concept(100));
        idx.register_concept(concept(200));
        idx.register_concept(concept(300));

        assert!(idx.has_concept(concept(100)));
        assert!(idx.has_concept(concept(200)));
        assert!(!idx.has_concept(concept(400)));
        assert_eq!(idx.count(), 3);
    }

    #[test]
    fn test_vacuum_filter_populated() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concept(concept(42));
        idx.register_concept(concept(99));

        // VacuumFilter should contain registered concepts
        assert!(idx.might_have_concept(concept(42)));
        assert!(idx.might_have_concept(concept(99)));
        // Unregistered concept — most likely false
        // (VacuumFilter has false positives, so we can't assert false definitively)
    }

    #[test]
    fn test_register_bulk() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concepts(&[concept(1), concept(2), concept(3), concept(4), concept(5)]);
        assert_eq!(idx.count(), 5);
        assert!(idx.has_concept(concept(3)));
    }

    #[test]
    fn test_no_duplicate_registration() {
        let mut idx = ConceptIndex::new(100);
        idx.register_concept(concept(42));
        idx.register_concept(concept(42));
        idx.register_concept(concept(42));
        assert_eq!(idx.count(), 1);
    }

    #[test]
    fn test_capacity_limit() {
        let mut idx = ConceptIndex::new(3);
        idx.register_concept(concept(1));
        idx.register_concept(concept(2));
        idx.register_concept(concept(3));
        idx.register_concept(concept(4)); // Should be ignored
        assert_eq!(idx.count(), 3);
        assert!(!idx.has_concept(concept(4)));
    }

    #[test]
    fn test_publish_to_dht() {
        use crate::identity::{generate_node_id, KeyPair, PUZZLE_C_SMALL};

        let kp = KeyPair::generate();
        let proof = generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL);
        let mut dht = DhtNode::new(proof.node_id);

        let mut idx = ConceptIndex::new(100);
        idx.register_concept(concept(42));
        idx.register_concept(concept(99));

        let node_info = b"node:192.168.1.1:4242";
        let count = idx.publish_to_dht(&mut dht, node_info);
        assert_eq!(count, 2);
        assert_eq!(dht.storage_count(), 2);

        // Verify we can look up by concept key
        let key = ConceptIndex::concept_to_key(concept(42));
        assert!(dht.has(&key));
    }

    #[test]
    fn equal_local_ids_cannot_alias_distinct_ccids() {
        let first = ConceptCcid::from_bytes([0x11; 16]);
        let second = ConceptCcid::from_bytes([0x22; 16]);
        let mut index = ConceptIndex::new(2);
        index.register_concept(first);
        index.register_concept(second);

        assert_eq!(index.count(), 2);
        assert_ne!(
            ConceptIndex::concept_to_key(first),
            ConceptIndex::concept_to_key(second)
        );
    }
}
