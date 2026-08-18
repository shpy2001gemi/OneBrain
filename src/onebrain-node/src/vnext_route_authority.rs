//! Local route, policy, and authority boundaries for vNext product runtimes.
//!
//! None of the mutation capabilities in this module are exposed to product/API
//! callers. Routes are learned only from a completed authenticated session,
//! policies are selected only by an allow-listed local version, and authority
//! frontiers are derived from validated local authority records.

#![cfg(feature = "vnext-network-runtime")]

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use ku_core::foundation::{
    authority_event_descriptor, AuthorityEventDescriptor, EventCid, FeedAuthorityDecision,
    MetabolicViewPolicy, NodeId, ReservedDomain,
};
use ku_net::vnext_session::AuthenticatedSession;
use thiserror::Error;

#[cfg(feature = "vnext-outbound-first")]
use crate::vnext_connection_planner::{RoutedVNextSession, VerifiedCarrierIdentity};

pub const MAX_AUTHENTICATED_ROUTES: usize = 4_096;
pub const MAX_LOCAL_POLICY_VERSIONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthenticatedRouteOrigin {
    OutboundResponder,
    InboundInitiator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthenticatedRoute {
    pub node: NodeId,
    pub addr: SocketAddr,
    pub origin: AuthenticatedRouteOrigin,
    pub session_id: [u8; 32],
    pub generation: u64,
}

#[derive(Default)]
struct RouteDirectoryState {
    by_node: BTreeMap<NodeId, AuthenticatedRoute>,
    by_addr: BTreeMap<SocketAddr, NodeId>,
    #[cfg(feature = "vnext-outbound-first")]
    routed_by_node: BTreeMap<NodeId, RoutedAuthenticatedRoute>,
    #[cfg(feature = "vnext-outbound-first")]
    routed_direct_by_addr: BTreeMap<SocketAddr, NodeId>,
    #[cfg(feature = "vnext-outbound-first")]
    routed_relay_index: BTreeMap<(NodeId, [u8; 32], NodeId), NodeId>,
    next_generation: u64,
}

#[cfg(feature = "vnext-outbound-first")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedAuthenticatedRoute {
    pub peer: NodeId,
    pub session_id: [u8; 32],
    pub transport_binding_digest: [u8; 32],
    pub carrier: VerifiedCarrierIdentity,
    pub generation: u64,
}

/// A cloneable, read-mostly directory whose only writers require a completed
/// authenticated session and the exact local session role.
#[derive(Clone, Default)]
pub struct AuthenticatedRouteDirectory {
    inner: Arc<RwLock<RouteDirectoryState>>,
}

impl AuthenticatedRouteDirectory {
    pub fn resolve(&self, node: NodeId) -> Result<Option<AuthenticatedRoute>, RouteDirectoryError> {
        self.inner
            .read()
            .map_err(|_| RouteDirectoryError::LockPoisoned)
            .map(|state| state.by_node.get(&node).copied())
    }

    pub fn node_at(&self, addr: SocketAddr) -> Result<Option<NodeId>, RouteDirectoryError> {
        self.inner
            .read()
            .map_err(|_| RouteDirectoryError::LockPoisoned)
            .map(|state| state.by_addr.get(&addr).copied())
    }

    pub fn len(&self) -> Result<usize, RouteDirectoryError> {
        self.inner
            .read()
            .map_err(|_| RouteDirectoryError::LockPoisoned)
            .map(|state| state.by_node.len())
    }

    pub fn is_empty(&self) -> Result<bool, RouteDirectoryError> {
        self.len().map(|len| len == 0)
    }

    #[cfg(feature = "vnext-outbound-first")]
    pub fn resolve_routed(
        &self,
        peer: NodeId,
    ) -> Result<Option<RoutedAuthenticatedRoute>, RouteDirectoryError> {
        self.inner
            .read()
            .map_err(|_| RouteDirectoryError::LockPoisoned)
            .map(|state| state.routed_by_node.get(&peer).cloned())
    }

    #[cfg(feature = "vnext-outbound-first")]
    pub fn routed_len(&self) -> Result<usize, RouteDirectoryError> {
        self.inner
            .read()
            .map_err(|_| RouteDirectoryError::LockPoisoned)
            .map(|state| state.routed_by_node.len())
    }

