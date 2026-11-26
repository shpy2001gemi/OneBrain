//! Local `FidelityAssessmentReducer` Portfolio Persistence (`ARCH-10` / `MOB-FID-001/007/008` & node parity).
//!
//! Persists policy-versioned encoding-fidelity targets, publisher `EncodingAttempt`s,
//! signed external `ValidatedEncodingFidelityAttestationEvent`s, and `LegacyEncodingClaim`s
//! in `redb`, re-validating canonical and cryptographic proofs on reload and exposing
//! honest frontier-relative status + 4D `CorroborationGroupSummary` views.

use std::collections::BTreeMap;
use std::path::Path;

use ku_core::foundation::{
    decode_feed_inception, decode_knowledge_event, decode_knowledge_object,
    group_attestations_by_4d_correlation, CorroborationGroupSummary, EncodingAttempt, EventCid,
    FidelityAssessment, FidelityAssessmentReducer, FidelityAssessmentStatus, FidelityPolicy,
    FidelityRecordOutcome, KnownObjectKind, LegacyEncodingClaim, ObjectReference, ResourceProfile,
    ValidatedEncodingFidelityAttestationEvent, ValidatedFeedInception,
    ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE, ENCODING_FIDELITY_ATTESTATION_KIND,
    FIDELITY_PROFILE_MAJOR,
};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::error::CoreEngineError;

const FID_TARGETS_V1: TableDefinition<&[u8; 32], &[u8]> = TableDefinition::new("fid_targets_v1");
const FID_PUBLISHER_ATTEMPTS_V1: TableDefinition<&[u8; 64], &[u8]> =
    TableDefinition::new("fid_publisher_attempts_v1");
const FID_ATTESTATIONS_V1: TableDefinition<&[u8; 64], &[u8]> =
    TableDefinition::new("fid_attestations_v1");
const FID_LEGACY_CLAIMS_V1: TableDefinition<&[u8; 64], &[u8]> =
    TableDefinition::new("fid_legacy_claims_v1");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FidelityPortfolioTargetKey([u8; 32]);

impl FidelityPortfolioTargetKey {
    pub fn derive(
        policy_ref: &ObjectReference,
        source_artifact: &ObjectReference,
        encoding_artifact: &ObjectReference,
    ) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"onebrain:core:fidelity-portfolio-target:1\0");
        h.update(&policy_ref.reference_kind.to_be_bytes());
        h.update(&policy_ref.cid);
        h.update(&source_artifact.reference_kind.to_be_bytes());
        h.update(&source_artifact.cid);
        h.update(&encoding_artifact.reference_kind.to_be_bytes());
        h.update(&encoding_artifact.cid);
        Self(*h.finalize().as_bytes())
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Honest, non-authoritative portfolio status view for `MOB-FID-001/007/008` and node parity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityPortfolioStatusView {
    pub target_key: FidelityPortfolioTargetKey,
    pub assessment: Option<FidelityAssessment>,
    pub corroboration_4d: CorroborationGroupSummary,
}

impl FidelityPortfolioStatusView {
    pub fn relative_status(&self) -> Option<FidelityAssessmentStatus> {
        self.assessment.as_ref().map(|a| a.status)
    }

    pub const fn is_global_or_final(&self) -> bool {
        false
    }

    pub const fn establishes_proposition_truth(&self) -> bool {
        false
    }

