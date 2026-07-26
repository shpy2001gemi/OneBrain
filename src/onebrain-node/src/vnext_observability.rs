//! Fixed-cardinality, privacy-safe observability for the vNext runtime.
//!
//! This module deliberately accepts no NodeID, selector, FeedID, object CID,
//! private Need, or free-form label. Callers can only record one of the frozen
//! reason codes and bounded numeric measurements.

use std::array;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};

pub const VNEXT_OBSERVABILITY_PROFILE_MAJOR: u16 = 1;
pub const REASON_CODE_COUNT: usize = 22;

const RECORD_BYTES_BUCKETS: [u64; 8] = [
    64,
    1_024,
    4_096,
    16_384,
    65_536,
    262_144,
    1_048_576,
    u64::MAX,
];
const WORK_BUCKETS: [u64; 8] = [1, 2, 4, 8, 16, 64, 256, u64::MAX];
const AGE_SECONDS_BUCKETS: [u64; 8] = [0, 1, 5, 30, 60, 300, 900, u64::MAX];
const LAG_RECORDS_BUCKETS: [u64; 8] = [0, 1, 2, 4, 8, 16, 64, u64::MAX];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(usize)]
pub enum VNextReasonCode {
    AcceptedNew = 0,
    AlreadyPresent = 1,
    Replayed = 2,
    DeferredMissingDependency = 3,
    DeferredBudget = 4,
    QuarantinedInvalid = 5,
    RejectedContextBinding = 6,
    RejectedSelector = 7,
    RejectedLength = 8,
    RejectedContentCid = 9,
    RejectedSink = 10,
    RejectedAuthority = 11,
    RejectedStorage = 12,
    RejectedRateLimit = 13,
    RejectedReplay = 14,
    RejectedSession = 15,
    RejectedProtocol = 16,
    JournalFailure = 17,
    OutboxRetryExhausted = 18,
    PomvIdentityConflict = 19,
    RegistryFallback = 20,
    TransportFailure = 21,
}

