//! Durable, non-authoritative observations of which authenticated peer
//! delivered a validated record under an exact selector.

#![cfg(feature = "vnext-network-runtime")]

use std::path::Path;
use std::sync::Arc;

use ku_core::foundation::{NodeId, SelectorCid};
use onebrain_protocol::ReconcileManifestKind;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::archive::{PortableArchiveRow, PortableArchiveRows};
use crate::error::NodeError;
use onebrain_archive::{ArchiveEntryKind, ArchiveOwner};

const OBSERVATIONS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_record_source_observations_v1");
const TYPED_RECORDS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_selector_typed_records_v1");
const TYPED_RECORD_KEYS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_selector_typed_record_keys_v1");
const TYPED_RECORD_HEADS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_selector_typed_record_heads_v1");
const TYPED_RECORD_PEERS: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_selector_typed_record_peers_v1");
const KEY_BYTES: usize = 8 + 32 + 32 + 32;
const TYPED_PREFIX_BYTES: usize = 32 + 8 + 8;
const TYPED_RECORD_KEY_BYTES: usize = TYPED_PREFIX_BYTES + 8;
const TYPED_LOOKUP_KEY_BYTES: usize = TYPED_PREFIX_BYTES + 32;
const TYPED_PEER_KEY_BYTES: usize = TYPED_LOOKUP_KEY_BYTES + 32;
pub const MAX_PROVENANCE_OBSERVATIONS: u64 = 262_144;
pub const MAX_TYPED_PROVENANCE_RECORDS: u64 = 65_536;
pub const MAX_TYPED_PROVENANCE_PREFIXES: u64 = 65_536;
pub const MAX_SOURCE_PEERS_PER_RECORD: usize = 64;
pub const MAX_TYPED_DELTA_PAGE_RECORDS: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexedTypedRecord {
    pub sequence: u64,
    pub cid: [u8; 32],
    pub canonical_bytes: Vec<u8>,
    pub source_peers: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexedTypedDelta {
    pub records: Vec<IndexedTypedRecord>,
    pub next_cursor: u64,
    pub exhausted: bool,
}

#[derive(Clone)]
pub struct RedbRecordProvenance {
    database: Arc<Database>,
}

impl RedbRecordProvenance {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let database = Database::create(path).map_err(|error| error.to_string())?;
        let write = database.begin_write().map_err(|error| error.to_string())?;
        {
            write
                .open_table(OBSERVATIONS)
                .map_err(|error| error.to_string())?;
            write
                .open_table(TYPED_RECORDS)
                .map_err(|error| error.to_string())?;
            write
                .open_table(TYPED_RECORD_KEYS)
                .map_err(|error| error.to_string())?;
            write
                .open_table(TYPED_RECORD_HEADS)
                .map_err(|error| error.to_string())?;
            write
                .open_table(TYPED_RECORD_PEERS)
                .map_err(|error| error.to_string())?;
        }
        write.commit().map_err(|error| error.to_string())?;
        Ok(Self {
            database: Arc::new(database),
        })
    }

    pub fn observe(
        &self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        selector: SelectorCid,
        peer: NodeId,
    ) -> Result<(), String> {
        let key = observation_key(kind, cid, selector, peer);
        let write = self
            .database
            .begin_write()
            .map_err(|error| error.to_string())?;
        {
            let mut table = write
                .open_table(OBSERVATIONS)
                .map_err(|error| error.to_string())?;
            if table
                .get(key.as_slice())
                .map_err(|error| error.to_string())?
                .is_none()
            {
                if table.len().map_err(|error| error.to_string())? >= MAX_PROVENANCE_OBSERVATIONS {
                    return Err("VNEXT_PROVENANCE_OBSERVATION_LIMIT".to_string());
                }
                let prefix = observation_prefix(kind, cid, selector);
                let mut peers = 0usize;
                for entry in table
                    .range::<&[u8]>(prefix.as_slice()..)
                    .map_err(|error| error.to_string())?
                {
                    let (peer_key, _) = entry.map_err(|error| error.to_string())?;
                    if !peer_key.value().starts_with(&prefix) {
                        break;
                    }
                    peers += 1;
                    if peers >= MAX_SOURCE_PEERS_PER_RECORD {
                        return Err("VNEXT_PROVENANCE_PEER_LIMIT".to_string());
                    }
                }
            }
            table
                .insert(key.as_slice(), &[][..])
                .map_err(|error| error.to_string())?;
        }
        write.commit().map_err(|error| error.to_string())
    }

    pub fn observe_typed(
        &self,
        kind: ReconcileManifestKind,
        type_id: u64,
        cid: [u8; 32],
        canonical_bytes: &[u8],
        selector: SelectorCid,
        peer: NodeId,
    ) -> Result<(), String> {
        let prefix = typed_prefix(selector, kind, type_id);
        let lookup_key = typed_lookup_key(prefix, cid);
        let write = self
            .database
            .begin_write()
            .map_err(|error| error.to_string())?;

        let existing_sequence = {
            let table = write
                .open_table(TYPED_RECORD_KEYS)
                .map_err(|error| error.to_string())?;
            let sequence = table
                .get(lookup_key.as_slice())
                .map_err(|error| error.to_string())?
                .map(|value| decode_sequence(value.value()))
                .transpose()?;
            sequence
        };
        let sequence = match existing_sequence {
            Some(sequence) => {
                let record_key = typed_record_key(prefix, sequence);
                let table = write
                    .open_table(TYPED_RECORDS)
                    .map_err(|error| error.to_string())?;
                let stored = table
                    .get(record_key.as_slice())
                    .map_err(|error| error.to_string())?
                    .map(|value| value.value().to_vec())
                    .ok_or_else(|| "VNEXT_TYPED_RECORD_INDEX_CORRUPT".to_string())?;
                let expected = typed_record_value(cid, canonical_bytes);
                if stored != expected {
                    return Err("VNEXT_TYPED_RECORD_CONFLICT".to_string());
                }
                sequence
            }
            None => {
                let sequence = {
                    let mut heads = write
                        .open_table(TYPED_RECORD_HEADS)
                        .map_err(|error| error.to_string())?;
                    let current = heads
                        .get(prefix.as_slice())
                        .map_err(|error| error.to_string())?
                        .map(|value| decode_sequence(value.value()))
                        .transpose()?;
                    if current.is_none()
                        && heads.len().map_err(|error| error.to_string())?
                            >= MAX_TYPED_PROVENANCE_PREFIXES
                    {
                        return Err("VNEXT_TYPED_PREFIX_LIMIT".to_string());
                    }
                    let current = current.unwrap_or(0);
                    let next = current
                        .checked_add(1)
                        .ok_or_else(|| "VNEXT_TYPED_RECORD_SEQUENCE_EXHAUSTED".to_string())?;
                    heads
                        .insert(prefix.as_slice(), next.to_be_bytes().as_slice())
                        .map_err(|error| error.to_string())?;
                    next
                };
                let record_key = typed_record_key(prefix, sequence);
                let value = typed_record_value(cid, canonical_bytes);
                {
                    let mut records = write
                        .open_table(TYPED_RECORDS)
                        .map_err(|error| error.to_string())?;
                    if records.len().map_err(|error| error.to_string())?
                        >= MAX_TYPED_PROVENANCE_RECORDS
                    {
                        return Err("VNEXT_TYPED_RECORD_LIMIT".to_string());
                    }
                    records
                        .insert(record_key.as_slice(), value.as_slice())
                        .map_err(|error| error.to_string())?;
                }
                write
                    .open_table(TYPED_RECORD_KEYS)
                    .map_err(|error| error.to_string())?
                    .insert(lookup_key.as_slice(), sequence.to_be_bytes().as_slice())
                    .map_err(|error| error.to_string())?;
                sequence
            }
        };

        let peer_key = typed_peer_key(prefix, cid, peer);
        {
            let mut peers = write
                .open_table(TYPED_RECORD_PEERS)
                .map_err(|error| error.to_string())?;
            if peers
                .get(peer_key.as_slice())
                .map_err(|error| error.to_string())?
                .is_none()
            {
                if peers.len().map_err(|error| error.to_string())? >= MAX_PROVENANCE_OBSERVATIONS {
                    return Err("VNEXT_TYPED_PEER_GLOBAL_LIMIT".to_string());
                }
                let peer_prefix = typed_lookup_key(prefix, cid);
                let mut peer_count = 0usize;
                for entry in peers
                    .range::<&[u8]>(peer_prefix.as_slice()..)
                    .map_err(|error| error.to_string())?
                {
                    let (stored_key, _) = entry.map_err(|error| error.to_string())?;
                    if !stored_key.value().starts_with(&peer_prefix) {
                        break;
                    }
                    peer_count += 1;
                    if peer_count >= MAX_SOURCE_PEERS_PER_RECORD {
                        return Err("VNEXT_TYPED_PEER_LIMIT".to_string());
                    }
                }
            }
            peers
                .insert(peer_key.as_slice(), &[][..])
                .map_err(|error| error.to_string())?;
        }
        write.commit().map_err(|error| error.to_string())?;

        // Keep the compatibility provenance view populated for callers that
        // do not know the typed discriminator.
        let _ = sequence;
        self.observe(kind, cid, selector, peer)
    }

    pub(crate) fn typed_delta(
        &self,
        selector: SelectorCid,
        kind: ReconcileManifestKind,
        type_id: u64,
        after_sequence: u64,
        limit: usize,
    ) -> Result<IndexedTypedDelta, String> {
        if limit == 0 {
            return Err("VNEXT_TYPED_RECORD_LIMIT_ZERO".to_string());
        }
        if limit > MAX_TYPED_DELTA_PAGE_RECORDS {
            return Err("VNEXT_TYPED_RECORD_PAGE_LIMIT".to_string());
        }
        let prefix = typed_prefix(selector, kind, type_id);
        let start = typed_record_key(
            prefix,
            after_sequence
                .checked_add(1)
                .ok_or_else(|| "VNEXT_TYPED_RECORD_CURSOR_EXHAUSTED".to_string())?,
        );
        let read = self
            .database
            .begin_read()
            .map_err(|error| error.to_string())?;
        let records_table = read
            .open_table(TYPED_RECORDS)
            .map_err(|error| error.to_string())?;
        let peers_table = read
            .open_table(TYPED_RECORD_PEERS)
            .map_err(|error| error.to_string())?;
        let mut records = Vec::new();
        let mut has_more = false;
        for entry in records_table
            .range::<&[u8]>(start.as_slice()..)
            .map_err(|error| error.to_string())?
        {
            let (key, value) = entry.map_err(|error| error.to_string())?;
            let key = key.value();
            if key.len() != TYPED_RECORD_KEY_BYTES || !key.starts_with(&prefix) {
                break;
            }
            if records.len() == limit {
                has_more = true;
                break;
            }
            let sequence = decode_sequence(&key[TYPED_PREFIX_BYTES..])?;
            let value = value.value();
            if value.len() < 32 {
                return Err("VNEXT_TYPED_RECORD_INDEX_CORRUPT".to_string());
            }
            let mut cid = [0u8; 32];
            cid.copy_from_slice(&value[..32]);
            let peer_prefix = typed_lookup_key(prefix, cid);
            let mut source_peers = Vec::new();
            for peer_entry in peers_table
                .range::<&[u8]>(peer_prefix.as_slice()..)
                .map_err(|error| error.to_string())?
            {
                let (peer_key, _) = peer_entry.map_err(|error| error.to_string())?;
                let peer_key = peer_key.value();
                if peer_key.len() != TYPED_PEER_KEY_BYTES || !peer_key.starts_with(&peer_prefix) {
                    break;
                }
                let mut peer = [0u8; 32];
                peer.copy_from_slice(&peer_key[TYPED_LOOKUP_KEY_BYTES..]);
                source_peers.push(NodeId::from_bytes(peer));
            }
            source_peers.sort_by_key(|peer| *peer.as_bytes());
            source_peers.dedup();
            records.push(IndexedTypedRecord {
                sequence,
                cid,
                canonical_bytes: value[32..].to_vec(),
                source_peers,
            });
        }
        let next_cursor = records
            .last()
            .map_or(after_sequence, |record| record.sequence);
        Ok(IndexedTypedDelta {
            records,
            next_cursor,
            exhausted: !has_more,
        })
    }

    pub fn peers(
        &self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        selector: SelectorCid,
    ) -> Result<Vec<NodeId>, String> {
        let prefix = observation_prefix(kind, cid, selector);
        let read = self
            .database
            .begin_read()
            .map_err(|error| error.to_string())?;
        let table = read
            .open_table(OBSERVATIONS)
            .map_err(|error| error.to_string())?;
        let mut peers = Vec::new();
        for entry in table
            .range::<&[u8]>(prefix.as_slice()..)
            .map_err(|error| error.to_string())?
        {
            let (key, _) = entry.map_err(|error| error.to_string())?;
            let key = key.value();
            if key.len() != KEY_BYTES || !key.starts_with(&prefix) {
                break;
            }
            let mut peer = [0u8; 32];
            peer.copy_from_slice(&key[prefix.len()..]);
            peers.push(NodeId::from_bytes(peer));
        }
        peers.sort_by_key(|peer| *peer.as_bytes());
        peers.dedup();
        Ok(peers)
    }
}

