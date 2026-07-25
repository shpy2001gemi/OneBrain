//! Deterministic in-memory and file-bundle carriers for OBP store-carry-forward.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, ResourceProfile,
    SelectorCid,
};
use onebrain_protocol::{
    decode_reconciliation_message, encode_reconciliation_message, ReconcileManifestKind,
};

use crate::vnext_reconciliation::BoundPayloadFrame;

const CARRIER_RECORD_MAJOR: u64 = 1;
const CARRIER_BUNDLE_MAJOR: u64 = 1;
const MAX_BUNDLE_RECORDS: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarrierRecord {
    ReconciliationMessage(Vec<u8>),
    BoundPayload(BoundPayloadFrame),
}

impl CarrierRecord {
    pub fn reconciliation_message(bytes: &[u8]) -> Result<Self, CarrierError> {
        let message = decode_reconciliation_message(bytes)?;
        Ok(Self::ReconciliationMessage(encode_reconciliation_message(
            &message,
        )?))
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CarrierError> {
        let body = match self {
            Self::ReconciliationMessage(bytes) => {
                let message = decode_reconciliation_message(bytes)?;
                CanonicalValue::Map(vec![
                    (0, CanonicalValue::Unsigned(1)),
                    (
                        1,
                        CanonicalValue::Bytes(encode_reconciliation_message(&message)?),
                    ),
                ])
            }
            Self::BoundPayload(frame) => CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(2)),
                (1, CanonicalValue::Bytes(frame.binding_digest.to_vec())),
                (2, CanonicalValue::Bytes(frame.selector.as_bytes().to_vec())),
                (3, CanonicalValue::Unsigned(frame.kind as u64)),
                (4, CanonicalValue::Bytes(frame.cid.to_vec())),
                (5, CanonicalValue::Bytes(frame.canonical_bytes.clone())),
            ]),
        };
        encode_canonical(
            &CanonicalValue::Map(vec![
                (0, CanonicalValue::Unsigned(CARRIER_RECORD_MAJOR)),
                (1, body),
            ]),
            ResourceProfile::ObjectV1,
        )
        .map_err(Into::into)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CarrierError> {
        let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
        let root = map(&value, "carrier.record")?;
        if unsigned(root, 0, "carrier.major")? != CARRIER_RECORD_MAJOR {
            return Err(CarrierError::UnsupportedVersion);
        }
        let body = map(required(root, 1, "carrier.body")?, "carrier.body")?;
        let record = match unsigned(body, 0, "carrier.kind")? {
            1 => Self::reconciliation_message(byte_string(body, 1, "carrier.message")?)?,
            2 => Self::BoundPayload(BoundPayloadFrame {
                binding_digest: bytes32(body, 1, "carrier.binding")?,
                selector: SelectorCid::from_bytes(bytes32(body, 2, "carrier.selector")?),
                kind: parse_manifest_kind(unsigned(body, 3, "carrier.payload_kind")?)?,
                cid: bytes32(body, 4, "carrier.cid")?,
                canonical_bytes: byte_string(body, 5, "carrier.payload")?.to_vec(),
            }),
            _ => return Err(CarrierError::InvalidField("carrier.kind")),
        };
        if record.canonical_bytes()? != bytes {
            return Err(CarrierError::NonCanonicalRecord);
        }
        Ok(record)
    }

    pub fn digest(&self) -> Result<[u8; 32], CarrierError> {
        let bytes = self.canonical_bytes()?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:carrier-record:1\0");
        hasher.update(&bytes);
        Ok(*hasher.finalize().as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryOrder {
    Canonical,
    ReverseCanonical,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryInjection {
    pub order: DeliveryOrder,
    pub copies_per_record: u64,
    /// Ordinals refer to the canonical sorted list before duplication.
    pub dropped_ordinals: BTreeSet<usize>,
}

impl Default for DeliveryInjection {
    fn default() -> Self {
        Self {
            order: DeliveryOrder::Canonical,
            copies_per_record: 1,
            dropped_ordinals: BTreeSet::new(),
        }
    }
}

impl DeliveryInjection {
    fn validate(&self) -> Result<(), CarrierError> {
        if self.copies_per_record == 0 || self.copies_per_record > 1_000 {
            Err(CarrierError::InvalidInjection)
        } else {
            Ok(())
        }
    }
}

pub trait DeterministicCarrier {
    fn enqueue(&mut self, record: CarrierRecord) -> Result<(), CarrierError>;
    fn deliver(&self, injection: &DeliveryInjection) -> Result<Vec<CarrierRecord>, CarrierError>;
    fn record_count(&self) -> usize;
}

#[derive(Default)]
pub struct InMemoryCarrier {
    records: Vec<Vec<u8>>,
}

impl DeterministicCarrier for InMemoryCarrier {
    fn enqueue(&mut self, record: CarrierRecord) -> Result<(), CarrierError> {
        if self.records.len() >= MAX_BUNDLE_RECORDS {
            return Err(CarrierError::Limit);
        }
        self.records.push(record.canonical_bytes()?);
        Ok(())
    }

    fn deliver(&self, injection: &DeliveryInjection) -> Result<Vec<CarrierRecord>, CarrierError> {
        deliver_bytes(&self.records, injection)
    }

    fn record_count(&self) -> usize {
        self.records.len()
    }
}

pub struct FileBundleCarrier {
    path: PathBuf,
    records: Vec<Vec<u8>>,
}

impl FileBundleCarrier {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CarrierError> {
        let path = path.as_ref().to_path_buf();
        let records = if path.exists() {
            decode_bundle(&std::fs::read(&path).map_err(CarrierError::Io)?)?
        } else {
            Vec::new()
        };
        Ok(Self { path, records })
    }

    fn persist(&self) -> Result<(), CarrierError> {
        let bytes = encode_bundle(&self.records)?;
        let temporary = self.path.with_extension("onebrain.tmp");
        std::fs::write(&temporary, bytes).map_err(CarrierError::Io)?;
        std::fs::rename(&temporary, &self.path).map_err(CarrierError::Io)
    }
}

impl DeterministicCarrier for FileBundleCarrier {
    fn enqueue(&mut self, record: CarrierRecord) -> Result<(), CarrierError> {
        if self.records.len() >= MAX_BUNDLE_RECORDS {
            return Err(CarrierError::Limit);
        }
        let mut next = self.records.clone();
        next.push(record.canonical_bytes()?);
        let previous = std::mem::replace(&mut self.records, next);
        if let Err(error) = self.persist() {
            self.records = previous;
            return Err(error);
        }
        Ok(())
    }

    fn deliver(&self, injection: &DeliveryInjection) -> Result<Vec<CarrierRecord>, CarrierError> {
        deliver_bytes(&self.records, injection)
    }

    fn record_count(&self) -> usize {
        self.records.len()
    }
}

fn deliver_bytes(
    records: &[Vec<u8>],
    injection: &DeliveryInjection,
) -> Result<Vec<CarrierRecord>, CarrierError> {
    injection.validate()?;
    let mut sorted = records.to_vec();
    sorted.sort_by_key(|bytes| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onebrain:vnext:carrier-sort:1\0");
        hasher.update(bytes);
        *hasher.finalize().as_bytes()
    });
    if injection.order == DeliveryOrder::ReverseCanonical {
        sorted.reverse();
    }
    let mut output = Vec::new();
    for (ordinal, bytes) in sorted.iter().enumerate() {
        if injection.dropped_ordinals.contains(&ordinal) {
            continue;
        }
        let record = CarrierRecord::decode(bytes)?;
        for _ in 0..injection.copies_per_record {
            output.push(record.clone());
        }
    }
    Ok(output)
}

fn encode_bundle(records: &[Vec<u8>]) -> Result<Vec<u8>, CarrierError> {
    if records.len() > MAX_BUNDLE_RECORDS {
        return Err(CarrierError::Limit);
    }
    let mut canonical = records.to_vec();
    canonical.sort();
    encode_canonical(
        &CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(CARRIER_BUNDLE_MAJOR)),
            (
                1,
                CanonicalValue::Array(canonical.into_iter().map(CanonicalValue::Bytes).collect()),
            ),
        ]),
        ResourceProfile::ManifestV1,
    )
    .map_err(Into::into)
}

