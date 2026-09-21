use super::cache::CacheRecovery;
use super::discovery::*;
use crate::vnext_reachability_replay_store::RedbReachabilityReplayStore;
use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_reachability_crypto::*;
use ku_net::vnext_relay_discovery::*;
use ku_net::vnext_session::principal_node_id;
use onebrain_protocol::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const NOW: u64 = 10_000;

struct Resolver;
impl PublicEndpointResolver for Resolver {
    fn resolve(
        &self,
        host: &HostAddressV1,
        _: Instant,
    ) -> Result<Vec<std::net::IpAddr>, RelayAdmissionError> {
        Ok(vec![match host {
            HostAddressV1::Ipv4(ip) => (*ip).into(),
            _ => "1.1.1.1".parse().unwrap(),
        }])
    }
}

struct Possession {
    calls: AtomicUsize,
    revoke: Option<Arc<AtomicBool>>,
}
impl RelayPossessionClient for Possession {
    fn prove<'a>(
        &'a self,
        staged: &'a StagedRelayAdmission,
        _: Instant,
    ) -> ReachabilityFuture<'a, Result<Vec<RelayPossessionProofV1>, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(current) = &self.revoke {
                current.store(false, Ordering::SeqCst);
            }
            Ok(staged
                .challenges()
                .iter()
                .map(|challenge| {
                    let key = (1..=8)
                        .map(|n| SigningKey::from_bytes(&[n; 32]))
                        .find(|key| {
                            principal_node_id(key.verifying_key().as_bytes())
                                == challenge.relay_node_id
                        })
                        .unwrap();
                    let binding = [99; 32];
                    RelayPossessionProofV1 {
                        challenge_digest: possession_challenge_digest(challenge),
                        connection_binding_digest: binding,
                        signature: key
                            .sign(&possession_proof_signing_bytes(challenge, binding))
                            .to_bytes(),
                    }
                })
                .collect())
        })
    }
}

struct Transport {
    manifest: Vec<u8>,
    first: Vec<u8>,
    second: Vec<u8>,
    seed_lost: AtomicBool,
    all_lost: AtomicBool,
    hang_first: AtomicBool,
    calls: Mutex<Vec<String>>,
}
impl DiscoveryTransport for Transport {
    fn fetch<'a>(
        &'a self,
        endpoint: &'a ValidatedPublicDialEndpoint,
        _: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>> {
        Box::pin(async move {
            let path = endpoint.signed_path().unwrap().to_owned();
            self.calls.lock().unwrap().push(path.clone());
            if path == "/learned-a" && self.hang_first.load(Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            if self.all_lost.load(Ordering::SeqCst) {
                return Err(RelayDiscoveryLimitation::NoBootstrapReachable);
            }
            match path.as_str() {
                "/seed" if !self.seed_lost.load(Ordering::SeqCst) => {
                    Ok(vec![self.manifest.clone()])
                }
                "/learned-a" if !self.seed_lost.load(Ordering::SeqCst) => {
                    Ok(vec![self.first.clone()])
                }
                "/learned-b" => Ok(vec![self.second.clone()]),
                _ => Err(RelayDiscoveryLimitation::NoBootstrapReachable),
            }
        })
    }
}

fn descriptor(seed: u8) -> Vec<u8> {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut value = RelayDescriptorV1 {
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
        issued_at: NOW,
        expires_at: NOW + 600,
        relay_signature: [0; 64],
    };
    value.relay_signature = key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(value.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(value)).unwrap()
}

fn bootstrap(dir: &std::path::Path, dns: bool) -> (ConfiguredBootstrapSource, Arc<Transport>) {
    let key = SigningKey::from_bytes(&[21; 32]);
    let path = dir.join("source.conf");
    let public_key = key
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let host = if dns {
        "dns:seed.example"
    } else {
        "ipv4:1.1.1.1"
    };
    std::fs::write(&path, format!("format=onebrain/bootstrap-source/1\npublic_key={public_key}\ntransport=https\nhost={host}\nport=443\npath=/seed\n")).unwrap();
    let source = ConfiguredBootstrapSource::load_from_trusted_local_file(&path).unwrap();
    let mut value = BootstrapManifestV1 {
        format: 1,
        discovery_source_id: *source.source_id(),
        discovery_endpoints: ["/learned-a", "/learned-b"]
            .into_iter()
            .map(|path| DiscoveryEndpointV1 {
                transport: DiscoveryTransportV1::Https,
                host: HostAddressV1::Dns("learned.example".into()),
                port: 443,
                path: path.into(),
            })
            .collect(),
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        sequence: 1,
        issued_at: NOW,
        expires_at: NOW + 600,
        source_signature: [0; 64],
    };
    value.source_signature = key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::BootstrapManifest(value.clone()),
                ReachabilitySignatureRoleV1::BootstrapSource,
            )
            .unwrap(),
        )
        .to_bytes();
    let manifest =
        encode_reachability_object(&ReachabilityObjectV1::BootstrapManifest(value)).unwrap();
    (
        source,
        Arc::new(Transport {
            manifest,
            first: descriptor(1),
            second: descriptor(2),
            seed_lost: AtomicBool::new(false),
            all_lost: AtomicBool::new(false),
            hang_first: AtomicBool::new(false),
            calls: Mutex::new(vec![]),
        }),
    )
}

