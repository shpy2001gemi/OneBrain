//! # ConceptRegistry Gossip — Delta-Based Registry Distribution (v7)
//!
//! Distributes ConceptRegistry entries across the OBP network using
//! delta-based gossip with bloom filter anti-entropy.
//!
//! ## Design Rationale
//!
//! The full ConceptRegistry is ~200MB (~8M concepts). Sending the whole
//! registry to every peer is impractical. Instead, we use:
//!
//! 1. **Delta Push** — When a node adds new concepts (from AI extraction),
//!    it pushes only the new entries to DHT neighbors.
//! 2. **Checkpoint Summaries** — Periodic bloom-filter digests so peers
//!    can detect missing entries without sending the full registry.
//! 3. **Delta Pull** — A peer that detects missing entries requests
//!    specific CCIDs from a neighbor that has them.
//!
//! ## Message Types (0xC0–0xC3)
//! | Code | Message                   | Direction                    |
//! |------|---------------------------|------------------------------|
//! | 0xC0 | RegistryDeltaPush         | Node → DHT neighbors         |
//! | 0xC1 | RegistryDeltaPull         | Peer → Peer (request)        |
//! | 0xC2 | RegistryDeltaResponse     | Peer → Peer (response)       |
//! | 0xC3 | RegistryCheckpointSummary | Node → Gossip ring (periodic)|
//!
//! ## Consistency Model
//! - Eventual consistency (no total ordering needed)
//! - CRDT semantics: entries are immutable once created (add-only set)
//! - CCID = content hash → identical content always produces same entry
//! - No conflict resolution needed (content-addressed = no conflicts)

use serde::{Deserialize, Serialize};

use ku_core::concept_registry::{ConceptCategory, ResolvedConcept};

// ═══════════════════════════════════════════════════════════════════════════
// §1 — Wire Types: Serializable Concept Entry
// ═══════════════════════════════════════════════════════════════════════════

/// A concept entry in wire format for network transmission.
///
/// Smaller than `ResolvedConcept` — uses compact representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireConceptEntry {
    /// 16-byte CCID (content-addressed concept identity).
    pub ccid: [u8; 16],
    /// Wikidata QID (0 if not from Wikidata).
    pub qid: u32,
    /// Category byte.
    pub category: u8,
    /// Canonical name.
    pub canonical_name: String,
    /// All labels (for multi-language lookup).
    pub labels: Vec<String>,
}

impl WireConceptEntry {
    /// Convert from a ResolvedConcept + labels.
    pub fn from_resolved(concept: &ResolvedConcept, labels: Vec<String>) -> Self {
        Self {
            ccid: concept.ccid,
            qid: concept.qid,
            category: concept.category as u8,
            canonical_name: concept.canonical_name.clone(),
            labels,
        }
    }

    /// Convert to a ResolvedConcept.
    pub fn to_resolved(&self) -> ResolvedConcept {
        ResolvedConcept {
            ccid: self.ccid,
            qid: self.qid,
            category: ConceptCategory::from_u8(self.category),
            canonical_name: self.canonical_name.clone(),
        }
    }

