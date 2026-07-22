//! Canonical vNext message codec shared by all carriers.

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, DisclosureClass, EventCid,
    ObjectCid, ReservedDomain, ResourceProfile, SelectorCid,
};

use crate::types::{
    wire_id, VNextMessage, MAX_VNEXT_PAYLOAD_BYTES, VNEXT_PROTOCOL_SCHEMA_ID,
    VNEXT_PROTOCOL_SCHEMA_MAJOR, VNEXT_PROTOCOL_SCHEMA_MINOR,
};

pub fn encode_message(message: &VNextMessage) -> Result<Vec<u8>, VNextCodecError> {
    validate(message)?;
    let root = CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_ID)),
        (1, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_MAJOR)),
        (2, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_MINOR)),
        (3, CanonicalValue::Unsigned(message.wire_id())),
        (4, body(message)),
    ]);
    encode_canonical(&root, ResourceProfile::ObjectV1).map_err(Into::into)
}

pub fn decode_message(bytes: &[u8]) -> Result<VNextMessage, VNextCodecError> {
    let value = decode_canonical(bytes, ResourceProfile::ObjectV1)?;
    let root = map(&value, "root")?;
    if unsigned(root, 0, "schema")? != VNEXT_PROTOCOL_SCHEMA_ID {
        return Err(VNextCodecError::WrongSchema);
    }
    if unsigned(root, 1, "major")? != VNEXT_PROTOCOL_SCHEMA_MAJOR {
        return Err(VNextCodecError::UnsupportedMajor);
    }
    let _minor = unsigned(root, 2, "minor")?;
    let wire = unsigned(root, 3, "wire_id")?;
    let body = map(required(root, 4, "body")?, "body")?;
    let selector = SelectorCid::from_bytes(bytes32(body, 0, "selector")?);
    let message = match wire {
        wire_id::OBJECT_MANIFEST => VNextMessage::ObjectManifest {
            selector,
            object: ObjectCid::from_bytes(bytes32(body, 1, "object")?),
            disclosure: disclosure(unsigned(body, 2, "disclosure")?)?,
            canonical_length: unsigned(body, 3, "length")?,
        },
        wire_id::OBJECT_PAYLOAD => VNextMessage::ObjectPayload {
            selector,
            object: ObjectCid::from_bytes(bytes32(body, 1, "object")?),
            canonical_bytes: byte_string(body, 2, "payload")?.to_vec(),
        },
        wire_id::EVENT_MANIFEST => VNextMessage::EventManifest {
            selector,
            event: EventCid::from_bytes(bytes32(body, 1, "event")?),
            disclosure: disclosure(unsigned(body, 2, "disclosure")?)?,
            canonical_length: unsigned(body, 3, "length")?,
        },
        wire_id::EVENT_PAYLOAD => VNextMessage::EventPayload {
            selector,
            event: EventCid::from_bytes(bytes32(body, 1, "event")?),
            canonical_bytes: byte_string(body, 2, "payload")?.to_vec(),
        },
        value => return Err(VNextCodecError::UnknownWireId(value)),
    };
    validate(&message)?;
    if encode_message(&message)? != bytes {
        return Err(VNextCodecError::NonCanonicalMessage);
    }
    Ok(message)
}

fn validate(message: &VNextMessage) -> Result<(), VNextCodecError> {
    match message {
        VNextMessage::ObjectManifest {
            canonical_length, ..
        }
        | VNextMessage::EventManifest {
            canonical_length, ..
        } => {
            if *canonical_length == 0 || *canonical_length > MAX_VNEXT_PAYLOAD_BYTES as u64 {
                return Err(VNextCodecError::InvalidPayloadLength);
            }
        }
        VNextMessage::ObjectPayload {
            object,
            canonical_bytes,
            ..
        } => {
            validate_payload_length(canonical_bytes)?;
            let calculated = ObjectCid::compute(ReservedDomain::Object, canonical_bytes)
                .expect("object domain produces ObjectCid");
            if calculated != *object {
                return Err(VNextCodecError::PayloadCidMismatch);
            }
        }
        VNextMessage::EventPayload {
            event,
            canonical_bytes,
            ..
        } => {
            validate_payload_length(canonical_bytes)?;
            let calculated = EventCid::compute(ReservedDomain::Event, canonical_bytes)
                .expect("event domain produces EventCid");
            if calculated != *event {
                return Err(VNextCodecError::PayloadCidMismatch);
            }
        }
    }
    Ok(())
}

fn validate_payload_length(bytes: &[u8]) -> Result<(), VNextCodecError> {
    if bytes.is_empty() || bytes.len() > MAX_VNEXT_PAYLOAD_BYTES {
        Err(VNextCodecError::InvalidPayloadLength)
    } else {
        Ok(())
    }
}

