//! Pillar 5 Post-Commit `onebrain-obt` Host Runtime Coordinator (`P5-OBT-ECONOMY-VNEXT-001`, `FW-07`, `NEG-OBT-001`).
//!
//! Enforces strict one-way isolation between the knowledge plane (`P1..P4`, `P6..P8`) and the
//! dual-track `onebrain-obt` economy (`D-014` Track A accepted work + `M7` Track B attributed benefit):
//! - Modes: `Disabled` (default) | `ShadowEvaluate` (`M7.1` diagnostics only; zero mints/blocks)
//!   | `ActiveProvisional` | `ActiveDomainFinal`.
//! - Default `economic_status`: `ObtEconomicStatus::SimulatedNonEconomic` (`"simulated_non_economic"`).
//! - Lazily opens `<data_dir>/obt-ledger-v1/obt.redb` ONLY when explicitly enabled or invoked —
//!   never during default `OneBrainNode` / `VNextProductRuntime` startup.
//! - Corruption, disk-fullness, or failure in `obt-ledger-v1/obt.redb` degrades cleanly inside
//!   this coordinator and NEVER panics, blocks startup, or fails any knowledge-plane commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ku_core::foundation::{
    ActorId, FeedId, KeyStateReducer, ObjectCid, PermitCid, ValidatedFeedInception,
};
use onebrain_obt::{
    AccountBlockV1, AccountChainEngine, AccountChainError, AccountEquivocationWarrant,
    AuditCoSignature, BenefitRewardClaimV1, BlockInsertOutcome, ChallengeDispute, ClaimError,
    ClaimEvaluationOutcome, ClaimEvaluator, EpochCapTracker, FinalizedMintCertificate,
    FirewallDrainReport, FirewallDropReason, InMemoryFirewallSink, MintPipeline, MintPipelineError,
    NoticeEvaluationCandidate, NoticeIngestOutcome, ObtAmount, ObtEconomicStatus, ObtFeatureFlags,
    ObtFirewallConsumer, ObtLedgerDeltaBatch, ObtLedgerSyncEngine, ObtLedgerSyncSummary,
    ObtNoticeV2, ObtRuntimeMode, ObtStoreError, ObtSyncApplyReport, ObtSyncError,
    PendingMintRecord, RedbObtStore, RewardPolicyV1, ScopedNullifier, SettlementDomainCheckpoint,
    SettlementDomainManifest, ShadowClaimEvaluation, SyncAuthorityContext, WorkRewardClaimV1,
};
use thiserror::Error;

use crate::vnext_extended_config::VNextExtendedFeatureConfig;
use crate::vnext_reward_firewall::{
    RewardConsumerError, RewardEvidenceConsumerV2, RewardEvidenceNoticeV2,
};

pub const OBT_LEDGER_SUBDIR: &str = "obt-ledger-v1";
pub const OBT_LEDGER_DB_FILENAME: &str = "obt.redb";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObtStoreHealth {
    LazyUnopened,
    Healthy,
    IsolatedDegraded(String),
}

