//! Closed canonical CBOR codec for portable reachability roots.

use std::collections::BTreeSet;
use std::net::{Ipv4Addr, Ipv6Addr};

use ku_core::foundation::{
    decode_canonical, encode_canonical, CanonicalError, CanonicalValue, NodeId, ResourceProfile,
};

use crate::reachability_types::*;

const DOMAIN_BOOTSTRAP: &[u8] = b"onebrain/reachability/bootstrap-manifest/v1\0";
const DOMAIN_DESCRIPTOR: &[u8] = b"onebrain/reachability/relay-descriptor/v1\0";
const DOMAIN_RESERVATION_TARGET: &[u8] = b"onebrain/reachability/relay-reservation-target/v1\0";
const DOMAIN_RESERVATION_RELAY: &[u8] = b"onebrain/reachability/relay-reservation-relay/v1\0";
const DOMAIN_ADVERTISEMENT: &[u8] = b"onebrain/reachability/advertisement/v1\0";
const DOMAIN_RECEIPT: &[u8] = b"onebrain/reachability/route-receipt/v1\0";

pub fn encode_reachability_object(
    value: &ReachabilityObjectV1,
) -> Result<Vec<u8>, ReachabilityCodecError> {
    validate(value)?;
    encode_root(value, SignatureMode::All)
}

pub fn decode_reachability_object(
    bytes: &[u8],
) -> Result<ReachabilityObjectV1, ReachabilityCodecError> {
    if bytes.len() > 65_536 {
        return Err(ReachabilityCodecError::Limit);
    }
    let value = decode_canonical(bytes, ResourceProfile::ControlV1)?;
    let root = map(&value, "root")?;
    exact_keys(root, &[0, 1], "root")?;
    let schema = unsigned(root, 0, "root.schema")?;
    let body = map(required(root, 1, "root.body")?, "root.body")?;
    let object = match schema {
        reachability_schema_id::BOOTSTRAP_MANIFEST => {
            ReachabilityObjectV1::BootstrapManifest(parse_bootstrap(body)?)
        }
        reachability_schema_id::RELAY_DESCRIPTOR => {
            ReachabilityObjectV1::RelayDescriptor(parse_descriptor(body)?)
        }
        reachability_schema_id::RELAY_RESERVATION => {
            ReachabilityObjectV1::RelayReservation(parse_reservation(body)?)
        }
        reachability_schema_id::REACHABILITY_ADVERTISEMENT => {
            ReachabilityObjectV1::Advertisement(parse_advertisement(body)?)
        }
        reachability_schema_id::ROUTE_PLAN => {
            ReachabilityObjectV1::RoutePlan(parse_route_plan(body)?)
        }
        reachability_schema_id::ROUTE_RECEIPT => {
            ReachabilityObjectV1::RouteReceipt(parse_route_receipt(body)?)
        }
        value => return Err(ReachabilityCodecError::UnknownSchema(value)),
    };
    validate(&object)?;
    if encode_reachability_object(&object)? != bytes {
        return Err(ReachabilityCodecError::NonCanonicalMessage);
    }
    Ok(object)
}

pub fn reachability_signing_bytes(
    value: &ReachabilityObjectV1,
    role: ReachabilitySignatureRoleV1,
) -> Result<Vec<u8>, ReachabilityCodecError> {
    let (domain, canonical) = reachability_signing_parts(value, role)?;
    let mut output = Vec::with_capacity(domain.len() + canonical.len());
    output.extend_from_slice(domain);
    output.extend_from_slice(&canonical);
    Ok(output)
}

/// Returns the fixed signature domain separately from the canonical unsigned
/// message so an external identity signer can preserve domain separation.
pub fn reachability_signing_parts(
    value: &ReachabilityObjectV1,
    role: ReachabilitySignatureRoleV1,
) -> Result<(&'static [u8], Vec<u8>), ReachabilityCodecError> {
    validate(value)?;
    let (expected, domain, mode) = match (value, role) {
        (
            ReachabilityObjectV1::BootstrapManifest(_),
            ReachabilitySignatureRoleV1::BootstrapSource,
        ) => (true, DOMAIN_BOOTSTRAP, SignatureMode::OmitPrimary),
        (
            ReachabilityObjectV1::RelayDescriptor(_),
            ReachabilitySignatureRoleV1::RelayDescriptor,
        ) => (true, DOMAIN_DESCRIPTOR, SignatureMode::OmitPrimary),
        (
            ReachabilityObjectV1::RelayReservation(_),
            ReachabilitySignatureRoleV1::ReservationTarget,
        ) => (true, DOMAIN_RESERVATION_TARGET, SignatureMode::OmitBoth),
        (
            ReachabilityObjectV1::RelayReservation(_),
            ReachabilitySignatureRoleV1::ReservationRelay,
        ) => (true, DOMAIN_RESERVATION_RELAY, SignatureMode::OmitSecondary),
        (
            ReachabilityObjectV1::Advertisement(_),
            ReachabilitySignatureRoleV1::AdvertisementTarget,
        ) => (true, DOMAIN_ADVERTISEMENT, SignatureMode::OmitPrimary),
        (ReachabilityObjectV1::RouteReceipt(_), ReachabilitySignatureRoleV1::RouteReceiptLocal) => {
            (true, DOMAIN_RECEIPT, SignatureMode::OmitPrimary)
        }
        _ => (false, &b""[..], SignatureMode::All),
    };
    if !expected {
        return Err(ReachabilityCodecError::WrongSignatureRole);
    }
    let canonical = encode_root(value, mode)?;
    Ok((domain, canonical))
}

#[derive(Clone, Copy)]
enum SignatureMode {
    All,
    OmitPrimary,
    OmitSecondary,
    OmitBoth,
}

fn encode_root(
    value: &ReachabilityObjectV1,
    mode: SignatureMode,
) -> Result<Vec<u8>, ReachabilityCodecError> {
    let (schema, body) = match value {
        ReachabilityObjectV1::BootstrapManifest(v) => (
            reachability_schema_id::BOOTSTRAP_MANIFEST,
            bootstrap_value(v, mode),
        ),
        ReachabilityObjectV1::RelayDescriptor(v) => (
            reachability_schema_id::RELAY_DESCRIPTOR,
            descriptor_value(v, mode),
        ),
        ReachabilityObjectV1::RelayReservation(v) => (
            reachability_schema_id::RELAY_RESERVATION,
            reservation_value(v, mode),
        ),
        ReachabilityObjectV1::Advertisement(v) => (
            reachability_schema_id::REACHABILITY_ADVERTISEMENT,
            advertisement_value(v, mode),
        ),
        ReachabilityObjectV1::RoutePlan(v) => {
            (reachability_schema_id::ROUTE_PLAN, route_plan_value(v))
        }
        ReachabilityObjectV1::RouteReceipt(v) => (
            reachability_schema_id::ROUTE_RECEIPT,
            route_receipt_value(v, mode),
        ),
    };
    let bytes = encode_canonical(
        &CanonicalValue::Map(vec![(0, CanonicalValue::Unsigned(schema)), (1, body)]),
        ResourceProfile::ControlV1,
    )?;
    if bytes.len() > object_byte_limit(value) {
        return Err(ReachabilityCodecError::Limit);
    }
    Ok(bytes)
}

