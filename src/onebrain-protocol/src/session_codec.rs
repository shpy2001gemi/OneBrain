//! Canonical schema for authenticated session handshake records.

use std::collections::BTreeSet;

use ku_core::foundation::{
    canonicalize_set_by_key, decode_canonical, encode_canonical, CanonicalError, CanonicalValue,
    FeedId, NamespaceCommitment, NodeId, ObjectReference, ResourceProfile,
};

use crate::types::{
    wire_id, SelectiveFeedProof, SessionCapability, SessionFinish, SessionHandshakeMessage,
    SessionHello, SessionProfile, SessionWelcome, VNEXT_PROTOCOL_SCHEMA_ID,
    VNEXT_PROTOCOL_SCHEMA_MAJOR, VNEXT_PROTOCOL_SCHEMA_MINOR,
};

pub fn encode_session_message(
    message: &SessionHandshakeMessage,
) -> Result<Vec<u8>, SessionCodecError> {
    validate(message, true)?;
    encode_root(message, true)
}

pub fn session_signing_bytes(
    message: &SessionHandshakeMessage,
) -> Result<Vec<u8>, SessionCodecError> {
    validate(message, false)?;
    encode_root(message, false)
}

pub fn decode_session_message(bytes: &[u8]) -> Result<SessionHandshakeMessage, SessionCodecError> {
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value, "root")?;
    if unsigned(root, 0, "schema")? != VNEXT_PROTOCOL_SCHEMA_ID {
        return Err(SessionCodecError::WrongSchema);
    }
    if unsigned(root, 1, "major")? != VNEXT_PROTOCOL_SCHEMA_MAJOR {
        return Err(SessionCodecError::UnsupportedMajor);
    }
    let _minor = unsigned(root, 2, "minor")?;
    let wire = unsigned(root, 3, "wire")?;
    let body = map(required(root, 4, "body")?, "body")?;
    let message = match wire {
        wire_id::SESSION_HELLO => SessionHandshakeMessage::Hello(SessionHello {
            transport_binding: bytes32(body, 0, "hello.binding")?,
            initiator_nonce: bytes32(body, 1, "hello.nonce")?,
            node: NodeId::from_bytes(bytes32(body, 2, "hello.node")?),
            node_public_key: bytes32(body, 3, "hello.key")?,
            profiles: parse_profiles(array(body, 4, "hello.profiles")?)?,
            capabilities: parse_capabilities(array(body, 5, "hello.capabilities")?)?,
            feed_proofs: parse_feed_proofs(array(body, 6, "hello.feeds")?)?,
            signature: bytes64(body, 7, "hello.signature")?,
        }),
        wire_id::SESSION_WELCOME => SessionHandshakeMessage::Welcome(SessionWelcome {
            transport_binding: bytes32(body, 0, "welcome.binding")?,
            initiator_transcript: bytes32(body, 1, "welcome.transcript")?,
            responder_nonce: bytes32(body, 2, "welcome.nonce")?,
            node: NodeId::from_bytes(bytes32(body, 3, "welcome.node")?),
            node_public_key: bytes32(body, 4, "welcome.key")?,
            selected_profile: parse_profile(required(body, 5, "welcome.profile")?)?,
            negotiated_capabilities: parse_capabilities(array(body, 6, "welcome.capabilities")?)?,
            feed_proofs: parse_feed_proofs(array(body, 7, "welcome.feeds")?)?,
            signature: bytes64(body, 8, "welcome.signature")?,
        }),
        wire_id::SESSION_FINISH => SessionHandshakeMessage::Finish(SessionFinish {
            transcript: bytes32(body, 0, "finish.transcript")?,
            initiator: NodeId::from_bytes(bytes32(body, 1, "finish.initiator")?),
            signature: bytes64(body, 2, "finish.signature")?,
        }),
        value => return Err(SessionCodecError::UnknownWireId(value)),
    };
    validate(&message, true)?;
    if encode_session_message(&message)? != bytes {
        return Err(SessionCodecError::NonCanonicalMessage);
    }
    Ok(message)
}

