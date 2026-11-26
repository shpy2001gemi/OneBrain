//! One-way post-commit `ObtNoticeV2` consumer and `M7.1` `ShadowEvaluate` gate (`§2.2`, `FW-07`).
//!
//! Knowledge-plane commits never wait on, query, or fail because of `onebrain-obt`.
//! Under `Disabled`, backpressure, or consumer/store errors, notices degrade to
//! `NoticeIngestOutcome::DroppedFirewallSafe` without panicking or propagating errors.
//! Under `ShadowEvaluate`, draining evaluates and persists `ShadowClaimEvaluation`
//! diagnostics ONLY with strictly zero `PendingMintRecord`, `AccountBlockV1`, or balance mutations.

use std::collections::{BTreeSet, VecDeque};

use ku_core::foundation::{CanonicalError, CanonicalValue, EventCid, ObjectCid, ResourceProfile};
use serde::{Deserialize, Serialize};

use crate::codec::{
    decode_value, domain_digest, encode_event_cid, encode_object_cid, encode_value, expect_bool,
    expect_bytes_exact, expect_event_cid, expect_map, expect_object_cid, expect_text, expect_u64,
    map_lookup, req_field,
};
use crate::mint_pipeline::PendingMintRecord;
use crate::nullifier::ScopedNullifier;
use crate::units::ObtAmount;

const OBT_NOTICE_V2_DOMAIN: &[u8] = b"onebrain:vnext:obt-notice-v2:1\0";
const SHADOW_EVAL_DOMAIN: &[u8] = b"onebrain:vnext:obt-shadow-eval:1\0";

/// Runtime execution mode for `onebrain-obt` (`§2.2`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u64)]
pub enum ObtRuntimeMode {
    #[default]
    Disabled = 0,
    ShadowEvaluate = 1,
    ActiveProvisional = 2,
    ActiveDomainFinal = 3,
}

impl ObtRuntimeMode {
    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Disabled),
            1 => Some(Self::ShadowEvaluate),
            2 => Some(Self::ActiveProvisional),
            3 => Some(Self::ActiveDomainFinal),
            _ => None,
        }
    }
}

/// Product-surface economic qualification status (`P0-ADR-001`, default `SimulatedNonEconomic`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u64)]
pub enum ObtEconomicStatus {
    #[default]
    SimulatedNonEconomic = 0,
    DomainQualified = 1,
}

impl ObtEconomicStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SimulatedNonEconomic => "simulated_non_economic",
            Self::DomainQualified => "domain_qualified",
        }
    }

    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::SimulatedNonEconomic),
            1 => Some(Self::DomainQualified),
            _ => None,
        }
    }
}

/// Default-off `FND-006` feature flags and independent kill switches (`§2.1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObtFeatureFlags {
    pub reward_evidence_export: bool,
    pub reward_export_kill_switch: bool,
    pub obt_ledger_v1: bool,
    pub obt_ledger_kill_switch: bool,
}

impl ObtFeatureFlags {
    pub const fn export_allowed(self) -> bool {
        self.reward_evidence_export && !self.reward_export_kill_switch
    }

    pub const fn ledger_sync_allowed(self) -> bool {
        self.obt_ledger_v1 && !self.obt_ledger_kill_switch
    }
}

/// Accepted Track A work kind carried in `ObtNoticeV2::AcceptedWork` (`D-014`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum AcceptedWorkKind {
    DelegatedEncode = 0,
    BlindFidelityVerify = 1,
}

impl AcceptedWorkKind {
    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::DelegatedEncode),
            1 => Some(Self::BlindFidelityVerify),
            _ => None,
        }
    }
}

/// Additive post-commit firewall notice v2 (`§2.2`).
///
/// Carries content identifiers and frontier/policy commitments only — never token
/// balances, prices, or mint authorizations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObtNoticeV2 {
    BenefitClosure {
        benefit_event_cid: EventCid,
        outcome_case_id: [u8; 32],
        evidence_frontier_root: [u8; 32],
        policy_cid: ObjectCid,
    },
    AcceptedWork {
        work_record_cid: ObjectCid,
        attestation_event_cid: EventCid,
        work_kind: AcceptedWorkKind,
        evidence_frontier_root: [u8; 32],
        policy_cid: ObjectCid,
    },
}

