//! Closed canonical relay-control messages and signing preimages.

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, NodeId, ResourceProfile,
};

use crate::{
    RelayPossessionChallengeV1, RelayPossessionProofV1, RelayReservationV1, RelayTransportV1,
};

pub const MAX_RELAY_CONTROL_BYTES: usize = 65_536;

pub mod relay_control_schema_id {
    pub const RESERVE: u64 = 50;
    pub const GRANTED: u64 = 51;
    pub const KEEPALIVE: u64 = 52;
    pub const REVOKE: u64 = 53;
    pub const POSSESSION_CHALLENGE: u64 = 54;
    pub const POSSESSION_PROOF: u64 = 55;
    pub const DENIED: u64 = 61;
    pub const OUTER_CLIENT_CHALLENGE: u64 = 62;
    pub const OUTER_CLIENT_HELLO: u64 = 63;
}

const DOMAIN_RESERVE: &[u8] = b"onebrain/reachability/relay-reserve-request/v1\0";
const DOMAIN_KEEPALIVE: &[u8] = b"onebrain/reachability/relay-keepalive/v1\0";
const DOMAIN_REVOKE: &[u8] = b"onebrain/reachability/relay-revoke/v1\0";
const DOMAIN_DENIAL: &[u8] = b"onebrain/reachability/relay-denial/v1\0";
const DOMAIN_OUTER_CHALLENGE: &[u8] = b"onebrain/reachability/relay-outer-client-challenge/v1\0";
const DOMAIN_OUTER_HELLO: &[u8] = b"onebrain/reachability/relay-outer-client-hello/v1\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayReserveRequestV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub target_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub transport_scope: Vec<RelayTransportV1>,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub target_reservation_signature: [u8; 64],
    pub target_request_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayKeepaliveV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub target_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub target_signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayRevocationActorV1 {
    Target,
    Relay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayRevocationReasonV1 {
    TargetClosed,
    RelayShutdown,
    CapacityReclaimed,
    PolicyRejected,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayRevokeV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub target_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub actor: RelayRevocationActorV1,
    pub reason: RelayRevocationReasonV1,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub actor_signature: [u8; 64],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayDenialCodeV1 {
    Capacity,
    Policy,
    InvalidTransportScope,
    RateLimited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayDenialV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub target_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub code: RelayDenialCodeV1,
    pub retry_after: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayOuterClientChallengeV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub challenge_nonce: [u8; 32],
    pub outer_connection_binding: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayOuterClientHelloV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub client_node_id: NodeId,
    pub client_public_key: [u8; 32],
    pub challenge_nonce: [u8; 32],
    pub outer_connection_binding: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
    pub client_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayControlV1 {
    Reserve(RelayReserveRequestV1),
    Granted(RelayReservationV1),
    Keepalive(RelayKeepaliveV1),
    Revoke(RelayRevokeV1),
    PossessionChallenge(RelayPossessionChallengeV1),
    PossessionProof(RelayPossessionProofV1),
    OuterClientChallenge(RelayOuterClientChallengeV1),
    OuterClientHello(RelayOuterClientHelloV1),
    Denied(RelayDenialV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayControlSignatureRoleV1 {
    ReserveRequestTarget,
    KeepaliveTarget,
    RevokeActor,
    DenialRelay,
    OuterChallengeRelay,
    OuterHelloClient,
}

#[derive(Clone, Copy)]
enum SignatureMode {
    All,
    OmitRequest,
}

pub fn encode_relay_control(value: &RelayControlV1) -> Result<Vec<u8>, RelayCodecError> {
    validate(value)?;
    encode_root(value, SignatureMode::All)
}

pub fn decode_relay_control(bytes: &[u8]) -> Result<RelayControlV1, RelayCodecError> {
    if bytes.len() > MAX_RELAY_CONTROL_BYTES {
        return Err(RelayCodecError::Limit);
    }
    let decoded = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&decoded, "root")?;
    exact_keys(root, &[0, 1], "root")?;
    let schema = unsigned(root, 0, "root.schema")?;
    let body = map(required(root, 1, "root.body")?, "root.body")?;
    let value = match schema {
        relay_control_schema_id::RESERVE => RelayControlV1::Reserve(parse_reserve(body)?),
        relay_control_schema_id::GRANTED => RelayControlV1::Granted(parse_reservation(body)?),
        relay_control_schema_id::KEEPALIVE => RelayControlV1::Keepalive(parse_keepalive(body)?),
        relay_control_schema_id::REVOKE => RelayControlV1::Revoke(parse_revoke(body)?),
        relay_control_schema_id::POSSESSION_CHALLENGE => {
            RelayControlV1::PossessionChallenge(parse_possession_challenge(body)?)
        }
        relay_control_schema_id::POSSESSION_PROOF => {
            RelayControlV1::PossessionProof(parse_possession_proof(body)?)
        }
        relay_control_schema_id::DENIED => RelayControlV1::Denied(parse_denial(body)?),
        relay_control_schema_id::OUTER_CLIENT_CHALLENGE => {
            RelayControlV1::OuterClientChallenge(parse_outer_challenge(body)?)
        }
        relay_control_schema_id::OUTER_CLIENT_HELLO => {
            RelayControlV1::OuterClientHello(parse_outer_hello(body)?)
        }
        other => return Err(RelayCodecError::UnknownSchema(other)),
    };
    validate(&value)?;
    if encode_relay_control(&value)? != bytes {
        return Err(RelayCodecError::NonCanonicalMessage);
    }
    Ok(value)
}

pub fn relay_control_signing_bytes(
    value: &RelayControlV1,
    role: RelayControlSignatureRoleV1,
) -> Result<Vec<u8>, RelayCodecError> {
    let (domain, canonical) = relay_control_signing_parts(value, role)?;
    let mut output = Vec::with_capacity(domain.len() + canonical.len());
    output.extend_from_slice(domain);
    output.extend_from_slice(&canonical);
    Ok(output)
}

/// Return the exact signature domain and canonical unsigned message separately.
/// External signers use this form so the domain is applied exactly once.
pub fn relay_control_signing_parts(
    value: &RelayControlV1,
    role: RelayControlSignatureRoleV1,
) -> Result<(&'static [u8], Vec<u8>), RelayCodecError> {
    validate(value)?;
    let domain = match (value, role) {
        (RelayControlV1::Reserve(_), RelayControlSignatureRoleV1::ReserveRequestTarget) => {
            DOMAIN_RESERVE
        }
        (RelayControlV1::Keepalive(_), RelayControlSignatureRoleV1::KeepaliveTarget) => {
            DOMAIN_KEEPALIVE
        }
        (RelayControlV1::Revoke(_), RelayControlSignatureRoleV1::RevokeActor) => DOMAIN_REVOKE,
        (RelayControlV1::Denied(_), RelayControlSignatureRoleV1::DenialRelay) => DOMAIN_DENIAL,
        (
            RelayControlV1::OuterClientChallenge(_),
            RelayControlSignatureRoleV1::OuterChallengeRelay,
        ) => DOMAIN_OUTER_CHALLENGE,
        (RelayControlV1::OuterClientHello(_), RelayControlSignatureRoleV1::OuterHelloClient) => {
            DOMAIN_OUTER_HELLO
        }
        _ => return Err(RelayCodecError::WrongSignatureRole),
    };
    Ok((domain, encode_root(value, SignatureMode::OmitRequest)?))
}

fn encode_root(value: &RelayControlV1, mode: SignatureMode) -> Result<Vec<u8>, RelayCodecError> {
    let (schema, body) = match value {
        RelayControlV1::Reserve(value) => {
            (relay_control_schema_id::RESERVE, reserve_value(value, mode))
        }
        RelayControlV1::Granted(value) => {
            (relay_control_schema_id::GRANTED, reservation_value(value))
        }
        RelayControlV1::Keepalive(value) => (
            relay_control_schema_id::KEEPALIVE,
            keepalive_value(value, mode),
        ),
        RelayControlV1::Revoke(value) => {
            (relay_control_schema_id::REVOKE, revoke_value(value, mode))
        }
        RelayControlV1::PossessionChallenge(value) => (
            relay_control_schema_id::POSSESSION_CHALLENGE,
            possession_challenge_value(value),
        ),
        RelayControlV1::PossessionProof(value) => (
            relay_control_schema_id::POSSESSION_PROOF,
            possession_proof_value(value),
        ),
        RelayControlV1::Denied(value) => {
            (relay_control_schema_id::DENIED, denial_value(value, mode))
        }
        RelayControlV1::OuterClientChallenge(value) => (
            relay_control_schema_id::OUTER_CLIENT_CHALLENGE,
            outer_challenge_value(value, mode),
        ),
        RelayControlV1::OuterClientHello(value) => (
            relay_control_schema_id::OUTER_CLIENT_HELLO,
            outer_hello_value(value, mode),
        ),
    };
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![(0, u(schema)), (1, body)]),
        ResourceProfile::ControlV1,
    )?;
    if bytes.len() > MAX_RELAY_CONTROL_BYTES {
        return Err(RelayCodecError::Limit);
    }
    Ok(bytes)
}

fn reserve_value(value: &RelayReserveRequestV1, mode: SignatureMode) -> CanonicalValue {
    let mut fields = vec![
        (0, u(value.format)),
        (1, node(value.relay_node_id)),
        (2, node(value.target_node_id)),
        (3, b(&value.reservation_id)),
        (4, transports_value(&value.transport_scope)),
        (5, u(value.sequence)),
        (6, u(value.issued_at)),
        (7, u(value.expires_at)),
        (8, b(&value.target_reservation_signature)),
    ];
    if matches!(mode, SignatureMode::All) {
        fields.push((9, b(&value.target_request_signature)));
    }
    CanonicalValue::Map(fields)
}

fn reservation_value(value: &RelayReservationV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, u(value.format)),
        (1, node(value.relay_node_id)),
        (2, node(value.target_node_id)),
        (3, b(&value.reservation_id)),
        (4, transports_value(&value.transport_scope)),
        (5, u(value.issued_at)),
        (6, u(value.expires_at)),
        (7, b(&value.target_signature)),
        (8, b(&value.relay_signature)),
    ])
}