impl PortableArchiveRows for RedbRecordProvenance {
    fn archive_owner(&self) -> ArchiveOwner {
        ArchiveOwner::PROVENANCE
    }

    fn archive_entry_kind(&self) -> ArchiveEntryKind {
        ArchiveEntryKind::ProvenanceRecord
    }

    fn archive_rows(&self) -> Result<Vec<PortableArchiveRow>, NodeError> {
        let read = self
            .database
            .begin_read()
            .map_err(provenance_archive_error)?;
        let mut rows = Vec::new();
        for (table_id, table) in [
            (
                1u8,
                read.open_table(OBSERVATIONS)
                    .map_err(provenance_archive_error)?,
            ),
            (
                2u8,
                read.open_table(TYPED_RECORDS)
                    .map_err(provenance_archive_error)?,
            ),
            (
                3u8,
                read.open_table(TYPED_RECORD_KEYS)
                    .map_err(provenance_archive_error)?,
            ),
            (
                4u8,
                read.open_table(TYPED_RECORD_HEADS)
                    .map_err(provenance_archive_error)?,
            ),
            (
                5u8,
                read.open_table(TYPED_RECORD_PEERS)
                    .map_err(provenance_archive_error)?,
            ),
        ] {
            for row in table.iter().map_err(provenance_archive_error)? {
                let (key, value) = row.map_err(provenance_archive_error)?;
                let row = PortableArchiveRow {
                    table: table_id,
                    key: key.value().to_vec(),
                    value: value.value().to_vec(),
                };
                validate_provenance_archive_row(&row)?;
                rows.push(row);
            }
        }
        rows.sort_by(|left, right| (left.table, &left.key).cmp(&(right.table, &right.key)));
        Ok(rows)
    }

