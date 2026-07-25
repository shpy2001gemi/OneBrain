//! Shared selector, budget, carrier and scoped-coverage contracts.

use std::collections::BTreeSet;

use super::canonical::{
    canonicalize_set_by_key, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
};
use super::content_id::{EventCid, ReservedDomain, SelectorCid};
use super::feed::NamespaceCommitment;
use super::object::{DisclosureClass, ObjectKind};

pub const SELECTOR_PROFILE_MAJOR: u64 = 1;
pub const SELECTOR_PROFILE_MINOR: u64 = 0;
pub const MAX_SELECTOR_MEMBERS: usize = 4_096;
pub const MAX_BUDGET_RECORDS: u64 = 1_000_000;
pub const MAX_BUDGET_BYTES: u64 = 1 << 30;
pub const MAX_BUDGET_WORK_UNITS: u64 = 1_000_000_000;
pub const MAX_BUDGET_DEPTH: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum SelectorPurpose {
    PublicKnowledgeExchange = 0,
    ExactCidFetch = 1,
    Reconciliation = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum InventoryRecordKind {
    Object = 0,
    Event = 1,
    MappingKernel = 2,
    /// Signed feed/key inception material required before authored events can
    /// be signature-validated. This is control-plane data, never KU content.
    FeedInception = 3,
    /// Self-certifying actor-root and authority-chain control records. These
    /// are not knowledge events and must pass their dedicated schema decoder.
    AuthorityEvent = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub max_records: u64,
    pub max_bytes: u64,
    pub max_work_units: u64,
    pub max_depth: u32,
}

impl Budget {
    pub fn new(
        max_records: u64,
        max_bytes: u64,
        max_work_units: u64,
        max_depth: u32,
    ) -> Result<Self, InventoryError> {
        let budget = Self {
            max_records,
            max_bytes,
            max_work_units,
            max_depth,
        };
        budget.validate()?;
        Ok(budget)
    }

    fn validate(self) -> Result<(), InventoryError> {
        if self.max_records == 0
            || self.max_records > MAX_BUDGET_RECORDS
            || self.max_bytes == 0
            || self.max_bytes > MAX_BUDGET_BYTES
            || self.max_work_units == 0
            || self.max_work_units > MAX_BUDGET_WORK_UNITS
            || self.max_depth == 0
            || self.max_depth > MAX_BUDGET_DEPTH
        {
            return Err(InventoryError::InvalidBudget);
        }
        Ok(())
    }

    fn to_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.max_records)),
            (1, CanonicalValue::Unsigned(self.max_bytes)),
            (2, CanonicalValue::Unsigned(self.max_work_units)),
            (3, CanonicalValue::Unsigned(u64::from(self.max_depth))),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum CarrierKind {
    InMemory = 0,
    FileBundle = 1,
    Quic = 2,
    Ble = 3,
    DelayTolerantBundle = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarrierProfile {
    pub kind: CarrierKind,
    pub max_frame_bytes: u64,
    pub max_bundle_bytes: u64,
    pub store_carry_forward: bool,
    pub bidirectional: bool,
}

impl CarrierProfile {
    pub fn validate(self) -> Result<(), InventoryError> {
        if self.max_frame_bytes == 0
            || self.max_frame_bytes > self.max_bundle_bytes
            || self.max_bundle_bytes > MAX_BUDGET_BYTES
        {
            return Err(InventoryError::InvalidCarrierProfile);
        }
        Ok(())
    }

    fn to_value(self) -> CanonicalValue {
        CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(self.kind as u64)),
            (1, CanonicalValue::Unsigned(self.max_frame_bytes)),
            (2, CanonicalValue::Unsigned(self.max_bundle_bytes)),
            (3, CanonicalValue::Bool(self.store_carry_forward)),
            (4, CanonicalValue::Bool(self.bidirectional)),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub purpose: SelectorPurpose,
    pub namespace: NamespaceCommitment,
    pub record_kinds: Vec<InventoryRecordKind>,
    pub object_kinds: Vec<ObjectKind>,
    pub disclosure_classes: Vec<DisclosureClass>,
    pub frontier: Vec<EventCid>,
    pub budget: Budget,
    pub carrier: CarrierProfile,
}

impl Selector {
    pub fn validate(&self) -> Result<(), InventoryError> {
        self.budget.validate()?;
        self.carrier.validate()?;
        if self.record_kinds.is_empty()
            || self.disclosure_classes.is_empty()
            || self.record_kinds.len() > MAX_SELECTOR_MEMBERS
            || self.object_kinds.len() > MAX_SELECTOR_MEMBERS
            || self.frontier.len() > MAX_SELECTOR_MEMBERS
        {
            return Err(InventoryError::Limit);
        }
        if self.disclosure_classes.iter().any(|class| {
            matches!(
                class,
                DisclosureClass::LocalOnly | DisclosureClass::NegotiatedEncrypted
            )
        }) {
            return Err(InventoryError::PrivateStorageClass);
        }
        unique_codes(self.record_kinds.iter().map(|kind| *kind as u64))?;
        unique_codes(self.object_kinds.iter().map(|kind| kind.0))?;
        unique_codes(self.disclosure_classes.iter().map(|class| *class as u64))?;
        unique_bytes(self.frontier.iter().map(|event| *event.as_bytes()))?;
        if self.record_kinds.contains(&InventoryRecordKind::Object) && self.object_kinds.is_empty()
        {
            return Err(InventoryError::MissingObjectKinds);
        }
        Ok(())
    }

    pub fn canonical_value(&self) -> Result<CanonicalValue, InventoryError> {
        self.validate()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(SELECTOR_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(SELECTOR_PROFILE_MINOR)),
            (2, CanonicalValue::Unsigned(self.purpose as u64)),
            (3, CanonicalValue::Bytes(self.namespace.as_bytes().to_vec())),
            (
                4,
                canonical_unsigned_set(self.record_kinds.iter().map(|kind| *kind as u64))?,
            ),
            (
                5,
                canonical_unsigned_set(self.object_kinds.iter().map(|kind| kind.0))?,
            ),
            (
                6,
                canonical_unsigned_set(self.disclosure_classes.iter().map(|class| *class as u64))?,
            ),
            (
                7,
                canonical_bytes_set(self.frontier.iter().map(|event| *event.as_bytes()))?,
            ),
            (8, self.budget.to_value()),
            (9, self.carrier.to_value()),
        ]))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, InventoryError> {
        encode_canonical(&self.canonical_value()?, ResourceProfile::ControlV1).map_err(Into::into)
    }

    pub fn cid(&self) -> Result<SelectorCid, InventoryError> {
        SelectorCid::compute(ReservedDomain::Selector, &self.canonical_bytes()?)
            .map_err(|_| InventoryError::Domain)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorOffer {
    pub selector: SelectorCid,
    pub supported_carriers: Vec<CarrierProfile>,
    pub offered_budget: Budget,
    pub source_frontier: Vec<EventCid>,
}

impl SelectorOffer {
    pub fn canonical_value(&self) -> Result<CanonicalValue, InventoryError> {
        self.offered_budget.validate()?;
        if self.supported_carriers.is_empty()
            || self.supported_carriers.len() > MAX_SELECTOR_MEMBERS
            || self.source_frontier.len() > MAX_SELECTOR_MEMBERS
        {
            return Err(InventoryError::Limit);
        }
        for carrier in &self.supported_carriers {
            carrier.validate()?;
        }
        let carriers = self
            .supported_carriers
            .iter()
            .map(|carrier| carrier.to_value())
            .collect();
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(self.selector.as_bytes().to_vec())),
            (1, canonical_set(carriers)?),
            (2, self.offered_budget.to_value()),
            (
                3,
                canonical_bytes_set(self.source_frontier.iter().map(|event| *event.as_bytes()))?,
            ),
        ]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageBasis {
    ExactInventory,
    ProbabilisticSummary { false_positive_ppm: u32 },
    Sampled,
}

impl CoverageBasis {
    fn to_value(self) -> Result<CanonicalValue, InventoryError> {
        match self {
            Self::ExactInventory => Ok(CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(0))])),
            Self::ProbabilisticSummary { false_positive_ppm }
                if (1..=1_000_000).contains(&false_positive_ppm) =>
            {
                Ok(CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(1)),
                    (1, CanonicalValue::Unsigned(u64::from(false_positive_ppm))),
                ]))
            }
            Self::ProbabilisticSummary { .. } => Err(InventoryError::InvalidCoverageBasis),
            Self::Sampled => Ok(CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(2))])),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageStatus {
    Partial,
    CompleteWithinSelector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u64)]
