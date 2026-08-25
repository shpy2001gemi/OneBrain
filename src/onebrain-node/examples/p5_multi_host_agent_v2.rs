//! Long-lived P5 V2 agent process. Private keys are intentionally absent.

#[cfg(unix)]
use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(any(unix, test))]
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
#[cfg(unix)]
use ku_core::foundation::NodeId;
#[cfg(unix)]
use ku_net::transport::{QuicTransport, TransportConfig};
#[cfg(unix)]
use ku_net::vnext_connection_executor::{
    ConnectionPlannerExecutor, ProductionRelayAssociationClient, ProductionRelayCarrierDialer,
    QuicDirectCarrierDialer,
};
#[cfg(unix)]
use ku_net::vnext_connectivity_signaling::ConnectivitySignalingValidator;
#[cfg(unix)]
use ku_net::vnext_reachability_crypto::{
    InMemoryReachabilityReplayStore, KnownPeerIdentity, ReachabilityAdmission,
    ReachabilityAdmissionPreparer, ReachabilityDialValidator, ReachabilityIdentitySigner,
    ReachabilityLockFreePreparation, ReachabilityRecordAdmission, SystemPublicEndpointResolver,
    ValidatedReachabilityAdvertisement,
};
#[cfg(unix)]
use ku_net::vnext_relay_discovery::{
    InMemoryAuthenticatedSessionRegistry, RelayDiscovery, RelayDiscoveryPolicy,
    RelayDiscoveryPreparer, RelayDiscoverySource, VerifiedRelayDiscovery,
};
use onebrain_base_contract::{SourceCommitId, SourceCommitIdentity, ToolchainIdentity};
use onebrain_node::compiled_base_runtime_config;
#[cfg(unix)]
use onebrain_node::vnext_config::VNextNetworkPolicy;
#[cfg(unix)]
use onebrain_node::vnext_connection_planner::{
    ProductionExpectedPeerCarrierSelector, RoutedVNextSession,
};
#[cfg(unix)]
use onebrain_node::vnext_network_runtime::{ArmedDirectInbound, VNextNetworkRuntime};
#[cfg(unix)]
use onebrain_node::vnext_p5_signer_provider::{
    DurableSequenceCursor, ExternalP5Signer, UnixSocketP5SigningProvider,
};
#[cfg(unix)]
use onebrain_node::vnext_reachability_manager::{
    admit_relay_records, ProductionRelayDialRouteProvider, ProductionRelayPossessionClient,
    ProductionRelayReservationClient, RelayReservationManager, VNextReachabilityPolicy,
};
#[cfg(unix)]
use onebrain_node::vnext_runtime_rollout::{VNextRuntimeLaneRequest, VNextRuntimeRollout};
#[cfg(unix)]
use onebrain_protocol::{
    decode_connectivity_signaling, decode_reachability_object, encode_reachability_object,
    reachability_signing_parts, relay_control_signing_parts, ConnectivitySignalingV1,
    HostAddressV1, PublicCandidateKindV1, PublicCandidateV1, ReachabilityAdvertisementV1,
    ReachabilityEndpointV1, ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayCandidateV1,
    RelayControlSignatureRoleV1, RelayControlV1, RelayReservationV1, RelayReserveRequestV1,
};
#[cfg(unix)]
use rand::rngs::OsRng;
#[cfg(unix)]
use rand::RngCore;
#[cfg(unix)]
use serde::{Deserialize, Serialize};

const SESSION_CONFIG: &str = "/run/onebrain/p5-v2/current-session.json";
const IDENTITY_SOCKET: &str = "/run/onebrain/p5-v2/identity-signer.sock";
const RECEIPT_SOCKET: &str = "/run/onebrain/p5-v2/receipt-signer.sock";
#[cfg(any(unix, test))]
const AGENT_STATE_ROOT: &str = "/var/lib/onebrain/p5-v2-agent";
#[cfg(any(unix, test))]
const COMMAND_CURSOR: &str = "/var/lib/onebrain/p5-v2-agent/agent-command.cursor";
#[cfg(unix)]
const MAX_FRAME: usize = 65_536;
#[cfg(unix)]
const CONTROL_DOMAIN: &[u8] = b"onebrain/p5/signed-control-frame/v2\0";
#[cfg(unix)]
const ADMIN_RECEIPT_DOMAIN: &[u8] = b"onebrain/p5/admin-operation-receipt/v2";

#[cfg(any(unix, test))]
fn identity_client_cursor(host_id: &str) -> PathBuf {
    PathBuf::from(AGENT_STATE_ROOT).join(format!("{host_id}-identity-client.cursor"))
}

#[cfg(any(unix, test))]
fn advertisement_cursor(host_id: &str) -> PathBuf {
    PathBuf::from(AGENT_STATE_ROOT).join(format!("{host_id}-advertisement.cursor"))
}

#[cfg(any(unix, test))]
fn runner_data_root(host_id: &str) -> PathBuf {
    PathBuf::from(AGENT_STATE_ROOT).join(host_id)
}

#[cfg(any(unix, test))]
fn relay_reservation_cursor(relay_key: &str) -> PathBuf {
    PathBuf::from(AGENT_STATE_ROOT).join(format!("relay-reservation-{relay_key}.cursor"))
}

#[cfg(unix)]
#[derive(Deserialize, Serialize)]
struct AgentSessionConfig {
    host_id: String,
    controller_application_public_key: String,
    identity_signer_public_key: String,
    receipt_signer_public_key: String,
    request_digest: String,
    inventory_blake3: String,
    evidence_authority: EvidenceAuthorityConfig,
    session_id: String,
    expires_at: u64,
}

#[cfg(unix)]
#[derive(Clone, Deserialize, Serialize)]
struct EvidenceAuthorityConfig {
    inventory_blake3: String,
    public_probe_set_blake3: String,
    topology_attestation_blake3: String,
    provider_evidence_blake3: String,
    provider_evidence_status: String,
    qualification_tier: String,
}

#[cfg(unix)]
struct AgentRuntimeState {
    host_id: String,
    runtime: tokio::runtime::Runtime,
    network: Option<VNextNetworkRuntime>,
    rollout: VNextRuntimeRollout,
    sessions: BTreeMap<String, RoutedVNextSession>,
    advertisements: BTreeMap<String, ([u8; 32], ValidatedReachabilityAdvertisement)>,
    identity: Arc<ExternalP5Signer>,
    advertisement_preparer: Arc<ReachabilityAdmissionPreparer>,
    advertisement_admission: std::sync::Mutex<ReachabilityAdmission>,
    connectivity_validator: std::sync::Mutex<ConnectivitySignalingValidator>,
    discovery: Arc<tokio::sync::RwLock<RelayDiscovery>>,
    discovery_preparer: Arc<RelayDiscoveryPreparer>,
    possession_client: Arc<ProductionRelayPossessionClient>,
    reservation_client: Arc<ProductionRelayReservationClient>,
    reservations: Arc<RelayReservationManager>,
    relay_sequences: BTreeMap<String, DurableSequenceCursor>,
    advertisement_sequence: DurableSequenceCursor,
    cursor_binding: [u8; 32],
    selector: ProductionExpectedPeerCarrierSelector,
    executor: Arc<ConnectionPlannerExecutor>,
    selector_transport: Arc<QuicTransport>,
    network_data_root: PathBuf,
    checkpoints: BTreeMap<String, AgentCheckpointV2>,
    direct_inbound_arms: BTreeMap<String, ArmedDirectInbound>,
}

#[cfg(unix)]
#[derive(Clone, Deserialize, Serialize)]
struct AgentCheckpointV2 {
    expected_peer: String,
    acknowledged_sequence: u64,
    intent_blake3: String,
    roots_blake3: String,
    route_receipt_blake3: String,
    session_id: String,
    transport_binding_blake3: String,
    checkpoint_blake3: String,
}

