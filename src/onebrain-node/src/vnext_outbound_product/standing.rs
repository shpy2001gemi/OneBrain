//! Standing reservations and explicitly opted-in, minimal advertisements.

use crate::vnext_reachability_manager::{ReachabilityError, ReachabilityManager};
use crate::vnext_reachability_replay_store::RedbReachabilityReplayStore;
use ku_net::vnext_reachability_crypto::{
    ReachabilityIdentitySigner, RelayAdmissionError, ValidatedRelayDescriptor,
};
use ku_net::vnext_relay_discovery::{ReachabilityFuture, VerifiedRelayDiscovery};
use ku_net::vnext_session::principal_node_id;
use onebrain_protocol::{
    decode_reachability_object, decode_relay_control, encode_reachability_object,
    encode_relay_control, reachability_signing_parts, relay_control_signing_parts,
    ReachabilityAdvertisementV1, ReachabilityObjectV1, ReachabilitySignatureRoleV1,
    RelayControlSignatureRoleV1, RelayControlV1, RelayReservationV1, RelayReserveRequestV1,
};
use rand::RngCore;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) struct StandingOwner {
    signer: Arc<dyn ReachabilityIdentitySigner>,
    replay: Arc<RedbReachabilityReplayStore>,
    capability_ceiling: Option<[u8; 32]>,
    published: Option<ReachabilityAdvertisementV1>,
    pub(crate) advertisement_state: &'static str,
    pub(crate) limitation: Option<&'static str>,
    pub(crate) reservation_statuses:
        std::collections::BTreeMap<ku_core::foundation::NodeId, super::OutboundReservationStatus>,
}

/// Private orchestration port; the product owner always supplies its existing
/// manager. No application can inject reservation success through this port.
trait StandingPorts: Sync {
    fn active(&self) -> ReachabilityFuture<'_, Vec<RelayReservationV1>>;
    fn reserve<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        request: RelayReserveRequestV1,
        current: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>;
    fn publish<'a>(
        &'a self,
        advertisement: &'a ReachabilityAdvertisementV1,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>>;
}

impl StandingPorts for ReachabilityManager {
    fn active(&self) -> ReachabilityFuture<'_, Vec<RelayReservationV1>> {
        Box::pin(async {
            self.reservations
                .active_reservations()
                .await
                .into_iter()
                .map(|value| value.canonical().clone())
                .collect()
        })
    }
    fn reserve<'a>(
        &'a self,
        relay: &'a ValidatedRelayDescriptor,
        request: RelayReserveRequestV1,
        current: &'a (dyn Fn() -> bool + Send + Sync),
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(async move {
            self.reservations
                .ensure_route_reservation_guarded(
                    relay,
                    request,
                    Instant::now() + Duration::from_secs(5),
                    current,
                )
                .await
                .map(|_| ())
        })
    }
    fn publish<'a>(
        &'a self,
        advertisement: &'a ReachabilityAdvertisementV1,
    ) -> ReachabilityFuture<'a, Result<(), ReachabilityError>> {
        Box::pin(ReachabilityManager::publish(self, advertisement))
    }
}

impl StandingOwner {
    pub(crate) fn new(
        signer: Arc<dyn ReachabilityIdentitySigner>,
        replay: Arc<RedbReachabilityReplayStore>,
        capability_ceiling: Option<[u8; 32]>,
    ) -> Self {
        Self {
            signer,
            replay,
            capability_ceiling,
            published: None,
            advertisement_state: "pending",
            limitation: None,
            reservation_statuses: std::collections::BTreeMap::new(),
        }
    }

    pub(crate) fn published_expiry(&self) -> Option<u64> {
        self.published.as_ref().map(|value| value.expires_at)
    }

    pub(crate) async fn maintain(
        &mut self,
        manager: &ReachabilityManager,
        advertise: bool,
        now: u64,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), ReachabilityError> {
        self.limitation = None;
        if !current() {
            return Err(ReachabilityError::NetworkChanged);
        }
        manager.reservations.maintain_keepalives(current).await?;
        let relays: Vec<_> = manager
            .discovery
            .read()
            .await
            .verified_relays()
            .filter(|relay| relay.canonical().expires_at > now)
            .take(6)
            .cloned()
            .collect();
        self.maintain_with(manager, relays, advertise, now, current)
            .await
    }

