use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};

use ku_net::vnext_reachability_resolver::{
    ReachabilityAdvertisementResolver, ReachabilityRecordQueryV1, ReachabilityRecordSource,
};
use ku_net::vnext_relay_discovery::{
    ReachabilityFuture, RelayDiscoveryLimitation, RelayDiscoveryPolicy, SourceBudget,
};
use ku_net::vnext_session::principal_node_id;

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

struct StaticSource(Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>);

impl ReachabilityRecordSource for StaticSource {
    fn fetch<'a>(
        &'a self,
        _query: &'a ReachabilityRecordQueryV1,
        _budget: SourceBudget,
    ) -> ReachabilityFuture<'a, Result<Vec<Vec<u8>>, RelayDiscoveryLimitation>> {
        Box::pin(async move { self.0.clone() })
    }
}

#[test]
fn resolver_merges_bounded_sources_and_ignores_transport_failures() {
    let resolver = ReachabilityAdvertisementResolver::new(
        vec![
            Arc::new(StaticSource(Err(RelayDiscoveryLimitation::PoisonedSource))),
            Arc::new(StaticSource(Ok(vec![vec![1, 2, 3]]))),
        ],
        RelayDiscoveryPolicy::default(),
    );
    let records = block_on_ready(resolver.fetch_records(
        &ReachabilityRecordQueryV1::RelayDescriptor {
            relay: principal_node_id(&[1; 32]),
        },
        Instant::now() + Duration::from_secs(1),
    ))
    .unwrap();
    assert_eq!(records, vec![vec![1, 2, 3]]);
}

#[test]
fn resolver_reports_no_bootstrap_when_every_source_fails_and_caps_output() {
    let resolver = ReachabilityAdvertisementResolver::new(
        vec![Arc::new(StaticSource(Err(
            RelayDiscoveryLimitation::Deadline,
        )))],
        RelayDiscoveryPolicy::default(),
    );
    assert_eq!(
        block_on_ready(resolver.fetch_records(
            &ReachabilityRecordQueryV1::PeerAdvertisement {
                target: principal_node_id(&[2; 32]),
            },
            Instant::now(),
        )),
        Err(RelayDiscoveryLimitation::NoBootstrapReachable)
    );

    let policy = RelayDiscoveryPolicy::default();
    let resolver = ReachabilityAdvertisementResolver::new(
        vec![Arc::new(StaticSource(Ok(vec![vec![1]; 65])))],
        policy,
    );
    assert_eq!(
        block_on_ready(resolver.fetch_records(
            &ReachabilityRecordQueryV1::RelayDescriptor {
                relay: principal_node_id(&[3; 32]),
            },
            Instant::now() + Duration::from_secs(1),
        )),
        Err(RelayDiscoveryLimitation::RecordLimit)
    );
}