#[cfg(unix)]
impl AgentRuntimeState {
    fn new(
        config: &AgentSessionConfig,
        binding: [u8; 32],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cursor = DurableSequenceCursor::open(identity_client_cursor(&config.host_id), binding)?;
        let initial_sequence = cursor.highest()?;
        let provider = UnixSocketP5SigningProvider::new(
            IDENTITY_SOCKET,
            decode_hex::<32>(&config.identity_signer_public_key)?,
            Duration::from_secs(3),
        )?
        .with_cursor(cursor);
        let identity = Arc::new(ExternalP5Signer::new(
            Arc::new(provider),
            initial_sequence,
            Duration::from_secs(3),
        ));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(1)
            .build()?;
        let endpoint_resolver = Arc::new(SystemPublicEndpointResolver::new(4)?);
        let advertisement_preparer = Arc::new(ReachabilityAdmissionPreparer::new(
            endpoint_resolver.clone(),
            4,
        )?);
        let dial_validator = Arc::new(ReachabilityDialValidator::new(
            endpoint_resolver.clone(),
            4,
        )?);
        let discovery_preparer = Arc::new(RelayDiscoveryPreparer::new(
            Arc::new(ReachabilityAdmissionPreparer::new(endpoint_resolver, 4)?),
            Arc::clone(&dial_validator),
        ));
        let discovery = Arc::new(tokio::sync::RwLock::new(RelayDiscovery::new(
            RelayDiscoveryPolicy::default(),
            ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default())),
            Arc::new(InMemoryAuthenticatedSessionRegistry::default()),
        )));
        let possession_client = Arc::new(ProductionRelayPossessionClient::new(identity.clone()));
        let reachability_policy = VNextReachabilityPolicy::default();
        let reservation_client = Arc::new(ProductionRelayReservationClient::new(identity.clone()));
        let reservations = Arc::new(
            RelayReservationManager::new(
                reservation_client.clone(),
                Arc::new(ProductionRelayDialRouteProvider::new(Arc::clone(
                    &dial_validator,
                ))),
                reachability_policy,
            )
            .map_err(|error| format!("relay reservation manager init failed: {error:?}"))?,
        );
        let selector_transport =
            Arc::new(runtime.block_on(QuicTransport::bind(TransportConfig {
                bind_addr: "0.0.0.0:0".parse()?,
                ..TransportConfig::default()
            }))?);
        let executor = Arc::new(ConnectionPlannerExecutor::new(
            Arc::new(QuicDirectCarrierDialer::new(Arc::clone(
                &selector_transport,
            ))),
            Arc::new(ProductionRelayCarrierDialer::standard()),
            Arc::new(ProductionRelayAssociationClient::new(identity.public_key())),
        ));
        let selector = ProductionExpectedPeerCarrierSelector::new(
            dial_validator,
            Arc::clone(&executor),
            Duration::from_secs(20),
        )
        .map_err(|error| format!("carrier selector init failed: {error:?}"))?
        .with_relay(
            Arc::clone(&discovery),
            Arc::clone(&reservations),
            identity.clone(),
            1,
        )
        .map_err(|error| format!("relay selector init failed: {error:?}"))?;
        let advertisement_sequence =
            DurableSequenceCursor::open(advertisement_cursor(&config.host_id), binding)?;
        let runner_data_root = runner_data_root(&config.host_id);
        let network_data_root = runner_data_root.join("network");
        let rollout = VNextRuntimeRollout::open(
            &runner_data_root.join("rollout"),
            VNextRuntimeLaneRequest::all_enabled(),
            VNextRuntimeLaneRequest {
                network: false,
                distributed_kql: false,
                public_use_evidence_publish: false,
                distributed_pomv_view: false,
            },
        )?;
        Ok(Self {
            host_id: config.host_id.clone(),
            runtime,
            network: None,
            rollout,
            sessions: BTreeMap::new(),
            advertisements: BTreeMap::new(),
            identity,
            advertisement_preparer,
            advertisement_admission: std::sync::Mutex::new(ReachabilityAdmission::new(Arc::new(
                InMemoryReachabilityReplayStore::default(),
            ))),
            connectivity_validator: std::sync::Mutex::new(ConnectivitySignalingValidator::new(
                Arc::new(InMemoryReachabilityReplayStore::default()),
            )),
            discovery,
            discovery_preparer,
            possession_client,
            reservation_client,
            reservations,
            relay_sequences: BTreeMap::new(),
            advertisement_sequence,
            cursor_binding: binding,
            selector,
            executor,
            selector_transport,
            network_data_root,
            checkpoints: BTreeMap::new(),
            direct_inbound_arms: BTreeMap::new(),
        })
    }
}

#[cfg(unix)]
#[derive(Deserialize, Serialize)]
struct AgentCommandFrame {
    format: u64,
    host_id: String,
    session_id: String,
    sequence: u64,
    issued_at: u64,
    expires_at: u64,
    command: String,
    #[serde(default)]
    parameters: serde_json::Value,
    signature: String,
}

#[cfg(unix)]
#[derive(Serialize)]
struct UnsignedAgentCommand<'a> {
    command: &'a str,
    expires_at: u64,
    format: u64,
    host_id: &'a str,
    issued_at: u64,
    parameters: &'a serde_json::Value,
    sequence: u64,
    session_id: &'a str,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("P5 V2 agent failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--print-compiled-binding"] {
        return print_compiled_binding();
    }
    if args
        != [
            "--control-socket-fd",
            "3",
            "--identity-signer-socket",
            IDENTITY_SOCKET,
            "--receipt-signer-socket",
            RECEIPT_SOCKET,
            "--session-config",
            SESSION_CONFIG,
            "--bind",
            "0.0.0.0:41010",
        ]
    {
        return Err("expected fixed P5 V2 socket/config arguments".into());
    }
    if !cfg!(target_os = "linux") {
        return Err("P5 V2 service requires Linux".into());
    }
    let metadata = fs::symlink_metadata(SESSION_CONFIG)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("invalid session config".into());
    }
    serve_fd3()
}

#[cfg(unix)]
fn serve_fd3() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixListener;
    let config_bytes = fs::read(SESSION_CONFIG)?;
    let config: AgentSessionConfig = serde_json::from_slice(&config_bytes)?;
    validate_session_config(&config)?;
    let cursor_binding = *blake3::hash(&config_bytes).as_bytes();
    let cursor = DurableSequenceCursor::open(COMMAND_CURSOR, cursor_binding)?;
    let mut runtime_state = AgentRuntimeState::new(&config, cursor_binding)?;
    let listener = unsafe { UnixListener::from_raw_fd(3) };
    loop {
        let (mut stream, _) = listener.accept()?;
        let mut length = [0u8; 4];
        stream.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > MAX_FRAME {
            continue;
        }
        let mut frame = vec![0; length];
        stream.read_exact(&mut frame)?;
        let command = validate_command(&config, &frame)?;
        // Commit replay state before any command effect or receipt emission.
        cursor.advance(command.sequence)?;
        let (result, shutdown) = execute_closed_command(&config, &command, &mut runtime_state)?;
        let response = signed_child_receipt(&config, &command, &frame, &result)?;
        stream.write_all(&(response.len() as u32).to_be_bytes())?;
        stream.write_all(&response)?;
        stream.flush()?;
        if shutdown {
            return Ok(());
        }
    }
}

