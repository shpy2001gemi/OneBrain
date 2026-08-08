//! Exact, public-only Base exchange. This is not an encrypted archive and it
//! never carries Vault, identity, receipt, signer, or projection state.

use std::collections::BTreeSet;
use std::io::{Read, Write};

use ku_core::foundation::schema_registry::{EVENT_TYPES_V1, OBJECT_KINDS_V1};
use ku_core::foundation::{
    authority_event_descriptor, decode_actor_delegation, decode_actor_revocation,
    decode_actor_root_delegation, decode_feed_inception, decode_knowledge_event,
    decode_knowledge_object, event_author_feed, AuthorityEventDescriptor, DisclosureClass,
    EventType, KnownObjectKind, ObjectKind, ReservedDomain, ResourceProfile, StoredRecordKind,
    ValidatedFeedInception,
};
use ku_core::KuRuntime;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CANONICAL_EXCHANGE_MAGIC: &[u8] = b"OBXV1\n";
pub const MAX_EXCHANGE_RECORDS: usize = 65_536;
pub const MAX_EXCHANGE_RECORD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EXCHANGE_TOTAL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseExchangeEntryV1 {
    VNextPublic {
        kind: StoredRecordKind,
        cid: [u8; 32],
        canonical_bytes: Vec<u8>,
    },
    LegacyReadOnlyEvidence {
        cid: [u8; 32],
        wire_bytes: Vec<u8>,
        epigenetics_json: Vec<u8>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExchangeReceipt {
    pub count: u64,
    pub byte_length: u64,
    pub root: [u8; 32],
}

#[derive(Debug, Error)]
pub enum ExchangeError {
    #[error("exchange I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("exchange JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown exchange version or record kind")]
    UnknownVersionOrKind,
    #[error("exchange magic is invalid")]
    InvalidMagic,
    #[error("exchange field is malformed")]
    Malformed,
    #[error("exchange exceeds a Base bound")]
    Limit,
    #[error("exchange CID does not match exact bytes")]
    CidMismatch,
    #[error("exchange contains duplicate CID")]
    DuplicateCid,
    #[error("exchange records are not canonically ordered")]
    NonCanonicalOrder,
    #[error("exchange JSON is not the canonical fixed-field encoding")]
    NonCanonicalEncoding,
    #[error("exchange footer count, length, or root mismatch")]
    FooterMismatch,
    #[error("private or route-minimal data is forbidden in public exchange")]
    PrivateClass,
    #[error("canonical vNext validation failed: {0}")]
    CanonicalValidation(String),
    #[error("exchange is missing a required feed or authority dependency")]
    MissingDependency,
    #[error("trailing bytes follow the exchange footer")]
    TrailingBytes,
}

pub fn write_canonical_exchange<W: Write>(
    entries: &[BaseExchangeEntryV1],
    mut output: W,
) -> Result<ExchangeReceipt, ExchangeError> {
    if entries.len() > MAX_EXCHANGE_RECORDS {
        return Err(ExchangeError::Limit);
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(sort_key);
    validate_unique_and_ordered(&sorted)?;
    validate_exchange_entries(&sorted)?;

    let mut payload = Vec::new();
    for entry in &sorted {
        let line = encode_record(entry)?;
        if line.len() > MAX_EXCHANGE_RECORD_BYTES {
            return Err(ExchangeError::Limit);
        }
        payload.extend_from_slice(&line);
        payload.push(b'\n');
        if payload.len() > MAX_EXCHANGE_TOTAL_BYTES {
            return Err(ExchangeError::Limit);
        }
    }
    let receipt = receipt(&payload, sorted.len());
    let footer = serde_json::to_vec(&FooterEnvelope {
        footer: Footer {
            version: 1,
            count: receipt.count,
            byte_length: receipt.byte_length,
            root: encode_hex(&receipt.root),
        },
    })?;
    output.write_all(CANONICAL_EXCHANGE_MAGIC)?;
    output.write_all(&payload)?;
    output.write_all(&footer)?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(receipt)
}

pub fn read_canonical_exchange<R: Read>(
    input: R,
) -> Result<Vec<BaseExchangeEntryV1>, ExchangeError> {
    let mut bytes = Vec::new();
    input
        .take((MAX_EXCHANGE_TOTAL_BYTES + MAX_EXCHANGE_RECORD_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_EXCHANGE_TOTAL_BYTES + MAX_EXCHANGE_RECORD_BYTES {
        return Err(ExchangeError::Limit);
    }
    if !bytes.starts_with(CANONICAL_EXCHANGE_MAGIC) {
        return Err(ExchangeError::InvalidMagic);
    }
    let body = &bytes[CANONICAL_EXCHANGE_MAGIC.len()..];
    if !body.ends_with(b"\n") {
        return Err(ExchangeError::TrailingBytes);
    }
    let lines = body[..body.len() - 1]
        .split(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let (footer_line, record_lines) = lines.split_last().ok_or(ExchangeError::Malformed)?;
    if footer_line.is_empty() || record_lines.len() > MAX_EXCHANGE_RECORDS {
        return Err(ExchangeError::Limit);
    }
    let footer: FooterEnvelope = serde_json::from_slice(footer_line)?;
    if footer.footer.version != 1 {
        return Err(ExchangeError::UnknownVersionOrKind);
    }
    if serde_json::to_vec(&footer)?.as_slice() != *footer_line {
        return Err(ExchangeError::NonCanonicalEncoding);
    }

    let mut payload = Vec::new();
    let mut entries = Vec::with_capacity(record_lines.len());
    for line in record_lines {
        if line.is_empty() || line.len() > MAX_EXCHANGE_RECORD_BYTES {
            return Err(ExchangeError::Limit);
        }
        payload.extend_from_slice(line);
        payload.push(b'\n');
        if payload.len() > MAX_EXCHANGE_TOTAL_BYTES {
            return Err(ExchangeError::Limit);
        }
        let entry = decode_record(line)?;
        if encode_record(&entry)?.as_slice() != *line {
            return Err(ExchangeError::NonCanonicalEncoding);
        }
        entries.push(entry);
    }
    let actual = receipt(&payload, entries.len());
    validate_unique_and_ordered(&entries)?;
    let declared_root = decode_hex_32(&footer.footer.root)?;
    if footer.footer.count != actual.count
        || footer.footer.byte_length != actual.byte_length
        || declared_root != actual.root
    {
        return Err(ExchangeError::FooterMismatch);
    }
    validate_exchange_entries(&entries)?;
    Ok(entries)
}

fn validate_unique_and_ordered(entries: &[BaseExchangeEntryV1]) -> Result<(), ExchangeError> {
    let mut cids = BTreeSet::new();
    let mut prior = None;
    for entry in entries {
        if !cids.insert(entry_cid(entry)) {
            return Err(ExchangeError::DuplicateCid);
        }
        let key = sort_key(entry);
        if prior.is_some_and(|prior_key| prior_key >= key) {
            return Err(ExchangeError::NonCanonicalOrder);
        }
        prior = Some(key);
    }
    Ok(())
}

fn validate_exchange_entries(entries: &[BaseExchangeEntryV1]) -> Result<(), ExchangeError> {
    let known_kinds = OBJECT_KINDS_V1
        .iter()
        .map(|entry| KnownObjectKind::new(ObjectKind(entry.id), 1))
        .collect::<Vec<_>>();
    let known_events = EVENT_TYPES_V1
        .iter()
        .map(|entry| EventType(entry.id))
        .collect::<Vec<_>>();
    let mut feeds = Vec::<ValidatedFeedInception>::new();
    let authority_cids = entries
        .iter()
        .filter_map(|entry| match entry {
            BaseExchangeEntryV1::VNextPublic {
                kind: StoredRecordKind::AuthorityEvent,
                cid,
                ..
            } => Some(*cid),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    for entry in entries {
        match entry {
            BaseExchangeEntryV1::VNextPublic {
                kind: StoredRecordKind::FeedInception,
                cid,
                canonical_bytes,
            } => {
                check_domain(ReservedDomain::FeedInception, cid, canonical_bytes)?;
                feeds
                    .push(decode_feed_inception(canonical_bytes).map_err(|error| {
                        ExchangeError::CanonicalValidation(error.code().into())
                    })?);
            }
            BaseExchangeEntryV1::LegacyReadOnlyEvidence {
                cid,
                wire_bytes,
                epigenetics_json,
            } => {
                if wire_bytes.len() > MAX_EXCHANGE_RECORD_BYTES
                    || epigenetics_json.len() > MAX_EXCHANGE_RECORD_BYTES
                {
                    return Err(ExchangeError::Limit);
                }
                let ku = KuRuntime::from_wire(wire_bytes.clone())
                    .map_err(|error| ExchangeError::CanonicalValidation(error.to_string()))?;
                if &ku.cid != cid {
                    return Err(ExchangeError::CidMismatch);
                }
                let _: serde_json::Value = serde_json::from_slice(epigenetics_json)?;
            }
            _ => {}
        }
    }

    for entry in entries {
        let BaseExchangeEntryV1::VNextPublic {
            kind,
            cid,
            canonical_bytes,
        } = entry
        else {
            continue;
        };
        match kind {
            StoredRecordKind::Object => {
                check_domain(ReservedDomain::Object, cid, canonical_bytes)?;
                let object = decode_knowledge_object(
                    canonical_bytes,
                    ResourceProfile::ObjectV1,
                    &known_kinds,
                    &[],
                )
                .map_err(|error| ExchangeError::CanonicalValidation(error.code().into()))?;
                if object.disclosure() != DisclosureClass::Public {
                    return Err(ExchangeError::PrivateClass);
                }
            }
            StoredRecordKind::Event => {
                check_domain(ReservedDomain::Event, cid, canonical_bytes)?;
                let author = event_author_feed(canonical_bytes)
                    .map_err(|error| ExchangeError::CanonicalValidation(error.code().into()))?;
                let event = feeds
                    .iter()
                    .filter(|feed| feed.feed_id == author)
                    .find_map(|feed| {
                        decode_knowledge_event(canonical_bytes, feed, &known_events).ok()
                    })
                    .ok_or(ExchangeError::MissingDependency)?;
                if event.signed.event.disclosure != DisclosureClass::Public {
                    return Err(ExchangeError::PrivateClass);
                }
            }
            StoredRecordKind::FeedInception => {}
            StoredRecordKind::AuthorityEvent => {
                check_domain(ReservedDomain::AuthorityEvent, cid, canonical_bytes)?;
                match authority_event_descriptor(canonical_bytes)
                    .map_err(|error| ExchangeError::CanonicalValidation(error.code().into()))?
                {
                    AuthorityEventDescriptor::Root => {
                        decode_actor_root_delegation(canonical_bytes).map_err(|error| {
                            ExchangeError::CanonicalValidation(error.code().into())
                        })?;
                    }
                    AuthorityEventDescriptor::Delegation {
                        parent,
                        authorizing_feed,
                    } => {
                        if !authority_cids.contains(parent.as_bytes()) {
                            return Err(ExchangeError::MissingDependency);
                        }
                        if !feeds
                            .iter()
                            .filter(|feed| feed.feed_id == authorizing_feed)
                            .any(|feed| decode_actor_delegation(canonical_bytes, feed).is_ok())
                        {
                            return Err(ExchangeError::MissingDependency);
                        }
                    }
                    AuthorityEventDescriptor::Revocation {
                        target,
                        authorized_by,
                        authorizing_feed,
                    } => {
                        if !authority_cids.contains(target.as_bytes())
                            || !authority_cids.contains(authorized_by.as_bytes())
                        {
                            return Err(ExchangeError::MissingDependency);
                        }
                        if !feeds
                            .iter()
                            .filter(|feed| feed.feed_id == authorizing_feed)
                            .any(|feed| decode_actor_revocation(canonical_bytes, feed).is_ok())
                        {
                            return Err(ExchangeError::MissingDependency);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_domain(
    domain: ReservedDomain,
    expected: &[u8; 32],
    bytes: &[u8],
) -> Result<(), ExchangeError> {
    if &domain.digest(bytes) == expected {
        Ok(())
    } else {
        Err(ExchangeError::CidMismatch)
    }
}

fn encode_record(entry: &BaseExchangeEntryV1) -> Result<Vec<u8>, ExchangeError> {
    match entry {
        BaseExchangeEntryV1::VNextPublic {
            kind,
            cid,
            canonical_bytes,
        } => Ok(serde_json::to_vec(&VNextWireRecord {
            version: 1,
            class: "vnext-public",
            kind: *kind as u8,
            cid: encode_hex(cid),
            canonical_hex: encode_bytes(canonical_bytes),
        })?),
        BaseExchangeEntryV1::LegacyReadOnlyEvidence {
            cid,
            wire_bytes,
            epigenetics_json,
        } => Ok(serde_json::to_vec(&LegacyWireRecord {
            version: 1,
            class: "legacy-read-only-evidence",
            cid: encode_hex(cid),
            wire_hex: encode_bytes(wire_bytes),
            epigenetics_hex: encode_bytes(epigenetics_json),
        })?),
    }
}

fn decode_record(line: &[u8]) -> Result<BaseExchangeEntryV1, ExchangeError> {
    let wire: WireRecord = serde_json::from_slice(line)?;
    match wire {
        WireRecord::VNextPublic {
            version,
            kind,
            cid,
            canonical_hex,
        } => {
            if version != 1 {
                return Err(ExchangeError::UnknownVersionOrKind);
            }
            Ok(BaseExchangeEntryV1::VNextPublic {
                kind: decode_kind(kind)?,
                cid: decode_hex_32(&cid)?,
                canonical_bytes: decode_bytes(&canonical_hex)?,
            })
        }
        WireRecord::LegacyReadOnlyEvidence {
            version,
            cid,
            wire_hex,
            epigenetics_hex,
        } => {
            if version != 1 {
                return Err(ExchangeError::UnknownVersionOrKind);
            }
            Ok(BaseExchangeEntryV1::LegacyReadOnlyEvidence {
                cid: decode_hex_32(&cid)?,
                wire_bytes: decode_bytes(&wire_hex)?,
                epigenetics_json: decode_bytes(&epigenetics_hex)?,
            })
        }
    }
}

fn decode_kind(value: u8) -> Result<StoredRecordKind, ExchangeError> {
    match value {
        1 => Ok(StoredRecordKind::Object),
        2 => Ok(StoredRecordKind::Event),
        3 => Ok(StoredRecordKind::FeedInception),
        4 => Ok(StoredRecordKind::AuthorityEvent),
        _ => Err(ExchangeError::UnknownVersionOrKind),
    }
}

fn receipt(payload: &[u8], count: usize) -> ExchangeReceipt {
    let mut hasher = blake3::Hasher::new_derive_key("onebrain:canonical-public-exchange:1");
    hasher.update(&(count as u64).to_be_bytes());
    hasher.update(&(payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    ExchangeReceipt {
        count: count as u64,
        byte_length: payload.len() as u64,
        root: *hasher.finalize().as_bytes(),
    }
}

fn sort_key(entry: &BaseExchangeEntryV1) -> (u8, [u8; 32]) {
    match entry {
        BaseExchangeEntryV1::VNextPublic { kind, cid, .. } => (*kind as u8, *cid),
        BaseExchangeEntryV1::LegacyReadOnlyEvidence { cid, .. } => (u8::MAX, *cid),
    }
}

fn entry_cid(entry: &BaseExchangeEntryV1) -> [u8; 32] {
    match entry {
        BaseExchangeEntryV1::VNextPublic { cid, .. }
        | BaseExchangeEntryV1::LegacyReadOnlyEvidence { cid, .. } => *cid,
    }
}

#[derive(Serialize)]
struct VNextWireRecord {
    version: u8,
    class: &'static str,
    kind: u8,
    cid: String,
    canonical_hex: String,
}

#[derive(Serialize)]
struct LegacyWireRecord {
    version: u8,
    class: &'static str,
    cid: String,
    wire_hex: String,
    epigenetics_hex: String,
}

#[derive(Deserialize)]
#[serde(tag = "class", deny_unknown_fields)]
enum WireRecord {
    #[serde(rename = "vnext-public")]
    VNextPublic {
        version: u8,
        kind: u8,
        cid: String,
        canonical_hex: String,
    },
    #[serde(rename = "legacy-read-only-evidence")]
    LegacyReadOnlyEvidence {
        version: u8,
        cid: String,
        wire_hex: String,
        epigenetics_hex: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FooterEnvelope {
    footer: Footer,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Footer {
    version: u8,
    count: u64,
    byte_length: u64,
    root: String,
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    encode_bytes(bytes)
}

fn encode_bytes(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_hex_32(value: &str) -> Result<[u8; 32], ExchangeError> {
    let bytes = decode_bytes(value)?;
    bytes.try_into().map_err(|_| ExchangeError::Malformed)
}

fn decode_bytes(value: &str) -> Result<Vec<u8>, ExchangeError> {
    if value.len() % 2 != 0 || value.len() / 2 > MAX_EXCHANGE_RECORD_BYTES {
        return Err(ExchangeError::Malformed);
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| ExchangeError::Malformed)
        })
        .collect()
}
