//! Pillar 7 Host Runtime Service (`P7-OBKG-UNIFIED-001`, `FW-03`, `FW-09`, `NEG-P7-001`).
//!
//! Coordinates:
//! - Pure-functional 5-plane `ObkgMultiPlaneProjection` (`project_and_record`, `at_frontier`,
//!   `inspect_historical`, `at_wall_clock_timestamp` failing closed `NEG-P7-001`,
//!   `diff_frontiers`, `clear_disposable_caches`, and deterministic rebuild verification).
//! - Deterministic 5-Stage `D1..D5` `DiscoveryPipelineOrchestrator` terminating strictly in
//!   non-executable `ProposalQuarantine` (`FW-02`).
//! - `PrivateVault`-sealed `LocalSalienceTieBreaker` (`purpose = 4`, `LOCAL_ONLY`).
//! - Firewalled `GroundedKgeRecallAdapter` (`FW-03`), feature-gated `FedRQuarantineValidator`
//!   (`obp/obkg-fedr/1` -> `VaultQuarantineRecord` with zero `obkg_rewards`), and feature-gated
//!   `DreamConsolidatorV1` executing inside `TypedCognitiveExecutor`.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::BTreeMap;

use ku_ai::vnext_executor::{CancellationToken, CognitiveTask, TypedCognitiveExecutor};
use ku_ai::vnext_kge_recall::{
    DreamConsolidationError, DreamConsolidationRun, DreamConsolidatorV1, DreamGraphSnapshot,
    FedRError, FedROperatorMatrixV1, FedRQuarantineOutcome, FedRQuarantinePolicy,
    FedRQuarantineValidator, GroundedConceptEmbedding, GroundedKgeRecallAdapter,
    GroundedKgeRecallConfig, KgeRecallError,
};
use ku_ai::vnext_model_recall::{
    CandidateRecallAdapter, ModelRecallError, ModelRecallPage, ModelRecallRequest,
    RecallCandidateSeed, SymbolicMappingValidator,
};
use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, AtomicVerifiedBackend, CanonicalValue,
    ConceptCcid, ObjectCid, PermitValidator, PrivateVault, ResourceProfile, SelectorCid,
    VerifiedStoreError,
};
use ku_kql::vnext_discovery_pipeline::{
    DiscoveryPipelineError, DiscoveryPipelineOrchestrator, DiscoveryPipelineReport,
    DiscoveryPipelineRequest, LocalSalienceTieBreaker,
};
use ku_kql::vnext_exploration::ExplorationState;
use ku_kql::vnext_obkg_projection::{
    ObkgFrontierDiff, ObkgHistoricalSelector, ObkgMultiPlaneConfig, ObkgMultiPlaneProjection,
    ObkgMultiPlaneSnapshot, ObkgMultiPlaneSources, ObkgProjectionError, ObkgSourceFrontier,
};
use ku_kql::vnext_proposal::{BindingProposal, ProposalId, ProposalQuarantine};
use ku_kql::vnext_query_view::PropensityAwareLearner;
use ku_kql::vnext_reunion::ReunionFrontier;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::vnext_extended_config::{VNextExtendedFeature, VNextExtendedFeatureConfig};

const SALIENCE_VAULT_DOMAIN: &[u8] = b"onebrain:vnext:obkg:local-salience:1\0";
const SALIENCE_SCHEMA_MAJOR: u64 = 1;

