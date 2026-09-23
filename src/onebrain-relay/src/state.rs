//! Durable create-new relay state primitives.

use std::fs::OpenOptions;
use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};

const DESCRIPTOR: TableDefinition<&[u8], &[u8]> = TableDefinition::new("descriptor_floors");
const CONTROL: TableDefinition<&[u8], &[u8]> = TableDefinition::new("control_floors");
const NONCES: TableDefinition<&[u8], &[u8]> = TableDefinition::new("consumed_nonces");
const RESERVATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("reservations");
const REVOCATIONS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("revocations");
const RENDEZVOUS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("rendezvous_records");
pub(crate) const CANDIDATE_KEY: &[u8] = b"candidate-v1";
pub(crate) const ACTIVATION_KEY: &[u8] = b"descriptor-activation-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurableStateKind {
    DescriptorFloor,
    ControlFloor,
    ConsumedNonce,
    Reservation,
    Revocation,
    RendezvousRecord,
}

pub struct DurableRelayState {
    database: Database,
}

impl DurableRelayState {
    pub fn initialize(path: &Path) -> Result<Self, DurableStateError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| DurableStateError::Io)?;
        }
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|_| DurableStateError::AlreadyExists)?;
        let database = Database::create(path).map_err(|_| DurableStateError::Corrupt)?;
        initialize_tables(&database)?;
        Ok(Self { database })
    }

    pub fn open(path: &Path) -> Result<Self, DurableStateError> {
        if !path.is_file() {
            return Err(DurableStateError::Missing);
        }
        let database = Database::open(path).map_err(|_| DurableStateError::Corrupt)?;
        initialize_tables(&database)?;
        Ok(Self { database })
    }

    pub fn create_new(
        &self,
        kind: DurableStateKind,
        key: &[u8],
        value: &[u8],
    ) -> Result<(), DurableStateError> {
        if key.is_empty() || value.is_empty() {
            return Err(DurableStateError::Invalid);
        }
        let write = self
            .database
            .begin_write()
            .map_err(|_| DurableStateError::Corrupt)?;
        {
            let mut table = write
                .open_table(table(kind))
                .map_err(|_| DurableStateError::Corrupt)?;
            if table
                .get(key)
                .map_err(|_| DurableStateError::Corrupt)?
                .is_some()
            {
                return Err(DurableStateError::Replay);
            }
            table
                .insert(key, value)
                .map_err(|_| DurableStateError::Corrupt)?;
        }
        write.commit().map_err(|_| DurableStateError::Corrupt)
    }

    pub fn get(
        &self,
        kind: DurableStateKind,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, DurableStateError> {
        let read = self
            .database
            .begin_read()
            .map_err(|_| DurableStateError::Corrupt)?;
        let table = read
            .open_table(table(kind))
            .map_err(|_| DurableStateError::Corrupt)?;
        Ok(table
            .get(key)
            .map_err(|_| DurableStateError::Corrupt)?
            .map(|value| value.value().to_vec()))
    }

    /// Recover the monotonic reservation request floor for one target. Keys
    /// are the target NodeID followed by its big-endian sequence. Other
    /// control entries, including descriptor activation, are ignored.
    pub fn highest_reservation_request_sequence(
        &self,
        target: &[u8; 32],
    ) -> Result<Option<u64>, DurableStateError> {
        let read = self
            .database
            .begin_read()
            .map_err(|_| DurableStateError::Corrupt)?;
        let table = read
            .open_table(CONTROL)
            .map_err(|_| DurableStateError::Corrupt)?;
        let mut highest: Option<u64> = None;
        for entry in table.iter().map_err(|_| DurableStateError::Corrupt)? {
            let (key, _) = entry.map_err(|_| DurableStateError::Corrupt)?;
            let bytes = key.value();
            if bytes.len() == 40 && bytes.starts_with(target) {
                let sequence = u64::from_be_bytes(
                    bytes[32..].try_into().map_err(|_| DurableStateError::Corrupt)?,
                );
                highest = Some(highest.map_or(sequence, |value| value.max(sequence)));
            }
        }
        Ok(highest)
    }

    /// Replace only the descriptor floor and fence its old activation together.
    /// The caller validates the signed contiguous successor before this CAS.
    pub(crate) fn advance_descriptor(
        &self,
        expected: Option<&[u8]>,
        successor: &[u8],
    ) -> Result<(), DurableStateError> {
        if successor.is_empty() {
            return Err(DurableStateError::Invalid);
        }
        let write = self
            .database
            .begin_write()
            .map_err(|_| DurableStateError::Corrupt)?;
        {
            let mut descriptors = write
                .open_table(DESCRIPTOR)
                .map_err(|_| DurableStateError::Corrupt)?;
            let current = descriptors
                .get(CANDIDATE_KEY)
                .map_err(|_| DurableStateError::Corrupt)?
                .map(|value| value.value().to_vec());
            if current.as_deref() != expected {
                return Err(DurableStateError::Replay);
            }
            descriptors
                .insert(CANDIDATE_KEY, successor)
                .map_err(|_| DurableStateError::Corrupt)?;
            // This is a lifecycle marker, not a control-message replay floor.
            // Removing it also fences an older binary that only checks presence.
            write
                .open_table(CONTROL)
                .map_err(|_| DurableStateError::Corrupt)?
                .remove(ACTIVATION_KEY)
                .map_err(|_| DurableStateError::Corrupt)?;
        }
        write.commit().map_err(|_| DurableStateError::Corrupt)
    }

    pub(crate) fn activate_current_descriptor(
        &self,
        expected: &[u8],
        probe_digest: &[u8; 32],
    ) -> Result<(), DurableStateError> {
        let write = self
            .database
            .begin_write()
            .map_err(|_| DurableStateError::Corrupt)?;
        {
            let descriptors = write
                .open_table(DESCRIPTOR)
                .map_err(|_| DurableStateError::Corrupt)?;
            let current = descriptors
                .get(CANDIDATE_KEY)
                .map_err(|_| DurableStateError::Corrupt)?;
            if current.as_ref().map(|value| value.value()) != Some(expected) {
                return Err(DurableStateError::Replay);
            }
            let mut binding = Vec::from(blake3::hash(expected).as_bytes().as_slice());
            binding.extend_from_slice(probe_digest);
            write
                .open_table(CONTROL)
                .map_err(|_| DurableStateError::Corrupt)?
                .insert(ACTIVATION_KEY, binding.as_slice())
                .map_err(|_| DurableStateError::Corrupt)?;
        }
        write.commit().map_err(|_| DurableStateError::Corrupt)
    }
}

