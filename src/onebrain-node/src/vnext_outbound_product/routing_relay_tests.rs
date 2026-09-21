use super::*;
use crate::vnext_connection_planner::*;
use crate::vnext_reachability_manager::*;
use ed25519_dalek::Signer;
use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
use ku_net::vnext_connection_executor::*;
use ku_net::vnext_reachability_crypto::*;
use ku_net::vnext_relay_discovery::*;
use ku_net::vnext_relay_tunnel::*;
use onebrain_protocol::*;
use onebrain_relay::{
    principal_node_id, relay_identity_certificate, DurableRelayState, RelayProductionService,
    Tcp443RelayListener,
};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Weak;
use std::time::Instant;
use tokio::sync::RwLock;
struct PublicTestResolver;

struct Proofs(Vec<SigningKey>);
impl RelayPossessionClient for Proofs {
    fn prove<'a>(
        &'a self,
        staged: &'a StagedRelayAdmission,
        _: Instant,
    ) -> ReachabilityFuture<'a, Result<Vec<RelayPossessionProofV1>, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            Ok(staged
                .challenges()
                .iter()
                .map(|challenge| {
                    let key = self
                        .0
                        .iter()
                        .find(|k| {
                            principal_node_id(k.verifying_key().as_bytes())
                                == challenge.relay_node_id
                        })
                        .unwrap();
                    RelayPossessionProofV1 {
                        challenge_digest: possession_challenge_digest(challenge),
                        connection_binding_digest: [17; 32],
                        signature: key
                            .sign(&possession_proof_signing_bytes(challenge, [17; 32]))
                            .to_bytes(),
                    }
                })
                .collect())
        })
    }
}

struct Routes(BTreeMap<NodeId, ValidatedRelayDialSet>);
impl RelayDialRouteProvider for Routes {
    fn route_set_for<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        _: Instant,
    ) -> ReachabilityFuture<'a, Result<ValidatedRelayDialSet, ReachabilityError>> {
        Box::pin(async move { Ok(self.0[&relay.canonical().relay_node_id].clone()) })
    }
}

#[derive(Default)]
struct UnavailableOptionalPaths(Mutex<Vec<RoutePathKindV1>>);
impl OptionalPeerPaths for UnavailableOptionalPaths {
    fn select<'a>(
        &'a self,
        _: NodeId,
        _: &'a ValidatedReachabilityAdvertisement,
        path: RoutePathKindV1,
        _: Instant,
    ) -> ReachabilityFuture<
        'a,
        Result<
            Option<ku_net::vnext_connection_executor::SelectedCarrier>,
            ku_net::vnext_route_plan::RouteFailure,
        >,
    > {
        Box::pin(async move {
            self.0.lock().unwrap().push(path);
            Ok(None)
        })
    }
}

struct RelayPort {
    runtime: Weak<VNextNetworkRuntime>,
    selector: ProductionExpectedPeerCarrierSelector,
    advertisement: ValidatedReachabilityAdvertisement,
}
impl ProductRouteConnector for RelayPort {
    fn current(&self) -> bool {
        true
    }
    fn connect<'a>(
        &'a self,
        peer: NodeId,
    ) -> ReachabilityFuture<'a, Result<RoutedVNextSession, VNextNetworkRuntimeError>> {
        Box::pin(async move {
            self.selector
                .connect_expected(&self.runtime.upgrade().unwrap(), peer, &self.advertisement)
                .await
        })
    }
}

async fn keyed_runtime(path: &Path, key: Arc<SigningKey>) -> Arc<VNextNetworkRuntime> {
    let public = *key.verifying_key().as_bytes();
    Arc::new(
        VNextNetworkRuntime::start_initialized(
            path,
            "127.0.0.1:0".parse().unwrap(),
            VNextNetworkPolicy::default(),
            false,
            1_073_741_824,
            key,
            public,
            None,
        )
        .await
        .unwrap(),
    )
}

