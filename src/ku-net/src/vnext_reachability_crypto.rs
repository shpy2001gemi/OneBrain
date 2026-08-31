//! Identity, signature, freshness, replay and possession admission for
//! outbound-first reachability records.
//!
//! Public/fetched bytes can only become prepared values. Private validated
//! wrappers are produced after all authority checks and cannot be fabricated
//! by transport or platform adapters.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_reachability_object, reachability_signing_bytes, BootstrapManifestV1,
    DiscoveryEndpointV1, DiscoveryTransportV1, HostAddressV1, ReachabilityAdvertisementV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1,
    RelayPossessionChallengeV1, RelayPossessionProofV1, RelayReservationV1, RelayTransportV1,
};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::vnext_session::principal_node_id;

const MAX_RESOLVED_PER_ENDPOINT: usize = 8;
const MAX_RESOLVED_PER_OBJECT: usize = 32;
const MAX_PENDING_DESCRIPTORS: usize = 32;
const MAX_PENDING_CHALLENGES: usize = 256;
const POSSESSION_VALIDITY_SECONDS: u64 = 30;
const MAX_CLOCK_SKEW_SECONDS: u64 = 30;
const POSSESSION_DOMAIN: &[u8] = b"onebrain/reachability/relay-possession-proof/v1\0";

pub type AdmissionFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ReachabilityIdentitySigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];
    fn sign_reachability_message(
        &self,
        domain: &'static [u8],
        message: &[u8],
    ) -> Result<[u8; 64], ReachabilityCryptoError>;
}

impl ReachabilityIdentitySigner for SigningKey {
    fn public_key(&self) -> [u8; 32] {
        *self.verifying_key().as_bytes()
    }

