//! Opaque, bounded datagram framing and association-bound forwarding.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::time::timeout;

use crate::service::BudgetLease;
use crate::tcp443_relay::{extract_ed25519_spki, SpkiPinVerifier};
use crate::{install_aws_lc_provider, RelayGlobalBudget, RelayIdentityCertificate};

pub const MAX_INNER_DATAGRAM_BYTES: usize = 1_350;
pub const MAX_FRAGMENTS: usize = 8;
pub const MAX_IN_FLIGHT_PER_ASSOCIATION: usize = 64;
pub const REASSEMBLY_EXPIRY_SECONDS: u64 = 2;
const MAGIC: [u8; 4] = *b"OBPR";
const VERSION: u8 = 1;
const HEADER_BYTES: usize = 60;

pub struct UdpRelayListener {
    endpoint: Endpoint,
}

impl UdpRelayListener {
    pub fn bind(
        address: SocketAddr,
        identity: &RelayIdentityCertificate,
    ) -> Result<Self, RelayDataPlaneError> {
        install_aws_lc_provider()?;
        let certificate = CertificateDer::from(identity.certificate_der().to_vec());
        if extract_ed25519_spki(&certificate)? != identity.spki_ed25519() {
            return Err(RelayDataPlaneError::IdentityMismatch);
        }
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            identity.private_key_der().to_vec(),
        ));
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        tls.alpn_protocols = vec![b"obp-relay/1".to_vec()];
        let mut server = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls)
                .map_err(|_| RelayDataPlaneError::IdentityMismatch)?,
        ));
        let mut transport = quinn::TransportConfig::default();
        transport.datagram_receive_buffer_size(Some(1024 * 1024));
        transport.datagram_send_buffer_size(1024 * 1024);
        server.transport_config(Arc::new(transport));
        let endpoint =
            Endpoint::server(server, address).map_err(|_| RelayDataPlaneError::Closed)?;
        Ok(Self { endpoint })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RelayDataPlaneError> {
        self.endpoint
            .local_addr()
            .map_err(|_| RelayDataPlaneError::Closed)
    }

    pub async fn accept_echo_once(&self) -> Result<(), RelayDataPlaneError> {
        let incoming = timeout(Duration::from_secs(5), self.endpoint.accept())
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .ok_or(RelayDataPlaneError::Closed)?;
        let connection = timeout(Duration::from_secs(5), incoming)
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        let payload = timeout(Duration::from_secs(5), connection.read_datagram())
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .map_err(|_| RelayDataPlaneError::Closed)?;
        connection
            .send_datagram_wait(payload)
            .await
            .map_err(|_| RelayDataPlaneError::Closed)?;
        // DATAGRAM send completion means that Quinn accepted the frame, not
        // that the peer has received it. Keep this one-shot test listener
        // alive long enough for the endpoint driver to flush the packet.
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok(())
    }
}