fn fixture(
    path: &std::path::Path,
    inputs: Vec<DiscoveryInput>,
    possession: Arc<Possession>,
) -> (
    DiscoveryOwner,
    Arc<RwLock<RelayDiscovery>>,
    Arc<RedbReachabilityReplayStore>,
) {
    let store = Arc::new(RedbReachabilityReplayStore::open_validated(path, 16_777_216).unwrap());
    let cached = cached_inputs(&inputs, &store).unwrap();
    let recovery = Arc::new(CacheRecovery::new(store.clone(), &cached).unwrap());
    for input in &inputs {
        if let DiscoveryInput::ManualPeer { invitation, .. } = input {
            let peer = decode_manual_peer_invitation(invitation).unwrap();
            for bytes in store
                .cached_records(*peer.identity().node_id.as_bytes())
                .unwrap()
            {
                if matches!(
                    decode_reachability_object(&bytes),
                    Ok(ReachabilityObjectV1::Advertisement(_))
                ) {
                    recovery
                        .allow_advertisement(peer.identity().public_key, &bytes)
                        .unwrap();
                }
            }
        }
        if let DiscoveryInput::Bootstrap { source, .. } = input {
            for bytes in store.cached_records(*source.source_id()).unwrap() {
                if matches!(
                    decode_reachability_object(&bytes),
                    Ok(ReachabilityObjectV1::BootstrapManifest(_))
                ) {
                    recovery
                        .allow_manifest(*source.public_key(), &bytes)
                        .unwrap();
                }
            }
        }
    }
    let resolver = Arc::new(Resolver);
    let preparer = Arc::new(ReachabilityAdmissionPreparer::new(resolver.clone(), 4).unwrap());
    let dial = Arc::new(ReachabilityDialValidator::new(resolver, 4).unwrap());
    let sessions = Arc::new(InMemoryAuthenticatedSessionRegistry::default());
    let discovery = Arc::new(RwLock::new(RelayDiscovery::new(
        RelayDiscoveryPolicy::default(),
        ReachabilityAdmission::new(recovery.clone()),
        sessions.clone(),
    )));
    let owner = DiscoveryOwner::new(
        inputs,
        ReachabilityAdmission::new(recovery),
        store.clone(),
        preparer,
        dial,
        possession,
        sessions,
    )
    .unwrap();
    (owner, discovery, store)
}

fn proof() -> Arc<Possession> {
    Arc::new(Possession {
        calls: AtomicUsize::new(0),
        revoke: None,
    })
}

pub(super) async fn validated_relays(
    path: &std::path::Path,
    count: u8,
) -> Vec<ValidatedRelayDescriptor> {
    let inputs = (1..=count)
        .map(|seed| DiscoveryInput::RelayEndpoint {
            signed_descriptor: descriptor(seed),
        })
        .collect();
    let (mut owner, discovery, _) = fixture(path, inputs, proof());
    owner.refresh(&discovery, NOW, &|| true).await.unwrap();
    let records = discovery.read().await.verified_relays().cloned().collect();
    records
}

