use serde::{Deserialize, Serialize};

use crate::{
    BootstrapStore, MobileCoreError, RegistryCapacityPlan, RegistryChunkWriteStart,
    RegistryLandingProgress, RegistryNetworkPolicy, RegistrySourceKind,
};

const SOURCE_PLAN_DOMAIN: &[u8] = b"onebrain:registry-source-plan:1\0";
const MAX_PROVIDER_BLOCK_BYTES: usize = 256 * 1024;
const MAX_CONSECUTIVE_FAILURES_BEFORE_QUARANTINE: u32 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistrySourceDescriptor {
    pub descriptor_id: String,
    pub source_kind: RegistrySourceKind,
    pub priority_rank: u16,
    pub transport_label: String,
    pub supports_byte_ranges: bool,
    pub requires_unmetered: bool,
}

impl RegistrySourceDescriptor {
    pub fn validate(&self) -> Result<(), MobileCoreError> {
        if self.descriptor_id.is_empty() || self.descriptor_id.len() > 96 {
            return Err(MobileCoreError::InvalidArgument(
                "descriptor_id must contain 1..=96 UTF-8 bytes".into(),
            ));
        }
        if self.transport_label.is_empty() || self.transport_label.len() > 128 {
            return Err(MobileCoreError::InvalidArgument(
                "transport_label must contain 1..=128 UTF-8 bytes".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistrySourcePlan {
    pub operation_id: String,
    pub release_id: String,
    pub manifest_digest: String,
    pub confirmed_network_policy: RegistryNetworkPolicy,
    pub one_time_network_override: bool,
    pub descriptors: Vec<RegistrySourceDescriptor>,
}

impl RegistrySourcePlan {
    pub fn canonical(
        operation_id: impl Into<String>,
        release_id: impl Into<String>,
        manifest_digest: impl Into<String>,
        confirmed_network_policy: RegistryNetworkPolicy,
        one_time_network_override: bool,
        mut descriptors: Vec<RegistrySourceDescriptor>,
    ) -> Result<Self, MobileCoreError> {
        let operation_id = operation_id.into();
        let release_id = release_id.into();
        let manifest_digest = manifest_digest.into();
        if operation_id.is_empty() || release_id.len() != 64 || manifest_digest.len() != 64 {
            return Err(MobileCoreError::InvalidArgument(
                "RegistrySourcePlan requires non-empty operation_id and 32-byte hex digests".into(),
            ));
        }
        if descriptors.is_empty() || descriptors.len() > 16 {
            return Err(MobileCoreError::InvalidArgument(
                "RegistrySourcePlan requires 1..=16 source descriptors".into(),
            ));
        }
        for descriptor in &descriptors {
            descriptor.validate()?;
        }
        descriptors.sort_by(|left, right| {
            left.priority_rank
                .cmp(&right.priority_rank)
                .then_with(|| left.descriptor_id.cmp(&right.descriptor_id))
        });
        for window in descriptors.windows(2) {
            if window[0].descriptor_id == window[1].descriptor_id {
                return Err(MobileCoreError::InvalidArgument(
                    "RegistrySourcePlan cannot contain duplicate descriptor_id entries".into(),
                ));
            }
        }
        Ok(Self {
            operation_id,
            release_id,
            manifest_digest,
            confirmed_network_policy,
            one_time_network_override,
            descriptors,
        })
    }

    pub fn source_plan_digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SOURCE_PLAN_DOMAIN);
        for field in [
            self.operation_id.as_bytes(),
            self.release_id.as_bytes(),
            self.manifest_digest.as_bytes(),
            self.confirmed_network_policy.as_str().as_bytes(),
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field);
        }
        hasher.update(&[u8::from(self.one_time_network_override)]);
        hasher.update(&(self.descriptors.len() as u64).to_le_bytes());
        for descriptor in &self.descriptors {
            hasher.update(&(descriptor.descriptor_id.len() as u64).to_le_bytes());
            hasher.update(descriptor.descriptor_id.as_bytes());
            hasher.update(&(descriptor.source_kind.as_str().len() as u64).to_le_bytes());
            hasher.update(descriptor.source_kind.as_str().as_bytes());
            hasher.update(&descriptor.priority_rank.to_le_bytes());
            hasher.update(&(descriptor.transport_label.len() as u64).to_le_bytes());
            hasher.update(descriptor.transport_label.as_bytes());
            hasher.update(&[
                u8::from(descriptor.supports_byte_ranges),
                u8::from(descriptor.requires_unmetered),
            ]);
        }
        hex::encode(hasher.finalize().as_bytes())
    }

    pub fn request_fingerprint(&self, expected_total_bytes: u64) -> String {
        let digest = self.source_plan_digest();
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:registry-multi-source-request:1\0");
        hasher.update(&(digest.len() as u64).to_le_bytes());
        hasher.update(digest.as_bytes());
        hasher.update(&expected_total_bytes.to_le_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistrySourceObservation {
    pub descriptor_id: String,
    pub consecutive_failures: u32,
    pub corrupt_chunk_strikes: u32,
    pub quarantined: bool,
    pub backoff_until_ms: u64,
    pub last_error_code: Option<String>,
    pub verified_bytes_contributed: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThermalLevel {
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedNetworkClass {
    Offline,
    WifiUnmetered,
    WifiMetered,
    CellularMetered,
    CellularRoaming,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceSnapshot {
    pub battery_percent: u8,
    pub is_charging: bool,
    pub thermal_state: ThermalLevel,
    pub network_class: ObservedNetworkClass,
    pub low_power_mode: bool,
    pub data_saver_enabled: bool,
    pub measured_free_bytes: u64,
    pub foreground_user_initiated: bool,
    pub now_monotonic_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryRemainingCapacityPlan {
    pub remaining_publisher_min_bytes: u64,
    pub remaining_target_alloc_bytes: u64,
    pub remaining_transfer_bytes: u64,
    pub verification_workspace_bytes: u64,
    pub catalog_growth_bytes: u64,
    pub safety_reserve_bytes: u64,
    pub remaining_required_free_bytes: u64,
    pub measured_free_bytes: u64,
    pub admitted: bool,
}

impl RegistryRemainingCapacityPlan {
    pub fn compute(
        initial: &RegistryCapacityPlan,
        verified_progress_bytes: u64,
        materialized_alloc_bytes: u64,
        measured_free_bytes: u64,
    ) -> Self {
        let p_remaining = initial
            .publisher_min_additional_free_bytes
            .saturating_sub(verified_progress_bytes);
        let n_remaining_alloc = initial
            .target_total_alloc_bytes
            .saturating_sub(materialized_alloc_bytes);
        let t_remaining = initial
            .transfer_initial_bytes
            .saturating_sub(verified_progress_bytes);
        let structural_sum = n_remaining_alloc
            .saturating_add(t_remaining)
            .saturating_add(initial.verification_workspace_bytes)
            .saturating_add(initial.catalog_growth_bytes)
            .saturating_add(initial.safety_reserve_bytes);
        let remaining_required_free_bytes = p_remaining.max(structural_sum);
        Self {
            remaining_publisher_min_bytes: p_remaining,
            remaining_target_alloc_bytes: n_remaining_alloc,
            remaining_transfer_bytes: t_remaining,
            verification_workspace_bytes: initial.verification_workspace_bytes,
            catalog_growth_bytes: initial.catalog_growth_bytes,
            safety_reserve_bytes: initial.safety_reserve_bytes,
            remaining_required_free_bytes,
            measured_free_bytes,
            admitted: measured_free_bytes >= remaining_required_free_bytes,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResourcePolicyDecision {
    Proceed,
    WaitingNetwork {
        reason: String,
    },
    WaitingPower {
        reason: String,
    },
    WaitingThermal {
        reason: String,
    },
    WaitingStorage {
        remaining_required_free_bytes: u64,
        measured_free_bytes: u64,
    },
    RequiresExplicitConfirmation {
        reason: String,
    },
}

pub fn evaluate_registry_resource_policy(
    plan: &RegistrySourcePlan,
    source_kind: RegistrySourceKind,
    remaining_capacity: &RegistryRemainingCapacityPlan,
    snapshot: &ResourceSnapshot,
) -> ResourcePolicyDecision {
    if !remaining_capacity.admitted {
        return ResourcePolicyDecision::WaitingStorage {
            remaining_required_free_bytes: remaining_capacity.remaining_required_free_bytes,
            measured_free_bytes: remaining_capacity.measured_free_bytes,
        };
    }
    if matches!(
        snapshot.thermal_state,
        ThermalLevel::Serious | ThermalLevel::Critical
    ) {
        return ResourcePolicyDecision::WaitingThermal {
            reason: "device thermal state requires cooling before bulk Registry I/O".into(),
        };
    }
    if !snapshot.is_charging && snapshot.battery_percent < 15 && !snapshot.foreground_user_initiated
    {
        return ResourcePolicyDecision::WaitingPower {
            reason: "background Registry transfer pauses below 15% battery when unplugged".into(),
        };
    }
    if snapshot.low_power_mode && !snapshot.foreground_user_initiated {
        return ResourcePolicyDecision::WaitingPower {
            reason: "Low Power Mode defers background Registry transfer".into(),
        };
    }
    if source_kind == RegistrySourceKind::LocalImport {
        return ResourcePolicyDecision::Proceed;
    }
    match snapshot.network_class {
        ObservedNetworkClass::Offline => ResourcePolicyDecision::WaitingNetwork {
            reason: "no active network route is available".into(),
        },
        ObservedNetworkClass::CellularRoaming => {
            if plan.confirmed_network_policy == RegistryNetworkPolicy::AnyNetwork
                && plan.one_time_network_override
                && !snapshot.data_saver_enabled
            {
                ResourcePolicyDecision::Proceed
            } else {
                ResourcePolicyDecision::RequiresExplicitConfirmation {
                    reason:
                        "roaming network requires explicit AnyNetwork override without Data Saver"
                            .into(),
                }
            }
        }
        ObservedNetworkClass::CellularMetered | ObservedNetworkClass::WifiMetered => {
            if snapshot.data_saver_enabled && !snapshot.foreground_user_initiated {
                return ResourcePolicyDecision::WaitingNetwork {
                    reason: "system Data Saver blocks background metered transfer".into(),
                };
            }
            match plan.confirmed_network_policy {
                RegistryNetworkPolicy::AnyNetwork if plan.one_time_network_override => {
                    ResourcePolicyDecision::Proceed
                }
                RegistryNetworkPolicy::AnyNetwork => {
                    ResourcePolicyDecision::RequiresExplicitConfirmation {
                        reason: "metered network requires one-time override confirmation".into(),
                    }
                }
                RegistryNetworkPolicy::WifiOnly | RegistryNetworkPolicy::Unmetered => {
                    ResourcePolicyDecision::WaitingNetwork {
                        reason: "confirmed Registry policy requires an unmetered Wi-Fi route"
                            .into(),
                    }
                }
            }
        }
        ObservedNetworkClass::WifiUnmetered => ResourcePolicyDecision::Proceed,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryProviderFault {
    TransientIo(String),
    RangeUnavailable,
    UnsupportedRange,
    StorageFull,
    PolicyBlocked(String),
    CorruptUpstream,
}

pub trait RegistryByteProvider {
    fn descriptor_id(&self) -> &str;
    fn source_kind(&self) -> RegistrySourceKind;
    fn read_range(
        &mut self,
        artifact_role: u8,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, RegistryProviderFault>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MultiProviderTransferOutcome {
    pub transfer_nonce: String,
    pub progress: RegistryLandingProgress,
    pub decision: ResourcePolicyDecision,
    pub active_descriptor_id: Option<String>,
    pub failover_count: u32,
    pub quarantined_descriptors: Vec<String>,
}

pub fn execute_multi_provider_registry_transfer(
    store: &BootstrapStore,
    transfer_nonce: &str,
    plan: &RegistrySourcePlan,
    initial_capacity: &RegistryCapacityPlan,
    snapshot: &ResourceSnapshot,
    providers: &mut [&mut dyn RegistryByteProvider],
) -> Result<MultiProviderTransferOutcome, MobileCoreError> {
    let mut progress = store.recover_registry_chunk_ledger(transfer_nonce)?;
    let remaining_capacity = RegistryRemainingCapacityPlan::compute(
        initial_capacity,
        progress.verified_bytes,
        0,
        snapshot.measured_free_bytes,
    );
    if !remaining_capacity.admitted {
        store.mark_registry_operation_waiting_storage(&plan.operation_id)?;
        return Ok(MultiProviderTransferOutcome {
            transfer_nonce: transfer_nonce.to_owned(),
            progress,
            decision: ResourcePolicyDecision::WaitingStorage {
                remaining_required_free_bytes: remaining_capacity.remaining_required_free_bytes,
                measured_free_bytes: remaining_capacity.measured_free_bytes,
            },
            active_descriptor_id: None,
            failover_count: 0,
            quarantined_descriptors: store.quarantined_source_descriptors(&plan.operation_id)?,
        });
    }

    for provider in providers.iter() {
        let confirmed = plan
            .descriptors
            .iter()
            .find(|item| item.descriptor_id == provider.descriptor_id());
        let Some(confirmed) = confirmed else {
            store.return_registry_operation_to_awaiting_confirm(&plan.operation_id)?;
            return Err(MobileCoreError::RegistryAdmission(format!(
                "provider descriptor '{}' is not in the confirmed RegistrySourcePlan",
                provider.descriptor_id()
            )));
        };
        if confirmed.source_kind != provider.source_kind() {
            store.return_registry_operation_to_awaiting_confirm(&plan.operation_id)?;
            return Err(MobileCoreError::RegistryAdmission(
                "provider source_kind diverged from the confirmed RegistrySourcePlan".into(),
            ));
        }
    }

    let mut failover_count = 0u32;
    let mut last_active_descriptor: Option<String> = None;
    let mut last_blocking_decision: Option<ResourcePolicyDecision> = None;

    for role in [0u8, 1u8, 2u8] {
        while let Some(target) = store.next_registry_chunk_source_target(transfer_nonce, role)? {
            let mut chunk_completed = false;
            for descriptor in &plan.descriptors {
                let mut obs = store
                    .registry_source_observation(&plan.operation_id, &descriptor.descriptor_id)?
                    .unwrap_or_else(|| RegistrySourceObservation {
                        descriptor_id: descriptor.descriptor_id.clone(),
                        ..Default::default()
                    });
                if obs.quarantined || obs.backoff_until_ms > snapshot.now_monotonic_ms {
                    continue;
                }
                let decision = evaluate_registry_resource_policy(
                    plan,
                    descriptor.source_kind,
                    &remaining_capacity,
                    snapshot,
                );
                if decision != ResourcePolicyDecision::Proceed {
                    last_blocking_decision = Some(decision);
                    continue;
                }
                if descriptor.requires_unmetered
                    && !matches!(snapshot.network_class, ObservedNetworkClass::WifiUnmetered)
                    && descriptor.source_kind != RegistrySourceKind::LocalImport
                {
                    last_blocking_decision = Some(ResourcePolicyDecision::WaitingNetwork {
                        reason: "descriptor requires unmetered Wi-Fi".into(),
                    });
                    continue;
                }
                let Some(provider_slot) = providers
                    .iter_mut()
                    .find(|item| item.descriptor_id() == descriptor.descriptor_id)
                else {
                    continue;
                };

                if last_active_descriptor
                    .as_deref()
                    .is_some_and(|prev| prev != descriptor.descriptor_id)
                {
                    failover_count = failover_count.saturating_add(1);
                }
                last_active_descriptor = Some(descriptor.descriptor_id.clone());

                let resume_offset =
                    store.registry_chunk_resume_offset(transfer_nonce, role, target.chunk_index)?;
                if resume_offset > 0 && !descriptor.supports_byte_ranges {
                    failover_count = failover_count.saturating_add(1);
                    continue;
                }

                let start = store.begin_registry_chunk_write(
                    transfer_nonce,
                    role,
                    target.chunk_index,
                    resume_offset,
                )?;
                let mut session = Some(match start {
                    RegistryChunkWriteStart::AlreadyVerified(_) => {
                        chunk_completed = true;
                        break;
                    }
                    RegistryChunkWriteStart::Ready(session) => *session,
                });

                let mut cursor_in_chunk = resume_offset;
                let mut provider_failed = false;
                while cursor_in_chunk < target.expected_length {
                    let wanted = usize::try_from(
                        (target.expected_length - cursor_in_chunk)
                            .min(MAX_PROVIDER_BLOCK_BYTES as u64),
                    )
                    .unwrap_or(MAX_PROVIDER_BLOCK_BYTES);
                    let absolute_offset = target
                        .artifact_source_offset
                        .saturating_add(cursor_in_chunk);
                    match provider_slot.read_range(role, absolute_offset, wanted) {
                        Ok(bytes) if bytes.is_empty() || bytes.len() > wanted => {
                            if let Some(open) = session.take() {
                                let _ = store.suspend_registry_chunk_write(open);
                            }
                            obs.consecutive_failures = obs.consecutive_failures.saturating_add(1);
                            obs.backoff_until_ms = snapshot.now_monotonic_ms.saturating_add(5_000);
                            obs.last_error_code = Some("PROVIDER_SHORT_OR_OVERLONG_READ".into());
                            if obs.consecutive_failures
                                >= MAX_CONSECUTIVE_FAILURES_BEFORE_QUARANTINE
                            {
                                obs.quarantined = true;
                            }
                            store.save_registry_source_observation(&plan.operation_id, &obs)?;
                            provider_failed = true;
                            break;
                        }
                        Ok(bytes) => {
                            let step_len = bytes.len() as u64;
                            let append_result = match session.as_mut() {
                                Some(open) => store.append_registry_chunk_write(open, &bytes),
                                None => Err(MobileCoreError::Storage(
                                    "Registry chunk session closed unexpectedly".into(),
                                )),
                            };
                            if let Err(error) = append_result {
                                if let Some(open) = session.take() {
                                    let _ = store.suspend_registry_chunk_write(open);
                                }
                                if error.to_string().contains("ENOSPC")
                                    || error.to_string().contains("No space left")
                                    || error.to_string().contains("disk full")
                                {
                                    store.mark_registry_operation_waiting_storage(
                                        &plan.operation_id,
                                    )?;
                                    progress = store.registry_landing_progress(transfer_nonce)?;
                                    return Ok(MultiProviderTransferOutcome {
                                        transfer_nonce: transfer_nonce.to_owned(),
                                        progress,
                                        decision: ResourcePolicyDecision::WaitingStorage {
                                            remaining_required_free_bytes: remaining_capacity
                                                .remaining_required_free_bytes,
                                            measured_free_bytes: 0,
                                        },
                                        active_descriptor_id: last_active_descriptor,
                                        failover_count,
                                        quarantined_descriptors: store
                                            .quarantined_source_descriptors(&plan.operation_id)?,
                                    });
                                }
                                return Err(error);
                            }
                            cursor_in_chunk = cursor_in_chunk.saturating_add(step_len);
                        }
                        Err(RegistryProviderFault::StorageFull) => {
                            if let Some(open) = session.take() {
                                let _ = store.suspend_registry_chunk_write(open);
                            }
                            store.mark_registry_operation_waiting_storage(&plan.operation_id)?;
                            progress = store.registry_landing_progress(transfer_nonce)?;
                            return Ok(MultiProviderTransferOutcome {
                                transfer_nonce: transfer_nonce.to_owned(),
                                progress,
                                decision: ResourcePolicyDecision::WaitingStorage {
                                    remaining_required_free_bytes: remaining_capacity
                                        .remaining_required_free_bytes,
                                    measured_free_bytes: 0,
                                },
                                active_descriptor_id: last_active_descriptor,
                                failover_count,
                                quarantined_descriptors: store
                                    .quarantined_source_descriptors(&plan.operation_id)?,
                            });
                        }
                        Err(RegistryProviderFault::CorruptUpstream) => {
                            if let Some(open) = session.take() {
                                let _ = store.suspend_registry_chunk_write(open);
                            }
                            store.reset_registry_chunk_partial(
                                transfer_nonce,
                                role,
                                target.chunk_index,
                            )?;
                            obs.corrupt_chunk_strikes = obs.corrupt_chunk_strikes.saturating_add(1);
                            obs.quarantined = true;
                            obs.last_error_code = Some("CORRUPT_UPSTREAM_QUARANTINED".into());
                            store.save_registry_source_observation(&plan.operation_id, &obs)?;
                            provider_failed = true;
                            break;
                        }
                        Err(
                            RegistryProviderFault::TransientIo(reason)
                            | RegistryProviderFault::PolicyBlocked(reason),
                        ) => {
                            if let Some(open) = session.take() {
                                let _ = store.suspend_registry_chunk_write(open);
                            }
                            obs.consecutive_failures = obs.consecutive_failures.saturating_add(1);
                            obs.backoff_until_ms = snapshot.now_monotonic_ms.saturating_add(5_000);
                            obs.last_error_code = Some(reason);
                            if obs.consecutive_failures
                                >= MAX_CONSECUTIVE_FAILURES_BEFORE_QUARANTINE
                            {
                                obs.quarantined = true;
                            }
                            store.save_registry_source_observation(&plan.operation_id, &obs)?;
                            provider_failed = true;
                            break;
                        }
                        Err(
                            RegistryProviderFault::RangeUnavailable
                            | RegistryProviderFault::UnsupportedRange,
                        ) => {
                            if let Some(open) = session.take() {
                                let _ = store.suspend_registry_chunk_write(open);
                            }
                            obs.consecutive_failures = obs.consecutive_failures.saturating_add(1);
                            obs.backoff_until_ms = snapshot.now_monotonic_ms.saturating_add(2_000);
                            obs.last_error_code = Some("RANGE_UNAVAILABLE".into());
                            store.save_registry_source_observation(&plan.operation_id, &obs)?;
                            provider_failed = true;
                            break;
                        }
                    }
                }

                if provider_failed {
                    failover_count = failover_count.saturating_add(1);
                    continue;
                }

                let open_session = session.take().ok_or_else(|| {
                    MobileCoreError::Storage("Registry chunk session missing at finish".into())
                })?;
                match store.finish_registry_chunk_write(open_session) {
                    Ok(verified_record) => {
                        obs.consecutive_failures = 0;
                        obs.last_error_code = None;
                        obs.verified_bytes_contributed = obs
                            .verified_bytes_contributed
                            .saturating_add(verified_record.expected_length);
                        store.save_registry_source_observation(&plan.operation_id, &obs)?;
                        chunk_completed = true;
                        break;
                    }
                    Err(MobileCoreError::Security(_)) => {
                        obs.corrupt_chunk_strikes = obs.corrupt_chunk_strikes.saturating_add(1);
                        obs.quarantined = true;
                        obs.last_error_code = Some("CHUNK_BLAKE3_MISMATCH_QUARANTINED".into());
                        store.save_registry_source_observation(&plan.operation_id, &obs)?;
                        failover_count = failover_count.saturating_add(1);
                        continue;
                    }
                    Err(other) => return Err(other),
                }
            }

            if !chunk_completed {
                progress = store.registry_landing_progress(transfer_nonce)?;
                let waiting_decision = last_blocking_decision.unwrap_or_else(|| {
                    ResourcePolicyDecision::WaitingNetwork {
                        reason: "all confirmed Registry providers are unavailable or quarantined"
                            .into(),
                    }
                });
                store.mark_registry_operation_waiting_network(&plan.operation_id)?;
                return Ok(MultiProviderTransferOutcome {
                    transfer_nonce: transfer_nonce.to_owned(),
                    progress,
                    decision: waiting_decision,
                    active_descriptor_id: last_active_descriptor,
                    failover_count,
                    quarantined_descriptors: store
                        .quarantined_source_descriptors(&plan.operation_id)?,
                });
            }
        }
    }

    progress = store.registry_landing_progress(transfer_nonce)?;
    Ok(MultiProviderTransferOutcome {
        transfer_nonce: transfer_nonce.to_owned(),
        progress,
        decision: ResourcePolicyDecision::Proceed,
        active_descriptor_id: last_active_descriptor,
        failover_count,
        quarantined_descriptors: store.quarantined_source_descriptors(&plan.operation_id)?,
    })
}
