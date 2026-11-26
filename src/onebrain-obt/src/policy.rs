//! Geography- and identity-neutral `RewardPolicyV1` (`§3.3`).
//!
//! `RewardPolicyV1` is canonically CBOR-encoded and content-addressed under
//! `"onebrain:vnext:obt-reward-policy:1\0"`. It explicitly rejects negative or
//! unbounded factors (`> 20_000 bps`) and contains zero geographic, IP, `NodeTier`,
//! fiat exchange rate, wage index, or identity-rank inputs.

use std::collections::BTreeMap;

use ku_core::foundation::{
    CanonicalError, CanonicalValue, ConceptCcid, ObjectCid, ResourceProfile,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::codec::{
    contains_negative_anywhere, decode_value, domain_digest, encode_ccid, encode_value,
    expect_array, expect_ccid, expect_map, expect_u16, expect_u32, expect_u64, req_field,
};
use crate::units::{ObtAmount, MAX_FACTOR_BPS, QUANTA_PER_OBT};

/// Domain separator for `RewardPolicyV1` `ObjectCid` derivation.
pub const REWARD_POLICY_DOMAIN: &[u8] = b"onebrain:vnext:obt-reward-policy:1\0";

pub const REWARD_POLICY_SCHEMA_MAJOR: u64 = 1;
pub const REWARD_POLICY_SCHEMA_MINOR: u64 = 0;

/// Maximum statement count honored in Track A complexity formula (`§4.1.3`).
pub const MAX_VERIFIED_STATEMENTS_CAP: u32 = 32;

/// Maximum constraint count honored in Track A complexity formula (`§4.1.3`).
pub const MAX_RESOLVED_CONSTRAINTS_CAP: u32 = 64;

const MAX_POLICY_TABLE_ENTRIES: usize = 1_024;

/// Track A cognitive work classes (`D-014`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum WorkClass {
    DelegatedEncode = 0,
    BlindFidelityVerify = 1,
}

impl WorkClass {
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::DelegatedEncode),
            1 => Some(Self::BlindFidelityVerify),
            _ => None,
        }
    }
}

/// Structural quanta weights for one `WorkClass` (zero UTF-8 byte-length input).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkClassWeights {
    pub base_quanta: ObtAmount,
    pub per_statement_quanta: ObtAmount,
    pub per_constraint_quanta: ObtAmount,
}

impl WorkClassWeights {
    pub(crate) fn to_canonical_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, self.base_quanta.to_canonical_value()),
            (1, self.per_statement_quanta.to_canonical_value()),
            (2, self.per_constraint_quanta.to_canonical_value()),
        ])
    }

    pub(crate) fn from_canonical_value(value: &CanonicalValue) -> Result<Self, PolicyError> {
        let entries = expect_map(value, &[0, 1, 2])?;
        Ok(Self {
            base_quanta: ObtAmount::from_canonical_value(req_field(entries, 0)?)?,
            per_statement_quanta: ObtAmount::from_canonical_value(req_field(entries, 1)?)?,
            per_constraint_quanta: ObtAmount::from_canonical_value(req_field(entries, 2)?)?,
        })
    }
}

/// Discrete evidence quality tiers mapped to `F_evidence` basis points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum EvidenceQualityTier {
    SelfReportedOrUnwitnessed = 0,
    SingleWitnessed = 1,
    MultiMeasurementWithBaseline = 2,
    CounterfactualSupported = 3,
}

impl EvidenceQualityTier {
    pub const ALL: [Self; 4] = [
        Self::SelfReportedOrUnwitnessed,
        Self::SingleWitnessed,
        Self::MultiMeasurementWithBaseline,
        Self::CounterfactualSupported,
    ];

    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::SelfReportedOrUnwitnessed),
            1 => Some(Self::SingleWitnessed),
            2 => Some(Self::MultiMeasurementWithBaseline),
            3 => Some(Self::CounterfactualSupported),
            _ => None,
        }
    }
}