#[tokio::test]
async fn dns_and_ip_bootstrap_keep_learned_sources_after_seed_loss() {
    for dns in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (source, transport) = bootstrap(dir.path(), dns);
        let possession = proof();
        let (mut owner, discovery, _) = fixture(
            &dir.path().join("replay.redb"),
            vec![DiscoveryInput::Bootstrap {
                source,
                manifest: None,
                transport: transport.clone(),
            }],
            possession.clone(),
        );
        owner.refresh(&discovery, NOW, &|| true).await.unwrap();
        assert_eq!(discovery.read().await.verified_relays().count(), 2);
        transport.seed_lost.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(1)).await;
        owner.force_refresh_for_test();
        owner.refresh(&discovery, NOW + 2, &|| true).await.unwrap();
        let status = owner.statuses.read().await;
        assert_eq!(status[0].state, "usable");
        assert_eq!(status[0].admitted_records, 2);
        assert_eq!(
            possession.calls.load(Ordering::SeqCst),
            2,
            "dedup avoids repeated proof and sequence admission"
        );
        assert!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|path| *path == "/learned-b")
                .count()
                >= 2
        );
    }
}

#[tokio::test]
async fn restart_recovers_only_exact_signed_cache_with_fresh_possession() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay.redb");
    let (source, transport) = bootstrap(dir.path(), true);
    {
        let (mut owner, discovery, _) = fixture(
            &path,
            vec![DiscoveryInput::Bootstrap {
                source: source.clone(),
                manifest: None,
                transport: transport.clone(),
            }],
            proof(),
        );
        owner.refresh(&discovery, NOW, &|| true).await.unwrap();
        assert_eq!(discovery.read().await.verified_relays().count(), 2);
    }
    transport.all_lost.store(true, Ordering::SeqCst);
    let possession = proof();
    let (mut owner, discovery, store) = fixture(
        &path,
        vec![DiscoveryInput::Bootstrap {
            source,
            manifest: None,
            transport,
        }],
        possession.clone(),
    );
    assert_eq!(discovery.read().await.verified_relays().count(), 0);
    owner.refresh(&discovery, NOW + 3, &|| true).await.unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 2);
    assert_eq!(possession.calls.load(Ordering::SeqCst), 2);
    let key = ReachabilitySequenceKeyV1 {
        kind: ReachabilitySequenceKindV1::RelayDescriptor,
        signer: *SigningKey::from_bytes(&[1; 32]).verifying_key().as_bytes(),
        scope: [0; 32],
    };
    assert!(
        store
            .compare_and_advance_sequence(
                key,
                None,
                1,
                *blake3::hash(&descriptor(1)).as_bytes(),
                NOW + 600
            )
            .is_err(),
        "normal admission still rejects replay"
    );
}

#[tokio::test]
async fn cancelled_generation_cannot_commit_and_next_round_recovers_budget() {
    let dir = tempfile::tempdir().unwrap();
    let current = Arc::new(AtomicBool::new(true));
    let possession = Arc::new(Possession {
        calls: AtomicUsize::new(0),
        revoke: Some(current.clone()),
    });
    let input = DiscoveryInput::RelayEndpoint {
        signed_descriptor: descriptor(1),
    };
    let (mut owner, discovery, store) =
        fixture(&dir.path().join("replay.redb"), vec![input], possession);
    owner
        .refresh(&discovery, NOW, &|| current.load(Ordering::SeqCst))
        .await
        .unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 0);
    assert!(store
        .cached_records(
            *principal_node_id(SigningKey::from_bytes(&[1; 32]).verifying_key().as_bytes())
                .as_bytes()
        )
        .unwrap()
        .is_empty());
    owner.replace_possession_for_test(proof());
    current.store(true, Ordering::SeqCst);
    owner.force_refresh_for_test();
    owner.refresh(&discovery, NOW + 2, &|| true).await.unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 1);
}

