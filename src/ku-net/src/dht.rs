//! # S/Kademlia DHT — SPEC B §5
//!
//! Modified Kademlia routing with:
//! - 256-bit XOR distance metric
//! - k-buckets (K=20) with replacement cache
//! - S/Kademlia disjoint lookup paths (β=3)
//! - Iterative FIND_NODE / FIND_VALUE / STORE

use std::collections::HashMap;
use std::time::Instant;

use crate::identity::NodeId;
use crate::messages::NetworkAddress;
use crate::constants::*;

// ─── Types ─────────────────────────────────────────────────────────────────

/// Entry in a k-bucket.
#[derive(Debug, Clone)]
pub struct KBucketEntry {
    /// The remote node's ID.
    pub node_id: NodeId,
    /// Network address of the node.
    pub address: NetworkAddress,
    /// When we last successfully communicated.
    pub last_seen: Instant,
    /// Estimated round-trip time in milliseconds.
    pub rtt_ms: u16,
    /// Number of consecutive failed probes (evicted when >= 3).
    pub stale_count: u8,
}

/// Result of attempting to insert into a k-bucket.
#[derive(Debug, PartialEq, Eq)]
pub enum InsertResult {
    /// Inserted successfully (bucket had space).
    Inserted,
    /// Node already in bucket, updated last_seen.
    Updated,
    /// Bucket full, added to replacement cache.
    AddedToReplacement,
    /// Cannot insert (own node ID).
    Rejected,
}

// ─── K-Bucket ──────────────────────────────────────────────────────────────

/// A single k-bucket holding up to K entries.
///
/// Entries are ordered: most-recently-seen at the tail.
/// Replacement cache holds candidates when bucket is full.
#[derive(Debug, Clone)]
pub struct KBucket {
    /// Active entries (max K).
    entries: Vec<KBucketEntry>,
    /// Replacement candidates (max K).
    replacement_cache: Vec<KBucketEntry>,
}

impl KBucket {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(K_BUCKET_SIZE),
            replacement_cache: Vec::with_capacity(K_BUCKET_SIZE),
        }
    }

    /// Number of active entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is this bucket empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries in replacement cache.
    pub fn replacement_len(&self) -> usize {
        self.replacement_cache.len()
    }

    /// Try to insert or update an entry.
    fn insert(&mut self, entry: KBucketEntry) -> InsertResult {
        // Check if already present → update
        if let Some(pos) = self.entries.iter().position(|e| e.node_id == entry.node_id) {
            self.entries[pos].last_seen = entry.last_seen;
            self.entries[pos].rtt_ms = entry.rtt_ms;
            self.entries[pos].stale_count = 0;
            // Move to tail (most recently seen)
            let updated = self.entries.remove(pos);
            self.entries.push(updated);
            return InsertResult::Updated;
        }

        // Bucket has space → insert at tail
        if self.entries.len() < K_BUCKET_SIZE {
            self.entries.push(entry);
            return InsertResult::Inserted;
        }

        // Bucket full → add to replacement cache
        // Remove from replacement cache if already there
        self.replacement_cache.retain(|e| e.node_id != entry.node_id);
        if self.replacement_cache.len() >= K_BUCKET_SIZE {
            self.replacement_cache.remove(0); // Drop oldest replacement
        }
        self.replacement_cache.push(entry);
        InsertResult::AddedToReplacement
    }

    /// Remove a node from the bucket.
    fn remove(&mut self, node_id: &NodeId) -> bool {
        if let Some(pos) = self.entries.iter().position(|e| e.node_id == *node_id) {
            self.entries.remove(pos);
            // Promote from replacement cache if available
            if let Some(replacement) = self.replacement_cache.pop() {
                self.entries.push(replacement);
            }
            true
        } else {
            false
        }
    }

    /// Mark a node as stale (failed probe). Returns true if evicted.
    fn mark_stale(&mut self, node_id: &NodeId) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.node_id == *node_id) {
            entry.stale_count += 1;
            if entry.stale_count >= 3 {
                self.remove(node_id);
                return true;
            }
        }
        false
    }

    /// Get all active entries, sorted by last_seen (most recent first).
    fn entries(&self) -> &[KBucketEntry] {
        &self.entries
    }
}

// ─── Routing Table ─────────────────────────────────────────────────────────