impl ObtNoticeV2 {
    pub const fn contains_mint_or_balance_authority(&self) -> bool {
        false
    }

    pub fn validate(&self) -> bool {
        match self {
            Self::BenefitClosure {
                outcome_case_id,
                evidence_frontier_root,
                policy_cid,
                ..
            } => {
                *outcome_case_id != [0u8; 32]
                    && *evidence_frontier_root != [0u8; 32]
                    && *policy_cid.as_bytes() != [0u8; 32]
            }
            Self::AcceptedWork {
                work_record_cid,
                evidence_frontier_root,
                policy_cid,
                ..
            } => {
                *work_record_cid.as_bytes() != [0u8; 32]
                    && *evidence_frontier_root != [0u8; 32]
                    && *policy_cid.as_bytes() != [0u8; 32]
            }
        }
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        match self {
            Self::BenefitClosure {
                benefit_event_cid,
                outcome_case_id,
                evidence_frontier_root,
                policy_cid,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(0)),
                (1, encode_event_cid(*benefit_event_cid)),
                (2, CanonicalValue::Bytes(outcome_case_id.to_vec())),
                (3, CanonicalValue::Bytes(evidence_frontier_root.to_vec())),
                (4, encode_object_cid(*policy_cid)),
            ]),
            Self::AcceptedWork {
                work_record_cid,
                attestation_event_cid,
                work_kind,
                evidence_frontier_root,
                policy_cid,
            } => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(1)),
                (1, encode_object_cid(*work_record_cid)),
                (2, encode_event_cid(*attestation_event_cid)),
                (3, CanonicalValue::Unsigned(*work_kind as u64)),
                (4, CanonicalValue::Bytes(evidence_frontier_root.to_vec())),
                (5, encode_object_cid(*policy_cid)),
            ]),
        }
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ControlV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_value(bytes, ResourceProfile::ControlV1)?;
        let entries = expect_map(&value, &[0, 1, 2, 3, 4, 5])?;
        match expect_u64(req_field(entries, 0)?)? {
            0 if map_lookup(entries, 5).is_none() => Ok(Self::BenefitClosure {
                benefit_event_cid: expect_event_cid(req_field(entries, 1)?)?,
                outcome_case_id: expect_bytes_exact::<32>(req_field(entries, 2)?)?,
                evidence_frontier_root: expect_bytes_exact::<32>(req_field(entries, 3)?)?,
                policy_cid: expect_object_cid(req_field(entries, 4)?)?,
            }),
            1 => {
                let kind = AcceptedWorkKind::from_code(expect_u64(req_field(entries, 3)?)?)
                    .ok_or_else(|| {
                        CanonicalError::new(
                            ku_core::foundation::CanonicalErrorKind::ForbiddenType,
                            0,
                        )
                    })?;
                Ok(Self::AcceptedWork {
                    work_record_cid: expect_object_cid(req_field(entries, 1)?)?,
                    attestation_event_cid: expect_event_cid(req_field(entries, 2)?)?,
                    work_kind: kind,
                    evidence_frontier_root: expect_bytes_exact::<32>(req_field(entries, 4)?)?,
                    policy_cid: expect_object_cid(req_field(entries, 5)?)?,
                })
            }
            _ => Err(CanonicalError::new(
                ku_core::foundation::CanonicalErrorKind::ForbiddenType,
                0,
            )),
        }
    }

    pub fn notice_id(&self) -> [u8; 32] {
        let bytes = self
            .encode_canonical_bytes()
            .expect("ObtNoticeV2 fits ControlV1 limits");
        domain_digest(OBT_NOTICE_V2_DOMAIN, &bytes)
    }
}

