use std::{collections::BTreeMap, io::Read, sync::Arc};

use crate::{
    activation::{EnergyPolicyEvaluation, LifecyclePressureKind},
    ai_tools::{
        ActiveModelRouteRecord, AssistantThreadRecord, CloudDisclosurePreview,
        Llm0DeterministicAssistant, LlmStreamAssembler, MobileLlmProvider, ModelCatalogRecord,
        ModelReleaseManifest, ModelRouteKind, ToolExecutionReceiptView, ToolProposalView,
    },
    backup::{
        create_encrypted_backup_v2, export_reviewed_scoped_bundle, restore_encrypted_backup_v2,
        MobileBackupReceipt, MobileRestoreReceipt, ScopedExportBundle,
    },
    knowledge::{
        EncodingDraftRecord, KuLocalOrganizationRecord, LocalNeighborhoodProjection,
        LocalReadOnlyKqlView, MobileKuDetailView, MobileKuSummary, PaginatedKuPage,
        PreparedKuPreviewView,
    },
    media_staging::{
        DerivedCacheGcReport, DerivedCacheRecord, MediaVerifiedRangeView, SignedMediaManifest,
    },
    migrations::{
        build_redacted_diagnostics_report, MigrationCoordinator, RedactedDiagnosticsReport,
    },
    notifications::{
        ActivityInboxEntry, ActivityInboxStore, NotificationChannelKind, NotificationPolicyConfig,
        NotificationReceiptRecord, ReversibleNotificationAction,
    },
    obp_reunion::{
        evaluate_feature_gate_status, FeatureGateStatusView, MatchProposalDisposition,
        MatchTargetLifecycle, PrivateMatchTarget, PublicCandidateAffordance,
        QuarantinedReunionProposal, ReunionCoverageClass,
    },
    registry_activation::{
        activate_verified_registry_release, derive_registry_readiness,
        gc_retired_registry_releases, list_registry_concepts, recover_or_open_active_registry,
        rollback_to_previous_registry_release, DerivedRegistryReadiness,
        RegistryActivationReceiptRecord, RegistryConceptView, RegistryGarbageCollectionReport,
        RegistryGeneration,
    },
    registry_source_plan::{
        execute_multi_provider_registry_transfer, MultiProviderTransferOutcome,
        RegistryByteProvider, RegistrySourcePlan, ResourceSnapshot,
    },
    run_signed_local_kql_smoke, ActivationArbiter, ActivationPhase, AppLockPolicy, ExecutionGrant,
    ExecutionGrantKind, MediaStageReceipt, MobileCoreError, MobileFeatureFlags, NetworkScope,
    OnboardingCursor, OwnedMediaSummary, RawDraftReceipt, RegistryCapacityPlan,
    RegistryChunkRecord, RegistryChunkSourceTarget, RegistryChunkWriteProgress,
    RegistryChunkWriteSession, RegistryChunkWriteStart, RegistryLandingProgress,
    RegistryLimitedReceipt, RegistryNetworkPolicy, RegistryOperationRecord, RegistryOperationState,
    RegistryReleaseCatalogRecord, RegistrySourceKind, RegistryTransferPlatform,
    RegistryTransferScheduleRecord, RegistryTrustProfile, ResourceBudgets, RuntimeServices,
    SecureIdentitySession, SecurityBootstrapMaterial, SecuritySessionState, ShareSpoolSummary,
    TransferLandingRecord, MOBILE_RUNTIME_PROFILE_VERSION,
};

