//! # Query Cache — Phase F1
//!
//! LRU cache for recently executed queries.
//! Avoids redundant network queries by caching results
//! with configurable TTL and maximum entries.
//!
//! ## Cache Key
//! The cache key is a BLAKE3 hash of the normalized KQL string.
//! This ensures that semantically identical queries hit the cache
//! regardless of whitespace differences.

use std::collections::HashMap;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Cache key: BLAKE3 hash of normalized KQL.
type CacheKey = [u8; 32];

/// A cached query result.
#[derive(Debug, Clone)]
pub struct CachedResult {
    /// The original KQL query.
    pub kql: String,
    /// Serialized results (Core DNA wire bytes).
    pub results_payload: Vec<u8>,
    /// Number of results.
    pub result_count: usize,
    /// When this entry was cached.
    pub cached_at: Instant,
    /// Time-to-live for this entry.
    pub ttl: Duration,
    /// Number of times this cache entry was hit.
    pub hit_count: u64,
}

impl CachedResult {
    /// Whether this cache entry has expired.
    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Query Cache
// ═══════════════════════════════════════════════════════════════════════════

/// LRU query cache with TTL-based expiration.
pub struct QueryCache {
    /// Entries keyed by cache key.
    entries: HashMap<CacheKey, CachedResult>,
    /// Access order for LRU eviction (most recent at end).
    access_order: Vec<CacheKey>,
    /// Maximum cache entries.
    capacity: usize,
    /// Default TTL for new entries.
    default_ttl: Duration,
    /// Total hits across all entries.
    total_hits: u64,
    /// Total misses.
    total_misses: u64,
}

impl QueryCache {
    /// Create a new cache with given capacity and TTL.
    pub fn new(capacity: usize, default_ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            access_order: Vec::with_capacity(capacity),
            capacity,
            default_ttl,
            total_hits: 0,
            total_misses: 0,
        }
    }

    /// Create with default parameters (1000 entries, 5 min TTL).
    pub fn with_defaults() -> Self {
        Self::new(1000, Duration::from_secs(300))
    }

    /// Compute cache key from a KQL string.
    fn cache_key(kql: &str) -> CacheKey {
        // Normalize: lowercase + collapse whitespace
        let normalized: String = kql
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join(" ");
        *blake3::hash(normalized.as_bytes()).as_bytes()
    }

