//! Production outer-carrier authentication and relay-control serving.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ku_core::foundation::NodeId;
use ku_net::vnext_reachability_crypto::{
    possession_challenge_digest, possession_proof_signing_bytes,
};
use onebrain_protocol::{
    connectivity_signing_bytes, decode_connectivity_signaling, decode_relay_control,
    encode_connectivity_signaling, encode_relay_control, ConnectivitySignalingV1,
    ConnectivitySignatureRoleV1, HostAddressV1, ReachabilityEndpointV1, ReflexiveObservationV1,
    RelayAssociationV1, RelayConnectRequestV1, RelayControlV1, RelayPossessionProofV1,
    RelayWireFrameV1, RelayWireKindV1, MAX_RELAY_WIRE_PAYLOAD_BYTES,
};
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_rustls::server::TlsStream;

use crate::{
    principal_node_id, ActiveOuterAdmission, AssociationBinding, AuthenticatedOuterClient,
    DurableRelayState, OuterClientAuthenticator, OuterConnectionLimiter, RelayDataPlane,
    RelayDataPlaneError, RelayGlobalBudget, ReservationDecision, ReservationStore,
};

const IO_DEADLINE: Duration = Duration::from_secs(5);
const OUTER_EXPORTER_LABEL: &[u8] = b"EXPORTER-OneBrain-Relay-V1";

pub struct RelayProductionService {
    relay_signer: SigningKey,
    authenticator: Mutex<OuterClientAuthenticator>,
    reservations: Mutex<ReservationStore>,
    connections: Mutex<BTreeMap<[u8; 32], ConnectedClient>>,
    connect_sequences: Mutex<BTreeMap<NodeId, u64>>,
    reflexive_sequences: Mutex<BTreeMap<NodeId, u64>>,
    data_plane: Mutex<RelayDataPlane>,
    limiter: OuterConnectionLimiter,
}

#[derive(Clone)]
struct ConnectedClient {
    client: AuthenticatedOuterClient,
    outbound: mpsc::Sender<RelayWireFrameV1>,
}

impl RelayProductionService {
    pub fn new(
        signer: SigningKey,
        max_reservations: usize,
        max_per_target: usize,
        durable: Arc<DurableRelayState>,
    ) -> Result<Self, RelayDataPlaneError> {
        let authenticator = OuterClientAuthenticator::new_durable(signer.clone(), durable.clone());
        let reservations = ReservationStore::new_durable(
            signer.clone(),
            max_reservations,
            max_per_target,
            durable,
        )
        .map_err(|_| RelayDataPlaneError::InvalidAssociation)?;
        Ok(Self {
            relay_signer: signer.clone(),
            authenticator: Mutex::new(authenticator),
            reservations: Mutex::new(reservations),
            connections: Mutex::new(BTreeMap::new()),
            connect_sequences: Mutex::new(BTreeMap::new()),
            reflexive_sequences: Mutex::new(BTreeMap::new()),
            data_plane: Mutex::new(RelayDataPlane::new(RelayGlobalBudget::standard())),
            limiter: OuterConnectionLimiter::standard(),
        })
    }

