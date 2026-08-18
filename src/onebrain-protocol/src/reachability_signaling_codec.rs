//! Closed canonical CBOR codec for target-scoped connectivity signaling.

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, NodeId, ResourceProfile,
};

use crate::{
    connectivity_schema_id, ConnectivitySignalingV1, ConnectivitySignatureRoleV1,
    HolePunchScheduleV1, HostAddressV1, PrivateCandidateSignalV1, PrivateCandidateV1,
    ReachabilityEndpointV1, ReflexiveObservationV1, RelayAssociationV1, RelayConnectRequestV1,
    HOLE_PUNCH_ATTEMPT_COUNT, HOLE_PUNCH_INTERVAL_MS, HOLE_PUNCH_START_DELAY_MS,
    MAX_CONNECTIVITY_SIGNAL_BYTES, MAX_PRIVATE_SIGNAL_CANDIDATES,
};

const DOMAIN_REFLEXIVE: &[u8] = b"onebrain/reachability/reflexive-observation/v1\0";
const DOMAIN_PUNCH: &[u8] = b"onebrain/reachability/hole-punch-schedule/v1\0";
const DOMAIN_CONNECT: &[u8] = b"onebrain/reachability/relay-connect-request/v1\0";
const DOMAIN_ASSOCIATION: &[u8] = b"onebrain/reachability/relay-association/v1\0";
const DOMAIN_PRIVATE: &[u8] = b"onebrain/reachability/private-candidate-signal/v1\0";

pub fn encode_connectivity_signaling(
    value: &ConnectivitySignalingV1,
) -> Result<Vec<u8>, ConnectivitySignalingCodecError> {
    validate(value)?;
    encode_root(value, true)
}

pub fn decode_connectivity_signaling(
    bytes: &[u8],
) -> Result<ConnectivitySignalingV1, ConnectivitySignalingCodecError> {
    if bytes.len() > MAX_CONNECTIVITY_SIGNAL_BYTES {
        return Err(ConnectivitySignalingCodecError::Limit);
    }
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value, "root")?;
    exact_keys(root, &[0, 1], "root")?;
    let schema = unsigned(root, 0, "root.schema")?;
    let body = map(required(root, 1, "root.body")?, "root.body")?;
    let object = match schema {
        connectivity_schema_id::REFLEXIVE_OBSERVATION => {
            ConnectivitySignalingV1::ReflexiveObservation(parse_reflexive(body)?)
        }
        connectivity_schema_id::HOLE_PUNCH_SCHEDULE => {
            ConnectivitySignalingV1::HolePunchSchedule(parse_punch(body)?)
        }
        connectivity_schema_id::RELAY_CONNECT_REQUEST => {
            ConnectivitySignalingV1::RelayConnectRequest(parse_connect(body)?)
        }
        connectivity_schema_id::RELAY_ASSOCIATION => {
            ConnectivitySignalingV1::RelayAssociation(parse_association(body)?)
        }
        connectivity_schema_id::PRIVATE_CANDIDATE_SIGNAL => {
            ConnectivitySignalingV1::PrivateCandidateSignal(parse_private(body)?)
        }
        other => return Err(ConnectivitySignalingCodecError::UnknownSchema(other)),
    };
    validate(&object)?;
    if encode_connectivity_signaling(&object)? != bytes {
        return Err(ConnectivitySignalingCodecError::NonCanonicalMessage);
    }
    Ok(object)
}

