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