#[tokio::test]
async fn invalid_signature_and_expiry_never_create_usable_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut object = decode_reachability_object(&descriptor(1)).unwrap();
    if let ReachabilityObjectV1::RelayDescriptor(value) = &mut object {
        value.relay_signature[0] ^= 1;
    }
    let bytes = encode_reachability_object(&object).unwrap();
    let possession = proof();
    let (mut owner, discovery, _) = fixture(
        &dir.path().join("replay.redb"),
        vec![DiscoveryInput::RelayEndpoint {
            signed_descriptor: bytes,
        }],
        possession.clone(),
    );
    owner.refresh(&discovery, NOW, &|| true).await.unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 0);
    assert_ne!(owner.statuses.read().await[0].state, "usable");
    assert_eq!(possession.calls.load(Ordering::SeqCst), 0);
    let (mut owner, discovery, _) = fixture(
        &dir.path().join("expired.redb"),
        vec![DiscoveryInput::RelayEndpoint {
            signed_descriptor: descriptor(2),
        }],
        proof(),
    );
    owner
        .refresh(&discovery, NOW + 601, &|| true)
        .await
        .unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 0);
    assert_eq!(owner.statuses.read().await[0].admitted_records, 0);
}

#[test]
fn durable_unknown_request_prevents_new_sequence_and_mismatched_completion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay.redb");
    {
        let store = RedbReachabilityReplayStore::open(&path).unwrap();
        assert_eq!(
            store
                .prepare_local([7; 32], |sequence| Ok(sequence.to_be_bytes().to_vec()))
                .unwrap(),
            1u64.to_be_bytes()
        );
    }
    let store = RedbReachabilityReplayStore::open(&path).unwrap();
    assert!(store
        .prepare_local([7; 32], |_| panic!("must not mint another request"))
        .is_err());
    assert!(store.finish_local([7; 32], &[9]).is_err());
    store.finish_local([7; 32], &1u64.to_be_bytes()).unwrap();
    assert_eq!(
        store
            .prepare_local([7; 32], |sequence| Ok(sequence.to_be_bytes().to_vec()))
            .unwrap(),
        2u64.to_be_bytes()
    );
}

#[test]
fn source_inputs_are_bounded_and_duplicate_identities_fail_preflight() {
    let duplicate = || DiscoveryInput::RelayEndpoint {
        signed_descriptor: descriptor(1),
    };
    assert!(validate_inputs(&[duplicate(), duplicate()]).is_err());
    assert!(validate_inputs(&(0..9).map(|_| duplicate()).collect::<Vec<_>>()).is_err());
    assert!(validate_inputs(&[DiscoveryInput::ManualRelay {
        invitation: "x".repeat(22_001)
    }])
    .is_err());
}

#[tokio::test]
async fn hanging_learned_endpoint_does_not_starve_another_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let (source, transport) = bootstrap(dir.path(), true);
    transport.hang_first.store(true, Ordering::SeqCst);
    let input = DiscoveryInput::Bootstrap {
        source,
        manifest: Some(transport.manifest.clone()),
        transport: transport.clone(),
    };
    let (mut owner, discovery, _) = fixture(&dir.path().join("replay.redb"), vec![input], proof());
    let start = Instant::now();
    owner
        .refresh_until(&discovery, NOW, &|| true, start + Duration::from_secs(2))
        .await
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(discovery.read().await.verified_relays().count(), 1);
    assert_eq!(owner.statuses.read().await[0].state, "usable");
    assert!(transport
        .calls
        .lock()
        .unwrap()
        .contains(&"/learned-b".to_owned()));
}

