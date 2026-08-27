#![cfg(feature = "vnext-outbound-first")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_reachability_crypto::{
    possession_challenge_digest, possession_proof_signing_bytes, InMemoryReachabilityReplayStore,
    PublicEndpointResolver, ReachabilityAdmission, ReachabilityAdmissionPreparer,
    ReachabilityLockFreePreparation, ReachabilityRecordAdmission, RelayAdmissionError,
    ValidatedRelayDescriptor,
};
use ku_net::vnext_relay_tunnel::{
    connect_authenticated_outer, AlternateRelayProbeObservation, ValidatedRelayDialRoute,
    ValidatedRelayDialSet,
};
use onebrain_protocol::{
    encode_reachability_object, reachability_signing_bytes, HostAddressV1, ProtocolVersionV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1, RelayEndpointV1,
    RelayPossessionProofV1, RelayTransportV1,
};
use onebrain_relay::{
    principal_node_id, relay_identity_certificate, DurableRelayState, RelayDataPlaneError,
    RelayProductionService, Tcp443RelayListener,
};

struct PublicTestResolver;

impl PublicEndpointResolver for PublicTestResolver {
    fn resolve(
        &self,
        host: &HostAddressV1,
        _deadline: Instant,
    ) -> Result<Vec<IpAddr>, RelayAdmissionError> {
        match host {
            HostAddressV1::Ipv4(value) => Ok(vec![IpAddr::V4(Ipv4Addr::from(*value))]),
            _ => Err(RelayAdmissionError::DnsResolutionFailed),
        }
    }
}

fn unsigned_descriptor(relay_key: &SigningKey, now: u64) -> RelayDescriptorV1 {
    let public = *relay_key.verifying_key().as_bytes();
    RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&public),
        relay_public_key: public,
        endpoints: Vec::new(),
        supported_transports: vec![RelayTransportV1::TlsTcp443],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [90; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: now,
        expires_at: now + 300,
        relay_signature: [0; 64],
    }
}

async fn start_relay(
    relay_key: SigningKey,
    now: u64,
) -> (
    Vec<u8>,
    SocketAddr,
    SigningKey,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let provisional = unsigned_descriptor(&relay_key, now);
    let identity = relay_identity_certificate(&relay_key, &provisional).unwrap();
    let listener = Arc::new(
        Tcp443RelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity)
            .await
            .unwrap(),
    );
    let address = listener.local_addr().unwrap();
    let mut descriptor = provisional;
    descriptor.endpoints = vec![RelayEndpointV1 {
        transport: RelayTransportV1::TlsTcp443,
        host: HostAddressV1::Ipv4([8, 8, 8, 8]),
        port: address.port(),
    }];
    descriptor.relay_signature = relay_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    let encoded =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let durable =
        Arc::new(DurableRelayState::initialize(&directory.path().join("relay.redb")).unwrap());
    let service = Arc::new(RelayProductionService::new(relay_key.clone(), 16, 3, durable).unwrap());
    let server = tokio::spawn(async move {
        let mut connections = Vec::new();
        for _ in 0..3 {
            let listener = Arc::clone(&listener);
            let service = Arc::clone(&service);
            connections.push(tokio::spawn(async move {
                match listener.serve_production_once(service).await {
                    Ok(()) | Err(RelayDataPlaneError::Closed) => {}
                    Err(error) => panic!("live relay connection failed: {error:?}"),
                }
            }));
        }
        for connection in connections {
            connection.await.unwrap();
        }
    });
    (encoded, address, relay_key, server, directory)
}

async fn validated_route(
    record: &[u8],
    relay_key: &SigningKey,
    address: SocketAddr,
    now: u64,
    probe_tag: u8,
) -> ValidatedRelayDialSet {
    let preparer = ReachabilityAdmissionPreparer::new(Arc::new(PublicTestResolver), 1).unwrap();
    let prepared = preparer
        .prepare_descriptor(record, now, Instant::now() + Duration::from_secs(2))
        .await
        .unwrap();
    let mut admission =
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    let pending = admission
        .register_prepared_descriptor(prepared, [probe_tag; 32], now)
        .unwrap();
    let proofs = pending
        .challenges()
        .iter()
        .map(|challenge| RelayPossessionProofV1 {
            challenge_digest: possession_challenge_digest(challenge),
            connection_binding_digest: [probe_tag.wrapping_add(1); 32],
            signature: relay_key
                .sign(&possession_proof_signing_bytes(
                    challenge,
                    [probe_tag.wrapping_add(1); 32],
                ))
                .to_bytes(),
        })
        .collect::<Vec<_>>();
    let descriptor = admission
        .complete_descriptor_admission(pending, &proofs, now)
        .unwrap();
    alternate_route(descriptor, address, relay_key, now, probe_tag)
}

fn alternate_route(
    descriptor: ValidatedRelayDescriptor,
    address: SocketAddr,
    relay_key: &SigningKey,
    now: u64,
    probe_tag: u8,
) -> ValidatedRelayDialSet {
    let route = ValidatedRelayDialRoute::alternate_from_verified_probe(
        &descriptor,
        AlternateRelayProbeObservation::new(
            0,
            address,
            RelayTransportV1::TlsTcp443,
            *relay_key.verifying_key().as_bytes(),
            [probe_tag; 32],
            now,
            now + 30,
        )
        .unwrap(),
    )
    .unwrap();
    ValidatedRelayDialSet::from_admitted_descriptor(route, None).unwrap()
}

async fn connect_host_to_both_relays(
    host_seed: u8,
    relay_b: ValidatedRelayDialSet,
    relay_c: ValidatedRelayDialSet,
    now: u64,
) {
    let signer = SigningKey::from_bytes(&[host_seed; 32]);
    let (outer_b, outer_c) = tokio::join!(
        connect_authenticated_outer(
            &relay_b,
            &signer,
            now,
            Instant::now() + Duration::from_secs(10),
        ),
        connect_authenticated_outer(
            &relay_c,
            &signer,
            now,
            Instant::now() + Duration::from_secs(10),
        ),
    );
    let outer_b = outer_b.unwrap();
    let outer_c = outer_c.unwrap();
    assert_ne!(outer_b.relay_node_id(), outer_c.relay_node_id());
    outer_b.close();
    outer_c.close();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
async fn three_real_hosts_concurrently_authenticate_to_both_live_relays() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (record_b, address_b, key_b, server_b, _directory_b) =
        start_relay(SigningKey::from_bytes(&[101; 32]), now).await;
    let (record_c, address_c, key_c, server_c, _directory_c) =
        start_relay(SigningKey::from_bytes(&[102; 32]), now).await;
    let route_b = validated_route(&record_b, &key_b, address_b, now, 121).await;
    let route_c = validated_route(&record_c, &key_c, address_c, now, 122).await;

    tokio::join!(
        connect_host_to_both_relays(111, route_b.clone(), route_c.clone(), now),
        connect_host_to_both_relays(112, route_b.clone(), route_c.clone(), now),
        connect_host_to_both_relays(113, route_b, route_c, now),
    );
    tokio::time::timeout(Duration::from_secs(5), server_b)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), server_c)
        .await
        .unwrap()
        .unwrap();
}