#[cfg(unix)]
fn validate_session_config(config: &AgentSessionConfig) -> Result<(), Box<dyn std::error::Error>> {
    let now = unix_now()?;
    if config.host_id.is_empty()
        || config.host_id.len() > 128
        || !config
            .host_id
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
        || decode_hex::<32>(&config.controller_application_public_key).is_err()
        || decode_hex::<32>(&config.identity_signer_public_key).is_err()
        || decode_hex::<32>(&config.receipt_signer_public_key).is_err()
        || decode_hex::<32>(&config.request_digest).is_err()
        || decode_hex::<32>(&config.inventory_blake3).is_err()
        || config.inventory_blake3 != config.evidence_authority.inventory_blake3
        || decode_hex::<32>(&config.evidence_authority.inventory_blake3).is_err()
        || decode_hex::<32>(&config.evidence_authority.public_probe_set_blake3).is_err()
        || decode_hex::<32>(&config.evidence_authority.topology_attestation_blake3).is_err()
        || decode_hex::<32>(&config.evidence_authority.provider_evidence_blake3).is_err()
        || config
            .evidence_authority
            .provider_evidence_status
            .is_empty()
        || config.evidence_authority.qualification_tier != "production-reference"
        || decode_hex::<32>(&config.session_id).is_err()
        || config.expires_at < now
    {
        return Err("invalid P5 V2 session authority".into());
    }
    Ok(())
}

#[cfg(unix)]
fn validate_command(
    config: &AgentSessionConfig,
    bytes: &[u8],
) -> Result<AgentCommandFrame, Box<dyn std::error::Error>> {
    let command: AgentCommandFrame = serde_json::from_slice(bytes)?;
    let now = unix_now()?;
    if command.format != 2
        || command.host_id != config.host_id
        || command.session_id != config.session_id
        || command.sequence == 0
        || command.issued_at > now.saturating_add(30)
        || command.expires_at.saturating_add(30) < now
        || command.expires_at > config.expires_at
    {
        return Err("command authority/freshness mismatch".into());
    }
    let unsigned = UnsignedAgentCommand {
        command: &command.command,
        expires_at: command.expires_at,
        format: command.format,
        host_id: &command.host_id,
        issued_at: command.issued_at,
        parameters: &command.parameters,
        sequence: command.sequence,
        session_id: &command.session_id,
    };
    let canonical = serde_json::to_vec(&unsigned)?;
    let mut preimage = Vec::with_capacity(CONTROL_DOMAIN.len() + canonical.len());
    preimage.extend_from_slice(CONTROL_DOMAIN);
    preimage.extend_from_slice(&canonical);
    let public = VerifyingKey::from_bytes(&decode_hex::<32>(
        &config.controller_application_public_key,
    )?)?;
    public.verify(
        &preimage,
        &Signature::from_bytes(&decode_hex::<64>(&command.signature)?),
    )?;
    Ok(command)
}

#[cfg(unix)]
fn execute_closed_command(
    config: &AgentSessionConfig,
    command: &AgentCommandFrame,
    state: &mut AgentRuntimeState,
) -> Result<(serde_json::Value, bool), Box<dyn std::error::Error>> {
    let value = match command.command.as_str() {
        "status" => {
            let status = fs::read_to_string("/proc/self/status")?;
            serde_json::json!({
                "accepted": true,
                "command": "status",
                "network_started": state.network.is_some(),
                "process_status_blake3": blake3::hash(status.as_bytes()).to_hex().to_string(),
                "routed_sessions": state.sessions.len(),
            })
        }
        "start-reachability" => {
            if state.network.is_some() {
                return Err("reachability runtime is already started".into());
            }
            fs::create_dir_all(&state.network_data_root)?;
            let network =
                state
                    .runtime
                    .block_on(VNextNetworkRuntime::start_with_signer_and_rollout(
                        &state.network_data_root,
                        "0.0.0.0:41010".parse()?,
                        VNextNetworkPolicy::default(),
                        state.identity.clone(),
                        state.rollout.clone(),
                    ))?;
            state
                .reservation_client
                .attach_shared_quic_transport(network.shared_transport())
                .map_err(|error| {
                    format!("shared relay/direct QUIC attachment failed: {error:?}")
                })?;
            let local_node_id = hex(&network.status().principal);
            state.network = Some(network);
            serde_json::json!({
                "accepted": true,
                "bind": "0.0.0.0:41010",
                "command": "start-reachability",
                "local_node_id": local_node_id,
                "rollout": rollout_json(&state.rollout)?,
            })
        }
        "connect-expected" | "reconnect-expected" => {
            let expected_peer = command_parameter_string(command, "expected_peer")?;
            let peer = NodeId::from_bytes(decode_hex::<32>(expected_peer)?);
            let peer_public_key =
                decode_hex::<32>(command_parameter_string(command, "peer_public_key")?)?;
            let identity = KnownPeerIdentity::from_public_key(peer_public_key);
            if identity.node_id != peer {
                return Err("expected peer/public-key mismatch".into());
            }
            let advertisement_bytes =
                decode_hex_vec(command_parameter_string(command, "advertisement_hex")?)?;
            let advertisement_digest = *blake3::hash(&advertisement_bytes).as_bytes();
            let advertisement = if let Some((cached_digest, cached)) =
                state.advertisements.get(expected_peer)
            {
                if *cached_digest != advertisement_digest {
                    return Err("peer advertisement changed without a new session command".into());
                }
                cached.clone()
            } else {
                let admitted = admit_peer_advertisement(state, &advertisement_bytes, &identity)?;
                state.advertisements.insert(
                    expected_peer.to_owned(),
                    (advertisement_digest, admitted.clone()),
                );
                admitted
            };
            let network = state
                .network
                .as_ref()
                .ok_or("reachability runtime is not started")?;
            if command.command == "reconnect-expected" {
                if let Some(previous) = state.sessions.remove(expected_peer) {
                    previous.close();
                }
            } else if state.sessions.contains_key(expected_peer) {
                return Err("expected peer already has a routed session".into());
            }
            let session = state
                .runtime
                .block_on(network.connect_expected_advertisement(
                    &state.selector,
                    peer,
                    &advertisement,
                ))?;
            let session_id = hex(&session.authenticated().session_id);
            let route_receipt_blake3 = hex(&session.route_receipt_digest());
            let path_kind = format!("{:?}", session.carrier().path_kind());
            state.sessions.insert(expected_peer.to_owned(), session);
            serde_json::json!({
                "accepted": true,
                "command": command.command,
                "expected_peer": expected_peer,
                "path_kind": path_kind,
                "route_receipt_blake3": route_receipt_blake3,
                "session_id": session_id,
            })
        }
        "arm-direct-inbound" => arm_direct_inbound(command, state)?,
        "connect-ring" => connect_ring(command, state, false)?,
        "reconnect-ring" => connect_ring(command, state, true)?,
        "deliver-marker" => {
            let expected_peer = command_parameter_string(command, "expected_peer")?;
            let payload = decode_hex_vec(command_parameter_string(command, "payload_hex")?)?;
            if payload.is_empty() || payload.len() > 4_096 {
                return Err("marker payload is outside the closed byte bound".into());
            }
            let session = state
                .sessions
                .get(expected_peer)
                .ok_or("expected peer has no routed session")?;
            state.runtime.block_on(session.send_uni(&payload))?;
            serde_json::json!({
                "accepted": true,
                "command": "deliver-marker",
                "expected_peer": expected_peer,
                "marker_blake3": blake3::hash(&payload).to_hex().to_string(),
                "marker_bytes": payload.len(),
                "session_id": hex(&session.authenticated().session_id),
            })
        }
        "receive-marker" => {
            let expected_peer = command_parameter_string(command, "expected_peer")?;
            let expected_blake3 = command_parameter_string(command, "expected_blake3")?;
            let expected_bytes = command_parameter_u64(command, "expected_bytes")?;
            if expected_bytes == 0 || expected_bytes > 4_096 {
                return Err("expected marker size is outside the closed byte bound".into());
            }
            let session = state
                .sessions
                .get(expected_peer)
                .ok_or("expected peer has no routed session")?;
            let payload = state
                .runtime
                .block_on(session.recv_uni(expected_bytes as usize))?;
            let observed = blake3::hash(&payload).to_hex().to_string();
            if payload.len() != expected_bytes as usize || observed != expected_blake3 {
                return Err("received marker bytes/digest mismatch".into());
            }
            serde_json::json!({
                "accepted": true,
                "command": "receive-marker",
                "expected_peer": expected_peer,
                "marker_blake3": observed,
                "marker_bytes": payload.len(),
                "session_id": hex(&session.authenticated().session_id),
            })
        }
        "wait-barrier" => serde_json::json!({
            "accepted": true,
            "command": "wait-barrier",
            "parameters_blake3": blake3::hash(&serde_json::to_vec(&command.parameters)?).to_hex().to_string(),
        }),
        "ensure-reservations" => ensure_reservations(command, state)?,
        "publish-advertisement" => publish_advertisement(state)?,
        "record-checkpoint" => record_checkpoint(command, state)?,
        "prepare-fault-target" => prepare_fault_target(command, state)?,
        "measure-fault-boundary" => measure_fault_boundary(config, command, state)?,
        "shutdown" => {
            for (_, session) in std::mem::take(&mut state.sessions) {
                session.close();
            }
            if let Some(mut network) = state.network.take() {
                state.runtime.block_on(network.shutdown());
            }
            state.runtime.block_on(state.selector_transport.shutdown());
            serde_json::json!({"accepted": true, "command": "shutdown"})
        }
        _ => return Err("command is outside the P5 V2 allowlist".into()),
    };
    Ok((value, command.command == "shutdown"))
}

