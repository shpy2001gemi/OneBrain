//! # OBS Cache — Metabolism-Aware Adaptive Replacement Cache (M-ARC)
//!
//! Provides O(1) read-through caching for hot Knowledge Units, using
//! metabolism signal for eviction priority. Built on a simplified ARC
//! (Adaptive Replacement Cache) with 4 lists:
//!
//! - **T1**: Recently accessed entries (one-hit wonders)
//! - **T2**: Frequently accessed entries (confirmed hot)
//! - **B1**: Ghost list for T1 evictions (CID only, no data)
//! - **B2**: Ghost list for T2 evictions (CID only, no data)
//!
//! ## Query Flow
//! ```text
//! Query → Check Hot Cache (HashMap)
//!   ├── HIT: Return immediately
//!   └── MISS: Caller fetches from redb (warm tier)
//!        └── Optionally promote to hot cache
//! ```
//!
//! ## Metabolism-Aware Eviction
//! When the cache is full, eviction prefers entries with the lowest
//! `metabolic_rate`, ensuring that "dead" knowledge units are evicted
//! before active ones.

use indexmap::IndexMap;
use std::collections::VecDeque;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Default cache capacity (number of KU entries)
pub const DEFAULT_CACHE_CAPACITY: usize = 10_000;

/// Metabolic rate below which a KU is considered "dead"
pub const METABOLISM_DEAD_THRESHOLD: f64 = 0.001;

// ═══════════════════════════════════════════════════════════════════════════
// CachedKu
// ═══════════════════════════════════════════════════════════════════════════

/// Cached KU entry in the hot tier.
#[derive(Debug, Clone)]
pub struct CachedKu {
    /// Core DNA wire bytes
    pub wire_bytes: Vec<u8>,
    /// Serialized epigenetics (JSON string)
    pub epigenetics_json: String,
    /// CIDs of 1-hop neighbors (for prefetch)
    pub neighbor_cids: Vec<[u8; 32]>,
    /// Cached metabolic rate \[0.0, 1.0\]
    pub metabolic_rate: f64,
    /// Timestamp when inserted into cache (epoch seconds)
    pub inserted_at: u64,
    /// Number of cache hits since insertion
    pub hit_count: u32,
}

// ═══════════════════════════════════════════════════════════════════════════
// CacheStats
// ═══════════════════════════════════════════════════════════════════════════

/// Cache statistics snapshot.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub t1_size: usize,
    pub t2_size: usize,
    pub b1_size: usize,
    pub b2_size: usize,
    pub capacity: usize,
}

impl CacheStats {
    /// Compute cache hit rate as a fraction in \[0.0, 1.0\].
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ObsCache — Metabolism-Aware ARC
// ═══════════════════════════════════════════════════════════════════════════

/// Metabolism-Aware Adaptive Replacement Cache (M-ARC).
///
/// Uses a simplified ARC algorithm with 4 lists (T1, T2, B1, B2) and a
/// self-tuning parameter `p` that balances recency vs frequency. Eviction
/// is metabolism-aware: entries with the lowest `metabolic_rate` are evicted
/// first.
pub struct ObsCache {
    /// Maximum number of data entries (T1 + T2 ≤ capacity)
    capacity: usize,

    // ── Data lists (IndexMap: insertion order = LRU order) ──────────────
    /// T1 data: recently accessed (one-hit wonders)
    t1_data: IndexMap<[u8; 32], CachedKu>,

    /// T2 data: frequently accessed (confirmed hot)
    t2_data: IndexMap<[u8; 32], CachedKu>,

    // ── Ghost lists (CID only, no data) ─────────────────────────────────
    /// B1: ghost entries evicted from T1
    b1: VecDeque<[u8; 32]>,
    /// B2: ghost entries evicted from T2
    b2: VecDeque<[u8; 32]>,

    // ── ARC tuning ──────────────────────────────────────────────────────
    /// Target size for T1 (self-tuning, starts at capacity/2)
    p: usize,

