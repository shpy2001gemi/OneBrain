use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{MobileCoreError, MobileFeatureFlags, ResourceBudgets};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePressureKind {
    None,
    LowPowerMode,
    ThermalSerious,
    ThermalCritical,
    MemoryPressureModerate,
    MemoryPressureCritical,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnergyPolicyEvaluation {
    pub idle_polling_timers: u32,
    pub background_grants_drained: usize,
    pub foreground_retained: bool,
    pub resulting_phase: ActivationPhase,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationPhase {
    Dormant,
    Starting,
    Active,
    Draining,
}

impl ActivationPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dormant => "Dormant",
            Self::Starting => "Starting",
            Self::Active => "Active",
            Self::Draining => "Draining",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionGrantKind {
    Foreground,
    BackgroundRefresh,
    BackgroundTransferCallback,
    UserVisibleBackground,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkScope {
    None,
    RegistryOnly,
    PeerTransport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutionGrant {
    pub grant_id: String,
    pub process_generation: u64,
    pub kind: ExecutionGrantKind,
    pub user_visible: bool,
    pub deadline_monotonic_ms: Option<u64>,
    pub network_scope: NetworkScope,
}

pub struct ActivationArbiter {
    process_generation: u64,
    phase: ActivationPhase,
    grants: BTreeMap<String, ExecutionGrant>,
    max_active_grants: usize,
}

impl ActivationArbiter {
    pub fn starting(process_generation: u64, budgets: &ResourceBudgets) -> Self {
        Self {
            process_generation,
            phase: ActivationPhase::Starting,
            grants: BTreeMap::new(),
            max_active_grants: budgets.max_active_grants,
        }
    }

    pub fn process_generation(&self) -> u64 {
        self.process_generation
    }

    pub fn phase(&self) -> ActivationPhase {
        self.phase
    }

    pub fn active_grant_count(&self) -> usize {
        self.grants.len()
    }

    pub fn register_grant(
        &mut self,
        grant: ExecutionGrant,
        now_monotonic_ms: u64,
    ) -> Result<(), MobileCoreError> {
        if grant.process_generation != self.process_generation {
            return Err(MobileCoreError::StaleGeneration {
                received: grant.process_generation,
                current: self.process_generation,
            });
        }
        if grant.grant_id.is_empty() || grant.grant_id.len() > 128 {
            return Err(MobileCoreError::InvalidArgument(
                "grant_id must contain between 1 and 128 UTF-8 bytes".into(),
            ));
        }
        if grant.network_scope != NetworkScope::None {
            return Err(MobileCoreError::InvalidArgument(
                "MOB-02 grants cannot authorize network access".into(),
            ));
        }
        if grant
            .deadline_monotonic_ms
            .is_some_and(|deadline| deadline <= now_monotonic_ms)
        {
            return Err(MobileCoreError::InvalidArgument(
                "execution grant deadline has already expired".into(),
            ));
        }
        if !self.grants.contains_key(&grant.grant_id) && self.grants.len() >= self.max_active_grants
        {
            return Err(MobileCoreError::BudgetExceeded(format!(
                "at most {} execution grants may be active",
                self.max_active_grants
            )));
        }
        self.grants.insert(grant.grant_id.clone(), grant);
        self.phase = ActivationPhase::Active;
        Ok(())
    }

    pub fn revoke_grant(&mut self, grant_id: &str) -> bool {
        let removed = self.grants.remove(grant_id).is_some();
        if self.grants.is_empty() && self.phase != ActivationPhase::Dormant {
            self.phase = ActivationPhase::Draining;
        }
        removed
    }

    pub fn expire_deadlines(&mut self, now_monotonic_ms: u64) -> usize {
        let before = self.grants.len();
        self.grants.retain(|_, grant| {
            grant
                .deadline_monotonic_ms
                .is_none_or(|deadline| deadline > now_monotonic_ms)
        });
        if self.grants.is_empty() && self.phase != ActivationPhase::Dormant {
            self.phase = ActivationPhase::Draining;
        }
        before - self.grants.len()
    }

    pub fn register_grant_with_flags(
        &mut self,
        grant: ExecutionGrant,
        flags: &MobileFeatureFlags,
        now_monotonic_ms: u64,
    ) -> Result<(), MobileCoreError> {
        if grant.process_generation != self.process_generation {
            return Err(MobileCoreError::StaleGeneration {
                received: grant.process_generation,
                current: self.process_generation,
            });
        }
        if grant.grant_id.is_empty() || grant.grant_id.len() > 128 {
            return Err(MobileCoreError::InvalidArgument(
                "grant_id must contain between 1 and 128 UTF-8 bytes".into(),
            ));
        }
        match grant.network_scope {
            NetworkScope::None => {}
            NetworkScope::RegistryOnly => {
                if !flags.registry_network {
                    return Err(MobileCoreError::InvalidArgument(
                        "RegistryOnly grant requires registry_network feature flag".into(),
                    ));
                }
            }
            NetworkScope::PeerTransport => {
                if !flags.peer_transport {
                    return Err(MobileCoreError::FeatureGateDisabled(
                        "PeerTransport grant requires peer_transport feature flag".into(),
                    ));
                }
            }
        }
        if grant
            .deadline_monotonic_ms
            .is_some_and(|deadline| deadline <= now_monotonic_ms)
        {
            return Err(MobileCoreError::InvalidArgument(
                "execution grant deadline has already expired".into(),
            ));
        }
        if !self.grants.contains_key(&grant.grant_id) && self.grants.len() >= self.max_active_grants
        {
            return Err(MobileCoreError::BudgetExceeded(format!(
                "at most {} execution grants may be active",
                self.max_active_grants
            )));
        }
        self.grants.insert(grant.grant_id.clone(), grant);
        self.phase = ActivationPhase::Active;
        Ok(())
    }

    pub fn apply_lifecycle_pressure(
        &mut self,
        pressure: LifecyclePressureKind,
    ) -> EnergyPolicyEvaluation {
        let before = self.grants.len();
        match pressure {
            LifecyclePressureKind::None => {}
            LifecyclePressureKind::LowPowerMode | LifecyclePressureKind::MemoryPressureModerate => {
                self.grants.retain(|_, grant| {
                    matches!(
                        grant.kind,
                        ExecutionGrantKind::Foreground | ExecutionGrantKind::UserVisibleBackground
                    )
                });
            }
            LifecyclePressureKind::ThermalSerious
            | LifecyclePressureKind::ThermalCritical
            | LifecyclePressureKind::MemoryPressureCritical => {
                self.grants
                    .retain(|_, grant| grant.kind == ExecutionGrantKind::Foreground);
            }
        }
        let drained = before.saturating_sub(self.grants.len());
        if self.grants.is_empty() && self.phase != ActivationPhase::Dormant {
            self.phase = ActivationPhase::Draining;
        }
        let foreground_retained = self
            .grants
            .values()
            .any(|grant| grant.kind == ExecutionGrantKind::Foreground);
        EnergyPolicyEvaluation {
            idle_polling_timers: 0,
            background_grants_drained: drained,
            foreground_retained,
            resulting_phase: self.phase,
            reason: match pressure {
                LifecyclePressureKind::None => "NOMINAL_ZERO_POLLING",
                LifecyclePressureKind::LowPowerMode => "LOW_POWER_NON_VISIBLE_DRAINED",
                LifecyclePressureKind::MemoryPressureModerate => {
                    "MEMORY_MODERATE_BACKGROUND_DRAINED"
                }
                LifecyclePressureKind::ThermalSerious => "THERMAL_SERIOUS_BACKGROUND_SUSPENDED",
                LifecyclePressureKind::ThermalCritical => "THERMAL_CRITICAL_FOREGROUND_ONLY",
                LifecyclePressureKind::MemoryPressureCritical => "MEMORY_CRITICAL_FOREGROUND_ONLY",
            },
        }
    }

    pub fn mark_dormant(&mut self) {
        self.grants.clear();
        self.phase = ActivationPhase::Dormant;
    }
}
