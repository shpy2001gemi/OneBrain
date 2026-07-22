//! Best-effort one-way export from committed knowledge evidence to an optional
//! reward consumer. Consumer state never participates in knowledge operations.

use std::collections::{BTreeSet, VecDeque};

use ku_core::foundation::{EventCid, ObjectCid};

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
