use std::collections::BTreeMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_reachability_crypto::{
    InMemoryReachabilityReplayStore, PublicEndpointResolver, ReachabilityAdmission,
    ReachabilityAdmissionPreparer, ReachabilityDialValidator, RelayAdmissionError,
};
use ku_net::vnext_relay_discovery::{
    decode_manual_peer_invitation, decode_manual_relay_invitation, encode_manual_relay_invitation,
    InMemoryAuthenticatedSessionRegistry, RelayDiscovery, RelayDiscoveryLimitation,
    RelayDiscoveryPolicy, RelayDiscoveryPreparer, RelayDiscoverySource, VerifiedRelayDiscovery,
};
use ku_net::vnext_session::principal_node_id;
use onebrain_protocol::{
    encode_reachability_object, reachability_signing_bytes, HostAddressV1, ProtocolVersionV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1, RelayEndpointV1,
    RelayPossessionProofV1, RelayTransportV1,
};

fn block_on_ready<F: Future>(future: F) -> F::Output {
    fn raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            raw_waker()
        }
        fn noop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    let waker = unsafe { Waker::from_raw(raw_waker()) };
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    match Pin::new(&mut future).poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

#[derive(Default)]
struct Resolver {
    answers: Mutex<BTreeMap<String, Vec<IpAddr>>>,
}

impl Resolver {
    fn set(&self, host: &str, addresses: Vec<IpAddr>) {
        self.answers.lock().unwrap().insert(host.into(), addresses);
    }
}

impl PublicEndpointResolver for Resolver {
    fn resolve(
        &self,
        host: &HostAddressV1,
        _deadline: Instant,
    ) -> Result<Vec<IpAddr>, RelayAdmissionError> {
        match host {
            HostAddressV1::Ipv4(value) => Ok(vec![IpAddr::V4(Ipv4Addr::from(*value))]),
            HostAddressV1::Ipv6(value) => Ok(vec![IpAddr::V6((*value).into())]),
            HostAddressV1::Dns(value) => self
                .answers
                .lock()
                .unwrap()
                .get(value)
                .cloned()
                .ok_or(RelayAdmissionError::DnsResolutionFailed),
        }
    }
}

fn signed_descriptor(key: &SigningKey, now: u64) -> Vec<u8> {
    let mut descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(key.verifying_key().as_bytes()),
        relay_public_key: *key.verifying_key().as_bytes(),
        endpoints: vec![RelayEndpointV1 {
            transport: RelayTransportV1::TlsTcp443,
            host: HostAddressV1::Dns("relay.example".into()),
            port: 443,
        }],
        supported_transports: vec![RelayTransportV1::TlsTcp443],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [5; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: now,
        expires_at: now + 600,
        relay_signature: [0; 64],
    };
    descriptor.relay_signature = key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).unwrap()
}

fn setup_discovery() -> (RelayDiscovery, RelayDiscoveryPreparer) {
    let resolver = Arc::new(Resolver::default());
    resolver.set("relay.example", vec!["1.1.1.1".parse().unwrap()]);
    let admission_preparer = Arc::new(ReachabilityAdmissionPreparer::new(resolver, 4).unwrap());
    let dial_resolver = Arc::new(Resolver::default());
    dial_resolver.set("relay.example", vec!["1.1.1.1".parse().unwrap()]);
    let dial_validator = Arc::new(ReachabilityDialValidator::new(dial_resolver, 4).unwrap());
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let admission = ReachabilityAdmission::new(replay);
    let sessions = Arc::new(InMemoryAuthenticatedSessionRegistry::default());
    (
        RelayDiscovery::new(RelayDiscoveryPolicy::default(), admission, sessions),
        RelayDiscoveryPreparer::new(admission_preparer, dial_validator),
    )
}

#[test]
fn ninth_source_sixty_fifth_record_and_two_hundred_fifty_seventh_total_reject() {
    let (mut discovery, _) = setup_discovery();
    for index in 0..8_u8 {
        let source = RelayDiscoverySource::rendezvous(principal_node_id(&[index + 1; 32]));
        let permit = discovery.reserve_preparation(source, &[1], 10_000).unwrap();
        discovery.abort_preparation(permit, 10_000).unwrap();
    }
    assert_eq!(
        discovery.reserve_preparation(
            RelayDiscoverySource::rendezvous(principal_node_id(&[99; 32])),
            &[1],
            10_000,
        ),
        Err(RelayDiscoveryLimitation::SourceKeyLimit)
    );

    let (mut discovery, _) = setup_discovery();
    assert_eq!(
        discovery.reserve_preparation(RelayDiscoverySource::manual_relay(), &[1; 65], 10_000),
        Err(RelayDiscoveryLimitation::RecordLimit)
    );
    for index in 0..4_u8 {
        let source = RelayDiscoverySource::rendezvous(principal_node_id(&[index + 1; 32]));
        let permit = discovery
            .reserve_preparation(source, &[1; 64], 10_000)
            .unwrap();
        discovery.abort_preparation(permit, 10_000).unwrap();
    }
    assert_eq!(
        discovery.reserve_preparation(RelayDiscoverySource::manual_relay(), &[1], 10_000),
        Err(RelayDiscoveryLimitation::RecordLimit)
    );
}