fn keepalive_value(value: &RelayKeepaliveV1, mode: SignatureMode) -> CanonicalValue {
    signed_tail(
        vec![
            (0, u(value.format)),
            (1, node(value.relay_node_id)),
            (2, node(value.target_node_id)),
            (3, b(&value.reservation_id)),
            (4, u(value.sequence)),
            (5, u(value.issued_at)),
            (6, u(value.expires_at)),
        ],
        7,
        &value.target_signature,
        mode,
    )
}

fn revoke_value(value: &RelayRevokeV1, mode: SignatureMode) -> CanonicalValue {
    signed_tail(
        vec![
            (0, u(value.format)),
            (1, node(value.relay_node_id)),
            (2, node(value.target_node_id)),
            (3, b(&value.reservation_id)),
            (4, u(revocation_actor(value.actor))),
            (5, u(revocation_reason(value.reason))),
            (6, u(value.sequence)),
            (7, u(value.issued_at)),
            (8, u(value.expires_at)),
        ],
        9,
        &value.actor_signature,
        mode,
    )
}

fn possession_challenge_value(value: &RelayPossessionChallengeV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, node(value.relay_node_id)),
        (1, b(&value.descriptor_digest)),
        (2, u(value.endpoint_index)),
        (3, u(transport(value.transport))),
        (4, b(&value.verifier_context)),
        (5, b(&value.nonce)),
        (6, u(value.issued_at)),
        (7, u(value.expires_at)),
    ])
}

