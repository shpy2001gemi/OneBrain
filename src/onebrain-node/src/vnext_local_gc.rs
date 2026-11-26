//! Wave 6 `P8` Local Garbage Collection PinSet Evaluator (`VNextFeature::CheckpointGc`).
//!
//! Protects all locally retained artifacts from eviction:
//! 1. User collections / saved KUs (`local_collection_cids`),
//! 2. Unexpired `ProviderLease` subject references (`ProviderLeaseMap` + `LeaseObservationStore`),
//! 3. Active `FrontierAssemblyManifest` source/placement references and `ResolutionView`
//!    adopted `MappingKernelCid` / `MappingEnvelope` references,
//! 4. Unfinalized `P4` causal evidence chains (`BenefitClosureView` in `DeferredIncomplete`
//!    or `ContestedOrMixed`) and `P5` `PendingMintRecord` open challenge windows,
//! 5. Canonical `OwnedBlobReferenceV1::retains_blob()` (`CanonicalBlobReferenceOracle`).

use std::collections::{BTreeMap, BTreeSet};

use ku_core::blob_store::BlobCid;
use ku_core::foundation::{
    BenefitClosureState, BenefitClosureView, FrontierAssemblyManifest, LeaseObservationStore,
    ObjectCid, ProviderLeaseMap, ProviderSubject, ResolutionView,
};
use onebrain_obt::PendingMintRecord;
use thiserror::Error;

use crate::blob_authority::CanonicalBlobReferenceOracle;
use crate::vnext_config::{VNextFeature, VNextFeatureConfig};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PinRetentionReason {
    LocalCollection,
    UnexpiredProviderLease,
    ActiveAssemblyOrResolution,
    UnfinalizedP4EvidenceWindow,
    UnfinalizedP5ChallengeWindow,
    CanonicalOwnedBlobReference,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalEvictionDecision {
    Pinned(Vec<PinRetentionReason>),
    EligibleForEviction,
}

impl LocalEvictionDecision {
    pub const fn is_pinned(&self) -> bool {
        matches!(self, Self::Pinned(_))
    }
}

#[derive(Debug, PartialEq, Eq, Error)]
pub enum LocalGcPinError {
    #[error(
        "local checkpoint/GC is disabled (`VNextFeature::CheckpointGc` inactive or kill-switched)"
    )]
    CheckpointGcDisabled,
    #[error("blob reference oracle failed: {0}")]
    BlobOracle(String),
}

/// Evaluated PinSet over local `P1`/`P3`/`P4`/`P5`/`P8` runtime state.
#[derive(Clone, Debug, Default)]
pub struct PinSetEvaluator {
    pinned_objects: BTreeMap<[u8; 32], BTreeSet<PinRetentionReason>>,
    pinned_blobs: BTreeMap<[u8; 34], BTreeSet<PinRetentionReason>>,
}