fn bootstrap_value(v: &BootstrapManifestV1, mode: SignatureMode) -> CanonicalValue {
    let mut f = vec![
        (0, u(v.format)),
        (1, b(&v.discovery_source_id)),
        (
            2,
            arr(v.discovery_endpoints.iter().map(discovery_endpoint_value)),
        ),
        (3, arr(v.protocol_versions.iter().map(version_value))),
        (4, u(v.sequence)),
        (5, u(v.issued_at)),
        (6, u(v.expires_at)),
    ];
    if matches!(mode, SignatureMode::All) {
        f.push((7, b(&v.source_signature)));
    }
    CanonicalValue::Map(f)
}

fn descriptor_value(v: &RelayDescriptorV1, mode: SignatureMode) -> CanonicalValue {
    let mut f = vec![
        (0, u(v.format)),
        (1, node(v.relay_node_id)),
        (2, b(&v.relay_public_key)),
        (3, arr(v.endpoints.iter().map(relay_endpoint_value))),
        (
            4,
            arr(v
                .supported_transports
                .iter()
                .map(|x| u(relay_transport(*x)))),
        ),
        (5, arr(v.protocol_versions.iter().map(version_value))),
        (6, b(&v.capacity_policy_digest)),
        (7, opt32(v.previous_descriptor_blake3)),
        (8, u(v.sequence)),
        (9, u(v.issued_at)),
        (10, u(v.expires_at)),
    ];
    if matches!(mode, SignatureMode::All) {
        f.push((11, b(&v.relay_signature)));
    }
    CanonicalValue::Map(f)
}

fn reservation_value(v: &RelayReservationV1, mode: SignatureMode) -> CanonicalValue {
    let mut f = vec![
        (0, u(v.format)),
        (1, node(v.relay_node_id)),
        (2, node(v.target_node_id)),
        (3, b(&v.reservation_id)),
        (
            4,
            arr(v.transport_scope.iter().map(|x| u(relay_transport(*x)))),
        ),
        (5, u(v.issued_at)),
        (6, u(v.expires_at)),
    ];
    if matches!(mode, SignatureMode::All | SignatureMode::OmitSecondary) {
        f.push((7, b(&v.target_signature)));
    }
    if matches!(mode, SignatureMode::All) {
        f.push((8, b(&v.relay_signature)));
    }
    CanonicalValue::Map(f)
}

fn advertisement_value(v: &ReachabilityAdvertisementV1, mode: SignatureMode) -> CanonicalValue {
    let mut f = vec![
        (0, u(v.format)),
        (1, node(v.target_node_id)),
        (
            2,
            arr(v
                .relay_reservations
                .iter()
                .map(|x| reservation_value(x, SignatureMode::All))),
        ),
        (
            3,
            arr(v
                .optional_public_candidates
                .iter()
                .map(public_candidate_value)),
        ),
        (4, b(&v.capability_ceiling)),
        (5, u(v.sequence)),
        (6, u(v.issued_at)),
        (7, u(v.expires_at)),
    ];
    if matches!(mode, SignatureMode::All) {
        f.push((8, b(&v.target_signature)));
    }
    CanonicalValue::Map(f)
}

fn route_plan_value(v: &RoutePlanV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, node(v.expected_peer)),
        (
            1,
            arr(v.direct_candidates.iter().map(direct_candidate_value)),
        ),
        (2, arr(v.relay_candidates.iter().map(relay_candidate_value))),
        (3, u(v.deadline)),
        (4, u(v.attempt_budget)),
        (
            5,
            CanonicalValue::Map(vec![
                (0, u(v.resource_budget.max_concurrent_checks)),
                (1, u(v.resource_budget.max_signature_checks)),
                (2, u(v.resource_budget.max_probe_bytes)),
            ]),
        ),
        (6, b(&v.privacy_policy_digest)),
    ])
}

fn route_receipt_value(v: &RouteReceiptV1, mode: SignatureMode) -> CanonicalValue {
    let mut f = vec![
        (0, node(v.expected_peer)),
        (1, opt_node(v.authenticated_peer)),
        (2, opt_u(v.selected_path_kind.map(path_kind))),
        (3, opt_node(v.selected_carrier_identity)),
        (4, arr(v.attempts.iter().map(route_attempt_value))),
        (5, opt32(v.transport_binding_digest)),
        (6, opt32(v.session_id)),
        (7, u(v.started_at)),
        (8, opt_u(v.authenticated_at)),
        (9, terminal_value(v.terminal_outcome)),
        (
            10,
            arr(v.limitations.iter().map(|x| {
                CanonicalValue::Map(vec![(0, u(limitation_code(x.code))), (1, u(x.count))])
            })),
        ),
    ];
    if matches!(mode, SignatureMode::All) {
        f.push((11, b(&v.local_signature)));
    }
    CanonicalValue::Map(f)
}