fn initialize_tables(database: &Database) -> Result<(), DurableStateError> {
    let write = database
        .begin_write()
        .map_err(|_| DurableStateError::Corrupt)?;
    for definition in [
        DESCRIPTOR,
        CONTROL,
        NONCES,
        RESERVATIONS,
        REVOCATIONS,
        RENDEZVOUS,
    ] {
        write
            .open_table(definition)
            .map_err(|_| DurableStateError::Corrupt)?;
    }
    write.commit().map_err(|_| DurableStateError::Corrupt)
}

fn table(kind: DurableStateKind) -> TableDefinition<'static, &'static [u8], &'static [u8]> {
    match kind {
        DurableStateKind::DescriptorFloor => DESCRIPTOR,
        DurableStateKind::ControlFloor => CONTROL,
        DurableStateKind::ConsumedNonce => NONCES,
        DurableStateKind::Reservation => RESERVATIONS,
        DurableStateKind::Revocation => REVOCATIONS,
        DurableStateKind::RendezvousRecord => RENDEZVOUS,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurableStateError {
    Missing,
    AlreadyExists,
    Corrupt,
    Replay,
    Invalid,
    Io,
}

impl std::fmt::Display for DurableStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RELAY_STATE: {self:?}")
    }
}

impl std::error::Error for DurableStateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_cas_has_one_winner_and_stale_activation_cannot_cross_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("relay.redb");
        let state = std::sync::Arc::new(DurableRelayState::initialize(&path).unwrap());
        state.advance_descriptor(None, b"first").unwrap();
        state
            .activate_current_descriptor(b"first", &[1; 32])
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for successor in [b"second-a", b"second-b"] {
            let state = state.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                state.advance_descriptor(Some(b"first"), successor)
            }));
        }
        barrier.wait();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Err(DurableStateError::Replay))
                .count(),
            1
        );
        assert_eq!(
            state.activate_current_descriptor(b"first", &[2; 32]),
            Err(DurableStateError::Replay)
        );
        assert_eq!(
            state
                .get(DurableStateKind::ControlFloor, ACTIVATION_KEY)
                .unwrap(),
            None
        );
        let winner = state
            .get(DurableStateKind::DescriptorFloor, CANDIDATE_KEY)
            .unwrap()
            .unwrap();
        drop(state);
        let reopened = DurableRelayState::open(&path).unwrap();
        assert_eq!(
            reopened
                .get(DurableStateKind::DescriptorFloor, CANDIDATE_KEY)
                .unwrap(),
            Some(winner)
        );
        assert_eq!(
            reopened
                .get(DurableStateKind::ControlFloor, ACTIVATION_KEY)
                .unwrap(),
            None
        );
    }

    #[test]
    fn create_new_state_survives_reopen_and_replay_rejects() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("relay.redb");
        let state = DurableRelayState::initialize(&path).unwrap();
        state
            .create_new(DurableStateKind::ConsumedNonce, b"nonce", b"used")
            .unwrap();
        assert_eq!(
            state
                .create_new(DurableStateKind::ConsumedNonce, b"nonce", b"again")
                .unwrap_err(),
            DurableStateError::Replay
        );
        drop(state);
        let reopened = DurableRelayState::open(&path).unwrap();
        assert_eq!(
            reopened
                .get(DurableStateKind::ConsumedNonce, b"nonce")
                .unwrap(),
            Some(b"used".to_vec())
        );
        assert_eq!(
            DurableRelayState::initialize(&path).err().unwrap(),
            DurableStateError::AlreadyExists
        );
    }
}