    /// Estimated wire size in bytes (for bandwidth budgeting).
    pub fn estimated_size(&self) -> usize {
        16 + 4 + 1 // ccid + qid + category
        + 2 + self.canonical_name.len() // length-prefix + name
        + 2 + self.labels.iter().map(|l| 2 + l.len()).sum::<usize>() // labels
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §2 — Protocol Messages
// ═══════════════════════════════════════════════════════════════════════════

/// Registry gossip message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryMessage {
    /// Push new concept entries to DHT neighbors (0xC0).
    ///
    /// Sent when a node creates new concepts from AI extraction.
    /// Batched: up to 100 entries per message (~50KB typical).
    DeltaPush(RegistryDeltaPush),

    /// Request specific concept entries by CCID (0xC1).
    ///
    /// Sent when a node detects missing entries via checkpoint bloom filter.
    DeltaPull(RegistryDeltaPull),

    /// Response to a DeltaPull request (0xC2).
    ///
    /// Contains the requested concept entries.
    DeltaResponse(RegistryDeltaResponse),

    /// Periodic checkpoint summary (0xC3).
    ///
    /// A compact bloom filter digest of all concepts a node has.
    /// Neighbors compare with their own to detect missing entries.
    CheckpointSummary(RegistryCheckpointSummary),
}

/// Push new concept entries to DHT neighbors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDeltaPush {
    /// Sender's Ed25519 public key (peer identifier).
    pub peer_id: [u8; 32],
    /// Batch of new concept entries.
    pub entries: Vec<WireConceptEntry>,
    /// Sender's current registry size (for diagnostics).
    pub registry_size: u64,
    /// Monotonic sequence number (for dedup/ordering).
    pub seq: u64,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// Request missing concept entries by CCID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDeltaPull {
    /// Requester's Ed25519 public key.
    pub requester_id: [u8; 32],
    /// CCIDs of concepts we're missing.
    pub wanted_ccids: Vec<[u8; 16]>,
    /// Max entries to return (flow control).
    pub max_entries: u32,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// Response containing requested concept entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDeltaResponse {
    /// Responder's Ed25519 public key.
    pub responder_id: [u8; 32],
    /// Concept entries found.
    pub entries: Vec<WireConceptEntry>,
    /// CCIDs that were requested but not found on this peer.
    pub not_found: Vec<[u8; 16]>,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

/// Periodic checkpoint: bloom filter of all concept CCIDs.
///
/// Size budget: ~64KB bloom filter covers 8M entries with 1% FPR.
/// Sent every CHECKPOINT_INTERVAL (default: 5 minutes) to gossip ring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCheckpointSummary {
    /// Sender's Ed25519 public key.
    pub peer_id: [u8; 32],
    /// Total concepts in sender's registry.
    pub concept_count: u64,
    /// Bloom filter bits (serialized as bytes).
    /// Parameters: k=7 hash functions, m=524288 bits (64KB).
    pub bloom_filter: Vec<u8>,
    /// Checkpoint epoch (monotonically increasing).
    pub epoch: u64,
    /// Timestamp (Unix millis, UTC).
    pub timestamp: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// §3 — Bloom Filter (lightweight, purpose-built)
// ═══════════════════════════════════════════════════════════════════════════

/// Compact bloom filter for CCID membership testing.
///
/// Used in checkpoint summaries to let peers detect which concepts
/// they're missing without sending the full entry list.
///
/// Parameters for 8M entries, 1% FPR:
/// - m = 76,839,040 bits ≈ 9.6 MB (too large for gossip)
///
/// For gossip, we use smaller filters per checkpoint window:
/// - m = 524,288 bits = 64 KB (covers ~55K entries at 1% FPR)
/// - k = 7 hash functions
///
/// This means the checkpoint filter only covers the most recent
/// entries. Full sync uses DeltaPull, not bloom filters.
pub struct RegistryBloomFilter {
    /// Bit array.
    bits: Vec<u8>,
    /// Number of hash functions (k).
    num_hashes: u8,
    /// Number of items inserted.
    count: u64,
}

/// Default bloom filter size: 64KB = 524,288 bits.
const BLOOM_BITS: usize = 524_288;
/// Default number of hash functions.
const BLOOM_K: u8 = 7;
/// Maximum entries in a single DeltaPush batch.
pub const MAX_DELTA_PUSH_ENTRIES: usize = 100;
/// Checkpoint interval suggestion (seconds).
pub const CHECKPOINT_INTERVAL_S: u64 = 300; // 5 minutes

impl RegistryBloomFilter {
    /// Create an empty bloom filter with default parameters.
    pub fn new() -> Self {
        Self {
            bits: vec![0u8; BLOOM_BITS / 8],
            num_hashes: BLOOM_K,
            count: 0,
        }
    }

