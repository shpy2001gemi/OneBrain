//! Release-build real-QUIC performance and long-soak evidence for DR-M5.
//!
//! The same harness owns the short CI smoke, 24-hour nightly and 72-hour
//! pre-release profiles. A short run can pass its own budgets but cannot claim
//! either long-duration qualification.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use ku_core::foundation::{
    AcceptedInput, AffordanceOrigin, AffordanceSemantics, ConceptCcid, DisclosureClass, EventCid,
    KnowledgeAffordance, MetabolicViewPolicy, NamespaceCommitment, NodeId, ObjectReference,
    ReceptorAcceptanceProfile, ReceptorCardinality, ReceptorDefinition, ReceptorOrigin,
    ResourceProfile, SelectorCid, SemanticFrameSet, StatementFrame, StatementId, StatementLocator,
    StatementQualifiers, TermRef, UnknownConstraintPolicy, UseEvidencePayload, UseMode,
    RECEPTOR_DEFINITION_KIND,
};
use ku_kql::vnext_matcher::MatcherMetricConcepts;
use ku_kql::vnext_private_need::{LocalNeedVaultKey, PrivateNeedBundle};
use ku_kql::vnext_query::{KnowledgeNeedIr, QueryDefinition};
use ku_kql::vnext_reunion::LocalNeedTarget;
use ku_kql::vnext_standing_need::StandingNeed;
use ku_net::vnext_chaos::{
    expected_oracle_root, run_delivery_trace, ChaosTraceConfig, ChaosTraceReport,
};
use onebrain_protocol::ReconcileManifestKind;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::vnext_config::VNextNetworkPolicy;
use crate::vnext_distributed_kql::{DistributedKqlBudget, DistributedKqlRuntime};
use crate::vnext_distributed_pomv::DistributedPomvRuntime;
use crate::vnext_network_runtime::{
    VNextNetworkRuntime, VNextNetworkRuntimeError, VNextNetworkRuntimeStatus,
};
use crate::vnext_outbox::{OutboundIntentState, OutboundTransferIntent};
use crate::vnext_route_authority::{LocalPolicyRegistry, LocalPolicyVersion};

pub const SOAK_RELEASE_PROFILE: &str = "onebrain/dr-m5-soak-release/1";
pub const NIGHTLY_QUALIFICATION_SECONDS: u64 = 24 * 60 * 60;
pub const PRE_RELEASE_QUALIFICATION_SECONDS: u64 = 72 * 60 * 60;
pub const MIN_FAULT_CYCLES: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoakProfile {
    Smoke,
    Nightly24h,
    PreRelease72h,
}

impl SoakProfile {
    pub fn parse(value: &str) -> Result<Self, SoakReleaseError> {
        match value {
            "smoke" => Ok(Self::Smoke),
            "nightly-24h" => Ok(Self::Nightly24h),
            "pre-release-72h" => Ok(Self::PreRelease72h),
            _ => Err(SoakReleaseError::InvalidConfig(format!(
                "unknown soak profile {value}"
            ))),
        }
    }

    pub const fn minimum_elapsed_seconds(self) -> u64 {
        match self {
            Self::Smoke => 0,
            Self::Nightly24h => NIGHTLY_QUALIFICATION_SECONDS,
            Self::PreRelease72h => PRE_RELEASE_QUALIFICATION_SECONDS,
        }
    }

