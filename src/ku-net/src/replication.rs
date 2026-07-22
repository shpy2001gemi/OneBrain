//! Replication Manager — R=7 tier-aware KU replication strategy.
//!
//! Manages replication of Knowledge Units across the network with a
//! placement strategy that balances XOR-closeness, tier-aware anchoring,
//! and subnet diversity.
//!
//! ## Placement Strategy (4 + 2 + 1 = 7)
//! 1. **4 XOR-closest** nodes (standard Kademlia proximity)
//! 2. **2 tier-anchored** nodes (T2+ and T3+ for durability)
//! 3. **1 diversity** node (different subnet for geo-distribution)

use std::collections::HashMap;

use crate::constants::{MIN_HEALTHY_REPLICAS, STORAGE_REPLICATION_FACTOR};

// ─── XOR Distance Helpers ───────────────────────────────────────────────────

/// Compute XOR distance between two 256-bit keys.
fn xor_distance(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut result = [0u8; 32];
    for i in 0..32 {
        result[i] = a[i] ^ b[i];
    }
    result
}

/// Convert a u64 node ID to a 256-bit key via BLAKE3 hash.
///
/// Uses BLAKE3 to produce a uniformly distributed 256-bit key,
/// ensuring good XOR distance distribution across all 32 bytes.
fn node_id_to_key(node_id: u64) -> [u8; 32] {
    *blake3::hash(&node_id.to_be_bytes()).as_bytes()
}

// ─── Types ──────────────────────────────────────────────────────────────────

/// Replication health status for a CID.
#[derive(Debug, Clone, PartialEq)]
pub enum ReplicationStatus {
    /// Healthy: actual replicas >= STORAGE_REPLICATION_FACTOR.
    Healthy,
    /// Degraded: actual >= MIN_HEALTHY but < target. Contains actual count.
    Degraded(usize),
    /// Critical: actual < MIN_HEALTHY. Contains actual count.
    Critical(usize),
    /// Unknown: no replication data available.
    Unknown,
}

/// A pending STORE operation awaiting ACKs from target nodes.
#[derive(Debug, Clone)]
pub struct PendingStore {
    /// Content ID being replicated.
    pub cid: [u8; 32],
    /// Nodes selected as replication targets.
    pub target_nodes: Vec<u64>,
    /// Nodes that have ACKed the STORE.
    pub acked_nodes: Vec<u64>,
    /// Epoch timestamp when the STORE was initiated.
    pub initiated_at: u64,
}

/// Target selection result from tier-aware placement.
#[derive(Debug, Clone)]
pub struct ReplicationTargets {
    /// 4 XOR-closest nodes (standard Kademlia proximity).
    pub xor_closest: Vec<u64>,
    /// 2 tier-anchored nodes (T2+, T3+ for durability).
    pub tier_anchored: Vec<u64>,
    /// 1 diversity node (different subnet / random selection).
    pub diversity: Vec<u64>,
}

impl ReplicationTargets {
    /// Total number of selected target nodes.
    pub fn total_count(&self) -> usize {
        self.xor_closest.len() + self.tier_anchored.len() + self.diversity.len()
    }

    /// All target node IDs as a flat vector.
    pub fn all_targets(&self) -> Vec<u64> {
        let mut all = Vec::with_capacity(self.total_count());
        all.extend_from_slice(&self.xor_closest);
        all.extend_from_slice(&self.tier_anchored);
        all.extend_from_slice(&self.diversity);
        all
    }
}

// ─── ReplicationManager ─────────────────────────────────────────────────────

/// Manages KU replication across the network.
///
/// Implements R=7 tier-aware placement strategy:
/// - 4 XOR-closest for standard Kademlia semantics
/// - 2 tier-anchored (T2+, T3+) for infrastructure durability
/// - 1 diversity for geo-distribution / partition resilience
pub struct ReplicationManager {
    /// Local node ID.
    #[allow(dead_code)]
    node_id: u64,
    /// Replication factor (default: STORAGE_REPLICATION_FACTOR = 7).
    replication_factor: usize,
    /// Minimum healthy replicas before triggering repair.
    min_healthy: usize,
    /// Pending STORE operations awaiting ACKs, keyed by CID.
    pending_stores: HashMap<[u8; 32], PendingStore>,
}

