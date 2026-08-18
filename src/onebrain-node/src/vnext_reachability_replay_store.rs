//! Immediate-durability replay floors for reachability authority objects.

use std::path::Path;
use std::sync::Arc;

use ku_core::foundation::NodeId;
use ku_net::vnext_reachability_crypto::{
    ReachabilityNonceDomainV1, ReachabilityReplayStore, ReachabilitySequenceKeyV1,
    ReachabilitySequenceKindV1, RelayAdmissionError,
};
use redb::{Database, Durability, ReadableTable, TableDefinition};

const STATE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_reachability_replay_v1");

#[derive(Clone)]
pub struct RedbReachabilityReplayStore {
    database: Arc<Database>,
}

impl RedbReachabilityReplayStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RelayAdmissionError> {
        let path = path.as_ref();
        let created = !path.exists();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        let database = Database::create(path).map_err(|_| RelayAdmissionError::StateUnavailable)?;
        let mut write = database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        if created {
            sync_parent(path)?;
        }
        Ok(Self {
            database: Arc::new(database),
        })
    }

    fn read(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RelayAdmissionError> {
        let read = self
            .database
            .begin_read()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        let table = read
            .open_table(STATE)
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        Ok(table
            .get(key)
            .map_err(|_| RelayAdmissionError::StateUnavailable)?
            .map(|value| value.value().to_vec()))
    }
}

impl ReachabilityReplayStore for RedbReachabilityReplayStore {
    fn check_sequence_candidate(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        previous_digest: Option<[u8; 32]>,
    ) -> Result<(), RelayAdmissionError> {
        match self.read(&sequence_key(key))? {
            None if sequence == 1 && previous_digest.is_none() => Ok(()),
            Some(value) => {
                let (current, digest, _) = decode_sequence(&value)?;
                if sequence == current + 1 && previous_digest == Some(digest) {
                    Ok(())
                } else if sequence == current && previous_digest == Some(digest) {
                    Err(RelayAdmissionError::Replay)
                } else {
                    Err(RelayAdmissionError::SequenceRollback)
                }
            }
            None => Err(RelayAdmissionError::SequenceRollback),
        }
    }

    fn compare_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        expected_previous_digest: Option<[u8; 32]>,
        sequence: u64,
        new_digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let storage_key = sequence_key(key);
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let current = table
                .get(storage_key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .map(|value| value.value().to_vec());
            match current {
                None if sequence == 1 && expected_previous_digest.is_none() => {}
                Some(value) => {
                    let (current_sequence, current_digest, _) = decode_sequence(&value)?;
                    if sequence == current_sequence
                        && expected_previous_digest == Some(current_digest)
                    {
                        return Err(RelayAdmissionError::Replay);
                    }
                    if sequence != current_sequence + 1
                        || expected_previous_digest != Some(current_digest)
                    {
                        return Err(RelayAdmissionError::SequenceRollback);
                    }
                }
                None => return Err(RelayAdmissionError::SequenceRollback),
            }
            let encoded = encode_sequence(sequence, new_digest, expires_at);
            table
                .insert(storage_key.as_slice(), encoded.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)
    }

    fn check_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let storage_key = sequence_key(key);
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let current = table
                .get(storage_key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .map(|value| value.value().to_vec());
            match current {
                None if sequence == 1 => {}
                Some(value) => {
                    let (current_sequence, existing, _) = decode_sequence(&value)?;
                    if sequence == current_sequence && digest == existing {
                        return Err(RelayAdmissionError::Replay);
                    }
                    if sequence != current_sequence + 1 {
                        return Err(RelayAdmissionError::SequenceRollback);
                    }
                }
                None => return Err(RelayAdmissionError::SequenceRollback),
            }
            let encoded = encode_sequence(sequence, digest, expires_at);
            table
                .insert(storage_key.as_slice(), encoded.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)
    }