fn executor(runtime: &VNextNetworkRuntime, key: &SigningKey) -> Arc<ConnectionPlannerExecutor> {
    Arc::new(ConnectionPlannerExecutor::new(
        Arc::new(QuicDirectCarrierDialer::new(runtime.shared_transport())),
        Arc::new(ProductionRelayCarrierDialer::standard()),
        Arc::new(ProductionRelayAssociationClient::new(
            *key.verifying_key().as_bytes(),
        )),
    ))
}

async fn serve_relay(
    runtime: Arc<VNextNetworkRuntime>,
    executor: Arc<ConnectionPlannerExecutor>,
    descriptor: ValidatedRelayDescriptor,
    remote: ValidatedRelayReservation,
    local: ValidatedRelayReservation,
    outer: Arc<AuthenticatedOuterRelayConnection>,
    identity: KnownPeerIdentity,
) {
    let endpoint = outer.public_endpoint();
    let candidate = RelayCandidateV1 {
        relay_node_id: descriptor.canonical().relay_node_id,
        reservation_id: remote.canonical().reservation_id,
        transport: outer.transport(),
        endpoint: ReachabilityEndpointV1 {
            host: endpoint.host.clone(),
            port: endpoint.port,
        },
        priority: 1,
        expires_at: local
            .canonical()
            .expires_at
            .min(remote.canonical().expires_at),
    };
    let peer = identity.node_id;
    let carrier = executor
        .accept_relay_inbound(
            descriptor,
            remote,
            local,
            identity,
            candidate,
            outer,
            Instant::now() + Duration::from_secs(15),
        )
        .await
        .unwrap();
    let session = runtime
        .accept_expected_selected(peer, carrier)
        .await
        .unwrap();
    let _ = runtime.serve_product_inbound(session).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn durable_delivery_moves_to_pre_reserved_alternate_tls_relay() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let relay_a = start_relay(SigningKey::from_bytes(&[101; 32]), now).await;
    let relay_b = start_relay(SigningKey::from_bytes(&[102; 32]), now).await;
    let resolver = Arc::new(PublicTestResolver);
    let preparer = Arc::new(ReachabilityAdmissionPreparer::new(resolver.clone(), 4).unwrap());
    let dial = Arc::new(ReachabilityDialValidator::new(resolver, 4).unwrap());
    let discovery = Arc::new(RwLock::new(RelayDiscovery::new(
        RelayDiscoveryPolicy::default(),
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default())),
        Arc::new(InMemoryAuthenticatedSessionRegistry::default()),
    )));
    admit_relay_records(
        &discovery,
        &RelayDiscoveryPreparer::new(preparer.clone(), dial.clone()),
        &Proofs(vec![relay_a.2.clone(), relay_b.2.clone()]),
        RelayDiscoverySource::manual_relay(),
        &[relay_a.0.clone(), relay_b.0.clone()],
        now,
        Instant::now() + Duration::from_secs(5),
    )
    .await
    .unwrap();
    let descriptors: Vec<_> = discovery.read().await.verified_relays().cloned().collect();
    let mut routes = BTreeMap::new();
    for descriptor in &descriptors {
        let fixture = if descriptor.canonical().relay_node_id
            == principal_node_id(relay_a.2.verifying_key().as_bytes())
        {
            &relay_a
        } else {
            &relay_b
        };
        routes.insert(
            descriptor.canonical().relay_node_id,
            alternate_route(descriptor.clone(), fixture.1, &fixture.2, now, 99),
        );
    }
    let routes = Arc::new(Routes(routes));
    let left_key = Arc::new(SigningKey::from_bytes(&[111; 32]));
    let right_key = Arc::new(SigningKey::from_bytes(&[112; 32]));
    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let left = keyed_runtime(left_dir.path(), left_key.clone()).await;
    let right = keyed_runtime(right_dir.path(), right_key.clone()).await;
    let left_client = Arc::new(ProductionRelayReservationClient::new(left_key.clone()));
    let right_client = Arc::new(ProductionRelayReservationClient::new(right_key.clone()));
    let left_reservations = Arc::new(
        RelayReservationManager::new(
            left_client,
            routes.clone(),
            VNextReachabilityPolicy::default(),
        )
        .unwrap(),
    );
    let right_reservations = Arc::new(
        RelayReservationManager::new(right_client, routes, VNextReachabilityPolicy::default())
            .unwrap(),
    );
    for descriptor in &descriptors {
        let key = if descriptor.canonical().relay_node_id
            == principal_node_id(relay_a.2.verifying_key().as_bytes())
        {
            &relay_a.2
        } else {
            &relay_b.2
        };
        left_reservations
            .ensure_route_reservation(
                descriptor,
                signed_request(&left_key, key, 1, now, 300),
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .unwrap();
        right_reservations
            .ensure_route_reservation(
                descriptor,
                signed_request(&right_key, key, 1, now, 300),
                Instant::now() + Duration::from_secs(5),
            )
            .await
            .unwrap();
    }
    let peer = principal_node_id(right_key.verifying_key().as_bytes());
    let reservations = right_reservations.active_reservations().await;
    let mut advertisement = ReachabilityAdvertisementV1 {
        format: 1,
        target_node_id: peer,
        relay_reservations: reservations.iter().map(|r| r.canonical().clone()).collect(),
        optional_public_candidates: vec![],
        capability_ceiling: [9; 32],
        sequence: 1,
        issued_at: now,
        expires_at: now + 200,
        target_signature: [0; 64],
    };
    advertisement.target_signature = right_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::Advertisement(advertisement.clone()),
                ReachabilitySignatureRoleV1::AdvertisementTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    let identity = KnownPeerIdentity::from_public_key(*right_key.verifying_key().as_bytes());
    let prepared = preparer
        .prepare_advertisement(
            &encode_reachability_object(&ReachabilityObjectV1::Advertisement(advertisement))
                .unwrap(),
            &identity,
            &reservations,
            now,
            Instant::now() + Duration::from_secs(5),
        )
        .await
        .unwrap();
    let advertisement =
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default()))
            .register_prepared_advertisement(prepared, &identity, &reservations, now)
            .unwrap();
    let optional = Arc::new(UnavailableOptionalPaths::default());
    let selector = ProductionExpectedPeerCarrierSelector::new(
        dial,
        executor(&left, &left_key),
        Duration::from_secs(20),
    )
    .unwrap()
    .with_optional_paths(optional.clone())
    .with_relay(discovery, left_reservations.clone(), left_key.clone(), 1)
    .unwrap();
    left.install_product_routing(Arc::new(RelayPort {
        runtime: Arc::downgrade(&left),
        selector,
        advertisement,
    }))
    .unwrap();
    for marker in [41, 42, 43] {
        let intent = OutboundTransferIntent::new(
            peer,
            "127.0.0.1:1".parse().unwrap(),
            SelectorCid::from_bytes([marker; 32]),
            NamespaceCommitment::from_bytes([5; 32]),
            DisclosureClass::Public,
            ReconcileManifestKind::FeedInception,
            tests::feed_and_event().0,
        )
        .unwrap();
        left.enqueue_outbound(&intent).unwrap();
    }
    let inbound_executor = executor(&right, &right_key);
    let mut previous_session = None;
    let mut first_checkpoint = None;
    let deliveries = [
        descriptors[0].clone(),
        descriptors[1].clone(),
        descriptors[1].clone(),
    ];
    for (index, descriptor) in deliveries.iter().enumerate() {
        let relay = descriptor.canonical().relay_node_id;
        let (remote, sender_outer) = left_reservations.active_for(relay).await.unwrap();
        let (local, target_outer) = right_reservations.active_for(relay).await.unwrap();
        let serve = tokio::spawn(serve_relay(
            right.clone(),
            inbound_executor.clone(),
            descriptor.clone(),
            remote,
            local,
            target_outer,
            KnownPeerIdentity::from_public_key(*left_key.verifying_key().as_bytes()),
        ));
        let report = tokio::time::timeout(Duration::from_secs(25), left.deliver_outbound_once(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report.acknowledged, 1, "{report:?}");
        tokio::time::timeout(Duration::from_secs(5), serve)
            .await
            .unwrap()
            .unwrap();
        let route = left.authenticated_routed_route(peer).unwrap().unwrap();
        assert!(
            matches!(route.carrier, VerifiedCarrierIdentity::Relay { relay_node_id, .. } if relay_node_id == relay)
        );
        assert_ne!(previous_session, Some(route.session_id));
        previous_session = Some(route.session_id);
        let checkpoint = left.outbound_checkpoint(peer).unwrap().unwrap();
        assert_eq!(checkpoint.acknowledged_sequence(), index as u64 + 1);
        if index == 0 {
            first_checkpoint = Some(checkpoint.clone());
            // Both alternates were reserved before selected-carrier loss.
            assert_eq!(left_reservations.active_count().await, 2);
            sender_outer.close();
            left_reservations.invalidate_closed().await;
            assert_eq!(left.outbound_checkpoint(peer).unwrap(), Some(checkpoint));
        } else {
            assert_ne!(
                first_checkpoint.as_ref().unwrap().acknowledged_intent_id(),
                checkpoint.acknowledged_intent_id()
            );
        }
    }
    let feed = ku_core::foundation::decode_feed_inception(&tests::feed_and_event().0).unwrap();
    assert_eq!(right.feed_inception_branch_count(feed.feed_id).unwrap(), 1);
    assert_eq!(left.outbound_pending_count().unwrap(), 0);
    assert_eq!(
        *optional.0.lock().unwrap(),
        vec![
            RoutePathKindV1::Direct,
            RoutePathKindV1::HolePunched,
            RoutePathKindV1::Direct,
            RoutePathKindV1::HolePunched,
            RoutePathKindV1::Direct,
            RoutePathKindV1::HolePunched
        ]
    );
    left_reservations.close_all().await;
    right_reservations.close_all().await;
    let mut left = Arc::try_unwrap(left).ok().unwrap();
    left.shutdown().await;
    let mut right = Arc::try_unwrap(right).ok().unwrap();
    right.shutdown().await;
    relay_a.3.abort();
    relay_b.3.abort();
    let _ = relay_a.3.await;
    let _ = relay_b.3.await;
}

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
        let mut connections = tokio::task::JoinSet::new();
        for _ in 0..2 {
            let listener = listener.clone();
            let service = service.clone();
            connections.spawn(async move {
                let _ = listener.serve_production_once(service).await;
            });
        }
        while connections.join_next().await.is_some() {}
    });
    (encoded, address, relay_key, server, directory)
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