fn decode_bundle(bytes: &[u8]) -> Result<Vec<Vec<u8>>, CarrierError> {
    let value = decode_canonical(bytes, ResourceProfile::ManifestV1)?;
    let root = map(&value, "bundle")?;
    if unsigned(root, 0, "bundle.major")? != CARRIER_BUNDLE_MAJOR {
        return Err(CarrierError::UnsupportedVersion);
    }
    let values = array(root, 1, "bundle.records")?;
    if values.len() > MAX_BUNDLE_RECORDS {
        return Err(CarrierError::Limit);
    }
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        let CanonicalValue::Bytes(bytes) = value else {
            return Err(CarrierError::InvalidField("bundle.record"));
        };
        CarrierRecord::decode(bytes)?;
        records.push(bytes.clone());
    }
    if encode_bundle(&records)? != bytes {
        return Err(CarrierError::NonCanonicalBundle);
    }
    Ok(records)
}

fn parse_manifest_kind(value: u64) -> Result<ReconcileManifestKind, CarrierError> {
    match value {
        1 => Ok(ReconcileManifestKind::Object),
        2 => Ok(ReconcileManifestKind::Event),
        3 => Ok(ReconcileManifestKind::MappingKernel),
        4 => Ok(ReconcileManifestKind::FeedInception),
        5 => Ok(ReconcileManifestKind::AuthorityEvent),
        _ => Err(CarrierError::InvalidField("carrier.payload_kind")),
    }
}

fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], CarrierError> {
    match value {
        CanonicalValue::Map(value) => Ok(value),
        _ => Err(CarrierError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, CarrierError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(CarrierError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, CarrierError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(CarrierError::InvalidField(field)),
    }
}

fn byte_string<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], CarrierError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(CarrierError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], CarrierError> {
    let bytes = byte_string(map, key, field)?;
    if bytes.len() != 32 {
        return Err(CarrierError::InvalidField(field));
    }
    let mut result = [0; 32];
    result.copy_from_slice(bytes);
    Ok(result)
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], CarrierError> {
    match required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(CarrierError::InvalidField(field)),
    }
}

#[derive(Debug)]
pub enum CarrierError {
    Canonical(CanonicalError),
    Protocol(onebrain_protocol::ReconciliationCodecError),
    Io(std::io::Error),
    InvalidField(&'static str),
    UnsupportedVersion,
    NonCanonicalRecord,
    NonCanonicalBundle,
    InvalidInjection,
    Limit,
}

impl From<CanonicalError> for CarrierError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<onebrain_protocol::ReconciliationCodecError> for CarrierError {
    fn from(error: onebrain_protocol::ReconciliationCodecError) -> Self {
        Self::Protocol(error)
    }
}

#[cfg(test)]
mod tests {
    use ku_core::foundation::{DisclosureClass, NamespaceCommitment};
    use onebrain_protocol::{
        bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
        ReconciliationBody, ReconciliationBudget, ReconciliationContext, ReconciliationResumeMode,
        ReconciliationSummaryMethod,
    };

    use super::*;

    fn context() -> ReconciliationContext {
        ReconciliationContext {
            authenticated_transcript: [1; 32],
            selector: SelectorCid::from_bytes([2; 32]),
            namespace: NamespaceCommitment::from_bytes([3; 32]),
            disclosure: DisclosureClass::Public,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 32,
                max_diff_ranges: 32,
                max_manifest_entries: 32,
                max_payload_bytes: 4096,
            },
            resume_mode: ReconciliationResumeMode::BoundTokenV1,
        }
    }

    fn records() -> Vec<CarrierRecord> {
        let payload = BoundPayloadFrame::new(
            &context(),
            ReconcileManifestKind::Object,
            b"bundle-object".to_vec(),
        )
        .unwrap();
        let message = bind_reconciliation_message(
            context(),
            1,
            ReconciliationBody::Manifest {
                entries: vec![ReconcileManifestEntry {
                    kind: payload.kind,
                    cid: payload.cid,
                    canonical_length: payload.canonical_bytes.len() as u64,
                }],
            },
        )
        .unwrap();
        vec![
            CarrierRecord::reconciliation_message(
                &encode_reconciliation_message(&message).unwrap(),
            )
            .unwrap(),
            CarrierRecord::BoundPayload(payload),
        ]
    }

    fn digests(records: &[CarrierRecord]) -> Vec<[u8; 32]> {
        records
            .iter()
            .map(|record| record.digest().unwrap())
            .collect()
    }

    #[test]
    fn memory_and_file_bundle_deliver_the_same_canonical_records_after_reopen() {
        let records = records();
        let mut memory = InMemoryCarrier::default();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("carry.obp");
        let mut file = FileBundleCarrier::open(&path).unwrap();
        for record in records {
            memory.enqueue(record.clone()).unwrap();
            file.enqueue(record).unwrap();
        }
        drop(file);
        let reopened = FileBundleCarrier::open(&path).unwrap();
        let plan = DeliveryInjection::default();
        assert_eq!(
            digests(&memory.deliver(&plan).unwrap()),
            digests(&reopened.deliver(&plan).unwrap())
        );
        assert_eq!(reopened.record_count(), 2);
    }

    #[test]
    fn duplicate_reverse_and_drop_injection_are_exact_and_repeatable() {
        let mut carrier = InMemoryCarrier::default();
        for record in records() {
            carrier.enqueue(record).unwrap();
        }
        let plan = DeliveryInjection {
            order: DeliveryOrder::ReverseCanonical,
            copies_per_record: 3,
            dropped_ordinals: BTreeSet::from([1]),
        };
        let first = carrier.deliver(&plan).unwrap();
        let second = carrier.deliver(&plan).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
        assert!(first.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn malformed_or_noncanonical_bundle_is_rejected_without_partial_delivery() {
        let malformed = vec![0xff; 32];
        assert!(CarrierRecord::decode(&malformed).is_err());
        assert!(decode_bundle(&malformed).is_err());
    }
}
