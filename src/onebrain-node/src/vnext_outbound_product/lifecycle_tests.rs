use super::tests::dependencies;
use super::*;
use crate::vnext_reachability_manager::*;
use ku_net::vnext_reachability_crypto::{
    ReachabilityNonceDomainV1, ReachabilityReplayStore, SystemPublicEndpointResolver,
};
use ku_net::vnext_relay_discovery::ReachabilityFuture;
use onebrain_protocol::ReachabilityAdvertisementV1;
use std::sync::atomic::AtomicUsize;

struct Probe {
    calls: AtomicUsize,
    live: AtomicUsize,
    entered: Notify,
    release: Notify,
    hang: bool,
    fail: bool,
}

struct InFlight<'a>(&'a AtomicUsize);
impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl CandidateGatherer for Probe {
    fn gather(
        &self,
        epoch: NetworkEpoch,
    ) -> ReachabilityFuture<'_, Result<GatheredCandidates, ReachabilityError>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.live.fetch_add(1, Ordering::SeqCst);
            let _in_flight = InFlight(&self.live);
            self.entered.notify_one();
            if self.hang {
                self.release.notified().await;
            }
            if self.fail {
                return Err(ReachabilityError::Io);
            }
            Ok(GatheredCandidates {
                private: PrivateCandidateSet::local(vec![], epoch)?,
                public: vec![],
                direct: vec![],
                relay: vec![],
                epoch,
                observed_at: 1,
            })
        })
    }
}

struct NeverPublish;
impl AdvertisementPublisher for NeverPublish {
    fn publish<'a>(
        &'a self,
        _: &'a ReachabilityAdvertisementV1,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        panic!("lifecycle must not publish an advertisement")
    }
}

fn ports(hang: bool, fail: bool) -> (OutboundFirstDependencies, Arc<Probe>, watch::Sender<bool>) {
    let probe = Arc::new(Probe {
        calls: AtomicUsize::new(0),
        live: AtomicUsize::new(0),
        entered: Notify::new(),
        release: Notify::new(),
        hang,
        fail,
    });
    let (grant, receiver) = watch::channel(true);
    let ports = OutboundFirstDependencies::new(
        probe.clone(),
        Arc::new(NeverPublish),
        Arc::new(SystemPublicEndpointResolver::new(4).unwrap()),
        receiver,
    );
    (ports, probe, grant)
}

fn config() -> VNextFeatureConfig {
    let mut config = VNextFeatureConfig::default();
    config.enabled.object_event_v1 = true;
    config.enabled.obp_rp = true;
    config
}

struct PendingDiscovery {
    entered: Notify,
    live: AtomicUsize,
}

