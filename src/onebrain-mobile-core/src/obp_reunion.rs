use std::{fmt, path::Path};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use ku_core::foundation::{
    CorrespondenceKind, DisclosureClass, EventCid, MappingEnvelope, MappingKernel,
    MappingTermLocator, MappingTransform, ObjectReference, SemanticFrameSet, TermCorrespondence,
};
use ku_kql::vnext_proposal::{BindingProposal, ProposalExpiry};
use onebrain_core::RedbCompositionStore;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{MobileCoreError, MobileFeatureFlags};

const REUNION_TARGETS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("private_reunion_targets");
const REUNION_PROPOSALS: TableDefinition<&str, &[u8]> =
    TableDefinition::new("private_reunion_proposals");
const REUNION_DEDUPE: TableDefinition<&str, u8> = TableDefinition::new("private_reunion_dedupe");

const TARGET_AAD: &[u8] = b"onebrain:mobile:reunion-target:1\0";
const PROPOSAL_AAD: &[u8] = b"onebrain:mobile:reunion-proposal:1\0";
const NONCE_BYTES: usize = 24;
const ENVELOPE_VERSION: u8 = 1;

pub struct ReunionVaultKey(Zeroizing<[u8; 32]>);

impl ReunionVaultKey {
    pub fn derive(vault_key: &[u8; 32]) -> Self {
        Self(Zeroizing::new(blake3::derive_key(
            "onebrain:mobile:reunion-vault-key:1",
            vault_key,
        )))
    }
}

