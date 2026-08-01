use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    registry_admission::{
        compare_bound_generation, compare_bound_release, registry_admission,
        verified_registry_artifacts, verify_registry_target, VerifiedRegistryTarget,
        FIXED_CHUNK_BYTES, REGISTRY_CHUNK_DOMAIN,
    },
    MobileCoreError, RegistryCapacityPlan, RegistryChannelHighWater, RegistryLimitedReceipt,
    RegistryNetworkPolicy, RegistryOperationState, RegistryReleaseCatalogRecord,
    RegistryReleaseHighWater, RegistryReleaseState, RegistryTrustProfile, RegistryWaitingReason,
    ResourceBudgets,
};

const PROCESS_GENERATIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("process_generations");
const REGISTRY_OPERATIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_operations");
const REGISTRY_CHUNKS: TableDefinition<&str, &[u8]> = TableDefinition::new("registry_chunks");
const TRANSFER_LANDING: TableDefinition<&str, &[u8]> = TableDefinition::new("transfer_landing");
const REGISTRY_TRANSFER_SCHEDULES: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_transfer_schedules");
const BOOTSTRAP_OPERATION_IDS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("bootstrap_op_ids");
const INSTALLATION_AUTHORITY: TableDefinition<&str, &[u8]> =
    TableDefinition::new("installation_authority");
const PRIVACY_POLICY: TableDefinition<&str, &[u8]> = TableDefinition::new("privacy_policy");
const SECURITY_HISTORY: TableDefinition<u64, &[u8]> = TableDefinition::new("security_history");
const SECURITY_METADATA: TableDefinition<&str, &[u8]> = TableDefinition::new("security_metadata");
const ONBOARDING_STATE: TableDefinition<&str, &[u8]> = TableDefinition::new("onboarding_state");
const REGISTRY_RELEASE_CATALOG: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_release_catalog");
const REGISTRY_SECURITY_HIGHWATER: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_security_highwater");
const REGISTRY_REVOCATIONS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_revocations");
const REGISTRY_CHANNEL_INTENTS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_channel_intents");
const REGISTRY_LIMITED_RECEIPTS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("registry_limited_receipts");
const CURRENT_PROCESS_KEY: &str = "current";
const CURRENT_INSTALLATION_KEY: &str = "current";
const CURRENT_PRIVACY_POLICY_KEY: &str = "current";
const NEXT_SECURITY_SEQUENCE_KEY: &str = "next_sequence";
const CURRENT_ONBOARDING_KEY: &str = "current";
const TRUST_PROFILE_HIGHWATER_KEY: &str = "trust_profile";
const RELEASE_HIGHWATER_KEY: &str = "publisher_release";
const MAX_SECURITY_HISTORY_RECORDS: u64 = 512;
const CALLBACK_FENCE_PROBE_OPERATION: &str = "mob02.bootstrap.probe";
const CALLBACK_FENCE_PROBE_RELEASE: &str = "none";
const CALLBACK_FENCE_PROBE_ROLE: &str = "fence_probe";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingCursor {
    #[default]
    Welcome,
    Preflight,
    Identity,
    Security,
    InitHandoff,
    LimitedHome,
}

