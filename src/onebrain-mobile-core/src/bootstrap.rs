use std::{
    fs,
    path::{Path, PathBuf},
};

use redb::{Database, ReadableTable, TableDefinition};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    registry_admission::{
        compare_bound_generation, compare_bound_release, registry_admission,
        verify_registry_target, VerifiedRegistryTarget,
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
pub struct RegistryChunkRecord {
    pub operation_id: String,
    pub chunk_index: u32,
    pub expected_hash: String,
    pub expected_length: u64,
    pub state: String,
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

    pub fn upsert_registry_chunk(
        &self,
        record: &RegistryChunkRecord,
        budgets: &ResourceBudgets,
    ) -> Result<(), MobileCoreError> {
        require_bounded(
            "operation_id",
            &record.operation_id,
            budgets.max_operation_id_bytes,
        )?;
        validate_hash(&record.expected_hash)?;
        let key = chunk_key(&record.operation_id, record.chunk_index);
        let bytes = encode(record)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(REGISTRY_CHUNKS)?;
            table.insert(key.as_str(), bytes.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn registry_chunk(
        &self,
        operation_id: &str,
        chunk_index: u32,
    ) -> Result<Option<RegistryChunkRecord>, MobileCoreError> {
        let key = chunk_key(operation_id, chunk_index);
        let read = self.database.begin_read()?;
        let table = read.open_table(REGISTRY_CHUNKS)?;
        table
            .get(key.as_str())?
            .map(|value| decode(value.value()))
            .transpose()
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
                let operations = write.open_table(REGISTRY_OPERATIONS)?;
                let operation = operations
                    .get(record.operation_id.as_str())?
                    .map(|value| decode::<RegistryOperationRecord>(value.value()))
                    .transpose()?
                    .ok_or_else(|| {
                        registry_admission("transfer has no durable Registry operation")
                    })?;
                if !operation.state.permits_transfer_preparation()
                    || operation.release_id != record.release_id
                {
                    return Err(registry_admission(
                        "large Registry transfer is forbidden before exact admission",
                    ));
                }
                drop(operations);
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

fn chunk_key(operation_id: &str, chunk_index: u32) -> String {
    format!("{operation_id}/{chunk_index:010}")
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
