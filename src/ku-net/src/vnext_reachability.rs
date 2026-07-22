//! Local derived reachability observations. No island/component identity exists.

use std::collections::{BTreeMap, BTreeSet};

use ku_core::foundation::{Budget, CarrierProfile, EventCid, NodeId, SelectorCid};

use crate::vnext_session::AuthenticatedSession;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationInterval {
    pub start_tick: u64,
    pub end_tick: u64,
}

impl ObservationInterval {
    pub fn new(start_tick: u64, end_tick: u64) -> Result<Self, ReachabilityError> {
        if end_tick < start_tick {
            return Err(ReachabilityError::InvalidObservationInterval);
        }
        Ok(Self {
            start_tick,
            end_tick,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorFrontierObservation {
    pub selector: SelectorCid,
    pub inventory_root: [u8; 32],
    pub event_frontier: Vec<EventCid>,
    pub offered_budget: Budget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarrierPathObservation {
    pub path_commitment: [u8; 32],
    pub carrier: CarrierProfile,
    pub interval: ObservationInterval,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerReachabilityObservation {
    peer: NodeId,
    session_id: [u8; 32],
    selectors: Vec<SelectorFrontierObservation>,
    paths: Vec<CarrierPathObservation>,
}

impl PeerReachabilityObservation {
    pub fn from_authenticated_session(
        session: &AuthenticatedSession,
        remote_is_responder: bool,
        selectors: Vec<SelectorFrontierObservation>,
        paths: Vec<CarrierPathObservation>,
    ) -> Result<Self, ReachabilityError> {
        if paths.is_empty() {
            return Err(ReachabilityError::MissingCarrierPath);
        }
        if selectors
            .iter()
            .any(|selector| selector.inventory_root == [0; 32])
        {
            return Err(ReachabilityError::InvalidInventoryRoot);
        }
        let peer = if remote_is_responder {
            session.responder
        } else {
            session.initiator
        };
        Ok(Self {
            peer,
            session_id: session.session_id,
            selectors,
            paths,
        })
    }

    pub const fn peer(&self) -> NodeId {
        self.peer
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedRendezvousHint {
    pub address_commitment: [u8; 32],
    pub observed_tick: u64,
}

impl SeedRendezvousHint {
    pub const fn grants_authority(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReachabilityLimitation {
    NoAuthenticatedPeer,
    SeedUnavailable,
    OneWayPath,
    StoreCarryForwardOnly,
    FrontierUnobserved,
    BudgetBound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachabilityMode {
    Standalone,
    ComponentReachable,
    PathLimited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectorReachability {
    pub selector: SelectorCid,
    pub peer_roots: Vec<(NodeId, [u8; 32])>,
    pub event_frontier: Vec<EventCid>,
    pub peer_budgets: Vec<(NodeId, Budget)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachableCarrierPath {
    pub peer: NodeId,
    pub path_commitment: [u8; 32],
    pub carrier: CarrierProfile,
    pub interval: ObservationInterval,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachabilityView {
    pub local_node: NodeId,
    pub peer_digest: [u8; 32],
    pub authenticated_peers: Vec<NodeId>,
    pub selectors: Vec<SelectorReachability>,
    pub carrier_paths: Vec<ReachableCarrierPath>,
    pub observation_interval: ObservationInterval,
    pub limitations: Vec<ReachabilityLimitation>,
    pub seed_hints: Vec<SeedRendezvousHint>,
}

impl ReachabilityView {
    pub fn derive(
        local_node: NodeId,
        observations: &[PeerReachabilityObservation],
        observation_interval: ObservationInterval,
        mut seed_hints: Vec<SeedRendezvousHint>,
    ) -> Result<Self, ReachabilityError> {
        let mut peers = observations
            .iter()
            .map(|observation| observation.peer)
            .collect::<Vec<_>>();
        peers.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        peers.dedup();

        let mut selector_map = BTreeMap::<[u8; 32], SelectorReachability>::new();
        let mut carrier_paths = Vec::new();
        let mut limitations = BTreeSet::new();
        for observation in observations {
            for selector in &observation.selectors {
                let entry = selector_map
                    .entry(*selector.selector.as_bytes())
                    .or_insert_with(|| SelectorReachability {
                        selector: selector.selector,
                        peer_roots: Vec::new(),
                        event_frontier: Vec::new(),
                        peer_budgets: Vec::new(),
                    });
                entry
                    .peer_roots
                    .push((observation.peer, selector.inventory_root));
                entry
                    .peer_budgets
                    .push((observation.peer, selector.offered_budget));
                entry
                    .event_frontier
                    .extend(selector.event_frontier.iter().copied());
                limitations.insert(ReachabilityLimitation::BudgetBound);
                if selector.event_frontier.is_empty() {
                    limitations.insert(ReachabilityLimitation::FrontierUnobserved);
                }
            }
            for path in &observation.paths {
                if !path.carrier.bidirectional {
                    limitations.insert(ReachabilityLimitation::OneWayPath);
                }
                if path.carrier.store_carry_forward && !path.carrier.bidirectional {
                    limitations.insert(ReachabilityLimitation::StoreCarryForwardOnly);
                }
                carrier_paths.push(ReachableCarrierPath {
                    peer: observation.peer,
                    path_commitment: path.path_commitment,
                    carrier: path.carrier,
                    interval: path.interval,
                });
            }
        }
        if observations.is_empty() {
            limitations.insert(ReachabilityLimitation::NoAuthenticatedPeer);
        }
        if seed_hints.is_empty() {
            limitations.insert(ReachabilityLimitation::SeedUnavailable);
        }

        for selector in selector_map.values_mut() {
            selector.peer_roots.sort_by(|left, right| {
                left.0
                    .as_bytes()
                    .cmp(right.0.as_bytes())
                    .then_with(|| left.1.cmp(&right.1))
            });
            selector.peer_roots.dedup();
            selector.peer_budgets.sort_by(|left, right| {
                left.0
                    .as_bytes()
                    .cmp(right.0.as_bytes())
                    .then_with(|| budget_key(left.1).cmp(&budget_key(right.1)))
            });
            selector.peer_budgets.dedup();
            selector
                .event_frontier
                .sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            selector.event_frontier.dedup();
        }
        carrier_paths.sort_by(|left, right| {
            left.peer
                .as_bytes()
                .cmp(right.peer.as_bytes())
                .then_with(|| left.path_commitment.cmp(&right.path_commitment))
        });
        carrier_paths.dedup();
        seed_hints.sort_by_key(|hint| (hint.address_commitment, hint.observed_tick));
        seed_hints.dedup();

        let peer_digest = peer_digest(observations);
        Ok(Self {
            local_node,
            peer_digest,
            authenticated_peers: peers,
            selectors: selector_map.into_values().collect(),
            carrier_paths,
            observation_interval,
            limitations: limitations.into_iter().collect(),
            seed_hints,
        })
    }

    pub fn mode(&self) -> ReachabilityMode {
        if self.authenticated_peers.is_empty() {
            ReachabilityMode::Standalone
        } else if self
            .carrier_paths
            .iter()
            .any(|path| path.carrier.bidirectional)
        {
            ReachabilityMode::ComponentReachable
        } else {
            ReachabilityMode::PathLimited
        }
    }

    /// Reachability never gates the local cognitive loop.
    pub const fn can_encode_query_and_use_locally(&self) -> bool {
        true
    }

    pub const fn has_global_component_knowledge(&self) -> bool {
        false
    }

    pub const fn grants_seed_authority(&self) -> bool {
        false
    }
}

fn peer_digest(observations: &[PeerReachabilityObservation]) -> [u8; 32] {
    let mut sessions = observations
        .iter()
        .map(|observation| (*observation.peer.as_bytes(), observation.session_id))
        .collect::<Vec<_>>();
    sessions.sort();
    sessions.dedup();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:reachability-peer-digest:1\0");
    for (peer, session) in sessions {
        hasher.update(&peer);
        hasher.update(&session);
    }
    *hasher.finalize().as_bytes()
}

const fn budget_key(budget: Budget) -> (u64, u64, u64, u32) {
    (
        budget.max_records,
        budget.max_bytes,
        budget.max_work_units,
        budget.max_depth,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReachabilityError {
    InvalidObservationInterval,
    MissingCarrierPath,
    InvalidInventoryRoot,
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{CarrierKind, DisclosureClass};
    use onebrain_protocol::{SessionCapability, SessionProfile};

    use crate::vnext_session::{authenticate_session, create_finish, create_hello, create_welcome};

    use super::*;

    fn session() -> AuthenticatedSession {
        let initiator = SigningKey::from_bytes(&[1; 32]);
        let responder = SigningKey::from_bytes(&[2; 32]);
        let profiles = vec![SessionProfile {
            family: 1,
            major: 1,
            minor: 0,
        }];
        let capabilities = vec![SessionCapability::from_bytes([3; 32])];
        let hello = create_hello(
            &initiator,
            [4; 32],
            [5; 32],
            profiles.clone(),
            capabilities.clone(),
            Vec::new(),
        )
        .unwrap();
        let welcome = create_welcome(
            &hello,
            [4; 32],
            &responder,
            [6; 32],
            &profiles,
            &capabilities,
            Vec::new(),
        )
        .unwrap();
        let finish = create_finish(
            &hello,
            &welcome,
            &initiator,
            [4; 32],
            &profiles,
            &capabilities,
        )
        .unwrap();
        authenticate_session(&hello, &welcome, &finish, [4; 32], &profiles, &capabilities).unwrap()
    }

    fn carrier(bidirectional: bool) -> CarrierProfile {
        CarrierProfile {
            kind: CarrierKind::InMemory,
            max_frame_bytes: 64 * 1024,
            max_bundle_bytes: 1 << 20,
            store_carry_forward: !bidirectional,
            bidirectional,
        }
    }

    fn observation(bidirectional: bool) -> PeerReachabilityObservation {
        let session = session();
        PeerReachabilityObservation::from_authenticated_session(
            &session,
            true,
            vec![SelectorFrontierObservation {
                selector: SelectorCid::from_bytes([7; 32]),
                inventory_root: [8; 32],
                event_frontier: vec![EventCid::from_bytes([9; 32])],
                offered_budget: Budget::new(10, 10_000, 100, 4).unwrap(),
            }],
            vec![CarrierPathObservation {
                path_commitment: [10; 32],
                carrier: carrier(bidirectional),
                interval: ObservationInterval::new(1, 2).unwrap(),
            }],
        )
        .unwrap()
    }

    #[test]
    fn standalone_remains_locally_usable_without_seed() {
        let view = ReachabilityView::derive(
            NodeId::from_bytes([1; 32]),
            &[],
            ObservationInterval::new(1, 2).unwrap(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(view.mode(), ReachabilityMode::Standalone);
        assert!(view.can_encode_query_and_use_locally());
        assert!(!view.has_global_component_knowledge());
        assert!(!view.grants_seed_authority());
    }

    #[test]
    fn authenticated_lan_pair_is_component_reachable() {
        let observation = observation(true);
        let view = ReachabilityView::derive(
            NodeId::from_bytes([1; 32]),
            &[observation],
            ObservationInterval::new(1, 3).unwrap(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(view.mode(), ReachabilityMode::ComponentReachable);
        assert_eq!(view.authenticated_peers.len(), 1);
        assert_eq!(view.selectors.len(), 1);
        assert!(view.can_encode_query_and_use_locally());
    }

    #[test]
    fn one_way_store_carry_forward_path_is_explicitly_limited() {
        let view = ReachabilityView::derive(
            NodeId::from_bytes([1; 32]),
            &[observation(false)],
            ObservationInterval::new(1, 3).unwrap(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(view.mode(), ReachabilityMode::PathLimited);
        assert!(view
            .limitations
            .contains(&ReachabilityLimitation::OneWayPath));
        assert!(view
            .limitations
            .contains(&ReachabilityLimitation::StoreCarryForwardOnly));
    }

    #[test]
    fn observation_order_and_seed_outage_do_not_change_peer_digest_or_authority() {
        let first = observation(true);
        let mut second = observation(true);
        second.session_id = [11; 32];
        second.peer = NodeId::from_bytes([12; 32]);
        let interval = ObservationInterval::new(1, 3).unwrap();
        let left = ReachabilityView::derive(
            NodeId::from_bytes([1; 32]),
            &[first.clone(), second.clone()],
            interval,
            Vec::new(),
        )
        .unwrap();
        let hint = SeedRendezvousHint {
            address_commitment: [13; 32],
            observed_tick: 2,
        };
        assert!(!hint.grants_authority());
        let right = ReachabilityView::derive(
            NodeId::from_bytes([1; 32]),
            &[second, first],
            interval,
            vec![hint],
        )
        .unwrap();
        assert_eq!(left.peer_digest, right.peer_digest);
        assert_eq!(left.mode(), right.mode());
        assert!(!left.grants_seed_authority());
        assert!(!right.grants_seed_authority());
    }

    #[test]
    fn private_disclosure_is_not_invented_by_reachability() {
        // Reachability carries SelectorCID/root only, never a full private Need.
        let _private_class = DisclosureClass::LocalOnly;
        let view = ReachabilityView::derive(
            NodeId::from_bytes([1; 32]),
            &[observation(true)],
            ObservationInterval::new(1, 3).unwrap(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(view.selectors.len(), 1);
    }
}