/// Kademlia routing table with 256 k-buckets.
///
/// Bucket `i` holds nodes whose XOR distance from our ID
/// has a leading-zero-bit count of exactly `i`.
pub struct RoutingTable {
    /// Our own node ID.
    my_id: NodeId,
    /// 256 k-buckets indexed by XOR distance prefix length.
    buckets: Vec<KBucket>,
    /// Total number of entries across all buckets.
    total_entries: usize,
}

impl RoutingTable {
    /// Create a new routing table for the given node ID.
    pub fn new(my_id: NodeId) -> Self {
        let mut buckets = Vec::with_capacity(NUM_BUCKETS);
        for _ in 0..NUM_BUCKETS {
            buckets.push(KBucket::new());
        }
        Self {
            my_id,
            buckets,
            total_entries: 0,
        }
    }

    /// Calculate which bucket a target node belongs in.
    ///
    /// Returns the index of the first differing bit (0-255).
    /// Nodes closer to us go in higher-index buckets.
    pub fn bucket_index(&self, target: &NodeId) -> Option<usize> {
        let distance = self.my_id.xor_distance(target);

        // Find the first non-zero byte, then the first set bit
        for (byte_idx, &byte) in distance.iter().enumerate() {
            if byte != 0 {
                let bit_idx = byte.leading_zeros() as usize;
                return Some(byte_idx * 8 + bit_idx);
            }
        }
        None // Target is our own ID
    }

    /// Insert a node into the routing table.
    pub fn insert(&mut self, entry: KBucketEntry) -> InsertResult {
        if entry.node_id == self.my_id {
            return InsertResult::Rejected;
        }

        let idx = match self.bucket_index(&entry.node_id) {
            Some(i) => i,
            None => return InsertResult::Rejected,
        };

        let result = self.buckets[idx].insert(entry);
        if result == InsertResult::Inserted {
            self.total_entries += 1;
        }
        result
    }

    /// Remove a node from the routing table.
    pub fn remove(&mut self, node_id: &NodeId) -> bool {
        if let Some(idx) = self.bucket_index(node_id) {
            if self.buckets[idx].remove(node_id) {
                self.total_entries -= 1;
                return true;
            }
        }
        false
    }

    /// Mark a node as stale. Returns true if evicted.
    pub fn mark_stale(&mut self, node_id: &NodeId) -> bool {
        if let Some(idx) = self.bucket_index(node_id) {
            let evicted = self.buckets[idx].mark_stale(node_id);
            if evicted {
                self.total_entries -= 1;
            }
            evicted
        } else {
            false
        }
    }

    /// Find the `count` closest nodes to a target.
    ///
    /// Searches outward from the target's bucket, collecting entries
    /// and sorting by XOR distance.
    pub fn find_closest(&self, target: &NodeId, count: usize) -> Vec<&KBucketEntry> {
        let mut candidates: Vec<&KBucketEntry> = Vec::new();

        // Collect all entries
        for bucket in &self.buckets {
            for entry in bucket.entries() {
                candidates.push(entry);
            }
        }

        // Sort by XOR distance to target
        candidates.sort_by(|a, b| {
            let dist_a = a.node_id.xor_distance(target);
            let dist_b = b.node_id.xor_distance(target);
            dist_a.cmp(&dist_b)
        });

        candidates.truncate(count);
        candidates
    }

    /// Total number of known nodes.
    pub fn total_entries(&self) -> usize {
        self.total_entries
    }

    /// Our node ID.
    pub fn my_id(&self) -> &NodeId {
        &self.my_id
    }

    /// Get a reference to a specific bucket.
    pub fn bucket(&self, index: usize) -> Option<&KBucket> {
        self.buckets.get(index)
    }

    /// Get all non-empty bucket indices and their sizes.
    pub fn bucket_stats(&self) -> Vec<(usize, usize)> {
        self.buckets.iter()
            .enumerate()
            .filter(|(_, b)| !b.is_empty())
            .map(|(i, b)| (i, b.len()))
            .collect()
    }
}

// ─── DHT Entry ─────────────────────────────────────────────────────────────

/// A stored entry in the DHT with optional TTL.
#[derive(Debug, Clone)]
pub struct DhtEntry {
    /// Stored value (serialized bytes).
    pub value: Vec<u8>,
    /// When this entry was stored (unix timestamp seconds).
    pub stored_at: u64,
    /// When this entry expires (None = permanent).
    pub expires_at: Option<u64>,
}