/// Discrete causal support tiers mapped to `F_causal` basis points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum CausalSupportTier {
    WeakIndirect = 0,
    DirectUseSupported = 1,
    MultiHopDerivationSupported = 2,
    StrongCounterfactualCausal = 3,
}

impl CausalSupportTier {
    pub const ALL: [Self; 4] = [
        Self::WeakIndirect,
        Self::DirectUseSupported,
        Self::MultiHopDerivationSupported,
        Self::StrongCounterfactualCausal,
    ];

    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::WeakIndirect),
            1 => Some(Self::DirectUseSupported),
            2 => Some(Self::MultiHopDerivationSupported),
            3 => Some(Self::StrongCounterfactualCausal),
            _ => None,
        }
    }
}

/// Discrete corroboration tiers mapped to `F_corr` basis points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum CorroborationTier {
    Uncorroborated = 0,
    SingleGroup = 1,
    TwoDistinctGroups = 2,
    ThreePlusDistinctGroups = 3,
}

impl CorroborationTier {
    pub const ALL: [Self; 4] = [
        Self::Uncorroborated,
        Self::SingleGroup,
        Self::TwoDistinctGroups,
        Self::ThreePlusDistinctGroups,
    ];

    pub(crate) const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Uncorroborated),
            1 => Some(Self::SingleGroup),
            2 => Some(Self::TwoDistinctGroups),
            3 => Some(Self::ThreePlusDistinctGroups),
            _ => None,
        }
    }
}

/// Validation and canonicalization errors for `RewardPolicyV1`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PolicyError {
    #[error("OBT_POLICY_CANONICAL: {0}")]
    Canonical(#[from] CanonicalError),
    #[error(
        "OBT_POLICY_NEGATIVE_FACTOR: negative integer values are prohibited in RewardPolicyV1"
    )]
    NegativeFactorProhibited,
    #[error(
        "OBT_POLICY_NON_NEUTRAL_FIELD: unknown or non-neutral geography/identity field rejected"
    )]
    NonNeutralOrUnknownField,
    #[error("OBT_POLICY_FACTOR_EXCEEDS_MAX_BPS: {field} = {bps} bps exceeds {MAX_FACTOR_BPS} bps")]
    FactorExceedsMaxBps { field: &'static str, bps: u32 },
    #[error("OBT_POLICY_HOP_ATTENUATION_INVALID: {bps} bps exceeds 10_000 bps")]
    InvalidHopAttenuation { bps: u32 },
    #[error("OBT_POLICY_INVALID_BOUNDS: {0}")]
    InvalidBounds(&'static str),
    #[error("OBT_POLICY_SCHEMA_VERSION: unsupported schema version {major}.{minor}")]
    UnsupportedSchemaVersion { major: u64, minor: u64 },
}

/// Geography- and identity-neutral reward policy (`P5 §3.3`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RewardPolicyV1 {
    /// Base `ObtAmount` per `outcome_class` (`ConceptCcid`).
    pub outcome_base_budgets: BTreeMap<ConceptCcid, ObtAmount>,
    /// Fallback base budget when an `outcome_class` has no explicit override.
    pub default_outcome_base_budget: ObtAmount,
    /// Normalized gain bucket (`u16` bucket id) -> `G_norm` in `0..=20_000` bps.
    pub gain_norm_bps_table: BTreeMap<u16, u32>,
    /// `EvidenceQualityTier` -> `F_evidence` in `0..=20_000` bps.
    pub evidence_bps_table: BTreeMap<EvidenceQualityTier, u32>,
    /// `CausalSupportTier` -> `F_causal` in `0..=20_000` bps.
    pub causal_bps_table: BTreeMap<CausalSupportTier, u32>,
    /// `CorroborationTier` -> `F_corr` in `0..=20_000` bps.
    pub corroboration_bps_table: BTreeMap<CorroborationTier, u32>,
    /// Track A structural weights for `WorkClass::DelegatedEncode`.
    pub delegated_encode_weights: WorkClassWeights,
    /// Track A structural weights for `WorkClass::BlindFidelityVerify`.
    pub blind_verify_weights: WorkClassWeights,
    /// Per-`ActorId` epoch issuance ceiling.
    pub per_actor_epoch_ceiling: ObtAmount,
    /// Per-4D-correlation-group epoch issuance ceiling.
    pub per_correlation_group_epoch_ceiling: ObtAmount,
    /// Micro-ceiling for self-reported / unwitnessed Track B closures (`0` or bounded).
    pub self_reported_micro_ceiling: ObtAmount,
    /// Per-hop retention factor in bps (`0..=10_000`) along `DerivationEvidence` DAGs.
    pub derivation_hop_attenuation_bps: u32,
    /// Frontier sequence span required before an uncontested `PendingMintRecord` can finalize.
    pub challenge_window_frontier_span: u64,
    /// Required VRF-selected audit co-signatures (`>= 1`).
    pub required_audit_quorum: u16,
    /// Minimum distinct 4D correlation verifier groups required for Track A (`>= 2`).
    pub minimum_work_verifier_groups: u16,
}