    pub async fn serve_quic_connection(
        &self,
        connection: quinn::Connection,
    ) -> Result<(), RelayDataPlaneError> {
        let pending = self.limiter.begin(connection.remote_address().ip(), 1)?;
        let mut binding = [0u8; 32];
        connection
            .export_keying_material(&mut binding, OUTER_EXPORTER_LABEL, &[])
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        let (mut send, mut recv) = timeout(IO_DEADLINE, connection.accept_bi())
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .map_err(|_| RelayDataPlaneError::Closed)?;
        let mut preface = [0u8; 1];
        timeout(IO_DEADLINE, recv.read_exact(&mut preface))
            .await
            .map_err(|_| RelayDataPlaneError::Expired)?
            .map_err(|_| RelayDataPlaneError::Closed)?;
        if preface != [0] {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let (client, active) = self
            .authenticate_split(&mut send, &mut recv, binding, pending)
            .await?;
        let client = client.bind_observed_socket(connection.remote_address());
        let (outbound, mut outgoing) = mpsc::channel(64);
        self.register_connection(client.clone(), outbound)?;
        let result = self
            .control_loop_quic(&connection, &mut send, recv, &client, &mut outgoing, active)
            .await;
        self.unregister_connection(client.outer_connection_binding());
        result
    }

    pub async fn serve_tcp_connection(
        &self,
        mut stream: TlsStream<TcpStream>,
        peer: std::net::SocketAddr,
    ) -> Result<(), RelayDataPlaneError> {
        let pending = self.limiter.begin(peer.ip(), 1)?;
        let mut binding = [0u8; 32];
        stream
            .get_ref()
            .1
            .export_keying_material(&mut binding, OUTER_EXPORTER_LABEL, None)
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        let (client, active) = self
            .authenticate_single(&mut stream, binding, pending)
            .await?;
        let client = client.bind_observed_socket(peer);
        let (outbound, mut outgoing) = mpsc::channel(64);
        self.register_connection(client.clone(), outbound)?;
        let (read, mut write) = tokio::io::split(stream);
        let result = self
            .control_loop_tcp(read, &mut write, &client, &mut outgoing, active)
            .await;
        self.unregister_connection(client.outer_connection_binding());
        result
    }

    async fn authenticate_split<W, R>(
        &self,
        send: &mut W,
        recv: &mut R,
        binding: [u8; 32],
        pending: crate::PendingOuterAdmission,
    ) -> Result<(AuthenticatedOuterClient, ActiveOuterAdmission), RelayDataPlaneError>
    where
        W: AsyncWrite + Unpin,
        R: AsyncRead + Unpin,
    {
        let (request_id, challenge) = self.issue_challenge(binding)?;
        write_wire(
            send,
            &RelayWireFrameV1::new(
                RelayWireKindV1::Control,
                request_id,
                encode_relay_control(&RelayControlV1::OuterClientChallenge(challenge))
                    .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
            )
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
        )
        .await?;
        let hello = read_wire(recv).await?;
        let client = self.finish_authentication(hello, request_id, binding)?;
        write_wire(
            send,
            &RelayWireFrameV1::new(RelayWireKindV1::Authenticated, request_id, binding.to_vec())
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
        )
        .await?;
        Ok((client, pending.promote()?))
    }

    async fn authenticate_single<S>(
        &self,
        stream: &mut S,
        binding: [u8; 32],
        pending: crate::PendingOuterAdmission,
    ) -> Result<(AuthenticatedOuterClient, ActiveOuterAdmission), RelayDataPlaneError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (request_id, challenge) = self.issue_challenge(binding)?;
        write_wire(
            stream,
            &RelayWireFrameV1::new(
                RelayWireKindV1::Control,
                request_id,
                encode_relay_control(&RelayControlV1::OuterClientChallenge(challenge))
                    .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
            )
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
        )
        .await?;
        let hello = read_wire(stream).await?;
        let client = self.finish_authentication(hello, request_id, binding)?;
        write_wire(
            stream,
            &RelayWireFrameV1::new(RelayWireKindV1::Authenticated, request_id, binding.to_vec())
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
        )
        .await?;
        Ok((client, pending.promote()?))
    }

    fn issue_challenge(
        &self,
        binding: [u8; 32],
    ) -> Result<([u8; 16], onebrain_protocol::RelayOuterClientChallengeV1), RelayDataPlaneError>
    {
        let mut nonce = [0u8; 32];
        let mut request_id = [0u8; 16];
        OsRng.fill_bytes(&mut nonce);
        OsRng.fill_bytes(&mut request_id);
        let challenge = self
            .authenticator
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .issue_challenge(nonce, binding, unix_now()?)
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?;
        Ok((request_id, challenge))
    }