pub async fn udp_pinned_round_trip(
    address: SocketAddr,
    expected_spki: [u8; 32],
    payload: &[u8],
) -> Result<Vec<u8>, RelayDataPlaneError> {
    install_aws_lc_provider()?;
    let bind: SocketAddr = if address.is_ipv6() {
        "[::]:0".parse().map_err(|_| RelayDataPlaneError::Closed)?
    } else {
        "0.0.0.0:0"
            .parse()
            .map_err(|_| RelayDataPlaneError::Closed)?
    };
    let mut endpoint = Endpoint::client(bind).map_err(|_| RelayDataPlaneError::Closed)?;
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SpkiPinVerifier::new(expected_spki)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"obp-relay/1".to_vec()];
    let mut client = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?,
    ));
    let mut transport = quinn::TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(1024 * 1024));
    transport.datagram_send_buffer_size(1024 * 1024);
    client.transport_config(Arc::new(transport));
    endpoint.set_default_client_config(client);
    let connecting = endpoint
        .connect(address, "relay.onebrain")
        .map_err(|_| RelayDataPlaneError::Closed)?;
    let connection = timeout(Duration::from_secs(5), connecting)
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
    let max = connection
        .max_datagram_size()
        .ok_or(RelayDataPlaneError::NoDatagramSupport)?;
    if payload.is_empty() || payload.len() > max {
        return Err(RelayDataPlaneError::Oversize);
    }
    connection
        .send_datagram(payload.to_vec().into())
        .map_err(|_| RelayDataPlaneError::Closed)?;
    let response = timeout(Duration::from_secs(5), connection.read_datagram())
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::Closed)?;
    endpoint.close(0u32.into(), b"done");
    Ok(response.to_vec())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DatagramDirectionV1 {
    InitiatorToTarget,
    TargetToInitiator,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueDatagramEnvelopeV1 {
    association_id: [u8; 32],
    direction: DatagramDirectionV1,
    datagram_sequence: u64,
    message_id: u64,
    fragment_index: u8,
    fragment_count: u8,
    total_plaintext_length: u16,
    payload: Vec<u8>,
}

impl OpaqueDatagramEnvelopeV1 {
    pub fn fragment(
        association_id: [u8; 32],
        direction: DatagramDirectionV1,
        datagram_sequence: u64,
        message_id: u64,
        payload: &[u8],
        max_fragment_payload: usize,
    ) -> Result<Vec<Self>, RelayDataPlaneError> {
        if association_id == [0; 32]
            || datagram_sequence == 0
            || message_id == 0
            || payload.is_empty()
            || payload.len() > MAX_INNER_DATAGRAM_BYTES
            || max_fragment_payload == 0
        {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let count = payload.len().div_ceil(max_fragment_payload);
        if count == 0 || count > MAX_FRAGMENTS {
            return Err(RelayDataPlaneError::TooManyFragments);
        }
        payload
            .chunks(max_fragment_payload)
            .enumerate()
            .map(|(index, part)| {
                Ok(Self {
                    association_id,
                    direction,
                    datagram_sequence,
                    message_id,
                    fragment_index: index as u8,
                    fragment_count: count as u8,
                    total_plaintext_length: payload.len() as u16,
                    payload: part.to_vec(),
                })
            })
            .collect()
    }

    pub fn encode(&self) -> Result<Vec<u8>, RelayDataPlaneError> {
        validate_envelope(self)?;
        let mut output = Vec::with_capacity(HEADER_BYTES + self.payload.len());
        output.extend_from_slice(&MAGIC);
        output.push(VERSION);
        output.extend_from_slice(&self.association_id);
        output.push(match self.direction {
            DatagramDirectionV1::InitiatorToTarget => 0,
            DatagramDirectionV1::TargetToInitiator => 1,
        });
        output.extend_from_slice(&self.datagram_sequence.to_be_bytes());
        output.extend_from_slice(&self.message_id.to_be_bytes());
        output.push(self.fragment_index);
        output.push(self.fragment_count);
        output.extend_from_slice(&self.total_plaintext_length.to_be_bytes());
        output.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        output.extend_from_slice(&self.payload);
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RelayDataPlaneError> {
        if bytes.len() < HEADER_BYTES || bytes[..4] != MAGIC || bytes[4] != VERSION {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let association_id = bytes[5..37]
            .try_into()
            .map_err(|_| RelayDataPlaneError::Truncated)?;
        let direction = match bytes[37] {
            0 => DatagramDirectionV1::InitiatorToTarget,
            1 => DatagramDirectionV1::TargetToInitiator,
            _ => return Err(RelayDataPlaneError::InvalidEnvelope),
        };
        let datagram_sequence = u64::from_be_bytes(
            bytes[38..46]
                .try_into()
                .map_err(|_| RelayDataPlaneError::Truncated)?,
        );
        let message_id = u64::from_be_bytes(
            bytes[46..54]
                .try_into()
                .map_err(|_| RelayDataPlaneError::Truncated)?,
        );
        let fragment_index = bytes[54];
        let fragment_count = bytes[55];
        let total_plaintext_length = u16::from_be_bytes(
            bytes[56..58]
                .try_into()
                .map_err(|_| RelayDataPlaneError::Truncated)?,
        );
        let fragment_length = u16::from_be_bytes(
            bytes[58..60]
                .try_into()
                .map_err(|_| RelayDataPlaneError::Truncated)?,
        ) as usize;
        if bytes.len() != HEADER_BYTES + fragment_length {
            return Err(RelayDataPlaneError::Truncated);
        }
        let value = Self {
            association_id,
            direction,
            datagram_sequence,
            message_id,
            fragment_index,
            fragment_count,
            total_plaintext_length,
            payload: bytes[HEADER_BYTES..].to_vec(),
        };
        validate_envelope(&value)?;
        Ok(value)
    }
}

fn validate_envelope(value: &OpaqueDatagramEnvelopeV1) -> Result<(), RelayDataPlaneError> {
    if value.association_id == [0; 32]
        || value.datagram_sequence == 0
        || value.message_id == 0
        || value.fragment_count == 0
        || value.fragment_count as usize > MAX_FRAGMENTS
        || value.fragment_index >= value.fragment_count
        || value.total_plaintext_length == 0
        || value.total_plaintext_length as usize > MAX_INNER_DATAGRAM_BYTES
        || value.payload.is_empty()
        || value.payload.len() > value.total_plaintext_length as usize
    {
        Err(RelayDataPlaneError::InvalidEnvelope)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssociationBinding {
    association_id: [u8; 32],
    initiator_reservation_id: [u8; 32],
    target_reservation_id: [u8; 32],
    initiator_connection: [u8; 32],
    target_connection: [u8; 32],
    expires_at: u64,
}

impl AssociationBinding {
    pub fn new(
        association_id: [u8; 32],
        initiator_reservation_id: [u8; 32],
        target_reservation_id: [u8; 32],
        initiator_connection: [u8; 32],
        target_connection: [u8; 32],
        expires_at: u64,
    ) -> Result<Self, RelayDataPlaneError> {
        if [
            association_id,
            initiator_reservation_id,
            target_reservation_id,
            initiator_connection,
            target_connection,
        ]
        .contains(&[0; 32])
            || initiator_connection == target_connection
            || expires_at == 0
        {
            return Err(RelayDataPlaneError::InvalidAssociation);
        }
        Ok(Self {
            association_id,
            initiator_reservation_id,
            target_reservation_id,
            initiator_connection,
            target_connection,
            expires_at,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveredOpaqueDatagram {
    recipient_connection: [u8; 32],
    payload: Vec<u8>,
}

impl DeliveredOpaqueDatagram {
    pub fn recipient_connection(&self) -> [u8; 32] {
        self.recipient_connection
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReassemblyKey {
    association_id: [u8; 32],
    direction: DatagramDirectionV1,
    message_id: u64,
}

#[derive(Debug)]
struct ReassemblyState {
    datagram_sequence: u64,
    total: usize,
    fragments: Vec<Option<Vec<u8>>>,
    received_bytes: usize,
    expires_at: u64,
    _lease: BudgetLease,
}

pub struct RelayDataPlane {
    associations: BTreeMap<[u8; 32], AssociationBinding>,
    reassemblies: BTreeMap<ReassemblyKey, ReassemblyState>,
    completed: BTreeSet<ReassemblyKey>,
    budget: RelayGlobalBudget,
}

impl RelayDataPlane {
    pub fn new(budget: RelayGlobalBudget) -> Self {
        Self {
            associations: BTreeMap::new(),
            reassemblies: BTreeMap::new(),
            completed: BTreeSet::new(),
            budget,
        }
    }

    pub fn register(&mut self, binding: AssociationBinding) -> Result<(), RelayDataPlaneError> {
        if self.associations.contains_key(&binding.association_id) {
            return Err(RelayDataPlaneError::DuplicateAssociation);
        }
        self.associations.insert(binding.association_id, binding);
        Ok(())
    }

    pub fn accept_fragment(
        &mut self,
        sender_connection: [u8; 32],
        bytes: &[u8],
        now: u64,
    ) -> Result<Option<DeliveredOpaqueDatagram>, RelayDataPlaneError> {
        self.reassemblies.retain(|_, state| state.expires_at >= now);
        let envelope = OpaqueDatagramEnvelopeV1::decode(bytes)?;
        let binding = self
            .associations
            .get(&envelope.association_id)
            .ok_or(RelayDataPlaneError::UnknownAssociation)?;
        if binding.expires_at < now {
            return Err(RelayDataPlaneError::Expired);
        }
        let recipient = match envelope.direction {
            DatagramDirectionV1::InitiatorToTarget
                if sender_connection == binding.initiator_connection =>
            {
                binding.target_connection
            }
            DatagramDirectionV1::TargetToInitiator
                if sender_connection == binding.target_connection =>
            {
                binding.initiator_connection
            }
            _ => return Err(RelayDataPlaneError::ConnectionMismatch),
        };
        let key = ReassemblyKey {
            association_id: envelope.association_id,
            direction: envelope.direction,
            message_id: envelope.message_id,
        };
        if self.completed.contains(&key) {
            return Err(RelayDataPlaneError::DuplicateFragment);
        }
        if !self.reassemblies.contains_key(&key) {
            let count = self
                .reassemblies
                .keys()
                .filter(|candidate| candidate.association_id == key.association_id)
                .count();
            if count >= MAX_IN_FLIGHT_PER_ASSOCIATION {
                return Err(RelayDataPlaneError::Capacity);
            }
            let total = envelope.total_plaintext_length as usize;
            let lease = self.budget.reserve_reassembly(total)?;
            self.reassemblies.insert(
                key.clone(),
                ReassemblyState {
                    datagram_sequence: envelope.datagram_sequence,
                    total,
                    fragments: vec![None; envelope.fragment_count as usize],
                    received_bytes: 0,
                    expires_at: now.saturating_add(REASSEMBLY_EXPIRY_SECONDS),
                    _lease: lease,
                },
            );
        }
        let state = self
            .reassemblies
            .get_mut(&key)
            .ok_or(RelayDataPlaneError::Closed)?;
        if state.datagram_sequence != envelope.datagram_sequence
            || state.total != envelope.total_plaintext_length as usize
            || state.fragments.len() != envelope.fragment_count as usize
        {
            return Err(RelayDataPlaneError::ConflictingFragment);
        }
        let slot = &mut state.fragments[envelope.fragment_index as usize];
        if slot.is_some() {
            return Err(RelayDataPlaneError::DuplicateFragment);
        }
        state.received_bytes = state
            .received_bytes
            .checked_add(envelope.payload.len())
            .ok_or(RelayDataPlaneError::Capacity)?;
        if state.received_bytes > state.total {
            return Err(RelayDataPlaneError::ConflictingFragment);
        }
        *slot = Some(envelope.payload);
        if state.fragments.iter().any(Option::is_none) {
            return Ok(None);
        }
        if state.received_bytes != state.total {
            return Err(RelayDataPlaneError::ConflictingFragment);
        }
        let completed = self
            .reassemblies
            .remove(&key)
            .ok_or(RelayDataPlaneError::Closed)?;
        let mut payload = Vec::with_capacity(completed.total);
        for fragment in completed.fragments {
            payload.extend(fragment.ok_or(RelayDataPlaneError::ConflictingFragment)?);
        }
        if payload.len() != completed.total {
            return Err(RelayDataPlaneError::ConflictingFragment);
        }
        if self.completed.len() >= 4_096 {
            let first = self.completed.iter().next().cloned();
            if let Some(first) = first {
                self.completed.remove(&first);
            }
        }
        self.completed.insert(key);
        Ok(Some(DeliveredOpaqueDatagram {
            recipient_connection: recipient,
            payload,
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelayDataPlaneError {
    InvalidEnvelope,
    InvalidAssociation,
    DuplicateAssociation,
    UnknownAssociation,
    ConnectionMismatch,
    DuplicateFragment,
    ConflictingFragment,
    TooManyFragments,
    Truncated,
    Oversize,
    Capacity,
    Expired,
    Closed,
    IdentityMismatch,
    NoDatagramSupport,
}

impl std::fmt::Display for RelayDataPlaneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_RELAY_DATA: {self:?}")
    }
}

impl std::error::Error for RelayDataPlaneError {}
