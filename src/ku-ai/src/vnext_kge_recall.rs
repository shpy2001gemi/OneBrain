//! P7 Grounded KGE Recall (`FW-03`), `FedR` Operator Quarantine (`obp/obkg-fedr/1`, `OBS-002`, `FW-07`),
//! and `DreamConsolidatorV1` inside `TypedCognitiveExecutor` (`NEG-MAP-004`).
//!
//! Key architectural invariants enforced by construction:
//! - **Grounded KGE Initialization:** Every concept embedding vector is deterministically derived
//!   from `BLAKE3("onebrain:vnext:kge-init:1\0" || registry_release_root || ccid || structural_sig_digest)`.
//!   Legacy `Cid::to_bytes()` hash-seeded initialization is explicitly rejected fail-closed.
//! - **Recall-Only Firewall (`FW-03`):** `GroundedKgeRecallAdapter` implements `CandidateRecallAdapter`
//!   only. It has zero symbolic, unit-compatibility, mapping-materialization, or execution authority.
//! - **`FedR` Quarantine (`OBS-002`, `FW-07`):** Default-off (`fedr_enabled == false`). When enabled,
//!   `FedRQuarantineValidator` enforces exact `(registry_release_root, embedding_space_spec_cid)`
//!   lineage and integer L2 Frobenius norm-clipping ceilings, emitting non-executable
//!   `VaultQuarantineRecord`s rather than mutating active KGE weights directly, with zero
//!   `obkg_rewards` emission.
//! - **`DreamConsolidatorV1` (`NEG-MAP-004`):** Runs step-by-step inside `TypedCognitiveExecutor`
//!   (`vnext_executor.rs`), discovers transitive closures, opposing-claim clusters, and standing-need
//!   multi-hop bridges, validates every candidate through `SymbolicMappingValidator`, and emits
//!   strictly non-executable `DreamConsolidationProposal`s (`is_executable() == false`).

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, CanonicalError, CanonicalValue, ConceptCcid,
    ExactRatio, MappingKernelCid, ObjectCid, ObjectReference, PermitValidator, ReservedDomain,
    ResourceProfile, SelectorCid, StoredRecordKind, VaultQuarantineRecord,
};

use crate::vnext_executor::{
    cognitive_input_commitment, CancellationToken, CognitiveExecutionError,
    CognitiveExecutionResult, CognitiveStep, CognitiveStepRequest, CognitiveTask,
    CognitiveTermination, TypedCapabilityBackend, TypedCognitiveExecutor,
};
use crate::vnext_model_recall::{
    CandidateRecallAdapter, ModelRecallError, ModelRecallEvidence, ModelRecallPage,
    ModelRecallRequest, ModelScoredCandidate, RecallCandidateSeed, SymbolicDisposition,
    SymbolicMappingAssessment, SymbolicMappingValidator, SymbolicValidationRequest,
};

pub const KGE_INIT_DOMAIN: &[u8] = b"onebrain:vnext:kge-init:1\0";
pub const FEDR_PROTOCOL_ID: &str = "obp/obkg-fedr/1";
pub const FEDR_QUARANTINE_REASON_CODE: &str = "OBS-002:obkg-fedr-quarantine-v1";
pub const DREAM_CONSOLIDATOR_PROFILE_ID: &str = "obkg-dream-consolidator/1";
pub const DEFAULT_KGE_EMBEDDING_DIM: usize = 32;
pub const MAX_KGE_EMBEDDING_DIM: usize = 256;
pub const KGE_FIXED_POINT_SCALE: i64 = 1_000_000;
const FEDR_MAJOR: u64 = 1;
const DREAM_MAJOR: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KgeRecallError {
    #[error("canonical encoding error: {0:?}")]
    Canonical(CanonicalError),
    #[error("ungrounded KGE initialization requires non-zero registry_release_root, ccid, and structural_sig_digest")]
    UngroundedInitialization,
    #[error("random or legacy Cid::to_bytes() KGE seed initialization is forbidden")]
    RandomCidSeedForbidden,
    #[error("invalid KGE embedding dimension {0} (must be 1..={MAX_KGE_EMBEDDING_DIM})")]
    InvalidDimension(usize),
    #[error("invalid KGE configuration or candidate metadata")]
    InvalidConfiguration,
}

impl From<CanonicalError> for KgeRecallError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

/// Initialization source discriminated so legacy `Cid::to_bytes()` seeds fail closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KgeEmbeddingInitSource {
    Grounded {
        registry_release_root: [u8; 32],
        ccid: ConceptCcid,
        structural_sig_digest: [u8; 32],
    },
    /// Legacy random seed derived from raw `Cid::to_bytes()` — strictly forbidden in vNext.
    LegacyCidBytes([u8; 32]),
}

/// Derive the canonical 32-byte KGE initialization seed bound to
/// `(registry_release_root, ccid, structural_sig_digest)`.
pub fn derive_grounded_kge_seed(
    registry_release_root: [u8; 32],
    ccid: ConceptCcid,
    structural_sig_digest: [u8; 32],
) -> Result<[u8; 32], KgeRecallError> {
    if registry_release_root == [0; 32]
        || ccid.as_bytes() == &[0; 16]
        || structural_sig_digest == [0; 32]
    {
        return Err(KgeRecallError::UngroundedInitialization);
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(KGE_INIT_DOMAIN);
    hasher.update(&registry_release_root);
    hasher.update(ccid.as_bytes());
    hasher.update(&structural_sig_digest);
    Ok(*hasher.finalize().as_bytes())
}

/// Deterministic fixed-point concept embedding vector in `[-KGE_FIXED_POINT_SCALE, KGE_FIXED_POINT_SCALE]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundedConceptEmbedding {
    pub registry_release_root: [u8; 32],
    pub ccid: ConceptCcid,
    pub structural_sig_digest: [u8; 32],
    pub init_seed: [u8; 32],
    pub components_micros: Vec<i32>,
}

impl GroundedConceptEmbedding {
    pub fn from_source(
        source: KgeEmbeddingInitSource,
        dimension: usize,
    ) -> Result<Self, KgeRecallError> {
        if dimension == 0 || dimension > MAX_KGE_EMBEDDING_DIM {
            return Err(KgeRecallError::InvalidDimension(dimension));
        }
        let (registry_release_root, ccid, structural_sig_digest) = match source {
            KgeEmbeddingInitSource::Grounded {
                registry_release_root,
                ccid,
                structural_sig_digest,
            } => (registry_release_root, ccid, structural_sig_digest),
            KgeEmbeddingInitSource::LegacyCidBytes(_) => {
                return Err(KgeRecallError::RandomCidSeedForbidden);
            }
        };
        let init_seed =
            derive_grounded_kge_seed(registry_release_root, ccid, structural_sig_digest)?;

        let mut hasher = blake3::Hasher::new();
        hasher.update(KGE_INIT_DOMAIN);
        hasher.update(&registry_release_root);
        hasher.update(ccid.as_bytes());
        hasher.update(&structural_sig_digest);
        let mut xof = hasher.finalize_xof();
        let mut raw = vec![0u8; dimension * 4];
        xof.fill(&mut raw);

        let span = (2 * KGE_FIXED_POINT_SCALE + 1) as u64;
        let components_micros = raw
            .chunks_exact(4)
            .map(|chunk| {
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(chunk);
                let sample = u64::from(u32::from_le_bytes(bytes));
                let centered = (sample % span) as i64 - KGE_FIXED_POINT_SCALE;
                centered as i32
            })
            .collect();

        Ok(Self {
            registry_release_root,
            ccid,
            structural_sig_digest,
            init_seed,
            components_micros,
        })
    }

