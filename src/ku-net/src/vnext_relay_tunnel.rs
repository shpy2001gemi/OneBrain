//! Bounded datagram adaptation for an opaque authenticated relay carrier.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use quinn::udp::{EcnCodepoint, RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, ClientConfig, Endpoint, UdpPoller};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};
use tokio_rustls::client::TlsStream;

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    decode_relay_control, encode_relay_control, relay_control_signing_bytes,
    relay_control_signing_parts, RelayControlSignatureRoleV1, RelayControlV1,
    RelayOuterClientHelloV1, RelayPossessionProofV1, RelayTransportV1, RelayWireFrameV1,
    RelayWireKindV1, MAX_RELAY_WIRE_PAYLOAD_BYTES,
};
use rand::rngs::OsRng;
use rand::RngCore;

use crate::transport::QuicTransport;
use crate::vnext_reachability_crypto::{
    ReachabilityIdentitySigner, ValidatedPossessionDialEndpoint, ValidatedPublicDialEndpoint,
    ValidatedPublicDialTransportV1, ValidatedRelayDescriptor,
};

pub const RELAY_SOCKET_FRAME_LIMIT: usize = 64;
pub const RELAY_SOCKET_BYTE_LIMIT: usize = 1024 * 1024;
pub const RELAY_GLOBAL_FRAME_LIMIT: usize = 1_024;
pub const RELAY_GLOBAL_BYTE_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPublicRelayDialEndpoint {
    descriptor: ValidatedRelayDescriptor,
    endpoint_index: usize,
    endpoint: ValidatedPublicDialEndpoint,
    transport: RelayTransportV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedAlternateRelayDialEndpoint {
    descriptor: ValidatedRelayDescriptor,
    public_endpoint_index: usize,
    alternate_socket: SocketAddr,
    transport: RelayTransportV1,
    spki_observation_digest: [u8; 32],
    expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlternateRelayProbeObservation {
    public_endpoint_index: usize,
    alternate_socket: SocketAddr,
    transport: RelayTransportV1,
    observed_spki: [u8; 32],
    observation_digest: [u8; 32],
    expires_at: u64,
}

impl AlternateRelayProbeObservation {
    pub fn new(
        public_endpoint_index: usize,
        alternate_socket: SocketAddr,
        transport: RelayTransportV1,
        observed_spki: [u8; 32],
        observation_digest: [u8; 32],
        observed_at: u64,
        expires_at: u64,
    ) -> Result<Self, RelayRouteError> {
        if observation_digest == [0; 32]
            || alternate_socket.ip().is_unspecified()
            || observed_at >= expires_at
            || expires_at > observed_at.saturating_add(30)
        {
            return Err(RelayRouteError::InvalidProbe);
        }
        Ok(Self {
            public_endpoint_index,
            alternate_socket,
            transport,
            observed_spki,
            observation_digest,
            expires_at,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidatedRelayDialRoute {
    Public(ValidatedPublicRelayDialEndpoint),
    Alternate(ValidatedAlternateRelayDialEndpoint),
}

impl ValidatedRelayDialRoute {
    pub fn public(
        descriptor: &ValidatedRelayDescriptor,
        endpoint: ValidatedPublicDialEndpoint,
    ) -> Result<Self, RelayRouteError> {
        if endpoint.source_digest() != *descriptor.digest() {
            return Err(RelayRouteError::DescriptorMismatch);
        }
        let index = endpoint.endpoint_index();
        let signed = descriptor
            .canonical()
            .endpoints
            .get(index)
            .ok_or(RelayRouteError::EndpointMismatch)?;
        let transport = match endpoint.transport() {
            ValidatedPublicDialTransportV1::RelayQuicUdp => RelayTransportV1::QuicUdp,
            ValidatedPublicDialTransportV1::RelayTlsTcp443 => RelayTransportV1::TlsTcp443,
            _ => return Err(RelayRouteError::TransportMismatch),
        };
        if signed.transport != transport
            || signed.port != endpoint.port()
            || &signed.host != endpoint.signed_host()
        {
            return Err(RelayRouteError::EndpointMismatch);
        }
        Ok(Self::Public(ValidatedPublicRelayDialEndpoint {
            descriptor: descriptor.clone(),
            endpoint_index: index,
            endpoint,
            transport,
        }))
    }

    pub fn alternate_from_verified_probe(
        descriptor: &ValidatedRelayDescriptor,
        probe: AlternateRelayProbeObservation,
    ) -> Result<Self, RelayRouteError> {
        let endpoint = descriptor
            .canonical()
            .endpoints
            .get(probe.public_endpoint_index)
            .ok_or(RelayRouteError::EndpointMismatch)?;
        if endpoint.transport != probe.transport
            || probe.observed_spki != descriptor.canonical().relay_public_key
            || probe.expires_at > descriptor.canonical().expires_at
        {
            return Err(RelayRouteError::InvalidProbe);
        }
        Ok(Self::Alternate(ValidatedAlternateRelayDialEndpoint {
            descriptor: descriptor.clone(),
            public_endpoint_index: probe.public_endpoint_index,
            alternate_socket: probe.alternate_socket,
            transport: probe.transport,
            spki_observation_digest: probe.observation_digest,
            expires_at: probe.expires_at,
        }))
    }

    pub fn relay_node_id(&self) -> NodeId {
        self.descriptor().canonical().relay_node_id
    }

    pub fn relay_public_key(&self) -> [u8; 32] {
        self.descriptor().canonical().relay_public_key
    }

    pub fn descriptor_digest(&self) -> [u8; 32] {
        *self.descriptor().digest()
    }

    pub fn transport(&self) -> RelayTransportV1 {
        match self {
            Self::Public(value) => value.transport,
            Self::Alternate(value) => value.transport,
        }
    }

    pub fn dial_addresses(&self) -> Vec<SocketAddr> {
        match self {
            Self::Public(value) => value.endpoint.dial_addresses().to_vec(),
            Self::Alternate(value) => vec![value.alternate_socket],
        }
    }

    pub fn expires_at(&self) -> u64 {
        match self {
            Self::Public(value) => value.endpoint.expires_at(),
            Self::Alternate(value) => value.expires_at,
        }
    }

    pub fn endpoint_index(&self) -> usize {
        match self {
            Self::Public(value) => value.endpoint_index,
            Self::Alternate(value) => value.public_endpoint_index,
        }
    }

    pub fn alternate_spki_observation_digest(&self) -> Option<[u8; 32]> {
        match self {
            Self::Public(_) => None,
            Self::Alternate(value) => Some(value.spki_observation_digest),
        }
    }

    pub(crate) fn descriptor(&self) -> &ValidatedRelayDescriptor {
        match self {
            Self::Public(value) => &value.descriptor,
            Self::Alternate(value) => &value.descriptor,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedRelayDialSet {
    primary: ValidatedRelayDialRoute,
    tcp_fallback: Option<ValidatedRelayDialRoute>,
}

impl ValidatedRelayDialSet {
    pub fn from_admitted_descriptor(
        primary: ValidatedRelayDialRoute,
        tcp_fallback: Option<ValidatedRelayDialRoute>,
    ) -> Result<Self, RelayRouteError> {
        if let Some(fallback) = &tcp_fallback {
            if primary.transport() != RelayTransportV1::QuicUdp
                || fallback.transport() != RelayTransportV1::TlsTcp443
                || fallback.descriptor_digest() != primary.descriptor_digest()
                || fallback.relay_node_id() != primary.relay_node_id()
                || fallback.relay_public_key() != primary.relay_public_key()
            {
                return Err(RelayRouteError::DescriptorMismatch);
            }
        }
        Ok(Self {
            primary,
            tcp_fallback,
        })
    }

    pub fn primary(&self) -> &ValidatedRelayDialRoute {
        &self.primary
    }

    pub fn tcp_fallback(&self) -> Option<&ValidatedRelayDialRoute> {
        self.tcp_fallback.as_ref()
    }

    pub fn select_for_udp_datagram_limit(
        &self,
        max_datagram_size: Option<usize>,
        encoded_header_len: usize,
    ) -> Result<&ValidatedRelayDialRoute, RelayRouteError> {
        if self.primary.transport() == RelayTransportV1::TlsTcp443 {
            return Ok(&self.primary);
        }
        let usable = max_datagram_size.and_then(|size| size.checked_sub(encoded_header_len));
        if usable.is_some_and(|value| value > 0) {
            Ok(&self.primary)
        } else {
            self.tcp_fallback
                .as_ref()
                .ok_or(RelayRouteError::NoUsableTransport)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayRouteError {
    DescriptorMismatch,
    EndpointMismatch,
    TransportMismatch,
    NoUsableTransport,
    InvalidHandshake,
    InvalidProbe,
}

/// Identity and measured transport binding for one live outer relay carrier.
/// Construction remains inside ku-net's authenticated handshake path; node
/// policy can inspect but cannot relabel the connection.
pub struct AuthenticatedOuterRelayConnection {
    route: ValidatedRelayDialRoute,
    client_node_id: NodeId,
    relay_node_id: NodeId,
    connected_socket: SocketAddr,
    connection_binding: [u8; 32],
    transport: RelayTransportV1,
    established_at: u64,
    expires_at: u64,
    open: Arc<AtomicBool>,
    inner: OuterRelayConnection,
    control_transaction: AsyncMutex<()>,
    pending_control: Arc<Mutex<BTreeMap<[u8; 16], oneshot::Sender<RelayWireFrameV1>>>>,
    notifications: AsyncMutex<mpsc::Receiver<RelayWireFrameV1>>,
    reader_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

enum OuterRelayConnection {
    Quic {
        _endpoint: Endpoint,
        connection: quinn::Connection,
        control_send: AsyncMutex<quinn::SendStream>,
    },
    TlsTcp443 {
        control_send: AsyncMutex<tokio::io::WriteHalf<TlsStream<TcpStream>>>,
    },
}

impl fmt::Debug for AuthenticatedOuterRelayConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedOuterRelayConnection")
            .field("client_node_id", &self.client_node_id)
            .field("relay_node_id", &self.relay_node_id)
            .field("connected_socket", &self.connected_socket)
            .field("connection_binding", &self.connection_binding)
            .field("transport", &self.transport)
            .field("established_at", &self.established_at)
            .field("expires_at", &self.expires_at)
            .field("open", &self.is_open())
            .finish_non_exhaustive()
    }
}

impl AuthenticatedOuterRelayConnection {
    #[allow(dead_code)]
    fn from_verified_handshake(
        route: ValidatedRelayDialRoute,
        client_node_id: NodeId,
        connected_socket: SocketAddr,
        connection_binding: [u8; 32],
        established_at: u64,
        expires_at: u64,
        inner: OuterRelayConnection,
        control_recv: Pin<Box<dyn tokio::io::AsyncRead + Send>>,
    ) -> Result<Self, RelayRouteError> {
        if connection_binding == [0; 32]
            || established_at >= expires_at
            || expires_at > route.expires_at()
        {
            return Err(RelayRouteError::InvalidHandshake);
        }
        let open = Arc::new(AtomicBool::new(true));
        let pending_control = Arc::new(Mutex::new(BTreeMap::new()));
        let (notification_tx, notification_rx) = mpsc::channel(64);
        let reader_open = open.clone();
        let reader_pending = pending_control.clone();
        let reader_task = tokio::spawn(async move {
            run_control_reader(control_recv, reader_pending, notification_tx, reader_open).await;
        });
        Ok(Self {
            relay_node_id: route.relay_node_id(),
            transport: route.transport(),
            route,
            client_node_id,
            connected_socket,
            connection_binding,
            established_at,
            expires_at,
            open,
            inner,
            control_transaction: AsyncMutex::new(()),
            pending_control,
            notifications: AsyncMutex::new(notification_rx),
            reader_task: Mutex::new(Some(reader_task)),
        })
    }

    pub(crate) fn from_verified_quic_handshake(
        route: ValidatedRelayDialRoute,
        client_node_id: NodeId,
        connection_binding: [u8; 32],
        established_at: u64,
        expires_at: u64,
        endpoint: Endpoint,
        connection: quinn::Connection,
        control_send: quinn::SendStream,
        control_recv: quinn::RecvStream,
    ) -> Result<Self, RelayRouteError> {
        let connected_socket = connection.remote_address();
        Self::from_verified_handshake(
            route,
            client_node_id,
            connected_socket,
            connection_binding,
            established_at,
            expires_at,
            OuterRelayConnection::Quic {
                _endpoint: endpoint,
                connection,
                control_send: AsyncMutex::new(control_send),
            },
            Box::pin(control_recv),
        )
    }

    pub(crate) fn from_verified_tls_handshake(
        route: ValidatedRelayDialRoute,
        client_node_id: NodeId,
        connected_socket: SocketAddr,
        connection_binding: [u8; 32],
        established_at: u64,
        expires_at: u64,
        stream: TlsStream<TcpStream>,
    ) -> Result<Self, RelayRouteError> {
        let (recv, send) = tokio::io::split(stream);
        Self::from_verified_handshake(
            route,
            client_node_id,
            connected_socket,
            connection_binding,
            established_at,
            expires_at,
            OuterRelayConnection::TlsTcp443 {
                control_send: AsyncMutex::new(send),
            },
            Box::pin(recv),
        )
    }

    pub fn client_node_id(&self) -> NodeId {
        self.client_node_id
    }

    pub fn relay_node_id(&self) -> NodeId {
        self.relay_node_id
    }

    pub fn connected_socket(&self) -> SocketAddr {
        self.connected_socket
    }

    pub fn connection_binding(&self) -> [u8; 32] {
        self.connection_binding
    }

    pub fn transport(&self) -> RelayTransportV1 {
        self.transport
    }

    pub fn established_at(&self) -> u64 {
        self.established_at
    }

    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }

    pub fn route(&self) -> &ValidatedRelayDialRoute {
        &self.route
    }

    pub fn public_endpoint(&self) -> &onebrain_protocol::RelayEndpointV1 {
        let descriptor = self.route.descriptor();
        &descriptor.canonical().endpoints[self.route.endpoint_index()]
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    pub fn close(&self) {
        self.open.store(false, Ordering::Release);
        if let OuterRelayConnection::Quic { connection, .. } = &self.inner {
            connection.close(0u32.into(), b"outer relay closed");
        }
        if let Ok(mut reader) = self.reader_task.lock() {
            if let Some(reader) = reader.take() {
                reader.abort();
            }
        }
    }

    pub fn max_datagram_size(&self) -> Option<usize> {
        match &self.inner {
            OuterRelayConnection::Quic { connection, .. } => connection.max_datagram_size(),
            OuterRelayConnection::TlsTcp443 { .. } => None,
        }
    }

    pub async fn send_control_frame(
        &self,
        frame: &RelayWireFrameV1,
    ) -> Result<(), OuterRelayIoError> {
        if !self.is_open() {
            return Err(OuterRelayIoError::Closed);
        }
        let bytes = frame.encode();
        match &self.inner {
            OuterRelayConnection::Quic { control_send, .. } => {
                let mut send = control_send.lock().await;
                write_stream_frame(&mut *send, &bytes).await
            }
            OuterRelayConnection::TlsTcp443 { control_send } => {
                let mut send = control_send.lock().await;
                write_stream_frame(&mut *send, &bytes).await
            }
        }
    }

    pub async fn receive_control_frame(&self) -> Result<RelayWireFrameV1, OuterRelayIoError> {
        if !self.is_open() {
            return Err(OuterRelayIoError::Closed);
        }
        self.notifications
            .lock()
            .await
            .recv()
            .await
            .ok_or(OuterRelayIoError::Closed)
    }

    pub async fn request_control_frame(
        &self,
        frame: &RelayWireFrameV1,
    ) -> Result<RelayWireFrameV1, OuterRelayIoError> {
        let _transaction = self.control_transaction.lock().await;
        let (sender, receiver) = oneshot::channel();
        self.pending_control
            .lock()
            .map_err(|_| OuterRelayIoError::Closed)?
            .insert(frame.request_id(), sender);
        if let Err(error) = self.send_control_frame(frame).await {
            if let Ok(mut pending) = self.pending_control.lock() {
                pending.remove(&frame.request_id());
            }
            return Err(error);
        }
        let response = match tokio::time::timeout(Duration::from_secs(5), receiver).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return Err(OuterRelayIoError::Closed),
            Err(_) => {
                if let Ok(mut pending) = self.pending_control.lock() {
                    pending.remove(&frame.request_id());
                }
                return Err(OuterRelayIoError::Deadline);
            }
        };
        if response.request_id() != frame.request_id() {
            return Err(OuterRelayIoError::InvalidFrame);
        }
        Ok(response)
    }

    pub async fn send_opaque_datagram(&self, payload: Vec<u8>) -> Result<(), OuterRelayIoError> {
        if payload.is_empty() || payload.len() > MAX_RELAY_WIRE_PAYLOAD_BYTES {
            return Err(OuterRelayIoError::InvalidFrame);
        }
        match &self.inner {
            OuterRelayConnection::Quic { connection, .. } => connection
                .send_datagram_wait(payload.into())
                .await
                .map_err(|_| OuterRelayIoError::Closed),
            OuterRelayConnection::TlsTcp443 { .. } => Err(OuterRelayIoError::WrongTransport),
        }
    }

    pub async fn receive_opaque_datagram(&self) -> Result<Vec<u8>, OuterRelayIoError> {
        match &self.inner {
            OuterRelayConnection::Quic { connection, .. } => connection
                .read_datagram()
                .await
                .map(|value| value.to_vec())
                .map_err(|_| OuterRelayIoError::Closed),
            OuterRelayConnection::TlsTcp443 { .. } => Err(OuterRelayIoError::WrongTransport),
        }
    }
}

async fn run_control_reader(
    mut recv: Pin<Box<dyn tokio::io::AsyncRead + Send>>,
    pending: Arc<Mutex<BTreeMap<[u8; 16], oneshot::Sender<RelayWireFrameV1>>>>,
    notifications: mpsc::Sender<RelayWireFrameV1>,
    open: Arc<AtomicBool>,
) {
    while open.load(Ordering::Acquire) {
        let bytes = match read_stream_frame(&mut recv).await {
            Ok(bytes) => bytes,
            Err(_) => break,
        };
        let frame = match RelayWireFrameV1::decode(&bytes) {
            Ok(frame) => frame,
            Err(_) => break,
        };
        let waiter = pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&frame.request_id()));
        if let Some(waiter) = waiter {
            let _ = waiter.send(frame);
        } else if notifications.send(frame).await.is_err() {
            break;
        }
    }
    open.store(false, Ordering::Release);
    if let Ok(mut pending) = pending.lock() {
        pending.clear();
    }
}

async fn write_stream_frame<W>(stream: &mut W, bytes: &[u8]) -> Result<(), OuterRelayIoError>
where
    W: AsyncWriteExt + Unpin,
{
    let length = u32::try_from(bytes.len()).map_err(|_| OuterRelayIoError::InvalidFrame)?;
    stream
        .write_all(&length.to_be_bytes())
        .await
        .and_then(|_| Ok(()))
        .map_err(|_| OuterRelayIoError::Closed)?;
    stream
        .write_all(bytes)
        .await
        .map_err(|_| OuterRelayIoError::Closed)?;
    stream.flush().await.map_err(|_| OuterRelayIoError::Closed)
}

async fn read_stream_frame<R>(stream: &mut R) -> Result<Vec<u8>, OuterRelayIoError>
where
    R: AsyncReadExt + Unpin,
{
    let mut prefix = [0u8; 4];
    stream
        .read_exact(&mut prefix)
        .await
        .map_err(|_| OuterRelayIoError::Closed)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_RELAY_WIRE_PAYLOAD_BYTES + 26 {
        return Err(OuterRelayIoError::InvalidFrame);
    }
    let mut bytes = vec![0; length];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| OuterRelayIoError::Closed)?;
    Ok(bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OuterRelayIoError {
    Closed,
    InvalidFrame,
    WrongTransport,
    Connect,
    Handshake,
    Deadline,
}

impl std::fmt::Display for OuterRelayIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "OBP_OUTER_RELAY_IO: {self:?}")
    }
}

impl std::error::Error for OuterRelayIoError {}

const OUTER_EXPORTER_LABEL: &[u8] = b"EXPORTER-OneBrain-Relay-V1";
const OUTER_ALPN: &[u8] = b"obp-relay/1";

/// Open the selected relay carrier, pin its descriptor key, complete the
/// challenge/hello handshake, and return the only handle accepted by later
/// control and data-plane clients.
pub async fn connect_authenticated_outer(
    routes: &ValidatedRelayDialSet,
    signer: &dyn ReachabilityIdentitySigner,
    now: u64,
    deadline: Instant,
) -> Result<AuthenticatedOuterRelayConnection, OuterRelayIoError> {
    if Instant::now() >= deadline {
        return Err(OuterRelayIoError::Deadline);
    }
    connect_authenticated_outer_inner(routes, signer, now, deadline, None).await
}

/// Use an existing OBP QUIC endpoint for the relay's UDP outer carrier. This
/// keeps the node's NAT mapping and its direct-listener socket identical while
/// preserving the descriptor-key-pinned relay TLS policy.
pub async fn connect_authenticated_outer_on_transport(
    routes: &ValidatedRelayDialSet,
    signer: &dyn ReachabilityIdentitySigner,
    now: u64,
    deadline: Instant,
    transport: &QuicTransport,
) -> Result<AuthenticatedOuterRelayConnection, OuterRelayIoError> {
    connect_authenticated_outer_inner(routes, signer, now, deadline, Some(transport)).await
}

async fn connect_authenticated_outer_inner(
    routes: &ValidatedRelayDialSet,
    signer: &dyn ReachabilityIdentitySigner,
    now: u64,
    deadline: Instant,
    transport: Option<&QuicTransport>,
) -> Result<AuthenticatedOuterRelayConnection, OuterRelayIoError> {
    if Instant::now() >= deadline {
        return Err(OuterRelayIoError::Deadline);
    }
    let primary = connect_route(routes.primary(), signer, now, deadline, transport).await;
    match primary {
        Ok(connection)
            if routes.primary().transport() == RelayTransportV1::TlsTcp443
                || connection.max_datagram_size().is_some() =>
        {
            Ok(connection)
        }
        Ok(connection) => {
            connection.close();
            let fallback = routes
                .tcp_fallback()
                .ok_or(OuterRelayIoError::WrongTransport)?;
            connect_route(fallback, signer, now, deadline, transport).await
        }
        Err(error) => {
            let Some(fallback) = routes.tcp_fallback() else {
                return Err(error);
            };
            connect_route(fallback, signer, now, deadline, transport).await
        }
    }
}

trait RelayHandshakeRoute: Sync {
    fn relay_node_id(&self) -> NodeId;
    fn relay_public_key(&self) -> [u8; 32];
    fn expires_at(&self) -> u64;
}

impl RelayHandshakeRoute for ValidatedRelayDialRoute {
    fn relay_node_id(&self) -> NodeId {
        self.relay_node_id()
    }
    fn relay_public_key(&self) -> [u8; 32] {
        self.relay_public_key()
    }
    fn expires_at(&self) -> u64 {
        self.expires_at()
    }
}

impl RelayHandshakeRoute for ValidatedPossessionDialEndpoint {
    fn relay_node_id(&self) -> NodeId {
        self.relay_node_id()
    }
    fn relay_public_key(&self) -> [u8; 32] {
        self.relay_public_key()
    }
    fn expires_at(&self) -> u64 {
        self.expires_at()
    }
}

/// Prove that each descriptor endpoint terminates at the relay identity that
/// signed the pending descriptor. The purpose-limited token cannot be used
/// for reservations or data-plane traffic.
pub async fn prove_relay_possession(
    route: &ValidatedPossessionDialEndpoint,
    signer: &dyn ReachabilityIdentitySigner,
    now: u64,
    deadline: Instant,
) -> Result<RelayPossessionProofV1, OuterRelayIoError> {
    install_aws_lc_provider()?;
    let address = *route
        .dial_addresses()
        .first()
        .ok_or(OuterRelayIoError::Connect)?;
    let client_node_id = crate::vnext_session::principal_node_id(&signer.public_key());
    let mut request_id = [0u8; 16];
    OsRng.fill_bytes(&mut request_id);
    match route.transport() {
        RelayTransportV1::QuicUdp => {
            let bind: SocketAddr = if address.is_ipv6() {
                "[::]:0".parse().map_err(|_| OuterRelayIoError::Connect)?
            } else {
                "0.0.0.0:0"
                    .parse()
                    .map_err(|_| OuterRelayIoError::Connect)?
            };
            let mut endpoint = Endpoint::client(bind).map_err(|_| OuterRelayIoError::Connect)?;
            endpoint.set_default_client_config(quic_client_config(route.relay_public_key())?);
            let connection = tokio::time::timeout_at(
                deadline.into(),
                endpoint
                    .connect(address, "relay.onebrain")
                    .map_err(|_| OuterRelayIoError::Connect)?,
            )
            .await
            .map_err(|_| OuterRelayIoError::Deadline)?
            .map_err(|_| OuterRelayIoError::Connect)?;
            let (mut send, mut recv) = connection
                .open_bi()
                .await
                .map_err(|_| OuterRelayIoError::Connect)?;
            send.write_all(&[0])
                .await
                .map_err(|_| OuterRelayIoError::Connect)?;
            send.flush().await.map_err(|_| OuterRelayIoError::Connect)?;
            let mut binding = [0u8; 32];
            connection
                .export_keying_material(&mut binding, OUTER_EXPORTER_LABEL, &[])
                .map_err(|_| OuterRelayIoError::Handshake)?;
            client_handshake(
                &mut send,
                &mut recv,
                route,
                signer,
                client_node_id,
                binding,
                now,
                deadline,
            )
            .await?;
            let proof = possession_exchange_split(
                &mut send, &mut recv, route, binding, request_id, deadline,
            )
            .await?;
            connection.close(0u32.into(), b"possession proof complete");
            endpoint.close(0u32.into(), b"possession proof complete");
            Ok(proof)
        }
        RelayTransportV1::TlsTcp443 => {
            let tcp = tokio::time::timeout_at(deadline.into(), TcpStream::connect(address))
                .await
                .map_err(|_| OuterRelayIoError::Deadline)?
                .map_err(|_| OuterRelayIoError::Connect)?;
            let name =
                ServerName::try_from("relay.onebrain").map_err(|_| OuterRelayIoError::Connect)?;
            let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_client_config(
                route.relay_public_key(),
            )));
            let mut stream = tokio::time::timeout_at(deadline.into(), connector.connect(name, tcp))
                .await
                .map_err(|_| OuterRelayIoError::Deadline)?
                .map_err(|_| OuterRelayIoError::Connect)?;
            let mut binding = [0u8; 32];
            stream
                .get_ref()
                .1
                .export_keying_material(&mut binding, OUTER_EXPORTER_LABEL, None)
                .map_err(|_| OuterRelayIoError::Handshake)?;
            client_handshake_single(
                &mut stream,
                route,
                signer,
                client_node_id,
                binding,
                now,
                deadline,
            )
            .await?;
            possession_exchange_single(&mut stream, route, binding, request_id, deadline).await
        }
    }
}

async fn possession_exchange_split<W, R>(
    send: &mut W,
    recv: &mut R,
    route: &ValidatedPossessionDialEndpoint,
    binding: [u8; 32],
    request_id: [u8; 16],
    deadline: Instant,
) -> Result<RelayPossessionProofV1, OuterRelayIoError>
where
    W: tokio::io::AsyncWrite + Unpin,
    R: tokio::io::AsyncRead + Unpin,
{
    let frame = RelayWireFrameV1::new(
        RelayWireKindV1::Control,
        request_id,
        encode_relay_control(&RelayControlV1::PossessionChallenge(
            route.challenge().clone(),
        ))
        .map_err(|_| OuterRelayIoError::InvalidFrame)?,
    )
    .map_err(|_| OuterRelayIoError::InvalidFrame)?;
    tokio::time::timeout_at(deadline.into(), write_stream_frame(send, &frame.encode()))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    let response = tokio::time::timeout_at(deadline.into(), read_stream_frame(recv))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    validate_possession_response(route, binding, request_id, &response)
}

async fn possession_exchange_single<S>(
    stream: &mut S,
    route: &ValidatedPossessionDialEndpoint,
    binding: [u8; 32],
    request_id: [u8; 16],
    deadline: Instant,
) -> Result<RelayPossessionProofV1, OuterRelayIoError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let frame = RelayWireFrameV1::new(
        RelayWireKindV1::Control,
        request_id,
        encode_relay_control(&RelayControlV1::PossessionChallenge(
            route.challenge().clone(),
        ))
        .map_err(|_| OuterRelayIoError::InvalidFrame)?,
    )
    .map_err(|_| OuterRelayIoError::InvalidFrame)?;
    tokio::time::timeout_at(deadline.into(), write_stream_frame(stream, &frame.encode()))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    let response = tokio::time::timeout_at(deadline.into(), read_stream_frame(stream))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    validate_possession_response(route, binding, request_id, &response)
}

fn validate_possession_response(
    route: &ValidatedPossessionDialEndpoint,
    binding: [u8; 32],
    request_id: [u8; 16],
    bytes: &[u8],
) -> Result<RelayPossessionProofV1, OuterRelayIoError> {
    let frame = RelayWireFrameV1::decode(bytes).map_err(|_| OuterRelayIoError::InvalidFrame)?;
    if frame.kind() != RelayWireKindV1::Control || frame.request_id() != request_id {
        return Err(OuterRelayIoError::InvalidFrame);
    }
    let proof =
        match decode_relay_control(frame.payload()).map_err(|_| OuterRelayIoError::InvalidFrame)? {
            RelayControlV1::PossessionProof(value) => value,
            _ => return Err(OuterRelayIoError::InvalidFrame),
        };
    if proof.challenge_digest != route.challenge_digest()
        || proof.connection_binding_digest != binding
    {
        return Err(OuterRelayIoError::Handshake);
    }
    Ok(proof)
}

async fn connect_route(
    route: &ValidatedRelayDialRoute,
    signer: &dyn ReachabilityIdentitySigner,
    now: u64,
    deadline: Instant,
    shared_transport: Option<&QuicTransport>,
) -> Result<AuthenticatedOuterRelayConnection, OuterRelayIoError> {
    install_aws_lc_provider()?;
    let address = *route
        .dial_addresses()
        .first()
        .ok_or(OuterRelayIoError::Connect)?;
    let client_node_id = crate::vnext_session::principal_node_id(&signer.public_key());
    match route.transport() {
        RelayTransportV1::QuicUdp => {
            let bind: SocketAddr = if address.is_ipv6() {
                "[::]:0".parse().map_err(|_| OuterRelayIoError::Connect)?
            } else {
                "0.0.0.0:0"
                    .parse()
                    .map_err(|_| OuterRelayIoError::Connect)?
            };
            let client_config = quic_client_config(route.relay_public_key())?;
            let (endpoint, connection) = if let Some(transport) = shared_transport {
                tokio::time::timeout_at(
                    deadline.into(),
                    transport.connect_quinn_with_config(address, "relay.onebrain", client_config),
                )
                .await
                .map_err(|_| OuterRelayIoError::Deadline)?
                .map_err(|_| OuterRelayIoError::Connect)?
            } else {
                let mut endpoint =
                    Endpoint::client(bind).map_err(|_| OuterRelayIoError::Connect)?;
                endpoint.set_default_client_config(client_config);
                let connection = tokio::time::timeout_at(
                    deadline.into(),
                    endpoint
                        .connect(address, "relay.onebrain")
                        .map_err(|_| OuterRelayIoError::Connect)?,
                )
                .await
                .map_err(|_| OuterRelayIoError::Deadline)?
                .map_err(|_| OuterRelayIoError::Connect)?;
                (endpoint, connection)
            };
            let (mut send, mut recv) =
                tokio::time::timeout_at(deadline.into(), connection.open_bi())
                    .await
                    .map_err(|_| OuterRelayIoError::Deadline)?
                    .map_err(|_| OuterRelayIoError::Connect)?;
            // Quinn announces a locally-opened stream only after the first
            // bytes are written. This fixed preface lets the relay send the
            // first authenticated challenge without a stream-open deadlock.
            send.write_all(&[0])
                .await
                .map_err(|_| OuterRelayIoError::Connect)?;
            send.flush().await.map_err(|_| OuterRelayIoError::Connect)?;
            let mut binding = [0u8; 32];
            connection
                .export_keying_material(&mut binding, OUTER_EXPORTER_LABEL, &[])
                .map_err(|_| OuterRelayIoError::Handshake)?;
            let expires_at = client_handshake(
                &mut send,
                &mut recv,
                route,
                signer,
                client_node_id,
                binding,
                now,
                deadline,
            )
            .await?;
            AuthenticatedOuterRelayConnection::from_verified_quic_handshake(
                route.clone(),
                client_node_id,
                binding,
                now,
                expires_at,
                endpoint,
                connection,
                send,
                recv,
            )
            .map_err(|_| OuterRelayIoError::Handshake)
        }
        RelayTransportV1::TlsTcp443 => {
            let tcp = tokio::time::timeout_at(deadline.into(), TcpStream::connect(address))
                .await
                .map_err(|_| OuterRelayIoError::Deadline)?
                .map_err(|_| OuterRelayIoError::Connect)?;
            let connected_socket = tcp.peer_addr().map_err(|_| OuterRelayIoError::Connect)?;
            let name =
                ServerName::try_from("relay.onebrain").map_err(|_| OuterRelayIoError::Connect)?;
            let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_client_config(
                route.relay_public_key(),
            )));
            let mut stream = tokio::time::timeout_at(deadline.into(), connector.connect(name, tcp))
                .await
                .map_err(|_| OuterRelayIoError::Deadline)?
                .map_err(|_| OuterRelayIoError::Connect)?;
            let mut binding = [0u8; 32];
            stream
                .get_ref()
                .1
                .export_keying_material(&mut binding, OUTER_EXPORTER_LABEL, None)
                .map_err(|_| OuterRelayIoError::Handshake)?;
            let expires_at = client_handshake_single(
                &mut stream,
                route,
                signer,
                client_node_id,
                binding,
                now,
                deadline,
            )
            .await?;
            AuthenticatedOuterRelayConnection::from_verified_tls_handshake(
                route.clone(),
                client_node_id,
                connected_socket,
                binding,
                now,
                expires_at,
                stream,
            )
            .map_err(|_| OuterRelayIoError::Handshake)
        }
    }
}

async fn client_handshake<W, R>(
    send: &mut W,
    recv: &mut R,
    route: &dyn RelayHandshakeRoute,
    signer: &dyn ReachabilityIdentitySigner,
    client_node_id: NodeId,
    binding: [u8; 32],
    now: u64,
    deadline: Instant,
) -> Result<u64, OuterRelayIoError>
where
    W: tokio::io::AsyncWrite + Unpin,
    R: tokio::io::AsyncRead + Unpin,
{
    let challenge = tokio::time::timeout_at(deadline.into(), read_stream_frame(recv))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    let challenge_frame =
        RelayWireFrameV1::decode(&challenge).map_err(|_| OuterRelayIoError::InvalidFrame)?;
    if challenge_frame.kind() != RelayWireKindV1::Control {
        return Err(OuterRelayIoError::Handshake);
    }
    let challenge = match decode_relay_control(challenge_frame.payload())
        .map_err(|_| OuterRelayIoError::Handshake)?
    {
        RelayControlV1::OuterClientChallenge(value) => value,
        _ => return Err(OuterRelayIoError::Handshake),
    };
    if challenge.relay_node_id != route.relay_node_id()
        || challenge.outer_connection_binding != binding
        || challenge.issued_at > now.saturating_add(30)
        || challenge.expires_at.saturating_add(30) < now
    {
        return Err(OuterRelayIoError::Handshake);
    }
    verify_relay_control_signature(
        &RelayControlV1::OuterClientChallenge(challenge.clone()),
        RelayControlSignatureRoleV1::OuterChallengeRelay,
        route.relay_public_key(),
        challenge.relay_signature,
    )?;
    let mut hello = RelayOuterClientHelloV1 {
        format: 1,
        relay_node_id: route.relay_node_id(),
        client_node_id,
        client_public_key: signer.public_key(),
        challenge_nonce: challenge.challenge_nonce,
        outer_connection_binding: binding,
        issued_at: now,
        expires_at: challenge.expires_at.min(now.saturating_add(30)),
        client_signature: [0; 64],
    };
    let unsigned = RelayControlV1::OuterClientHello(hello.clone());
    let (domain, message) =
        relay_control_signing_parts(&unsigned, RelayControlSignatureRoleV1::OuterHelloClient)
            .map_err(|_| OuterRelayIoError::Handshake)?;
    hello.client_signature = signer
        .sign_reachability_message(domain, &message)
        .map_err(|_| OuterRelayIoError::Handshake)?;
    let payload = encode_relay_control(&RelayControlV1::OuterClientHello(hello))
        .map_err(|_| OuterRelayIoError::Handshake)?;
    let response = RelayWireFrameV1::new(
        RelayWireKindV1::Control,
        challenge_frame.request_id(),
        payload,
    )
    .map_err(|_| OuterRelayIoError::Handshake)?;
    tokio::time::timeout_at(
        deadline.into(),
        write_stream_frame(send, &response.encode()),
    )
    .await
    .map_err(|_| OuterRelayIoError::Deadline)??;
    let ack = tokio::time::timeout_at(deadline.into(), read_stream_frame(recv))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    let ack = RelayWireFrameV1::decode(&ack).map_err(|_| OuterRelayIoError::InvalidFrame)?;
    if ack.kind() != RelayWireKindV1::Authenticated
        || ack.request_id() != challenge_frame.request_id()
        || ack.payload() != binding
    {
        return Err(OuterRelayIoError::Handshake);
    }
    Ok(challenge.expires_at.min(route.expires_at()))
}

async fn client_handshake_single<S>(
    stream: &mut S,
    route: &dyn RelayHandshakeRoute,
    signer: &dyn ReachabilityIdentitySigner,
    client_node_id: NodeId,
    binding: [u8; 32],
    now: u64,
    deadline: Instant,
) -> Result<u64, OuterRelayIoError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    // A single TLS stream cannot be mutably borrowed twice at the call site;
    // perform the same closed exchange in-place.
    let challenge = tokio::time::timeout_at(deadline.into(), read_stream_frame(stream))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    let frame =
        RelayWireFrameV1::decode(&challenge).map_err(|_| OuterRelayIoError::InvalidFrame)?;
    let value =
        match decode_relay_control(frame.payload()).map_err(|_| OuterRelayIoError::Handshake)? {
            RelayControlV1::OuterClientChallenge(value) => value,
            _ => return Err(OuterRelayIoError::Handshake),
        };
    if frame.kind() != RelayWireKindV1::Control
        || value.relay_node_id != route.relay_node_id()
        || value.outer_connection_binding != binding
        || value.issued_at > now.saturating_add(30)
        || value.expires_at.saturating_add(30) < now
    {
        return Err(OuterRelayIoError::Handshake);
    }
    verify_relay_control_signature(
        &RelayControlV1::OuterClientChallenge(value.clone()),
        RelayControlSignatureRoleV1::OuterChallengeRelay,
        route.relay_public_key(),
        value.relay_signature,
    )?;
    let mut hello = RelayOuterClientHelloV1 {
        format: 1,
        relay_node_id: route.relay_node_id(),
        client_node_id,
        client_public_key: signer.public_key(),
        challenge_nonce: value.challenge_nonce,
        outer_connection_binding: binding,
        issued_at: now,
        expires_at: value.expires_at.min(now.saturating_add(30)),
        client_signature: [0; 64],
    };
    let unsigned = RelayControlV1::OuterClientHello(hello.clone());
    let (domain, message) =
        relay_control_signing_parts(&unsigned, RelayControlSignatureRoleV1::OuterHelloClient)
            .map_err(|_| OuterRelayIoError::Handshake)?;
    hello.client_signature = signer
        .sign_reachability_message(domain, &message)
        .map_err(|_| OuterRelayIoError::Handshake)?;
    let response = RelayWireFrameV1::new(
        RelayWireKindV1::Control,
        frame.request_id(),
        encode_relay_control(&RelayControlV1::OuterClientHello(hello))
            .map_err(|_| OuterRelayIoError::Handshake)?,
    )
    .map_err(|_| OuterRelayIoError::Handshake)?;
    tokio::time::timeout_at(
        deadline.into(),
        write_stream_frame(stream, &response.encode()),
    )
    .await
    .map_err(|_| OuterRelayIoError::Deadline)??;
    let ack = tokio::time::timeout_at(deadline.into(), read_stream_frame(stream))
        .await
        .map_err(|_| OuterRelayIoError::Deadline)??;
    let ack = RelayWireFrameV1::decode(&ack).map_err(|_| OuterRelayIoError::InvalidFrame)?;
    if ack.kind() != RelayWireKindV1::Authenticated
        || ack.request_id() != frame.request_id()
        || ack.payload() != binding
    {
        return Err(OuterRelayIoError::Handshake);
    }
    Ok(value.expires_at.min(route.expires_at()))
}