    #[cfg(feature = "vnext-outbound-first")]
    pub(crate) fn observe_routed(
        &self,
        routed: &RoutedVNextSession,
    ) -> Result<RoutedAuthenticatedRoute, RouteDirectoryError> {
        let peer = routed.expected_peer();
        if peer.as_bytes() == &[0; 32]
            || routed.authenticated().session_id == [0; 32]
            || routed.transport_binding_digest() == [0; 32]
        {
            return Err(RouteDirectoryError::InvalidObservation);
        }
        let mut state = self
            .inner
            .write()
            .map_err(|_| RouteDirectoryError::LockPoisoned)?;
        if !state.routed_by_node.contains_key(&peer)
            && state.routed_by_node.len() >= MAX_AUTHENTICATED_ROUTES
        {
            return Err(RouteDirectoryError::CapacityReached);
        }
        if let Some(previous) = state.routed_by_node.remove(&peer) {
            remove_routed_reverse_indexes(&mut state, &previous);
        }
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .ok_or(RouteDirectoryError::GenerationExhausted)?;
        let route = RoutedAuthenticatedRoute {
            peer,
            session_id: routed.authenticated().session_id,
            transport_binding_digest: routed.transport_binding_digest(),
            carrier: routed.carrier().clone(),
            generation: state.next_generation,
        };
        match &route.carrier {
            VerifiedCarrierIdentity::Direct {
                connected_socket, ..
            }
            | VerifiedCarrierIdentity::HolePunched {
                connected_socket, ..
            } => {
                if let Some(other) = state.routed_direct_by_addr.get(connected_socket) {
                    if *other != peer {
                        return Err(RouteDirectoryError::VerifiedOutboundRouteConflict);
                    }
                }
                state.routed_direct_by_addr.insert(*connected_socket, peer);
            }
            VerifiedCarrierIdentity::Relay {
                relay_node_id,
                association_id,
                ..
            } => {
                state
                    .routed_relay_index
                    .insert((*relay_node_id, *association_id, peer), peer);
            }
        }
        state.routed_by_node.insert(peer, route.clone());
        Ok(route)
    }

    #[cfg(feature = "vnext-outbound-first")]
    pub(crate) fn remove_routed(
        &self,
        peer: NodeId,
        session_id: [u8; 32],
    ) -> Result<(), RouteDirectoryError> {
        let mut state = self
            .inner
            .write()
            .map_err(|_| RouteDirectoryError::LockPoisoned)?;
        let Some(route) = state.routed_by_node.get(&peer) else {
            return Ok(());
        };
        if route.session_id != session_id {
            return Err(RouteDirectoryError::InvalidObservation);
        }
        let route = state
            .routed_by_node
            .remove(&peer)
            .ok_or(RouteDirectoryError::InvalidObservation)?;
        remove_routed_reverse_indexes(&mut state, &route);
        Ok(())
    }

    pub(crate) fn observe_outbound(
        &self,
        local: NodeId,
        session: &AuthenticatedSession,
        addr: SocketAddr,
    ) -> Result<AuthenticatedRoute, RouteDirectoryError> {
        if session.initiator != local {
            return Err(RouteDirectoryError::SessionRoleMismatch);
        }
        self.observe(
            session.responder,
            addr,
            AuthenticatedRouteOrigin::OutboundResponder,
            session.session_id,
        )
    }

    pub(crate) fn observe_inbound(
        &self,
        local: NodeId,
        session: &AuthenticatedSession,
        addr: SocketAddr,
    ) -> Result<AuthenticatedRoute, RouteDirectoryError> {
        if session.responder != local {
            return Err(RouteDirectoryError::SessionRoleMismatch);
        }
        self.observe(
            session.initiator,
            addr,
            AuthenticatedRouteOrigin::InboundInitiator,
            session.session_id,
        )
    }

    fn observe(
        &self,
        node: NodeId,
        addr: SocketAddr,
        origin: AuthenticatedRouteOrigin,
        session_id: [u8; 32],
    ) -> Result<AuthenticatedRoute, RouteDirectoryError> {
        if node.as_bytes() == &[0; 32] || addr.port() == 0 || session_id == [0; 32] {
            return Err(RouteDirectoryError::InvalidObservation);
        }
        let mut state = self
            .inner
            .write()
            .map_err(|_| RouteDirectoryError::LockPoisoned)?;
        if origin == AuthenticatedRouteOrigin::InboundInitiator {
            if let Some(previous) = state.by_node.get(&node).copied() {
                if previous.origin == AuthenticatedRouteOrigin::OutboundResponder {
                    // An inbound source port is authenticated but is not
                    // necessarily a reusable listening route. Never downgrade
                    // a previously verified outbound responder address.
                    return Ok(previous);
                }
            }
            if let Some(previous_node) = state.by_addr.get(&addr).copied() {
                if previous_node != node
                    && state.by_node.get(&previous_node).is_some_and(|route| {
                        route.origin == AuthenticatedRouteOrigin::OutboundResponder
                    })
                {
                    return Err(RouteDirectoryError::VerifiedOutboundRouteConflict);
                }
            }
        }
        if !state.by_node.contains_key(&node) && state.by_node.len() >= MAX_AUTHENTICATED_ROUTES {
            return Err(RouteDirectoryError::CapacityReached);
        }

        if let Some(previous) = state.by_node.get(&node).copied() {
            state.by_addr.remove(&previous.addr);
        }
        if let Some(previous_node) = state.by_addr.get(&addr).copied() {
            state.by_node.remove(&previous_node);
        }
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .ok_or(RouteDirectoryError::GenerationExhausted)?;
        let route = AuthenticatedRoute {
            node,
            addr,
            origin,
            session_id,
            generation: state.next_generation,
        };
        state.by_node.insert(node, route);
        state.by_addr.insert(addr, node);
        Ok(route)
    }
}