    pub const fn blocks_preserve_publish_query_or_use(&self) -> bool {
        false
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedPolicySpec {
    publisher_attempt_required: bool,
    minimum_external_blind_attempts: u16,
    minimum_evidenced_distinct_external_groups: u16,
    required_distinct_dimensions: Vec<u64>,
    accepted_strengths_by_dimension: Vec<(u64, Vec<u64>)>,
    required_checks: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
struct PersistedTargetRow {
    policy_ref: (u64, [u8; 32]),
    policy: PersistedPolicySpec,
    source_artifact: (u64, [u8; 32]),
    encoding_artifact: (u64, [u8; 32]),
    assessed_frontier: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct PersistedPublisherAttemptRow {
    attempt_ref: (u64, [u8; 32]),
    source_artifact: (u64, [u8; 32]),
    candidate_encoding: (u64, [u8; 32]),
    output_commitment: [u8; 32],
    pipeline_model_tool_commitments: Vec<[u8; 32]>,
    source_acquisition_or_derivation_commitment: [u8; 32],
    execution_record_ref: (u64, [u8; 32]),
}

#[derive(Serialize, Deserialize)]
struct PersistedAttestationRow {
    feed_bytes: Vec<u8>,
    attestation_object_bytes: Vec<u8>,
    event_bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct PersistedLegacyClaimRow {
    source_artifact: (u64, [u8; 32]),
    encoding_artifact: (u64, [u8; 32]),
    imported_evidence_ref: (u64, [u8; 32]),
    adapter_profile_commitment: [u8; 32],
    normalized_claim_commitment: [u8; 32],
    limitations: Vec<[u8; 16]>,
}

struct TargetSlot {
    reducer: FidelityAssessmentReducer,
    attestation_events: BTreeMap<[u8; 32], ValidatedEncodingFidelityAttestationEvent>,
}

/// Synchronous `redb`-backed encoding-fidelity portfolio (`ARCH-10`).
pub struct RedbFidelityPortfolio {
    db: Database,
    targets: BTreeMap<FidelityPortfolioTargetKey, TargetSlot>,
}

impl RedbFidelityPortfolio {
    pub fn open(path: &Path) -> Result<Self, CoreEngineError> {
        let db = Database::create(path)?;
        {
            let write_txn = db.begin_write()?;
            let _ = write_txn.open_table(FID_TARGETS_V1)?;
            let _ = write_txn.open_table(FID_PUBLISHER_ATTEMPTS_V1)?;
            let _ = write_txn.open_table(FID_ATTESTATIONS_V1)?;
            let _ = write_txn.open_table(FID_LEGACY_CLAIMS_V1)?;
            write_txn.commit()?;
        }

        let mut portfolio = Self {
            db,
            targets: BTreeMap::new(),
        };
        portfolio.rebuild_from_storage()?;
        Ok(portfolio)
    }

    pub fn register_target(
        &mut self,
        policy_ref: ObjectReference,
        policy: FidelityPolicy,
        source_artifact: ObjectReference,
        encoding_artifact: ObjectReference,
        assessed_frontier: EventCid,
    ) -> Result<FidelityPortfolioTargetKey, CoreEngineError> {
        let key =
            FidelityPortfolioTargetKey::derive(&policy_ref, &source_artifact, &encoding_artifact);
        if self.targets.contains_key(&key) {
            return Ok(key);
        }
        let reducer = FidelityAssessmentReducer::new(
            policy_ref.clone(),
            policy.clone(),
            source_artifact.clone(),
            encoding_artifact.clone(),
            assessed_frontier,
        )?;
        let row = PersistedTargetRow {
            policy_ref: (policy_ref.reference_kind, policy_ref.cid),
            policy: encode_policy_spec(&policy),
            source_artifact: (source_artifact.reference_kind, source_artifact.cid),
            encoding_artifact: (encoding_artifact.reference_kind, encoding_artifact.cid),
            assessed_frontier: assessed_frontier.into_bytes(),
        };
        let bytes = serde_json::to_vec(&row)
            .map_err(|_| CoreEngineError::InvalidInput("target_serialize"))?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(FID_TARGETS_V1)?;
            table.insert(key.as_bytes(), bytes.as_slice())?;
        }
        write_txn.commit()?;

        self.targets.insert(
            key,
            TargetSlot {
                reducer,
                attestation_events: BTreeMap::new(),
            },
        );
        Ok(key)
    }

    pub fn record_publisher_attempt(
        &mut self,
        target: FidelityPortfolioTargetKey,
        attempt_ref: ObjectReference,
        attempt: EncodingAttempt,
    ) -> Result<FidelityRecordOutcome, CoreEngineError> {
        let Some(slot) = self.targets.get_mut(&target) else {
            return Err(CoreEngineError::InvalidInput("unknown_fidelity_target"));
        };
        let cand = attempt
            .candidate_encoding
            .clone()
            .ok_or(CoreEngineError::InvalidInput("missing_publisher_candidate"))?;
        let outcome = slot
            .reducer
            .record_publisher_attempt(attempt_ref.clone(), attempt.clone())?;

        if outcome == FidelityRecordOutcome::Added {
            let row = PersistedPublisherAttemptRow {
                attempt_ref: (attempt_ref.reference_kind, attempt_ref.cid),
                source_artifact: (
                    attempt.source_artifact.reference_kind,
                    attempt.source_artifact.cid,
                ),
                candidate_encoding: (cand.reference_kind, cand.cid),
                output_commitment: attempt.output_commitment,
                pipeline_model_tool_commitments: attempt.pipeline_model_tool_commitments,
                source_acquisition_or_derivation_commitment: attempt
                    .source_acquisition_or_derivation_commitment,
                execution_record_ref: (
                    attempt.execution_record_ref.reference_kind,
                    attempt.execution_record_ref.cid,
                ),
            };
            let encoded = serde_json::to_vec(&row)
                .map_err(|_| CoreEngineError::InvalidInput("attempt_serialize"))?;
            let k64 = compound_key(target.as_bytes(), &attempt_ref.cid);
            let write_txn = self.db.begin_write()?;
            {
                let mut table = write_txn.open_table(FID_PUBLISHER_ATTEMPTS_V1)?;
                table.insert(&k64, encoded.as_slice())?;
            }
            write_txn.commit()?;
        }
        Ok(outcome)
    }

    pub fn record_signed_attestation(
        &mut self,
        target: FidelityPortfolioTargetKey,
        feed: &ValidatedFeedInception,
        attestation_object_bytes: &[u8],
        event_bytes: &[u8],
    ) -> Result<FidelityRecordOutcome, CoreEngineError> {
        let Some(slot) = self.targets.get_mut(&target) else {
            return Err(CoreEngineError::InvalidInput("unknown_fidelity_target"));
        };
        let v_obj = decode_knowledge_object(
            attestation_object_bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(
                ENCODING_FIDELITY_ATTESTATION_KIND,
                FIDELITY_PROFILE_MAJOR,
            )],
            &[],
        )?;
        let v_ev = decode_knowledge_event(
            event_bytes,
            feed,
            &[ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE],
        )
        .map_err(|_| CoreEngineError::InvalidInput("invalid_fidelity_event"))?;
        let bound = ValidatedEncodingFidelityAttestationEvent::bind(&v_ev, &v_obj)?;
        let ev_cid_bytes = bound.event_cid().into_bytes();

        let outcome = slot.reducer.record_attestation(bound.clone())?;
        slot.attestation_events.insert(ev_cid_bytes, bound);

        if outcome == FidelityRecordOutcome::Added {
            let row = PersistedAttestationRow {
                feed_bytes: feed.original_bytes().to_vec(),
                attestation_object_bytes: attestation_object_bytes.to_vec(),
                event_bytes: event_bytes.to_vec(),
            };
            let encoded = serde_json::to_vec(&row)
                .map_err(|_| CoreEngineError::InvalidInput("attestation_serialize"))?;
            let k64 = compound_key(target.as_bytes(), &ev_cid_bytes);
            let write_txn = self.db.begin_write()?;
            {
                let mut table = write_txn.open_table(FID_ATTESTATIONS_V1)?;
                table.insert(&k64, encoded.as_slice())?;
            }
            write_txn.commit()?;
        }
        Ok(outcome)
    }

    pub fn record_legacy_claim(
        &mut self,
        target: FidelityPortfolioTargetKey,
        claim: LegacyEncodingClaim,
    ) -> Result<FidelityRecordOutcome, CoreEngineError> {
        let Some(slot) = self.targets.get_mut(&target) else {
            return Err(CoreEngineError::InvalidInput("unknown_fidelity_target"));
        };
        let claim_id = blake3::hash(&claim.normalized_claim_commitment);
        let outcome = slot.reducer.record_legacy_claim(claim.clone())?;
        if outcome == FidelityRecordOutcome::Added {
            let row = PersistedLegacyClaimRow {
                source_artifact: (
                    claim.source_artifact.reference_kind,
                    claim.source_artifact.cid,
                ),
                encoding_artifact: (
                    claim.encoding_artifact.reference_kind,
                    claim.encoding_artifact.cid,
                ),
                imported_evidence_ref: (
                    claim.imported_evidence_ref.reference_kind,
                    claim.imported_evidence_ref.cid,
                ),
                adapter_profile_commitment: claim.adapter_profile_commitment,
                normalized_claim_commitment: claim.normalized_claim_commitment,
                limitations: claim
                    .limitations
                    .into_iter()
                    .map(|c| *c.as_bytes())
                    .collect(),
            };
            let encoded = serde_json::to_vec(&row)
                .map_err(|_| CoreEngineError::InvalidInput("legacy_claim_serialize"))?;
            let k64 = compound_key(target.as_bytes(), claim_id.as_bytes());
            let write_txn = self.db.begin_write()?;
            {
                let mut table = write_txn.open_table(FID_LEGACY_CLAIMS_V1)?;
                table.insert(&k64, encoded.as_slice())?;
            }
            write_txn.commit()?;
        }
        Ok(outcome)
    }

    pub fn status_view(
        &self,
        target: FidelityPortfolioTargetKey,
    ) -> Result<FidelityPortfolioStatusView, CoreEngineError> {
        let Some(slot) = self.targets.get(&target) else {
            return Err(CoreEngineError::InvalidInput("unknown_fidelity_target"));
        };
        let assessment = match slot.reducer.assess() {
            Ok(a) => Some(a),
            Err(ku_core::foundation::FidelityAssessmentError::NoVNextEvidence) => None,
            Err(other) => return Err(CoreEngineError::FidelityAssessment(other)),
        };
        let events: Vec<ValidatedEncodingFidelityAttestationEvent> =
            slot.attestation_events.values().cloned().collect();
        let corroboration_4d = group_attestations_by_4d_correlation(&events)?;
        Ok(FidelityPortfolioStatusView {
            target_key: target,
            assessment,
            corroboration_4d,
        })
    }

    pub fn list_targets(&self) -> Vec<FidelityPortfolioTargetKey> {
        self.targets.keys().copied().collect()
    }

    fn rebuild_from_storage(&mut self) -> Result<(), CoreEngineError> {
        self.targets.clear();
        let read_txn = self.db.begin_read()?;

        let t_table = read_txn.open_table(FID_TARGETS_V1)?;
        for item in t_table.iter()? {
            let (k_g, v_g) = item?;
            let key = FidelityPortfolioTargetKey(*k_g.value());
            let row: PersistedTargetRow = serde_json::from_slice(v_g.value())
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_target_row"))?;
            let policy = decode_policy_spec(&row.policy)?;
            let reducer = FidelityAssessmentReducer::new(
                ObjectReference::new(row.policy_ref.0, row.policy_ref.1),
                policy,
                ObjectReference::new(row.source_artifact.0, row.source_artifact.1),
                ObjectReference::new(row.encoding_artifact.0, row.encoding_artifact.1),
                EventCid::from_bytes(row.assessed_frontier),
            )?;
            self.targets.insert(
                key,
                TargetSlot {
                    reducer,
                    attestation_events: BTreeMap::new(),
                },
            );
        }

        let p_table = read_txn.open_table(FID_PUBLISHER_ATTEMPTS_V1)?;
        for item in p_table.iter()? {
            let (k_g, v_g) = item?;
            let mut t_bytes = [0u8; 32];
            t_bytes.copy_from_slice(&k_g.value()[..32]);
            let t_key = FidelityPortfolioTargetKey(t_bytes);
            let row: PersistedPublisherAttemptRow = serde_json::from_slice(v_g.value())
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_attempt_row"))?;
            if let Some(slot) = self.targets.get_mut(&t_key) {
                let attempt = EncodingAttempt {
                    role: ku_core::foundation::EncodingAttemptRole::Publisher,
                    source_artifact: ObjectReference::new(
                        row.source_artifact.0,
                        row.source_artifact.1,
                    ),
                    candidate_encoding: Some(ObjectReference::new(
                        row.candidate_encoding.0,
                        row.candidate_encoding.1,
                    )),
                    output_commitment: row.output_commitment,
                    pipeline_model_tool_commitments: row.pipeline_model_tool_commitments,
                    source_acquisition_or_derivation_commitment: row
                        .source_acquisition_or_derivation_commitment,
                    execution_record_ref: ObjectReference::new(
                        row.execution_record_ref.0,
                        row.execution_record_ref.1,
                    ),
                    blind_session_commitment: None,
                    challenge_nonce_commitment: None,
                };
                slot.reducer.record_publisher_attempt(
                    ObjectReference::new(row.attempt_ref.0, row.attempt_ref.1),
                    attempt,
                )?;
            }
        }

        let a_table = read_txn.open_table(FID_ATTESTATIONS_V1)?;
        for item in a_table.iter()? {
            let (k_g, v_g) = item?;
            let mut t_bytes = [0u8; 32];
            t_bytes.copy_from_slice(&k_g.value()[..32]);
            let t_key = FidelityPortfolioTargetKey(t_bytes);
            let row: PersistedAttestationRow = serde_json::from_slice(v_g.value())
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_attestation_row"))?;
            if let Some(slot) = self.targets.get_mut(&t_key) {
                let feed = decode_feed_inception(&row.feed_bytes)
                    .map_err(|_| CoreEngineError::InvalidInput("corrupt_attestation_feed"))?;
                let v_obj = decode_knowledge_object(
                    &row.attestation_object_bytes,
                    ResourceProfile::ObjectV1,
                    &[KnownObjectKind::new(
                        ENCODING_FIDELITY_ATTESTATION_KIND,
                        FIDELITY_PROFILE_MAJOR,
                    )],
                    &[],
                )?;
                let v_ev = decode_knowledge_event(
                    &row.event_bytes,
                    &feed,
                    &[ENCODING_FIDELITY_ATTESTATION_EVENT_TYPE],
                )
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_attestation_event"))?;
                let bound = ValidatedEncodingFidelityAttestationEvent::bind(&v_ev, &v_obj)?;
                slot.attestation_events
                    .insert(bound.event_cid().into_bytes(), bound.clone());
                slot.reducer.record_attestation(bound)?;
            }
        }

        let l_table = read_txn.open_table(FID_LEGACY_CLAIMS_V1)?;
        for item in l_table.iter()? {
            let (k_g, v_g) = item?;
            let mut t_bytes = [0u8; 32];
            t_bytes.copy_from_slice(&k_g.value()[..32]);
            let t_key = FidelityPortfolioTargetKey(t_bytes);
            let row: PersistedLegacyClaimRow = serde_json::from_slice(v_g.value())
                .map_err(|_| CoreEngineError::InvalidInput("corrupt_legacy_claim_row"))?;
            if let Some(slot) = self.targets.get_mut(&t_key) {
                let claim = LegacyEncodingClaim {
                    source_artifact: ObjectReference::new(
                        row.source_artifact.0,
                        row.source_artifact.1,
                    ),
                    encoding_artifact: ObjectReference::new(
                        row.encoding_artifact.0,
                        row.encoding_artifact.1,
                    ),
                    imported_evidence_ref: ObjectReference::new(
                        row.imported_evidence_ref.0,
                        row.imported_evidence_ref.1,
                    ),
                    adapter_profile_commitment: row.adapter_profile_commitment,
                    normalized_claim_commitment: row.normalized_claim_commitment,
                    limitations: row
                        .limitations
                        .into_iter()
                        .map(ku_core::foundation::ConceptCcid::from_bytes)
                        .collect(),
                };
                slot.reducer.record_legacy_claim(claim)?;
            }
        }

        Ok(())
    }
}

fn compound_key(target: &[u8; 32], item: &[u8; 32]) -> [u8; 64] {
    let mut k = [0u8; 64];
    k[..32].copy_from_slice(target);
    k[32..].copy_from_slice(item);
    k
}

fn encode_policy_spec(policy: &FidelityPolicy) -> PersistedPolicySpec {
    PersistedPolicySpec {
        publisher_attempt_required: policy.publisher_attempt_required,
        minimum_external_blind_attempts: policy.minimum_external_blind_attempts,
        minimum_evidenced_distinct_external_groups: policy
            .minimum_evidenced_distinct_external_groups,
        required_distinct_dimensions: policy
            .required_distinct_dimensions
            .iter()
            .map(|d| *d as u64)
            .collect(),
        accepted_strengths_by_dimension: policy
            .accepted_strengths_by_dimension
            .iter()
            .map(|(d, list)| (*d as u64, list.iter().map(|s| *s as u64).collect()))
            .collect(),
        required_checks: policy.required_checks.iter().map(|c| *c as u64).collect(),
    }
}

fn decode_policy_spec(spec: &PersistedPolicySpec) -> Result<FidelityPolicy, CoreEngineError> {
    use ku_core::foundation::{CorrelationDimension, EvidenceStrength, FidelityCheckKind};

    let map_dim = |code: u64| -> Result<CorrelationDimension, CoreEngineError> {
        match code {
            0 => Ok(CorrelationDimension::AdministrativePrincipal),
            1 => Ok(CorrelationDimension::DeviceOrFeed),
            2 => Ok(CorrelationDimension::PipelineModelLineage),
            3 => Ok(CorrelationDimension::PromptTemplate),
            4 => Ok(CorrelationDimension::Preprocessing),
            5 => Ok(CorrelationDimension::SourceAcquisitionOrDerivation),
            6 => Ok(CorrelationDimension::ExecutionEnvironment),
            7 => Ok(CorrelationDimension::BlindSession),
            8 => Ok(CorrelationDimension::ChallengeNonce),
            9 => Ok(CorrelationDimension::CausalStakeholderRelation),
            _ => Err(CoreEngineError::InvalidInput("policy_dimension")),
        }
    };
    let map_strength = |code: u64| -> Result<EvidenceStrength, CoreEngineError> {
        match code {
            0 => Ok(EvidenceStrength::Unknown),
            1 => Ok(EvidenceStrength::SelfClaimed),
            2 => Ok(EvidenceStrength::CryptoBound),
            3 => Ok(EvidenceStrength::ExternallyAttested),
            4 => Ok(EvidenceStrength::EmpiricallyEstimated),
            _ => Err(CoreEngineError::InvalidInput("policy_strength")),
        }
    };
    let map_check = |code: u64| -> Result<FidelityCheckKind, CoreEngineError> {
        match code {
            0 => Ok(FidelityCheckKind::SourceSpanAlignment),
            1 => Ok(FidelityCheckKind::GeneSelection),
            2 => Ok(FidelityCheckKind::ConceptSelection),
            _ => Err(CoreEngineError::InvalidInput("policy_check")),
        }
    };

    let mut req_dims = Vec::with_capacity(spec.required_distinct_dimensions.len());
    for &c in &spec.required_distinct_dimensions {
        req_dims.push(map_dim(c)?);
    }
    let mut accepted = Vec::with_capacity(spec.accepted_strengths_by_dimension.len());
    for (d_code, s_codes) in &spec.accepted_strengths_by_dimension {
        let mut s_list = Vec::with_capacity(s_codes.len());
        for &sc in s_codes {
            s_list.push(map_strength(sc)?);
        }
        accepted.push((map_dim(*d_code)?, s_list));
    }
    let mut checks = Vec::with_capacity(spec.required_checks.len());
    for &cc in &spec.required_checks {
        checks.push(map_check(cc)?);
    }

    Ok(FidelityPolicy {
        publisher_attempt_required: spec.publisher_attempt_required,
        minimum_external_blind_attempts: spec.minimum_external_blind_attempts,
        minimum_evidenced_distinct_external_groups: spec.minimum_evidenced_distinct_external_groups,
        required_distinct_dimensions: req_dims,
        accepted_strengths_by_dimension: accepted,
        required_checks: checks,
    })
}