#[derive(Debug, Error)]
pub enum ObtRuntimeError {
    #[error("OBT runtime is disabled (`ObtRuntimeMode::Disabled`)")]
    RuntimeDisabled,
    #[error("OBT ledger feature (`obt_ledger_v1`) is disabled or kill-switched")]
    FeatureDisabled,
    #[error("OBT isolated store is degraded/unavailable: {0}")]
    StoreUnavailable(String),
    #[error("OBT store error: {0}")]
    Store(#[from] ObtStoreError),
    #[error("OBT claim evaluation error: {0}")]
    Claim(#[from] ClaimError),
    #[error("OBT mint pipeline error: {0}")]
    Mint(#[from] MintPipelineError),
    #[error("OBT account chain error: {0}")]
    AccountChain(#[from] AccountChainError),
    #[error("OBT sync error: {0}")]
    Sync(#[from] ObtSyncError),
}

/// Host-level coordinator owning isolated `onebrain-obt` engines and lazy `RedbObtStore`.
pub struct ObtRuntimeCoordinator {
    data_dir: PathBuf,
    store: Option<RedbObtStore>,
    health: ObtStoreHealth,
    firewall: ObtFirewallConsumer,
    fallback_sink: InMemoryFirewallSink,
    policies: BTreeMap<[u8; 32], RewardPolicyV1>,
    epoch_caps: EpochCapTracker,
    mint_pipeline: MintPipeline,
    account_chain: AccountChainEngine,
}

impl ObtRuntimeCoordinator {
    /// Construct a default-off (`ObtRuntimeMode::Disabled`, `SimulatedNonEconomic`) coordinator
    /// that touches zero files on disk until explicitly enabled.
    pub fn new_lazy(data_dir: impl AsRef<Path>) -> Self {
        let flags = ObtFeatureFlags::default();
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            store: None,
            health: ObtStoreHealth::LazyUnopened,
            firewall: ObtFirewallConsumer::new(ObtRuntimeMode::Disabled, flags, 4_096, 2),
            fallback_sink: InMemoryFirewallSink::default(),
            policies: BTreeMap::new(),
            epoch_caps: EpochCapTracker::default(),
            mint_pipeline: MintPipeline::new(),
            account_chain: AccountChainEngine::new(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir
            .join(OBT_LEDGER_SUBDIR)
            .join(OBT_LEDGER_DB_FILENAME)
    }

    pub const fn can_gate_knowledge_plane(&self) -> bool {
        false
    }

    pub const fn mode(&self) -> ObtRuntimeMode {
        self.firewall.mode()
    }

    pub const fn economic_status(&self) -> ObtEconomicStatus {
        self.firewall.economic_status()
    }

    pub const fn economic_status_label(&self) -> &'static str {
        self.firewall.economic_status().as_str()
    }

    pub fn store_health(&self) -> &ObtStoreHealth {
        &self.health
    }

    /// Synchronize feature flags from `VNextExtendedFeatureConfig` and `reward_evidence_export`.
    pub fn sync_feature_flags(
        &mut self,
        reward_evidence_export_enabled: bool,
        reward_export_kill_switch: bool,
        extended: &VNextExtendedFeatureConfig,
    ) {
        self.firewall.set_flags(ObtFeatureFlags {
            reward_evidence_export: reward_evidence_export_enabled,
            reward_export_kill_switch,
            obt_ledger_v1: extended.enabled.obt_ledger_v1,
            obt_ledger_kill_switch: extended.kill_switches.obt_ledger_v1,
        });
    }

    pub fn configure_mode(
        &mut self,
        mode: ObtRuntimeMode,
        economic_status: ObtEconomicStatus,
    ) -> Result<(), ObtRuntimeError> {
        self.firewall.set_mode(mode);
        self.firewall.set_economic_status(economic_status);
        if mode != ObtRuntimeMode::Disabled {
            let store = self.ensure_store_open()?;
            store.set_runtime_mode(mode)?;
            store.set_economic_status(economic_status)?;
        }
        Ok(())
    }

    fn ensure_store_open(&mut self) -> Result<&mut RedbObtStore, ObtRuntimeError> {
        if self.store.is_none() {
            let path = self.db_path();
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    let msg = e.to_string();
                    self.health = ObtStoreHealth::IsolatedDegraded(msg.clone());
                    return Err(ObtRuntimeError::StoreUnavailable(msg));
                }
            }
            match RedbObtStore::open(&path) {
                Ok(store) => {
                    match (
                        store.load_epoch_caps(),
                        store.load_mint_pipeline(),
                        store.load_account_chain(),
                    ) {
                        (Ok(caps), Ok(mints), Ok(chain)) => {
                            if let Ok(saved_mode) = store.get_runtime_mode() {
                                if self.firewall.mode() == ObtRuntimeMode::Disabled
                                    && saved_mode != ObtRuntimeMode::Disabled
                                {
                                    self.firewall.set_mode(saved_mode);
                                }
                            }
                            if let Ok(saved_econ) = store.get_economic_status() {
                                self.firewall.set_economic_status(saved_econ);
                            }
                            self.epoch_caps = caps;
                            self.mint_pipeline = mints;
                            self.account_chain = chain;
                            self.health = ObtStoreHealth::Healthy;
                            self.store = Some(store);
                        }
                        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                            let msg = e.to_string();
                            self.health = ObtStoreHealth::IsolatedDegraded(msg.clone());
                            return Err(ObtRuntimeError::StoreUnavailable(msg));
                        }
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    self.health = ObtStoreHealth::IsolatedDegraded(msg.clone());
                    return Err(ObtRuntimeError::StoreUnavailable(msg));
                }
            }
        }
        Ok(self.store.as_mut().expect("opened above"))
    }

    pub fn register_reward_policy(
        &mut self,
        policy: RewardPolicyV1,
    ) -> Result<ObjectCid, ObtRuntimeError> {
        policy
            .validate()
            .map_err(|e| ObtRuntimeError::Claim(ClaimError::Policy(e)))?;
        let cid = policy.policy_cid();
        self.policies.insert(cid.into_bytes(), policy.clone());
        if self.firewall.mode() != ObtRuntimeMode::Disabled {
            let store = self.ensure_store_open()?;
            store.put_policy(&policy)?;
        }
        Ok(cid)
    }

    // ─── One-Way Post-Commit Firewall Ingest & `ShadowEvaluate` Drain ────────

    /// Best-effort post-commit notice ingest (`FW-07`).
    ///
    /// Even if `obt-ledger-v1/obt.redb` is corrupt, locked, or disk-full, this method
    /// NEVER panics and NEVER returns an error to knowledge-plane callers!
    pub fn ingest_post_commit_notice(
        &mut self,
        notice: RewardEvidenceNoticeV2,
    ) -> NoticeIngestOutcome {
        if self.firewall.mode() == ObtRuntimeMode::Disabled {
            return NoticeIngestOutcome::DroppedFirewallSafe {
                reason: FirewallDropReason::RuntimeDisabled,
            };
        }
        let obt_notice = notice.to_obt_notice();
        if self.store.is_none() {
            if self.ensure_store_open().is_err() {
                return NoticeIngestOutcome::DroppedFirewallSafe {
                    reason: FirewallDropReason::BackendUnavailableOrCorrupt,
                };
            }
        }
        let store_opt = self
            .store
            .as_mut()
            .map(|s| s as &mut dyn onebrain_obt::FirewallSinkBackend);
        self.firewall.ingest_notice(obt_notice, store_opt)
    }

    /// Drain queued post-commit notices under the current `ObtRuntimeMode`.
    /// Under `ShadowEvaluate`, persists `ShadowClaimEvaluation` diagnostics ONLY with
    /// zero `PendingMintRecord` or `AccountBlockV1` mutations (`M7.1`).
    pub fn drain_post_commit_notices<F>(&mut self, evaluator: F) -> FirewallDrainReport
    where
        F: FnMut(&ObtNoticeV2, ObtEconomicStatus) -> Result<NoticeEvaluationCandidate, String>,
    {
        if self.firewall.mode() != ObtRuntimeMode::Disabled && self.store.is_none() {
            let _ = self.ensure_store_open();
        }
        if let Some(store) = self.store.as_mut() {
            self.firewall.drain(store, evaluator)
        } else {
            self.firewall.drain(&mut self.fallback_sink, evaluator)
        }
    }

    /// Helper building a deterministic `ShadowClaimEvaluation` candidate for `ShadowEvaluate`
    /// or active notice draining.
    pub fn build_shadow_candidate(
        notice: &ObtNoticeV2,
        candidate_nullifier: ScopedNullifier,
        would_approve: bool,
        candidate_amount: ObtAmount,
        diagnostic_code: impl Into<String>,
        economic_status: ObtEconomicStatus,
        pending_mints_if_active: Vec<PendingMintRecord>,
    ) -> NoticeEvaluationCandidate {
        let (policy_cid, evidence_frontier_root) = match notice {
            ObtNoticeV2::BenefitClosure {
                policy_cid,
                evidence_frontier_root,
                ..
            }
            | ObtNoticeV2::AcceptedWork {
                policy_cid,
                evidence_frontier_root,
                ..
            } => (*policy_cid, *evidence_frontier_root),
        };
        NoticeEvaluationCandidate {
            shadow: ShadowClaimEvaluation::new(
                notice.notice_id(),
                policy_cid,
                evidence_frontier_root,
                candidate_nullifier,
                would_approve,
                candidate_amount,
                diagnostic_code,
                economic_status,
            ),
            pending_mints_if_active,
        }
    }

    pub fn list_shadow_evaluations(
        &mut self,
    ) -> Result<Vec<ShadowClaimEvaluation>, ObtRuntimeError> {
        if self.store.is_none() && !self.db_path().exists() {
            return Ok(self.fallback_sink.shadow_evaluations.clone());
        }
        let store = self.ensure_store_open()?;
        store.list_shadow_evaluations().map_err(Into::into)
    }

    // ─── `D-014` Track A & `M7` Track B Evaluation -> Challenge Window ───────

    pub fn evaluate_and_stage_work_claim(
        &mut self,
        claim: &WorkRewardClaimV1,
        opened_at_frontier_seq: u64,
    ) -> Result<(ClaimEvaluationOutcome, Vec<PendingMintRecord>), ObtRuntimeError> {
        let mode = self.firewall.mode();
        if mode == ObtRuntimeMode::Disabled {
            return Err(ObtRuntimeError::RuntimeDisabled);
        }
        let _ = self.ensure_store_open()?;
        let policy = self
            .policies
            .get(claim.reward_policy_cid.as_bytes())
            .cloned()
            .or_else(|| {
                self.store
                    .as_ref()
                    .and_then(|s| s.get_policy(claim.reward_policy_cid).ok().flatten())
            })
            .ok_or(ObtRuntimeError::Claim(ClaimError::PolicyCidMismatch))?;

        if mode == ObtRuntimeMode::ShadowEvaluate {
            let mut shadow_caps = self.epoch_caps.clone();
            let outcome = ClaimEvaluator::evaluate_work_claim(claim, &policy, &mut shadow_caps)?;
            return Ok((outcome, Vec::new()));
        }

        let outcome = ClaimEvaluator::evaluate_work_claim(claim, &policy, &mut self.epoch_caps)?;
        let staged =
            self.mint_pipeline
                .stage_approved_claim(&outcome, &policy, opened_at_frontier_seq);
        if let Some(store) = self.store.as_ref() {
            store.persist_epoch_caps(&self.epoch_caps)?;
            store.persist_mint_pipeline(&self.mint_pipeline)?;
        }
        Ok((outcome, staged))
    }

    pub fn evaluate_and_stage_benefit_claim(
        &mut self,
        claim: &BenefitRewardClaimV1,
        opened_at_frontier_seq: u64,
    ) -> Result<(ClaimEvaluationOutcome, Vec<PendingMintRecord>), ObtRuntimeError> {
        let mode = self.firewall.mode();
        if mode == ObtRuntimeMode::Disabled {
            return Err(ObtRuntimeError::RuntimeDisabled);
        }
        let _ = self.ensure_store_open()?;
        let policy = self
            .policies
            .get(claim.reward_policy_cid.as_bytes())
            .cloned()
            .or_else(|| {
                self.store
                    .as_ref()
                    .and_then(|s| s.get_policy(claim.reward_policy_cid).ok().flatten())
            })
            .ok_or(ObtRuntimeError::Claim(ClaimError::PolicyCidMismatch))?;

        if mode == ObtRuntimeMode::ShadowEvaluate {
            let mut shadow_caps = self.epoch_caps.clone();
            let outcome = ClaimEvaluator::evaluate_benefit_claim(claim, &policy, &mut shadow_caps)?;
            return Ok((outcome, Vec::new()));
        }

        let outcome = ClaimEvaluator::evaluate_benefit_claim(claim, &policy, &mut self.epoch_caps)?;
        let staged =
            self.mint_pipeline
                .stage_approved_claim(&outcome, &policy, opened_at_frontier_seq);
        if let Some(store) = self.store.as_ref() {
            store.persist_epoch_caps(&self.epoch_caps)?;
            store.persist_mint_pipeline(&self.mint_pipeline)?;
        }
        Ok((outcome, staged))
    }

    pub fn submit_challenge_dispute(
        &mut self,
        pending_mint_cid: [u8; 32],
        dispute: ChallengeDispute,
        challenger_feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        current_frontier_seq: u64,
    ) -> Result<PendingMintRecord, ObtRuntimeError> {
        let _ = self.ensure_store_open()?;
        let updated = self
            .mint_pipeline
            .submit_challenge_dispute(
                pending_mint_cid,
                dispute,
                challenger_feed,
                key_state,
                current_frontier_seq,
            )?
            .clone();
        if let Some(store) = self.store.as_ref() {
            store.persist_mint_pipeline(&self.mint_pipeline)?;
        }
        Ok(updated)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finalize_pending_mint(
        &mut self,
        pending_mint_cid: [u8; 32],
        epoch_entropy_reveal: [u8; 32],
        active_evaluators: &[(ActorId, PermitCid)],
        committee_size: usize,
        cosignatures: Vec<AuditCoSignature>,
        auditor_feeds: &BTreeMap<FeedId, ValidatedFeedInception>,
        key_state: &KeyStateReducer,
        current_frontier_seq: u64,
    ) -> Result<FinalizedMintCertificate, ObtRuntimeError> {
        let _ = self.ensure_store_open()?;
        let cert = self.mint_pipeline.finalize_mint(
            pending_mint_cid,
            epoch_entropy_reveal,
            active_evaluators,
            committee_size,
            cosignatures,
            auditor_feeds,
            key_state,
            current_frontier_seq,
        )?;
        if let Some(store) = self.store.as_ref() {
            store.persist_mint_pipeline(&self.mint_pipeline)?;
        }
        Ok(cert)
    }

    // ─── `ActorId` Account Chains, Domain Finality & `obp/obt-ledger/1` Sync ─

    pub fn register_settlement_domain(
        &mut self,
        manifest: SettlementDomainManifest,
    ) -> Result<(), ObtRuntimeError> {
        self.account_chain.register_domain_manifest(manifest)?;
        Ok(())
    }

    pub fn submit_account_block(
        &mut self,
        block: AccountBlockV1,
        feed: &ValidatedFeedInception,
        key_state: &KeyStateReducer,
        authority_observed_at_secs: u64,
        current_local_secs: u64,
        recipient_accepts_provisional: bool,
    ) -> Result<BlockInsertOutcome, ObtRuntimeError> {
        if matches!(
            self.firewall.mode(),
            ObtRuntimeMode::Disabled | ObtRuntimeMode::ShadowEvaluate
        ) {
            return Err(ObtRuntimeError::RuntimeDisabled);
        }
        let _ = self.ensure_store_open()?;
        let outcome = self.account_chain.submit_block(
            block,
            feed,
            key_state,
            authority_observed_at_secs,
            current_local_secs,
            recipient_accepts_provisional,
        )?;
        if let Some(store) = self.store.as_ref() {
            store.persist_account_chain(&self.account_chain)?;
        }
        Ok(outcome)
    }

    pub fn apply_settlement_checkpoint(
        &mut self,
        checkpoint: SettlementDomainCheckpoint,
    ) -> Result<(), ObtRuntimeError> {
        let _ = self.ensure_store_open()?;
        self.account_chain.apply_domain_checkpoint(checkpoint)?;
        if let Some(store) = self.store.as_ref() {
            store.persist_account_chain(&self.account_chain)?;
        }
        Ok(())
    }

    pub fn build_sync_summary(&mut self) -> Result<ObtLedgerSyncSummary, ObtRuntimeError> {
        let _ = self.ensure_store_open()?;
        let engine = ObtLedgerSyncEngine::new(
            self.firewall.flags(),
            self.mint_pipeline.clone(),
            self.account_chain.clone(),
        );
        engine.build_summary().map_err(Into::into)
    }

    pub fn compute_sync_delta_for_peer(
        &mut self,
        peer_summary: &ObtLedgerSyncSummary,
    ) -> Result<ObtLedgerDeltaBatch, ObtRuntimeError> {
        let _ = self.ensure_store_open()?;
        let engine = ObtLedgerSyncEngine::new(
            self.firewall.flags(),
            self.mint_pipeline.clone(),
            self.account_chain.clone(),
        );
        engine
            .compute_delta_for_peer(peer_summary)
            .map_err(Into::into)
    }

    pub fn apply_sync_delta(
        &mut self,
        batch: &ObtLedgerDeltaBatch,
        ctx: &SyncAuthorityContext<'_>,
    ) -> Result<ObtSyncApplyReport, ObtRuntimeError> {
        let _ = self.ensure_store_open()?;
        let mut engine = ObtLedgerSyncEngine::new(
            self.firewall.flags(),
            self.mint_pipeline.clone(),
            self.account_chain.clone(),
        );
        let report = engine.apply_delta(batch, ctx)?;
        self.mint_pipeline = engine.mint_pipeline;
        self.account_chain = engine.account_chain;
        if let Some(store) = self.store.as_ref() {
            store.persist_mint_pipeline(&self.mint_pipeline)?;
            store.persist_account_chain(&self.account_chain)?;
        }
        Ok(report)
    }

    pub fn settled_balance(&self, account: ActorId) -> ObtAmount {
        self.account_chain.settled_balance(account)
    }

    pub fn available_provisional_balance(
        &self,
        account: ActorId,
    ) -> Option<(ObtAmount, &'static str)> {
        self.account_chain.available_provisional_balance(account)
    }

    pub fn warrants(&self) -> Vec<&AccountEquivocationWarrant> {
        self.account_chain.warrants.values().collect()
    }

    pub fn pending_mints(&self) -> Vec<&PendingMintRecord> {
        self.mint_pipeline.pending_mints.values().collect()
    }
}

impl RewardEvidenceConsumerV2 for ObtRuntimeCoordinator {
    fn try_export_v2(
        &mut self,
        notice: &RewardEvidenceNoticeV2,
    ) -> Result<(), RewardConsumerError> {
        match self.ingest_post_commit_notice(*notice) {
            NoticeIngestOutcome::Enqueued { .. } | NoticeIngestOutcome::DuplicateIgnored { .. } => {
                Ok(())
            }
            NoticeIngestOutcome::DroppedFirewallSafe { reason } => match reason {
                FirewallDropReason::QueueBackpressureFull => Err(RewardConsumerError::Backpressure),
                FirewallDropReason::BackendUnavailableOrCorrupt => {
                    Err(RewardConsumerError::CorruptState)
                }
                FirewallDropReason::RuntimeDisabled
                | FirewallDropReason::ExportFlagDisabled
                | FirewallDropReason::InvalidNoticeCommitment => {
                    Err(RewardConsumerError::Unavailable)
                }
            },
        }
    }
}
