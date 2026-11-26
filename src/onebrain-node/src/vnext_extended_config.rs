//! Default-off extended Pillar feature flags and independent kill switches (`P1`..`P10`).
//!
//! Keeps `VNextFeature` (`vnext_config.rs`) at its frozen 12-variant compatibility set
//! and `VNextRuntimeLane` (`vnext_runtime_rollout.rs`) at its frozen 4-lane set while
//! providing typed, independently kill-switchable rollout controls for Wave 1–6 surfaces.

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalErrorKind, CanonicalValue,
    ResourceProfile,
};
use serde::{Deserialize, Serialize};

pub const VNEXT_EXTENDED_CONFIG_SCHEMA_V1: u64 = 1;

/// Extended vNext runtime capabilities introduced across Pillars `P1`–`P10`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VNextExtendedFeature {
    RemoteCognitionRequester,
    RemoteCognitionExecutor,
    ObtLedgerV1,
    ObkgFedrV1,
    PersonalMeshV1,
    DreamConsolidatorV1,
    SemanticSelectionV2Host,
}

impl VNextExtendedFeature {
    pub const ALL: [Self; 7] = [
        Self::RemoteCognitionRequester,
        Self::RemoteCognitionExecutor,
        Self::ObtLedgerV1,
        Self::ObkgFedrV1,
        Self::PersonalMeshV1,
        Self::DreamConsolidatorV1,
        Self::SemanticSelectionV2Host,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::RemoteCognitionRequester => "remote_cognition_requester",
            Self::RemoteCognitionExecutor => "remote_cognition_executor",
            Self::ObtLedgerV1 => "obt_ledger_v1",
            Self::ObkgFedrV1 => "obkg_fedr_v1",
            Self::PersonalMeshV1 => "personal_mesh_v1",
            Self::DreamConsolidatorV1 => "dream_consolidator_v1",
            Self::SemanticSelectionV2Host => "semantic_selection_v2_host",
        }
    }
}

/// Boolean matrix for extended Pillar capabilities; all `false` by default (`FND-006`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VNextExtendedFeatureFlags {
    pub remote_cognition_requester: bool,
    pub remote_cognition_executor: bool,
    pub obt_ledger_v1: bool,
    pub obkg_fedr_v1: bool,
    pub personal_mesh_v1: bool,
    pub dream_consolidator_v1: bool,
    pub semantic_selection_v2_host: bool,
}

impl VNextExtendedFeatureFlags {
    pub const fn get(self, feature: VNextExtendedFeature) -> bool {
        match feature {
            VNextExtendedFeature::RemoteCognitionRequester => self.remote_cognition_requester,
            VNextExtendedFeature::RemoteCognitionExecutor => self.remote_cognition_executor,
            VNextExtendedFeature::ObtLedgerV1 => self.obt_ledger_v1,
            VNextExtendedFeature::ObkgFedrV1 => self.obkg_fedr_v1,
            VNextExtendedFeature::PersonalMeshV1 => self.personal_mesh_v1,
            VNextExtendedFeature::DreamConsolidatorV1 => self.dream_consolidator_v1,
            VNextExtendedFeature::SemanticSelectionV2Host => self.semantic_selection_v2_host,
        }
    }

    pub fn set(&mut self, feature: VNextExtendedFeature, value: bool) {
        match feature {
            VNextExtendedFeature::RemoteCognitionRequester => {
                self.remote_cognition_requester = value;
            }
            VNextExtendedFeature::RemoteCognitionExecutor => {
                self.remote_cognition_executor = value;
            }
            VNextExtendedFeature::ObtLedgerV1 => self.obt_ledger_v1 = value,
            VNextExtendedFeature::ObkgFedrV1 => self.obkg_fedr_v1 = value,
            VNextExtendedFeature::PersonalMeshV1 => self.personal_mesh_v1 = value,
            VNextExtendedFeature::DreamConsolidatorV1 => self.dream_consolidator_v1 = value,
            VNextExtendedFeature::SemanticSelectionV2Host => {
                self.semantic_selection_v2_host = value;
            }
        }
    }