    fn finish_authentication(
        &self,
        frame: RelayWireFrameV1,
        request_id: [u8; 16],
        binding: [u8; 32],
    ) -> Result<AuthenticatedOuterClient, RelayDataPlaneError> {
        if frame.kind() != RelayWireKindV1::Control || frame.request_id() != request_id {
            return Err(RelayDataPlaneError::IdentityMismatch);
        }
        let hello = match decode_relay_control(frame.payload())
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?
        {
            RelayControlV1::OuterClientHello(value) => value,
            _ => return Err(RelayDataPlaneError::IdentityMismatch),
        };
        self.authenticator
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .authenticate(hello, binding, unix_now()?)
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)
    }

    async fn control_loop_quic<W, R>(
        &self,
        connection: &quinn::Connection,
        send: &mut W,
        recv: R,
        client: &AuthenticatedOuterClient,
        outgoing: &mut mpsc::Receiver<RelayWireFrameV1>,
        _active: ActiveOuterAdmission,
    ) -> Result<(), RelayDataPlaneError>
    where
        W: AsyncWrite + Unpin,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let (incoming_tx, mut incoming_rx) = mpsc::channel(64);
        tokio::spawn(read_wire_loop(recv, incoming_tx));
        loop {
            tokio::select! {
                request = incoming_rx.recv() => {
                    let request = request.ok_or(RelayDataPlaneError::Closed)??;
                    if let Some(response) = self.handle_frame(request, client).await? {
                        write_wire(send, &response).await?;
                    }
                }
                datagram = connection.read_datagram() => {
                    let datagram = datagram.map_err(|_| RelayDataPlaneError::Closed)?.to_vec();
                    self.forward_opaque(client, datagram).await?;
                }
                frame = outgoing.recv() => {
                    let frame = frame.ok_or(RelayDataPlaneError::Closed)?;
                    if frame.kind() == RelayWireKindV1::OpaqueDatagram {
                        connection.send_datagram(frame.payload().to_vec().into())
                            .map_err(|_| RelayDataPlaneError::Closed)?;
                    } else {
                        write_wire(send, &frame).await?;
                    }
                }
            }
        }
    }

    async fn control_loop_tcp<R, W>(
        &self,
        read: R,
        write: &mut W,
        client: &AuthenticatedOuterClient,
        outgoing: &mut mpsc::Receiver<RelayWireFrameV1>,
        _active: ActiveOuterAdmission,
    ) -> Result<(), RelayDataPlaneError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin,
    {
        let (incoming_tx, mut incoming_rx) = mpsc::channel(64);
        tokio::spawn(read_wire_loop(read, incoming_tx));
        loop {
            tokio::select! {
                request = incoming_rx.recv() => {
                    let request = request.ok_or(RelayDataPlaneError::Closed)??;
                    if request.kind() == RelayWireKindV1::OpaqueDatagram {
                        self.forward_opaque(client, request.payload().to_vec()).await?;
                    } else if let Some(response) = self.handle_frame(request, client).await? {
                        write_wire(write, &response).await?;
                    }
                }
                frame = outgoing.recv() => {
                    write_wire(write, &frame.ok_or(RelayDataPlaneError::Closed)?).await?;
                }
            }
        }
    }

    fn register_connection(
        &self,
        client: AuthenticatedOuterClient,
        outbound: mpsc::Sender<RelayWireFrameV1>,
    ) -> Result<(), RelayDataPlaneError> {
        let mut connections = self
            .connections
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?;
        let binding = client.outer_connection_binding();
        if connections.contains_key(&binding) {
            return Err(RelayDataPlaneError::DuplicateAssociation);
        }
        connections.insert(binding, ConnectedClient { client, outbound });
        Ok(())
    }

    fn unregister_connection(&self, binding: [u8; 32]) {
        if let Ok(mut connections) = self.connections.lock() {
            connections.remove(&binding);
        }
    }

    async fn handle_frame(
        &self,
        request: RelayWireFrameV1,
        client: &AuthenticatedOuterClient,
    ) -> Result<Option<RelayWireFrameV1>, RelayDataPlaneError> {
        match request.kind() {
            RelayWireKindV1::Control => self.handle_control(request, client).map(Some),
            RelayWireKindV1::ConnectRequest => self.handle_connect(request, client).await.map(Some),
            RelayWireKindV1::ReflexiveObservation => {
                self.handle_reflexive_observation(request, client).map(Some)
            }
            RelayWireKindV1::OpaqueDatagram => {
                self.forward_opaque(client, request.payload().to_vec())
                    .await?;
                Ok(None)
            }
            _ => Err(RelayDataPlaneError::InvalidEnvelope),
        }
    }

    fn handle_reflexive_observation(
        &self,
        frame: RelayWireFrameV1,
        client: &AuthenticatedOuterClient,
    ) -> Result<RelayWireFrameV1, RelayDataPlaneError> {
        if frame.payload().len() != 40 {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let reservation_id: [u8; 32] = frame.payload()[..32]
            .try_into()
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        let network_epoch = u64::from_be_bytes(
            frame.payload()[32..]
                .try_into()
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
        );
        if network_epoch == 0 {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let reservation = self
            .reservations
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .get(reservation_id)
            .cloned()
            .ok_or(RelayDataPlaneError::InvalidAssociation)?;
        if reservation.canonical.target_node_id != client.client_node_id()
            || reservation.bound_outer_connection != client.outer_connection_binding()
        {
            return Err(RelayDataPlaneError::ConnectionMismatch);
        }
        let observed = client
            .observed_socket()
            .ok_or(RelayDataPlaneError::ConnectionMismatch)?;
        if observed.port() == 0 {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let now = unix_now()?;
        let mut sequences = self
            .reflexive_sequences
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?;
        let sequence = sequences
            .get(&client.client_node_id())
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RelayDataPlaneError::Capacity)?;
        let host = match observed.ip() {
            std::net::IpAddr::V4(value) => HostAddressV1::Ipv4(value.octets()),
            std::net::IpAddr::V6(value) => HostAddressV1::Ipv6(value.octets()),
        };
        let mut value = ReflexiveObservationV1 {
            format: 1,
            relay_node_id: principal_node_id(self.relay_signer.verifying_key().as_bytes()),
            target_node_id: client.client_node_id(),
            reservation_id,
            observed_endpoint: ReachabilityEndpointV1 {
                host,
                port: observed.port(),
            },
            network_epoch,
            sequence,
            issued_at: now,
            expires_at: now.saturating_add(30).min(reservation.canonical.expires_at),
            relay_signature: [0; 64],
        };
        if value.expires_at <= now {
            return Err(RelayDataPlaneError::Expired);
        }
        value.relay_signature = self
            .relay_signer
            .sign(
                &connectivity_signing_bytes(
                    &ConnectivitySignalingV1::ReflexiveObservation(value.clone()),
                    ConnectivitySignatureRoleV1::ReflexiveRelay,
                )
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
            )
            .to_bytes();
        sequences.insert(client.client_node_id(), sequence);
        let payload =
            encode_connectivity_signaling(&ConnectivitySignalingV1::ReflexiveObservation(value))
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        RelayWireFrameV1::new(
            RelayWireKindV1::ReflexiveObservation,
            frame.request_id(),
            payload,
        )
        .map_err(|_| RelayDataPlaneError::InvalidEnvelope)
    }

    async fn handle_connect(
        &self,
        frame: RelayWireFrameV1,
        initiator: &AuthenticatedOuterClient,
    ) -> Result<RelayWireFrameV1, RelayDataPlaneError> {
        let request = match decode_connectivity_signaling(frame.payload())
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?
        {
            ConnectivitySignalingV1::RelayConnectRequest(value) => value,
            _ => return Err(RelayDataPlaneError::InvalidEnvelope),
        };
        let now = unix_now()?;
        self.validate_connect_request(&request, initiator, now)?;

        let (initiator_reservation, target_reservation) = {
            let reservations = self
                .reservations
                .lock()
                .map_err(|_| RelayDataPlaneError::Closed)?;
            (
                reservations
                    .get(request.initiator_reservation_id)
                    .cloned()
                    .ok_or(RelayDataPlaneError::InvalidAssociation)?,
                reservations
                    .get(request.target_reservation_id)
                    .cloned()
                    .ok_or(RelayDataPlaneError::InvalidAssociation)?,
            )
        };
        if initiator_reservation.canonical.target_node_id != request.initiator_node_id
            || initiator_reservation.bound_outer_connection != initiator.outer_connection_binding()
            || target_reservation.canonical.target_node_id != request.target_node_id
        {
            return Err(RelayDataPlaneError::ConnectionMismatch);
        }
        let target = self
            .connections
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .get(&target_reservation.bound_outer_connection)
            .cloned()
            .ok_or(RelayDataPlaneError::Closed)?;
        if target.client.client_node_id() != request.target_node_id {
            return Err(RelayDataPlaneError::IdentityMismatch);
        }

        let expires_at = request
            .expires_at
            .min(initiator_reservation.canonical.expires_at)
            .min(target_reservation.canonical.expires_at)
            .min(initiator.expires_at())
            .min(target.client.expires_at());
        if expires_at <= now {
            return Err(RelayDataPlaneError::Expired);
        }
        let mut association_id = [0u8; 32];
        OsRng.fill_bytes(&mut association_id);
        let mut association = RelayAssociationV1 {
            format: 1,
            relay_node_id: principal_node_id(self.relay_signer.verifying_key().as_bytes()),
            initiator_node_id: request.initiator_node_id,
            target_node_id: request.target_node_id,
            initiator_reservation_id: request.initiator_reservation_id,
            target_reservation_id: request.target_reservation_id,
            association_id,
            issued_at: now,
            expires_at,
            relay_signature: [0; 64],
        };
        association.relay_signature = self
            .relay_signer
            .sign(
                &connectivity_signing_bytes(
                    &ConnectivitySignalingV1::RelayAssociation(association.clone()),
                    ConnectivitySignatureRoleV1::RelayAssociationRelay,
                )
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
            )
            .to_bytes();
        self.data_plane
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .register(AssociationBinding::new(
                association_id,
                request.initiator_reservation_id,
                request.target_reservation_id,
                initiator.outer_connection_binding(),
                target.client.outer_connection_binding(),
                expires_at,
            )?)?;
        self.connect_sequences
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .insert(request.initiator_node_id, request.sequence);
        let request_notification = RelayWireFrameV1::new(
            RelayWireKindV1::ConnectRequest,
            frame.request_id(),
            frame.payload().to_vec(),
        )
        .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        let payload =
            encode_connectivity_signaling(&ConnectivitySignalingV1::RelayAssociation(association))
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        let notification = RelayWireFrameV1::new(
            RelayWireKindV1::Association,
            frame.request_id(),
            payload.clone(),
        )
        .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        // The target must independently validate the exact signed connect
        // request before it can admit the following relay association. The
        // relay forwards both canonical objects; it does not become peer
        // identity authority by creating the association.
        target
            .outbound
            .try_send(request_notification)
            .map_err(|_| RelayDataPlaneError::Capacity)?;
        target
            .outbound
            .try_send(notification)
            .map_err(|_| RelayDataPlaneError::Capacity)?;
        RelayWireFrameV1::new(RelayWireKindV1::Association, frame.request_id(), payload)
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)
    }

    fn validate_connect_request(
        &self,
        request: &RelayConnectRequestV1,
        initiator: &AuthenticatedOuterClient,
        now: u64,
    ) -> Result<(), RelayDataPlaneError> {
        if request.format != 1
            || request.initiator_node_id != initiator.client_node_id()
            || request.target_node_id == request.initiator_node_id
            || request.issued_at > now.saturating_add(30)
            || request.expires_at.saturating_add(30) < now
            || request.sequence == 0
            || self
                .connect_sequences
                .lock()
                .map_err(|_| RelayDataPlaneError::Closed)?
                .get(&request.initiator_node_id)
                .is_some_and(|value| request.sequence != value + 1)
        {
            return Err(RelayDataPlaneError::InvalidAssociation);
        }
        let signature = Signature::from_bytes(&request.initiator_signature);
        VerifyingKey::from_bytes(&initiator.client_public_key())
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)?
            .verify(
                &connectivity_signing_bytes(
                    &ConnectivitySignalingV1::RelayConnectRequest(request.clone()),
                    ConnectivitySignatureRoleV1::RelayConnectInitiator,
                )
                .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
                &signature,
            )
            .map_err(|_| RelayDataPlaneError::IdentityMismatch)
    }

    async fn forward_opaque(
        &self,
        sender: &AuthenticatedOuterClient,
        payload: Vec<u8>,
    ) -> Result<(), RelayDataPlaneError> {
        let delivered = self
            .data_plane
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .accept_fragment(sender.outer_connection_binding(), &payload, unix_now()?)?;
        let Some(delivered) = delivered else {
            return Ok(());
        };
        let target = self
            .connections
            .lock()
            .map_err(|_| RelayDataPlaneError::Closed)?
            .get(&delivered.recipient_connection())
            .cloned()
            .ok_or(RelayDataPlaneError::Closed)?;
        let frame = RelayWireFrameV1::new(
            RelayWireKindV1::OpaqueDatagram,
            [1; 16],
            delivered.payload().to_vec(),
        )
        .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        target
            .outbound
            .try_send(frame)
            .map_err(|_| RelayDataPlaneError::Capacity)
    }

    fn handle_control(
        &self,
        request: RelayWireFrameV1,
        client: &AuthenticatedOuterClient,
    ) -> Result<RelayWireFrameV1, RelayDataPlaneError> {
        if request.kind() != RelayWireKindV1::Control {
            return Err(RelayDataPlaneError::InvalidEnvelope);
        }
        let control = decode_relay_control(request.payload())
            .map_err(|_| RelayDataPlaneError::InvalidEnvelope)?;
        let now = unix_now()?;
        let response = match control {
            RelayControlV1::Reserve(value) => {
                match self
                    .reservations
                    .lock()
                    .map_err(|_| RelayDataPlaneError::Closed)?
                    .reserve(value, client, now)
                    .map_err(|_| RelayDataPlaneError::InvalidAssociation)?
                {
                    ReservationDecision::Granted(value) => RelayControlV1::Granted(value),
                    ReservationDecision::Denied(value) => RelayControlV1::Denied(value),
                }
            }
            RelayControlV1::Keepalive(value) => {
                self.reservations
                    .lock()
                    .map_err(|_| RelayDataPlaneError::Closed)?
                    .keepalive(value.clone(), client, now)
                    .map_err(|_| RelayDataPlaneError::InvalidAssociation)?;
                RelayControlV1::Keepalive(value)
            }
            RelayControlV1::Revoke(value) => {
                self.reservations
                    .lock()
                    .map_err(|_| RelayDataPlaneError::Closed)?
                    .revoke(value.clone(), client, now)
                    .map_err(|_| RelayDataPlaneError::InvalidAssociation)?;
                RelayControlV1::Revoke(value)
            }
            RelayControlV1::PossessionChallenge(value) => {
                if value.relay_node_id
                    != principal_node_id(self.relay_signer.verifying_key().as_bytes())
                    || value.issued_at > now.saturating_add(30)
                    || value.expires_at.saturating_add(30) < now
                {
                    return Err(RelayDataPlaneError::IdentityMismatch);
                }
                let binding = client.outer_connection_binding();
                let mut proof = RelayPossessionProofV1 {
                    challenge_digest: possession_challenge_digest(&value),
                    connection_binding_digest: binding,
                    signature: [0; 64],
                };
                proof.signature = self
                    .relay_signer
                    .sign(&possession_proof_signing_bytes(&value, binding))
                    .to_bytes();
                RelayControlV1::PossessionProof(proof)
            }
            _ => return Err(RelayDataPlaneError::InvalidEnvelope),
        };
        RelayWireFrameV1::new(
            RelayWireKindV1::Control,
            request.request_id(),
            encode_relay_control(&response).map_err(|_| RelayDataPlaneError::InvalidEnvelope)?,
        )
        .map_err(|_| RelayDataPlaneError::InvalidEnvelope)
    }
}

