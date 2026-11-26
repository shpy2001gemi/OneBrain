//! P1 Concept Lifecycle Table (`concepts.lifecycle.cbor`, `P1-KU-LIFECYCLE-001` §2.1–2.2).
//!
//! Represents upstream ontology merges/redirects, concept splits, and retirements
//! without mutating historical `SemanticFrameSet` or `CoreDna` bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::foundation::canonical::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, NormalizedText,
    ResourceProfile,
};
use crate::foundation::semantic::ConceptCcid;

pub const CONCEPT_LIFECYCLE_PROFILE_MAJOR: u64 = 1;
pub const CONCEPT_LIFECYCLE_PROFILE_MINOR: u64 = 0;
pub const MAX_REDIRECT_HOPS: usize = 8;
pub const MIN_SPLIT_BRANCHES: usize = 2;
pub const MAX_SPLIT_BRANCHES: usize = 16;
pub const MAX_PROVENANCE_REF_BYTES: usize = 512;

/// 1-to-1 upstream concept merge or deprecation redirect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptRedirect {
    pub deprecated_ccid: ConceptCcid,
    pub canonical_ccid: ConceptCcid,
    pub since_release_seq: u64,
    pub provenance_ref: String,
}

/// 1-to-many upstream concept split (`2..=16` unique branches).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptSplit {
    pub parent_ccid: ConceptCcid,
    pub branch_ccids: Vec<ConceptCcid>,
    pub disambiguation_role_ccid: ConceptCcid,
    pub since_release_seq: u64,
}

/// Categorical reason why a concept was retired upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum ConceptRetireReason {
    RetractedUpstream = 0,
    InvalidArtifact = 1,
    DuplicateSuperseded = 2,
    PolicyObsoleted = 3,
}

impl ConceptRetireReason {
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::RetractedUpstream),
            1 => Some(Self::InvalidArtifact),
            2 => Some(Self::DuplicateSuperseded),
            3 => Some(Self::PolicyObsoleted),
            _ => None,
        }
    }
}

/// Upstream concept retirement record. Historical KUs remain readable with an
/// explicit `[retired_concept]` annotation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptRetire {
    pub retired_ccid: ConceptCcid,
    pub reason_code: ConceptRetireReason,
    pub since_release_seq: u64,
}

/// Append-only lifecycle record keyed by its subject `ConceptCcid`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConceptLifecycleRecord {
    Redirect(ConceptRedirect),
    Split(ConceptSplit),
    Retire(ConceptRetire),
}

impl ConceptLifecycleRecord {
    pub const fn subject_ccid(&self) -> ConceptCcid {
        match self {
            Self::Redirect(record) => record.deprecated_ccid,
            Self::Split(record) => record.parent_ccid,
            Self::Retire(record) => record.retired_ccid,
        }
    }

    pub const fn since_release_seq(&self) -> u64 {
        match self {
            Self::Redirect(record) => record.since_release_seq,
            Self::Split(record) => record.since_release_seq,
            Self::Retire(record) => record.since_release_seq,
        }
    }

