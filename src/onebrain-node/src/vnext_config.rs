//! Safe-default runtime gates for independently deployable vNext capabilities.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextFeature {
    ObjectEventV1,
    ObpRp,
    DistributedKqlOneHop,
    PublicUseEvidencePublish,
    DistributedPomvView,
    InventoryShadow,
    ProviderLease,
    Fidelity,
    RewardEvidenceExport,
    CheckpointGc,
    Riblt,
    LegacyAdapter,
}

impl VNextFeature {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ObjectEventV1 => "object_event_v1",
            Self::ObpRp => "obp_rp",
            Self::DistributedKqlOneHop => "distributed_kql_one_hop",
            Self::PublicUseEvidencePublish => "public_use_evidence_publish",
            Self::DistributedPomvView => "distributed_pomv_view",
            Self::InventoryShadow => "inventory_shadow",
            Self::ProviderLease => "provider_lease",
            Self::Fidelity => "fidelity",
            Self::RewardEvidenceExport => "reward_evidence_export",
            Self::CheckpointGc => "checkpoint_gc",
            Self::Riblt => "riblt",
            Self::LegacyAdapter => "legacy_adapter",
        }
    }
}

/// Boolean switches kept as a stable, explicit configuration surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VNextFeatureFlags {
    pub object_event_v1: bool,
    pub obp_rp: bool,
    pub distributed_kql_one_hop: bool,
    pub public_use_evidence_publish: bool,
    pub distributed_pomv_view: bool,
    pub inventory_shadow: bool,
    pub provider_lease: bool,
    pub fidelity: bool,
    pub reward_evidence_export: bool,
    pub checkpoint_gc: bool,
    pub riblt: bool,
    pub legacy_adapter: bool,
}

impl VNextFeatureFlags {
    pub const fn is_set(self, feature: VNextFeature) -> bool {
        match feature {
            VNextFeature::ObjectEventV1 => self.object_event_v1,
            VNextFeature::ObpRp => self.obp_rp,
            VNextFeature::DistributedKqlOneHop => self.distributed_kql_one_hop,
            VNextFeature::PublicUseEvidencePublish => self.public_use_evidence_publish,
            VNextFeature::DistributedPomvView => self.distributed_pomv_view,
            VNextFeature::InventoryShadow => self.inventory_shadow,
            VNextFeature::ProviderLease => self.provider_lease,
            VNextFeature::Fidelity => self.fidelity,
            VNextFeature::RewardEvidenceExport => self.reward_evidence_export,
            VNextFeature::CheckpointGc => self.checkpoint_gc,
            VNextFeature::Riblt => self.riblt,
            VNextFeature::LegacyAdapter => self.legacy_adapter,
        }
    }
}

/// Requested features and emergency kill switches are intentionally separate.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VNextFeatureConfig {
    pub enabled: VNextFeatureFlags,
    pub kill_switches: VNextFeatureFlags,
    pub network: VNextNetworkPolicy,
    pub runtime_budgets: VNextRuntimeBudgets,
}

/// Hard product-runtime bounds. These are configuration policy, not caller
/// hints: typed service requests may narrow them but can never exceed them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VNextRuntimeBudgets {
    pub kql_max_scan_records: u64,
    pub kql_max_affordances: u64,
    pub kql_max_pairs: u64,
    pub kql_max_proposals: u64,
    pub pomv_max_records: usize,
    pub pomv_max_view_records: usize,
    pub publication_flush_batch: usize,
    pub worker_poll_interval_millis: u64,
    pub per_peer_work_units: u64,
    pub per_peer_bytes: u64,
    pub storage_soft_watermark_bytes: u64,
    pub storage_hard_watermark_bytes: u64,
}

impl Default for VNextRuntimeBudgets {
    fn default() -> Self {
        Self {
            kql_max_scan_records: 4_096,
            kql_max_affordances: 1_024,
            kql_max_pairs: 65_536,
            kql_max_proposals: 4_096,
            pomv_max_records: 4_096,
            pomv_max_view_records: 1_024,
            publication_flush_batch: 128,
            worker_poll_interval_millis: 1_000,
            per_peer_work_units: 1_000_000,
            per_peer_bytes: 4 * 1_048_576,
            storage_soft_watermark_bytes: 512 * 1_048_576,
            storage_hard_watermark_bytes: 1_024 * 1_048_576,
        }
    }
}