fn encode_root(
    message: &SessionHandshakeMessage,
    include_signature: bool,
) -> Result<Vec<u8>, SessionCodecError> {
    let value = CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_ID)),
        (1, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_MAJOR)),
        (2, CanonicalValue::Unsigned(VNEXT_PROTOCOL_SCHEMA_MINOR)),
        (3, CanonicalValue::Unsigned(message.wire_id())),
        (4, body(message, include_signature)?),
    ]);
    encode_canonical(&value, ResourceProfile::ControlV1).map_err(Into::into)
}

fn body(
    message: &SessionHandshakeMessage,
    include_signature: bool,
) -> Result<CanonicalValue, SessionCodecError> {
    let mut fields = match message {
        SessionHandshakeMessage::Hello(hello) => vec![
            (0, CanonicalValue::Bytes(hello.transport_binding.to_vec())),
            (1, CanonicalValue::Bytes(hello.initiator_nonce.to_vec())),
            (2, CanonicalValue::Bytes(hello.node.as_bytes().to_vec())),
            (3, CanonicalValue::Bytes(hello.node_public_key.to_vec())),
            (4, profiles_value(&hello.profiles)),
            (5, capabilities_value(&hello.capabilities)?),
            (6, feed_proofs_value(&hello.feed_proofs)?),
        ],
        SessionHandshakeMessage::Welcome(welcome) => vec![
            (0, CanonicalValue::Bytes(welcome.transport_binding.to_vec())),
            (
                1,
                CanonicalValue::Bytes(welcome.initiator_transcript.to_vec()),
            ),
            (2, CanonicalValue::Bytes(welcome.responder_nonce.to_vec())),
            (3, CanonicalValue::Bytes(welcome.node.as_bytes().to_vec())),
            (4, CanonicalValue::Bytes(welcome.node_public_key.to_vec())),
            (5, profile_value(welcome.selected_profile)),
            (6, capabilities_value(&welcome.negotiated_capabilities)?),
            (7, feed_proofs_value(&welcome.feed_proofs)?),
        ],
        SessionHandshakeMessage::Finish(finish) => vec![
            (0, CanonicalValue::Bytes(finish.transcript.to_vec())),
            (
                1,
                CanonicalValue::Bytes(finish.initiator.as_bytes().to_vec()),
            ),
        ],
    };
    if include_signature {
        match message {
            SessionHandshakeMessage::Hello(hello) => {
                fields.push((7, CanonicalValue::Bytes(hello.signature.to_vec())))
            }
            SessionHandshakeMessage::Welcome(welcome) => {
                fields.push((8, CanonicalValue::Bytes(welcome.signature.to_vec())))
            }
            SessionHandshakeMessage::Finish(finish) => {
                fields.push((2, CanonicalValue::Bytes(finish.signature.to_vec())))
            }
        }
    }
    Ok(CanonicalValue::Map(fields))
}

fn validate(
    message: &SessionHandshakeMessage,
    require_signature: bool,
) -> Result<(), SessionCodecError> {
    match message {
        SessionHandshakeMessage::Hello(hello) => {
            nonzero32(hello.transport_binding, "hello.binding")?;
            nonzero32(hello.initiator_nonce, "hello.nonce")?;
            nonzero32(hello.node_public_key, "hello.key")?;
            validate_profiles(&hello.profiles)?;
            validate_capabilities(&hello.capabilities, false)?;
            validate_feed_proofs(&hello.feed_proofs, &hello.capabilities)?;
            if require_signature {
                nonzero64(hello.signature, "hello.signature")?;
            }
        }
        SessionHandshakeMessage::Welcome(welcome) => {
            nonzero32(welcome.transport_binding, "welcome.binding")?;
            nonzero32(welcome.initiator_transcript, "welcome.transcript")?;
            nonzero32(welcome.responder_nonce, "welcome.nonce")?;
            nonzero32(welcome.node_public_key, "welcome.key")?;
            validate_capabilities(&welcome.negotiated_capabilities, true)?;
            validate_feed_proofs(&welcome.feed_proofs, &welcome.negotiated_capabilities)?;
            if require_signature {
                nonzero64(welcome.signature, "welcome.signature")?;
            }
        }
        SessionHandshakeMessage::Finish(finish) => {
            nonzero32(finish.transcript, "finish.transcript")?;
            if require_signature {
                nonzero64(finish.signature, "finish.signature")?;
            }
        }
    }
    Ok(())
}

