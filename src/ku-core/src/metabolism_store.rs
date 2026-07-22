//! # Metabolism Store — Per-KU Storage & Sync
//!
//! Stores KUMetabolism instances for all known KUs on this node.
//! Supports CRDT merge for incoming remote metabolism data,
//! garbage collection of dead KUs, and delta sync generation.

use crate::metabolism::{KUMetabolism, MetabolismEvent, DEFAULT_HALF_LIFE_SECS};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Maximum number of KU metabolisms to track
pub const MAX_METABOLISM_ENTRIES: usize = 100_000;

/// GC threshold — KUs below this metabolic rate are eligible for removal
pub const GC_RATE_THRESHOLD: f64 = 0.0001;

/// Maximum age for GC (365 days in seconds) — never GC if younger
pub const GC_MIN_AGE_SECS: u64 = 365 * 24 * 3600;

// ═══════════════════════════════════════════════════════════════════════════
// Metabolism Delta (for sync)
// ═══════════════════════════════════════════════════════════════════════════

/// A sync delta containing metabolism data for a single KU.
#[derive(Debug, Clone)]
pub struct MetabolismDelta {
    /// CID of the KU
    pub cid: [u8; 32],
    /// Full metabolism state (CRDT — merge is safe)
    pub metabolism: KUMetabolism,
}

// ═══════════════════════════════════════════════════════════════════════════
// Metabolism Store
// ═══════════════════════════════════════════════════════════════════════════

/// Per-node store of all KU metabolisms.
///
/// Each node maintains its own view — CRDT merge guarantees
/// eventual consistency across the network.
#[derive(Debug)]
pub struct MetabolismStore {
    /// CID → metabolism tracker
    entries: HashMap<[u8; 32], KUMetabolism>,
    /// This node's ID (for GCounter increments)
    my_node_id: u64,
}

impl MetabolismStore {
    /// Create a new empty store.
    pub fn new(my_node_id: u64) -> Self {
        Self {
            entries: HashMap::new(),
            my_node_id,
        }
    }

    /// Record a metabolism event for a KU.
    ///
    /// If the KU doesn't exist yet, creates a new entry.
    pub fn record_event(&mut self, cid: [u8; 32], event: MetabolismEvent, timestamp: u64) {
        let entry = self
            .entries
            .entry(cid)
            .or_insert_with(|| KUMetabolism::new(timestamp));
        entry.record_event(self.my_node_id, event, timestamp);
    }

    /// Get the current metabolic rate for a KU.
    pub fn get_rate(&self, cid: &[u8; 32], now: u64) -> Option<f64> {
        self.entries
            .get(cid)
            .map(|m| m.metabolic_rate(now, DEFAULT_HALF_LIFE_SECS))
    }

    /// Get the metabolism tracker for a KU.
    pub fn get(&self, cid: &[u8; 32]) -> Option<&KUMetabolism> {
        self.entries.get(cid)
    }

    /// Merge remote metabolism data (from CRDT sync).
    pub fn merge_remote(&mut self, cid: [u8; 32], remote: &KUMetabolism) {
        match self.entries.get_mut(&cid) {
            Some(local) => local.merge(remote),
            None => {
                self.entries.insert(cid, remote.clone());
            }
        }
    }

    /// Apply a batch of sync deltas from a remote node.
    pub fn apply_sync_deltas(&mut self, deltas: &[MetabolismDelta]) {
        for delta in deltas {
            self.merge_remote(delta.cid, &delta.metabolism);
        }
    }

    /// Create sync deltas for all entries (full state).
    ///
    /// In production, this should be filtered by version/clock
    /// to send only changes since last sync.
    pub fn create_sync_deltas(&self) -> Vec<MetabolismDelta> {
        self.entries
            .iter()
            .map(|(&cid, metabolism)| MetabolismDelta {
                cid,
                metabolism: metabolism.clone(),
            })
            .collect()
    }