    fn restore_row(&self, row: &PortableArchiveRow) -> Result<(), NodeError> {
        validate_provenance_archive_row(row)?;
        let write = self
            .database
            .begin_write()
            .map_err(provenance_archive_error)?;
        match row.table {
            1 => restore_provenance_value(&write, OBSERVATIONS, row)?,
            2 => restore_provenance_value(&write, TYPED_RECORDS, row)?,
            3 => restore_provenance_value(&write, TYPED_RECORD_KEYS, row)?,
            4 => restore_provenance_value(&write, TYPED_RECORD_HEADS, row)?,
            5 => restore_provenance_value(&write, TYPED_RECORD_PEERS, row)?,
            _ => {
                return Err(NodeError::ArchiveCapability(
                    "provenance archive table is unknown".into(),
                ))
            }
        }
        write.commit().map_err(provenance_archive_error)
    }
}

fn validate_provenance_archive_row(row: &PortableArchiveRow) -> Result<(), NodeError> {
    let valid = match row.table {
        1 => row.key.len() == KEY_BYTES && row.value.is_empty(),
        2 => row.key.len() == TYPED_RECORD_KEY_BYTES && row.value.len() > 32,
        3 => row.key.len() == TYPED_LOOKUP_KEY_BYTES && row.value.len() == 8,
        4 => row.key.len() == TYPED_PREFIX_BYTES && row.value.len() == 8,
        5 => row.key.len() == TYPED_PEER_KEY_BYTES && row.value.is_empty(),
        _ => false,
    };
    if !valid {
        return Err(NodeError::ArchiveCapability(
            "provenance archive row is invalid".into(),
        ));
    }
    if matches!(row.table, 3 | 4) {
        decode_sequence(&row.value).map_err(NodeError::Storage)?;
    }
    Ok(())
}