impl PinSetEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pin_local_collection_objects(&mut self, cids: impl IntoIterator<Item = ObjectCid>) {
        for cid in cids {
            self.pinned_objects
                .entry(cid.into_bytes())
                .or_default()
                .insert(PinRetentionReason::LocalCollection);
        }
    }

    pub fn pin_unexpired_provider_leases(
        &mut self,
        lease_map: &ProviderLeaseMap,
        observations: &LeaseObservationStore,
        index_keys: &[[u8; 32]],
        local_tick: u64,
    ) {
        for &index_key in index_keys {
            for lease in lease_map.active_for_index(index_key, observations, local_tick) {
                match lease.body.subject {
                    ProviderSubject::SelectorRoot(_) => {}
                    ProviderSubject::ContentRoot(cid) => {
                        self.pinned_objects
                            .entry(cid)
                            .or_default()
                            .insert(PinRetentionReason::UnexpiredProviderLease);
                    }
                }
                for endpoint_ref in &lease.body.endpoint_refs {
                    self.pinned_objects
                        .entry(endpoint_ref.cid)
                        .or_default()
                        .insert(PinRetentionReason::UnexpiredProviderLease);
                }
            }
        }
    }

    pub fn pin_active_assemblies_and_resolutions(
        &mut self,
        manifests: &[FrontierAssemblyManifest],
        resolutions: &[ResolutionView],
        kernel_to_envelope_cids: &BTreeMap<[u8; 32], ObjectCid>,
    ) {
        for manifest in manifests {
            for src in &manifest.source_objects {
                self.pinned_objects
                    .entry(src.cid)
                    .or_default()
                    .insert(PinRetentionReason::ActiveAssemblyOrResolution);
            }
            for placement in &manifest.placements {
                self.pinned_objects
                    .entry(placement.receptor_definition.cid)
                    .or_default()
                    .insert(PinRetentionReason::ActiveAssemblyOrResolution);
                if let Some(policy) = &placement.resolution_policy_override {
                    self.pinned_objects
                        .entry(policy.cid)
                        .or_default()
                        .insert(PinRetentionReason::ActiveAssemblyOrResolution);
                }
            }
            self.pinned_objects
                .entry(manifest.default_resolution_policy.cid)
                .or_default()
                .insert(PinRetentionReason::ActiveAssemblyOrResolution);
        }

        for view in resolutions {
            self.pinned_objects
                .entry(view.target.assembly_revision.into_bytes())
                .or_default()
                .insert(PinRetentionReason::ActiveAssemblyOrResolution);
            self.pinned_objects
                .entry(view.resolution_policy.cid)
                .or_default()
                .insert(PinRetentionReason::ActiveAssemblyOrResolution);
            for branch in &view.branches {
                for kernel in &branch.active_bindings {
                    if let Some(envelope_cid) = kernel_to_envelope_cids.get(kernel.as_bytes()) {
                        self.pinned_objects
                            .entry(envelope_cid.into_bytes())
                            .or_default()
                            .insert(PinRetentionReason::ActiveAssemblyOrResolution);
                    }
                }
            }
        }
    }

    pub fn pin_unfinalized_p4_closures(&mut self, closures: &[BenefitClosureView]) {
        for closure in closures {
            if matches!(
                closure.state,
                BenefitClosureState::DeferredIncomplete | BenefitClosureState::ContestedOrMixed
            ) {
                for out in &closure.outcome_branches {
                    self.pinned_objects
                        .entry(out.payload_object_cid.into_bytes())
                        .or_default()
                        .insert(PinRetentionReason::UnfinalizedP4EvidenceWindow);
                }
                for ben in &closure.benefit_branches {
                    self.pinned_objects
                        .entry(ben.payload_object_cid.into_bytes())
                        .or_default()
                        .insert(PinRetentionReason::UnfinalizedP4EvidenceWindow);
                }
            }
        }
    }

    pub fn pin_unfinalized_p5_challenge_windows(
        &mut self,
        pending_mints: &[PendingMintRecord],
        claim_supporting_objects: &BTreeMap<[u8; 32], Vec<ObjectCid>>,
    ) {
        for mint in pending_mints {
            if matches!(
                mint.state,
                onebrain_obt::MintLifecycleState::PendingInChallengeWindow { .. }
            ) {
                self.pinned_objects
                    .entry(mint.policy_cid.into_bytes())
                    .or_default()
                    .insert(PinRetentionReason::UnfinalizedP5ChallengeWindow);
                if let Some(objs) = claim_supporting_objects.get(&mint.source.claim_cid()) {
                    for cid in objs {
                        self.pinned_objects
                            .entry(cid.into_bytes())
                            .or_default()
                            .insert(PinRetentionReason::UnfinalizedP5ChallengeWindow);
                    }
                }
            }
        }
    }

    pub fn pin_owned_blobs_from_oracle(
        &mut self,
        oracle: &CanonicalBlobReferenceOracle,
    ) -> Result<usize, LocalGcPinError> {
        let records = oracle
            .retained_canonical_records()
            .map_err(|e| LocalGcPinError::BlobOracle(e.to_string()))?;
        let mut count = 0usize;
        for reference in records {
            if reference.retains_blob() {
                self.pinned_blobs
                    .entry(reference.blob_cid.0)
                    .or_default()
                    .insert(PinRetentionReason::CanonicalOwnedBlobReference);
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn evaluate_object_eviction(
        &self,
        config: &VNextFeatureConfig,
        cid: &ObjectCid,
    ) -> Result<LocalEvictionDecision, LocalGcPinError> {
        if !config.is_active(VNextFeature::CheckpointGc) {
            return Err(LocalGcPinError::CheckpointGcDisabled);
        }
        match self.pinned_objects.get(cid.as_bytes()) {
            Some(reasons) if !reasons.is_empty() => Ok(LocalEvictionDecision::Pinned(
                reasons.iter().cloned().collect(),
            )),
            _ => Ok(LocalEvictionDecision::EligibleForEviction),
        }
    }

    pub fn evaluate_blob_eviction(
        &self,
        config: &VNextFeatureConfig,
        cid: &BlobCid,
    ) -> Result<LocalEvictionDecision, LocalGcPinError> {
        if !config.is_active(VNextFeature::CheckpointGc) {
            return Err(LocalGcPinError::CheckpointGcDisabled);
        }
        match self.pinned_blobs.get(&cid.0) {
            Some(reasons) if !reasons.is_empty() => Ok(LocalEvictionDecision::Pinned(
                reasons.iter().cloned().collect(),
            )),
            _ => Ok(LocalEvictionDecision::EligibleForEviction),
        }
    }
}
