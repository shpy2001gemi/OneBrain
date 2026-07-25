//! Safe-default runtime gates for independently deployable vNext capabilities.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VNextFeature {
    ObjectEventV1,
    ObpRp,
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
}

/// Bounded runtime policy for authenticated QUIC/reconciliation sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VNextNetworkPolicy {
    pub max_concurrent_sessions: usize,
    pub handshake_timeout_seconds: u64,
    pub max_records_per_session: u64,
    pub max_retries_per_record: u64,
    pub max_inflight_bytes: u64,
}

impl Default for VNextNetworkPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 64,
            handshake_timeout_seconds: 10,
            max_records_per_session: 4_096,
            max_retries_per_record: 8,
            max_inflight_bytes: 4 * 1_048_576,
        }
    }
}

impl VNextFeatureConfig {
    pub const fn is_active(&self, feature: VNextFeature) -> bool {
        self.enabled.is_set(feature) && !self.kill_switches.is_set(feature)
    }

    pub fn validate(&self) -> Result<(), VNextFeatureConfigError> {
        self.network.validate()?;
        self.require(VNextFeature::ObpRp, VNextFeature::ObjectEventV1)?;
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

impl VNextNetworkPolicy {
    pub fn validate(self) -> Result<(), VNextFeatureConfigError> {
        if self.max_concurrent_sessions == 0
            || self.max_concurrent_sessions > 4_096
            || self.handshake_timeout_seconds == 0
            || self.handshake_timeout_seconds > 300
            // One reconciliation delivery needs at least one manifest record
            // and one payload record on the authenticated session.
            || self.max_records_per_session < 2
            || self.max_records_per_session > 1_000_000
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
}
