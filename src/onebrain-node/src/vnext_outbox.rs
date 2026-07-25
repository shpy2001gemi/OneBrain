//! Durable, peer-authenticated outbound transfer intents for OBP-RP.
//!
//! Intents are independent of an ephemeral QUIC/session transcript. A runtime
//! binds them to a fresh authenticated context only after the connected peer's
//! NodeId matches `expected_peer`.

#![cfg(feature = "vnext-network-runtime")]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;

use ku_core::foundation::{
    DisclosureClass, NamespaceCommitment, NodeId, ReservedDomain, SelectorCid,
};
use onebrain_protocol::{ReconcileManifestKind, ReconcileReceiptStatus};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use thiserror::Error;

const OUTBOX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_outbound_intents");
const MAGIC: &[u8; 8] = b"OBOUTV1\0";
const FIXED_BYTES: usize = 206;
pub const MAX_OUTBOX_PAYLOAD_BYTES: usize = 1_048_576;
pub const MAX_OUTBOX_RECORDS: u64 = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutboundIntentState {
    Pending = 0,
    Acknowledged = 1,
    Rejected = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundTransferIntent {
    pub id: [u8; 32],
    pub expected_peer: NodeId,
    pub last_known_addr: SocketAddr,
    pub selector: SelectorCid,
    pub namespace: NamespaceCommitment,
    pub disclosure: DisclosureClass,
    pub kind: ReconcileManifestKind,
    pub cid: [u8; 32],
    pub canonical_bytes: Vec<u8>,
    pub attempts: u64,
    pub state: OutboundIntentState,
}

impl OutboundTransferIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        expected_peer: NodeId,
        last_known_addr: SocketAddr,
        selector: SelectorCid,
        namespace: NamespaceCommitment,
        disclosure: DisclosureClass,
        kind: ReconcileManifestKind,
        canonical_bytes: Vec<u8>,
    ) -> Result<Self, OutboundOutboxError> {
        if canonical_bytes.is_empty() || canonical_bytes.len() > MAX_OUTBOX_PAYLOAD_BYTES {
            return Err(OutboundOutboxError::PayloadLimit);
        }
        let cid = content_domain(kind).digest(&canonical_bytes);
        let id = intent_id(expected_peer, selector, namespace, disclosure, kind, cid);
        let intent = Self {
            id,
            expected_peer,
            last_known_addr,
            selector,
            namespace,
            disclosure,
            kind,
            cid,
            canonical_bytes,
            attempts: 0,
            state: OutboundIntentState::Pending,
        };
        validate_intent(&intent)?;
        Ok(intent)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboxEnqueueOutcome {
    Added,
    Existing,
    RouteUpdated,
}

#[derive(Clone)]
pub struct OutboundOutbox {
    db: Arc<Database>,
}

impl OutboundOutbox {
    pub fn open(path: &Path) -> Result<Self, OutboundOutboxError> {
        let db = Database::create(path).map_err(backend)?;
        let write = db.begin_write().map_err(backend)?;
        {
            write.open_table(OUTBOX).map_err(backend)?;
        }
        write.commit().map_err(backend)?;
        Ok(Self { db: Arc::new(db) })
    }

    pub fn enqueue(
        &self,
        intent: &OutboundTransferIntent,
    ) -> Result<OutboxEnqueueOutcome, OutboundOutboxError> {
        validate_intent(intent)?;
        let write = self.db.begin_write().map_err(backend)?;
        let outcome;
        {
            let mut table = write.open_table(OUTBOX).map_err(backend)?;
            let existing = table
                .get(intent.id.as_slice())
                .map_err(backend)?
                .map(|guard| guard.value().to_vec());
            match existing {
                Some(bytes) => {
                    let mut stored = decode_intent(&bytes)?;
                    if stored.id != intent.id
                        || stored.expected_peer != intent.expected_peer
                        || stored.selector != intent.selector
                        || stored.namespace != intent.namespace
                        || stored.disclosure != intent.disclosure
                        || stored.kind != intent.kind
                        || stored.cid != intent.cid
                        || stored.canonical_bytes != intent.canonical_bytes
                    {
                        return Err(OutboundOutboxError::IdentityCollision);
                    }
                    if stored.last_known_addr == intent.last_known_addr {
                        outcome = OutboxEnqueueOutcome::Existing;
                    } else {
                        stored.last_known_addr = intent.last_known_addr;
                        if stored.state == OutboundIntentState::Pending {
                            stored.attempts = 0;
                        }
                        let encoded = encode_intent(&stored)?;
                        table
                            .insert(intent.id.as_slice(), encoded.as_slice())
                            .map_err(backend)?;
                        outcome = OutboxEnqueueOutcome::RouteUpdated;
                    }
                }
                None => {
                    if table.len().map_err(backend)? >= MAX_OUTBOX_RECORDS {
                        return Err(OutboundOutboxError::RecordLimit);
                    }
                    let encoded = encode_intent(intent)?;
                    table
                        .insert(intent.id.as_slice(), encoded.as_slice())
                        .map_err(backend)?;
                    outcome = OutboxEnqueueOutcome::Added;
                }
            }
        }
        write.commit().map_err(backend)?;
        Ok(outcome)
    }

    pub fn pending(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboundTransferIntent>, OutboundOutboxError> {
        if limit == 0 || limit > MAX_OUTBOX_RECORDS as usize {
            return Err(OutboundOutboxError::InvalidLimit);
        }
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(OUTBOX).map_err(backend)?;
        let mut pending = Vec::new();
        for entry in table.iter().map_err(backend)? {
            let (_, value) = entry.map_err(backend)?;
            let intent = decode_intent(value.value())?;
            if intent.state == OutboundIntentState::Pending {
                pending.push(intent);
                if pending.len() == limit {
                    break;
                }
            }
        }
        Ok(pending)
    }

    pub fn get(
        &self,
        id: &[u8; 32],
    ) -> Result<Option<OutboundTransferIntent>, OutboundOutboxError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(OUTBOX).map_err(backend)?;
        table
            .get(id.as_slice())
            .map_err(backend)?
            .map(|guard| decode_intent(guard.value()))
            .transpose()
    }

    pub fn record_attempt(&self, id: &[u8; 32]) -> Result<u64, OutboundOutboxError> {
        self.update(id, |intent| {
            if intent.state == OutboundIntentState::Pending {
                intent.attempts = intent.attempts.saturating_add(1);
            }
            Ok(intent.attempts)
        })
    }

    pub fn apply_receipt(
        &self,
        id: &[u8; 32],
        status: ReconcileReceiptStatus,
    ) -> Result<OutboundIntentState, OutboundOutboxError> {
        self.update(id, |intent| {
            intent.state = match status {
                ReconcileReceiptStatus::ValidatedStored
                | ReconcileReceiptStatus::AlreadyPresent => OutboundIntentState::Acknowledged,
                ReconcileReceiptStatus::RejectedInvalid => OutboundIntentState::Rejected,
                ReconcileReceiptStatus::DeferredBudget
                | ReconcileReceiptStatus::DeferredMissingDependency => {
                    // Protocol-level deferral is non-terminal and must not
                    // consume the terminal retry budget.
                    intent.attempts = intent.attempts.saturating_sub(1);
                    OutboundIntentState::Pending
                }
            };
            Ok(intent.state)
        })
    }

    fn update<T>(
        &self,
        id: &[u8; 32],
        update: impl FnOnce(&mut OutboundTransferIntent) -> Result<T, OutboundOutboxError>,
    ) -> Result<T, OutboundOutboxError> {
        let write = self.db.begin_write().map_err(backend)?;
        let result;
        {
            let mut table = write.open_table(OUTBOX).map_err(backend)?;
            let bytes = table
                .get(id.as_slice())
                .map_err(backend)?
                .map(|guard| guard.value().to_vec())
                .ok_or(OutboundOutboxError::MissingIntent)?;
            let mut intent = decode_intent(&bytes)?;
            result = update(&mut intent)?;
            let encoded = encode_intent(&intent)?;
            table
                .insert(id.as_slice(), encoded.as_slice())
                .map_err(backend)?;
        }
        write.commit().map_err(backend)?;
        Ok(result)
    }
}

fn validate_intent(intent: &OutboundTransferIntent) -> Result<(), OutboundOutboxError> {
    if intent.canonical_bytes.is_empty()
        || intent.canonical_bytes.len() > MAX_OUTBOX_PAYLOAD_BYTES
        || !matches!(
            intent.disclosure,
            DisclosureClass::Public | DisclosureClass::RouteMinimal
        )
        || content_domain(intent.kind).digest(&intent.canonical_bytes) != intent.cid
        || intent_id(
            intent.expected_peer,
            intent.selector,
            intent.namespace,
            intent.disclosure,
            intent.kind,
            intent.cid,
        ) != intent.id
    {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    Ok(())
}

fn intent_id(
    expected_peer: NodeId,
    selector: SelectorCid,
    namespace: NamespaceCommitment,
    disclosure: DisclosureClass,
    kind: ReconcileManifestKind,
    cid: [u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:outbound-intent:1\0");
    hasher.update(expected_peer.as_bytes());
    hasher.update(selector.as_bytes());
    hasher.update(namespace.as_bytes());
    hasher.update(&[disclosure_code(disclosure), kind as u8]);
    hasher.update(&cid);
    *hasher.finalize().as_bytes()
}

fn content_domain(kind: ReconcileManifestKind) -> ReservedDomain {
    match kind {
        ReconcileManifestKind::Object => ReservedDomain::Object,
        ReconcileManifestKind::Event => ReservedDomain::Event,
        ReconcileManifestKind::MappingKernel => ReservedDomain::MappingKernel,
        ReconcileManifestKind::FeedInception => ReservedDomain::FeedInception,
        ReconcileManifestKind::AuthorityEvent => ReservedDomain::AuthorityEvent,
    }
}

fn disclosure_code(disclosure: DisclosureClass) -> u8 {
    match disclosure {
        DisclosureClass::Public => 0,
        DisclosureClass::NegotiatedEncrypted => 1,
        DisclosureClass::RouteMinimal => 2,
        DisclosureClass::LocalOnly => 3,
    }
}

fn parse_disclosure(value: u8) -> Result<DisclosureClass, OutboundOutboxError> {
    match value {
        0 => Ok(DisclosureClass::Public),
        1 => Ok(DisclosureClass::NegotiatedEncrypted),
        2 => Ok(DisclosureClass::RouteMinimal),
        3 => Ok(DisclosureClass::LocalOnly),
        _ => Err(OutboundOutboxError::InvalidRecord),
    }
}

fn parse_kind(value: u8) -> Result<ReconcileManifestKind, OutboundOutboxError> {
    match value {
        1 => Ok(ReconcileManifestKind::Object),
        2 => Ok(ReconcileManifestKind::Event),
        3 => Ok(ReconcileManifestKind::MappingKernel),
        4 => Ok(ReconcileManifestKind::FeedInception),
        5 => Ok(ReconcileManifestKind::AuthorityEvent),
        _ => Err(OutboundOutboxError::InvalidRecord),
    }
}

fn parse_state(value: u8) -> Result<OutboundIntentState, OutboundOutboxError> {
    match value {
        0 => Ok(OutboundIntentState::Pending),
        1 => Ok(OutboundIntentState::Acknowledged),
        2 => Ok(OutboundIntentState::Rejected),
        _ => Err(OutboundOutboxError::InvalidRecord),
    }
}

fn encode_intent(intent: &OutboundTransferIntent) -> Result<Vec<u8>, OutboundOutboxError> {
    validate_intent(intent)?;
    let mut output = Vec::with_capacity(FIXED_BYTES + intent.canonical_bytes.len());
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&intent.id);
    output.extend_from_slice(intent.expected_peer.as_bytes());
    match intent.last_known_addr.ip() {
        IpAddr::V4(ip) => {
            output.push(4);
            output.extend_from_slice(&ip.octets());
            output.extend_from_slice(&[0; 12]);
        }
        IpAddr::V6(ip) => {
            output.push(6);
            output.extend_from_slice(&ip.octets());
        }
    }
    output.extend_from_slice(&intent.last_known_addr.port().to_be_bytes());
    output.extend_from_slice(intent.selector.as_bytes());
    output.extend_from_slice(intent.namespace.as_bytes());
    output.push(disclosure_code(intent.disclosure));
    output.push(intent.kind as u8);
    output.extend_from_slice(&intent.cid);
    output.extend_from_slice(&intent.attempts.to_be_bytes());
    output.push(intent.state as u8);
    output.extend_from_slice(&(intent.canonical_bytes.len() as u64).to_be_bytes());
    output.extend_from_slice(&intent.canonical_bytes);
    debug_assert_eq!(output.len(), FIXED_BYTES + intent.canonical_bytes.len());
    Ok(output)
}

fn decode_intent(bytes: &[u8]) -> Result<OutboundTransferIntent, OutboundOutboxError> {
    if bytes.len() < FIXED_BYTES || &bytes[..8] != MAGIC {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let id = array32(&bytes[8..40])?;
    let expected_peer = NodeId::from_bytes(array32(&bytes[40..72])?);
    let family = bytes[72];
    let ip = match family {
        4 => {
            let octets: [u8; 4] = bytes[73..77]
                .try_into()
                .map_err(|_| OutboundOutboxError::InvalidRecord)?;
            IpAddr::V4(Ipv4Addr::from(octets))
        }
        6 => {
            let octets: [u8; 16] = bytes[73..89]
                .try_into()
                .map_err(|_| OutboundOutboxError::InvalidRecord)?;
            IpAddr::V6(Ipv6Addr::from(octets))
        }
        _ => return Err(OutboundOutboxError::InvalidRecord),
    };
    let port = u16::from_be_bytes(
        bytes[89..91]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    );
    let selector = SelectorCid::from_bytes(array32(&bytes[91..123])?);
    let namespace = NamespaceCommitment::from_bytes(array32(&bytes[123..155])?);
    let disclosure = parse_disclosure(bytes[155])?;
    let kind = parse_kind(bytes[156])?;
    let cid = array32(&bytes[157..189])?;
    let attempts = u64::from_be_bytes(
        bytes[189..197]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    );
    let state = parse_state(bytes[197])?;
    let payload_len = u64::from_be_bytes(
        bytes[198..206]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    ) as usize;
    if payload_len == 0
        || payload_len > MAX_OUTBOX_PAYLOAD_BYTES
        || bytes.len().checked_sub(FIXED_BYTES) != Some(payload_len)
    {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let intent = OutboundTransferIntent {
        id,
        expected_peer,
        last_known_addr: SocketAddr::new(ip, port),
        selector,
        namespace,
        disclosure,
        kind,
        cid,
        canonical_bytes: bytes[FIXED_BYTES..].to_vec(),
        attempts,
        state,
    };
    validate_intent(&intent)?;
    Ok(intent)
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], OutboundOutboxError> {
    bytes
        .try_into()
        .map_err(|_| OutboundOutboxError::InvalidRecord)
}

fn backend(error: impl std::fmt::Display) -> OutboundOutboxError {
    OutboundOutboxError::Backend(error.to_string())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OutboundOutboxError {
    #[error("outbox backend failed: {0}")]
    Backend(String),
    #[error("outbox record is corrupt or inconsistent")]
    InvalidRecord,
    #[error("outbox payload exceeds the reconciliation profile")]
    PayloadLimit,
    #[error("outbox record limit reached")]
    RecordLimit,
    #[error("outbox query limit is invalid")]
    InvalidLimit,
    #[error("outbox intent is missing")]
    MissingIntent,
    #[error("outbox identity collision")]
    IdentityCollision,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(addr: SocketAddr) -> OutboundTransferIntent {
        OutboundTransferIntent::new(
            NodeId::from_bytes([1; 32]),
            addr,
            SelectorCid::from_bytes([2; 32]),
            NamespaceCommitment::from_bytes([3; 32]),
            DisclosureClass::Public,
            ReconcileManifestKind::Object,
            b"canonical-outbox-test".to_vec(),
        )
        .unwrap()
    }

    #[test]
    fn pending_intent_survives_restart_and_receipt_is_terminal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("outbox.redb");
        let first = intent("127.0.0.1:5001".parse().unwrap());
        {
            let outbox = OutboundOutbox::open(&path).unwrap();
            assert_eq!(outbox.enqueue(&first).unwrap(), OutboxEnqueueOutcome::Added);
            assert_eq!(outbox.record_attempt(&first.id).unwrap(), 1);
        }
        let reopened = OutboundOutbox::open(&path).unwrap();
        let pending = reopened.pending(8).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].attempts, 1);
        assert_eq!(
            reopened
                .apply_receipt(&first.id, ReconcileReceiptStatus::ValidatedStored)
                .unwrap(),
            OutboundIntentState::Acknowledged
        );
        assert!(reopened.pending(8).unwrap().is_empty());
        assert_eq!(
            reopened.get(&first.id).unwrap().unwrap().state,
            OutboundIntentState::Acknowledged
        );
    }

    #[test]
    fn route_can_change_without_changing_identity_or_terminal_state() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let first = intent("127.0.0.1:5001".parse().unwrap());
        outbox.enqueue(&first).unwrap();
        let moved = intent("127.0.0.1:5002".parse().unwrap());
        assert_eq!(first.id, moved.id);
        assert_eq!(
            outbox.enqueue(&moved).unwrap(),
            OutboxEnqueueOutcome::RouteUpdated
        );
        assert_eq!(
            outbox.get(&first.id).unwrap().unwrap().last_known_addr,
            moved.last_known_addr
        );
    }

    #[test]
    fn private_disclosure_never_enters_network_outbox() {
        let result = OutboundTransferIntent::new(
            NodeId::from_bytes([1; 32]),
            "127.0.0.1:5001".parse().unwrap(),
            SelectorCid::from_bytes([2; 32]),
            NamespaceCommitment::from_bytes([3; 32]),
            DisclosureClass::LocalOnly,
            ReconcileManifestKind::Object,
            b"private".to_vec(),
        );
        assert_eq!(result.unwrap_err(), OutboundOutboxError::InvalidRecord);
    }

    #[test]
    fn missing_dependency_receipt_does_not_consume_retry_budget() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let intent = intent("127.0.0.1:5001".parse().unwrap());
        outbox.enqueue(&intent).unwrap();
        outbox.record_attempt(&intent.id).unwrap();
        assert_eq!(
            outbox
                .apply_receipt(
                    &intent.id,
                    ReconcileReceiptStatus::DeferredMissingDependency
                )
                .unwrap(),
            OutboundIntentState::Pending
        );
        assert_eq!(outbox.get(&intent.id).unwrap().unwrap().attempts, 0);
    }
}
