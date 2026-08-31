#![cfg(feature = "vnext-outbound-first")]

use std::sync::{Arc, Barrier};
use std::time::Duration;

use ku_net::vnext_reachability_crypto::{
    ReachabilityNonceDomainV1, ReachabilityReplayStore, ReachabilitySequenceKeyV1,
    ReachabilitySequenceKindV1, RelayAdmissionError,
};
use onebrain_node::vnext_reachability_manager::{NetworkEpoch, VNextReachabilityPolicy};
use onebrain_node::vnext_reachability_replay_store::RedbReachabilityReplayStore;

#[test]
fn runtime_policy_accepts_only_the_frozen_reservation_and_resource_bounds() {
    let policy = VNextReachabilityPolicy::default();
    policy.validate().unwrap();
    assert_eq!(policy.min_relay_reservations, 2);
    assert_eq!(policy.target_relay_reservations, 3);
    assert_eq!(policy.max_relay_reservations, 3);
    let mut invalid = policy;
    invalid.keepalive_interval = Duration::ZERO;
    assert!(invalid.validate().is_err());
}

#[test]
fn network_epoch_is_monotonic_and_local_start_does_not_require_discovery() {
    let first = NetworkEpoch::initial();
    assert_eq!(first.get(), 1);
    assert_eq!(first.next().unwrap().get(), 2);
}

#[test]
fn replay_sequence_and_nonce_floors_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reachability.redb");
    let key = ReachabilitySequenceKeyV1 {
        kind: ReachabilitySequenceKindV1::RelayKeepalive,
        signer: [7; 32],
        scope: [8; 32],
    };
    {
        let store = RedbReachabilityReplayStore::open(&path).unwrap();
        store
            .check_and_advance_sequence(key, 1, [9; 32], 500)
            .unwrap();
        store
            .consume_nonce(
                ReachabilityNonceDomainV1::RelayControl,
                [10; 32],
                [11; 32],
                500,
            )
            .unwrap();
    }
    let reopened = RedbReachabilityReplayStore::open(&path).unwrap();
    assert_eq!(
        reopened.check_and_advance_sequence(key, 1, [9; 32], 500),
        Err(RelayAdmissionError::Replay)
    );
    assert_eq!(
        reopened.consume_nonce(
            ReachabilityNonceDomainV1::RelayControl,
            [10; 32],
            [11; 32],
            500,
        ),
        Err(RelayAdmissionError::ChallengeConsumed)
    );
}

#[test]
fn sequence_compare_and_advance_is_atomic_under_a_racing_fork() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(
        RedbReachabilityReplayStore::open(directory.path().join("reachability.redb")).unwrap(),
    );
    let key = ReachabilitySequenceKeyV1 {
        kind: ReachabilitySequenceKindV1::RelayDescriptor,
        signer: [21; 32],
        scope: [22; 32],
    };
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for digest in [[31; 32], [32; 32]] {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            store.compare_and_advance_sequence(key, None, 1, digest, 500)
        }));
    }
    barrier.wait();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
}