#[test]
fn bytes_duplicates_and_failed_possession_fail_closed_without_publishing() {
    let now = 20_000;
    let key = SigningKey::from_bytes(&[7; 32]);
    let bytes = signed_descriptor(&key, now);
    let (mut discovery, preparer) = setup_discovery();
    let oversized = vec![1; RelayDiscoveryPolicy::default().max_bytes_per_source + 1];
    assert_eq!(
        discovery.reserve_preparation(
            RelayDiscoverySource::manual_relay(),
            &[oversized.len()],
            now,
        ),
        Err(RelayDiscoveryLimitation::ByteLimit)
    );

    let permit = discovery
        .reserve_preparation(
            RelayDiscoverySource::manual_relay(),
            &[bytes.len(), bytes.len()],
            now,
        )
        .unwrap();
    let prepared = block_on_ready(preparer.prepare_records(
        &permit,
        &[bytes.clone(), bytes],
        now,
        Instant::now() + Duration::from_secs(1),
    ))
    .unwrap();
    assert!(matches!(
        discovery.stage_prepared(permit, prepared, now),
        Err(RelayDiscoveryLimitation::PoisonedSource)
    ));
    assert_eq!(discovery.verified_relays().count(), 0);

    let permit = discovery
        .reserve_preparation(RelayDiscoverySource::manual_relay(), &[3], now)
        .unwrap();
    assert_eq!(
        block_on_ready(preparer.prepare_records(
            &permit,
            &[vec![1, 2, 3]],
            now,
            Instant::now() + Duration::from_secs(1),
        )),
        Err(RelayDiscoveryLimitation::PoisonedSource)
    );
    discovery.abort_preparation(permit, now).unwrap();
    assert_eq!(discovery.verified_relays().count(), 0);
}

#[test]
fn complete_live_possession_is_required_before_relay_becomes_visible() {
    let now = 30_000;
    let key = SigningKey::from_bytes(&[8; 32]);
    let bytes = signed_descriptor(&key, now);
    let (mut discovery, preparer) = setup_discovery();
    let permit = discovery
        .reserve_preparation(RelayDiscoverySource::manual_relay(), &[bytes.len()], now)
        .unwrap();
    let prepared = block_on_ready(preparer.prepare_records(
        &permit,
        &[bytes],
        now,
        Instant::now() + Duration::from_secs(1),
    ))
    .unwrap();
    let staged = discovery
        .stage_prepared(permit, prepared, now)
        .unwrap()
        .pop()
        .unwrap();
    let staged = block_on_ready(
        preparer.prepare_possession(staged, Instant::now() + Duration::from_secs(1)),
    )
    .unwrap();
    assert_eq!(discovery.verified_relays().count(), 0);
    let binding = [9; 32];
    let challenge = staged.possession_dials()[0].challenge();
    let proof = RelayPossessionProofV1 {
        challenge_digest: ku_net::vnext_reachability_crypto::possession_challenge_digest(challenge),
        connection_binding_digest: binding,
        signature: key
            .sign(
                &ku_net::vnext_reachability_crypto::possession_proof_signing_bytes(
                    challenge, binding,
                ),
            )
            .to_bytes(),
    };
    discovery
        .commit_descriptor(staged, &[proof], now + 1)
        .unwrap();
    assert_eq!(discovery.verified_relays().count(), 1);
}

#[test]
fn invitation_prefix_canonicality_and_peer_envelope_are_separate() {
    let key = SigningKey::from_bytes(&[10; 32]);
    let bytes = signed_descriptor(&key, 40_000);
    let invitation = encode_manual_relay_invitation(&bytes).unwrap();
    assert_eq!(decode_manual_relay_invitation(&invitation).unwrap(), bytes);
    assert!(decode_manual_relay_invitation("onebrain://relay/v1/not+base64").is_err());
    assert!(decode_manual_peer_invitation(&invitation).is_err());
    assert!(decode_manual_peer_invitation("onebrain://peer/v1/broken").is_err());
}