impl DhtEntry {
    /// Create a permanent entry (no TTL).
    pub fn permanent(value: Vec<u8>, now: u64) -> Self {
        Self { value, stored_at: now, expires_at: None }
    }

    /// Create an entry with TTL.
    pub fn with_ttl(value: Vec<u8>, now: u64, ttl_s: u64) -> Self {
        Self { value, stored_at: now, expires_at: Some(now + ttl_s) }
    }

    /// Whether this entry has expired.
    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.map_or(false, |exp| now >= exp)
    }
}

// ─── DHT Node ──────────────────────────────────────────────────────────────

/// DHT key-value storage with routing table.
pub struct DhtNode {
    /// Kademlia routing table.
    pub routing_table: RoutingTable,
    /// Local key-value store (CID → DhtEntry with optional TTL).
    storage: HashMap<[u8; 32], DhtEntry>,
    /// Maximum items in local storage.
    max_storage: usize,
}

impl DhtNode {
    /// Create a new DHT node.
    pub fn new(my_id: NodeId) -> Self {
        Self {
            routing_table: RoutingTable::new(my_id),
            storage: HashMap::new(),
            max_storage: 10_000,
        }
    }

    /// Store a key-value pair locally (permanent, no TTL).
    pub fn store(&mut self, key: [u8; 32], value: Vec<u8>) -> Result<(), DhtError> {
        if self.storage.len() >= self.max_storage {
            return Err(DhtError::StorageFull {
                capacity: self.max_storage,
            });
        }
        self.storage.insert(key, DhtEntry::permanent(value, 0));
        Ok(())
    }

    /// Store a key-value pair with TTL (auto-expires after `ttl_s` seconds).
    pub fn store_with_ttl(
        &mut self,
        key: [u8; 32],
        value: Vec<u8>,
        now: u64,
        ttl_s: u64,
    ) -> Result<(), DhtError> {
        if self.storage.len() >= self.max_storage {
            return Err(DhtError::StorageFull {
                capacity: self.max_storage,
            });
        }
        self.storage.insert(key, DhtEntry::with_ttl(value, now, ttl_s));
        Ok(())
    }

    /// Retrieve a value by key from local storage.
    pub fn get(&self, key: &[u8; 32]) -> Option<&Vec<u8>> {
        self.storage.get(key).map(|entry| &entry.value)
    }

    /// Retrieve the full DhtEntry (includes TTL metadata).
    pub fn get_entry(&self, key: &[u8; 32]) -> Option<&DhtEntry> {
        self.storage.get(key)
    }

    /// Check if we have a key locally.
    pub fn has(&self, key: &[u8; 32]) -> bool {
        self.storage.contains_key(key)
    }

    /// Number of items in local storage.
    pub fn storage_count(&self) -> usize {
        self.storage.len()
    }

    /// Expire all stale entries. Returns number of entries removed.
    pub fn expire_stale(&mut self, now: u64) -> usize {
        let before = self.storage.len();
        self.storage.retain(|_, entry| !entry.is_expired(now));
        before - self.storage.len()
    }

    /// Find the K closest nodes to a key (for FIND_NODE).
    pub fn find_closest_nodes(&self, key: &[u8; 32]) -> Vec<&KBucketEntry> {
        let target = NodeId(*key);
        self.routing_table.find_closest(&target, K_BUCKET_SIZE)
    }

    /// Handle a FIND_VALUE request.
    ///
    /// Returns either the value (if stored locally) or the K closest nodes.
    pub fn find_value(&self, key: &[u8; 32]) -> FindValueResult {
        if let Some(entry) = self.storage.get(key) {
            FindValueResult::Found(entry.value.clone())
        } else {
            let closest = self.find_closest_nodes(key);
            FindValueResult::ClosestNodes(
                closest.iter().map(|e| (e.node_id, e.address)).collect()
            )
        }
    }

    // ── Encoding Job Helpers ──────────────────────────────────────────────

    /// Store an encoding job on DHT with standard TTL.
    ///
    /// Uses the job's DHT key and serializes via CBOR (ciborium).
    pub fn store_encoding_job(
        &mut self,
        job: &crate::encoding_job::EncodingJob,
        now: u64,
    ) -> Result<(), DhtError> {
        let key = job.dht_key();
        let mut value = Vec::new();
        ciborium::ser::into_writer(job, &mut value)
            .map_err(|_| DhtError::StorageFull { capacity: 0 })?; // TODO: proper error variant
        self.store_with_ttl(key, value, now, crate::constants::ENCODING_JOB_TTL_S)
    }

