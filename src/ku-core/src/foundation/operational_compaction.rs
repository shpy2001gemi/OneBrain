//! Runtime generation fence for operational compaction.
//!
//! Compaction is opt-in and default-off. A permit is tied to the exact switch
//! generation that issued it, so a disable/re-enable cycle cannot revive stale
//! in-flight work.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use super::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};

#[derive(Debug)]
struct CompactionSwitchState {
    enabled: AtomicBool,
    generation: AtomicU64,
    commit_gate: RwLock<()>,
}

/// Node-local operational compaction kill switch.
#[derive(Clone, Debug)]
pub struct OperationalCompactionSwitch {
    state: Arc<CompactionSwitchState>,
}

impl Default for OperationalCompactionSwitch {
    fn default() -> Self {
        Self::new_disabled()
    }
}

impl OperationalCompactionSwitch {
    pub fn new_disabled() -> Self {
        Self {
            state: Arc::new(CompactionSwitchState {
                enabled: AtomicBool::new(false),
                generation: AtomicU64::new(0),
                commit_gate: RwLock::new(()),
            }),
        }
    }

    /// Enable a fresh generation. Existing permits remain fenced.
    pub fn enable(&self) -> u64 {
        let _gate = self
            .state
            .commit_gate
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = self
            .state
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.state.enabled.store(true, Ordering::Release);
        generation
    }

    /// Disable immediately and invalidate every issued permit.
    pub fn disable(&self) -> u64 {
        let _gate = self
            .state
            .commit_gate
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.state.enabled.store(false, Ordering::Release);
        self.state
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1)
    }

    pub fn is_enabled(&self) -> bool {
        self.state.enabled.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> u64 {
        self.state.generation.load(Ordering::Acquire)
    }

    pub fn acquire(&self) -> Result<OperationalCompactionPermit, CompactionFenceError> {
        let _gate = self
            .state
            .commit_gate
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.is_enabled() {
            return Err(CompactionFenceError::Disabled);
        }
        let generation = self.generation();
        if !self.is_enabled() || generation != self.generation() {
            return Err(CompactionFenceError::Stale);
        }
        Ok(OperationalCompactionPermit {
            state: Arc::clone(&self.state),
            generation,
        })
    }
}

/// Unforgeable permit for one enabled compaction generation.
#[derive(Clone, Debug)]
pub struct OperationalCompactionPermit {
    state: Arc<CompactionSwitchState>,
    generation: u64,
}

