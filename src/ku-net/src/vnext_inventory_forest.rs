//! Selector-scoped deterministic inventory forest for OBP-RP vNext.

use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "persist")]
use ku_core::foundation::dr_m5_failpoint;
use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, CheckpointCid, EventCid,
    FeedId, InventoryRecordKind, ResourceProfile, SelectorCid,
};

pub const INVENTORY_FOREST_PROFILE_MAJOR: u64 = 1;
pub const INVENTORY_FOREST_PROFILE_MINOR: u64 = 0;
pub const MAX_INVENTORY_RECORDS: usize = 65_536;
pub const MAX_FEED_PREFIXES: usize = 65_536;
pub const MAX_CHECKPOINT_REFS: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryLeaf {
    pub record_kind: InventoryRecordKind,
    pub cid: [u8; 32],
    pub canonical_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedPrefixInventory {
    pub feed: FeedId,
    pub through_sequence: u64,
    pub head_event: EventCid,
    /// Present in v1 even when no checkpoint is known.
    pub checkpoint_frontier_refs: Vec<CheckpointCid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InventoryRange {
    pub prefix_bits: u16,
    pub prefix: [u8; 32],
}

impl InventoryRange {
    pub fn new(prefix_bits: u16, mut prefix: [u8; 32]) -> Result<Self, InventoryForestError> {
        if prefix_bits > 256 {
            return Err(InventoryForestError::InvalidPrefix);
        }
        mask_after(&mut prefix, prefix_bits);
        Ok(Self {
            prefix_bits,
            prefix,
        })
    }

    pub fn contains(self, cid: &[u8; 32]) -> bool {
        common_prefix_bits(&self.prefix, cid) >= self.prefix_bits
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeSummary {
    pub record_kind: InventoryRecordKind,
    pub range: InventoryRange,
    pub root: [u8; 32],
    pub record_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DivergentPrefix {
    pub record_kind: InventoryRecordKind,
    pub range: InventoryRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticShardHint {
    pub source_root: [u8; 32],
    pub projection_root: [u8; 32],
    pub index_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryCoverageAssessment {
    pub selector: SelectorCid,
    pub exact_root_match: bool,
    pub unknown_checkpoint_refs: Vec<CheckpointCid>,
}

impl InventoryCoverageAssessment {
    pub fn complete_within_selector(&self) -> bool {
        self.exact_root_match && self.unknown_checkpoint_refs.is_empty()
    }

    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InventoryInsertOutcome {
    Added,
    ExactReplay,
}

#[derive(Clone, Debug)]
pub struct HybridInventoryForest {
    selector: SelectorCid,
    records: BTreeMap<(u64, [u8; 32]), InventoryLeaf>,
    /// Multiple heads at one sequence are retained; arrival order cannot win.
    feed_prefixes: BTreeMap<([u8; 32], u64, [u8; 32]), FeedPrefixInventory>,
    /// Derived only and deliberately excluded from authoritative roots/snapshots.
    semantic_shards: BTreeMap<[u8; 32], SemanticShardHint>,
}

impl HybridInventoryForest {
    pub fn new(selector: SelectorCid) -> Self {
        Self {
            selector,
            records: BTreeMap::new(),
            feed_prefixes: BTreeMap::new(),
            semantic_shards: BTreeMap::new(),
        }
    }

    pub const fn selector(&self) -> SelectorCid {
        self.selector
    }

    pub fn insert_record(
        &mut self,
        leaf: InventoryLeaf,
    ) -> Result<InventoryInsertOutcome, InventoryForestError> {
        if leaf.canonical_length == 0 {
            return Err(InventoryForestError::InvalidLength);
        }
        let key = (kind_code(leaf.record_kind), leaf.cid);
        match self.records.get(&key) {
            Some(existing) if existing == &leaf => Ok(InventoryInsertOutcome::ExactReplay),
            Some(_) => Err(InventoryForestError::CidCollision),
            None if self.records.len() >= MAX_INVENTORY_RECORDS => Err(InventoryForestError::Limit),
            None => {
                self.records.insert(key, leaf);
                Ok(InventoryInsertOutcome::Added)
            }
        }
    }

    pub fn insert_feed_prefix(
        &mut self,
        mut prefix: FeedPrefixInventory,
    ) -> Result<InventoryInsertOutcome, InventoryForestError> {
        if prefix.checkpoint_frontier_refs.len() > MAX_CHECKPOINT_REFS {
            return Err(InventoryForestError::Limit);
        }
        prefix
            .checkpoint_frontier_refs
            .sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        let before = prefix.checkpoint_frontier_refs.len();
        prefix.checkpoint_frontier_refs.dedup();
        if before != prefix.checkpoint_frontier_refs.len() {
            return Err(InventoryForestError::DuplicateCheckpointReference);
        }
        let key = (
            prefix.feed.into_bytes(),
            prefix.through_sequence,
            prefix.head_event.into_bytes(),
        );
        match self.feed_prefixes.get(&key) {
            Some(existing) if existing == &prefix => Ok(InventoryInsertOutcome::ExactReplay),
            Some(_) => Err(InventoryForestError::FeedPrefixConflict),
            None if self.feed_prefixes.len() >= MAX_FEED_PREFIXES => {
                Err(InventoryForestError::Limit)
            }
            None => {
                self.feed_prefixes.insert(key, prefix);
                Ok(InventoryInsertOutcome::Added)
            }
        }
    }

    pub fn set_semantic_shard(&mut self, key: [u8; 32], hint: SemanticShardHint) {
        self.semantic_shards.insert(key, hint);
    }

    pub fn semantic_shard(&self, key: &[u8; 32]) -> Option<&SemanticShardHint> {
        self.semantic_shards.get(key)
    }

    pub fn root(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:inventory-forest-root:1\0");
        hasher.update(self.selector.as_bytes());
        for kind in all_kinds() {
            hasher.update(&kind_code(kind).to_be_bytes());
            hasher.update(&radix_root(&self.records_for(kind, None), 0));
        }
        for prefix in self.feed_prefixes.values() {
            hasher.update(&feed_digest(prefix));
        }
        *hasher.finalize().as_bytes()
    }

    pub fn range_summary(
        &self,
        record_kind: InventoryRecordKind,
        range: InventoryRange,
    ) -> RangeSummary {
        let leaves = self.records_for(record_kind, Some(range));
        RangeSummary {
            record_kind,
            range,
            root: radix_root(&leaves, range.prefix_bits),
            record_count: leaves.len() as u64,
        }
    }

    /// Canonically ordered manifest projection for one authoritative radix
    /// lane/range. It exposes no semantic shard hints or local-only records.
    pub fn records_in_range(
        &self,
        record_kind: InventoryRecordKind,
        range: InventoryRange,
    ) -> Vec<InventoryLeaf> {
        self.records_for(record_kind, Some(range))
    }

    pub fn first_divergent_prefix(
        &self,
        other: &Self,
    ) -> Result<Option<DivergentPrefix>, InventoryForestError> {
        if self.selector != other.selector {
            return Err(InventoryForestError::SelectorMismatch);
        }
        for kind in all_kinds() {
            let left = self.records_for(kind, None);
            let right = other.records_for(kind, None);
            // Both projections are exact canonical leaf sequences. Comparing
            // them directly avoids recomputing Merkle subtrees at every
            // descent while preserving the same lexicographic divergence.
            if left != right {
                let (bits, prefix) = divergent_prefix(&left, &right, 0, [0; 32]);
                return Ok(Some(DivergentPrefix {
                    record_kind: kind,
                    range: InventoryRange::new(bits, prefix)?,
                }));
            }
        }
        if self.feed_prefixes != other.feed_prefixes {
            // A zero-bit Event lane requests the complete feed-prefix summary.
            return Ok(Some(DivergentPrefix {
                record_kind: InventoryRecordKind::Event,
                range: InventoryRange::new(0, [0; 32])?,
            }));
        }
        Ok(None)
    }

    pub fn assess_coverage(
        &self,
        remote_root: [u8; 32],
        known_checkpoints: &BTreeSet<[u8; 32]>,
    ) -> InventoryCoverageAssessment {
        let mut unknown = self
            .feed_prefixes
            .values()
            .flat_map(|prefix| &prefix.checkpoint_frontier_refs)
            .filter(|checkpoint| !known_checkpoints.contains(checkpoint.as_bytes()))
            .copied()
            .collect::<Vec<_>>();
        unknown.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        unknown.dedup();
        InventoryCoverageAssessment {
            selector: self.selector,
            exact_root_match: self.root() == remote_root,
            unknown_checkpoint_refs: unknown,
        }
    }

    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, InventoryForestError> {
        let records = self
            .records
            .values()
            .map(|leaf| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(kind_code(leaf.record_kind))),
                    (1, CanonicalValue::Bytes(leaf.cid.to_vec())),
                    (2, CanonicalValue::Unsigned(leaf.canonical_length)),
                ])
            })
            .collect();
        let feeds = self
            .feed_prefixes
            .values()
            .map(|prefix| {
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Bytes(prefix.feed.as_bytes().to_vec())),
                    (1, CanonicalValue::Unsigned(prefix.through_sequence)),
                    (
                        2,
                        CanonicalValue::Bytes(prefix.head_event.as_bytes().to_vec()),
                    ),
                    (
                        3,
                        CanonicalValue::Array(
                            prefix
                                .checkpoint_frontier_refs
                                .iter()
                                .map(|cid| CanonicalValue::Bytes(cid.as_bytes().to_vec()))
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect();
        encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(INVENTORY_FOREST_PROFILE_MAJOR)),
                (1, CanonicalValue::Unsigned(INVENTORY_FOREST_PROFILE_MINOR)),
                (2, CanonicalValue::Bytes(self.selector.as_bytes().to_vec())),
                (3, CanonicalValue::Array(records)),
                (4, CanonicalValue::Array(feeds)),
            ]),
            ResourceProfile::ManifestV1,
        )
        .map_err(Into::into)
    }

    pub fn restore(bytes: &[u8]) -> Result<Self, InventoryForestError> {
        let value = decode_canonical(bytes, ResourceProfile::ManifestV1)?;
        let root = as_map(&value, "snapshot")?;
        if unsigned(root, 0, "major")? != INVENTORY_FOREST_PROFILE_MAJOR {
            return Err(InventoryForestError::UnsupportedVersion);
        }
        let mut forest = Self::new(SelectorCid::from_bytes(bytes32(root, 2, "selector")?));
        for value in array(root, 3, "records")? {
            let record = as_map(value, "record")?;
            forest.insert_record(InventoryLeaf {
                record_kind: parse_kind(unsigned(record, 0, "record.kind")?)?,
                cid: bytes32(record, 1, "record.cid")?,
                canonical_length: unsigned(record, 2, "record.length")?,
            })?;
        }
        for value in array(root, 4, "feeds")? {
            let feed = as_map(value, "feed")?;
            let checkpoints = array(feed, 3, "feed.checkpoints")?
                .iter()
                .map(|value| bytes32_value(value, "checkpoint").map(CheckpointCid::from_bytes))
                .collect::<Result<Vec<_>, _>>()?;
            forest.insert_feed_prefix(FeedPrefixInventory {
                feed: FeedId::from_bytes(bytes32(feed, 0, "feed.id")?),
                through_sequence: unsigned(feed, 1, "feed.sequence")?,
                head_event: EventCid::from_bytes(bytes32(feed, 2, "feed.head")?),
                checkpoint_frontier_refs: checkpoints,
            })?;
        }
        if forest.snapshot_bytes()? != bytes {
            return Err(InventoryForestError::NonCanonicalSnapshot);
        }
        Ok(forest)
    }

    fn records_for(
        &self,
        kind: InventoryRecordKind,
        range: Option<InventoryRange>,
    ) -> Vec<InventoryLeaf> {
        self.records
            .range((kind_code(kind), [0; 32])..=(kind_code(kind), [0xff; 32]))
            .map(|(_, leaf)| *leaf)
            .filter(|leaf| range.is_none_or(|range| range.contains(&leaf.cid)))
            .collect()
    }
}

fn radix_root(leaves: &[InventoryLeaf], depth: u16) -> [u8; 32] {
    if leaves.is_empty() {
        return empty_hash(depth);
    }
    if depth == 256 {
        return leaf_hash(leaves[0]);
    }
    let split = leaves.partition_point(|leaf| !bit(&leaf.cid, depth));
    branch_hash(
        depth,
        radix_root(&leaves[..split], depth + 1),
        radix_root(&leaves[split..], depth + 1),
    )
}

fn divergent_prefix(
    left: &[InventoryLeaf],
    right: &[InventoryLeaf],
    depth: u16,
    mut prefix: [u8; 32],
) -> (u16, [u8; 32]) {
    if depth == 256 || left.is_empty() || right.is_empty() {
        return (depth, prefix);
    }
    let left_split = left.partition_point(|leaf| !bit(&leaf.cid, depth));
    let right_split = right.partition_point(|leaf| !bit(&leaf.cid, depth));
    if left[..left_split] != right[..right_split] {
        set_bit(&mut prefix, depth, false);
        return divergent_prefix(
            &left[..left_split],
            &right[..right_split],
            depth + 1,
            prefix,
        );
    }
    set_bit(&mut prefix, depth, true);
    divergent_prefix(
        &left[left_split..],
        &right[right_split..],
        depth + 1,
        prefix,
    )
}

fn leaf_hash(leaf: InventoryLeaf) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:inventory-leaf:1\0");
    hasher.update(&kind_code(leaf.record_kind).to_be_bytes());
    hasher.update(&leaf.cid);
    hasher.update(&leaf.canonical_length.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn empty_hash(depth: u16) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:inventory-empty:1\0");
    hasher.update(&depth.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn branch_hash(depth: u16, left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:inventory-branch:1\0");
    hasher.update(&depth.to_be_bytes());
    hasher.update(&left);
    hasher.update(&right);
    *hasher.finalize().as_bytes()
}

fn feed_digest(prefix: &FeedPrefixInventory) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:feed-prefix-inventory:1\0");
    hasher.update(prefix.feed.as_bytes());
    hasher.update(&prefix.through_sequence.to_be_bytes());
    hasher.update(prefix.head_event.as_bytes());
    for checkpoint in &prefix.checkpoint_frontier_refs {
        hasher.update(checkpoint.as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn bit(cid: &[u8; 32], index: u16) -> bool {
    cid[(index / 8) as usize] & (0x80 >> (index % 8)) != 0
}

fn set_bit(prefix: &mut [u8; 32], index: u16, value: bool) {
    let mask = 0x80 >> (index % 8);
    let byte = &mut prefix[(index / 8) as usize];
    if value {
        *byte |= mask;
    } else {
        *byte &= !mask;
    }
}

fn mask_after(prefix: &mut [u8; 32], bits: u16) {
    if bits == 256 {
        return;
    }
    let full = (bits / 8) as usize;
    let remainder = bits % 8;
    if remainder == 0 {
        prefix[full..].fill(0);
    } else {
        prefix[full] &= 0xff << (8 - remainder);
        prefix[(full + 1)..].fill(0);
    }
}

fn common_prefix_bits(left: &[u8; 32], right: &[u8; 32]) -> u16 {
    let mut count = 0;
    for (left, right) in left.iter().zip(right) {
        let difference = left ^ right;
        if difference == 0 {
            count += 8;
        } else {
            count += difference.leading_zeros() as u16;
            break;
        }
    }
    count
}

const fn kind_code(kind: InventoryRecordKind) -> u64 {
    kind as u64
}

fn parse_kind(value: u64) -> Result<InventoryRecordKind, InventoryForestError> {
    match value {
        0 => Ok(InventoryRecordKind::Object),
        1 => Ok(InventoryRecordKind::Event),
        2 => Ok(InventoryRecordKind::MappingKernel),
        3 => Ok(InventoryRecordKind::FeedInception),
        4 => Ok(InventoryRecordKind::AuthorityEvent),
        _ => Err(InventoryForestError::InvalidRecordKind),
    }
}

const fn all_kinds() -> [InventoryRecordKind; 5] {
    [
        InventoryRecordKind::Object,
        InventoryRecordKind::Event,
        InventoryRecordKind::MappingKernel,
        InventoryRecordKind::FeedInception,
        InventoryRecordKind::AuthorityEvent,
    ]
}

fn as_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], InventoryForestError> {
    match value {
        CanonicalValue::Map(map) => Ok(map),
        _ => Err(InventoryForestError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, InventoryForestError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(InventoryForestError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, InventoryForestError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(InventoryForestError::InvalidField(field)),
    }
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], InventoryForestError> {
    match required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(InventoryForestError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], InventoryForestError> {
    bytes32_value(required(map, key, field)?, field)
}

fn bytes32_value(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 32], InventoryForestError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(InventoryForestError::InvalidField(field));
    };
    if bytes.len() != 32 {
        return Err(InventoryForestError::InvalidField(field));
    }
    let mut result = [0; 32];
    result.copy_from_slice(bytes);
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryForestError {
    Canonical(CanonicalError),
    Backend(String),
    InvalidField(&'static str),
    InvalidPrefix,
    InvalidLength,
    InvalidRecordKind,
    UnsupportedVersion,
    NonCanonicalSnapshot,
    Limit,
    CidCollision,
    FeedPrefixConflict,
    DuplicateCheckpointReference,
    SelectorMismatch,
}

impl From<CanonicalError> for InventoryForestError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(feature = "persist")]
pub mod persistent {
    use std::path::Path;
    use std::sync::Arc;

    use redb::{Database, ReadableTable, TableDefinition};

    use super::*;

    const FORESTS: TableDefinition<&[u8], &[u8]> =
        TableDefinition::new("vnext_selector_inventory_forests");

    /// ACID snapshot store for selector-scoped authoritative inventory.
    /// Semantic shard hints are derived and therefore intentionally omitted by
    /// `HybridInventoryForest::snapshot_bytes`.
    #[derive(Clone)]
    pub struct RedbInventoryForestBackend {
        db: Arc<Database>,
    }

    impl RedbInventoryForestBackend {
        pub fn open(path: &Path) -> Result<Self, InventoryForestError> {
            let db = Database::create(path)
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            let write = db
                .begin_write()
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            {
                write
                    .open_table(FORESTS)
                    .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            }
            write
                .commit()
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            Ok(Self { db: Arc::new(db) })
        }

        pub fn load(
            &self,
            selector: SelectorCid,
        ) -> Result<HybridInventoryForest, InventoryForestError> {
            let read = self
                .db
                .begin_read()
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            let table = read
                .open_table(FORESTS)
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            let bytes = table
                .get(selector.as_bytes().as_slice())
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?
                .map(|guard| guard.value().to_vec());
            match bytes {
                Some(bytes) => HybridInventoryForest::restore(&bytes),
                None => Ok(HybridInventoryForest::new(selector)),
            }
        }

        /// Atomically read, update, and replace one selector snapshot. Redb
        /// serializes writers, preventing concurrent sessions from losing a
        /// leaf through read-modify-write races.
        pub fn insert_record(
            &self,
            selector: SelectorCid,
            leaf: InventoryLeaf,
        ) -> Result<InventoryInsertOutcome, InventoryForestError> {
            dr_m5_failpoint::hit("TX-INV-001", "before_begin_write");
            let write = self
                .db
                .begin_write()
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            dr_m5_failpoint::hit("TX-INV-001", "after_begin_write_before_mutation");
            let outcome;
            {
                let mut table = write
                    .open_table(FORESTS)
                    .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
                let bytes = table
                    .get(selector.as_bytes().as_slice())
                    .map_err(|error| InventoryForestError::Backend(error.to_string()))?
                    .map(|guard| guard.value().to_vec());
                let mut forest = match bytes {
                    Some(bytes) => HybridInventoryForest::restore(&bytes)?,
                    None => HybridInventoryForest::new(selector),
                };
                outcome = forest.insert_record(leaf)?;
                let snapshot = forest.snapshot_bytes()?;
                table
                    .insert(selector.as_bytes().as_slice(), snapshot.as_slice())
                    .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            }
            dr_m5_failpoint::hit("TX-INV-001", "after_mutation_before_commit");
            write
                .commit()
                .map_err(|error| InventoryForestError::Backend(error.to_string()))?;
            dr_m5_failpoint::hit("TX-INV-001", "after_commit_before_next_side_effect");
            dr_m5_failpoint::hit("TX-INV-001", "after_next_side_effect_before_ack");
            Ok(outcome)
        }
    }
}

#[cfg(feature = "persist")]
pub use persistent::RedbInventoryForestBackend;

#[cfg(test)]
mod tests {
    use super::*;

    fn selector(byte: u8) -> SelectorCid {
        SelectorCid::from_bytes([byte; 32])
    }

    fn leaf(kind: InventoryRecordKind, byte: u8) -> InventoryLeaf {
        InventoryLeaf {
            record_kind: kind,
            cid: [byte; 32],
            canonical_length: 100 + u64::from(byte),
        }
    }

    fn feed() -> FeedPrefixInventory {
        FeedPrefixInventory {
            feed: FeedId::from_bytes([7; 32]),
            through_sequence: 10,
            head_event: EventCid::from_bytes([8; 32]),
            checkpoint_frontier_refs: vec![CheckpointCid::from_bytes([9; 32])],
        }
    }

    #[test]
    fn root_is_order_independent_and_survives_restart() {
        let mut left = HybridInventoryForest::new(selector(1));
        left.insert_record(leaf(InventoryRecordKind::Object, 0x10))
            .unwrap();
        left.insert_record(leaf(InventoryRecordKind::Event, 0x80))
            .unwrap();
        left.insert_feed_prefix(feed()).unwrap();
        let mut right = HybridInventoryForest::new(selector(1));
        right.insert_feed_prefix(feed()).unwrap();
        right
            .insert_record(leaf(InventoryRecordKind::Event, 0x80))
            .unwrap();
        right
            .insert_record(leaf(InventoryRecordKind::Object, 0x10))
            .unwrap();
        assert_eq!(left.root(), right.root());
        let bytes = left.snapshot_bytes().unwrap();
        let restarted = HybridInventoryForest::restore(&bytes).unwrap();
        assert_eq!(left.root(), restarted.root());
        assert_eq!(bytes, restarted.snapshot_bytes().unwrap());
    }

    #[test]
    fn exact_divergent_child_prefix_is_returned() {
        let mut left = HybridInventoryForest::new(selector(1));
        let mut right = HybridInventoryForest::new(selector(1));
        left.insert_record(leaf(InventoryRecordKind::Object, 0x10))
            .unwrap();
        right
            .insert_record(leaf(InventoryRecordKind::Object, 0x30))
            .unwrap();
        let divergence = left.first_divergent_prefix(&right).unwrap().unwrap();
        assert_eq!(divergence.record_kind, InventoryRecordKind::Object);
        assert_eq!(divergence.range.prefix_bits, 3);
        assert_eq!(
            left.range_summary(InventoryRecordKind::Object, divergence.range)
                .record_count,
            1
        );
    }

    #[test]
    fn semantic_shard_is_derived_and_not_root_authority() {
        let mut forest = HybridInventoryForest::new(selector(1));
        forest
            .insert_record(leaf(InventoryRecordKind::Object, 1))
            .unwrap();
        let root = forest.root();
        forest.set_semantic_shard(
            [2; 32],
            SemanticShardHint {
                source_root: root,
                projection_root: [3; 32],
                index_version: 1,
            },
        );
        assert_eq!(forest.root(), root);
        let restarted = HybridInventoryForest::restore(&forest.snapshot_bytes().unwrap()).unwrap();
        assert!(restarted.semantic_shard(&[2; 32]).is_none());
        assert_eq!(restarted.root(), root);
    }

    #[test]
    fn unknown_checkpoint_never_establishes_completion() {
        let mut forest = HybridInventoryForest::new(selector(1));
        forest.insert_feed_prefix(feed()).unwrap();
        let root = forest.root();
        let unknown = forest.assess_coverage(root, &BTreeSet::new());
        assert!(!unknown.complete_within_selector());
        assert!(!unknown.is_globally_complete());
        assert_eq!(unknown.unknown_checkpoint_refs.len(), 1);
        let known = BTreeSet::from([[9; 32]]);
        assert!(forest
            .assess_coverage(root, &known)
            .complete_within_selector());
    }

    #[test]
    fn same_cid_with_different_length_is_collision() {
        let mut forest = HybridInventoryForest::new(selector(1));
        let first = leaf(InventoryRecordKind::Object, 1);
        forest.insert_record(first).unwrap();
        let mut changed = first;
        changed.canonical_length += 1;
        assert_eq!(
            forest.insert_record(changed).unwrap_err(),
            InventoryForestError::CidCollision
        );
    }

    #[cfg(feature = "persist")]
    #[test]
    fn selector_inventory_survives_redb_restart_and_remains_isolated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.redb");
        let first_selector = selector(1);
        let second_selector = selector(2);
        let first_leaf = leaf(InventoryRecordKind::Object, 0x11);
        let first_root = {
            let backend = RedbInventoryForestBackend::open(&path).unwrap();
            assert_eq!(
                backend.insert_record(first_selector, first_leaf).unwrap(),
                InventoryInsertOutcome::Added
            );
            assert_eq!(
                backend.insert_record(first_selector, first_leaf).unwrap(),
                InventoryInsertOutcome::ExactReplay
            );
            assert_eq!(
                backend
                    .load(second_selector)
                    .unwrap()
                    .range_summary(
                        InventoryRecordKind::Object,
                        InventoryRange::new(0, [0; 32]).unwrap()
                    )
                    .record_count,
                0
            );
            backend.load(first_selector).unwrap().root()
        };
        let reopened = RedbInventoryForestBackend::open(&path).unwrap();
        let forest = reopened.load(first_selector).unwrap();
        assert_eq!(forest.root(), first_root);
        assert_eq!(
            forest
                .range_summary(
                    InventoryRecordKind::Object,
                    InventoryRange::new(0, [0; 32]).unwrap()
                )
                .record_count,
            1
        );
    }
}