fn parse_bootstrap(
    m: &[(u64, CanonicalValue)],
) -> Result<BootstrapManifestV1, ReachabilityCodecError> {
    exact_keys(m, &[0, 1, 2, 3, 4, 5, 6, 7], "bootstrap")?;
    Ok(BootstrapManifestV1 {
        format: unsigned(m, 0, "bootstrap.format")?,
        discovery_source_id: bytes32(m, 1, "bootstrap.source")?,
        discovery_endpoints: array(m, 2, "bootstrap.endpoints")?
            .iter()
            .map(parse_discovery_endpoint)
            .collect::<Result<_, _>>()?,
        protocol_versions: array(m, 3, "bootstrap.versions")?
            .iter()
            .map(parse_version)
            .collect::<Result<_, _>>()?,
        sequence: unsigned(m, 4, "bootstrap.sequence")?,
        issued_at: unsigned(m, 5, "bootstrap.issued")?,
        expires_at: unsigned(m, 6, "bootstrap.expires")?,
        source_signature: bytes64(m, 7, "bootstrap.signature")?,
    })
}
fn parse_descriptor(
    m: &[(u64, CanonicalValue)],
) -> Result<RelayDescriptorV1, ReachabilityCodecError> {
    exact_keys(m, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], "descriptor")?;
    Ok(RelayDescriptorV1 {
        format: unsigned(m, 0, "descriptor.format")?,
        relay_node_id: node_at(m, 1, "descriptor.node")?,
        relay_public_key: bytes32(m, 2, "descriptor.key")?,
        endpoints: array(m, 3, "descriptor.endpoints")?
            .iter()
            .map(parse_relay_endpoint)
            .collect::<Result<_, _>>()?,
        supported_transports: array(m, 4, "descriptor.transports")?
            .iter()
            .map(parse_relay_transport_value)
            .collect::<Result<_, _>>()?,
        protocol_versions: array(m, 5, "descriptor.versions")?
            .iter()
            .map(parse_version)
            .collect::<Result<_, _>>()?,
        capacity_policy_digest: bytes32(m, 6, "descriptor.capacity")?,
        previous_descriptor_blake3: optional32(
            required(m, 7, "descriptor.previous")?,
            "descriptor.previous",
        )?,
        sequence: unsigned(m, 8, "descriptor.sequence")?,
        issued_at: unsigned(m, 9, "descriptor.issued")?,
        expires_at: unsigned(m, 10, "descriptor.expires")?,
        relay_signature: bytes64(m, 11, "descriptor.signature")?,
    })
}
fn parse_reservation(
    m: &[(u64, CanonicalValue)],
) -> Result<RelayReservationV1, ReachabilityCodecError> {
    exact_keys(m, &[0, 1, 2, 3, 4, 5, 6, 7, 8], "reservation")?;
    Ok(RelayReservationV1 {
        format: unsigned(m, 0, "reservation.format")?,
        relay_node_id: node_at(m, 1, "reservation.relay")?,
        target_node_id: node_at(m, 2, "reservation.target")?,
        reservation_id: bytes32(m, 3, "reservation.id")?,
        transport_scope: array(m, 4, "reservation.transports")?
            .iter()
            .map(parse_relay_transport_value)
            .collect::<Result<_, _>>()?,
        issued_at: unsigned(m, 5, "reservation.issued")?,
        expires_at: unsigned(m, 6, "reservation.expires")?,
        target_signature: bytes64(m, 7, "reservation.target_signature")?,
        relay_signature: bytes64(m, 8, "reservation.relay_signature")?,
    })
}
fn parse_advertisement(
    m: &[(u64, CanonicalValue)],
) -> Result<ReachabilityAdvertisementV1, ReachabilityCodecError> {
    exact_keys(m, &[0, 1, 2, 3, 4, 5, 6, 7, 8], "advertisement")?;
    Ok(ReachabilityAdvertisementV1 {
        format: unsigned(m, 0, "advertisement.format")?,
        target_node_id: node_at(m, 1, "advertisement.target")?,
        relay_reservations: array(m, 2, "advertisement.reservations")?
            .iter()
            .map(|v| parse_reservation(map(v, "advertisement.reservation")?))
            .collect::<Result<_, _>>()?,
        optional_public_candidates: array(m, 3, "advertisement.candidates")?
            .iter()
            .map(parse_public_candidate)
            .collect::<Result<_, _>>()?,
        capability_ceiling: bytes32(m, 4, "advertisement.capability")?,
        sequence: unsigned(m, 5, "advertisement.sequence")?,
        issued_at: unsigned(m, 6, "advertisement.issued")?,
        expires_at: unsigned(m, 7, "advertisement.expires")?,
        target_signature: bytes64(m, 8, "advertisement.signature")?,
    })
}
fn parse_route_plan(m: &[(u64, CanonicalValue)]) -> Result<RoutePlanV1, ReachabilityCodecError> {
    exact_keys(m, &[0, 1, 2, 3, 4, 5, 6], "plan")?;
    let budget = map(required(m, 5, "plan.budget")?, "plan.budget")?;
    exact_keys(budget, &[0, 1, 2], "plan.budget")?;
    Ok(RoutePlanV1 {
        expected_peer: node_at(m, 0, "plan.peer")?,
        direct_candidates: array(m, 1, "plan.direct")?
            .iter()
            .map(parse_direct_candidate)
            .collect::<Result<_, _>>()?,
        relay_candidates: array(m, 2, "plan.relay")?
            .iter()
            .map(parse_relay_candidate)
            .collect::<Result<_, _>>()?,
        deadline: unsigned(m, 3, "plan.deadline")?,
        attempt_budget: unsigned(m, 4, "plan.attempt_budget")?,
        resource_budget: RouteResourceBudgetV1 {
            max_concurrent_checks: unsigned(budget, 0, "budget.concurrent")?,
            max_signature_checks: unsigned(budget, 1, "budget.signatures")?,
            max_probe_bytes: unsigned(budget, 2, "budget.probes")?,
        },
        privacy_policy_digest: bytes32(m, 6, "plan.privacy")?,
    })
}
fn parse_route_receipt(
    m: &[(u64, CanonicalValue)],
) -> Result<RouteReceiptV1, ReachabilityCodecError> {
    exact_keys(m, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], "receipt")?;
    Ok(RouteReceiptV1 {
        expected_peer: node_at(m, 0, "receipt.peer")?,
        authenticated_peer: optional_node(
            required(m, 1, "receipt.auth_peer")?,
            "receipt.auth_peer",
        )?,
        selected_path_kind: optional_enum(required(m, 2, "receipt.path")?, parse_path_kind)?,
        selected_carrier_identity: optional_node(
            required(m, 3, "receipt.carrier")?,
            "receipt.carrier",
        )?,
        attempts: array(m, 4, "receipt.attempts")?
            .iter()
            .map(parse_route_attempt)
            .collect::<Result<_, _>>()?,
        transport_binding_digest: optional32(
            required(m, 5, "receipt.binding")?,
            "receipt.binding",
        )?,
        session_id: optional32(required(m, 6, "receipt.session")?, "receipt.session")?,
        started_at: unsigned(m, 7, "receipt.started")?,
        authenticated_at: optional_unsigned(
            required(m, 8, "receipt.authenticated")?,
            "receipt.authenticated",
        )?,
        terminal_outcome: parse_terminal(required(m, 9, "receipt.terminal")?)?,
        limitations: array(m, 10, "receipt.limitations")?
            .iter()
            .map(parse_limitation)
            .collect::<Result<_, _>>()?,
        local_signature: bytes64(m, 11, "receipt.signature")?,
    })
}