impl OperationalCompactionPermit {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self) -> bool {
        self.state.enabled.load(Ordering::Acquire)
            && self.state.generation.load(Ordering::Acquire) == self.generation
    }

    pub fn ensure_current(&self) -> Result<(), CompactionFenceError> {
        if self.is_current() {
            Ok(())
        } else {
            Err(CompactionFenceError::Stale)
        }
    }

    /// Run one commit while excluding enable/disable generation changes.
    ///
    /// Callers may prepare a transaction without the gate, but the durable
    /// commit itself must run inside this closure. Once `disable` returns, no
    /// permit from an older generation can still be committing.
    pub fn run_if_current<T>(&self, commit: impl FnOnce() -> T) -> Result<T, CompactionFenceError> {
        let _gate = self
            .state
            .commit_gate
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.ensure_current()?;
        Ok(commit())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactionFenceError {
    Disabled,
    Stale,
}

pub const TOMBSTONE_EPOCH_BLOOM_BYTES: usize = 262_144;
pub const TOMBSTONE_EPOCH_BLOOM_BITS: u64 = (TOMBSTONE_EPOCH_BLOOM_BYTES as u64) * 8;
pub const TOMBSTONE_EPOCH_BLOOM_HASHES: u8 = 8;

const TOMBSTONE_ACCUMULATOR_PROFILE: &str = "onebrain/tombstone-epoch-accumulator/1";
const TOMBSTONE_EMPTY_DOMAIN: &[u8] = b"onebrain:vnext:tombstone-empty:1\0";
const TOMBSTONE_LEAF_DOMAIN: &[u8] = b"onebrain:vnext:tombstone-leaf:1\0";
const TOMBSTONE_NODE_DOMAIN: &[u8] = b"onebrain:vnext:tombstone-node:1\0";
const TOMBSTONE_BLOOM_DOMAIN: &[u8] = b"onebrain:vnext:tombstone-bloom:1\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TombstoneEpochError {
    Canonical(CanonicalError),
    TooManyTombstones(usize),
    InvalidBloomFilterLength(usize),
    InvalidAccumulatorField(&'static str),
    NonCanonicalAccumulator,
    TombstoneNotFound,
}

impl From<CanonicalError> for TombstoneEpochError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TombstoneMerkleProof {
    pub tombstone: [u8; 32],
    /// `(sibling_is_left, sibling_hash)` from leaf level up to root.
    pub siblings: Vec<(bool, [u8; 32])>,
}

/// Deterministic epoch tombstone accumulator (`P8` §3): fixed 256 KiB (`262_144` bytes)
/// 8-hash BLAKE3 Bloom filter (zero false negatives) + exact binary Merkle root over
/// sorted unique 32-byte tombstones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TombstoneEpochAccumulatorV1 {
    pub epoch_seq: u64,
    pub cutoff_sequence_floor: u64,
    pub tombstone_count: u32,
    pub sorted_tombstone_merkle_root: [u8; 32],
    pub bloom_filter_256kb: Vec<u8>,
}

impl TombstoneEpochAccumulatorV1 {
    pub fn build(
        epoch_seq: u64,
        cutoff_sequence_floor: u64,
        tombstones: &[[u8; 32]],
    ) -> Result<Self, TombstoneEpochError> {
        let sorted: Vec<[u8; 32]> = tombstones
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let tombstone_count = u32::try_from(sorted.len())
            .map_err(|_| TombstoneEpochError::TooManyTombstones(sorted.len()))?;
        let sorted_tombstone_merkle_root = compute_tombstone_merkle_root(&sorted);
        let mut bloom_filter_256kb = vec![0u8; TOMBSTONE_EPOCH_BLOOM_BYTES];
        for tombstone in &sorted {
            for bit in bloom_bit_indices(tombstone) {
                let byte_idx = (bit / 8) as usize;
                let bit_mask = 1u8 << (bit % 8);
                bloom_filter_256kb[byte_idx] |= bit_mask;
            }
        }
        Ok(Self {
            epoch_seq,
            cutoff_sequence_floor,
            tombstone_count,
            sorted_tombstone_merkle_root,
            bloom_filter_256kb,
        })
    }

    /// Deterministic 8-hash BLAKE3 Bloom filter lookup (guaranteed zero false negatives).
    pub fn might_contain(&self, tombstone_id: &[u8; 32]) -> bool {
        if self.bloom_filter_256kb.len() != TOMBSTONE_EPOCH_BLOOM_BYTES || self.tombstone_count == 0
        {
            return false;
        }
        for bit in bloom_bit_indices(tombstone_id) {
            let byte_idx = (bit / 8) as usize;
            let bit_mask = 1u8 << (bit % 8);
            if (self.bloom_filter_256kb[byte_idx] & bit_mask) == 0 {
                return false;
            }
        }
        true
    }

    /// Exact verification against both the 256 KiB Bloom filter and the sorted Merkle root.
    pub fn verify_tombstone_inclusion(&self, proof: &TombstoneMerkleProof) -> bool {
        if self.tombstone_count == 0 || !self.might_contain(&proof.tombstone) {
            return false;
        }
        let mut acc = tombstone_leaf_hash(&proof.tombstone);
        for (sibling_is_left, sibling) in &proof.siblings {
            acc = if *sibling_is_left {
                tombstone_node_hash(sibling, &acc)
            } else {
                tombstone_node_hash(&acc, sibling)
            };
        }
        acc == self.sorted_tombstone_merkle_root
    }