fn verify_relay_control_signature(
    value: &RelayControlV1,
    role: RelayControlSignatureRoleV1,
    public_key: [u8; 32],
    signature: [u8; 64],
) -> Result<(), OuterRelayIoError> {
    let preimage =
        relay_control_signing_bytes(value, role).map_err(|_| OuterRelayIoError::Handshake)?;
    let key = VerifyingKey::from_bytes(&public_key).map_err(|_| OuterRelayIoError::Handshake)?;
    key.verify(&preimage, &Signature::from_bytes(&signature))
        .map_err(|_| OuterRelayIoError::Handshake)
}

fn install_aws_lc_provider() -> Result<(), OuterRelayIoError> {
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    match provider.install_default() {
        Ok(()) => Ok(()),
        Err(_) if rustls::crypto::CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => Err(OuterRelayIoError::Connect),
    }
}

fn tls_client_config(expected_spki: [u8; 32]) -> rustls::ClientConfig {
    let mut config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(RelaySpkiVerifier::new(expected_spki)))
        .with_no_client_auth();
    config.alpn_protocols = vec![OUTER_ALPN.to_vec()];
    config
}

fn quic_client_config(expected_spki: [u8; 32]) -> Result<ClientConfig, OuterRelayIoError> {
    let config = tls_client_config(expected_spki);
    let mut client = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(config)
            .map_err(|_| OuterRelayIoError::Connect)?,
    ));
    let mut transport = quinn::TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(1024 * 1024));
    transport.datagram_send_buffer_size(1024 * 1024);
    client.transport_config(Arc::new(transport));
    Ok(client)
}

