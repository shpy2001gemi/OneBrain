//! Durable, peer-authenticated outbound transfer intents for OBP-RP.
//!
//! Intents are independent of an ephemeral QUIC/session transcript. A runtime
//! binds them to a fresh authenticated context only after the connected peer's
//! NodeId matches `expected_peer`.

#![cfg(feature = "vnext-network-runtime")]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ku_core::foundation::{
    dr_m5_failpoint, DisclosureClass, NamespaceCommitment, NodeId, OperationalCompactionPermit,
    ReservedDomain, SelectorCid,
};
use onebrain_protocol::{ReconcileManifestKind, ReconcileReceiptStatus};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use thiserror::Error;

use crate::archive::{PortableArchiveRow, PortableArchiveRows};
use crate::error::NodeError;
use onebrain_archive::{ArchiveEntryKind, ArchiveOwner};

const OUTBOX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_outbound_intents");
const META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("vnext_outbound_meta_v2");
const TOMBSTONES: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_outbound_terminal_tombstones_v2");
const MAGIC_V1: &[u8; 8] = b"OBOUTV1\0";
const MAGIC_V2: &[u8; 8] = b"OBOUTV2\0";
const MAGIC_V3: &[u8; 8] = b"OBOUTV3\0";
const FIXED_BYTES_V1: usize = 206;
const FIXED_BYTES_V2: usize = 222;
const FIXED_BYTES_V3: usize = 238;
const TOMBSTONE_BYTES_V1: usize = 25;
const TOMBSTONE_BYTES_V2: usize = 89;
const FAIR_CURSOR_KEY: &[u8] = b"fair_cursor";
const TERMINAL_HEAD_KEY: &[u8] = b"terminal_head";
pub const MAX_OUTBOX_PAYLOAD_BYTES: usize = 1_048_576;
pub const MAX_OUTBOX_RECORDS: u64 = 65_536;
pub const MAX_OUTBOX_TOMBSTONES: u64 = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutboundIntentState {
    Pending = 0,
    Acknowledged = 1,
    DeadLetter = 2,
    RetryExhausted = 3,
}