fn version_value(v: &ProtocolVersionV1) -> CanonicalValue {
    CanonicalValue::Map(vec![(0, u(v.major)), (1, u(v.minor))])
}
fn parse_version(v: &CanonicalValue) -> Result<ProtocolVersionV1, ReachabilityCodecError> {
    let m = map(v, "version")?;
    exact_keys(m, &[0, 1], "version")?;
    Ok(ProtocolVersionV1 {
        major: unsigned(m, 0, "version.major")?,
        minor: unsigned(m, 1, "version.minor")?,
    })
}
fn host_value(v: &HostAddressV1) -> CanonicalValue {
    match v {
        HostAddressV1::Ipv4(x) => CanonicalValue::Map(vec![(0, u(1)), (1, b(x))]),
        HostAddressV1::Ipv6(x) => CanonicalValue::Map(vec![(0, u(2)), (1, b(x))]),
        HostAddressV1::Dns(x) => {
            CanonicalValue::Map(vec![(0, u(3)), (1, CanonicalValue::Text(x.clone()))])
        }
    }
}
fn parse_host(v: &CanonicalValue) -> Result<HostAddressV1, ReachabilityCodecError> {
    let m = map(v, "host")?;
    exact_keys(m, &[0, 1], "host")?;
    match unsigned(m, 0, "host.kind")? {
        1 => Ok(HostAddressV1::Ipv4(bytes4(m, 1, "host.ipv4")?)),
        2 => Ok(HostAddressV1::Ipv6(bytes16(m, 1, "host.ipv6")?)),
        3 => Ok(HostAddressV1::Dns(text(m, 1, "host.dns")?.to_owned())),
        _ => Err(ReachabilityCodecError::InvalidField("host.kind")),
    }
}
fn endpoint_value(v: &ReachabilityEndpointV1) -> CanonicalValue {
    CanonicalValue::Map(vec![(0, host_value(&v.host)), (1, u(v.port.into()))])
}
fn parse_endpoint(v: &CanonicalValue) -> Result<ReachabilityEndpointV1, ReachabilityCodecError> {
    let m = map(v, "endpoint")?;
    exact_keys(m, &[0, 1], "endpoint")?;
    Ok(ReachabilityEndpointV1 {
        host: parse_host(required(m, 0, "endpoint.host")?)?,
        port: port(m, 1, "endpoint.port")?,
    })
}
fn discovery_endpoint_value(v: &DiscoveryEndpointV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, u(discovery_transport(v.transport))),
        (1, host_value(&v.host)),
        (2, u(v.port.into())),
        (3, CanonicalValue::Text(v.path.clone())),
    ])
}
fn parse_discovery_endpoint(
    v: &CanonicalValue,
) -> Result<DiscoveryEndpointV1, ReachabilityCodecError> {
    let m = map(v, "discovery_endpoint")?;
    exact_keys(m, &[0, 1, 2, 3], "discovery_endpoint")?;
    Ok(DiscoveryEndpointV1 {
        transport: parse_discovery_transport(unsigned(m, 0, "discovery.transport")?)?,
        host: parse_host(required(m, 1, "discovery.host")?)?,
        port: port(m, 2, "discovery.port")?,
        path: text(m, 3, "discovery.path")?.to_owned(),
    })
}
fn relay_endpoint_value(v: &RelayEndpointV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, u(relay_transport(v.transport))),
        (1, host_value(&v.host)),
        (2, u(v.port.into())),
    ])
}
fn parse_relay_endpoint(v: &CanonicalValue) -> Result<RelayEndpointV1, ReachabilityCodecError> {
    let m = map(v, "relay_endpoint")?;
    exact_keys(m, &[0, 1, 2], "relay_endpoint")?;
    Ok(RelayEndpointV1 {
        transport: parse_relay_transport(unsigned(m, 0, "relay_endpoint.transport")?)?,
        host: parse_host(required(m, 1, "relay_endpoint.host")?)?,
        port: port(m, 2, "relay_endpoint.port")?,
    })
}
fn public_candidate_value(v: &PublicCandidateV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, u(public_kind(v.kind))),
        (1, endpoint_value(&v.endpoint)),
        (2, u(v.priority.into())),
        (3, b(&v.foundation)),
    ])
}
fn parse_public_candidate(v: &CanonicalValue) -> Result<PublicCandidateV1, ReachabilityCodecError> {
    let m = map(v, "public_candidate")?;
    exact_keys(m, &[0, 1, 2, 3], "public_candidate")?;
    Ok(PublicCandidateV1 {
        kind: parse_public_kind(unsigned(m, 0, "public_candidate.kind")?)?,
        endpoint: parse_endpoint(required(m, 1, "public_candidate.endpoint")?)?,
        priority: u32_at(m, 2, "public_candidate.priority")?,
        foundation: bytes16(m, 3, "public_candidate.foundation")?,
    })
}
fn direct_candidate_value(v: &DirectCandidateV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, endpoint_value(&v.endpoint)),
        (1, u(direct_kind(v.kind))),
        (2, u(v.priority.into())),
        (3, u(v.network_epoch)),
        (4, u(v.expires_at)),
    ])
}
fn parse_direct_candidate(v: &CanonicalValue) -> Result<DirectCandidateV1, ReachabilityCodecError> {
    let m = map(v, "direct_candidate")?;
    exact_keys(m, &[0, 1, 2, 3, 4], "direct_candidate")?;
    Ok(DirectCandidateV1 {
        endpoint: parse_endpoint(required(m, 0, "direct.endpoint")?)?,
        kind: parse_direct_kind(unsigned(m, 1, "direct.kind")?)?,
        priority: u32_at(m, 2, "direct.priority")?,
        network_epoch: unsigned(m, 3, "direct.epoch")?,
        expires_at: unsigned(m, 4, "direct.expires")?,
    })
}
fn relay_candidate_value(v: &RelayCandidateV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, node(v.relay_node_id)),
        (1, b(&v.reservation_id)),
        (2, u(relay_transport(v.transport))),
        (3, endpoint_value(&v.endpoint)),
        (4, u(v.priority.into())),
        (5, u(v.expires_at)),
    ])
}
fn parse_relay_candidate(v: &CanonicalValue) -> Result<RelayCandidateV1, ReachabilityCodecError> {
    let m = map(v, "relay_candidate")?;
    exact_keys(m, &[0, 1, 2, 3, 4, 5], "relay_candidate")?;
    Ok(RelayCandidateV1 {
        relay_node_id: node_at(m, 0, "relay_candidate.node")?,
        reservation_id: bytes32(m, 1, "relay_candidate.reservation")?,
        transport: parse_relay_transport(unsigned(m, 2, "relay_candidate.transport")?)?,
        endpoint: parse_endpoint(required(m, 3, "relay_candidate.endpoint")?)?,
        priority: u32_at(m, 4, "relay_candidate.priority")?,
        expires_at: unsigned(m, 5, "relay_candidate.expires")?,
    })
}
fn route_attempt_value(v: &RouteAttemptV1) -> CanonicalValue {
    CanonicalValue::Map(vec![
        (0, u(path_kind(v.path_kind))),
        (1, opt_node(v.carrier_identity)),
        (2, u(v.started_at)),
        (3, u(v.finished_at)),
        (4, attempt_outcome_value(v.outcome)),
    ])
}
fn parse_route_attempt(v: &CanonicalValue) -> Result<RouteAttemptV1, ReachabilityCodecError> {
    let m = map(v, "attempt")?;
    exact_keys(m, &[0, 1, 2, 3, 4], "attempt")?;
    Ok(RouteAttemptV1 {
        path_kind: parse_path_kind(unsigned(m, 0, "attempt.path")?)?,
        carrier_identity: optional_node(required(m, 1, "attempt.carrier")?, "attempt.carrier")?,
        started_at: unsigned(m, 2, "attempt.started")?,
        finished_at: unsigned(m, 3, "attempt.finished")?,
        outcome: parse_attempt_outcome(required(m, 4, "attempt.outcome")?)?,
    })
}
fn attempt_outcome_value(v: RouteAttemptOutcomeV1) -> CanonicalValue {
    match v {
        RouteAttemptOutcomeV1::Connected => CanonicalValue::Map(vec![(0, u(1))]),
        RouteAttemptOutcomeV1::Failed(x) => {
            CanonicalValue::Map(vec![(0, u(2)), (1, u(failure_code(x)))])
        }
    }
}
fn parse_attempt_outcome(
    v: &CanonicalValue,
) -> Result<RouteAttemptOutcomeV1, ReachabilityCodecError> {
    let m = map(v, "attempt.outcome")?;
    match unsigned(m, 0, "attempt.outcome.kind")? {
        1 => {
            exact_keys(m, &[0], "attempt.outcome")?;
            Ok(RouteAttemptOutcomeV1::Connected)
        }
        2 => {
            exact_keys(m, &[0, 1], "attempt.outcome")?;
            Ok(RouteAttemptOutcomeV1::Failed(parse_failure_code(
                unsigned(m, 1, "attempt.outcome.failure")?,
            )?))
        }
        _ => Err(ReachabilityCodecError::InvalidField("attempt.outcome.kind")),
    }
}
fn terminal_value(v: RouteTerminalOutcomeV1) -> CanonicalValue {
    match v {
        RouteTerminalOutcomeV1::Connected => CanonicalValue::Map(vec![(0, u(1))]),
        RouteTerminalOutcomeV1::PathLimited => CanonicalValue::Map(vec![(0, u(2))]),
        RouteTerminalOutcomeV1::Failed(x) => {
            CanonicalValue::Map(vec![(0, u(3)), (1, u(failure_code(x)))])
        }
    }
}
fn parse_terminal(v: &CanonicalValue) -> Result<RouteTerminalOutcomeV1, ReachabilityCodecError> {
    let m = map(v, "terminal")?;
    match unsigned(m, 0, "terminal.kind")? {
        1 => {
            exact_keys(m, &[0], "terminal")?;
            Ok(RouteTerminalOutcomeV1::Connected)
        }
        2 => {
            exact_keys(m, &[0], "terminal")?;
            Ok(RouteTerminalOutcomeV1::PathLimited)
        }
        3 => {
            exact_keys(m, &[0, 1], "terminal")?;
            Ok(RouteTerminalOutcomeV1::Failed(parse_failure_code(
                unsigned(m, 1, "terminal.failure")?,
            )?))
        }
        _ => Err(ReachabilityCodecError::InvalidField("terminal.kind")),
    }
}
fn parse_limitation(v: &CanonicalValue) -> Result<RouteLimitationV1, ReachabilityCodecError> {
    let m = map(v, "limitation")?;
    exact_keys(m, &[0, 1], "limitation")?;
    Ok(RouteLimitationV1 {
        code: parse_limitation_code(unsigned(m, 0, "limitation.code")?)?,
        count: unsigned(m, 1, "limitation.count")?,
    })
}