    // ── Statistics ──────────────────────────────────────────────────────
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl ObsCache {
    /// Create a new cache with the given capacity (number of KU entries).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            t1_data: IndexMap::new(),
            t2_data: IndexMap::new(),
            b1: VecDeque::new(),
            b2: VecDeque::new(),
            p: capacity / 2,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Look up a KU in the cache. Returns `None` on miss.
    ///
    /// On hit: promotes entry within ARC structure and increments `hit_count`.
    /// - If in T1 → move to T2 (confirmed frequent)
    /// - If in T2 → move to MRU position of T2
    pub fn get(&mut self, cid: &[u8; 32]) -> Option<&CachedKu> {
        // Case 1: CID is in T1 → promote to T2
        if self.t1_data.contains_key(cid) {
            self.hits += 1;
            // Remove from T1 (O(1) with shift_remove)
            let mut entry = self.t1_data.shift_remove(cid).unwrap();
            // Increment hit count
            entry.hit_count += 1;
            // Insert into T2 (MRU position = back)
            self.t2_data.insert(*cid, entry);
            return self.t2_data.get(cid);
        }

        // Case 2: CID is in T2 → move to MRU of T2
        if self.t2_data.contains_key(cid) {
            self.hits += 1;
            // Remove and re-insert to move to back (MRU)
            let mut entry = self.t2_data.shift_remove(cid).unwrap();
            entry.hit_count += 1;
            self.t2_data.insert(*cid, entry);
            return self.t2_data.get(cid);
        }

        // Miss
        self.misses += 1;
        None
    }

    /// Check if a CID is in the cache without promoting it.
    pub fn contains(&self, cid: &[u8; 32]) -> bool {
        self.t1_data.contains_key(cid) || self.t2_data.contains_key(cid)
    }

    /// Insert or promote a KU into the cache.
    ///
    /// ARC logic:
    /// - If CID was in B1 (ghost of T1): increase `p` (favor T1), insert to T2
    /// - If CID was in B2 (ghost of T2): decrease `p` (favor T2), insert to T2
    /// - If CID already in T1 or T2: update entry in-place
    /// - Else: insert to T1
    /// - If T1 + T2 > capacity: evict based on `p` and metabolic rate
    pub fn put(&mut self, cid: [u8; 32], entry: CachedKu) {
        // Zero capacity → no-op
        if self.capacity == 0 {
            return;
        }

        // Already in T1 → update in place, move to MRU
        if self.t1_data.contains_key(&cid) {
            self.t1_data.shift_remove(&cid);
            self.t1_data.insert(cid, entry);
            return;
        }

        // Already in T2 → update in place, move to MRU
        if self.t2_data.contains_key(&cid) {
            self.t2_data.shift_remove(&cid);
            self.t2_data.insert(cid, entry);
            return;
        }

        // Check ghost lists
        let in_b1 = self.b1.contains(&cid);
        let in_b2 = self.b2.contains(&cid);

        if in_b1 {
            // Ghost hit in B1: adapt p upward (favor T1)
            let delta = std::cmp::max(1, self.b2.len() / std::cmp::max(1, self.b1.len()));
            self.p = std::cmp::min(self.capacity, self.p.saturating_add(delta));
            self.b1.retain(|c| *c != cid);
            // Insert to T2 (promoted: previously seen in T1, now confirmed)
            self.ensure_room_for_insert();
            self.t2_data.insert(cid, entry);
            return;
        }

        if in_b2 {
            // Ghost hit in B2: adapt p downward (favor T2)
            let delta = std::cmp::max(1, self.b1.len() / std::cmp::max(1, self.b2.len()));
            self.p = self.p.saturating_sub(delta);
            self.b2.retain(|c| *c != cid);
            // Insert to T2
            self.ensure_room_for_insert();
            self.t2_data.insert(cid, entry);
            return;
        }

        // New entry → insert to T1
        self.ensure_room_for_insert();
        self.t1_data.insert(cid, entry);
    }

    /// Remove a specific entry from the cache (for invalidation on epi update).
    pub fn invalidate(&mut self, cid: &[u8; 32]) {
        self.t1_data.shift_remove(cid);
        self.t2_data.shift_remove(cid);
        // Also remove from ghost lists
        self.b1.retain(|c| c != cid);
        self.b2.retain(|c| c != cid);
    }

    /// Clear entire cache (data + ghost lists + stats).
    pub fn clear(&mut self) {
        self.t1_data.clear();
        self.t2_data.clear();
        self.b1.clear();
        self.b2.clear();
        self.p = self.capacity / 2;
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
    }

    /// Number of data entries currently cached (T1 + T2).
    pub fn len(&self) -> usize {
        self.t1_data.len() + self.t2_data.len()
    }

    /// Returns `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return a snapshot of cache statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            t1_size: self.t1_data.len(),
            t2_size: self.t2_data.len(),
            b1_size: self.b1.len(),
            b2_size: self.b2.len(),
            capacity: self.capacity,
        }
    }