    async fn maintain_with(
        &mut self,
        ports: &dyn StandingPorts,
        relays: Vec<ValidatedRelayDescriptor>,
        advertise: bool,
        now: u64,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<(), ReachabilityError> {
        self.limitation = None;
        if !current() {
            return Err(ReachabilityError::NetworkChanged);
        }
        for relay in relays {
            if !current() {
                return Err(ReachabilityError::NetworkChanged);
            }
            let id = relay.canonical().relay_node_id;
            let active_all = ports.active().await;
            let active = active_all.iter().find(|reservation| {
                reservation.relay_node_id == id && reservation.expires_at > now
            });
            if active
                .as_ref()
                .is_some_and(|reservation| reservation.expires_at > now.saturating_add(180))
            {
                continue;
            }
            if active.is_none()
                && active_all
                    .iter()
                    .filter(|value| value.expires_at > now)
                    .count()
                    >= 3
            {
                continue;
            }
            let scope = local_scope(b"reservation", id.as_bytes());
            if self
                .replay
                .pending_local(scope)
                .map_err(ReachabilityError::Admission)?
                .is_some()
            {
                self.limitation = Some("reservation_reconcile_required");
                self.reservation_statuses.insert(
                    id,
                    super::OutboundReservationStatus {
                        relay_node_id: id,
                        state: "pending",
                        expires_at_unix_seconds: 0,
                        limitations: vec!["reservation_reconcile_required"],
                    },
                );
                continue;
            }
            let bytes = self
                .replay
                .prepare_local(scope, |sequence| {
                    make_reserve(self.signer.as_ref(), &relay, sequence, now)
                })
                .map_err(ReachabilityError::Admission)?;
            let RelayControlV1::Reserve(request) =
                decode_relay_control(&bytes).map_err(|_| ReachabilityError::CorruptState)?
            else {
                return Err(ReachabilityError::CorruptState);
            };
            if !current() {
                return Err(ReachabilityError::NetworkChanged);
            }
            self.reservation_statuses.insert(
                id,
                super::OutboundReservationStatus {
                    relay_node_id: id,
                    state: if active.is_some() {
                        "refreshing"
                    } else {
                        "pending"
                    },
                    expires_at_unix_seconds: request.expires_at,
                    limitations: vec![],
                },
            );
            let result = tokio::time::timeout(
                Duration::from_secs(5),
                ports.reserve(&relay, request, current),
            )
            .await;
            match result {
                Ok(Ok(_)) if current() => {
                    self.replay
                        .finish_local(scope, &bytes)
                        .map_err(ReachabilityError::Admission)?;
                    self.reservation_statuses.remove(&id);
                }
                _ => {
                    self.limitation = Some("reservation_reconcile_required");
                    if let Some(status) = self.reservation_statuses.get_mut(&id) {
                        status.state = "pending";
                        status.limitations = vec!["reservation_reconcile_required"];
                    }
                }
            }
        }
        // Keep only the bounded selected scope. Durable unknown requests remain
        // in the store and reappear if their relay is selected again.
        while self.reservation_statuses.len() > 6 {
            self.reservation_statuses.pop_last();
        }
        if ports
            .active()
            .await
            .iter()
            .filter(|value| value.expires_at > now)
            .count()
            < 2
        {
            self.limitation
                .get_or_insert("insufficient_relay_reservations");
        }
        if !advertise {
            self.advertisement_state = "disabled";
            return Ok(());
        }
        let Some(capability_ceiling) = self.capability_ceiling else {
            self.advertisement_state = "pending";
            self.limitation
                .get_or_insert("advertisement_capability_unavailable");
            return Ok(());
        };
        let mut reservations: Vec<_> = ports
            .active()
            .await
            .into_iter()
            .filter(|reservation| reservation.expires_at > now + 20)
            .collect();
        reservations
            .sort_by_key(|reservation| (reservation.relay_node_id, reservation.reservation_id));
        if reservations.is_empty() {
            self.advertisement_state = if self
                .published
                .as_ref()
                .is_some_and(|value| value.expires_at <= now)
            {
                "expired"
            } else {
                "pending"
            };
            return Ok(());
        }
        if self.published.as_ref().is_some_and(|previous| {
            previous.expires_at > now + 20 && previous.relay_reservations == reservations
        }) {
            self.advertisement_state = "published";
            return Ok(());
        }
        let scope = local_scope(b"advertisement", &self.signer.public_key());
        let bytes = if let Some(bytes) = self
            .replay
            .pending_local(scope)
            .map_err(ReachabilityError::Admission)?
        {
            bytes
        } else {
            if !current() {
                return Err(ReachabilityError::NetworkChanged);
            }
            self.replay
                .prepare_local(scope, |sequence| {
                    let mut advertisement = ReachabilityAdvertisementV1 {
                        format: 1,
                        target_node_id: principal_node_id(&self.signer.public_key()),
                        expires_at: reservations
                            .iter()
                            .map(|value| value.expires_at)
                            .min()
                            .unwrap_or(now)
                            .min(now + 300),
                        relay_reservations: reservations.clone(),
                        optional_public_candidates: vec![],
                        capability_ceiling,
                        sequence,
                        issued_at: now,
                        target_signature: [0; 64],
                    };
                    advertisement.target_signature = sign(
                        self.signer.as_ref(),
                        &ReachabilityObjectV1::Advertisement(advertisement.clone()),
                        ReachabilitySignatureRoleV1::AdvertisementTarget,
                    )?;
                    encode_reachability_object(&ReachabilityObjectV1::Advertisement(advertisement))
                        .map_err(|_| RelayAdmissionError::Codec)
                })
                .map_err(ReachabilityError::Admission)?
        };
        let ReachabilityObjectV1::Advertisement(advertisement) =
            decode_reachability_object(&bytes).map_err(|_| ReachabilityError::CorruptState)?
        else {
            return Err(ReachabilityError::CorruptState);
        };
        // Never replay an advertisement with expired or replaced carrier grants.
        if advertisement.expires_at <= now || advertisement.relay_reservations != reservations {
            self.advertisement_state = "degraded";
            self.limitation = Some("advertisement_reconcile_required");
            return Ok(());
        }
        if !current() {
            return Err(ReachabilityError::NetworkChanged);
        }
        self.advertisement_state = "pending";
        match tokio::time::timeout(Duration::from_secs(5), ports.publish(&advertisement)).await {
            Ok(Ok(())) if current() => {
                self.replay
                    .finish_local(scope, &bytes)
                    .map_err(ReachabilityError::Admission)?;
                self.published = Some(advertisement);
                self.advertisement_state = "published";
            }
            _ => {
                self.advertisement_state = "degraded";
                self.limitation = Some("advertisement_publication_unavailable");
            }
        }
        Ok(())
    }
}

fn local_scope(kind: &[u8], identity: &[u8; 32]) -> [u8; 32] {
    let mut bytes = b"local-obp-scheduling/1/".to_vec();
    bytes.extend(kind);
    bytes.extend(identity);
    *blake3::hash(&bytes).as_bytes()
}

fn sign(
    signer: &dyn ReachabilityIdentitySigner,
    value: &ReachabilityObjectV1,
    role: ReachabilitySignatureRoleV1,
) -> Result<[u8; 64], RelayAdmissionError> {
    let (domain, bytes) =
        reachability_signing_parts(value, role).map_err(|_| RelayAdmissionError::Codec)?;
    signer
        .sign_reachability_message(domain, &bytes)
        .map_err(|_| RelayAdmissionError::StateUnavailable)
}

fn make_reserve(
    signer: &dyn ReachabilityIdentitySigner,
    relay: &ku_net::vnext_reachability_crypto::ValidatedRelayDescriptor,
    sequence: u64,
    now: u64,
) -> Result<Vec<u8>, RelayAdmissionError> {
    let mut reservation_id = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut reservation_id);
    let mut request = RelayReserveRequestV1 {
        format: 1,
        relay_node_id: relay.canonical().relay_node_id,
        target_node_id: principal_node_id(&signer.public_key()),
        reservation_id,
        transport_scope: relay.canonical().supported_transports.clone(),
        sequence,
        issued_at: now,
        expires_at: now + 900,
        target_reservation_signature: [0; 64],
        target_request_signature: [0; 64],
    };
    let grant = RelayReservationV1 {
        format: 1,
        relay_node_id: request.relay_node_id,
        target_node_id: request.target_node_id,
        reservation_id,
        transport_scope: request.transport_scope.clone(),
        issued_at: now,
        expires_at: request.expires_at,
        target_signature: [0; 64],
        relay_signature: [0; 64],
    };
    request.target_reservation_signature = sign(
        signer,
        &ReachabilityObjectV1::RelayReservation(grant),
        ReachabilitySignatureRoleV1::ReservationTarget,
    )?;
    let (domain, bytes) = relay_control_signing_parts(
        &RelayControlV1::Reserve(request.clone()),
        RelayControlSignatureRoleV1::ReserveRequestTarget,
    )
    .map_err(|_| RelayAdmissionError::Codec)?;
    request.target_request_signature = signer
        .sign_reachability_message(domain, &bytes)
        .map_err(|_| RelayAdmissionError::StateUnavailable)?;
    encode_relay_control(&RelayControlV1::Reserve(request)).map_err(|_| RelayAdmissionError::Codec)
}

#[cfg(test)]
#[path = "standing_tests.rs"]
mod tests;