fn validate(v: &ReachabilityObjectV1) -> Result<(), ReachabilityCodecError> {
    match v {
        ReachabilityObjectV1::BootstrapManifest(x) => {
            format(x.format)?;
            bounded(&x.discovery_endpoints, 1, MAX_DISCOVERY_ENDPOINTS)?;
            bounded(&x.protocol_versions, 1, MAX_PROTOCOL_VERSIONS)?;
            strictly_sorted(&x.protocol_versions, "bootstrap.versions")?;
            validity(x.issued_at, x.expires_at, 86_400)?;
            for e in &x.discovery_endpoints {
                validate_discovery_endpoint(e)?;
            }
        }
        ReachabilityObjectV1::RelayDescriptor(x) => {
            format(x.format)?;
            bounded(&x.endpoints, 1, MAX_RELAY_ENDPOINTS)?;
            bounded(&x.supported_transports, 1, 2)?;
            bounded(&x.protocol_versions, 1, MAX_PROTOCOL_VERSIONS)?;
            strictly_sorted(&x.supported_transports, "descriptor.transports")?;
            strictly_sorted(&x.protocol_versions, "descriptor.versions")?;
            validity(
                x.issued_at,
                x.expires_at,
                MAX_RELAY_DESCRIPTOR_VALIDITY_SECONDS,
            )?;
            let a = x
                .endpoints
                .iter()
                .map(|e| e.transport)
                .collect::<BTreeSet<_>>();
            let bset = x
                .supported_transports
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            if a != bset || bset.len() != x.supported_transports.len() {
                return Err(ReachabilityCodecError::InvalidField(
                    "descriptor.transports",
                ));
            }
            for e in &x.endpoints {
                validate_host(&e.host)?;
                if e.port == 0 {
                    return Err(ReachabilityCodecError::InvalidField("endpoint.port"));
                }
            }
        }
        ReachabilityObjectV1::RelayReservation(x) => validate_reservation(x)?,
        ReachabilityObjectV1::Advertisement(x) => {
            format(x.format)?;
            bounded(&x.relay_reservations, 1, MAX_RELAY_RESERVATIONS)?;
            if x.optional_public_candidates.len() > MAX_PUBLIC_CANDIDATES {
                return Err(ReachabilityCodecError::Limit);
            }
            validity(x.issued_at, x.expires_at, 300)?;
            for r in &x.relay_reservations {
                validate_reservation(r)?;
            }
            for c in &x.optional_public_candidates {
                validate_public_endpoint(&c.endpoint)?;
            }
        }
        ReachabilityObjectV1::RoutePlan(x) => {
            if x.direct_candidates.len() > MAX_DIRECT_CANDIDATES
                || x.relay_candidates.len() > MAX_RELAY_CANDIDATES
                || x.attempt_budget == 0
                || x.attempt_budget > MAX_ROUTE_PLAN_ATTEMPTS as u64
            {
                return Err(ReachabilityCodecError::Limit);
            }
            if x.resource_budget.max_concurrent_checks == 0
                || x.resource_budget.max_signature_checks == 0
                || x.resource_budget.max_probe_bytes == 0
            {
                return Err(ReachabilityCodecError::InvalidField("plan.budget"));
            }
            for c in &x.direct_candidates {
                if c.endpoint.port == 0 {
                    return Err(ReachabilityCodecError::InvalidField("direct.endpoint.port"));
                }
                if c.kind != DirectCandidateKindV1::Host {
                    validate_public_endpoint(&c.endpoint)?;
                }
            }
            for c in &x.relay_candidates {
                validate_public_endpoint(&c.endpoint)?;
            }
        }
        ReachabilityObjectV1::RouteReceipt(x) => {
            if x.attempts.len() > MAX_ROUTE_RECEIPT_ATTEMPTS
                || x.limitations.len() > MAX_ROUTE_LIMITATIONS
            {
                return Err(ReachabilityCodecError::Limit);
            }
            if x.authenticated_peer.is_some() != x.session_id.is_some() {
                return Err(ReachabilityCodecError::InvalidField("receipt.session"));
            }
            if matches!(x.terminal_outcome, RouteTerminalOutcomeV1::Connected)
                && (x.authenticated_peer != Some(x.expected_peer)
                    || x.selected_path_kind.is_none()
                    || x.transport_binding_digest.is_none())
            {
                return Err(ReachabilityCodecError::InvalidField("receipt.connected"));
            }
            for a in &x.attempts {
                if a.finished_at < a.started_at {
                    return Err(ReachabilityCodecError::InvalidField("attempt.time"));
                }
            }
        }
    }
    Ok(())
}
fn validate_reservation(x: &RelayReservationV1) -> Result<(), ReachabilityCodecError> {
    format(x.format)?;
    bounded(&x.transport_scope, 1, 2)?;
    strictly_sorted(&x.transport_scope, "reservation.transports")?;
    if x.transport_scope
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .len()
        != x.transport_scope.len()
    {
        return Err(ReachabilityCodecError::InvalidField(
            "reservation.transports",
        ));
    }
    validity(x.issued_at, x.expires_at, 900)
}
fn validate_discovery_endpoint(e: &DiscoveryEndpointV1) -> Result<(), ReachabilityCodecError> {
    validate_host(&e.host)?;
    if e.port == 0 || !e.path.starts_with('/') || e.path.starts_with("//") || !e.path.is_ascii() {
        return Err(ReachabilityCodecError::InvalidField("discovery.endpoint"));
    }
    Ok(())
}
fn validate_public_endpoint(e: &ReachabilityEndpointV1) -> Result<(), ReachabilityCodecError> {
    validate_host(&e.host)?;
    if e.port == 0 {
        return Err(ReachabilityCodecError::InvalidField("endpoint.port"));
    }
    Ok(())
}
fn validate_host(h: &HostAddressV1) -> Result<(), ReachabilityCodecError> {
    match h {
        HostAddressV1::Dns(x) => {
            if x.is_empty()
                || x.len() > 253
                || !x.is_ascii()
                || x != &x.to_ascii_lowercase()
                || x.starts_with('.')
                || x.ends_with('.')
                || x.split('.').any(|p| {
                    p.is_empty()
                        || p.len() > 63
                        || p.starts_with('-')
                        || p.ends_with('-')
                        || !p
                            .bytes()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                })
            {
                return Err(ReachabilityCodecError::InvalidField("host.dns"));
            }
        }
        HostAddressV1::Ipv4(x) => {
            let ip = Ipv4Addr::from(*x);
            if ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || x[0] == 0
                || x[0] >= 224
                || x[0] == 100 && (64..=127).contains(&x[1])
                || x[0] == 192 && x[1] == 0 && x[2] == 0
                || x[0] == 192 && x[1] == 0 && x[2] == 2
                || x[0] == 198 && (x[1] == 18 || x[1] == 19)
                || x[0] == 198 && x[1] == 51 && x[2] == 100
                || x[0] == 203 && x[1] == 0 && x[2] == 113
            {
                return Err(ReachabilityCodecError::EndpointNotGlobal);
            }
        }
        HostAddressV1::Ipv6(x) => {
            let ip = Ipv6Addr::from(*x);
            if ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unspecified()
                || (x[0] & 0xfe) == 0xfc
                || (x[0] == 0xfe && (x[1] & 0xc0) == 0x80)
                || (x[0] == 0x20 && x[1] == 0x01 && x[2] == 0x0d && x[3] == 0xb8)
            {
                return Err(ReachabilityCodecError::EndpointNotGlobal);
            }
        }
    }
    Ok(())
}
fn format(x: u64) -> Result<(), ReachabilityCodecError> {
    if x == 1 {
        Ok(())
    } else {
        Err(ReachabilityCodecError::InvalidField("format"))
    }
}
fn validity(i: u64, e: u64, maximum: u64) -> Result<(), ReachabilityCodecError> {
    if i < e && e - i <= maximum {
        Ok(())
    } else {
        Err(ReachabilityCodecError::InvalidField("validity"))
    }
}