fn restore_provenance_value(
    write: &redb::WriteTransaction,
    definition: TableDefinition<&[u8], &[u8]>,
    row: &PortableArchiveRow,
) -> Result<(), NodeError> {
    let mut table = write
        .open_table(definition)
        .map_err(provenance_archive_error)?;
    if let Some(existing) = table
        .get(row.key.as_slice())
        .map_err(provenance_archive_error)?
    {
        if existing.value() == row.value.as_slice() {
            return Ok(());
        }
        return Err(NodeError::ArchiveCapability(
            "provenance archive restore conflict".into(),
        ));
    }
    table
        .insert(row.key.as_slice(), row.value.as_slice())
        .map_err(provenance_archive_error)?;
    Ok(())
}

fn provenance_archive_error(error: impl std::fmt::Display) -> NodeError {
    NodeError::Storage(error.to_string())
}

fn typed_prefix(
    selector: SelectorCid,
    kind: ReconcileManifestKind,
    type_id: u64,
) -> [u8; TYPED_PREFIX_BYTES] {
    let mut key = [0u8; TYPED_PREFIX_BYTES];
    key[..32].copy_from_slice(selector.as_bytes());
    key[32..40].copy_from_slice(&(kind as u64).to_be_bytes());
    key[40..].copy_from_slice(&type_id.to_be_bytes());
    key
}