pub fn connectivity_signing_bytes(
    value: &ConnectivitySignalingV1,
    role: ConnectivitySignatureRoleV1,
) -> Result<Vec<u8>, ConnectivitySignalingCodecError> {
    validate(value)?;
    let domain = match (value, role) {
        (
            ConnectivitySignalingV1::ReflexiveObservation(_),
            ConnectivitySignatureRoleV1::ReflexiveRelay,
        ) => DOMAIN_REFLEXIVE,
        (
            ConnectivitySignalingV1::HolePunchSchedule(_),
            ConnectivitySignatureRoleV1::HolePunchRelay,
        ) => DOMAIN_PUNCH,
        (
            ConnectivitySignalingV1::RelayConnectRequest(_),
            ConnectivitySignatureRoleV1::RelayConnectInitiator,
        ) => DOMAIN_CONNECT,
        (
            ConnectivitySignalingV1::RelayAssociation(_),
            ConnectivitySignatureRoleV1::RelayAssociationRelay,
        ) => DOMAIN_ASSOCIATION,
        (
            ConnectivitySignalingV1::PrivateCandidateSignal(_),
            ConnectivitySignatureRoleV1::PrivateCandidateSender,
        ) => DOMAIN_PRIVATE,
        _ => return Err(ConnectivitySignalingCodecError::WrongSignatureRole),
    };
    let canonical = encode_root(value, false)?;
    let mut output = Vec::with_capacity(domain.len() + canonical.len());
    output.extend_from_slice(domain);
    output.extend_from_slice(&canonical);
    Ok(output)
}

fn encode_root(
    value: &ConnectivitySignalingV1,
    include_signature: bool,
) -> Result<Vec<u8>, ConnectivitySignalingCodecError> {
    let (schema, body) = match value {
        ConnectivitySignalingV1::ReflexiveObservation(value) => (
            connectivity_schema_id::REFLEXIVE_OBSERVATION,
            reflexive_value(value, include_signature),
        ),
        ConnectivitySignalingV1::HolePunchSchedule(value) => (
            connectivity_schema_id::HOLE_PUNCH_SCHEDULE,
            punch_value(value, include_signature),
        ),
        ConnectivitySignalingV1::RelayConnectRequest(value) => (
            connectivity_schema_id::RELAY_CONNECT_REQUEST,
            connect_value(value, include_signature),
        ),
        ConnectivitySignalingV1::RelayAssociation(value) => (
            connectivity_schema_id::RELAY_ASSOCIATION,
            association_value(value, include_signature),
        ),
        ConnectivitySignalingV1::PrivateCandidateSignal(value) => (
            connectivity_schema_id::PRIVATE_CANDIDATE_SIGNAL,
            private_value(value, include_signature),
        ),
    };
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![(0, u(schema)), (1, body)]),
        ResourceProfile::ControlV1,
    )?;
    if bytes.len() > MAX_CONNECTIVITY_SIGNAL_BYTES {
        return Err(ConnectivitySignalingCodecError::Limit);
    }
    Ok(bytes)
}

fn reflexive_value(value: &ReflexiveObservationV1, signature: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, u(value.format)),
        (1, node(value.relay_node_id)),
        (2, node(value.target_node_id)),
        (3, b(&value.reservation_id)),
        (4, endpoint_value(&value.observed_endpoint)),
        (5, u(value.network_epoch)),
        (6, u(value.sequence)),
        (7, u(value.issued_at)),
        (8, u(value.expires_at)),
    ];
    if signature {
        fields.push((9, b(&value.relay_signature)));
    }
    CanonicalValue::Map(fields)
}

fn punch_value(value: &HolePunchScheduleV1, signature: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, u(value.format)),
        (1, node(value.relay_node_id)),
        (2, node(value.initiator_node_id)),
        (3, node(value.responder_node_id)),
        (4, b(&value.initiator_reservation_id)),
        (5, b(&value.responder_reservation_id)),
        (6, b(&value.rendezvous_token)),
        (7, b(&value.association_barrier_digest)),
        (8, u(value.start_delay_ms)),
        (9, u(value.interval_ms)),
        (10, u(value.attempt_count)),
        (11, u(value.expires_at)),
    ];
    if signature {
        fields.push((12, b(&value.relay_signature)));
    }
    CanonicalValue::Map(fields)
}

fn connect_value(value: &RelayConnectRequestV1, signature: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, u(value.format)),
        (1, node(value.initiator_node_id)),
        (2, node(value.target_node_id)),
        (3, b(&value.initiator_reservation_id)),
        (4, b(&value.target_reservation_id)),
        (5, b(&value.nonce)),
        (6, u(value.sequence)),
        (7, u(value.issued_at)),
        (8, u(value.expires_at)),
    ];
    if signature {
        fields.push((9, b(&value.initiator_signature)));
    }
    CanonicalValue::Map(fields)
}