    pub const fn union(self, other: Self) -> Self {
        Self {
            remote_cognition_requester: self.remote_cognition_requester
                || other.remote_cognition_requester,
            remote_cognition_executor: self.remote_cognition_executor
                || other.remote_cognition_executor,
            obt_ledger_v1: self.obt_ledger_v1 || other.obt_ledger_v1,
            obkg_fedr_v1: self.obkg_fedr_v1 || other.obkg_fedr_v1,
            personal_mesh_v1: self.personal_mesh_v1 || other.personal_mesh_v1,
            dream_consolidator_v1: self.dream_consolidator_v1 || other.dream_consolidator_v1,
            semantic_selection_v2_host: self.semantic_selection_v2_host
                || other.semantic_selection_v2_host,
        }
    }

    pub const fn to_bitmask(self) -> u64 {
        (self.remote_cognition_requester as u64)
            | ((self.remote_cognition_executor as u64) << 1)
            | ((self.obt_ledger_v1 as u64) << 2)
            | ((self.obkg_fedr_v1 as u64) << 3)
            | ((self.personal_mesh_v1 as u64) << 4)
            | ((self.dream_consolidator_v1 as u64) << 5)
            | ((self.semantic_selection_v2_host as u64) << 6)
    }

    pub const fn from_bitmask(mask: u64) -> Option<Self> {
        if mask & !0x7f != 0 {
            return None;
        }
        Some(Self {
            remote_cognition_requester: (mask & (1 << 0)) != 0,
            remote_cognition_executor: (mask & (1 << 1)) != 0,
            obt_ledger_v1: (mask & (1 << 2)) != 0,
            obkg_fedr_v1: (mask & (1 << 3)) != 0,
            personal_mesh_v1: (mask & (1 << 4)) != 0,
            dream_consolidator_v1: (mask & (1 << 5)) != 0,
            semantic_selection_v2_host: (mask & (1 << 6)) != 0,
        })
    }
}

/// Host rollout configuration for extended Pillar surfaces with independent kill switches
/// and conservative `0x000E` (`ArchiveOwner::ROLLOUT`) restore semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VNextExtendedFeatureConfig {
    pub enabled: VNextExtendedFeatureFlags,
    pub kill_switches: VNextExtendedFeatureFlags,
    pub version_fence: u64,
}

impl Default for VNextExtendedFeatureConfig {
    fn default() -> Self {
        Self {
            enabled: VNextExtendedFeatureFlags::default(),
            kill_switches: VNextExtendedFeatureFlags::default(),
            version_fence: VNEXT_EXTENDED_CONFIG_SCHEMA_V1,
        }
    }
}

impl VNextExtendedFeatureConfig {
    /// Effective activation requires explicit enablement AND absence of the independent kill switch.
    pub const fn is_active(&self, feature: VNextExtendedFeature) -> bool {
        self.enabled.get(feature) && !self.kill_switches.get(feature)
    }

    pub fn enable(&mut self, feature: VNextExtendedFeature) {
        self.enabled.set(feature, true);
    }

    pub fn kill(&mut self, feature: VNextExtendedFeature) {
        self.kill_switches.set(feature, true);
    }

    pub fn clear_kill_switch(&mut self, feature: VNextExtendedFeature) {
        self.kill_switches.set(feature, false);
    }

    pub const fn effective_flags(&self) -> VNextExtendedFeatureFlags {
        VNextExtendedFeatureFlags {
            remote_cognition_requester: self
                .is_active(VNextExtendedFeature::RemoteCognitionRequester),
            remote_cognition_executor: self
                .is_active(VNextExtendedFeature::RemoteCognitionExecutor),
            obt_ledger_v1: self.is_active(VNextExtendedFeature::ObtLedgerV1),
            obkg_fedr_v1: self.is_active(VNextExtendedFeature::ObkgFedrV1),
            personal_mesh_v1: self.is_active(VNextExtendedFeature::PersonalMeshV1),
            dream_consolidator_v1: self.is_active(VNextExtendedFeature::DreamConsolidatorV1),
            semantic_selection_v2_host: self
                .is_active(VNextExtendedFeature::SemanticSelectionV2Host),
        }
    }