impl Default for RewardPolicyV1 {
    fn default() -> Self {
        Self::default_v1()
    }
}

impl RewardPolicyV1 {
    /// Deterministic reference policy v1 with neutral integer parameters.
    pub fn default_v1() -> Self {
        let mut gain_norm_bps_table = BTreeMap::new();
        gain_norm_bps_table.insert(0, 0);
        gain_norm_bps_table.insert(1, 5_000);
        gain_norm_bps_table.insert(2, 10_000);
        gain_norm_bps_table.insert(3, 15_000);
        gain_norm_bps_table.insert(4, 20_000);

        let mut evidence_bps_table = BTreeMap::new();
        evidence_bps_table.insert(EvidenceQualityTier::SelfReportedOrUnwitnessed, 0);
        evidence_bps_table.insert(EvidenceQualityTier::SingleWitnessed, 5_000);
        evidence_bps_table.insert(EvidenceQualityTier::MultiMeasurementWithBaseline, 10_000);
        evidence_bps_table.insert(EvidenceQualityTier::CounterfactualSupported, 15_000);

        let mut causal_bps_table = BTreeMap::new();
        causal_bps_table.insert(CausalSupportTier::WeakIndirect, 2_500);
        causal_bps_table.insert(CausalSupportTier::DirectUseSupported, 10_000);
        causal_bps_table.insert(CausalSupportTier::MultiHopDerivationSupported, 10_000);
        causal_bps_table.insert(CausalSupportTier::StrongCounterfactualCausal, 15_000);

        let mut corroboration_bps_table = BTreeMap::new();
        corroboration_bps_table.insert(CorroborationTier::Uncorroborated, 0);
        corroboration_bps_table.insert(CorroborationTier::SingleGroup, 5_000);
        corroboration_bps_table.insert(CorroborationTier::TwoDistinctGroups, 10_000);
        corroboration_bps_table.insert(CorroborationTier::ThreePlusDistinctGroups, 15_000);

        Self {
            outcome_base_budgets: BTreeMap::new(),
            default_outcome_base_budget: ObtAmount(10 * QUANTA_PER_OBT),
            gain_norm_bps_table,
            evidence_bps_table,
            causal_bps_table,
            corroboration_bps_table,
            delegated_encode_weights: WorkClassWeights {
                base_quanta: ObtAmount(100_000_000),
                per_statement_quanta: ObtAmount(50_000_000),
                per_constraint_quanta: ObtAmount(25_000_000),
            },
            blind_verify_weights: WorkClassWeights {
                base_quanta: ObtAmount(100_000_000),
                per_statement_quanta: ObtAmount(40_000_000),
                per_constraint_quanta: ObtAmount(20_000_000),
            },
            per_actor_epoch_ceiling: ObtAmount(100 * QUANTA_PER_OBT),
            per_correlation_group_epoch_ceiling: ObtAmount(250 * QUANTA_PER_OBT),
            self_reported_micro_ceiling: ObtAmount::ZERO,
            derivation_hop_attenuation_bps: 7_000,
            challenge_window_frontier_span: 16,
            required_audit_quorum: 2,
            minimum_work_verifier_groups: 2,
        }
    }

