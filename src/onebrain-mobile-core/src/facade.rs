use crate::{
    run_signed_local_kql_smoke, ActivationArbiter, ActivationPhase, ExecutionGrant,
    ExecutionGrantKind, MobileCoreError, MobileFeatureFlags, NetworkScope, ResourceBudgets,
    RuntimeServices, TransferLandingRecord, MOBILE_RUNTIME_PROFILE_VERSION,
};

const FOREGROUND_GRANT_ID: &str = "native.foreground";
const CALLBACK_FENCE_PROBE_NONCE: &str = "mob02.callback-fence.probe";
const PROBE_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

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
    quiesced: bool,
}

impl MobileRuntimeFacade {
    pub fn open_bootstrap_only(
        services: RuntimeServices,
        flags: MobileFeatureFlags,
        budgets: ResourceBudgets,
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
            signer_available: self.services.signer.is_available(),
            connectivity_online: self.services.connectivity.is_online(),
            background_scheduler_available: self
                .services
                .scheduler
                .background_execution_available(),
            stale_callback_rejected: self.stale_callback_rejected,
        }
    }

    pub fn flags(&self) -> &MobileFeatureFlags {
        &self.flags
    }

    pub fn budgets(&self) -> &ResourceBudgets {
        &self.budgets
    }

    pub fn graceful_stop(&mut self) -> Result<(), MobileCoreError> {
        if self.quiesced {
            return Err(MobileCoreError::AlreadyQuiesced);
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