    fn to_canonical_value(&self) -> Result<CanonicalValue, ConceptLifecycleError> {
        match self {
            Self::Redirect(record) => {
                if record.provenance_ref.trim().is_empty()
                    || record.provenance_ref.len() > MAX_PROVENANCE_REF_BYTES
                {
                    return Err(ConceptLifecycleError::InvalidField(
                        "redirect.provenance_ref",
                    ));
                }
                let text = NormalizedText::new(record.provenance_ref.clone())?;
                Ok(CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(0)),
                    (
                        1,
                        CanonicalValue::Bytes(record.deprecated_ccid.as_bytes().to_vec()),
                    ),
                    (
                        2,
                        CanonicalValue::Bytes(record.canonical_ccid.as_bytes().to_vec()),
                    ),
                    (3, CanonicalValue::Unsigned(record.since_release_seq)),
                    (4, CanonicalValue::Text(text.into_string())),
                ]))
            }
            Self::Split(record) => {
                let branches = record
                    .branch_ccids
                    .iter()
                    .map(|ccid| CanonicalValue::Bytes(ccid.as_bytes().to_vec()))
                    .collect();
                Ok(CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(1)),
                    (
                        1,
                        CanonicalValue::Bytes(record.parent_ccid.as_bytes().to_vec()),
                    ),
                    (2, CanonicalValue::Array(branches)),
                    (
                        3,
                        CanonicalValue::Bytes(record.disambiguation_role_ccid.as_bytes().to_vec()),
                    ),
                    (4, CanonicalValue::Unsigned(record.since_release_seq)),
                ]))
            }
            Self::Retire(record) => Ok(CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (
                    1,
                    CanonicalValue::Bytes(record.retired_ccid.as_bytes().to_vec()),
                ),
                (2, CanonicalValue::Unsigned(record.reason_code as u64)),
                (3, CanonicalValue::Unsigned(record.since_release_seq)),
            ])),
        }
    }

    fn from_canonical_value(value: &CanonicalValue) -> Result<Self, ConceptLifecycleError> {
        let map = expect_map(value, "lifecycle.record")?;
        let kind = expect_unsigned(
            expect_field(map, 0, "lifecycle.record.kind")?,
            "lifecycle.record.kind",
        )?;
        match kind {
            0 => {
                if map.len() != 5 {
                    return Err(ConceptLifecycleError::InvalidField("redirect.fields"));
                }
                let deprecated_ccid = expect_ccid(
                    expect_field(map, 1, "redirect.deprecated_ccid")?,
                    "redirect.deprecated_ccid",
                )?;
                let canonical_ccid = expect_ccid(
                    expect_field(map, 2, "redirect.canonical_ccid")?,
                    "redirect.canonical_ccid",
                )?;
                let since_release_seq = expect_unsigned(
                    expect_field(map, 3, "redirect.since_release_seq")?,
                    "redirect.since_release_seq",
                )?;
                let provenance_ref = expect_text(
                    expect_field(map, 4, "redirect.provenance_ref")?,
                    "redirect.provenance_ref",
                )?;
                Ok(Self::Redirect(ConceptRedirect {
                    deprecated_ccid,
                    canonical_ccid,
                    since_release_seq,
                    provenance_ref,
                }))
            }
            1 => {
                if map.len() != 5 {
                    return Err(ConceptLifecycleError::InvalidField("split.fields"));
                }
                let parent_ccid = expect_ccid(
                    expect_field(map, 1, "split.parent_ccid")?,
                    "split.parent_ccid",
                )?;
                let branch_values = expect_array(
                    expect_field(map, 2, "split.branch_ccids")?,
                    "split.branch_ccids",
                )?;
                let branch_ccids = branch_values
                    .iter()
                    .map(|item| expect_ccid(item, "split.branch_ccid"))
                    .collect::<Result<Vec<_>, _>>()?;
                let disambiguation_role_ccid = expect_ccid(
                    expect_field(map, 3, "split.disambiguation_role_ccid")?,
                    "split.disambiguation_role_ccid",
                )?;
                let since_release_seq = expect_unsigned(
                    expect_field(map, 4, "split.since_release_seq")?,
                    "split.since_release_seq",
                )?;
                Ok(Self::Split(ConceptSplit {
                    parent_ccid,
                    branch_ccids,
                    disambiguation_role_ccid,
                    since_release_seq,
                }))
            }
            2 => {
                if map.len() != 4 {
                    return Err(ConceptLifecycleError::InvalidField("retire.fields"));
                }
                let retired_ccid = expect_ccid(
                    expect_field(map, 1, "retire.retired_ccid")?,
                    "retire.retired_ccid",
                )?;
                let reason_raw = expect_unsigned(
                    expect_field(map, 2, "retire.reason_code")?,
                    "retire.reason_code",
                )?;
                let reason_code = ConceptRetireReason::from_code(reason_raw)
                    .ok_or(ConceptLifecycleError::InvalidField("retire.reason_code"))?;
                let since_release_seq = expect_unsigned(
                    expect_field(map, 3, "retire.since_release_seq")?,
                    "retire.since_release_seq",
                )?;
                Ok(Self::Retire(ConceptRetire {
                    retired_ccid,
                    reason_code,
                    since_release_seq,
                }))
            }
            _ => Err(ConceptLifecycleError::InvalidField("lifecycle.record.kind")),
        }
    }
}