pub enum CoverageLimitation {
    BudgetExhausted = 0,
    PathLimited = 1,
    Probabilistic = 2,
    FrontierIncomplete = 3,
    UnsupportedRecordKind = 4,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageStatement {
    pub selector: SelectorCid,
    pub assessed_frontier: Vec<EventCid>,
    pub basis: CoverageBasis,
    pub status: CoverageStatus,
    pub returned_records: u64,
    pub returned_bytes: u64,
    pub continuation: Option<[u8; 32]>,
    pub limitations: Vec<CoverageLimitation>,
}

impl CoverageStatement {
    pub fn validate(&self) -> Result<(), InventoryError> {
        self.basis.to_value()?;
        if self.assessed_frontier.len() > MAX_SELECTOR_MEMBERS
            || self.limitations.len() > MAX_SELECTOR_MEMBERS
            || self.returned_records > MAX_BUDGET_RECORDS
            || self.returned_bytes > MAX_BUDGET_BYTES
        {
            return Err(InventoryError::Limit);
        }
        unique_bytes(self.assessed_frontier.iter().map(|event| *event.as_bytes()))?;
        unique_codes(self.limitations.iter().map(|limitation| *limitation as u64))?;
        if self.status == CoverageStatus::CompleteWithinSelector
            && (!matches!(self.basis, CoverageBasis::ExactInventory)
                || self.continuation.is_some()
                || !self.limitations.is_empty())
        {
            return Err(InventoryError::InvalidCompletionClaim);
        }
        Ok(())
    }