    pub fn canonical_value(&self) -> Result<CanonicalValue, TombstoneEpochError> {
        self.validate_invariants()?;
        Ok(CanonicalValue::Map(vec![
            (
                0,
                CanonicalValue::Text(TOMBSTONE_ACCUMULATOR_PROFILE.to_string()),
            ),
            (1, CanonicalValue::Unsigned(self.epoch_seq)),
            (2, CanonicalValue::Unsigned(self.cutoff_sequence_floor)),
            (3, CanonicalValue::Unsigned(u64::from(self.tombstone_count))),
            (
                4,
                CanonicalValue::Bytes(self.sorted_tombstone_merkle_root.to_vec()),
            ),
            (5, CanonicalValue::Bytes(self.bloom_filter_256kb.clone())),
        ]))
    }

    pub fn encode_bytes(&self) -> Result<Vec<u8>, TombstoneEpochError> {
        encode_canonical(&self.canonical_value()?, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, TombstoneEpochError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(map) = &value else {
            return Err(TombstoneEpochError::InvalidAccumulatorField("root"));
        };
        let find = |key: u64, name: &'static str| {
            map.iter()
                .find_map(|(k, v)| (*k == key).then_some(v))
                .ok_or(TombstoneEpochError::InvalidAccumulatorField(name))
        };
        let CanonicalValue::Text(profile) = find(0, "profile")? else {
            return Err(TombstoneEpochError::InvalidAccumulatorField("profile"));
        };
        if profile != TOMBSTONE_ACCUMULATOR_PROFILE {
            return Err(TombstoneEpochError::InvalidAccumulatorField("profile"));
        }
        let CanonicalValue::Unsigned(epoch_seq) = find(1, "epoch_seq")? else {
            return Err(TombstoneEpochError::InvalidAccumulatorField("epoch_seq"));
        };
        let CanonicalValue::Unsigned(cutoff_sequence_floor) = find(2, "cutoff_sequence_floor")?
        else {
            return Err(TombstoneEpochError::InvalidAccumulatorField(
                "cutoff_sequence_floor",
            ));
        };
        let CanonicalValue::Unsigned(count_u64) = find(3, "tombstone_count")? else {
            return Err(TombstoneEpochError::InvalidAccumulatorField(
                "tombstone_count",
            ));
        };
        let tombstone_count = u32::try_from(*count_u64)
            .map_err(|_| TombstoneEpochError::InvalidAccumulatorField("tombstone_count"))?;
        let CanonicalValue::Bytes(root_bytes) = find(4, "merkle_root")? else {
            return Err(TombstoneEpochError::InvalidAccumulatorField("merkle_root"));
        };
        let sorted_tombstone_merkle_root: [u8; 32] = root_bytes
            .as_slice()
            .try_into()
            .map_err(|_| TombstoneEpochError::InvalidAccumulatorField("merkle_root"))?;
        let CanonicalValue::Bytes(bloom_bytes) = find(5, "bloom_filter_256kb")? else {
            return Err(TombstoneEpochError::InvalidAccumulatorField(
                "bloom_filter_256kb",
            ));
        };
        let decoded = Self {
            epoch_seq: *epoch_seq,
            cutoff_sequence_floor: *cutoff_sequence_floor,
            tombstone_count,
            sorted_tombstone_merkle_root,
            bloom_filter_256kb: bloom_bytes.clone(),
        };
        decoded.validate_invariants()?;
        if decoded.canonical_value()? != value {
            return Err(TombstoneEpochError::NonCanonicalAccumulator);
        }
        Ok(decoded)
    }