/// Reason a post-commit notice was safely dropped without affecting knowledge commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirewallDropReason {
    RuntimeDisabled,
    ExportFlagDisabled,
    QueueBackpressureFull,
    InvalidNoticeCommitment,
    BackendUnavailableOrCorrupt,
}

/// Firewall-safe ingest outcome returned to post-commit adapters (`NEG-OBT-001`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeIngestOutcome {
    Enqueued { notice_id: [u8; 32] },
    DuplicateIgnored { notice_id: [u8; 32] },
    DroppedFirewallSafe { reason: FirewallDropReason },
}

/// Diagnostic record persisted under `ObtRuntimeMode::ShadowEvaluate` (`M7.1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowClaimEvaluation {
    pub evaluation_id: [u8; 32],
    pub notice_id: [u8; 32],
    pub policy_cid: ObjectCid,
    pub evidence_frontier_root: [u8; 32],
    pub candidate_nullifier: ScopedNullifier,
    pub would_approve: bool,
    pub candidate_amount: ObtAmount,
    pub diagnostic_code: String,
    pub economic_status: ObtEconomicStatus,
}

impl ShadowClaimEvaluation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        notice_id: [u8; 32],
        policy_cid: ObjectCid,
        evidence_frontier_root: [u8; 32],
        candidate_nullifier: ScopedNullifier,
        would_approve: bool,
        candidate_amount: ObtAmount,
        diagnostic_code: impl Into<String>,
        economic_status: ObtEconomicStatus,
    ) -> Self {
        let diagnostic_code = diagnostic_code.into();
        let mut preimage = Vec::with_capacity(144 + diagnostic_code.len());
        preimage.extend_from_slice(&notice_id);
        preimage.extend_from_slice(policy_cid.as_bytes());
        preimage.extend_from_slice(&evidence_frontier_root);
        preimage.extend_from_slice(candidate_nullifier.as_bytes());
        preimage.push(u8::from(would_approve));
        preimage.extend_from_slice(&candidate_amount.0.to_be_bytes());
        preimage.extend_from_slice(diagnostic_code.as_bytes());
        preimage.push(economic_status as u8);
        let evaluation_id = domain_digest(SHADOW_EVAL_DOMAIN, &preimage);
        Self {
            evaluation_id,
            notice_id,
            policy_cid,
            evidence_frontier_root,
            candidate_nullifier,
            would_approve,
            candidate_amount,
            diagnostic_code,
            economic_status,
        }
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.evaluation_id.to_vec())),
            (1, CanonicalValue::Bytes(self.notice_id.to_vec())),
            (2, encode_object_cid(self.policy_cid)),
            (
                3,
                CanonicalValue::Bytes(self.evidence_frontier_root.to_vec()),
            ),
            (4, self.candidate_nullifier.to_canonical_value()),
            (5, CanonicalValue::Bool(self.would_approve)),
            (6, self.candidate_amount.to_canonical_value()),
            (7, CanonicalValue::Text(self.diagnostic_code.clone())),
            (8, CanonicalValue::Unsigned(self.economic_status as u64)),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        encode_value(&self.to_canonical_value(), ResourceProfile::ControlV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_value(bytes, ResourceProfile::ControlV1)?;
        let entries = expect_map(&value, &[0, 1, 2, 3, 4, 5, 6, 7, 8])?;
        let status =
            ObtEconomicStatus::from_code(expect_u64(req_field(entries, 8)?)?).ok_or_else(|| {
                CanonicalError::new(ku_core::foundation::CanonicalErrorKind::ForbiddenType, 0)
            })?;
        Ok(Self {
            evaluation_id: expect_bytes_exact::<32>(req_field(entries, 0)?)?,
            notice_id: expect_bytes_exact::<32>(req_field(entries, 1)?)?,
            policy_cid: expect_object_cid(req_field(entries, 2)?)?,
            evidence_frontier_root: expect_bytes_exact::<32>(req_field(entries, 3)?)?,
            candidate_nullifier: ScopedNullifier::from_canonical_value(req_field(entries, 4)?)?,
            would_approve: expect_bool(req_field(entries, 5)?)?,
            candidate_amount: ObtAmount::from_canonical_value(req_field(entries, 6)?)?,
            diagnostic_code: expect_text(req_field(entries, 7)?, 256)?,
            economic_status: status,
        })
    }
}

