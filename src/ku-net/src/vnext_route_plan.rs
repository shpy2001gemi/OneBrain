//! Pure deterministic outbound-first connection planner.

use std::cmp::Ordering;

use ku_core::foundation::NodeId;
use onebrain_protocol::{
    DirectCandidateKindV1, DirectCandidateV1, HolePunchCandidateV1, RelayCandidateV1,
    RelayTransportV1, RouteAttemptOutcomeV1, RouteAttemptV1, RouteFailureCodeV1,
    RouteLimitationCodeV1, RouteLimitationV1, RoutePlanV1,
};

pub use onebrain_protocol::RoutePathKindV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteStateV1 {
    Discovering,
    DirectChecking,
    HolePunching,
    RelayConnecting,
    PeerAuthenticating,
    Connected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteFailure {
    NoBootstrapReachable,
    CandidateExpired,
    DirectTimeout,
    HolePunchFailed,
    RelayDenied,
    RelayUnavailable,
    PeerIdentityMismatch,
    NetworkChanged,
    BudgetExceeded,
    PathLimited {
        attempts: Vec<RouteAttemptV1>,
        limitations: Vec<RouteLimitationV1>,
    },
}

impl From<RouteFailureCodeV1> for RouteFailure {
    fn from(value: RouteFailureCodeV1) -> Self {
        match value {
            RouteFailureCodeV1::NoBootstrapReachable => Self::NoBootstrapReachable,
            RouteFailureCodeV1::CandidateExpired => Self::CandidateExpired,
            RouteFailureCodeV1::DirectTimeout => Self::DirectTimeout,
            RouteFailureCodeV1::HolePunchFailed => Self::HolePunchFailed,
            RouteFailureCodeV1::RelayDenied => Self::RelayDenied,
            RouteFailureCodeV1::RelayUnavailable => Self::RelayUnavailable,
            RouteFailureCodeV1::PeerIdentityMismatch => Self::PeerIdentityMismatch,
            RouteFailureCodeV1::NetworkChanged => Self::NetworkChanged,
            RouteFailureCodeV1::BudgetExceeded => Self::BudgetExceeded,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedHolePunchCandidate {
    candidate: HolePunchCandidateV1,
    schedule_digest: [u8; 32],
}

impl AdmittedHolePunchCandidate {
    pub fn candidate(&self) -> &HolePunchCandidateV1 {
        &self.candidate
    }

    pub fn schedule_digest(&self) -> [u8; 32] {
        self.schedule_digest
    }

    #[cfg(test)]
    fn test_only(
        relay_node_id: NodeId,
        local_reservation_id: [u8; 32],
        remote_reservation_id: [u8; 32],
        schedule_digest: [u8; 32],
        priority: u32,
        expires_at: u64,
    ) -> Self {
        Self {
            candidate: HolePunchCandidateV1 {
                relay_node_id,
                local_reservation_id,
                remote_reservation_id,
                schedule_digest,
                priority,
                expires_at,
            },
            schedule_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedRelayPath {
    candidate: RelayCandidateV1,
    association_id: [u8; 32],
    local_reservation_id: [u8; 32],
    remote_reservation_id: [u8; 32],
    association_digest: [u8; 32],
}

impl AdmittedRelayPath {
    pub fn candidate(&self) -> &RelayCandidateV1 {
        &self.candidate
    }

    pub fn association_id(&self) -> [u8; 32] {
        self.association_id
    }

    pub fn reservation_ids(&self) -> ([u8; 32], [u8; 32]) {
        (self.local_reservation_id, self.remote_reservation_id)
    }

    pub fn association_digest(&self) -> [u8; 32] {
        self.association_digest
    }

    #[cfg(test)]
    fn test_only(
        candidate: RelayCandidateV1,
        association_id: [u8; 32],
        local_reservation_id: [u8; 32],
        remote_reservation_id: [u8; 32],
        association_digest: [u8; 32],
    ) -> Self {
        Self {
            candidate,
            association_id,
            local_reservation_id,
            remote_reservation_id,
            association_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannerAction {
    Gather,
    CheckDirect(DirectCandidateV1),
    EnsureRouteReservation {
        relay: NodeId,
    },
    CoordinateHolePunch(AdmittedHolePunchCandidate),
    AssociateRelay {
        candidate: RelayCandidateV1,
        local_reservation_id: [u8; 32],
        remote_reservation_id: [u8; 32],
    },
    ConnectRelay(AdmittedRelayPath),
    AuthenticatePeer {
        expected_peer: NodeId,
    },
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannerEvent {
    CandidatesGathered {
        direct: Vec<DirectCandidateV1>,
        relay: Vec<RelayCandidateV1>,
    },
    HolePunchAdmitted(AdmittedHolePunchCandidate),
    ReservationPairAdmitted {
        relay: NodeId,
        local_reservation_id: [u8; 32],
        remote_reservation_id: [u8; 32],
    },
    RelayAssociationAdmitted(AdmittedRelayPath),
    AttemptSucceeded {
        path: RoutePathKindV1,
        carrier: Option<NodeId>,
    },
    AttemptFailed(RouteFailureCodeV1),
    DeadlineReached,
    NetworkEpochChanged(u64),
}

#[derive(Clone, Debug)]
struct ActiveAttempt {
    path: RoutePathKindV1,
    carrier: Option<NodeId>,
    started_at: u64,
}

#[derive(Clone, Debug)]
pub struct ConnectionPlanner {
    state: RouteStateV1,
    plan: RoutePlanV1,
    next_direct: usize,
    next_relay: usize,
    attempts: Vec<RouteAttemptV1>,
    limitations: Vec<RouteLimitationV1>,
    active_attempt: Option<ActiveAttempt>,
    current_relay: Option<RelayCandidateV1>,
    reservation_pair: Option<([u8; 32], [u8; 32])>,
    network_epoch: Option<u64>,
    saw_expired_candidate: bool,
}

impl ConnectionPlanner {
    pub fn new(plan: RoutePlanV1) -> Self {
        Self {
            state: RouteStateV1::Discovering,
            plan,
            next_direct: 0,
            next_relay: 0,
            attempts: Vec::new(),
            limitations: Vec::new(),
            active_attempt: None,
            current_relay: None,
            reservation_pair: None,
            network_epoch: None,
            saw_expired_candidate: false,
        }
    }

    pub fn state(&self) -> RouteStateV1 {
        self.state
    }

    pub fn attempts(&self) -> &[RouteAttemptV1] {
        &self.attempts
    }

    /// Advance the planner without performing I/O. The same state, time and
    /// event always produce the same transition.
    pub fn next(
        &mut self,
        now: u64,
        event: Option<PlannerEvent>,
    ) -> Result<PlannerAction, RouteFailure> {
        if now > self.plan.deadline || matches!(event, Some(PlannerEvent::DeadlineReached)) {
            self.add_limitation(RouteLimitationCodeV1::DeadlineExceeded);
            return Err(self.path_limited());
        }

        let Some(event) = event else {
            return if self.state == RouteStateV1::Discovering {
                Ok(PlannerAction::Gather)
            } else {
                self.select_next(now)
            };
        };

        match event {
            PlannerEvent::NetworkEpochChanged(epoch) => {
                if self.network_epoch != Some(epoch) {
                    self.add_limitation(RouteLimitationCodeV1::NetworkChangedDuringAttempt);
                    return Err(RouteFailure::NetworkChanged);
                }
                self.select_next(now)
            }
            PlannerEvent::CandidatesGathered {
                mut direct,
                mut relay,
            } => {
                if self.state != RouteStateV1::Discovering {
                    return Err(RouteFailure::BudgetExceeded);
                }
                if direct.len() > onebrain_protocol::MAX_DIRECT_CANDIDATES
                    || relay.len() > onebrain_protocol::MAX_RELAY_CANDIDATES
                    || self.plan.attempt_budget == 0
                    || self.plan.resource_budget.max_concurrent_checks == 0
                    || self.plan.resource_budget.max_signature_checks == 0
                    || self.plan.resource_budget.max_probe_bytes == 0
                {
                    return Err(RouteFailure::BudgetExceeded);
                }
                direct.sort_by(compare_direct);
                relay.sort_by(compare_relay);
                self.network_epoch = direct.first().map(|candidate| candidate.network_epoch);
                self.plan.direct_candidates = direct;
                self.plan.relay_candidates = relay;
                self.state = RouteStateV1::DirectChecking;
                self.select_next(now)
            }
            PlannerEvent::HolePunchAdmitted(admitted) => {
                let current = self
                    .current_relay
                    .as_ref()
                    .ok_or(RouteFailure::RelayDenied)?;
                if admitted.candidate.relay_node_id != current.relay_node_id
                    || admitted.candidate.expires_at <= now
                    || admitted.schedule_digest != admitted.candidate.schedule_digest
                {
                    return Err(RouteFailure::RelayDenied);
                }
                self.reservation_pair = Some((
                    admitted.candidate.local_reservation_id,
                    admitted.candidate.remote_reservation_id,
                ));
                self.start_attempt(
                    RoutePathKindV1::HolePunched,
                    Some(current.relay_node_id),
                    now,
                )?;
                self.state = RouteStateV1::HolePunching;
                Ok(PlannerAction::CoordinateHolePunch(admitted))
            }
            PlannerEvent::ReservationPairAdmitted {
                relay,
                local_reservation_id,
                remote_reservation_id,
            } => {
                let candidate = self
                    .current_relay
                    .clone()
                    .ok_or(RouteFailure::RelayDenied)?;
                if candidate.relay_node_id != relay {
                    return Err(RouteFailure::RelayDenied);
                }
                self.reservation_pair = Some((local_reservation_id, remote_reservation_id));
                Ok(PlannerAction::AssociateRelay {
                    candidate,
                    local_reservation_id,
                    remote_reservation_id,
                })
            }
            PlannerEvent::RelayAssociationAdmitted(path) => {
                let candidate = self
                    .current_relay
                    .as_ref()
                    .ok_or(RouteFailure::RelayDenied)?;
                let pair = self.reservation_pair.ok_or(RouteFailure::RelayDenied)?;
                if path.candidate.relay_node_id != candidate.relay_node_id
                    || path.candidate.reservation_id != candidate.reservation_id
                    || path.reservation_ids() != pair
                    || path.candidate.expires_at <= now
                {
                    return Err(RouteFailure::RelayDenied);
                }
                let kind = relay_path_kind(path.candidate.transport);
                self.start_attempt(kind, Some(path.candidate.relay_node_id), now)?;
                self.state = RouteStateV1::RelayConnecting;
                Ok(PlannerAction::ConnectRelay(path))
            }
            PlannerEvent::AttemptSucceeded { path, carrier } => {
                if self.state == RouteStateV1::PeerAuthenticating {
                    let active = self
                        .active_attempt
                        .take()
                        .ok_or(RouteFailure::PeerIdentityMismatch)?;
                    if active.path != path || active.carrier != carrier {
                        return Err(RouteFailure::PeerIdentityMismatch);
                    }
                    self.attempts.push(RouteAttemptV1 {
                        path_kind: active.path,
                        carrier_identity: active.carrier,
                        started_at: active.started_at,
                        finished_at: now,
                        outcome: RouteAttemptOutcomeV1::Connected,
                    });
                    self.state = RouteStateV1::Connected;
                    return Ok(PlannerAction::Complete);
                }
                let active = self
                    .active_attempt
                    .as_ref()
                    .ok_or(RouteFailure::PeerIdentityMismatch)?;
                if active.path != path || active.carrier != carrier {
                    return Err(RouteFailure::PeerIdentityMismatch);
                }
                self.state = RouteStateV1::PeerAuthenticating;
                Ok(PlannerAction::AuthenticatePeer {
                    expected_peer: self.plan.expected_peer,
                })
            }
            PlannerEvent::AttemptFailed(code) => self.handle_attempt_failure(now, code),
            PlannerEvent::DeadlineReached => unreachable!("handled before event dispatch"),
        }
    }

    fn handle_attempt_failure(
        &mut self,
        now: u64,
        code: RouteFailureCodeV1,
    ) -> Result<PlannerAction, RouteFailure> {
        if self.active_attempt.is_none()
            && self.current_relay.is_some()
            && matches!(
                code,
                RouteFailureCodeV1::RelayDenied | RouteFailureCodeV1::RelayUnavailable
            )
        {
            self.current_relay = None;
            self.reservation_pair = None;
            return self.select_next(now);
        }
        if matches!(
            code,
            RouteFailureCodeV1::PeerIdentityMismatch
                | RouteFailureCodeV1::NetworkChanged
                | RouteFailureCodeV1::BudgetExceeded
                | RouteFailureCodeV1::NoBootstrapReachable
                | RouteFailureCodeV1::CandidateExpired
        ) && self.active_attempt.is_none()
        {
            return Err(RouteFailure::from(code));
        }
        let active = self
            .active_attempt
            .take()
            .ok_or_else(|| RouteFailure::from(code))?;
        self.attempts.push(RouteAttemptV1 {
            path_kind: active.path,
            carrier_identity: active.carrier,
            started_at: active.started_at,
            finished_at: now,
            outcome: RouteAttemptOutcomeV1::Failed(code),
        });
        if self.state == RouteStateV1::PeerAuthenticating
            && code == RouteFailureCodeV1::PeerIdentityMismatch
        {
            return Err(RouteFailure::PeerIdentityMismatch);
        }
        if active.path == RoutePathKindV1::HolePunched {
            let candidate = self
                .current_relay
                .clone()
                .ok_or(RouteFailure::RelayUnavailable)?;
            let (local_reservation_id, remote_reservation_id) = self
                .reservation_pair
                .ok_or(RouteFailure::RelayUnavailable)?;
            self.state = RouteStateV1::RelayConnecting;
            return Ok(PlannerAction::AssociateRelay {
                candidate,
                local_reservation_id,
                remote_reservation_id,
            });
        }
        if matches!(
            active.path,
            RoutePathKindV1::RelayUdp | RoutePathKindV1::RelayTcp443
        ) {
            self.current_relay = None;
            self.reservation_pair = None;
            self.state = RouteStateV1::RelayConnecting;
        } else {
            self.state = RouteStateV1::DirectChecking;
        }
        self.select_next(now)
    }

    fn select_next(&mut self, now: u64) -> Result<PlannerAction, RouteFailure> {
        if self.state == RouteStateV1::Connected {
            return Ok(PlannerAction::Complete);
        }
        if self.attempts.len() as u64 >= self.plan.attempt_budget {
            self.add_limitation(RouteLimitationCodeV1::CandidateBudgetExhausted);
            return Err(self.path_limited());
        }

        while let Some(candidate) = self.plan.direct_candidates.get(self.next_direct).cloned() {
            self.next_direct += 1;
            if candidate.expires_at <= now {
                self.saw_expired_candidate = true;
                continue;
            }
            if let Some(epoch) = self.network_epoch {
                if candidate.network_epoch != epoch {
                    self.add_limitation(RouteLimitationCodeV1::NetworkChangedDuringAttempt);
                    return Err(RouteFailure::NetworkChanged);
                }
            }
            self.start_attempt(RoutePathKindV1::Direct, None, now)?;
            self.state = RouteStateV1::DirectChecking;
            return Ok(PlannerAction::CheckDirect(candidate));
        }

        while let Some(candidate) = self.plan.relay_candidates.get(self.next_relay).cloned() {
            self.next_relay += 1;
            if candidate.expires_at <= now {
                self.saw_expired_candidate = true;
                continue;
            }
            self.current_relay = Some(candidate.clone());
            self.reservation_pair = None;
            self.state = RouteStateV1::RelayConnecting;
            return Ok(PlannerAction::EnsureRouteReservation {
                relay: candidate.relay_node_id,
            });
        }

        if self.attempts.is_empty() && self.saw_expired_candidate {
            return Err(RouteFailure::CandidateExpired);
        }
        self.add_limitation(RouteLimitationCodeV1::CandidateBudgetExhausted);
        Err(self.path_limited())
    }

    fn start_attempt(
        &mut self,
        path: RoutePathKindV1,
        carrier: Option<NodeId>,
        now: u64,
    ) -> Result<(), RouteFailure> {
        if self.active_attempt.is_some() {
            return Err(RouteFailure::BudgetExceeded);
        }
        if self.attempts.len() as u64 >= self.plan.attempt_budget {
            self.add_limitation(RouteLimitationCodeV1::CandidateBudgetExhausted);
            return Err(self.path_limited());
        }
        self.active_attempt = Some(ActiveAttempt {
            path,
            carrier,
            started_at: now,
        });
        Ok(())
    }

    fn add_limitation(&mut self, code: RouteLimitationCodeV1) {
        if let Some(existing) = self.limitations.iter_mut().find(|item| item.code == code) {
            existing.count = existing.count.saturating_add(1);
        } else {
            self.limitations.push(RouteLimitationV1 { code, count: 1 });
        }
    }

    fn path_limited(&self) -> RouteFailure {
        RouteFailure::PathLimited {
            attempts: self.attempts.clone(),
            limitations: self.limitations.clone(),
        }
    }
}

fn compare_direct(left: &DirectCandidateV1, right: &DirectCandidateV1) -> Ordering {
    direct_rank(left.kind)
        .cmp(&direct_rank(right.kind))
        .then_with(|| right.priority.cmp(&left.priority))
        .then_with(|| left.endpoint.cmp(&right.endpoint))
}

fn direct_rank(kind: DirectCandidateKindV1) -> u8 {
    match kind {
        DirectCandidateKindV1::Host => 0,
        DirectCandidateKindV1::ServerReflexive => 1,
        DirectCandidateKindV1::ProviderMapped => 2,
    }
}

fn compare_relay(left: &RelayCandidateV1, right: &RelayCandidateV1) -> Ordering {
    relay_rank(left.transport)
        .cmp(&relay_rank(right.transport))
        .then_with(|| right.priority.cmp(&left.priority))
        .then_with(|| left.relay_node_id.cmp(&right.relay_node_id))
}

fn relay_rank(transport: RelayTransportV1) -> u8 {
    match transport {
        RelayTransportV1::QuicUdp => 0,
        RelayTransportV1::TlsTcp443 => 1,
    }
}

fn relay_path_kind(transport: RelayTransportV1) -> RoutePathKindV1 {
    match transport {
        RelayTransportV1::QuicUdp => RoutePathKindV1::RelayUdp,
        RelayTransportV1::TlsTcp443 => RoutePathKindV1::RelayTcp443,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onebrain_protocol::{
        DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, ReachabilityEndpointV1,
        RelayCandidateV1, RelayTransportV1, RouteLimitationCodeV1, RouteResourceBudgetV1,
    };

    fn endpoint(last: u8, port: u16) -> ReachabilityEndpointV1 {
        ReachabilityEndpointV1 {
            host: HostAddressV1::Ipv4([8, 8, 4, last]),
            port,
        }
    }

    fn direct(
        kind: DirectCandidateKindV1,
        priority: u32,
        epoch: u64,
        expiry: u64,
    ) -> DirectCandidateV1 {
        DirectCandidateV1 {
            endpoint: endpoint(priority as u8, 41_000),
            kind,
            priority,
            network_epoch: epoch,
            expires_at: expiry,
        }
    }

    fn relay(transport: RelayTransportV1, priority: u32, expiry: u64) -> RelayCandidateV1 {
        RelayCandidateV1 {
            relay_node_id: NodeId::from_bytes([priority as u8; 32]),
            reservation_id: [priority as u8; 32],
            transport,
            endpoint: endpoint(
                priority as u8,
                if transport == RelayTransportV1::QuicUdp {
                    41_000
                } else {
                    443
                },
            ),
            priority,
            expires_at: expiry,
        }
    }

    fn plan(
        direct_candidates: Vec<DirectCandidateV1>,
        relay_candidates: Vec<RelayCandidateV1>,
        budget: u64,
    ) -> RoutePlanV1 {
        RoutePlanV1 {
            expected_peer: NodeId::from_bytes([99; 32]),
            direct_candidates,
            relay_candidates,
            deadline: 1_000,
            attempt_budget: budget,
            resource_budget: RouteResourceBudgetV1 {
                max_concurrent_checks: 2,
                max_signature_checks: 16,
                max_probe_bytes: 65_536,
            },
            privacy_policy_digest: [5; 32],
        }
    }

    fn gathered(p: &RoutePlanV1) -> PlannerEvent {
        PlannerEvent::CandidatesGathered {
            direct: p.direct_candidates.clone(),
            relay: p.relay_candidates.clone(),
        }
    }

    #[test]
    fn full_cone_and_public_ip_prefer_direct_then_authenticate() {
        for kind in [
            DirectCandidateKindV1::Host,
            DirectCandidateKindV1::ServerReflexive,
        ] {
            let p = plan(vec![direct(kind, 10, 1, 900)], vec![], 4);
            let mut planner = ConnectionPlanner::new(p.clone());
            assert_eq!(planner.next(10, None).unwrap(), PlannerAction::Gather);
            assert!(matches!(
                planner.next(11, Some(gathered(&p))).unwrap(),
                PlannerAction::CheckDirect(_)
            ));
            assert!(matches!(
                planner
                    .next(
                        12,
                        Some(PlannerEvent::AttemptSucceeded {
                            path: RoutePathKindV1::Direct,
                            carrier: None
                        })
                    )
                    .unwrap(),
                PlannerAction::AuthenticatePeer { .. }
            ));
            assert_eq!(
                planner
                    .next(
                        13,
                        Some(PlannerEvent::AttemptSucceeded {
                            path: RoutePathKindV1::Direct,
                            carrier: None
                        })
                    )
                    .unwrap(),
                PlannerAction::Complete
            );
        }
    }

    #[test]
    fn restricted_port_restricted_symmetric_and_cgnat_fall_through_to_relay() {
        for _nat_class in 0..4 {
            let p = plan(
                vec![direct(DirectCandidateKindV1::ServerReflexive, 7, 1, 900)],
                vec![relay(RelayTransportV1::QuicUdp, 9, 900)],
                8,
            );
            let mut planner = ConnectionPlanner::new(p.clone());
            planner.next(10, None).unwrap();
            assert!(matches!(
                planner.next(11, Some(gathered(&p))).unwrap(),
                PlannerAction::CheckDirect(_)
            ));
            assert!(matches!(
                planner
                    .next(
                        12,
                        Some(PlannerEvent::AttemptFailed(
                            RouteFailureCodeV1::DirectTimeout,
                        )),
                    )
                    .unwrap(),
                PlannerAction::EnsureRouteReservation { .. }
            ));
        }
    }

    #[test]
    fn upstream_udp_drop_uses_admitted_punch_then_tcp443_fallback() {
        let p = plan(vec![], vec![relay(RelayTransportV1::TlsTcp443, 5, 900)], 8);
        let mut planner = ConnectionPlanner::new(p.clone());
        planner.next(10, None).unwrap();
        assert!(matches!(
            planner.next(11, Some(gathered(&p))).unwrap(),
            PlannerAction::EnsureRouteReservation { .. }
        ));
        let admitted = AdmittedHolePunchCandidate::test_only(
            NodeId::from_bytes([5; 32]),
            [1; 32],
            [2; 32],
            [3; 32],
            20,
            900,
        );
        assert!(matches!(
            planner
                .next(12, Some(PlannerEvent::HolePunchAdmitted(admitted)))
                .unwrap(),
            PlannerAction::CoordinateHolePunch(_)
        ));
        assert!(matches!(
            planner
                .next(
                    13,
                    Some(PlannerEvent::AttemptFailed(
                        RouteFailureCodeV1::HolePunchFailed
                    ))
                )
                .unwrap(),
            PlannerAction::AssociateRelay {
                candidate: RelayCandidateV1 {
                    transport: RelayTransportV1::TlsTcp443,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn relay_connection_requires_admitted_association() {
        let candidate = relay(RelayTransportV1::QuicUdp, 4, 900);
        let p = plan(vec![], vec![candidate.clone()], 8);
        let mut planner = ConnectionPlanner::new(p.clone());
        planner.next(10, None).unwrap();
        planner.next(11, Some(gathered(&p))).unwrap();
        assert!(matches!(
            planner
                .next(
                    12,
                    Some(PlannerEvent::ReservationPairAdmitted {
                        relay: candidate.relay_node_id,
                        local_reservation_id: [6; 32],
                        remote_reservation_id: [7; 32]
                    })
                )
                .unwrap(),
            PlannerAction::AssociateRelay { .. }
        ));
        let path = AdmittedRelayPath::test_only(candidate, [8; 32], [6; 32], [7; 32], [9; 32]);
        assert!(matches!(
            planner
                .next(13, Some(PlannerEvent::RelayAssociationAdmitted(path)))
                .unwrap(),
            PlannerAction::ConnectRelay(_)
        ));
    }

    #[test]
    fn candidate_order_is_host_then_reflexive_then_udp_then_tcp443() {
        let p = plan(
            vec![
                direct(DirectCandidateKindV1::ProviderMapped, 100, 1, 900),
                direct(DirectCandidateKindV1::ServerReflexive, 1, 1, 900),
                direct(DirectCandidateKindV1::Host, 1, 1, 900),
            ],
            vec![
                relay(RelayTransportV1::TlsTcp443, 100, 900),
                relay(RelayTransportV1::QuicUdp, 1, 900),
            ],
            12,
        );
        let mut planner = ConnectionPlanner::new(p.clone());
        planner.next(1, None).unwrap();

        let first = planner.next(2, Some(gathered(&p))).unwrap();
        let PlannerAction::CheckDirect(first) = first else {
            panic!("expected first direct candidate");
        };
        assert_eq!(first.kind, DirectCandidateKindV1::Host);

        for expected_kind in [
            DirectCandidateKindV1::ServerReflexive,
            DirectCandidateKindV1::ProviderMapped,
        ] {
            let action = planner
                .next(
                    3,
                    Some(PlannerEvent::AttemptFailed(
                        RouteFailureCodeV1::DirectTimeout,
                    )),
                )
                .unwrap();
            let PlannerAction::CheckDirect(candidate) = action else {
                panic!("expected direct candidate");
            };
            assert_eq!(candidate.kind, expected_kind);
        }

        let action = planner
            .next(
                4,
                Some(PlannerEvent::AttemptFailed(
                    RouteFailureCodeV1::DirectTimeout,
                )),
            )
            .unwrap();
        let PlannerAction::EnsureRouteReservation { relay } = action else {
            panic!("expected relay reservation");
        };
        assert_eq!(relay, NodeId::from_bytes([1; 32]));

        let udp_candidate = p
            .relay_candidates
            .iter()
            .find(|candidate| candidate.transport == RelayTransportV1::QuicUdp)
            .unwrap()
            .clone();
        planner
            .next(
                5,
                Some(PlannerEvent::ReservationPairAdmitted {
                    relay,
                    local_reservation_id: [6; 32],
                    remote_reservation_id: [7; 32],
                }),
            )
            .unwrap();
        planner
            .next(
                6,
                Some(PlannerEvent::RelayAssociationAdmitted(
                    AdmittedRelayPath::test_only(udp_candidate, [8; 32], [6; 32], [7; 32], [9; 32]),
                )),
            )
            .unwrap();
        let next = planner
            .next(
                7,
                Some(PlannerEvent::AttemptFailed(
                    RouteFailureCodeV1::RelayUnavailable,
                )),
            )
            .unwrap();
        assert!(
            matches!(next, PlannerAction::EnsureRouteReservation { relay } if relay == NodeId::from_bytes([100; 32]))
        );
    }

    #[test]
    fn expired_candidates_network_change_and_deadline_fail_closed() {
        let p = plan(
            vec![direct(DirectCandidateKindV1::ServerReflexive, 1, 3, 9)],
            vec![],
            4,
        );
        let mut expired = ConnectionPlanner::new(p.clone());
        expired.next(1, None).unwrap();
        assert!(matches!(
            expired.next(10, Some(gathered(&p))),
            Err(RouteFailure::CandidateExpired)
        ));

        let mut changed = ConnectionPlanner::new(plan(
            vec![direct(DirectCandidateKindV1::Host, 1, 3, 900)],
            vec![],
            4,
        ));
        changed.next(1, None).unwrap();
        assert!(matches!(
            changed.next(2, Some(PlannerEvent::NetworkEpochChanged(4))),
            Err(RouteFailure::NetworkChanged)
        ));

        let mut deadline = ConnectionPlanner::new(plan(vec![], vec![], 4));
        deadline.next(1, None).unwrap();
        let failure = deadline.next(1_001, None).unwrap_err();
        assert!(
            matches!(failure, RouteFailure::PathLimited { limitations, .. } if limitations.iter().any(|l| l.code == RouteLimitationCodeV1::DeadlineExceeded))
        );
    }

    #[test]
    fn total_budget_exhaustion_is_honest_path_limited_not_global_unreachable() {
        let p = plan(
            vec![
                direct(DirectCandidateKindV1::Host, 9, 1, 900),
                direct(DirectCandidateKindV1::ServerReflexive, 8, 1, 900),
            ],
            vec![],
            1,
        );
        let mut planner = ConnectionPlanner::new(p.clone());
        planner.next(1, None).unwrap();
        planner.next(2, Some(gathered(&p))).unwrap();
        let failure = planner
            .next(
                3,
                Some(PlannerEvent::AttemptFailed(
                    RouteFailureCodeV1::DirectTimeout,
                )),
            )
            .unwrap_err();
        assert!(
            matches!(&failure, RouteFailure::PathLimited { attempts, limitations } if attempts.len() == 1 && limitations.iter().any(|l| l.code == RouteLimitationCodeV1::CandidateBudgetExhausted))
        );
        assert!(!format!("{failure:?}")
            .to_ascii_lowercase()
            .contains("globally unreachable"));
    }

    #[test]
    fn zero_resource_budget_and_oversized_candidate_sets_fail_closed() {
        let mut zero = plan(
            vec![direct(DirectCandidateKindV1::Host, 1, 1, 900)],
            vec![],
            4,
        );
        zero.resource_budget.max_probe_bytes = 0;
        let mut planner = ConnectionPlanner::new(zero.clone());
        planner.next(1, None).unwrap();
        assert_eq!(
            planner.next(2, Some(gathered(&zero))).unwrap_err(),
            RouteFailure::BudgetExceeded
        );

        let oversized = plan(
            (0..=onebrain_protocol::MAX_DIRECT_CANDIDATES)
                .map(|index| {
                    direct(
                        DirectCandidateKindV1::ServerReflexive,
                        index as u32 + 1,
                        1,
                        900,
                    )
                })
                .collect(),
            vec![],
            20,
        );
        let mut planner = ConnectionPlanner::new(oversized.clone());
        planner.next(1, None).unwrap();
        assert_eq!(
            planner.next(2, Some(gathered(&oversized))).unwrap_err(),
            RouteFailure::BudgetExceeded
        );
    }

    #[test]
    fn every_typed_failure_code_has_a_closed_mapping() {
        let cases = [
            (
                RouteFailureCodeV1::NoBootstrapReachable,
                RouteFailure::NoBootstrapReachable,
            ),
            (
                RouteFailureCodeV1::CandidateExpired,
                RouteFailure::CandidateExpired,
            ),
            (
                RouteFailureCodeV1::DirectTimeout,
                RouteFailure::DirectTimeout,
            ),
            (
                RouteFailureCodeV1::HolePunchFailed,
                RouteFailure::HolePunchFailed,
            ),
            (RouteFailureCodeV1::RelayDenied, RouteFailure::RelayDenied),
            (
                RouteFailureCodeV1::RelayUnavailable,
                RouteFailure::RelayUnavailable,
            ),
            (
                RouteFailureCodeV1::PeerIdentityMismatch,
                RouteFailure::PeerIdentityMismatch,
            ),
            (
                RouteFailureCodeV1::NetworkChanged,
                RouteFailure::NetworkChanged,
            ),
            (
                RouteFailureCodeV1::BudgetExceeded,
                RouteFailure::BudgetExceeded,
            ),
        ];
        for (code, expected) in cases {
            assert_eq!(RouteFailure::from(code), expected);
        }
    }
}