fn typed_record_key(
    prefix: [u8; TYPED_PREFIX_BYTES],
    sequence: u64,
) -> [u8; TYPED_RECORD_KEY_BYTES] {
    let mut key = [0u8; TYPED_RECORD_KEY_BYTES];
    key[..TYPED_PREFIX_BYTES].copy_from_slice(&prefix);
    key[TYPED_PREFIX_BYTES..].copy_from_slice(&sequence.to_be_bytes());
    key
}

fn typed_lookup_key(
    prefix: [u8; TYPED_PREFIX_BYTES],
    cid: [u8; 32],
) -> [u8; TYPED_LOOKUP_KEY_BYTES] {
    let mut key = [0u8; TYPED_LOOKUP_KEY_BYTES];
    key[..TYPED_PREFIX_BYTES].copy_from_slice(&prefix);
    key[TYPED_PREFIX_BYTES..].copy_from_slice(&cid);
    key
}

fn typed_peer_key(
    prefix: [u8; TYPED_PREFIX_BYTES],
    cid: [u8; 32],
    peer: NodeId,
) -> [u8; TYPED_PEER_KEY_BYTES] {
    let lookup = typed_lookup_key(prefix, cid);
    let mut key = [0u8; TYPED_PEER_KEY_BYTES];
    key[..TYPED_LOOKUP_KEY_BYTES].copy_from_slice(&lookup);
    key[TYPED_LOOKUP_KEY_BYTES..].copy_from_slice(peer.as_bytes());
    key
}

fn typed_record_value(cid: [u8; 32], canonical_bytes: &[u8]) -> Vec<u8> {
    let mut value = Vec::with_capacity(32 + canonical_bytes.len());
    value.extend_from_slice(&cid);
    value.extend_from_slice(canonical_bytes);
    value
}

fn decode_sequence(value: &[u8]) -> Result<u64, String> {
    value
        .try_into()
        .map(u64::from_be_bytes)
        .map_err(|_| "VNEXT_TYPED_RECORD_INDEX_CORRUPT".to_string())
}

fn observation_prefix(
    kind: ReconcileManifestKind,
    cid: [u8; 32],
    selector: SelectorCid,
) -> [u8; 72] {
    let mut key = [0u8; 72];
    key[..8].copy_from_slice(&(kind as u64).to_be_bytes());
    key[8..40].copy_from_slice(&cid);
    key[40..72].copy_from_slice(selector.as_bytes());
    key
}

