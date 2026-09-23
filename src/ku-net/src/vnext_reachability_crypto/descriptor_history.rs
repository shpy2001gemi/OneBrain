//! Explicit, bounded cold-start/recovery input. History never grants live authority.
use super::*;

pub const MAX_DESCRIPTOR_CHAIN_OBJECTS: usize = 16;
pub const MAX_DESCRIPTOR_CHAIN_BYTES: usize = 8192;

#[derive(Clone)]
pub struct VerifiedDescriptorHistory {
    key: ReachabilitySequenceKeyV1,
    floors: Vec<(u64, [u8; 32], u64)>,
    terminal_previous: Option<[u8; 32]>,
}

impl VerifiedDescriptorHistory {
    pub fn verify(
        predecessors: &[Vec<u8>],
        terminal: &[u8],
        now: u64,
    ) -> Result<Self, RelayAdmissionError> {
        if predecessors.len() >= MAX_DESCRIPTOR_CHAIN_OBJECTS
            || predecessors
                .iter()
                .try_fold(terminal.len(), |n, b| n.checked_add(b.len()))
                .is_none_or(|n| n > MAX_DESCRIPTOR_CHAIN_BYTES)
        {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        let mut previous: Option<RelayDescriptorV1> = None;
        let mut floors = Vec::new();
        for bytes in predecessors
            .iter()
            .map(Vec::as_slice)
            .chain(std::iter::once(terminal))
        {
            let ReachabilityObjectV1::RelayDescriptor(value) =
                decode_reachability_object(bytes).map_err(|_| RelayAdmissionError::Codec)?
            else {
                return Err(RelayAdmissionError::Codec);
            };
            if principal_node_id(&value.relay_public_key) != value.relay_node_id {
                return Err(RelayAdmissionError::IdentityMismatch);
            }
            verify_object_signature(
                &ReachabilityObjectV1::RelayDescriptor(value.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
                value.relay_public_key,
                value.relay_signature,
            )?;
            if value.expires_at <= value.issued_at || value.expires_at - value.issued_at > 1800 {
                return Err(RelayAdmissionError::Expired);
            }
            match &previous {
                None if value.sequence == 1 && value.previous_descriptor_blake3.is_none() => {}
                Some(prior)
                    if prior.sequence.checked_add(1) == Some(value.sequence)
                        && value.previous_descriptor_blake3
                            == floors.last().map(|v: &(u64, [u8; 32], u64)| v.1)
                        && value.relay_public_key == prior.relay_public_key
                        && value.relay_node_id == prior.relay_node_id
                        && value.endpoints == prior.endpoints
                        && value.supported_transports == prior.supported_transports
                        && value.protocol_versions == prior.protocol_versions
                        && value.capacity_policy_digest == prior.capacity_policy_digest
                        && value.issued_at >= prior.issued_at
                        && value.expires_at > prior.expires_at => {}
                _ => return Err(RelayAdmissionError::SequenceRollback),
            }
            floors.push((
                value.sequence,
                *blake3::hash(bytes).as_bytes(),
                value.expires_at,
            ));
            previous = Some(value);
        }
        let value = previous.ok_or(RelayAdmissionError::Codec)?;
        freshness(value.issued_at, value.expires_at, now)?;
        Ok(Self {
            key: sequence_key(
                ReachabilitySequenceKindV1::RelayDescriptor,
                value.relay_public_key,
            ),
            floors,
            terminal_previous: value.previous_descriptor_blake3,
        })
    }
    pub fn key(&self) -> ReachabilitySequenceKeyV1 {
        self.key
    }
    pub fn terminal_floor(&self) -> (u64, [u8; 32], u64) {
        *self.floors.last().expect("verified nonempty chain")
    }
    pub fn check_floor(
        &self,
        current: Option<(u64, [u8; 32], u64)>,
    ) -> Result<(), RelayAdmissionError> {
        if current.is_none() || current.is_some_and(|v| self.floors.contains(&v)) {
            Ok(())
        } else {
            Err(RelayAdmissionError::SequenceRollback)
        }
    }
}

/// Only exact explicitly verified terminal inputs receive history recovery.
/// All unrelated replay/control behavior stays with the injected node-owned store.
pub struct DescriptorHistoryReplayStore {
    inner: Arc<dyn ReachabilityReplayStore>,
    histories: Mutex<BTreeMap<ReachabilitySequenceKeyV1, VerifiedDescriptorHistory>>,
}
impl DescriptorHistoryReplayStore {
    pub fn new(inner: Arc<dyn ReachabilityReplayStore>) -> Self {
        Self {
            inner,
            histories: Mutex::new(BTreeMap::new()),
        }
    }
    pub fn install(
        &self,
        histories: Vec<VerifiedDescriptorHistory>,
    ) -> Result<(), RelayAdmissionError> {
        if histories.len() > 3 {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        let mut next = BTreeMap::new();
        for history in histories {
            self.inner.descriptor_history(&history, false)?;
            if next.insert(history.key(), history).is_some() {
                return Err(RelayAdmissionError::Replay);
            }
        }
        *self
            .histories
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)? = next;
        Ok(())
    }
}
impl ReachabilityReplayStore for DescriptorHistoryReplayStore {
    fn check_sequence_candidate(
        &self,
        key: ReachabilitySequenceKeyV1,
        seq: u64,
        previous: Option<[u8; 32]>,
    ) -> Result<(), RelayAdmissionError> {
        let histories = self
            .histories
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        if let Some(h) = histories.get(&key) {
            if h.terminal_floor().0 != seq || h.terminal_previous != previous {
                return Err(RelayAdmissionError::SequenceRollback);
            }
            return self.inner.descriptor_history(h, false);
        }
        self.inner.check_sequence_candidate(key, seq, previous)
    }
    fn compare_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        previous: Option<[u8; 32]>,
        seq: u64,
        digest: [u8; 32],
        expires: u64,
    ) -> Result<(), RelayAdmissionError> {
        let histories = self
            .histories
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        if let Some(h) = histories.get(&key) {
            if h.terminal_floor() != (seq, digest, expires) || h.terminal_previous != previous {
                return Err(RelayAdmissionError::SequenceRollback);
            }
            return self.inner.descriptor_history(h, true);
        }
        self.inner
            .compare_and_advance_sequence(key, previous, seq, digest, expires)
    }
    fn check_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        seq: u64,
        digest: [u8; 32],
        expires: u64,
    ) -> Result<(), RelayAdmissionError> {
        self.inner
            .check_and_advance_sequence(key, seq, digest, expires)
    }
    fn check_and_store_reservation(
        &self,
        relay: NodeId,
        target: NodeId,
        id: [u8; 32],
        digest: [u8; 32],
        expires: u64,
    ) -> Result<(), RelayAdmissionError> {
        self.inner
            .check_and_store_reservation(relay, target, id, digest, expires)
    }
    fn consume_nonce(
        &self,
        domain: ReachabilityNonceDomainV1,
        scope: [u8; 32],
        nonce: [u8; 32],
        expires: u64,
    ) -> Result<(), RelayAdmissionError> {
        self.inner.consume_nonce(domain, scope, nonce, expires)
    }
}