/// Bounded runtime policy for authenticated QUIC/reconciliation sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VNextNetworkPolicy {
    pub max_concurrent_handshakes: usize,
    pub max_handshakes_per_ip: usize,
    pub max_concurrent_sessions: usize,
    pub max_sessions_per_ip: usize,
    pub max_sessions_per_peer: usize,
    pub max_contexts_per_session: usize,
    pub max_replay_entries: usize,
    pub handshake_timeout_seconds: u64,
    pub rate_window_seconds: u64,
    pub max_records_per_session: u64,
    pub max_work_per_session: u64,
    pub max_records_per_peer_window: u64,
    pub max_bytes_per_peer_window: u64,
    pub max_work_per_peer_window: u64,
    pub max_retries_per_record: u64,
    pub max_inflight_bytes: u64,
}

impl Default for VNextNetworkPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_handshakes: 128,
            max_handshakes_per_ip: 8,
            max_concurrent_sessions: 64,
            max_sessions_per_ip: 8,
            max_sessions_per_peer: 4,
            max_contexts_per_session: 64,
            max_replay_entries: 65_536,
            handshake_timeout_seconds: 10,
            rate_window_seconds: 60,
            max_records_per_session: 4_096,
            max_work_per_session: 1_000_000,
            max_records_per_peer_window: 8_192,
            max_bytes_per_peer_window: 16 * 1_048_576,
            max_work_per_peer_window: 2_000_000,
            max_retries_per_record: 8,
            max_inflight_bytes: 4 * 1_048_576,
        }
    }
}

impl VNextFeatureConfig {
    pub const fn is_active(&self, feature: VNextFeature) -> bool {
        self.enabled.is_set(feature) && !self.kill_switches.is_set(feature)
    }

    #[cfg(feature = "vnext-outbound-first")]
    pub fn outbound_reachability_policy(
        &self,
    ) -> Result<crate::vnext_reachability_manager::VNextReachabilityPolicy, VNextFeatureConfigError>
    {
        let policy = crate::vnext_reachability_manager::VNextReachabilityPolicy::default();
        policy
            .validate()
            .map_err(|_| VNextFeatureConfigError::InvalidNetworkPolicy)?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), VNextFeatureConfigError> {
        self.network.validate()?;
        self.runtime_budgets.validate(&self.network)?;
        self.require(VNextFeature::ObpRp, VNextFeature::ObjectEventV1)?;
        for lane in [
            VNextFeature::DistributedKqlOneHop,
            VNextFeature::PublicUseEvidencePublish,
            VNextFeature::DistributedPomvView,
        ] {
            self.require(lane, VNextFeature::ObjectEventV1)?;
            self.require(lane, VNextFeature::ObpRp)?;
        }
        self.require(VNextFeature::InventoryShadow, VNextFeature::ObjectEventV1)?;
        self.require(VNextFeature::ProviderLease, VNextFeature::ObjectEventV1)?;
        self.require(VNextFeature::ProviderLease, VNextFeature::ObpRp)?;
        self.require(VNextFeature::Fidelity, VNextFeature::ObjectEventV1)?;
        self.require(
            VNextFeature::RewardEvidenceExport,
            VNextFeature::ObjectEventV1,
        )?;
        self.require(VNextFeature::CheckpointGc, VNextFeature::ObjectEventV1)?;
        self.require(VNextFeature::CheckpointGc, VNextFeature::ObpRp)?;
        self.require(VNextFeature::Riblt, VNextFeature::ObpRp)?;
        Ok(())
    }

    fn require(
        &self,
        feature: VNextFeature,
        dependency: VNextFeature,
    ) -> Result<(), VNextFeatureConfigError> {
        if self.is_active(feature) && !self.is_active(dependency) {
            Err(VNextFeatureConfigError::DependencyDisabled {
                feature,
                dependency,
            })
        } else {
            Ok(())
        }
    }
}