/// Authoring-time resolution outcome enforcing compiler/index asymmetry (§2.1.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoringConceptResolution {
    Canonical(ConceptCcid),
    Redirected {
        from: ConceptCcid,
        to: ConceptCcid,
    },
    NeedsSplitDisambiguation {
        parent: ConceptCcid,
        branches: Vec<ConceptCcid>,
        role: ConceptCcid,
    },
    Retired {
        ccid: ConceptCcid,
        reason: ConceptRetireReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConceptLifecycleError {
    Canonical(CanonicalError),
    UnsupportedVersion {
        major: u64,
        minor: u64,
    },
    InvalidField(&'static str),
    DuplicateSubject(ConceptCcid),
    SelfRedirect(ConceptCcid),
    RedirectCycle(ConceptCcid),
    RedirectDepthExceeded {
        start: ConceptCcid,
        max_hops: usize,
    },
    InvalidSplitBranches {
        parent: ConceptCcid,
        count: usize,
    },
    DuplicateSplitBranch {
        parent: ConceptCcid,
        branch: ConceptCcid,
    },
    SplitContainsParent(ConceptCcid),
    NonCanonicalEncoding,
}

impl fmt::Display for ConceptLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Canonical(error) => write!(formatter, "lifecycle canonical error: {error}"),
            Self::UnsupportedVersion { major, minor } => {
                write!(formatter, "unsupported lifecycle version: {major}.{minor}")
            }
            Self::InvalidField(field) => write!(formatter, "invalid lifecycle field: {field}"),
            Self::DuplicateSubject(ccid) => {
                write!(formatter, "duplicate lifecycle subject CCID: {ccid}")
            }
            Self::SelfRedirect(ccid) => write!(formatter, "self-redirect on CCID: {ccid}"),
            Self::RedirectCycle(ccid) => {
                write!(formatter, "redirect cycle detected starting from CCID: {ccid}")
            }
            Self::RedirectDepthExceeded { start, max_hops } => write!(
                formatter,
                "redirect chain from {start} exceeds max hops ({max_hops})"
            ),
            Self::InvalidSplitBranches { parent, count } => write!(
                formatter,
                "split on {parent} has invalid branch count {count} (expected {MIN_SPLIT_BRANCHES}..={MAX_SPLIT_BRANCHES})"
            ),
            Self::DuplicateSplitBranch { parent, branch } => write!(
                formatter,
                "split on {parent} contains duplicate branch {branch}"
            ),
            Self::SplitContainsParent(parent) => {
                write!(formatter, "split on {parent} lists parent among branches")
            }
            Self::NonCanonicalEncoding => {
                formatter.write_str("lifecycle table CBOR encoding is non-canonical")
            }
        }
    }
}

impl std::error::Error for ConceptLifecycleError {}

impl From<CanonicalError> for ConceptLifecycleError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

/// Validated, acyclic `ConceptLifecycleTable` (`ResourceProfile::ControlV1`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConceptLifecycleTable {
    records: BTreeMap<ConceptCcid, ConceptLifecycleRecord>,
}