fn association_value(value: &RelayAssociationV1, signature: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, u(value.format)),
        (1, node(value.relay_node_id)),
        (2, node(value.initiator_node_id)),
        (3, node(value.target_node_id)),
        (4, b(&value.initiator_reservation_id)),
        (5, b(&value.target_reservation_id)),
        (6, b(&value.association_id)),
        (7, u(value.issued_at)),
        (8, u(value.expires_at)),
    ];
    if signature {
        fields.push((9, b(&value.relay_signature)));
    }
    CanonicalValue::Map(fields)
}

fn private_value(value: &PrivateCandidateSignalV1, signature: bool) -> CanonicalValue {
    let mut fields = vec![
        (0, u(value.format)),
        (1, node(value.sender_node_id)),
        (2, node(value.target_node_id)),
        (3, b(&value.session_id)),
        (4, u(value.network_epoch)),
        (
            5,
            CanonicalValue::Array(
                value
                    .candidates
                    .iter()
                    .map(private_candidate_value)
                    .collect(),
            ),
        ),
        (6, u(value.sequence)),
        (7, u(value.issued_at)),
        (8, u(value.expires_at)),
    ];
    if signature {
        fields.push((9, b(&value.sender_signature)));
    }
    CanonicalValue::Map(fields)
}

fn parse_reflexive(
    fields: &[(u64, CanonicalValue)],
) -> Result<ReflexiveObservationV1, ConnectivitySignalingCodecError> {
    exact_keys(fields, &(0..=9).collect::<Vec<_>>(), "reflexive")?;
    Ok(ReflexiveObservationV1 {
        format: unsigned(fields, 0, "reflexive.format")?,
        relay_node_id: node_at(fields, 1, "reflexive.relay")?,
        target_node_id: node_at(fields, 2, "reflexive.target")?,
        reservation_id: bytes32(fields, 3, "reflexive.reservation")?,
        observed_endpoint: parse_endpoint(required(fields, 4, "reflexive.endpoint")?)?,
        network_epoch: unsigned(fields, 5, "reflexive.epoch")?,
        sequence: unsigned(fields, 6, "reflexive.sequence")?,
        issued_at: unsigned(fields, 7, "reflexive.issued")?,
        expires_at: unsigned(fields, 8, "reflexive.expires")?,
        relay_signature: bytes64(fields, 9, "reflexive.signature")?,
    })
}

fn parse_punch(
    fields: &[(u64, CanonicalValue)],
) -> Result<HolePunchScheduleV1, ConnectivitySignalingCodecError> {
    exact_keys(fields, &(0..=12).collect::<Vec<_>>(), "punch")?;
    Ok(HolePunchScheduleV1 {
        format: unsigned(fields, 0, "punch.format")?,
        relay_node_id: node_at(fields, 1, "punch.relay")?,
        initiator_node_id: node_at(fields, 2, "punch.initiator")?,
        responder_node_id: node_at(fields, 3, "punch.responder")?,
        initiator_reservation_id: bytes32(fields, 4, "punch.initiator-reservation")?,
        responder_reservation_id: bytes32(fields, 5, "punch.responder-reservation")?,
        rendezvous_token: bytes32(fields, 6, "punch.token")?,
        association_barrier_digest: bytes32(fields, 7, "punch.barrier")?,
        start_delay_ms: unsigned(fields, 8, "punch.delay")?,
        interval_ms: unsigned(fields, 9, "punch.interval")?,
        attempt_count: unsigned(fields, 10, "punch.attempts")?,
        expires_at: unsigned(fields, 11, "punch.expires")?,
        relay_signature: bytes64(fields, 12, "punch.signature")?,
    })
}