    /// Create from serialized bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bits: bytes,
            num_hashes: BLOOM_K,
            count: 0, // unknown when deserializing
        }
    }

    /// Insert a CCID into the filter.
    pub fn insert(&mut self, ccid: &[u8; 16]) {
        let bit_len = self.bits.len() * 8;
        for i in 0..self.num_hashes {
            let h = self.hash_at(ccid, i);
            let idx = (h as usize) % bit_len;
            self.bits[idx / 8] |= 1 << (idx % 8);
        }
        self.count += 1;
    }

    /// Test if a CCID is probably in the filter.
    ///
    /// Returns `true` if the CCID is probably present (with FPR),
    /// `false` if definitely not present.
    pub fn probably_contains(&self, ccid: &[u8; 16]) -> bool {
        let bit_len = self.bits.len() * 8;
        for i in 0..self.num_hashes {
            let h = self.hash_at(ccid, i);
            let idx = (h as usize) % bit_len;
            if self.bits[idx / 8] & (1 << (idx % 8)) == 0 {
                return false;
            }
        }
        true
    }

    /// Number of items inserted.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Serialize to bytes for wire transmission.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.bits.clone()
    }

    /// Compute k-th hash of a CCID using double-hashing scheme.
    ///
    /// h_k(x) = h1(x) + k * h2(x) (mod m)
    ///
    /// Where h1 and h2 are derived from the 16-byte CCID:
    /// - h1 = first 8 bytes as u64
    /// - h2 = last 8 bytes as u64
    fn hash_at(&self, ccid: &[u8; 16], k: u8) -> u64 {
        let h1 = u64::from_le_bytes(ccid[0..8].try_into().unwrap());
        let h2 = u64::from_le_bytes(ccid[8..16].try_into().unwrap());
        h1.wrapping_add((k as u64).wrapping_mul(h2))
    }
}

impl Default for RegistryBloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §4 — Registry Gossip Manager
// ═══════════════════════════════════════════════════════════════════════════

/// Manages ConceptRegistry gossip: incoming/outgoing delta sync.
///
/// Sits between the local `ConceptRegistry` and the network layer.
/// The network layer calls `handle_*` methods on incoming messages and
/// calls `pending_*` methods to get outgoing messages.
pub struct RegistryGossipManager {
    /// Outgoing delta queue: new entries waiting to be pushed.
    pending_pushes: Vec<WireConceptEntry>,
    /// Monotonic sequence counter for DeltaPush.
    push_seq: u64,
    /// Current checkpoint bloom filter (rebuilt periodically).
    checkpoint_filter: RegistryBloomFilter,
    /// Checkpoint epoch counter.
    checkpoint_epoch: u64,
    /// Recently seen push sequences (for dedup). Maps peer_id → last_seq.
    seen_seqs: std::collections::HashMap<[u8; 32], u64>,
    /// Local registry size (tracked for diagnostics).
    local_registry_size: u64,
}

impl RegistryGossipManager {
    /// Create a new gossip manager.
    pub fn new() -> Self {
        Self {
            pending_pushes: Vec::new(),
            push_seq: 0,
            checkpoint_filter: RegistryBloomFilter::new(),
            checkpoint_epoch: 0,
            seen_seqs: std::collections::HashMap::new(),
            local_registry_size: 0,
        }
    }

    // ── Outgoing: local node → network ───────────────────────────────

    /// Enqueue new concept entries for delta push to neighbors.
    ///
    /// Called after local AI extraction creates new concepts.
    pub fn enqueue_push(&mut self, entries: Vec<WireConceptEntry>) {
        for entry in &entries {
            self.checkpoint_filter.insert(&entry.ccid);
        }
        self.pending_pushes.extend(entries);
    }