impl OnboardingCursor {
    pub const fn code(self) -> u32 {
        match self {
            Self::Welcome => 0,
            Self::Preflight => 1,
            Self::Identity => 2,
            Self::Security => 3,
            Self::InitHandoff => 4,
            Self::LimitedHome => 5,
        }
    }

    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Welcome),
            1 => Some(Self::Preflight),
            2 => Some(Self::Identity),
            3 => Some(Self::Security),
            4 => Some(Self::InitHandoff),
            5 => Some(Self::LimitedHome),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessLifecycle {
    Started,
    Quiesced,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessGenerationRecord {
    pub generation: u64,
    pub lifecycle: ProcessLifecycle,
    pub observed_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessStart {
    pub generation: u64,
    pub recovered_unclean_start: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegistryOperationRecord {
    pub operation_id: String,
    pub release_id: String,
    pub state: RegistryOperationState,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub head_generation: Option<u64>,
    #[serde(default)]
    pub head_digest: Option<String>,
    #[serde(default)]
    pub manifest_digest: Option<String>,
    #[serde(default)]
    pub trust_profile_digest: Option<String>,
    #[serde(default)]
    pub trust_profile_generation: Option<u64>,
    #[serde(default)]
    pub confirmed_manifest_digest: Option<String>,
    #[serde(default)]
    pub confirmed_trust_profile_digest: Option<String>,
    #[serde(default)]
    pub capacity_plan: Option<RegistryCapacityPlan>,
    #[serde(default)]
    pub network_policy: Option<RegistryNetworkPolicy>,
    #[serde(default)]
    pub one_time_network_override: bool,
    #[serde(default)]
    pub waiting_reason: Option<RegistryWaitingReason>,
    #[serde(default)]
    pub resume_state: Option<RegistryOperationState>,
    #[serde(default)]
    pub active_transfer_nonce: Option<String>,
}

impl RegistryOperationRecord {
    fn intent(operation_id: String, channel_id: String, profile: &RegistryTrustProfile) -> Self {
        Self {
            operation_id,
            release_id: String::new(),
            state: RegistryOperationState::IntentRecorded,
            channel_id: Some(channel_id),
            head_generation: None,
            head_digest: None,
            manifest_digest: None,
            trust_profile_digest: Some(profile.digest_hex()),
            trust_profile_generation: Some(profile.profile_generation()),
            confirmed_manifest_digest: None,
            confirmed_trust_profile_digest: None,
            capacity_plan: None,
            network_policy: None,
            one_time_network_override: false,
            waiting_reason: None,
            resume_state: None,
            active_transfer_nonce: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryTrustProfileRecord {
    profile_generation: u64,
    profile_digest: String,
    canonical_cbor: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryRevocationRecord {
    revoked_release_id: String,
    accepted_by_release_id: String,
    accepted_at_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryChunkRecord {
    pub transfer_nonce: String,
    pub operation_id: String,
    pub release_id: String,
    pub artifact_role: u8,
    pub chunk_index: u32,
    pub expected_hash: String,
    pub expected_length: u64,
    pub state: RegistryChunkState,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryChunkState {
    Planned,
    Receiving,
    Verified,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryLandingProgress {
    pub transfer_nonce: String,
    pub total_chunks: u32,
    pub verified_chunks: u32,
    pub expected_bytes: u64,
    pub verified_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransferLandingRecord {
    pub transfer_nonce: String,
    pub operation_id: String,
    pub release_id: String,
    pub artifact_role: String,
    pub chunk_index: u32,
    pub expected_hash: String,
    pub expected_length: u64,
    pub os_transfer_id: Option<String>,
    pub receiving_process_generation: Option<u64>,
    pub app_assigned_callback_sequence: Option<u64>,
    pub landed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryTransferPlatform {
    AndroidUidt,
    IosBackgroundUrlSession,
    ForegroundHttps,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryTransferScheduleState {
    SchedulePrepared,
    TransferSubmitted,
    TransferAdopted,
    ResumeRequiredAfterUnobservedStop,
    UserStoppedOsJob,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryTransferScheduleRecord {
    pub transfer_nonce: String,
    pub operation_id: String,
    pub release_id: String,
    pub manifest_digest: String,
    pub trust_profile_digest: String,
    pub request_fingerprint: String,
    pub transport_descriptor_digest: String,
    pub expected_total_bytes: u64,
    pub platform: RegistryTransferPlatform,
    pub android_job_id: Option<u32>,
    pub os_transfer_id: Option<String>,
    pub state: RegistryTransferScheduleState,
    pub prepared_process_generation: u64,
    pub submitted_process_generation: Option<u64>,
    pub adopted_process_generation: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationAuthorityRecord {
    pub profile_version: u32,
    pub installation_epoch: String,
    pub installation_instance_nonce: String,
    pub binding_digest: String,
    pub node_id: String,
    pub feed_id: String,
    pub actor_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyPolicyRecord {
    pub generation: u64,
    pub private_local_default: bool,
    pub private_shared_requires_confirmation: bool,
    pub public_candidate_requires_confirmation: bool,
    pub public_accepted_requires_confirmation: bool,
}

impl Default for PrivacyPolicyRecord {
    fn default() -> Self {
        Self {
            generation: 1,
            private_local_default: true,
            private_shared_requires_confirmation: true,
            public_candidate_requires_confirmation: true,
            public_accepted_requires_confirmation: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityHistoryRecord {
    pub sequence: u64,
    pub process_generation: u64,
    pub monotonic_ms: u64,
    pub event_code: String,
    pub scope_code: String,
    pub succeeded: bool,
}

pub struct BootstrapStore {
    database: Database,
    path: PathBuf,
}

impl BootstrapStore {
    pub fn open(path: &Path) -> Result<Self, MobileCoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create bootstrap directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let database = Database::create(path)?;
        let write = database.begin_write()?;
        {
            let _ = write.open_table(PROCESS_GENERATIONS)?;
            let _ = write.open_table(REGISTRY_OPERATIONS)?;
            let _ = write.open_table(REGISTRY_CHUNKS)?;
            let _ = write.open_table(TRANSFER_LANDING)?;
            let _ = write.open_table(REGISTRY_TRANSFER_SCHEDULES)?;
            let _ = write.open_table(BOOTSTRAP_OPERATION_IDS)?;
            let _ = write.open_table(INSTALLATION_AUTHORITY)?;
            let _ = write.open_table(PRIVACY_POLICY)?;
            let _ = write.open_table(SECURITY_HISTORY)?;
            let _ = write.open_table(SECURITY_METADATA)?;
            let _ = write.open_table(ONBOARDING_STATE)?;
            let _ = write.open_table(REGISTRY_RELEASE_CATALOG)?;
            let _ = write.open_table(REGISTRY_SECURITY_HIGHWATER)?;
            let _ = write.open_table(REGISTRY_REVOCATIONS)?;
            let _ = write.open_table(REGISTRY_CHANNEL_INTENTS)?;
            let _ = write.open_table(REGISTRY_LIMITED_RECEIPTS)?;
        }
        write.commit()?;
        Ok(Self {
            database,
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn start_process(
        &self,
        observed_at_monotonic_ms: u64,
    ) -> Result<ProcessStart, MobileCoreError> {
        let write = self.database.begin_write()?;
        let (generation, recovered_unclean_start);
        {
            let mut table = write.open_table(PROCESS_GENERATIONS)?;
            let previous = table
                .get(CURRENT_PROCESS_KEY)?
                .map(|value| decode::<ProcessGenerationRecord>(value.value()))
                .transpose()?;
            generation = previous
                .as_ref()
                .map_or(1, |record| record.generation.saturating_add(1));
            if generation == u64::MAX
                && previous
                    .as_ref()
                    .is_some_and(|record| record.generation == u64::MAX)
            {
                return Err(MobileCoreError::Storage(
                    "process generation exhausted".into(),
                ));
            }
            recovered_unclean_start = previous
                .as_ref()
                .is_some_and(|record| record.lifecycle == ProcessLifecycle::Started);
            let record = ProcessGenerationRecord {
                generation,
                lifecycle: ProcessLifecycle::Started,
                observed_at_monotonic_ms,
            };
            let bytes = encode(&record)?;
            table.insert(CURRENT_PROCESS_KEY, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(ProcessStart {
            generation,
            recovered_unclean_start,
        })
    }

    pub fn current_process(&self) -> Result<Option<ProcessGenerationRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(PROCESS_GENERATIONS)?;
        table
            .get(CURRENT_PROCESS_KEY)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn bind_installation_authority(
        &self,
        authority: &InstallationAuthorityRecord,
    ) -> Result<bool, MobileCoreError> {
        validate_installation_authority(authority)?;
        let write = self.database.begin_write()?;
        let created;
        {
            let mut table = write.open_table(INSTALLATION_AUTHORITY)?;
            let current = table
                .get(CURRENT_INSTALLATION_KEY)?
                .map(|value| decode::<InstallationAuthorityRecord>(value.value()))
                .transpose()?;
            match current {
                Some(current) if current == *authority => {
                    created = false;
                }
                Some(_) => {
                    return Err(MobileCoreError::UnexpectedRestore(
                        "installation epoch, nonce, seal or signer authority does not match".into(),
                    ));
                }
                None => {
                    let bytes = encode(authority)?;
                    table.insert(CURRENT_INSTALLATION_KEY, bytes.as_slice())?;
                    created = true;
                }
            }
        }
        write.commit()?;
        Ok(created)
    }

    pub fn installation_authority(
        &self,
    ) -> Result<Option<InstallationAuthorityRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(INSTALLATION_AUTHORITY)?;
        table
            .get(CURRENT_INSTALLATION_KEY)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn privacy_policy(&self) -> Result<PrivacyPolicyRecord, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(PRIVACY_POLICY)?;
        Ok(table
            .get(CURRENT_PRIVACY_POLICY_KEY)?
            .map(|value| decode(value.value()))
            .transpose()?
            .unwrap_or_default())
    }

    pub fn onboarding_cursor(&self) -> Result<OnboardingCursor, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(ONBOARDING_STATE)?;
        Ok(table
            .get(CURRENT_ONBOARDING_KEY)?
            .map(|value| decode(value.value()))
            .transpose()?
            .unwrap_or_default())
    }

    pub fn set_onboarding_cursor(&self, next: OnboardingCursor) -> Result<(), MobileCoreError> {
        let current = self.onboarding_cursor()?;
        if !valid_onboarding_transition(current, next) {
            return Err(MobileCoreError::InvalidArgument(
                "onboarding cursor transition is not allowed".into(),
            ));
        }
        let bytes = encode(&next)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(ONBOARDING_STATE)?;
            table.insert(CURRENT_ONBOARDING_KEY, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn replace_privacy_policy(
        &self,
        policy: &PrivacyPolicyRecord,
    ) -> Result<(), MobileCoreError> {
        if !policy.private_local_default
            || !policy.private_shared_requires_confirmation
            || !policy.public_candidate_requires_confirmation
            || !policy.public_accepted_requires_confirmation
        {
            return Err(MobileCoreError::Security(
                "MOB-03 foundation only accepts fail-safe privacy defaults".into(),
            ));
        }
        let current = self.privacy_policy()?;
        if policy.generation < current.generation {
            return Err(MobileCoreError::Security(
                "privacy policy generation cannot roll back".into(),
            ));
        }
        let bytes = encode(policy)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(PRIVACY_POLICY)?;
            table.insert(CURRENT_PRIVACY_POLICY_KEY, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn append_security_history(
        &self,
        process_generation: u64,
        monotonic_ms: u64,
        event_code: &str,
        scope_code: &str,
        succeeded: bool,
    ) -> Result<SecurityHistoryRecord, MobileCoreError> {
        validate_audit_code("event_code", event_code)?;
        validate_audit_code("scope_code", scope_code)?;
        let write = self.database.begin_write()?;
        let record;
        {
            let mut metadata = write.open_table(SECURITY_METADATA)?;
            let sequence = metadata
                .get(NEXT_SECURITY_SEQUENCE_KEY)?
                .map(|value| decode::<u64>(value.value()))
                .transpose()?
                .unwrap_or(1);
            if sequence == u64::MAX {
                return Err(MobileCoreError::Security(
                    "security history sequence exhausted".into(),
                ));
            }
            record = SecurityHistoryRecord {
                sequence,
                process_generation,
                monotonic_ms,
                event_code: event_code.to_owned(),
                scope_code: scope_code.to_owned(),
                succeeded,
            };
            let bytes = encode(&record)?;
            let mut history = write.open_table(SECURITY_HISTORY)?;
            history.insert(sequence, bytes.as_slice())?;
            if sequence > MAX_SECURITY_HISTORY_RECORDS {
                history.remove(sequence - MAX_SECURITY_HISTORY_RECORDS)?;
            }
            let next = encode(&sequence.saturating_add(1))?;
            metadata.insert(NEXT_SECURITY_SEQUENCE_KEY, next.as_slice())?;
        }
        write.commit()?;
        Ok(record)
    }

    pub fn recent_security_history(
        &self,
        limit: usize,
    ) -> Result<Vec<SecurityHistoryRecord>, MobileCoreError> {
        if limit == 0 || limit > MAX_SECURITY_HISTORY_RECORDS as usize {
            return Err(MobileCoreError::BudgetExceeded(
                "security history page must contain 1..=512 records".into(),
            ));
        }
        let read = self.database.begin_read()?;
        let table = read.open_table(SECURITY_HISTORY)?;
        let mut records = Vec::with_capacity(limit);
        for entry in table.iter()?.rev().take(limit) {
            let (_, value) = entry?;
            records.push(decode(value.value())?);
        }
        Ok(records)
    }

    pub fn quiesce_process(
        &self,
        generation: u64,
        observed_at_monotonic_ms: u64,
    ) -> Result<(), MobileCoreError> {
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(PROCESS_GENERATIONS)?;
            let current = required_current_process(&table)?;
            ensure_generation(current.generation, generation)?;
            let record = ProcessGenerationRecord {
                generation,
                lifecycle: ProcessLifecycle::Quiesced,
                observed_at_monotonic_ms,
            };
            let bytes = encode(&record)?;
            table.insert(CURRENT_PROCESS_KEY, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn install_registry_trust_profile(
        &self,
        profile: &RegistryTrustProfile,
    ) -> Result<bool, MobileCoreError> {
        let write = self.database.begin_write()?;
        let changed;
        {
            let mut table = write.open_table(REGISTRY_SECURITY_HIGHWATER)?;
            let current = table
                .get(TRUST_PROFILE_HIGHWATER_KEY)?
                .map(|value| decode::<RegistryTrustProfileRecord>(value.value()))
                .transpose()?;
            match current {
                Some(current) => {
                    let parsed =
                        RegistryTrustProfile::from_canonical_cbor(&current.canonical_cbor)?;
                    if parsed.profile_generation() != current.profile_generation
                        || parsed.digest_hex() != current.profile_digest
                    {
                        return Err(MobileCoreError::RegistryTrust(
                            "durable trust profile record failed its own digest binding".into(),
                        ));
                    }
                    profile.compatible_update_from(&parsed)?;
                    changed = profile.digest_hex() != current.profile_digest;
                }
                None => changed = true,
            }
            if changed {
                let record = RegistryTrustProfileRecord {
                    profile_generation: profile.profile_generation(),
                    profile_digest: profile.digest_hex(),
                    canonical_cbor: profile.canonical_cbor().to_vec(),
                };
                let bytes = encode(&record)?;
                table.insert(TRUST_PROFILE_HIGHWATER_KEY, bytes.as_slice())?;
            }
        }
        write.commit()?;
        Ok(changed)
    }

    pub fn begin_registry_init(
        &self,
        channel_id: &str,
        profile: &RegistryTrustProfile,
        budgets: &ResourceBudgets,
    ) -> Result<RegistryOperationRecord, MobileCoreError> {
        let floor = profile.floor(channel_id).ok_or_else(|| {
            MobileCoreError::RegistryAdmission(
                "cannot begin Init for a channel without an embedded floor".into(),
            )
        })?;
        if floor.channel_id != channel_id {
            return Err(registry_admission("channel floor lookup is inconsistent"));
        }
        require_bounded("channel_id", channel_id, 64)?;
        self.install_registry_trust_profile(profile)?;

        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| {
            MobileCoreError::Security("Registry operation CSPRNG unavailable".into())
        })?;
        let candidate_id = format!("registry-init-{}", hex::encode(random));
        require_bounded(
            "operation_id",
            &candidate_id,
            budgets.max_operation_id_bytes,
        )?;

        let write = self.database.begin_write()?;
        let result;
        {
            let mut intents = write.open_table(REGISTRY_CHANNEL_INTENTS)?;
            let existing_id = intents
                .get(channel_id)?
                .map(|value| decode::<String>(value.value()))
                .transpose()?;
            let existing_operation = if let Some(existing_id) = existing_id {
                let operations = write.open_table(REGISTRY_OPERATIONS)?;
                let decoded = Some(
                    operations
                        .get(existing_id.as_str())?
                        .map(|value| decode(value.value()))
                        .transpose()?
                        .ok_or_else(|| {
                            MobileCoreError::Storage(
                                "Registry channel intent lost its operation record".into(),
                            )
                        })?,
                );
                decoded
            } else {
                None
            };
            if let Some(existing) =
                existing_operation.filter(|operation: &RegistryOperationRecord| {
                    !matches!(
                        operation.state,
                        RegistryOperationState::Completed
                            | RegistryOperationState::Failed
                            | RegistryOperationState::Cancelled
                    )
                })
            {
                result = existing;
            } else {
                let record = RegistryOperationRecord::intent(
                    candidate_id.clone(),
                    channel_id.to_owned(),
                    profile,
                );
                let id_bytes = encode(&candidate_id)?;
                intents.insert(channel_id, id_bytes.as_slice())?;
                drop(intents);
                let mut op_ids = write.open_table(BOOTSTRAP_OPERATION_IDS)?;
                op_ids.insert(candidate_id.as_str(), b"registry".as_slice())?;
                drop(op_ids);
                let bytes = encode(&record)?;
                let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
                operations.insert(candidate_id.as_str(), bytes.as_slice())?;
                result = record;
            }
        }
        write.commit()?;
        Ok(result)
    }

    pub fn verify_and_accept_registry_target(
        &self,
        operation_id: &str,
        profile: &RegistryTrustProfile,
        requested_channel_id: &str,
        channel_head_envelope_cbor: &[u8],
        release_envelope_cbor: &[u8],
    ) -> Result<RegistryReleaseCatalogRecord, MobileCoreError> {
        let target = verify_registry_target(
            profile,
            requested_channel_id,
            channel_head_envelope_cbor,
            release_envelope_cbor,
        )?;
        self.accept_verified_registry_target(operation_id, profile, &target)
    }

    fn accept_verified_registry_target(
        &self,
        operation_id: &str,
        profile: &RegistryTrustProfile,
        target: &VerifiedRegistryTarget,
    ) -> Result<RegistryReleaseCatalogRecord, MobileCoreError> {
        if target.trust_profile_digest != profile.digest()
            || target.trust_profile_generation != profile.profile_generation()
        {
            return Err(registry_admission(
                "verified target lost its exact trust-profile binding",
            ));
        }
        let release_id = hex::encode(target.release_id);
        let manifest_digest = hex::encode(target.manifest_digest);
        let head_digest = hex::encode(target.head_digest);
        let write = self.database.begin_write()?;
        let mut catalog_record;
        {
            let highwater = write.open_table(REGISTRY_SECURITY_HIGHWATER)?;
            let trust = highwater
                .get(TRUST_PROFILE_HIGHWATER_KEY)?
                .map(|value| decode::<RegistryTrustProfileRecord>(value.value()))
                .transpose()?
                .ok_or_else(|| registry_admission("embedded trust profile was not installed"))?;
            if trust.profile_generation != profile.profile_generation()
                || trust.profile_digest != profile.digest_hex()
            {
                return Err(registry_admission(
                    "operation trust profile is not the current durable profile",
                ));
            }
            let channel_highwater_key = channel_highwater_key(&target.channel_id);
            let channel_highwater = highwater
                .get(channel_highwater_key.as_str())?
                .map(|value| decode::<RegistryChannelHighWater>(value.value()))
                .transpose()?;
            let release_highwater = highwater
                .get(RELEASE_HIGHWATER_KEY)?
                .map(|value| decode::<RegistryReleaseHighWater>(value.value()))
                .transpose()?;
            drop(highwater);

            if let Some(current) = &channel_highwater {
                compare_bound_generation(
                    target.head_generation,
                    &target.head_digest,
                    current.head_generation,
                    &decode_digest_hex(&current.head_digest, "channel high-water digest")?,
                    "durable channel high-water",
                )?;
            }
            if let Some(current) = &release_highwater {
                compare_bound_release(
                    target.release_sequence,
                    &target.release_id,
                    &target.manifest_digest,
                    current.release_sequence,
                    &decode_digest_hex(&current.release_id, "release high-water ID")?,
                    &decode_digest_hex(
                        &current.manifest_digest,
                        "release high-water manifest digest",
                    )?,
                    "durable publisher high-water",
                )?;
            }

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(operation_id)?
                .ok_or_else(|| registry_admission("unknown Registry Init operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            if operation.channel_id.as_deref() != Some(target.channel_id.as_str()) {
                return Err(registry_admission(
                    "Registry operation channel does not match signed target",
                ));
            }
            if matches!(
                operation.state,
                RegistryOperationState::AwaitingExactConfirm
                    | RegistryOperationState::AdmissionPending
                    | RegistryOperationState::CapacityAdmitted
                    | RegistryOperationState::Waiting
            ) {
                if operation.release_id == release_id
                    && operation.manifest_digest.as_deref() == Some(manifest_digest.as_str())
                    && operation.head_digest.as_deref() == Some(head_digest.as_str())
                {
                    drop(operations);
                    let catalog = write.open_table(REGISTRY_RELEASE_CATALOG)?;
                    return catalog
                        .get(release_id.as_str())?
                        .map(|value| decode(value.value()))
                        .transpose()?
                        .ok_or_else(|| {
                            MobileCoreError::Storage(
                                "accepted Registry operation lost its catalog record".into(),
                            )
                        });
                }
                return Err(registry_admission(
                    "confirmed or admitted operation cannot silently change target",
                ));
            }
            if !matches!(
                operation.state,
                RegistryOperationState::IntentRecorded
                    | RegistryOperationState::ResolvingHead
                    | RegistryOperationState::HeadVerified
                    | RegistryOperationState::ResolvingManifest
                    | RegistryOperationState::ManifestVerified
            ) {
                return Err(registry_admission(
                    "Registry operation state cannot accept a manifest",
                ));
            }

            let revocations = write.open_table(REGISTRY_REVOCATIONS)?;
            if revocations.get(release_id.as_str())?.is_some() {
                return Err(registry_admission(
                    "a previously revoked Registry release cannot be accepted",
                ));
            }
            drop(revocations);

            catalog_record = RegistryReleaseCatalogRecord {
                release_id: release_id.clone(),
                release_sequence: target.release_sequence,
                manifest_digest: manifest_digest.clone(),
                manifest_body_cbor: target.manifest_body_cbor.clone(),
                release_signing_key_id: target.release_signing_key_id.clone(),
                channel_signing_key_id: target.channel_signing_key_id.clone(),
                trust_profile_digest: profile.digest_hex(),
                required_runtime_range: target.required_runtime_range.clone(),
                publisher_min_additional_free_bytes: target.publisher_min_additional_free_bytes,
                artifact_total_bytes: target.artifact_total_bytes,
                state: RegistryReleaseState::ManifestVerified,
            };
            let mut catalog = write.open_table(REGISTRY_RELEASE_CATALOG)?;
            if let Some(existing) = catalog.get(release_id.as_str())? {
                let existing: RegistryReleaseCatalogRecord = decode(existing.value())?;
                if existing.state == RegistryReleaseState::Revoked
                    || existing.release_sequence != catalog_record.release_sequence
                    || existing.manifest_digest != catalog_record.manifest_digest
                    || existing.manifest_body_cbor != catalog_record.manifest_body_cbor
                    || existing.release_signing_key_id != catalog_record.release_signing_key_id
                    || existing.channel_signing_key_id != catalog_record.channel_signing_key_id
                    || existing.required_runtime_range != catalog_record.required_runtime_range
                {
                    return Err(registry_admission(
                        "release catalog contains a conflicting immutable binding",
                    ));
                }
                catalog_record = existing;
            }

            for revoked in &target.revoked_release_ids {
                let revoked_id = hex::encode(revoked);
                let revocation = RegistryRevocationRecord {
                    revoked_release_id: revoked_id.clone(),
                    accepted_by_release_id: release_id.clone(),
                    accepted_at_sequence: target.release_sequence,
                };
                let bytes = encode(&revocation)?;
                let mut revocations = write.open_table(REGISTRY_REVOCATIONS)?;
                let existing_revocation = revocations
                    .get(revoked_id.as_str())?
                    .map(|value| decode::<RegistryRevocationRecord>(value.value()))
                    .transpose()?;
                if let Some(existing) = existing_revocation {
                    if existing.revoked_release_id != revoked_id {
                        return Err(registry_admission(
                            "revocation authority contains a conflicting release key",
                        ));
                    }
                } else {
                    revocations.insert(revoked_id.as_str(), bytes.as_slice())?;
                }
                drop(revocations);
                let existing_catalog = catalog
                    .get(revoked_id.as_str())?
                    .map(|value| decode::<RegistryReleaseCatalogRecord>(value.value()))
                    .transpose()?;
                if let Some(mut record) = existing_catalog {
                    record.state = RegistryReleaseState::Revoked;
                    let bytes = encode(&record)?;
                    catalog.insert(revoked_id.as_str(), bytes.as_slice())?;
                }
            }
            let catalog_bytes = encode(&catalog_record)?;
            catalog.insert(release_id.as_str(), catalog_bytes.as_slice())?;
            drop(catalog);

            operation.release_id = release_id.clone();
            operation.state = RegistryOperationState::ManifestVerified;
            operation.head_generation = Some(target.head_generation);
            operation.head_digest = Some(head_digest.clone());
            operation.manifest_digest = Some(manifest_digest.clone());
            operation.trust_profile_digest = Some(profile.digest_hex());
            operation.trust_profile_generation = Some(profile.profile_generation());
            operation.confirmed_manifest_digest = None;
            operation.confirmed_trust_profile_digest = None;
            operation.capacity_plan = None;
            operation.network_policy = None;
            operation.one_time_network_override = false;
            operation.waiting_reason = None;
            operation.resume_state = None;
            let operation_bytes = encode(&operation)?;
            operations.insert(operation_id, operation_bytes.as_slice())?;
            drop(operations);

            let mut highwater = write.open_table(REGISTRY_SECURITY_HIGHWATER)?;
            let channel_record = RegistryChannelHighWater {
                channel_id: target.channel_id.clone(),
                head_generation: target.head_generation,
                head_digest,
            };
            let channel_bytes = encode(&channel_record)?;
            highwater.insert(channel_highwater_key.as_str(), channel_bytes.as_slice())?;
            let release_record = RegistryReleaseHighWater {
                release_sequence: target.release_sequence,
                release_id,
                manifest_digest,
            };
            let release_bytes = encode(&release_record)?;
            highwater.insert(RELEASE_HIGHWATER_KEY, release_bytes.as_slice())?;
        }
        write.commit()?;
        Ok(catalog_record)
    }

    pub fn await_registry_exact_confirmation(
        &self,
        operation_id: &str,
    ) -> Result<RegistryOperationRecord, MobileCoreError> {
        self.update_registry_operation(operation_id, |operation| {
            match operation.state {
                RegistryOperationState::ManifestVerified => {
                    operation.state = RegistryOperationState::AwaitingExactConfirm;
                }
                RegistryOperationState::AwaitingExactConfirm => {}
                _ => {
                    return Err(registry_admission(
                        "only a verified manifest can enter exact confirmation",
                    ));
                }
            }
            Ok(())
        })
    }

    pub fn defer_registry_init(
        &self,
        operation_id: &str,
        manifest_digest: &str,
    ) -> Result<RegistryLimitedReceipt, MobileCoreError> {
        validate_hash(manifest_digest)?;
        let write = self.database.begin_write()?;
        let receipt;
        {
            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(operation_id)?
                .ok_or_else(|| registry_admission("unknown Registry Init operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            if operation.state == RegistryOperationState::DeferredByUser {
                let receipts = write.open_table(REGISTRY_LIMITED_RECEIPTS)?;
                let existing = receipts
                    .get(operation_id)?
                    .map(|value| decode::<RegistryLimitedReceipt>(value.value()))
                    .transpose()?
                    .ok_or_else(|| {
                        MobileCoreError::Storage(
                            "deferred Registry operation lost its Limited receipt".into(),
                        )
                    })?;
                if existing.deferred_manifest_digest == manifest_digest {
                    return Ok(existing);
                }
                return Err(registry_admission(
                    "deferred Init receipt is bound to a different manifest",
                ));
            }
            if operation.state != RegistryOperationState::AwaitingExactConfirm
                || operation.manifest_digest.as_deref() != Some(manifest_digest)
            {
                return Err(registry_admission(
                    "Init Defer requires the exact awaiting manifest",
                ));
            }
            receipt = RegistryLimitedReceipt {
                operation_id: operation_id.to_owned(),
                channel_id: operation
                    .channel_id
                    .clone()
                    .ok_or_else(|| registry_admission("Registry operation lost its channel"))?,
                deferred_manifest_digest: manifest_digest.to_owned(),
                trust_profile_digest: operation.trust_profile_digest.clone().ok_or_else(|| {
                    registry_admission("Registry operation lost its trust-profile binding")
                })?,
            };
            operation.state = RegistryOperationState::DeferredByUser;
            let operation_bytes = encode(&operation)?;
            operations.insert(operation_id, operation_bytes.as_slice())?;
            drop(operations);
            let receipt_bytes = encode(&receipt)?;
            let mut receipts = write.open_table(REGISTRY_LIMITED_RECEIPTS)?;
            receipts.insert(operation_id, receipt_bytes.as_slice())?;
        }
        write.commit()?;
        Ok(receipt)
    }

    pub fn resume_deferred_registry_init(
        &self,
        operation_id: &str,
    ) -> Result<RegistryOperationRecord, MobileCoreError> {
        self.update_registry_operation(operation_id, |operation| {
            if operation.state == RegistryOperationState::ResolvingHead {
                return Ok(());
            }
            if operation.state != RegistryOperationState::DeferredByUser {
                return Err(registry_admission(
                    "only a deferred Init operation can be re-resolved",
                ));
            }
            operation.state = RegistryOperationState::ResolvingHead;
            operation.confirmed_manifest_digest = None;
            operation.confirmed_trust_profile_digest = None;
            operation.capacity_plan = None;
            operation.network_policy = None;
            operation.one_time_network_override = false;
            operation.waiting_reason = None;
            operation.resume_state = None;
            Ok(())
        })
    }

    pub fn confirm_registry_init(
        &self,
        operation_id: &str,
        manifest_digest: &str,
        profile: &RegistryTrustProfile,
        network_policy: RegistryNetworkPolicy,
        one_time_network_override: bool,
        capacity_plan: RegistryCapacityPlan,
    ) -> Result<RegistryOperationRecord, MobileCoreError> {
        validate_hash(manifest_digest)?;
        capacity_plan.validate_exact()?;
        if one_time_network_override && network_policy != RegistryNetworkPolicy::AnyNetwork {
            return Err(registry_admission(
                "one-time Registry network override requires AnyNetwork policy",
            ));
        }
        let write = self.database.begin_write()?;
        let result;
        {
            let highwater = write.open_table(REGISTRY_SECURITY_HIGHWATER)?;
            let trust = highwater
                .get(TRUST_PROFILE_HIGHWATER_KEY)?
                .map(|value| decode::<RegistryTrustProfileRecord>(value.value()))
                .transpose()?
                .ok_or_else(|| registry_admission("embedded trust profile was not installed"))?;
            if trust.profile_generation != profile.profile_generation()
                || trust.profile_digest != profile.digest_hex()
            {
                return Err(registry_admission(
                    "exact confirmation uses a stale trust profile",
                ));
            }
            drop(highwater);

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(operation_id)?
                .ok_or_else(|| registry_admission("unknown Registry Init operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            if operation.state == RegistryOperationState::CapacityAdmitted {
                if operation.confirmed_manifest_digest.as_deref() == Some(manifest_digest)
                    && operation.confirmed_trust_profile_digest.as_deref()
                        == Some(profile.digest_hex().as_str())
                    && operation.capacity_plan.as_ref() == Some(&capacity_plan)
                    && operation.network_policy == Some(network_policy)
                    && operation.one_time_network_override == one_time_network_override
                {
                    return Ok(operation);
                }
                return Err(registry_admission(
                    "an admitted Init cannot be rebound by a repeated confirmation",
                ));
            }
            let retrying_storage_wait = operation.state == RegistryOperationState::Waiting
                && operation.waiting_reason == Some(RegistryWaitingReason::Storage)
                && operation.resume_state == Some(RegistryOperationState::AdmissionPending);
            if operation.state != RegistryOperationState::AwaitingExactConfirm
                && !retrying_storage_wait
            {
                return Err(registry_admission(
                    "Init confirmation is valid only for the exact review state",
                ));
            }
            if operation.manifest_digest.as_deref() != Some(manifest_digest)
                || operation.trust_profile_digest.as_deref() != Some(profile.digest_hex().as_str())
            {
                return Err(registry_admission(
                    "manifest or trust profile changed after exact review",
                ));
            }
            let catalog = write.open_table(REGISTRY_RELEASE_CATALOG)?;
            let release = catalog
                .get(operation.release_id.as_str())?
                .map(|value| decode::<RegistryReleaseCatalogRecord>(value.value()))
                .transpose()?
                .ok_or_else(|| registry_admission("verified release lost its catalog record"))?;
            if release.state == RegistryReleaseState::Revoked {
                return Err(registry_admission("revoked release cannot be confirmed"));
            }
            if release.manifest_digest != manifest_digest
                || release.publisher_min_additional_free_bytes
                    != capacity_plan.publisher_min_additional_free_bytes
                || capacity_plan.target_total_alloc_bytes < release.artifact_total_bytes
            {
                return Err(registry_admission(
                    "capacity plan is not bound to the exact signed release",
                ));
            }
            operation.confirmed_manifest_digest = Some(manifest_digest.to_owned());
            operation.confirmed_trust_profile_digest = Some(profile.digest_hex());
            operation.capacity_plan = Some(capacity_plan.clone());
            operation.network_policy = Some(network_policy);
            operation.one_time_network_override = one_time_network_override;
            if capacity_plan.admitted() {
                operation.state = RegistryOperationState::CapacityAdmitted;
                operation.waiting_reason = None;
                operation.resume_state = None;
            } else {
                operation.state = RegistryOperationState::Waiting;
                operation.waiting_reason = Some(RegistryWaitingReason::Storage);
                operation.resume_state = Some(RegistryOperationState::AdmissionPending);
            }
            let bytes = encode(&operation)?;
            operations.insert(operation_id, bytes.as_slice())?;
            result = operation;
        }
        write.commit()?;
        Ok(result)
    }

    pub fn registry_release_catalog(
        &self,
        release_id: &str,
    ) -> Result<Option<RegistryReleaseCatalogRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_RELEASE_CATALOG)?;
        table
            .get(release_id)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn registry_channel_highwater(
        &self,
        channel_id: &str,
    ) -> Result<Option<RegistryChannelHighWater>, MobileCoreError> {
        let key = channel_highwater_key(channel_id);
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_SECURITY_HIGHWATER)?;
        table
            .get(key.as_str())?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn registry_release_highwater(
        &self,
    ) -> Result<Option<RegistryReleaseHighWater>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_SECURITY_HIGHWATER)?;
        table
            .get(RELEASE_HIGHWATER_KEY)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn registry_release_is_revoked(&self, release_id: &str) -> Result<bool, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_REVOCATIONS)?;
        Ok(table.get(release_id)?.is_some())
    }

    pub fn registry_transfer_count(&self, operation_id: &str) -> Result<u64, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(TRANSFER_LANDING)?;
        let mut count = 0u64;
        for entry in table.iter()? {
            let (_, value) = entry?;
            let record: TransferLandingRecord = decode(value.value())?;
            if record.operation_id == operation_id {
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_registry_transfer_schedule(
        &self,
        operation_id: &str,
        manifest_digest: &str,
        platform: RegistryTransferPlatform,
        request_fingerprint: &str,
        transport_descriptor_digest: &str,
        expected_total_bytes: u64,
        foreground_user_resume: bool,
        budgets: &ResourceBudgets,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        require_bounded("operation_id", operation_id, budgets.max_operation_id_bytes)?;
        validate_hash(manifest_digest)?;
        validate_hash(request_fingerprint)?;
        validate_hash(transport_descriptor_digest)?;
        if expected_total_bytes == 0 {
            return Err(registry_admission(
                "Registry transfer schedule must bind non-zero exact bytes",
            ));
        }

        let operation_snapshot = self
            .registry_operation(operation_id)?
            .ok_or_else(|| registry_admission("unknown Registry Init operation"))?;
        if matches!(
            operation_snapshot.state,
            RegistryOperationState::SchedulePrepared
                | RegistryOperationState::TransferSubmitted
                | RegistryOperationState::TransferAdopted
        ) {
            let active_nonce = operation_snapshot
                .active_transfer_nonce
                .as_deref()
                .ok_or_else(|| registry_admission("active Registry transfer lost its nonce"))?;
            let schedule = self
                .registry_transfer_schedule(active_nonce)?
                .ok_or_else(|| registry_admission("active Registry transfer lost its schedule"))?;
            if registry_transfer_request_matches(
                &schedule,
                operation_id,
                manifest_digest,
                platform,
                request_fingerprint,
                transport_descriptor_digest,
                expected_total_bytes,
            ) {
                return Ok(schedule);
            }
            return Err(registry_admission(
                "an active Registry transfer cannot be rebound to another request",
            ));
        }

        let mut random_nonce = [0u8; 24];
        getrandom::fill(&mut random_nonce).map_err(|_| {
            MobileCoreError::Security("Registry transfer nonce CSPRNG unavailable".into())
        })?;
        let transfer_nonce = format!("registry_transfer_{}", hex::encode(random_nonce));
        require_bounded(
            "transfer_nonce",
            &transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;

        let write = self.database.begin_write()?;
        let result;
        {
            let process_table = write.open_table(PROCESS_GENERATIONS)?;
            let process_generation = required_current_process(&process_table)?.generation;
            drop(process_table);

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(operation_id)?
                .ok_or_else(|| registry_admission("unknown Registry Init operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);

            let is_foreground_resume = operation.state == RegistryOperationState::Waiting
                && matches!(
                    operation.waiting_reason,
                    Some(
                        RegistryWaitingReason::ResumeRequiredAfterUnobservedStop
                            | RegistryWaitingReason::UserStoppedOsJob
                    )
                );
            if is_foreground_resume && !foreground_user_resume {
                return Err(registry_admission(
                    "a stopped Registry transfer requires an explicit foreground Resume",
                ));
            }

            let mut schedules = write.open_table(REGISTRY_TRANSFER_SCHEDULES)?;

            if operation.state != RegistryOperationState::CapacityAdmitted && !is_foreground_resume
            {
                return Err(registry_admission(
                    "Registry transfer scheduling requires exact admitted capacity",
                ));
            }
            if operation.confirmed_manifest_digest.as_deref() != Some(manifest_digest)
                || operation
                    .capacity_plan
                    .as_ref()
                    .is_none_or(|plan| !plan.admitted())
            {
                return Err(registry_admission(
                    "Registry transfer schedule is not bound to the admitted exact plan",
                ));
            }

            let catalog = write.open_table(REGISTRY_RELEASE_CATALOG)?;
            let release = catalog
                .get(operation.release_id.as_str())?
                .map(|value| decode::<RegistryReleaseCatalogRecord>(value.value()))
                .transpose()?
                .ok_or_else(|| registry_admission("verified release lost its catalog record"))?;
            if release.state == RegistryReleaseState::Revoked
                || release.manifest_digest != manifest_digest
                || release.trust_profile_digest
                    != operation
                        .confirmed_trust_profile_digest
                        .clone()
                        .ok_or_else(|| registry_admission("missing confirmed trust binding"))?
                || release.artifact_total_bytes != expected_total_bytes
            {
                return Err(registry_admission(
                    "Registry transfer schedule does not match the eligible signed release",
                ));
            }
            drop(catalog);

            let android_job_id = match platform {
                RegistryTransferPlatform::AndroidUidt => Some(android_registry_job_id(
                    operation_id,
                    operation.release_id.as_str(),
                    request_fingerprint,
                    transfer_nonce.as_str(),
                )),
                RegistryTransferPlatform::IosBackgroundUrlSession
                | RegistryTransferPlatform::ForegroundHttps => None,
            };
            let mut platform_id_collision = false;
            if let Some(job_id) = android_job_id {
                for entry in schedules.iter()? {
                    let (_, value) = entry?;
                    let existing: RegistryTransferScheduleRecord = decode(value.value())?;
                    if existing.android_job_id == Some(job_id) {
                        platform_id_collision = true;
                        break;
                    }
                }
            }
            if schedules.get(transfer_nonce.as_str())?.is_some() || platform_id_collision {
                return Err(MobileCoreError::Security(
                    "Registry transfer nonce or platform job ID collided".into(),
                ));
            }

            let schedule = RegistryTransferScheduleRecord {
                transfer_nonce: transfer_nonce.clone(),
                operation_id: operation_id.to_owned(),
                release_id: operation.release_id.clone(),
                manifest_digest: manifest_digest.to_owned(),
                trust_profile_digest: release.trust_profile_digest,
                request_fingerprint: request_fingerprint.to_owned(),
                transport_descriptor_digest: transport_descriptor_digest.to_owned(),
                expected_total_bytes,
                platform,
                android_job_id,
                os_transfer_id: None,
                state: RegistryTransferScheduleState::SchedulePrepared,
                prepared_process_generation: process_generation,
                submitted_process_generation: None,
                adopted_process_generation: None,
            };
            let bytes = encode(&schedule)?;
            schedules.insert(transfer_nonce.as_str(), bytes.as_slice())?;
            drop(schedules);

            operation.active_transfer_nonce = Some(transfer_nonce);
            operation.state = RegistryOperationState::SchedulePrepared;
            operation.waiting_reason = None;
            operation.resume_state = None;
            let bytes = encode(&operation)?;
            operations.insert(operation_id, bytes.as_slice())?;
            result = schedule;
        }
        write.commit()?;
        Ok(result)
    }

    pub fn mark_registry_transfer_submitted(
        &self,
        transfer_nonce: &str,
        os_transfer_id: &str,
        budgets: &ResourceBudgets,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        require_bounded(
            "transfer_nonce",
            transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;
        require_bounded(
            "os_transfer_id",
            os_transfer_id,
            budgets.max_os_transfer_id_bytes,
        )?;
        self.update_registry_transfer_schedule(transfer_nonce, |operation, schedule, generation| {
            if operation.active_transfer_nonce.as_deref() != Some(transfer_nonce) {
                return Err(registry_admission("Registry transfer is no longer active"));
            }
            match schedule.state {
                RegistryTransferScheduleState::SchedulePrepared => {
                    schedule.os_transfer_id = Some(os_transfer_id.to_owned());
                    schedule.state = RegistryTransferScheduleState::TransferSubmitted;
                    schedule.submitted_process_generation = Some(generation);
                    operation.state = RegistryOperationState::TransferSubmitted;
                }
                RegistryTransferScheduleState::TransferSubmitted
                    if schedule.os_transfer_id.as_deref() == Some(os_transfer_id) => {}
                RegistryTransferScheduleState::TransferAdopted
                    if schedule.os_transfer_id.as_deref() == Some(os_transfer_id) => {}
                _ => {
                    return Err(registry_admission(
                        "Registry transfer submit receipt is stale or mismatched",
                    ));
                }
            }
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn adopt_registry_transfer(
        &self,
        transfer_nonce: &str,
        os_transfer_id: &str,
        observed_request_fingerprint: &str,
        observed_android_job_id: Option<u32>,
        matching_task_count: u32,
        budgets: &ResourceBudgets,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        require_bounded(
            "transfer_nonce",
            transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;
        require_bounded(
            "os_transfer_id",
            os_transfer_id,
            budgets.max_os_transfer_id_bytes,
        )?;
        validate_hash(observed_request_fingerprint)?;
        if matching_task_count != 1 {
            return Err(registry_admission(
                "Registry transfer adoption requires exactly one matching platform task",
            ));
        }
        self.update_registry_transfer_schedule(transfer_nonce, |operation, schedule, generation| {
            if operation.active_transfer_nonce.as_deref() != Some(transfer_nonce)
                || schedule.request_fingerprint != observed_request_fingerprint
                || schedule.android_job_id != observed_android_job_id
            {
                return Err(registry_admission(
                    "platform transfer inventory does not match the durable request",
                ));
            }
            match schedule.state {
                RegistryTransferScheduleState::SchedulePrepared => {
                    schedule.os_transfer_id = Some(os_transfer_id.to_owned());
                    schedule.submitted_process_generation = Some(generation);
                }
                RegistryTransferScheduleState::TransferSubmitted
                    if schedule.os_transfer_id.as_deref() == Some(os_transfer_id) => {}
                RegistryTransferScheduleState::TransferAdopted
                    if schedule.os_transfer_id.as_deref() == Some(os_transfer_id) =>
                {
                    return Ok(());
                }
                _ => {
                    return Err(registry_admission(
                        "Registry transfer task cannot be adopted from its current state",
                    ));
                }
            }
            schedule.state = RegistryTransferScheduleState::TransferAdopted;
            schedule.adopted_process_generation = Some(generation);
            operation.state = RegistryOperationState::TransferAdopted;
            Ok(())
        })
    }

    pub fn record_registry_transfer_missing(
        &self,
        transfer_nonce: &str,
        positive_user_stop_evidence: bool,
        budgets: &ResourceBudgets,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        require_bounded(
            "transfer_nonce",
            transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;
        self.update_registry_transfer_schedule(transfer_nonce, |operation, schedule, _| {
            if operation.active_transfer_nonce.as_deref() != Some(transfer_nonce) {
                return Err(registry_admission("Registry transfer is no longer active"));
            }
            if schedule.state == RegistryTransferScheduleState::SchedulePrepared {
                return Ok(());
            }
            if !matches!(
                schedule.state,
                RegistryTransferScheduleState::TransferSubmitted
                    | RegistryTransferScheduleState::TransferAdopted
            ) {
                return Err(registry_admission(
                    "missing-task recovery is invalid for this Registry transfer",
                ));
            }
            let (schedule_state, reason) = if positive_user_stop_evidence {
                (
                    RegistryTransferScheduleState::UserStoppedOsJob,
                    RegistryWaitingReason::UserStoppedOsJob,
                )
            } else {
                (
                    RegistryTransferScheduleState::ResumeRequiredAfterUnobservedStop,
                    RegistryWaitingReason::ResumeRequiredAfterUnobservedStop,
                )
            };
            schedule.state = schedule_state;
            operation.state = RegistryOperationState::Waiting;
            operation.waiting_reason = Some(reason);
            operation.resume_state = Some(RegistryOperationState::SchedulePrepared);
            Ok(())
        })
    }

    pub fn registry_transfer_schedule(
        &self,
        transfer_nonce: &str,
    ) -> Result<Option<RegistryTransferScheduleRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_TRANSFER_SCHEDULES)?;
        table
            .get(transfer_nonce)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn registry_transfer_schedule_for_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<RegistryTransferScheduleRecord>, MobileCoreError> {
        require_bounded("channel_id", channel_id, 64)?;
        let read = self.database.begin_read()?;
        let intents = read.open_table(REGISTRY_CHANNEL_INTENTS)?;
        let Some(operation_id) = intents
            .get(channel_id)?
            .map(|value| decode::<String>(value.value()))
            .transpose()?
        else {
            return Ok(None);
        };
        drop(intents);

        let operations = read.open_table(REGISTRY_OPERATIONS)?;
        let Some(operation) = operations
            .get(operation_id.as_str())?
            .map(|value| decode::<RegistryOperationRecord>(value.value()))
            .transpose()?
        else {
            return Err(MobileCoreError::Storage(
                "Registry channel intent lost its operation record".into(),
            ));
        };
        drop(operations);
        let Some(transfer_nonce) = operation.active_transfer_nonce else {
            return Ok(None);
        };

        let schedules = read.open_table(REGISTRY_TRANSFER_SCHEDULES)?;
        schedules
            .get(transfer_nonce.as_str())?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn registry_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<RegistryOperationRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_OPERATIONS)?;
        table
            .get(operation_id)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn prepare_registry_chunk_ledger(
        &self,
        transfer_nonce: &str,
        budgets: &ResourceBudgets,
    ) -> Result<RegistryLandingProgress, MobileCoreError> {
        require_bounded(
            "transfer_nonce",
            transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;
        let write = self.database.begin_write()?;
        {
            let schedules = write.open_table(REGISTRY_TRANSFER_SCHEDULES)?;
            let schedule = schedules
                .get(transfer_nonce)?
                .map(|value| decode::<RegistryTransferScheduleRecord>(value.value()))
                .transpose()?
                .ok_or_else(|| MobileCoreError::UnknownTransfer(transfer_nonce.to_owned()))?;
            if schedule.state != RegistryTransferScheduleState::TransferAdopted {
                return Err(registry_admission(
                    "Registry chunk ledger requires an adopted platform transfer",
                ));
            }
            drop(schedules);

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(schedule.operation_id.as_str())?
                .ok_or_else(|| registry_admission("Registry transfer lost its operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            if operation.active_transfer_nonce.as_deref() != Some(transfer_nonce)
                || operation.release_id != schedule.release_id
                || operation.confirmed_manifest_digest.as_deref()
                    != Some(schedule.manifest_digest.as_str())
                || operation.confirmed_trust_profile_digest.as_deref()
                    != Some(schedule.trust_profile_digest.as_str())
                || !matches!(
                    operation.state,
                    RegistryOperationState::TransferAdopted
                        | RegistryOperationState::TransferQueued
                        | RegistryOperationState::Downloading
                        | RegistryOperationState::BytesComplete
                )
            {
                return Err(registry_admission(
                    "Registry chunk ledger is not bound to the active exact operation",
                ));
            }

            let catalog = write.open_table(REGISTRY_RELEASE_CATALOG)?;
            let release = catalog
                .get(schedule.release_id.as_str())?
                .map(|value| decode::<RegistryReleaseCatalogRecord>(value.value()))
                .transpose()?
                .ok_or_else(|| registry_admission("Registry transfer lost its signed release"))?;
            if release.state == RegistryReleaseState::Revoked
                || release.manifest_digest != schedule.manifest_digest
                || release.trust_profile_digest != schedule.trust_profile_digest
                || release.artifact_total_bytes != schedule.expected_total_bytes
            {
                return Err(registry_admission(
                    "Registry chunk ledger does not match the eligible signed release",
                ));
            }
            let artifacts = verified_registry_artifacts(&release.manifest_body_cbor)?;
            drop(catalog);

            let expected_chunks = artifacts.iter().try_fold(0u32, |total, artifact| {
                let count = u32::try_from(artifact.chunks.len())
                    .map_err(|_| registry_admission("Registry chunk count exceeds u32"))?;
                total
                    .checked_add(count)
                    .ok_or_else(|| registry_admission("Registry chunk count overflow"))
            })?;
            let mut chunks = write.open_table(REGISTRY_CHUNKS)?;
            for artifact in artifacts {
                for chunk in artifact.chunks {
                    let record = RegistryChunkRecord {
                        transfer_nonce: transfer_nonce.to_owned(),
                        operation_id: schedule.operation_id.clone(),
                        release_id: schedule.release_id.clone(),
                        artifact_role: artifact.role,
                        chunk_index: chunk.index,
                        expected_hash: hex::encode(chunk.leaf_blake3),
                        expected_length: u64::from(chunk.length),
                        state: RegistryChunkState::Planned,
                    };
                    let key = chunk_key(transfer_nonce, artifact.role, chunk.index);
                    let existing = chunks
                        .get(key.as_str())?
                        .map(|value| decode::<RegistryChunkRecord>(value.value()))
                        .transpose()?;
                    if let Some(existing) = existing {
                        if registry_chunk_identity(&existing) != registry_chunk_identity(&record) {
                            return Err(MobileCoreError::Security(
                                "Registry chunk ledger contains a conflicting signed binding"
                                    .into(),
                            ));
                        }
                    } else {
                        let bytes = encode(&record)?;
                        chunks.insert(key.as_str(), bytes.as_slice())?;
                    }
                }
            }
            let mut observed_chunks = 0u32;
            for entry in chunks.iter()? {
                let (_, value) = entry?;
                let record: RegistryChunkRecord = decode(value.value())?;
                if record.transfer_nonce == transfer_nonce {
                    observed_chunks = observed_chunks.checked_add(1).ok_or_else(|| {
                        MobileCoreError::Security("Registry chunk ledger count overflow".into())
                    })?;
                }
            }
            if observed_chunks != expected_chunks {
                return Err(MobileCoreError::Security(
                    "Registry chunk ledger contains an unexpected chunk binding".into(),
                ));
            }
            drop(chunks);

            if operation.state == RegistryOperationState::TransferAdopted {
                operation.state = RegistryOperationState::TransferQueued;
                let bytes = encode(&operation)?;
                operations.insert(schedule.operation_id.as_str(), bytes.as_slice())?;
            }
        }
        write.commit()?;
        self.registry_landing_progress(transfer_nonce)
    }

    pub fn registry_chunk(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
    ) -> Result<Option<RegistryChunkRecord>, MobileCoreError> {
        let key = chunk_key(transfer_nonce, artifact_role, chunk_index);
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_CHUNKS)?;
        table
            .get(key.as_str())?
            .map(|value| decode(value.value()))
            .transpose()
    }

    pub fn registry_chunk_resume_offset(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
    ) -> Result<u64, MobileCoreError> {
        let record = self
            .registry_chunk(transfer_nonce, artifact_role, chunk_index)?
            .ok_or_else(|| registry_admission("unknown Registry chunk binding"))?;
        let paths = self.registry_chunk_paths(&record)?;
        if paths.verified.exists() {
            verify_registry_chunk_file(&paths.verified, &record)?;
            if record.state != RegistryChunkState::Verified {
                self.mark_registry_chunk_verified(&record)?;
            }
            return Ok(record.expected_length);
        }
        if record.state == RegistryChunkState::Verified {
            return Err(MobileCoreError::Security(
                "verified Registry chunk file is missing".into(),
            ));
        }
        bounded_regular_file_length(&paths.partial, record.expected_length)
    }

    pub fn land_registry_chunk<R: Read>(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
        source_offset: u64,
        source: &mut R,
    ) -> Result<RegistryChunkRecord, MobileCoreError> {
        let record = self
            .registry_chunk(transfer_nonce, artifact_role, chunk_index)?
            .ok_or_else(|| registry_admission("unknown Registry chunk binding"))?;
        let paths = self.registry_chunk_paths(&record)?;
        fs::create_dir_all(&paths.directory)
            .map_err(|error| registry_io("create Registry landing directory", error))?;

        if paths.verified.exists() {
            verify_registry_chunk_file(&paths.verified, &record)?;
            if record.state != RegistryChunkState::Verified {
                return self.mark_registry_chunk_verified(&record);
            }
            return Ok(record);
        }
        if record.state == RegistryChunkState::Verified {
            return Err(MobileCoreError::Security(
                "verified Registry chunk file is missing".into(),
            ));
        }

        let partial_length = bounded_regular_file_length(&paths.partial, record.expected_length)?;
        if source_offset != partial_length {
            return Err(registry_admission(format!(
                "Registry chunk source offset {source_offset} does not match durable partial length {partial_length}",
            )));
        }
        self.mark_registry_chunk_receiving(&record)?;

        let (mut hasher, rehashed_length) = rehash_registry_chunk_prefix(&paths.partial, &record)?;
        if rehashed_length != partial_length {
            return Err(MobileCoreError::Security(
                "Registry partial changed while resuming".into(),
            ));
        }
        let mut output = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.partial)
            .map_err(|error| registry_io("open Registry partial chunk", error))?;
        let remaining = record.expected_length - partial_length;
        let copied = copy_exact_bounded(source, &mut output, &mut hasher, remaining)?;
        output
            .sync_all()
            .map_err(|error| registry_io("sync Registry partial chunk", error))?;
        drop(output);
        if copied != remaining {
            return Err(registry_admission(format!(
                "Registry chunk source ended after {copied} of {remaining} remaining bytes",
            )));
        }
        let mut extra = [0u8; 1];
        if source
            .read(&mut extra)
            .map_err(|error| registry_io("read Registry chunk source tail", error))?
            != 0
        {
            reset_registry_partial(&paths.partial)?;
            self.reset_registry_chunk_planned(&record)?;
            return Err(registry_admission(
                "Registry chunk source exceeded the signed exact length",
            ));
        }
        let observed_hash = hex::encode(hasher.finalize().as_bytes());
        if observed_hash != record.expected_hash {
            reset_registry_partial(&paths.partial)?;
            self.reset_registry_chunk_planned(&record)?;
            return Err(MobileCoreError::Security(
                "Registry chunk leaf hash does not match the signed manifest".into(),
            ));
        }

        fs::rename(&paths.partial, &paths.verified)
            .map_err(|error| registry_io("commit Registry verified chunk", error))?;
        sync_directory(&paths.directory)?;
        self.mark_registry_chunk_verified(&record)
    }

    pub fn recover_registry_chunk_ledger(
        &self,
        transfer_nonce: &str,
    ) -> Result<RegistryLandingProgress, MobileCoreError> {
        let records = self.registry_chunks_for_transfer(transfer_nonce)?;
        if records.is_empty() {
            return Err(registry_admission("Registry chunk ledger is not prepared"));
        }
        for record in records {
            let paths = self.registry_chunk_paths(&record)?;
            if paths.verified.exists() {
                verify_registry_chunk_file(&paths.verified, &record)?;
                if record.state != RegistryChunkState::Verified {
                    self.mark_registry_chunk_verified(&record)?;
                }
                continue;
            }
            if record.state == RegistryChunkState::Verified {
                return Err(MobileCoreError::Security(
                    "verified Registry chunk file is missing during recovery".into(),
                ));
            }
            let partial_length =
                bounded_regular_file_length(&paths.partial, record.expected_length)?;
            if partial_length == record.expected_length {
                if verify_registry_chunk_file(&paths.partial, &record).is_ok() {
                    fs::rename(&paths.partial, &paths.verified).map_err(|error| {
                        registry_io("recover complete Registry partial chunk", error)
                    })?;
                    sync_directory(&paths.directory)?;
                    self.mark_registry_chunk_verified(&record)?;
                } else {
                    reset_registry_partial(&paths.partial)?;
                    self.reset_registry_chunk_planned(&record)?;
                }
            } else if partial_length > 0 {
                self.mark_registry_chunk_receiving(&record)?;
            } else {
                self.reset_registry_chunk_planned(&record)?;
            }
        }
        self.registry_landing_progress(transfer_nonce)
    }

    pub fn registry_landing_progress(
        &self,
        transfer_nonce: &str,
    ) -> Result<RegistryLandingProgress, MobileCoreError> {
        let records = self.registry_chunks_for_transfer(transfer_nonce)?;
        if records.is_empty() {
            return Err(registry_admission("Registry chunk ledger is not prepared"));
        }
        let mut progress = RegistryLandingProgress {
            transfer_nonce: transfer_nonce.to_owned(),
            total_chunks: 0,
            verified_chunks: 0,
            expected_bytes: 0,
            verified_bytes: 0,
        };
        for record in records {
            progress.total_chunks = progress
                .total_chunks
                .checked_add(1)
                .ok_or_else(|| MobileCoreError::Security("Registry chunk count overflow".into()))?;
            progress.expected_bytes = progress
                .expected_bytes
                .checked_add(record.expected_length)
                .ok_or_else(|| MobileCoreError::Security("Registry byte total overflow".into()))?;
            if record.state == RegistryChunkState::Verified {
                progress.verified_chunks =
                    progress.verified_chunks.checked_add(1).ok_or_else(|| {
                        MobileCoreError::Security("Registry verified count overflow".into())
                    })?;
                progress.verified_bytes = progress
                    .verified_bytes
                    .checked_add(record.expected_length)
                    .ok_or_else(|| {
                        MobileCoreError::Security("Registry verified bytes overflow".into())
                    })?;
            }
        }
        Ok(progress)
    }

    fn registry_chunks_for_transfer(
        &self,
        transfer_nonce: &str,
    ) -> Result<Vec<RegistryChunkRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_CHUNKS)?;
        let mut records = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let record: RegistryChunkRecord = decode(value.value())?;
            if record.transfer_nonce == transfer_nonce {
                records.push(record);
            }
        }
        records.sort_by_key(|record| (record.artifact_role, record.chunk_index));
        Ok(records)
    }

    fn registry_chunk_paths(
        &self,
        record: &RegistryChunkRecord,
    ) -> Result<RegistryChunkPaths, MobileCoreError> {
        validate_hash(&record.release_id)?;
        validate_hash(&record.expected_hash)?;
        if record.artifact_role > 2
            || record.expected_length == 0
            || record.expected_length > FIXED_CHUNK_BYTES
        {
            return Err(MobileCoreError::Security(
                "Registry chunk ledger contains an invalid role or length".into(),
            ));
        }
        let root = self.path.parent().unwrap_or_else(|| Path::new("."));
        let directory = root
            .join("registry")
            .join("landing")
            .join(&record.release_id)
            .join(format!("role-{}", record.artifact_role));
        let stem = format!("chunk-{:010}", record.chunk_index);
        Ok(RegistryChunkPaths {
            partial: directory.join(format!("{stem}.partial")),
            verified: directory.join(format!("{stem}.verified")),
            directory,
        })
    }

    fn mark_registry_chunk_receiving(
        &self,
        expected: &RegistryChunkRecord,
    ) -> Result<RegistryChunkRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let key = chunk_key(
                &expected.transfer_nonce,
                expected.artifact_role,
                expected.chunk_index,
            );
            let mut chunks = write.open_table(REGISTRY_CHUNKS)?;
            let existing = chunks
                .get(key.as_str())?
                .ok_or_else(|| registry_admission("Registry chunk binding disappeared"))?;
            let mut record: RegistryChunkRecord = decode(existing.value())?;
            drop(existing);
            if registry_chunk_identity(&record) != registry_chunk_identity(expected) {
                return Err(MobileCoreError::Security(
                    "Registry chunk binding changed before byte landing".into(),
                ));
            }
            if record.state != RegistryChunkState::Verified {
                record.state = RegistryChunkState::Receiving;
                let bytes = encode(&record)?;
                chunks.insert(key.as_str(), bytes.as_slice())?;
            }
            drop(chunks);

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(record.operation_id.as_str())?
                .ok_or_else(|| registry_admission("Registry chunk lost its operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            if operation.active_transfer_nonce.as_deref() != Some(record.transfer_nonce.as_str())
                || !matches!(
                    operation.state,
                    RegistryOperationState::TransferQueued
                        | RegistryOperationState::Downloading
                        | RegistryOperationState::BytesComplete
                )
            {
                return Err(registry_admission(
                    "Registry chunk cannot receive bytes from the current operation state",
                ));
            }
            if operation.state != RegistryOperationState::BytesComplete {
                operation.state = RegistryOperationState::Downloading;
                let bytes = encode(&operation)?;
                operations.insert(record.operation_id.as_str(), bytes.as_slice())?;
            }
            drop(operations);
            validate_registry_chunk_eligibility(&write, &record)?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }

    fn reset_registry_chunk_planned(
        &self,
        expected: &RegistryChunkRecord,
    ) -> Result<RegistryChunkRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let key = chunk_key(
                &expected.transfer_nonce,
                expected.artifact_role,
                expected.chunk_index,
            );
            let mut chunks = write.open_table(REGISTRY_CHUNKS)?;
            let existing = chunks
                .get(key.as_str())?
                .ok_or_else(|| registry_admission("Registry chunk binding disappeared"))?;
            let mut record: RegistryChunkRecord = decode(existing.value())?;
            drop(existing);
            if registry_chunk_identity(&record) != registry_chunk_identity(expected)
                || record.state == RegistryChunkState::Verified
            {
                return Err(MobileCoreError::Security(
                    "verified or rebound Registry chunk cannot be reset".into(),
                ));
            }
            record.state = RegistryChunkState::Planned;
            let bytes = encode(&record)?;
            chunks.insert(key.as_str(), bytes.as_slice())?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }

    fn mark_registry_chunk_verified(
        &self,
        expected: &RegistryChunkRecord,
    ) -> Result<RegistryChunkRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let key = chunk_key(
                &expected.transfer_nonce,
                expected.artifact_role,
                expected.chunk_index,
            );
            let mut chunks = write.open_table(REGISTRY_CHUNKS)?;
            let existing = chunks
                .get(key.as_str())?
                .ok_or_else(|| registry_admission("Registry chunk binding disappeared"))?;
            let mut record: RegistryChunkRecord = decode(existing.value())?;
            drop(existing);
            if registry_chunk_identity(&record) != registry_chunk_identity(expected) {
                return Err(MobileCoreError::Security(
                    "Registry chunk binding changed before verification commit".into(),
                ));
            }
            record.state = RegistryChunkState::Verified;
            let bytes = encode(&record)?;
            chunks.insert(key.as_str(), bytes.as_slice())?;

            let mut total_chunks = 0u32;
            let mut verified_chunks = 0u32;
            for entry in chunks.iter()? {
                let (_, value) = entry?;
                let observed: RegistryChunkRecord = decode(value.value())?;
                if observed.transfer_nonce == record.transfer_nonce {
                    total_chunks = total_chunks.checked_add(1).ok_or_else(|| {
                        MobileCoreError::Security("Registry chunk count overflow".into())
                    })?;
                    if observed.state == RegistryChunkState::Verified {
                        verified_chunks = verified_chunks.checked_add(1).ok_or_else(|| {
                            MobileCoreError::Security("Registry verified count overflow".into())
                        })?;
                    }
                }
            }
            drop(chunks);
            if total_chunks == 0 {
                return Err(MobileCoreError::Security(
                    "Registry verified an empty chunk ledger".into(),
                ));
            }

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(record.operation_id.as_str())?
                .ok_or_else(|| registry_admission("Registry chunk lost its operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            if operation.active_transfer_nonce.as_deref() != Some(record.transfer_nonce.as_str())
                || !matches!(
                    operation.state,
                    RegistryOperationState::TransferQueued
                        | RegistryOperationState::Downloading
                        | RegistryOperationState::BytesComplete
                )
            {
                return Err(registry_admission(
                    "Registry verified bytes do not belong to the active operation",
                ));
            }
            operation.state = if total_chunks == verified_chunks {
                RegistryOperationState::BytesComplete
            } else {
                RegistryOperationState::Downloading
            };
            let bytes = encode(&operation)?;
            operations.insert(record.operation_id.as_str(), bytes.as_slice())?;
            drop(operations);
            validate_registry_chunk_eligibility(&write, &record)?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }

    #[cfg(test)]
    pub(crate) fn registry_chunk_test_paths(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
    ) -> Result<(PathBuf, PathBuf), MobileCoreError> {
        let record = self
            .registry_chunk(transfer_nonce, artifact_role, chunk_index)?
            .ok_or_else(|| registry_admission("unknown Registry chunk binding"))?;
        let paths = self.registry_chunk_paths(&record)?;
        Ok((paths.partial, paths.verified))
    }

    pub fn prepare_transfer(
        &self,
        record: &TransferLandingRecord,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "transfer_nonce",
            &record.transfer_nonce,
            budgets.max_transfer_nonce_bytes,
        )?;
        require_bounded(
            "operation_id",
            &record.operation_id,
            budgets.max_operation_id_bytes,
        )?;
        require_bounded(
            "artifact_role",
            &record.artifact_role,
            budgets.max_artifact_role_bytes,
        )?;
        if let Some(os_transfer_id) = &record.os_transfer_id {
            require_bounded(
                "os_transfer_id",
                os_transfer_id,
                budgets.max_os_transfer_id_bytes,
            )?;
        }
        validate_hash(&record.expected_hash)?;
        let bytes = encode(record)?;
        let write = self.database.begin_write()?;
        {
            let is_zero_byte_callback_probe = record.operation_id == CALLBACK_FENCE_PROBE_OPERATION
                && record.release_id == CALLBACK_FENCE_PROBE_RELEASE
                && record.artifact_role == CALLBACK_FENCE_PROBE_ROLE
                && record.expected_length == 0;
            if !is_zero_byte_callback_probe {
                return Err(registry_admission(
                    "caller-authored Registry landing records are forbidden; prepare the signed chunk ledger",
                ));
            }
            let mut table = write.open_table(TRANSFER_LANDING)?;
            let current = table
                .get(record.transfer_nonce.as_str())?
                .map(|existing| decode::<TransferLandingRecord>(existing.value()))
                .transpose()?;
            if let Some(current) = current {
                if stable_transfer_identity(&current) != stable_transfer_identity(record) {
                    return Err(MobileCoreError::InvalidArgument(
                        "transfer nonce is already bound to another stable identity".into(),
                    ));
                }
            } else {
                table.insert(record.transfer_nonce.as_str(), bytes.as_slice())?;
            }
        }
        write.commit()?;
        Ok(())
    }

    pub fn bind_os_transfer(
        &self,
        transfer_nonce: &str,
        os_transfer_id: &str,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "os_transfer_id",
            os_transfer_id,
            budgets.max_os_transfer_id_bytes,
        )?;
        self.update_transfer(transfer_nonce, |record| {
            record.os_transfer_id = Some(os_transfer_id.to_owned());
            Ok(())
        })
    }

    pub fn claim_transfer_callback(
        &self,
        transfer_nonce: &str,
        receiving_generation: u64,
        callback_sequence: u64,
    ) -> Result<TransferLandingRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let process_table = write.open_table(PROCESS_GENERATIONS)?;
            let current = required_current_process(&process_table)?;
            ensure_generation(current.generation, receiving_generation)?;
            drop(process_table);

            let mut transfers = write.open_table(TRANSFER_LANDING)?;
            let existing = transfers
                .get(transfer_nonce)?
                .ok_or_else(|| MobileCoreError::UnknownTransfer(transfer_nonce.to_owned()))?;
            let mut record: TransferLandingRecord = decode(existing.value())?;
            drop(existing);
            if record.receiving_process_generation == Some(receiving_generation) {
                if let Some(current_sequence) = record.app_assigned_callback_sequence {
                    if callback_sequence <= current_sequence {
                        return Err(MobileCoreError::StaleCallbackSequence {
                            received: callback_sequence,
                            current: current_sequence,
                        });
                    }
                }
            }
            record.receiving_process_generation = Some(receiving_generation);
            record.app_assigned_callback_sequence = Some(callback_sequence);
            let bytes = encode(&record)?;
            transfers.insert(transfer_nonce, bytes.as_slice())?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }

    pub fn mark_transfer_landed(
        &self,
        transfer_nonce: &str,
        receiving_generation: u64,
        callback_sequence: u64,
    ) -> Result<TransferLandingRecord, MobileCoreError> {
        let claimed =
            self.claim_transfer_callback(transfer_nonce, receiving_generation, callback_sequence)?;
        self.update_transfer(transfer_nonce, |record| {
            if record.receiving_process_generation != Some(receiving_generation)
                || record.app_assigned_callback_sequence != Some(callback_sequence)
            {
                return Err(MobileCoreError::StaleCallbackSequence {
                    received: callback_sequence,
                    current: record.app_assigned_callback_sequence.unwrap_or(0),
                });
            }
            record.landed = true;
            Ok(())
        })?;
        Ok(TransferLandingRecord {
            landed: true,
            ..claimed
        })
    }

    pub fn transfer(
        &self,
        transfer_nonce: &str,
    ) -> Result<Option<TransferLandingRecord>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(TRANSFER_LANDING)?;
        table
            .get(transfer_nonce)?
            .map(|value| decode(value.value()))
            .transpose()
    }

    fn update_registry_operation(
        &self,
        operation_id: &str,
        update: impl FnOnce(&mut RegistryOperationRecord) -> Result<(), MobileCoreError>,
    ) -> Result<RegistryOperationRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let mut table = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = table
                .get(operation_id)?
                .ok_or_else(|| registry_admission("unknown Registry Init operation"))?;
            let mut record: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);
            update(&mut record)?;
            let bytes = encode(&record)?;
            table.insert(operation_id, bytes.as_slice())?;
            updated = record;
        }
        write.commit()?;
        Ok(updated)
    }

    fn update_registry_transfer_schedule(
        &self,
        transfer_nonce: &str,
        update: impl FnOnce(
            &mut RegistryOperationRecord,
            &mut RegistryTransferScheduleRecord,
            u64,
        ) -> Result<(), MobileCoreError>,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        let write = self.database.begin_write()?;
        let updated;
        {
            let process_table = write.open_table(PROCESS_GENERATIONS)?;
            let process_generation = required_current_process(&process_table)?.generation;
            drop(process_table);

            let mut schedules = write.open_table(REGISTRY_TRANSFER_SCHEDULES)?;
            let existing = schedules
                .get(transfer_nonce)?
                .ok_or_else(|| MobileCoreError::UnknownTransfer(transfer_nonce.to_owned()))?;
            let mut schedule: RegistryTransferScheduleRecord = decode(existing.value())?;
            drop(existing);

            let mut operations = write.open_table(REGISTRY_OPERATIONS)?;
            let existing = operations
                .get(schedule.operation_id.as_str())?
                .ok_or_else(|| registry_admission("Registry transfer lost its operation"))?;
            let mut operation: RegistryOperationRecord = decode(existing.value())?;
            drop(existing);

            update(&mut operation, &mut schedule, process_generation)?;
            let operation_bytes = encode(&operation)?;
            operations.insert(schedule.operation_id.as_str(), operation_bytes.as_slice())?;
            drop(operations);

            let schedule_bytes = encode(&schedule)?;
            schedules.insert(transfer_nonce, schedule_bytes.as_slice())?;
            updated = schedule;
        }
        write.commit()?;
        Ok(updated)
    }

    fn update_transfer(
        &self,
        transfer_nonce: &str,
        update: impl FnOnce(&mut TransferLandingRecord) -> Result<(), MobileCoreError>,
    ) -> Result<(), MobileCoreError> {
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(TRANSFER_LANDING)?;
            let existing = table
                .get(transfer_nonce)?
                .ok_or_else(|| MobileCoreError::UnknownTransfer(transfer_nonce.to_owned()))?;
            let mut record: TransferLandingRecord = decode(existing.value())?;
            drop(existing);
            update(&mut record)?;
            let bytes = encode(&record)?;
            table.insert(transfer_nonce, bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }
}

fn required_current_process(
    table: &impl ReadableTable<&'static str, &'static [u8]>,
) -> Result<ProcessGenerationRecord, MobileCoreError> {
    table
        .get(CURRENT_PROCESS_KEY)?
        .map(|value| decode(value.value()))
        .transpose()?
        .ok_or_else(|| MobileCoreError::Storage("process generation is not initialized".into()))
}

fn ensure_generation(current: u64, received: u64) -> Result<(), MobileCoreError> {
    if current == received {
        Ok(())
    } else {
        Err(MobileCoreError::StaleGeneration { received, current })
    }
}

const fn valid_onboarding_transition(current: OnboardingCursor, next: OnboardingCursor) -> bool {
    matches!(
        (current, next),
        (OnboardingCursor::Welcome, OnboardingCursor::Welcome)
            | (OnboardingCursor::Welcome, OnboardingCursor::Preflight)
            | (OnboardingCursor::Preflight, OnboardingCursor::Welcome)
            | (OnboardingCursor::Preflight, OnboardingCursor::Preflight)
            | (OnboardingCursor::Preflight, OnboardingCursor::Identity)
            | (OnboardingCursor::Identity, OnboardingCursor::Preflight)
            | (OnboardingCursor::Identity, OnboardingCursor::Identity)
            | (OnboardingCursor::Identity, OnboardingCursor::Security)
            | (OnboardingCursor::Security, OnboardingCursor::Identity)
            | (OnboardingCursor::Security, OnboardingCursor::Security)
            | (OnboardingCursor::Security, OnboardingCursor::InitHandoff)
            | (OnboardingCursor::InitHandoff, OnboardingCursor::Security)
            | (OnboardingCursor::InitHandoff, OnboardingCursor::InitHandoff)
            | (OnboardingCursor::InitHandoff, OnboardingCursor::LimitedHome)
            | (OnboardingCursor::LimitedHome, OnboardingCursor::LimitedHome)
    )
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, MobileCoreError> {
    serde_json::to_vec(value).map_err(Into::into)
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, MobileCoreError> {
    serde_json::from_slice(bytes).map_err(Into::into)
}

struct RegistryChunkPaths {
    directory: PathBuf,
    partial: PathBuf,
    verified: PathBuf,
}

fn chunk_key(transfer_nonce: &str, artifact_role: u8, chunk_index: u32) -> String {
    format!("{transfer_nonce}/{artifact_role}/{chunk_index:010}")
}

fn registry_chunk_identity(record: &RegistryChunkRecord) -> (&str, &str, &str, u8, u32, &str, u64) {
    (
        record.transfer_nonce.as_str(),
        record.operation_id.as_str(),
        record.release_id.as_str(),
        record.artifact_role,
        record.chunk_index,
        record.expected_hash.as_str(),
        record.expected_length,
    )
}

fn validate_registry_chunk_eligibility(
    write: &redb::WriteTransaction,
    record: &RegistryChunkRecord,
) -> Result<(), MobileCoreError> {
    let schedules = write.open_table(REGISTRY_TRANSFER_SCHEDULES)?;
    let schedule = schedules
        .get(record.transfer_nonce.as_str())?
        .map(|value| decode::<RegistryTransferScheduleRecord>(value.value()))
        .transpose()?
        .ok_or_else(|| registry_admission("Registry chunk lost its transfer schedule"))?;
    if schedule.state != RegistryTransferScheduleState::TransferAdopted
        || schedule.operation_id != record.operation_id
        || schedule.release_id != record.release_id
    {
        return Err(registry_admission(
            "Registry chunk transfer is no longer adopted or exact",
        ));
    }
    drop(schedules);

    let catalog = write.open_table(REGISTRY_RELEASE_CATALOG)?;
    let release = catalog
        .get(record.release_id.as_str())?
        .map(|value| decode::<RegistryReleaseCatalogRecord>(value.value()))
        .transpose()?
        .ok_or_else(|| registry_admission("Registry chunk lost its signed release"))?;
    if release.state == RegistryReleaseState::Revoked
        || release.manifest_digest != schedule.manifest_digest
        || release.trust_profile_digest != schedule.trust_profile_digest
    {
        return Err(registry_admission(
            "revoked or rebound Registry bytes cannot be landed",
        ));
    }
    Ok(())
}

fn registry_chunk_hasher(record: &RegistryChunkRecord) -> Result<blake3::Hasher, MobileCoreError> {
    let exact_length = u32::try_from(record.expected_length).map_err(|_| {
        MobileCoreError::Security("Registry chunk length exceeds the signed u32 profile".into())
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(REGISTRY_CHUNK_DOMAIN);
    hasher.update(&[record.artifact_role]);
    hasher.update(&record.chunk_index.to_le_bytes());
    hasher.update(&exact_length.to_le_bytes());
    Ok(hasher)
}

fn bounded_regular_file_length(path: &Path, maximum: u64) -> Result<u64, MobileCoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(registry_io("inspect Registry chunk", error)),
    };
    if !metadata.file_type().is_file() {
        return Err(MobileCoreError::Security(
            "Registry chunk landing path is not a regular file".into(),
        ));
    }
    if metadata.len() > maximum {
        return Err(MobileCoreError::Security(
            "Registry chunk landing exceeds the signed exact length".into(),
        ));
    }
    Ok(metadata.len())
}

fn rehash_registry_chunk_prefix(
    path: &Path,
    record: &RegistryChunkRecord,
) -> Result<(blake3::Hasher, u64), MobileCoreError> {
    let mut hasher = registry_chunk_hasher(record)?;
    let length = bounded_regular_file_length(path, record.expected_length)?;
    if length == 0 {
        return Ok((hasher, 0));
    }
    let mut file =
        File::open(path).map_err(|error| registry_io("open Registry partial for rehash", error))?;
    let mut observed = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| registry_io("rehash Registry partial", error))?;
        if read == 0 {
            break;
        }
        observed = observed
            .checked_add(read as u64)
            .ok_or_else(|| MobileCoreError::Security("Registry rehash length overflow".into()))?;
        if observed > record.expected_length {
            return Err(MobileCoreError::Security(
                "Registry partial grew beyond the signed length".into(),
            ));
        }
        hasher.update(&buffer[..read]);
    }
    Ok((hasher, observed))
}

fn copy_exact_bounded<R: Read>(
    source: &mut R,
    output: &mut File,
    hasher: &mut blake3::Hasher,
    maximum: u64,
) -> Result<u64, MobileCoreError> {
    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    while copied < maximum {
        let wanted = usize::try_from((maximum - copied).min(buffer.len() as u64))
            .map_err(|_| MobileCoreError::Security("Registry read bound overflow".into()))?;
        let read = source
            .read(&mut buffer[..wanted])
            .map_err(|error| registry_io("read Registry chunk source", error))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| registry_io("write Registry partial chunk", error))?;
        hasher.update(&buffer[..read]);
        copied = copied
            .checked_add(read as u64)
            .ok_or_else(|| MobileCoreError::Security("Registry copy length overflow".into()))?;
    }
    Ok(copied)
}

fn verify_registry_chunk_file(
    path: &Path,
    record: &RegistryChunkRecord,
) -> Result<(), MobileCoreError> {
    let length = bounded_regular_file_length(path, record.expected_length)?;
    if length != record.expected_length {
        return Err(MobileCoreError::Security(
            "Registry chunk file length does not match the signed manifest".into(),
        ));
    }
    let (hasher, observed) = rehash_registry_chunk_prefix(path, record)?;
    if observed != record.expected_length
        || hex::encode(hasher.finalize().as_bytes()) != record.expected_hash
    {
        return Err(MobileCoreError::Security(
            "Registry chunk file hash does not match the signed manifest".into(),
        ));
    }
    Ok(())
}

fn reset_registry_partial(path: &Path) -> Result<(), MobileCoreError> {
    match fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(registry_io("remove rejected Registry partial", error)),
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), MobileCoreError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| registry_io("sync Registry landing directory", error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), MobileCoreError> {
    Ok(())
}

fn registry_io(context: &str, error: std::io::Error) -> MobileCoreError {
    MobileCoreError::Storage(format!("{context}: {error}"))
}

fn channel_highwater_key(channel_id: &str) -> String {
    format!("channel/{channel_id}")
}

fn decode_digest_hex(value: &str, label: &str) -> Result<[u8; 32], MobileCoreError> {
    let bytes = hex::decode(value)
        .map_err(|_| MobileCoreError::Storage(format!("{label} is not canonical hex")))?;
    bytes
        .try_into()
        .map_err(|_| MobileCoreError::Storage(format!("{label} must contain 32 bytes")))
}

fn require_bounded(name: &str, value: &str, max: usize) -> Result<(), MobileCoreError> {
    if value.is_empty() || value.len() > max {
        return Err(MobileCoreError::InvalidArgument(format!(
            "{name} must contain between 1 and {max} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), MobileCoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MobileCoreError::InvalidArgument(
            "expected_hash must be a 32-byte lowercase or uppercase hex digest".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn registry_transfer_request_matches(
    schedule: &RegistryTransferScheduleRecord,
    operation_id: &str,
    manifest_digest: &str,
    platform: RegistryTransferPlatform,
    request_fingerprint: &str,
    transport_descriptor_digest: &str,
    expected_total_bytes: u64,
) -> bool {
    schedule.operation_id == operation_id
        && schedule.manifest_digest == manifest_digest
        && schedule.platform == platform
        && schedule.request_fingerprint == request_fingerprint
        && schedule.transport_descriptor_digest == transport_descriptor_digest
        && schedule.expected_total_bytes == expected_total_bytes
}

fn android_registry_job_id(
    operation_id: &str,
    release_id: &str,
    request_fingerprint: &str,
    transfer_nonce: &str,
) -> u32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:android-registry-uidt-job-id:1\0");
    for value in [
        operation_id.as_bytes(),
        release_id.as_bytes(),
        request_fingerprint.as_bytes(),
        transfer_nonce.as_bytes(),
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    let digest = hasher.finalize();
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&digest.as_bytes()[..4]);
    (u32::from_le_bytes(raw) & 0x7fff_ffff).max(1)
}

fn validate_installation_authority(
    authority: &InstallationAuthorityRecord,
) -> Result<(), MobileCoreError> {
    if authority.profile_version != 1 {
        return Err(MobileCoreError::Security(
            "unsupported installation authority profile".into(),
        ));
    }
    for (name, value) in [
        ("installation_epoch", &authority.installation_epoch),
        (
            "installation_instance_nonce",
            &authority.installation_instance_nonce,
        ),
        ("binding_digest", &authority.binding_digest),
        ("node_id", &authority.node_id),
        ("feed_id", &authority.feed_id),
        ("actor_id", &authority.actor_id),
    ] {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MobileCoreError::Security(format!(
                "{name} must be a 32-byte hex value"
            )));
        }
    }
    if authority.node_id == authority.feed_id
        || authority.node_id == authority.actor_id
        || authority.feed_id == authority.actor_id
    {
        return Err(MobileCoreError::Security(
            "public signer domains must be independent".into(),
        ));
    }
    Ok(())
}

fn validate_audit_code(name: &str, value: &str) -> Result<(), MobileCoreError> {
    if value.is_empty()
        || value.len() > 64
        || !value.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        })
    {
        return Err(MobileCoreError::Security(format!(
            "{name} must be bounded uppercase ASCII"
        )));
    }
    Ok(())
}

fn stable_transfer_identity(
    record: &TransferLandingRecord,
) -> (&str, &str, &str, &str, u32, &str, u64) {
    (
        record.transfer_nonce.as_str(),
        record.operation_id.as_str(),
        record.release_id.as_str(),
        record.artifact_role.as_str(),
        record.chunk_index,
        record.expected_hash.as_str(),
        record.expected_length,
    )
}
