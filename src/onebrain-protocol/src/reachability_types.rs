//! Portable outbound-first reachability objects.
//!
//! These types describe authenticated routing inputs and privacy-safe local
//! evidence. They do not grant peer identity or alter the existing OBP session
//! wire protocol.

use ku_core::foundation::NodeId;

pub const MAX_REACHABILITY_OBJECT_BYTES: usize = 262_144;
pub const MAX_RELAY_ENDPOINTS: usize = 8;
/// Maximum lifetime of a signed relay descriptor.
pub const MAX_RELAY_DESCRIPTOR_VALIDITY_SECONDS: u64 = 1_800;
pub const MAX_DISCOVERY_ENDPOINTS: usize = 16;
pub const MAX_PROTOCOL_VERSIONS: usize = 8;
pub const MAX_RELAY_RESERVATIONS: usize = 3;
pub const MAX_PUBLIC_CANDIDATES: usize = 8;
pub const MAX_DIRECT_CANDIDATES: usize = 8;
pub const MAX_RELAY_CANDIDATES: usize = 6;
pub const MAX_ROUTE_PLAN_ATTEMPTS: usize = 12;
pub const MAX_ROUTE_RECEIPT_ATTEMPTS: usize = 16;
pub const MAX_ROUTE_LIMITATIONS: usize = 16;