impl OutboundIntentState {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
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
    pub transport_attempts: u64,
    pub validation_retries: u64,
    pub terminal_sequence: u64,
    pub enqueued_at_unix_seconds: u64,
    pub updated_at_unix_seconds: u64,
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
        let now = unix_seconds();
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
            transport_attempts: 0,
            validation_retries: 0,
            terminal_sequence: 0,
            enqueued_at_unix_seconds: now,
            updated_at_unix_seconds: now,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutboundOutboxStats {
    pub total: u64,
    pub pending: u64,
    pub acknowledged: u64,
    pub dead_letter: u64,
    pub retry_exhausted: u64,
    pub oldest_pending_age_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundAuditTombstone {
    pub intent_id: [u8; 32],
    pub state: OutboundIntentState,
    pub terminal_sequence: u64,
    pub transport_attempts: u64,
    pub validation_retries: u64,
    pub cid: [u8; 32],
    pub payload_digest: [u8; 32],
    pub legacy_without_payload_digest: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutboundCompactionReport {
    pub removed_records: u64,
    pub removed_payload_bytes: u64,
    pub retained_pending: u64,
    pub retained_terminal: u64,
    pub audit_tombstones: u64,
    pub audit_root: [u8; 32],
}

#[derive(Clone)]
pub struct OutboundOutbox {
    db: Arc<Database>,
    path: Arc<PathBuf>,
}

impl OutboundOutbox {
    pub fn open(path: &Path) -> Result<Self, OutboundOutboxError> {
        let path = path.to_path_buf();
        let db = Database::create(&path).map_err(backend)?;
        let write = db.begin_write().map_err(backend)?;
        {
            write.open_table(OUTBOX).map_err(backend)?;
            write.open_table(META).map_err(backend)?;
            write.open_table(TOMBSTONES).map_err(backend)?;
        }
        write.commit().map_err(backend)?;
        Ok(Self {
            db: Arc::new(db),
            path: Arc::new(path),
        })
    }

    pub fn enqueue(
        &self,
        intent: &OutboundTransferIntent,
    ) -> Result<OutboxEnqueueOutcome, OutboundOutboxError> {
        validate_intent(intent)?;
        dr_m5_failpoint::hit("TX-OUT-001", "before_begin_write");
        let write = self.db.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit("TX-OUT-001", "after_begin_write_before_mutation");
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
                            stored.transport_attempts = 0;
                        }
                        touch(&mut stored);
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
        dr_m5_failpoint::hit("TX-OUT-001", "after_mutation_before_commit");
        write.commit().map_err(backend)?;
        dr_m5_failpoint::hit("TX-OUT-001", "after_commit_before_next_side_effect");
        dr_m5_failpoint::hit("TX-OUT-001", "after_next_side_effect_before_ack");
        Ok(outcome)
    }

    pub fn pending(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboundTransferIntent>, OutboundOutboxError> {
        self.pending_fair(limit)
    }

    /// Persisted round-robin scan. Terminal and exhausted records cannot pin
    /// the first page, and one invocation inspects at most the hard store cap.
    pub fn pending_fair(
        &self,
        limit: usize,
    ) -> Result<Vec<OutboundTransferIntent>, OutboundOutboxError> {
        if limit == 0 || limit > MAX_OUTBOX_RECORDS as usize {
            return Err(OutboundOutboxError::InvalidLimit);
        }
        let write = self.db.begin_write().map_err(backend)?;
        let mut pending = Vec::new();
        {
            let table = write.open_table(OUTBOX).map_err(backend)?;
            let meta = write.open_table(META).map_err(backend)?;
            let cursor = meta
                .get(FAIR_CURSOR_KEY)
                .map_err(backend)?
                .map(|value| value.value().to_vec());
            let mut rows = Vec::with_capacity(table.len().map_err(backend)? as usize);
            for entry in table.iter().map_err(backend)? {
                let (key, value) = entry.map_err(backend)?;
                rows.push((key.value().to_vec(), value.value().to_vec()));
            }
            drop(table);
            drop(meta);

            if !rows.is_empty() {
                let start = cursor
                    .as_deref()
                    .and_then(|cursor| rows.iter().position(|(key, _)| key.as_slice() > cursor))
                    .unwrap_or(0);
                let mut last_inspected = None;
                for offset in 0..rows.len().min(MAX_OUTBOX_RECORDS as usize) {
                    let index = (start + offset) % rows.len();
                    let (key, value) = &rows[index];
                    last_inspected = Some(key.clone());
                    let intent = decode_intent(value)?;
                    if intent.state == OutboundIntentState::Pending {
                        pending.push(intent);
                        if pending.len() == limit {
                            break;
                        }
                    }
                }
                if let Some(cursor) = last_inspected {
                    write
                        .open_table(META)
                        .map_err(backend)?
                        .insert(FAIR_CURSOR_KEY, cursor.as_slice())
                        .map_err(backend)?;
                }
            }
        }
        write.commit().map_err(backend)?;
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

    pub fn stats(&self) -> Result<OutboundOutboxStats, OutboundOutboxError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(OUTBOX).map_err(backend)?;
        let now = unix_seconds();
        let mut stats = OutboundOutboxStats::default();
        let mut oldest_pending = None::<u64>;
        let mut pending_age_known = true;
        for entry in table.iter().map_err(backend)? {
            let (_, value) = entry.map_err(backend)?;
            let intent = decode_intent(value.value())?;
            stats.total = stats.total.saturating_add(1);
            match intent.state {
                OutboundIntentState::Pending => {
                    stats.pending = stats.pending.saturating_add(1);
                    if intent.enqueued_at_unix_seconds == 0 {
                        pending_age_known = false;
                    } else {
                        oldest_pending = Some(
                            oldest_pending
                                .unwrap_or(intent.enqueued_at_unix_seconds)
                                .min(intent.enqueued_at_unix_seconds),
                        );
                    }
                }
                OutboundIntentState::Acknowledged => {
                    stats.acknowledged = stats.acknowledged.saturating_add(1);
                }
                OutboundIntentState::DeadLetter => {
                    stats.dead_letter = stats.dead_letter.saturating_add(1);
                }
                OutboundIntentState::RetryExhausted => {
                    stats.retry_exhausted = stats.retry_exhausted.saturating_add(1);
                }
            }
        }
        stats.oldest_pending_age_seconds = if pending_age_known {
            oldest_pending.map(|timestamp| now.saturating_sub(timestamp))
        } else {
            None
        };
        Ok(stats)
    }

    pub fn record_transport_attempt(&self, id: &[u8; 32]) -> Result<u64, OutboundOutboxError> {
        self.update(id, "TX-OUT-001", |intent| {
            if intent.state == OutboundIntentState::Pending {
                intent.transport_attempts = intent.transport_attempts.saturating_add(1);
            }
            Ok(intent.transport_attempts)
        })
    }

    pub fn mark_retry_exhausted(
        &self,
        id: &[u8; 32],
        max_transport_attempts: u64,
    ) -> Result<OutboundIntentState, OutboundOutboxError> {
        if max_transport_attempts == 0 {
            return Err(OutboundOutboxError::InvalidLimit);
        }
        self.update_terminal(id, "TX-OUT-001", |intent| {
            if intent.state == OutboundIntentState::Pending
                && intent.transport_attempts >= max_transport_attempts
            {
                intent.state = OutboundIntentState::RetryExhausted;
            }
            Ok(intent.state)
        })
    }

    pub fn apply_receipt(
        &self,
        id: &[u8; 32],
        status: ReconcileReceiptStatus,
        max_validation_retries: u64,
    ) -> Result<OutboundIntentState, OutboundOutboxError> {
        if max_validation_retries == 0 {
            return Err(OutboundOutboxError::InvalidLimit);
        }
        self.update_terminal(id, "TX-OUT-002", |intent| {
            if intent.state.is_terminal() {
                return Ok(intent.state);
            }
            // A protocol receipt proves transport delivery. Only the separate
            // validation retry counter survives a non-terminal deferral.
            intent.transport_attempts = 0;
            intent.state = match status {
                ReconcileReceiptStatus::ValidatedStored
                | ReconcileReceiptStatus::AlreadyPresent => OutboundIntentState::Acknowledged,
                ReconcileReceiptStatus::RejectedInvalid => OutboundIntentState::DeadLetter,
                ReconcileReceiptStatus::DeferredBudget
                | ReconcileReceiptStatus::DeferredMissingDependency => {
                    intent.validation_retries = intent.validation_retries.saturating_add(1);
                    if intent.validation_retries >= max_validation_retries {
                        OutboundIntentState::RetryExhausted
                    } else {
                        OutboundIntentState::Pending
                    }
                }
            };
            Ok(intent.state)
        })
    }

    /// Remove old terminal payloads while atomically retaining bounded audit
    /// tombstones. Pending work is never compacted, and a stale kill-switch
    /// generation aborts before commit.
    pub fn compact_terminal(
        &self,
        permit: &OperationalCompactionPermit,
        retain_latest: usize,
        limit: usize,
    ) -> Result<OutboundCompactionReport, OutboundOutboxError> {
        if limit == 0 || limit > MAX_OUTBOX_RECORDS as usize {
            return Err(OutboundOutboxError::InvalidLimit);
        }
        permit
            .ensure_current()
            .map_err(|_| OutboundOutboxError::CompactionFenced)?;
        dr_m5_failpoint::hit("TX-CMP-OUT-001", "before_begin_write");
        let write = self.db.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit("TX-CMP-OUT-001", "after_begin_write_before_mutation");
        let mut terminal = Vec::new();
        let mut pending_count = 0u64;
        {
            let table = write.open_table(OUTBOX).map_err(backend)?;
            for entry in table.iter().map_err(backend)? {
                let (key, value) = entry.map_err(backend)?;
                let intent = decode_intent(value.value())?;
                if intent.state.is_terminal() {
                    terminal.push((
                        intent.terminal_sequence,
                        key.value().to_vec(),
                        encode_tombstone(&intent),
                        intent.canonical_bytes.len() as u64,
                    ));
                } else {
                    pending_count = pending_count.saturating_add(1);
                }
            }
        }
        terminal.sort_by_key(|(sequence, key, _, _)| (*sequence, key.clone()));
        let remove_count = terminal.len().saturating_sub(retain_latest).min(limit);
        let removed_payload_bytes = terminal
            .iter()
            .take(remove_count)
            .map(|(_, _, _, payload_bytes)| *payload_bytes)
            .sum();
        {
            let mut outbox = write.open_table(OUTBOX).map_err(backend)?;
            let mut tombstones = write.open_table(TOMBSTONES).map_err(backend)?;
            for (_, key, tombstone, _) in terminal.iter().take(remove_count) {
                if tombstones.len().map_err(backend)? >= MAX_OUTBOX_TOMBSTONES {
                    remove_oldest_tombstone(&mut tombstones)?;
                }
                tombstones
                    .insert(key.as_slice(), tombstone.as_slice())
                    .map_err(backend)?;
                outbox.remove(key.as_slice()).map_err(backend)?;
            }
        }
        dr_m5_failpoint::hit("TX-CMP-OUT-001", "after_mutation_before_commit");
        permit
            .run_if_current(|| write.commit())
            .map_err(|_| OutboundOutboxError::CompactionFenced)?
            .map_err(backend)?;
        dr_m5_failpoint::hit("TX-CMP-OUT-001", "after_commit_before_next_side_effect");
        let audit_tombstones = self.tombstone_count()?;
        let audit_root = self.audit_root()?;
        dr_m5_failpoint::hit("TX-CMP-OUT-001", "after_next_side_effect_before_ack");
        Ok(OutboundCompactionReport {
            removed_records: remove_count as u64,
            removed_payload_bytes,
            retained_pending: pending_count,
            retained_terminal: terminal.len().saturating_sub(remove_count) as u64,
            audit_tombstones,
            audit_root,
        })
    }

    pub fn tombstone(
        &self,
        id: &[u8; 32],
    ) -> Result<Option<OutboundAuditTombstone>, OutboundOutboxError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(TOMBSTONES).map_err(backend)?;
        table
            .get(id.as_slice())
            .map_err(backend)?
            .map(|value| decode_tombstone(*id, value.value()))
            .transpose()
    }

    pub fn tombstone_count(&self) -> Result<u64, OutboundOutboxError> {
        let read = self.db.begin_read().map_err(backend)?;
        read.open_table(TOMBSTONES)
            .map_err(backend)?
            .len()
            .map_err(backend)
    }

    pub fn audit_root(&self) -> Result<[u8; 32], OutboundOutboxError> {
        let read = self.db.begin_read().map_err(backend)?;
        let table = read.open_table(TOMBSTONES).map_err(backend)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:outbox-compaction-audit:1\0");
        for entry in table.iter().map_err(backend)? {
            let (key, value) = entry.map_err(backend)?;
            let id = array32(key.value())?;
            let tombstone = decode_tombstone(id, value.value())?;
            hasher.update(&tombstone.intent_id);
            hasher.update(&[tombstone.state as u8]);
            hasher.update(&tombstone.terminal_sequence.to_be_bytes());
            hasher.update(&tombstone.transport_attempts.to_be_bytes());
            hasher.update(&tombstone.validation_retries.to_be_bytes());
            hasher.update(&tombstone.cid);
            hasher.update(&tombstone.payload_digest);
            hasher.update(&[u8::from(tombstone.legacy_without_payload_digest)]);
        }
        Ok(*hasher.finalize().as_bytes())
    }

    pub fn disk_bytes(&self) -> Result<u64, OutboundOutboxError> {
        std::fs::metadata(self.path.as_ref())
            .map(|metadata| metadata.len())
            .map_err(backend)
    }

    /// Reclaim free Redb pages after logical compaction. The outbox must not
    /// have live clones so the database can be borrowed exclusively.
    pub fn reclaim_disk(
        &mut self,
        permit: &OperationalCompactionPermit,
    ) -> Result<bool, OutboundOutboxError> {
        permit
            .ensure_current()
            .map_err(|_| OutboundOutboxError::CompactionFenced)?;
        let database =
            Arc::get_mut(&mut self.db).ok_or(OutboundOutboxError::CompactionDatabaseBusy)?;
        let mut reclaimed = false;
        for _ in 0..64 {
            if !permit
                .run_if_current(|| database.compact())
                .map_err(|_| OutboundOutboxError::CompactionFenced)?
                .map_err(backend)?
            {
                break;
            }
            reclaimed = true;
        }
        Ok(reclaimed)
    }

    fn update_terminal<T>(
        &self,
        id: &[u8; 32],
        boundary: &'static str,
        update: impl FnOnce(&mut OutboundTransferIntent) -> Result<T, OutboundOutboxError>,
    ) -> Result<T, OutboundOutboxError> {
        dr_m5_failpoint::hit(boundary, "before_begin_write");
        let write = self.db.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit(boundary, "after_begin_write_before_mutation");
        let result;
        {
            let mut table = write.open_table(OUTBOX).map_err(backend)?;
            let bytes = table
                .get(id.as_slice())
                .map_err(backend)?
                .map(|guard| guard.value().to_vec())
                .ok_or(OutboundOutboxError::MissingIntent)?;
            let mut intent = decode_intent(&bytes)?;
            let was_terminal = intent.state.is_terminal();
            result = update(&mut intent)?;
            touch(&mut intent);
            if !was_terminal && intent.state.is_terminal() {
                intent.terminal_sequence = next_terminal_sequence(&write)?;
            }
            let encoded = encode_intent(&intent)?;
            table
                .insert(id.as_slice(), encoded.as_slice())
                .map_err(backend)?;
        }
        dr_m5_failpoint::hit(boundary, "after_mutation_before_commit");
        write.commit().map_err(backend)?;
        dr_m5_failpoint::hit(boundary, "after_commit_before_next_side_effect");
        dr_m5_failpoint::hit(boundary, "after_next_side_effect_before_ack");
        Ok(result)
    }

    fn update<T>(
        &self,
        id: &[u8; 32],
        boundary: &'static str,
        update: impl FnOnce(&mut OutboundTransferIntent) -> Result<T, OutboundOutboxError>,
    ) -> Result<T, OutboundOutboxError> {
        dr_m5_failpoint::hit(boundary, "before_begin_write");
        let write = self.db.begin_write().map_err(backend)?;
        dr_m5_failpoint::hit(boundary, "after_begin_write_before_mutation");
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
            touch(&mut intent);
            let encoded = encode_intent(&intent)?;
            table
                .insert(id.as_slice(), encoded.as_slice())
                .map_err(backend)?;
        }
        dr_m5_failpoint::hit(boundary, "after_mutation_before_commit");
        write.commit().map_err(backend)?;
        dr_m5_failpoint::hit(boundary, "after_commit_before_next_side_effect");
        dr_m5_failpoint::hit(boundary, "after_next_side_effect_before_ack");
        Ok(result)
    }
}

impl PortableArchiveRows for OutboundOutbox {
    fn archive_owner(&self) -> ArchiveOwner {
        ArchiveOwner::OUTBOX
    }

