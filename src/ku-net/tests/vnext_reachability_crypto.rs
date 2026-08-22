use std::collections::BTreeMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_reachability_crypto::{
    possession_proof_signing_bytes, ConfiguredBootstrapSource, InMemoryReachabilityReplayStore,
    KnownPeerIdentity, PublicEndpointResolver, ReachabilityAdmission,
    ReachabilityAdmissionPreparer, ReachabilityDialValidator, ReachabilityLockFreeDialValidation,
    ReachabilityLockFreePreparation, ReachabilityNonceDomainV1, ReachabilityRecordAdmission,
    ReachabilityReplayStore, ReachabilitySequenceKeyV1, ReachabilitySequenceKindV1,
    RelayAdmissionError, SystemPublicEndpointResolver,
};
use ku_net::vnext_session::principal_node_id;
use onebrain_protocol::{
    encode_reachability_object, reachability_signing_bytes, HostAddressV1, ProtocolVersionV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1, RelayEndpointV1,
    RelayPossessionProofV1, RelayReservationV1, RelayTransportV1,
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
        Poll::Pending => panic!("test resolver future unexpectedly pending"),
    }
}

#[derive(Default)]
struct MutableResolver {
    answers: Mutex<BTreeMap<String, Vec<IpAddr>>>,
}

impl MutableResolver {
    fn set(&self, host: &str, addresses: Vec<IpAddr>) {
        self.answers.lock().unwrap().insert(host.into(), addresses);
    }
}

impl PublicEndpointResolver for MutableResolver {
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

fn signed_descriptor(key: &SigningKey, sequence: u64, issued_at: u64) -> RelayDescriptorV1 {
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
        capacity_policy_digest: [4; 32],
        previous_descriptor_blake3: None,
        sequence,
        issued_at,
        expires_at: issued_at + 600,
        relay_signature: [0; 64],
    };
    let object = ReachabilityObjectV1::RelayDescriptor(descriptor.clone());
    descriptor.relay_signature = key
        .sign(
            &reachability_signing_bytes(&object, ReachabilitySignatureRoleV1::RelayDescriptor)
                .unwrap(),
        )
        .to_bytes();
    descriptor
}

fn signed_reservation(
    target_key: &SigningKey,
    relay_key: &SigningKey,
    reservation_id: [u8; 32],
    now: u64,
) -> RelayReservationV1 {
    let mut reservation = RelayReservationV1 {
        format: 1,
        relay_node_id: principal_node_id(relay_key.verifying_key().as_bytes()),
        target_node_id: principal_node_id(target_key.verifying_key().as_bytes()),
        reservation_id,
        transport_scope: vec![RelayTransportV1::TlsTcp443],
        issued_at: now,
        expires_at: now + 900,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    reservation.target_signature = target_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(reservation.clone()),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    reservation.relay_signature = relay_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(reservation.clone()),
                ReachabilitySignatureRoleV1::ReservationRelay,
            )
            .unwrap(),
        )
        .to_bytes();
    reservation
}

#[test]
fn descriptor_requires_identity_signature_freshness_and_live_possession() {
    let now = 10_000;
    let key = SigningKey::from_bytes(&[7; 32]);
    let resolver = Arc::new(MutableResolver::default());
    resolver.set("relay.example", vec!["1.1.1.1".parse().unwrap()]);
    let preparer = ReachabilityAdmissionPreparer::new(resolver.clone(), 4).unwrap();
    let dial = ReachabilityDialValidator::new(resolver, 4).unwrap();
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay);

    let descriptor = signed_descriptor(&key, 1, now);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).unwrap();
    let prepared = block_on_ready(preparer.prepare_descriptor(
        &bytes,
        now,
        Instant::now() + Duration::from_secs(1),
    ))
    .unwrap();
    let pending = admission
        .register_prepared_descriptor(prepared, [8; 32], now)
        .unwrap();
    let token = block_on_ready(dial.validate_possession_dial(
        &pending,
        0,
        Instant::now() + Duration::from_secs(1),
    ))
    .unwrap();
    let binding = [9; 32];
    let proof = RelayPossessionProofV1 {
        challenge_digest: token.challenge_digest(),
        connection_binding_digest: binding,
        signature: key
            .sign(&possession_proof_signing_bytes(token.challenge(), binding))
            .to_bytes(),
    };
    let validated = admission
        .complete_descriptor_admission(pending, &[proof], now + 1)
        .unwrap();
    assert_eq!(
        validated.canonical().relay_public_key,
        *key.verifying_key().as_bytes()
    );
}