    /// Evict all entries with `metabolic_rate` below the given threshold.
    /// Returns the number of entries evicted.
    ///
    /// Uses `IndexMap::retain()` for O(n) single-pass eviction.
    pub fn evict_dead(&mut self, threshold: f64) -> usize {
        let before = self.t1_data.len() + self.t2_data.len();
        self.t1_data.retain(|_, v| v.metabolic_rate >= threshold);
        self.t2_data.retain(|_, v| v.metabolic_rate >= threshold);
        let after = self.t1_data.len() + self.t2_data.len();
        let evicted = before - after;
        self.evictions += evicted as u64;
        evicted
    }

    /// Get list of neighbor CIDs for prefetch from a cached entry.
    ///
    /// The `_min_weight` parameter is reserved for future filtering by
    /// bond weight; currently all neighbor CIDs are returned.
    pub fn prefetch_candidates(&self, cid: &[u8; 32], _min_weight: u16) -> Vec<[u8; 32]> {
        if let Some(entry) = self.t1_data.get(cid) {
            return entry.neighbor_cids.clone();
        }
        if let Some(entry) = self.t2_data.get(cid) {
            return entry.neighbor_cids.clone();
        }
        Vec::new()
    }

    // ═════════════════════════════════════════════════════════════════════
    // Private helpers
    // ═════════════════════════════════════════════════════════════════════

    /// Ensure there is room for one more entry. If T1+T2 == capacity,
    /// evict one entry using the ARC replacement policy.
    fn ensure_room_for_insert(&mut self) {
        if self.t1_data.len() + self.t2_data.len() >= self.capacity {
            self.replace();
        }
    }

    /// ARC replacement: decide whether to evict from T1 or T2.
    ///
    /// - If T1 is larger than target `p`, evict from T1
    /// - Otherwise, evict from T2
    /// - Within each list, evict the entry with the lowest `metabolic_rate`
    ///   (metabolism-aware). Ties broken by LRU order (oldest first).
    fn replace(&mut self) {
        let evict_from_t1 = if self.t2_data.is_empty() {
            true
        } else if self.t1_data.is_empty() {
            false
        } else {
            self.t1_data.len() > self.p
        };

        if evict_from_t1 {
            self.evict_from_t1();
        } else {
            self.evict_from_t2();
        }
    }

    /// Evict the lowest-metabolism entry from T1. CID goes to B1 ghost list.
    fn evict_from_t1(&mut self) {
        if self.t1_data.is_empty() {
            return;
        }

        // Find the entry with the lowest metabolic rate in T1.
        // IndexMap iteration is in insertion order = LRU order.
        // Among equal rates, prefer the one earliest (oldest/LRU).
        let victim_cid = Self::find_lowest_metabolism_victim(&self.t1_data);

        if let Some(cid) = victim_cid {
            self.t1_data.shift_remove(&cid);
            // Add to B1 ghost list
            self.b1.push_back(cid);
            self.cap_ghost_list_b1();
            self.evictions += 1;
        }
    }