fn validate_profiles(profiles: &[SessionProfile]) -> Result<(), SessionCodecError> {
    if profiles.is_empty() || profiles.len() > 64 {
        return Err(SessionCodecError::Limit);
    }
    if profiles.iter().copied().collect::<BTreeSet<_>>().len() != profiles.len() {
        return Err(SessionCodecError::DuplicateProfile);
    }
    Ok(())
}

fn validate_capabilities(
    capabilities: &[SessionCapability],
    may_be_empty: bool,
) -> Result<(), SessionCodecError> {
    if (!may_be_empty && capabilities.is_empty()) || capabilities.len() > 4_096 {
        return Err(SessionCodecError::Limit);
    }
    if capabilities.iter().copied().collect::<BTreeSet<_>>().len() != capabilities.len() {
        return Err(SessionCodecError::DuplicateCapability);
    }
    Ok(())
}

fn validate_feed_proofs(
    proofs: &[SelectiveFeedProof],
    capabilities: &[SessionCapability],
) -> Result<(), SessionCodecError> {
    if proofs.len() > 1_024 {
        return Err(SessionCodecError::Limit);
    }
    let keys = proofs
        .iter()
        .map(|proof| (*proof.feed.as_bytes(), proof.capability))
        .collect::<BTreeSet<_>>();
    if keys.len() != proofs.len() {
        return Err(SessionCodecError::DuplicateFeedProof);
    }
    if proofs
        .iter()
        .any(|proof| !capabilities.contains(&proof.capability))
    {
        return Err(SessionCodecError::FeedProofOutsideCapabilities);
    }
    Ok(())
}

fn profile_value(profile: SessionProfile) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(profile.family)),
        (1, CanonicalValue::Unsigned(profile.major)),
        (2, CanonicalValue::Unsigned(profile.minor)),
    ])
}

fn profiles_value(profiles: &[SessionProfile]) -> CanonicalValue {
    CanonicalValue::Array(profiles.iter().copied().map(profile_value).collect())
}

fn capabilities_value(
    capabilities: &[SessionCapability],
) -> Result<CanonicalValue, SessionCodecError> {
    let members = capabilities
        .iter()
        .map(|capability| CanonicalValue::Bytes(capability.as_bytes().to_vec()))
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ControlV1,
    )?))
}

fn feed_proofs_value(proofs: &[SelectiveFeedProof]) -> Result<CanonicalValue, SessionCodecError> {
    let members = proofs
        .iter()
        .map(|proof| {
            CanonicalValue::Map(vec![
                (0, CanonicalValue::Bytes(proof.feed.as_bytes().to_vec())),
                (
                    1,
                    CanonicalValue::Bytes(proof.namespace.as_bytes().to_vec()),
                ),
                (
                    2,
                    CanonicalValue::Bytes(proof.capability.as_bytes().to_vec()),
                ),
                (3, reference_value(&proof.proof)),
            ])
        })
        .map(|value| (value.clone(), value))
        .collect();
    Ok(CanonicalValue::Array(canonicalize_set_by_key(
        members,
        ResourceProfile::ControlV1,
    )?))
}

fn reference_value(reference: &ObjectReference) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, CanonicalValue::Unsigned(reference.reference_kind)),
        (1, CanonicalValue::Bytes(reference.cid.to_vec())),
    ])
}

fn parse_profiles(values: &[CanonicalValue]) -> Result<Vec<SessionProfile>, SessionCodecError> {
    values.iter().map(parse_profile).collect()
}

fn parse_profile(value: &CanonicalValue) -> Result<SessionProfile, SessionCodecError> {
    let value = map(value, "profile")?;
    Ok(SessionProfile {
        family: unsigned(value, 0, "profile.family")?,
        major: unsigned(value, 1, "profile.major")?,
        minor: unsigned(value, 2, "profile.minor")?,
    })
}

fn parse_capabilities(
    values: &[CanonicalValue],
) -> Result<Vec<SessionCapability>, SessionCodecError> {
    values
        .iter()
        .map(|value| bytes32_value(value, "capability").map(SessionCapability::from_bytes))
        .collect()
}