    fn validate_invariants(&self) -> Result<(), TombstoneEpochError> {
        if self.bloom_filter_256kb.len() != TOMBSTONE_EPOCH_BLOOM_BYTES {
            return Err(TombstoneEpochError::InvalidBloomFilterLength(
                self.bloom_filter_256kb.len(),
            ));
        }
        if self.tombstone_count == 0 {
            if self.sorted_tombstone_merkle_root != empty_tombstone_merkle_root()
                || self.bloom_filter_256kb.iter().any(|b| *b != 0)
            {
                return Err(TombstoneEpochError::InvalidAccumulatorField(
                    "empty_epoch_invariants",
                ));
            }
        } else if self.sorted_tombstone_merkle_root == [0; 32]
            || self.bloom_filter_256kb.iter().all(|b| *b == 0)
        {
            return Err(TombstoneEpochError::InvalidAccumulatorField(
                "non_empty_epoch_invariants",
            ));
        }
        Ok(())
    }
}

/// Computes the deterministic binary Merkle root over sorted unique `[u8; 32]` tombstones.
pub fn compute_tombstone_merkle_root(tombstones: &[[u8; 32]]) -> [u8; 32] {
    let sorted: Vec<[u8; 32]> = tombstones
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if sorted.is_empty() {
        return empty_tombstone_merkle_root();
    }
    let mut level: Vec<[u8; 32]> = sorted.iter().map(tombstone_leaf_hash).collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut idx = 0usize;
        while idx < level.len() {
            if idx + 1 < level.len() {
                next.push(tombstone_node_hash(&level[idx], &level[idx + 1]));
            } else {
                next.push(level[idx]);
            }
            idx += 2;
        }
        level = next;
    }
    level[0]
}

/// Builds a Merkle inclusion proof for `target` within the sorted unique tombstone set.
pub fn prove_tombstone_inclusion(
    tombstones: &[[u8; 32]],
    target: &[u8; 32],
) -> Result<TombstoneMerkleProof, TombstoneEpochError> {
    let sorted: Vec<[u8; 32]> = tombstones
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut pos = sorted
        .binary_search(target)
        .map_err(|_| TombstoneEpochError::TombstoneNotFound)?;
    let mut level: Vec<[u8; 32]> = sorted.iter().map(tombstone_leaf_hash).collect();
    let mut siblings = Vec::new();
    while level.len() > 1 {
        if pos % 2 == 1 {
            siblings.push((true, level[pos - 1]));
        } else if pos + 1 < level.len() {
            siblings.push((false, level[pos + 1]));
        }
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut idx = 0usize;
        while idx < level.len() {
            if idx + 1 < level.len() {
                next.push(tombstone_node_hash(&level[idx], &level[idx + 1]));
            } else {
                next.push(level[idx]);
            }
            idx += 2;
        }
        pos /= 2;
        level = next;
    }
    Ok(TombstoneMerkleProof {
        tombstone: *target,
        siblings,
    })
}

fn empty_tombstone_merkle_root() -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TOMBSTONE_EMPTY_DOMAIN);
    *hasher.finalize().as_bytes()
}

fn tombstone_leaf_hash(tombstone: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TOMBSTONE_LEAF_DOMAIN);
    hasher.update(tombstone);
    *hasher.finalize().as_bytes()
}

fn tombstone_node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TOMBSTONE_NODE_DOMAIN);
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}

