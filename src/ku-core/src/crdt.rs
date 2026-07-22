//! # Conflict-Free Replicated Data Types (CRDTs)
//!
//! Core CRDT implementations for OBP eventual consistency:
//! - **GCounter**: Grow-only counter (corroboration/challenge counts)
//! - **PNCounter**: Positive-Negative counter (trust_score)
//! - **LWWRegister**: Last-Writer-Wins register (epistemic_status)
//! - **ORSet**: Observed-Remove set (domain_codes, verifications)

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// ═══════════════════════════════════════════════════════════════════════════
// G-Counter (Grow-only Counter)
// ═══════════════════════════════════════════════════════════════════════════

/// Grow-only counter — each node tracks its own increments.
///
/// Merge = per-node max. Value = sum across all nodes.
/// Used for `corroboration_count`, `challenge_count`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GCounter {
    /// node_id (truncated to u64) → local count
    counts: BTreeMap<u64, u64>,
}

impl GCounter {
    pub fn new() -> Self {
        Self {
            counts: BTreeMap::new(),
        }
    }

    /// Increment by 1 for the given node. Returns Err on overflow.
    pub fn increment(&mut self, node_id: u64) -> Result<(), &'static str> {
        let entry = self.counts.entry(node_id).or_insert(0);
        *entry = entry.checked_add(1).ok_or("GCounter overflow")?;
        Ok(())
    }

    /// Increment by a specific amount. Returns Err on overflow.
    pub fn increment_by(&mut self, node_id: u64, amount: u64) -> Result<(), &'static str> {
        let entry = self.counts.entry(node_id).or_insert(0);
        *entry = entry.checked_add(amount).ok_or("GCounter overflow")?;
        Ok(())
    }

    /// Total value across all nodes. Uses saturating arithmetic to prevent overflow.
    pub fn value(&self) -> u64 {
        self.counts
            .values()
            .copied()
            .fold(0u64, |acc, v| acc.saturating_add(v))
    }

    /// Merge with another G-Counter (per-node max).
    pub fn merge(&mut self, other: &GCounter) {
        for (&node_id, &count) in &other.counts {
            let entry = self.counts.entry(node_id).or_insert(0);
            *entry = (*entry).max(count);
        }
    }

    /// Get the count for a specific node.
    pub fn node_count(&self, node_id: u64) -> u64 {
        self.counts.get(&node_id).copied().unwrap_or(0)
    }

    /// Number of contributing nodes.
    pub fn num_nodes(&self) -> usize {
        self.counts.len()
    }
}

impl Default for GCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PN-Counter (Positive-Negative Counter)
// ═══════════════════════════════════════════════════════════════════════════

/// Positive-Negative counter — two G-Counters combined.
///
/// Value = positive.value() - negative.value().
/// Used for `trust_score` derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PNCounter {
    pub positive: GCounter,
    pub negative: GCounter,
}

impl PNCounter {
    pub fn new() -> Self {
        Self {
            positive: GCounter::new(),
            negative: GCounter::new(),
        }
    }

    /// Increment positive count (e.g., corroboration).
    pub fn increment(&mut self, node_id: u64) -> Result<(), &'static str> {
        self.positive.increment(node_id)
    }

    /// Decrement (increment negative, e.g., challenge).
    pub fn decrement(&mut self, node_id: u64) -> Result<(), &'static str> {
        self.negative.increment(node_id)
    }

    /// Net value (positive - negative). Can be negative.
    pub fn value(&self) -> i64 {
        self.positive.value() as i64 - self.negative.value() as i64
    }

    /// Merge with another PN-Counter.
    pub fn merge(&mut self, other: &PNCounter) {
        self.positive.merge(&other.positive);
        self.negative.merge(&other.negative);
    }
}

impl Default for PNCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LWW-Register (Last-Writer-Wins)
// ═══════════════════════════════════════════════════════════════════════════

/// Last-Writer-Wins register — highest timestamp wins.
///
/// Tiebreak: higher node_id wins (deterministic).
/// Used for `epistemic_status`, `verification_level`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LWWRegister<T: Clone + Eq> {
    value: T,
    timestamp: u64,
    node_id: u64,
}