#[derive(Clone)]
struct RelaySpkiVerifier {
    expected: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
}

impl RelaySpkiVerifier {
    fn new(expected: [u8; 32]) -> Self {
        Self {
            expected,
            algorithms: rustls::crypto::aws_lc_rs::default_provider()
                .signature_verification_algorithms,
        }
    }
}

impl fmt::Debug for RelaySpkiVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelaySpkiVerifier")
            .finish_non_exhaustive()
    }
}

impl ServerCertVerifier for RelaySpkiVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let observed = extract_ed25519_spki(end_entity)
            .ok_or_else(|| rustls::Error::General("invalid relay Ed25519 SPKI".into()))?;
        if observed != self.expected {
            return Err(rustls::Error::General("relay SPKI mismatch".into()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

fn extract_ed25519_spki(certificate: &CertificateDer<'_>) -> Option<[u8; 32]> {
    const PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let bytes = certificate.as_ref();
    let mut matches = bytes
        .windows(PREFIX.len())
        .enumerate()
        .filter_map(|(index, value)| {
            (value == PREFIX && index + PREFIX.len() + 32 <= bytes.len()).then_some(index)
        });
    let index = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    bytes[index + PREFIX.len()..index + PREFIX.len() + 32]
        .try_into()
        .ok()
}

#[derive(Clone, Debug)]
pub struct RelaySocketGlobalBudget {
    frames: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
    frame_limit: usize,
    byte_limit: usize,
}

impl RelaySocketGlobalBudget {
    pub fn standard() -> Self {
        Self::with_limits(RELAY_GLOBAL_FRAME_LIMIT, RELAY_GLOBAL_BYTE_LIMIT)
    }

    pub fn with_limits(frame_limit: usize, byte_limit: usize) -> Self {
        Self {
            frames: Arc::new(AtomicUsize::new(0)),
            bytes: Arc::new(AtomicUsize::new(0)),
            frame_limit,
            byte_limit,
        }
    }

    fn reserve(&self, bytes: usize) -> io::Result<GlobalLease> {
        reserve_atomic(&self.frames, 1, self.frame_limit)?;
        if let Err(error) = reserve_atomic(&self.bytes, bytes, self.byte_limit) {
            self.frames.fetch_sub(1, Ordering::AcqRel);
            return Err(error);
        }
        Ok(GlobalLease {
            budget: self.clone(),
            bytes,
        })
    }
}

#[derive(Debug)]
struct GlobalLease {
    budget: RelaySocketGlobalBudget,
    bytes: usize,
}

impl Drop for GlobalLease {
    fn drop(&mut self) {
        self.budget.frames.fetch_sub(1, Ordering::AcqRel);
        self.budget.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

fn reserve_atomic(value: &AtomicUsize, amount: usize, limit: usize) -> io::Result<()> {
    let mut current = value.load(Ordering::Acquire);
    loop {
        let next = current.checked_add(amount).ok_or_else(would_block)?;
        if next > limit {
            return Err(would_block());
        }
        match value.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(()),
            Err(observed) => current = observed,
        }
    }
}

fn would_block() -> io::Error {
    io::Error::from(io::ErrorKind::WouldBlock)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRelayTransmit {
    pub destination: SocketAddr,
    pub ecn: Option<EcnCodepoint>,
    pub contents: Vec<u8>,
    pub segment_size: Option<usize>,
    pub src_ip: Option<IpAddr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayInboundDatagram {
    pub source: SocketAddr,
    pub destination_ip: Option<IpAddr>,
    pub ecn: Option<EcnCodepoint>,
    pub contents: Vec<u8>,
}

#[derive(Debug)]
struct QueuedTransmit {
    value: OwnedRelayTransmit,
    _lease: GlobalLease,
}

#[derive(Debug)]
struct QueuedInbound {
    value: RelayInboundDatagram,
    _lease: GlobalLease,
}

#[derive(Debug)]
struct Shared {
    send_tx: mpsc::Sender<QueuedTransmit>,
    send_bytes: AtomicUsize,
    recv_queue: Mutex<VecDeque<QueuedInbound>>,
    recv_bytes: AtomicUsize,
    write_waiters: Mutex<Vec<Waker>>,
    recv_waker: Mutex<Option<Waker>>,
    terminal: Mutex<Option<io::ErrorKind>>,
    closed: AtomicBool,
    frame_limit: usize,
    byte_limit: usize,
    global: RelaySocketGlobalBudget,
}

pub struct RelayDatagramSocket {
    local_addr: SocketAddr,
    shared: Arc<Shared>,
}

impl fmt::Debug for RelayDatagramSocket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayDatagramSocket")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

pub struct RelaySocketDriver {
    shared: Arc<Shared>,
    send_rx: mpsc::Receiver<QueuedTransmit>,
}

impl RelayDatagramSocket {
    pub fn pair(
        local_addr: SocketAddr,
        global: RelaySocketGlobalBudget,
    ) -> (Arc<Self>, RelaySocketDriver) {
        Self::pair_with_limits(
            local_addr,
            RELAY_SOCKET_FRAME_LIMIT,
            RELAY_SOCKET_BYTE_LIMIT,
            global,
        )
    }

    pub fn pair_with_limits(
        local_addr: SocketAddr,
        frame_limit: usize,
        byte_limit: usize,
        global: RelaySocketGlobalBudget,
    ) -> (Arc<Self>, RelaySocketDriver) {
        let (send_tx, send_rx) = mpsc::channel(frame_limit);
        let shared = Arc::new(Shared {
            send_tx,
            send_bytes: AtomicUsize::new(0),
            recv_queue: Mutex::new(VecDeque::new()),
            recv_bytes: AtomicUsize::new(0),
            write_waiters: Mutex::new(Vec::new()),
            recv_waker: Mutex::new(None),
            terminal: Mutex::new(None),
            closed: AtomicBool::new(false),
            frame_limit,
            byte_limit,
            global,
        });
        (
            Arc::new(Self {
                local_addr,
                shared: shared.clone(),
            }),
            RelaySocketDriver { shared, send_rx },
        )
    }

    fn terminal_error(&self) -> Option<io::Error> {
        self.shared
            .terminal
            .lock()
            .ok()
            .and_then(|guard| *guard)
            .map(io::Error::from)
    }
}

impl AsyncUdpSocket for RelayDatagramSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(RelayWritePoller {
            shared: self.shared.clone(),
        })
    }

    fn try_send(&self, transmit: &Transmit<'_>) -> io::Result<()> {
        if let Some(error) = self.terminal_error() {
            return Err(error);
        }
        if transmit.contents.is_empty() || transmit.contents.len() > self.shared.byte_limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relay frame limit",
            ));
        }
        reserve_atomic(
            &self.shared.send_bytes,
            transmit.contents.len(),
            self.shared.byte_limit,
        )?;
        let lease = match self.shared.global.reserve(transmit.contents.len()) {
            Ok(lease) => lease,
            Err(error) => {
                self.shared
                    .send_bytes
                    .fetch_sub(transmit.contents.len(), Ordering::AcqRel);
                return Err(error);
            }
        };
        let queued = QueuedTransmit {
            value: OwnedRelayTransmit {
                destination: transmit.destination,
                ecn: transmit.ecn,
                contents: transmit.contents.to_vec(),
                segment_size: transmit.segment_size,
                src_ip: transmit.src_ip,
            },
            _lease: lease,
        };
        match self.shared.send_tx.try_send(queued) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.shared
                    .send_bytes
                    .fetch_sub(transmit.contents.len(), Ordering::AcqRel);
                match error {
                    mpsc::error::TrySendError::Full(_) => Err(would_block()),
                    mpsc::error::TrySendError::Closed(_) => {
                        Err(io::Error::from(io::ErrorKind::BrokenPipe))
                    }
                }
            }
        }
    }

    fn poll_recv(
        &self,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        if let Some(error) = self.terminal_error() {
            return Poll::Ready(Err(error));
        }
        let capacity = bufs.len().min(meta.len());
        let mut queue = match self.shared.recv_queue.lock() {
            Ok(queue) => queue,
            Err(_) => return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe))),
        };
        let mut count = 0;
        while count < capacity {
            let Some(queued) = queue.pop_front() else {
                break;
            };
            if queued.value.contents.len() > bufs[count].len() {
                self.shared
                    .recv_bytes
                    .fetch_sub(queued.value.contents.len(), Ordering::AcqRel);
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "receive buffer too small",
                )));
            }
            let len = queued.value.contents.len();
            bufs[count][..len].copy_from_slice(&queued.value.contents);
            meta[count] = RecvMeta {
                addr: queued.value.source,
                len,
                stride: len,
                ecn: queued.value.ecn,
                dst_ip: queued.value.destination_ip,
            };
            self.shared.recv_bytes.fetch_sub(len, Ordering::AcqRel);
            count += 1;
        }
        drop(queue);
        if count > 0 {
            Poll::Ready(Ok(count))
        } else {
            if let Ok(mut waker) = self.shared.recv_waker.lock() {
                *waker = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn may_fragment(&self) -> bool {
        false
    }
}

#[derive(Debug)]
struct RelayWritePoller {
    shared: Arc<Shared>,
}

impl UdpPoller for RelayWritePoller {
    fn poll_writable(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let terminal = self.shared.terminal.lock().ok().and_then(|guard| *guard);
        if let Some(kind) = terminal {
            return Poll::Ready(Err(io::Error::from(kind)));
        }
        if self.shared.send_tx.capacity() > 0
            && self.shared.send_bytes.load(Ordering::Acquire) < self.shared.byte_limit
        {
            return Poll::Ready(Ok(()));
        }
        if let Ok(mut waiters) = self.shared.write_waiters.lock() {
            waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}

impl RelaySocketDriver {
    pub async fn recv_outbound(&mut self) -> Option<OwnedRelayTransmit> {
        let queued = self.send_rx.recv().await?;
        self.shared
            .send_bytes
            .fetch_sub(queued.value.contents.len(), Ordering::AcqRel);
        wake_all(&self.shared.write_waiters);
        Some(queued.value)
    }

    pub fn push_inbound(&self, value: RelayInboundDatagram) -> io::Result<()> {
        if value.contents.is_empty() || value.contents.len() > self.shared.byte_limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "relay frame limit",
            ));
        }
        let mut queue = self
            .shared
            .recv_queue
            .lock()
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        if queue.len() >= self.shared.frame_limit {
            return Err(would_block());
        }
        reserve_atomic(
            &self.shared.recv_bytes,
            value.contents.len(),
            self.shared.byte_limit,
        )?;
        let lease = match self.shared.global.reserve(value.contents.len()) {
            Ok(lease) => lease,
            Err(error) => {
                self.shared
                    .recv_bytes
                    .fetch_sub(value.contents.len(), Ordering::AcqRel);
                return Err(error);
            }
        };
        queue.push_back(QueuedInbound {
            value,
            _lease: lease,
        });
        drop(queue);
        if let Ok(mut waker) = self.shared.recv_waker.lock() {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
        }
        Ok(())
    }

    pub fn fail(&self, kind: io::ErrorKind) {
        if let Ok(mut terminal) = self.shared.terminal.lock() {
            *terminal = Some(kind);
        }
        self.shared.closed.store(true, Ordering::Release);
        wake_all(&self.shared.write_waiters);
        if let Ok(mut waker) = self.shared.recv_waker.lock() {
            if let Some(waker) = waker.take() {
                waker.wake();
            }
        }
    }
}