fn possession_proof_value(value: &RelayPossessionProofV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, b(&value.challenge_digest)),
        (1, b(&value.connection_binding_digest)),
        (2, b(&value.signature)),
    ])
}

fn denial_value(value: &RelayDenialV1, mode: SignatureMode) -> CanonicalValue {
    signed_tail(
        vec![
            (0, u(value.format)),
            (1, node(value.relay_node_id)),
            (2, node(value.target_node_id)),
            (3, b(&value.reservation_id)),
            (4, u(denial_code(value.code))),
            (5, u(value.retry_after)),
            (6, u(value.issued_at)),
            (7, u(value.expires_at)),
        ],
        8,
        &value.relay_signature,
        mode,
    )
}

fn outer_challenge_value(
    value: &RelayOuterClientChallengeV1,
    mode: SignatureMode,
) -> CanonicalValue {
    signed_tail(
        vec![
            (0, u(value.format)),
            (1, node(value.relay_node_id)),
            (2, b(&value.challenge_nonce)),
            (3, b(&value.outer_connection_binding)),
            (4, u(value.issued_at)),
            (5, u(value.expires_at)),
        ],
        6,
        &value.relay_signature,
        mode,
    )
}

fn outer_hello_value(value: &RelayOuterClientHelloV1, mode: SignatureMode) -> CanonicalValue {
    signed_tail(
        vec![
            (0, u(value.format)),
            (1, node(value.relay_node_id)),
            (2, node(value.client_node_id)),
            (3, b(&value.client_public_key)),
            (4, b(&value.challenge_nonce)),
            (5, b(&value.outer_connection_binding)),
            (6, u(value.issued_at)),
            (7, u(value.expires_at)),
        ],
        8,
        &value.client_signature,
        mode,
    )
}