impl fmt::Debug for ReunionVaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ReunionVaultKey([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchTargetLifecycle {
    Active,
    Paused,
    Retired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReunionCoverageClass {
    Partial,
    PathLimited,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchProposalDisposition {
    PendingReview,
    Retained,
    Dismissed,
    ReEvaluateRequested,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MatchAuthorStatus {
    Unresolved,
    VerifiedActor { actor_id_hex: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivateMatchTarget {
    pub target_id: String,
    pub title: String,
    pub role_ccid: [u8; 16],
    pub expected_type_ccid: [u8; 16],
    pub lifecycle: MatchTargetLifecycle,
    pub created_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PublicCandidateAffordance {
    pub candidate_object_cid: [u8; 32],
    pub role_ccid: [u8; 16],
    pub provided_type_ccids: Vec<[u8; 16]>,
    pub qualifying_authorship_actor: Option<[u8; 32]>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuarantinedReunionProposal {
    pub proposal_ref: String,
    pub target_id: String,
    pub candidate_object_cid_hex: String,
    pub mapping_kernel_cid_hex: String,
    pub executable: bool,
    pub disclosure: String,
    pub coverage: ReunionCoverageClass,
    pub author_status: MatchAuthorStatus,
    pub disposition: MatchProposalDisposition,
    pub reunion_frontier_hex: String,
    pub matcher_version: u32,
    pub created_at_monotonic_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeatureGateStatusView {
    pub gate_id: String,
    pub compiled_in: bool,
    pub requested_by_flags: bool,
    pub active_in_runtime: bool,
    pub kill_switch_engaged: bool,
    pub safe_mode_active: bool,
    pub status_summary_code: String,
    pub network_routes_exposed: bool,
    pub matching_routes_exposed: bool,
    pub publication_routes_exposed: bool,
    pub fidelity_marketplace_routes_exposed: bool,
    pub wallet_routes_exposed: bool,
}

pub fn evaluate_feature_gate_status(
    flags: &MobileFeatureFlags,
    safe_mode_active: bool,
    kill_switch_engaged: bool,
) -> FeatureGateStatusView {
    let active = flags.obp_reunion_match && !safe_mode_active && !kill_switch_engaged;
    FeatureGateStatusView {
        gate_id: "MOB-GATE-NETWORKED-BETA".into(),
        compiled_in: true,
        requested_by_flags: flags.obp_reunion_match || flags.peer_transport,
        active_in_runtime: active,
        kill_switch_engaged,
        safe_mode_active,
        status_summary_code: if kill_switch_engaged {
            "KILL_SWITCH_DISABLED".into()
        } else if safe_mode_active {
            "SAFE_MODE_SUSPENDED".into()
        } else if active {
            "GATED_BETA_ENGINE_ENABLED_CONSUMER_NAV_HIDDEN".into()
        } else {
            "GATED_OFF_PENDING_OWNER_BETA_DECISION".into()
        },
        network_routes_exposed: false,
        matching_routes_exposed: false,
        publication_routes_exposed: false,
        fidelity_marketplace_routes_exposed: false,
        wallet_routes_exposed: false,
    }
}

pub struct PassiveObpReunionEngine {
    database: Database,
    cipher: XChaCha20Poly1305,
}

impl PassiveObpReunionEngine {
    pub fn open(path: &Path, key: ReunionVaultKey) -> Result<Self, MobileCoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                MobileCoreError::Storage(format!(
                    "cannot create reunion engine directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let database = Database::create(path)?;
        let write = database.begin_write()?;
        {
            write.open_table(REUNION_TARGETS)?;
            write.open_table(REUNION_PROPOSALS)?;
            write.open_table(REUNION_DEDUPE)?;
        }
        write.commit()?;
        Ok(Self {
            database,
            cipher: XChaCha20Poly1305::new((&*key.0).into()),
        })
    }

    fn ensure_gate_enabled(flags: &MobileFeatureFlags) -> Result<(), MobileCoreError> {
        if !flags.obp_reunion_match {
            return Err(MobileCoreError::FeatureGateDisabled(
                "MOB-GATE-NETWORKED-BETA: passive OBP reunion matching is gated off by default"
                    .into(),
            ));
        }
        Ok(())
    }

    pub fn create_private_target(
        &self,
        flags: &MobileFeatureFlags,
        title: &str,
        role_ccid: [u8; 16],
        expected_type_ccid: [u8; 16],
        now_monotonic_ms: u64,
    ) -> Result<PrivateMatchTarget, MobileCoreError> {
        Self::ensure_gate_enabled(flags)?;
        if title.trim().is_empty() || title.len() > 256 {
            return Err(MobileCoreError::InvalidArgument(
                "private matching target title must contain 1..=256 bytes".into(),
            ));
        }
        let mut id_bytes = [0u8; 16];
        getrandom::fill(&mut id_bytes)
            .map_err(|_| MobileCoreError::Security("reunion target CSPRNG unavailable".into()))?;
        let target = PrivateMatchTarget {
            target_id: format!("mtarget_{}", hex::encode(id_bytes)),
            title: title.trim().to_owned(),
            role_ccid,
            expected_type_ccid,
            lifecycle: MatchTargetLifecycle::Active,
            created_at_monotonic_ms: now_monotonic_ms,
        };
        self.write_target(&target)?;
        Ok(target)
    }

    pub fn set_target_lifecycle(
        &self,
        flags: &MobileFeatureFlags,
        target_id: &str,
        lifecycle: MatchTargetLifecycle,
    ) -> Result<PrivateMatchTarget, MobileCoreError> {
        Self::ensure_gate_enabled(flags)?;
        let mut target = self
            .read_target(target_id)?
            .ok_or_else(|| MobileCoreError::InvalidArgument("unknown target_id".into()))?;
        target.lifecycle = lifecycle;
        self.write_target(&target)?;
        Ok(target)
    }

    /// Passive local delta join (`MOB-08`): matches `Active` private targets against
    /// validated public candidate affordances without network calls, deduplicates by
    /// `(reunion_frontier, target_id, matcher_version)`, and emits quarantined
    /// `BindingProposal`s with `executable == false` and honest `PARTIAL` / `PATH_LIMITED` coverage.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_passive_reunion_delta(
        &self,
        flags: &MobileFeatureFlags,
        composition: &RedbCompositionStore,
        reunion_frontier: [u8; 32],
        matcher_version: u32,
        coverage: ReunionCoverageClass,
        candidates: &[PublicCandidateAffordance],
        now_monotonic_ms: u64,
    ) -> Result<Vec<QuarantinedReunionProposal>, MobileCoreError> {
        Self::ensure_gate_enabled(flags)?;
        let targets = self.list_targets(flags)?;
        let frontier_hex = hex::encode(reunion_frontier);
        let mut emitted = Vec::new();

        for target in targets {
            if target.lifecycle != MatchTargetLifecycle::Active {
                continue;
            }
            let dedupe_key = format!("{frontier_hex}/{}/{matcher_version}", target.target_id);
            {
                let read = self.database.begin_read()?;
                let dedupe = read.open_table(REUNION_DEDUPE)?;
                if dedupe.get(dedupe_key.as_str())?.is_some() {
                    continue;
                }
            }

            for candidate in candidates {
                if candidate.role_ccid != target.role_ccid
                    || !candidate
                        .provided_type_ccids
                        .contains(&target.expected_type_ccid)
                {
                    continue;
                }

                let mut target_hash = [0u8; 32];
                target_hash.copy_from_slice(blake3::hash(target.target_id.as_bytes()).as_bytes());
                let source_ref = ObjectReference::new(0, target_hash);
                let target_ref = ObjectReference::new(0, candidate.candidate_object_cid);
                let generator_ref = ObjectReference::new(0, [0x91; 32]);

                let kernel = MappingKernel {
                    source_objects: vec![source_ref.clone()],
                    target_objects: vec![target_ref.clone()],
                    correspondences: vec![TermCorrespondence {
                        source: MappingTermLocator {
                            object: source_ref,
                            statement_index: 0,
                            argument_index: Some(0),
                        },
                        target: MappingTermLocator {
                            object: target_ref.clone(),
                            statement_index: 0,
                            argument_index: Some(0),
                        },
                        kind: CorrespondenceKind::Equivalent,
                        transform: MappingTransform::Identity,
                    }],
                    assumptions: SemanticFrameSet {
                        statements: Vec::new(),
                    },
                    constraint_regions: Vec::new(),
                    unmapped_regions: Vec::new(),
                };
                let kernel_cid = kernel.cid().map_err(|error| {
                    MobileCoreError::Serialization(format!("mapping kernel CID failed: {error:?}"))
                })?;
                let envelope = MappingEnvelope {
                    kernel: kernel_cid,
                    generator: generator_ref,
                    derivation_rule: None,
                    evidence: vec![target_ref.clone()],
                    source_event: None,
                };
                let binding_proposal = BindingProposal {
                    mapping_kernel: kernel,
                    proposed_envelope: envelope,
                    candidate_objects: vec![target_ref],
                    index_commitment: None,
                    model_commitment: None,
                    rule_commitment: None,
                    scores: Vec::new(),
                    constraints: Vec::new(),
                    expiry: ProposalExpiry {
                        created_at_evaluation: 1,
                        expires_after_evaluations: 16,
                        source_frontier: EventCid::from_bytes(reunion_frontier),
                    },
                    privacy: DisclosureClass::LocalOnly,
                };
                let staged_id = composition
                    .stage_binding_proposal(binding_proposal)
                    .map_err(|error| {
                        MobileCoreError::Storage(format!(
                            "cannot quarantine reunion BindingProposal: {error}"
                        ))
                    })?;

                let author_status = match candidate.qualifying_authorship_actor {
                    Some(actor) => MatchAuthorStatus::VerifiedActor {
                        actor_id_hex: hex::encode(actor),
                    },
                    None => MatchAuthorStatus::Unresolved,
                };

                let proposal_ref = format!("mprop_{}", hex::encode(staged_id.as_bytes()));
                let quarantined = QuarantinedReunionProposal {
                    proposal_ref: proposal_ref.clone(),
                    target_id: target.target_id.clone(),
                    candidate_object_cid_hex: hex::encode(candidate.candidate_object_cid),
                    mapping_kernel_cid_hex: hex::encode(kernel_cid.as_bytes()),
                    executable: false,
                    disclosure: "LOCAL_ONLY".into(),
                    coverage,
                    author_status,
                    disposition: MatchProposalDisposition::PendingReview,
                    reunion_frontier_hex: frontier_hex.clone(),
                    matcher_version,
                    created_at_monotonic_ms: now_monotonic_ms,
                };
                self.write_proposal(&quarantined)?;
                emitted.push(quarantined);
            }

            let write = self.database.begin_write()?;
            {
                let mut dedupe = write.open_table(REUNION_DEDUPE)?;
                dedupe.insert(dedupe_key.as_str(), 1)?;
            }
            write.commit()?;
        }

        Ok(emitted)
    }

    pub fn update_proposal_disposition(
        &self,
        flags: &MobileFeatureFlags,
        proposal_ref: &str,
        disposition: MatchProposalDisposition,
    ) -> Result<QuarantinedReunionProposal, MobileCoreError> {
        Self::ensure_gate_enabled(flags)?;
        let mut proposal = self
            .read_proposal(proposal_ref)?
            .ok_or_else(|| MobileCoreError::InvalidArgument("unknown proposal_ref".into()))?;
        proposal.disposition = disposition;
        if disposition == MatchProposalDisposition::ReEvaluateRequested {
            let dedupe_key = format!(
                "{}/{}/{}",
                proposal.reunion_frontier_hex, proposal.target_id, proposal.matcher_version
            );
            let write = self.database.begin_write()?;
            {
                let mut dedupe = write.open_table(REUNION_DEDUPE)?;
                let _ = dedupe.remove(dedupe_key.as_str())?;
            }
            write.commit()?;
        }
        self.write_proposal(&proposal)?;
        Ok(proposal)
    }

    pub fn list_targets(
        &self,
        flags: &MobileFeatureFlags,
    ) -> Result<Vec<PrivateMatchTarget>, MobileCoreError> {
        Self::ensure_gate_enabled(flags)?;
        let read = self.database.begin_read()?;
        let table = read.open_table(REUNION_TARGETS)?;
        let mut out = Vec::new();
        for row in table.iter()? {
            let (key, sealed) = row?;
            let plaintext = self.open_sealed(TARGET_AAD, key.value(), sealed.value())?;
            out.push(serde_json::from_slice(&plaintext)?);
        }
        out.sort_by(|left: &PrivateMatchTarget, right: &PrivateMatchTarget| {
            left.created_at_monotonic_ms
                .cmp(&right.created_at_monotonic_ms)
                .then_with(|| left.target_id.cmp(&right.target_id))
        });
        Ok(out)
    }

    pub fn list_proposals(
        &self,
        flags: &MobileFeatureFlags,
    ) -> Result<Vec<QuarantinedReunionProposal>, MobileCoreError> {
        Self::ensure_gate_enabled(flags)?;
        let read = self.database.begin_read()?;
        let table = read.open_table(REUNION_PROPOSALS)?;
        let mut out = Vec::new();
        for row in table.iter()? {
            let (key, sealed) = row?;
            let plaintext = self.open_sealed(PROPOSAL_AAD, key.value(), sealed.value())?;
            out.push(serde_json::from_slice(&plaintext)?);
        }
        out.sort_by(
            |left: &QuarantinedReunionProposal, right: &QuarantinedReunionProposal| {
                right
                    .created_at_monotonic_ms
                    .cmp(&left.created_at_monotonic_ms)
                    .then_with(|| left.proposal_ref.cmp(&right.proposal_ref))
            },
        );
        Ok(out)
    }

    fn read_target(&self, target_id: &str) -> Result<Option<PrivateMatchTarget>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REUNION_TARGETS)?;
        let Some(sealed) = table.get(target_id)? else {
            return Ok(None);
        };
        let plaintext = self.open_sealed(TARGET_AAD, target_id, sealed.value())?;
        Ok(Some(serde_json::from_slice(&plaintext)?))
    }

    fn write_target(&self, target: &PrivateMatchTarget) -> Result<(), MobileCoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(target)?);
        let sealed = self.seal(TARGET_AAD, &target.target_id, &plaintext)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(REUNION_TARGETS)?;
            table.insert(target.target_id.as_str(), sealed.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn read_proposal(
        &self,
        proposal_ref: &str,
    ) -> Result<Option<QuarantinedReunionProposal>, MobileCoreError> {
        let read = self.database.begin_read()?;
        let table = read.open_table(REUNION_PROPOSALS)?;
        let Some(sealed) = table.get(proposal_ref)? else {
            return Ok(None);
        };
        let plaintext = self.open_sealed(PROPOSAL_AAD, proposal_ref, sealed.value())?;
        Ok(Some(serde_json::from_slice(&plaintext)?))
    }

    fn write_proposal(&self, proposal: &QuarantinedReunionProposal) -> Result<(), MobileCoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(proposal)?);
        let sealed = self.seal(PROPOSAL_AAD, &proposal.proposal_ref, &plaintext)?;
        let write = self.database.begin_write()?;
        {
            let mut table = write.open_table(REUNION_PROPOSALS)?;
            table.insert(proposal.proposal_ref.as_str(), sealed.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn seal(
        &self,
        context: &[u8],
        object_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        let mut nonce_bytes = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce_bytes)
            .map_err(|_| MobileCoreError::Security("reunion CSPRNG unavailable".into()))?;
        let mut aad = Vec::with_capacity(context.len() + object_id.len());
        aad.extend_from_slice(context);
        aad.extend_from_slice(object_id.as_bytes());
        let ciphertext = self
            .cipher
            .encrypt(
                &XNonce::from(nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| MobileCoreError::Security("cannot seal reunion record".into()))?;
        let mut out = Vec::with_capacity(1 + NONCE_BYTES + ciphertext.len());
        out.push(ENVELOPE_VERSION);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn open_sealed(
        &self,
        context: &[u8],
        object_id: &str,
        sealed: &[u8],
    ) -> Result<Vec<u8>, MobileCoreError> {
        if sealed.len() < 1 + NONCE_BYTES + 16 || sealed[0] != ENVELOPE_VERSION {
            return Err(MobileCoreError::Security(
                "reunion sealed envelope is invalid".into(),
            ));
        }
        let nonce = XNonce::try_from(&sealed[1..1 + NONCE_BYTES])
            .map_err(|_| MobileCoreError::Security("reunion nonce is invalid".into()))?;
        let mut aad = Vec::with_capacity(context.len() + object_id.len());
        aad.extend_from_slice(context);
        aad.extend_from_slice(object_id.as_bytes());
        self.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed[1 + NONCE_BYTES..],
                    aad: &aad,
                },
            )
            .map_err(|_| MobileCoreError::Security("reunion record authentication failed".into()))
    }
}