fn parse_connect(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayConnectRequestV1, ConnectivitySignalingCodecError> {
    exact_keys(fields, &(0..=9).collect::<Vec<_>>(), "connect")?;
    Ok(RelayConnectRequestV1 {
        format: unsigned(fields, 0, "connect.format")?,
        initiator_node_id: node_at(fields, 1, "connect.initiator")?,
        target_node_id: node_at(fields, 2, "connect.target")?,
        initiator_reservation_id: bytes32(fields, 3, "connect.initiator-reservation")?,
        target_reservation_id: bytes32(fields, 4, "connect.target-reservation")?,
        nonce: bytes32(fields, 5, "connect.nonce")?,
        sequence: unsigned(fields, 6, "connect.sequence")?,
        issued_at: unsigned(fields, 7, "connect.issued")?,
        expires_at: unsigned(fields, 8, "connect.expires")?,
        initiator_signature: bytes64(fields, 9, "connect.signature")?,
    })
}

fn parse_association(
    fields: &[(u64, CanonicalValue)],
) -> Result<RelayAssociationV1, ConnectivitySignalingCodecError> {
    exact_keys(fields, &(0..=9).collect::<Vec<_>>(), "association")?;
    Ok(RelayAssociationV1 {
        format: unsigned(fields, 0, "association.format")?,
        relay_node_id: node_at(fields, 1, "association.relay")?,
        initiator_node_id: node_at(fields, 2, "association.initiator")?,
        target_node_id: node_at(fields, 3, "association.target")?,
        initiator_reservation_id: bytes32(fields, 4, "association.initiator-reservation")?,
        target_reservation_id: bytes32(fields, 5, "association.target-reservation")?,
        association_id: bytes32(fields, 6, "association.id")?,
        issued_at: unsigned(fields, 7, "association.issued")?,
        expires_at: unsigned(fields, 8, "association.expires")?,
        relay_signature: bytes64(fields, 9, "association.signature")?,
    })
}

fn parse_private(
    fields: &[(u64, CanonicalValue)],
) -> Result<PrivateCandidateSignalV1, ConnectivitySignalingCodecError> {
    exact_keys(fields, &(0..=9).collect::<Vec<_>>(), "private")?;
    let values = array(fields, 5, "private.candidates")?;
    if values.is_empty() || values.len() > MAX_PRIVATE_SIGNAL_CANDIDATES {
        return Err(ConnectivitySignalingCodecError::Limit);
    }
    Ok(PrivateCandidateSignalV1 {
        format: unsigned(fields, 0, "private.format")?,
        sender_node_id: node_at(fields, 1, "private.sender")?,
        target_node_id: node_at(fields, 2, "private.target")?,
        session_id: bytes32(fields, 3, "private.session")?,
        network_epoch: unsigned(fields, 4, "private.epoch")?,
        candidates: values
            .iter()
            .map(parse_private_candidate)
            .collect::<Result<Vec<_>, _>>()?,
        sequence: unsigned(fields, 6, "private.sequence")?,
        issued_at: unsigned(fields, 7, "private.issued")?,
        expires_at: unsigned(fields, 8, "private.expires")?,
        sender_signature: bytes64(fields, 9, "private.signature")?,
    })
}

fn validate(value: &ConnectivitySignalingV1) -> Result<(), ConnectivitySignalingCodecError> {
    match value {
        ConnectivitySignalingV1::ReflexiveObservation(value) => {
            format(value.format)?;
            validity(value.issued_at, value.expires_at, 300)?;
            endpoint(&value.observed_endpoint)?;
            nonzero(value.sequence, "reflexive.sequence")?;
        }
        ConnectivitySignalingV1::HolePunchSchedule(value) => {
            format(value.format)?;
            if value.start_delay_ms != HOLE_PUNCH_START_DELAY_MS
                || value.interval_ms != HOLE_PUNCH_INTERVAL_MS
                || value.attempt_count != HOLE_PUNCH_ATTEMPT_COUNT
                || value.expires_at == 0
            {
                return Err(ConnectivitySignalingCodecError::InvalidField(
                    "punch.schedule",
                ));
            }
        }
        ConnectivitySignalingV1::RelayConnectRequest(value) => {
            format(value.format)?;
            validity(value.issued_at, value.expires_at, 300)?;
            nonzero(value.sequence, "connect.sequence")?;
        }
        ConnectivitySignalingV1::RelayAssociation(value) => {
            format(value.format)?;
            validity(value.issued_at, value.expires_at, 300)?;
        }
        ConnectivitySignalingV1::PrivateCandidateSignal(value) => {
            format(value.format)?;
            validity(value.issued_at, value.expires_at, 300)?;
            nonzero(value.sequence, "private.sequence")?;
            if value.candidates.is_empty() || value.candidates.len() > MAX_PRIVATE_SIGNAL_CANDIDATES
            {
                return Err(ConnectivitySignalingCodecError::Limit);
            }
            for candidate in &value.candidates {
                endpoint(&candidate.endpoint)?;
            }
        }
    }
    Ok(())
}

fn private_candidate_value(value: &PrivateCandidateV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, endpoint_value(&value.endpoint)),
        (1, u(value.priority.into())),
        (2, b(&value.foundation)),
    ])
}