    /// Deterministic rational cosine-like similarity normalized to `[0, 1]` using exact integers.
    pub fn normalized_similarity(&self, other: &Self) -> Option<ExactRatio> {
        if self.components_micros.len() != other.components_micros.len()
            || self.components_micros.is_empty()
        {
            return None;
        }
        let mut dot: i128 = 0;
        let mut max_abs_pair: i128 = 0;
        for (&a, &b) in self
            .components_micros
            .iter()
            .zip(other.components_micros.iter())
        {
            let ai = i128::from(a);
            let bi = i128::from(b);
            dot += ai * bi;
            let bound = ai.abs().max(bi.abs());
            max_abs_pair += bound * bound;
        }
        if max_abs_pair == 0 {
            return ExactRatio::new(1, 2).ok();
        }
        // Map `dot / max_abs_pair` from `[-1, 1]` into `[0, 1]` as `(dot + max_abs_pair) / (2 * max_abs_pair)`,
        // scaled down to fit safely in `(i64, u64)`.
        let shifted_num = (dot + max_abs_pair).max(0);
        let denom = 2 * max_abs_pair;
        let scale_shift = ((denom as u128).leading_zeros()).saturating_sub(65);
        let divisor = if denom > (i64::MAX as i128) {
            (denom / (i64::MAX as i128)) + 1
        } else {
            let _ = scale_shift;
            1
        };
        let num_i64 = i64::try_from(shifted_num / divisor).ok()?;
        let den_u64 = u64::try_from((denom / divisor).max(1)).ok()?;
        ExactRatio::new(num_i64.min(den_u64 as i64), den_u64).ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundedKgeCandidateEntry {
    pub candidate: RecallCandidateSeed,
    pub selector: SelectorCid,
    pub embedding: GroundedConceptEmbedding,
}

#[derive(Clone, Debug)]
pub struct GroundedKgeRecallConfig {
    pub registry_release_root: [u8; 32],
    pub embedding_space_spec_cid: ObjectCid,
    pub capability_definition: ObjectCid,
    pub implementation_manifest: ObjectReference,
    pub model_version: ObjectReference,
    pub dimension: usize,
}

/// Deterministic grounded KGE recall adapter (`CandidateRecallAdapter`) behind `ModelRecallFirewall`.
#[derive(Clone, Debug)]
pub struct GroundedKgeRecallAdapter {
    config: GroundedKgeRecallConfig,
    entries: BTreeMap<[u8; 32], GroundedKgeCandidateEntry>,
    query_anchors: BTreeMap<[u8; 32], GroundedConceptEmbedding>,
}

impl GroundedKgeRecallAdapter {
    pub fn new(config: GroundedKgeRecallConfig) -> Result<Self, KgeRecallError> {
        if config.registry_release_root == [0; 32]
            || config.embedding_space_spec_cid.as_bytes() == &[0; 32]
            || config.capability_definition.as_bytes() == &[0; 32]
            || config.implementation_manifest.cid == [0; 32]
            || config.model_version.cid == [0; 32]
        {
            return Err(KgeRecallError::InvalidConfiguration);
        }
        if config.dimension == 0 || config.dimension > MAX_KGE_EMBEDDING_DIM {
            return Err(KgeRecallError::InvalidDimension(config.dimension));
        }
        Ok(Self {
            config,
            entries: BTreeMap::new(),
            query_anchors: BTreeMap::new(),
        })
    }

    pub const fn registry_release_root(&self) -> &[u8; 32] {
        &self.config.registry_release_root
    }

    pub const fn embedding_space_spec_cid(&self) -> ObjectCid {
        self.config.embedding_space_spec_cid
    }

    pub const fn has_symbolic_authority(&self) -> bool {
        false
    }

    pub const fn has_unit_authority(&self) -> bool {
        false
    }

    pub fn index_grounded_candidate(
        &mut self,
        candidate: RecallCandidateSeed,
        selector: SelectorCid,
        ccid: ConceptCcid,
        structural_sig_digest: [u8; 32],
    ) -> Result<GroundedConceptEmbedding, KgeRecallError> {
        if selector.as_bytes() == &[0; 32] {
            return Err(KgeRecallError::InvalidConfiguration);
        }
        let embedding = GroundedConceptEmbedding::from_source(
            KgeEmbeddingInitSource::Grounded {
                registry_release_root: self.config.registry_release_root,
                ccid,
                structural_sig_digest,
            },
            self.config.dimension,
        )?;
        self.entries.insert(
            *candidate.candidate_id(),
            GroundedKgeCandidateEntry {
                candidate,
                selector,
                embedding: embedding.clone(),
            },
        );
        Ok(embedding)
    }

    pub fn bind_query_anchor(
        &mut self,
        query_commitment: [u8; 32],
        target_ccid: ConceptCcid,
        target_structural_sig_digest: [u8; 32],
    ) -> Result<GroundedConceptEmbedding, KgeRecallError> {
        if query_commitment == [0; 32] {
            return Err(KgeRecallError::InvalidConfiguration);
        }
        let embedding = GroundedConceptEmbedding::from_source(
            KgeEmbeddingInitSource::Grounded {
                registry_release_root: self.config.registry_release_root,
                ccid: target_ccid,
                structural_sig_digest: target_structural_sig_digest,
            },
            self.config.dimension,
        )?;
        self.query_anchors
            .insert(query_commitment, embedding.clone());
        Ok(embedding)
    }

    fn fallback_query_embedding(&self, query_commitment: [u8; 32]) -> GroundedConceptEmbedding {
        let mut synthetic_ccid = [0u8; 16];
        synthetic_ccid.copy_from_slice(&query_commitment[..16]);
        if synthetic_ccid == [0; 16] {
            synthetic_ccid[0] = 1;
        }
        GroundedConceptEmbedding::from_source(
            KgeEmbeddingInitSource::Grounded {
                registry_release_root: self.config.registry_release_root,
                ccid: ConceptCcid::from_bytes(synthetic_ccid),
                structural_sig_digest: query_commitment,
            },
            self.config.dimension,
        )
        .expect("non-zero query commitment produces valid grounded anchor")
    }
}

impl CandidateRecallAdapter for GroundedKgeRecallAdapter {
    fn recall(&mut self, request: ModelRecallRequest) -> Result<ModelRecallPage, ModelRecallError> {
        if request.query_commitment == [0; 32]
            || request.selector.as_bytes() == &[0; 32]
            || request.max_candidates == 0
            || request.max_work_units == 0
        {
            return Err(ModelRecallError::InvalidRequest);
        }
        let anchor = self
            .query_anchors
            .get(&request.query_commitment)
            .cloned()
            .unwrap_or_else(|| self.fallback_query_embedding(request.query_commitment));

        let mut scored = Vec::new();
        let mut work_units = 0u64;
        for (id, entry) in &self.entries {
            if entry.selector != request.selector {
                continue;
            }
            if let Some(cursor) = request.continuation {
                if id <= &cursor {
                    continue;
                }
            }
            if work_units >= request.max_work_units {
                break;
            }
            work_units += 1;
            let score = anchor
                .normalized_similarity(&entry.embedding)
                .ok_or(ModelRecallError::Adapter("kge_similarity_overflow"))?;
            scored.push((
                *id,
                ModelScoredCandidate {
                    candidate: entry.candidate.clone(),
                    recall_score: score,
                },
            ));
        }

        scored.sort_by(|(id_a, cand_a), (id_b, cand_b)| {
            let lhs = i128::from(cand_b.recall_score.numerator())
                * i128::from(cand_a.recall_score.denominator());
            let rhs = i128::from(cand_a.recall_score.numerator())
                * i128::from(cand_b.recall_score.denominator());
            lhs.cmp(&rhs).then_with(|| id_a.cmp(id_b))
        });

        let truncated = scored.len() > request.max_candidates;
        scored.truncate(request.max_candidates);
        let continuation = if truncated {
            scored.last().map(|(id, _)| *id)
        } else {
            None
        };
        let candidates: Vec<ModelScoredCandidate> = scored.into_iter().map(|(_, c)| c).collect();

        let mut inv_hasher = blake3::Hasher::new();
        inv_hasher.update(b"onebrain:vnext:kge-recall-invocation:1\0");
        inv_hasher.update(&self.config.registry_release_root);
        inv_hasher.update(self.config.embedding_space_spec_cid.as_bytes());
        inv_hasher.update(&request.query_commitment);
        inv_hasher.update(request.selector.as_bytes());
        let invocation_commitment = *inv_hasher.finalize().as_bytes();

        Ok(ModelRecallPage {
            evidence: ModelRecallEvidence {
                capability_definition: self.config.capability_definition,
                implementation_manifest: self.config.implementation_manifest.clone(),
                model_version: self.config.model_version.clone(),
                invocation_commitment,
                query_commitment: request.query_commitment,
            },
            candidates,
            consumed_work_units: work_units.max(1).min(request.max_work_units),
            continuation,
        })
    }
}

// ---------------------------------------------------------------------------
// FedR Operator Matrix & Quarantine Validator (`obp/obkg-fedr/1`, `OBS-002`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FedRError {
    #[error("FedR operator ingestion is disabled by default (`fedr_enabled == false`)")]
    DisabledByDefault,
    #[error("unsupported FedR protocol identifier: {0}")]
    UnsupportedProtocol(String),
    #[error("FedR lineage mismatch: registry_release_root or embedding_space_spec_cid differs")]
    LineageMismatch,
    #[error("FedR operator matrix dimensions or payload schema invalid")]
    InvalidMatrix,
    #[error("FedR operator L2 Frobenius norm ceiling exceeded")]
    NormCeilingExceeded,
    #[error("FedR operator CID mismatch")]
    CidMismatch,
    #[error("canonical encoding error: {0:?}")]
    Canonical(CanonicalError),
}

impl From<CanonicalError> for FedRError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

/// Federated Relational Operator Matrix (`obp/obkg-fedr/1`) sharing relation matrices only
/// (never entity embeddings or private vault graphs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FedROperatorMatrixV1 {
    pub protocol_id: String,
    pub sender_actor: ActorId,
    pub relation_predicate: ConceptCcid,
    pub registry_release_root: [u8; 32],
    pub embedding_space_spec_cid: ObjectCid,
    pub dimension: u16,
    /// Flattened `dimension * dimension` fixed-point entries scaled by `KGE_FIXED_POINT_SCALE`.
    pub matrix_micros: Vec<i64>,
    pub claimed_cid: ObjectCid,
}

