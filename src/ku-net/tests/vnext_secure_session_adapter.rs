#![cfg(feature = "outbound-first")]

use ed25519_dalek::SigningKey;
use ku_net::transport::{QuicTransport, TransportConfig};
use ku_net::vnext_connection_executor::ConnectionPlannerExecutor;
use ku_net::vnext_route_plan::{PlannerAction, RouteFailure};
use ku_net::vnext_secure_session_adapter::{
    accept_authenticated_direct, authenticate_expected_outbound,
};
use ku_net::vnext_session::principal_node_id;
use onebrain_protocol::{
    reconciliation_capability, reconciliation_profile, DirectCandidateKindV1, DirectCandidateV1,
    HostAddressV1, ReachabilityEndpointV1,
};

async fn connected_pair() -> (
    ku_net::transport::OBPConnection,
    ku_net::transport::OBPConnection,
    std::net::SocketAddr,
) {
    let server = QuicTransport::bind(TransportConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..TransportConfig::default()
    })
    .await
    .unwrap();
    let client = QuicTransport::bind(TransportConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..TransportConfig::default()
    })
    .await
    .unwrap();
    let server_addr = server.local_addr().unwrap();
    let (accepted, connected) = tokio::join!(server.accept(), client.connect(server_addr));
    (accepted.unwrap(), connected.unwrap(), server_addr)
}

fn direct_candidate(server_addr: std::net::SocketAddr) -> DirectCandidateV1 {
    DirectCandidateV1 {
        endpoint: ReachabilityEndpointV1 {
            host: match server_addr.ip() {
                std::net::IpAddr::V4(value) => HostAddressV1::Ipv4(value.octets()),
                std::net::IpAddr::V6(value) => HostAddressV1::Ipv6(value.octets()),
            },
            port: server_addr.port(),
        },
        kind: DirectCandidateKindV1::Host,
        priority: 100,
        network_epoch: 1,
        expires_at: 500,
    }
}

#[tokio::test]
async fn direct_selected_carrier_preserves_obp_peer_authentication() {
    let (server_connection, client_connection, server_addr) = connected_pair().await;
    let outbound = ConnectionPlannerExecutor::seal_connected_direct(
        PlannerAction::CheckDirect(direct_candidate(server_addr)),
        client_connection,
        Vec::new(),
    )
    .unwrap();
    let inbound =
        ConnectionPlannerExecutor::seal_unbound_direct_inbound(server_connection, Vec::new())
            .unwrap();
    let initiator_key = SigningKey::from_bytes(&[0x11; 32]);
    let responder_key = SigningKey::from_bytes(&[0x22; 32]);
    let profiles = [reconciliation_profile()];
    let capabilities = [reconciliation_capability()];
    let expected = principal_node_id(responder_key.verifying_key().as_bytes());
    let (accepted, initiated) = tokio::join!(
        accept_authenticated_direct(
            inbound,
            &responder_key,
            [0xBB; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        ),
        authenticate_expected_outbound(
            outbound,
            expected,
            &initiator_key,
            [0xAA; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        )
    );
    let accepted = accepted.unwrap();
    let initiated = initiated.unwrap();
    assert_eq!(accepted.session(), initiated.session());
    assert_eq!(
        initiated.selection().path_kind(),
        onebrain_protocol::RoutePathKindV1::Direct
    );
    assert_eq!(initiated.authenticated_peer(), expected);
}

#[tokio::test]
async fn wrong_expected_peer_fails_after_the_signed_handshake() {
    let (server_connection, client_connection, server_addr) = connected_pair().await;
    let outbound = ConnectionPlannerExecutor::seal_connected_direct(
        PlannerAction::CheckDirect(direct_candidate(server_addr)),
        client_connection,
        Vec::new(),
    )
    .unwrap();
    let inbound =
        ConnectionPlannerExecutor::seal_unbound_direct_inbound(server_connection, Vec::new())
            .unwrap();
    let initiator_key = SigningKey::from_bytes(&[0x31; 32]);
    let responder_key = SigningKey::from_bytes(&[0x32; 32]);
    let profiles = [reconciliation_profile()];
    let capabilities = [reconciliation_capability()];
    let (_accepted, initiated) = tokio::join!(
        accept_authenticated_direct(
            inbound,
            &responder_key,
            [0xCB; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        ),
        authenticate_expected_outbound(
            outbound,
            principal_node_id(&[0x99; 32]),
            &initiator_key,
            [0xCA; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        )
    );
    assert_eq!(initiated.unwrap_err(), RouteFailure::PeerIdentityMismatch);
}

#[tokio::test]
async fn direct_action_cannot_be_relabelled_to_another_measured_socket() {
    let (_server_connection, client_connection, server_addr) = connected_pair().await;
    let mut candidate = direct_candidate(server_addr);
    candidate.endpoint.port = candidate.endpoint.port.saturating_add(1);
    assert_eq!(
        ConnectionPlannerExecutor::seal_connected_direct(
            PlannerAction::CheckDirect(candidate),
            client_connection,
            Vec::new(),
        )
        .unwrap_err(),
        RouteFailure::PeerIdentityMismatch
    );
}