#[test]
fn wrong_node_key_signature_and_time_reject_before_possession() {
    let now = 20_000;
    let key = SigningKey::from_bytes(&[3; 32]);
    let resolver = Arc::new(MutableResolver::default());
    resolver.set("relay.example", vec!["1.1.1.1".parse().unwrap()]);
    let preparer = ReachabilityAdmissionPreparer::new(resolver, 1).unwrap();

    let mut wrong_node = signed_descriptor(&key, 1, now);
    wrong_node.relay_node_id = principal_node_id(&[99; 32]);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(wrong_node)).unwrap();
    assert_eq!(
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())),
        Err(RelayAdmissionError::IdentityMismatch)
    );

    let mut wrong_signature = signed_descriptor(&key, 1, now);
    wrong_signature.relay_signature[0] ^= 1;
    let bytes = encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(wrong_signature))
        .unwrap();
    assert_eq!(
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())),
        Err(RelayAdmissionError::SignatureInvalid)
    );

    let expired = signed_descriptor(&key, 1, now - 601);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(expired)).unwrap();
    assert_eq!(
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())),
        Err(RelayAdmissionError::Expired)
    );

    let not_yet_valid = signed_descriptor(&key, 1, now + 1);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(not_yet_valid)).unwrap();
    assert_eq!(
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())),
        Err(RelayAdmissionError::NotYetValid)
    );
}

#[test]
fn dns_private_answer_and_public_to_private_rebinding_reject() {
    let now = 30_000;
    let key = SigningKey::from_bytes(&[5; 32]);
    let resolver = Arc::new(MutableResolver::default());
    resolver.set("relay.example", vec!["192.168.1.1".parse().unwrap()]);
    let preparer = ReachabilityAdmissionPreparer::new(resolver.clone(), 1).unwrap();
    let descriptor = signed_descriptor(&key, 1, now);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).unwrap();
    assert_eq!(
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())),
        Err(RelayAdmissionError::EndpointNotGlobal)
    );

    resolver.set("relay.example", vec!["1.1.1.1".parse().unwrap()]);
    let prepared =
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())).unwrap();
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay);
    let pending = admission
        .register_prepared_descriptor(prepared, [6; 32], now)
        .unwrap();
    resolver.set("relay.example", vec!["10.0.0.1".parse().unwrap()]);
    let dial = ReachabilityDialValidator::new(resolver, 1).unwrap();
    assert_eq!(
        block_on_ready(dial.validate_possession_dial(&pending, 0, Instant::now())),
        Err(RelayAdmissionError::EndpointNotGlobal)
    );
}

#[test]
fn reservation_requires_target_then_relay_signatures_and_rejects_reuse() {
    let now = 40_000;
    let target_key = SigningKey::from_bytes(&[11; 32]);
    let relay_key = SigningKey::from_bytes(&[12; 32]);
    let target = KnownPeerIdentity::from_public_key(*target_key.verifying_key().as_bytes());
    let relay = KnownPeerIdentity::from_public_key(*relay_key.verifying_key().as_bytes());
    let mut reservation = signed_reservation(&target_key, &relay_key, [13; 32], now);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayReservation(reservation.clone()))
            .unwrap();
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay);
    admission
        .admit_reservation(&bytes, &target, &relay, now)
        .unwrap();

    reservation.relay_signature = [1; 64];
    let changed =
        encode_reachability_object(&ReachabilityObjectV1::RelayReservation(reservation)).unwrap();
    assert_eq!(
        admission.admit_reservation(&changed, &target, &relay, now),
        Err(RelayAdmissionError::SignatureInvalid)
    );
}

#[test]
fn reservation_rejects_missing_reversed_and_reused_identity() {
    let now = 41_000;
    let target_key = SigningKey::from_bytes(&[21; 32]);
    let relay_key = SigningKey::from_bytes(&[22; 32]);
    let target = KnownPeerIdentity::from_public_key(*target_key.verifying_key().as_bytes());
    let relay = KnownPeerIdentity::from_public_key(*relay_key.verifying_key().as_bytes());
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay);

    let valid = signed_reservation(&target_key, &relay_key, [23; 32], now);
    let mut missing_target = valid.clone();
    missing_target.target_signature = [0; 64];
    let bytes = encode_reachability_object(&ReachabilityObjectV1::RelayReservation(missing_target))
        .unwrap();
    assert_eq!(
        admission.admit_reservation(&bytes, &target, &relay, now),
        Err(RelayAdmissionError::SignatureInvalid)
    );

    let mut reversed = valid.clone();
    std::mem::swap(
        &mut reversed.target_signature,
        &mut reversed.relay_signature,
    );
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayReservation(reversed)).unwrap();
    assert_eq!(
        admission.admit_reservation(&bytes, &target, &relay, now),
        Err(RelayAdmissionError::SignatureInvalid)
    );

    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayReservation(valid.clone())).unwrap();
    admission
        .admit_reservation(&bytes, &target, &relay, now)
        .unwrap();
    assert_eq!(
        admission.admit_reservation(&bytes, &target, &relay, now),
        Err(RelayAdmissionError::Replay)
    );

    let changed = signed_reservation(&target_key, &relay_key, [23; 32], now + 1);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayReservation(changed)).unwrap();
    assert_eq!(
        admission.admit_reservation(&bytes, &target, &relay, now + 1),
        Err(RelayAdmissionError::ReservationIdReuse)
    );
}