#[cfg(unix)]
fn record_checkpoint(
    command: &AgentCommandFrame,
    state: &mut AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let expected_peer = command_parameter_string(command, "expected_peer")?;
    let acknowledged_sequence = command_parameter_u64(command, "acknowledged_sequence")?;
    if acknowledged_sequence == 0 {
        return Err("checkpoint sequence must be nonzero".into());
    }
    let intent_blake3 = canonical_digest_parameter(command, "intent_blake3")?;
    let roots_blake3 = canonical_digest_parameter(command, "roots_blake3")?;
    let session = state
        .sessions
        .get(expected_peer)
        .ok_or("checkpoint peer has no authenticated routed session")?;
    if let Some(previous) = state.checkpoints.get(expected_peer) {
        if acknowledged_sequence != previous.acknowledged_sequence.saturating_add(1)
            || intent_blake3 != previous.intent_blake3
            || roots_blake3 != previous.roots_blake3
        {
            return Err(
                "checkpoint resume does not preserve exact acknowledged intent/roots".into(),
            );
        }
    } else if acknowledged_sequence != 1 {
        return Err("first checkpoint sequence must be one".into());
    }
    let mut checkpoint = AgentCheckpointV2 {
        expected_peer: expected_peer.to_owned(),
        acknowledged_sequence,
        intent_blake3,
        roots_blake3,
        route_receipt_blake3: hex(&session.route_receipt_digest()),
        session_id: hex(&session.authenticated().session_id),
        transport_binding_blake3: hex(&session.transport_binding_digest()),
        checkpoint_blake3: String::new(),
    };
    checkpoint.checkpoint_blake3 = blake3::hash(&serde_json::to_vec(&checkpoint)?)
        .to_hex()
        .to_string();
    let checkpoint_bytes = serde_json::to_vec(&checkpoint)?;
    let checkpoint_root = state.network_data_root.join("p5-checkpoints");
    fs::create_dir_all(&checkpoint_root)?;
    let checkpoint_path = checkpoint_root.join(format!(
        "{}-{:020}.json",
        expected_peer, acknowledged_sequence
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(&checkpoint_path)?;
    output.write_all(&checkpoint_bytes)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    sync_directory(&checkpoint_root)?;
    state
        .checkpoints
        .insert(expected_peer.to_owned(), checkpoint.clone());
    Ok(serde_json::json!({
        "accepted": true,
        "checkpoint": checkpoint,
        "command": "record-checkpoint",
    }))
}

#[cfg(unix)]
fn prepare_fault_target(
    command: &AgentCommandFrame,
    state: &AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let expected_peer = command_parameter_string(command, "expected_peer")?;
    let operation_id = canonical_digest_parameter(command, "operation_id")?;
    let fault = command_parameter_string(command, "fault")?;
    let session = state
        .sessions
        .get(expected_peer)
        .ok_or("fault target peer has no authenticated routed session")?;
    let connected_socket = session.carrier().connected_socket().to_string();
    let selected_relay = match session.carrier() {
        onebrain_node::vnext_connection_planner::VerifiedCarrierIdentity::Relay {
            relay_node_id,
            ..
        } => Some(hex(relay_node_id.as_bytes())),
        _ => None,
    };
    let target = serde_json::json!({
        "expected_peer": expected_peer,
        "fault": fault,
        "operation_id": operation_id,
        "peer_endpoints": [connected_socket],
        "route_receipt_blake3": hex(&session.route_receipt_digest()),
        "selected_relay": selected_relay,
        "session_id": hex(&session.authenticated().session_id),
        "transport_binding_blake3": hex(&session.transport_binding_digest()),
    });
    Ok(serde_json::json!({
        "accepted": true,
        "command": "prepare-fault-target",
        "target": target,
        "target_blake3": blake3::hash(&serde_json::to_vec(&target)?).to_hex().to_string(),
    }))
}

#[cfg(unix)]
fn measure_fault_boundary(
    config: &AgentSessionConfig,
    command: &AgentCommandFrame,
    state: &AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let parameters = command
        .parameters
        .as_object()
        .ok_or("measurement parameters must be a closed object")?;
    if parameters
        .keys()
        .any(|key| !matches!(key.as_str(), "admin_response" | "fault" | "phase"))
    {
        return Err("unknown measurement parameter".into());
    }
    let fault = command_parameter_string(command, "fault")?;
    let phase = command_parameter_string(command, "phase")?;
    let response = parameters
        .get("admin_response")
        .and_then(serde_json::Value::as_object)
        .ok_or("measurement lacks the canonical admin response")?;
    if response
        .keys()
        .any(|key| !matches!(key.as_str(), "receipt" | "signature" | "signer_public_key"))
        || response.len() != 3
    {
        return Err("admin response schema is not closed".into());
    }
    let receipt = response
        .get("receipt")
        .and_then(serde_json::Value::as_object)
        .ok_or("admin response lacks its receipt")?;
    let expected_action = match phase {
        "before" => "observe",
        "during" => "apply",
        "after" => "clear",
        _ => return Err("fault measurement phase is invalid".into()),
    };
    if receipt.get("host_id").and_then(serde_json::Value::as_str) != Some(&config.host_id)
        || receipt
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            != Some(&config.session_id)
        || receipt.get("fault").and_then(serde_json::Value::as_str) != Some(fault)
        || receipt.get("action").and_then(serde_json::Value::as_str) != Some(expected_action)
    {
        return Err("admin response is not bound to the measured host/session/fault/phase".into());
    }
    let signer_public_key = response
        .get("signer_public_key")
        .and_then(serde_json::Value::as_str)
        .ok_or("admin response lacks signer identity")?;
    if signer_public_key != config.receipt_signer_public_key {
        return Err("admin response signer substitution".into());
    }
    let signature = decode_hex::<64>(
        response
            .get("signature")
            .and_then(serde_json::Value::as_str)
            .ok_or("admin response lacks signature")?,
    )?;
    let receipt_bytes = serde_json::to_vec(receipt)?;
    let mut preimage = ADMIN_RECEIPT_DOMAIN.to_vec();
    preimage.extend_from_slice(&receipt_bytes);
    VerifyingKey::from_bytes(&decode_hex::<32>(signer_public_key)?)?
        .verify(&preimage, &Signature::from_bytes(&signature))?;

    let roots = agent_root_set(state)?;
    Ok(serde_json::json!({
        "accepted": true,
        "admin_receipt_blake3": blake3::hash(&receipt_bytes).to_hex().to_string(),
        "command": "measure-fault-boundary",
        "fault": fault,
        "phase": phase,
        "roots": roots,
    }))
}

#[cfg(unix)]
fn agent_root_set(
    state: &AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let network = state
        .network
        .as_ref()
        .ok_or("reachability runtime is not started")?;
    let canonical_root = blake3::hash(&network.status().principal);
    let mut journal = blake3::Hasher::new();
    journal.update(b"onebrain/p5/agent-route-journal-root/v2\0");
    let mut outbox = blake3::Hasher::new();
    outbox.update(b"onebrain/p5/agent-checkpoint-root/v2\0");
    for (peer, session) in &state.sessions {
        journal.update(&(peer.len() as u64).to_be_bytes());
        journal.update(peer.as_bytes());
        journal.update(&session.route_receipt_digest());
        journal.update(&session.transport_binding_digest());
    }
    for (peer, checkpoint) in &state.checkpoints {
        outbox.update(&(peer.len() as u64).to_be_bytes());
        outbox.update(peer.as_bytes());
        outbox.update(&decode_hex::<32>(&checkpoint.checkpoint_blake3)?);
    }
    let status = fs::read("/proc/self/status")?;
    let rollout = serde_json::to_vec(&rollout_json(&state.rollout)?)?;
    let mut operational = blake3::Hasher::new();
    operational.update(b"onebrain/p5/agent-operational-root/v2\0");
    operational.update(&status);
    operational.update(&rollout);
    Ok(serde_json::json!({
        "canonical_root": canonical_root.to_hex().to_string(),
        "journal_root": journal.finalize().to_hex().to_string(),
        "operational_root": operational.finalize().to_hex().to_string(),
        "outbox_root": outbox.finalize().to_hex().to_string(),
    }))
}

#[cfg(unix)]
fn canonical_digest_parameter(
    command: &AgentCommandFrame,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let value = command_parameter_string(command, name)?;
    decode_hex::<32>(value)?;
    Ok(value.to_owned())
}

#[cfg(unix)]
fn sync_directory(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn arm_direct_inbound(
    command: &AgentCommandFrame,
    state: &mut AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let expected_name = command_parameter_string(command, "expected_peer")?;
    let expected_peer = NodeId::from_bytes(decode_hex::<32>(expected_name)?);
    let peer_public = decode_hex::<32>(command_parameter_string(command, "peer_public_key")?)?;
    let peer_identity = KnownPeerIdentity::from_public_key(peer_public);
    if peer_identity.node_id != expected_peer {
        return Err("direct arm peer/public-key mismatch".into());
    }
    let advertisement_bytes =
        decode_hex_vec(command_parameter_string(command, "peer_advertisement_hex")?)?;
    let advertisement = admit_peer_advertisement(state, &advertisement_bytes, &peer_identity)?;
    let observation_bytes = decode_hex_vec(command_parameter_string(
        command,
        "peer_reflexive_observation_hex",
    )?)?;
    let ConnectivitySignalingV1::ReflexiveObservation(observation) =
        decode_connectivity_signaling(&observation_bytes)?
    else {
        return Err("direct arm requires a reflexive observation".into());
    };
    if observation.target_node_id != expected_peer {
        return Err("direct arm observation target mismatch".into());
    }
    let reservation = advertisement
        .reservations()
        .iter()
        .find(|value| {
            value.canonical().reservation_id == observation.reservation_id
                && value.canonical().relay_node_id == observation.relay_node_id
        })
        .ok_or("direct arm observation lacks its admitted reservation")?;
    let descriptor = state.runtime.block_on(async {
        state
            .discovery
            .read()
            .await
            .verified_relays()
            .find(|value| value.canonical().relay_node_id == observation.relay_node_id)
            .cloned()
    });
    let descriptor = descriptor.ok_or("direct arm observation relay is not admitted")?;
    let relay_identity = KnownPeerIdentity {
        node_id: descriptor.canonical().relay_node_id,
        public_key: descriptor.canonical().relay_public_key,
    };
    let validated = state
        .connectivity_validator
        .lock()
        .map_err(|_| "connectivity validator unavailable")?
        .validate_reflexive_observation(
            &observation_bytes,
            &relay_identity,
            expected_peer,
            reservation,
            unix_now()?,
        )
        .map_err(|error| format!("direct arm reflexive observation rejected: {error:?}"))?;
    let endpoint = &validated.canonical().observed_endpoint;
    let ip = match endpoint.host {
        HostAddressV1::Ipv4(value) => std::net::IpAddr::V4(value.into()),
        HostAddressV1::Ipv6(value) => std::net::IpAddr::V6(value.into()),
        HostAddressV1::Dns(_) => return Err("reflexive observation cannot contain DNS".into()),
    };
    let socket = std::net::SocketAddr::new(ip, endpoint.port);
    let network = state
        .network
        .as_ref()
        .ok_or("reachability runtime is not started")?;
    if state.direct_inbound_arms.contains_key(expected_name) {
        return Err("direct inbound peer is already armed".into());
    }
    let armed = network.arm_expected_direct(expected_peer)?;
    network.prime_direct_path(socket)?;
    state
        .direct_inbound_arms
        .insert(expected_name.to_owned(), armed);
    Ok(serde_json::json!({
        "accepted": true,
        "command": "arm-direct-inbound",
        "expected_peer": expected_name,
        "observation_blake3": hex(&validated.digest()),
    }))
}

#[cfg(unix)]
fn connect_ring(
    command: &AgentCommandFrame,
    state: &mut AgentRuntimeState,
    replace_existing: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let outgoing_name = command_parameter_string(command, "outgoing_expected_peer")?;
    let outgoing_peer = NodeId::from_bytes(decode_hex::<32>(outgoing_name)?);
    let outgoing_public = decode_hex::<32>(command_parameter_string(
        command,
        "outgoing_peer_public_key",
    )?)?;
    let outgoing_identity = KnownPeerIdentity::from_public_key(outgoing_public);
    if outgoing_identity.node_id != outgoing_peer {
        return Err("outgoing peer/public-key mismatch".into());
    }
    let outgoing_bytes = decode_hex_vec(command_parameter_string(
        command,
        "outgoing_advertisement_hex",
    )?)?;
    let outgoing = admit_peer_advertisement(state, &outgoing_bytes, &outgoing_identity)?;

    let incoming_name = command_parameter_string(command, "incoming_expected_peer")?;
    let incoming_peer = NodeId::from_bytes(decode_hex::<32>(incoming_name)?);
    let incoming_public = decode_hex::<32>(command_parameter_string(
        command,
        "incoming_peer_public_key",
    )?)?;
    let incoming_identity = KnownPeerIdentity::from_public_key(incoming_public);
    if incoming_identity.node_id != incoming_peer || incoming_peer == outgoing_peer {
        return Err("incoming peer/public-key/ring identity mismatch".into());
    }
    let incoming_bytes = decode_hex_vec(command_parameter_string(
        command,
        "incoming_advertisement_hex",
    )?)?;
    let incoming = admit_peer_advertisement(state, &incoming_bytes, &incoming_identity)?;

    if replace_existing {
        if let Some(previous) = state.sessions.remove(outgoing_name) {
            previous.close();
        }
        if let Some(previous) = state.sessions.remove(incoming_name) {
            previous.close();
        }
    } else if state.sessions.contains_key(outgoing_name)
        || state.sessions.contains_key(incoming_name)
    {
        return Err("ring peer already has a routed session".into());
    }
    let direct_inbound = state.direct_inbound_arms.remove(incoming_name);
    let network = state
        .network
        .as_ref()
        .ok_or("reachability runtime is not started")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let (outbound, inbound) = if let Some(armed) = direct_inbound {
        state.runtime.block_on(async {
            let (outbound, inbound) = tokio::join!(
                network.connect_expected_advertisement(&state.selector, outgoing_peer, &outgoing,),
                network.accept_armed_direct(armed),
            );
            (
                outbound,
                inbound
                    .map_err(|error| format!("direct inbound OBP authentication failed: {error}")),
            )
        })
    } else {
        state.runtime.block_on(async {
            tokio::join!(
                network.connect_expected_advertisement(&state.selector, outgoing_peer, &outgoing,),
                accept_expected_relay(
                    network,
                    state.executor.as_ref(),
                    &state.discovery,
                    &state.reservations,
                    incoming_peer,
                    incoming_identity,
                    &incoming,
                    deadline,
                )
            )
        })
    };
    let (outbound, inbound) = match (outbound, inbound) {
        (Ok(outbound), Ok(inbound)) => (outbound, inbound),
        (Ok(outbound), Err(error)) => {
            outbound.close();
            return Err(format!("ring inbound failed: {error}").into());
        }
        (Err(error), Ok(inbound)) => {
            inbound.close();
            return Err(format!("ring outbound failed: {error}").into());
        }
        (Err(outbound), Err(inbound)) => {
            return Err(format!("ring failed: outbound={outbound}; inbound={inbound}").into());
        }
    };
    let outgoing_selected_relay = match outbound.carrier() {
        onebrain_node::vnext_connection_planner::VerifiedCarrierIdentity::Relay {
            relay_node_id,
            ..
        } => Some(hex(relay_node_id.as_bytes())),
        _ => None,
    };
    let incoming_selected_relay = match inbound.carrier() {
        onebrain_node::vnext_connection_planner::VerifiedCarrierIdentity::Relay {
            relay_node_id,
            ..
        } => Some(hex(relay_node_id.as_bytes())),
        _ => None,
    };
    let outgoing_result = serde_json::json!({
        "expected_peer": outgoing_name,
        "path_kind": format!("{:?}", outbound.carrier().path_kind()),
        "route_receipt_blake3": hex(&outbound.route_receipt_digest()),
        "selected_relay": outgoing_selected_relay,
        "session_id": hex(&outbound.authenticated().session_id),
        "transport_binding_blake3": hex(&outbound.transport_binding_digest()),
    });
    let incoming_result = serde_json::json!({
        "expected_peer": incoming_name,
        "path_kind": format!("{:?}", inbound.carrier().path_kind()),
        "route_receipt_blake3": hex(&inbound.route_receipt_digest()),
        "selected_relay": incoming_selected_relay,
        "session_id": hex(&inbound.authenticated().session_id),
        "transport_binding_blake3": hex(&inbound.transport_binding_digest()),
    });
    state.sessions.insert(outgoing_name.to_owned(), outbound);
    state.sessions.insert(incoming_name.to_owned(), inbound);
    Ok(serde_json::json!({
        "accepted": true,
        "command": command.command,
        "incoming": incoming_result,
        "outgoing": outgoing_result,
    }))
}

#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
async fn accept_expected_relay(
    network: &VNextNetworkRuntime,
    executor: &ConnectionPlannerExecutor,
    discovery: &Arc<tokio::sync::RwLock<RelayDiscovery>>,
    reservations: &Arc<RelayReservationManager>,
    expected_peer: NodeId,
    expected_identity: KnownPeerIdentity,
    advertisement: &ValidatedReachabilityAdvertisement,
    deadline: std::time::Instant,
) -> Result<RoutedVNextSession, String> {
    for initiator_reservation in advertisement.reservations() {
        if std::time::Instant::now() >= deadline {
            return Err("relay inbound deadline elapsed".into());
        }
        let relay_id = initiator_reservation.canonical().relay_node_id;
        let descriptor = {
            discovery
                .read()
                .await
                .verified_relays()
                .find(|value| value.canonical().relay_node_id == relay_id)
                .cloned()
        };
        let Some(descriptor) = descriptor else {
            continue;
        };
        let Some((target_reservation, outer)) = reservations.active_for(relay_id).await else {
            continue;
        };
        if initiator_reservation.canonical().target_node_id != expected_peer
            || target_reservation.canonical().target_node_id
                != ku_net::vnext_session::principal_node_id(&network.status().principal)
        {
            return Err("relay inbound reservation identity mismatch".into());
        }
        let public = outer.public_endpoint();
        let candidate = RelayCandidateV1 {
            relay_node_id: relay_id,
            reservation_id: initiator_reservation.canonical().reservation_id,
            transport: outer.transport(),
            endpoint: ReachabilityEndpointV1 {
                host: public.host.clone(),
                port: public.port,
            },
            priority: 1,
            expires_at: initiator_reservation
                .canonical()
                .expires_at
                .min(target_reservation.canonical().expires_at),
        };
        let carrier = executor
            .accept_relay_inbound(
                descriptor,
                initiator_reservation.clone(),
                target_reservation,
                expected_identity.clone(),
                candidate,
                outer,
                deadline,
            )
            .await
            .map_err(|error| format!("relay association/inner carrier rejected: {error:?}"))?;
        return network
            .accept_expected_selected(expected_peer, carrier)
            .await
            .map_err(|error| format!("relay inbound OBP authentication failed: {error}"));
    }
    Err("no common live relay reservation for inbound peer".into())
}

#[cfg(unix)]
fn admit_peer_advertisement(
    state: &mut AgentRuntimeState,
    advertisement_bytes: &[u8],
    identity: &KnownPeerIdentity,
) -> Result<ValidatedReachabilityAdvertisement, Box<dyn std::error::Error>> {
    let ReachabilityObjectV1::Advertisement(canonical) =
        decode_reachability_object(advertisement_bytes)?
    else {
        return Err("expected canonical reachability advertisement".into());
    };
    if canonical.target_node_id != identity.node_id {
        return Err("advertisement target identity mismatch".into());
    }
    let descriptors = state.runtime.block_on(async {
        state
            .discovery
            .read()
            .await
            .verified_relays()
            .cloned()
            .collect::<Vec<_>>()
    });
    let now = unix_now()?;
    let mut reservations = Vec::with_capacity(canonical.relay_reservations.len());
    {
        let mut admission = state
            .advertisement_admission
            .lock()
            .map_err(|_| "advertisement admission state unavailable")?;
        for reservation in &canonical.relay_reservations {
            let relay = descriptors
                .iter()
                .find(|value| value.canonical().relay_node_id == reservation.relay_node_id)
                .ok_or("advertisement references a relay that was not PoP-admitted")?;
            let relay_identity = KnownPeerIdentity {
                node_id: relay.canonical().relay_node_id,
                public_key: relay.canonical().relay_public_key,
            };
            let bytes = encode_reachability_object(&ReachabilityObjectV1::RelayReservation(
                reservation.clone(),
            ))?;
            reservations.push(admission.admit_reservation(
                &bytes,
                identity,
                &relay_identity,
                now,
            )?);
        }
    }
    let prepared = state
        .runtime
        .block_on(state.advertisement_preparer.prepare_advertisement(
            advertisement_bytes,
            identity,
            &reservations,
            now,
            std::time::Instant::now() + Duration::from_secs(5),
        ))?;
    state
        .advertisement_admission
        .lock()
        .map_err(|_| "advertisement admission state unavailable")?
        .register_prepared_advertisement(prepared, identity, &reservations, now)
        .map_err(Into::into)
}

#[cfg(unix)]
fn ensure_reservations(
    command: &AgentCommandFrame,
    state: &mut AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let records = command_parameter_string_array(command, "relay_descriptors")?
        .iter()
        .map(|value| decode_hex_vec(value))
        .collect::<Result<Vec<_>, _>>()?;
    if records.len() < 2 || records.len() > 3 {
        return Err("production reservation set must contain two or three relays".into());
    }
    let now = unix_now()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let delta = state
        .runtime
        .block_on(admit_relay_records(
            &state.discovery,
            state.discovery_preparer.as_ref(),
            state.possession_client.as_ref(),
            RelayDiscoverySource::manual_relay(),
            &records,
            now,
            deadline,
        ))
        .map_err(|error| format!("relay admission failed: {error:?}"))?;
    let descriptors = state.runtime.block_on(async {
        state
            .discovery
            .read()
            .await
            .verified_relays()
            .cloned()
            .collect::<Vec<_>>()
    });
    if descriptors.len() < 2 || descriptors.len() > 3 {
        return Err("PoP-admitted relay count is outside the production bound".into());
    }
    let local_node_id = ku_net::vnext_session::principal_node_id(&state.identity.public_key());
    let mut grants = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        let relay_id = descriptor.canonical().relay_node_id;
        let relay_key = hex(relay_id.as_bytes());
        let cursor = match state.relay_sequences.entry(relay_key.clone()) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(DurableSequenceCursor::open(
                    relay_reservation_cursor(&relay_key),
                    state.cursor_binding,
                )?)
            }
        };
        let sequence = next_durable_sequence(cursor)?;
        let issued_at = unix_now()?;
        let expires_at = descriptor
            .canonical()
            .expires_at
            .min(issued_at.saturating_add(300));
        if expires_at <= issued_at {
            return Err("relay descriptor expires before reservation can be created".into());
        }
        let mut reservation_id = [0u8; 32];
        OsRng.fill_bytes(&mut reservation_id);
        let unsigned_reservation = RelayReservationV1 {
            format: 1,
            relay_node_id: relay_id,
            target_node_id: local_node_id,
            reservation_id,
            transport_scope: descriptor.canonical().supported_transports.clone(),
            issued_at,
            expires_at,
            target_signature: [0; 64],
            relay_signature: [0; 64],
        };
        let mut request = RelayReserveRequestV1 {
            format: 1,
            relay_node_id: relay_id,
            target_node_id: local_node_id,
            reservation_id,
            transport_scope: unsigned_reservation.transport_scope.clone(),
            sequence,
            issued_at,
            expires_at,
            target_reservation_signature: sign_reachability(
                state.identity.as_ref(),
                &ReachabilityObjectV1::RelayReservation(unsigned_reservation),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )?,
            target_request_signature: [0; 64],
        };
        let (domain, unsigned) = relay_control_signing_parts(
            &RelayControlV1::Reserve(request.clone()),
            RelayControlSignatureRoleV1::ReserveRequestTarget,
        )?;
        request.target_request_signature = state
            .identity
            .sign_reachability_message(domain, &unsigned)?;
        let grant = state
            .runtime
            .block_on(
                state
                    .reservations
                    .ensure_route_reservation(&descriptor, request, deadline),
            )
            .map_err(|error| format!("relay reservation failed: {error:?}"))?;
        grants.push(hex(grant.digest()));
    }
    if state.runtime.block_on(state.reservations.active_count()) < 2 {
        return Err("fewer than two live relay reservations were established".into());
    }
    Ok(serde_json::json!({
        "accepted": true,
        "admitted": delta.admitted.iter().map(|value| hex(value.as_bytes())).collect::<Vec<_>>(),
        "command": "ensure-reservations",
        "grant_digests": grants,
        "refreshed": delta.refreshed.iter().map(|value| hex(value.as_bytes())).collect::<Vec<_>>(),
    }))
}