impl<T: Clone + Eq> LWWRegister<T> {
    /// Create a new register with initial value.
    pub fn new(value: T, timestamp: u64, node_id: u64) -> Self {
        Self {
            value,
            timestamp,
            node_id,
        }
    }

    /// Get the current value.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get the timestamp.
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Update the value (only if timestamp is newer).
    pub fn set(&mut self, value: T, timestamp: u64, node_id: u64) -> bool {
        if timestamp > self.timestamp || (timestamp == self.timestamp && node_id > self.node_id) {
            self.value = value;
            self.timestamp = timestamp;
            self.node_id = node_id;
            true
        } else {
            false
        }
    }

    /// Merge with another register (highest timestamp wins).
    pub fn merge(&mut self, other: &LWWRegister<T>) {
        self.set(other.value.clone(), other.timestamp, other.node_id);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// OR-Set (Observed-Remove Set)
// ═══════════════════════════════════════════════════════════════════════════

/// Observed-Remove set — concurrent add/remove without anomalies.
///
/// Each add creates a unique tag. Remove deletes all known tags for a value.
/// Concurrent add + remove: the add wins (add-wins semantics).
///
/// Used for `domain_codes`, `verifications`, `challenges`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ORSet<T: Clone + Eq + Ord> {
    /// Active elements: value → set of unique tags
    elements: BTreeMap<T, BTreeSet<u64>>,
    /// Tombstones: removed tags
    tombstones: BTreeSet<u64>,
    /// Next unique tag (node_id-local counter)
    next_tag: u64,
    /// Our node identifier for tag generation
    node_id: u64,
}

impl<T: Clone + Eq + Ord> ORSet<T> {
    /// Create a new OR-Set for the given node.
    pub fn new(node_id: u64) -> Self {
        Self {
            elements: BTreeMap::new(),
            tombstones: BTreeSet::new(),
            next_tag: node_id << 32, // Namespace tags by node
            node_id,
        }
    }

    /// Add an element. Returns the assigned tag.
    pub fn add(&mut self, value: T) -> u64 {
        let tag = self.next_tag;
        self.next_tag += 1;
        self.elements.entry(value).or_default().insert(tag);
        tag
    }

    /// Remove all instances of a value.
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.elements.remove(value) {
            self.tombstones.extend(tags);
        }
    }

    /// Check if a value is in the set.
    pub fn contains(&self, value: &T) -> bool {
        self.elements
            .get(value)
            .map(|tags| !tags.is_empty())
            .unwrap_or(false)
    }

    /// Get all current values.
    pub fn values(&self) -> Vec<&T> {
        self.elements.keys().collect()
    }