#[test]
fn every_forbidden_literal_address_class_rejects() {
    let now = 42_000;
    let key = SigningKey::from_bytes(&[24; 32]);
    let forbidden = [
        "0.0.0.0",
        "10.0.0.1",
        "100.64.0.1",
        "127.0.0.1",
        "169.254.1.1",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
        "224.0.0.1",
        "255.255.255.255",
    ];
    for address in forbidden {
        let mut descriptor = signed_descriptor(&key, 1, now);
        descriptor.endpoints[0].host =
            HostAddressV1::Ipv4(address.parse::<Ipv4Addr>().unwrap().octets());
        assert!(
            encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).is_err(),
            "address {address} must fail closed"
        );
    }
}

#[test]
fn incomplete_or_invalid_possession_can_be_aborted_without_advancing_sequence() {
    let now = 43_000;
    let key = SigningKey::from_bytes(&[25; 32]);
    let resolver = Arc::new(MutableResolver::default());
    resolver.set("relay.example", vec!["1.1.1.1".parse().unwrap()]);
    let preparer = ReachabilityAdmissionPreparer::new(resolver, 1).unwrap();
    let replay = Arc::new(InMemoryReachabilityReplayStore::default());
    let mut admission = ReachabilityAdmission::new(replay);

    let descriptor = signed_descriptor(&key, 1, now);
    let bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor)).unwrap();
    let prepared =
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())).unwrap();
    let pending = admission
        .register_prepared_descriptor(prepared, [26; 32], now)
        .unwrap();
    let duplicate =
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())).unwrap();
    assert_eq!(
        admission.register_prepared_descriptor(duplicate, [26; 32], now),
        Err(RelayAdmissionError::Replay)
    );
    assert_eq!(
        admission.complete_descriptor_admission(pending.clone(), &[], now),
        Err(RelayAdmissionError::PossessionInvalid)
    );
    admission.abort_descriptor_admission(pending).unwrap();

    let prepared =
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())).unwrap();
    let pending = admission
        .register_prepared_descriptor(prepared, [27; 32], now)
        .unwrap();
    assert_eq!(admission.expire_pending_descriptors(now + 31), 1);
    assert_eq!(
        admission.abort_descriptor_admission(pending),
        Err(RelayAdmissionError::ChallengeMissing)
    );

    let prepared =
        block_on_ready(preparer.prepare_descriptor(&bytes, now, Instant::now())).unwrap();
    admission
        .register_prepared_descriptor(prepared, [28; 32], now)
        .unwrap();
}

#[test]
fn replay_store_enforces_monotonic_sequence_and_nonce_consumption() {
    let store = InMemoryReachabilityReplayStore::default();
    let key = ReachabilitySequenceKeyV1 {
        kind: ReachabilitySequenceKindV1::Advertisement,
        signer: [31; 32],
        scope: [0; 32],
    };
    store
        .check_and_advance_sequence(key, 1, [1; 32], 10)
        .unwrap();
    assert_eq!(
        store.check_and_advance_sequence(key, 1, [1; 32], 10),
        Err(RelayAdmissionError::Replay)
    );
    assert_eq!(
        store.check_and_advance_sequence(key, 3, [3; 32], 30),
        Err(RelayAdmissionError::SequenceRollback)
    );
    store
        .check_and_advance_sequence(key, 2, [2; 32], 20)
        .unwrap();

    store
        .consume_nonce(
            ReachabilityNonceDomainV1::RelayControl,
            [2; 32],
            [3; 32],
            20,
        )
        .unwrap();
    assert_eq!(
        store.consume_nonce(
            ReachabilityNonceDomainV1::RelayControl,
            [2; 32],
            [3; 32],
            20,
        ),
        Err(RelayAdmissionError::ChallengeConsumed)
    );
}

#[test]
fn trusted_bootstrap_authority_cannot_be_constructed_from_network_bytes() {
    assert!(
        ConfiguredBootstrapSource::load_from_trusted_local_file(std::path::Path::new(
            "definitely-missing-bootstrap-source.conf"
        ))
        .is_err()
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bootstrap-source.conf");
    let key = SigningKey::from_bytes(&[33; 32]);
    let public_key = key
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::fs::write(
        &path,
        format!(
            "format=onebrain/bootstrap-source/1\npublic_key={public_key}\ntransport=https\nhost=ipv4:1.1.1.1\nport=443\npath=/reachability\n"
        ),
    )
    .unwrap();
    let source = ConfiguredBootstrapSource::load_from_trusted_local_file(&path).unwrap();
    assert_eq!(source.fetch_endpoint().port, 443);
}

#[test]
fn system_resolver_is_bounded_and_preserves_literal_addresses() {
    assert!(SystemPublicEndpointResolver::new(0).is_err());
    assert!(SystemPublicEndpointResolver::new(5).is_err());
    let resolver = SystemPublicEndpointResolver::new(2).unwrap();
    assert_eq!(
        resolver
            .resolve(
                &HostAddressV1::Ipv4([8, 8, 8, 8]),
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap(),
        vec![IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))]
    );
    assert_eq!(
        resolver.resolve(&HostAddressV1::Ipv4([8, 8, 4, 4]), Instant::now()),
        Err(RelayAdmissionError::DnsResolutionFailed)
    );
}