#[derive(Debug, Error)]
pub enum ObkgRuntimeError {
    #[error("OBKG multi-plane projection failed: {0:?}")]
    Projection(ObkgProjectionError),
    #[error("OBKG D1..D5 discovery pipeline failed: {0:?}")]
    Discovery(DiscoveryPipelineError),
    #[error("OBKG grounded KGE recall failed: {0}")]
    Kge(#[from] KgeRecallError),
    #[error("OBKG model recall failed: {0:?}")]
    ModelRecall(ModelRecallError),
    #[error("OBKG FedR quarantine failed: {0}")]
    FedR(#[from] FedRError),
    #[error("OBKG DreamConsolidatorV1 failed: {0}")]
    Dream(#[from] DreamConsolidationError),
    #[error("OBKG extended feature {0:?} is disabled or kill-switched")]
    FeatureDisabled(VNextExtendedFeature),
    #[error("OBKG PrivateVault local salience seal/open failed: {0:?}")]
    Vault(String),
    #[error("OBKG local salience cache payload is corrupt")]
    CorruptSalienceCache,
}

impl From<ObkgProjectionError> for ObkgRuntimeError {
    fn from(error: ObkgProjectionError) -> Self {
        Self::Projection(error)
    }
}

impl From<DiscoveryPipelineError> for ObkgRuntimeError {
    fn from(error: DiscoveryPipelineError) -> Self {
        Self::Discovery(error)
    }
}

impl From<ModelRecallError> for ObkgRuntimeError {
    fn from(error: ModelRecallError) -> Self {
        Self::ModelRecall(error)
    }
}

/// Disposable, deterministic Pillar 7 OBKG host service (`FW-09`).
pub struct ObkgRuntimeService {
    projection: ObkgMultiPlaneProjection,
    reunion_frontier: ReunionFrontier,
    exploration_state: ExplorationState,
    propensity_learner: PropensityAwareLearner,
    proposal_quarantine: ProposalQuarantine,
    salience: LocalSalienceTieBreaker,
    salience_keys: Vec<[u8; 32]>,
    kge_adapter: Option<GroundedKgeRecallAdapter>,
}

impl Default for ObkgRuntimeService {
    fn default() -> Self {
        Self::new()
    }
}

impl ObkgRuntimeService {
    pub fn new() -> Self {
        Self {
            projection: ObkgMultiPlaneProjection::default(),
            reunion_frontier: ReunionFrontier::default(),
            exploration_state: ExplorationState::new(
                [0x01; 32],
                ObjectCid::from_bytes([0x02; 32]),
                [0x03; 32],
                [0x04; 32],
            )
            .expect("valid default exploration state"),
            propensity_learner: PropensityAwareLearner::new(4_096)
                .expect("valid default propensity learner"),
            proposal_quarantine: ProposalQuarantine::default(),
            salience: LocalSalienceTieBreaker::new(),
            salience_keys: Vec::new(),
            kge_adapter: None,
        }
    }

    pub const fn holds_canonical_truth(&self) -> bool {
        false
    }

    pub const fn emits_obt_rewards(&self) -> bool {
        false
    }

    pub const fn proposal_quarantine_is_executable(&self) -> bool {
        false
    }

    // ─── 5-Plane Disposable OBKG Projection (`Plane 1..5`) ───────────────────

    pub fn project_and_record(
        &mut self,
        config: ObkgMultiPlaneConfig,
        sources: ObkgMultiPlaneSources<'_>,
    ) -> Result<([u8; 32], [u8; 32]), ObkgRuntimeError> {
        let snapshot = ObkgMultiPlaneProjection::project(config, sources)?;
        let commitment_cid = snapshot.snapshot_commitment_cid();
        let frontier_root = self.projection.record_snapshot(snapshot);
        Ok((frontier_root, commitment_cid))
    }

    pub fn at_frontier(
        &self,
        frontier: &ObkgSourceFrontier,
    ) -> Result<&ObkgMultiPlaneSnapshot, ObkgRuntimeError> {
        self.projection.at_frontier(frontier).map_err(Into::into)
    }

    pub fn snapshot_commitment_cid(
        &self,
        frontier: &ObkgSourceFrontier,
    ) -> Result<[u8; 32], ObkgRuntimeError> {
        Ok(self
            .projection
            .at_frontier(frontier)?
            .snapshot_commitment_cid())
    }

    pub fn inspect_historical(
        &self,
        selector: &ObkgHistoricalSelector,
    ) -> Result<&ObkgMultiPlaneSnapshot, ObkgRuntimeError> {
        self.projection
            .inspect_historical(selector)
            .map_err(Into::into)
    }

    /// Wall-clock historical queries fail closed with `WallClockHistoryForbidden` (`NEG-P7-001`).
    pub fn at_wall_clock_timestamp(
        &self,
        unix_timestamp_s: u64,
    ) -> Result<&ObkgMultiPlaneSnapshot, ObkgRuntimeError> {
        self.projection
            .at_wall_clock_timestamp(unix_timestamp_s)
            .map_err(Into::into)
    }

    pub fn diff_frontiers(
        &self,
        from: &ObkgSourceFrontier,
        to: &ObkgSourceFrontier,
    ) -> Result<ObkgFrontierDiff, ObkgRuntimeError> {
        let snap_a = self.projection.at_frontier(from)?;
        let snap_b = self.projection.at_frontier(to)?;
        Ok(ObkgMultiPlaneProjection::diff_frontiers(snap_a, snap_b))
    }

    /// Delete all disposable OBKG projection caches in memory (`FW-09`).
    pub fn clear_disposable_caches(&mut self) {
        self.projection.clear_disposable_caches();
    }

    // ─── D1..D5 Cross-Domain Discovery Pipeline (`ProposalQuarantine`) ──────

    pub fn run_discovery_pipeline(
        &mut self,
        request: DiscoveryPipelineRequest<'_>,
    ) -> Result<DiscoveryPipelineReport, ObkgRuntimeError> {
        DiscoveryPipelineOrchestrator::run(
            request,
            &mut self.reunion_frontier,
            &mut self.exploration_state,
            &mut self.propensity_learner,
            &mut self.proposal_quarantine,
        )
        .map_err(Into::into)
    }

    pub fn quarantined_proposal(&self, id: ProposalId) -> Option<&BindingProposal> {
        self.proposal_quarantine.get(id)
    }

    pub fn proposal_quarantine(&self) -> &ProposalQuarantine {
        &self.proposal_quarantine
    }

    pub fn proposal_quarantine_mut(&mut self) -> &mut ProposalQuarantine {
        &mut self.proposal_quarantine
    }

    // ─── PrivateVault-Sealed `LocalSalienceCache` (`purpose = 4`) ────────────

    pub fn salience_cache(&self) -> &LocalSalienceTieBreaker {
        &self.salience
    }

    pub fn set_local_salience(&mut self, object_cid: [u8; 32], weight_milli: u64) {
        if !self.salience_keys.contains(&object_cid) {
            self.salience_keys.push(object_cid);
            self.salience_keys.sort();
        }
        self.salience.set_salience_milli(object_cid, weight_milli);
    }

    pub fn record_local_co_activation(&mut self, object_cid: [u8; 32], delta_milli: u64) {
        if !self.salience_keys.contains(&object_cid) {
            self.salience_keys.push(object_cid);
            self.salience_keys.sort();
        }
        self.salience.record_co_activation(object_cid, delta_milli);
    }

    pub fn apply_salience_decay(&mut self, retain_bps: u16) {
        self.salience.apply_decay(retain_bps);
    }

    /// Seal the `LOCAL_ONLY` salience cache under `PrivateVault` `purpose = 4`
    /// (`seal_local_metadata`). Never enters canonical objects or network frames.
    pub fn seal_salience_cache<B: AtomicVerifiedBackend>(
        &self,
        owner_actor: ActorId,
        vault: &PrivateVault<B>,
    ) -> Result<([u8; 32], Vec<u8>), ObkgRuntimeError> {
        let binding = salience_binding(owner_actor);
        let mut entries = Vec::with_capacity(self.salience_keys.len());
        for cid in &self.salience_keys {
            let weight = self.salience.salience_milli(cid);
            if weight > 0 {
                entries.push(CanonicalValue::Array(vec![
                    CanonicalValue::Bytes(cid.to_vec()),
                    CanonicalValue::Unsigned(weight),
                ]));
            }
        }
        let root = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SALIENCE_SCHEMA_MAJOR)),
            (1, CanonicalValue::Array(entries)),
        ]);
        let plaintext = Zeroizing::new(
            encode_canonical(&root, ResourceProfile::ObjectV1)
                .map_err(|_| ObkgRuntimeError::CorruptSalienceCache)?,
        );
        let ciphertext = vault
            .seal_local_metadata(binding, &plaintext)
            .map_err(vault_err)?;
        Ok((binding, ciphertext))
    }

    pub fn open_salience_cache<B: AtomicVerifiedBackend>(
        &mut self,
        owner_actor: ActorId,
        vault: &PrivateVault<B>,
        ciphertext: &[u8],
    ) -> Result<usize, ObkgRuntimeError> {
        let binding = salience_binding(owner_actor);
        let plaintext = Zeroizing::new(
            vault
                .open_local_metadata(binding, ciphertext)
                .map_err(vault_err)?,
        );
        let decoded = decode_canonical(&plaintext, ResourceProfile::ObjectV1)
            .map_err(|_| ObkgRuntimeError::CorruptSalienceCache)?;
        let CanonicalValue::Map(fields) = decoded else {
            return Err(ObkgRuntimeError::CorruptSalienceCache);
        };
        if fields.len() != 2 || fields[0].0 != 0 || fields[1].0 != 1 {
            return Err(ObkgRuntimeError::CorruptSalienceCache);
        }
        let CanonicalValue::Unsigned(SALIENCE_SCHEMA_MAJOR) = fields[0].1 else {
            return Err(ObkgRuntimeError::CorruptSalienceCache);
        };
        let CanonicalValue::Array(items) = &fields[1].1 else {
            return Err(ObkgRuntimeError::CorruptSalienceCache);
        };
        let mut restored = LocalSalienceTieBreaker::new();
        let mut keys = BTreeMap::<[u8; 32], u64>::new();
        for item in items {
            let CanonicalValue::Array(pair) = item else {
                return Err(ObkgRuntimeError::CorruptSalienceCache);
            };
            if pair.len() != 2 {
                return Err(ObkgRuntimeError::CorruptSalienceCache);
            }
            let CanonicalValue::Bytes(cid_bytes) = &pair[0] else {
                return Err(ObkgRuntimeError::CorruptSalienceCache);
            };
            let cid: [u8; 32] = cid_bytes
                .as_slice()
                .try_into()
                .map_err(|_| ObkgRuntimeError::CorruptSalienceCache)?;
            let CanonicalValue::Unsigned(weight) = pair[1] else {
                return Err(ObkgRuntimeError::CorruptSalienceCache);
            };
            restored.set_salience_milli(cid, weight);
            keys.insert(cid, weight);
        }
        self.salience = restored;
        self.salience_keys = keys.into_keys().collect();
        Ok(self.salience_keys.len())
    }

    // ─── Grounded KGE Recall (`FW-03`), `FedR` Quarantine & `DreamConsolidatorV1`

    pub fn install_grounded_kge(
        &mut self,
        config: GroundedKgeRecallConfig,
    ) -> Result<(), ObkgRuntimeError> {
        self.kge_adapter = Some(GroundedKgeRecallAdapter::new(config)?);
        Ok(())
    }

    pub fn index_kge_candidate(
        &mut self,
        candidate: RecallCandidateSeed,
        selector: SelectorCid,
        ccid: ConceptCcid,
        structural_sig_digest: [u8; 32],
    ) -> Result<GroundedConceptEmbedding, ObkgRuntimeError> {
        let adapter = self
            .kge_adapter
            .as_mut()
            .ok_or(KgeRecallError::InvalidConfiguration)?;
        adapter
            .index_grounded_candidate(candidate, selector, ccid, structural_sig_digest)
            .map_err(Into::into)
    }

    pub fn recall_kge_candidates(
        &mut self,
        request: ModelRecallRequest,
    ) -> Result<ModelRecallPage, ObkgRuntimeError> {
        let adapter = self
            .kge_adapter
            .as_mut()
            .ok_or(KgeRecallError::InvalidConfiguration)?;
        adapter.recall(request).map_err(Into::into)
    }

    /// Validate an incoming `obp/obkg-fedr/1` relational operator matrix when
    /// `VNextExtendedFeature::ObkgFedrV1` is active, landing strictly in a non-executable
    /// `VaultQuarantineRecord` (`OBS-002`) with zero `obkg_rewards`.
    pub fn ingest_fedr_operator(
        &self,
        extended: &VNextExtendedFeatureConfig,
        expected_registry_release_root: [u8; 32],
        expected_embedding_space_spec_cid: ObjectCid,
        max_l2_norm_micros: u64,
        operator: &FedROperatorMatrixV1,
    ) -> Result<FedRQuarantineOutcome, ObkgRuntimeError> {
        let fedr_enabled = extended.is_active(VNextExtendedFeature::ObkgFedrV1);
        let validator = FedRQuarantineValidator::new(FedRQuarantinePolicy {
            fedr_enabled,
            expected_registry_release_root,
            expected_embedding_space_spec_cid,
            max_l2_norm_micros,
        });
        let outcome = validator.validate_and_quarantine(operator)?;
        debug_assert!(!outcome.quarantine_record.is_executable());
        debug_assert!(!outcome.mutated_active_model());
        debug_assert!(!outcome.emits_obkg_rewards());
        Ok(outcome)
    }

    /// Execute `DreamConsolidatorV1` inside `TypedCognitiveExecutor` when
    /// `VNextExtendedFeature::DreamConsolidatorV1` is active (`NEG-MAP-004`).
    #[allow(clippy::too_many_arguments)]
    pub fn run_dream_consolidation<V: SymbolicMappingValidator>(
        &self,
        extended: &VNextExtendedFeatureConfig,
        executor: &TypedCognitiveExecutor,
        permits: &PermitValidator,
        task: CognitiveTask,
        snapshot: &DreamGraphSnapshot,
        validator: &mut V,
        started_at: u64,
        cancellation: &CancellationToken,
    ) -> Result<DreamConsolidationRun, ObkgRuntimeError> {
        if !extended.is_active(VNextExtendedFeature::DreamConsolidatorV1) {
            return Err(ObkgRuntimeError::FeatureDisabled(
                VNextExtendedFeature::DreamConsolidatorV1,
            ));
        }
        let run = DreamConsolidatorV1::run_in_executor(
            executor,
            permits,
            task,
            snapshot,
            validator,
            started_at,
            cancellation,
        )?;
        debug_assert!(!run.is_executable());
        debug_assert!(!run.emits_obkg_rewards());
        Ok(run)
    }
}

fn salience_binding(actor: ActorId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SALIENCE_VAULT_DOMAIN);
    hasher.update(actor.as_bytes());
    *hasher.finalize().as_bytes()
}

fn vault_err(error: VerifiedStoreError) -> ObkgRuntimeError {
    ObkgRuntimeError::Vault(format!("{error:?}"))
}