impl FedROperatorMatrixV1 {
    pub fn build(
        sender_actor: ActorId,
        relation_predicate: ConceptCcid,
        registry_release_root: [u8; 32],
        embedding_space_spec_cid: ObjectCid,
        dimension: u16,
        matrix_micros: Vec<i64>,
    ) -> Result<Self, FedRError> {
        let dim = usize::from(dimension);
        if sender_actor.as_bytes() == &[0; 32]
            || relation_predicate.as_bytes() == &[0; 16]
            || registry_release_root == [0; 32]
            || embedding_space_spec_cid.as_bytes() == &[0; 32]
            || dim == 0
            || dim > MAX_KGE_EMBEDDING_DIM
            || matrix_micros.len() != dim * dim
        {
            return Err(FedRError::InvalidMatrix);
        }
        let mut candidate = Self {
            protocol_id: FEDR_PROTOCOL_ID.to_string(),
            sender_actor,
            relation_predicate,
            registry_release_root,
            embedding_space_spec_cid,
            dimension,
            matrix_micros,
            claimed_cid: ObjectCid::from_bytes([0; 32]),
        };
        let body_bytes = candidate.canonical_body_bytes()?;
        let cid = ObjectCid::compute(ReservedDomain::Object, &body_bytes)
            .map_err(|_| FedRError::InvalidMatrix)?;
        candidate.claimed_cid = cid;
        Ok(candidate)
    }

    pub fn canonical_body_bytes(&self) -> Result<Vec<u8>, FedRError> {
        let entries = self
            .matrix_micros
            .iter()
            .map(|&v| {
                if v >= 0 {
                    CanonicalValue::Unsigned(v as u64)
                } else {
                    let arg = (-1i128 - i128::from(v)) as u64;
                    CanonicalValue::Negative(arg)
                }
            })
            .collect();
        let value = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(FEDR_MAJOR)),
            (1, CanonicalValue::Text(self.protocol_id.clone())),
            (
                2,
                CanonicalValue::Bytes(self.sender_actor.as_bytes().to_vec()),
            ),
            (
                3,
                CanonicalValue::Bytes(self.relation_predicate.as_bytes().to_vec()),
            ),
            (
                4,
                CanonicalValue::Bytes(self.registry_release_root.to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(self.embedding_space_spec_cid.as_bytes().to_vec()),
            ),
            (6, CanonicalValue::Unsigned(u64::from(self.dimension))),
            (7, CanonicalValue::Array(entries)),
        ]);
        encode_canonical(&value, ResourceProfile::ObjectV1).map_err(Into::into)
    }

    pub fn from_canonical_body_bytes(
        bytes: &[u8],
        claimed_cid: ObjectCid,
    ) -> Result<Self, FedRError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let CanonicalValue::Map(fields) = value else {
            return Err(FedRError::InvalidMatrix);
        };
        if fields.len() != 8 || fields.iter().enumerate().any(|(i, (k, _))| *k != i as u64) {
            return Err(FedREErrorHelper::invalid());
        }
        let CanonicalValue::Unsigned(FEDR_MAJOR) = fields[0].1 else {
            return Err(FedRError::InvalidMatrix);
        };
        let CanonicalValue::Text(protocol_id) = &fields[1].1 else {
            return Err(FedRError::InvalidMatrix);
        };
        let sender_actor = ActorId::from_bytes(fixed_bytes_fedr::<32>(&fields[2].1)?);
        let relation_predicate = ConceptCcid::from_bytes(fixed_bytes_fedr::<16>(&fields[3].1)?);
        let registry_release_root = fixed_bytes_fedr::<32>(&fields[4].1)?;
        let embedding_space_spec_cid = ObjectCid::from_bytes(fixed_bytes_fedr::<32>(&fields[5].1)?);
        let CanonicalValue::Unsigned(dim_u64) = fields[6].1 else {
            return Err(FedRError::InvalidMatrix);
        };
        let dimension = u16::try_from(dim_u64).map_err(|_| FedRError::InvalidMatrix)?;
        let CanonicalValue::Array(arr) = &fields[7].1 else {
            return Err(FedRError::InvalidMatrix);
        };
        let mut matrix_micros = Vec::with_capacity(arr.len());
        for entry in arr {
            let v = match entry {
                CanonicalValue::Unsigned(u) => {
                    i64::try_from(*u).map_err(|_| FedRError::InvalidMatrix)?
                }
                CanonicalValue::Negative(n) => {
                    i64::try_from(-1i128 - i128::from(*n)).map_err(|_| FedRError::InvalidMatrix)?
                }
                _ => return Err(FedRError::InvalidMatrix),
            };
            matrix_micros.push(v);
        }
        let dim = usize::from(dimension);
        if dim == 0 || dim > MAX_KGE_EMBEDDING_DIM || matrix_micros.len() != dim * dim {
            return Err(FedRError::InvalidMatrix);
        }
        let computed = ObjectCid::compute(ReservedDomain::Object, bytes)
            .map_err(|_| FedRError::InvalidMatrix)?;
        if computed != claimed_cid {
            return Err(FedRError::CidMismatch);
        }
        Ok(Self {
            protocol_id: protocol_id.clone(),
            sender_actor,
            relation_predicate,
            registry_release_root,
            embedding_space_spec_cid,
            dimension,
            matrix_micros,
            claimed_cid,
        })
    }

    /// Exact squared Frobenius L2 norm in `micros^2`: `sum_{i,j} M_{i,j}^2`.
    pub fn squared_frobenius_norm_micros2(&self) -> Result<u128, FedRError> {
        let mut acc = 0u128;
        for &v in &self.matrix_micros {
            let abs_v = v.unsigned_abs() as u128;
            let sq = abs_v
                .checked_mul(abs_v)
                .ok_or(FedRError::NormCeilingExceeded)?;
            acc = acc.checked_add(sq).ok_or(FedRError::NormCeilingExceeded)?;
        }
        Ok(acc)
    }

    pub const fn contains_entity_embeddings(&self) -> bool {
        false
    }

    pub const fn emits_obkg_rewards(&self) -> bool {
        false
    }
}