impl ConceptLifecycleTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_records(
        records: impl IntoIterator<Item = ConceptLifecycleRecord>,
    ) -> Result<Self, ConceptLifecycleError> {
        let mut map = BTreeMap::new();
        for record in records {
            validate_single_record(&record)?;
            let subject = record.subject_ccid();
            if map.insert(subject, record).is_some() {
                return Err(ConceptLifecycleError::DuplicateSubject(subject));
            }
        }
        let table = Self { records: map };
        table.validate_redirect_dag()?;
        Ok(table)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn get(&self, ccid: &ConceptCcid) -> Option<&ConceptLifecycleRecord> {
        self.records.get(ccid)
    }

    pub fn records(&self) -> impl Iterator<Item = &ConceptLifecycleRecord> {
        self.records.values()
    }

    /// Resolve a `ConceptCcid` for new KU authoring / compilation (`P1` §2.1.3).
    pub fn resolve_for_authoring(&self, ccid: ConceptCcid) -> AuthoringConceptResolution {
        let mut current = ccid;
        let mut hops = 0usize;
        while let Some(record) = self.records.get(&current) {
            match record {
                ConceptLifecycleRecord::Redirect(redirect) => {
                    current = redirect.canonical_ccid;
                    hops += 1;
                    if hops > MAX_REDIRECT_HOPS {
                        break;
                    }
                }
                ConceptLifecycleRecord::Split(split) => {
                    return AuthoringConceptResolution::NeedsSplitDisambiguation {
                        parent: split.parent_ccid,
                        branches: split.branch_ccids.clone(),
                        role: split.disambiguation_role_ccid,
                    };
                }
                ConceptLifecycleRecord::Retire(retire) => {
                    return AuthoringConceptResolution::Retired {
                        ccid: retire.retired_ccid,
                        reason: retire.reason_code,
                    };
                }
            }
        }
        if hops > 0 {
            AuthoringConceptResolution::Redirected {
                from: ccid,
                to: current,
            }
        } else {
            AuthoringConceptResolution::Canonical(ccid)
        }
    }

    /// Compute redirect equivalence classes for `P3` semantic index and `P7` OBKG
    /// projection expansion (`canonical_ccid -> {canonical_ccid, deprecated_ccids...}`).
    ///
    /// Concept splits and retirements are intentionally excluded per `P1` §2.1.2.
    pub fn equivalence_classes(&self) -> BTreeMap<ConceptCcid, BTreeSet<ConceptCcid>> {
        let mut classes: BTreeMap<ConceptCcid, BTreeSet<ConceptCcid>> = BTreeMap::new();
        for record in self.records.values() {
            if let ConceptLifecycleRecord::Redirect(redirect) = record {
                if let AuthoringConceptResolution::Redirected { from, to } =
                    self.resolve_for_authoring(redirect.deprecated_ccid)
                {
                    let members = classes.entry(to).or_default();
                    members.insert(to);
                    members.insert(from);
                }
            }
        }
        classes
    }

    pub fn to_canonical_value(&self) -> Result<CanonicalValue, ConceptLifecycleError> {
        let encoded_records = self
            .records
            .values()
            .map(ConceptLifecycleRecord::to_canonical_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(CONCEPT_LIFECYCLE_PROFILE_MAJOR)),
            (1, CanonicalValue::Unsigned(CONCEPT_LIFECYCLE_PROFILE_MINOR)),
            (2, CanonicalValue::Array(encoded_records)),
        ]))
    }

    pub fn from_canonical_value(value: &CanonicalValue) -> Result<Self, ConceptLifecycleError> {
        let map = expect_map(value, "lifecycle.table")?;
        if map.len() != 3 {
            return Err(ConceptLifecycleError::InvalidField(
                "lifecycle.table.fields",
            ));
        }
        let major = expect_unsigned(expect_field(map, 0, "lifecycle.major")?, "lifecycle.major")?;
        let minor = expect_unsigned(expect_field(map, 1, "lifecycle.minor")?, "lifecycle.minor")?;
        if major != CONCEPT_LIFECYCLE_PROFILE_MAJOR || minor != CONCEPT_LIFECYCLE_PROFILE_MINOR {
            return Err(ConceptLifecycleError::UnsupportedVersion { major, minor });
        }
        let raw_records = expect_array(
            expect_field(map, 2, "lifecycle.records")?,
            "lifecycle.records",
        )?;
        let mut parsed = Vec::with_capacity(raw_records.len());
        let mut last_subject: Option<ConceptCcid> = None;
        for raw in raw_records {
            let record = ConceptLifecycleRecord::from_canonical_value(raw)?;
            let subject = record.subject_ccid();
            if let Some(prev) = last_subject {
                if subject == prev {
                    return Err(ConceptLifecycleError::DuplicateSubject(subject));
                }
                if subject < prev {
                    return Err(ConceptLifecycleError::NonCanonicalEncoding);
                }
            }
            last_subject = Some(subject);
            parsed.push(record);
        }
        let table = Self::from_records(parsed)?;
        if table.to_canonical_value()? != *value {
            return Err(ConceptLifecycleError::NonCanonicalEncoding);
        }
        Ok(table)
    }

    pub fn encode_bytes(&self) -> Result<Vec<u8>, ConceptLifecycleError> {
        let value = self.to_canonical_value()?;
        encode_canonical(&value, ResourceProfile::ControlV1).map_err(Into::into)
    }

    pub fn decode_bytes(bytes: &[u8]) -> Result<Self, ConceptLifecycleError> {
        let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
        let table = Self::from_canonical_value(&value)?;
        if table.encode_bytes()? != bytes {
            return Err(ConceptLifecycleError::NonCanonicalEncoding);
        }
        Ok(table)
    }

    fn validate_redirect_dag(&self) -> Result<(), ConceptLifecycleError> {
        for (&start, record) in &self.records {
            let ConceptLifecycleRecord::Redirect(first) = record else {
                continue;
            };
            let mut visited = BTreeSet::new();
            visited.insert(start);
            let mut current = first.canonical_ccid;
            let mut hops = 1usize;
            loop {
                if !visited.insert(current) {
                    return Err(ConceptLifecycleError::RedirectCycle(start));
                }
                if hops > MAX_REDIRECT_HOPS {
                    return Err(ConceptLifecycleError::RedirectDepthExceeded {
                        start,
                        max_hops: MAX_REDIRECT_HOPS,
                    });
                }
                match self.records.get(&current) {
                    Some(ConceptLifecycleRecord::Redirect(next)) => {
                        current = next.canonical_ccid;
                        hops += 1;
                    }
                    Some(ConceptLifecycleRecord::Split(_))
                    | Some(ConceptLifecycleRecord::Retire(_))
                    | None => break,
                }
            }
        }
        Ok(())
    }
}