#[cfg(unix)]
fn publish_advertisement(
    state: &mut AgentRuntimeState,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let reservations = state
        .runtime
        .block_on(state.reservations.active_reservations());
    if reservations.len() < 2 || reservations.len() > 3 {
        return Err("advertisement requires two or three live relay reservations".into());
    }
    let issued_at = unix_now()?;
    let expires_at = reservations
        .iter()
        .map(|value| value.canonical().expires_at)
        .min()
        .ok_or("missing reservation expiry")?
        .min(issued_at.saturating_add(300));
    if expires_at <= issued_at {
        return Err("reservation set expired before advertisement publication".into());
    }
    let sequence = next_durable_sequence(&state.advertisement_sequence)?;
    let public_key = state.identity.public_key();
    let node_id = ku_net::vnext_session::principal_node_id(&public_key);
    let descriptor_by_relay = state.runtime.block_on(async {
        state
            .discovery
            .read()
            .await
            .verified_relays()
            .map(|descriptor| (descriptor.canonical().relay_node_id, descriptor.clone()))
            .collect::<BTreeMap<_, _>>()
    });
    let observations = state
        .runtime
        .block_on(state.reservations.reflexive_observations(1))
        .map_err(|error| format!("relay reflexive observation failed: {error:?}"))?;
    let mut validated_observations = Vec::new();
    let mut candidates = Vec::new();
    for (reservation, bytes) in observations {
        let Some(descriptor) = descriptor_by_relay.get(&reservation.canonical().relay_node_id)
        else {
            return Err("reflexive observation relay was not admitted".into());
        };
        let relay_identity = KnownPeerIdentity {
            node_id: descriptor.canonical().relay_node_id,
            public_key: descriptor.canonical().relay_public_key,
        };
        let validated = match state
            .connectivity_validator
            .lock()
            .map_err(|_| "connectivity validator unavailable")?
            .validate_reflexive_observation(
                &bytes,
                &relay_identity,
                node_id,
                &reservation,
                issued_at,
            ) {
            Ok(value) => value,
            // A co-resident relay legitimately observes the private veth
            // socket. It is never publishable and does not invalidate a
            // separate public observation from the other physical relay.
            Err(_) => continue,
        };
        let digest = validated.digest();
        validated_observations.push(hex(&bytes));
        if candidates.is_empty() {
            let mut foundation = [0u8; 16];
            foundation.copy_from_slice(&digest[..16]);
            candidates.push(PublicCandidateV1 {
                kind: PublicCandidateKindV1::ServerReflexive,
                endpoint: validated.canonical().observed_endpoint.clone(),
                priority: 1,
                foundation,
            });
        }
    }
    let mut advertisement = ReachabilityAdvertisementV1 {
        format: 1,
        target_node_id: node_id,
        relay_reservations: reservations
            .iter()
            .map(|value| value.canonical().clone())
            .collect(),
        optional_public_candidates: candidates,
        capability_ceiling: *blake3::hash(b"onebrain/reachability/mixed-direct-relay/v1")
            .as_bytes(),
        sequence,
        issued_at,
        expires_at,
        target_signature: [0; 64],
    };
    advertisement.target_signature = sign_reachability(
        state.identity.as_ref(),
        &ReachabilityObjectV1::Advertisement(advertisement.clone()),
        ReachabilitySignatureRoleV1::AdvertisementTarget,
    )?;
    let bytes = encode_reachability_object(&ReachabilityObjectV1::Advertisement(advertisement))?;
    Ok(serde_json::json!({
        "accepted": true,
        "advertisement_blake3": blake3::hash(&bytes).to_hex().to_string(),
        "advertisement_hex": hex(&bytes),
        "command": "publish-advertisement",
        "peer_node_id": hex(node_id.as_bytes()),
        "peer_public_key": hex(&public_key),
        "reflexive_observations": validated_observations,
        "reservation_count": reservations.len(),
        "reservation_records": reservations.iter().map(|value| serde_json::json!({
            "expires_at": value.canonical().expires_at,
            "issued_at": value.canonical().issued_at,
            "relay_node_id": hex(value.canonical().relay_node_id.as_bytes()),
            "reservation_id": hex(&value.canonical().reservation_id),
        })).collect::<Vec<_>>(),
    }))
}