/// Quarantined notice record isolated after consumer/resolver errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantinedNotice {
    pub notice_id: [u8; 32],
    pub reason: String,
}

/// Sink trait persisted by `ObtFirewallConsumer` (`RedbObtStore` or `InMemoryFirewallSink`).
pub trait FirewallSinkBackend {
    fn put_outbox_notice(&mut self, notice: &ObtNoticeV2) -> Result<bool, String>;
    fn remove_outbox_notice(&mut self, notice_id: &[u8; 32]) -> Result<(), String>;
    fn put_shadow_evaluation(&mut self, eval: &ShadowClaimEvaluation) -> Result<(), String>;
    fn put_pending_mint_from_firewall(&mut self, mint: &PendingMintRecord) -> Result<(), String>;
    fn put_quarantined_notice(&mut self, quarantined: &QuarantinedNotice) -> Result<(), String>;
}

/// In-memory sink backend used in unit tests and fault-injection scenarios.
#[derive(Clone, Debug, Default)]
pub struct InMemoryFirewallSink {
    pub outbox: Vec<ObtNoticeV2>,
    pub shadow_evaluations: Vec<ShadowClaimEvaluation>,
    pub pending_mints: Vec<PendingMintRecord>,
    pub quarantined: Vec<QuarantinedNotice>,
    pub inject_failure: bool,
}

impl FirewallSinkBackend for InMemoryFirewallSink {
    fn put_outbox_notice(&mut self, notice: &ObtNoticeV2) -> Result<bool, String> {
        if self.inject_failure {
            return Err("INJECTED_DB_ERROR".to_owned());
        }
        let id = notice.notice_id();
        if self
            .outbox
            .iter()
            .any(|existing| existing.notice_id() == id)
        {
            return Ok(false);
        }
        self.outbox.push(notice.clone());
        Ok(true)
    }

    fn remove_outbox_notice(&mut self, notice_id: &[u8; 32]) -> Result<(), String> {
        if self.inject_failure {
            return Err("INJECTED_DB_ERROR".to_owned());
        }
        self.outbox.retain(|n| &n.notice_id() != notice_id);
        Ok(())
    }

    fn put_shadow_evaluation(&mut self, eval: &ShadowClaimEvaluation) -> Result<(), String> {
        if self.inject_failure {
            return Err("INJECTED_DB_ERROR".to_owned());
        }
        if !self
            .shadow_evaluations
            .iter()
            .any(|e| e.evaluation_id == eval.evaluation_id)
        {
            self.shadow_evaluations.push(eval.clone());
        }
        Ok(())
    }

    fn put_pending_mint_from_firewall(&mut self, mint: &PendingMintRecord) -> Result<(), String> {
        if self.inject_failure {
            return Err("INJECTED_DB_ERROR".to_owned());
        }
        if !self
            .pending_mints
            .iter()
            .any(|m| m.pending_mint_cid == mint.pending_mint_cid)
        {
            self.pending_mints.push(mint.clone());
        }
        Ok(())
    }

    fn put_quarantined_notice(&mut self, quarantined: &QuarantinedNotice) -> Result<(), String> {
        if self.inject_failure {
            return Err("INJECTED_DB_ERROR".to_owned());
        }
        self.quarantined.push(quarantined.clone());
        Ok(())
    }
}

/// Candidate output returned by a notice evaluation callback during queue drain.
#[derive(Clone, Debug)]
pub struct NoticeEvaluationCandidate {
    pub shadow: ShadowClaimEvaluation,
    pub pending_mints_if_active: Vec<PendingMintRecord>,
}