fn parse_private_candidate(
    value: &CanonicalValue,
) -> Result<PrivateCandidateV1, ConnectivitySignalingCodecError> {
    let fields = map(value, "private-candidate")?;
    exact_keys(fields, &[0, 1, 2], "private-candidate")?;
    Ok(PrivateCandidateV1 {
        endpoint: parse_endpoint(required(fields, 0, "private-candidate.endpoint")?)?,
        priority: u32::try_from(unsigned(fields, 1, "private-candidate.priority")?).map_err(
            |_| ConnectivitySignalingCodecError::InvalidField("private-candidate.priority"),
        )?,
        foundation: bytes16(fields, 2, "private-candidate.foundation")?,
    })
}

fn endpoint_value(value: &ReachabilityEndpointV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, host_value(&value.host)),
        (1, u(value.port.into())),
    ])
}

fn parse_endpoint(
    value: &CanonicalValue,
) -> Result<ReachabilityEndpointV1, ConnectivitySignalingCodecError> {
    let fields = map(value, "endpoint")?;
    exact_keys(fields, &[0, 1], "endpoint")?;
    Ok(ReachabilityEndpointV1 {
        host: parse_host(required(fields, 0, "endpoint.host")?)?,
        port: u16::try_from(unsigned(fields, 1, "endpoint.port")?)
            .map_err(|_| ConnectivitySignalingCodecError::InvalidField("endpoint.port"))?,
    })
}

fn host_value(value: &HostAddressV1) -> CanonicalValue {
    match value {
        HostAddressV1::Ipv4(value) => CanonicalValue::Map(vec![(0, u(1)), (1, b(value))]),
        HostAddressV1::Ipv6(value) => CanonicalValue::Map(vec![(0, u(2)), (1, b(value))]),
        HostAddressV1::Dns(value) => {
            CanonicalValue::Map(vec![(0, u(3)), (1, CanonicalValue::Text(value.clone()))])
        }
    }
}

fn parse_host(value: &CanonicalValue) -> Result<HostAddressV1, ConnectivitySignalingCodecError> {
    let fields = map(value, "host")?;
    exact_keys(fields, &[0, 1], "host")?;
    match unsigned(fields, 0, "host.kind")? {
        1 => Ok(HostAddressV1::Ipv4(bytes_n(
            required(fields, 1, "host.ipv4")?,
            "host.ipv4",
        )?)),
        2 => Ok(HostAddressV1::Ipv6(bytes_n(
            required(fields, 1, "host.ipv6")?,
            "host.ipv6",
        )?)),
        3 => match required(fields, 1, "host.dns")? {
            CanonicalValue::Text(value) => Ok(HostAddressV1::Dns(value.clone())),
            _ => Err(ConnectivitySignalingCodecError::InvalidField("host.dns")),
        },
        _ => Err(ConnectivitySignalingCodecError::InvalidField("host.kind")),
    }
}

fn endpoint(value: &ReachabilityEndpointV1) -> Result<(), ConnectivitySignalingCodecError> {
    if value.port == 0 {
        return Err(ConnectivitySignalingCodecError::InvalidField(
            "endpoint.port",
        ));
    }
    match &value.host {
        HostAddressV1::Dns(name) if name.is_empty() || name.len() > 253 || !name.is_ascii() => Err(
            ConnectivitySignalingCodecError::InvalidField("endpoint.dns"),
        ),
        _ => Ok(()),
    }
}