    /// Retrieve an encoding job from DHT by raw_hash.
    pub fn find_encoding_job(
        &self,
        raw_hash: &[u8; 32],
    ) -> Option<crate::encoding_job::EncodingJob> {
        let key = crate::encoding_job::EncodingJob::compute_dht_key(raw_hash);
        self.get(&key)
            .and_then(|bytes| ciborium::de::from_reader(bytes.as_slice()).ok())
    }
}

/// Result of a FIND_VALUE operation.
#[derive(Debug)]
pub enum FindValueResult {
    /// Value was found locally.
    Found(Vec<u8>),
    /// Value not found; here are the closest known nodes.
    ClosestNodes(Vec<(NodeId, NetworkAddress)>),
}

/// DHT operation errors.
#[derive(Debug)]
pub enum DhtError {
    /// Local storage is full.
    StorageFull { capacity: usize },
    /// Node not found in routing table.
    NodeNotFound,
    /// Lookup timed out.
    LookupTimeout,
}

impl std::fmt::Display for DhtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StorageFull { capacity } =>
                write!(f, "DHT storage full: {} items", capacity),
            Self::NodeNotFound =>
                write!(f, "Node not found in routing table"),
            Self::LookupTimeout =>
                write!(f, "DHT lookup timed out"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// OBT Replica Tracking — Storage Reward Support
// ═══════════════════════════════════════════════════════════════════════════

/// Metadata tracked per stored KU for OBT storage reward computation.
#[derive(Debug, Clone)]
pub struct StoredKuMeta {
    pub actual_replicas: u32,
    pub first_stored_epoch: u64,
    pub epochs_stored: u64,
    pub last_updated_epoch: u64,
}

impl StoredKuMeta {
    pub fn new(current_epoch: u64) -> Self {
        Self { actual_replicas: 1, first_stored_epoch: current_epoch, epochs_stored: 1, last_updated_epoch: current_epoch }
    }
    pub fn update_replicas(&mut self, count: u32) { self.actual_replicas = count; }
    pub fn advance_epoch(&mut self, current_epoch: u64) {
        if current_epoch > self.last_updated_epoch {
            self.epochs_stored += current_epoch - self.last_updated_epoch;
            self.last_updated_epoch = current_epoch;
        }
    }
}

/// Tracks replica metadata for all locally stored KUs.
#[derive(Debug, Clone, Default)]
pub struct ReplicaTracker {
    entries: std::collections::HashMap<[u8; 32], StoredKuMeta>,
}

impl ReplicaTracker {
    pub fn new() -> Self { Self::default() }
    pub fn record_store(&mut self, ku_cid: [u8; 32], current_epoch: u64) {
        self.entries.entry(ku_cid).or_insert_with(|| StoredKuMeta::new(current_epoch));
    }
    pub fn record_eviction(&mut self, ku_cid: &[u8; 32]) { self.entries.remove(ku_cid); }
    pub fn update_replicas(&mut self, ku_cid: &[u8; 32], count: u32) {
        if let Some(meta) = self.entries.get_mut(ku_cid) { meta.update_replicas(count); }
    }
    pub fn advance_epoch(&mut self, current_epoch: u64) {
        for meta in self.entries.values_mut() { meta.advance_epoch(current_epoch); }
    }
    pub fn get(&self, ku_cid: &[u8; 32]) -> Option<&StoredKuMeta> { self.entries.get(ku_cid) }
    pub fn all_stored(&self) -> &std::collections::HashMap<[u8; 32], StoredKuMeta> { &self.entries }
    pub fn count(&self) -> usize { self.entries.len() }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{generate_node_id, KeyPair, PUZZLE_C_SMALL};

    fn make_node() -> (NodeId, NetworkAddress) {
        let kp = KeyPair::generate();
        let proof = generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL);
        let addr = NetworkAddress::new_v4(10, 0, 0, 1, 4242);
        (proof.node_id, addr)
    }

    fn make_entry() -> KBucketEntry {
        let (node_id, address) = make_node();
        KBucketEntry {
            node_id,
            address,
            last_seen: Instant::now(),
            rtt_ms: 50,
            stale_count: 0,
        }
    }

    #[test]
    fn test_kbucket_insert_and_len() {
        let mut bucket = KBucket::new();
        assert!(bucket.is_empty());

        let entry = make_entry();
        let result = bucket.insert(entry);
        assert_eq!(result, InsertResult::Inserted);
        assert_eq!(bucket.len(), 1);
    }

    #[test]
    fn test_kbucket_update_existing() {
        let mut bucket = KBucket::new();
        let entry = make_entry();
        let node_id = entry.node_id;

        bucket.insert(entry);
        assert_eq!(bucket.len(), 1);

        // Insert same node again → Update
        let entry2 = KBucketEntry {
            node_id,
            address: NetworkAddress::new_v4(10, 0, 0, 2, 4242),
            last_seen: Instant::now(),
            rtt_ms: 30,
            stale_count: 0,
        };
        let result = bucket.insert(entry2);
        assert_eq!(result, InsertResult::Updated);
        assert_eq!(bucket.len(), 1);
    }

    #[test]
    fn test_kbucket_full_replacement_cache() {
        let mut bucket = KBucket::new();

        // Fill bucket to K
        for _ in 0..K_BUCKET_SIZE {
            bucket.insert(make_entry());
        }
        assert_eq!(bucket.len(), K_BUCKET_SIZE);

        // Next insert goes to replacement
        let overflow_entry = make_entry();
        let result = bucket.insert(overflow_entry);
        assert_eq!(result, InsertResult::AddedToReplacement);
        assert_eq!(bucket.len(), K_BUCKET_SIZE);
        assert_eq!(bucket.replacement_len(), 1);
    }

    #[test]
    fn test_kbucket_remove_promotes_replacement() {
        let mut bucket = KBucket::new();

        // Fill bucket
        let mut node_ids = Vec::new();
        for _ in 0..K_BUCKET_SIZE {
            let entry = make_entry();
            node_ids.push(entry.node_id);
            bucket.insert(entry);
        }

        // Add to replacement cache
        let replacement = make_entry();
        let replacement_id = replacement.node_id;
        bucket.insert(replacement);
        assert_eq!(bucket.replacement_len(), 1);

        // Remove first node → replacement promoted
        bucket.remove(&node_ids[0]);
        assert_eq!(bucket.len(), K_BUCKET_SIZE);
        assert_eq!(bucket.replacement_len(), 0);
        // Replacement should now be in active entries
        assert!(bucket.entries().iter().any(|e| e.node_id == replacement_id));
    }

    #[test]
    fn test_kbucket_stale_eviction() {
        let mut bucket = KBucket::new();
        let entry = make_entry();
        let node_id = entry.node_id;
        bucket.insert(entry);

        // 3 stale marks → eviction
        assert!(!bucket.mark_stale(&node_id));
        assert!(!bucket.mark_stale(&node_id));
        assert!(bucket.mark_stale(&node_id)); // Evicted
        assert!(bucket.is_empty());
    }

    #[test]
    fn test_routing_table_bucket_index() {
        let (my_id, _) = make_node();
        let rt = RoutingTable::new(my_id);

        // Own ID → None
        assert_eq!(rt.bucket_index(&my_id), None);

        // Different ID → Some(index)
        let (other_id, _) = make_node();
        let idx = rt.bucket_index(&other_id);
        assert!(idx.is_some());
        assert!(idx.unwrap() < NUM_BUCKETS);
    }

    #[test]
    fn test_routing_table_insert_and_find() {
        let (my_id, _) = make_node();
        let mut rt = RoutingTable::new(my_id);

        // Insert 10 nodes
        let mut inserted_ids = Vec::new();
        for _ in 0..10 {
            let entry = make_entry();
            inserted_ids.push(entry.node_id);
            rt.insert(entry);
        }

        assert_eq!(rt.total_entries(), 10);

        // Find closest to a random target
        let (target, _) = make_node();
        let closest = rt.find_closest(&target, 5);
        assert!(closest.len() <= 5);
        assert!(!closest.is_empty());

        // Verify XOR ordering
        for i in 1..closest.len() {
            let dist_prev = closest[i - 1].node_id.xor_distance(&target);
            let dist_curr = closest[i].node_id.xor_distance(&target);
            assert!(dist_prev <= dist_curr, "Results should be sorted by distance");
        }
    }

    #[test]
    fn test_routing_table_reject_self() {
        let (my_id, _) = make_node();
        let mut rt = RoutingTable::new(my_id);

        let entry = KBucketEntry {
            node_id: my_id,
            address: NetworkAddress::new_v4(127, 0, 0, 1, 4242),
            last_seen: Instant::now(),
            rtt_ms: 0,
            stale_count: 0,
        };

        assert_eq!(rt.insert(entry), InsertResult::Rejected);
        assert_eq!(rt.total_entries(), 0);
    }

    #[test]
    fn test_dht_node_store_and_get() {
        let (my_id, _) = make_node();
        let mut dht = DhtNode::new(my_id);

        let key = [0x42; 32];
        let value = b"test KU data".to_vec();

        dht.store(key, value.clone()).unwrap();
        assert!(dht.has(&key));
        assert_eq!(dht.get(&key).unwrap(), &value);
        assert_eq!(dht.storage_count(), 1);
    }

    #[test]
    fn test_dht_node_find_value_found() {
        let (my_id, _) = make_node();
        let mut dht = DhtNode::new(my_id);

        let key = [0xAA; 32];
        let value = b"some data".to_vec();
        dht.store(key, value.clone()).unwrap();

        match dht.find_value(&key) {
            FindValueResult::Found(v) => assert_eq!(v, value),
            FindValueResult::ClosestNodes(_) => panic!("Should have found value"),
        }
    }

    #[test]
    fn test_dht_node_find_value_not_found() {
        let (my_id, _) = make_node();
        let mut dht = DhtNode::new(my_id);

        // Insert some nodes into routing table
        for _ in 0..5 {
            let entry = make_entry();
            dht.routing_table.insert(entry);
        }

        let key = [0xBB; 32];
        match dht.find_value(&key) {
            FindValueResult::Found(_) => panic!("Should not find value"),
            FindValueResult::ClosestNodes(nodes) => {
                assert!(!nodes.is_empty(), "Should return closest nodes");
            }
        }
    }

    #[test]
    fn test_routing_table_bucket_stats() {
        let (my_id, _) = make_node();
        let mut rt = RoutingTable::new(my_id);

        for _ in 0..20 {
            rt.insert(make_entry());
        }

        let stats = rt.bucket_stats();
        let total: usize = stats.iter().map(|(_, count)| count).sum();
        assert_eq!(total, rt.total_entries());
    }

    // ── OBT Replica Tracking Tests ──

    #[test]
    fn test_stored_ku_meta_new() {
        let meta = StoredKuMeta::new(100);
        assert_eq!(meta.actual_replicas, 1);
        assert_eq!(meta.first_stored_epoch, 100);
        assert_eq!(meta.epochs_stored, 1);
    }

    #[test]
    fn test_stored_ku_meta_advance_epoch() {
        let mut meta = StoredKuMeta::new(100);
        meta.advance_epoch(105);
        assert_eq!(meta.epochs_stored, 6);
        assert_eq!(meta.last_updated_epoch, 105);
        meta.advance_epoch(105); // idempotent
        assert_eq!(meta.epochs_stored, 6);
    }

    #[test]
    fn test_replica_tracker_basic() {
        let mut tracker = ReplicaTracker::new();
        let cid = [42u8; 32];
        tracker.record_store(cid, 100);
        assert_eq!(tracker.count(), 1);
        assert_eq!(tracker.get(&cid).unwrap().actual_replicas, 1);
    }

    #[test]
    fn test_replica_tracker_update_and_evict() {
        let mut tracker = ReplicaTracker::new();
        let cid = [42u8; 32];
        tracker.record_store(cid, 100);
        tracker.update_replicas(&cid, 15);
        assert_eq!(tracker.get(&cid).unwrap().actual_replicas, 15);
        tracker.record_eviction(&cid);
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_replica_tracker_advance_all() {
        let mut tracker = ReplicaTracker::new();
        tracker.record_store([1u8; 32], 100);
        tracker.record_store([2u8; 32], 102);
        tracker.advance_epoch(110);
        assert_eq!(tracker.get(&[1u8; 32]).unwrap().epochs_stored, 11);
        assert_eq!(tracker.get(&[2u8; 32]).unwrap().epochs_stored, 9);
    }

    #[test]
    fn test_replica_tracker_no_double_insert() {
        let mut tracker = ReplicaTracker::new();
        let cid = [42u8; 32];
        tracker.record_store(cid, 100);
        tracker.record_store(cid, 200); // should NOT reset
        assert_eq!(tracker.get(&cid).unwrap().first_stored_epoch, 100);
    }
}