#[cfg(unix)]
fn sign_reachability(
    signer: &dyn ReachabilityIdentitySigner,
    object: &ReachabilityObjectV1,
    role: ReachabilitySignatureRoleV1,
) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    let (domain, unsigned) = reachability_signing_parts(object, role)?;
    Ok(signer.sign_reachability_message(domain, &unsigned)?)
}

#[cfg(unix)]
fn next_durable_sequence(
    cursor: &DurableSequenceCursor,
) -> Result<u64, Box<dyn std::error::Error>> {
    let sequence = cursor
        .highest()?
        .checked_add(1)
        .ok_or("reachability sequence exhausted")?;
    cursor.advance(sequence)?;
    Ok(sequence)
}

#[cfg(unix)]
fn command_parameter_string_array<'a>(
    command: &'a AgentCommandFrame,
    name: &str,
) -> Result<Vec<&'a str>, Box<dyn std::error::Error>> {
    let values = command
        .parameters
        .as_object()
        .and_then(|parameters| parameters.get(name))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("missing canonical command parameter array: {name}"))?;
    if values.is_empty() || values.len() > 3 {
        return Err(format!("command parameter array is outside bounds: {name}").into());
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("non-string command parameter: {name}").into())
        })
        .collect()
}

#[cfg(unix)]
fn command_parameter_string<'a>(
    command: &'a AgentCommandFrame,
    name: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    command
        .parameters
        .as_object()
        .and_then(|parameters| parameters.get(name))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing canonical command parameter: {name}").into())
}