impl VNextRuntimeBudgets {
    pub fn validate(self, network: &VNextNetworkPolicy) -> Result<(), VNextFeatureConfigError> {
        let kql_work = self
            .kql_max_scan_records
            .checked_add(self.kql_max_pairs)
            .and_then(|value| value.checked_add(self.kql_max_proposals));
        if self.kql_max_scan_records == 0
            || self.kql_max_scan_records > 1_000_000
            || self.kql_max_affordances == 0
            || self.kql_max_affordances > 65_536
            || self.kql_max_affordances > self.kql_max_scan_records
            || self.kql_max_pairs == 0
            || self.kql_max_pairs > 1_000_000
            || self.kql_max_proposals == 0
            || self.kql_max_proposals > 65_536
            || self.pomv_max_records == 0
            || self.pomv_max_records > 65_536
            || self.pomv_max_view_records == 0
            || self.pomv_max_view_records > self.pomv_max_records
            || self.publication_flush_batch == 0
            || self.publication_flush_batch > 4_096
            || self.worker_poll_interval_millis < 10
            || self.worker_poll_interval_millis > 600_000
            || self.per_peer_work_units == 0
            || self.per_peer_work_units > 10_000_000
            || self.per_peer_bytes == 0
            || self.per_peer_bytes > 16 * 1_048_576
            || network.max_records_per_session > self.per_peer_work_units
            || network.max_inflight_bytes > self.per_peer_bytes
            || kql_work.is_none_or(|work| work > self.per_peer_work_units)
            || self.storage_soft_watermark_bytes == 0
            || self.storage_soft_watermark_bytes >= self.storage_hard_watermark_bytes
            || self.storage_hard_watermark_bytes > 1_099_511_627_776
        {
            Err(VNextFeatureConfigError::InvalidRuntimeBudgets)
        } else {
            Ok(())
        }
    }
}

impl VNextNetworkPolicy {
    pub fn validate(self) -> Result<(), VNextFeatureConfigError> {
        let minimum_pipeline_work = self.max_records_per_session.checked_mul(4);
        if self.max_concurrent_handshakes == 0
            || self.max_concurrent_handshakes > 8_192
            || self.max_handshakes_per_ip == 0
            || self.max_handshakes_per_ip > self.max_concurrent_handshakes
            || self.max_concurrent_sessions == 0
            || self.max_concurrent_sessions > 4_096
            || self.max_sessions_per_ip == 0
            || self.max_sessions_per_ip > self.max_concurrent_sessions
            || self.max_sessions_per_peer == 0
            || self.max_sessions_per_peer > self.max_concurrent_sessions
            || self.max_contexts_per_session == 0
            || self.max_contexts_per_session > 65_536
            || self.max_replay_entries == 0
            || self.max_replay_entries > 1_000_000
            || self.handshake_timeout_seconds == 0
            || self.handshake_timeout_seconds > 300
            || self.rate_window_seconds == 0
            || self.rate_window_seconds > 3_600
            // One reconciliation delivery needs at least one manifest record
            // and one payload record on the authenticated session.
            || self.max_records_per_session < 2
            || self.max_records_per_session > 1_000_000
            || minimum_pipeline_work
                .is_none_or(|minimum| self.max_work_per_session < minimum)
            || self.max_work_per_session > 100_000_000
            || self.max_records_per_peer_window < self.max_records_per_session
            || self.max_records_per_peer_window > 10_000_000
            || self.max_bytes_per_peer_window < self.max_inflight_bytes
            || self.max_bytes_per_peer_window > 1_073_741_824
            || self.max_work_per_peer_window < self.max_work_per_session
            || self.max_work_per_peer_window > 1_000_000_000
            || self.max_retries_per_record == 0
            || self.max_retries_per_record > 1_024
            || self.max_inflight_bytes == 0
            || self.max_inflight_bytes > 16 * 1_048_576
        {
            Err(VNextFeatureConfigError::InvalidNetworkPolicy)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum VNextFeatureConfigError {
    #[error(
        "vNext feature {feature_name} requires active dependency {dependency_name}",
        feature_name = .feature.name(),
        dependency_name = .dependency.name()
    )]
    DependencyDisabled {
        feature: VNextFeature,
        dependency: VNextFeature,
    },
    #[error("vNext network resource policy is outside the supported bounds")]
    InvalidNetworkPolicy,
    #[error("vNext product runtime budgets are outside the supported bounds")]
    InvalidRuntimeBudgets,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe_and_inactive() {
        let config = VNextFeatureConfig::default();
        for feature in [
            VNextFeature::ObjectEventV1,
            VNextFeature::ObpRp,
            VNextFeature::DistributedKqlOneHop,
            VNextFeature::PublicUseEvidencePublish,
            VNextFeature::DistributedPomvView,
            VNextFeature::InventoryShadow,
            VNextFeature::ProviderLease,
            VNextFeature::Fidelity,
            VNextFeature::RewardEvidenceExport,
            VNextFeature::CheckpointGc,
            VNextFeature::Riblt,
            VNextFeature::LegacyAdapter,
        ] {
            assert!(!config.is_active(feature));
        }
        assert!(config.validate().is_ok());
    }

    #[test]
    fn dependency_conflicts_fail_validation() {
        let mut config = VNextFeatureConfig::default();
        config.enabled.obp_rp = true;
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::DependencyDisabled {
                feature: VNextFeature::ObpRp,
                dependency: VNextFeature::ObjectEventV1,
            }
        );
    }