fn observation_key(
    kind: ReconcileManifestKind,
    cid: [u8; 32],
    selector: SelectorCid,
    peer: NodeId,
) -> [u8; KEY_BYTES] {
    let prefix = observation_prefix(kind, cid, selector);
    let mut key = [0u8; KEY_BYTES];
    key[..prefix.len()].copy_from_slice(&prefix);
    key[prefix.len()..].copy_from_slice(peer.as_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_observations_are_idempotent_selector_scoped_and_restart_safe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("provenance.redb");
        let selector = SelectorCid::from_bytes([1; 32]);
        let other_selector = SelectorCid::from_bytes([2; 32]);
        let first = NodeId::from_bytes([3; 32]);
        let second = NodeId::from_bytes([4; 32]);
        {
            let store = RedbRecordProvenance::open(&path).unwrap();
            store
                .observe(ReconcileManifestKind::Object, [5; 32], selector, second)
                .unwrap();
            store
                .observe(ReconcileManifestKind::Object, [5; 32], selector, first)
                .unwrap();
            store
                .observe(ReconcileManifestKind::Object, [5; 32], selector, first)
                .unwrap();
            store
                .observe(
                    ReconcileManifestKind::Object,
                    [5; 32],
                    other_selector,
                    second,
                )
                .unwrap();
        }
        let reopened = RedbRecordProvenance::open(&path).unwrap();
        assert_eq!(
            reopened
                .peers(ReconcileManifestKind::Object, [5; 32], selector)
                .unwrap(),
            vec![first, second]
        );
        assert_eq!(
            reopened
                .peers(ReconcileManifestKind::Object, [5; 32], other_selector)
                .unwrap(),
            vec![second]
        );
    }

    #[test]
    fn typed_selector_delta_uses_monotonic_sequence_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("typed-provenance.redb");
        let selector = SelectorCid::from_bytes([0x11; 32]);
        let first_peer = NodeId::from_bytes([0x21; 32]);
        let second_peer = NodeId::from_bytes([0x22; 32]);
        let high_cid = [0xf0; 32];
        let low_cid = [0x01; 32];
        {
            let store = RedbRecordProvenance::open(&path).unwrap();
            store
                .observe_typed(
                    ReconcileManifestKind::Object,
                    77,
                    high_cid,
                    b"first",
                    selector,
                    first_peer,
                )
                .unwrap();
            store
                .observe_typed(
                    ReconcileManifestKind::Object,
                    77,
                    low_cid,
                    b"second",
                    selector,
                    first_peer,
                )
                .unwrap();
            store
                .observe_typed(
                    ReconcileManifestKind::Object,
                    77,
                    high_cid,
                    b"first",
                    selector,
                    second_peer,
                )
                .unwrap();
            let first = store
                .typed_delta(selector, ReconcileManifestKind::Object, 77, 0, 1)
                .unwrap();
            assert_eq!(first.records.len(), 1);
            assert_eq!(first.records[0].sequence, 1);
            assert_eq!(first.records[0].cid, high_cid);
            assert_eq!(first.records[0].source_peers, vec![first_peer, second_peer]);
            assert!(!first.exhausted);
        }

        let reopened = RedbRecordProvenance::open(&path).unwrap();
        let second = reopened
            .typed_delta(selector, ReconcileManifestKind::Object, 77, 1, 1)
            .unwrap();
        assert_eq!(second.records.len(), 1);
        assert_eq!(second.records[0].sequence, 2);
        assert_eq!(second.records[0].cid, low_cid);
        assert_eq!(second.records[0].canonical_bytes, b"second");
        assert!(second.exhausted);
        assert!(reopened
            .typed_delta(selector, ReconcileManifestKind::Object, 78, 0, 8)
            .unwrap()
            .records
            .is_empty());
    }

    #[test]
    fn provenance_peer_fanout_and_delta_page_are_hard_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let store =
            RedbRecordProvenance::open(directory.path().join("bounded-provenance.redb")).unwrap();
        let selector = SelectorCid::from_bytes([0x41; 32]);
        for marker in 0..MAX_SOURCE_PEERS_PER_RECORD {
            store
                .observe(
                    ReconcileManifestKind::Object,
                    [0x42; 32],
                    selector,
                    NodeId::from_bytes([marker as u8; 32]),
                )
                .unwrap();
        }
        assert_eq!(
            store.observe(
                ReconcileManifestKind::Object,
                [0x42; 32],
                selector,
                NodeId::from_bytes([0xff; 32]),
            ),
            Err("VNEXT_PROVENANCE_PEER_LIMIT".to_string())
        );
        assert_eq!(
            store.typed_delta(
                selector,
                ReconcileManifestKind::Object,
                1,
                0,
                MAX_TYPED_DELTA_PAGE_RECORDS + 1,
            ),
            Err("VNEXT_TYPED_RECORD_PAGE_LIMIT".to_string())
        );
    }
}
