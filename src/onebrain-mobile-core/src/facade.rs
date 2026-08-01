use crate::{
    run_signed_local_kql_smoke, ActivationArbiter, ActivationPhase, AppLockPolicy, ExecutionGrant,
    ExecutionGrantKind, MediaStageReceipt, MobileCoreError, MobileFeatureFlags, NetworkScope,
    OnboardingCursor, OwnedMediaSummary, RawDraftReceipt, RegistryCapacityPlan,
    RegistryLimitedReceipt, RegistryNetworkPolicy, RegistryOperationRecord, RegistryOperationState,
    RegistryReleaseCatalogRecord, RegistryTrustProfile, ResourceBudgets, RuntimeServices,
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
        Self::open_internal(services, flags, budgets, None)
    }

    pub fn open_secured(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
        material: SecurityBootstrapMaterial,
        lock_policy: AppLockPolicy,
    ) -> Result<Self, MobileCoreError> {
        Self::open_internal(services, flags, budgets, Some((material, lock_policy)))
    }

    fn open_internal(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
        security: Option<(SecurityBootstrapMaterial, AppLockPolicy)>,
    ) -> Result<Self, MobileCoreError> {
        flags.validate_bootstrap_only()?;
        budgets.validate()?;
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
            installation_binding_verified,
            installation_created,
            quiesced: false,
        })
    }

    pub fn snapshot(&self) -> MobileRuntimeSnapshot {
        MobileRuntimeSnapshot {
            profile_version: MOBILE_RUNTIME_PROFILE_VERSION,
            process_generation: self.arbiter.process_generation(),
            activation_phase: self.arbiter.phase(),
            active_grant_count: self.arbiter.active_grant_count(),
            recovered_unclean_start: self.recovered_unclean_start,
            bootstrap_store_opened: true,
            registry_state: "BootstrapOnly",
            local_kql_fixture_verified: self.local_kql_fixture_verified,
            private_planner_verified: self.private_planner_verified,
            local_kql_rows: self.local_kql_rows,
            llm_provider_id: self.services.llm.provider_id(),
            llm_available: self.services.llm.is_available(),
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
        if self.snapshot().registry_state != "BootstrapOnly" {
            return Err(MobileCoreError::InvalidArgument(
                "raw draft command is only the Limited-mode source intake".into(),
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