impl ReplicationManager {
    /// Create a new ReplicationManager for the given local node.
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            replication_factor: STORAGE_REPLICATION_FACTOR,
            min_healthy: MIN_HEALTHY_REPLICAS,
            pending_stores: HashMap::new(),
        }
    }

    /// Select R=7 target nodes for storing a KU.
    ///
    /// Uses tier-aware placement: 4 XOR + 2 tier + 1 diversity.
    ///
    /// # Arguments
    /// - `cid`: Content ID to replicate
    /// - `candidates`: Slice of `(node_id, tier)` pairs representing available nodes
    ///
    /// # Algorithm
    /// 1. Sort all candidates by XOR distance to CID
    /// 2. Take top 4 as `xor_closest`
    /// 3. From remaining, find first node with tier >= 2 → `tier_anchored[0]`
    /// 4. From remaining, find first node with tier >= 3 → `tier_anchored[1]`
    /// 5. From remaining, pick first available → `diversity[0]`
    /// 6. If any category is empty, fill from XOR overflow
    pub fn select_targets(&self, cid: &[u8; 32], candidates: &[(u64, u8)]) -> ReplicationTargets {
        if candidates.is_empty() {
            return ReplicationTargets {
                xor_closest: Vec::new(),
                tier_anchored: Vec::new(),
                diversity: Vec::new(),
            };
        }

        // Sort candidates by XOR distance to CID
        let mut sorted: Vec<(u64, u8)> = candidates.to_vec();
        sorted.sort_by(|a, b| {
            let dist_a = xor_distance(cid, &node_id_to_key(a.0));
            let dist_b = xor_distance(cid, &node_id_to_key(b.0));
            dist_a.cmp(&dist_b)
        });

        // Deduplicate by node_id
        sorted.dedup_by_key(|c| c.0);

        // Track which nodes are already selected
        let mut selected = std::collections::HashSet::new();

        // 1. Take top 4 XOR-closest
        let mut xor_closest = Vec::new();
        for &(node_id, _tier) in &sorted {
            if xor_closest.len() >= 4 {
                break;
            }
            if selected.insert(node_id) {
                xor_closest.push(node_id);
            }
        }

        // 2. From remaining, find tier >= 2 node (closest by XOR)
        let mut tier_anchored = Vec::new();
        for &(node_id, tier) in &sorted {
            if tier_anchored.len() >= 1 {
                break;
            }
            if tier >= 2 && !selected.contains(&node_id) {
                selected.insert(node_id);
                tier_anchored.push(node_id);
            }
        }

        // 3. From remaining, find tier >= 3 node (closest by XOR)
        for &(node_id, tier) in &sorted {
            if tier_anchored.len() >= 2 {
                break;
            }
            if tier >= 3 && !selected.contains(&node_id) {
                selected.insert(node_id);
                tier_anchored.push(node_id);
            }
        }

        // 4. From remaining, pick first unselected as diversity node
        let mut diversity = Vec::new();
        for &(node_id, _tier) in &sorted {
            if diversity.len() >= 1 {
                break;
            }
            if !selected.contains(&node_id) {
                selected.insert(node_id);
                diversity.push(node_id);
            }
        }

        // 5. Fill shortfalls from XOR overflow
        let total = xor_closest.len() + tier_anchored.len() + diversity.len();
        if total < self.replication_factor {
            // Fill tier_anchored shortfall
            while tier_anchored.len() < 2 && xor_closest.len() > 4 {
                // Won't happen in practice since xor_closest caps at 4
                break;
            }
            // Fill from any remaining sorted candidates
            for &(node_id, _tier) in &sorted {
                if selected.len() >= self.replication_factor {
                    break;
                }
                if !selected.contains(&node_id) {
                    selected.insert(node_id);
                    // Add to whichever bucket is short
                    if tier_anchored.len() < 2 {
                        tier_anchored.push(node_id);
                    } else if diversity.len() < 1 {
                        diversity.push(node_id);
                    } else {
                        xor_closest.push(node_id);
                    }
                }
            }
        }

        ReplicationTargets {
            xor_closest,
            tier_anchored,
            diversity,
        }
    }

    /// Initiate a STORE operation. Records pending state for ACK tracking.
    ///
    /// Returns the `PendingStore` that was created.
    pub fn initiate_store(&mut self, cid: [u8; 32], targets: &ReplicationTargets) -> PendingStore {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let pending = PendingStore {
            cid,
            target_nodes: targets.all_targets(),
            acked_nodes: Vec::new(),
            initiated_at: now,
        };

        self.pending_stores.insert(cid, pending.clone());
        pending
    }

    /// Handle a STORE_ACK from a target node.
    ///
    /// Returns the updated `ReplicationStatus` if the CID is being tracked,
    /// or `None` if the CID is unknown.
    pub fn handle_ack(&mut self, cid: &[u8; 32], node_id: u64) -> Option<ReplicationStatus> {
        let pending = self.pending_stores.get_mut(cid)?;

        if !pending.acked_nodes.contains(&node_id) {
            pending.acked_nodes.push(node_id);
        }

        let acked_count = pending.acked_nodes.len();
        Some(self.check_health(cid, acked_count))
    }

    /// Check replication health for a CID based on actual replica count.
    pub fn check_health(&self, _cid: &[u8; 32], actual_replicas: usize) -> ReplicationStatus {
        if actual_replicas >= self.replication_factor {
            ReplicationStatus::Healthy
        } else if actual_replicas >= self.min_healthy {
            ReplicationStatus::Degraded(actual_replicas)
        } else {
            ReplicationStatus::Critical(actual_replicas)
        }
    }

    /// Get CIDs of pending stores that have timed out (no full ACK within timeout).
    pub fn timed_out_stores(&self, now: u64, timeout_secs: u64) -> Vec<[u8; 32]> {
        self.pending_stores
            .iter()
            .filter(|(_, pending)| {
                // Not fully ACKed and past timeout
                pending.acked_nodes.len() < pending.target_nodes.len()
                    && now.saturating_sub(pending.initiated_at) >= timeout_secs
            })
            .map(|(cid, _)| *cid)
            .collect()
    }

    /// Clean up completed pending stores (all targets ACKed).
    pub fn cleanup_completed(&mut self) {
        self.pending_stores
            .retain(|_, pending| pending.acked_nodes.len() < pending.target_nodes.len());
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidates(count: usize) -> Vec<(u64, u8)> {
        (1..=count as u64)
            .map(|id| {
                let tier = match id {
                    1..=3 => 0, // Leaf nodes
                    4..=6 => 1, // Contributors
                    7..=8 => 2, // Local SP
                    9 => 3,     // District SP
                    10 => 4,    // Country SP
                    _ => 0,
                };
                (id, tier)
            })
            .collect()
    }

    fn make_cid(byte: u8) -> [u8; 32] {
        let mut cid = [0u8; 32];
        cid[0] = byte;
        cid
    }

    #[test]
    fn test_new_replication_manager() {
        let rm = ReplicationManager::new(42);
        assert_eq!(rm.node_id, 42);
        assert_eq!(rm.replication_factor, STORAGE_REPLICATION_FACTOR);
        assert_eq!(rm.min_healthy, MIN_HEALTHY_REPLICAS);
        assert!(rm.pending_stores.is_empty());
    }

    #[test]
    fn test_select_targets_basic() {
        let rm = ReplicationManager::new(1);
        let candidates = make_candidates(10);
        let cid = make_cid(0x42);

        let targets = rm.select_targets(&cid, &candidates);

        // Should have up to 7 targets total
        assert!(targets.total_count() <= 7);
        assert!(targets.total_count() > 0);
    }

    #[test]
    fn test_select_targets_tier_aware() {
        let rm = ReplicationManager::new(1);
        let candidates = make_candidates(10);
        let cid = make_cid(0x42);

        let targets = rm.select_targets(&cid, &candidates);

        // Should have xor_closest nodes
        assert!(!targets.xor_closest.is_empty());
        assert!(targets.xor_closest.len() <= 4);

        // Should have tier-anchored nodes (candidates include T2+ and T3+ nodes)
        assert!(!targets.tier_anchored.is_empty());
    }

    #[test]
    fn test_select_targets_insufficient_nodes() {
        let rm = ReplicationManager::new(1);
        // Only 3 candidates (< 7)
        let candidates: Vec<(u64, u8)> = vec![(10, 0), (20, 1), (30, 2)];
        let cid = make_cid(0x01);

        let targets = rm.select_targets(&cid, &candidates);

        // Should use all available candidates
        assert_eq!(targets.total_count(), 3);
    }

    #[test]
    fn test_select_targets_no_high_tier() {
        let rm = ReplicationManager::new(1);
        // All tier-0 nodes
        let candidates: Vec<(u64, u8)> = (1..=10).map(|id| (id, 0u8)).collect();
        let cid = make_cid(0x01);

        let targets = rm.select_targets(&cid, &candidates);

        // Should still select 7 nodes (fallback to XOR overflow)
        assert_eq!(targets.total_count(), 7);
        // All come from XOR-closest (and overflow buckets)
        assert_eq!(targets.xor_closest.len(), 4);
    }

    #[test]
    fn test_initiate_store() {
        let mut rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);
        let targets = ReplicationTargets {
            xor_closest: vec![10, 20, 30, 40],
            tier_anchored: vec![50, 60],
            diversity: vec![70],
        };

        let pending = rm.initiate_store(cid, &targets);

        assert_eq!(pending.cid, cid);
        assert_eq!(pending.target_nodes.len(), 7);
        assert!(pending.acked_nodes.is_empty());
        assert!(rm.pending_stores.contains_key(&cid));
    }

    #[test]
    fn test_handle_ack() {
        let mut rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);
        let targets = ReplicationTargets {
            xor_closest: vec![10, 20, 30, 40],
            tier_anchored: vec![50, 60],
            diversity: vec![70],
        };
        rm.initiate_store(cid, &targets);

        let status = rm.handle_ack(&cid, 10);
        assert!(status.is_some());
        assert_eq!(status.unwrap(), ReplicationStatus::Critical(1));

        // Second ACK
        let status = rm.handle_ack(&cid, 20);
        assert_eq!(status.unwrap(), ReplicationStatus::Critical(2));
    }

    #[test]
    fn test_handle_ack_completes_store() {
        let mut rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);
        let targets = ReplicationTargets {
            xor_closest: vec![10, 20, 30, 40],
            tier_anchored: vec![50, 60],
            diversity: vec![70],
        };
        rm.initiate_store(cid, &targets);

        // ACK all 7 targets
        for &node in &[10, 20, 30, 40, 50, 60, 70] {
            rm.handle_ack(&cid, node);
        }

        let pending = rm.pending_stores.get(&cid).unwrap();
        assert_eq!(pending.acked_nodes.len(), 7);

        // Check health should be Healthy
        let status = rm.check_health(&cid, pending.acked_nodes.len());
        assert_eq!(status, ReplicationStatus::Healthy);
    }

    #[test]
    fn test_handle_ack_unknown_cid() {
        let mut rm = ReplicationManager::new(1);
        let cid = make_cid(0xFF);

        let status = rm.handle_ack(&cid, 10);
        assert!(status.is_none());
    }

    #[test]
    fn test_check_health_healthy() {
        let rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);

        assert_eq!(rm.check_health(&cid, 7), ReplicationStatus::Healthy);
        assert_eq!(rm.check_health(&cid, 10), ReplicationStatus::Healthy);
    }

    #[test]
    fn test_check_health_degraded() {
        let rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);

        assert_eq!(rm.check_health(&cid, 4), ReplicationStatus::Degraded(4));
        assert_eq!(rm.check_health(&cid, 5), ReplicationStatus::Degraded(5));
        assert_eq!(rm.check_health(&cid, 6), ReplicationStatus::Degraded(6));
    }

    #[test]
    fn test_check_health_critical() {
        let rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);

        assert_eq!(rm.check_health(&cid, 0), ReplicationStatus::Critical(0));
        assert_eq!(rm.check_health(&cid, 1), ReplicationStatus::Critical(1));
        assert_eq!(rm.check_health(&cid, 3), ReplicationStatus::Critical(3));
    }

    #[test]
    fn test_timed_out_stores() {
        let mut rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);

        // Manually set initiated_at to a known time
        let pending = PendingStore {
            cid,
            target_nodes: vec![10, 20, 30],
            acked_nodes: vec![10], // Only 1 of 3 ACKed
            initiated_at: 1000,
        };
        rm.pending_stores.insert(cid, pending);

        // Not timed out yet
        let timed = rm.timed_out_stores(1029, 30);
        assert!(timed.is_empty());

        // Timed out at 1030 (30s timeout)
        let timed = rm.timed_out_stores(1030, 30);
        assert_eq!(timed.len(), 1);
        assert_eq!(timed[0], cid);
    }

    #[test]
    fn test_cleanup_completed() {
        let mut rm = ReplicationManager::new(1);

        let cid1 = make_cid(0x01);
        let cid2 = make_cid(0x02);

        // Completed store (all ACKed)
        rm.pending_stores.insert(
            cid1,
            PendingStore {
                cid: cid1,
                target_nodes: vec![10, 20],
                acked_nodes: vec![10, 20],
                initiated_at: 1000,
            },
        );

        // Incomplete store
        rm.pending_stores.insert(
            cid2,
            PendingStore {
                cid: cid2,
                target_nodes: vec![30, 40, 50],
                acked_nodes: vec![30],
                initiated_at: 1000,
            },
        );

        rm.cleanup_completed();

        assert_eq!(rm.pending_stores.len(), 1);
        assert!(rm.pending_stores.contains_key(&cid2));
        assert!(!rm.pending_stores.contains_key(&cid1));
    }

    #[test]
    fn test_xor_distance() {
        let a = [0xFF; 32];
        let b = [0x00; 32];
        let dist = xor_distance(&a, &b);
        assert_eq!(dist, [0xFF; 32]); // max distance

        let dist_self = xor_distance(&a, &a);
        assert_eq!(dist_self, [0x00; 32]); // zero distance to self

        let mut c = [0u8; 32];
        c[0] = 0x01;
        let dist_small = xor_distance(&[0u8; 32], &c);
        assert_eq!(dist_small[0], 0x01);
        assert_eq!(dist_small[1], 0x00);
    }

    #[test]
    fn test_replication_targets_total_count() {
        let targets = ReplicationTargets {
            xor_closest: vec![1, 2, 3, 4],
            tier_anchored: vec![5, 6],
            diversity: vec![7],
        };

        assert_eq!(targets.total_count(), 7);
        assert_eq!(targets.all_targets(), vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_pending_store_tracks_acks() {
        let mut rm = ReplicationManager::new(1);
        let cid = make_cid(0x01);
        let targets = ReplicationTargets {
            xor_closest: vec![10, 20, 30, 40],
            tier_anchored: vec![50, 60],
            diversity: vec![70],
        };

        rm.initiate_store(cid, &targets);

        // ACK node 10 twice — should only count once
        rm.handle_ack(&cid, 10);
        rm.handle_ack(&cid, 10);

        let pending = rm.pending_stores.get(&cid).unwrap();
        assert_eq!(pending.acked_nodes.len(), 1);
    }

    #[test]
    fn test_select_targets_deduplication() {
        let rm = ReplicationManager::new(1);
        // Duplicate node IDs in candidates
        let candidates: Vec<(u64, u8)> = vec![
            (1, 0),
            (1, 0),
            (2, 1),
            (3, 2),
            (4, 3),
            (5, 0),
            (6, 1),
            (7, 2),
        ];
        let cid = make_cid(0x42);

        let targets = rm.select_targets(&cid, &candidates);

        // All targets should be unique
        let all = targets.all_targets();
        let mut unique = all.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(all.len(), unique.len());
    }

    #[test]
    fn test_replication_status_eq() {
        assert_eq!(ReplicationStatus::Healthy, ReplicationStatus::Healthy);
        assert_eq!(
            ReplicationStatus::Degraded(5),
            ReplicationStatus::Degraded(5)
        );
        assert_ne!(
            ReplicationStatus::Degraded(5),
            ReplicationStatus::Degraded(4)
        );
        assert_eq!(
            ReplicationStatus::Critical(1),
            ReplicationStatus::Critical(1)
        );
        assert_eq!(ReplicationStatus::Unknown, ReplicationStatus::Unknown);
        assert_ne!(ReplicationStatus::Healthy, ReplicationStatus::Degraded(7));
    }

    #[test]
    fn test_node_id_to_key() {
        // BLAKE3-hashed keys: deterministic, distinct, full 32-byte spread
        let key1a = node_id_to_key(1);
        let key1b = node_id_to_key(1);
        assert_eq!(key1a, key1b, "deterministic for same input");

        let key0 = node_id_to_key(0);
        let key_max = node_id_to_key(u64::MAX);
        assert_ne!(key0, key1a, "distinct inputs → distinct keys");
        assert_ne!(key1a, key_max);
        assert_ne!(key0, key_max);

        // Verify full 32-byte spread (not zero-padded)
        assert_ne!(key1a[8..], [0u8; 24], "BLAKE3 should populate all bytes");
        assert_ne!(key0[8..], [0u8; 24]);

        // Verify it matches direct BLAKE3 computation
        let expected = *blake3::hash(&1u64.to_be_bytes()).as_bytes();
        assert_eq!(key1a, expected);
    }
}