    /// Conservative `0x000E` restore merge: takes the union of local and archive kill switches
    /// and the maximum of version fences so restoring an older backup never clears a local
    /// emergency kill switch or downgrades a schema fence.
    pub fn merge_conservative_restore(&self, archive_config: &Self) -> Self {
        let merged_kills = self.kill_switches.union(archive_config.kill_switches);
        let mut merged_enabled = archive_config.enabled;
        for feature in VNextExtendedFeature::ALL {
            if merged_kills.get(feature) {
                merged_enabled.set(feature, false);
            }
        }
        Self {
            enabled: merged_enabled,
            kill_switches: merged_kills,
            version_fence: self.version_fence.max(archive_config.version_fence).max(1),
        }
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(VNEXT_EXTENDED_CONFIG_SCHEMA_V1)),
            (1, CanonicalValue::Unsigned(self.enabled.to_bitmask())),
            (2, CanonicalValue::Unsigned(self.kill_switches.to_bitmask())),
            (3, CanonicalValue::Unsigned(self.version_fence.max(1))),
        ]);
        encode_canonical(&value, ResourceProfile::ControlV1)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, CanonicalError> {
        let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
        let CanonicalValue::Map(entries) = value else {
            return Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0));
        };
        if entries.len() != 4
            || entries
                .iter()
                .enumerate()
                .any(|(idx, (key, _))| *key != idx as u64)
        {
            return Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0));
        }
        let u64_at = |idx: usize| -> Result<u64, CanonicalError> {
            match entries[idx].1 {
                CanonicalValue::Unsigned(v) => Ok(v),
                _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
            }
        };
        if u64_at(0)? != VNEXT_EXTENDED_CONFIG_SCHEMA_V1 {
            return Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0));
        }
        let enabled = VNextExtendedFeatureFlags::from_bitmask(u64_at(1)?)
            .ok_or_else(|| CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0))?;
        let kill_switches = VNextExtendedFeatureFlags::from_bitmask(u64_at(2)?)
            .ok_or_else(|| CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0))?;
        let version_fence = u64_at(3)?;
        if version_fence == 0 {
            return Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0));
        }
        Ok(Self {
            enabled,
            kill_switches,
            version_fence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_extended_flags_default_off_with_independent_kill_switches() {
        let mut cfg = VNextExtendedFeatureConfig::default();
        for feature in VNextExtendedFeature::ALL {
            assert!(
                !cfg.is_active(feature),
                "{} must default off",
                feature.name()
            );
        }
        cfg.enable(VNextExtendedFeature::ObtLedgerV1);
        cfg.enable(VNextExtendedFeature::RemoteCognitionRequester);
        assert!(cfg.is_active(VNextExtendedFeature::ObtLedgerV1));
        assert!(cfg.is_active(VNextExtendedFeature::RemoteCognitionRequester));
        assert!(!cfg.is_active(VNextExtendedFeature::RemoteCognitionExecutor));

        cfg.kill(VNextExtendedFeature::ObtLedgerV1);
        assert!(!cfg.is_active(VNextExtendedFeature::ObtLedgerV1));
        assert!(cfg.is_active(VNextExtendedFeature::RemoteCognitionRequester));

        let bytes = cfg.encode_canonical_bytes().unwrap();
        let decoded = VNextExtendedFeatureConfig::decode_canonical_bytes(&bytes).unwrap();
        assert_eq!(decoded, cfg);
    }

    #[test]
    fn conservative_restore_preserves_local_kill_switches_and_max_fence() {
        let mut local = VNextExtendedFeatureConfig {
            version_fence: 5,
            ..Default::default()
        };
        local.kill(VNextExtendedFeature::RemoteCognitionExecutor);

        let mut archive = VNextExtendedFeatureConfig {
            version_fence: 2,
            ..Default::default()
        };
        archive.enable(VNextExtendedFeature::RemoteCognitionExecutor);
        archive.enable(VNextExtendedFeature::PersonalMeshV1);
        archive.kill(VNextExtendedFeature::ObkgFedrV1);

        let merged = local.merge_conservative_restore(&archive);
        assert_eq!(merged.version_fence, 5);
        assert!(!merged.is_active(VNextExtendedFeature::RemoteCognitionExecutor));
        assert!(merged.kill_switches.remote_cognition_executor);
        assert!(merged.kill_switches.obkg_fedr_v1);
        assert!(merged.is_active(VNextExtendedFeature::PersonalMeshV1));
    }
}