fn object_byte_limit(value: &ReachabilityObjectV1) -> usize {
    match value {
        ReachabilityObjectV1::BootstrapManifest(_) => 65_536,
        ReachabilityObjectV1::RelayDescriptor(_) => 16_384,
        ReachabilityObjectV1::RelayReservation(_) => 8_192,
        ReachabilityObjectV1::Advertisement(_) => 32_768,
        ReachabilityObjectV1::RoutePlan(_) | ReachabilityObjectV1::RouteReceipt(_) => {
            MAX_REACHABILITY_OBJECT_BYTES
        }
    }
}
fn bounded<T>(x: &[T], min: usize, max: usize) -> Result<(), ReachabilityCodecError> {
    if x.len() < min || x.len() > max {
        Err(ReachabilityCodecError::Limit)
    } else {
        Ok(())
    }
}

fn strictly_sorted<T: Ord>(
    values: &[T],
    field: &'static str,
) -> Result<(), ReachabilityCodecError> {
    if values.windows(2).all(|pair| pair[0] < pair[1]) {
        Ok(())
    } else {
        Err(ReachabilityCodecError::InvalidField(field))
    }
}

fn relay_transport(v: RelayTransportV1) -> u64 {
    match v {
        RelayTransportV1::QuicUdp => 1,
        RelayTransportV1::TlsTcp443 => 2,
    }
}
fn parse_relay_transport(v: u64) -> Result<RelayTransportV1, ReachabilityCodecError> {
    match v {
        1 => Ok(RelayTransportV1::QuicUdp),
        2 => Ok(RelayTransportV1::TlsTcp443),
        _ => Err(ReachabilityCodecError::InvalidField("relay.transport")),
    }
}
fn parse_relay_transport_value(
    v: &CanonicalValue,
) -> Result<RelayTransportV1, ReachabilityCodecError> {
    match v {
        CanonicalValue::Unsigned(x) => parse_relay_transport(*x),
        _ => Err(ReachabilityCodecError::InvalidField("relay.transport")),
    }
}
fn discovery_transport(v: DiscoveryTransportV1) -> u64 {
    match v {
        DiscoveryTransportV1::Https => 1,
        DiscoveryTransportV1::RendezvousQuic => 2,
    }
}
fn parse_discovery_transport(v: u64) -> Result<DiscoveryTransportV1, ReachabilityCodecError> {
    match v {
        1 => Ok(DiscoveryTransportV1::Https),
        2 => Ok(DiscoveryTransportV1::RendezvousQuic),
        _ => Err(ReachabilityCodecError::InvalidField("discovery.transport")),
    }
}
fn public_kind(v: PublicCandidateKindV1) -> u64 {
    match v {
        PublicCandidateKindV1::ServerReflexive => 1,
        PublicCandidateKindV1::ProviderMapped => 2,
    }
}
fn parse_public_kind(v: u64) -> Result<PublicCandidateKindV1, ReachabilityCodecError> {
    match v {
        1 => Ok(PublicCandidateKindV1::ServerReflexive),
        2 => Ok(PublicCandidateKindV1::ProviderMapped),
        _ => Err(ReachabilityCodecError::InvalidField("public.kind")),
    }
}
fn direct_kind(v: DirectCandidateKindV1) -> u64 {
    match v {
        DirectCandidateKindV1::Host => 1,
        DirectCandidateKindV1::ServerReflexive => 2,
        DirectCandidateKindV1::ProviderMapped => 3,
    }
}
fn parse_direct_kind(v: u64) -> Result<DirectCandidateKindV1, ReachabilityCodecError> {
    match v {
        1 => Ok(DirectCandidateKindV1::Host),
        2 => Ok(DirectCandidateKindV1::ServerReflexive),
        3 => Ok(DirectCandidateKindV1::ProviderMapped),
        _ => Err(ReachabilityCodecError::InvalidField("direct.kind")),
    }
}
fn path_kind(v: RoutePathKindV1) -> u64 {
    match v {
        RoutePathKindV1::Direct => 1,
        RoutePathKindV1::HolePunched => 2,
        RoutePathKindV1::RelayUdp => 3,
        RoutePathKindV1::RelayTcp443 => 4,
    }
}
fn parse_path_kind(v: u64) -> Result<RoutePathKindV1, ReachabilityCodecError> {
    match v {
        1 => Ok(RoutePathKindV1::Direct),
        2 => Ok(RoutePathKindV1::HolePunched),
        3 => Ok(RoutePathKindV1::RelayUdp),
        4 => Ok(RoutePathKindV1::RelayTcp443),
        _ => Err(ReachabilityCodecError::InvalidField("path.kind")),
    }
}
fn failure_code(v: RouteFailureCodeV1) -> u64 {
    match v {
        RouteFailureCodeV1::NoBootstrapReachable => 1,
        RouteFailureCodeV1::CandidateExpired => 2,
        RouteFailureCodeV1::DirectTimeout => 3,
        RouteFailureCodeV1::HolePunchFailed => 4,
        RouteFailureCodeV1::RelayDenied => 5,
        RouteFailureCodeV1::RelayUnavailable => 6,
        RouteFailureCodeV1::PeerIdentityMismatch => 7,
        RouteFailureCodeV1::NetworkChanged => 8,
        RouteFailureCodeV1::BudgetExceeded => 9,
    }
}
fn parse_failure_code(v: u64) -> Result<RouteFailureCodeV1, ReachabilityCodecError> {
    match v {
        1 => Ok(RouteFailureCodeV1::NoBootstrapReachable),
        2 => Ok(RouteFailureCodeV1::CandidateExpired),
        3 => Ok(RouteFailureCodeV1::DirectTimeout),
        4 => Ok(RouteFailureCodeV1::HolePunchFailed),
        5 => Ok(RouteFailureCodeV1::RelayDenied),
        6 => Ok(RouteFailureCodeV1::RelayUnavailable),
        7 => Ok(RouteFailureCodeV1::PeerIdentityMismatch),
        8 => Ok(RouteFailureCodeV1::NetworkChanged),
        9 => Ok(RouteFailureCodeV1::BudgetExceeded),
        _ => Err(ReachabilityCodecError::InvalidField("failure.code")),
    }
}
fn limitation_code(v: RouteLimitationCodeV1) -> u64 {
    match v {
        RouteLimitationCodeV1::BootstrapSourcesExhausted => 1,
        RouteLimitationCodeV1::SignatureBudgetExhausted => 2,
        RouteLimitationCodeV1::CandidateBudgetExhausted => 3,
        RouteLimitationCodeV1::ProbeBudgetExhausted => 4,
        RouteLimitationCodeV1::DeadlineExceeded => 5,
        RouteLimitationCodeV1::NetworkChangedDuringAttempt => 6,
    }
}
fn parse_limitation_code(v: u64) -> Result<RouteLimitationCodeV1, ReachabilityCodecError> {
    match v {
        1 => Ok(RouteLimitationCodeV1::BootstrapSourcesExhausted),
        2 => Ok(RouteLimitationCodeV1::SignatureBudgetExhausted),
        3 => Ok(RouteLimitationCodeV1::CandidateBudgetExhausted),
        4 => Ok(RouteLimitationCodeV1::ProbeBudgetExhausted),
        5 => Ok(RouteLimitationCodeV1::DeadlineExceeded),
        6 => Ok(RouteLimitationCodeV1::NetworkChangedDuringAttempt),
        _ => Err(ReachabilityCodecError::InvalidField("limitation.code")),
    }
}

