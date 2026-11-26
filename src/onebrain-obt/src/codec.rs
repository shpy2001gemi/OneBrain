//! Internal deterministic CBOR helpers built on `ku_core::foundation::canonical`.

use ku_core::foundation::{
    decode_canonical, encode_canonical, ActorId, CanonicalError, CanonicalErrorKind,
    CanonicalValue, ConceptCcid, EventCid, FeedId, ObjectCid, PermitCid, ResourceProfile,
};

pub(crate) fn map_lookup(entries: &[(u64, CanonicalValue)], key: u64) -> Option<&CanonicalValue> {
    entries
        .iter()
        .find_map(|(k, value)| (*k == key).then_some(value))
}

pub(crate) fn req_field(
    entries: &[(u64, CanonicalValue)],
    key: u64,
) -> Result<&CanonicalValue, CanonicalError> {
    map_lookup(entries, key).ok_or_else(|| CanonicalError::new(CanonicalErrorKind::Truncated, 0))
}

pub(crate) fn expect_map<'a>(
    value: &'a CanonicalValue,
    allowed_keys: &[u64],
) -> Result<&'a [(u64, CanonicalValue)], CanonicalError> {
    let CanonicalValue::Map(entries) = value else {
        return Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0));
    };
    for (k, _) in entries {
        if !allowed_keys.contains(k) {
            return Err(CanonicalError::new(CanonicalErrorKind::UnknownField, 0));
        }
    }
    Ok(entries)
}

pub(crate) fn expect_u64(value: &CanonicalValue) -> Result<u64, CanonicalError> {
    match value {
        CanonicalValue::Unsigned(v) => Ok(*v),
        CanonicalValue::Negative(_) => {
            Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0))
        }
        _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
    }
}

pub(crate) fn expect_u32(value: &CanonicalValue) -> Result<u32, CanonicalError> {
    let raw = expect_u64(value)?;
    u32::try_from(raw).map_err(|_| CanonicalError::new(CanonicalErrorKind::LimitItems, 0))
}

pub(crate) fn expect_u16(value: &CanonicalValue) -> Result<u16, CanonicalError> {
    let raw = expect_u64(value)?;
    u16::try_from(raw).map_err(|_| CanonicalError::new(CanonicalErrorKind::LimitItems, 0))
}

pub(crate) fn expect_bool(value: &CanonicalValue) -> Result<bool, CanonicalError> {
    match value {
        CanonicalValue::Bool(b) => Ok(*b),
        _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
    }
}

pub(crate) fn expect_text(
    value: &CanonicalValue,
    max_len: usize,
) -> Result<String, CanonicalError> {
    match value {
        CanonicalValue::Text(s) if s.len() <= max_len => Ok(s.clone()),
        CanonicalValue::Text(_) => Err(CanonicalError::new(CanonicalErrorKind::LimitBytes, 0)),
        _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
    }
}

pub(crate) fn expect_array(value: &CanonicalValue) -> Result<&[CanonicalValue], CanonicalError> {
    match value {
        CanonicalValue::Array(items) => Ok(items),
        _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
    }
}

pub(crate) fn expect_bytes_exact<const N: usize>(
    value: &CanonicalValue,
) -> Result<[u8; N], CanonicalError> {
    match value {
        CanonicalValue::Bytes(bytes) if bytes.len() == N => {
            let mut out = [0u8; N];
            out.copy_from_slice(bytes);
            Ok(out)
        }
        _ => Err(CanonicalError::new(CanonicalErrorKind::ForbiddenType, 0)),
    }
}

pub(crate) fn encode_u128(value: u128) -> CanonicalValue {
    CanonicalValue::Bytes(value.to_be_bytes().to_vec())
}

pub(crate) fn expect_u128(value: &CanonicalValue) -> Result<u128, CanonicalError> {
    let bytes = expect_bytes_exact::<16>(value)?;
    Ok(u128::from_be_bytes(bytes))
}

pub(crate) fn encode_actor(actor: ActorId) -> CanonicalValue {
    CanonicalValue::Bytes(actor.as_bytes().to_vec())
}

pub(crate) fn expect_actor(value: &CanonicalValue) -> Result<ActorId, CanonicalError> {
    Ok(ActorId::from_bytes(expect_bytes_exact::<32>(value)?))
}

pub(crate) fn encode_feed(feed: FeedId) -> CanonicalValue {
    CanonicalValue::Bytes(feed.as_bytes().to_vec())
}

pub(crate) fn expect_feed(value: &CanonicalValue) -> Result<FeedId, CanonicalError> {
    Ok(FeedId::from_bytes(expect_bytes_exact::<32>(value)?))
}

pub(crate) fn encode_object_cid(cid: ObjectCid) -> CanonicalValue {
    CanonicalValue::Bytes(cid.as_bytes().to_vec())
}

pub(crate) fn expect_object_cid(value: &CanonicalValue) -> Result<ObjectCid, CanonicalError> {
    Ok(ObjectCid::from_bytes(expect_bytes_exact::<32>(value)?))
}

pub(crate) fn encode_event_cid(cid: EventCid) -> CanonicalValue {
    CanonicalValue::Bytes(cid.as_bytes().to_vec())
}

pub(crate) fn expect_event_cid(value: &CanonicalValue) -> Result<EventCid, CanonicalError> {
    Ok(EventCid::from_bytes(expect_bytes_exact::<32>(value)?))
}

pub(crate) fn encode_permit_cid(cid: PermitCid) -> CanonicalValue {
    CanonicalValue::Bytes(cid.as_bytes().to_vec())
}

pub(crate) fn expect_permit_cid(value: &CanonicalValue) -> Result<PermitCid, CanonicalError> {
    Ok(PermitCid::from_bytes(expect_bytes_exact::<32>(value)?))
}

pub(crate) fn encode_ccid(ccid: ConceptCcid) -> CanonicalValue {
    CanonicalValue::Bytes(ccid.as_bytes().to_vec())
}

pub(crate) fn expect_ccid(value: &CanonicalValue) -> Result<ConceptCcid, CanonicalError> {
    Ok(ConceptCcid::from_bytes(expect_bytes_exact::<16>(value)?))
}

pub(crate) fn encode_value(
    value: &CanonicalValue,
    profile: ResourceProfile,
) -> Result<Vec<u8>, CanonicalError> {
    encode_canonical(value, profile)
}

pub(crate) fn decode_value(
    bytes: &[u8],
    profile: ResourceProfile,
) -> Result<CanonicalValue, CanonicalError> {
    decode_canonical(bytes, profile)
}

pub(crate) fn domain_digest(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

pub(crate) fn contains_negative_anywhere(value: &CanonicalValue) -> bool {
    match value {
        CanonicalValue::Negative(_) => true,
        CanonicalValue::Array(items) => items.iter().any(contains_negative_anywhere),
        CanonicalValue::Map(entries) => entries.iter().any(|(_, v)| contains_negative_anywhere(v)),
        CanonicalValue::Unsigned(_)
        | CanonicalValue::Bytes(_)
        | CanonicalValue::Text(_)
        | CanonicalValue::Bool(_)
        | CanonicalValue::Null => false,
    }
}