#[cfg(unix)]
fn command_parameter_u64(
    command: &AgentCommandFrame,
    name: &str,
) -> Result<u64, Box<dyn std::error::Error>> {
    command
        .parameters
        .as_object()
        .and_then(|parameters| parameters.get(name))
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("missing canonical unsigned command parameter: {name}").into())
}

#[cfg(unix)]
fn decode_hex_vec(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 || value.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
        return Err("invalid lowercase hex payload".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for index in (0..value.len()).step_by(2) {
        output.push(u8::from_str_radix(&value[index..index + 2], 16)?);
    }
    if hex(&output) != value {
        return Err("noncanonical hex payload".into());
    }
    Ok(output)
}

#[cfg(unix)]
fn signed_child_receipt(
    config: &AgentSessionConfig,
    command: &AgentCommandFrame,
    frame: &[u8],
    result: &serde_json::Value,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let issued_at = unix_now()?;
    let unsigned = serde_json::json!({
        "command_blake3": blake3::hash(frame).to_hex().to_string(),
        "evidence_authority": config.evidence_authority,
        "format": 2,
        "host_id": config.host_id,
        "inventory_blake3": config.inventory_blake3,
        "issued_at": issued_at,
        "request_digest": config.request_digest,
        "result": result,
        "sequence": command.sequence,
        "session_id": config.session_id,
    });
    let unsigned_bytes = serde_json::to_vec(&unsigned)?;
    let (public, signature) = request_signature(1, command.sequence, &unsigned_bytes)?;
    let mut receipt = unsigned.as_object().cloned().ok_or("invalid receipt")?;
    receipt.insert(
        "signer_public_key".into(),
        serde_json::Value::String(hex(&public)),
    );
    receipt.insert(
        "signature".into(),
        serde_json::Value::String(hex(&signature)),
    );
    Ok(serde_json::to_vec(&receipt)?)
}

#[cfg(unix)]
fn request_signature(
    domain: u8,
    sequence: u64,
    message: &[u8],
) -> Result<([u8; 32], [u8; 64]), Box<dyn std::error::Error>> {
    let mut stream = UnixStream::connect(RECEIPT_SOCKET)?;
    stream.write_all(&[domain])?;
    stream.write_all(&sequence.to_be_bytes())?;
    stream.write_all(&(message.len() as u32).to_be_bytes())?;
    stream.write_all(message)?;
    stream.flush()?;
    let mut response = [0u8; 96];
    stream.read_exact(&mut response)?;
    Ok((response[..32].try_into()?, response[32..].try_into()?))
}

#[cfg(unix)]
fn unix_now() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(unix)]
fn rollout_json(
    rollout: &VNextRuntimeRollout,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let snapshot = rollout.snapshot()?;
    Ok(serde_json::Value::Array(
        snapshot
            .lanes
            .iter()
            .map(|lane| {
                serde_json::json!({
                    "enabled": lane.enabled,
                    "generation": lane.generation,
                    "lane": lane.lane.name(),
                    "requested": lane.requested,
                })
            })
            .collect(),
    ))
}