fn signed_tail(
    mut fields: Vec<(u64, CanonicalValue)>,
    key: u64,
    signature: &[u8; 64],
    mode: SignatureMode,
) -> CanonicalValue {
    if matches!(mode, SignatureMode::All) {
        fields.push((key, b(signature)));
    }
    CanonicalValue::Map(fields)
}

fn parse_reserve(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayReserveRequestV1, RelayCodecError> {
    exact_range(fields, 9, "reserve")?;
    Ok(RelayReserveRequestV1 {
        format: unsigned(fields, 0, "reserve.format")?,
        relay_node_id: node_at(fields, 1, "reserve.relay")?,
        target_node_id: node_at(fields, 2, "reserve.target")?,
        reservation_id: bytes32(fields, 3, "reserve.id")?,
        transport_scope: parse_transports(fields, 4, "reserve.transports")?,
        sequence: unsigned(fields, 5, "reserve.sequence")?,
        issued_at: unsigned(fields, 6, "reserve.issued")?,
        expires_at: unsigned(fields, 7, "reserve.expires")?,
        target_reservation_signature: bytes64(fields, 8, "reserve.reservation-signature")?,
        target_request_signature: bytes64(fields, 9, "reserve.request-signature")?,
    })
}

fn parse_reservation(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayReservationV1, RelayCodecError> {
    exact_range(fields, 8, "reservation")?;
    Ok(RelayReservationV1 {
        format: unsigned(fields, 0, "reservation.format")?,
        relay_node_id: node_at(fields, 1, "reservation.relay")?,
        target_node_id: node_at(fields, 2, "reservation.target")?,
        reservation_id: bytes32(fields, 3, "reservation.id")?,
        transport_scope: parse_transports(fields, 4, "reservation.transports")?,
        issued_at: unsigned(fields, 5, "reservation.issued")?,
        expires_at: unsigned(fields, 6, "reservation.expires")?,
        target_signature: bytes64(fields, 7, "reservation.target-signature")?,
        relay_signature: bytes64(fields, 8, "reservation.relay-signature")?,
    })
}

fn parse_keepalive(fields: &[(u64, CanonicalValue)]) -> Result<RelayKeepaliveV1, RelayCodecError> {
    exact_range(fields, 7, "keepalive")?;
    Ok(RelayKeepaliveV1 {
        format: unsigned(fields, 0, "keepalive.format")?,
        relay_node_id: node_at(fields, 1, "keepalive.relay")?,
        target_node_id: node_at(fields, 2, "keepalive.target")?,
        reservation_id: bytes32(fields, 3, "keepalive.id")?,
        sequence: unsigned(fields, 4, "keepalive.sequence")?,
        issued_at: unsigned(fields, 5, "keepalive.issued")?,
        expires_at: unsigned(fields, 6, "keepalive.expires")?,
        target_signature: bytes64(fields, 7, "keepalive.signature")?,
    })
}

fn parse_revoke(fields: &[(u64, CanonicalValue)]) -> Result<RelayRevokeV1, RelayCodecError> {
    exact_range(fields, 9, "revoke")?;
    Ok(RelayRevokeV1 {
        format: unsigned(fields, 0, "revoke.format")?,
        relay_node_id: node_at(fields, 1, "revoke.relay")?,
        target_node_id: node_at(fields, 2, "revoke.target")?,
        reservation_id: bytes32(fields, 3, "revoke.id")?,
        actor: parse_revocation_actor(unsigned(fields, 4, "revoke.actor")?)?,
        reason: parse_revocation_reason(unsigned(fields, 5, "revoke.reason")?)?,
        sequence: unsigned(fields, 6, "revoke.sequence")?,
        issued_at: unsigned(fields, 7, "revoke.issued")?,
        expires_at: unsigned(fields, 8, "revoke.expires")?,
        actor_signature: bytes64(fields, 9, "revoke.signature")?,
    })
}

fn parse_possession_challenge(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayPossessionChallengeV1, RelayCodecError> {
    exact_range(fields, 7, "possession-challenge")?;
    Ok(RelayPossessionChallengeV1 {
        relay_node_id: node_at(fields, 0, "possession.relay")?,
        descriptor_digest: bytes32(fields, 1, "possession.descriptor")?,
        endpoint_index: unsigned(fields, 2, "possession.endpoint")?,
        transport: parse_transport(unsigned(fields, 3, "possession.transport")?)?,
        verifier_context: bytes32(fields, 4, "possession.context")?,
        nonce: bytes32(fields, 5, "possession.nonce")?,
        issued_at: unsigned(fields, 6, "possession.issued")?,
        expires_at: unsigned(fields, 7, "possession.expires")?,
    })
}

fn parse_possession_proof(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayPossessionProofV1, RelayCodecError> {
    exact_range(fields, 2, "possession-proof")?;
    Ok(RelayPossessionProofV1 {
        challenge_digest: bytes32(fields, 0, "proof.challenge")?,
        connection_binding_digest: bytes32(fields, 1, "proof.binding")?,
        signature: bytes64(fields, 2, "proof.signature")?,
    })
}

fn parse_denial(fields: &[(u64, CanonicalValue)]) -> Result<RelayDenialV1, RelayCodecError> {
    exact_range(fields, 8, "denial")?;
    Ok(RelayDenialV1 {
        format: unsigned(fields, 0, "denial.format")?,
        relay_node_id: node_at(fields, 1, "denial.relay")?,
        target_node_id: node_at(fields, 2, "denial.target")?,
        reservation_id: bytes32(fields, 3, "denial.id")?,
        code: parse_denial_code(unsigned(fields, 4, "denial.code")?)?,
        retry_after: unsigned(fields, 5, "denial.retry")?,
        issued_at: unsigned(fields, 6, "denial.issued")?,
        expires_at: unsigned(fields, 7, "denial.expires")?,
        relay_signature: bytes64(fields, 8, "denial.signature")?,
    })
}

fn parse_outer_challenge(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayOuterClientChallengeV1, RelayCodecError> {
    exact_range(fields, 6, "outer-challenge")?;
    Ok(RelayOuterClientChallengeV1 {
        format: unsigned(fields, 0, "outer-challenge.format")?,
        relay_node_id: node_at(fields, 1, "outer-challenge.relay")?,
        challenge_nonce: bytes32(fields, 2, "outer-challenge.nonce")?,
        outer_connection_binding: bytes32(fields, 3, "outer-challenge.binding")?,
        issued_at: unsigned(fields, 4, "outer-challenge.issued")?,
        expires_at: unsigned(fields, 5, "outer-challenge.expires")?,
        relay_signature: bytes64(fields, 6, "outer-challenge.signature")?,
    })
}

fn parse_outer_hello(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayOuterClientHelloV1, RelayCodecError> {
    exact_range(fields, 8, "outer-hello")?;
    Ok(RelayOuterClientHelloV1 {
        format: unsigned(fields, 0, "outer-hello.format")?,
        relay_node_id: node_at(fields, 1, "outer-hello.relay")?,
        client_node_id: node_at(fields, 2, "outer-hello.client")?,
        client_public_key: bytes32(fields, 3, "outer-hello.public-key")?,
        challenge_nonce: bytes32(fields, 4, "outer-hello.nonce")?,
        outer_connection_binding: bytes32(fields, 5, "outer-hello.binding")?,
        issued_at: unsigned(fields, 6, "outer-hello.issued")?,
        expires_at: unsigned(fields, 7, "outer-hello.expires")?,
        client_signature: bytes64(fields, 8, "outer-hello.signature")?,
    })
}

fn validate(value: &RelayControlV1) -> Result<(), RelayCodecError> {
    match value {
        RelayControlV1::Reserve(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
            sequence(value.sequence)?;
            transports(&value.transport_scope)?;
        }
        RelayControlV1::Granted(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
            transports(&value.transport_scope)?;
        }
        RelayControlV1::Keepalive(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
            sequence(value.sequence)?;
        }
        RelayControlV1::Revoke(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
            sequence(value.sequence)?;
        }
        RelayControlV1::PossessionChallenge(value) => {
            validity(value.issued_at, value.expires_at)?;
            if value.endpoint_index >= 8 {
                return Err(RelayCodecError::InvalidField("possession.endpoint"));
            }
        }
        RelayControlV1::PossessionProof(_) => {}
        RelayControlV1::Denied(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
        }
        RelayControlV1::OuterClientChallenge(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
        }
        RelayControlV1::OuterClientHello(value) => {
            base(value.format, value.issued_at, value.expires_at)?;
        }
    }
    Ok(())
}

fn base(format: u64, issued: u64, expires: u64) -> Result<(), RelayCodecError> {
    if format != 1 {
        return Err(RelayCodecError::InvalidField("format"));
    }
    validity(issued, expires)
}

fn validity(issued: u64, expires: u64) -> Result<(), RelayCodecError> {
    if issued < expires && expires - issued <= 900 {
        Ok(())
    } else {
        Err(RelayCodecError::InvalidField("validity"))
    }
}

fn sequence(value: u64) -> Result<(), RelayCodecError> {
    if value == 0 {
        Err(RelayCodecError::InvalidField("sequence"))
    } else {
        Ok(())
    }
}

fn transports(values: &[RelayTransportV1]) -> Result<(), RelayCodecError> {
    if values.is_empty() || values.len() > 2 || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        Err(RelayCodecError::InvalidField("transport-scope"))
    } else {
        Ok(())
    }
}

fn transports_value(values: &[RelayTransportV1]) -> CanonicalValue {
    CanonicalValue::Array(values.iter().map(|value| u(transport(*value))).collect())
}

fn parse_transports(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<Vec<RelayTransportV1>, RelayCodecError> {
    let CanonicalValue::Array(values) = required(fields, key, field)? else {
        return Err(RelayCodecError::InvalidField(field));
    };
    values
        .iter()
        .map(|value| match value {
            CanonicalValue::Unsigned(value) => parse_transport(*value),
            _ => Err(RelayCodecError::InvalidField(field)),
        })
        .collect()
}

fn transport(value: RelayTransportV1) -> u64 {
    match value {
        RelayTransportV1::QuicUdp => 1,
        RelayTransportV1::TlsTcp443 => 2,
    }
}
fn parse_transport(value: u64) -> Result<RelayTransportV1, RelayCodecError> {
    match value {
        1 => Ok(RelayTransportV1::QuicUdp),
        2 => Ok(RelayTransportV1::TlsTcp443),
        _ => Err(RelayCodecError::InvalidField("transport")),
    }
}
fn revocation_actor(value: RelayRevocationActorV1) -> u64 {
    match value {
        RelayRevocationActorV1::Target => 1,
        RelayRevocationActorV1::Relay => 2,
    }
}
fn parse_revocation_actor(value: u64) -> Result<RelayRevocationActorV1, RelayCodecError> {
    match value {
        1 => Ok(RelayRevocationActorV1::Target),
        2 => Ok(RelayRevocationActorV1::Relay),
        _ => Err(RelayCodecError::InvalidField("revoke.actor")),
    }
}
fn revocation_reason(value: RelayRevocationReasonV1) -> u64 {
    match value {
        RelayRevocationReasonV1::TargetClosed => 1,
        RelayRevocationReasonV1::RelayShutdown => 2,
        RelayRevocationReasonV1::CapacityReclaimed => 3,
        RelayRevocationReasonV1::PolicyRejected => 4,
        RelayRevocationReasonV1::Expired => 5,
    }
}
fn parse_revocation_reason(value: u64) -> Result<RelayRevocationReasonV1, RelayCodecError> {
    match value {
        1 => Ok(RelayRevocationReasonV1::TargetClosed),
        2 => Ok(RelayRevocationReasonV1::RelayShutdown),
        3 => Ok(RelayRevocationReasonV1::CapacityReclaimed),
        4 => Ok(RelayRevocationReasonV1::PolicyRejected),
        5 => Ok(RelayRevocationReasonV1::Expired),
        _ => Err(RelayCodecError::InvalidField("revoke.reason")),
    }
}
fn denial_code(value: RelayDenialCodeV1) -> u64 {
    match value {
        RelayDenialCodeV1::Capacity => 1,
        RelayDenialCodeV1::Policy => 2,
        RelayDenialCodeV1::InvalidTransportScope => 3,
        RelayDenialCodeV1::RateLimited => 4,
    }
}
fn parse_denial_code(value: u64) -> Result<RelayDenialCodeV1, RelayCodecError> {
    match value {
        1 => Ok(RelayDenialCodeV1::Capacity),
        2 => Ok(RelayDenialCodeV1::Policy),
        3 => Ok(RelayDenialCodeV1::InvalidTransportScope),
        4 => Ok(RelayDenialCodeV1::RateLimited),
        _ => Err(RelayCodecError::InvalidField("denial.code")),
    }
}

fn u(value: u64) -> CanonicalValue {
    CanonicalValue::Unsigned(value)
}
fn b(value: &[u8]) -> CanonicalValue {
    CanonicalValue::Bytes(value.to_vec())
}
fn node(value: NodeId) -> CanonicalValue {
    b(value.as_bytes())
}
fn map<'a>(
    value: &'a CanonicalValue,
    field: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], RelayCodecError> {
    match value {
        CanonicalValue::Map(value) => Ok(value),
        _ => Err(RelayCodecError::InvalidField(field)),
    }
}
fn exact_keys(
    fields: &[(u64, CanonicalValue)],
    keys: &[u64],
    field: &'static str,
) -> Result<(), RelayCodecError> {
    if fields.len() == keys.len() && fields.iter().map(|entry| entry.0).eq(keys.iter().copied()) {
        Ok(())
    } else {
        Err(RelayCodecError::InvalidField(field))
    }
}
fn exact_range(
    fields: &[(u64, CanonicalValue)],
    last: u64,
    field: &'static str,
) -> Result<(), RelayCodecError> {
    let keys: Vec<_> = (0..=last).collect();
    exact_keys(fields, &keys, field)
}
fn required<'a>(
    fields: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, RelayCodecError> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(RelayCodecError::InvalidField(field))
}
fn unsigned(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, RelayCodecError> {
    match required(fields, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(RelayCodecError::InvalidField(field)),
    }
}
fn bytes_n<const N: usize>(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; N], RelayCodecError> {
    match value {
        CanonicalValue::Bytes(value) if value.len() == N => {
            let mut output = [0; N];
            output.copy_from_slice(value);
            Ok(output)
        }
        _ => Err(RelayCodecError::InvalidField(field)),
    }
}
fn bytes32(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], RelayCodecError> {
    bytes_n(required(fields, key, field)?, field)
}
fn bytes64(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], RelayCodecError> {
    bytes_n(required(fields, key, field)?, field)
}
fn node_at(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<NodeId, RelayCodecError> {
    Ok(NodeId::from_bytes(bytes32(fields, key, field)?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayCodecError {
    Canonical(CanonicalError),
    UnknownSchema(u64),
    InvalidField(&'static str),
    WrongSignatureRole,
    NonCanonicalMessage,
    Limit,
}

impl From<CanonicalError> for RelayCodecError {
    fn from(value: CanonicalError) -> Self {
        Self::Canonical(value)
    }
}
impl std::fmt::Display for RelayCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RELAY_CODEC: {self:?}")
    }
}
impl std::error::Error for RelayCodecError {}