    pub fn canonical_value(&self) -> Result<CanonicalValue, InventoryError> {
        self.validate()?;
        let mut fields = vec![
            (0, CanonicalValue::Bytes(self.selector.as_bytes().to_vec())),
            (
                1,
                canonical_bytes_set(self.assessed_frontier.iter().map(|event| *event.as_bytes()))?,
            ),
            (2, self.basis.to_value()?),
            (
                3,
                CanonicalValue::Unsigned(match self.status {
                    CoverageStatus::Partial => 0,
                    CoverageStatus::CompleteWithinSelector => 1,
                }),
            ),
            (4, CanonicalValue::Unsigned(self.returned_records)),
            (5, CanonicalValue::Unsigned(self.returned_bytes)),
            (
                7,
                canonical_unsigned_set(
                    self.limitations.iter().map(|limitation| *limitation as u64),
                )?,
            ),
        ];
        if let Some(continuation) = self.continuation {
            fields.push((6, CanonicalValue::Bytes(continuation.to_vec())));
            fields.sort_by_key(|(key, _)| *key);
        }
        Ok(CanonicalValue::Map(fields))
    }

    /// Completion is always relative to `selector` and `assessed_frontier`.
    pub const fn is_complete_within_selector(&self) -> bool {
        matches!(self.status, CoverageStatus::CompleteWithinSelector)
    }

    /// OneBrain has no network-global completeness claim.
    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

pub fn public_knowledge_exchange_fixture_v1() -> Selector {
    Selector {
        purpose: SelectorPurpose::PublicKnowledgeExchange,
        namespace: NamespaceCommitment::from_bytes([0x11; 32]),
        record_kinds: vec![
            InventoryRecordKind::Object,
            InventoryRecordKind::Event,
            InventoryRecordKind::MappingKernel,
        ],
        object_kinds: vec![
            ObjectKind(2),
            ObjectKind(3),
            ObjectKind(4),
            ObjectKind(5),
            ObjectKind(6),
        ],
        disclosure_classes: vec![DisclosureClass::Public],
        frontier: vec![EventCid::from_bytes([0x22; 32])],
        budget: Budget {
            max_records: 1_024,
            max_bytes: 16 * 1024 * 1024,
            max_work_units: 100_000,
            max_depth: 32,
        },
        carrier: CarrierProfile {
            kind: CarrierKind::FileBundle,
            max_frame_bytes: 64 * 1024,
            max_bundle_bytes: 16 * 1024 * 1024,
            store_carry_forward: true,
            bidirectional: false,
        },
    }
}

fn unique_codes(values: impl Iterator<Item = u64>) -> Result<(), InventoryError> {
    let values = values.collect::<Vec<_>>();
    let unique = values.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(InventoryError::DuplicateMember)
    }
}

fn unique_bytes(values: impl Iterator<Item = [u8; 32]>) -> Result<(), InventoryError> {
    let values = values.collect::<Vec<_>>();
    let unique = values.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() == values.len() {
        Ok(())
    } else {
        Err(InventoryError::DuplicateMember)
    }
}

fn canonical_unsigned_set(
    values: impl Iterator<Item = u64>,
) -> Result<CanonicalValue, InventoryError> {
    canonical_set(values.map(CanonicalValue::Unsigned).collect())
}

fn canonical_bytes_set(
    values: impl Iterator<Item = [u8; 32]>,
) -> Result<CanonicalValue, InventoryError> {
    canonical_set(
        values
            .map(|value| CanonicalValue::Bytes(value.to_vec()))
            .collect(),
    )
}

fn canonical_set(values: Vec<CanonicalValue>) -> Result<CanonicalValue, InventoryError> {
    if values.len() > MAX_SELECTOR_MEMBERS {
        return Err(InventoryError::Limit);
    }
    let members = values
        .into_iter()
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ControlV1,
    )?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryError {
    Canonical(CanonicalError),
    Domain,
    Limit,
    InvalidBudget,
    InvalidCarrierProfile,
    PrivateStorageClass,
    MissingObjectKinds,
    DuplicateMember,
    InvalidCoverageBasis,
    InvalidCompletionClaim,
}

impl From<CanonicalError> for InventoryError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_identity_is_independent_of_set_insertion_order() {
        let first = public_knowledge_exchange_fixture_v1();
        let mut second = first.clone();
        second.record_kinds.reverse();
        second.object_kinds.reverse();
        assert_eq!(
            first.canonical_bytes().unwrap(),
            second.canonical_bytes().unwrap()
        );
        assert_eq!(first.cid().unwrap(), second.cid().unwrap());
    }