#[cfg(feature = "vnext-outbound-first")]
fn remove_routed_reverse_indexes(
    state: &mut RouteDirectoryState,
    route: &RoutedAuthenticatedRoute,
) {
    match &route.carrier {
        VerifiedCarrierIdentity::Direct {
            connected_socket, ..
        }
        | VerifiedCarrierIdentity::HolePunched {
            connected_socket, ..
        } => {
            state.routed_direct_by_addr.remove(connected_socket);
        }
        VerifiedCarrierIdentity::Relay {
            relay_node_id,
            association_id,
            ..
        } => {
            state
                .routed_relay_index
                .remove(&(*relay_node_id, *association_id, route.peer));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalPolicyVersion(u32);

impl LocalPolicyVersion {
    pub fn new(value: u32) -> Result<Self, LocalPolicyRegistryError> {
        if value == 0 {
            return Err(LocalPolicyRegistryError::InvalidVersion);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Immutable after startup. Product/API callers may select a version but
/// cannot inject policy code or policy fields into a materialization request.
#[derive(Clone)]
pub struct LocalPolicyRegistry {
    policies: BTreeMap<LocalPolicyVersion, MetabolicViewPolicy>,
}

impl LocalPolicyRegistry {
    pub fn new(
        policies: impl IntoIterator<Item = (LocalPolicyVersion, MetabolicViewPolicy)>,
    ) -> Result<Self, LocalPolicyRegistryError> {
        let mut registered = BTreeMap::new();
        for (version, policy) in policies {
            if registered.len() >= MAX_LOCAL_POLICY_VERSIONS {
                return Err(LocalPolicyRegistryError::CapacityReached);
            }
            let policy = policy
                .validated()
                .map_err(|_| LocalPolicyRegistryError::InvalidPolicy)?;
            if registered.insert(version, policy).is_some() {
                return Err(LocalPolicyRegistryError::DuplicateVersion);
            }
        }
        if registered.is_empty() {
            return Err(LocalPolicyRegistryError::Empty);
        }
        Ok(Self {
            policies: registered,
        })
    }

    pub fn resolve(&self, version: LocalPolicyVersion) -> Option<&MetabolicViewPolicy> {
        self.policies.get(&version)
    }

    pub fn versions(&self) -> Vec<LocalPolicyVersion> {
        self.policies.keys().copied().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityFrontierResolution {
    Resolved {
        frontier: EventCid,
        decisions: Vec<FeedAuthorityDecision>,
    },
    Missing,
    Ambiguous {
        frontiers: Vec<EventCid>,
    },
}

/// Resolve a feed's authority frontier from the terminal tips of validated
/// local authority state. Multiple relevant, incomparable tips fail closed.
pub(crate) fn resolve_authority_frontier(
    accepted_authority_events: &[Vec<u8>],
    mut evaluate: impl FnMut(EventCid) -> Result<Vec<FeedAuthorityDecision>, String>,
) -> Result<AuthorityFrontierResolution, AuthorityResolverError> {
    let mut all = BTreeSet::new();
    let mut referenced = BTreeSet::new();
    for bytes in accepted_authority_events {
        let cid = EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(bytes));
        all.insert(*cid.as_bytes());
        match authority_event_descriptor(bytes)
            .map_err(|error| AuthorityResolverError::InvalidLocalState(error.to_string()))?
        {
            AuthorityEventDescriptor::Root => {}
            AuthorityEventDescriptor::Delegation { parent, .. } => {
                referenced.insert(*parent.as_bytes());
            }
            AuthorityEventDescriptor::Revocation {
                target,
                authorized_by,
                ..
            } => {
                referenced.insert(*target.as_bytes());
                referenced.insert(*authorized_by.as_bytes());
            }
        }
    }

    let mut relevant = Vec::new();
    for terminal in all.difference(&referenced) {
        let frontier = EventCid::from_bytes(*terminal);
        let decisions =
            evaluate(frontier).map_err(AuthorityResolverError::ValidatedStateUnavailable)?;
        if decisions.iter().any(|decision| {
            matches!(
                decision,
                FeedAuthorityDecision::AuthorizedRelative { .. }
                    | FeedAuthorityDecision::QuarantinedRevokedRelative { .. }
            )
        }) {
            relevant.push((frontier, decisions));
        }
    }
    match relevant.len() {
        0 => Ok(AuthorityFrontierResolution::Missing),
        1 => {
            let (frontier, decisions) = relevant.remove(0);
            Ok(AuthorityFrontierResolution::Resolved {
                frontier,
                decisions,
            })
        }
        _ => Ok(AuthorityFrontierResolution::Ambiguous {
            frontiers: relevant.into_iter().map(|(frontier, _)| frontier).collect(),
        }),
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteDirectoryError {
    #[error("authenticated route observation is invalid")]
    InvalidObservation,
    #[error("authenticated session role does not match the local node")]
    SessionRoleMismatch,
    #[error("inbound source address conflicts with a verified outbound route")]
    VerifiedOutboundRouteConflict,
    #[error("authenticated route directory capacity reached")]
    CapacityReached,
    #[error("authenticated route generation exhausted")]
    GenerationExhausted,
    #[error("authenticated route directory lock poisoned")]
    LockPoisoned,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LocalPolicyRegistryError {
    #[error("local policy registry must not be empty")]
    Empty,
    #[error("local policy version must be non-zero")]
    InvalidVersion,
    #[error("local policy version is duplicated")]
    DuplicateVersion,
    #[error("local policy is invalid")]
    InvalidPolicy,
    #[error("local policy registry capacity reached")]
    CapacityReached,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthorityResolverError {
    #[error("validated local authority state is invalid: {0}")]
    InvalidLocalState(String),
    #[error("validated local authority state is unavailable: {0}")]
    ValidatedStateUnavailable(String),
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{
        ActorRootDelegation, DeviceId, FeedId, MetabolicViewPolicy, NamespaceCommitment,
        ObjectReference, UnresolvedAuthorityReason,
    };
    use onebrain_protocol::reconciliation_profile;

    use super::*;

    fn session(initiator: NodeId, responder: NodeId, marker: u8) -> AuthenticatedSession {
        AuthenticatedSession {
            session_id: [marker; 32],
            transport_binding: [marker.wrapping_add(1); 32],
            initiator,
            responder,
            profile: reconciliation_profile(),
            capabilities: Vec::new(),
            feed_evidence: Vec::new(),
        }
    }

    fn policy(marker: u8) -> MetabolicViewPolicy {
        MetabolicViewPolicy {
            policy_ref: ObjectReference::new(0, [marker; 32]),
            accepted_evidence_policies: vec![ObjectReference::new(0, [marker.wrapping_add(1); 32])],
            recent_event_horizon: 16,
        }
    }

    fn root(marker: u8) -> (Vec<u8>, EventCid) {
        let key = SigningKey::from_bytes(&[marker; 32]);
        let feed = FeedId::from_bytes([marker.wrapping_add(1); 32]);
        let bytes = ActorRootDelegation::new(
            *key.verifying_key().as_bytes(),
            feed,
            DeviceId::from_bytes([marker.wrapping_add(2); 32]),
            Some(
                NamespaceCommitment::derive(b"p1.5-authority-root", [marker.wrapping_add(3); 32])
                    .unwrap(),
            ),
            0,
            0,
        )
        .unwrap()
        .sign(&key)
        .unwrap()
        .encode()
        .unwrap();
        let cid = EventCid::from_bytes(ReservedDomain::AuthorityEvent.digest(&bytes));
        (bytes, cid)
    }

    #[test]
    fn route_directory_requires_authenticated_role_and_preserves_bijection() {
        let routes = AuthenticatedRouteDirectory::default();
        let local = NodeId::from_bytes([1; 32]);
        let first_peer = NodeId::from_bytes([2; 32]);
        let second_peer = NodeId::from_bytes([3; 32]);
        let first_addr: SocketAddr = "127.0.0.1:41001".parse().unwrap();
        let second_addr: SocketAddr = "127.0.0.1:41002".parse().unwrap();

        assert!(routes.is_empty().unwrap());
        assert_eq!(
            routes
                .observe_outbound(local, &session(second_peer, local, 9), first_addr,)
                .unwrap_err(),
            RouteDirectoryError::SessionRoleMismatch
        );
        assert!(routes.is_empty().unwrap());

        let first = routes
            .observe_outbound(local, &session(local, first_peer, 10), first_addr)
            .unwrap();
        assert_eq!(first.node, first_peer);
        assert_eq!(routes.resolve(first_peer).unwrap(), Some(first));
        assert_eq!(routes.node_at(first_addr).unwrap(), Some(first_peer));

        let moved = routes
            .observe_outbound(local, &session(local, first_peer, 11), second_addr)
            .unwrap();
        assert!(moved.generation > first.generation);
        assert_eq!(routes.node_at(first_addr).unwrap(), None);

        let not_downgraded = routes
            .observe_inbound(local, &session(first_peer, local, 12), first_addr)
            .unwrap();
        assert_eq!(not_downgraded, moved);
        assert_eq!(routes.node_at(first_addr).unwrap(), None);

        assert_eq!(
            routes
                .observe_inbound(local, &session(second_peer, local, 13), second_addr)
                .unwrap_err(),
            RouteDirectoryError::VerifiedOutboundRouteConflict
        );
        assert_eq!(routes.resolve(first_peer).unwrap(), Some(moved));

        let inbound = routes
            .observe_inbound(local, &session(second_peer, local, 14), first_addr)
            .unwrap();
        assert_eq!(inbound.origin, AuthenticatedRouteOrigin::InboundInitiator);
        assert_eq!(routes.resolve(second_peer).unwrap(), Some(inbound));
        assert_eq!(routes.len().unwrap(), 2);
    }

    #[test]
    fn policy_registry_rejects_arbitrary_or_duplicate_versions() {
        assert_eq!(
            LocalPolicyVersion::new(0).unwrap_err(),
            LocalPolicyRegistryError::InvalidVersion
        );
        let v1 = LocalPolicyVersion::new(1).unwrap();
        let registry = LocalPolicyRegistry::new([(v1, policy(10))]).unwrap();
        assert_eq!(registry.versions(), vec![v1]);
        assert!(registry.resolve(v1).is_some());
        assert!(registry
            .resolve(LocalPolicyVersion::new(2).unwrap())
            .is_none());
        assert!(matches!(
            LocalPolicyRegistry::new([(v1, policy(10)), (v1, policy(11))]),
            Err(LocalPolicyRegistryError::DuplicateVersion)
        ));
        let mut invalid = policy(12);
        invalid.recent_event_horizon = 0;
        assert!(matches!(
            LocalPolicyRegistry::new([(v1, invalid)]),
            Err(LocalPolicyRegistryError::InvalidPolicy)
        ));
    }

    #[test]
    fn authority_resolver_uses_local_terminal_state_and_fails_closed_on_ambiguity() {
        let (first_bytes, first) = root(20);
        let (second_bytes, second) = root(30);
        let actor_key = SigningKey::from_bytes(&[7; 32]);
        let actor =
            ku_core::foundation::actor_id_from_root_key(*actor_key.verifying_key().as_bytes())
                .unwrap();

        let resolved =
            resolve_authority_frontier(&[first_bytes.clone(), second_bytes.clone()], |frontier| {
                Ok(if frontier == first {
                    vec![FeedAuthorityDecision::AuthorizedRelative {
                        actor,
                        grant: first,
                        frontier,
                    }]
                } else {
                    vec![FeedAuthorityDecision::StaleOrUnresolved {
                        reason: UnresolvedAuthorityReason::MissingAcceptedGrant,
                        frontier,
                    }]
                })
            })
            .unwrap();
        assert!(matches!(
            resolved,
            AuthorityFrontierResolution::Resolved { frontier, .. } if frontier == first
        ));

        let ambiguous =
            resolve_authority_frontier(&[first_bytes.clone(), second_bytes], |frontier| {
                Ok(vec![FeedAuthorityDecision::AuthorizedRelative {
                    actor,
                    grant: frontier,
                    frontier,
                }])
            })
            .unwrap();
        assert!(matches!(
            ambiguous,
            AuthorityFrontierResolution::Ambiguous { ref frontiers }
                if frontiers == &vec![first, second]
                    || frontiers == &vec![second, first]
        ));

        let missing = resolve_authority_frontier(&[first_bytes], |frontier| {
            Ok(vec![FeedAuthorityDecision::StaleOrUnresolved {
                reason: UnresolvedAuthorityReason::MissingAcceptedGrant,
                frontier,
            }])
        })
        .unwrap();
        assert_eq!(missing, AuthorityFrontierResolution::Missing);
    }
}