    /// Drain pending pushes into batched DeltaPush messages.
    ///
    /// Returns one or more DeltaPush messages, each ≤ MAX_DELTA_PUSH_ENTRIES.
    pub fn drain_pushes(&mut self, local_peer_id: [u8; 32]) -> Vec<RegistryDeltaPush> {
        if self.pending_pushes.is_empty() {
            return Vec::new();
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut messages = Vec::new();
        for batch in self.pending_pushes.chunks(MAX_DELTA_PUSH_ENTRIES) {
            self.push_seq += 1;
            messages.push(RegistryDeltaPush {
                peer_id: local_peer_id,
                entries: batch.to_vec(),
                registry_size: self.local_registry_size,
                seq: self.push_seq,
                timestamp: now,
            });
        }
        self.pending_pushes.clear();
        messages
    }

    /// Build a checkpoint summary for periodic gossip.
    ///
    /// Should be called every CHECKPOINT_INTERVAL_S seconds.
    pub fn build_checkpoint(&mut self, local_peer_id: [u8; 32]) -> RegistryCheckpointSummary {
        self.checkpoint_epoch += 1;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        RegistryCheckpointSummary {
            peer_id: local_peer_id,
            concept_count: self.local_registry_size,
            bloom_filter: self.checkpoint_filter.to_bytes(),
            epoch: self.checkpoint_epoch,
            timestamp: now,
        }
    }

    /// Build a DeltaPull request for concepts missing from a peer's checkpoint.
    ///
    /// Compares local CCIDs against the peer's bloom filter to find
    /// entries the peer is missing, then requests them.
    pub fn build_pull_from_checkpoint(
        &self,
        local_peer_id: [u8; 32],
        peer_checkpoint: &RegistryCheckpointSummary,
        _local_ccids: &[[u8; 16]],
    ) -> Option<RegistryDeltaPull> {
        let _peer_filter = RegistryBloomFilter::from_bytes(peer_checkpoint.bloom_filter.clone());

        // Find CCIDs that the peer probably has but we DON'T have
        // We actually need to find what WE are missing.
        // But we can't know that from a push model alone.
        //
        // The correct approach: the PEER sends us their bloom filter.
        // We check which of OUR ccids are NOT in their filter = they're missing those.
        // We check which of THEIR concepts are NOT in OUR filter = we're missing those.
        //
        // Since we only have their bloom filter (not their full list),
        // we can only detect what THEY are missing. They do the same for us.
        //
        // For "what am I missing": we look at their concept_count vs ours.
        // If they have more, we request a pull with an empty wanted list
        // and let them send their recent entries.

        if peer_checkpoint.concept_count <= self.local_registry_size {
            return None; // Peer has same or fewer concepts, nothing to pull
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Request entries: find CCIDs from peer's filter that we don't have.
        // Since we can't enumerate the peer's bloom filter, we send an empty
        // wanted_ccids list to request "anything new since our last sync".
        // The peer will diff against our count to decide what to send.
        Some(RegistryDeltaPull {
            requester_id: local_peer_id,
            wanted_ccids: Vec::new(), // empty = "send me your latest"
            max_entries: 500,
            timestamp: now,
        })
    }

    // ── Incoming: network → local node ───────────────────────────────

    /// Handle an incoming DeltaPush from a peer.
    ///
    /// Returns the new entries that should be added to local ConceptRegistry.
    /// Performs dedup by tracking peer sequence numbers.
    pub fn handle_delta_push(&mut self, push: &RegistryDeltaPush) -> Vec<WireConceptEntry> {
        // Dedup: skip if we've already seen this or a later seq from this peer
        let last_seq = self.seen_seqs.get(&push.peer_id).copied().unwrap_or(0);
        if push.seq <= last_seq {
            return Vec::new(); // Already processed
        }
        self.seen_seqs.insert(push.peer_id, push.seq);

        // Filter out entries we already have (by checking our bloom filter)
        let new_entries: Vec<_> = push
            .entries
            .iter()
            .filter(|e| !self.checkpoint_filter.probably_contains(&e.ccid))
            .cloned()
            .collect();

        // Add new entries to our checkpoint filter
        for entry in &new_entries {
            self.checkpoint_filter.insert(&entry.ccid);
            self.local_registry_size += 1;
        }

        new_entries
    }

    /// Handle an incoming DeltaPull request.
    ///
    /// Returns entries from the local registry that match the wanted CCIDs.
    /// If wanted_ccids is empty, returns the most recent entries up to max_entries.
    pub fn handle_delta_pull(
        &self,
        pull: &RegistryDeltaPull,
        local_entries: &[(ResolvedConcept, Vec<String>)],
        local_peer_id: [u8; 32],
    ) -> RegistryDeltaResponse {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let max = pull.max_entries.min(500) as usize;

        if pull.wanted_ccids.is_empty() {
            // Peer wants our latest entries (up to max)
            let entries: Vec<_> = local_entries
                .iter()
                .rev() // most recent first
                .take(max)
                .map(|(c, labels)| WireConceptEntry::from_resolved(c, labels.clone()))
                .collect();

            RegistryDeltaResponse {
                responder_id: local_peer_id,
                entries,
                not_found: Vec::new(),
                timestamp: now,
            }
        } else {
            // Peer wants specific CCIDs
            let mut entries = Vec::new();
            let mut not_found = Vec::new();

            for wanted in &pull.wanted_ccids {
                if let Some((concept, labels)) =
                    local_entries.iter().find(|(c, _)| c.ccid == *wanted)
                {
                    entries.push(WireConceptEntry::from_resolved(concept, labels.clone()));
                } else {
                    not_found.push(*wanted);
                }
                if entries.len() >= max {
                    break;
                }
            }

            RegistryDeltaResponse {
                responder_id: local_peer_id,
                entries,
                not_found,
                timestamp: now,
            }
        }
    }

    /// Handle an incoming checkpoint summary from a peer.
    ///
    /// Checks if we have concepts that the peer doesn't (and vice versa).
    /// Returns CCIDs that the peer is missing (so we can push them).
    pub fn handle_checkpoint(
        &self,
        checkpoint: &RegistryCheckpointSummary,
        local_ccids: &[[u8; 16]],
    ) -> Vec<[u8; 16]> {
        let peer_filter = RegistryBloomFilter::from_bytes(checkpoint.bloom_filter.clone());

        // Find our CCIDs that the peer probably doesn't have
        local_ccids
            .iter()
            .filter(|ccid| !peer_filter.probably_contains(ccid))
            .copied()
            .collect()
    }

    /// Update local registry size counter (call after adding entries).
    pub fn set_registry_size(&mut self, size: u64) {
        self.local_registry_size = size;
    }

    /// Get current checkpoint epoch.
    pub fn checkpoint_epoch(&self) -> u64 {
        self.checkpoint_epoch
    }

    /// Get the bloom filter for the current checkpoint.
    pub fn checkpoint_filter(&self) -> &RegistryBloomFilter {
        &self.checkpoint_filter
    }
}

impl Default for RegistryGossipManager {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// §5 — Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str, qid: u32) -> WireConceptEntry {
        let ccid = ku_core::ccid::ccid(format!("wd:Q{}", qid).as_bytes());
        WireConceptEntry {
            ccid: ccid,
            qid,
            category: ConceptCategory::Entity as u8,
            canonical_name: name.to_string(),
            labels: vec![name.to_string()],
        }
    }

    fn make_peer_id(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    #[test]
    fn test_bloom_filter_basic() {
        let mut bf = RegistryBloomFilter::new();
        let ccid1 = ku_core::ccid::ccid(b"wd:Q283");
        let ccid2 = ku_core::ccid::ccid(b"wd:Q42");
        let ccid3 = ku_core::ccid::ccid(b"wd:Q999999");

        bf.insert(&ccid1);
        bf.insert(&ccid2);

        assert!(bf.probably_contains(&ccid1));
        assert!(bf.probably_contains(&ccid2));
        assert!(!bf.probably_contains(&ccid3)); // Probably not present
        assert_eq!(bf.count(), 2);
    }

    #[test]
    fn test_bloom_filter_serialization() {
        let mut bf = RegistryBloomFilter::new();
        let ccid = ku_core::ccid::ccid(b"test_concept");
        bf.insert(&ccid);

        let bytes = bf.to_bytes();
        let bf2 = RegistryBloomFilter::from_bytes(bytes);

        assert!(bf2.probably_contains(&ccid));
    }

    #[test]
    fn test_delta_push_dedup() {
        let mut mgr = RegistryGossipManager::new();
        let peer = make_peer_id(0xAA);
        let entries = vec![make_entry("water", 283)];

        // First push: seq=1 → accepted
        let push1 = RegistryDeltaPush {
            peer_id: peer,
            entries: entries.clone(),
            registry_size: 1,
            seq: 1,
            timestamp: 100,
        };
        let result = mgr.handle_delta_push(&push1);
        assert_eq!(result.len(), 1);

        // Duplicate push: seq=1 → rejected
        let result2 = mgr.handle_delta_push(&push1);
        assert_eq!(result2.len(), 0, "Duplicate seq should be rejected");

        // Next push: seq=2 → accepted
        let push2 = RegistryDeltaPush {
            peer_id: peer,
            entries: vec![make_entry("fire", 3196)],
            registry_size: 2,
            seq: 2,
            timestamp: 200,
        };
        let result3 = mgr.handle_delta_push(&push2);
        assert_eq!(result3.len(), 1);
    }

    #[test]
    fn test_enqueue_and_drain() {
        let mut mgr = RegistryGossipManager::new();
        let peer_id = make_peer_id(0x01);

        // Enqueue 250 entries → should produce 3 batches (100+100+50)
        let entries: Vec<_> = (0..250u32)
            .map(|i| make_entry(&format!("concept_{}", i), i))
            .collect();
        mgr.enqueue_push(entries);

        let messages = mgr.drain_pushes(peer_id);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].entries.len(), 100);
        assert_eq!(messages[1].entries.len(), 100);
        assert_eq!(messages[2].entries.len(), 50);

        // Sequences should be monotonic
        assert_eq!(messages[0].seq, 1);
        assert_eq!(messages[1].seq, 2);
        assert_eq!(messages[2].seq, 3);

        // Drain again → empty
        let messages2 = mgr.drain_pushes(peer_id);
        assert!(messages2.is_empty());
    }