    /// Evict the lowest-metabolism entry from T2. CID goes to B2 ghost list.
    fn evict_from_t2(&mut self) {
        if self.t2_data.is_empty() {
            return;
        }

        let victim_cid = Self::find_lowest_metabolism_victim(&self.t2_data);

        if let Some(cid) = victim_cid {
            self.t2_data.shift_remove(&cid);
            // Add to B2 ghost list
            self.b2.push_back(cid);
            self.cap_ghost_list_b2();
            self.evictions += 1;
        }
    }

    /// Find the entry with the lowest metabolic rate in the given IndexMap.
    /// IndexMap iteration is in insertion order (oldest first = LRU).
    /// Among equal rates, the one appearing earliest (oldest/LRU) wins.
    fn find_lowest_metabolism_victim(data: &IndexMap<[u8; 32], CachedKu>) -> Option<[u8; 32]> {
        let mut victim: Option<(usize, f64, [u8; 32])> = None;
        for (idx, (cid, entry)) in data.iter().enumerate() {
            let rate = entry.metabolic_rate;
            match &victim {
                None => victim = Some((idx, rate, *cid)),
                Some((_, best_rate, _)) => {
                    if rate < *best_rate || (rate == *best_rate && idx < victim.unwrap().0) {
                        victim = Some((idx, rate, *cid));
                    }
                }
            }
        }
        victim.map(|(_, _, cid)| cid)
    }

    /// Cap B1 ghost list to `capacity` entries.
    fn cap_ghost_list_b1(&mut self) {
        if self.b1.len() > self.capacity {
            let excess = self.b1.len() - self.capacity;
            self.b1.drain(..excess);
        }
    }

