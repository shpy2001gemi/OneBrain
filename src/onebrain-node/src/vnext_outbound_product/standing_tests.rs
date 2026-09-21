use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};
use onebrain_protocol::{reachability_signing_bytes, relay_control_signing_bytes};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Default)]
struct Ports {
    active: Mutex<Vec<RelayReservationV1>>,
    requests: Mutex<Vec<RelayReserveRequestV1>>,
    published: Mutex<Vec<ReachabilityAdvertisementV1>>,
    fail_publish: AtomicBool,
    fail_reserve: AtomicBool,
    publish_calls: AtomicUsize,
    revoke: Option<Arc<AtomicBool>>,
}

impl StandingPorts for Ports {
    fn active(&self) -> ReachabilityFuture<'_, Vec<RelayReservationV1>> {
        Box::pin(async { self.active.lock().unwrap().clone() })
    }
    fn reserve<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        request: RelayReserveRequestV1,
        current: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(async move {
            let target = SigningKey::from_bytes(&[42; 32]);
            target
                .verifying_key()
                .verify(
                    &relay_control_signing_bytes(
                        &RelayControlV1::Reserve(request.clone()),
                        RelayControlSignatureRoleV1::ReserveRequestTarget,
                    )
                    .unwrap(),
                    &Signature::from_bytes(&request.target_request_signature),
                )
                .unwrap();
            self.requests.lock().unwrap().push(request.clone());
            if self.fail_reserve.load(Ordering::SeqCst) {
                return Err(ReachabilityError::Io);
            }
            if let Some(granted) = &self.revoke {
                granted.store(false, Ordering::SeqCst);
            }
            if !current() {
                return Err(ReachabilityError::NetworkChanged);
            }
            let mut value = RelayReservationV1 {
                format: 1,
                relay_node_id: request.relay_node_id,
                target_node_id: request.target_node_id,
                reservation_id: request.reservation_id,
                transport_scope: request.transport_scope.clone(),
                issued_at: request.issued_at,
                expires_at: request.expires_at,
                target_signature: request.target_reservation_signature,
                relay_signature: [0; 64],
            };
            target
                .verifying_key()
                .verify(
                    &reachability_signing_bytes(
                        &ReachabilityObjectV1::RelayReservation(value.clone()),
                        ReachabilitySignatureRoleV1::ReservationTarget,
                    )
                    .unwrap(),
                    &Signature::from_bytes(&value.target_signature),
                )
                .unwrap();
            let key = (1..=8)
                .map(|n| SigningKey::from_bytes(&[n; 32]))
                .find(|key| *key.verifying_key().as_bytes() == relay.canonical().relay_public_key)
                .unwrap();
            value.relay_signature = key
                .sign(
                    &reachability_signing_bytes(
                        &ReachabilityObjectV1::RelayReservation(value.clone()),
                        ReachabilitySignatureRoleV1::ReservationRelay,
                    )
                    .unwrap(),
                )
                .to_bytes();
            let mut active = self.active.lock().unwrap();
            active.retain(|entry| entry.relay_node_id != value.relay_node_id);
            active.push(value);
            Ok(())
        })
    }
    fn publish<'a>(
        &'a self,
        advertisement: &'a ReachabilityAdvertisementV1,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(async move {
            self.publish_calls.fetch_add(1, Ordering::SeqCst);
            SigningKey::from_bytes(&[42; 32])
                .verifying_key()
                .verify(
                    &reachability_signing_bytes(
                        &ReachabilityObjectV1::Advertisement(advertisement.clone()),
                        ReachabilitySignatureRoleV1::AdvertisementTarget,
                    )
                    .unwrap(),
                    &Signature::from_bytes(&advertisement.target_signature),
                )
                .unwrap();
            if self.fail_publish.load(Ordering::SeqCst) {
                return Err(ReachabilityError::Io);
            }
            self.published.lock().unwrap().push(advertisement.clone());
            Ok(())
        })
    }
}

fn owner(path: &std::path::Path) -> StandingOwner {
    StandingOwner::new(
        Arc::new(SigningKey::from_bytes(&[42; 32])),
        Arc::new(RedbReachabilityReplayStore::open(path).unwrap()),
        Some([7; 32]),
    )
}