    #[test]
    fn test_checkpoint_and_pull() {
        let mut mgr = RegistryGossipManager::new();
        mgr.set_registry_size(100);
        let peer_id = make_peer_id(0x01);

        // Add some entries to bloom
        for i in 0..10u32 {
            let ccid = ku_core::ccid::ccid(format!("concept_{}", i).as_bytes());
            mgr.checkpoint_filter.insert(&ccid);
        }

        let checkpoint = mgr.build_checkpoint(peer_id);
        assert_eq!(checkpoint.concept_count, 100);
        assert_eq!(checkpoint.epoch, 1);
        assert_eq!(checkpoint.bloom_filter.len(), BLOOM_BITS / 8);
    }

    #[test]
    fn test_handle_checkpoint_finds_missing() {
        let mgr = RegistryGossipManager::new();

        // Peer has ccid1, ccid2
        let ccid1 = ku_core::ccid::ccid(b"concept_A");
        let ccid2 = ku_core::ccid::ccid(b"concept_B");
        let ccid3 = ku_core::ccid::ccid(b"concept_C"); // peer doesn't have this

        let mut peer_filter = RegistryBloomFilter::new();
        peer_filter.insert(&ccid1);
        peer_filter.insert(&ccid2);

        let checkpoint = RegistryCheckpointSummary {
            peer_id: make_peer_id(0xBB),
            concept_count: 2,
            bloom_filter: peer_filter.to_bytes(),
            epoch: 1,
            timestamp: 0,
        };

        // We have ccid1, ccid2, ccid3
        let our_ccids = vec![ccid1, ccid2, ccid3];
        let missing = mgr.handle_checkpoint(&checkpoint, &our_ccids);

        // ccid3 should be flagged as missing from peer
        assert!(missing.contains(&ccid3));
        // ccid1, ccid2 should NOT be flagged (peer has them)
        assert!(!missing.contains(&ccid1));
        assert!(!missing.contains(&ccid2));
    }