impl crate::vnext_outbound_product::DiscoveryRecordSource for PendingDiscovery {
    fn fetch(
        &self,
        _: ku_net::vnext_relay_discovery::SourceBudget,
    ) -> ReachabilityFuture<
        '_,
        Result<Vec<Vec<u8>>, ku_net::vnext_relay_discovery::RelayDiscoveryLimitation>,
    > {
        Box::pin(async {
            self.live.fetch_add(1, Ordering::SeqCst);
            let _guard = InFlight(&self.live);
            self.entered.notify_one();
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn discovery_snapshots_do_not_wait_on_io_and_grant_revocation_cancels_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = Arc::new(PendingDiscovery {
        entered: Notify::new(),
        live: AtomicUsize::new(0),
    });
    let (ports, _, grant) = ports(false, false);
    let ports = ports
        .with_discovery(vec![
            crate::vnext_outbound_product::DiscoveryInput::Rendezvous {
                relay: NodeId::from_bytes([33; 32]),
                source: source.clone(),
            },
        ])
        .unwrap();
    let mut runtime = start(dir.path(), Some(ports)).await;
    tokio::time::timeout(Duration::from_secs(3), source.entered.notified())
        .await
        .unwrap();
    let services = runtime.services();
    let sources = tokio::time::timeout(
        Duration::from_millis(500),
        services.outbound_first_sources(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].admitted_records, 0);
    assert_eq!(
        services.outbound_first_status().await.unwrap().source_count,
        1
    );
    assert!(services
        .outbound_first_reservations()
        .await
        .unwrap()
        .is_empty());
    grant.send_replace(false);
    tokio::time::timeout(Duration::from_secs(2), async {
        while source.live.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        services.outbound_first_sources().await.unwrap()[0].state,
        "disabled"
    );
    runtime.shutdown().await;
    assert!(services.outbound_first_sources().await.is_err());
    assert!(services.outbound_first_reservations().await.is_err());
}

async fn start(path: &Path, ports: Option<OutboundFirstDependencies>) -> VNextProductRuntime {
    let deps = match ports {
        Some(ports) => dependencies(42).with_outbound_first(ports),
        None => dependencies(42),
    };
    VNextProductRuntime::start(
        path,
        "127.0.0.1:0".parse().unwrap(),
        &config(),
        deps,
        Some(Arc::new(ed25519_dalek::SigningKey::from_bytes(&[17; 32]))),
    )
    .await
    .unwrap()
}

async fn entered(probe: &Probe) {
    tokio::time::timeout(Duration::from_secs(5), probe.entered.notified())
        .await
        .unwrap();
}

#[tokio::test]
async fn default_has_no_reachability_store_or_worker() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = start(dir.path(), None).await;
    assert_eq!(runtime.workers.len(), 0);
    assert!(!dir.path().join("vnext_reachability_replay.redb").exists());
    let status = runtime.services().outbound_first_status().await.unwrap();
    assert!(!status.requested && !status.active);
    assert_eq!(status.lifecycle, "disabled");
    runtime.shutdown().await;
}

#[tokio::test]
async fn owner_uses_one_worker_and_shutdown_cancels_pending_port() {
    let dir = tempfile::tempdir().unwrap();
    let (ports, probe, _grant) = ports(true, false);
    let mut runtime = start(dir.path(), Some(ports.with_advertising(true))).await;
    let services = runtime.services();
    entered(&probe).await;
    assert_eq!(runtime.workers.len(), 1);
    let status = services.outbound_first_status().await.unwrap();
    assert!(status.active);
    assert_eq!(status.lifecycle, "degraded");
    assert_eq!(status.advertisement_state, "pending");
    assert_eq!(status.usable_reservations, 0);
    assert!(!status.claims_global_completion && !status.authorizes_reward);
    let addr = runtime.local_addr;
    tokio::time::timeout(Duration::from_secs(2), runtime.shutdown())
        .await
        .unwrap();
    assert_eq!(probe.live.load(Ordering::SeqCst), 0);
    assert!(matches!(
        services.outbound_first_status().await,
        Err(VNextProductRuntimeError::Stopped)
    ));
    assert_socket_released(addr).await;
    assert!(dir.path().join("vnext_reachability_replay.redb").exists());
}

#[tokio::test]
async fn preflight_rejects_missing_grant_and_feature_before_files() {
    for missing_grant in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (ports, probe, grant) = ports(false, false);
        let mut config = config();
        if missing_grant {
            grant.send_replace(false);
        } else {
            config.enabled.obp_rp = false;
        }
        let result = VNextProductRuntime::start(
            dir.path(),
            "127.0.0.1:0".parse().unwrap(),
            &config,
            dependencies(42).with_outbound_first(ports),
            None,
        )
        .await;
        assert!(matches!(
            result,
            Err(VNextProductRuntimeError::OutboundFirst(_))
        ));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    }
    struct NoPaths;
    impl DatasetPathResolver for NoPaths {
        fn current_generation(&self) -> crate::dataset_path::DatasetGenerationId {
            crate::dataset_path::DatasetGenerationId::BOOTSTRAP
        }
        fn owner_path(
            &self,
            _: BaseStorageOwnerId,
        ) -> Result<PathBuf, ku_kql::blob_storage::BlobStorageError> {
            panic!("invalid request must be rejected before resolving/creating owner paths")
        }
    }
    let (ports, _, grant) = ports(false, false);
    grant.send_replace(false);
    let result = VNextProductRuntime::start_in_dataset(
        &NoPaths,
        "127.0.0.1:0".parse().unwrap(),
        &config(),
        dependencies(42).with_outbound_first(ports),
        None,
    )
    .await;
    assert!(matches!(
        result,
        Err(VNextProductRuntimeError::OutboundFirst(_))
    ));
}

#[tokio::test]
async fn grant_revocation_cancels_pending_collection() {
    let dir = tempfile::tempdir().unwrap();
    let (ports, probe, grant) = ports(true, false);
    let mut runtime = start(dir.path(), Some(ports)).await;
    entered(&probe).await;
    grant.send_replace(false);
    tokio::time::timeout(Duration::from_secs(2), async {
        while probe.live.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        !runtime
            .services()
            .outbound_first_status()
            .await
            .unwrap()
            .active
    );
    runtime.shutdown().await;
}

#[tokio::test]
async fn durable_kill_and_replay_survive_restart_without_live_reservations() {
    let dir = tempfile::tempdir().unwrap();
    let replay_path = dir.path().join("vnext_reachability_replay.redb");
    {
        let replay = RedbReachabilityReplayStore::open(&replay_path).unwrap();
        replay
            .consume_nonce(
                ReachabilityNonceDomainV1::RelayControl,
                [1; 32],
                [2; 32],
                100,
            )
            .unwrap();
    }
    let (first_ports, first_probe, _first_grant) = ports(false, false);
    let mut runtime = start(dir.path(), Some(first_ports)).await;
    entered(&first_probe).await;
    let killed = runtime
        .services()
        .kill_runtime_lane(VNextRuntimeLane::Network)
        .unwrap();
    let generation = killed.lane(VNextRuntimeLane::Network).generation;
    runtime.shutdown().await;
    let (next_ports, probe, _grant) = ports(false, false);
    let mut restarted = start(dir.path(), Some(next_ports)).await;
    let status = restarted.services().outbound_first_status().await.unwrap();
    assert!(status.kill_switch && !status.active);
    assert_eq!(status.generation, generation);
    assert_eq!(status.usable_reservations, 0);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    restarted
        .services()
        .reenable_runtime_lane(VNextRuntimeLane::Network)
        .unwrap();
    entered(&probe).await;
    restarted.shutdown().await;
    let replay = RedbReachabilityReplayStore::open(&replay_path).unwrap();
    assert!(replay
        .consume_nonce(
            ReachabilityNonceDomainV1::RelayControl,
            [1; 32],
            [2; 32],
            100
        )
        .is_err());
}

#[tokio::test]
async fn candidate_failure_is_degraded_and_worker_remains_owned() {
    let dir = tempfile::tempdir().unwrap();
    let (ports, probe, _grant) = ports(false, true);
    let mut runtime = start(dir.path(), Some(ports)).await;
    entered(&probe).await;
    assert!(runtime
        .services()
        .outbound_first_status()
        .await
        .unwrap()
        .limitations
        .contains(&"candidate_collection_unavailable"));
    entered(&probe).await;
    assert_eq!(runtime.workers.len(), 1);
    runtime.shutdown().await;
}

#[tokio::test]
async fn rollback_removes_only_new_reachability_artifacts() {
    for preexisting in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let replay_path = dir.path().join("vnext_reachability_replay.redb");
        if preexisting {
            let store = RedbReachabilityReplayStore::open(&replay_path).unwrap();
            store
                .consume_nonce(
                    ReachabilityNonceDomainV1::RelayControl,
                    [1; 32],
                    [2; 32],
                    100,
                )
                .unwrap();
        }
        let (ports, probe, _grant) = ports(true, false);
        let runtime = start(dir.path(), Some(ports)).await;
        entered(&probe).await;
        let addr = runtime.local_addr;
        runtime.rollback_startup().await.unwrap();
        assert_eq!(probe.live.load(Ordering::SeqCst), 0);
        assert_eq!(replay_path.exists(), preexisting);
        assert!(!dir.path().join("vnext_route_journal.redb").exists());
        assert!(!dir.path().join("vnext_outbox.redb").exists());
        assert_socket_released(addr).await;
        if preexisting {
            let store = RedbReachabilityReplayStore::open(&replay_path).unwrap();
            assert!(store
                .consume_nonce(
                    ReachabilityNonceDomainV1::RelayControl,
                    [1; 32],
                    [2; 32],
                    100
                )
                .is_err());
        }
    }
}

#[tokio::test]
async fn corrupt_journal_fails_before_bind_without_leaking_new_replay_or_socket() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("vnext_route_journal.redb");
    std::fs::write(&journal, b"preserve-corrupt-journal").unwrap();
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = socket.local_addr().unwrap();
    drop(socket);
    let (ports, probe, _grant) = ports(false, false);
    let result = VNextProductRuntime::start(
        dir.path(),
        addr,
        &config(),
        dependencies(42).with_outbound_first(ports),
        Some(Arc::new(ed25519_dalek::SigningKey::from_bytes(&[17; 32]))),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    assert!(!dir.path().join("vnext_reachability_replay.redb").exists());
    assert!(!dir.path().join("vnext_outbox.redb").exists());
    assert_eq!(
        std::fs::read(&journal).unwrap(),
        b"preserve-corrupt-journal"
    );
    assert_socket_released(addr).await;
}

#[tokio::test]
async fn journal_recovery_does_not_restore_an_authenticated_route() {
    use crate::vnext_route_journal::{RouteJournal, RouteJournalEntryV1};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vnext_route_journal.redb");
    let root = {
        let journal = RouteJournal::open(&path, 4096, 16 * 1024 * 1024).unwrap();
        journal
            .append(RouteJournalEntryV1::direct(
                NodeId::from_bytes([3; 32]),
                [4; 32],
                1,
                1,
            ))
            .unwrap();
        journal.root().unwrap()
    };
    let (ports, _, _grant) = ports(false, false);
    let runtime = start(dir.path(), Some(ports)).await;
    let services = runtime.services();
    let locked_owner = tokio::sync::Mutex::new(runtime);
    let mut guard = locked_owner.lock().await;
    let status = tokio::time::timeout(Duration::from_secs(2), services.outbound_first_status())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(status.authenticated_routes, 0);
    assert_eq!(status.usable_reservations, 0);
    guard.shutdown().await;
    let journal = RouteJournal::open(&path, 4096, 16 * 1024 * 1024).unwrap();
    assert_eq!(journal.root().unwrap(), root);
}

// Quinn's endpoint driver owns the UDP handle until it processes its drop wake.
// Bound the actual rebind check instead of assuming synchronous OS release.
async fn assert_socket_released(addr: SocketAddr) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match std::net::UdpSocket::bind(addr) {
                Ok(socket) => {
                    drop(socket);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                Err(error) => panic!("unexpected rebind failure: {error}"),
            }
        }
    })
    .await
    .expect("QUIC socket orphaned after shutdown");
}