fn bloom_bit_indices(tombstone: &[u8; 32]) -> [u64; TOMBSTONE_EPOCH_BLOOM_HASHES as usize] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(TOMBSTONE_BLOOM_DOMAIN);
    hasher.update(tombstone);
    let mut stream = [0u8; 64];
    hasher.finalize_xof().fill(&mut stream);
    let mut indices = [0u64; TOMBSTONE_EPOCH_BLOOM_HASHES as usize];
    for (k, slot) in indices.iter_mut().enumerate() {
        let mut chunk = [0u8; 8];
        chunk.copy_from_slice(&stream[k * 8..(k + 1) * 8]);
        // Since TOMBSTONE_EPOCH_BLOOM_BITS = 2_097_152 = 2^21, modulo is exact bit-masking.
        *slot = u64::from_le_bytes(chunk) % TOMBSTONE_EPOCH_BLOOM_BITS;
    }
    indices
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn switch_is_default_off_and_stale_permits_never_revive() {
        let switch = OperationalCompactionSwitch::default();
        assert_eq!(
            switch.acquire().unwrap_err(),
            CompactionFenceError::Disabled
        );

        let first_generation = switch.enable();
        let first = switch.acquire().unwrap();
        assert_eq!(first.generation(), first_generation);
        assert!(first.is_current());

        switch.disable();
        assert_eq!(first.ensure_current(), Err(CompactionFenceError::Stale));
        switch.enable();
        assert_eq!(first.ensure_current(), Err(CompactionFenceError::Stale));
        assert!(switch.acquire().unwrap().is_current());
    }

    #[test]
    fn disable_waits_for_the_current_commit_gate_then_fences_the_permit() {
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        let later_check = permit.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let commit = thread::spawn(move || {
            permit
                .run_if_current(|| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                })
                .unwrap();
        });
        entered_rx.recv().unwrap();

        let disabling_switch = switch.clone();
        let (attempt_tx, attempt_rx) = mpsc::channel();
        let (disabled_tx, disabled_rx) = mpsc::channel();
        let disable = thread::spawn(move || {
            attempt_tx.send(()).unwrap();
            let generation = disabling_switch.disable();
            disabled_tx.send(generation).unwrap();
        });
        attempt_rx.recv().unwrap();
        assert!(disabled_rx.recv_timeout(Duration::from_millis(50)).is_err());

        release_tx.send(()).unwrap();
        commit.join().unwrap();
        assert!(disabled_rx.recv_timeout(Duration::from_secs(1)).is_ok());
        disable.join().unwrap();
        assert_eq!(
            later_check.ensure_current(),
            Err(CompactionFenceError::Stale)
        );
    }

    #[test]
    fn tombstone_epoch_accumulator_v1_zero_false_negatives_merkle_proof_and_cbor_roundtrip() {
        let mut tombstones = Vec::with_capacity(512);
        for i in 0u32..512 {
            let mut h = blake3::Hasher::new();
            h.update(b"test-tombstone:");
            h.update(&i.to_le_bytes());
            tombstones.push(*h.finalize().as_bytes());
        }

        let acc = TombstoneEpochAccumulatorV1::build(3, 1_024, &tombstones).unwrap();
        assert_eq!(acc.epoch_seq, 3);
        assert_eq!(acc.cutoff_sequence_floor, 1_024);
        assert_eq!(acc.tombstone_count, 512);
        assert_eq!(acc.bloom_filter_256kb.len(), TOMBSTONE_EPOCH_BLOOM_BYTES);

        // Zero false negatives across all inserted tombstones + valid Merkle proofs.
        for item in &tombstones {
            assert!(acc.might_contain(item));
            let proof = prove_tombstone_inclusion(&tombstones, item).unwrap();
            assert!(acc.verify_tombstone_inclusion(&proof));
        }

        // Tampered proof fails closed.
        let mut bad_proof = prove_tombstone_inclusion(&tombstones, &tombstones[17]).unwrap();
        bad_proof.siblings[0].1[0] ^= 0xff;
        assert!(!acc.verify_tombstone_inclusion(&bad_proof));

        // Canonical CBOR round-trip preserves exact bytes and fields.
        let encoded = acc.encode_bytes().unwrap();
        let decoded = TombstoneEpochAccumulatorV1::decode_bytes(&encoded).unwrap();
        assert_eq!(decoded, acc);

        // Empty epoch accumulator round-trips cleanly too.
        let empty = TombstoneEpochAccumulatorV1::build(0, 0, &[]).unwrap();
        assert!(!empty.might_contain(&[1; 32]));
        assert_eq!(
            TombstoneEpochAccumulatorV1::decode_bytes(&empty.encode_bytes().unwrap()).unwrap(),
            empty
        );
    }
}