fn signed_request(
    target: &SigningKey,
    relay: &SigningKey,
    sequence: u64,
    now: u64,
    ttl: u64,
) -> onebrain_protocol::RelayReserveRequestV1 {
    use onebrain_protocol::*;
    let mut request = RelayReserveRequestV1 {
        format: 1,
        relay_node_id: principal_node_id(relay.verifying_key().as_bytes()),
        target_node_id: principal_node_id(target.verifying_key().as_bytes()),
        reservation_id: *blake3::hash(
            &[
                target.verifying_key().as_bytes().as_slice(),
                relay.verifying_key().as_bytes().as_slice(),
                &sequence.to_be_bytes(),
            ]
            .concat(),
        )
        .as_bytes(),
        transport_scope: vec![RelayTransportV1::TlsTcp443],
        sequence,
        issued_at: now,
        expires_at: now + ttl,
        target_reservation_signature: [0; 64],
        target_request_signature: [0; 64],
    };
    let grant = RelayReservationV1 {
        format: 1,
        relay_node_id: request.relay_node_id,
        target_node_id: request.target_node_id,
        reservation_id: request.reservation_id,
        transport_scope: request.transport_scope.clone(),
        issued_at: now,
        expires_at: now + ttl,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    request.target_reservation_signature = target
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(grant),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    request.target_request_signature = target
        .sign(
            &relay_control_signing_bytes(
                &RelayControlV1::Reserve(request.clone()),
                RelayControlSignatureRoleV1::ReserveRequestTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    request
}