fn format(value: u64) -> Result<(), ConnectivitySignalingCodecError> {
    if value == 1 {
        Ok(())
    } else {
        Err(ConnectivitySignalingCodecError::InvalidField("format"))
    }
}

fn validity(
    issued: u64,
    expires: u64,
    maximum: u64,
) -> Result<(), ConnectivitySignalingCodecError> {
    if issued < expires && expires - issued <= maximum {
        Ok(())
    } else {
        Err(ConnectivitySignalingCodecError::InvalidField("validity"))
    }
}

fn nonzero(value: u64, field: &'static str) -> Result<(), ConnectivitySignalingCodecError> {
    if value == 0 {
        Err(ConnectivitySignalingCodecError::InvalidField(field))
    } else {
        Ok(())
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
) -> Result<&'a [(u64, CanonicalValue)], ConnectivitySignalingCodecError> {
    match value {
        CanonicalValue::Map(value) => Ok(value),
        _ => Err(ConnectivitySignalingCodecError::InvalidField(field)),
    }
}
fn exact_keys(
    fields: &[(u64, CanonicalValue)],
    keys: &[u64],
    field: &'static str,
) -> Result<(), ConnectivitySignalingCodecError> {
    if fields.len() == keys.len() && fields.iter().map(|entry| entry.0).eq(keys.iter().copied()) {
        Ok(())
    } else {
        Err(ConnectivitySignalingCodecError::InvalidField(field))
    }
}
fn required<'a>(
    fields: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a CanonicalValue, ConnectivitySignalingCodecError> {
    fields
        .iter()
        .find_map(|(candidate, value)| (*candidate == key).then_some(value))
        .ok_or(ConnectivitySignalingCodecError::InvalidField(field))
}
fn unsigned(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<u64, ConnectivitySignalingCodecError> {
    match required(fields, key, field)? {
        CanonicalValue::Unsigned(value) => Ok(*value),
        _ => Err(ConnectivitySignalingCodecError::InvalidField(field)),
    }
}
fn array<'a>(
    fields: &'a [(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<&'a [CanonicalValue], ConnectivitySignalingCodecError> {
    match required(fields, key, field)? {
        CanonicalValue::Array(value) => Ok(value),
        _ => Err(ConnectivitySignalingCodecError::InvalidField(field)),
    }
}
fn bytes_n<const N: usize>(
    value: &CanonicalValue,
    field: &'static str,
) -> Result<[u8; N], ConnectivitySignalingCodecError> {
    match value {
        CanonicalValue::Bytes(value) if value.len() == N => {
            let mut output = [0; N];
            output.copy_from_slice(value);
            Ok(output)
        }
        _ => Err(ConnectivitySignalingCodecError::InvalidField(field)),
    }
}
fn bytes16(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 16], ConnectivitySignalingCodecError> {
    bytes_n(required(fields, key, field)?, field)
}
fn bytes32(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 32], ConnectivitySignalingCodecError> {
    bytes_n(required(fields, key, field)?, field)
}
fn bytes64(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<[u8; 64], ConnectivitySignalingCodecError> {
    bytes_n(required(fields, key, field)?, field)
}
fn node_at(
    fields: &[(u64, CanonicalValue)],
    key: u64,
    field: &'static str,
) -> Result<NodeId, ConnectivitySignalingCodecError> {
    Ok(NodeId::from_bytes(bytes32(fields, key, field)?))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectivitySignalingCodecError {
    Canonical(CanonicalError),
    UnknownSchema(u64),
    InvalidField(&'static str),
    WrongSignatureRole,
    NonCanonicalMessage,
    Limit,
}

impl From<CanonicalError> for ConnectivitySignalingCodecError {
    fn from(value: CanonicalError) -> Self {
        Self::Canonical(value)
    }
}

impl std::fmt::Display for ConnectivitySignalingCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_CONNECTIVITY_SIGNALING_CODEC: {self:?}")
    }
}

impl std::error::Error for ConnectivitySignalingCodecError {}