    fn archive_entry_kind(&self) -> ArchiveEntryKind {
        ArchiveEntryKind::OutboxRecord
    }

    fn archive_rows(&self) -> Result<Vec<PortableArchiveRow>, NodeError> {
        let read = self.db.begin_read().map_err(|error| archive_error(error))?;
        let mut rows = Vec::new();
        for (table_id, table) in [
            (1u8, read.open_table(OUTBOX).map_err(archive_error)?),
            (2u8, read.open_table(META).map_err(archive_error)?),
            (3u8, read.open_table(TOMBSTONES).map_err(archive_error)?),
        ] {
            for row in table.iter().map_err(archive_error)? {
                let (key, value) = row.map_err(archive_error)?;
                let row = PortableArchiveRow {
                    table: table_id,
                    key: key.value().to_vec(),
                    value: value.value().to_vec(),
                };
                validate_archive_row(&row)?;
                rows.push(row);
            }
        }
        rows.sort_by(|left, right| (left.table, &left.key).cmp(&(right.table, &right.key)));
        Ok(rows)
    }

    fn restore_row(&self, row: &PortableArchiveRow) -> Result<(), NodeError> {
        validate_archive_row(row)?;
        let write = self.db.begin_write().map_err(archive_error)?;
        match row.table {
            1 => restore_table_value(&write, OUTBOX, row)?,
            2 => restore_table_value(&write, META, row)?,
            3 => restore_table_value(&write, TOMBSTONES, row)?,
            _ => {
                return Err(NodeError::ArchiveCapability(
                    "outbox archive table is unknown".into(),
                ))
            }
        }
        write.commit().map_err(archive_error)
    }