impl Drop for RelaySocketDriver {
    fn drop(&mut self) {
        self.fail(io::ErrorKind::BrokenPipe);
    }
}

fn wake_all(waiters: &Mutex<Vec<Waker>>) {
    if let Ok(mut waiters) = waiters.lock() {
        for waker in waiters.drain(..) {
            waker.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn vnext_relay_tunnel_copies_bounds_drains_and_recovers() {
        let global = RelaySocketGlobalBudget::with_limits(2, 16);
        let (socket, mut driver) =
            RelayDatagramSocket::pair_with_limits("127.0.0.1:40000".parse().unwrap(), 1, 8, global);
        let destination = "127.0.0.1:40001".parse().unwrap();
        socket
            .try_send(&Transmit {
                destination,
                ecn: None,
                contents: b"opaque",
                segment_size: None,
                src_ip: None,
            })
            .unwrap();
        assert_eq!(
            socket
                .try_send(&Transmit {
                    destination,
                    ecn: None,
                    contents: b"again",
                    segment_size: None,
                    src_ip: None,
                })
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(driver.recv_outbound().await.unwrap().contents, b"opaque");
        socket
            .try_send(&Transmit {
                destination,
                ecn: None,
                contents: b"again",
                segment_size: None,
                src_ip: None,
            })
            .unwrap();
    }

    #[test]
    fn vnext_relay_tunnel_worker_failure_is_terminal() {
        let (socket, driver) = RelayDatagramSocket::pair(
            "127.0.0.1:41000".parse().unwrap(),
            RelaySocketGlobalBudget::standard(),
        );
        driver.fail(io::ErrorKind::ConnectionReset);
        let error = socket
            .try_send(&Transmit {
                destination: "127.0.0.1:41001".parse().unwrap(),
                ecn: None,
                contents: b"x",
                segment_size: None,
                src_ip: None,
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
    }
}