#[cfg(unix)]
fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], Box<dyn std::error::Error>> {
    if value.len() != N * 2 {
        return Err("invalid lowercase hex length".into());
    }
    let mut output = [0u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    if hex(&output) != value {
        return Err("noncanonical hex".into());
    }
    Ok(output)
}

#[cfg(not(unix))]
fn serve_fd3() -> Result<(), Box<dyn std::error::Error>> {
    Err("fd 3 listener requires Unix".into())
}

fn print_compiled_binding() -> Result<(), Box<dyn std::error::Error>> {
    let compiled = compiled_base_runtime_config();
    let tuple = &compiled.compatibility_policy.current;
    let commit = match tuple.base_commit {
        SourceCommitIdentity::Known(SourceCommitId::Sha1(value)) => hex(&value.0),
        _ => return Err("known SHA-1 candidate commit required".into()),
    };
    let toolchain = match tuple.toolchain {
        ToolchainIdentity::Known(value) => hex(&value.0),
        ToolchainIdentity::Unknown => return Err("known toolchain required".into()),
    };
    let value = serde_json::json!({
        "agent_binary_identity": hex(blake3::hash(env!("CARGO_PKG_NAME").as_bytes()).as_bytes()),
        "candidate_commit": commit,
        "candidate_tree": option_env!("ONEBRAIN_SOURCE_TREE").unwrap_or("bound-by-bundle-provenance"),
        "format": "onebrain/p5-compiled-binding/2",
        "profile_digest": option_env!("ONEBRAIN_P5_PROFILE_BLAKE3").unwrap_or("bound-by-session-config"),
        "toolchain_digest": toolchain,
        "vector_digest": option_env!("ONEBRAIN_P5_VECTOR_BLAKE3").unwrap_or("bound-by-session-config")
    });
    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_agent_state_stays_inside_the_agent_owned_root() {
        let paths = [
            PathBuf::from(COMMAND_CURSOR),
            identity_client_cursor("host-a"),
            advertisement_cursor("host-a"),
            runner_data_root("host-a"),
            relay_reservation_cursor(&"ab".repeat(32)),
        ];
        let owned_root = std::path::Path::new(AGENT_STATE_ROOT);
        for path in paths {
            assert!(
                path.starts_with(owned_root),
                "escaped agent state root: {path:?}"
            );
            assert!(!path.starts_with("/var/lib/onebrain/p5-v2/"));
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