    #[test]
    fn kill_switches_are_independent_and_dependency_aware() {
        let mut config = VNextFeatureConfig::default();
        config.enabled.object_event_v1 = true;
        config.enabled.obp_rp = true;
        config.kill_switches.obp_rp = true;
        assert!(config.is_active(VNextFeature::ObjectEventV1));
        assert!(!config.is_active(VNextFeature::ObpRp));
        assert!(config.validate().is_ok());

        config.kill_switches.obp_rp = false;
        config.kill_switches.object_event_v1 = true;
        assert!(config.validate().is_err());
    }

    #[test]
    fn absent_serialized_fields_remain_disabled() {
        let config: VNextFeatureConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, VNextFeatureConfig::default());
        assert!(serde_json::from_str::<VNextFeatureConfig>(r#"{"unknown":true}"#).is_err());
    }

    #[test]
    fn one_record_session_cannot_carry_manifest_and_payload() {
        let mut config = VNextFeatureConfig::default();
        config.network.max_records_per_session = 1;
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::InvalidNetworkPolicy
        );
    }

    #[test]
    fn network_identity_and_rate_quotas_fail_closed_when_inconsistent() {
        let mut config = VNextFeatureConfig::default();
        config.network.max_sessions_per_ip =
            config.network.max_concurrent_sessions.saturating_add(1);
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::InvalidNetworkPolicy
        );

        let mut config = VNextFeatureConfig::default();
        config.network.max_bytes_per_peer_window =
            config.network.max_inflight_bytes.saturating_sub(1);
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::InvalidNetworkPolicy
        );

        let mut config = VNextFeatureConfig::default();
        config.network.max_replay_entries = 0;
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::InvalidNetworkPolicy
        );
    }

    #[test]
    fn product_lanes_have_independent_kill_switches_and_dependencies() {
        let mut config = VNextFeatureConfig::default();
        config.enabled.object_event_v1 = true;
        config.enabled.obp_rp = true;
        config.enabled.distributed_kql_one_hop = true;
        config.enabled.public_use_evidence_publish = true;
        config.enabled.distributed_pomv_view = true;
        config.kill_switches.public_use_evidence_publish = true;
        assert!(config.is_active(VNextFeature::DistributedKqlOneHop));
        assert!(!config.is_active(VNextFeature::PublicUseEvidencePublish));
        assert!(config.is_active(VNextFeature::DistributedPomvView));
        assert!(config.validate().is_ok());

        config.kill_switches.obp_rp = true;
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::DependencyDisabled {
                feature: VNextFeature::DistributedKqlOneHop,
                dependency: VNextFeature::ObpRp,
            }
        );
    }

    #[test]
    fn runtime_budgets_are_bounded_and_cross_checked() {
        let mut config = VNextFeatureConfig::default();
        config.runtime_budgets.kql_max_affordances =
            config.runtime_budgets.kql_max_scan_records + 1;
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::InvalidRuntimeBudgets
        );

        let mut config = VNextFeatureConfig::default();
        config.runtime_budgets.storage_soft_watermark_bytes =
            config.runtime_budgets.storage_hard_watermark_bytes;
        assert_eq!(
            config.validate().unwrap_err(),
            VNextFeatureConfigError::InvalidRuntimeBudgets
        );
    }
}