    pub const fn is_release_qualification(self) -> bool {
        matches!(self, Self::PreRelease72h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyBudget {
    pub p50_max_micros: u64,
    pub p95_max_micros: u64,
    pub p99_max_micros: u64,
}

impl LatencyBudget {
    fn validate(self) -> Result<(), SoakReleaseError> {
        if self.p50_max_micros == 0
            || self.p50_max_micros > self.p95_max_micros
            || self.p95_max_micros > self.p99_max_micros
        {
            return Err(SoakReleaseError::InvalidConfig(
                "latency percentile budget order".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrowthBudget {
    pub hard_cap: u64,
    pub max_growth: u64,
    pub max_positive_slope_per_cycle: u64,
}

impl GrowthBudget {
    fn validate(self) -> Result<(), SoakReleaseError> {
        if self.hard_cap == 0
            || self.max_growth > self.hard_cap
            || self.max_positive_slope_per_cycle > self.max_growth
        {
            return Err(SoakReleaseError::InvalidConfig(
                "growth budget bounds".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoakReleaseBudgets {
    pub quic_connect: LatencyBudget,
    pub fsync: LatencyBudget,
    pub rss: GrowthBudget,
    pub disk: GrowthBudget,
    pub task_count: GrowthBudget,
    pub incremental_scan_max_micros: u64,
    pub incremental_scan_max_records: u64,
}

impl Default for SoakReleaseBudgets {
    fn default() -> Self {
        Self {
            quic_connect: LatencyBudget {
                p50_max_micros: 500_000,
                p95_max_micros: 1_000_000,
                p99_max_micros: 2_000_000,
            },
            fsync: LatencyBudget {
                p50_max_micros: 100_000,
                p95_max_micros: 500_000,
                p99_max_micros: 2_000_000,
            },
            rss: GrowthBudget {
                hard_cap: 512 * 1_048_576,
                max_growth: 128 * 1_048_576,
                max_positive_slope_per_cycle: 8 * 1_048_576,
            },
            disk: GrowthBudget {
                hard_cap: 512 * 1_048_576,
                max_growth: 32 * 1_048_576,
                max_positive_slope_per_cycle: 2 * 1_048_576,
            },
            task_count: GrowthBudget {
                hard_cap: 512,
                max_growth: 16,
                max_positive_slope_per_cycle: 4,
            },
            incremental_scan_max_micros: 250_000,
            incremental_scan_max_records: 64,
        }
    }
}

impl SoakReleaseBudgets {
    pub fn validate(self) -> Result<(), SoakReleaseError> {
        self.quic_connect.validate()?;
        self.fsync.validate()?;
        self.rss.validate()?;
        self.disk.validate()?;
        self.task_count.validate()?;
        if self.incremental_scan_max_micros == 0
            || self.incremental_scan_max_records == 0
            || self.incremental_scan_max_records > 4_096
        {
            return Err(SoakReleaseError::InvalidConfig(
                "incremental scan budget".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoakRunConfig {
    pub profile: SoakProfile,
    pub quic_samples: u64,
    pub fsync_samples: u64,
    pub cycle_interval_seconds: u64,
    pub budgets: SoakReleaseBudgets,
}

impl SoakRunConfig {
    pub fn smoke() -> Self {
        Self {
            profile: SoakProfile::Smoke,
            quic_samples: 16,
            fsync_samples: 16,
            cycle_interval_seconds: 0,
            budgets: SoakReleaseBudgets::default(),
        }
    }

    pub fn nightly_24h() -> Self {
        Self {
            profile: SoakProfile::Nightly24h,
            quic_samples: 64,
            fsync_samples: 64,
            cycle_interval_seconds: 60,
            budgets: SoakReleaseBudgets::default(),
        }
    }

    pub fn pre_release_72h() -> Self {
        Self {
            profile: SoakProfile::PreRelease72h,
            quic_samples: 128,
            fsync_samples: 128,
            cycle_interval_seconds: 60,
            budgets: SoakReleaseBudgets::default(),
        }
    }

    pub fn for_profile(profile: SoakProfile) -> Self {
        match profile {
            SoakProfile::Smoke => Self::smoke(),
            SoakProfile::Nightly24h => Self::nightly_24h(),
            SoakProfile::PreRelease72h => Self::pre_release_72h(),
        }
    }

    pub fn validate(self) -> Result<(), SoakReleaseError> {
        self.budgets.validate()?;
        if self.quic_samples < 3
            || self.quic_samples > 4_096
            || self.fsync_samples < 3
            || self.fsync_samples > 4_096
        {
            return Err(SoakReleaseError::InvalidConfig(
                "sample count outside 3..=4096".to_owned(),
            ));
        }
        if self.profile != SoakProfile::Smoke && self.cycle_interval_seconds == 0 {
            return Err(SoakReleaseError::InvalidConfig(
                "long soak requires a non-zero cycle interval".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub samples: u64,
    pub p50_micros: u64,
    pub p95_micros: u64,
    pub p99_micros: u64,
    pub max_micros: u64,
    pub budget: LatencyBudget,
}

impl LatencyPercentiles {
    pub fn passes(&self) -> bool {
        self.samples >= 3
            && self.p50_micros <= self.budget.p50_max_micros
            && self.p95_micros <= self.budget.p95_max_micros
            && self.p99_micros <= self.budget.p99_max_micros
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrowthMetric {
    pub available: bool,
    pub samples: u64,
    pub first: u64,
    pub last: u64,
    pub peak: u64,
    pub positive_growth: u64,
    pub signed_slope_per_cycle: i64,
    pub budget: GrowthBudget,
}

impl GrowthMetric {
    pub fn passes(&self) -> bool {
        self.available
            && self.samples >= MIN_FAULT_CYCLES + 1
            && self.peak <= self.budget.hard_cap
            && self.positive_growth <= self.budget.max_growth
            && self.signed_slope_per_cycle
                <= i64::try_from(self.budget.max_positive_slope_per_cycle).unwrap_or(i64::MAX)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncrementalScanMetric {
    pub lane: String,
    pub first_scan_records: u64,
    pub first_scan_micros: u64,
    pub drained_scan_records: u64,
    pub drained_scan_micros: u64,
    pub max_records: u64,
    pub max_micros: u64,
}

impl IncrementalScanMetric {
    pub fn passes(&self) -> bool {
        self.first_scan_records > 0
            && self.first_scan_records <= self.max_records
            && self.first_scan_micros <= self.max_micros
            && self.drained_scan_records == 0
            && self.drained_scan_micros <= self.max_micros
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSignalSnapshot {
    pub authenticated_sessions: u64,
    pub active_sessions: u64,
    pub rejected_sessions: u64,
    pub accepted_records: u64,
    pub deferred_records: u64,
    pub rejected_records: u64,
    pub pending_outbox: u64,
    pub retry_exhausted_outbox: u64,
    pub contains_high_cardinality_labels: bool,
    pub contains_private_need_labels: bool,
    pub claims_network_completion: bool,
}

impl RuntimeSignalSnapshot {
    fn from_status(status: &VNextNetworkRuntimeStatus) -> Self {
        Self {
            authenticated_sessions: status.authenticated_sessions,
            active_sessions: status.active_sessions as u64,
            rejected_sessions: status.rejected_sessions,
            accepted_records: status.accepted_records,
            deferred_records: status.deferred_records,
            rejected_records: status.rejected_records,
            pending_outbox: status.observability.gauges.pending_outbox,
            retry_exhausted_outbox: status.observability.gauges.retry_exhausted_outbox,
            contains_high_cardinality_labels: status.observability.contains_high_cardinality_labels,
            contains_private_need_labels: status.observability.contains_private_need_labels,
            claims_network_completion: status.claims_network_completion
                || status.observability.claims_network_completion,
        }
    }

    fn merge(&mut self, other: Self) {
        self.authenticated_sessions = self
            .authenticated_sessions
            .saturating_add(other.authenticated_sessions);
        self.active_sessions = self.active_sessions.saturating_add(other.active_sessions);
        self.rejected_sessions = self
            .rejected_sessions
            .saturating_add(other.rejected_sessions);
        self.accepted_records = self.accepted_records.saturating_add(other.accepted_records);
        self.deferred_records = self.deferred_records.saturating_add(other.deferred_records);
        self.rejected_records = self.rejected_records.saturating_add(other.rejected_records);
        self.pending_outbox = self.pending_outbox.saturating_add(other.pending_outbox);
        self.retry_exhausted_outbox = self
            .retry_exhausted_outbox
            .saturating_add(other.retry_exhausted_outbox);
        self.contains_high_cardinality_labels |= other.contains_high_cardinality_labels;
        self.contains_private_need_labels |= other.contains_private_need_labels;
        self.claims_network_completion |= other.claims_network_completion;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoakReleaseReport {
    pub profile: String,
    pub host_os: String,
    pub host_arch: String,
    pub run_profile: SoakProfile,
    pub elapsed_seconds: u64,
    pub cycles: u64,
    pub slow_peer_cycles: u64,
    pub bounded_flood_cycles: u64,
    pub partition_reunion_cycles: u64,
    pub fair_redelivery_oracle_matches: bool,
    pub quic_connect_latency: LatencyPercentiles,
    pub fsync_latency: LatencyPercentiles,
    pub rss_bytes: GrowthMetric,
    pub disk_bytes: GrowthMetric,
    pub task_count: GrowthMetric,
    pub kql_incremental_scan: IncrementalScanMetric,
    pub pomv_incremental_scan: IncrementalScanMetric,
    pub final_runtime_signals: RuntimeSignalSnapshot,
    pub active_sessions_after_shutdown: u64,
    pub task_leak_detected: bool,
    pub changes_wallet_state: bool,
    pub changes_obt_state: bool,
    pub grants_authority: bool,
    pub claims_truth: bool,
    pub claims_benefit: bool,
    pub claims_network_completion: bool,
    pub qualification_met: bool,
    pub pre_release_qualified: bool,
    pub rollback_recommended: bool,
    pub rollback_reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SemanticInvariantSnapshot {
    changes_wallet_state: bool,
    changes_obt_state: bool,
    grants_authority: bool,
    claims_truth: bool,
    claims_benefit: bool,
    claims_network_completion: bool,
}

impl SoakReleaseReport {
    pub fn passes(&self) -> bool {
        self.quic_connect_latency.passes()
            && self.fsync_latency.passes()
            && self.rss_bytes.passes()
            && self.disk_bytes.passes()
            && self.task_count.passes()
            && self.kql_incremental_scan.passes()
            && self.pomv_incremental_scan.passes()
            && self.cycles >= MIN_FAULT_CYCLES
            && self.slow_peer_cycles > 0
            && self.bounded_flood_cycles > 0
            && self.partition_reunion_cycles > 0
            && self.fair_redelivery_oracle_matches
            && self.active_sessions_after_shutdown == 0
            && !self.task_leak_detected
            && !self.changes_wallet_state
            && !self.changes_obt_state
            && !self.grants_authority
            && !self.claims_truth
            && !self.claims_benefit
            && !self.claims_network_completion
            && self.qualification_met
            && !self.rollback_recommended
            && self.rollback_reasons.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum SoakReleaseError {
    #[error("invalid M5-07 configuration: {0}")]
    InvalidConfig(String),
    #[error("M5-07 I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("M5-07 network failure: {0}")]
    Network(#[from] VNextNetworkRuntimeError),
    #[error("M5-07 fixture failure: {0}")]
    Fixture(String),
    #[error("M5-07 timeout: {0}")]
    Timeout(&'static str),
}

pub async fn run_soak_release(
    data_dir: &Path,
    config: SoakRunConfig,
) -> Result<SoakReleaseReport, SoakReleaseError> {
    config.validate()?;
    fs::create_dir_all(data_dir)?;
    let sender_dir = data_dir.join("sender");
    let receiver_dir = data_dir.join("receiver");
    fs::create_dir_all(&sender_dir)?;
    fs::create_dir_all(&receiver_dir)?;
    let started = Instant::now();

    let fsync_latency = measure_fsync(
        &data_dir.join("fsync-probe.bin"),
        config.fsync_samples,
        config.budgets.fsync,
    )?;
    let mut rss_samples = Vec::new();
    let mut disk_samples = Vec::new();
    let mut task_samples = Vec::new();

    let policy = VNextNetworkPolicy::default();
    let mut sender = VNextNetworkRuntime::start(
        &sender_dir,
        "127.0.0.1:0"
            .parse()
            .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?,
        policy,
    )
    .await?;
    let mut receiver = Some(
        VNextNetworkRuntime::start(
            &receiver_dir,
            "127.0.0.1:0"
                .parse()
                .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?,
            policy,
        )
        .await?,
    );

    let quic_connect_latency = measure_quic_connects(
        &sender,
        receiver
            .as_ref()
            .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
        config.quic_samples,
        config.budgets.quic_connect,
    )
    .await?;
    let (kql_incremental_scan, pomv_incremental_scan, semantic_invariants) =
        measure_incremental_scans(
            &sender,
            receiver
                .as_ref()
                .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
            &receiver_dir,
            config.budgets,
        )
        .await?;
    // Warm every fault path, including one durable receiver reopen, before
    // establishing the steady-state resource baseline.
    run_slow_peer_cycle(
        &sender,
        receiver
            .as_ref()
            .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
    )
    .await?;
    run_bounded_flood_cycle(
        &sender,
        receiver
            .as_ref()
            .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
        policy.max_sessions_per_peer,
    )
    .await?;
    run_partition_reunion_cycle(&sender, &mut receiver, &receiver_dir, policy).await?;
    wait_for_no_active_sessions(
        receiver
            .as_ref()
            .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
    )
    .await?;
    // Growth starts after the fixed release workload has warmed QUIC, Redb and
    // both selector/type indexes and exercised one full fault family window.
    // Expected provisioning and first-close allocation are not a leak slope.
    collect_resource_samples(
        data_dir,
        &mut rss_samples,
        &mut disk_samples,
        &mut task_samples,
    )?;

    let expected_chaos_root = expected_oracle_root(32)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
    let mut fair_redelivery_oracle_matches = true;
    let mut cycles = 0u64;
    let mut slow_peer_cycles = 0u64;
    let mut bounded_flood_cycles = 0u64;
    let mut partition_reunion_cycles = 0u64;
    let mut grants_authority = semantic_invariants.grants_authority;
    let mut claims_network_completion = semantic_invariants.claims_network_completion;

    loop {
        let chaos = run_delivery_trace(ChaosTraceConfig {
            seed: cycles,
            steps: 77,
            record_count: 32,
        })
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
        fair_redelivery_oracle_matches &= chaos_oracle_passes(&chaos, expected_chaos_root);
        grants_authority |= chaos.grants_authority;
        claims_network_completion |= chaos.claims_network_completion;

        match cycles % 3 {
            0 => {
                run_slow_peer_cycle(
                    &sender,
                    receiver
                        .as_ref()
                        .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
                )
                .await?;
                slow_peer_cycles = slow_peer_cycles.saturating_add(1);
            }
            1 => {
                run_bounded_flood_cycle(
                    &sender,
                    receiver
                        .as_ref()
                        .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
                    policy.max_sessions_per_peer,
                )
                .await?;
                bounded_flood_cycles = bounded_flood_cycles.saturating_add(1);
            }
            _ => {
                run_partition_reunion_cycle(&sender, &mut receiver, &receiver_dir, policy).await?;
                partition_reunion_cycles = partition_reunion_cycles.saturating_add(1);
            }
        }
        cycles = cycles.saturating_add(1);
        collect_resource_samples(
            data_dir,
            &mut rss_samples,
            &mut disk_samples,
            &mut task_samples,
        )?;

        let minimum_elapsed = config.profile.minimum_elapsed_seconds();
        if config.profile == SoakProfile::Smoke && cycles >= MIN_FAULT_CYCLES {
            break;
        }
        if minimum_elapsed > 0
            && started.elapsed().as_secs() >= minimum_elapsed
            && cycles >= MIN_FAULT_CYCLES
        {
            break;
        }
        if config.cycle_interval_seconds > 0 {
            tokio::time::sleep(Duration::from_secs(config.cycle_interval_seconds)).await;
        }
    }

    let mut final_runtime_signals = RuntimeSignalSnapshot::from_status(
        &receiver
            .as_ref()
            .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?
            .status(),
    );
    final_runtime_signals.merge(RuntimeSignalSnapshot::from_status(&sender.status()));
    wait_for_no_active_sessions(
        receiver
            .as_ref()
            .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?,
    )
    .await?;
    let mut active_sessions_after_shutdown = 0u64;
    if let Some(mut runtime) = receiver.take() {
        runtime.shutdown().await;
        wait_for_no_active_sessions(&runtime).await?;
        active_sessions_after_shutdown =
            active_sessions_after_shutdown.saturating_add(runtime.status().active_sessions as u64);
    }
    sender.shutdown().await;
    wait_for_no_active_sessions(&sender).await?;
    active_sessions_after_shutdown =
        active_sessions_after_shutdown.saturating_add(sender.status().active_sessions as u64);
    tokio::time::sleep(Duration::from_millis(50)).await;
    collect_resource_samples(
        data_dir,
        &mut rss_samples,
        &mut disk_samples,
        &mut task_samples,
    )?;

    let elapsed_seconds = started.elapsed().as_secs();
    let qualification_met =
        elapsed_seconds >= config.profile.minimum_elapsed_seconds() && cycles >= MIN_FAULT_CYCLES;
    let rss_bytes = growth_metric(&rss_samples, config.budgets.rss);
    let disk_bytes = growth_metric(&disk_samples, config.budgets.disk);
    let task_count = growth_metric(&task_samples, config.budgets.task_count);
    let task_leak_detected = !task_count.passes();
    let mut report = SoakReleaseReport {
        profile: SOAK_RELEASE_PROFILE.to_owned(),
        host_os: std::env::consts::OS.to_owned(),
        host_arch: std::env::consts::ARCH.to_owned(),
        run_profile: config.profile,
        elapsed_seconds,
        cycles,
        slow_peer_cycles,
        bounded_flood_cycles,
        partition_reunion_cycles,
        fair_redelivery_oracle_matches,
        quic_connect_latency,
        fsync_latency,
        rss_bytes,
        disk_bytes,
        task_count,
        kql_incremental_scan,
        pomv_incremental_scan,
        final_runtime_signals,
        active_sessions_after_shutdown,
        task_leak_detected,
        changes_wallet_state: semantic_invariants.changes_wallet_state,
        changes_obt_state: semantic_invariants.changes_obt_state,
        grants_authority,
        claims_truth: semantic_invariants.claims_truth,
        claims_benefit: semantic_invariants.claims_benefit,
        claims_network_completion,
        qualification_met,
        pre_release_qualified: false,
        rollback_recommended: false,
        rollback_reasons: Vec::new(),
    };
    report.rollback_reasons = rollback_reasons(&report);
    report.rollback_recommended = !report.rollback_reasons.is_empty();
    report.pre_release_qualified =
        config.profile.is_release_qualification() && report.passes_without_release_flag();
    Ok(report)
}

impl SoakReleaseReport {
    fn passes_without_release_flag(&self) -> bool {
        self.quic_connect_latency.passes()
            && self.fsync_latency.passes()
            && self.rss_bytes.passes()
            && self.disk_bytes.passes()
            && self.task_count.passes()
            && self.kql_incremental_scan.passes()
            && self.pomv_incremental_scan.passes()
            && self.cycles >= MIN_FAULT_CYCLES
            && self.slow_peer_cycles > 0
            && self.bounded_flood_cycles > 0
            && self.partition_reunion_cycles > 0
            && self.fair_redelivery_oracle_matches
            && self.active_sessions_after_shutdown == 0
            && !self.task_leak_detected
            && !self.changes_wallet_state
            && !self.changes_obt_state
            && !self.grants_authority
            && !self.claims_truth
            && !self.claims_benefit
            && !self.claims_network_completion
            && self.qualification_met
            && !self.rollback_recommended
            && self.rollback_reasons.is_empty()
    }
}

fn rollback_reasons(report: &SoakReleaseReport) -> Vec<String> {
    let mut reasons = Vec::new();
    if !report.quic_connect_latency.passes() {
        reasons.push("QUIC_LATENCY_BUDGET".to_owned());
    }
    if !report.fsync_latency.passes() {
        reasons.push("FSYNC_LATENCY_BUDGET".to_owned());
    }
    if !report.rss_bytes.passes() {
        reasons.push("RSS_GROWTH_OR_CAP".to_owned());
    }
    if !report.disk_bytes.passes() {
        reasons.push("DISK_GROWTH_OR_CAP".to_owned());
    }
    if !report.task_count.passes() || report.task_leak_detected {
        reasons.push("TASK_GROWTH_OR_LEAK".to_owned());
    }
    if !report.kql_incremental_scan.passes() {
        reasons.push("KQL_INCREMENTAL_SCAN_BUDGET".to_owned());
    }
    if !report.pomv_incremental_scan.passes() {
        reasons.push("POMV_INCREMENTAL_SCAN_BUDGET".to_owned());
    }
    if !report.fair_redelivery_oracle_matches {
        reasons.push("M3_REUNION_ORACLE".to_owned());
    }
    if report.active_sessions_after_shutdown != 0 {
        reasons.push("ACTIVE_SESSION_LEAK".to_owned());
    }
    if report.changes_wallet_state || report.changes_obt_state {
        reasons.push("WALLET_OR_OBT_MUTATION".to_owned());
    }
    if report.grants_authority
        || report.claims_truth
        || report.claims_benefit
        || report.claims_network_completion
    {
        reasons.push("SEMANTIC_AUTHORITY_AMPLIFICATION".to_owned());
    }
    if !report.qualification_met {
        reasons.push("DURATION_OR_CYCLE_EVIDENCE_INCOMPLETE".to_owned());
    }
    reasons
}

fn latency_percentiles(
    mut values: Vec<u64>,
    budget: LatencyBudget,
) -> Result<LatencyPercentiles, SoakReleaseError> {
    budget.validate()?;
    if values.len() < 3 {
        return Err(SoakReleaseError::InvalidConfig(
            "at least three latency samples required".to_owned(),
        ));
    }
    values.sort_unstable();
    Ok(LatencyPercentiles {
        samples: values.len() as u64,
        p50_micros: percentile(&values, 50),
        p95_micros: percentile(&values, 95),
        p99_micros: percentile(&values, 99),
        max_micros: *values.last().unwrap_or(&u64::MAX),
        budget,
    })
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    let rank = sorted.len().saturating_mul(percentile).saturating_add(99) / 100;
    sorted[rank.saturating_sub(1).min(sorted.len().saturating_sub(1))]
}

fn growth_metric(samples: &[Option<u64>], budget: GrowthBudget) -> GrowthMetric {
    let values = samples.iter().copied().flatten().collect::<Vec<_>>();
    let available = values.len() == samples.len() && !values.is_empty();
    let first = values.first().copied().unwrap_or(0);
    let last = values.last().copied().unwrap_or(0);
    let peak = values.iter().copied().max().unwrap_or(0);
    let positive_growth = last.saturating_sub(first);
    let denominator = values.len().saturating_sub(1).max(1) as i128;
    let signed_delta = i128::from(last) - i128::from(first);
    let signed_slope_per_cycle = i64::try_from(signed_delta / denominator).unwrap_or_else(|_| {
        if signed_delta.is_negative() {
            i64::MIN
        } else {
            i64::MAX
        }
    });
    GrowthMetric {
        available,
        samples: values.len() as u64,
        first,
        last,
        peak,
        positive_growth,
        signed_slope_per_cycle,
        budget,
    }
}

fn measure_fsync(
    path: &Path,
    samples: u64,
    budget: LatencyBudget,
) -> Result<LatencyPercentiles, SoakReleaseError> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let payload = [0xA5; 4_096];
    let mut values = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let started = Instant::now();
        file.write_all(&payload)?;
        file.sync_data()?;
        values.push(elapsed_micros(started));
    }
    latency_percentiles(values, budget)
}

async fn measure_quic_connects(
    sender: &VNextNetworkRuntime,
    receiver: &VNextNetworkRuntime,
    samples: u64,
    budget: LatencyBudget,
) -> Result<LatencyPercentiles, SoakReleaseError> {
    let mut values = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let started = Instant::now();
        let session = sender.connect(receiver.local_addr()).await?;
        values.push(elapsed_micros(started));
        session.close();
        drop(session);
        tokio::task::yield_now().await;
    }
    latency_percentiles(values, budget)
}

async fn measure_incremental_scans(
    sender: &VNextNetworkRuntime,
    receiver: &VNextNetworkRuntime,
    receiver_dir: &Path,
    budgets: SoakReleaseBudgets,
) -> Result<
    (
        IncrementalScanMetric,
        IncrementalScanMetric,
        SemanticInvariantSnapshot,
    ),
    SoakReleaseError,
> {
    let selector = SelectorCid::from_bytes([0xA7; 32]);
    let namespace = NamespaceCommitment::derive(b"m5-07-incremental-scan", [0xA8; 32])
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
    let peer = NodeId::from_bytes(receiver.status().principal);
    let affordance = benchmark_affordance()
        .to_knowledge_object(DisclosureClass::Public)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
    let (affordance_bytes, _) = affordance
        .encode(ResourceProfile::ObjectV1)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
    let use_evidence = benchmark_use_evidence()
        .to_knowledge_object(DisclosureClass::Public)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
    let (use_evidence_bytes, _) = use_evidence
        .encode(ResourceProfile::ObjectV1)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error:?}")))?;
    let intents = [
        OutboundTransferIntent::new(
            peer,
            receiver.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::Object,
            affordance_bytes,
        )
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?,
        OutboundTransferIntent::new(
            peer,
            receiver.local_addr(),
            selector,
            namespace,
            DisclosureClass::Public,
            ReconcileManifestKind::Object,
            use_evidence_bytes,
        )
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?,
    ];
    for intent in &intents {
        sender.enqueue_outbound(intent)?;
    }
    wait_acknowledged(sender, &intents).await?;

    let scan_records = budgets.incremental_scan_max_records;
    let mut kql =
        DistributedKqlRuntime::open(receiver_dir, LocalNeedVaultKey::from_bytes([0xB1; 32]))
            .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    kql.register_private_need(benchmark_private_need(selector))
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let kql_budget = DistributedKqlBudget {
        max_scan_records: scan_records,
        max_affordances: scan_records,
        max_pairs: scan_records,
        max_proposals: scan_records,
    };
    let kql_first_started = Instant::now();
    let kql_first = kql
        .process_one_hop_affordance_delta(receiver, selector, kql_budget)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let kql_first_micros = elapsed_micros(kql_first_started);
    let kql_drained_started = Instant::now();
    let kql_drained = kql
        .process_one_hop_affordance_delta(receiver, selector, kql_budget)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let kql_drained_micros = elapsed_micros(kql_drained_started);
    let kql_metric = IncrementalScanMetric {
        lane: "distributed-kql-one-hop".to_owned(),
        first_scan_records: kql_first.scanned_public_affordances,
        first_scan_micros: kql_first_micros,
        drained_scan_records: kql_drained.scanned_public_affordances,
        drained_scan_micros: kql_drained_micros,
        max_records: scan_records,
        max_micros: budgets.incremental_scan_max_micros,
    };

    let policy_version = LocalPolicyVersion::new(1)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let pomv = DistributedPomvRuntime::open(
        receiver_dir,
        usize::try_from(scan_records)
            .map_err(|_| SoakReleaseError::Fixture("PoMV scan limit overflow".to_owned()))?,
        LocalPolicyRegistry::new([(
            policy_version,
            MetabolicViewPolicy {
                policy_ref: object_reference(0x27),
                accepted_evidence_policies: vec![object_reference(0x25)],
                recent_event_horizon: scan_records,
            },
        )])
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?,
    )
    .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let pomv_first_started = Instant::now();
    let pomv_first = pomv
        .materialize_public_use_view(receiver, selector, object_reference(0x21), policy_version)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let pomv_first_micros = elapsed_micros(pomv_first_started);
    let pomv_drained_started = Instant::now();
    let pomv_drained = pomv
        .materialize_public_use_view(receiver, selector, object_reference(0x21), policy_version)
        .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?;
    let pomv_drained_micros = elapsed_micros(pomv_drained_started);
    let pomv_metric = IncrementalScanMetric {
        lane: "distributed-pomv-view".to_owned(),
        first_scan_records: pomv_first
            .changed_object_records
            .saturating_add(pomv_first.changed_event_records),
        first_scan_micros: pomv_first_micros,
        drained_scan_records: pomv_drained
            .changed_object_records
            .saturating_add(pomv_drained.changed_event_records),
        drained_scan_micros: pomv_drained_micros,
        max_records: scan_records,
        max_micros: budgets.incremental_scan_max_micros,
    };
    let semantic = SemanticInvariantSnapshot {
        changes_wallet_state: pomv_first.changes_wallet_state
            || pomv_drained.changes_wallet_state
            || pomv.changes_wallet_state(),
        changes_obt_state: pomv_first.changes_obt_state
            || pomv_drained.changes_obt_state
            || pomv.changes_obt_state(),
        grants_authority: kql_first.claims_automatic_materialization
            || kql_first.claims_automatic_adoption
            || kql_drained.claims_automatic_materialization
            || kql_drained.claims_automatic_adoption,
        claims_truth: pomv_first.claims_truth || pomv_drained.claims_truth,
        claims_benefit: pomv_first.claims_benefit || pomv_drained.claims_benefit,
        claims_network_completion: kql_first.claims_network_completion
            || kql_drained.claims_network_completion
            || pomv_first.claims_network_completion
            || pomv_drained.claims_network_completion,
    };
    Ok((kql_metric, pomv_metric, semantic))
}

async fn wait_acknowledged(
    runtime: &VNextNetworkRuntime,
    intents: &[OutboundTransferIntent],
) -> Result<(), SoakReleaseError> {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let complete = intents.iter().all(|intent| {
                runtime
                    .outbound_intent(&intent.id)
                    .ok()
                    .flatten()
                    .is_some_and(|stored| stored.state == OutboundIntentState::Acknowledged)
            });
            if complete {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| SoakReleaseError::Timeout("outbound acknowledgement"))
}

async fn run_slow_peer_cycle(
    sender: &VNextNetworkRuntime,
    receiver: &VNextNetworkRuntime,
) -> Result<(), SoakReleaseError> {
    let session = sender.connect(receiver.local_addr()).await?;
    tokio::time::sleep(Duration::from_millis(25)).await;
    session.close();
    drop(session);
    Ok(())
}

async fn run_bounded_flood_cycle(
    sender: &VNextNetworkRuntime,
    receiver: &VNextNetworkRuntime,
    max_sessions_per_peer: usize,
) -> Result<(), SoakReleaseError> {
    let mut sessions = Vec::with_capacity(max_sessions_per_peer);
    for _ in 0..max_sessions_per_peer {
        sessions.push(sender.connect(receiver.local_addr()).await?);
    }
    if sender.connect(receiver.local_addr()).await.is_ok() {
        return Err(SoakReleaseError::Fixture(
            "peer session flood exceeded configured cap".to_owned(),
        ));
    }
    for session in sessions {
        session.close();
    }
    tokio::time::sleep(Duration::from_millis(25)).await;
    Ok(())
}

async fn run_partition_reunion_cycle(
    sender: &VNextNetworkRuntime,
    receiver: &mut Option<VNextNetworkRuntime>,
    receiver_dir: &Path,
    policy: VNextNetworkPolicy,
) -> Result<(), SoakReleaseError> {
    let mut stopped = receiver
        .take()
        .ok_or_else(|| SoakReleaseError::Fixture("receiver missing".to_owned()))?;
    let old_addr = stopped.local_addr();
    stopped.shutdown().await;
    drop(stopped);
    let partition_attempt =
        tokio::time::timeout(Duration::from_millis(500), sender.connect(old_addr)).await;
    if matches!(partition_attempt, Ok(Ok(_))) {
        return Err(SoakReleaseError::Fixture(
            "partition accepted a new QUIC session".to_owned(),
        ));
    }
    let restarted = VNextNetworkRuntime::start(
        receiver_dir,
        "127.0.0.1:0"
            .parse()
            .map_err(|error| SoakReleaseError::Fixture(format!("{error}")))?,
        policy,
    )
    .await?;
    let session = sender.connect(restarted.local_addr()).await?;
    session.close();
    *receiver = Some(restarted);
    Ok(())
}

async fn wait_for_no_active_sessions(
    runtime: &VNextNetworkRuntime,
) -> Result<(), SoakReleaseError> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut zero_since = None;
        loop {
            if runtime.status().active_sessions == 0 {
                let since = zero_since.get_or_insert_with(Instant::now);
                // Runtime shutdown aborts the accept loop, while an already
                // accepted handshake may still publish its active-session
                // guard. Require a bounded quiescence window so that a
                // transient zero cannot hide that late task.
                if since.elapsed() >= Duration::from_millis(250) {
                    return;
                }
            } else {
                zero_since = None;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| SoakReleaseError::Timeout("active session drain"))
}

fn chaos_oracle_passes(report: &ChaosTraceReport, expected_root: [u8; 32]) -> bool {
    report.final_oracle_root == expected_root
        && report.accepted_after_fair_redelivery == 32
        && !report.grants_authority
        && !report.claims_network_completion
}

fn collect_resource_samples(
    data_dir: &Path,
    rss: &mut Vec<Option<u64>>,
    disk: &mut Vec<Option<u64>>,
    tasks: &mut Vec<Option<u64>>,
) -> Result<(), SoakReleaseError> {
    rss.push(current_rss_bytes());
    disk.push(Some(directory_bytes(data_dir)?));
    tasks.push(current_task_count());
    Ok(())
}

fn directory_bytes(path: &Path) -> Result<u64, SoakReleaseError> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total = total
                .checked_add(directory_bytes(&entry.path())?)
                .ok_or_else(|| SoakReleaseError::Fixture("disk size overflow".to_owned()))?;
        } else if metadata.is_file() {
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| SoakReleaseError::Fixture("disk size overflow".to_owned()))?;
        }
    }
    Ok(total)
}

#[cfg(target_os = "linux")]
fn current_rss_bytes() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    kib.checked_mul(1_024)
}

#[cfg(target_os = "windows")]
fn current_rss_bytes() -> Option<u64> {
    use std::ffi::c_void;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: GetCurrentProcess returns a process-local pseudo handle and the
    // initialized structure is passed with its exact byte size.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    (ok != 0)
        .then(|| u64::try_from(counters.working_set_size).ok())
        .flatten()
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct MacOsProcTaskInfo {
    virtual_size: u64,
    resident_size: u64,
    total_user: u64,
    total_system: u64,
    threads_user: u64,
    threads_system: u64,
    policy: i32,
    faults: i32,
    pageins: i32,
    cow_faults: i32,
    messages_sent: i32,
    messages_received: i32,
    syscalls_mach: i32,
    syscalls_unix: i32,
    context_switches: i32,
    thread_count: i32,
    running_thread_count: i32,
    priority: i32,
}

#[cfg(target_os = "macos")]
#[link(name = "proc")]
extern "C" {
    fn proc_pidinfo(
        pid: i32,
        flavor: i32,
        argument: u64,
        buffer: *mut std::ffi::c_void,
        buffer_size: i32,
    ) -> i32;
}

#[cfg(target_os = "macos")]
fn macos_proc_task_info() -> Option<MacOsProcTaskInfo> {
    const PROC_PIDTASKINFO: i32 = 4;
    let pid = i32::try_from(std::process::id()).ok()?;
    let buffer_size = i32::try_from(std::mem::size_of::<MacOsProcTaskInfo>()).ok()?;
    let mut info = std::mem::MaybeUninit::<MacOsProcTaskInfo>::zeroed();
    // SAFETY: proc_pidinfo writes at most buffer_size bytes into an aligned
    // proc_taskinfo-compatible buffer owned by this function.
    let written = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            buffer_size,
        )
    };
    if written != buffer_size {
        return None;
    }
    // SAFETY: A full proc_taskinfo payload was written above.
    Some(unsafe { info.assume_init() })
}

#[cfg(target_os = "macos")]
fn current_rss_bytes() -> Option<u64> {
    let bytes = macos_proc_task_info()?.resident_size;
    (bytes > 0).then_some(bytes)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn current_rss_bytes() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn current_task_count() -> Option<u64> {
    fs::read_dir("/proc/self/task")
        .ok()?
        .count()
        .try_into()
        .ok()
}

#[cfg(target_os = "windows")]
fn current_task_count() -> Option<u64> {
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    const INVALID_HANDLE_VALUE: isize = -1;

    #[repr(C)]
    struct ThreadEntry32 {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_process_id: u32,
        base_priority: i32,
        delta_priority: i32,
        flags: u32,
    }

    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> isize;
        fn Thread32First(snapshot: isize, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snapshot: isize, entry: *mut ThreadEntry32) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn CloseHandle(handle: isize) -> i32;
    }

    // SAFETY: The snapshot is read-only, the entry carries the exact Windows
    // structure size, and the handle is closed on every path after creation.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let process_id = GetCurrentProcessId();
        let mut entry = ThreadEntry32 {
            size: std::mem::size_of::<ThreadEntry32>() as u32,
            usage: 0,
            thread_id: 0,
            owner_process_id: 0,
            base_priority: 0,
            delta_priority: 0,
            flags: 0,
        };
        let mut count = 0u64;
        if Thread32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.owner_process_id == process_id {
                    count = count.saturating_add(1);
                }
                entry.size = std::mem::size_of::<ThreadEntry32>() as u32;
                if Thread32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        Some(count)
    }
}

#[cfg(target_os = "macos")]
fn current_task_count() -> Option<u64> {
    let count = u64::try_from(macos_proc_task_info()?.thread_count).ok()?;
    (count > 0).then_some(count)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn current_task_count() -> Option<u64> {
    None
}

fn benchmark_affordance() -> KnowledgeAffordance {
    let empty = SemanticFrameSet { statements: vec![] };
    KnowledgeAffordance {
        sources: vec![object_reference(0x11)],
        offered_roles: vec![concept(0x12)],
        accepted_inputs: vec![AcceptedInput {
            receptor_definition: object_reference(0x13),
            role: concept(0x14),
            required: true,
        }],
        semantics: AffordanceSemantics {
            preconditions: empty.clone(),
            outputs: SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(0),
                    operator_or_predicate: concept(0x15),
                    arguments: vec![TermRef::Concept(concept(0x16))],
                    constraints: vec![],
                    qualifiers: StatementQualifiers::default(),
                }],
            },
            effects: empty.clone(),
            properties: empty.clone(),
            invariants: empty.clone(),
            operating_conditions: empty.clone(),
            limits: empty,
        },
        abstraction_patterns: vec![],
        origin: AffordanceOrigin::Explicit {
            claims: vec![StatementLocator {
                object: object_reference(0x11),
                statement_index: 0,
            }],
        },
    }
}

fn benchmark_receptor() -> ReceptorDefinition {
    ReceptorDefinition {
        role: concept(0x12),
        expected_types: vec![concept(0x14)],
        hard_constraints: vec![],
        cardinality: ReceptorCardinality::new(1, Some(1))
            .expect("frozen M5-07 receptor cardinality is valid"),
        origin: ReceptorOrigin::Declared {
            source: StatementLocator {
                object: object_reference(0x31),
                statement_index: 0,
            },
        },
        acceptance: ReceptorAcceptanceProfile {
            policy: object_reference(0x32),
            required_evidence_kinds: vec![],
            unknown_constraint_policy: UnknownConstraintPolicy::KeepUnresolved,
        },
    }
}

fn benchmark_private_need(selector: SelectorCid) -> PrivateNeedBundle {
    let receptor = benchmark_receptor();
    let receptor_object = receptor
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .expect("frozen M5-07 receptor encodes");
    let (_, receptor_cid) = receptor_object
        .encode(ResourceProfile::ObjectV1)
        .expect("frozen M5-07 receptor is bounded");
    let receptor_definition =
        ObjectReference::new(RECEPTOR_DEFINITION_KIND.0, receptor_cid.into_bytes());
    let query_definition = QueryDefinition {
        need: KnowledgeNeedIr {
            receptor_definitions: vec![receptor_definition.clone()],
            desired_roles: vec![concept(0x12)],
            goal: benchmark_frames(0x16),
            local_context: benchmark_frames(0x33),
            privacy: DisclosureClass::LocalOnly,
        },
        query_policy: object_reference(0x34),
        exploration_policy: object_reference(0x35),
    };
    let query_cid = query_definition
        .private_cid()
        .expect("frozen M5-07 private query is canonical");
    PrivateNeedBundle {
        query_definition,
        target: LocalNeedTarget {
            need: StandingNeed::new_local(
                receptor_definition,
                query_cid,
                selector,
                object_reference(0x36),
                [0x37; 32],
            ),
            receptor,
            required_semantics: benchmark_frames(0x16),
            local_context: benchmark_frames(0x33),
            generator: object_reference(0x38),
            derivation_rule: Some(object_reference(0x39)),
            evidence: vec![object_reference(0x3A)],
            index_commitment: Some(object_reference(0x3B)),
            rule_commitment: Some(object_reference(0x3C)),
            metrics: MatcherMetricConcepts {
                structural_fit: concept(0x3D),
                constraint_fit: concept(0x3E),
            },
            unmapped_reason: concept(0x3F),
            source_frontier: EventCid::from_bytes([0x40; 32]),
            created_at_evaluation: 1,
            expires_after_evaluations: 10,
        },
    }
}

fn benchmark_frames(marker: u8) -> SemanticFrameSet {
    SemanticFrameSet {
        statements: vec![StatementFrame {
            statement_id: StatementId(0),
            operator_or_predicate: concept(0x15),
            arguments: vec![TermRef::Concept(concept(marker))],
            constraints: vec![],
            qualifiers: StatementQualifiers::default(),
        }],
    }
}

fn benchmark_use_evidence() -> UseEvidencePayload {
    UseEvidencePayload {
        subjects: vec![object_reference(0x21)],
        mode: UseMode::Application,
        actor_class: concept(0x22),
        task_context_commitment: [0x23; 32],
        causal_role: concept(0x24),
        assembly: None,
        mapping: None,
        outcome_observation: None,
        use_policy: object_reference(0x25),
        observed_frontier: [0x26; 32],
    }
}

fn concept(byte: u8) -> ConceptCcid {
    ConceptCcid::from_bytes([byte; 16])
}

fn object_reference(byte: u8) -> ObjectReference {
    ObjectReference::new(0, [byte; 32])
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m5_07_percentiles_and_growth_budgets_fail_closed() {
        let latency = latency_percentiles(
            vec![1, 2, 3, 4, 100],
            LatencyBudget {
                p50_max_micros: 3,
                p95_max_micros: 100,
                p99_max_micros: 100,
            },
        )
        .unwrap();
        assert_eq!(latency.p50_micros, 3);
        assert_eq!(latency.p95_micros, 100);
        assert_eq!(latency.p99_micros, 100);
        assert!(latency.passes());

        let invalid = LatencyBudget {
            p50_max_micros: 10,
            p95_max_micros: 9,
            p99_max_micros: 11,
        };
        assert!(invalid.validate().is_err());
        let growth = growth_metric(
            &[Some(10), Some(20), Some(30), Some(40)],
            GrowthBudget {
                hard_cap: 100,
                max_growth: 20,
                max_positive_slope_per_cycle: 5,
            },
        );
        assert!(!growth.passes());
    }

    #[test]
    fn m5_07_short_run_cannot_claim_nightly_or_pre_release_qualification() {
        assert_eq!(SoakProfile::Nightly24h.minimum_elapsed_seconds(), 86_400);
        assert_eq!(
            SoakProfile::PreRelease72h.minimum_elapsed_seconds(),
            259_200
        );
        assert!(!SoakProfile::Nightly24h.is_release_qualification());
        assert!(SoakProfile::PreRelease72h.is_release_qualification());
        let mut config = SoakRunConfig::nightly_24h();
        config.cycle_interval_seconds = 0;
        assert!(config.validate().is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn m5_07_release_smoke_uses_real_quic_fsync_and_all_fault_cycles() {
        let directory = tempfile::tempdir().unwrap();
        let report = run_soak_release(directory.path(), SoakRunConfig::smoke())
            .await
            .unwrap();
        assert!(report.passes(), "{report:#?}");
        assert_eq!(report.run_profile, SoakProfile::Smoke);
        assert_eq!(report.cycles, 3);
        assert_eq!(report.slow_peer_cycles, 1);
        assert_eq!(report.bounded_flood_cycles, 1);
        assert_eq!(report.partition_reunion_cycles, 1);
        assert!(!report.pre_release_qualified);
        assert!(!report.rollback_recommended);
        assert!(report.rollback_reasons.is_empty());
        assert_eq!(report.host_os, std::env::consts::OS);
        assert_eq!(report.host_arch, std::env::consts::ARCH);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn m5_07_macos_proc_metrics_are_available() {
        assert!(current_rss_bytes().is_some());
        assert!(current_task_count().is_some());
    }
}