    /// Number of distinct values.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Is the set empty?
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Merge with another OR-Set (add-wins semantics).
    pub fn merge(&mut self, other: &ORSet<T>) {
        // Add all elements from other that aren't tombstoned locally
        for (value, tags) in &other.elements {
            let entry = self.elements.entry(value.clone()).or_default();
            for &tag in tags {
                if !self.tombstones.contains(&tag) {
                    entry.insert(tag);
                }
            }
        }

        // Apply other's tombstones to our elements
        for &tag in &other.tombstones {
            self.tombstones.insert(tag);
            // Remove tombstoned tags from active elements
            for tags in self.elements.values_mut() {
                tags.remove(&tag);
            }
        }

        // Clean up empty entries
        self.elements.retain(|_, tags| !tags.is_empty());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// VectorClock (for causal ordering)
// ═══════════════════════════════════════════════════════════════════════════

/// Vector clock for causal ordering of events.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VectorClock {
    /// node_id → logical timestamp
    clocks: BTreeMap<u64, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            clocks: BTreeMap::new(),
        }
    }

    /// Increment our clock.
    pub fn tick(&mut self, node_id: u64) {
        let entry = self.clocks.entry(node_id).or_insert(0);
        *entry += 1;
    }

    /// Get clock value for a node.
    pub fn get(&self, node_id: u64) -> u64 {
        self.clocks.get(&node_id).copied().unwrap_or(0)
    }

    /// Merge with another vector clock (per-node max).
    pub fn merge(&mut self, other: &VectorClock) {
        for (&node_id, &ts) in &other.clocks {
            let entry = self.clocks.entry(node_id).or_insert(0);
            *entry = (*entry).max(ts);
        }
    }

    /// Check if this clock dominates (happens-after) another.
    pub fn dominates(&self, other: &VectorClock) -> bool {
        // Self dominates if every component >= other, and at least one >
        let mut dominated = false;
        for (&node_id, &ts) in &other.clocks {
            let our_ts = self.get(node_id);
            if our_ts < ts {
                return false;
            }
            if our_ts > ts {
                dominated = true;
            }
        }
        // Check our keys not in other
        if !dominated {
            for (&node_id, &ts) in &self.clocks {
                if ts > other.get(node_id) {
                    dominated = true;
                    break;
                }
            }
        }
        dominated
    }

    /// Check if two clocks are concurrent (neither dominates).
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.dominates(other) && !other.dominates(self) && self != other
    }

    /// Check if this clock covers another (all components ≥, including equal).
    /// This is "dominates OR equal" — used by sync to detect already-synced state.
    pub fn covers(&self, other: &VectorClock) -> bool {
        for (&node_id, &ts) in &other.clocks {
            if self.get(node_id) < ts {
                return false;
            }
        }
        true
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ─── GCounter ──────────────────────────────────────────────────────

    #[test]
    fn test_gcounter_increment() {
        let mut c = GCounter::new();
        c.increment(1).unwrap();
        c.increment(1).unwrap();
        c.increment(2).unwrap();
        assert_eq!(c.value(), 3);
        assert_eq!(c.node_count(1), 2);
        assert_eq!(c.node_count(2), 1);
    }

    #[test]
    fn test_gcounter_merge() {
        let mut a = GCounter::new();
        a.increment_by(1, 5).unwrap();
        a.increment_by(2, 3).unwrap();

        let mut b = GCounter::new();
        b.increment_by(1, 3).unwrap(); // lower than a
        b.increment_by(2, 7).unwrap(); // higher than a
        b.increment_by(3, 2).unwrap(); // new node

        a.merge(&b);
        assert_eq!(a.node_count(1), 5); // max(5, 3)
        assert_eq!(a.node_count(2), 7); // max(3, 7)
        assert_eq!(a.node_count(3), 2); // new
        assert_eq!(a.value(), 14);
    }

    #[test]
    fn test_gcounter_merge_idempotent() {
        let mut a = GCounter::new();
        a.increment_by(1, 10).unwrap();

        let b = a.clone();
        a.merge(&b);
        assert_eq!(a.value(), 10, "Merge with self should be idempotent");
    }

    #[test]
    fn test_gcounter_overflow_protection() {
        let mut gc = GCounter::new();
        gc.increment_by(1, u64::MAX).unwrap();
        assert!(
            gc.increment(1).is_err(),
            "increment should fail on overflow"
        );
        assert!(
            gc.increment_by(1, 1).is_err(),
            "increment_by should fail on overflow"
        );
    }

    // ─── PNCounter ─────────────────────────────────────────────────────

    #[test]
    fn test_pncounter_value() {
        let mut pn = PNCounter::new();
        pn.increment(1).unwrap(); // +1
        pn.increment(1).unwrap(); // +2
        pn.decrement(2).unwrap(); // -1
        assert_eq!(pn.value(), 1); // 2 - 1
    }

    #[test]
    fn test_pncounter_negative() {
        let mut pn = PNCounter::new();
        pn.decrement(1).unwrap();
        pn.decrement(1).unwrap();
        pn.decrement(1).unwrap();
        pn.increment(2).unwrap();
        assert_eq!(pn.value(), -2); // 1 - 3
    }

    #[test]
    fn test_pncounter_merge() {
        let mut a = PNCounter::new();
        a.increment(1).unwrap();
        a.increment(1).unwrap();

        let mut b = PNCounter::new();
        b.decrement(2).unwrap();

        a.merge(&b);
        assert_eq!(a.value(), 1); // 2 - 1
    }

    // ─── LWWRegister ───────────────────────────────────────────────────

    #[test]
    fn test_lww_set_newer() {
        let mut reg = LWWRegister::new("old", 1, 1);
        assert!(reg.set("new", 2, 1));
        assert_eq!(reg.value(), &"new");
    }

    #[test]
    fn test_lww_reject_older() {
        let mut reg = LWWRegister::new("current", 5, 1);
        assert!(!reg.set("old", 3, 1));
        assert_eq!(reg.value(), &"current");
    }

    #[test]
    fn test_lww_tiebreak_by_node_id() {
        let mut reg = LWWRegister::new("a", 10, 1);
        // Same timestamp, higher node_id wins
        assert!(reg.set("b", 10, 2));
        assert_eq!(reg.value(), &"b");
        // Same timestamp, lower node_id loses
        assert!(!reg.set("c", 10, 1));
        assert_eq!(reg.value(), &"b");
    }

    #[test]
    fn test_lww_merge() {
        let mut a = LWWRegister::new("old", 1, 1);
        let b = LWWRegister::new("new", 5, 2);
        a.merge(&b);
        assert_eq!(a.value(), &"new");
        assert_eq!(a.timestamp(), 5);
    }

    // ─── ORSet ─────────────────────────────────────────────────────────

    #[test]
    fn test_orset_add_contains() {
        let mut s = ORSet::new(1);
        s.add(42u32);
        assert!(s.contains(&42));
        assert!(!s.contains(&99));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn test_orset_remove() {
        let mut s = ORSet::new(1);
        s.add(10u32);
        s.add(20);
        s.remove(&10);
        assert!(!s.contains(&10));
        assert!(s.contains(&20));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn test_orset_add_wins_over_concurrent_remove() {
        // Node 1: adds element
        let mut s1 = ORSet::new(1);
        s1.add(42u32);

        // Node 2: independently adds same element
        let mut s2 = ORSet::new(2);
        s2.add(42u32);

        // Node 1 removes it
        s1.remove(&42);
        assert!(!s1.contains(&42));

        // Merge: s2's add should win (add-wins)
        s1.merge(&s2);
        assert!(s1.contains(&42), "Add should win over concurrent remove");
    }

    #[test]
    fn test_orset_merge_union() {
        let mut s1 = ORSet::new(1);
        s1.add(10u32);
        s1.add(20);

        let mut s2 = ORSet::new(2);
        s2.add(20u32);
        s2.add(30);

        s1.merge(&s2);
        assert!(s1.contains(&10));
        assert!(s1.contains(&20));
        assert!(s1.contains(&30));
    }

    // ─── VectorClock ───────────────────────────────────────────────────

    #[test]
    fn test_vclock_tick_and_get() {
        let mut vc = VectorClock::new();
        vc.tick(1);
        vc.tick(1);
        vc.tick(2);
        assert_eq!(vc.get(1), 2);
        assert_eq!(vc.get(2), 1);
        assert_eq!(vc.get(3), 0); // unknown node
    }

    #[test]
    fn test_vclock_dominates() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(1);

        let mut b = VectorClock::new();
        b.tick(1);

        assert!(a.dominates(&b));
        assert!(!b.dominates(&a));
    }

    #[test]
    fn test_vclock_concurrent() {
        let mut a = VectorClock::new();
        a.tick(1);

        let mut b = VectorClock::new();
        b.tick(2);

        assert!(a.is_concurrent(&b));
        assert!(b.is_concurrent(&a));
    }

    #[test]
    fn test_vclock_merge() {
        let mut a = VectorClock::new();
        a.tick(1);
        a.tick(1);

        let mut b = VectorClock::new();
        b.tick(2);
        b.tick(2);
        b.tick(2);

        a.merge(&b);
        assert_eq!(a.get(1), 2);
        assert_eq!(a.get(2), 3);
    }
}