    /// Garbage collect dead KUs.
    ///
    /// Removes entries where:
    /// - metabolic_rate < threshold AND
    /// - age > min_age AND
    /// - total_engagement == 0 (never actually used)
    ///
    /// Returns number of entries removed.
    pub fn gc_dead(&mut self, now: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, m| {
            let age = now.saturating_sub(m.created_at);
            let rate = m.metabolic_rate(now, DEFAULT_HALF_LIFE_SECS);

            // Keep if: alive OR young OR has any engagement
            rate > GC_RATE_THRESHOLD || age < GC_MIN_AGE_SECS || m.total_engagement() > 0
        });
        before - self.entries.len()
    }

    /// Number of tracked KUs.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the store empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get top N most metabolically active KUs.
    pub fn top_active(&self, n: usize, now: u64) -> Vec<([u8; 32], f64)> {
        let mut rated: Vec<_> = self
            .entries
            .iter()
            .map(|(&cid, m)| (cid, m.metabolic_rate(now, DEFAULT_HALF_LIFE_SECS)))
            .collect();
        rated.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        rated.truncate(n);
        rated
    }

    /// Total metabolic rate across all KUs (for reward distribution).
    pub fn total_metabolic_rate(&self, now: u64) -> f64 {
        self.entries
            .values()
            .map(|m| m.metabolic_rate(now, DEFAULT_HALF_LIFE_SECS))
            .sum()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const NODE_1: u64 = 1;
    const NODE_2: u64 = 2;
    const T0: u64 = 1_000_000;

    fn test_cid(id: u8) -> [u8; 32] {
        let mut cid = [0u8; 32];
        cid[0] = id;
        cid
    }

    #[test]
    fn test_store_record_and_get() {
        let mut store = MetabolismStore::new(NODE_1);
        let cid = test_cid(1);

        store.record_event(cid, MetabolismEvent::QueryHit, T0);
        store.record_event(cid, MetabolismEvent::Citation, T0 + 1);

        assert_eq!(store.len(), 1);
        let rate = store.get_rate(&cid, T0 + 1).unwrap();
        assert!(rate > 0.0);
    }

    #[test]
    fn test_store_multiple_kus() {
        let mut store = MetabolismStore::new(NODE_1);

        store.record_event(test_cid(1), MetabolismEvent::QueryHit, T0);
        store.record_event(test_cid(2), MetabolismEvent::Citation, T0);
        store.record_event(test_cid(3), MetabolismEvent::QueryHit, T0);

        assert_eq!(store.len(), 3);
    }

    #[test]
    fn test_store_merge_remote() {
        let mut store1 = MetabolismStore::new(NODE_1);
        let mut store2 = MetabolismStore::new(NODE_2);
        let cid = test_cid(1);

        store1.record_event(cid, MetabolismEvent::QueryHit, T0);
        store2.record_event(cid, MetabolismEvent::Citation, T0 + 1);

        // Sync: store2 → store1
        let deltas = store2.create_sync_deltas();
        store1.apply_sync_deltas(&deltas);

        let m = store1.get(&cid).unwrap();
        assert_eq!(m.query_hits.value(), 1, "Query from store1");
        assert_eq!(m.citation_count.value(), 1, "Citation from store2");
    }

    #[test]
    fn test_store_gc_dead() {
        let mut store = MetabolismStore::new(NODE_1);
        let cid = test_cid(1);

        // Create a KU with no activity
        store.entries.insert(cid, KUMetabolism::new(T0));

        // GC at far future — but engagement is 0 AND age > 1 year
        let far_future = T0 + 2 * GC_MIN_AGE_SECS;
        let removed = store.gc_dead(far_future);
        assert_eq!(removed, 1, "Dead KU should be GC'd");
        assert!(store.is_empty());
    }

    #[test]
    fn test_store_gc_preserves_active() {
        let mut store = MetabolismStore::new(NODE_1);
        let cid = test_cid(1);

        store.record_event(cid, MetabolismEvent::QueryHit, T0);

        // Even far future — engagement > 0 so it's preserved
        let far_future = T0 + 2 * GC_MIN_AGE_SECS;
        let removed = store.gc_dead(far_future);
        assert_eq!(removed, 0, "Active KU should NOT be GC'd");
    }

    #[test]
    fn test_store_top_active() {
        let mut store = MetabolismStore::new(NODE_1);

        // CID 1: lots of activity
        for _ in 0..10 {
            store.record_event(test_cid(1), MetabolismEvent::QueryHit, T0);
        }
        // CID 2: some activity
        store.record_event(test_cid(2), MetabolismEvent::QueryHit, T0);
        // CID 3: no activity
        store.entries.insert(test_cid(3), KUMetabolism::new(T0));

        let top = store.top_active(2, T0 + 1);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, test_cid(1), "CID 1 should be most active");
    }

    #[test]
    fn test_store_total_metabolic_rate() {
        let mut store = MetabolismStore::new(NODE_1);
        store.record_event(test_cid(1), MetabolismEvent::QueryHit, T0);
        store.record_event(test_cid(2), MetabolismEvent::Citation, T0);

        let total = store.total_metabolic_rate(T0 + 1);
        assert!(total > 0.0, "Total should be positive with activity");
    }
}