fn body(message: &VNextMessage) -> CanonicalValue {
    match message {
        VNextMessage::ObjectManifest {
            selector,
            object,
            disclosure,
            canonical_length,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(selector.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(object.as_bytes().to_vec())),
            (2, CanonicalValue::Unsigned(*disclosure as u64)),
            (3, CanonicalValue::Unsigned(*canonical_length)),
        ]),
        VNextMessage::ObjectPayload {
            selector,
            object,
            canonical_bytes,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(selector.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(object.as_bytes().to_vec())),
            (2, CanonicalValue::Bytes(canonical_bytes.clone())),
        ]),
        VNextMessage::EventManifest {
            selector,
            event,
            disclosure,
            canonical_length,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(selector.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(event.as_bytes().to_vec())),
            (2, CanonicalValue::Unsigned(*disclosure as u64)),
            (3, CanonicalValue::Unsigned(*canonical_length)),
        ]),
        VNextMessage::EventPayload {
            selector,
            event,
            canonical_bytes,
        } => CanonicalValue::Map(vec![
            (0, CanonicalValue::Bytes(selector.as_bytes().to_vec())),
            (1, CanonicalValue::Bytes(event.as_bytes().to_vec())),
            (2, CanonicalValue::Bytes(canonical_bytes.clone())),
        ]),
    }
}

fn disclosure(value: u64) -> Result<DisclosureClass, VNextCodecError> {
    match value {
        0 => Ok(DisclosureClass::Public),
        1 => Ok(DisclosureClass::NegotiatedEncrypted),
        2 => Ok(DisclosureClass::RouteMinimal),
        3 => Ok(DisclosureClass::LocalOnly),
        _ => Err(VNextCodecError::InvalidField("disclosure")),
    }
}

fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], VNextCodecError> {
    match value {
        CanonicalValue::Map(value) => Ok(value),
        _ => Err(VNextCodecError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, VNextCodecError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(VNextCodecError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, VNextCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(VNextCodecError::InvalidField(field)),
    }
}

fn byte_string<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [u8], VNextCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Bytes(value) => Ok(value),
        _ => Err(VNextCodecError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], VNextCodecError> {
    let bytes = byte_string(map, key, field)?;
    if bytes.len() != 32 {
        return Err(VNextCodecError::InvalidField(field));
    }
    let mut value = [0; 32];
    value.copy_from_slice(bytes);
    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VNextCodecError {
    Canonical(CanonicalError),
    WrongSchema,
    UnsupportedMajor,
    UnknownWireId(u64),
    InvalidField(&'static str),
    InvalidPayloadLength,
    PayloadCidMismatch,
    NonCanonicalMessage,
}

impl From<CanonicalError> for VNextCodecError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl std::fmt::Display for VNextCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "VNEXT_PROTOCOL_CODEC: {self:?}")
    }
}

impl std::error::Error for VNextCodecError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector() -> SelectorCid {
        SelectorCid::from_bytes([1; 32])
    }

    #[test]
    fn logical_message_has_one_deterministic_canonical_payload() {
        let message = VNextMessage::ObjectManifest {
            selector: selector(),
            object: ObjectCid::from_bytes([2; 32]),
            disclosure: DisclosureClass::Public,
            canonical_length: 512,
        };
        let first = encode_message(&message).unwrap();
        let second = encode_message(&message).unwrap();
        assert_eq!(first, second);
        assert_eq!(decode_message(&first).unwrap(), message);
    }

    #[test]
    fn payload_is_bound_to_declared_cid() {
        let payload = vec![0xa0];
        let object = ObjectCid::compute(ReservedDomain::Object, &payload).unwrap();
        let message = VNextMessage::ObjectPayload {
            selector: selector(),
            object,
            canonical_bytes: payload,
        };
        let bytes = encode_message(&message).unwrap();
        assert_eq!(decode_message(&bytes).unwrap(), message);

        let mismatched = VNextMessage::ObjectPayload {
            selector: selector(),
            object: ObjectCid::from_bytes([9; 32]),
            canonical_bytes: vec![0xa0],
        };
        assert_eq!(
            encode_message(&mismatched).unwrap_err(),
            VNextCodecError::PayloadCidMismatch
        );
    }

    #[test]
    fn unknown_wire_id_and_oversized_manifest_are_rejected() {
        let unknown = CanonicalValue::Map(vec![
            (0, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_ID)),
            (1, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_MAJOR)),
            (2, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_MINOR)),
            (3, CanonicalValue::Unsigned(999)),
            (
                4,
                CanonicalValue::Map(vec![(
                    0,
                    CanonicalValue::Bytes(selector().as_bytes().to_vec()),
                )]),
            ),
        ]);
        let bytes = encode_canonical(&unknown, ResourceProfile::ObjectV1).unwrap();
        assert_eq!(
            decode_message(&bytes).unwrap_err(),
            VNextCodecError::UnknownWireId(999)
        );

        let oversized = VNextMessage::EventManifest {
            selector: selector(),
            event: EventCid::from_bytes([3; 32]),
            disclosure: DisclosureClass::Public,
            canonical_length: MAX_VNEXT_PAYLOAD_BYTES as u64 + 1,
        };
        assert_eq!(
            encode_message(&oversized).unwrap_err(),
            VNextCodecError::InvalidPayloadLength
        );
    }
}