pub mod reachability_schema_id {
    pub const BOOTSTRAP_MANIFEST: u64 = 40;
    pub const RELAY_DESCRIPTOR: u64 = 41;
    pub const RELAY_RESERVATION: u64 = 42;
    pub const REACHABILITY_ADVERTISEMENT: u64 = 43;
    pub const ROUTE_PLAN: u64 = 44;
    pub const ROUTE_RECEIPT: u64 = 45;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProtocolVersionV1 {
    pub major: u64,
    pub minor: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HostAddressV1 {
    Ipv4([u8; 4]),
    Ipv6([u8; 16]),
    Dns(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReachabilityEndpointV1 {
    pub host: HostAddressV1,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelayTransportV1 {
    QuicUdp,
    TlsTcp443,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryTransportV1 {
    Https,
    RendezvousQuic,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiscoveryEndpointV1 {
    pub transport: DiscoveryTransportV1,
    pub host: HostAddressV1,
    pub port: u16,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelayEndpointV1 {
    pub transport: RelayTransportV1,
    pub host: HostAddressV1,
    pub port: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PublicCandidateKindV1 {
    ServerReflexive,
    ProviderMapped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirectCandidateKindV1 {
    Host,
    ServerReflexive,
    ProviderMapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicCandidateV1 {
    pub kind: PublicCandidateKindV1,
    pub endpoint: ReachabilityEndpointV1,
    pub priority: u32,
    pub foundation: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectCandidateV1 {
    pub endpoint: ReachabilityEndpointV1,
    pub kind: DirectCandidateKindV1,
    pub priority: u32,
    pub network_epoch: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateCandidateV1 {
    pub endpoint: ReachabilityEndpointV1,
    pub priority: u32,
    pub foundation: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HolePunchCandidateV1 {
    pub relay_node_id: NodeId,
    pub local_reservation_id: [u8; 32],
    pub remote_reservation_id: [u8; 32],
    pub schedule_digest: [u8; 32],
    pub priority: u32,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayCandidateV1 {
    pub relay_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub transport: RelayTransportV1,
    pub endpoint: ReachabilityEndpointV1,
    pub priority: u32,
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteResourceBudgetV1 {
    pub max_concurrent_checks: u64,
    pub max_signature_checks: u64,
    pub max_probe_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutePathKindV1 {
    Direct,
    HolePunched,
    RelayUdp,
    RelayTcp443,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteFailureCodeV1 {
    NoBootstrapReachable,
    CandidateExpired,
    DirectTimeout,
    HolePunchFailed,
    RelayDenied,
    RelayUnavailable,
    PeerIdentityMismatch,
    NetworkChanged,
    BudgetExceeded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteAttemptOutcomeV1 {
    Connected,
    Failed(RouteFailureCodeV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteLimitationCodeV1 {
    BootstrapSourcesExhausted,
    SignatureBudgetExhausted,
    CandidateBudgetExhausted,
    ProbeBudgetExhausted,
    DeadlineExceeded,
    NetworkChangedDuringAttempt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteAttemptV1 {
    pub path_kind: RoutePathKindV1,
    pub carrier_identity: Option<NodeId>,
    pub started_at: u64,
    pub finished_at: u64,
    pub outcome: RouteAttemptOutcomeV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteTerminalOutcomeV1 {
    Connected,
    PathLimited,
    Failed(RouteFailureCodeV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteLimitationV1 {
    pub code: RouteLimitationCodeV1,
    pub count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateRouteAttemptDetailV1 {
    pub attempt_index: u64,
    pub endpoint: ReachabilityEndpointV1,
    pub network_epoch: u64,
    pub diagnostic_code: RouteFailureCodeV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapManifestV1 {
    pub format: u64,
    pub discovery_source_id: [u8; 32],
    pub discovery_endpoints: Vec<DiscoveryEndpointV1>,
    pub protocol_versions: Vec<ProtocolVersionV1>,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub source_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayDescriptorV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub relay_public_key: [u8; 32],
    pub endpoints: Vec<RelayEndpointV1>,
    pub supported_transports: Vec<RelayTransportV1>,
    pub protocol_versions: Vec<ProtocolVersionV1>,
    pub capacity_policy_digest: [u8; 32],
    pub previous_descriptor_blake3: Option<[u8; 32]>,
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayReservationV1 {
    pub format: u64,
    pub relay_node_id: NodeId,
    pub target_node_id: NodeId,
    pub reservation_id: [u8; 32],
    pub transport_scope: Vec<RelayTransportV1>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub target_signature: [u8; 64],
    pub relay_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachabilityAdvertisementV1 {
    pub format: u64,
    pub target_node_id: NodeId,
    pub relay_reservations: Vec<RelayReservationV1>,
    pub optional_public_candidates: Vec<PublicCandidateV1>,
    pub capability_ceiling: [u8; 32],
    pub sequence: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub target_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePlanV1 {
    pub expected_peer: NodeId,
    pub direct_candidates: Vec<DirectCandidateV1>,
    pub relay_candidates: Vec<RelayCandidateV1>,
    pub deadline: u64,
    pub attempt_budget: u64,
    pub resource_budget: RouteResourceBudgetV1,
    pub privacy_policy_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteReceiptV1 {
    pub expected_peer: NodeId,
    pub authenticated_peer: Option<NodeId>,
    pub selected_path_kind: Option<RoutePathKindV1>,
    pub selected_carrier_identity: Option<NodeId>,
    pub attempts: Vec<RouteAttemptV1>,
    pub transport_binding_digest: Option<[u8; 32]>,
    pub session_id: Option<[u8; 32]>,
    pub started_at: u64,
    pub authenticated_at: Option<u64>,
    pub terminal_outcome: RouteTerminalOutcomeV1,
    pub limitations: Vec<RouteLimitationV1>,
    pub local_signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayPossessionChallengeV1 {
    pub relay_node_id: NodeId,
    pub descriptor_digest: [u8; 32],
    pub endpoint_index: u64,
    pub transport: RelayTransportV1,
    pub verifier_context: [u8; 32],
    pub nonce: [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayPossessionProofV1 {
    pub challenge_digest: [u8; 32],
    pub connection_binding_digest: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityObjectV1 {
    BootstrapManifest(BootstrapManifestV1),
    RelayDescriptor(RelayDescriptorV1),
    RelayReservation(RelayReservationV1),
    Advertisement(ReachabilityAdvertisementV1),
    RoutePlan(RoutePlanV1),
    RouteReceipt(RouteReceiptV1),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachabilitySignatureRoleV1 {
    BootstrapSource,
    RelayDescriptor,
    ReservationTarget,
    ReservationRelay,
    AdvertisementTarget,
    RouteReceiptLocal,
    PossessionRelay,
}