fn u(v: u64) -> CanonicalValue {
    CanonicalValue::Unsigned(v)
}
fn b(v: &[u8]) -> CanonicalValue {
    CanonicalValue::Bytes(v.to_vec())
}
fn node(v: NodeId) -> CanonicalValue {
    b(v.as_bytes())
}
fn arr<I: Iterator<Item = CanonicalValue>>(i: I) -> CanonicalValue {
    CanonicalValue::Array(i.collect())
}
fn opt32(v: Option<[u8; 32]>) -> CanonicalValue {
    v.map(|x| b(&x)).unwrap_or(CanonicalValue::Null)
}
fn opt_node(v: Option<NodeId>) -> CanonicalValue {
    v.map(node).unwrap_or(CanonicalValue::Null)
}
fn opt_u(v: Option<u64>) -> CanonicalValue {
    v.map(u).unwrap_or(CanonicalValue::Null)
}
fn map<'a>(
    v: &'a CanonicalValue,
    f: &'static str,
) -> Result<&'a [(u64, CanonicalValue)], ReachabilityCodecError> {
    if let CanonicalValue::Map(x) = v {
        Ok(x)
    } else {
        Err(ReachabilityCodecError::InvalidField(f))
    }
}
fn exact_keys(
    m: &[(u64, CanonicalValue)],
    keys: &[u64],
    f: &'static str,
) -> Result<(), ReachabilityCodecError> {
    if m.len() == keys.len() && m.iter().map(|x| x.0).eq(keys.iter().copied()) {
        Ok(())
    } else {
        Err(ReachabilityCodecError::InvalidField(f))
    }
}
fn required<'a>(
    m: &'a [(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<&'a CanonicalValue, ReachabilityCodecError> {
    m.iter()
        .find_map(|(x, v)| (*x == k).then_some(v))
        .ok_or(ReachabilityCodecError::InvalidField(f))
}
fn unsigned(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<u64, ReachabilityCodecError> {
    match required(m, k, f)? {
        CanonicalValue::Unsigned(x) => Ok(*x),
        _ => Err(ReachabilityCodecError::InvalidField(f)),
    }
}
fn text<'a>(
    m: &'a [(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<&'a str, ReachabilityCodecError> {
    match required(m, k, f)? {
        CanonicalValue::Text(x) => Ok(x),
        _ => Err(ReachabilityCodecError::InvalidField(f)),
    }
}
fn array<'a>(
    m: &'a [(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<&'a [CanonicalValue], ReachabilityCodecError> {
    match required(m, k, f)? {
        CanonicalValue::Array(x) => Ok(x),
        _ => Err(ReachabilityCodecError::InvalidField(f)),
    }
}
fn bytes_n<const N: usize>(
    v: &CanonicalValue,
    f: &'static str,
) -> Result<[u8; N], ReachabilityCodecError> {
    match v {
        CanonicalValue::Bytes(x) if x.len() == N => {
            let mut o = [0; N];
            o.copy_from_slice(x);
            Ok(o)
        }
        _ => Err(ReachabilityCodecError::InvalidField(f)),
    }
}
fn bytes4(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<[u8; 4], ReachabilityCodecError> {
    bytes_n(required(m, k, f)?, f)
}
fn bytes16(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<[u8; 16], ReachabilityCodecError> {
    bytes_n(required(m, k, f)?, f)
}
fn bytes32(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<[u8; 32], ReachabilityCodecError> {
    bytes_n(required(m, k, f)?, f)
}
fn bytes64(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<[u8; 64], ReachabilityCodecError> {
    bytes_n(required(m, k, f)?, f)
}
fn node_at(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<NodeId, ReachabilityCodecError> {
    Ok(NodeId::from_bytes(bytes32(m, k, f)?))
}
fn port(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<u16, ReachabilityCodecError> {
    u16::try_from(unsigned(m, k, f)?).map_err(|_| ReachabilityCodecError::InvalidField(f))
}
fn u32_at(
    m: &[(u64, CanonicalValue)],
    k: u64,
    f: &'static str,
) -> Result<u32, ReachabilityCodecError> {
    u32::try_from(unsigned(m, k, f)?).map_err(|_| ReachabilityCodecError::InvalidField(f))
}
fn optional32(
    v: &CanonicalValue,
    f: &'static str,
) -> Result<Option<[u8; 32]>, ReachabilityCodecError> {
    if matches!(v, CanonicalValue::Null) {
        Ok(None)
    } else {
        Ok(Some(bytes_n(v, f)?))
    }
}
fn optional_node(
    v: &CanonicalValue,
    f: &'static str,
) -> Result<Option<NodeId>, ReachabilityCodecError> {
    Ok(optional32(v, f)?.map(NodeId::from_bytes))
}
fn optional_unsigned(
    v: &CanonicalValue,
    f: &'static str,
) -> Result<Option<u64>, ReachabilityCodecError> {
    match v {
        CanonicalValue::Null => Ok(None),
        CanonicalValue::Unsigned(x) => Ok(Some(*x)),
        _ => Err(ReachabilityCodecError::InvalidField(f)),
    }
}
fn optional_enum<T>(
    v: &CanonicalValue,
    p: fn(u64) -> Result<T, ReachabilityCodecError>,
) -> Result<Option<T>, ReachabilityCodecError> {
    match v {
        CanonicalValue::Null => Ok(None),
        CanonicalValue::Unsigned(x) => p(*x).map(Some),
        _ => Err(ReachabilityCodecError::InvalidField("optional.enum")),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityCodecError {
    Canonical(CanonicalError),
    UnknownSchema(u64),
    InvalidField(&'static str),
    EndpointNotGlobal,
    WrongSignatureRole,
    NonCanonicalMessage,
    Limit,
}
impl From<CanonicalError> for ReachabilityCodecError {
    fn from(v: CanonicalError) -> Self {
        Self::Canonical(v)
    }
}
impl std::fmt::Display for ReachabilityCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_REACHABILITY_CODEC: {self:?}")
    }
}
impl std::error::Error for ReachabilityCodecError {}