#[tokio::test]
async fn stale_epoch_and_generation_results_are_not_projected_as_success() {
    let dir = tempfile::tempdir().unwrap();
    let (ports, probe, _grant) = ports(true, false);
    let mut runtime = start(dir.path(), Some(ports)).await;
    entered(&probe).await;
    {
        let owner = runtime
            .core
            .as_ref()
            .unwrap()
            .outbound_first
            .lock()
            .unwrap()
            .clone()
            .unwrap();
        owner.manager.advance_network_epoch().unwrap();
    }
    probe.release.notify_one();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if runtime
                .services()
                .outbound_first_status()
                .await
                .unwrap()
                .limitations
                .contains(&"candidate_collection_unavailable")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    entered(&probe).await;
    let services = runtime.services();
    services
        .kill_runtime_lane(VNextRuntimeLane::Network)
        .unwrap();
    services
        .reenable_runtime_lane(VNextRuntimeLane::Network)
        .unwrap();
    probe.release.notify_one();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if services
                .outbound_first_status()
                .await
                .unwrap()
                .limitations
                .contains(&"network_generation_changed")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.shutdown().await;
}

#[tokio::test]
async fn dropping_owner_aborts_pending_worker_and_releases_transport() {
    let dir = tempfile::tempdir().unwrap();
    let (ports, probe, _grant) = ports(true, false);
    let runtime = start(dir.path(), Some(ports)).await;
    entered(&probe).await;
    let addr = runtime.local_addr;
    let services = runtime.services();
    drop(runtime);
    assert_socket_released(addr).await;
    assert_eq!(probe.live.load(Ordering::SeqCst), 0);
    assert!(matches!(
        services.outbound_first_status().await,
        Err(VNextProductRuntimeError::Stopped)
    ));
    let _reopened =
        RedbReachabilityReplayStore::open(dir.path().join("vnext_reachability_replay.redb"))
            .unwrap();
}