    #[test]
    fn public_selector_cannot_include_private_vault_classes() {
        for private in [
            DisclosureClass::LocalOnly,
            DisclosureClass::NegotiatedEncrypted,
        ] {
            let mut selector = public_knowledge_exchange_fixture_v1();
            selector.disclosure_classes.push(private);
            assert_eq!(
                selector.validate().unwrap_err(),
                InventoryError::PrivateStorageClass
            );
        }
    }

    #[test]
    fn probabilistic_or_limited_coverage_cannot_claim_completion() {
        let selector = public_knowledge_exchange_fixture_v1().cid().unwrap();
        let probabilistic = CoverageStatement {
            selector,
            assessed_frontier: vec![EventCid::from_bytes([0x22; 32])],
            basis: CoverageBasis::ProbabilisticSummary {
                false_positive_ppm: 1_000,
            },
            status: CoverageStatus::CompleteWithinSelector,
            returned_records: 0,
            returned_bytes: 0,
            continuation: None,
            limitations: Vec::new(),
        };
        assert_eq!(
            probabilistic.validate().unwrap_err(),
            InventoryError::InvalidCompletionClaim
        );
        let mut limited = probabilistic;
        limited.basis = CoverageBasis::ExactInventory;
        limited.limitations = vec![CoverageLimitation::PathLimited];
        assert_eq!(
            limited.validate().unwrap_err(),
            InventoryError::InvalidCompletionClaim
        );
    }

    #[test]
    fn zero_results_can_only_be_complete_within_the_named_selector() {
        let statement = CoverageStatement {
            selector: public_knowledge_exchange_fixture_v1().cid().unwrap(),
            assessed_frontier: vec![EventCid::from_bytes([0x22; 32])],
            basis: CoverageBasis::ExactInventory,
            status: CoverageStatus::CompleteWithinSelector,
            returned_records: 0,
            returned_bytes: 0,
            continuation: None,
            limitations: Vec::new(),
        };
        statement.validate().unwrap();
        assert!(statement.is_complete_within_selector());
        assert!(!statement.is_globally_complete());
    }

    #[test]
    fn fixture_has_a_frozen_selector_cid() {
        let fixture = public_knowledge_exchange_fixture_v1();
        let bytes = fixture.canonical_bytes().unwrap();
        let cid = fixture.cid().unwrap();
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../test-vectors/vnext/inventory/public-knowledge-exchange-v1.json"
        ))
        .unwrap();
        assert_eq!(vector["canonical_hex"].as_str().unwrap(), hex(&bytes));
        assert_eq!(vector["selector_cid"].as_str().unwrap(), cid.to_string());
        assert_eq!(
            cid,
            SelectorCid::from_bytes([
                0x61, 0x8c, 0x0d, 0xbb, 0x8c, 0xa6, 0x87, 0x66, 0xaf, 0xc0, 0x49, 0x0b, 0xb4, 0x24,
                0x42, 0x99, 0x14, 0xff, 0x0e, 0xa1, 0x97, 0x27, 0x6c, 0xe6, 0x99, 0x09, 0xd5, 0x3a,
                0xc5, 0x6e, 0x2e, 0x87,
            ])
        );
    }

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push(DIGITS[(byte >> 4) as usize] as char);
            output.push(DIGITS[(byte & 0x0f) as usize] as char);
        }
        output
    }
}