struct FedREErrorHelper;
impl FedREErrorHelper {
    const fn invalid() -> FedRError {
        FedRError::InvalidMatrix
    }
}

fn fixed_bytes_fedr<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], FedRError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(FedRError::InvalidMatrix);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| FedRError::InvalidMatrix)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FedRQuarantinePolicy {
    /// Default-off (`false`) per P7 §8.2.
    pub fedr_enabled: bool,
    pub expected_registry_release_root: [u8; 32],
    pub expected_embedding_space_spec_cid: ObjectCid,
    pub max_l2_norm_micros: u64,
}

impl FedRQuarantinePolicy {
    pub const fn default_off(
        expected_registry_release_root: [u8; 32],
        expected_embedding_space_spec_cid: ObjectCid,
        max_l2_norm_micros: u64,
    ) -> Self {
        Self {
            fedr_enabled: false,
            expected_registry_release_root,
            expected_embedding_space_spec_cid,
            max_l2_norm_micros,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FedRQuarantineOutcome {
    pub quarantine_record: VaultQuarantineRecord,
    pub squared_l2_norm_micros2: u128,
    pub obkg_rewards_emitted: u64,
}

impl FedRQuarantineOutcome {
    pub const fn mutated_active_model(&self) -> bool {
        false
    }

    pub const fn emits_obkg_rewards(&self) -> bool {
        false
    }
}

/// Validates incoming `obp/obkg-fedr/1` operator matrices and stages them into
/// `VaultQuarantineRecord` (`OBS-002`) without mutating active KGE weights.
pub struct FedRQuarantineValidator {
    policy: FedRQuarantinePolicy,
}

impl FedRQuarantineValidator {
    pub const fn new(policy: FedRQuarantinePolicy) -> Self {
        Self { policy }
    }

    pub const fn policy(&self) -> &FedRQuarantinePolicy {
        &self.policy
    }

    pub fn validate_and_quarantine(
        &self,
        operator: &FedROperatorMatrixV1,
    ) -> Result<FedRQuarantineOutcome, FedRError> {
        if !self.policy.fedr_enabled {
            return Err(FedRError::DisabledByDefault);
        }
        if operator.protocol_id != FEDR_PROTOCOL_ID {
            return Err(FedRError::UnsupportedProtocol(operator.protocol_id.clone()));
        }
        if operator.registry_release_root != self.policy.expected_registry_release_root
            || operator.embedding_space_spec_cid != self.policy.expected_embedding_space_spec_cid
        {
            return Err(FedRError::LineageMismatch);
        }
        let dim = usize::from(operator.dimension);
        if dim == 0
            || dim > MAX_KGE_EMBEDDING_DIM
            || operator.matrix_micros.len() != dim * dim
            || self.policy.max_l2_norm_micros == 0
        {
            return Err(FedRError::InvalidMatrix);
        }

        let body_bytes = operator.canonical_body_bytes()?;
        let computed_cid = ObjectCid::compute(ReservedDomain::Object, &body_bytes)
            .map_err(|_| FedRError::InvalidMatrix)?;
        if computed_cid != operator.claimed_cid {
            return Err(FedRError::CidMismatch);
        }

        let sq_norm = operator.squared_frobenius_norm_micros2()?;
        let ceiling_sq = u128::from(self.policy.max_l2_norm_micros)
            .checked_mul(u128::from(self.policy.max_l2_norm_micros))
            .ok_or(FedRError::NormCeilingExceeded)?;
        if sq_norm > ceiling_sq {
            return Err(FedRError::NormCeilingExceeded);
        }

        let claimed_bytes = *operator.claimed_cid.as_bytes();
        let mut q_hasher = blake3::Hasher::new();
        q_hasher.update(b"onebrain:vnext:vault-portable-quarantine:1\0");
        q_hasher.update(&[StoredRecordKind::Object as u8]);
        q_hasher.update(&claimed_bytes);
        q_hasher.update(&(FEDR_QUARANTINE_REASON_CODE.len() as u64).to_be_bytes());
        q_hasher.update(FEDR_QUARANTINE_REASON_CODE.as_bytes());
        q_hasher.update(&body_bytes);
        let quarantine_id = *q_hasher.finalize().as_bytes();

        let quarantine_record = VaultQuarantineRecord {
            quarantine_id,
            record_kind: StoredRecordKind::Object,
            claimed_cid: claimed_bytes,
            reason_code: FEDR_QUARANTINE_REASON_CODE.to_string(),
            plaintext: body_bytes,
        };
        debug_assert!(!quarantine_record.is_executable());

        Ok(FedRQuarantineOutcome {
            quarantine_record,
            squared_l2_norm_micros2: sq_norm,
            obkg_rewards_emitted: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// DreamConsolidatorV1 inside TypedCognitiveExecutor (`NEG-MAP-004`, `FW-03`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum DreamCandidateKind {
    TransitiveClosure = 0,
    OpposingClaimCluster = 1,
    StandingNeedBridge = 2,
}

impl DreamCandidateKind {
    const fn from_u64(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::TransitiveClosure),
            1 => Some(Self::OpposingClaimCluster),
            2 => Some(Self::StandingNeedBridge),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DreamRelationEdge {
    pub source_object: ObjectReference,
    pub subject: ConceptCcid,
    pub predicate: ConceptCcid,
    pub object: ConceptCcid,
    pub polarity: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DreamNeedBridgeTarget {
    pub need_id: [u8; 32],
    pub desired_role: ConceptCcid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DreamAffordanceHop {
    pub affordance_object: ObjectReference,
    pub input_role: ConceptCcid,
    pub output_role: ConceptCcid,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DreamGraphSnapshot {
    pub validation_context: [u8; 32],
    pub edges: Vec<DreamRelationEdge>,
    pub unsatisfied_needs: Vec<DreamNeedBridgeTarget>,
    pub affordances: Vec<DreamAffordanceHop>,
}

impl DreamGraphSnapshot {
    pub fn to_canonical_input_bytes(&self) -> Result<Vec<u8>, KgeRecallError> {
        let edges_val = CanonicalValue::Array(
            self.edges
                .iter()
                .map(|e| {
                    CanonicalValue::Map(vec![
                        (0, kge_obj_ref_to_value(&e.source_object)),
                        (1, CanonicalValue::Bytes(e.subject.as_bytes().to_vec())),
                        (2, CanonicalValue::Bytes(e.predicate.as_bytes().to_vec())),
                        (3, CanonicalValue::Bytes(e.object.as_bytes().to_vec())),
                        (4, CanonicalValue::Bool(e.polarity)),
                    ])
                })
                .collect(),
        );
        let needs_val = CanonicalValue::Array(
            self.unsatisfied_needs
                .iter()
                .map(|n| {
                    CanonicalValue::Map(vec![
                        (0, CanonicalValue::Bytes(n.need_id.to_vec())),
                        (1, CanonicalValue::Bytes(n.desired_role.as_bytes().to_vec())),
                    ])
                })
                .collect(),
        );
        let aff_val = CanonicalValue::Array(
            self.affordances
                .iter()
                .map(|a| {
                    CanonicalValue::Map(vec![
                        (0, kge_obj_ref_to_value(&a.affordance_object)),
                        (1, CanonicalValue::Bytes(a.input_role.as_bytes().to_vec())),
                        (2, CanonicalValue::Bytes(a.output_role.as_bytes().to_vec())),
                    ])
                })
                .collect(),
        );
        let root = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(DREAM_MAJOR)),
            (1, CanonicalValue::Bytes(self.validation_context.to_vec())),
            (2, edges_val),
            (3, needs_val),
            (4, aff_val),
        ]);
        encode_canonical(&root, ResourceProfile::ObjectV1).map_err(Into::into)
    }
}

/// Non-executable proposal emitted by `DreamConsolidatorV1` after passing
/// `SymbolicMappingValidator` (`NEG-MAP-004`, `FW-03`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DreamConsolidationProposal {
    pub proposal_id: [u8; 32],
    pub kind: DreamCandidateKind,
    pub candidate_objects: Vec<ObjectReference>,
    pub inferred_subject: ConceptCcid,
    pub inferred_predicate_or_role: ConceptCcid,
    pub inferred_object: ConceptCcid,
    pub mapping_kernel: MappingKernelCid,
    pub symbolic_disposition: SymbolicDisposition,
}

impl DreamConsolidationProposal {
    pub const fn is_executable(&self) -> bool {
        false
    }

    pub const fn mutates_active_graph(&self) -> bool {
        false
    }

    pub const fn emits_obkg_rewards(&self) -> bool {
        false
    }

    fn to_canonical_value(&self) -> CanonicalValue {
        let disp_code = match self.symbolic_disposition {
            SymbolicDisposition::EligibleProposalCandidate => 0u64,
            SymbolicDisposition::DeferredRequiredUnknown => 1u64,
            SymbolicDisposition::RejectedRequiredViolation => 2u64,
        };
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.proposal_id.to_vec())),
            (1, CanonicalValue::Unsigned(self.kind as u64)),
            (
                2,
                CanonicalValue::Array(
                    self.candidate_objects
                        .iter()
                        .map(kge_obj_ref_to_value)
                        .collect(),
                ),
            ),
            (
                3,
                CanonicalValue::Bytes(self.inferred_subject.as_bytes().to_vec()),
            ),
            (
                4,
                CanonicalValue::Bytes(self.inferred_predicate_or_role.as_bytes().to_vec()),
            ),
            (
                5,
                CanonicalValue::Bytes(self.inferred_object.as_bytes().to_vec()),
            ),
            (
                6,
                CanonicalValue::Bytes(self.mapping_kernel.as_bytes().to_vec()),
            ),
            (7, CanonicalValue::Unsigned(disp_code)),
        ])
    }

    fn from_canonical_value(value: &CanonicalValue) -> Result<Self, KgeRecallError> {
        let CanonicalValue::Map(fields) = value else {
            return Err(KgeRecallError::InvalidConfiguration);
        };
        if fields.len() != 8 || fields.iter().enumerate().any(|(i, (k, _))| *k != i as u64) {
            return Err(KgeRecallError::InvalidConfiguration);
        }
        let proposal_id = fixed_bytes_dream::<32>(&fields[0].1)?;
        let CanonicalValue::Unsigned(kind_code) = fields[1].1 else {
            return Err(KgeRecallError::InvalidConfiguration);
        };
        let kind =
            DreamCandidateKind::from_u64(kind_code).ok_or(KgeRecallError::InvalidConfiguration)?;
        let CanonicalValue::Array(objs) = &fields[2].1 else {
            return Err(KgeRecallError::InvalidConfiguration);
        };
        let mut candidate_objects = Vec::with_capacity(objs.len());
        for obj in objs {
            candidate_objects.push(kge_obj_ref_from_value(obj)?);
        }
        let inferred_subject = ConceptCcid::from_bytes(fixed_bytes_dream::<16>(&fields[3].1)?);
        let inferred_predicate_or_role =
            ConceptCcid::from_bytes(fixed_bytes_dream::<16>(&fields[4].1)?);
        let inferred_object = ConceptCcid::from_bytes(fixed_bytes_dream::<16>(&fields[5].1)?);
        let mapping_kernel = MappingKernelCid::from_bytes(fixed_bytes_dream::<32>(&fields[6].1)?);
        let CanonicalValue::Unsigned(disp_code) = fields[7].1 else {
            return Err(KgeRecallError::InvalidConfiguration);
        };
        let symbolic_disposition = match disp_code {
            0 => SymbolicDisposition::EligibleProposalCandidate,
            1 => SymbolicDisposition::DeferredRequiredUnknown,
            _ => return Err(KgeRecallError::InvalidConfiguration),
        };
        Ok(Self {
            proposal_id,
            kind,
            candidate_objects,
            inferred_subject,
            inferred_predicate_or_role,
            inferred_object,
            mapping_kernel,
            symbolic_disposition,
        })
    }
}

fn fixed_bytes_dream<const N: usize>(value: &CanonicalValue) -> Result<[u8; N], KgeRecallError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(KgeRecallError::InvalidConfiguration);
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| KgeRecallError::InvalidConfiguration)
}

