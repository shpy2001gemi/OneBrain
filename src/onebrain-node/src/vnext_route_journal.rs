//! Bounded, privacy-safe route transition journal.

#![cfg(feature = "vnext-outbound-first")]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use ku_core::foundation::NodeId;
use onebrain_protocol::RoutePathKindV1;
use redb::{Database, Durability, ReadableTable, TableDefinition};
use thiserror::Error;

pub const MAX_ROUTE_JOURNAL_RECORDS: usize = 4_096;
pub const MAX_ROUTE_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const ROUTE_JOURNAL_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_route_journal_v1");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteJournalEntryV1 {
    peer: NodeId,
    path_kind: RoutePathKindV1,
    route_receipt_digest: [u8; 32],
    route_sequence: u64,
    observed_at_unix_seconds: u64,
}

impl RouteJournalEntryV1 {
    pub fn routed(
        peer: NodeId,
        path_kind: RoutePathKindV1,
        route_receipt_digest: [u8; 32],
        route_sequence: u64,
        observed_at_unix_seconds: u64,
    ) -> Self {
        Self {
            peer,
            path_kind,
            route_receipt_digest,
            route_sequence,
            observed_at_unix_seconds,
        }
    }

    pub fn direct(
        peer: NodeId,
        route_receipt_digest: [u8; 32],
        route_sequence: u64,
        observed_at_unix_seconds: u64,
    ) -> Self {
        Self::routed(
            peer,
            RoutePathKindV1::Direct,
            route_receipt_digest,
            route_sequence,
            observed_at_unix_seconds,
        )
    }

    pub fn peer(&self) -> NodeId {
        self.peer
    }

    pub fn route_sequence(&self) -> u64 {
        self.route_sequence
    }

    pub fn path_kind(&self) -> RoutePathKindV1 {
        self.path_kind
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RouteJournalError> {
        if self.peer.as_bytes() == &[0; 32]
            || self.route_receipt_digest == [0; 32]
            || self.route_sequence == 0
            || self.observed_at_unix_seconds == 0
        {
            return Err(RouteJournalError::InvalidEntry);
        }
        let mut bytes = Vec::with_capacity(88);
        bytes.extend_from_slice(b"OBRTJ1\0");
        bytes.extend_from_slice(self.peer.as_bytes());
        bytes.push(self.path_kind as u8);
        bytes.extend_from_slice(&self.route_receipt_digest);
        bytes.extend_from_slice(&self.route_sequence.to_be_bytes());
        bytes.extend_from_slice(&self.observed_at_unix_seconds.to_be_bytes());
        Ok(bytes)
    }
}

#[derive(Default)]
struct JournalState {
    entries: Vec<RouteJournalEntryV1>,
    encoded_bytes: usize,
    by_peer_sequence: BTreeMap<NodeId, u64>,
}

#[derive(Clone)]
pub struct RouteJournal {
    state: Arc<RwLock<JournalState>>,
    database: Option<Arc<Database>>,
    max_records: usize,
    max_bytes: usize,
}

impl RouteJournal {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, RouteJournalError> {
        if max_records == 0
            || max_records > MAX_ROUTE_JOURNAL_RECORDS
            || max_bytes == 0
            || max_bytes > MAX_ROUTE_JOURNAL_BYTES
        {
            return Err(RouteJournalError::InvalidLimit);
        }
        Ok(Self {
            state: Arc::new(RwLock::new(JournalState::default())),
            database: None,
            max_records,
            max_bytes,
        })
    }

    pub fn open(
        path: &Path,
        max_records: usize,
        max_bytes: usize,
    ) -> Result<Self, RouteJournalError> {
        let mut journal = Self::new(max_records, max_bytes)?;
        let first_create = !path.exists();
        let database = Database::create(path).map_err(backend)?;
        let mut write = database.begin_write().map_err(backend)?;
        write.set_durability(Durability::Immediate);
        write.open_table(ROUTE_JOURNAL_TABLE).map_err(backend)?;
        write.commit().map_err(backend)?;
        if first_create {
            sync_parent(path)?;
        }
        {
            let read = database.begin_read().map_err(backend)?;
            let table = read.open_table(ROUTE_JOURNAL_TABLE).map_err(backend)?;
            let mut state = journal
                .state
                .write()
                .map_err(|_| RouteJournalError::LockPoisoned)?;
            for row in table.iter().map_err(backend)? {
                let (_, value) = row.map_err(backend)?;
                register_in_memory(
                    &mut state,
                    decode_entry(value.value())?,
                    max_records,
                    max_bytes,
                )?;
            }
        }
        journal.database = Some(Arc::new(database));
        Ok(journal)
    }