fn validate_single_record(record: &ConceptLifecycleRecord) -> Result<(), ConceptLifecycleError> {
    match record {
        ConceptLifecycleRecord::Redirect(redirect) => {
            if redirect.deprecated_ccid == redirect.canonical_ccid {
                return Err(ConceptLifecycleError::SelfRedirect(
                    redirect.deprecated_ccid,
                ));
            }
            if redirect.provenance_ref.trim().is_empty()
                || redirect.provenance_ref.len() > MAX_PROVENANCE_REF_BYTES
            {
                return Err(ConceptLifecycleError::InvalidField(
                    "redirect.provenance_ref",
                ));
            }
            NormalizedText::new(redirect.provenance_ref.clone())?;
            Ok(())
        }
        ConceptLifecycleRecord::Split(split) => {
            let count = split.branch_ccids.len();
            if !(MIN_SPLIT_BRANCHES..=MAX_SPLIT_BRANCHES).contains(&count) {
                return Err(ConceptLifecycleError::InvalidSplitBranches {
                    parent: split.parent_ccid,
                    count,
                });
            }
            let mut seen = BTreeSet::new();
            for &branch in &split.branch_ccids {
                if branch == split.parent_ccid {
                    return Err(ConceptLifecycleError::SplitContainsParent(
                        split.parent_ccid,
                    ));
                }
                if !seen.insert(branch) {
                    return Err(ConceptLifecycleError::DuplicateSplitBranch {
                        parent: split.parent_ccid,
                        branch,
                    });
                }
            }
            Ok(())
        }
        ConceptLifecycleRecord::Retire(_) => Ok(()),
    }
}

fn expect_map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ConceptLifecycleError> {
    match value {
        CanonicalValue::Map(entries) => Ok(entries),
        _ => Err(ConceptLifecycleError::InvalidField(field)),
    }
}