impl VNextReasonCode {
    pub const ALL: [Self; REASON_CODE_COUNT] = [
        Self::AcceptedNew,
        Self::AlreadyPresent,
        Self::Replayed,
        Self::DeferredMissingDependency,
        Self::DeferredBudget,
        Self::QuarantinedInvalid,
        Self::RejectedContextBinding,
        Self::RejectedSelector,
        Self::RejectedLength,
        Self::RejectedContentCid,
        Self::RejectedSink,
        Self::RejectedAuthority,
        Self::RejectedStorage,
        Self::RejectedRateLimit,
        Self::RejectedReplay,
        Self::RejectedSession,
        Self::RejectedProtocol,
        Self::JournalFailure,
        Self::OutboxRetryExhausted,
        Self::PomvIdentityConflict,
        Self::RegistryFallback,
        Self::TransportFailure,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::AcceptedNew => "ACCEPTED_NEW",
            Self::AlreadyPresent => "ALREADY_PRESENT",
            Self::Replayed => "REPLAYED",
            Self::DeferredMissingDependency => "DEFERRED_MISSING_DEPENDENCY",
            Self::DeferredBudget => "DEFERRED_BUDGET",
            Self::QuarantinedInvalid => "QUARANTINED_INVALID",
            Self::RejectedContextBinding => "REJECTED_CONTEXT_BINDING",
            Self::RejectedSelector => "REJECTED_SELECTOR",
            Self::RejectedLength => "REJECTED_LENGTH",
            Self::RejectedContentCid => "REJECTED_CONTENT_CID",
            Self::RejectedSink => "REJECTED_SINK",
            Self::RejectedAuthority => "REJECTED_AUTHORITY",
            Self::RejectedStorage => "REJECTED_STORAGE",
            Self::RejectedRateLimit => "REJECTED_RATE_LIMIT",
            Self::RejectedReplay => "REJECTED_REPLAY",
            Self::RejectedSession => "REJECTED_SESSION",
            Self::RejectedProtocol => "REJECTED_PROTOCOL",
            Self::JournalFailure => "JOURNAL_FAILURE",
            Self::OutboxRetryExhausted => "OUTBOX_RETRY_EXHAUSTED",
            Self::PomvIdentityConflict => "POMV_IDENTITY_CONFLICT",
            Self::RegistryFallback => "REGISTRY_FALLBACK",
            Self::TransportFailure => "TRANSPORT_FAILURE",
        }
    }

    const fn is_accepted(self) -> bool {
        matches!(self, Self::AcceptedNew)
    }

    const fn is_already_present(self) -> bool {
        matches!(self, Self::AlreadyPresent)
    }

    const fn is_replayed(self) -> bool {
        matches!(self, Self::Replayed)
    }

    const fn is_deferred(self) -> bool {
        matches!(self, Self::DeferredMissingDependency | Self::DeferredBudget)
    }

    const fn is_quarantined(self) -> bool {
        matches!(self, Self::QuarantinedInvalid | Self::PomvIdentityConflict)
    }

    const fn is_rejected(self) -> bool {
        matches!(
            self,
            Self::RejectedContextBinding
                | Self::RejectedSelector
                | Self::RejectedLength
                | Self::RejectedContentCid
                | Self::RejectedSink
                | Self::RejectedAuthority
                | Self::RejectedStorage
                | Self::RejectedRateLimit
                | Self::RejectedReplay
                | Self::RejectedSession
                | Self::RejectedProtocol
                | Self::JournalFailure
                | Self::OutboxRetryExhausted
                | Self::TransportFailure
        )
    }

    const fn is_warning(self) -> bool {
        self.is_deferred()
            || self.is_quarantined()
            || self.is_rejected()
            || matches!(self, Self::RegistryFallback)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[repr(u64)]
pub enum VNextRegistryTelemetryState {
    #[default]
    Unknown,
    Disabled,
    Loaded,
    FallbackV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextReasonCount {
    pub reason: VNextReasonCode,
    pub count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextHistogramSnapshot {
    pub inclusive_upper_bounds: Vec<u64>,
    pub counts: Vec<u64>,
    pub samples: u64,
    pub sum: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextOutcomeSnapshot {
    pub accepted_new: u64,
    pub already_present: u64,
    pub replayed: u64,
    pub deferred: u64,
    pub quarantined: u64,
    pub rejected: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextResourceSnapshot {
    pub admitted_bytes: u64,
    pub admitted_work_units: u64,
    pub rate_limited: u64,
    pub record_bytes: VNextHistogramSnapshot,
    pub work_units: VNextHistogramSnapshot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextRuntimeGaugeSnapshot {
    pub active_journals: u64,
    pub pending_outbox: u64,
    pub retry_exhausted_outbox: u64,
    pub oldest_pending_outbox_age_seconds: Option<u64>,
    pub journal_age_seconds: VNextHistogramSnapshot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextReconciliationSnapshot {
    pub selector_scans: u64,
    pub partial_selector_scans: u64,
    pub assessed_frontier_items: u64,
    pub latest_lag_records: u64,
    pub lag_records: VNextHistogramSnapshot,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextPomvSnapshot {
    pub identity_conflicts: u64,
    pub latest_view_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VNextObservabilitySnapshot {
    pub profile_major: u16,
    pub reasons: Vec<VNextReasonCount>,
    pub outcomes: VNextOutcomeSnapshot,
    pub resources: VNextResourceSnapshot,
    pub gauges: VNextRuntimeGaugeSnapshot,
    pub reconciliation: VNextReconciliationSnapshot,
    pub pomv: VNextPomvSnapshot,
    pub registry_state: VNextRegistryTelemetryState,
    pub contains_high_cardinality_labels: bool,
    pub contains_private_need_labels: bool,
    pub claims_network_completion: bool,
}

impl Default for VNextObservabilitySnapshot {
    fn default() -> Self {
        VNextObservability::default().snapshot(VNextRegistryTelemetryState::Unknown)
    }
}

struct AtomicHistogram {
    bounds: [u64; 8],
    counts: [AtomicU64; 8],
    samples: AtomicU64,
    sum: AtomicU64,
}

impl AtomicHistogram {
    fn new(bounds: [u64; 8]) -> Self {
        Self {
            bounds,
            counts: array::from_fn(|_| AtomicU64::new(0)),
            samples: AtomicU64::new(0),
            sum: AtomicU64::new(0),
        }
    }

    fn observe(&self, value: u64) {
        let bucket = self
            .bounds
            .iter()
            .position(|upper| value <= *upper)
            .unwrap_or(self.bounds.len() - 1);
        self.counts[bucket].fetch_add(1, Ordering::Relaxed);
        self.samples.fetch_add(1, Ordering::Relaxed);
        self.sum.fetch_add(value, Ordering::Relaxed);
    }

    fn snapshot(&self) -> VNextHistogramSnapshot {
        VNextHistogramSnapshot {
            inclusive_upper_bounds: self.bounds.to_vec(),
            counts: self
                .counts
                .iter()
                .map(|count| count.load(Ordering::Relaxed))
                .collect(),
            samples: self.samples.load(Ordering::Relaxed),
            sum: self.sum.load(Ordering::Relaxed),
        }
    }
}

pub struct VNextObservability {
    reasons: [AtomicU64; REASON_CODE_COUNT],
    accepted_new: AtomicU64,
    already_present: AtomicU64,
    replayed: AtomicU64,
    deferred: AtomicU64,
    quarantined: AtomicU64,
    rejected: AtomicU64,
    admitted_bytes: AtomicU64,
    admitted_work_units: AtomicU64,
    rate_limited: AtomicU64,
    record_bytes: AtomicHistogram,
    work_units: AtomicHistogram,
    active_journals: AtomicU64,
    pending_outbox: AtomicU64,
    retry_exhausted_outbox: AtomicU64,
    oldest_pending_outbox_age_seconds: AtomicU64,
    oldest_pending_outbox_age_known: AtomicU64,
    journal_age_seconds: AtomicHistogram,
    selector_scans: AtomicU64,
    partial_selector_scans: AtomicU64,
    assessed_frontier_items: AtomicU64,
    latest_lag_records: AtomicU64,
    lag_records: AtomicHistogram,
    pomv_identity_conflicts: AtomicU64,
    latest_pomv_view_revision: AtomicU64,
    registry_state: AtomicU64,
}

impl Default for VNextObservability {
    fn default() -> Self {
        Self {
            reasons: array::from_fn(|_| AtomicU64::new(0)),
            accepted_new: AtomicU64::new(0),
            already_present: AtomicU64::new(0),
            replayed: AtomicU64::new(0),
            deferred: AtomicU64::new(0),
            quarantined: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            admitted_bytes: AtomicU64::new(0),
            admitted_work_units: AtomicU64::new(0),
            rate_limited: AtomicU64::new(0),
            record_bytes: AtomicHistogram::new(RECORD_BYTES_BUCKETS),
            work_units: AtomicHistogram::new(WORK_BUCKETS),
            active_journals: AtomicU64::new(0),
            pending_outbox: AtomicU64::new(0),
            retry_exhausted_outbox: AtomicU64::new(0),
            oldest_pending_outbox_age_seconds: AtomicU64::new(0),
            oldest_pending_outbox_age_known: AtomicU64::new(0),
            journal_age_seconds: AtomicHistogram::new(AGE_SECONDS_BUCKETS),
            selector_scans: AtomicU64::new(0),
            partial_selector_scans: AtomicU64::new(0),
            assessed_frontier_items: AtomicU64::new(0),
            latest_lag_records: AtomicU64::new(0),
            lag_records: AtomicHistogram::new(LAG_RECORDS_BUCKETS),
            pomv_identity_conflicts: AtomicU64::new(0),
            latest_pomv_view_revision: AtomicU64::new(0),
            registry_state: AtomicU64::new(VNextRegistryTelemetryState::Unknown as u64),
        }
    }
}

impl VNextObservability {
    pub fn record(&self, reason: VNextReasonCode, bytes: u64, work_units: u64) {
        self.record_count(reason, 1, bytes, work_units);
    }

    pub fn record_count(&self, reason: VNextReasonCode, count: u64, bytes: u64, work_units: u64) {
        if count == 0 {
            return;
        }
        self.reasons[reason as usize].fetch_add(count, Ordering::Relaxed);
        if reason.is_accepted() {
            self.accepted_new.fetch_add(count, Ordering::Relaxed);
        }
        if reason.is_already_present() {
            self.already_present.fetch_add(count, Ordering::Relaxed);
        }
        if reason.is_replayed() {
            self.replayed.fetch_add(count, Ordering::Relaxed);
        }
        if reason.is_deferred() {
            self.deferred.fetch_add(count, Ordering::Relaxed);
        }
        if reason.is_quarantined() {
            self.quarantined.fetch_add(count, Ordering::Relaxed);
        }
        if reason.is_rejected() {
            self.rejected.fetch_add(count, Ordering::Relaxed);
        }
        if reason == VNextReasonCode::RejectedRateLimit {
            self.rate_limited.fetch_add(count, Ordering::Relaxed);
        }
        if reason == VNextReasonCode::OutboxRetryExhausted {
            self.retry_exhausted_outbox
                .fetch_add(count, Ordering::Relaxed);
        }
        if reason == VNextReasonCode::PomvIdentityConflict {
            self.pomv_identity_conflicts
                .fetch_add(count, Ordering::Relaxed);
        }
        if bytes > 0 {
            self.admitted_bytes.fetch_add(bytes, Ordering::Relaxed);
            self.record_bytes.observe(bytes);
        }
        if work_units > 0 {
            self.admitted_work_units
                .fetch_add(work_units, Ordering::Relaxed);
            self.work_units.observe(work_units);
        }

        if reason.is_warning() {
            tracing::warn!(
                target: "onebrain::vnext::observability",
                reason_code = reason.code(),
                count,
                bytes,
                work_units,
                "vNext bounded outcome"
            );
        } else {
            tracing::debug!(
                target: "onebrain::vnext::observability",
                reason_code = reason.code(),
                count,
                bytes,
                work_units,
                "vNext bounded outcome"
            );
        }
    }

    pub fn observe_resources(&self, bytes: u64, work_units: u64) {
        if bytes > 0 {
            self.admitted_bytes.fetch_add(bytes, Ordering::Relaxed);
            self.record_bytes.observe(bytes);
        }
        if work_units > 0 {
            self.admitted_work_units
                .fetch_add(work_units, Ordering::Relaxed);
            self.work_units.observe(work_units);
        }
    }

    pub fn begin_journal(self: &std::sync::Arc<Self>) -> VNextJournalObservation {
        self.active_journals.fetch_add(1, Ordering::Relaxed);
        VNextJournalObservation {
            telemetry: std::sync::Arc::clone(self),
            opened_at: Instant::now(),
        }
    }

    pub fn observe_outbox(
        &self,
        pending: u64,
        retry_exhausted: u64,
        oldest_pending_age_seconds: Option<u64>,
    ) {
        self.pending_outbox.store(pending, Ordering::Relaxed);
        self.retry_exhausted_outbox
            .store(retry_exhausted, Ordering::Relaxed);
        match oldest_pending_age_seconds {
            Some(age) => {
                self.oldest_pending_outbox_age_seconds
                    .store(age, Ordering::Relaxed);
                self.oldest_pending_outbox_age_known
                    .store(1, Ordering::Relaxed);
            }
            None => {
                self.oldest_pending_outbox_age_seconds
                    .store(0, Ordering::Relaxed);
                self.oldest_pending_outbox_age_known
                    .store(0, Ordering::Relaxed);
            }
        }
    }

    pub fn observe_reconciliation_lag(&self, pending_records: u64) {
        self.latest_lag_records
            .store(pending_records, Ordering::Relaxed);
        self.lag_records.observe(pending_records);
    }

    pub fn observe_selector_coverage(&self, assessed_frontier_items: u64, has_continuation: bool) {
        self.selector_scans.fetch_add(1, Ordering::Relaxed);
        if has_continuation {
            self.partial_selector_scans.fetch_add(1, Ordering::Relaxed);
        }
        self.assessed_frontier_items
            .fetch_add(assessed_frontier_items, Ordering::Relaxed);
    }

    pub fn observe_pomv(&self, conflicts: u64, view_revision: u64) {
        self.record_count(VNextReasonCode::PomvIdentityConflict, conflicts, 0, 0);
        self.latest_pomv_view_revision
            .fetch_max(view_revision, Ordering::Relaxed);
    }

    pub fn observe_registry_state(&self, state: VNextRegistryTelemetryState) {
        let previous = self.registry_state.swap(state as u64, Ordering::Relaxed);
        if state == VNextRegistryTelemetryState::FallbackV1
            && previous != VNextRegistryTelemetryState::FallbackV1 as u64
        {
            self.record(VNextReasonCode::RegistryFallback, 0, 1);
        }
    }

    pub fn snapshot(
        &self,
        registry_state: VNextRegistryTelemetryState,
    ) -> VNextObservabilitySnapshot {
        let stored_registry_state = match self.registry_state.load(Ordering::Relaxed) {
            value if value == VNextRegistryTelemetryState::Disabled as u64 => {
                VNextRegistryTelemetryState::Disabled
            }
            value if value == VNextRegistryTelemetryState::Loaded as u64 => {
                VNextRegistryTelemetryState::Loaded
            }
            value if value == VNextRegistryTelemetryState::FallbackV1 as u64 => {
                VNextRegistryTelemetryState::FallbackV1
            }
            _ => VNextRegistryTelemetryState::Unknown,
        };
        VNextObservabilitySnapshot {
            profile_major: VNEXT_OBSERVABILITY_PROFILE_MAJOR,
            reasons: VNextReasonCode::ALL
                .into_iter()
                .map(|reason| VNextReasonCount {
                    reason,
                    count: self.reasons[reason as usize].load(Ordering::Relaxed),
                })
                .collect(),
            outcomes: VNextOutcomeSnapshot {
                accepted_new: self.accepted_new.load(Ordering::Relaxed),
                already_present: self.already_present.load(Ordering::Relaxed),
                replayed: self.replayed.load(Ordering::Relaxed),
                deferred: self.deferred.load(Ordering::Relaxed),
                quarantined: self.quarantined.load(Ordering::Relaxed),
                rejected: self.rejected.load(Ordering::Relaxed),
            },
            resources: VNextResourceSnapshot {
                admitted_bytes: self.admitted_bytes.load(Ordering::Relaxed),
                admitted_work_units: self.admitted_work_units.load(Ordering::Relaxed),
                rate_limited: self.rate_limited.load(Ordering::Relaxed),
                record_bytes: self.record_bytes.snapshot(),
                work_units: self.work_units.snapshot(),
            },
            gauges: VNextRuntimeGaugeSnapshot {
                active_journals: self.active_journals.load(Ordering::Relaxed),
                pending_outbox: self.pending_outbox.load(Ordering::Relaxed),
                retry_exhausted_outbox: self.retry_exhausted_outbox.load(Ordering::Relaxed),
                oldest_pending_outbox_age_seconds: (self
                    .oldest_pending_outbox_age_known
                    .load(Ordering::Relaxed)
                    != 0)
                    .then(|| {
                        self.oldest_pending_outbox_age_seconds
                            .load(Ordering::Relaxed)
                    }),
                journal_age_seconds: self.journal_age_seconds.snapshot(),
            },
            reconciliation: VNextReconciliationSnapshot {
                selector_scans: self.selector_scans.load(Ordering::Relaxed),
                partial_selector_scans: self.partial_selector_scans.load(Ordering::Relaxed),
                assessed_frontier_items: self.assessed_frontier_items.load(Ordering::Relaxed),
                latest_lag_records: self.latest_lag_records.load(Ordering::Relaxed),
                lag_records: self.lag_records.snapshot(),
            },
            pomv: VNextPomvSnapshot {
                identity_conflicts: self.pomv_identity_conflicts.load(Ordering::Relaxed),
                latest_view_revision: self.latest_pomv_view_revision.load(Ordering::Relaxed),
            },
            registry_state: if registry_state == VNextRegistryTelemetryState::Unknown {
                stored_registry_state
            } else {
                registry_state
            },
            contains_high_cardinality_labels: false,
            contains_private_need_labels: false,
            claims_network_completion: false,
        }
    }
}

pub struct VNextJournalObservation {
    telemetry: std::sync::Arc<VNextObservability>,
    opened_at: Instant,
}

impl Drop for VNextJournalObservation {
    fn drop(&mut self) {
        self.telemetry
            .active_journals
            .fetch_sub(1, Ordering::Relaxed);
        self.telemetry
            .journal_age_seconds
            .observe(self.opened_at.elapsed().as_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn exact_counter_transitions_and_histograms_are_deterministic() {
        let telemetry = VNextObservability::default();
        telemetry.record(VNextReasonCode::AcceptedNew, 100, 5);
        telemetry.record(VNextReasonCode::AlreadyPresent, 100, 2);
        telemetry.record(VNextReasonCode::Replayed, 0, 0);
        telemetry.record(VNextReasonCode::DeferredMissingDependency, 10, 1);
        telemetry.record(VNextReasonCode::RejectedRateLimit, 0, 0);

        let snapshot = telemetry.snapshot(VNextRegistryTelemetryState::Loaded);
        assert_eq!(snapshot.outcomes.accepted_new, 1);
        assert_eq!(snapshot.outcomes.already_present, 1);
        assert_eq!(snapshot.outcomes.replayed, 1);
        assert_eq!(snapshot.outcomes.deferred, 1);
        assert_eq!(snapshot.outcomes.rejected, 1);
        assert_eq!(snapshot.resources.admitted_bytes, 210);
        assert_eq!(snapshot.resources.admitted_work_units, 8);
        assert_eq!(snapshot.resources.rate_limited, 1);
        assert_eq!(snapshot.resources.record_bytes.samples, 3);
        assert_eq!(snapshot.resources.work_units.samples, 3);
        assert_eq!(snapshot.reasons.len(), REASON_CODE_COUNT);
    }

    #[test]
    fn journal_outbox_coverage_and_pomv_gauges_are_bounded_numeric_state() {
        let telemetry = Arc::new(VNextObservability::default());
        {
            let _journal = telemetry.begin_journal();
            assert_eq!(
                telemetry
                    .snapshot(VNextRegistryTelemetryState::Unknown)
                    .gauges
                    .active_journals,
                1
            );
        }
        telemetry.observe_outbox(7, 2, Some(11));
        telemetry.observe_reconciliation_lag(4);
        telemetry.observe_selector_coverage(3, true);
        telemetry.observe_pomv(2, 9);
        telemetry.observe_registry_state(VNextRegistryTelemetryState::FallbackV1);
        telemetry.observe_registry_state(VNextRegistryTelemetryState::FallbackV1);

        let snapshot = telemetry.snapshot(VNextRegistryTelemetryState::FallbackV1);
        assert_eq!(snapshot.gauges.active_journals, 0);
        assert_eq!(snapshot.gauges.pending_outbox, 7);
        assert_eq!(snapshot.gauges.retry_exhausted_outbox, 2);
        assert_eq!(snapshot.gauges.oldest_pending_outbox_age_seconds, Some(11));
        assert_eq!(snapshot.reconciliation.selector_scans, 1);
        assert_eq!(snapshot.reconciliation.partial_selector_scans, 1);
        assert_eq!(snapshot.reconciliation.assessed_frontier_items, 3);
        assert_eq!(snapshot.reconciliation.latest_lag_records, 4);
        assert_eq!(snapshot.pomv.identity_conflicts, 2);
        assert_eq!(snapshot.pomv.latest_view_revision, 9);
        assert_eq!(
            snapshot.registry_state,
            VNextRegistryTelemetryState::FallbackV1
        );
        assert_eq!(
            snapshot
                .reasons
                .iter()
                .find(|reason| reason.reason == VNextReasonCode::RegistryFallback)
                .unwrap()
                .count,
            1
        );
    }

    #[test]
    fn serialized_snapshot_has_no_identity_selector_or_private_need_label_surface() {
        let telemetry = VNextObservability::default();
        telemetry.record(VNextReasonCode::RejectedProtocol, 32, 1);
        let json =
            serde_json::to_string(&telemetry.snapshot(VNextRegistryTelemetryState::Disabled))
                .unwrap();
        for forbidden in [
            "NodeID",
            "selector_label",
            "standing_need_id",
            "local_query",
            "feed_id",
            "object_cid",
            "peer_id",
        ] {
            assert!(!json.contains(forbidden));
        }
        assert!(json.contains("\"contains_high_cardinality_labels\":false"));
        assert!(json.contains("\"contains_private_need_labels\":false"));
        assert!(json.contains("\"claims_network_completion\":false"));
    }

    #[test]
    fn reason_code_inventory_is_fixed_unique_and_low_cardinality() {
        let mut codes = VNextReasonCode::ALL
            .into_iter()
            .map(VNextReasonCode::code)
            .collect::<Vec<_>>();
        let original = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(original, REASON_CODE_COUNT);
        assert_eq!(codes.len(), REASON_CODE_COUNT);
        assert!(codes.iter().all(|code| code.len() <= 40));
    }
}