#[tokio::test]
async fn invalid_source_does_not_poison_other_sources_and_expiry_prunes_learned_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut object = decode_reachability_object(&descriptor(1)).unwrap();
    if let ReachabilityObjectV1::RelayDescriptor(value) = &mut object {
        value.relay_signature[0] ^= 1;
    }
    let inputs = vec![
        DiscoveryInput::RelayEndpoint {
            signed_descriptor: encode_reachability_object(&object).unwrap(),
        },
        DiscoveryInput::ManualRelay {
            invitation: encode_manual_relay_invitation(&descriptor(2)).unwrap(),
        },
    ];
    let (mut owner, discovery, store) = fixture(&dir.path().join("replay.redb"), inputs, proof());
    owner.refresh(&discovery, NOW, &|| true).await.unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 1);
    assert_ne!(owner.statuses.read().await[0].state, "usable");
    assert_eq!(owner.statuses.read().await[1].state, "usable");
    owner.force_refresh_for_test();
    owner
        .refresh(&discovery, NOW + 601, &|| true)
        .await
        .unwrap();
    assert_eq!(discovery.read().await.verified_relays().count(), 0);
    assert_eq!(owner.statuses.read().await[1].admitted_records, 0);
    let key = ReachabilitySequenceKeyV1 {
        kind: ReachabilitySequenceKindV1::RelayDescriptor,
        signer: *SigningKey::from_bytes(&[2; 32]).verifying_key().as_bytes(),
        scope: [0; 32],
    };
    assert!(store
        .is_current(key, 1, *blake3::hash(&descriptor(2)).as_bytes())
        .unwrap());
}

struct PeerRecords;
impl ku_net::vnext_reachability_resolver::ReachabilityRecordSource for PeerRecords {
    fn fetch<'a>(
        &'a self,
        _: &'a ku_net::vnext_reachability_resolver::ReachabilityRecordQueryV1,
        _: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>> {
        Box::pin(async { Ok(vec![descriptor(1)]) })
    }
}

fn peer_invitation() -> String {
    let target = SigningKey::from_bytes(&[42; 32]);
    let relay = SigningKey::from_bytes(&[1; 32]);
    let mut reservation = RelayReservationV1 {
        format: 1,
        relay_node_id: principal_node_id(relay.verifying_key().as_bytes()),
        target_node_id: principal_node_id(target.verifying_key().as_bytes()),
        reservation_id: [8; 32],
        transport_scope: vec![RelayTransportV1::TlsTcp443],
        issued_at: NOW,
        expires_at: NOW + 500,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    reservation.target_signature = target
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(reservation.clone()),
                ReachabilitySignatureRoleV1::ReservationTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    reservation.relay_signature = relay
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayReservation(reservation.clone()),
                ReachabilitySignatureRoleV1::ReservationRelay,
            )
            .unwrap(),
        )
        .to_bytes();
    let mut advertisement = ReachabilityAdvertisementV1 {
        format: 1,
        target_node_id: reservation.target_node_id,
        relay_reservations: vec![reservation],
        optional_public_candidates: vec![],
        capability_ceiling: [9; 32],
        sequence: 1,
        issued_at: NOW,
        expires_at: NOW + 300,
        target_signature: [0; 64],
    };
    advertisement.target_signature = target
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::Advertisement(advertisement.clone()),
                ReachabilitySignatureRoleV1::AdvertisementTarget,
            )
            .unwrap(),
        )
        .to_bytes();
    encode_manual_peer_invitation(
        &KnownPeerIdentity::from_public_key(*target.verifying_key().as_bytes()),
        &encode_reachability_object(&ReachabilityObjectV1::Advertisement(advertisement)).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn manual_peer_checks_dual_signed_reservations_and_survives_restart_without_a_session_claim()
{
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay.redb");
    for _ in 0..2 {
        let input = DiscoveryInput::ManualPeer {
            invitation: peer_invitation(),
            source: Arc::new(PeerRecords),
        };
        let (mut owner, discovery, _) = fixture(&path, vec![input], proof());
        assert_eq!(discovery.read().await.verified_relays().count(), 0);
        owner.refresh(&discovery, NOW, &|| true).await.unwrap();
        assert_eq!(owner.statuses.read().await[0].state, "usable");
        assert_eq!(owner.statuses.read().await[0].admitted_records, 2);
        assert_eq!(discovery.read().await.verified_relays().count(), 1);
        owner.force_refresh_for_test();
        owner.refresh(&discovery, NOW + 1, &|| true).await.unwrap();
        assert_eq!(owner.statuses.read().await[0].state, "usable");
    }
}