fn expect_array<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ConceptLifecycleError> {
    match value {
        CanonicalValue::Array(items) => Ok(items),
        _ => Err(ConceptLifecycleError::InvalidField(field)),
    }
}

fn expect_field<'a>(
    entries: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ConceptLifecycleError> {
    entries
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ConceptLifecycleError::InvalidField(field))
}

fn expect_unsigned(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<u64, ConceptLifecycleError> {
    match value {
        CanonicalValue::Unsigned(number) => Ok(*number),
        _ => Err(ConceptLifecycleError::InvalidField(field)),
    }
}

fn expect_text(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<String, ConceptLifecycleError> {
    match value {
        CanonicalValue::Text(text) => {
            NormalizedText::new(text.clone())?;
            Ok(text.clone())
        }
        _ => Err(ConceptLifecycleError::InvalidField(field)),
    }
}

fn expect_ccid(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<ConceptCcid, ConceptLifecycleError> {
    match value {
        CanonicalValue::Bytes(bytes) => {
            let array: [u8; 16] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| ConceptLifecycleError::InvalidField(field))?;
            Ok(ConceptCcid::from_bytes(array))
        }
        _ => Err(ConceptLifecycleError::InvalidField(field)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ccid(byte: u8) -> ConceptCcid {
        ConceptCcid::from_bytes([byte; 16])
    }

    #[test]
    fn canonical_cbor_roundtrip_is_insertion_order_independent() {
        let r1 = ConceptLifecycleRecord::Redirect(ConceptRedirect {
            deprecated_ccid: ccid(30),
            canonical_ccid: ccid(31),
            since_release_seq: 2,
            provenance_ref: "wd:Q100->wd:Q200".to_string(),
        });
        let r2 = ConceptLifecycleRecord::Split(ConceptSplit {
            parent_ccid: ccid(10),
            branch_ccids: vec![ccid(11), ccid(12), ccid(13)],
            disambiguation_role_ccid: ccid(99),
            since_release_seq: 3,
        });
        let r3 = ConceptLifecycleRecord::Retire(ConceptRetire {
            retired_ccid: ccid(20),
            reason_code: ConceptRetireReason::RetractedUpstream,
            since_release_seq: 4,
        });

        let forward =
            ConceptLifecycleTable::from_records(vec![r1.clone(), r2.clone(), r3.clone()]).unwrap();
        let reverse = ConceptLifecycleTable::from_records(vec![r3, r2, r1]).unwrap();
        let bytes_forward = forward.encode_bytes().unwrap();
        let bytes_reverse = reverse.encode_bytes().unwrap();
        assert_eq!(bytes_forward, bytes_reverse);

        let decoded = ConceptLifecycleTable::decode_bytes(&bytes_forward).unwrap();
        assert_eq!(decoded, forward);
    }

    #[test]
    fn transitive_redirects_resolve_up_to_8_hops_and_reject_9_hops_or_cycles() {
        let mut records = Vec::new();
        for hop in 1..=8u8 {
            records.push(ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: ccid(hop),
                canonical_ccid: ccid(hop + 1),
                since_release_seq: u64::from(hop),
                provenance_ref: format!("hop-{hop}"),
            }));
        }
        let table = ConceptLifecycleTable::from_records(records.clone()).unwrap();
        assert_eq!(
            table.resolve_for_authoring(ccid(1)),
            AuthoringConceptResolution::Redirected {
                from: ccid(1),
                to: ccid(9)
            }
        );
        assert_eq!(
            table.resolve_for_authoring(ccid(9)),
            AuthoringConceptResolution::Canonical(ccid(9))
        );

        let classes = table.equivalence_classes();
        let expected: BTreeSet<_> = (1..=9u8).map(ccid).collect();
        assert_eq!(classes.get(&ccid(9)), Some(&expected));

        // 9th hop exceeds MAX_REDIRECT_HOPS = 8.
        let mut nine_hops = records;
        nine_hops.push(ConceptLifecycleRecord::Redirect(ConceptRedirect {
            deprecated_ccid: ccid(9),
            canonical_ccid: ccid(10),
            since_release_seq: 9,
            provenance_ref: "hop-9".to_string(),
        }));
        assert_eq!(
            ConceptLifecycleTable::from_records(nine_hops).unwrap_err(),
            ConceptLifecycleError::RedirectDepthExceeded {
                start: ccid(1),
                max_hops: MAX_REDIRECT_HOPS,
            }
        );

        // Self-redirect and 3-node cycle are rejected fail-closed.
        assert_eq!(
            ConceptLifecycleTable::from_records(vec![ConceptLifecycleRecord::Redirect(
                ConceptRedirect {
                    deprecated_ccid: ccid(5),
                    canonical_ccid: ccid(5),
                    since_release_seq: 1,
                    provenance_ref: "self".to_string(),
                }
            )])
            .unwrap_err(),
            ConceptLifecycleError::SelfRedirect(ccid(5))
        );

        let cyclic = vec![
            ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: ccid(1),
                canonical_ccid: ccid(2),
                since_release_seq: 1,
                provenance_ref: "a->b".to_string(),
            }),
            ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: ccid(2),
                canonical_ccid: ccid(3),
                since_release_seq: 2,
                provenance_ref: "b->c".to_string(),
            }),
            ConceptLifecycleRecord::Redirect(ConceptRedirect {
                deprecated_ccid: ccid(3),
                canonical_ccid: ccid(1),
                since_release_seq: 3,
                provenance_ref: "c->a".to_string(),
            }),
        ];
        assert_eq!(
            ConceptLifecycleTable::from_records(cyclic).unwrap_err(),
            ConceptLifecycleError::RedirectCycle(ccid(1))
        );
    }

    #[test]
    fn split_and_retire_authoring_resolutions_and_validation_bounds() {
        let split = ConceptLifecycleRecord::Split(ConceptSplit {
            parent_ccid: ccid(40),
            branch_ccids: vec![ccid(41), ccid(42)],
            disambiguation_role_ccid: ccid(50),
            since_release_seq: 5,
        });
        let retire = ConceptLifecycleRecord::Retire(ConceptRetire {
            retired_ccid: ccid(60),
            reason_code: ConceptRetireReason::PolicyObsoleted,
            since_release_seq: 6,
        });
        let table = ConceptLifecycleTable::from_records(vec![split, retire]).unwrap();
        assert_eq!(
            table.resolve_for_authoring(ccid(40)),
            AuthoringConceptResolution::NeedsSplitDisambiguation {
                parent: ccid(40),
                branches: vec![ccid(41), ccid(42)],
                role: ccid(50),
            }
        );
        assert_eq!(
            table.resolve_for_authoring(ccid(60)),
            AuthoringConceptResolution::Retired {
                ccid: ccid(60),
                reason: ConceptRetireReason::PolicyObsoleted,
            }
        );
        assert!(table.equivalence_classes().is_empty());

        // Fewer than 2 branches or > 16 branches fail closed.
        assert!(matches!(
            ConceptLifecycleTable::from_records(vec![ConceptLifecycleRecord::Split(
                ConceptSplit {
                    parent_ccid: ccid(1),
                    branch_ccids: vec![ccid(2)],
                    disambiguation_role_ccid: ccid(3),
                    since_release_seq: 1,
                }
            )]),
            Err(ConceptLifecycleError::InvalidSplitBranches { count: 1, .. })
        ));
        let seventeen: Vec<_> = (2..=18u8).map(ccid).collect();
        assert!(matches!(
            ConceptLifecycleTable::from_records(vec![ConceptLifecycleRecord::Split(
                ConceptSplit {
                    parent_ccid: ccid(1),
                    branch_ccids: seventeen,
                    disambiguation_role_ccid: ccid(99),
                    since_release_seq: 1,
                }
            )]),
            Err(ConceptLifecycleError::InvalidSplitBranches { count: 17, .. })
        ));
    }
}