    /// Explicitly validate neutrality, factor ceilings (`<= 20_000` bps), and structural bounds.
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.outcome_base_budgets.len() > MAX_POLICY_TABLE_ENTRIES
            || self.gain_norm_bps_table.len() > MAX_POLICY_TABLE_ENTRIES
            || self.gain_norm_bps_table.is_empty()
        {
            return Err(PolicyError::InvalidBounds("policy lookup table size"));
        }
        for tier in EvidenceQualityTier::ALL {
            if !self.evidence_bps_table.contains_key(&tier) {
                return Err(PolicyError::InvalidBounds(
                    "missing evidence_bps_table tier",
                ));
            }
        }
        for tier in CausalSupportTier::ALL {
            if !self.causal_bps_table.contains_key(&tier) {
                return Err(PolicyError::InvalidBounds("missing causal_bps_table tier"));
            }
        }
        for tier in CorroborationTier::ALL {
            if !self.corroboration_bps_table.contains_key(&tier) {
                return Err(PolicyError::InvalidBounds(
                    "missing corroboration_bps_table tier",
                ));
            }
        }
        for &bps in self.gain_norm_bps_table.values() {
            if bps > MAX_FACTOR_BPS {
                return Err(PolicyError::FactorExceedsMaxBps {
                    field: "gain_norm_bps",
                    bps,
                });
            }
        }
        for &bps in self.evidence_bps_table.values() {
            if bps > MAX_FACTOR_BPS {
                return Err(PolicyError::FactorExceedsMaxBps {
                    field: "evidence_bps",
                    bps,
                });
            }
        }
        for &bps in self.causal_bps_table.values() {
            if bps > MAX_FACTOR_BPS {
                return Err(PolicyError::FactorExceedsMaxBps {
                    field: "causal_bps",
                    bps,
                });
            }
        }
        for &bps in self.corroboration_bps_table.values() {
            if bps > MAX_FACTOR_BPS {
                return Err(PolicyError::FactorExceedsMaxBps {
                    field: "corroboration_bps",
                    bps,
                });
            }
        }
        if self.derivation_hop_attenuation_bps > 10_000 {
            return Err(PolicyError::InvalidHopAttenuation {
                bps: self.derivation_hop_attenuation_bps,
            });
        }
        if self.challenge_window_frontier_span == 0 {
            return Err(PolicyError::InvalidBounds(
                "challenge_window_frontier_span must be >= 1",
            ));
        }
        if self.required_audit_quorum == 0 {
            return Err(PolicyError::InvalidBounds(
                "required_audit_quorum must be >= 1",
            ));
        }
        if self.minimum_work_verifier_groups < 2 {
            return Err(PolicyError::InvalidBounds(
                "minimum_work_verifier_groups must be >= 2",
            ));
        }
        if self.per_actor_epoch_ceiling.is_zero()
            || self.per_correlation_group_epoch_ceiling.is_zero()
            || self.self_reported_micro_ceiling > self.per_actor_epoch_ceiling
        {
            return Err(PolicyError::InvalidBounds("epoch issuance ceilings"));
        }
        Ok(())
    }

    pub const fn uses_geography_or_identity_rank(&self) -> bool {
        false
    }

    pub fn base_budget_for(&self, outcome_class: ConceptCcid) -> ObtAmount {
        self.outcome_base_budgets
            .get(&outcome_class)
            .copied()
            .unwrap_or(self.default_outcome_base_budget)
    }

    pub fn work_weights(&self, work_class: WorkClass) -> WorkClassWeights {
        match work_class {
            WorkClass::DelegatedEncode => self.delegated_encode_weights,
            WorkClass::BlindFidelityVerify => self.blind_verify_weights,
        }
    }

    pub fn to_canonical_value(&self) -> CanonicalValue {
        let outcome_rows = self
            .outcome_base_budgets
            .iter()
            .map(|(ccid, amt)| {
                CanonicalValue::Array(vec![encode_ccid(*ccid), amt.to_canonical_value()])
            })
            .collect();
        let gain_rows = self
            .gain_norm_bps_table
            .iter()
            .map(|(bucket, bps)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Unsigned(u64::from(*bucket)),
                    CanonicalValue::Unsigned(u64::from(*bps)),
                ])
            })
            .collect();
        let evidence_rows = self
            .evidence_bps_table
            .iter()
            .map(|(tier, bps)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Unsigned(*tier as u64),
                    CanonicalValue::Unsigned(u64::from(*bps)),
                ])
            })
            .collect();
        let causal_rows = self
            .causal_bps_table
            .iter()
            .map(|(tier, bps)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Unsigned(*tier as u64),
                    CanonicalValue::Unsigned(u64::from(*bps)),
                ])
            })
            .collect();
        let corr_rows = self
            .corroboration_bps_table
            .iter()
            .map(|(tier, bps)| {
                CanonicalValue::Array(vec![
                    CanonicalValue::Unsigned(*tier as u64),
                    CanonicalValue::Unsigned(u64::from(*bps)),
                ])
            })
            .collect();

        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(REWARD_POLICY_SCHEMA_MAJOR)),
            (1, CanonicalValue::Unsigned(REWARD_POLICY_SCHEMA_MINOR)),
            (2, CanonicalValue::Array(outcome_rows)),
            (3, self.default_outcome_base_budget.to_canonical_value()),
            (4, CanonicalValue::Array(gain_rows)),
            (5, CanonicalValue::Array(evidence_rows)),
            (6, CanonicalValue::Array(causal_rows)),
            (7, CanonicalValue::Array(corr_rows)),
            (8, self.delegated_encode_weights.to_canonical_value()),
            (9, self.blind_verify_weights.to_canonical_value()),
            (10, self.per_actor_epoch_ceiling.to_canonical_value()),
            (
                11,
                self.per_correlation_group_epoch_ceiling
                    .to_canonical_value(),
            ),
            (12, self.self_reported_micro_ceiling.to_canonical_value()),
            (
                13,
                CanonicalValue::Unsigned(u64::from(self.derivation_hop_attenuation_bps)),
            ),
            (
                14,
                CanonicalValue::Unsigned(self.challenge_window_frontier_span),
            ),
            (
                15,
                CanonicalValue::Unsigned(u64::from(self.required_audit_quorum)),
            ),
            (
                16,
                CanonicalValue::Unsigned(u64::from(self.minimum_work_verifier_groups)),
            ),
        ])
    }

    pub fn encode_canonical_bytes(&self) -> Result<Vec<u8>, PolicyError> {
        self.validate()?;
        Ok(encode_value(
            &self.to_canonical_value(),
            ResourceProfile::ObjectV1,
        )?)
    }

    pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Self, PolicyError> {
        let value = decode_value(bytes, ResourceProfile::ObjectV1)?;
        if contains_negative_anywhere(&value) {
            return Err(PolicyError::NegativeFactorProhibited);
        }
        let allowed = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let entries = expect_map(&value, &allowed).map_err(|err| match err.kind() {
            ku_core::foundation::CanonicalErrorKind::UnknownField => {
                PolicyError::NonNeutralOrUnknownField
            }
            _ => PolicyError::Canonical(err),
        })?;

        let major = expect_u64(req_field(entries, 0)?)?;
        let minor = expect_u64(req_field(entries, 1)?)?;
        if major != REWARD_POLICY_SCHEMA_MAJOR || minor != REWARD_POLICY_SCHEMA_MINOR {
            return Err(PolicyError::UnsupportedSchemaVersion { major, minor });
        }

        let mut outcome_base_budgets = BTreeMap::new();
        let mut prev_ccid: Option<ConceptCcid> = None;
        for row in expect_array(req_field(entries, 2)?)? {
            let pair = expect_array(row)?;
            if pair.len() != 2 {
                return Err(PolicyError::InvalidBounds("outcome_base_budgets row"));
            }
            let ccid = expect_ccid(&pair[0])?;
            if prev_ccid.is_some_and(|p| p >= ccid) {
                return Err(PolicyError::InvalidBounds(
                    "outcome_base_budgets sort order",
                ));
            }
            prev_ccid = Some(ccid);
            let amt = ObtAmount::from_canonical_value(&pair[1])?;
            outcome_base_budgets.insert(ccid, amt);
        }

        let default_outcome_base_budget = ObtAmount::from_canonical_value(req_field(entries, 3)?)?;

        let mut gain_norm_bps_table = BTreeMap::new();
        let mut prev_bucket: Option<u16> = None;
        for row in expect_array(req_field(entries, 4)?)? {
            let pair = expect_array(row)?;
            if pair.len() != 2 {
                return Err(PolicyError::InvalidBounds("gain_norm_bps_table row"));
            }
            let bucket = expect_u16(&pair[0])?;
            if prev_bucket.is_some_and(|p| p >= bucket) {
                return Err(PolicyError::InvalidBounds("gain_norm_bps_table sort order"));
            }
            prev_bucket = Some(bucket);
            let bps = expect_u32(&pair[1])?;
            gain_norm_bps_table.insert(bucket, bps);
        }

        let mut evidence_bps_table = BTreeMap::new();
        let mut prev_ev: Option<u64> = None;
        for row in expect_array(req_field(entries, 5)?)? {
            let pair = expect_array(row)?;
            if pair.len() != 2 {
                return Err(PolicyError::InvalidBounds("evidence_bps_table row"));
            }
            let raw_tier = expect_u64(&pair[0])?;
            if prev_ev.is_some_and(|p| p >= raw_tier) {
                return Err(PolicyError::InvalidBounds("evidence_bps_table sort order"));
            }
            prev_ev = Some(raw_tier);
            let tier = EvidenceQualityTier::from_code(raw_tier)
                .ok_or(PolicyError::InvalidBounds("unknown EvidenceQualityTier"))?;
            let bps = expect_u32(&pair[1])?;
            evidence_bps_table.insert(tier, bps);
        }

        let mut causal_bps_table = BTreeMap::new();
        let mut prev_causal: Option<u64> = None;
        for row in expect_array(req_field(entries, 6)?)? {
            let pair = expect_array(row)?;
            if pair.len() != 2 {
                return Err(PolicyError::InvalidBounds("causal_bps_table row"));
            }
            let raw_tier = expect_u64(&pair[0])?;
            if prev_causal.is_some_and(|p| p >= raw_tier) {
                return Err(PolicyError::InvalidBounds("causal_bps_table sort order"));
            }
            prev_causal = Some(raw_tier);
            let tier = CausalSupportTier::from_code(raw_tier)
                .ok_or(PolicyError::InvalidBounds("unknown CausalSupportTier"))?;
            let bps = expect_u32(&pair[1])?;
            causal_bps_table.insert(tier, bps);
        }

        let mut corroboration_bps_table = BTreeMap::new();
        let mut prev_corr: Option<u64> = None;
        for row in expect_array(req_field(entries, 7)?)? {
            let pair = expect_array(row)?;
            if pair.len() != 2 {
                return Err(PolicyError::InvalidBounds("corroboration_bps_table row"));
            }
            let raw_tier = expect_u64(&pair[0])?;
            if prev_corr.is_some_and(|p| p >= raw_tier) {
                return Err(PolicyError::InvalidBounds(
                    "corroboration_bps_table sort order",
                ));
            }
            prev_corr = Some(raw_tier);
            let tier = CorroborationTier::from_code(raw_tier)
                .ok_or(PolicyError::InvalidBounds("unknown CorroborationTier"))?;
            let bps = expect_u32(&pair[1])?;
            corroboration_bps_table.insert(tier, bps);
        }

        let policy = Self {
            outcome_base_budgets,
            default_outcome_base_budget,
            gain_norm_bps_table,
            evidence_bps_table,
            causal_bps_table,
            corroboration_bps_table,
            delegated_encode_weights: WorkClassWeights::from_canonical_value(req_field(
                entries, 8,
            )?)?,
            blind_verify_weights: WorkClassWeights::from_canonical_value(req_field(entries, 9)?)?,
            per_actor_epoch_ceiling: ObtAmount::from_canonical_value(req_field(entries, 10)?)?,
            per_correlation_group_epoch_ceiling: ObtAmount::from_canonical_value(req_field(
                entries, 11,
            )?)?,
            self_reported_micro_ceiling: ObtAmount::from_canonical_value(req_field(entries, 12)?)?,
            derivation_hop_attenuation_bps: expect_u32(req_field(entries, 13)?)?,
            challenge_window_frontier_span: expect_u64(req_field(entries, 14)?)?,
            required_audit_quorum: expect_u16(req_field(entries, 15)?)?,
            minimum_work_verifier_groups: expect_u16(req_field(entries, 16)?)?,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Compute the deterministic `ObjectCid` under `"onebrain:vnext:obt-reward-policy:1\0"`.
    pub fn policy_cid(&self) -> ObjectCid {
        let bytes = encode_value(&self.to_canonical_value(), ResourceProfile::ObjectV1)
            .expect("RewardPolicyV1 canonical value fits ObjectV1 profile");
        ObjectCid::from_bytes(domain_digest(REWARD_POLICY_DOMAIN, &bytes))
    }

    /// Validate policy bounds and neutrality before deriving its `ObjectCid`.
    pub fn try_policy_cid(&self) -> Result<ObjectCid, PolicyError> {
        let bytes = self.encode_canonical_bytes()?;
        Ok(ObjectCid::from_bytes(domain_digest(
            REWARD_POLICY_DOMAIN,
            &bytes,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reward_policy_canonical_roundtrip_and_neutrality_validation() {
        let mut policy = RewardPolicyV1::default_v1();
        policy.outcome_base_budgets.insert(
            ConceptCcid::from_bytes([7; 16]),
            ObtAmount(50 * QUANTA_PER_OBT),
        );
        let bytes = policy.encode_canonical_bytes().unwrap();
        let decoded = RewardPolicyV1::decode_canonical_bytes(&bytes).unwrap();
        assert_eq!(decoded, policy);
        assert_eq!(decoded.policy_cid(), policy.policy_cid());
        assert!(!decoded.uses_geography_or_identity_rank());

        // Reject factor > 20_000 bps.
        let mut bad = policy.clone();
        bad.gain_norm_bps_table.insert(9, 20_001);
        assert!(matches!(
            bad.validate(),
            Err(PolicyError::FactorExceedsMaxBps {
                field: "gain_norm_bps",
                bps: 20_001
            })
        ));

        // Reject injected negative CBOR integer or non-neutral extra field.
        let CanonicalValue::Map(mut raw_entries) = policy.to_canonical_value() else {
            unreachable!()
        };
        raw_entries.push((99, CanonicalValue::Text("GeoRegion:EU".to_owned())));
        let non_neutral_bytes =
            encode_value(&CanonicalValue::Map(raw_entries), ResourceProfile::ObjectV1).unwrap();
        assert_eq!(
            RewardPolicyV1::decode_canonical_bytes(&non_neutral_bytes),
            Err(PolicyError::NonNeutralOrUnknownField)
        );
    }
}