    /// Look up a query in the cache.
    pub fn get(&mut self, kql: &str) -> Option<&CachedResult> {
        let key = Self::cache_key(kql);

        // Check if expired
        if let Some(entry) = self.entries.get(&key) {
            if entry.is_expired() {
                self.entries.remove(&key);
                self.access_order.retain(|k| k != &key);
                self.total_misses += 1;
                return None;
            }
        }

        if self.entries.contains_key(&key) {
            // Update access order (move to end = most recent)
            self.access_order.retain(|k| k != &key);
            self.access_order.push(key);

            // Increment hit count
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.hit_count += 1;
            }
            self.total_hits += 1;
            self.entries.get(&key)
        } else {
            self.total_misses += 1;
            None
        }
    }

    /// Insert a query result into the cache.
    pub fn put(&mut self, kql: String, results_payload: Vec<u8>, result_count: usize) {
        let key = Self::cache_key(&kql);

        // Evict LRU if at capacity
        while self.entries.len() >= self.capacity {
            if let Some(lru_key) = self.access_order.first().copied() {
                self.entries.remove(&lru_key);
                self.access_order.remove(0);
            } else {
                break;
            }
        }

        // Remove old entry if exists
        self.access_order.retain(|k| k != &key);

        self.entries.insert(
            key,
            CachedResult {
                kql,
                results_payload,
                result_count,
                cached_at: Instant::now(),
                ttl: self.default_ttl,
                hit_count: 0,
            },
        );
        self.access_order.push(key);
    }

    /// Insert with custom TTL.
    pub fn put_with_ttl(
        &mut self,
        kql: String,
        results_payload: Vec<u8>,
        result_count: usize,
        ttl: Duration,
    ) {
        let key = Self::cache_key(&kql);
        while self.entries.len() >= self.capacity {
            if let Some(lru_key) = self.access_order.first().copied() {
                self.entries.remove(&lru_key);
                self.access_order.remove(0);
            } else {
                break;
            }
        }
        self.access_order.retain(|k| k != &key);
        self.entries.insert(
            key,
            CachedResult {
                kql,
                results_payload,
                result_count,
                cached_at: Instant::now(),
                ttl,
                hit_count: 0,
            },
        );
        self.access_order.push(key);
    }

    /// Invalidate a specific cached query.
    pub fn invalidate(&mut self, kql: &str) -> bool {
        let key = Self::cache_key(kql);
        self.access_order.retain(|k| k != &key);
        self.entries.remove(&key).is_some()
    }

    /// Remove all expired entries.
    pub fn cleanup_expired(&mut self) -> usize {
        let expired_keys: Vec<CacheKey> = self
            .entries
            .iter()
            .filter(|(_, v)| v.is_expired())
            .map(|(&k, _)| k)
            .collect();
        let count = expired_keys.len();
        for key in &expired_keys {
            self.entries.remove(key);
            self.access_order.retain(|k| k != key);
        }
        count
    }

    /// Clear the entire cache.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.access_order.clear();
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the cache empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache hit rate as a percentage [0.0, 100.0].
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_hits + self.total_misses;
        if total == 0 {
            0.0
        } else {
            (self.total_hits as f64 / total as f64) * 100.0
        }
    }

    /// Total hits.
    pub fn total_hits(&self) -> u64 {
        self.total_hits
    }
    /// Total misses.
    pub fn total_misses(&self) -> u64 {
        self.total_misses
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let mut cache = QueryCache::with_defaults();
        cache.put("FIND (k:KU)".to_string(), vec![1, 2, 3], 3);

        let result = cache.get("FIND (k:KU)");
        assert!(result.is_some());
        assert_eq!(result.unwrap().result_count, 3);
    }

    #[test]
    fn test_normalized_lookup() {
        let mut cache = QueryCache::with_defaults();
        cache.put("FIND  (k:KU)  WHERE  k.trust > 5".to_string(), vec![1], 1);

        // Should match despite different whitespace
        let result = cache.get("find (k:ku) where k.trust > 5");
        assert!(result.is_some());
    }

    #[test]
    fn test_miss() {
        let mut cache = QueryCache::with_defaults();
        assert!(cache.get("FIND (k:KU)").is_none());
        assert_eq!(cache.total_misses(), 1);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = QueryCache::new(2, Duration::from_secs(300));

        cache.put("query1".to_string(), vec![], 0);
        cache.put("query2".to_string(), vec![], 0);
        cache.put("query3".to_string(), vec![], 0); // Should evict query1

        assert_eq!(cache.len(), 2);
        assert!(cache.get("query1").is_none()); // Evicted
        assert!(cache.get("query2").is_some());
        assert!(cache.get("query3").is_some());
    }

    #[test]
    fn test_ttl_expiration() {
        let mut cache = QueryCache::new(100, Duration::from_millis(1));
        cache.put("test".to_string(), vec![], 0);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(10));

        assert!(
            cache.get("test").is_none(),
            "Expired entry should return None"
        );
    }

    #[test]
    fn test_invalidate() {
        let mut cache = QueryCache::with_defaults();
        cache.put("test".to_string(), vec![], 0);
        assert!(cache.invalidate("test"));
        assert!(cache.get("test").is_none());
        assert!(!cache.invalidate("test")); // Already removed
    }

    #[test]
    fn test_hit_rate() {
        let mut cache = QueryCache::with_defaults();
        cache.put("q1".to_string(), vec![], 0);

        cache.get("q1"); // Hit
        cache.get("q1"); // Hit
        cache.get("q2"); // Miss

        assert_eq!(cache.total_hits(), 2);
        assert_eq!(cache.total_misses(), 1);
        let rate = cache.hit_rate();
        assert!(
            (rate - 66.67).abs() < 1.0,
            "Hit rate should be ~66.7%, got {}",
            rate
        );
    }

    #[test]
    fn test_cleanup_expired() {
        let mut cache = QueryCache::new(100, Duration::from_millis(1));
        cache.put("a".to_string(), vec![], 0);
        cache.put("b".to_string(), vec![], 0);

        std::thread::sleep(Duration::from_millis(10));

        // Add a non-expired entry
        cache.put_with_ttl("c".to_string(), vec![], 0, Duration::from_secs(300));

        let cleaned = cache.cleanup_expired();
        assert_eq!(cleaned, 2);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut cache = QueryCache::with_defaults();
        cache.put("a".to_string(), vec![], 0);
        cache.put("b".to_string(), vec![], 0);
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_hit_count_increments() {
        let mut cache = QueryCache::with_defaults();
        cache.put("q".to_string(), vec![], 0);
        cache.get("q");
        cache.get("q");
        cache.get("q");
        let result = cache.get("q").unwrap();
        assert_eq!(result.hit_count, 4); // 3 previous + this get
    }
}