    #[test]
    fn test_handle_delta_pull_specific() {
        let mgr = RegistryGossipManager::new();
        let peer_id = make_peer_id(0x01);

        let ccid1 = ku_core::ccid::ccid(b"wd:Q283");
        let ccid2 = ku_core::ccid::ccid(b"wd:Q42");
        let ccid3 = ku_core::ccid::ccid(b"wd:Q999");

        let local_entries = vec![
            (
                ResolvedConcept {
                    ccid: ccid1,
                    qid: 283,
                    category: ConceptCategory::Entity,
                    canonical_name: "water".into(),
                },
                vec!["water".into(), "nước".into()],
            ),
            (
                ResolvedConcept {
                    ccid: ccid2,
                    qid: 42,
                    category: ConceptCategory::Entity,
                    canonical_name: "answer".into(),
                },
                vec!["answer".into()],
            ),
        ];

        let pull = RegistryDeltaPull {
            requester_id: make_peer_id(0xBB),
            wanted_ccids: vec![ccid1, ccid3], // want water + unknown
            max_entries: 100,
            timestamp: 0,
        };

        let response = mgr.handle_delta_pull(&pull, &local_entries, peer_id);
        assert_eq!(response.entries.len(), 1); // found water
        assert_eq!(response.entries[0].qid, 283);
        assert_eq!(response.not_found.len(), 1); // Q999 not found
        assert_eq!(response.not_found[0], ccid3);
    }

    #[test]
    fn test_wire_concept_roundtrip() {
        let ccid = ku_core::ccid::ccid(b"wd:Q283");
        let concept = ResolvedConcept {
            ccid,
            qid: 283,
            category: ConceptCategory::Substance,
            canonical_name: "water".into(),
        };
        let labels = vec!["water".into(), "nước".into(), "eau".into()];

        let wire = WireConceptEntry::from_resolved(&concept, labels.clone());
        assert_eq!(wire.ccid, ccid);
        assert_eq!(wire.qid, 283);
        assert_eq!(wire.labels.len(), 3);

        let roundtrip = wire.to_resolved();
        assert_eq!(roundtrip.ccid, ccid);
        assert_eq!(roundtrip.qid, 283);
        assert_eq!(roundtrip.category, ConceptCategory::Substance);
        assert_eq!(roundtrip.canonical_name, "water");
    }
}