#[tokio::test]
async fn standing_target_renewal_and_publication_require_independent_opt_in() {
    let dir = tempfile::tempdir().unwrap();
    let relays =
        super::super::discovery_tests::validated_relays(&dir.path().join("discovery.redb"), 4)
            .await;
    assert_eq!(relays.len(), 4);
    let mut owner = owner(&dir.path().join("standing.redb"));
    let ports = Ports::default();
    owner
        .maintain_with(&ports, relays.clone(), false, 10_000, &|| true)
        .await
        .unwrap();
    assert_eq!(ports.active.lock().unwrap().len(), 3);
    assert_eq!(ports.requests.lock().unwrap().len(), 3);
    assert_eq!(ports.publish_calls.load(Ordering::SeqCst), 0);
    assert_eq!(owner.advertisement_state, "disabled");
    owner
        .maintain_with(&ports, relays.clone(), true, 10_002, &|| true)
        .await
        .unwrap();
    assert_eq!(ports.requests.lock().unwrap().len(), 3);
    let first = ports.published.lock().unwrap()[0].clone();
    assert_eq!(first.sequence, 1);
    assert_eq!(first.relay_reservations.len(), 3);
    assert!(first.optional_public_candidates.is_empty());
    assert_eq!(owner.advertisement_state, "published");
    owner
        .maintain_with(&ports, relays.clone(), true, 10_003, &|| true)
        .await
        .unwrap();
    assert_eq!(
        ports.publish_calls.load(Ordering::SeqCst),
        1,
        "no publication storm"
    );
    owner
        .maintain_with(&ports, relays, true, 10_730, &|| true)
        .await
        .unwrap();
    assert_eq!(ports.requests.lock().unwrap().len(), 6);
    assert!(ports.requests.lock().unwrap()[3..]
        .iter()
        .all(|request| request.sequence == 2));
    assert_eq!(ports.published.lock().unwrap().last().unwrap().sequence, 2);
}

#[tokio::test]
async fn unknown_reservation_is_checkpointed_without_blind_replay_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let relays =
        super::super::discovery_tests::validated_relays(&dir.path().join("discovery.redb"), 1)
            .await;
    let path = dir.path().join("standing.redb");
    let ports = Ports::default();
    ports.fail_reserve.store(true, Ordering::SeqCst);
    {
        let mut owner = owner(&path);
        owner
            .maintain_with(&ports, relays.clone(), true, 10_000, &|| true)
            .await
            .unwrap();
        assert_eq!(owner.limitation, Some("reservation_reconcile_required"));
    }
    ports.fail_reserve.store(false, Ordering::SeqCst);
    let mut owner = owner(&path);
    owner
        .maintain_with(&ports, relays, true, 10_001, &|| true)
        .await
        .unwrap();
    assert_eq!(ports.requests.lock().unwrap().len(), 1);
    assert_eq!(ports.publish_calls.load(Ordering::SeqCst), 0);
    assert_eq!(owner.limitation, Some("reservation_reconcile_required"));
}

#[tokio::test]
async fn failed_publication_reuses_exact_bytes_but_never_stale_reservations() {
    let dir = tempfile::tempdir().unwrap();
    let relays =
        super::super::discovery_tests::validated_relays(&dir.path().join("discovery.redb"), 2)
            .await;
    let path = dir.path().join("standing.redb");
    let ports = Ports::default();
    ports.fail_publish.store(true, Ordering::SeqCst);
    let mut state = owner(&path);
    state
        .maintain_with(&ports, relays.clone(), true, 10_000, &|| true)
        .await
        .unwrap();
    assert_eq!(state.advertisement_state, "degraded");
    let scope = local_scope(b"advertisement", &state.signer.public_key());
    let exact = state.replay.pending_local(scope).unwrap().unwrap();
    ports.fail_publish.store(false, Ordering::SeqCst);
    drop(state);
    let mut state = owner(&path);
    state
        .maintain_with(&ports, relays.clone(), true, 10_001, &|| true)
        .await
        .unwrap();
    assert_eq!(
        encode_reachability_object(&ReachabilityObjectV1::Advertisement(
            ports.published.lock().unwrap()[0].clone()
        ))
        .unwrap(),
        exact
    );
    ports.fail_publish.store(true, Ordering::SeqCst);
    state
        .maintain_with(&ports, relays.clone(), true, 10_281, &|| true)
        .await
        .unwrap();
    let calls = ports.publish_calls.load(Ordering::SeqCst);
    ports.active.lock().unwrap().clear();
    state
        .maintain_with(&ports, relays, true, 10_282, &|| true)
        .await
        .unwrap();
    assert_eq!(ports.publish_calls.load(Ordering::SeqCst), calls);
    assert_eq!(state.limitation, Some("advertisement_reconcile_required"));
}

#[tokio::test]
async fn revoked_grant_cannot_create_active_reservation_or_publish() {
    let dir = tempfile::tempdir().unwrap();
    let relays =
        super::super::discovery_tests::validated_relays(&dir.path().join("discovery.redb"), 2)
            .await;
    let current = Arc::new(AtomicBool::new(true));
    let ports = Ports {
        revoke: Some(current.clone()),
        ..Ports::default()
    };
    let mut owner = owner(&dir.path().join("standing.redb"));
    assert!(owner
        .maintain_with(&ports, relays, true, 10_000, &|| current
            .load(Ordering::SeqCst))
        .await
        .is_err());
    assert!(ports.active.lock().unwrap().is_empty());
    assert_eq!(ports.publish_calls.load(Ordering::SeqCst), 0);
}