const FOREGROUND_GRANT_ID: &str = "native.foreground";
const CALLBACK_FENCE_PROBE_NONCE: &str = "mob02.callback-fence.probe";
const PROBE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const REGISTRY_DIRECT_TRANSFER_WINDOW_BYTES: u64 = 8_388_608;
const REGISTRY_VERIFICATION_WORKSPACE_BYTES: u64 = 64 * 1024 * 1024;
const REGISTRY_CATALOG_GROWTH_BYTES: u64 = 8 * 1024 * 1024;
const REGISTRY_MINIMUM_SAFETY_RESERVE_BYTES: u64 = 1_610_612_736;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryInitPlan {
    pub operation: RegistryOperationRecord,
    pub release: RegistryReleaseCatalogRecord,
    pub capacity: RegistryCapacityPlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MobileRuntimeSnapshot {
    pub profile_version: &'static str,
    pub process_generation: u64,
    pub activation_phase: ActivationPhase,
    pub active_grant_count: usize,
    pub recovered_unclean_start: bool,
    pub bootstrap_store_opened: bool,
    pub registry_state: &'static str,
    pub local_kql_fixture_verified: bool,
    pub private_planner_verified: bool,
    pub local_kql_rows: usize,
    pub llm_provider_id: &'static str,
    pub llm_available: bool,
    pub signer_available: bool,
    pub connectivity_online: bool,
    pub background_scheduler_available: bool,
    pub stale_callback_rejected: bool,
    pub secure_profile_active: bool,
    pub installation_binding_verified: bool,
    pub installation_created: bool,
    pub security_session_state: SecuritySessionState,
    pub private_vault_ready: bool,
    pub identity_domains_separated: bool,
    pub privacy_defaults_fail_safe: bool,
    pub redacted_history_ready: bool,
    pub encrypted_raw_draft_count: u64,
    pub pending_share_spool_count: u64,
    pub staged_verified_media_count: u64,
    pub owned_original_media_count: u64,
    pub onboarding_cursor: OnboardingCursor,
}

pub struct MobileRuntimeFacade {
    services: RuntimeServices,
    flags: MobileFeatureFlags,
    budgets: ResourceBudgets,
    store: crate::BootstrapStore,
    arbiter: ActivationArbiter,
    recovered_unclean_start: bool,
    local_kql_fixture_verified: bool,
    private_planner_verified: bool,
    local_kql_rows: usize,
    stale_callback_rejected: bool,
    secure_identity: Option<SecureIdentitySession>,
    active_registry_generation: Option<Arc<RegistryGeneration>>,
    retained_registry_generations: Vec<Arc<RegistryGeneration>>,
    activity_inbox: ActivityInboxStore,
    migrations: MigrationCoordinator,
    kill_switch_engaged: bool,
    installation_binding_verified: bool,
    installation_created: bool,
    quiesced: bool,
}

impl MobileRuntimeFacade {
    pub fn open_bootstrap_only(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
    ) -> Result<Self, MobileCoreError> {
        Self::open_internal(services, flags, budgets, None, true)
    }

    pub fn open_secured(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
        material: SecurityBootstrapMaterial,
        lock_policy: AppLockPolicy,
    ) -> Result<Self, MobileCoreError> {
        Self::open_internal(
            services,
            flags,
            budgets,
            Some((material, lock_policy)),
            true,
        )
    }

    pub fn open_feature_secured(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
        material: SecurityBootstrapMaterial,
        lock_policy: AppLockPolicy,
    ) -> Result<Self, MobileCoreError> {
        Self::open_internal(
            services,
            flags,
            budgets,
            Some((material, lock_policy)),
            false,
        )
    }

    fn open_internal(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
        security: Option<(SecurityBootstrapMaterial, AppLockPolicy)>,
        enforce_bootstrap_flags: bool,
    ) -> Result<Self, MobileCoreError> {
        if enforce_bootstrap_flags {
            flags.validate_bootstrap_only()?;
            if services.connectivity.is_online() {
                return Err(MobileCoreError::InvalidArgument(
                    "BootstrapOnly services must report offline connectivity".into(),
                ));
            }
            if services.llm.is_available() {
                return Err(MobileCoreError::InvalidArgument(
                    "BootstrapOnly services cannot expose an available LLM".into(),
                ));
            }
        } else {
            flags.validate_feature_runtime()?;
        }
        budgets.validate()?;
        let path = services.paths.bootstrap_database_path();
        let store = services.storage.open(&path)?;
        let now = services.clock.monotonic_millis();
        let (secure_identity, installation_binding_verified, installation_created) =
            if let Some((material, policy)) = security {
                let authority = material.installation_authority();
                let vault_path = services.paths.private_vault_database_path();
                let draft_path = services.paths.private_draft_database_path();
                let media_database_path = services.paths.private_media_staging_database_path();
                let media_root = services.paths.private_media_staging_root();
                let session = SecureIdentitySession::open(
                    material,
                    &vault_path,
                    &draft_path,
                    &media_database_path,
                    &media_root,
                    now,
                    policy,
                )?;
                let created = store.bind_installation_authority(&authority)?;
                (Some(session), true, created)
            } else {
                if store.installation_authority()?.is_some() {
                    return Err(MobileCoreError::UnexpectedRestore(
                        "secured authority cannot be opened by an unsecured runtime".into(),
                    ));
                }
                (None, false, false)
            };
        let process = store.start_process(now)?;
        let mut arbiter = ActivationArbiter::starting(process.generation, &budgets);
        arbiter.register_grant(
            ExecutionGrant {
                grant_id: FOREGROUND_GRANT_ID.into(),
                process_generation: process.generation,
                kind: ExecutionGrantKind::Foreground,
                user_visible: true,
                deadline_monotonic_ms: None,
                network_scope: NetworkScope::None,
            },
            now,
        )?;
        let kql = run_signed_local_kql_smoke(&budgets)?;
        let stale_callback_rejected = verify_callback_fence(&store, process.generation, &budgets)?;
        if secure_identity.is_some() {
            store.replace_privacy_policy(&store.privacy_policy()?)?;
            store.append_security_history(
                process.generation,
                now,
                "SECURE_SESSION_OPENED",
                "PRIVATE_NODE",
                true,
            )?;
        }
        let data_root = services.paths.data_root();
        let activity_inbox =
            ActivityInboxStore::open(&data_root.join("private-activity-inbox.redb"))?;
        let migrations = MigrationCoordinator::open(&data_root.join("private-migrations.redb"))?;
        let active_registry_generation = recover_or_open_active_registry(&store).ok().flatten();
        let mut retained_registry_generations = Vec::new();
        if let Some(gen) = &active_registry_generation {
            retained_registry_generations.push(Arc::clone(gen));
        }
        services.telemetry.record("mobile_runtime_started");
        Ok(Self {
            services,
            flags,
            budgets,
            store,
            arbiter,
            recovered_unclean_start: process.recovered_unclean_start,
            local_kql_fixture_verified: kql.signature_verified && kql.query_scope_local,
            private_planner_verified: kql.private_planner_verified,
            local_kql_rows: kql.rows,
            stale_callback_rejected,
            secure_identity,
            active_registry_generation,
            retained_registry_generations,
            activity_inbox,
            migrations,
            kill_switch_engaged: false,
            installation_binding_verified,
            installation_created,
            quiesced: false,
        })
    }

    pub fn snapshot(&self) -> MobileRuntimeSnapshot {
        let derived_readiness =
            derive_registry_readiness(&self.store, self.active_registry_generation.as_ref());
        let active_model_route = self
            .secure_identity
            .as_ref()
            .and_then(|session| session.tool_orchestrator().ok())
            .and_then(|orch| orch.active_model_route().ok())
            .unwrap_or_default();
        let (llm_provider_id, llm_available) =
            if active_model_route.route_kind != ModelRouteKind::None {
                ("activated_model", true)
            } else {
                (
                    self.services.llm.provider_id(),
                    self.services.llm.is_available(),
                )
            };
        MobileRuntimeSnapshot {
            profile_version: MOBILE_RUNTIME_PROFILE_VERSION,
            process_generation: self.arbiter.process_generation(),
            activation_phase: self.arbiter.phase(),
            active_grant_count: self.arbiter.active_grant_count(),
            recovered_unclean_start: self.recovered_unclean_start,
            bootstrap_store_opened: true,
            registry_state: derived_readiness.as_snapshot_str(),
            local_kql_fixture_verified: self.local_kql_fixture_verified,
            private_planner_verified: self.private_planner_verified,
            local_kql_rows: self.local_kql_rows,
            llm_provider_id,
            llm_available,
            signer_available: self.secure_identity.as_ref().is_some_and(|session| {
                session.session_is_eligible(self.services.clock.monotonic_millis())
            }) || self.services.signer.is_available(),
            connectivity_online: self.services.connectivity.is_online(),
            background_scheduler_available: self
                .services
                .scheduler
                .background_execution_available(),
            stale_callback_rejected: self.stale_callback_rejected,
            secure_profile_active: self.secure_identity.is_some(),
            installation_binding_verified: self.installation_binding_verified,
            installation_created: self.installation_created,
            security_session_state: self
                .secure_identity
                .as_ref()
                .map_or(SecuritySessionState::Locked, SecureIdentitySession::state),
            private_vault_ready: self
                .secure_identity
                .as_ref()
                .is_some_and(SecureIdentitySession::private_vault_ready),
            identity_domains_separated: self
                .secure_identity
                .as_ref()
                .is_some_and(|session| session.public_identities().domains_are_independent()),
            privacy_defaults_fail_safe: self.store.privacy_policy().is_ok_and(|policy| {
                policy.private_local_default
                    && policy.private_shared_requires_confirmation
                    && policy.public_candidate_requires_confirmation
                    && policy.public_accepted_requires_confirmation
            }),
            redacted_history_ready: self
                .store
                .recent_security_history(1)
                .is_ok_and(|records| self.secure_identity.is_none() || !records.is_empty()),
            encrypted_raw_draft_count: self
                .secure_identity
                .as_ref()
                .and_then(|session| session.raw_draft_count().ok())
                .unwrap_or(0),
            pending_share_spool_count: self
                .secure_identity
                .as_ref()
                .and_then(|session| session.pending_share_spool_count().ok())
                .unwrap_or(0),
            staged_verified_media_count: self
                .secure_identity
                .as_ref()
                .and_then(|session| session.staged_verified_media_count().ok())
                .unwrap_or(0),
            owned_original_media_count: self
                .secure_identity
                .as_ref()
                .and_then(|session| session.owned_media_count().ok())
                .unwrap_or(0),
            onboarding_cursor: self.store.onboarding_cursor().unwrap_or_default(),
        }
    }

    pub fn flags(&self) -> &MobileFeatureFlags {
        &self.flags
    }

    pub fn budgets(&self) -> &ResourceBudgets {
        &self.budgets
    }

    pub fn lock_private_node(&mut self) -> Result<(), MobileCoreError> {
        let Some(session) = self.secure_identity.as_mut() else {
            return Err(MobileCoreError::Security(
                "the runtime has no platform-protected identity session".into(),
            ));
        };
        if session.state() == SecuritySessionState::Locked {
            return Ok(());
        }
        session.lock();
        self.store.append_security_history(
            self.arbiter.process_generation(),
            self.services.clock.monotonic_millis(),
            "SECURE_SESSION_LOCKED",
            "PRIVATE_NODE",
            true,
        )?;
        self.services.telemetry.record("mobile_private_node_locked");
        Ok(())
    }

    pub fn save_raw_text_draft(
        &mut self,
        content_language: &str,
        content_utf8: &[u8],
    ) -> Result<RawDraftReceipt, MobileCoreError> {
        if matches!(
            self.derived_registry_readiness(),
            DerivedRegistryReadiness::RegistryDegraded { .. }
        ) {
            return Err(MobileCoreError::InvalidArgument(
                "raw draft intake is suspended while Registry authority is degraded".into(),
            ));
        }
        let now = self.services.clock.monotonic_millis();
        let receipt = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .save_raw_text_draft(content_language, content_utf8, now)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "PRIVATE_RAW_DRAFT_SAVED",
            "CAPTURE",
            true,
        )?;
        self.services.telemetry.record("mobile_raw_draft_saved");
        Ok(receipt)
    }

    pub fn enqueue_shared_text(
        &mut self,
        callback_token: &str,
        mime_type: &str,
        content_utf8: &[u8],
    ) -> Result<ShareSpoolSummary, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let receipt = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .enqueue_shared_text(callback_token, mime_type, content_utf8, now)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "PRIVATE_SHARE_SPOOL_LANDED",
            "CAPTURE",
            true,
        )?;
        self.services.telemetry.record("mobile_share_spool_landed");
        Ok(receipt)
    }

    pub fn pending_share_spools(
        &self,
        limit: usize,
    ) -> Result<Vec<ShareSpoolSummary>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .pending_share_spools(limit)
    }

    pub fn import_shared_text(
        &mut self,
        spool_ref: &str,
        content_language: &str,
    ) -> Result<RawDraftReceipt, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let receipt = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .import_shared_text(spool_ref, content_language, now)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "PRIVATE_SHARE_SPOOL_IMPORTED",
            "CAPTURE",
            true,
        )?;
        self.services
            .telemetry
            .record("mobile_share_spool_imported");
        Ok(receipt)
    }

    pub fn start_media_stage(
        &mut self,
        requested_class: &str,
        declared_mime_type: &str,
    ) -> Result<String, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let source_ref = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .start_media_stage(requested_class, declared_mime_type, now)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "PRIVATE_MEDIA_STAGE_STARTED",
            "CAPTURE",
            true,
        )?;
        self.services.telemetry.record("mobile_media_stage_started");
        Ok(source_ref)
    }

    pub fn append_media_stage(
        &mut self,
        source_ref: &str,
        chunk: &[u8],
    ) -> Result<(), MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .append_media_stage(source_ref, chunk, now)
    }

    pub fn finish_media_stage(
        &mut self,
        source_ref: &str,
    ) -> Result<MediaStageReceipt, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let receipt = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .finish_media_stage(source_ref, now)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "PRIVATE_MEDIA_STAGE_VERIFIED",
            "CAPTURE",
            true,
        )?;
        self.services
            .telemetry
            .record("mobile_media_stage_verified");
        Ok(receipt)
    }

    pub fn finish_owned_media_import(
        &mut self,
        source_ref: &str,
    ) -> Result<OwnedMediaSummary, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let receipt = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .finish_owned_media_import(source_ref, now)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "OWNED_ORIGINAL_MEDIA_COMMITTED",
            "MEDIA",
            true,
        )?;
        self.services
            .telemetry
            .record("mobile_owned_original_committed");
        Ok(receipt)
    }

    pub fn owned_media(&self, limit: usize) -> Result<Vec<OwnedMediaSummary>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .owned_media(limit)
    }

    pub fn abort_media_stage(&mut self, source_ref: &str) -> Result<(), MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .abort_media_stage(source_ref, now)?;
        self.services.telemetry.record("mobile_media_stage_aborted");
        Ok(())
    }

    pub fn set_onboarding_cursor(&self, cursor: OnboardingCursor) -> Result<(), MobileCoreError> {
        self.store.set_onboarding_cursor(cursor)?;
        self.services
            .telemetry
            .record("mobile_onboarding_cursor_saved");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_signed_registry_init(
        &self,
        channel_id: &str,
        trust_profile_cbor: &[u8],
        channel_head_envelope_cbor: &[u8],
        release_envelope_cbor: &[u8],
        allocation_unit_bytes: u64,
        destination_total_usable_bytes: u64,
        measured_free_bytes: u64,
    ) -> Result<RegistryInitPlan, MobileCoreError> {
        let profile = RegistryTrustProfile::from_canonical_cbor(trust_profile_cbor)?;
        let mut operation = self
            .store
            .begin_registry_init(channel_id, &profile, &self.budgets)?;
        if operation.state == RegistryOperationState::DeferredByUser {
            operation = self
                .store
                .resume_deferred_registry_init(&operation.operation_id)?;
        }
        let release = self.store.verify_and_accept_registry_target(
            &operation.operation_id,
            &profile,
            channel_id,
            channel_head_envelope_cbor,
            release_envelope_cbor,
        )?;
        operation = self
            .store
            .registry_operation(&operation.operation_id)?
            .ok_or_else(|| {
                MobileCoreError::Storage("Registry Init operation disappeared".into())
            })?;
        if operation.state == RegistryOperationState::ManifestVerified {
            operation = self
                .store
                .await_registry_exact_confirmation(&operation.operation_id)?;
        }
        let capacity = operation
            .capacity_plan
            .clone()
            .unwrap_or(registry_initial_capacity_plan(
                &release,
                allocation_unit_bytes,
                destination_total_usable_bytes,
                measured_free_bytes,
            )?);
        Ok(RegistryInitPlan {
            operation,
            release,
            capacity,
        })
    }

    pub fn defer_registry_init(
        &self,
        operation_id: &str,
        manifest_digest: &str,
    ) -> Result<RegistryLimitedReceipt, MobileCoreError> {
        self.store
            .defer_registry_init(operation_id, manifest_digest)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn confirm_signed_registry_init(
        &self,
        operation_id: &str,
        manifest_digest: &str,
        trust_profile_cbor: &[u8],
        network_policy: RegistryNetworkPolicy,
        one_time_network_override: bool,
        allocation_unit_bytes: u64,
        destination_total_usable_bytes: u64,
        measured_free_bytes: u64,
    ) -> Result<RegistryInitPlan, MobileCoreError> {
        let profile = RegistryTrustProfile::from_canonical_cbor(trust_profile_cbor)?;
        let existing = self
            .store
            .registry_operation(operation_id)?
            .ok_or_else(|| {
                MobileCoreError::RegistryAdmission("unknown Registry Init operation".into())
            })?;
        let release = self
            .store
            .registry_release_catalog(&existing.release_id)?
            .ok_or_else(|| {
                MobileCoreError::RegistryAdmission(
                    "verified Registry release lost its durable catalog".into(),
                )
            })?;
        let capacity = registry_initial_capacity_plan(
            &release,
            allocation_unit_bytes,
            destination_total_usable_bytes,
            measured_free_bytes,
        )?;
        let operation = self.store.confirm_registry_init(
            operation_id,
            manifest_digest,
            &profile,
            network_policy,
            one_time_network_override,
            capacity.clone(),
        )?;
        Ok(RegistryInitPlan {
            operation,
            release,
            capacity,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_registry_transfer_schedule(
        &self,
        operation_id: &str,
        manifest_digest: &str,
        platform: RegistryTransferPlatform,
        source_kind: RegistrySourceKind,
        request_fingerprint: &str,
        source_plan_digest: &str,
        expected_total_bytes: u64,
        foreground_user_resume: bool,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        self.store.prepare_registry_transfer_schedule(
            operation_id,
            manifest_digest,
            platform,
            source_kind,
            request_fingerprint,
            source_plan_digest,
            expected_total_bytes,
            foreground_user_resume,
            &self.budgets,
        )
    }

    pub fn prepare_registry_local_import_schedule(
        &self,
        operation_id: &str,
        manifest_digest: &str,
        foreground_user_resume: bool,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        self.store.prepare_registry_local_import_schedule(
            operation_id,
            manifest_digest,
            foreground_user_resume,
            &self.budgets,
        )
    }

    pub fn mark_registry_transfer_submitted(
        &self,
        transfer_nonce: &str,
        os_transfer_id: &str,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        self.store
            .mark_registry_transfer_submitted(transfer_nonce, os_transfer_id, &self.budgets)
    }

    pub fn adopt_registry_transfer(
        &self,
        transfer_nonce: &str,
        os_transfer_id: &str,
        observed_request_fingerprint: &str,
        observed_android_job_id: Option<u32>,
        matching_task_count: u32,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        self.store.adopt_registry_transfer(
            transfer_nonce,
            os_transfer_id,
            observed_request_fingerprint,
            observed_android_job_id,
            matching_task_count,
            &self.budgets,
        )
    }

    pub fn record_registry_transfer_missing(
        &self,
        transfer_nonce: &str,
        positive_user_stop_evidence: bool,
    ) -> Result<RegistryTransferScheduleRecord, MobileCoreError> {
        self.store.record_registry_transfer_missing(
            transfer_nonce,
            positive_user_stop_evidence,
            &self.budgets,
        )
    }

    pub fn registry_transfer_schedule_for_channel(
        &self,
        channel_id: &str,
    ) -> Result<Option<RegistryTransferScheduleRecord>, MobileCoreError> {
        self.store
            .registry_transfer_schedule_for_channel(channel_id)
    }

    pub fn prepare_registry_chunk_ledger(
        &self,
        transfer_nonce: &str,
    ) -> Result<RegistryLandingProgress, MobileCoreError> {
        self.store
            .prepare_registry_chunk_ledger(transfer_nonce, &self.budgets)
    }

    pub fn registry_chunk_resume_offset(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
    ) -> Result<u64, MobileCoreError> {
        self.store
            .registry_chunk_resume_offset(transfer_nonce, artifact_role, chunk_index)
    }

    pub fn next_registry_chunk_source_target(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
    ) -> Result<Option<RegistryChunkSourceTarget>, MobileCoreError> {
        self.store
            .next_registry_chunk_source_target(transfer_nonce, artifact_role)
    }

    pub fn begin_registry_chunk_write(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
        source_offset: u64,
    ) -> Result<RegistryChunkWriteStart, MobileCoreError> {
        self.store.begin_registry_chunk_write(
            transfer_nonce,
            artifact_role,
            chunk_index,
            source_offset,
        )
    }

    pub fn append_registry_chunk_write(
        &self,
        session: &mut RegistryChunkWriteSession,
        block: &[u8],
    ) -> Result<RegistryChunkWriteProgress, MobileCoreError> {
        self.store.append_registry_chunk_write(session, block)
    }

    pub fn checkpoint_registry_chunk_write(
        &self,
        session: &mut RegistryChunkWriteSession,
    ) -> Result<RegistryChunkWriteProgress, MobileCoreError> {
        self.store.checkpoint_registry_chunk_write(session)
    }

    pub fn suspend_registry_chunk_write(
        &self,
        session: RegistryChunkWriteSession,
    ) -> Result<RegistryChunkWriteProgress, MobileCoreError> {
        self.store.suspend_registry_chunk_write(session)
    }

    pub fn finish_registry_chunk_write(
        &self,
        session: RegistryChunkWriteSession,
    ) -> Result<RegistryChunkRecord, MobileCoreError> {
        self.store.finish_registry_chunk_write(session)
    }

    pub fn land_registry_chunk<R: Read>(
        &self,
        transfer_nonce: &str,
        artifact_role: u8,
        chunk_index: u32,
        source_offset: u64,
        source: &mut R,
    ) -> Result<RegistryChunkRecord, MobileCoreError> {
        self.store.land_registry_chunk(
            transfer_nonce,
            artifact_role,
            chunk_index,
            source_offset,
            source,
        )
    }

    pub fn recover_registry_chunk_ledger(
        &self,
        transfer_nonce: &str,
    ) -> Result<RegistryLandingProgress, MobileCoreError> {
        self.store.recover_registry_chunk_ledger(transfer_nonce)
    }

    pub fn registry_landing_progress(
        &self,
        transfer_nonce: &str,
    ) -> Result<RegistryLandingProgress, MobileCoreError> {
        self.store.registry_landing_progress(transfer_nonce)
    }

    pub fn unlock_private_node(
        &mut self,
        material: SecurityBootstrapMaterial,
        lock_policy: AppLockPolicy,
    ) -> Result<(), MobileCoreError> {
        let authority = material.installation_authority();
        self.store.bind_installation_authority(&authority)?;
        if self
            .secure_identity
            .as_ref()
            .is_some_and(|session| session.state() == SecuritySessionState::Unlocked)
        {
            return Ok(());
        }
        if !self.installation_binding_verified || self.secure_identity.is_none() {
            return Err(MobileCoreError::UnexpectedRestore(
                "an unsecured runtime cannot adopt protected identity material in-process".into(),
            ));
        }
        let now = self.services.clock.monotonic_millis();
        let vault_path = self.services.paths.private_vault_database_path();
        let draft_path = self.services.paths.private_draft_database_path();
        let media_database_path = self.services.paths.private_media_staging_database_path();
        let media_root = self.services.paths.private_media_staging_root();
        self.secure_identity = Some(SecureIdentitySession::open(
            material,
            &vault_path,
            &draft_path,
            &media_database_path,
            &media_root,
            now,
            lock_policy,
        )?);
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "SECURE_SESSION_REOPENED",
            "PRIVATE_NODE",
            true,
        )?;
        self.services
            .telemetry
            .record("mobile_private_node_unlocked");
        Ok(())
    }

    pub fn graceful_stop(&mut self) -> Result<(), MobileCoreError> {
        if self.quiesced {
            return Err(MobileCoreError::AlreadyQuiesced);
        }
        if self.secure_identity.is_some() {
            self.lock_private_node()?;
        }
        self.arbiter.revoke_grant(FOREGROUND_GRANT_ID);
        self.store.quiesce_process(
            self.arbiter.process_generation(),
            self.services.clock.monotonic_millis(),
        )?;
        self.arbiter.mark_dormant();
        self.services.telemetry.record("mobile_runtime_quiesced");
        self.quiesced = true;
        Ok(())
    }

    // --- Wave 7: MOB-05C & MOB-05B ---

    pub fn derived_registry_readiness(&self) -> DerivedRegistryReadiness {
        derive_registry_readiness(&self.store, self.active_registry_generation.as_ref())
    }

    pub fn active_registry_generation(&self) -> Option<Arc<RegistryGeneration>> {
        self.active_registry_generation.as_ref().map(Arc::clone)
    }

    pub fn activate_registry_release(
        &mut self,
        operation_id: &str,
        transfer_nonce: &str,
    ) -> Result<RegistryActivationReceiptRecord, MobileCoreError> {
        self.activate_registry_release_inner(operation_id, transfer_nonce, false)
    }

    pub fn activate_registry_release_with_test_fault(
        &mut self,
        operation_id: &str,
        transfer_nonce: &str,
        corrupt_sidecar_before_health: bool,
    ) -> Result<RegistryActivationReceiptRecord, MobileCoreError> {
        self.activate_registry_release_inner(
            operation_id,
            transfer_nonce,
            corrupt_sidecar_before_health,
        )
    }

    fn activate_registry_release_inner(
        &mut self,
        operation_id: &str,
        transfer_nonce: &str,
        corrupt_sidecar_before_health: bool,
    ) -> Result<RegistryActivationReceiptRecord, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let (receipt, next_gen) = activate_verified_registry_release(
            &self.store,
            operation_id,
            transfer_nonce,
            now,
            corrupt_sidecar_before_health,
        )?;
        if let Some(gen) = &next_gen {
            if !self
                .retained_registry_generations
                .iter()
                .any(|existing| existing.release_id == gen.release_id)
            {
                self.retained_registry_generations.push(Arc::clone(gen));
            }
        }
        self.active_registry_generation = next_gen;
        Ok(receipt)
    }

    pub fn rollback_registry_release(&mut self) -> Result<String, MobileCoreError> {
        let restored = rollback_to_previous_registry_release(&self.store)?;
        let release_id = restored.release_id.clone();
        if !self
            .retained_registry_generations
            .iter()
            .any(|existing| existing.release_id == release_id)
        {
            self.retained_registry_generations
                .push(Arc::clone(&restored));
        }
        self.active_registry_generation = Some(restored);
        Ok(release_id)
    }

    pub fn gc_retired_registry_releases(
        &mut self,
        force_reclaim_rollback_slot: bool,
    ) -> Result<RegistryGarbageCollectionReport, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.retained_registry_generations.retain(|gen| {
            self.active_registry_generation
                .as_ref()
                .is_some_and(|active| active.release_id == gen.release_id)
                || Arc::strong_count(gen) > 1
        });
        gc_retired_registry_releases(
            &self.store,
            &self.retained_registry_generations,
            now,
            force_reclaim_rollback_slot,
        )
    }

    pub fn list_registry_concepts(
        &self,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RegistryConceptView>, MobileCoreError> {
        let gen = self.active_registry_generation.as_ref().ok_or_else(|| {
            MobileCoreError::RegistryNotReady(
                "Concept Registry browse requires ReadyOffline".into(),
            )
        })?;
        list_registry_concepts(gen, query, limit)
    }

    pub fn execute_multi_provider_registry_transfer(
        &self,
        transfer_nonce: &str,
        plan: &RegistrySourcePlan,
        initial_capacity: &RegistryCapacityPlan,
        snapshot: &ResourceSnapshot,
        providers: &mut [&mut dyn RegistryByteProvider],
    ) -> Result<MultiProviderTransferOutcome, MobileCoreError> {
        execute_multi_provider_registry_transfer(
            &self.store,
            transfer_nonce,
            plan,
            initial_capacity,
            snapshot,
            providers,
        )
    }

    // --- Wave 7: MOB-04 Shared ARCH-10 KU, Fidelity, Search, Local KQL, Backup ---

    pub fn create_encoding_draft_from_raw_draft(
        &self,
        raw_draft_ref: &str,
        title: &str,
        subject_label: &str,
        predicate_label: &str,
        object_label: &str,
    ) -> Result<EncodingDraftRecord, MobileCoreError> {
        let readiness = self.derived_registry_readiness();
        let now = self.services.clock.monotonic_millis();
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        session
            .knowledge_engine()?
            .create_encoding_draft_from_raw_draft(
                &readiness,
                session.drafts_store()?,
                raw_draft_ref,
                title,
                subject_label,
                predicate_label,
                object_label,
                now,
            )
    }

    pub fn prepare_canonical_triple(
        &self,
        title: &str,
        subject_label: &str,
        predicate_label: &str,
        object_label: &str,
        source_text: Option<&str>,
        predecessor_cid_hex: Option<&str>,
    ) -> Result<PreparedKuPreviewView, MobileCoreError> {
        let readiness = self.derived_registry_readiness();
        let gen = self.active_registry_generation.as_ref().ok_or_else(|| {
            MobileCoreError::RegistryNotReady(format!(
                "canonical KU prepare requires ReadyOffline (current: {})",
                readiness.as_snapshot_str()
            ))
        })?;
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        session.knowledge_engine()?.prepare_canonical_triple(
            &readiness,
            &gen.reader,
            title,
            subject_label,
            predicate_label,
            object_label,
            source_text,
            predecessor_cid_hex,
        )
    }

    pub fn preview_prepared_ku(
        &self,
        prepare_id: &str,
    ) -> Result<PreparedKuPreviewView, MobileCoreError> {
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        session.knowledge_engine()?.preview_prepared_ku(prepare_id)
    }

    pub fn save_prepared_ku(
        &mut self,
        prepare_id: &str,
        alternate_of_cid_hex: Option<&str>,
        source_raw_draft_ref: Option<&str>,
    ) -> Result<MobileKuDetailView, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let detail = self
            .secure_identity
            .as_mut()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine_mut()?
            .save_prepared_ku(prepare_id, alternate_of_cid_hex, source_raw_draft_ref)?;
        self.store.append_security_history(
            self.arbiter.process_generation(),
            now,
            "PRIVATE_KU_SAVED_LOCAL_ONLY",
            "KNOWLEDGE",
            true,
        )?;
        Ok(detail)
    }

    pub fn update_ku_local_organization(
        &self,
        object_cid_hex: &str,
        tags: Vec<String>,
        collections: Vec<String>,
    ) -> Result<KuLocalOrganizationRecord, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine()?
            .update_local_organization(object_cid_hex, tags, collections)
    }

    pub fn list_my_ku_paginated(
        &self,
        cursor_saved_seq_exclusive: Option<u64>,
        limit: usize,
        collection_filter: Option<&str>,
    ) -> Result<PaginatedKuPage, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine()?
            .list_paginated(cursor_saved_seq_exclusive, limit, collection_filter)
    }

    pub fn inspect_ku_detail(
        &self,
        object_cid_hex: &str,
    ) -> Result<MobileKuDetailView, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine()?
            .inspect_ku_detail(object_cid_hex)
    }

    pub fn search_library(&self, query: &str) -> Result<Vec<MobileKuSummary>, MobileCoreError> {
        let reg = self
            .active_registry_generation
            .as_ref()
            .map(|gen| &gen.reader as &dyn ku_core::concept_registry::ConceptLookup);
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine()?
            .search_semantic(query, reg)
    }

    pub fn execute_local_kql(
        &self,
        query_text: &str,
    ) -> Result<LocalReadOnlyKqlView, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine()?
            .execute_read_only_kql(query_text)
    }

    pub fn project_local_neighborhood(
        &self,
        object_cid_hex: &str,
    ) -> Result<LocalNeighborhoodProjection, MobileCoreError> {
        let reg = self
            .active_registry_generation
            .as_ref()
            .map(|gen| &gen.reader as &dyn ku_core::concept_registry::ConceptLookup);
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .knowledge_engine()?
            .project_local_neighborhood(object_cid_hex, reg)
    }

    pub fn create_encrypted_backup_v2(
        &self,
        recovery_key_bytes: [u8; 32],
    ) -> Result<(Vec<u8>, MobileBackupReceipt), MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        let media_refs = session
            .owned_media(100)?
            .into_iter()
            .map(|item| item.media_ref)
            .collect();
        create_encrypted_backup_v2(
            &self.store,
            session.drafts_store()?,
            Some(session.knowledge_engine()?),
            media_refs,
            recovery_key_bytes,
            now,
        )
    }

    pub fn restore_encrypted_backup_v2(
        &mut self,
        archive_bytes: &[u8],
        recovery_key_bytes: [u8; 32],
        embedded_trust_profile_generation: u64,
    ) -> Result<MobileRestoreReceipt, MobileCoreError> {
        let spool_root = self.services.paths.backup_staging_root();
        let reg_clone = self.active_registry_generation.as_ref().map(Arc::clone);
        let session = self
            .secure_identity
            .as_mut()
            .ok_or(MobileCoreError::Locked)?;
        let (knowledge, drafts) = session.knowledge_and_drafts_mut()?;
        let kn_pair = reg_clone.as_ref().map(|gen| {
            (
                knowledge,
                &gen.reader as &dyn ku_core::concept_registry::ConceptLookup,
            )
        });
        restore_encrypted_backup_v2(
            archive_bytes,
            &spool_root,
            recovery_key_bytes,
            embedded_trust_profile_generation,
            &self.store,
            drafts,
            kn_pair,
            &self.migrations,
        )
    }

    pub fn export_reviewed_scoped_bundle(
        &self,
        selected_object_cids_hex: &[String],
        include_source_text: bool,
    ) -> Result<ScopedExportBundle, MobileCoreError> {
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        export_reviewed_scoped_bundle(
            session.knowledge_engine()?,
            selected_object_cids_hex,
            include_source_text,
        )
    }

    // --- Wave 7: MOB-06 AI Provider, ToolOrchestrator, Signed Model Catalog ---

    pub fn run_llm0_assistant_turn(
        &self,
        thread_id: &str,
        user_prompt: &str,
        scoped_context_refs: Vec<String>,
    ) -> Result<(AssistantThreadRecord, Vec<ToolProposalView>), MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        let orchestrator = session.tool_orchestrator()?;
        let provider = Llm0DeterministicAssistant;
        let events = provider.run_turn(user_prompt, &scoped_context_refs)?;
        let mut assembler =
            LlmStreamAssembler::new(provider.provider_id(), provider.is_cloud_route());
        for ev in events {
            assembler.push(ev)?;
        }
        let completed = assembler.finalize()?;
        let mut proposals = Vec::new();
        for (tool_name, args_json) in &completed.candidate_tool_calls {
            proposals.push(orchestrator.propose_tool_call(tool_name, args_json, now)?);
        }
        let proposal_ids = proposals.iter().map(|p| p.proposal_id.clone()).collect();
        let thread = orchestrator.append_thread_turn(
            thread_id,
            user_prompt,
            &completed,
            proposal_ids,
            scoped_context_refs,
            now,
        )?;
        Ok((thread, proposals))
    }

    pub fn propose_tool_call(
        &self,
        tool_name: &str,
        arguments_json: &str,
    ) -> Result<ToolProposalView, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .propose_tool_call(tool_name, arguments_json, now)
    }

    pub fn authorize_and_execute_tool(
        &mut self,
        proposal_id: &str,
        consent_nonce: Option<&str>,
    ) -> Result<ToolExecutionReceiptView, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        let reg = self.active_registry_generation.as_ref().map(Arc::clone);
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        let orchestrator = session.tool_orchestrator()?;
        let knowledge = session.knowledge_engine()?;
        let drafts = session.drafts_store()?;
        orchestrator.authorize_and_execute(proposal_id, consent_nonce, now, |tool_name, args| {
            match tool_name {
                "search_local_library" => {
                    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                    let lookup = reg
                        .as_ref()
                        .map(|g| &g.reader as &dyn ku_core::concept_registry::ConceptLookup);
                    let hits = knowledge.search_semantic(query, lookup)?;
                    Ok(serde_json::to_value(hits)?)
                }
                "run_local_read_only_kql" => {
                    let query = args
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("FIND (k:KU)");
                    let view = knowledge.execute_read_only_kql(query)?;
                    Ok(serde_json::to_value(view)?)
                }
                "save_private_raw_draft" => {
                    let lang = args
                        .get("content_language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("en");
                    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    let receipt = drafts.save_text(lang, content.as_bytes(), now)?;
                    Ok(serde_json::json!({
                        "draft_ref": receipt.draft_ref,
                        "content_bytes": receipt.content_bytes,
                        "total_drafts": receipt.total_drafts,
                    }))
                }
                "draft_ku_encoding_candidate" => Ok(serde_json::json!({
                    "status": "candidate_staged_for_user_review",
                })),
                other => Err(MobileCoreError::ForbiddenTool(format!(
                    "tool '{other}' requires dedicated interactive flow"
                ))),
            }
        })
    }

    pub fn deny_tool_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<ToolExecutionReceiptView, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .deny_proposal(proposal_id, now)
    }

    pub fn list_tool_proposals(
        &self,
        limit: usize,
    ) -> Result<Vec<ToolProposalView>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .list_proposals(limit)
    }

    pub fn prepare_cloud_disclosure(
        &self,
        destination_provider_id: &str,
        minimized_fields: &[(&str, &str)],
        switching_from_local_thread: bool,
    ) -> Result<CloudDisclosurePreview, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .prepare_cloud_disclosure(
                destination_provider_id,
                minimized_fields,
                switching_from_local_thread,
                now,
            )
    }

    pub fn approve_cloud_disclosure(
        &self,
        request_id: &str,
        consent_nonce: &str,
    ) -> Result<CloudDisclosurePreview, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .approve_cloud_disclosure(request_id, consent_nonce)
    }

    pub fn verify_and_stage_model_release(
        &self,
        manifest: &ModelReleaseManifest,
        publisher_public_key: &[u8; 32],
        artifact_bytes: &[u8],
    ) -> Result<ModelCatalogRecord, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .verify_and_stage_model_release(manifest, publisher_public_key, artifact_bytes, now)
    }

    pub fn activate_model_release(
        &self,
        release_id: &str,
    ) -> Result<ActiveModelRouteRecord, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .activate_model_release(release_id)
    }

    pub fn rollback_model_release(&self) -> Result<ActiveModelRouteRecord, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .rollback_model_release()
    }

    pub fn delete_model_release(&self, release_id: &str) -> Result<(), MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .delete_model_release(release_id)
    }

    pub fn list_model_releases(&self) -> Result<Vec<ModelCatalogRecord>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .list_model_releases()
    }

    pub fn active_model_route(&self) -> Result<ActiveModelRouteRecord, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .active_model_route()
    }

    pub fn read_assistant_thread(
        &self,
        thread_id: &str,
    ) -> Result<Option<AssistantThreadRecord>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .tool_orchestrator()?
            .read_thread(thread_id)
    }

    // --- Wave 7: MOB-07 Media Manifest, Range Viewer, Cache GC, Activity Inbox, Energy ---

    pub fn inspect_media_manifest(
        &self,
        media_ref: &str,
    ) -> Result<Option<SignedMediaManifest>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .media_store()?
            .inspect_media_manifest(media_ref)
    }

    pub fn read_verified_media_range(
        &self,
        media_ref: &str,
        offset: u64,
        max_bytes: usize,
    ) -> Result<MediaVerifiedRangeView, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .media_store()?
            .read_verified_range(media_ref, offset, max_bytes)
    }

    pub fn set_media_pinned(&self, media_ref: &str, pinned: bool) -> Result<bool, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .media_store()?
            .set_media_pinned(media_ref, pinned)
    }

    pub fn put_derived_media_cache(
        &self,
        cache_key: &str,
        media_ref: &str,
        derived_bytes: &[u8],
    ) -> Result<DerivedCacheRecord, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .media_store()?
            .put_derived_cache(cache_key, media_ref, derived_bytes, now)
    }

    pub fn gc_derived_media_cache(
        &self,
        target_reclaim_bytes: u64,
    ) -> Result<DerivedCacheGcReport, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .media_store()?
            .gc_derived_cache(target_reclaim_bytes)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn schedule_notification_intent(
        &self,
        intent_id: &str,
        channel: NotificationChannelKind,
        title_code: &str,
        redacted_summary: &str,
        target_route: &str,
        allowed_actions: Vec<ReversibleNotificationAction>,
        requires_in_app_approval: bool,
        local_hour: u8,
    ) -> Result<ActivityInboxEntry, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.activity_inbox.schedule_intent(
            intent_id,
            channel,
            title_code,
            redacted_summary,
            target_route,
            allowed_actions,
            requires_in_app_approval,
            local_hour,
            now,
        )
    }

    pub fn record_os_notification_submission(
        &self,
        intent_id: &str,
        os_notification_tag: &str,
        observed_in_active_tray: bool,
    ) -> Result<NotificationReceiptRecord, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.activity_inbox.record_os_submission(
            intent_id,
            os_notification_tag,
            observed_in_active_tray,
            now,
        )
    }

    pub fn reconcile_notifications_after_restart(
        &self,
        active_os_tags: &[String],
    ) -> Result<usize, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.activity_inbox
            .reconcile_unobserved_after_restart(active_os_tags, now)
    }

    pub fn execute_reversible_notification_action(
        &self,
        intent_id: &str,
        action_nonce: &str,
        action: ReversibleNotificationAction,
    ) -> Result<ActivityInboxEntry, MobileCoreError> {
        let now = self.services.clock.monotonic_millis();
        self.activity_inbox
            .execute_reversible_action(intent_id, action_nonce, action, now)
    }

    pub fn list_activity_inbox(
        &self,
        limit: usize,
    ) -> Result<Vec<ActivityInboxEntry>, MobileCoreError> {
        self.activity_inbox.list_entries(limit)
    }

    pub fn notification_policy(&self) -> Result<NotificationPolicyConfig, MobileCoreError> {
        self.activity_inbox.policy()
    }

    pub fn update_notification_policy(
        &self,
        config: &NotificationPolicyConfig,
    ) -> Result<(), MobileCoreError> {
        self.activity_inbox.update_policy(config)
    }

    pub fn apply_lifecycle_pressure(
        &mut self,
        pressure: LifecyclePressureKind,
    ) -> EnergyPolicyEvaluation {
        self.arbiter.apply_lifecycle_pressure(pressure)
    }

    // --- Wave 7: MOB-08 Gated Passive OBP Reunion Engine ---

    pub fn feature_gate_status(&self) -> Result<FeatureGateStatusView, MobileCoreError> {
        let active_ds = self.migrations.active_dataset()?;
        Ok(evaluate_feature_gate_status(
            &self.flags,
            active_ds.safe_mode_reason.is_some(),
            self.kill_switch_engaged,
        ))
    }

    pub fn set_feature_kill_switch(&mut self, engaged: bool) {
        self.kill_switch_engaged = engaged;
    }

    pub fn create_reunion_target(
        &self,
        title: &str,
        role_ccid: [u8; 16],
        expected_type_ccid: [u8; 16],
    ) -> Result<PrivateMatchTarget, MobileCoreError> {
        if self.kill_switch_engaged {
            return Err(MobileCoreError::FeatureGateDisabled(
                "MOB-GATE-NETWORKED-BETA kill-switch is engaged".into(),
            ));
        }
        let now = self.services.clock.monotonic_millis();
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .reunion_engine()?
            .create_private_target(&self.flags, title, role_ccid, expected_type_ccid, now)
    }

    pub fn set_reunion_target_lifecycle(
        &self,
        target_id: &str,
        lifecycle: MatchTargetLifecycle,
    ) -> Result<PrivateMatchTarget, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .reunion_engine()?
            .set_target_lifecycle(&self.flags, target_id, lifecycle)
    }

    pub fn evaluate_passive_reunion_delta(
        &self,
        reunion_frontier: [u8; 32],
        matcher_version: u32,
        coverage: ReunionCoverageClass,
        candidates: &[PublicCandidateAffordance],
    ) -> Result<Vec<QuarantinedReunionProposal>, MobileCoreError> {
        if self.kill_switch_engaged {
            return Err(MobileCoreError::FeatureGateDisabled(
                "MOB-GATE-NETWORKED-BETA kill-switch is engaged".into(),
            ));
        }
        let now = self.services.clock.monotonic_millis();
        let session = self
            .secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?;
        session.reunion_engine()?.evaluate_passive_reunion_delta(
            &self.flags,
            session.knowledge_engine()?.composition_store(),
            reunion_frontier,
            matcher_version,
            coverage,
            candidates,
            now,
        )
    }

    pub fn update_reunion_proposal_disposition(
        &self,
        proposal_ref: &str,
        disposition: MatchProposalDisposition,
    ) -> Result<QuarantinedReunionProposal, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .reunion_engine()?
            .update_proposal_disposition(&self.flags, proposal_ref, disposition)
    }

    pub fn list_reunion_targets(&self) -> Result<Vec<PrivateMatchTarget>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .reunion_engine()?
            .list_targets(&self.flags)
    }

    pub fn list_reunion_proposals(
        &self,
    ) -> Result<Vec<QuarantinedReunionProposal>, MobileCoreError> {
        self.secure_identity
            .as_ref()
            .ok_or(MobileCoreError::Locked)?
            .reunion_engine()?
            .list_proposals(&self.flags)
    }

    // --- Wave 7: MOB-09 Migrations & Privacy-Safe Redacted Diagnostics ---

    pub fn migration_coordinator(&self) -> &MigrationCoordinator {
        &self.migrations
    }

    pub fn export_redacted_diagnostics(
        &self,
    ) -> Result<RedactedDiagnosticsReport, MobileCoreError> {
        let snap = self.snapshot();
        let active_ds = self.migrations.active_dataset()?;
        let mut counts = BTreeMap::new();
        counts.insert("active_grants".into(), snap.active_grant_count as u64);
        counts.insert(
            "encrypted_raw_drafts".into(),
            snap.encrypted_raw_draft_count,
        );
        counts.insert(
            "pending_share_spools".into(),
            snap.pending_share_spool_count,
        );
        counts.insert(
            "owned_original_media".into(),
            snap.owned_original_media_count,
        );
        if let Some(session) = &self.secure_identity {
            if let Ok(kn) = session.knowledge_engine() {
                counts.insert("saved_private_kus".into(), kn.saved_ku_count() as u64);
            }
        }
        let history = self.store.recent_security_history(32)?;
        let codes: Vec<String> = history.into_iter().map(|h| h.event_code).collect();
        Ok(build_redacted_diagnostics_report(
            snap.profile_version,
            snap.process_generation,
            snap.registry_state,
            &active_ds,
            counts,
            &codes,
        ))
    }
}