    /// Cap B2 ghost list to `capacity` entries.
    fn cap_ghost_list_b2(&mut self) {
        if self.b2.len() > self.capacity {
            let excess = self.b2.len() - self.capacity;
            self.b2.drain(..excess);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a CachedKu with the given metabolic rate.
    fn make_entry(rate: f64) -> CachedKu {
        CachedKu {
            wire_bytes: vec![0x4B, 0x01],
            epigenetics_json: "{}".to_string(),
            neighbor_cids: Vec::new(),
            metabolic_rate: rate,
            inserted_at: 1_720_000_000,
            hit_count: 0,
        }
    }

    /// Helper: create a CachedKu with neighbors.
    fn make_entry_with_neighbors(rate: f64, neighbors: Vec<[u8; 32]>) -> CachedKu {
        CachedKu {
            wire_bytes: vec![0x4B],
            epigenetics_json: "{}".to_string(),
            neighbor_cids: neighbors,
            metabolic_rate: rate,
            inserted_at: 1_720_000_000,
            hit_count: 0,
        }
    }

    /// Helper: make a deterministic CID from a u8 seed.
    fn cid(seed: u8) -> [u8; 32] {
        let mut c = [0u8; 32];
        c[0] = seed;
        c
    }

    // ── 1. Basic lifecycle ──────────────────────────────────────────────

    #[test]
    fn test_new_empty_cache() {
        let cache = ObsCache::new(100);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.capacity, 100);
        assert_eq!(stats.t1_size, 0);
        assert_eq!(stats.t2_size, 0);
    }

    #[test]
    fn test_put_and_get() {
        let mut cache = ObsCache::new(10);
        let c = cid(1);
        cache.put(c, make_entry(0.8));
        let result = cache.get(&c);
        assert!(result.is_some());
        assert!((result.unwrap().metabolic_rate - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_miss_returns_none() {
        let mut cache = ObsCache::new(10);
        assert!(cache.get(&cid(42)).is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_capacity_eviction() {
        let mut cache = ObsCache::new(3);
        cache.put(cid(1), make_entry(0.5));
        cache.put(cid(2), make_entry(0.5));
        cache.put(cid(3), make_entry(0.5));
        // Cache is full (3/3). Inserting 4th should evict one.
        cache.put(cid(4), make_entry(0.5));
        assert_eq!(cache.len(), 3);
        assert!(cache.stats().evictions >= 1);
    }

    // ── 2. ARC promotion ────────────────────────────────────────────────

    #[test]
    fn test_arc_t1_to_t2_promotion() {
        let mut cache = ObsCache::new(10);
        let c = cid(1);
        cache.put(c, make_entry(0.7));
        assert_eq!(cache.stats().t1_size, 1);
        assert_eq!(cache.stats().t2_size, 0);

        // First get promotes T1 → T2
        let _ = cache.get(&c);
        assert_eq!(cache.stats().t1_size, 0);
        assert_eq!(cache.stats().t2_size, 1);
    }

    #[test]
    fn test_arc_ghost_b1_adjusts_p() {
        // Fill cache, let an entry evict from T1 to B1, then re-insert
        let mut cache = ObsCache::new(2);
        cache.put(cid(1), make_entry(0.1)); // T1
        cache.put(cid(2), make_entry(0.5)); // T1
                                            // T1 is full (2). Insert 3rd → evicts lowest metabolism (cid(1)) to B1
        cache.put(cid(3), make_entry(0.5));

        let p_before = cache.p;
        // cid(1) is now in B1. Re-inserting should increase p.
        cache.put(cid(1), make_entry(0.9));
        assert!(
            cache.p >= p_before,
            "p should increase on B1 ghost hit: was {}, now {}",
            p_before,
            cache.p,
        );
    }

    #[test]
    fn test_arc_ghost_b2_adjusts_p() {
        let mut cache = ObsCache::new(2);
        // Put two entries and promote both to T2
        cache.put(cid(1), make_entry(0.1));
        cache.put(cid(2), make_entry(0.5));
        let _ = cache.get(&cid(1)); // T1 → T2
        let _ = cache.get(&cid(2)); // T1 → T2

        // Now T2 has 2 entries. Insert new entry → should evict from T2 → B2
        cache.put(cid(3), make_entry(0.9));
        // cid(1) had lowest metabolism, should be evicted to B2
        assert!(
            cache.b2.contains(&cid(1)),
            "cid(1) should be in B2 ghost list"
        );

        let p_before = cache.p;
        // Re-insert cid(1) → should decrease p
        cache.put(cid(1), make_entry(0.9));
        assert!(
            cache.p <= p_before,
            "p should decrease on B2 ghost hit: was {}, now {}",
            p_before,
            cache.p,
        );
    }

    // ── 3. Invalidation & Clear ─────────────────────────────────────────

    #[test]
    fn test_invalidate() {
        let mut cache = ObsCache::new(10);
        cache.put(cid(1), make_entry(0.5));
        assert!(cache.contains(&cid(1)));
        cache.invalidate(&cid(1));
        assert!(!cache.contains(&cid(1)));
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_invalidate_nonexistent() {
        let mut cache = ObsCache::new(10);
        // Should not panic
        cache.invalidate(&cid(99));
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_clear() {
        let mut cache = ObsCache::new(10);
        for i in 0..5 {
            cache.put(cid(i), make_entry(0.5));
        }
        assert_eq!(cache.len(), 5);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.t1_size, 0);
        assert_eq!(stats.t2_size, 0);
    }

    // ── 4. Contains ─────────────────────────────────────────────────────

    #[test]
    fn test_contains() {
        let mut cache = ObsCache::new(10);
        cache.put(cid(1), make_entry(0.5));
        assert!(cache.contains(&cid(1)));
        assert!(!cache.contains(&cid(2)));
        // Verify contains doesn't promote (entry stays in T1)
        assert_eq!(cache.stats().t1_size, 1);
        assert_eq!(cache.stats().t2_size, 0);
    }

    // ── 5. Stats ────────────────────────────────────────────────────────

    #[test]
    fn test_stats_hit_miss_counting() {
        let mut cache = ObsCache::new(10);
        cache.put(cid(1), make_entry(0.5));
        let _ = cache.get(&cid(1)); // hit
        let _ = cache.get(&cid(2)); // miss
        let _ = cache.get(&cid(3)); // miss
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn test_hit_rate() {
        let stats_zero = CacheStats::default();
        assert!((stats_zero.hit_rate() - 0.0).abs() < f64::EPSILON);

        let stats = CacheStats {
            hits: 3,
            misses: 1,
            ..Default::default()
        };
        assert!((stats.hit_rate() - 0.75).abs() < f64::EPSILON);
    }

    // ── 6. Metabolism-aware eviction ────────────────────────────────────

    #[test]
    fn test_evict_dead() {
        let mut cache = ObsCache::new(10);
        cache.put(cid(1), make_entry(0.0001)); // dead
        cache.put(cid(2), make_entry(0.5)); // alive
        cache.put(cid(3), make_entry(0.0005)); // dead
        let evicted = cache.evict_dead(METABOLISM_DEAD_THRESHOLD);
        assert_eq!(evicted, 2);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains(&cid(2)));
    }

    #[test]
    fn test_metabolism_aware_eviction() {
        // When full, the entry with the lowest metabolic rate should be evicted
        let mut cache = ObsCache::new(3);
        cache.put(cid(1), make_entry(0.1)); // lowest metabolism
        cache.put(cid(2), make_entry(0.9));
        cache.put(cid(3), make_entry(0.5));
        // Insert 4th → evicts cid(1) (lowest metabolism)
        cache.put(cid(4), make_entry(0.7));
        assert!(
            !cache.contains(&cid(1)),
            "lowest metabolism entry should be evicted"
        );
        assert!(cache.contains(&cid(2)));
        assert!(cache.contains(&cid(3)));
        assert!(cache.contains(&cid(4)));
    }

    // ── 7. Prefetch ─────────────────────────────────────────────────────

    #[test]
    fn test_prefetch_candidates() {
        let mut cache = ObsCache::new(10);
        let n1 = cid(10);
        let n2 = cid(20);
        cache.put(cid(1), make_entry_with_neighbors(0.8, vec![n1, n2]));
        let candidates = cache.prefetch_candidates(&cid(1), 0);
        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&n1));
        assert!(candidates.contains(&n2));

        // Miss → empty
        assert!(cache.prefetch_candidates(&cid(99), 0).is_empty());
    }

    // ── 8. Ghost list capping ───────────────────────────────────────────

    #[test]
    fn test_ghost_list_capped() {
        let cap = 3;
        let mut cache = ObsCache::new(cap);
        // Insert and evict many entries to fill B1
        for i in 0u8..20 {
            cache.put(cid(i), make_entry(0.001));
        }
        assert!(
            cache.b1.len() <= cap,
            "B1 should be capped at capacity ({}), got {}",
            cap,
            cache.b1.len(),
        );
        assert!(
            cache.b2.len() <= cap,
            "B2 should be capped at capacity ({}), got {}",
            cap,
            cache.b2.len(),
        );
    }

    // ── 9. Stress / scale ───────────────────────────────────────────────

    #[test]
    fn test_many_puts_and_gets() {
        let mut cache = ObsCache::new(100);
        for i in 0u16..1000 {
            let mut c = [0u8; 32];
            c[0] = (i & 0xFF) as u8;
            c[1] = (i >> 8) as u8;
            cache.put(c, make_entry(i as f64 / 1000.0));
        }
        // Cache should not exceed capacity
        assert!(cache.len() <= 100);
        // Should have lots of evictions
        assert!(cache.stats().evictions >= 900);

        // Verify gets work
        let mut hits = 0u32;
        for i in 0u16..1000 {
            let mut c = [0u8; 32];
            c[0] = (i & 0xFF) as u8;
            c[1] = (i >> 8) as u8;
            if cache.get(&c).is_some() {
                hits += 1;
            }
        }
        assert!(hits > 0, "at least some entries should still be cached");
    }

    // ── 10. LRU ordering ────────────────────────────────────────────────

    #[test]
    fn test_lru_order_within_t1() {
        // Among equal metabolism, oldest in T1 should be evicted first
        let mut cache = ObsCache::new(3);
        cache.put(cid(1), make_entry(0.5)); // oldest
        cache.put(cid(2), make_entry(0.5));
        cache.put(cid(3), make_entry(0.5));
        // All same metabolism → LRU wins → cid(1) evicted
        cache.put(cid(4), make_entry(0.5));
        assert!(
            !cache.contains(&cid(1)),
            "oldest entry in T1 should be evicted first (same metabolism)"
        );
    }

    #[test]
    fn test_lru_order_within_t2() {
        let mut cache = ObsCache::new(3);
        // Fill T2: put + get to promote
        cache.put(cid(1), make_entry(0.5));
        let _ = cache.get(&cid(1)); // → T2
        cache.put(cid(2), make_entry(0.5));
        let _ = cache.get(&cid(2)); // → T2
        cache.put(cid(3), make_entry(0.5));
        let _ = cache.get(&cid(3)); // → T2
                                    // All in T2 now. Insert new → evicts from T2 (T1 is empty, so p comparison falls to T2)
        cache.put(cid(4), make_entry(0.5));
        // cid(1) was oldest in T2 with same metabolism → should be evicted
        assert!(
            !cache.contains(&cid(1)),
            "oldest entry in T2 should be evicted first (same metabolism)"
        );
    }

    // ── 11. Update existing ─────────────────────────────────────────────

    #[test]
    fn test_put_update_existing() {
        let mut cache = ObsCache::new(10);
        cache.put(cid(1), make_entry(0.3));
        cache.put(cid(1), make_entry(0.9)); // update
        let entry = cache.get(&cid(1)).unwrap();
        assert!((entry.metabolic_rate - 0.9).abs() < f64::EPSILON);
        // Should still be 1 entry, not 2
        // Note: get promoted it to T2, so total should be 1
        assert_eq!(cache.len(), 1);
    }

    // ── 12. Default capacity constant ───────────────────────────────────

    #[test]
    fn test_default_capacity() {
        assert_eq!(DEFAULT_CACHE_CAPACITY, 10_000);
        let cache = ObsCache::new(DEFAULT_CACHE_CAPACITY);
        assert_eq!(cache.stats().capacity, 10_000);
    }

    // ── 13. Edge: zero capacity ─────────────────────────────────────────

    #[test]
    fn test_zero_capacity() {
        let mut cache = ObsCache::new(0);
        cache.put(cid(1), make_entry(0.5));
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&cid(1)).is_none());
    }

    // ── 14. Hit count ───────────────────────────────────────────────────

    #[test]
    fn test_hit_count_increments() {
        let mut cache = ObsCache::new(10);
        cache.put(cid(1), make_entry(0.5));
        assert_eq!(cache.get(&cid(1)).unwrap().hit_count, 1); // first get: T1→T2
        assert_eq!(cache.get(&cid(1)).unwrap().hit_count, 2); // second get: MRU in T2
        assert_eq!(cache.get(&cid(1)).unwrap().hit_count, 3);
    }

    // ── 15. CachedKu field access ───────────────────────────────────────

    #[test]
    fn test_cache_entry_fields() {
        let entry = CachedKu {
            wire_bytes: vec![0x4B, 0x01, 0x02],
            epigenetics_json: r#"{"trust":0.9}"#.to_string(),
            neighbor_cids: vec![cid(10)],
            metabolic_rate: 0.75,
            inserted_at: 1_720_000_000,
            hit_count: 42,
        };
        assert_eq!(entry.wire_bytes.len(), 3);
        assert!(entry.epigenetics_json.contains("trust"));
        assert_eq!(entry.neighbor_cids.len(), 1);
        assert!((entry.metabolic_rate - 0.75).abs() < f64::EPSILON);
        assert_eq!(entry.inserted_at, 1_720_000_000);
        assert_eq!(entry.hit_count, 42);
    }
}