    fn sign_reachability_message(
        &self,
        domain: &'static [u8],
        message: &[u8],
    ) -> Result<[u8; 64], ReachabilityCryptoError> {
        let mut preimage = Vec::with_capacity(domain.len() + message.len());
        preimage.extend_from_slice(domain);
        preimage.extend_from_slice(message);
        Ok(self.sign(&preimage).to_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownPeerIdentity {
    pub node_id: NodeId,
    pub public_key: [u8; 32],
}

impl KnownPeerIdentity {
    pub fn from_public_key(public_key: [u8; 32]) -> Self {
        Self {
            node_id: principal_node_id(&public_key),
            public_key,
        }
    }

    pub fn validate(&self) -> Result<(), RelayAdmissionError> {
        if principal_node_id(&self.public_key) == self.node_id {
            Ok(())
        } else {
            Err(RelayAdmissionError::IdentityMismatch)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnownDiscoverySource {
    source_id: [u8; 32],
    public_key: [u8; 32],
    local_authority_digest: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfiguredBootstrapSource {
    identity: KnownDiscoverySource,
    fetch_endpoint: DiscoveryEndpointV1,
    local_authority_digest: [u8; 32],
}

impl ConfiguredBootstrapSource {
    /// Load a source authority only from an explicit local file. The format is
    /// a closed ASCII line set; fetched/network bytes have no constructor.
    pub fn load_from_trusted_local_file(path: &Path) -> Result<Self, RelayAdmissionError> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| RelayAdmissionError::TrustedBootstrapUnavailable)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(RelayAdmissionError::TrustedBootstrapInvalid);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o022 != 0 {
                return Err(RelayAdmissionError::TrustedBootstrapInvalid);
            }
        }
        let bytes =
            std::fs::read(path).map_err(|_| RelayAdmissionError::TrustedBootstrapInvalid)?;
        if bytes.len() > 4096 || !bytes.is_ascii() {
            return Err(RelayAdmissionError::TrustedBootstrapInvalid);
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| RelayAdmissionError::TrustedBootstrapInvalid)?;
        let mut fields = BTreeMap::new();
        for line in text.lines() {
            let (key, value) = line
                .split_once('=')
                .ok_or(RelayAdmissionError::TrustedBootstrapInvalid)?;
            if fields.insert(key, value).is_some() {
                return Err(RelayAdmissionError::TrustedBootstrapInvalid);
            }
        }
        let expected =
            BTreeSet::from(["format", "public_key", "transport", "host", "port", "path"]);
        if fields.keys().copied().collect::<BTreeSet<_>>() != expected {
            return Err(RelayAdmissionError::TrustedBootstrapInvalid);
        }
        if fields["format"] != "onebrain/bootstrap-source/1" {
            return Err(RelayAdmissionError::TrustedBootstrapInvalid);
        }
        let public_key = decode_hex_32(fields["public_key"])?;
        VerifyingKey::from_bytes(&public_key)
            .map_err(|_| RelayAdmissionError::TrustedBootstrapInvalid)?;
        let transport = match fields["transport"] {
            "https" => DiscoveryTransportV1::Https,
            "rendezvous-quic" => DiscoveryTransportV1::RendezvousQuic,
            _ => return Err(RelayAdmissionError::TrustedBootstrapInvalid),
        };
        let host = parse_configured_host(fields["host"])?;
        validate_public_host(&host)?;
        let port = fields["port"]
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(RelayAdmissionError::TrustedBootstrapInvalid)?;
        let path_value = fields["path"];
        if !path_value.starts_with('/') || path_value.starts_with("//") {
            return Err(RelayAdmissionError::TrustedBootstrapInvalid);
        }
        let authority_digest = *blake3::hash(&bytes).as_bytes();
        let source_id = *principal_node_id(&public_key).as_bytes();
        Ok(Self {
            identity: KnownDiscoverySource {
                source_id,
                public_key,
                local_authority_digest: authority_digest,
            },
            fetch_endpoint: DiscoveryEndpointV1 {
                transport,
                host,
                port,
                path: path_value.to_owned(),
            },
            local_authority_digest: authority_digest,
        })
    }

    pub fn source_id(&self) -> &[u8; 32] {
        &self.identity.source_id
    }

    pub fn fetch_endpoint(&self) -> &DiscoveryEndpointV1 {
        &self.fetch_endpoint
    }

    pub fn authority_digest(&self) -> &[u8; 32] {
        &self.local_authority_digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPublicEndpointV1 {
    endpoint_index: usize,
    addresses: Vec<IpAddr>,
    resolved_at: u64,
    expires_at: u64,
}

impl ResolvedPublicEndpointV1 {
    pub fn endpoint_index(&self) -> usize {
        self.endpoint_index
    }
    pub fn addresses(&self) -> &[IpAddr] {
        &self.addresses
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedBootstrapManifest {
    canonical: BootstrapManifestV1,
    digest: [u8; 32],
    source_authority_digest: [u8; 32],
    resolved_endpoints: Vec<ResolvedPublicEndpointV1>,
    prepared_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedBootstrapManifest {
    canonical: BootstrapManifestV1,
    digest: [u8; 32],
    resolved_endpoints: Vec<ResolvedPublicEndpointV1>,
}

impl ValidatedBootstrapManifest {
    pub fn canonical(&self) -> &BootstrapManifestV1 {
        &self.canonical
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRelayDescriptorAdmission {
    canonical: RelayDescriptorV1,
    digest: [u8; 32],
    resolved_endpoints: Vec<ResolvedPublicEndpointV1>,
    prepared_at: u64,
}

impl PreparedRelayDescriptorAdmission {
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRelayDescriptorAdmission {
    canonical: RelayDescriptorV1,
    digest: [u8; 32],
    resolved_endpoints: Vec<ResolvedPublicEndpointV1>,
    challenges: Vec<RelayPossessionChallengeV1>,
    expires_at: u64,
}

impl PendingRelayDescriptorAdmission {
    pub fn canonical(&self) -> &RelayDescriptorV1 {
        &self.canonical
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn challenges(&self) -> &[RelayPossessionChallengeV1] {
        &self.challenges
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPossessionDialEndpoint {
    pending_descriptor_digest: [u8; 32],
    pending_relay_public_key: [u8; 32],
    challenge: RelayPossessionChallengeV1,
    challenge_digest: [u8; 32],
    endpoint_index: usize,
    signed_host: HostAddressV1,
    port: u16,
    transport: RelayTransportV1,
    admitted_addresses: Vec<IpAddr>,
    dial_addresses: Vec<SocketAddr>,
    expires_at: u64,
}

impl ValidatedPossessionDialEndpoint {
    pub fn challenge(&self) -> &RelayPossessionChallengeV1 {
        &self.challenge
    }
    pub fn challenge_digest(&self) -> [u8; 32] {
        self.challenge_digest
    }
    pub fn dial_addresses(&self) -> &[SocketAddr] {
        &self.dial_addresses
    }

    pub fn relay_node_id(&self) -> NodeId {
        self.challenge.relay_node_id
    }

    pub(crate) fn relay_public_key(&self) -> [u8; 32] {
        self.pending_relay_public_key
    }

    pub fn endpoint_index(&self) -> usize {
        self.endpoint_index
    }

    pub fn transport(&self) -> RelayTransportV1 {
        self.transport
    }

    pub(crate) fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedRelayDescriptor {
    canonical: RelayDescriptorV1,
    digest: [u8; 32],
    resolved_endpoints: Vec<ResolvedPublicEndpointV1>,
    possession_connection_bindings: Vec<[u8; 32]>,
    possession_verified_at: u64,
}

impl ValidatedRelayDescriptor {
    pub fn canonical(&self) -> &RelayDescriptorV1 {
        &self.canonical
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedRelayReservation {
    canonical: RelayReservationV1,
    digest: [u8; 32],
}

impl ValidatedRelayReservation {
    pub fn canonical(&self) -> &RelayReservationV1 {
        &self.canonical
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedReachabilityAdvertisement {
    canonical: ReachabilityAdvertisementV1,
    digest: [u8; 32],
    reservation_digests: Vec<[u8; 32]>,
    resolved_public_candidates: Vec<ResolvedPublicEndpointV1>,
    prepared_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedReachabilityAdvertisement {
    canonical: ReachabilityAdvertisementV1,
    digest: [u8; 32],
    reservations: Vec<ValidatedRelayReservation>,
    resolved_public_candidates: Vec<ResolvedPublicEndpointV1>,
}

impl ValidatedReachabilityAdvertisement {
    pub fn canonical(&self) -> &ReachabilityAdvertisementV1 {
        &self.canonical
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    pub fn reservations(&self) -> &[ValidatedRelayReservation] {
        &self.reservations
    }
}

pub trait PublicEndpointResolver: Send + Sync {
    fn resolve(
        &self,
        host: &HostAddressV1,
        deadline: Instant,
    ) -> Result<Vec<IpAddr>, RelayAdmissionError>;
}

/// Cross-platform, bounded resolver for production dial validation. DNS work
/// runs outside the async runtime, has an exact concurrency ceiling and stays
/// charged until the OS resolver call returns even if the caller deadline
/// expires.
pub struct SystemPublicEndpointResolver {
    active_dns_jobs: Arc<AtomicUsize>,
    max_concurrent_dns_jobs: usize,
}

impl SystemPublicEndpointResolver {
    pub fn new(max_concurrent_dns_jobs: usize) -> Result<Self, RelayAdmissionError> {
        if max_concurrent_dns_jobs == 0 || max_concurrent_dns_jobs > 4 {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        Ok(Self {
            active_dns_jobs: Arc::new(AtomicUsize::new(0)),
            max_concurrent_dns_jobs,
        })
    }

    fn acquire_dns_job(&self) -> Result<(), RelayAdmissionError> {
        self.active_dns_jobs
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < self.max_concurrent_dns_jobs).then_some(active + 1)
            })
            .map(|_| ())
            .map_err(|_| RelayAdmissionError::BudgetExceeded)
    }
}

impl PublicEndpointResolver for SystemPublicEndpointResolver {
    fn resolve(
        &self,
        host: &HostAddressV1,
        deadline: Instant,
    ) -> Result<Vec<IpAddr>, RelayAdmissionError> {
        if Instant::now() >= deadline {
            return Err(RelayAdmissionError::DnsResolutionFailed);
        }
        match host {
            HostAddressV1::Ipv4(value) => Ok(vec![IpAddr::V4(Ipv4Addr::from(*value))]),
            HostAddressV1::Ipv6(value) => Ok(vec![IpAddr::V6(Ipv6Addr::from(*value))]),
            HostAddressV1::Dns(name) => {
                self.acquire_dns_job()?;
                let active = Arc::clone(&self.active_dns_jobs);
                let name = name.clone();
                let (sender, receiver) = mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let result = (name.as_str(), 0)
                        .to_socket_addrs()
                        .map(|values| {
                            let mut output = Vec::new();
                            for value in values {
                                if !output.contains(&value.ip()) {
                                    output.push(value.ip());
                                    if output.len() > MAX_RESOLVED_PER_ENDPOINT {
                                        break;
                                    }
                                }
                            }
                            output
                        })
                        .map_err(|_| RelayAdmissionError::DnsResolutionFailed);
                    let _ = sender.send(result);
                    active.fetch_sub(1, Ordering::AcqRel);
                });
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO);
                let addresses = receiver
                    .recv_timeout(remaining)
                    .map_err(|_| RelayAdmissionError::DnsResolutionFailed)??;
                if addresses.is_empty() || addresses.len() > MAX_RESOLVED_PER_ENDPOINT {
                    Err(RelayAdmissionError::DnsResolutionFailed)
                } else {
                    Ok(addresses)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValidatedPublicDialTransportV1 {
    BootstrapHttps,
    BootstrapRendezvousQuic,
    RelayQuicUdp,
    RelayTlsTcp443,
    DirectQuicUdp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPublicDialEndpoint {
    source_digest: [u8; 32],
    endpoint_index: usize,
    signed_host: HostAddressV1,
    port: u16,
    transport: ValidatedPublicDialTransportV1,
    signed_path: Option<String>,
    admitted_addresses: Vec<IpAddr>,
    dial_addresses: Vec<SocketAddr>,
    expires_at: u64,
}

impl ValidatedPublicDialEndpoint {
    #[cfg(feature = "outbound-first")]
    pub(crate) fn source_digest(&self) -> [u8; 32] {
        self.source_digest
    }

    #[cfg(feature = "outbound-first")]
    pub(crate) fn endpoint_index(&self) -> usize {
        self.endpoint_index
    }

    pub fn dial_addresses(&self) -> &[SocketAddr] {
        &self.dial_addresses
    }

    pub fn signed_host(&self) -> &HostAddressV1 {
        &self.signed_host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn transport(&self) -> ValidatedPublicDialTransportV1 {
        self.transport
    }

    pub fn signed_path(&self) -> Option<&str> {
        self.signed_path.as_deref()
    }

    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReachabilitySequenceKindV1 {
    BootstrapManifest,
    RelayDescriptor,
    Advertisement,
    RelayReserveRequest,
    RelayKeepalive,
    RelayRevoke,
    ReflexiveObservation,
    RelayConnectRequest,
    PrivateCandidateSignal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReachabilitySequenceKeyV1 {
    pub kind: ReachabilitySequenceKindV1,
    pub signer: [u8; 32],
    pub scope: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReachabilityNonceDomainV1 {
    RelayControl,
    PossessionChallenge,
    HolePunchToken,
    RelayConnect,
}

pub trait ReachabilityReplayStore: Send + Sync {
    fn check_sequence_candidate(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        previous_digest: Option<[u8; 32]>,
    ) -> Result<(), RelayAdmissionError>;
    fn compare_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        expected_previous_digest: Option<[u8; 32]>,
        sequence: u64,
        new_digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError>;
    fn check_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError>;
    fn check_and_store_reservation(
        &self,
        relay: NodeId,
        target: NodeId,
        reservation_id: [u8; 32],
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError>;
    fn consume_nonce(
        &self,
        domain: ReachabilityNonceDomainV1,
        scope: [u8; 32],
        nonce: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError>;
}

type SequenceState = BTreeMap<ReachabilitySequenceKeyV1, (u64, [u8; 32], u64)>;
type ReservationKey = ([u8; 32], [u8; 32], [u8; 32]);
type ReservationState = BTreeMap<ReservationKey, ([u8; 32], u64)>;
type NonceKey = (ReachabilityNonceDomainV1, [u8; 32], [u8; 32]);

#[derive(Default)]
pub struct InMemoryReachabilityReplayStore {
    sequences: Mutex<SequenceState>,
    reservations: Mutex<ReservationState>,
    nonces: Mutex<BTreeMap<NonceKey, u64>>,
}

impl ReachabilityReplayStore for InMemoryReachabilityReplayStore {
    fn check_sequence_candidate(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        previous_digest: Option<[u8; 32]>,
    ) -> Result<(), RelayAdmissionError> {
        let state = self
            .sequences
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        match state.get(&key) {
            None if sequence == 1 && previous_digest.is_none() => Ok(()),
            Some((current, digest, _))
                if sequence == current + 1 && previous_digest == Some(*digest) =>
            {
                Ok(())
            }
            Some((current, digest, _))
                if sequence == *current && previous_digest == Some(*digest) =>
            {
                Err(RelayAdmissionError::Replay)
            }
            _ => Err(RelayAdmissionError::SequenceRollback),
        }
    }

    fn compare_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        expected_previous_digest: Option<[u8; 32]>,
        sequence: u64,
        new_digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let mut state = self
            .sequences
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        match state.get(&key) {
            None if sequence == 1 && expected_previous_digest.is_none() => {}
            Some((current, digest, _))
                if sequence == current + 1 && expected_previous_digest == Some(*digest) => {}
            Some((current, digest, _)) if sequence == *current && new_digest == *digest => {
                return Err(RelayAdmissionError::Replay)
            }
            _ => return Err(RelayAdmissionError::SequenceRollback),
        }
        state.insert(key, (sequence, new_digest, expires_at));
        Ok(())
    }

    fn check_and_advance_sequence(
        &self,
        key: ReachabilitySequenceKeyV1,
        sequence: u64,
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let mut state = self
            .sequences
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        match state.get(&key) {
            None if sequence == 1 => {}
            Some((current, _, _)) if sequence == current + 1 => {}
            Some((current, existing, _)) if sequence == *current && digest == *existing => {
                return Err(RelayAdmissionError::Replay);
            }
            _ => return Err(RelayAdmissionError::SequenceRollback),
        }
        state.insert(key, (sequence, digest, expires_at));
        Ok(())
    }

    fn check_and_store_reservation(
        &self,
        relay: NodeId,
        target: NodeId,
        reservation_id: [u8; 32],
        digest: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let key = (*relay.as_bytes(), *target.as_bytes(), reservation_id);
        let mut state = self
            .reservations
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        match state.get(&key) {
            None => {
                state.insert(key, (digest, expires_at));
                Ok(())
            }
            Some((existing, _)) if *existing == digest => Err(RelayAdmissionError::Replay),
            Some(_) => Err(RelayAdmissionError::ReservationIdReuse),
        }
    }

    fn consume_nonce(
        &self,
        domain: ReachabilityNonceDomainV1,
        scope: [u8; 32],
        nonce: [u8; 32],
        expires_at: u64,
    ) -> Result<(), RelayAdmissionError> {
        let mut state = self
            .nonces
            .lock()
            .map_err(|_| RelayAdmissionError::StateUnavailable)?;
        if state.insert((domain, scope, nonce), expires_at).is_some() {
            Err(RelayAdmissionError::ChallengeConsumed)
        } else {
            Ok(())
        }
    }
}

pub struct ReachabilityAdmissionPreparer {
    resolver: Arc<dyn PublicEndpointResolver>,
    _max_concurrency: usize,
}

impl ReachabilityAdmissionPreparer {
    pub fn new(
        resolver: Arc<dyn PublicEndpointResolver>,
        max_concurrency: usize,
    ) -> Result<Self, RelayAdmissionError> {
        if max_concurrency == 0 {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        Ok(Self {
            resolver,
            _max_concurrency: max_concurrency,
        })
    }
}

pub struct ReachabilityDialValidator {
    resolver: Arc<dyn PublicEndpointResolver>,
    _max_concurrency: usize,
}

impl ReachabilityDialValidator {
    pub fn new(
        resolver: Arc<dyn PublicEndpointResolver>,
        max_concurrency: usize,
    ) -> Result<Self, RelayAdmissionError> {
        if max_concurrency == 0 {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        Ok(Self {
            resolver,
            _max_concurrency: max_concurrency,
        })
    }
}

pub struct ReachabilityAdmission {
    replay: Arc<dyn ReachabilityReplayStore>,
    pending_descriptors: BTreeMap<[u8; 32], PendingRelayDescriptorAdmission>,
    pending_challenges: usize,
}

impl ReachabilityAdmission {
    pub fn new(replay: Arc<dyn ReachabilityReplayStore>) -> Self {
        Self {
            replay,
            pending_descriptors: BTreeMap::new(),
            pending_challenges: 0,
        }
    }

    /// Release expired PoP work without changing any admitted sequence floor.
    pub fn expire_pending_descriptors(&mut self, now: u64) -> usize {
        let expired: Vec<_> = self
            .pending_descriptors
            .iter()
            .filter_map(|(digest, pending)| (pending.expires_at < now).then_some(*digest))
            .collect();
        for digest in &expired {
            if let Some(pending) = self.pending_descriptors.remove(digest) {
                self.pending_challenges = self
                    .pending_challenges
                    .saturating_sub(pending.challenges.len());
            }
        }
        expired.len()
    }
}

pub trait ReachabilityLockFreePreparation {
    fn prepare_bootstrap<'a>(
        &'a self,
        bytes: &'a [u8],
        source: &'a ConfiguredBootstrapSource,
        now: u64,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<PreparedBootstrapManifest, RelayAdmissionError>>;
    fn prepare_descriptor<'a>(
        &'a self,
        bytes: &'a [u8],
        now: u64,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<PreparedRelayDescriptorAdmission, RelayAdmissionError>>;
    fn prepare_advertisement<'a>(
        &'a self,
        bytes: &'a [u8],
        target: &'a KnownPeerIdentity,
        admitted_reservations: &'a [ValidatedRelayReservation],
        now: u64,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<PreparedReachabilityAdvertisement, RelayAdmissionError>>;
}

impl ReachabilityLockFreePreparation for ReachabilityAdmissionPreparer {
    fn prepare_bootstrap<'a>(
        &'a self,
        bytes: &'a [u8],
        source: &'a ConfiguredBootstrapSource,
        now: u64,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<PreparedBootstrapManifest, RelayAdmissionError>> {
        Box::pin(async move {
            prepare_bootstrap_impl(self.resolver.as_ref(), bytes, source, now, deadline)
        })
    }
    fn prepare_descriptor<'a>(
        &'a self,
        bytes: &'a [u8],
        now: u64,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<PreparedRelayDescriptorAdmission, RelayAdmissionError>> {
        Box::pin(
            async move { prepare_descriptor_impl(self.resolver.as_ref(), bytes, now, deadline) },
        )
    }
    fn prepare_advertisement<'a>(
        &'a self,
        bytes: &'a [u8],
        target: &'a KnownPeerIdentity,
        admitted_reservations: &'a [ValidatedRelayReservation],
        now: u64,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<PreparedReachabilityAdvertisement, RelayAdmissionError>> {
        Box::pin(async move {
            prepare_advertisement_impl(
                self.resolver.as_ref(),
                bytes,
                target,
                admitted_reservations,
                now,
                deadline,
            )
        })
    }
}

pub trait ReachabilityRecordAdmission {
    fn register_prepared_bootstrap(
        &mut self,
        prepared: PreparedBootstrapManifest,
        source: &ConfiguredBootstrapSource,
        now: u64,
    ) -> Result<ValidatedBootstrapManifest, RelayAdmissionError>;
    fn register_prepared_descriptor(
        &mut self,
        prepared: PreparedRelayDescriptorAdmission,
        verifier_context: [u8; 32],
        now: u64,
    ) -> Result<PendingRelayDescriptorAdmission, RelayAdmissionError>;
    fn complete_descriptor_admission(
        &mut self,
        pending: PendingRelayDescriptorAdmission,
        proofs: &[RelayPossessionProofV1],
        now: u64,
    ) -> Result<ValidatedRelayDescriptor, RelayAdmissionError>;
    fn abort_descriptor_admission(
        &mut self,
        pending: PendingRelayDescriptorAdmission,
    ) -> Result<(), RelayAdmissionError>;
    fn admit_reservation(
        &mut self,
        bytes: &[u8],
        target: &KnownPeerIdentity,
        relay: &KnownPeerIdentity,
        now: u64,
    ) -> Result<ValidatedRelayReservation, RelayAdmissionError>;
    fn register_prepared_advertisement(
        &mut self,
        prepared: PreparedReachabilityAdvertisement,
        target: &KnownPeerIdentity,
        admitted_reservations: &[ValidatedRelayReservation],
        now: u64,
    ) -> Result<ValidatedReachabilityAdvertisement, RelayAdmissionError>;
}

impl ReachabilityRecordAdmission for ReachabilityAdmission {
    fn register_prepared_bootstrap(
        &mut self,
        prepared: PreparedBootstrapManifest,
        source: &ConfiguredBootstrapSource,
        now: u64,
    ) -> Result<ValidatedBootstrapManifest, RelayAdmissionError> {
        freshness(
            prepared.canonical.issued_at,
            prepared.canonical.expires_at,
            now,
        )?;
        if prepared.source_authority_digest != source.local_authority_digest
            || prepared.prepared_at > now
        {
            return Err(RelayAdmissionError::AuthorityMismatch);
        }
        self.replay.check_and_advance_sequence(
            sequence_key(
                ReachabilitySequenceKindV1::BootstrapManifest,
                source.identity.public_key,
            ),
            prepared.canonical.sequence,
            prepared.digest,
            prepared.canonical.expires_at,
        )?;
        Ok(ValidatedBootstrapManifest {
            canonical: prepared.canonical,
            digest: prepared.digest,
            resolved_endpoints: prepared.resolved_endpoints,
        })
    }

    fn register_prepared_descriptor(
        &mut self,
        prepared: PreparedRelayDescriptorAdmission,
        verifier_context: [u8; 32],
        now: u64,
    ) -> Result<PendingRelayDescriptorAdmission, RelayAdmissionError> {
        freshness(
            prepared.canonical.issued_at,
            prepared.canonical.expires_at,
            now,
        )?;
        if prepared.prepared_at > now {
            return Err(RelayAdmissionError::NotYetValid);
        }
        if self.pending_descriptors.contains_key(&prepared.digest) {
            return Err(RelayAdmissionError::Replay);
        }
        if self.pending_descriptors.len() >= MAX_PENDING_DESCRIPTORS
            || self.pending_challenges + prepared.canonical.endpoints.len() > MAX_PENDING_CHALLENGES
        {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        let key = sequence_key(
            ReachabilitySequenceKindV1::RelayDescriptor,
            prepared.canonical.relay_public_key,
        );
        self.replay.check_sequence_candidate(
            key,
            prepared.canonical.sequence,
            prepared.canonical.previous_descriptor_blake3,
        )?;
        let mut challenges = Vec::with_capacity(prepared.canonical.endpoints.len());
        for (index, endpoint) in prepared.canonical.endpoints.iter().enumerate() {
            let mut nonce = [0; 32];
            OsRng.fill_bytes(&mut nonce);
            challenges.push(RelayPossessionChallengeV1 {
                relay_node_id: prepared.canonical.relay_node_id,
                descriptor_digest: prepared.digest,
                endpoint_index: index as u64,
                transport: endpoint.transport,
                verifier_context,
                nonce,
                issued_at: now,
                expires_at: now + POSSESSION_VALIDITY_SECONDS,
            });
        }
        let pending = PendingRelayDescriptorAdmission {
            canonical: prepared.canonical,
            digest: prepared.digest,
            resolved_endpoints: prepared.resolved_endpoints,
            challenges,
            expires_at: now + POSSESSION_VALIDITY_SECONDS,
        };
        self.pending_challenges += pending.challenges.len();
        self.pending_descriptors
            .insert(pending.digest, pending.clone());
        Ok(pending)
    }

    fn complete_descriptor_admission(
        &mut self,
        pending: PendingRelayDescriptorAdmission,
        proofs: &[RelayPossessionProofV1],
        now: u64,
    ) -> Result<ValidatedRelayDescriptor, RelayAdmissionError> {
        let stored = self
            .pending_descriptors
            .get(&pending.digest)
            .ok_or(RelayAdmissionError::ChallengeMissing)?;
        if stored != &pending {
            return Err(RelayAdmissionError::PossessionInvalid);
        }
        if now > pending.expires_at {
            return Err(RelayAdmissionError::ChallengeExpired);
        }
        if proofs.len() != pending.challenges.len() {
            return Err(RelayAdmissionError::PossessionInvalid);
        }
        let key = VerifyingKey::from_bytes(&pending.canonical.relay_public_key)
            .map_err(|_| RelayAdmissionError::SignatureInvalid)?;
        let mut seen = BTreeSet::new();
        let mut bindings = Vec::with_capacity(proofs.len());
        for (challenge, proof) in pending.challenges.iter().zip(proofs) {
            let challenge_digest = possession_challenge_digest(challenge);
            if proof.challenge_digest != challenge_digest || !seen.insert(challenge_digest) {
                return Err(RelayAdmissionError::PossessionInvalid);
            }
            verify_signature(
                &key,
                &possession_proof_signing_bytes(challenge, proof.connection_binding_digest),
                proof.signature,
            )?;
            self.replay.consume_nonce(
                ReachabilityNonceDomainV1::PossessionChallenge,
                pending.digest,
                challenge.nonce,
                challenge.expires_at,
            )?;
            bindings.push(proof.connection_binding_digest);
        }
        self.replay.compare_and_advance_sequence(
            sequence_key(
                ReachabilitySequenceKindV1::RelayDescriptor,
                pending.canonical.relay_public_key,
            ),
            pending.canonical.previous_descriptor_blake3,
            pending.canonical.sequence,
            pending.digest,
            pending.canonical.expires_at,
        )?;
        self.pending_descriptors.remove(&pending.digest);
        self.pending_challenges -= pending.challenges.len();
        Ok(ValidatedRelayDescriptor {
            canonical: pending.canonical,
            digest: pending.digest,
            resolved_endpoints: pending.resolved_endpoints,
            possession_connection_bindings: bindings,
            possession_verified_at: now,
        })
    }

    fn abort_descriptor_admission(
        &mut self,
        pending: PendingRelayDescriptorAdmission,
    ) -> Result<(), RelayAdmissionError> {
        let stored = self
            .pending_descriptors
            .get(&pending.digest)
            .ok_or(RelayAdmissionError::ChallengeMissing)?;
        if stored != &pending {
            return Err(RelayAdmissionError::PossessionInvalid);
        }
        self.pending_descriptors.remove(&pending.digest);
        self.pending_challenges = self
            .pending_challenges
            .checked_sub(pending.challenges.len())
            .ok_or(RelayAdmissionError::StateUnavailable)?;
        Ok(())
    }

    fn admit_reservation(
        &mut self,
        bytes: &[u8],
        target: &KnownPeerIdentity,
        relay: &KnownPeerIdentity,
        now: u64,
    ) -> Result<ValidatedRelayReservation, RelayAdmissionError> {
        target.validate()?;
        relay.validate()?;
        let object = decode_reachability_object(bytes).map_err(|_| RelayAdmissionError::Codec)?;
        let ReachabilityObjectV1::RelayReservation(reservation) = object else {
            return Err(RelayAdmissionError::Codec);
        };
        if reservation.target_node_id != target.node_id
            || reservation.relay_node_id != relay.node_id
        {
            return Err(RelayAdmissionError::IdentityMismatch);
        }
        freshness(reservation.issued_at, reservation.expires_at, now)?;
        verify_object_signature(
            &ReachabilityObjectV1::RelayReservation(reservation.clone()),
            ReachabilitySignatureRoleV1::ReservationTarget,
            target.public_key,
            reservation.target_signature,
        )?;
        verify_object_signature(
            &ReachabilityObjectV1::RelayReservation(reservation.clone()),
            ReachabilitySignatureRoleV1::ReservationRelay,
            relay.public_key,
            reservation.relay_signature,
        )?;
        let digest = *blake3::hash(bytes).as_bytes();
        self.replay.check_and_store_reservation(
            reservation.relay_node_id,
            reservation.target_node_id,
            reservation.reservation_id,
            digest,
            reservation.expires_at,
        )?;
        Ok(ValidatedRelayReservation {
            canonical: reservation,
            digest,
        })
    }

    fn register_prepared_advertisement(
        &mut self,
        prepared: PreparedReachabilityAdvertisement,
        target: &KnownPeerIdentity,
        admitted_reservations: &[ValidatedRelayReservation],
        now: u64,
    ) -> Result<ValidatedReachabilityAdvertisement, RelayAdmissionError> {
        target.validate()?;
        freshness(
            prepared.canonical.issued_at,
            prepared.canonical.expires_at,
            now,
        )?;
        if prepared.prepared_at > now || prepared.canonical.target_node_id != target.node_id {
            return Err(RelayAdmissionError::IdentityMismatch);
        }
        let expected: Vec<_> = admitted_reservations
            .iter()
            .map(|value| *value.digest())
            .collect();
        if expected != prepared.reservation_digests {
            return Err(RelayAdmissionError::ReservationMismatch);
        }
        self.replay.check_and_advance_sequence(
            sequence_key(ReachabilitySequenceKindV1::Advertisement, target.public_key),
            prepared.canonical.sequence,
            prepared.digest,
            prepared.canonical.expires_at,
        )?;
        Ok(ValidatedReachabilityAdvertisement {
            canonical: prepared.canonical,
            digest: prepared.digest,
            reservations: admitted_reservations.to_vec(),
            resolved_public_candidates: prepared.resolved_public_candidates,
        })
    }
}

pub trait ReachabilityLockFreeDialValidation {
    fn validate_configured_bootstrap_dial<'a>(
        &'a self,
        source: &'a ConfiguredBootstrapSource,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
    fn validate_bootstrap_dial<'a>(
        &'a self,
        object: &'a ValidatedBootstrapManifest,
        endpoint_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
    fn validate_possession_dial<'a>(
        &'a self,
        pending: &'a PendingRelayDescriptorAdmission,
        endpoint_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPossessionDialEndpoint, RelayAdmissionError>>;
    fn validate_relay_dial<'a>(
        &'a self,
        object: &'a ValidatedRelayDescriptor,
        endpoint_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
    fn validate_public_candidate_dial<'a>(
        &'a self,
        object: &'a ValidatedReachabilityAdvertisement,
        candidate_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>>;
}

impl ReachabilityLockFreeDialValidation for ReachabilityDialValidator {
    fn validate_configured_bootstrap_dial<'a>(
        &'a self,
        source: &'a ConfiguredBootstrapSource,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>> {
        Box::pin(async move {
            let admitted = resolve_one(
                self.resolver.as_ref(),
                0,
                &source.fetch_endpoint.host,
                source.fetch_endpoint.port,
                0,
                u64::MAX,
                deadline,
            )?;
            public_dial(
                PublicDialRequest {
                    source_digest: source.local_authority_digest,
                    endpoint_index: 0,
                    host: &source.fetch_endpoint.host,
                    port: source.fetch_endpoint.port,
                    transport: discovery_dial_transport(source.fetch_endpoint.transport),
                    signed_path: Some(source.fetch_endpoint.path.clone()),
                },
                &admitted,
                self.resolver.as_ref(),
                deadline,
            )
        })
    }
    fn validate_bootstrap_dial<'a>(
        &'a self,
        object: &'a ValidatedBootstrapManifest,
        endpoint_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>> {
        Box::pin(async move {
            let endpoint = object
                .canonical
                .discovery_endpoints
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            let admitted = object
                .resolved_endpoints
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            public_dial(
                PublicDialRequest {
                    source_digest: object.digest,
                    endpoint_index,
                    host: &endpoint.host,
                    port: endpoint.port,
                    transport: discovery_dial_transport(endpoint.transport),
                    signed_path: Some(endpoint.path.clone()),
                },
                admitted,
                self.resolver.as_ref(),
                deadline,
            )
        })
    }
    fn validate_possession_dial<'a>(
        &'a self,
        pending: &'a PendingRelayDescriptorAdmission,
        endpoint_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPossessionDialEndpoint, RelayAdmissionError>> {
        Box::pin(async move {
            let endpoint = pending
                .canonical
                .endpoints
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            let challenge = pending
                .challenges
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::ChallengeMissing)?
                .clone();
            let admitted = pending
                .resolved_endpoints
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            let current = resolve_one(
                self.resolver.as_ref(),
                endpoint_index,
                &endpoint.host,
                endpoint.port,
                admitted.resolved_at,
                admitted.expires_at,
                deadline,
            )?;
            ensure_same_addresses(admitted, &current)?;
            Ok(ValidatedPossessionDialEndpoint {
                pending_descriptor_digest: pending.digest,
                pending_relay_public_key: pending.canonical.relay_public_key,
                challenge_digest: possession_challenge_digest(&challenge),
                challenge,
                endpoint_index,
                signed_host: endpoint.host.clone(),
                port: endpoint.port,
                transport: endpoint.transport,
                admitted_addresses: admitted.addresses.clone(),
                dial_addresses: socket_addresses(&current.addresses, endpoint.port),
                expires_at: pending.expires_at,
            })
        })
    }
    fn validate_relay_dial<'a>(
        &'a self,
        object: &'a ValidatedRelayDescriptor,
        endpoint_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>> {
        Box::pin(async move {
            let endpoint = object
                .canonical
                .endpoints
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            let admitted = object
                .resolved_endpoints
                .get(endpoint_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            public_dial(
                PublicDialRequest {
                    source_digest: object.digest,
                    endpoint_index,
                    host: &endpoint.host,
                    port: endpoint.port,
                    transport: relay_dial_transport(endpoint.transport),
                    signed_path: None,
                },
                admitted,
                self.resolver.as_ref(),
                deadline,
            )
        })
    }
    fn validate_public_candidate_dial<'a>(
        &'a self,
        object: &'a ValidatedReachabilityAdvertisement,
        candidate_index: usize,
        deadline: Instant,
    ) -> AdmissionFuture<'a, Result<ValidatedPublicDialEndpoint, RelayAdmissionError>> {
        Box::pin(async move {
            let candidate = object
                .canonical
                .optional_public_candidates
                .get(candidate_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            let admitted = object
                .resolved_public_candidates
                .get(candidate_index)
                .ok_or(RelayAdmissionError::EndpointMissing)?;
            public_dial(
                PublicDialRequest {
                    source_digest: object.digest,
                    endpoint_index: candidate_index,
                    host: &candidate.endpoint.host,
                    port: candidate.endpoint.port,
                    transport: ValidatedPublicDialTransportV1::DirectQuicUdp,
                    signed_path: None,
                },
                admitted,
                self.resolver.as_ref(),
                deadline,
            )
        })
    }
}

fn prepare_descriptor_impl(
    resolver: &dyn PublicEndpointResolver,
    bytes: &[u8],
    now: u64,
    deadline: Instant,
) -> Result<PreparedRelayDescriptorAdmission, RelayAdmissionError> {
    let object = decode_reachability_object(bytes).map_err(|_| RelayAdmissionError::Codec)?;
    let ReachabilityObjectV1::RelayDescriptor(canonical) = object else {
        return Err(RelayAdmissionError::Codec);
    };
    if principal_node_id(&canonical.relay_public_key) != canonical.relay_node_id {
        return Err(RelayAdmissionError::IdentityMismatch);
    };
    freshness(canonical.issued_at, canonical.expires_at, now)?;
    verify_object_signature(
        &ReachabilityObjectV1::RelayDescriptor(canonical.clone()),
        ReachabilitySignatureRoleV1::RelayDescriptor,
        canonical.relay_public_key,
        canonical.relay_signature,
    )?;
    let resolved = resolve_relay_endpoints(resolver, &canonical, now, deadline)?;
    Ok(PreparedRelayDescriptorAdmission {
        canonical,
        digest: *blake3::hash(bytes).as_bytes(),
        resolved_endpoints: resolved,
        prepared_at: now,
    })
}
fn prepare_bootstrap_impl(
    resolver: &dyn PublicEndpointResolver,
    bytes: &[u8],
    source: &ConfiguredBootstrapSource,
    now: u64,
    deadline: Instant,
) -> Result<PreparedBootstrapManifest, RelayAdmissionError> {
    let object = decode_reachability_object(bytes).map_err(|_| RelayAdmissionError::Codec)?;
    let ReachabilityObjectV1::BootstrapManifest(canonical) = object else {
        return Err(RelayAdmissionError::Codec);
    };
    if canonical.discovery_source_id != source.identity.source_id {
        return Err(RelayAdmissionError::IdentityMismatch);
    };
    freshness(canonical.issued_at, canonical.expires_at, now)?;
    verify_object_signature(
        &ReachabilityObjectV1::BootstrapManifest(canonical.clone()),
        ReachabilitySignatureRoleV1::BootstrapSource,
        source.identity.public_key,
        canonical.source_signature,
    )?;
    let mut total = 0;
    let mut resolved = Vec::new();
    for (index, e) in canonical.discovery_endpoints.iter().enumerate() {
        let item = resolve_one(
            resolver,
            index,
            &e.host,
            e.port,
            now,
            canonical.expires_at,
            deadline,
        )?;
        total += item.addresses.len();
        if total > MAX_RESOLVED_PER_OBJECT {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        resolved.push(item);
    }
    Ok(PreparedBootstrapManifest {
        canonical,
        digest: *blake3::hash(bytes).as_bytes(),
        source_authority_digest: source.local_authority_digest,
        resolved_endpoints: resolved,
        prepared_at: now,
    })
}
fn prepare_advertisement_impl(
    resolver: &dyn PublicEndpointResolver,
    bytes: &[u8],
    target: &KnownPeerIdentity,
    admitted: &[ValidatedRelayReservation],
    now: u64,
    deadline: Instant,
) -> Result<PreparedReachabilityAdvertisement, RelayAdmissionError> {
    target.validate()?;
    let object = decode_reachability_object(bytes).map_err(|_| RelayAdmissionError::Codec)?;
    let ReachabilityObjectV1::Advertisement(canonical) = object else {
        return Err(RelayAdmissionError::Codec);
    };
    if canonical.target_node_id != target.node_id {
        return Err(RelayAdmissionError::IdentityMismatch);
    }
    freshness(canonical.issued_at, canonical.expires_at, now)?;
    verify_object_signature(
        &ReachabilityObjectV1::Advertisement(canonical.clone()),
        ReachabilitySignatureRoleV1::AdvertisementTarget,
        target.public_key,
        canonical.target_signature,
    )?;
    if canonical.relay_reservations.len() != admitted.len() {
        return Err(RelayAdmissionError::ReservationMismatch);
    }
    let mut digests = Vec::new();
    for (raw, valid) in canonical.relay_reservations.iter().zip(admitted) {
        if raw != valid.canonical() {
            return Err(RelayAdmissionError::ReservationMismatch);
        }
        if valid.canonical().target_node_id != target.node_id {
            return Err(RelayAdmissionError::ReservationMismatch);
        }
        freshness(
            valid.canonical().issued_at,
            valid.canonical().expires_at,
            now,
        )?;
        digests.push(*valid.digest());
    }
    let unique_ids: BTreeSet<_> = canonical
        .relay_reservations
        .iter()
        .map(|reservation| reservation.reservation_id)
        .collect();
    if unique_ids.len() != canonical.relay_reservations.len() {
        return Err(RelayAdmissionError::ReservationMismatch);
    }
    let mut total = 0;
    let mut resolved = Vec::new();
    for (index, c) in canonical.optional_public_candidates.iter().enumerate() {
        let item = resolve_one(
            resolver,
            index,
            &c.endpoint.host,
            c.endpoint.port,
            now,
            canonical.expires_at,
            deadline,
        )?;
        total += item.addresses.len();
        if total > MAX_RESOLVED_PER_OBJECT {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        resolved.push(item);
    }
    Ok(PreparedReachabilityAdvertisement {
        canonical,
        digest: *blake3::hash(bytes).as_bytes(),
        reservation_digests: digests,
        resolved_public_candidates: resolved,
        prepared_at: now,
    })
}

fn resolve_relay_endpoints(
    resolver: &dyn PublicEndpointResolver,
    descriptor: &RelayDescriptorV1,
    now: u64,
    deadline: Instant,
) -> Result<Vec<ResolvedPublicEndpointV1>, RelayAdmissionError> {
    let mut total = 0;
    let mut output = Vec::new();
    for (index, e) in descriptor.endpoints.iter().enumerate() {
        let item = resolve_one(
            resolver,
            index,
            &e.host,
            e.port,
            now,
            descriptor.expires_at,
            deadline,
        )?;
        total += item.addresses.len();
        if total > MAX_RESOLVED_PER_OBJECT {
            return Err(RelayAdmissionError::BudgetExceeded);
        }
        output.push(item);
    }
    Ok(output)
}
fn resolve_one(
    resolver: &dyn PublicEndpointResolver,
    index: usize,
    host: &HostAddressV1,
    port: u16,
    now: u64,
    expires_at: u64,
    deadline: Instant,
) -> Result<ResolvedPublicEndpointV1, RelayAdmissionError> {
    if port == 0 {
        return Err(RelayAdmissionError::EndpointNotGlobal);
    }
    let raw = resolver.resolve(host, deadline)?;
    if raw.is_empty() {
        return Err(RelayAdmissionError::DnsResolutionFailed);
    }
    if raw.len() > MAX_RESOLVED_PER_ENDPOINT {
        return Err(RelayAdmissionError::BudgetExceeded);
    }
    for address in &raw {
        validate_public_ip(*address)?;
    }
    let mut addresses = raw;
    addresses.sort();
    addresses.dedup();
    Ok(ResolvedPublicEndpointV1 {
        endpoint_index: index,
        addresses,
        resolved_at: now,
        expires_at,
    })
}
struct PublicDialRequest<'a> {
    source_digest: [u8; 32],
    endpoint_index: usize,
    host: &'a HostAddressV1,
    port: u16,
    transport: ValidatedPublicDialTransportV1,
    signed_path: Option<String>,
}

fn public_dial(
    request: PublicDialRequest<'_>,
    admitted: &ResolvedPublicEndpointV1,
    resolver: &dyn PublicEndpointResolver,
    deadline: Instant,
) -> Result<ValidatedPublicDialEndpoint, RelayAdmissionError> {
    let current = resolve_one(
        resolver,
        request.endpoint_index,
        request.host,
        request.port,
        admitted.resolved_at,
        admitted.expires_at,
        deadline,
    )?;
    ensure_same_addresses(admitted, &current)?;
    Ok(ValidatedPublicDialEndpoint {
        source_digest: request.source_digest,
        endpoint_index: request.endpoint_index,
        signed_host: request.host.clone(),
        port: request.port,
        transport: request.transport,
        signed_path: request.signed_path,
        admitted_addresses: admitted.addresses.clone(),
        dial_addresses: socket_addresses(&current.addresses, request.port),
        expires_at: admitted.expires_at,
    })
}
fn ensure_same_addresses(
    a: &ResolvedPublicEndpointV1,
    b: &ResolvedPublicEndpointV1,
) -> Result<(), RelayAdmissionError> {
    if a.addresses == b.addresses {
        Ok(())
    } else {
        Err(RelayAdmissionError::DnsRebinding)
    }
}
fn socket_addresses(addresses: &[IpAddr], port: u16) -> Vec<SocketAddr> {
    addresses
        .iter()
        .map(|ip| SocketAddr::new(*ip, port))
        .collect()
}
fn discovery_dial_transport(v: DiscoveryTransportV1) -> ValidatedPublicDialTransportV1 {
    match v {
        DiscoveryTransportV1::Https => ValidatedPublicDialTransportV1::BootstrapHttps,
        DiscoveryTransportV1::RendezvousQuic => {
            ValidatedPublicDialTransportV1::BootstrapRendezvousQuic
        }
    }
}
fn relay_dial_transport(v: RelayTransportV1) -> ValidatedPublicDialTransportV1 {
    match v {
        RelayTransportV1::QuicUdp => ValidatedPublicDialTransportV1::RelayQuicUdp,
        RelayTransportV1::TlsTcp443 => ValidatedPublicDialTransportV1::RelayTlsTcp443,
    }
}

pub fn possession_challenge_digest(challenge: &RelayPossessionChallengeV1) -> [u8; 32] {
    *blake3::hash(&possession_challenge_bytes(challenge)).as_bytes()
}
pub fn possession_proof_signing_bytes(
    challenge: &RelayPossessionChallengeV1,
    connection_binding_digest: [u8; 32],
) -> Vec<u8> {
    let mut output = Vec::with_capacity(POSSESSION_DOMAIN.len() + 64);
    output.extend_from_slice(POSSESSION_DOMAIN);
    output.extend_from_slice(&possession_challenge_digest(challenge));
    output.extend_from_slice(&connection_binding_digest);
    output
}
fn possession_challenge_bytes(c: &RelayPossessionChallengeV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(177);
    out.extend_from_slice(c.relay_node_id.as_bytes());
    out.extend_from_slice(&c.descriptor_digest);
    out.extend_from_slice(&c.endpoint_index.to_be_bytes());
    out.push(match c.transport {
        RelayTransportV1::QuicUdp => 1,
        RelayTransportV1::TlsTcp443 => 2,
    });
    out.extend_from_slice(&c.verifier_context);
    out.extend_from_slice(&c.nonce);
    out.extend_from_slice(&c.issued_at.to_be_bytes());
    out.extend_from_slice(&c.expires_at.to_be_bytes());
    out
}
fn verify_object_signature(
    object: &ReachabilityObjectV1,
    role: ReachabilitySignatureRoleV1,
    public_key: [u8; 32],
    signature: [u8; 64],
) -> Result<(), RelayAdmissionError> {
    let key =
        VerifyingKey::from_bytes(&public_key).map_err(|_| RelayAdmissionError::SignatureInvalid)?;
    let preimage =
        reachability_signing_bytes(object, role).map_err(|_| RelayAdmissionError::Codec)?;
    verify_signature(&key, &preimage, signature)
}
fn verify_signature(
    key: &VerifyingKey,
    message: &[u8],
    signature: [u8; 64],
) -> Result<(), RelayAdmissionError> {
    key.verify(message, &Signature::from_bytes(&signature))
        .map_err(|_| RelayAdmissionError::SignatureInvalid)
}
fn freshness(issued: u64, expires: u64, now: u64) -> Result<(), RelayAdmissionError> {
    if issued > now.saturating_add(MAX_CLOCK_SKEW_SECONDS) {
        Err(RelayAdmissionError::NotYetValid)
    } else if now > expires {
        Err(RelayAdmissionError::Expired)
    } else {
        Ok(())
    }
}
fn sequence_key(kind: ReachabilitySequenceKindV1, signer: [u8; 32]) -> ReachabilitySequenceKeyV1 {
    ReachabilitySequenceKeyV1 {
        kind,
        signer,
        scope: [0; 32],
    }
}

fn validate_public_host(host: &HostAddressV1) -> Result<(), RelayAdmissionError> {
    match host {
        HostAddressV1::Ipv4(value) => validate_public_ip(IpAddr::V4(Ipv4Addr::from(*value))),
        HostAddressV1::Ipv6(value) => validate_public_ip(IpAddr::V6(Ipv6Addr::from(*value))),
        HostAddressV1::Dns(value) if valid_dns(value) => Ok(()),
        _ => Err(RelayAdmissionError::EndpointNotGlobal),
    }
}
fn validate_public_ip(ip: IpAddr) -> Result<(), RelayAdmissionError> {
    let invalid = match ip {
        IpAddr::V4(ip) => {
            let x = ip.octets();
            ip.is_private()
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
        }
        IpAddr::V6(ip) => {
            let x = ip.octets();
            ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unspecified()
                || (x[0] & 0xfe) == 0xfc
                || (x[0] == 0xfe && (x[1] & 0xc0) == 0x80)
                || (x[0] == 0x20 && x[1] == 0x01 && x[2] == 0x0d && x[3] == 0xb8)
        }
    };
    if invalid {
        Err(RelayAdmissionError::EndpointNotGlobal)
    } else {
        Ok(())
    }
}
fn valid_dns(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.is_ascii()
        && value == value.to_ascii_lowercase()
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}
fn parse_configured_host(value: &str) -> Result<HostAddressV1, RelayAdmissionError> {
    if let Some(value) = value.strip_prefix("dns:") {
        if valid_dns(value) {
            Ok(HostAddressV1::Dns(value.to_owned()))
        } else {
            Err(RelayAdmissionError::TrustedBootstrapInvalid)
        }
    } else if let Some(value) = value.strip_prefix("ipv4:") {
        value
            .parse::<Ipv4Addr>()
            .map(|ip| HostAddressV1::Ipv4(ip.octets()))
            .map_err(|_| RelayAdmissionError::TrustedBootstrapInvalid)
    } else if let Some(value) = value.strip_prefix("ipv6:") {
        value
            .parse::<Ipv6Addr>()
            .map(|ip| HostAddressV1::Ipv6(ip.octets()))
            .map_err(|_| RelayAdmissionError::TrustedBootstrapInvalid)
    } else {
        Err(RelayAdmissionError::TrustedBootstrapInvalid)
    }
}
fn decode_hex_32(value: &str) -> Result<[u8; 32], RelayAdmissionError> {
    if value.len() != 64 {
        return Err(RelayAdmissionError::TrustedBootstrapInvalid);
    }
    let mut out = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        out[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(out)
}
fn hex_digit(value: u8) -> Result<u8, RelayAdmissionError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(RelayAdmissionError::TrustedBootstrapInvalid),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayAdmissionError {
    Codec,
    IdentityMismatch,
    SignatureInvalid,
    EndpointNotGlobal,
    DnsResolutionFailed,
    DnsRebinding,
    NotYetValid,
    Expired,
    SequenceRollback,
    Replay,
    ChallengeMissing,
    ChallengeExpired,
    ChallengeConsumed,
    PossessionInvalid,
    BudgetExceeded,
    ReservationIdReuse,
    ReservationMismatch,
    EndpointMissing,
    AuthorityMismatch,
    TrustedBootstrapUnavailable,
    TrustedBootstrapInvalid,
    StateUnavailable,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityCryptoError {
    SignerUnavailable,
}
impl std::fmt::Display for RelayAdmissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_REACHABILITY_ADMISSION: {self:?}")
    }
}
impl std::error::Error for RelayAdmissionError {}
impl std::fmt::Display for ReachabilityCryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OBP_REACHABILITY_CRYPTO: {self:?}")
    }
}
impl std::error::Error for ReachabilityCryptoError {}