    pub fn append(&self, entry: RouteJournalEntryV1) -> Result<[u8; 32], RouteJournalError> {
        let encoded = entry.canonical_bytes()?;
        let mut state = self
            .state
            .write()
            .map_err(|_| RouteJournalError::LockPoisoned)?;
        if state
            .by_peer_sequence
            .get(&entry.peer)
            .is_some_and(|previous| *previous >= entry.route_sequence)
        {
            return Err(RouteJournalError::SequenceRollback);
        }
        if state.entries.len() >= self.max_records
            || state.encoded_bytes.saturating_add(encoded.len()) > self.max_bytes
        {
            return Err(RouteJournalError::CapacityReached);
        }
        if let Some(database) = &self.database {
            let mut write = database.begin_write().map_err(backend)?;
            write.set_durability(Durability::Immediate);
            let mut table = write.open_table(ROUTE_JOURNAL_TABLE).map_err(backend)?;
            let key = journal_key(state.entries.len() as u64 + 1);
            if table.get(key.as_slice()).map_err(backend)?.is_some() {
                return Err(RouteJournalError::SequenceRollback);
            }
            table
                .insert(key.as_slice(), encoded.as_slice())
                .map_err(backend)?;
            drop(table);
            write.commit().map_err(backend)?;
        }
        state.encoded_bytes += encoded.len();
        state
            .by_peer_sequence
            .insert(entry.peer, entry.route_sequence);
        state.entries.push(entry);
        root_for(&state.entries)
    }

    pub fn root(&self) -> Result<[u8; 32], RouteJournalError> {
        let state = self
            .state
            .read()
            .map_err(|_| RouteJournalError::LockPoisoned)?;
        root_for(&state.entries)
    }

    pub fn len(&self) -> Result<usize, RouteJournalError> {
        self.state
            .read()
            .map(|state| state.entries.len())
            .map_err(|_| RouteJournalError::LockPoisoned)
    }

    pub fn is_empty(&self) -> Result<bool, RouteJournalError> {
        self.len().map(|length| length == 0)
    }
}

fn register_in_memory(
    state: &mut JournalState,
    entry: RouteJournalEntryV1,
    max_records: usize,
    max_bytes: usize,
) -> Result<(), RouteJournalError> {
    let encoded = entry.canonical_bytes()?;
    if state.entries.len() >= max_records
        || state.encoded_bytes.saturating_add(encoded.len()) > max_bytes
        || state
            .by_peer_sequence
            .get(&entry.peer)
            .is_some_and(|previous| *previous >= entry.route_sequence)
    {
        return Err(RouteJournalError::InvalidEntry);
    }
    state.encoded_bytes += encoded.len();
    state
        .by_peer_sequence
        .insert(entry.peer, entry.route_sequence);
    state.entries.push(entry);
    Ok(())
}

fn journal_key(global_sequence: u64) -> [u8; 8] {
    global_sequence.to_be_bytes()
}

fn decode_entry(bytes: &[u8]) -> Result<RouteJournalEntryV1, RouteJournalError> {
    if bytes.len() != 88 || &bytes[..7] != b"OBRTJ1\0" {
        return Err(RouteJournalError::InvalidEntry);
    }
    let peer = NodeId::from_bytes(
        bytes[7..39]
            .try_into()
            .map_err(|_| RouteJournalError::InvalidEntry)?,
    );
    let path_kind = match bytes[39] {
        0 => RoutePathKindV1::Direct,
        1 => RoutePathKindV1::HolePunched,
        2 => RoutePathKindV1::RelayUdp,
        3 => RoutePathKindV1::RelayTcp443,
        _ => return Err(RouteJournalError::InvalidEntry),
    };
    let entry = RouteJournalEntryV1 {
        peer,
        path_kind,
        route_receipt_digest: bytes[40..72]
            .try_into()
            .map_err(|_| RouteJournalError::InvalidEntry)?,
        route_sequence: u64::from_be_bytes(
            bytes[72..80]
                .try_into()
                .map_err(|_| RouteJournalError::InvalidEntry)?,
        ),
        observed_at_unix_seconds: u64::from_be_bytes(
            bytes[80..88]
                .try_into()
                .map_err(|_| RouteJournalError::InvalidEntry)?,
        ),
    };
    entry.canonical_bytes()?;
    Ok(entry)
}

fn backend(error: impl std::fmt::Display) -> RouteJournalError {
    RouteJournalError::Backend(error.to_string())
}

fn sync_parent(path: &Path) -> Result<(), RouteJournalError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(backend)?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

fn root_for(entries: &[RouteJournalEntryV1]) -> Result<[u8; 32], RouteJournalError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:route-journal:1\0");
    for entry in entries {
        let bytes = entry.canonical_bytes()?;
        hasher.update(&(bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Ok(*hasher.finalize().as_bytes())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteJournalError {
    #[error("route journal limit is invalid")]
    InvalidLimit,
    #[error("route journal entry is invalid")]
    InvalidEntry,
    #[error("route journal capacity reached")]
    CapacityReached,
    #[error("route sequence did not advance")]
    SequenceRollback,
    #[error("route journal lock is poisoned")]
    LockPoisoned,
    #[error("route journal backend failed: {0}")]
    Backend(String),
}