fn kge_obj_ref_to_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn kge_obj_ref_from_value(value: &CanonicalValue) -> Result<ObjectReference, KgeRecallError> {
    let CanonicalValue::Map(fields) = value else {
        return Err(KgeRecallError::InvalidConfiguration);
    };
    if fields.len() != 2 || fields[0].0 != 0 || fields[1].0 != 1 {
        return Err(KgeRecallError::InvalidConfiguration);
    }
    let CanonicalValue::Unsigned(kind) = fields[0].1 else {
        return Err(KgeRecallError::InvalidConfiguration);
    };
    let cid = fixed_bytes_dream::<32>(&fields[1].1)?;
    Ok(ObjectReference::new(kind, cid))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DreamConsolidationRun {
    pub execution: CognitiveExecutionResult,
    pub proposals: Vec<DreamConsolidationProposal>,
    pub rejected_by_symbolic_firewall: usize,
}

impl DreamConsolidationRun {
    pub const fn is_executable(&self) -> bool {
        false
    }

    pub const fn emits_obkg_rewards(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
struct RawDreamCandidate {
    kind: DreamCandidateKind,
    seed: RecallCandidateSeed,
    subject: ConceptCcid,
    predicate_or_role: ConceptCcid,
    object: ConceptCcid,
}

struct DreamExecutorBackend<'a, V: SymbolicMappingValidator> {
    candidates: Vec<RawDreamCandidate>,
    validation_context: [u8; 32],
    validator: &'a mut V,
    cursor: usize,
    rejected_count: usize,
}

impl<V: SymbolicMappingValidator> TypedCapabilityBackend for DreamExecutorBackend<'_, V> {
    fn execute_step(&mut self, request: CognitiveStepRequest<'_>) -> Result<CognitiveStep, String> {
        if self.cursor >= self.candidates.len() {
            let empty_batch = encode_canonical(
                &CanonicalValue::Array(Vec::new()),
                ResourceProfile::ObjectV1,
            )
            .map_err(|e| format!("dream encode error: {e:?}"))?;
            let mut framed = Vec::with_capacity(4 + empty_batch.len());
            framed.extend_from_slice(&(empty_batch.len() as u32).to_be_bytes());
            framed.extend_from_slice(&empty_batch);
            return Ok(CognitiveStep {
                output_fragment: framed,
                continuation: None,
                work_units: 1,
                elapsed_ticks: 1,
                limitations: Vec::new(),
            });
        }

        let max_per_step = (request.budget.remaining_work_units as usize).clamp(1, 8);
        let end = (self.cursor + max_per_step).min(self.candidates.len());
        let mut step_values = Vec::new();
        let mut work_units = 0u64;

        for raw in &self.candidates[self.cursor..end] {
            work_units += 1;
            let sym_req = SymbolicValidationRequest {
                candidate_id: *raw.seed.candidate_id(),
                candidate_objects: raw.seed.candidate_objects(),
                validation_context: self.validation_context,
            };
            let assessment: SymbolicMappingAssessment = self
                .validator
                .validate_mapping(sym_req)
                .map_err(|e| format!("symbolic validation failed: {e:?}"))?;
            let disposition = assessment.disposition();
            if disposition == SymbolicDisposition::RejectedRequiredViolation {
                self.rejected_count += 1;
                continue;
            }
            let proposal = DreamConsolidationProposal {
                proposal_id: compute_dream_proposal_id(
                    raw.kind,
                    raw.seed.candidate_id(),
                    raw.subject,
                    raw.predicate_or_role,
                    raw.object,
                    assessment.mapping_kernel,
                ),
                kind: raw.kind,
                candidate_objects: raw.seed.candidate_objects().to_vec(),
                inferred_subject: raw.subject,
                inferred_predicate_or_role: raw.predicate_or_role,
                inferred_object: raw.object,
                mapping_kernel: assessment.mapping_kernel,
                symbolic_disposition: disposition,
            };
            step_values.push(proposal.to_canonical_value());
        }

        self.cursor = end;
        let continuation = if self.cursor < self.candidates.len() {
            Some((self.cursor as u64).to_be_bytes().to_vec())
        } else {
            None
        };
        let encoded = encode_canonical(
            &CanonicalValue::Array(step_values),
            ResourceProfile::ObjectV1,
        )
        .map_err(|e| format!("dream step encode error: {e:?}"))?;
        let mut framed = Vec::with_capacity(4 + encoded.len());
        framed.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        framed.extend_from_slice(&encoded);

        Ok(CognitiveStep {
            output_fragment: framed,
            continuation,
            work_units: work_units.max(1),
            elapsed_ticks: 1,
            limitations: Vec::new(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DreamConsolidationError {
    #[error("KGE/input error: {0}")]
    Input(#[from] KgeRecallError),
    #[error("cognitive executor error: {0:?}")]
    Executor(CognitiveExecutionError),
    #[error("invalid validation context")]
    InvalidValidationContext,
}

impl From<CognitiveExecutionError> for DreamConsolidationError {
    fn from(error: CognitiveExecutionError) -> Self {
        Self::Executor(error)
    }
}

/// Offline/idle consolidation engine executing inside `TypedCognitiveExecutor` (`vnext_executor.rs`)
/// and gating every discovered candidate through `SymbolicMappingValidator` (`FW-03`).
pub struct DreamConsolidatorV1;

impl DreamConsolidatorV1 {
    pub fn run_in_executor<V: SymbolicMappingValidator>(
        executor: &TypedCognitiveExecutor,
        permits: &PermitValidator,
        mut task: CognitiveTask,
        snapshot: &DreamGraphSnapshot,
        validator: &mut V,
        started_at: u64,
        cancellation: &CancellationToken,
    ) -> Result<DreamConsolidationRun, DreamConsolidationError> {
        if snapshot.validation_context == [0; 32] {
            return Err(DreamConsolidationError::InvalidValidationContext);
        }
        let snapshot_bytes = snapshot.to_canonical_input_bytes()?;
        let expected_commitment = cognitive_input_commitment(&snapshot_bytes);
        if task.input_payload.is_empty() {
            task.input_payload = snapshot_bytes;
            if task.input_commitments.is_empty() {
                task.input_commitments.push(expected_commitment);
            }
        }

        let candidates = discover_raw_dream_candidates(snapshot);
        let mut backend = DreamExecutorBackend {
            candidates,
            validation_context: snapshot.validation_context,
            validator,
            cursor: 0,
            rejected_count: 0,
        };

        let execution = executor.execute(&mut backend, permits, task, started_at, cancellation)?;
        let proposals = if execution.termination == CognitiveTermination::Completed
            || !execution.output.is_empty()
        {
            decode_framed_dream_proposals(&execution.output)?
        } else {
            Vec::new()
        };

        Ok(DreamConsolidationRun {
            execution,
            proposals,
            rejected_by_symbolic_firewall: backend.rejected_count,
        })
    }
}

fn discover_raw_dream_candidates(snapshot: &DreamGraphSnapshot) -> Vec<RawDreamCandidate> {
    let mut raw = Vec::new();
    let mut seen_keys = BTreeSet::new();

    let existing_positive: BTreeSet<(ConceptCcid, ConceptCcid, ConceptCcid)> = snapshot
        .edges
        .iter()
        .filter(|e| e.polarity)
        .map(|e| (e.subject, e.predicate, e.object))
        .collect();

    // 1. Transitive closures: (A -p-> B) + (B -p-> C) => candidate (A -p-> C).
    for left in &snapshot.edges {
        if !left.polarity {
            continue;
        }
        for right in &snapshot.edges {
            if !right.polarity
                || left.predicate != right.predicate
                || left.object != right.subject
                || left.subject == right.object
            {
                continue;
            }
            if existing_positive.contains(&(left.subject, left.predicate, right.object)) {
                continue;
            }
            if let Ok(seed) = RecallCandidateSeed::new(vec![
                left.source_object.clone(),
                right.source_object.clone(),
            ]) {
                let key = (
                    0u8,
                    *seed.candidate_id(),
                    *left.subject.as_bytes(),
                    *left.predicate.as_bytes(),
                    *right.object.as_bytes(),
                );
                if seen_keys.insert(key) {
                    raw.push(RawDreamCandidate {
                        kind: DreamCandidateKind::TransitiveClosure,
                        seed,
                        subject: left.subject,
                        predicate_or_role: left.predicate,
                        object: right.object,
                    });
                }
            }
        }
    }

    // 2. Opposing-claim clusters: matching (subject, predicate) with opposing polarity or object.
    for (idx, left) in snapshot.edges.iter().enumerate() {
        for right in snapshot.edges.iter().skip(idx + 1) {
            if left.subject != right.subject || left.predicate != right.predicate {
                continue;
            }
            let opposing = left.polarity != right.polarity || left.object != right.object;
            if !opposing {
                continue;
            }
            if let Ok(seed) = RecallCandidateSeed::new(vec![
                left.source_object.clone(),
                right.source_object.clone(),
            ]) {
                let key = (
                    1u8,
                    *seed.candidate_id(),
                    *left.subject.as_bytes(),
                    *left.predicate.as_bytes(),
                    *right.object.as_bytes(),
                );
                if seen_keys.insert(key) {
                    raw.push(RawDreamCandidate {
                        kind: DreamCandidateKind::OpposingClaimCluster,
                        seed,
                        subject: left.subject,
                        predicate_or_role: left.predicate,
                        object: right.object,
                    });
                }
            }
        }
    }

    // 3. Multi-hop unsatisfied StandingNeed bridges: (A: in -> mid) + (B: mid -> desired_role).
    for need in &snapshot.unsatisfied_needs {
        for hop1 in &snapshot.affordances {
            for hop2 in &snapshot.affordances {
                if hop1.output_role != hop2.input_role
                    || hop2.output_role != need.desired_role
                    || hop1.input_role == need.desired_role
                {
                    continue;
                }
                if let Ok(seed) = RecallCandidateSeed::new(vec![
                    hop1.affordance_object.clone(),
                    hop2.affordance_object.clone(),
                ]) {
                    let key = (
                        2u8,
                        *seed.candidate_id(),
                        *hop1.input_role.as_bytes(),
                        *hop1.output_role.as_bytes(),
                        *need.desired_role.as_bytes(),
                    );
                    if seen_keys.insert(key) {
                        raw.push(RawDreamCandidate {
                            kind: DreamCandidateKind::StandingNeedBridge,
                            seed,
                            subject: hop1.input_role,
                            predicate_or_role: hop1.output_role,
                            object: need.desired_role,
                        });
                    }
                }
            }
        }
    }

    raw
}

fn compute_dream_proposal_id(
    kind: DreamCandidateKind,
    candidate_id: &[u8; 32],
    subject: ConceptCcid,
    predicate_or_role: ConceptCcid,
    object: ConceptCcid,
    mapping_kernel: MappingKernelCid,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:dream-consolidation-proposal:1\0");
    hasher.update(&(kind as u64).to_be_bytes());
    hasher.update(candidate_id);
    hasher.update(subject.as_bytes());
    hasher.update(predicate_or_role.as_bytes());
    hasher.update(object.as_bytes());
    hasher.update(mapping_kernel.as_bytes());
    *hasher.finalize().as_bytes()
}

fn decode_framed_dream_proposals(
    bytes: &[u8],
) -> Result<Vec<DreamConsolidationProposal>, KgeRecallError> {
    let mut offset = 0usize;
    let mut proposals = Vec::new();
    while offset < bytes.len() {
        if offset + 4 > bytes.len() {
            return Err(KgeRecallError::InvalidConfiguration);
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&bytes[offset..offset + 4]);
        let frame_len = u32::from_be_bytes(len_bytes) as usize;
        offset += 4;
        if offset + frame_len > bytes.len() {
            return Err(KgeRecallError::InvalidConfiguration);
        }
        let value = decode_canonical(
            &bytes[offset..offset + frame_len],
            ResourceProfile::ObjectV1,
        )?;
        offset += frame_len;
        let CanonicalValue::Array(items) = value else {
            return Err(KgeRecallError::InvalidConfiguration);
        };
        for item in items {
            proposals.push(DreamConsolidationProposal::from_canonical_value(&item)?);
        }
    }
    Ok(proposals)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        authenticate_delegation_permit, decode_feed_inception, Budget, ConstraintEvaluation,
        DelegationGrant, DelegationPermitBody, DeviceId, EventCid, FeedInception,
        KeyStateApplyOutcome, KeyStateReducer, NamespaceCommitment, RetentionRule,
        ScopedDelegation, SignedDelegationPermit, SignedFeedInception,
    };

    use super::*;
    use crate::vnext_executor::CognitiveExecutionPolicy;
    use crate::vnext_model_recall::{ModelRecallFirewall, SymbolicCheck};

    fn ccid(b: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([b; 16])
    }

    fn obj_ref(b: u8) -> ObjectReference {
        ObjectReference::new(0, [b; 32])
    }

    #[test]
    fn grounded_kge_initialization_is_deterministic_and_rejects_legacy_cid_seeds() {
        let root = [0xAA; 32];
        let sig = [0xBB; 32];
        let emb_a = GroundedConceptEmbedding::from_source(
            KgeEmbeddingInitSource::Grounded {
                registry_release_root: root,
                ccid: ccid(7),
                structural_sig_digest: sig,
            },
            DEFAULT_KGE_EMBEDDING_DIM,
        )
        .unwrap();
        let emb_b = GroundedConceptEmbedding::from_source(
            KgeEmbeddingInitSource::Grounded {
                registry_release_root: root,
                ccid: ccid(7),
                structural_sig_digest: sig,
            },
            DEFAULT_KGE_EMBEDDING_DIM,
        )
        .unwrap();
        assert_eq!(emb_a, emb_b);
        assert_eq!(
            GroundedConceptEmbedding::from_source(
                KgeEmbeddingInitSource::LegacyCidBytes([0x11; 32]),
                DEFAULT_KGE_EMBEDDING_DIM,
            ),
            Err(KgeRecallError::RandomCidSeedForbidden)
        );

        let selector = SelectorCid::from_bytes([0x22; 32]);
        let mut adapter = GroundedKgeRecallAdapter::new(GroundedKgeRecallConfig {
            registry_release_root: root,
            embedding_space_spec_cid: ObjectCid::from_bytes([0x33; 32]),
            capability_definition: ObjectCid::from_bytes([0x44; 32]),
            implementation_manifest: obj_ref(0x55),
            model_version: obj_ref(0x66),
            dimension: DEFAULT_KGE_EMBEDDING_DIM,
        })
        .unwrap();
        assert!(!adapter.has_symbolic_authority());
        assert!(!adapter.has_unit_authority());

        let seed_match = RecallCandidateSeed::new(vec![obj_ref(1)]).unwrap();
        let seed_violated = RecallCandidateSeed::new(vec![obj_ref(8)]).unwrap();
        adapter
            .index_grounded_candidate(seed_match, selector, ccid(7), sig)
            .unwrap();
        adapter
            .index_grounded_candidate(seed_violated, selector, ccid(7), sig)
            .unwrap();
        adapter.bind_query_anchor([0x99; 32], ccid(7), sig).unwrap();

        struct TestSymbolicValidator;
        impl SymbolicMappingValidator for TestSymbolicValidator {
            fn validate_mapping(
                &mut self,
                request: SymbolicValidationRequest<'_>,
            ) -> Result<SymbolicMappingAssessment, ModelRecallError> {
                let first = request.candidate_objects[0].cid[0];
                let eval = if first == 8 {
                    ConstraintEvaluation::Violated
                } else {
                    ConstraintEvaluation::Satisfied
                };
                Ok(SymbolicMappingAssessment {
                    candidate_id: request.candidate_id,
                    mapping_kernel: MappingKernelCid::from_bytes([first; 32]),
                    validation_context: request.validation_context,
                    validator_version: ObjectReference::new(0, [0x77; 32]),
                    checks: vec![SymbolicCheck {
                        check_kind: ConceptCcid::from_bytes([0x88; 16]),
                        evaluation: eval,
                        required: true,
                    }],
                })
            }
        }

        let firewall_res = ModelRecallFirewall::discover(
            ModelRecallRequest {
                query_commitment: [0x99; 32],
                selector,
                max_candidates: 8,
                max_work_units: 16,
                continuation: None,
            },
            [0xAB; 32],
            Vec::new(),
            Some(&mut adapter),
            &mut TestSymbolicValidator,
        )
        .unwrap();
        assert!(firewall_res.model_used);
        assert!(!firewall_res.can_adopt_or_execute());
        assert_eq!(firewall_res.evaluations.len(), 2);
        assert!(firewall_res
            .evaluations
            .iter()
            .any(|e| e.disposition == SymbolicDisposition::RejectedRequiredViolation));
        assert!(firewall_res
            .evaluations
            .iter()
            .any(|e| e.disposition == SymbolicDisposition::EligibleProposalCandidate));
    }

    #[test]
    fn fedr_validator_enforces_default_off_lineage_norm_clipping_and_quarantine_only() {
        let root = [0xAA; 32];
        let space_cid = ObjectCid::from_bytes([0xBB; 32]);
        let sender = ActorId::from_bytes([0x12; 32]);

        let valid_op = FedROperatorMatrixV1::build(
            sender,
            ccid(5),
            root,
            space_cid,
            2,
            vec![300_000, 0, 0, 400_000], // Frobenius norm = 500_000 micros
        )
        .unwrap();
        assert!(!valid_op.contains_entity_embeddings());
        assert!(!valid_op.emits_obkg_rewards());

        // 1. Default-off gate rejects immediately.
        let default_off = FedRQuarantineValidator::new(FedRQuarantinePolicy::default_off(
            root, space_cid, 600_000,
        ));
        assert_eq!(
            default_off.validate_and_quarantine(&valid_op),
            Err(FedRError::DisabledByDefault)
        );

        // 2. Enabled validator rejects wrong lineage and norm ceiling breaches, and quarantines valid operators.
        let validator = FedRQuarantineValidator::new(FedRQuarantinePolicy {
            fedr_enabled: true,
            expected_registry_release_root: root,
            expected_embedding_space_spec_cid: space_cid,
            max_l2_norm_micros: 600_000,
        });

        let wrong_lineage = FedROperatorMatrixV1::build(
            sender,
            ccid(5),
            [0xCC; 32],
            space_cid,
            2,
            vec![100_000, 0, 0, 100_000],
        )
        .unwrap();
        assert_eq!(
            validator.validate_and_quarantine(&wrong_lineage),
            Err(FedRError::LineageMismatch)
        );

        let norm_breach = FedROperatorMatrixV1::build(
            sender,
            ccid(5),
            root,
            space_cid,
            2,
            vec![500_000, 500_000, 0, 0], // sqrt(500k^2 + 500k^2) > 600k
        )
        .unwrap();
        assert_eq!(
            validator.validate_and_quarantine(&norm_breach),
            Err(FedRError::NormCeilingExceeded)
        );

        let outcome = validator.validate_and_quarantine(&valid_op).unwrap();
        assert!(!outcome.quarantine_record.is_executable());
        assert!(!outcome.mutated_active_model());
        assert!(!outcome.emits_obkg_rewards());
        assert_eq!(outcome.obkg_rewards_emitted, 0);
        assert_eq!(
            outcome.quarantine_record.reason_code,
            FEDR_QUARANTINE_REASON_CODE
        );
    }

    #[test]
    fn dream_consolidator_v1_executes_in_cognitive_executor_and_emits_non_executable_proposals() {
        let snapshot = DreamGraphSnapshot {
            validation_context: [0x5A; 32],
            edges: vec![
                // Transitive chain: ccid(1) -ccid(10)-> ccid(2) -ccid(10)-> ccid(3)
                DreamRelationEdge {
                    source_object: obj_ref(1),
                    subject: ccid(1),
                    predicate: ccid(10),
                    object: ccid(2),
                    polarity: true,
                },
                DreamRelationEdge {
                    source_object: obj_ref(2),
                    subject: ccid(2),
                    predicate: ccid(10),
                    object: ccid(3),
                    polarity: true,
                },
                // Opposing claim on (ccid(1), ccid(10)) with object ccid(99) & source_object 8 (which symbolic validator will reject!)
                DreamRelationEdge {
                    source_object: obj_ref(8),
                    subject: ccid(1),
                    predicate: ccid(10),
                    object: ccid(2),
                    polarity: false,
                },
            ],
            unsatisfied_needs: vec![DreamNeedBridgeTarget {
                need_id: [0x77; 32],
                desired_role: ccid(50),
            }],
            affordances: vec![
                DreamAffordanceHop {
                    affordance_object: obj_ref(3),
                    input_role: ccid(40),
                    output_role: ccid(45),
                },
                DreamAffordanceHop {
                    affordance_object: obj_ref(4),
                    input_role: ccid(45),
                    output_role: ccid(50),
                },
            ],
        };
        let input_payload = snapshot.to_canonical_input_bytes().unwrap();
        let input_commitment = cognitive_input_commitment(&input_payload);

        // Build authorized DelegationPermit for TypedCognitiveExecutor.
        let actor = ActorId::from_bytes([1; 32]);
        let key = SigningKey::from_bytes(&[2; 32]);
        let delegation_ref = EventCid::from_bytes([3; 32]);
        let mut inception = FeedInception::new(
            *key.verifying_key().as_bytes(),
            NamespaceCommitment::derive(b"dream-executor-test", [4; 32]).unwrap(),
            0,
            DeviceId::from_bytes([5; 32]),
        );
        inception.actor_delegation_ref = Some(delegation_ref.into_bytes());
        let signed: SignedFeedInception = inception.sign(&key).unwrap();
        let feed = decode_feed_inception(&signed.encode().unwrap()).unwrap();
        let mut key_state = KeyStateReducer::new(EventCid::from_bytes([6; 32]));
        assert_eq!(
            key_state.accept_root(ScopedDelegation {
                grant: DelegationGrant {
                    actor,
                    device: feed.signed.inception.owner_device,
                    subject_feed: feed.feed_id,
                    delegation_ref,
                    namespace_commitment: None,
                    first_generation: 0,
                    last_generation: 0,
                    proof: EventCid::from_bytes([7; 32]),
                },
                parent_delegation_ref: None,
            }),
            KeyStateApplyOutcome::Accepted
        );
        let capability = ObjectCid::from_bytes([8; 32]);
        let effect = ccid(9);
        let purpose = ccid(11);
        let budget = Budget::new(8, 65_536, 64, 8).unwrap();
        let permit_body = DelegationPermitBody {
            issuer: actor,
            executor: actor,
            capability_definition: capability,
            input_commitments: vec![input_commitment],
            allowed_effect_classes: vec![effect],
            purpose,
            budget,
            retention: RetentionRule::NoTraining,
            onward_delegation: false,
            parent_permit: None,
            not_before: 10,
            expires_at: 100,
            nonce: [12; 32],
        };
        let signed_permit = SignedDelegationPermit::sign(permit_body, &feed, &key)
            .unwrap()
            .encode()
            .unwrap();
        let authenticated =
            authenticate_delegation_permit(&signed_permit, &feed, &key_state).unwrap();
        let permit_id = authenticated.permit_id;
        let mut permits = PermitValidator::default();
        permits.submit(authenticated, 10).unwrap();

        let task = CognitiveTask {
            task_id: [0x21; 32],
            permit_id,
            offer_ref: obj_ref(13),
            implementation_manifest: ObjectCid::from_bytes([14; 32]),
            capability_definition: capability,
            input_payload,
            input_commitments: vec![input_commitment],
            schema_prompt_parameter_commitments: vec![[15; 32]],
            requested_effect_classes: vec![effect],
            purpose,
            budget,
            retention: RetentionRule::NoTraining,
            seed: Some([16; 32]),
            deadline_tick: 80,
        };

        struct DreamSymbolicFirewall;
        impl SymbolicMappingValidator for DreamSymbolicFirewall {
            fn validate_mapping(
                &mut self,
                request: SymbolicValidationRequest<'_>,
            ) -> Result<SymbolicMappingAssessment, ModelRecallError> {
                let has_rejected_obj = request.candidate_objects.iter().any(|o| o.cid == [8; 32]);
                Ok(SymbolicMappingAssessment {
                    candidate_id: request.candidate_id,
                    mapping_kernel: MappingKernelCid::from_bytes(request.candidate_id),
                    validation_context: request.validation_context,
                    validator_version: ObjectReference::new(0, [0x33; 32]),
                    checks: vec![SymbolicCheck {
                        check_kind: ConceptCcid::from_bytes([0x44; 16]),
                        evaluation: if has_rejected_obj {
                            ConstraintEvaluation::Violated
                        } else {
                            ConstraintEvaluation::Satisfied
                        },
                        required: true,
                    }],
                })
            }
        }

        let executor = TypedCognitiveExecutor::new(CognitiveExecutionPolicy {
            cancelled_limitation: ccid(20),
            deadline_limitation: ccid(21),
            resource_limitation: ccid(22),
            backend_failure_limitation: ccid(23),
            backend_protocol_limitation: ccid(24),
        });

        let run = DreamConsolidatorV1::run_in_executor(
            &executor,
            &permits,
            task,
            &snapshot,
            &mut DreamSymbolicFirewall,
            20,
            &CancellationToken::new(),
        )
        .unwrap();

        assert_eq!(run.execution.termination, CognitiveTermination::Completed);
        assert!(!run.execution.materializes_output());
        assert!(!run.is_executable());
        assert!(!run.emits_obkg_rewards());
        assert_eq!(run.rejected_by_symbolic_firewall, 1);
        assert_eq!(run.proposals.len(), 2);
        assert!(run.proposals.iter().all(|p| !p.is_executable()));
        assert!(run.proposals.iter().all(|p| !p.mutates_active_graph()));
        assert!(run
            .proposals
            .iter()
            .any(|p| p.kind == DreamCandidateKind::TransitiveClosure));
        assert!(run
            .proposals
            .iter()
            .any(|p| p.kind == DreamCandidateKind::StandingNeedBridge));
    }
}