    fn reconcile_restored_rows(&self) -> Result<(), NodeError> {
        // Pending records remain pending. Scheduler admission later validates
        // their exact target and authenticated route; nothing is sent here.
        self.stats()
            .map(|_| ())
            .map_err(|error| NodeError::Storage(error.to_string()))
    }
}

fn validate_archive_row(row: &PortableArchiveRow) -> Result<(), NodeError> {
    match row.table {
        1 => {
            let id: [u8; 32] = row
                .key
                .as_slice()
                .try_into()
                .map_err(|_| NodeError::ArchiveCapability("outbox intent key length".into()))?;
            let intent =
                decode_intent(&row.value).map_err(|error| NodeError::Storage(error.to_string()))?;
            if intent.id != id
                || encode_intent(&intent).map_err(|error| NodeError::Storage(error.to_string()))?
                    != row.value
            {
                return Err(NodeError::ArchiveCapability(
                    "outbox intent row is non-canonical".into(),
                ));
            }
        }
        2 => {
            let valid = (row.key.as_slice() == FAIR_CURSOR_KEY && row.value.len() == 32)
                || (row.key.as_slice() == TERMINAL_HEAD_KEY && row.value.len() == 8);
            if !valid {
                return Err(NodeError::ArchiveCapability(
                    "outbox metadata row is invalid".into(),
                ));
            }
        }
        3 => {
            let id: [u8; 32] =
                row.key.as_slice().try_into().map_err(|_| {
                    NodeError::ArchiveCapability("outbox tombstone key length".into())
                })?;
            decode_tombstone(id, &row.value)
                .map_err(|error| NodeError::Storage(error.to_string()))?;
        }
        _ => {
            return Err(NodeError::ArchiveCapability(
                "outbox archive table is unknown".into(),
            ))
        }
    }
    Ok(())
}

fn restore_table_value(
    write: &redb::WriteTransaction,
    definition: TableDefinition<&[u8], &[u8]>,
    row: &PortableArchiveRow,
) -> Result<(), NodeError> {
    let mut table = write.open_table(definition).map_err(archive_error)?;
    if let Some(existing) = table.get(row.key.as_slice()).map_err(archive_error)? {
        if existing.value() == row.value.as_slice() {
            return Ok(());
        }
        return Err(NodeError::ArchiveCapability(
            "outbox archive restore conflict".into(),
        ));
    }
    table
        .insert(row.key.as_slice(), row.value.as_slice())
        .map_err(archive_error)?;
    Ok(())
}

fn archive_error(error: impl std::fmt::Display) -> NodeError {
    NodeError::Storage(error.to_string())
}

fn validate_intent(intent: &OutboundTransferIntent) -> Result<(), OutboundOutboxError> {
    if intent.canonical_bytes.is_empty()
        || intent.canonical_bytes.len() > MAX_OUTBOX_PAYLOAD_BYTES
        || !matches!(
            intent.disclosure,
            DisclosureClass::Public | DisclosureClass::RouteMinimal
        )
        || content_domain(intent.kind).digest(&intent.canonical_bytes) != intent.cid
        || (intent.enqueued_at_unix_seconds != 0
            && intent.updated_at_unix_seconds < intent.enqueued_at_unix_seconds)
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
        2 => Ok(OutboundIntentState::DeadLetter),
        3 => Ok(OutboundIntentState::RetryExhausted),
        _ => Err(OutboundOutboxError::InvalidRecord),
    }
}