fn parse_feed_proofs(
    values: &[CanonicalValue],
) -> Result<Vec<SelectiveFeedProof>, SessionCodecError> {
    values
        .iter()
        .map(|value| {
            let value = map(value, "feed_proof")?;
            Ok(SelectiveFeedProof {
                feed: FeedId::from_bytes(bytes32(value, 0, "feed.id")?),
                namespace: NamespaceCommitment::from_bytes(bytes32(value, 1, "feed.namespace")?),
                capability: SessionCapability::from_bytes(bytes32(value, 2, "feed.capability")?),
                proof: parse_reference(required(value, 3, "feed.proof")?)?,
            })
        })
        .collect()
}

fn parse_reference(value: &CanonicalValue) -> Result<ObjectReference, SessionCodecError> {
    let value = map(value, "reference")?;
    Ok(ObjectReference::new(
        unsigned(value, 0, "reference.kind")?,
        bytes32(value, 1, "reference.cid")?,
    ))
}

fn nonzero32(value: [u8; 32], field: &'static str) -> Result<(), SessionCodecError> {
    if value == [0; 32] {
        Err(SessionCodecError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn nonzero64(value: [u8; 64], field: &'static str) -> Result<(), SessionCodecError> {
    if value == [0; 64] {
        Err(SessionCodecError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], SessionCodecError> {
    match value {
        CanonicalValue::Map(value) => Ok(value),
        _ => Err(SessionCodecError::InvalidField(field)),
    }
}

fn required<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, SessionCodecError> {
    map.iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(SessionCodecError::InvalidField(field))
}

fn unsigned(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, SessionCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(SessionCodecError::InvalidField(field)),
    }
}

fn array<'a>(
    map: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], SessionCodecError> {
    match required(map, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(SessionCodecError::InvalidField(field)),
    }
}

fn bytes32(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], SessionCodecError> {
    bytes32_value(required(map, key, field)?, field)
}

fn bytes32_value(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; 32], SessionCodecError> {
    let CanonicalValue::Bytes(bytes) = value else {
        return Err(SessionCodecError::InvalidField(field));
    };
    if bytes.len() != 32 {
        return Err(SessionCodecError::InvalidField(field));
    }
    let mut output = [0; 32];
    output.copy_from_slice(bytes);
    Ok(output)
}

fn bytes64(
    map: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], SessionCodecError> {
    let CanonicalValue::Bytes(bytes) = required(map, key, field)? else {
        return Err(SessionCodecError::InvalidField(field));
    };
    if bytes.len() != 64 {
        return Err(SessionCodecError::InvalidField(field));
    }
    let mut output = [0; 64];
    output.copy_from_slice(bytes);
    Ok(output)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionCodecError {
    Canonical(CanonicalError),
    WrongSchema,
    UnsupportedMajor,
    UnknownWireId(u64),
    InvalidField(&'static str),
    NonCanonicalMessage,
    Limit,
    DuplicateProfile,
    DuplicateCapability,
    DuplicateFeedProof,
    FeedProofOutsideCapabilities,
}

impl From<CanonicalError> for SessionCodecError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl std::fmt::Display for SessionCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SESSION_CODEC: {self:?}")
    }
}

impl std::error::Error for SessionCodecError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_hello_round_trips_and_unsigned_preimage_excludes_signature() {
        let mut hello = SessionHello {
            transport_binding: [1; 32],
            initiator_nonce: [2; 32],
            node: NodeId::from_bytes([3; 32]),
            node_public_key: [4; 32],
            profiles: vec![SessionProfile {
                family: 1,
                major: 1,
                minor: 0,
            }],
            capabilities: vec![SessionCapability::from_bytes([5; 32])],
            feed_proofs: Vec::new(),
            signature: [6; 64],
        };
        let first_preimage =
            session_signing_bytes(&SessionHandshakeMessage::Hello(hello.clone())).unwrap();
        hello.signature = [7; 64];
        let second_preimage =
            session_signing_bytes(&SessionHandshakeMessage::Hello(hello.clone())).unwrap();
        assert_eq!(first_preimage, second_preimage);
        let encoded =
            encode_session_message(&SessionHandshakeMessage::Hello(hello.clone())).unwrap();
        assert_eq!(
            decode_session_message(&encoded).unwrap(),
            SessionHandshakeMessage::Hello(hello)
        );
    }
}