/// Observability report returned by `ObtFirewallConsumer::drain` (never a knowledge error).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirewallDrainReport {
    pub drained_notices: usize,
    pub shadow_evaluations_persisted: usize,
    pub pending_mints_created: usize,
    pub account_blocks_created: usize,
    pub quarantined_notices: usize,
    pub dropped_notices: usize,
}

#[derive(Clone, Debug)]
struct QueuedNotice {
    notice: ObtNoticeV2,
    attempts: u8,
}

/// Post-commit `ObtNoticeV2` consumer enforcing `FW-07` and `M7.1` `ShadowEvaluate`.
pub struct ObtFirewallConsumer {
    mode: ObtRuntimeMode,
    economic_status: ObtEconomicStatus,
    flags: ObtFeatureFlags,
    max_queue_len: usize,
    max_retries: u8,
    max_quarantine_len: usize,
    queue: VecDeque<QueuedNotice>,
    seen_notice_ids: BTreeSet<[u8; 32]>,
    local_quarantine: VecDeque<QuarantinedNotice>,
}

impl ObtFirewallConsumer {
    pub fn new(
        mode: ObtRuntimeMode,
        flags: ObtFeatureFlags,
        max_queue_len: usize,
        max_retries: u8,
    ) -> Self {
        Self {
            mode,
            economic_status: ObtEconomicStatus::SimulatedNonEconomic,
            flags,
            max_queue_len: max_queue_len.max(1),
            max_retries,
            max_quarantine_len: 1_024,
            queue: VecDeque::new(),
            seen_notice_ids: BTreeSet::new(),
            local_quarantine: VecDeque::new(),
        }
    }

