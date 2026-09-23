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
    /// Stream validation before product startup. Bound work by the host's
    /// existing storage budget; never discard old replay floors to fit a limit.
    pub(crate) fn open_validated(path: &Path, max_bytes: u64) -> Result<Self, RelayAdmissionError> {
        if path.exists()
            && std::fs::metadata(path)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .len()
                > max_bytes
        {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        let store = Self::open(path)?;
        {
            let read = store
                .database
                .begin_read()
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let table = read
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let mut bytes = 0_u64;
            for row in table
                .iter()
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
            {
                let (key, value) = row.map_err(|_| RelayAdmissionError::StateUnavailable)?;
                let key = key.value();
                let value = value.value();
                bytes = bytes
                    .checked_add((key.len() + value.len()) as u64)
                    .ok_or(RelayAdmissionError::BudgetExceeded)?;
                if bytes > max_bytes {
                    return Err(RelayAdmissionError::BudgetExceeded);
                }
                let valid = match key.first() {
                    Some(b's') => {
                        key.len() == 66
                            && (1..=9).contains(&key[1])
                            && decode_sequence(value).is_ok_and(|(sequence, digest, expires)| {
                                sequence > 0 && digest != [0; 32] && expires > 0
                            })
                    }
                    Some(b'r') => key.len() == 97 && value.len() == 40,
                    Some(b'n') => key.len() == 66 && (1..=4).contains(&key[1]) && value.len() == 8,
                    Some(b'c') => {
                        key.len() == 65
                            && value.len() <= 65_536
                            && blake3::hash(value).as_bytes() == &key[33..]
                            && cache_identity(value).is_ok()
                    }
                    Some(b'e') => key.len() == 33 && value.len() == 8,
                    Some(b'p') => key.len() == 33 && valid_pending(value),
                    _ => false,
                };
                if !valid {
                    return Err(RelayAdmissionError::StateUnavailable);
                }
            }
        }
        Ok(store)
    }

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

    pub(crate) fn cached_records(
        &self,
        source: [u8; 32],
    ) -> Result<Vec<Vec<u8>>, RelayAdmissionError> {
        let read = self
            .database
            .begin_read()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        let table = read
            .open_table(STATE)
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        let mut output = Vec::new();
        let mut bytes = 0usize;
        for row in table
            .iter()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?
        {
            let (key, value) = row.map_err(|_| RelayAdmissionError::StateUnavailable)?;
            if key.value().first() == Some(&b'c')
                && key.value().get(1..33) == Some(source.as_slice())
            {
                bytes = bytes
                    .checked_add(value.value().len())
                    .ok_or(RelayAdmissionError::BudgetExceeded)?;
                if output.len() >= 64 || bytes > 1_048_576 {
                    return Err(RelayAdmissionError::BudgetExceeded);
                }
                cache_identity(value.value())?;
                output.push(value.value().to_vec());
            }
        }
        Ok(output)
    }

    /// Persist only canonical signed inputs already admitted by the owner. Cache
    /// expiry never removes replay floors. Recovery revalidates signed bytes and
    /// live possession; this table contains no live carrier or lease.
    pub(crate) fn cache_signed(
        &self,
        source: [u8; 32],
        bytes: &[u8],
        now: u64,
    ) -> Result<(), RelayAdmissionError> {
        let identity = cache_identity(bytes)?;
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let mut remove = Vec::new();
            let (mut total, mut local, mut local_bytes) = (0usize, 0usize, 0usize);
            let mut sources = std::collections::BTreeSet::new();
            for row in table
                .iter()
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
            {
                let (key, value) = row.map_err(|_| RelayAdmissionError::StateUnavailable)?;
                if key.value().first() != Some(&b'c') {
                    continue;
                }
                let previous = cache_identity(value.value())?;
                let same_source = key.value().get(1..33) == Some(source.as_slice());
                if previous.2 <= now
                    || (same_source && previous.0 == identity.0 && previous.1 == identity.1)
                {
                    remove.push(key.value().to_vec());
                } else {
                    total += 1;
                    sources.insert(key.value()[1..33].to_vec());
                    if same_source {
                        local += 1;
                        local_bytes += value.value().len();
                    }
                }
            }
            sources.insert(source.to_vec());
            if sources.len() > 8
                || total >= 256
                || local >= 64
                || local_bytes.saturating_add(bytes.len()) > 1_048_576
            {
                return Err(RelayAdmissionError::BudgetExceeded);
            }
            for key in remove {
                table
                    .remove(key.as_slice())
                    .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            }
            let mut key = vec![b'c'];
            key.extend(source);
            key.extend(blake3::hash(bytes).as_bytes());
            table
                .insert(key.as_slice(), bytes)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)
    }

    pub(crate) fn is_current(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        digest: [u8; 32],
    ) -> Result<bool, RelayAdmissionError> {
        Ok(self
            .read(&sequence_key(key))?
            .map(|bytes| decode_sequence(&bytes))
            .transpose()?
            .is_some_and(|(current, hash, _)| current == sequence && hash == digest))
    }

    pub(crate) fn is_current_reservation(
        &self,
        relay: NodeId,
        target: NodeId,
        id: [u8; 32],
        digest: [u8; 32],
    ) -> Result<bool, RelayAdmissionError> {
        Ok(self
            .read(&reservation_key(relay, target, id))?
            .is_some_and(|bytes| bytes.get(..32) == Some(digest.as_slice())))
    }

    pub(crate) fn pending_local(
        &self,
        scope: [u8; 32],
    ) -> Result<Option<Vec<u8>>, RelayAdmissionError> {
        let mut key = vec![b'p'];
        key.extend(scope);
        self.read(&key)?
            .map(|value| {
                if !valid_pending(&value) {
                    return Err(RelayAdmissionError::StateUnavailable);
                }
                Ok(value[32..].to_vec())
            })
            .transpose()
    }

    /// Atomic sequence allocation + exact request checkpoint before I/O. An
    /// unknown result stays pending and cannot consume a second sequence.
    pub(crate) fn prepare_local(
        &self,
        scope: [u8; 32],
        make: impl FnOnce(u64) -> Result<Vec<u8>, RelayAdmissionError>,
    ) -> Result<Vec<u8>, RelayAdmissionError> {
        let mut key = vec![b'e'];
        key.extend(scope);
        let mut pending = vec![b'p'];
        pending.extend(scope);
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        let bytes;
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            if table
                .get(pending.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .is_some()
            {
                return Err(RelayAdmissionError::Replay);
            }
            let previous = table
                .get(key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .map(|value| value.value().to_vec());
            let previous = previous
                .map(|value| <[u8; 8]>::try_from(value).map(u64::from_be_bytes))
                .transpose()
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .unwrap_or(0);
            let next = previous
                .checked_add(1)
                .ok_or(RelayAdmissionError::SequenceRollback)?;
            bytes = make(next)?;
            if bytes.is_empty() || bytes.len() > 65_536 {
                return Err(RelayAdmissionError::BudgetExceeded);
            }
            table
                .insert(key.as_slice(), next.to_be_bytes().as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let mut envelope = blake3::hash(&bytes).as_bytes().to_vec();
            envelope.extend_from_slice(&bytes);
            table
                .insert(pending.as_slice(), envelope.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        Ok(bytes)
    }

    pub(crate) fn finish_local(
        &self,
        scope: [u8; 32],
        bytes: &[u8],
    ) -> Result<(), RelayAdmissionError> {
        let mut key = vec![b'p'];
        key.extend(scope);
        let mut write = self
            .database
            .begin_write()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write
                .open_table(STATE)
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let matches = table
                .get(key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?
                .is_some_and(|value| valid_pending(value.value()) && &value.value()[32..] == bytes);
            if !matches {
                return Err(RelayAdmissionError::StateUnavailable);
            }
            table
                .remove(key.as_slice())
                .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        }
        write
            .commit()
            .map_err(|_| RelayAdmissionError::StateUnavailable)
    }
}

fn valid_pending(value: &[u8]) -> bool {
    (33..=65_568).contains(&value.len()) && blake3::hash(&value[32..]).as_bytes() == &value[..32]
}

fn cache_identity(bytes: &[u8]) -> Result<(u8, [u8; 32], u64), RelayAdmissionError> {
    use onebrain_protocol::{decode_reachability_object, ReachabilityObjectV1};
    match decode_reachability_object(bytes).map_err(|_| RelayAdmissionError::Codec)? {
        ReachabilityObjectV1::BootstrapManifest(value) => {
            Ok((1, value.discovery_source_id, value.expires_at))
        }
        ReachabilityObjectV1::RelayDescriptor(value) => {
            Ok((2, *value.relay_node_id.as_bytes(), value.expires_at))
        }
        ReachabilityObjectV1::Advertisement(value) => {
            Ok((3, *value.target_node_id.as_bytes(), value.expires_at))
        }
        _ => Err(RelayAdmissionError::Codec),
    }
}

impl ReachabilityReplayStore for RedbReachabilityReplayStore {
    fn descriptor_history(&self, history: &ku_net::vnext_reachability_crypto::VerifiedDescriptorHistory, commit: bool) -> Result<(), RelayAdmissionError> {
        let storage_key = sequence_key(history.key());
        let mut write = self.database.begin_write().map_err(|_| RelayAdmissionError::StateUnavailable)?;
        write.set_durability(Durability::Immediate);
        {
            let mut table = write.open_table(STATE).map_err(|_| RelayAdmissionError::StateUnavailable)?;
            let current = table.get(storage_key.as_slice()).map_err(|_| RelayAdmissionError::StateUnavailable)?
                .map(|v| decode_sequence(v.value())).transpose()?;
            history.check_floor(current)?;
            if commit {
                let (seq, digest, expires) = history.terminal_floor();
                let encoded = encode_sequence(seq,digest,expires);
                table.insert(storage_key.as_slice(),encoded.as_slice()).map_err(|_| RelayAdmissionError::StateUnavailable)?;
            }
        }
        if commit { write.commit().map_err(|_| RelayAdmissionError::StateUnavailable)?; }
        Ok(())
    }
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
                if current.checked_add(1) == Some(sequence) && previous_digest == Some(digest) {
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
                    if current_sequence.checked_add(1) != Some(sequence)
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
                    if current_sequence.checked_add(1) != Some(sequence) {
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

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn descriptor_history_commit_is_atomic_and_survives_restart() {
        use ed25519_dalek::{Signer, SigningKey};
        use onebrain_protocol::*;
        use ku_net::vnext_reachability_crypto::VerifiedDescriptorHistory;
        let key = SigningKey::from_bytes(&[17;32]);
        let mut value = RelayDescriptorV1 { format:1,
            relay_node_id:ku_net::vnext_session::principal_node_id(key.verifying_key().as_bytes()),
            relay_public_key:*key.verifying_key().as_bytes(),
            endpoints:vec![RelayEndpointV1 {transport:RelayTransportV1::TlsTcp443,host:HostAddressV1::Ipv4([8,8,8,8]),port:443}],
            supported_transports:vec![RelayTransportV1::TlsTcp443],protocol_versions:vec![ProtocolVersionV1 {major:1,minor:0}],
            capacity_policy_digest:[4;32],previous_descriptor_blake3:None,sequence:1,issued_at:100,expires_at:600,relay_signature:[0;64] };
        let sign = |value: &mut RelayDescriptorV1| {
            value.relay_signature=key.sign(&reachability_signing_bytes(&ReachabilityObjectV1::RelayDescriptor(value.clone()),ReachabilitySignatureRoleV1::RelayDescriptor).unwrap()).to_bytes();
            encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(value.clone())).unwrap()
        };
        let first=sign(&mut value);
        value.sequence=2;value.previous_descriptor_blake3=Some(*blake3::hash(&first).as_bytes());value.issued_at=1000;value.expires_at=1500;
        let second=sign(&mut value);
        let history=VerifiedDescriptorHistory::verify(&[first],&second,1000).unwrap();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("history.redb");
        {
            let store=RedbReachabilityReplayStore::open(&path).unwrap();
            store.descriptor_history(&history,false).unwrap();
            assert!(store.check_sequence_candidate(history.key(),1,None).is_ok());
            store.descriptor_history(&history,true).unwrap();
        }
        let store=RedbReachabilityReplayStore::open(&path).unwrap();
        store.descriptor_history(&history,false).unwrap();
        store.descriptor_history(&history,true).unwrap();
        let (seq,digest,_)=history.terminal_floor();
        store.compare_and_advance_sequence(history.key(),Some(digest),seq+1,[42;32],1600).unwrap();
        assert!(store.descriptor_history(&history,true).is_err());
        assert!(store.check_sequence_candidate(history.key(),4,Some([42;32])).is_ok());
    }

    #[test]
    fn corrupted_pending_emission_is_rejected_on_read_and_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pending.redb");
        {
            let store = RedbReachabilityReplayStore::open(&path).unwrap();
            store
                .prepare_local([7; 32], |seq| Ok(seq.to_be_bytes().to_vec()))
                .unwrap();
            let mut key = vec![b'p'];
            key.extend([7; 32]);
            let write = store.database.begin_write().unwrap();
            {
                let mut table = write.open_table(STATE).unwrap();
                let mut bytes = table.get(key.as_slice()).unwrap().unwrap().value().to_vec();
                *bytes.last_mut().unwrap() ^= 1;
                table.insert(key.as_slice(), bytes.as_slice()).unwrap();
            }
            write.commit().unwrap();
            assert_eq!(
                store.pending_local([7; 32]),
                Err(RelayAdmissionError::StateUnavailable)
            );
        }
        assert!(matches!(
            RedbReachabilityReplayStore::open_validated(&path, 64 * 1024 * 1024),
            Err(RelayAdmissionError::StateUnavailable)
        ));
        assert!(path.exists());
    }

    #[test]
    fn recovery_rejects_corrupt_rows_and_oversized_store_without_deleting_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replay.redb");
        {
            let store = RedbReachabilityReplayStore::open(&path).unwrap();
            let write = store.database.begin_write().unwrap();
            {
                let mut table = write.open_table(STATE).unwrap();
                table
                    .insert(b"unknown".as_slice(), b"bad".as_slice())
                    .unwrap();
            }
            write.commit().unwrap();
        }
        assert!(matches!(
            RedbReachabilityReplayStore::open_validated(&path, 1),
            Err(RelayAdmissionError::BudgetExceeded)
        ));
        assert!(matches!(
            RedbReachabilityReplayStore::open_validated(&path, 64 * 1024 * 1024),
            Err(RelayAdmissionError::StateUnavailable)
        ));
        assert!(path.exists());
    }

    #[test]
    fn exhausted_replay_sequence_fails_closed_without_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replay.redb");
        let key = ReachabilitySequenceKeyV1 {
            kind: ReachabilitySequenceKindV1::Advertisement,
            signer: [1; 32],
            scope: [2; 32],
        };
        {
            let store = RedbReachabilityReplayStore::open(&path).unwrap();
            let write = store.database.begin_write().unwrap();
            {
                let mut table = write.open_table(STATE).unwrap();
                table
                    .insert(
                        sequence_key(key).as_slice(),
                        encode_sequence(u64::MAX, [3; 32], 100).as_slice(),
                    )
                    .unwrap();
            }
            write.commit().unwrap();
        }
        let store = RedbReachabilityReplayStore::open_validated(&path, 64 * 1024 * 1024).unwrap();
        assert!(store
            .check_sequence_candidate(key, 0, Some([3; 32]))
            .is_err());
        assert!(store
            .compare_and_advance_sequence(key, Some([3; 32]), 0, [4; 32], 101)
            .is_err());
        assert!(store
            .check_and_advance_sequence(key, 0, [4; 32], 101)
            .is_err());
    }
}