fn registry_initial_capacity_plan(
    release: &RegistryReleaseCatalogRecord,
    allocation_unit_bytes: u64,
    destination_total_usable_bytes: u64,
    measured_free_bytes: u64,
) -> Result<RegistryCapacityPlan, MobileCoreError> {
    let target_total_alloc_bytes = release.target_total_alloc_bytes(allocation_unit_bytes)?;
    let safety_reserve_bytes =
        REGISTRY_MINIMUM_SAFETY_RESERVE_BYTES.max(destination_total_usable_bytes.div_ceil(10));
    RegistryCapacityPlan::exact_initial(
        release.publisher_min_additional_free_bytes,
        target_total_alloc_bytes,
        release
            .artifact_total_bytes
            .min(REGISTRY_DIRECT_TRANSFER_WINDOW_BYTES),
        REGISTRY_VERIFICATION_WORKSPACE_BYTES,
        REGISTRY_CATALOG_GROWTH_BYTES,
        safety_reserve_bytes,
        destination_total_usable_bytes,
        measured_free_bytes,
    )
}

fn verify_callback_fence(
    store: &crate::BootstrapStore,
    process_generation: u64,
    budgets: &ResourceBudgets,
) -> Result<bool, MobileCoreError> {
    store.prepare_transfer(
        &TransferLandingRecord {
            transfer_nonce: CALLBACK_FENCE_PROBE_NONCE.into(),
            operation_id: "mob02.bootstrap.probe".into(),
            release_id: "none".into(),
            artifact_role: "fence_probe".into(),
            chunk_index: 0,
            expected_hash: PROBE_HASH.into(),
            expected_length: 0,
            os_transfer_id: None,
            receiving_process_generation: None,
            app_assigned_callback_sequence: None,
            landed: false,
        },
        budgets,
    )?;
    let stale_generation = process_generation.saturating_sub(1);
    Ok(matches!(
        store.claim_transfer_callback(CALLBACK_FENCE_PROBE_NONCE, stale_generation, 1),
        Err(MobileCoreError::StaleGeneration { .. })
    ))
}