    pub const fn mode(&self) -> ObtRuntimeMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ObtRuntimeMode) {
        self.mode = mode;
    }

    pub const fn economic_status(&self) -> ObtEconomicStatus {
        self.economic_status
    }

    pub fn set_economic_status(&mut self, status: ObtEconomicStatus) {
        self.economic_status = status;
    }

    pub const fn flags(&self) -> ObtFeatureFlags {
        self.flags
    }

    pub fn set_flags(&mut self, flags: ObtFeatureFlags) {
        self.flags = flags;
    }

    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    pub fn local_quarantine(&self) -> &VecDeque<QuarantinedNotice> {
        &self.local_quarantine
    }

    /// Best-effort post-commit ingest (`FW-07`). Never panics or returns errors to
    /// knowledge-plane callers even under full queue backpressure or DB failure.
    pub fn ingest_notice(
        &mut self,
        notice: ObtNoticeV2,
        sink: Option<&mut dyn FirewallSinkBackend>,
    ) -> NoticeIngestOutcome {
        if self.mode == ObtRuntimeMode::Disabled {
            return NoticeIngestOutcome::DroppedFirewallSafe {
                reason: FirewallDropReason::RuntimeDisabled,
            };
        }
        if !self.flags.export_allowed() {
            return NoticeIngestOutcome::DroppedFirewallSafe {
                reason: FirewallDropReason::ExportFlagDisabled,
            };
        }
        if !notice.validate() {
            return NoticeIngestOutcome::DroppedFirewallSafe {
                reason: FirewallDropReason::InvalidNoticeCommitment,
            };
        }

        let notice_id = notice.notice_id();
        if self.seen_notice_ids.contains(&notice_id) {
            return NoticeIngestOutcome::DuplicateIgnored { notice_id };
        }
        if self.queue.len() >= self.max_queue_len {
            return NoticeIngestOutcome::DroppedFirewallSafe {
                reason: FirewallDropReason::QueueBackpressureFull,
            };
        }

        if let Some(backend) = sink {
            if backend.put_outbox_notice(&notice).is_err() {
                return NoticeIngestOutcome::DroppedFirewallSafe {
                    reason: FirewallDropReason::BackendUnavailableOrCorrupt,
                };
            }
        }

        self.seen_notice_ids.insert(notice_id);
        self.queue.push_back(QueuedNotice {
            notice,
            attempts: 0,
        });
        NoticeIngestOutcome::Enqueued { notice_id }
    }

    /// Drain queued notices according to `self.mode`.
    ///
    /// - `Disabled` / kill-switched: drops queued notices safely with zero mutations.
    /// - `ShadowEvaluate`: persists `ShadowClaimEvaluation` ONLY (`pending_mints_created == 0`,
    ///   `account_blocks_created == 0`).
    /// - `ActiveProvisional` / `ActiveDomainFinal`: persists approved `PendingMintRecord`s,
    ///   still creating zero `AccountBlockV1`s directly (minting always enters challenge window!).
    pub fn drain<F>(
        &mut self,
        sink: &mut dyn FirewallSinkBackend,
        mut evaluator: F,
    ) -> FirewallDrainReport
    where
        F: FnMut(&ObtNoticeV2, ObtEconomicStatus) -> Result<NoticeEvaluationCandidate, String>,
    {
        let mut report = FirewallDrainReport::default();
        if self.mode == ObtRuntimeMode::Disabled || !self.flags.export_allowed() {
            while let Some(item) = self.queue.pop_front() {
                let _ = sink.remove_outbox_notice(&item.notice.notice_id());
                report.dropped_notices += 1;
            }
            return report;
        }

        let batch_len = self.queue.len();
        for _ in 0..batch_len {
            let Some(item) = self.queue.pop_front() else {
                break;
            };
            let notice_id = item.notice.notice_id();
            match evaluator(&item.notice, self.economic_status) {
                Ok(candidate) => match self.mode {
                    ObtRuntimeMode::Disabled => {
                        report.dropped_notices += 1;
                    }
                    ObtRuntimeMode::ShadowEvaluate => {
                        if sink.put_shadow_evaluation(&candidate.shadow).is_ok() {
                            let _ = sink.remove_outbox_notice(&notice_id);
                            report.drained_notices += 1;
                            report.shadow_evaluations_persisted += 1;
                            // Strictly zero PendingMintRecord, AccountBlockV1, or balance mutations!
                        } else {
                            self.retry_or_quarantine(
                                item,
                                "SHADOW_SINK_WRITE_FAILED",
                                sink,
                                &mut report,
                            );
                        }
                    }
                    ObtRuntimeMode::ActiveProvisional | ObtRuntimeMode::ActiveDomainFinal => {
                        let mut write_failed = false;
                        if sink.put_shadow_evaluation(&candidate.shadow).is_err() {
                            write_failed = true;
                        }
                        if !write_failed && candidate.shadow.would_approve {
                            for mint in &candidate.pending_mints_if_active {
                                if sink.put_pending_mint_from_firewall(mint).is_err() {
                                    write_failed = true;
                                    break;
                                }
                                report.pending_mints_created += 1;
                            }
                        }
                        if write_failed {
                            self.retry_or_quarantine(
                                item,
                                "ACTIVE_SINK_WRITE_FAILED",
                                sink,
                                &mut report,
                            );
                        } else {
                            let _ = sink.remove_outbox_notice(&notice_id);
                            report.drained_notices += 1;
                        }
                    }
                },
                Err(reason) => {
                    self.retry_or_quarantine(item, &reason, sink, &mut report);
                }
            }
        }
        report
    }

    fn retry_or_quarantine(
        &mut self,
        mut item: QueuedNotice,
        reason: &str,
        sink: &mut dyn FirewallSinkBackend,
        report: &mut FirewallDrainReport,
    ) {
        item.attempts = item.attempts.saturating_add(1);
        if item.attempts <= self.max_retries {
            self.queue.push_back(item);
            return;
        }
        let notice_id = item.notice.notice_id();
        let _ = sink.remove_outbox_notice(&notice_id);
        let q = QuarantinedNotice {
            notice_id,
            reason: reason.to_owned(),
        };
        let _ = sink.put_quarantined_notice(&q);
        if self.local_quarantine.len() >= self.max_quarantine_len {
            self.local_quarantine.pop_front();
        }
        self.local_quarantine.push_back(q);
        report.quarantined_notices += 1;
    }
}
