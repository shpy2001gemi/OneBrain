use ku_core::foundation::NodeId;
use ku_net::vnext_relay_reservation::{RelayReservationBounds, StandingRelaySet};

fn node(byte: u8) -> NodeId {
    NodeId::from_bytes([byte; 32])
}

#[test]
fn minimum_target_and_maximum_reservations_are_closed_and_distinct() {
    let bounds = RelayReservationBounds::new(2, 3, 3).unwrap();
    let mut set = StandingRelaySet::new(bounds);
    assert!(set.insert(node(1), 30).unwrap());
    assert!(set.insert(node(2), 20).unwrap());
    assert!(set.insert(node(3), 10).unwrap());
    assert!(!set.insert(node(3), 99).unwrap());
    assert!(set.insert(node(4), 40).is_err());
    assert!(set.has_minimum());
    assert!(set.has_target());
    assert_eq!(set.relay_ids(), vec![node(1), node(2), node(3)]);
}

#[test]
fn on_demand_target_relay_is_bounded_and_in_use_is_not_evicted() {
    let bounds = RelayReservationBounds::new(2, 3, 3).unwrap();
    let mut set = StandingRelaySet::new(bounds);
    set.insert(node(1), 30).unwrap();
    set.insert(node(2), 20).unwrap();
    set.insert(node(3), 10).unwrap();
    set.mark_in_use(node(3), true).unwrap();
    assert_eq!(set.ensure_on_demand(node(4), 40).unwrap(), node(4));
    assert!(!set.contains(node(2)));
    assert!(set.contains(node(3)));
    assert!(set.contains(node(4)));
}
