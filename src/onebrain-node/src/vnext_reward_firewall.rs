//! Best-effort one-way export from committed knowledge evidence to an optional
//! reward consumer. Consumer state never participates in knowledge operations.
//!
//! Includes both the original v1 `RewardEvidenceNotice` / `KnowledgeRewardFirewall`
//! (untouched for full Base/M4 compatibility) and the additive Wave 5/6
//! `RewardEvidenceNoticeV2` (`BenefitClosureNotice` + `AcceptedWorkNotice`) +
//! lazy-opened `RedbRewardNoticeOutboxV2` (`vnext_reward_notice_outbox_v2` in
//! `<data_dir>/vnext_reward_outbox_v2.redb`).

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use ku_core::foundation::{EventCid, ObjectCid};
use onebrain_obt::{AcceptedWorkKind, ObtNoticeV2};
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardEvidenceKind {
    Use,
    Derivation,
    OutcomeObservation,
    BenefitEvidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardEvidenceNotice {
    pub event_cid: EventCid,
    pub payload_object_cid: ObjectCid,
    pub kind: RewardEvidenceKind,
    /// Exact evidence policy/frontier commitment, not a reward policy or mint.
    pub evidence_scope_commitment: [u8; 32],
}

impl RewardEvidenceNotice {
    pub const fn contains_token_authority(&self) -> bool {
        false
    }

    pub const fn is_reward_instruction(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardConsumerError {
    Unavailable,
    Backpressure,
    CorruptState,
}

pub trait RewardEvidenceConsumer {
    fn try_export(&mut self, notice: &RewardEvidenceNotice) -> Result<(), RewardConsumerError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RewardFirewallPolicy {
    pub enabled: bool,
    pub max_queued_notices: usize,
    pub max_attempts_per_notice: u8,
    pub max_quarantined_ids: usize,
}

impl RewardFirewallPolicy {
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            max_queued_notices: 1,
            max_attempts_per_notice: 1,
            max_quarantined_ids: 1,
        }
    }

    fn validate(self) -> Result<Self, RewardFirewallConfigError> {
        if self.max_queued_notices == 0
            || self.max_attempts_per_notice == 0
            || self.max_quarantined_ids == 0
        {
            Err(RewardFirewallConfigError::ZeroBound)
        } else {
            Ok(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardObserveOutcome {
    Disabled,
    QueuedAfterKnowledgeCommit,
    ExactReplayAlreadyQueued,
    DroppedByBackpressure,
    InvalidScopeSuppressed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingNotice {
    notice: RewardEvidenceNotice,
    attempts: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RewardDrainReport {
    pub exported: usize,
    pub retried_later: usize,
    pub quarantined: usize,
    pub remaining: usize,
}

pub struct KnowledgeRewardFirewall {
    policy: RewardFirewallPolicy,
    queue: VecDeque<PendingNotice>,
    queued_ids: BTreeSet<[u8; 32]>,
    quarantined_ids: VecDeque<[u8; 32]>,
}

impl KnowledgeRewardFirewall {
    pub fn new(policy: RewardFirewallPolicy) -> Result<Self, RewardFirewallConfigError> {
        Ok(Self {
            policy: policy.validate()?,
            queue: VecDeque::new(),
            queued_ids: BTreeSet::new(),
            quarantined_ids: VecDeque::new(),
        })
    }

    /// Called only after the knowledge artifact/event transaction has committed.
    /// The return value is observability, never part of the commit result.
    pub fn observe_committed_evidence(
        &mut self,
        notice: RewardEvidenceNotice,
    ) -> RewardObserveOutcome {
        if !self.policy.enabled {
            return RewardObserveOutcome::Disabled;
        }
        if notice.evidence_scope_commitment == [0; 32] {
            return RewardObserveOutcome::InvalidScopeSuppressed;
        }
        let event_id = notice.event_cid.into_bytes();
        if self.queued_ids.contains(&event_id) {
            return RewardObserveOutcome::ExactReplayAlreadyQueued;
        }
        if self.queue.len() == self.policy.max_queued_notices {
            return RewardObserveOutcome::DroppedByBackpressure;
        }
        self.queue.push_back(PendingNotice {
            notice,
            attempts: 0,
        });
        self.queued_ids.insert(event_id);
        RewardObserveOutcome::QueuedAfterKnowledgeCommit
    }

    /// Drain is invoked by a separate worker. All consumer failures collapse
    /// into a report and can never be returned through KU/KQL/OBP operations.
    pub fn drain<C: RewardEvidenceConsumer>(
        &mut self,
        consumer: &mut C,
        max_items: usize,
    ) -> RewardDrainReport {
        let mut report = RewardDrainReport::default();
        let available = self.queue.len().min(max_items);
        for _ in 0..available {
            let mut pending = self.queue.pop_front().expect("bounded by queue length");
            let event_id = pending.notice.event_cid.into_bytes();
            self.queued_ids.remove(&event_id);
            match consumer.try_export(&pending.notice) {
                Ok(()) => report.exported += 1,
                Err(RewardConsumerError::Unavailable | RewardConsumerError::Backpressure) => {
                    pending.attempts = pending.attempts.saturating_add(1);
                    if pending.attempts < self.policy.max_attempts_per_notice {
                        self.queued_ids.insert(event_id);
                        self.queue.push_back(pending);
                        report.retried_later += 1;
                    } else {
                        self.quarantine(event_id);
                        report.quarantined += 1;
                    }
                }
                Err(RewardConsumerError::CorruptState) => {
                    self.quarantine(event_id);
                    report.quarantined += 1;
                }
            }
        }
        report.remaining = self.queue.len();
        report
    }

    pub fn queued_count(&self) -> usize {
        self.queue.len()
    }

    pub fn quarantined_count(&self) -> usize {
        self.quarantined_ids.len()
    }

    pub const fn can_gate_knowledge_plane(&self) -> bool {
        false
    }

    fn quarantine(&mut self, event_id: [u8; 32]) {
        if self.quarantined_ids.len() == self.policy.max_quarantined_ids {
            self.quarantined_ids.pop_front();
        }
        self.quarantined_ids.push_back(event_id);
    }
}

/// Knowledge operations intentionally have no reward-consumer parameter.
pub fn execute_knowledge_operation<T, E>(operation: impl FnOnce() -> Result<T, E>) -> Result<T, E> {
    operation()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardFirewallConfigError {
    ZeroBound,
}

// ---------------------------------------------------------------------------
// Additive Wave 5/6 RewardEvidenceNoticeV2 & Lazy Durable Outbox (`FW-07`)
// ---------------------------------------------------------------------------

const REWARD_NOTICE_OUTBOX_V2_TABLE: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("vnext_reward_notice_outbox_v2");
pub const REWARD_OUTBOX_V2_DB_FILENAME: &str = "vnext_reward_outbox_v2.redb";
pub const DEFAULT_MAX_REWARD_OUTBOX_V2_NOTICES: u64 = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenefitClosureNotice {
    pub benefit_event_cid: EventCid,
    pub outcome_case_id: [u8; 32],
    pub evidence_frontier_root: [u8; 32],
    pub policy_cid: ObjectCid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedWorkNotice {
    pub work_record_cid: ObjectCid,
    pub attestation_event_cid: EventCid,
    pub work_kind: AcceptedWorkKind,
    pub evidence_frontier_root: [u8; 32],
    pub policy_cid: ObjectCid,
}

/// Additive post-commit firewall notice v2 (`P5` §2.2, `FW-07`).
///
/// Contains content identifiers and frontier/policy commitments only — never
/// balances, token amounts, or mint authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewardEvidenceNoticeV2 {
    BenefitClosure(BenefitClosureNotice),
    AcceptedWork(AcceptedWorkNotice),
}

impl RewardEvidenceNoticeV2 {
    pub const fn contains_token_authority(&self) -> bool {
        false
    }

    pub const fn is_reward_instruction(&self) -> bool {
        false
    }

    pub fn to_obt_notice(self) -> ObtNoticeV2 {
        match self {
            Self::BenefitClosure(b) => ObtNoticeV2::BenefitClosure {
                benefit_event_cid: b.benefit_event_cid,
                outcome_case_id: b.outcome_case_id,
                evidence_frontier_root: b.evidence_frontier_root,
                policy_cid: b.policy_cid,
            },
            Self::AcceptedWork(w) => ObtNoticeV2::AcceptedWork {
                work_record_cid: w.work_record_cid,
                attestation_event_cid: w.attestation_event_cid,
                work_kind: w.work_kind,
                evidence_frontier_root: w.evidence_frontier_root,
                policy_cid: w.policy_cid,
            },
        }
    }

    pub fn from_obt_notice(notice: &ObtNoticeV2) -> Self {
        match notice {
            ObtNoticeV2::BenefitClosure {
                benefit_event_cid,
                outcome_case_id,
                evidence_frontier_root,
                policy_cid,
            } => Self::BenefitClosure(BenefitClosureNotice {
                benefit_event_cid: *benefit_event_cid,
                outcome_case_id: *outcome_case_id,
                evidence_frontier_root: *evidence_frontier_root,
                policy_cid: *policy_cid,
            }),
            ObtNoticeV2::AcceptedWork {
                work_record_cid,
                attestation_event_cid,
                work_kind,
                evidence_frontier_root,
                policy_cid,
            } => Self::AcceptedWork(AcceptedWorkNotice {
                work_record_cid: *work_record_cid,
                attestation_event_cid: *attestation_event_cid,
                work_kind: *work_kind,
                evidence_frontier_root: *evidence_frontier_root,
                policy_cid: *policy_cid,
            }),
        }
    }

    pub fn notice_id(&self) -> [u8; 32] {
        self.to_obt_notice().notice_id()
    }
}

pub trait RewardEvidenceConsumerV2 {
    fn try_export_v2(&mut self, notice: &RewardEvidenceNoticeV2)
        -> Result<(), RewardConsumerError>;
}

/// Lazy-opened Redb post-commit outbox (`vnext_reward_notice_outbox_v2`).
///
/// All filesystem or Redb errors collapse into `RewardObserveOutcome` / `RewardDrainReport`
/// and NEVER propagate as knowledge-plane errors (`FW-07`, `NEG-OBT-001`).
pub struct DurableRewardNoticeOutboxV2 {
    db_path: PathBuf,
    enabled: bool,
    max_notices: u64,
    db: Option<Database>,
}

impl DurableRewardNoticeOutboxV2 {
    /// Construct a lazy outbox adapter; creates zero files until `enabled == true`
    /// and a post-commit notice is observed.
    pub fn new_lazy(data_dir: impl AsRef<Path>, enabled: bool, max_notices: u64) -> Self {
        Self {
            db_path: data_dir.as_ref().join(REWARD_OUTBOX_V2_DB_FILENAME),
            enabled,
            max_notices: max_notices.max(1),
            db: None,
        }
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub const fn can_gate_knowledge_plane(&self) -> bool {
        false
    }

    fn ensure_db(&mut self) -> Result<&Database, ()> {
        if self.db.is_none() {
            if let Some(parent) = self.db_path.parent() {
                std::fs::create_dir_all(parent).map_err(|_| ())?;
            }
            let db = Database::create(&self.db_path).map_err(|_| ())?;
            let write = db.begin_write().map_err(|_| ())?;
            {
                write
                    .open_table(REWARD_NOTICE_OUTBOX_V2_TABLE)
                    .map_err(|_| ())?;
            }
            write.commit().map_err(|_| ())?;
            self.db = Some(db);
        }
        Ok(self.db.as_ref().expect("opened above"))
    }

    /// Best-effort post-commit enqueue. Even if disk is full or the DB file is
    /// corrupt, returns `DroppedByBackpressure` without panicking or failing
    /// the already-committed knowledge operation.
    pub fn observe_committed_v2(&mut self, notice: RewardEvidenceNoticeV2) -> RewardObserveOutcome {
        if !self.enabled {
            return RewardObserveOutcome::Disabled;
        }
        let obt = notice.to_obt_notice();
        if !obt.validate() {
            return RewardObserveOutcome::InvalidScopeSuppressed;
        }
        let Ok(encoded) = obt.encode_canonical_bytes() else {
            return RewardObserveOutcome::InvalidScopeSuppressed;
        };
        let notice_id = obt.notice_id();
        let max_notices = self.max_notices;
        let Ok(db) = self.ensure_db() else {
            return RewardObserveOutcome::DroppedByBackpressure;
        };
        let Ok(write) = db.begin_write() else {
            return RewardObserveOutcome::DroppedByBackpressure;
        };
        let outcome = {
            let Ok(mut table) = write.open_table(REWARD_NOTICE_OUTBOX_V2_TABLE) else {
                return RewardObserveOutcome::DroppedByBackpressure;
            };
            let already_present = match table.get(notice_id.as_slice()) {
                Ok(opt) => opt.is_some(),
                Err(_) => return RewardObserveOutcome::DroppedByBackpressure,
            };
            if already_present {
                RewardObserveOutcome::ExactReplayAlreadyQueued
            } else {
                let Ok(len) = table.len() else {
                    return RewardObserveOutcome::DroppedByBackpressure;
                };
                if len >= max_notices {
                    RewardObserveOutcome::DroppedByBackpressure
                } else if table
                    .insert(notice_id.as_slice(), encoded.as_slice())
                    .is_ok()
                {
                    RewardObserveOutcome::QueuedAfterKnowledgeCommit
                } else {
                    RewardObserveOutcome::DroppedByBackpressure
                }
            }
        };
        if matches!(outcome, RewardObserveOutcome::QueuedAfterKnowledgeCommit)
            && write.commit().is_err()
        {
            return RewardObserveOutcome::DroppedByBackpressure;
        }
        outcome
    }

    pub fn drain_v2<C: RewardEvidenceConsumerV2>(
        &mut self,
        consumer: &mut C,
        max_items: usize,
    ) -> RewardDrainReport {
        let mut report = RewardDrainReport::default();
        if !self.enabled || max_items == 0 || (self.db.is_none() && !self.db_path.exists()) {
            return report;
        }
        let Ok(db) = self.ensure_db() else {
            return report;
        };
        let entries: Vec<([u8; 32], Vec<u8>)> = {
            let Ok(read) = db.begin_read() else {
                return report;
            };
            let Ok(table) = read.open_table(REWARD_NOTICE_OUTBOX_V2_TABLE) else {
                return report;
            };
            let Ok(iter) = table.iter() else {
                return report;
            };
            let mut batch = Vec::new();
            for item in iter.take(max_items) {
                let Ok((k, v)) = item else {
                    report.quarantined += 1;
                    continue;
                };
                if let Ok(id) = <[u8; 32]>::try_from(k.value()) {
                    batch.push((id, v.value().to_vec()));
                } else {
                    report.quarantined += 1;
                }
            }
            batch
        };

        let mut to_remove = Vec::new();
        for (id, bytes) in entries {
            let Ok(obt) = ObtNoticeV2::decode_canonical_bytes(&bytes) else {
                report.quarantined += 1;
                to_remove.push(id);
                continue;
            };
            let notice = RewardEvidenceNoticeV2::from_obt_notice(&obt);
            match consumer.try_export_v2(&notice) {
                Ok(()) => {
                    report.exported += 1;
                    to_remove.push(id);
                }
                Err(RewardConsumerError::Unavailable | RewardConsumerError::Backpressure) => {
                    report.retried_later += 1;
                }
                Err(RewardConsumerError::CorruptState) => {
                    report.quarantined += 1;
                    to_remove.push(id);
                }
            }
        }

        if !to_remove.is_empty() {
            if let Ok(write) = db.begin_write() {
                if let Ok(mut table) = write.open_table(REWARD_NOTICE_OUTBOX_V2_TABLE) {
                    for id in &to_remove {
                        let _ = table.remove(id.as_slice());
                    }
                }
                let _ = write.commit();
            }
        }

        if let Ok(read) = db.begin_read() {
            if let Ok(table) = read.open_table(REWARD_NOTICE_OUTBOX_V2_TABLE) {
                if let Ok(len) = table.len() {
                    report.remaining = usize::try_from(len).unwrap_or(usize::MAX);
                }
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(byte: u8) -> RewardEvidenceNotice {
        RewardEvidenceNotice {
            event_cid: EventCid::from_bytes([byte; 32]),
            payload_object_cid: ObjectCid::from_bytes([byte.wrapping_add(1); 32]),
            kind: RewardEvidenceKind::BenefitEvidence,
            evidence_scope_commitment: [byte.wrapping_add(2); 32],
        }
    }

    fn enabled(capacity: usize) -> KnowledgeRewardFirewall {
        KnowledgeRewardFirewall::new(RewardFirewallPolicy {
            enabled: true,
            max_queued_notices: capacity,
            max_attempts_per_notice: 2,
            max_quarantined_ids: capacity.max(1),
        })
        .unwrap()
    }

    struct Consumer {
        result: Result<(), RewardConsumerError>,
        seen: usize,
    }

    impl RewardEvidenceConsumer for Consumer {
        fn try_export(
            &mut self,
            _notice: &RewardEvidenceNotice,
        ) -> Result<(), RewardConsumerError> {
            self.seen += 1;
            self.result
        }
    }

    #[test]
    fn disabled_reward_path_is_the_safe_default_and_never_gates_operations() {
        let mut firewall = KnowledgeRewardFirewall::new(RewardFirewallPolicy::disabled()).unwrap();
        for operation in ["publish", "query", "sync", "adopt", "replay"] {
            assert_eq!(
                execute_knowledge_operation(|| Ok::<_, &'static str>(operation)),
                Ok(operation)
            );
        }
        assert_eq!(
            firewall.observe_committed_evidence(notice(1)),
            RewardObserveOutcome::Disabled
        );
        assert!(!firewall.can_gate_knowledge_plane());
    }

    #[test]
    fn unavailable_consumer_only_retries_then_quarantines() {
        let mut firewall = enabled(2);
        firewall.observe_committed_evidence(notice(2));
        let mut consumer = Consumer {
            result: Err(RewardConsumerError::Unavailable),
            seen: 0,
        };
        let first = firewall.drain(&mut consumer, 1);
        assert_eq!(first.retried_later, 1);
        assert_eq!(first.remaining, 1);
        let second = firewall.drain(&mut consumer, 1);
        assert_eq!(second.quarantined, 1);
        assert_eq!(firewall.queued_count(), 0);
        assert_eq!(firewall.quarantined_count(), 1);
        assert_eq!(execute_knowledge_operation(|| Ok::<_, ()>(7)), Ok(7));
    }

    #[test]
    fn corrupt_consumer_state_is_quarantined_without_error_path_coupling() {
        let mut firewall = enabled(2);
        firewall.observe_committed_evidence(notice(3));
        let mut consumer = Consumer {
            result: Err(RewardConsumerError::CorruptState),
            seen: 0,
        };
        let report = firewall.drain(&mut consumer, 1);
        assert_eq!(report.quarantined, 1);
        assert_eq!(
            execute_knowledge_operation(|| Ok::<_, ()>("preserved")),
            Ok("preserved")
        );
    }

    #[test]
    fn queue_backpressure_is_bounded_and_drops_only_export_notice() {
        let mut firewall = enabled(1);
        assert_eq!(
            firewall.observe_committed_evidence(notice(4)),
            RewardObserveOutcome::QueuedAfterKnowledgeCommit
        );
        assert_eq!(
            firewall.observe_committed_evidence(notice(5)),
            RewardObserveOutcome::DroppedByBackpressure
        );
        assert_eq!(firewall.queued_count(), 1);
        assert_eq!(
            execute_knowledge_operation(|| Ok::<_, ()>("sync-complete")),
            Ok("sync-complete")
        );
    }

    #[test]
    fn event_replay_is_deduplicated_while_queued() {
        let mut firewall = enabled(2);
        let evidence = notice(6);
        firewall.observe_committed_evidence(evidence);
        assert_eq!(
            firewall.observe_committed_evidence(evidence),
            RewardObserveOutcome::ExactReplayAlreadyQueued
        );
        assert_eq!(firewall.queued_count(), 1);
    }

    #[test]
    fn export_notice_contains_no_mint_token_or_reward_authority() {
        let evidence = notice(7);
        assert!(!evidence.contains_token_authority());
        assert!(!evidence.is_reward_instruction());
        assert_eq!(evidence.kind, RewardEvidenceKind::BenefitEvidence);
    }
}
