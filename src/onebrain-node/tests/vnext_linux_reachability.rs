#![cfg(feature = "vnext-outbound-first")]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use onebrain_node::vnext_linux_candidate_gatherer::LinuxCandidateGatherer;
use onebrain_node::vnext_linux_candidate_gatherer::{
    filter_host_addresses, InterfaceAddressObservation,
};
use onebrain_node::vnext_linux_network_epoch::network_snapshot_digest;
#[cfg(not(target_os = "linux"))]
use onebrain_node::vnext_reachability_manager::ReachabilityError;
use onebrain_node::vnext_reachability_manager::{CandidateGatherer, NetworkEpoch};

#[test]
fn getifaddrs_filter_keeps_only_up_non_loopback_unicast_addresses() {
    let observations = vec![
        InterfaceAddressObservation::new(IpAddr::V4(Ipv4Addr::LOCALHOST), true, false),
        InterfaceAddressObservation::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), true, false),
        InterfaceAddressObservation::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)), false, false),
    ];
    assert_eq!(
        filter_host_addresses(&observations),
        vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))]
    );
}

#[test]
fn address_and_route_changes_advance_the_snapshot_digest() {
    let before = network_snapshot_digest(&[b"10.0.0.2"], b"route-a", b"route6-a");
    let address_changed = network_snapshot_digest(&[b"10.0.0.3"], b"route-a", b"route6-a");
    let route_changed = network_snapshot_digest(&[b"10.0.0.2"], b"route-b", b"route6-a");
    assert_ne!(before, address_changed);
    assert_ne!(before, route_changed);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn production_linux_getifaddrs_gathering_is_bounded_and_epoch_bound() {
    let gatherer = LinuxCandidateGatherer::new(41_000, Arc::new(|| 100)).unwrap();
    let gathered = gatherer.gather(NetworkEpoch::initial()).await.unwrap();
    assert_eq!(gathered.epoch, NetworkEpoch::initial());
    assert!(gathered.direct.len() <= 8);
    assert_eq!(gathered.direct.len(), gathered.private.candidates().len());
    assert!(gathered
        .direct
        .iter()
        .all(|candidate| candidate.network_epoch == NetworkEpoch::initial().get()));
}

#[cfg(not(target_os = "linux"))]
#[tokio::test]
async fn production_linux_constructor_fails_typed_on_other_platforms() {
    let gatherer = LinuxCandidateGatherer::new(41_000, Arc::new(|| 100)).unwrap();
    assert!(matches!(
        gatherer.gather(NetworkEpoch::initial()).await,
        Err(ReachabilityError::UnsupportedPlatform)
    ));
}