fn encode_intent(intent: &OutboundTransferIntent) -> Result<Vec<u8>, OutboundOutboxError> {
    validate_intent(intent)?;
    let mut output = Vec::with_capacity(FIXED_BYTES_V3 + intent.canonical_bytes.len());
    output.extend_from_slice(MAGIC_V3);
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
    output.extend_from_slice(&intent.transport_attempts.to_be_bytes());
    output.push(intent.state as u8);
    output.extend_from_slice(&intent.validation_retries.to_be_bytes());
    output.extend_from_slice(&intent.terminal_sequence.to_be_bytes());
    output.extend_from_slice(&intent.enqueued_at_unix_seconds.to_be_bytes());
    output.extend_from_slice(&intent.updated_at_unix_seconds.to_be_bytes());
    output.extend_from_slice(&(intent.canonical_bytes.len() as u64).to_be_bytes());
    output.extend_from_slice(&intent.canonical_bytes);
    debug_assert_eq!(output.len(), FIXED_BYTES_V3 + intent.canonical_bytes.len());
    Ok(output)
}

fn decode_intent(bytes: &[u8]) -> Result<OutboundTransferIntent, OutboundOutboxError> {
    if bytes.len() < FIXED_BYTES_V1 {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let is_v1 = &bytes[..8] == MAGIC_V1;
    let is_v2 = &bytes[..8] == MAGIC_V2;
    let is_v3 = &bytes[..8] == MAGIC_V3;
    if !is_v1 && !is_v2 && !is_v3 {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let fixed_bytes = if is_v1 {
        FIXED_BYTES_V1
    } else if is_v2 {
        FIXED_BYTES_V2
    } else {
        FIXED_BYTES_V3
    };
    if bytes.len() < fixed_bytes {
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
    let transport_attempts = u64::from_be_bytes(
        bytes[189..197]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    );
    let state = parse_state(bytes[197])?;
    let (validation_retries, terminal_sequence, enqueued_at, updated_at, payload_offset) = if is_v1
    {
        (0, 0, 0, 0, 198)
    } else {
        let validation_retries = u64::from_be_bytes(
            bytes[198..206]
                .try_into()
                .map_err(|_| OutboundOutboxError::InvalidRecord)?,
        );
        let terminal_sequence = u64::from_be_bytes(
            bytes[206..214]
                .try_into()
                .map_err(|_| OutboundOutboxError::InvalidRecord)?,
        );
        if is_v2 {
            (validation_retries, terminal_sequence, 0, 0, 214)
        } else {
            let enqueued_at = u64::from_be_bytes(
                bytes[214..222]
                    .try_into()
                    .map_err(|_| OutboundOutboxError::InvalidRecord)?,
            );
            let updated_at = u64::from_be_bytes(
                bytes[222..230]
                    .try_into()
                    .map_err(|_| OutboundOutboxError::InvalidRecord)?,
            );
            (
                validation_retries,
                terminal_sequence,
                enqueued_at,
                updated_at,
                230,
            )
        }
    };
    let payload_len = u64::from_be_bytes(
        bytes[payload_offset..payload_offset + 8]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    ) as usize;
    if payload_len == 0
        || payload_len > MAX_OUTBOX_PAYLOAD_BYTES
        || bytes.len().checked_sub(fixed_bytes) != Some(payload_len)
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
        canonical_bytes: bytes[fixed_bytes..].to_vec(),
        transport_attempts,
        validation_retries,
        terminal_sequence,
        enqueued_at_unix_seconds: enqueued_at,
        updated_at_unix_seconds: updated_at,
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

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn touch(intent: &mut OutboundTransferIntent) {
    let now = unix_seconds();
    intent.updated_at_unix_seconds = now.max(intent.enqueued_at_unix_seconds);
}

fn next_terminal_sequence(write: &redb::WriteTransaction) -> Result<u64, OutboundOutboxError> {
    let mut meta = write.open_table(META).map_err(backend)?;
    let current = meta
        .get(TERMINAL_HEAD_KEY)
        .map_err(backend)?
        .map(|value| {
            value
                .value()
                .try_into()
                .map(u64::from_be_bytes)
                .map_err(|_| OutboundOutboxError::InvalidRecord)
        })
        .transpose()?
        .unwrap_or(0);
    let next = current
        .checked_add(1)
        .ok_or(OutboundOutboxError::SequenceExhausted)?;
    meta.insert(TERMINAL_HEAD_KEY, next.to_be_bytes().as_slice())
        .map_err(backend)?;
    Ok(next)
}

fn encode_tombstone(intent: &OutboundTransferIntent) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(TOMBSTONE_BYTES_V2);
    bytes.push(intent.state as u8);
    bytes.extend_from_slice(&intent.terminal_sequence.to_be_bytes());
    bytes.extend_from_slice(&intent.transport_attempts.to_be_bytes());
    bytes.extend_from_slice(&intent.validation_retries.to_be_bytes());
    bytes.extend_from_slice(&intent.cid);
    bytes.extend_from_slice(blake3::hash(&intent.canonical_bytes).as_bytes());
    bytes
}

fn decode_tombstone(
    intent_id: [u8; 32],
    bytes: &[u8],
) -> Result<OutboundAuditTombstone, OutboundOutboxError> {
    if !matches!(bytes.len(), TOMBSTONE_BYTES_V1 | TOMBSTONE_BYTES_V2) {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let state = parse_state(bytes[0])?;
    if !state.is_terminal() {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let terminal_sequence = u64::from_be_bytes(
        bytes[1..9]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    );
    if terminal_sequence == 0 {
        return Err(OutboundOutboxError::InvalidRecord);
    }
    let transport_attempts = u64::from_be_bytes(
        bytes[9..17]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    );
    let validation_retries = u64::from_be_bytes(
        bytes[17..25]
            .try_into()
            .map_err(|_| OutboundOutboxError::InvalidRecord)?,
    );
    let legacy_without_payload_digest = bytes.len() == TOMBSTONE_BYTES_V1;
    let (cid, payload_digest) = if legacy_without_payload_digest {
        ([0; 32], [0; 32])
    } else {
        (array32(&bytes[25..57])?, array32(&bytes[57..89])?)
    };
    Ok(OutboundAuditTombstone {
        intent_id,
        state,
        terminal_sequence,
        transport_attempts,
        validation_retries,
        cid,
        payload_digest,
        legacy_without_payload_digest,
    })
}

fn remove_oldest_tombstone(
    table: &mut redb::Table<'_, &'static [u8], &'static [u8]>,
) -> Result<(), OutboundOutboxError> {
    let mut oldest: Option<(u64, Vec<u8>)> = None;
    for entry in table.iter().map_err(backend)? {
        let (key, value) = entry.map_err(backend)?;
        let value = value.value();
        if !matches!(value.len(), TOMBSTONE_BYTES_V1 | TOMBSTONE_BYTES_V2) {
            return Err(OutboundOutboxError::InvalidRecord);
        }
        let sequence = u64::from_be_bytes(
            value[1..9]
                .try_into()
                .map_err(|_| OutboundOutboxError::InvalidRecord)?,
        );
        if oldest
            .as_ref()
            .is_none_or(|(oldest_sequence, _)| sequence < *oldest_sequence)
        {
            oldest = Some((sequence, key.value().to_vec()));
        }
    }
    if let Some((_, key)) = oldest {
        table.remove(key.as_slice()).map_err(backend)?;
    }
    Ok(())
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
    #[error("outbox terminal sequence exhausted")]
    SequenceExhausted,
    #[error("outbox compaction generation is disabled or stale")]
    CompactionFenced,
    #[error("outbox database has live clones and cannot reclaim disk")]
    CompactionDatabaseBusy,
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "vnext-compaction-harness")]
    use std::fs;
    #[cfg(feature = "vnext-compaction-harness")]
    use std::path::Path;
    #[cfg(feature = "vnext-compaction-harness")]
    use std::process::{Command, Stdio};
    #[cfg(feature = "vnext-compaction-harness")]
    use std::thread;
    #[cfg(feature = "vnext-compaction-harness")]
    use std::time::{Duration, Instant};

    use ku_core::foundation::OperationalCompactionSwitch;

    use super::*;

    fn intent(addr: SocketAddr) -> OutboundTransferIntent {
        intent_with(1, addr)
    }

    fn intent_with(marker: u8, addr: SocketAddr) -> OutboundTransferIntent {
        OutboundTransferIntent::new(
            NodeId::from_bytes([marker; 32]),
            addr,
            SelectorCid::from_bytes([2; 32]),
            NamespaceCommitment::from_bytes([3; 32]),
            DisclosureClass::Public,
            ReconcileManifestKind::Object,
            format!("canonical-outbox-test-{marker}").into_bytes(),
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
            assert_eq!(outbox.record_transport_attempt(&first.id).unwrap(), 1);
        }
        let reopened = OutboundOutbox::open(&path).unwrap();
        let pending = reopened.pending(8).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].transport_attempts, 1);
        assert_eq!(
            reopened
                .apply_receipt(&first.id, ReconcileReceiptStatus::ValidatedStored, 3)
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
    fn legacy_v1_and_v2_intents_decode_with_unknown_age() {
        let intent = intent("127.0.0.1:5001".parse().unwrap());
        let encoded_v3 = encode_intent(&intent).unwrap();
        let mut encoded_v1 = Vec::with_capacity(FIXED_BYTES_V1 + intent.canonical_bytes.len());
        encoded_v1.extend_from_slice(MAGIC_V1);
        encoded_v1.extend_from_slice(&encoded_v3[8..198]);
        encoded_v1.extend_from_slice(&encoded_v3[230..]);
        let decoded_v1 = decode_intent(&encoded_v1).unwrap();
        assert_eq!(decoded_v1.transport_attempts, 0);
        assert_eq!(decoded_v1.validation_retries, 0);
        assert_eq!(decoded_v1.terminal_sequence, 0);
        assert_eq!(decoded_v1.enqueued_at_unix_seconds, 0);
        assert_eq!(decoded_v1.updated_at_unix_seconds, 0);

        let mut encoded_v2 = Vec::with_capacity(FIXED_BYTES_V2 + intent.canonical_bytes.len());
        encoded_v2.extend_from_slice(MAGIC_V2);
        encoded_v2.extend_from_slice(&encoded_v3[8..214]);
        encoded_v2.extend_from_slice(&encoded_v3[230..]);
        let decoded_v2 = decode_intent(&encoded_v2).unwrap();
        assert_eq!(decoded_v2.id, intent.id);
        assert_eq!(decoded_v2.validation_retries, intent.validation_retries);
        assert_eq!(decoded_v2.terminal_sequence, intent.terminal_sequence);
        assert_eq!(decoded_v2.enqueued_at_unix_seconds, 0);
        assert_eq!(decoded_v2.updated_at_unix_seconds, 0);
        let mut touched_legacy = decoded_v2;
        touch(&mut touched_legacy);
        assert_eq!(touched_legacy.enqueued_at_unix_seconds, 0);
        assert!(touched_legacy.updated_at_unix_seconds > 0);
    }

    #[test]
    fn stats_report_terminal_depth_and_honest_oldest_pending_age() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let pending = intent_with(1, "127.0.0.1:5001".parse().unwrap());
        let exhausted = intent_with(2, "127.0.0.1:5002".parse().unwrap());
        outbox.enqueue(&pending).unwrap();
        outbox.enqueue(&exhausted).unwrap();
        outbox.record_transport_attempt(&exhausted.id).unwrap();
        outbox.mark_retry_exhausted(&exhausted.id, 1).unwrap();

        let stats = outbox.stats().unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.retry_exhausted, 1);
        assert!(stats.oldest_pending_age_seconds.is_some());
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
    fn transport_attempts_and_validation_retries_are_independent() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let intent = intent("127.0.0.1:5001".parse().unwrap());
        outbox.enqueue(&intent).unwrap();
        outbox.record_transport_attempt(&intent.id).unwrap();
        assert_eq!(
            outbox
                .apply_receipt(
                    &intent.id,
                    ReconcileReceiptStatus::DeferredMissingDependency,
                    2,
                )
                .unwrap(),
            OutboundIntentState::Pending
        );
        let stored = outbox.get(&intent.id).unwrap().unwrap();
        assert_eq!(stored.transport_attempts, 0);
        assert_eq!(stored.validation_retries, 1);
        assert_eq!(
            outbox
                .apply_receipt(
                    &intent.id,
                    ReconcileReceiptStatus::DeferredMissingDependency,
                    2,
                )
                .unwrap(),
            OutboundIntentState::RetryExhausted
        );
    }

    #[test]
    fn exhausted_first_page_cannot_starve_healthy_pending_work() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let addr = "127.0.0.1:5001".parse().unwrap();
        let mut intents = (1..=6)
            .map(|marker| intent_with(marker, addr))
            .collect::<Vec<_>>();
        intents.sort_by_key(|intent| intent.id);
        for intent in &intents {
            outbox.enqueue(intent).unwrap();
        }
        for intent in intents.iter().take(4) {
            outbox.record_transport_attempt(&intent.id).unwrap();
            assert_eq!(
                outbox.mark_retry_exhausted(&intent.id, 1).unwrap(),
                OutboundIntentState::RetryExhausted
            );
        }
        let first = outbox.pending_fair(1).unwrap();
        let second = outbox.pending_fair(1).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].id, second[0].id);
        assert!(intents
            .iter()
            .skip(4)
            .any(|intent| intent.id == first[0].id));
        assert!(intents
            .iter()
            .skip(4)
            .any(|intent| intent.id == second[0].id));
    }

    #[test]
    fn terminal_compaction_preserves_pending_and_audit_sequence() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let addr = "127.0.0.1:5001".parse().unwrap();
        let acknowledged = intent_with(1, addr);
        let dead = intent_with(2, addr);
        let pending = intent_with(3, addr);
        for intent in [&acknowledged, &dead, &pending] {
            outbox.enqueue(intent).unwrap();
        }
        outbox
            .apply_receipt(&acknowledged.id, ReconcileReceiptStatus::AlreadyPresent, 2)
            .unwrap();
        outbox
            .apply_receipt(&dead.id, ReconcileReceiptStatus::RejectedInvalid, 2)
            .unwrap();
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        let report = outbox.compact_terminal(&permit, 1, 8).unwrap();
        assert_eq!(report.removed_records, 1);
        assert_eq!(report.audit_tombstones, 1);
        let tombstone = outbox.tombstone(&acknowledged.id).unwrap().unwrap();
        assert_eq!(tombstone.state, OutboundIntentState::Acknowledged);
        assert_eq!(tombstone.cid, acknowledged.cid);
        assert_eq!(
            tombstone.payload_digest,
            *blake3::hash(&acknowledged.canonical_bytes).as_bytes()
        );
        assert!(!tombstone.legacy_without_payload_digest);
        assert!(outbox.get(&pending.id).unwrap().is_some());
        assert_eq!(outbox.pending_fair(8).unwrap(), vec![pending]);
    }

    #[test]
    fn stale_compaction_generation_cannot_delete_terminal_or_pending_work() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = OutboundOutbox::open(&directory.path().join("outbox.redb")).unwrap();
        let addr = "127.0.0.1:5001".parse().unwrap();
        let terminal = intent_with(1, addr);
        let pending = intent_with(2, addr);
        outbox.enqueue(&terminal).unwrap();
        outbox.enqueue(&pending).unwrap();
        outbox
            .apply_receipt(&terminal.id, ReconcileReceiptStatus::AlreadyPresent, 2)
            .unwrap();

        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let stale = switch.acquire().unwrap();
        switch.disable();
        assert_eq!(
            outbox.compact_terminal(&stale, 0, 8),
            Err(OutboundOutboxError::CompactionFenced)
        );
        assert!(outbox.get(&terminal.id).unwrap().is_some());
        assert!(outbox.get(&pending.id).unwrap().is_some());
        assert_eq!(outbox.tombstone_count().unwrap(), 0);
    }

    #[test]
    fn physical_reclaim_reduces_disk_after_terminal_payload_compaction() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reclaim.redb");
        let mut outbox = OutboundOutbox::open(&path).unwrap();
        let addr = "127.0.0.1:5001".parse().unwrap();
        let mut intents = Vec::new();
        for marker in 1..=32u8 {
            let intent = OutboundTransferIntent::new(
                NodeId::from_bytes([marker; 32]),
                addr,
                SelectorCid::from_bytes([2; 32]),
                NamespaceCommitment::from_bytes([3; 32]),
                DisclosureClass::Public,
                ReconcileManifestKind::Object,
                vec![marker; 128 * 1024],
            )
            .unwrap();
            outbox.enqueue(&intent).unwrap();
            outbox
                .apply_receipt(&intent.id, ReconcileReceiptStatus::AlreadyPresent, 2)
                .unwrap();
            intents.push(intent);
        }
        let disk_before = outbox.disk_bytes().unwrap();
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        let report = outbox.compact_terminal(&permit, 0, 32).unwrap();
        assert_eq!(report.removed_records, 32);
        assert!(report.removed_payload_bytes >= 4 * 1_048_576);
        assert!(intents
            .iter()
            .all(|intent| outbox.get(&intent.id).unwrap().is_none()));
        assert!(outbox.reclaim_disk(&permit).unwrap());
        let disk_after = outbox.disk_bytes().unwrap();
        assert!(
            disk_after < disk_before,
            "physical compaction must reduce disk bytes: before={disk_before}, after={disk_after}"
        );
    }

    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_CHILD_ENV: &str = "ONEBRAIN_M5_05_OUTBOX_CHILD";
    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_DATABASE_ENV: &str = "ONEBRAIN_M5_05_OUTBOX_DATABASE";
    #[cfg(feature = "vnext-compaction-harness")]
    const COMPACTION_CHILD_TEST: &str = "vnext_outbox::tests::m5_05_outbox_compaction_worker";

    #[cfg(feature = "vnext-compaction-harness")]
    #[test]
    fn m5_05_outbox_compaction_worker() {
        if std::env::var_os(COMPACTION_CHILD_ENV).is_none() {
            return;
        }
        let database = std::env::var_os(COMPACTION_DATABASE_ENV).unwrap();
        compact_outbox(Path::new(&database));
    }

    #[cfg(feature = "vnext-compaction-harness")]
    #[test]
    fn m5_05_outbox_process_kill_matrix_restores_exact_root() {
        let expected_directory = tempfile::tempdir().unwrap();
        let expected_path = expected_directory.path().join("expected.redb");
        initialize_outbox(&expected_path);
        let expected = compact_outbox(&expected_path);

        for phase in dr_m5_failpoint::FAILPOINT_PHASES {
            let directory = tempfile::tempdir().unwrap();
            let database = directory.path().join("outbox.redb");
            let marker = directory.path().join("armed.json");
            initialize_outbox(&database);
            let token = format!("outbox-{phase}-{}", std::process::id());
            let mut child = Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(COMPACTION_CHILD_TEST)
                .arg("--nocapture")
                .env(COMPACTION_CHILD_ENV, "1")
                .env(COMPACTION_DATABASE_ENV, &database)
                .env(dr_m5_failpoint::ENABLE_ENV, "1")
                .env(
                    dr_m5_failpoint::FAILPOINT_ENV,
                    format!("TX-CMP-OUT-001:{phase}"),
                )
                .env(dr_m5_failpoint::MARKER_ENV, &marker)
                .env(dr_m5_failpoint::TOKEN_ENV, &token)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            wait_for_compaction_marker(&mut child, &marker, &token, phase);
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            assert_eq!(compact_outbox(&database), expected, "outbox phase {phase}");
        }
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn initialize_outbox(path: &Path) {
        let outbox = OutboundOutbox::open(path).unwrap();
        let addr = "127.0.0.1:5001".parse().unwrap();
        let acknowledged = intent_with(1, addr);
        let dead = intent_with(2, addr);
        let pending = intent_with(3, addr);
        for intent in [&acknowledged, &dead, &pending] {
            outbox.enqueue(intent).unwrap();
        }
        outbox
            .apply_receipt(&acknowledged.id, ReconcileReceiptStatus::AlreadyPresent, 2)
            .unwrap();
        outbox
            .apply_receipt(&dead.id, ReconcileReceiptStatus::RejectedInvalid, 2)
            .unwrap();
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn compact_outbox(path: &Path) -> (OutboundOutboxStats, [u8; 32], u64, Vec<[u8; 32]>) {
        let outbox = OutboundOutbox::open(path).unwrap();
        let switch = OperationalCompactionSwitch::new_disabled();
        switch.enable();
        let permit = switch.acquire().unwrap();
        outbox.compact_terminal(&permit, 0, 8).unwrap();
        let mut pending = outbox
            .pending_fair(8)
            .unwrap()
            .into_iter()
            .map(|intent| intent.id)
            .collect::<Vec<_>>();
        pending.sort();
        let mut stats = outbox.stats().unwrap();
        // Wall-clock age is operator telemetry, not a crash-consistency
        // oracle. Durable counters, roots, tombstones and pending identities
        // remain exact below.
        stats.oldest_pending_age_seconds = None;
        (
            stats,
            outbox.audit_root().unwrap(),
            outbox.tombstone_count().unwrap(),
            pending,
        )
    }

    #[cfg(feature = "vnext-compaction-harness")]
    fn wait_for_compaction_marker(
        child: &mut std::process::Child,
        marker: &Path,
        token: &str,
        phase: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if marker.is_file() {
                let body = fs::read_to_string(marker).unwrap();
                assert!(body.contains("\"boundary\":\"TX-CMP-OUT-001\""));
                assert!(body.contains(&format!("\"phase\":\"{phase}\"")));
                assert!(body.contains(&format!("\"token\":\"{token}\"")));
                return;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("outbox {phase} exited before marker: {status}");
            }
            assert!(Instant::now() < deadline, "outbox {phase} marker timeout");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