    fn check_and_store_reservation(
        &self,
        relay: NodeId,
        target: NodeId,
        reservation_id: [u8; 32],
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let key = reservation_key(relay, target, reservation_id);
        let mut value = Vec::with_capacity(40);
        value.extend_from_slice(&digest);
        value.extend_from_slice(&expires_at.to_be_bytes());
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let current = table
                .get(key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .map(|stored| stored.value().to_vec());
            if let Some(current) = current {
                if current.get(..32) == Some(digest.as_slice()) {
                    return Err(RelayAdmissionError::Replay);
                }
                return Err(RelayAdmissionError::ReservationIdReuse);
            }
            table
                .insert(key.as_slice(), value.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)
    }

    fn consume_nonce(
        &self,
        domain: ReachabilityNonceDomainV1,
        scope: [u8; 32],
        nonce: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let key = nonce_key(domain, scope, nonce);
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            if table
                .get(key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .is_some()
            {
                return Err(RelayAdmissionError::ChallengeConsumed);
            }
            let encoded = expires_at.to_be_bytes();
            table
                .insert(key.as_slice(), encoded.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)
    }
}

fn sequence_key(key: ReachabilitySequenceKeyV1) -> Vec<u8> {
    let mut output = Vec::with_capacity(66);
    output.push(b's');
    output.push(sequence_kind(key.kind));
    output.extend_from_slice(&key.signer);
    output.extend_from_slice(&key.scope);
    output
}

fn reservation_key(relay: NodeId, target: NodeId, reservation: [u8; 32]) -> Vec<u8> {
    let mut output = Vec::with_capacity(97);
    output.push(b'r');
    output.extend_from_slice(relay.as_bytes());
    output.extend_from_slice(target.as_bytes());
    output.extend_from_slice(&reservation);
    output
}

fn nonce_key(domain: ReachabilityNonceDomainV1, scope: [u8; 32], nonce: [u8; 32]) -> Vec<u8> {
    let mut output = Vec::with_capacity(66);
    output.push(b'n');
    output.push(nonce_domain(domain));
    output.extend_from_slice(&scope);
    output.extend_from_slice(&nonce);
    output
}

fn sequence_kind(value: ReachabilitySequenceKindV1) -> u8 {
    match value {
        ReachabilitySequenceKindV1::BootstrapManifest => 1,
        ReachabilitySequenceKindV1::RelayDescriptor => 2,
        ReachabilitySequenceKindV1::Advertisement => 3,
        ReachabilitySequenceKindV1::RelayReserveRequest => 4,
        ReachabilitySequenceKindV1::RelayKeepalive => 5,
        ReachabilitySequenceKindV1::RelayRevoke => 6,
        ReachabilitySequenceKindV1::ReflexiveObservation => 7,
        ReachabilitySequenceKindV1::RelayConnectRequest => 8,
        ReachabilitySequenceKindV1::PrivateCandidateSignal => 9,
    }
}

fn nonce_domain(value: ReachabilityNonceDomainV1) -> u8 {
    match value {
        ReachabilityNonceDomainV1::RelayControl => 1,
        ReachabilityNonceDomainV1::PossessionChallenge => 2,
        ReachabilityNonceDomainV1::HolePunchToken => 3,
        ReachabilityNonceDomainV1::RelayConnect => 4,
    }
}

fn encode_sequence(sequence: u64, digest: [u8; 32], expires_at: u64) -> Vec<u8> {
    let mut output = Vec::with_capacity(48);
    output.extend_from_slice(&sequence.to_be_bytes());
    output.extend_from_slice(&digest);
    output.extend_from_slice(&expires_at.to_be_bytes());
    output
}

fn decode_sequence(value: &[u8]) -> Result<(u64, [u8; 32], u64), RelayAdmissionError> {
    if value.len() != 48 {
        return Err(RelayAdmissionError::StateUnavailable);
    }
    let sequence = u64::from_be_bytes(
        value[..8]
            .try_into()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?,
    );
    let digest = value[8..40]
        .try_into()
        .map_err(|_| RelayAdmissionError::StateUnavailable)?;
    let expires_at = u64::from_be_bytes(
        value[40..]
            .try_into()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?,
    );
    Ok((sequence, digest, expires_at))
}

fn sync_parent(path: &Path) -> Result<(), RelayAdmissionError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
    }
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}