async fn write_wire<W>(stream: &mut W, frame: &RelayWireFrameV1) -> Result<(), RelayDataPlaneError>
where
    W: AsyncWrite + Unpin,
{
    let bytes = frame.encode();
    let length = u32::try_from(bytes.len()).map_err(|_| RelayDataPlaneError::Oversize)?;
    timeout(IO_DEADLINE, async {
        stream.write_all(&length.to_be_bytes()).await?;
        stream.write_all(&bytes).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| RelayDataPlaneError::Expired)?
    .map_err(|_| RelayDataPlaneError::Closed)
}

async fn read_wire<R>(stream: &mut R) -> Result<RelayWireFrameV1, RelayDataPlaneError>
where
    R: AsyncRead + Unpin,
{
    let mut prefix = [0u8; 4];
    timeout(IO_DEADLINE, stream.read_exact(&mut prefix))
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::Closed)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_RELAY_WIRE_PAYLOAD_BYTES + 26 {
        return Err(RelayDataPlaneError::Oversize);
    }
    let mut bytes = vec![0; length];
    timeout(IO_DEADLINE, stream.read_exact(&mut bytes))
        .await
        .map_err(|_| RelayDataPlaneError::Expired)?
        .map_err(|_| RelayDataPlaneError::Closed)?;
    RelayWireFrameV1::decode(&bytes).map_err(|_| RelayDataPlaneError::InvalidEnvelope)
}

async fn read_wire_loop<R>(
    mut stream: R,
    sender: mpsc::Sender<Result<RelayWireFrameV1, RelayDataPlaneError>>,
) where
    R: AsyncRead + Unpin,
{
    loop {
        let frame = read_wire(&mut stream).await;
        let stop = frame.is_err();
        if sender.send(frame).await.is_err() || stop {
            break;
        }
    }
}

fn unix_now() -> Result<u64, RelayDataPlaneError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| RelayDataPlaneError::Expired)
}
