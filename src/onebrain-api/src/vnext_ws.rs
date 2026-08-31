//! Private, bounded vNext WebSocket projection.
//!
//! Browser-compatible WebSocket authentication uses a short-lived, single-use
//! ticket minted by a Bearer-authenticated REST request. The ticket binds an
//! immutable topic set and a random client-session capability. vNext events
//! are routed only to that exact session; there is no shared vNext broadcast
//! channel.

#![cfg_attr(not(feature = "vnext-network-runtime"), allow(dead_code))]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use futures::StreamExt;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::server::AppState;
use crate::types::ApiErrorResponse;
use crate::vnext_api::{
    active_meta, success, CoverageV1, LifecycleV1, VNextHttpError, VNextResult,
};

pub const VNEXT_WS_PROFILE: &str = "VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1";
pub const VNEXT_WS_CLIENT_SESSION_HEADER: &str = "x-onebrain-vnext-client-session";

const TOKEN_PREFIX: &str = "obw1.";
const TOKEN_BYTES: usize = 32;
const MAX_TOPICS: usize = 4;
const MAX_PENDING_TICKETS: usize = 128;
const MAX_ACTIVE_SESSIONS: usize = 64;
const EVENT_QUEUE_CAPACITY: usize = 32;
const TICKET_TTL: Duration = Duration::from_secs(30);
const SESSION_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_WS_MESSAGE_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum VNextWsTopicV1 {
    Matches,
    Publications,
    Views,
    Runtime,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VNextWsTicketRequestV1 {
    pub subscriptions: Vec<VNextWsTopicV1>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct VNextWsTicketV1 {
    pub ticket: String,
    pub client_session: String,
    pub expires_at: u64,
    pub session_expires_at: u64,
    pub subscriptions: Vec<VNextWsTopicV1>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VNextWsEventTypeV1 {
    SubscriptionReady,
    BoundedMatchAvailable,
    PublicationQueued,
    PublicationDelivered,
    PublicationDeferred,
    ViewRevision,
    ViewConflict,
    LaneActive,
    LaneDisabled,
    LaneDegraded,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VNextWsEventDataV1 {
    SubscriptionReady {
        subscriptions: Vec<VNextWsTopicV1>,
        session_expires_at: u64,
    },
    BoundedMatchAvailable {
        new_match_count: u64,
        state: &'static str,
        executable: bool,
    },
    PublicationState {
        state: &'static str,
        revision: u64,
        delivery_acknowledged: bool,
    },
    ViewState {
        revision: u64,
        conflict_count: u64,
        establishes_truth: bool,
        establishes_benefit: bool,
        authorizes_reward: bool,
        claims_global_completion: bool,
    },
    LaneState {
        lane: &'static str,
        compiled: bool,
        requested: bool,
        active: bool,
        kill_switch: bool,
        signer_ready: bool,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct VNextWsEventV1 {
    pub profile: &'static str,
    pub event_type: VNextWsEventTypeV1,
    pub sequence: u64,
    pub timestamp: u64,
    pub lifecycle: LifecycleV1,
    pub coverage: CoverageV1,
    pub limitations: Vec<String>,
    pub data: VNextWsEventDataV1,
}

#[derive(Clone, Copy)]
struct HubLimits {
    max_pending_tickets: usize,
    max_active_sessions: usize,
    event_queue_capacity: usize,
    ticket_ttl: Duration,
    session_ttl: Duration,
}

impl Default for HubLimits {
    fn default() -> Self {
        Self {
            max_pending_tickets: MAX_PENDING_TICKETS,
            max_active_sessions: MAX_ACTIVE_SESSIONS,
            event_queue_capacity: EVENT_QUEUE_CAPACITY,
            ticket_ttl: TICKET_TTL,
            session_ttl: SESSION_TTL,
        }
    }
}

#[derive(Clone)]
pub struct VNextWsHub {
    inner: Arc<Mutex<HubState>>,
    limits: HubLimits,
}

impl Default for VNextWsHub {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HubState::default())),
            limits: HubLimits::default(),
        }
    }
}

#[derive(Default)]
struct HubState {
    pending: BTreeMap<[u8; TOKEN_BYTES], PendingTicket>,
    active: BTreeMap<[u8; TOKEN_BYTES], ActiveSession>,
}

struct PendingTicket {
    client_session: [u8; TOKEN_BYTES],
    subscriptions: BTreeSet<VNextWsTopicV1>,
    ticket_expires: Instant,
    session_expires: Instant,
    session_expires_at: u64,
}

struct ActiveSession {
    subscriptions: BTreeSet<VNextWsTopicV1>,
    sender: mpsc::Sender<VNextWsEventV1>,
    expires: Instant,
    next_sequence: u64,
    view_states: BTreeMap<[u8; 32], ([u8; 32], u64)>,
}

struct AcceptedSession {
    client_session: [u8; TOKEN_BYTES],
    ready: VNextWsEventV1,
    receiver: mpsc::Receiver<VNextWsEventV1>,
    expires: Instant,
}

#[derive(Clone)]
struct EventDraft {
    topic: VNextWsTopicV1,
    event_type: VNextWsEventTypeV1,
    lifecycle: LifecycleV1,
    coverage: CoverageV1,
    limitations: Vec<String>,
    data: VNextWsEventDataV1,
}

#[derive(Clone, Copy)]
struct LaneSnapshot {
    lane: &'static str,
    compiled: bool,
    requested: bool,
    active: bool,
    kill_switch: bool,
    signer_ready: bool,
    coverage: CoverageV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeliveryOutcome {
    Sent,
    NoSession,
    NotSubscribed,
    DisconnectedBackpressure,
}

impl VNextWsHub {
    fn lock(&self) -> std::sync::MutexGuard<'_, HubState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn issue_ticket(
        &self,
        request: VNextWsTicketRequestV1,
    ) -> Result<VNextWsTicketV1, VNextHttpError> {
        let subscriptions = request
            .subscriptions
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if subscriptions.is_empty()
            || subscriptions.len() > MAX_TOPICS
            || subscriptions.len() != request.subscriptions.len()
        {
            return Err(VNextHttpError::invalid(
                "subscriptions must contain 1..4 unique vNext topics",
            ));
        }

        let now = Instant::now();
        let mut state = self.lock();
        prune_expired(&mut state, now);
        if state.pending.len() >= self.limits.max_pending_tickets
            || state.active.len() >= self.limits.max_active_sessions
        {
            return Err(VNextHttpError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "private WebSocket admission capacity reached",
                true,
                LifecycleV1::Degraded,
                CoverageV1::LocalOnly,
                ["allow_existing_tickets_or_sessions_to_expire"],
            ));
        }

        let ticket = fresh_token(&state, true)?;
        let client_session = fresh_token(&state, false)?;
        let ticket_expires = now + self.limits.ticket_ttl;
        let session_expires = now + self.limits.session_ttl;
        let now_epoch = now_epoch();
        let expires_at = now_epoch.saturating_add(self.limits.ticket_ttl.as_secs());
        let session_expires_at = now_epoch.saturating_add(self.limits.session_ttl.as_secs());
        state.pending.insert(
            ticket,
            PendingTicket {
                client_session,
                subscriptions: subscriptions.clone(),
                ticket_expires,
                session_expires,
                session_expires_at,
            },
        );
        Ok(VNextWsTicketV1 {
            ticket: encode_token(ticket),
            client_session: encode_token(client_session),
            expires_at,
            session_expires_at,
            subscriptions: subscriptions.into_iter().collect(),
            limitations: vec![
                "ticket_is_single_use_and_short_lived".into(),
                "subscription_scope_is_immutable_for_the_session".into(),
                "client_session_header_is_required_for_targeted_rest_events".into(),
                "events_are_hints_to_refetch_authoritative_local_rest_state".into(),
            ],
        })
    }

    fn accept_ticket(&self, encoded: &str) -> Result<AcceptedSession, ()> {
        let ticket = decode_token(encoded).ok_or(())?;
        let now = Instant::now();
        let mut state = self.lock();
        prune_expired(&mut state, now);
        let pending = state.pending.remove(&ticket).ok_or(())?;
        if pending.ticket_expires <= now
            || pending.session_expires <= now
            || state.active.len() >= self.limits.max_active_sessions
            || state.active.contains_key(&pending.client_session)
        {
            return Err(());
        }

        let (sender, receiver) = mpsc::channel(self.limits.event_queue_capacity);
        let ready = VNextWsEventV1 {
            profile: VNEXT_WS_PROFILE,
            event_type: VNextWsEventTypeV1::SubscriptionReady,
            sequence: 1,
            timestamp: now_epoch(),
            lifecycle: LifecycleV1::Active,
            coverage: CoverageV1::LocalOnly,
            limitations: vec![
                "subscription_is_bound_to_one_authenticated_client_session".into(),
                "events_do_not_replace_rest_state_reads".into(),
            ],
            data: VNextWsEventDataV1::SubscriptionReady {
                subscriptions: pending.subscriptions.iter().copied().collect(),
                session_expires_at: pending.session_expires_at,
            },
        };
        state.active.insert(
            pending.client_session,
            ActiveSession {
                subscriptions: pending.subscriptions,
                sender,
                expires: pending.session_expires,
                next_sequence: 2,
                view_states: BTreeMap::new(),
            },
        );
        Ok(AcceptedSession {
            client_session: pending.client_session,
            ready,
            receiver,
            expires: pending.session_expires,
        })
    }

    fn remove_session(&self, client_session: [u8; TOKEN_BYTES]) {
        self.lock().active.remove(&client_session);
    }

    fn publish_from_headers(&self, headers: &HeaderMap, draft: EventDraft) -> DeliveryOutcome {
        let Some(client_session) = client_session_from_headers(headers) else {
            return DeliveryOutcome::NoSession;
        };
        self.publish_to_session(client_session, draft)
    }

    fn publish_to_session(
        &self,
        client_session: [u8; TOKEN_BYTES],
        draft: EventDraft,
    ) -> DeliveryOutcome {
        let now = Instant::now();
        let mut state = self.lock();
        prune_expired(&mut state, now);
        let outcome = match state.active.get_mut(&client_session) {
            Some(session) => enqueue(session, draft),
            None => DeliveryOutcome::NoSession,
        };
        if outcome == DeliveryOutcome::DisconnectedBackpressure {
            state.active.remove(&client_session);
        }
        outcome
    }

    pub(crate) fn publish_bounded_match(&self, headers: &HeaderMap, new_match_count: usize) {
        if new_match_count == 0 {
            return;
        }
        let _ = self.publish_from_headers(
            headers,
            EventDraft {
                topic: VNextWsTopicV1::Matches,
                event_type: VNextWsEventTypeV1::BoundedMatchAvailable,
                lifecycle: LifecycleV1::Active,
                coverage: CoverageV1::Partial,
                limitations: vec![
                    "quarantined_non_executable_match_hint_only".into(),
                    "standing_need_private_target_and_proposal_are_not_exported".into(),
                    "refetch_matches_over_authenticated_rest".into(),
                ],
                data: VNextWsEventDataV1::BoundedMatchAvailable {
                    new_match_count: u64::try_from(new_match_count).unwrap_or(u64::MAX),
                    state: "quarantined",
                    executable: false,
                },
            },
        );
    }

    pub(crate) fn publish_publication_state(
        &self,
        headers: &HeaderMap,
        state: &str,
        revision: u64,
        delivery_acknowledged: bool,
    ) {
        let (event_type, state_literal, acknowledged) = match state {
            "pending" => (VNextWsEventTypeV1::PublicationQueued, "pending", false),
            "deferred" => (VNextWsEventTypeV1::PublicationDeferred, "deferred", false),
            "delivered" if delivery_acknowledged => {
                (VNextWsEventTypeV1::PublicationDelivered, "delivered", true)
            }
            _ => return,
        };
        let _ = self.publish_from_headers(
            headers,
            EventDraft {
                topic: VNextWsTopicV1::Publications,
                event_type,
                lifecycle: if state_literal == "deferred" {
                    LifecycleV1::Degraded
                } else {
                    LifecycleV1::Active
                },
                coverage: CoverageV1::Partial,
                limitations: vec![
                    "publication_state_is_local_and_path_limited".into(),
                    "use_evidence_does_not_establish_truth_benefit_or_reward".into(),
                    if acknowledged {
                        "delivery_requires_a_durable_authenticated_acknowledgement".into()
                    } else {
                        "delivery_acknowledgement_has_not_been_observed".into()
                    },
                ],
                data: VNextWsEventDataV1::PublicationState {
                    state: state_literal,
                    revision,
                    delivery_acknowledged: acknowledged,
                },
            },
        );
    }

    pub(crate) fn publish_view_state(
        &self,
        headers: &HeaderMap,
        target: [u8; 32],
        revision: u64,
        conflicts: &[String],
    ) {
        let Some(client_session) = client_session_from_headers(headers) else {
            return;
        };
        let mut fingerprint = blake3::Hasher::new();
        fingerprint.update(b"onebrain:vnext:ws-view-state:1\0");
        fingerprint.update(&target);
        for conflict in conflicts {
            fingerprint.update(&(conflict.len() as u64).to_be_bytes());
            fingerprint.update(conflict.as_bytes());
        }
        let fingerprint = *fingerprint.finalize().as_bytes();
        let now = Instant::now();
        let mut state = self.lock();
        prune_expired(&mut state, now);
        let Some(session) = state.active.get_mut(&client_session) else {
            return;
        };
        if !session.subscriptions.contains(&VNextWsTopicV1::Views) {
            return;
        }
        if session.view_states.get(&target) == Some(&(fingerprint, revision)) {
            return;
        }
        session.view_states.insert(target, (fingerprint, revision));
        let has_conflicts = !conflicts.is_empty();
        let outcome = enqueue(
            session,
            EventDraft {
                topic: VNextWsTopicV1::Views,
                event_type: if has_conflicts {
                    VNextWsEventTypeV1::ViewConflict
                } else {
                    VNextWsEventTypeV1::ViewRevision
                },
                lifecycle: if has_conflicts {
                    LifecycleV1::Degraded
                } else {
                    LifecycleV1::Active
                },
                coverage: CoverageV1::Partial,
                limitations: vec![
                    "target_policy_frontier_and_event_ids_are_not_exported".into(),
                    "view_is_relative_to_local_authority_and_policy".into(),
                    "view_does_not_establish_truth_benefit_reward_or_global_completion".into(),
                ],
                data: VNextWsEventDataV1::ViewState {
                    revision,
                    conflict_count: u64::try_from(conflicts.len()).unwrap_or(u64::MAX),
                    establishes_truth: false,
                    establishes_benefit: false,
                    authorizes_reward: false,
                    claims_global_completion: false,
                },
            },
        );
        if outcome == DeliveryOutcome::DisconnectedBackpressure {
            state.active.remove(&client_session);
        }
    }

    fn publish_lane_state(&self, client_session: [u8; TOKEN_BYTES], snapshot: LaneSnapshot) {
        let degraded = snapshot.requested && (!snapshot.active || snapshot.kill_switch);
        let (event_type, lifecycle) = if !snapshot.compiled || !snapshot.requested {
            (VNextWsEventTypeV1::LaneDisabled, LifecycleV1::Disabled)
        } else if degraded {
            (VNextWsEventTypeV1::LaneDegraded, LifecycleV1::Degraded)
        } else {
            (VNextWsEventTypeV1::LaneActive, LifecycleV1::Active)
        };
        let mut limitations = vec!["lane_status_is_a_local_runtime_snapshot".into()];
        if !snapshot.compiled {
            limitations.push("vnext_network_runtime_not_compiled".into());
        }
        if snapshot.kill_switch {
            limitations.push("lane_kill_switch_is_active".into());
        }
        if snapshot.requested && !snapshot.active {
            limitations.push("requested_lane_has_no_ready_runtime_dependency".into());
        }
        let _ = self.publish_to_session(
            client_session,
            EventDraft {
                topic: VNextWsTopicV1::Runtime,
                event_type,
                lifecycle,
                coverage: snapshot.coverage,
                limitations,
                data: VNextWsEventDataV1::LaneState {
                    lane: snapshot.lane,
                    compiled: snapshot.compiled,
                    requested: snapshot.requested,
                    active: snapshot.active,
                    kill_switch: snapshot.kill_switch,
                    signer_ready: snapshot.signer_ready,
                },
            },
        );
    }

    #[cfg(test)]
    fn with_limits(event_queue_capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HubState::default())),
            limits: HubLimits {
                event_queue_capacity,
                ..HubLimits::default()
            },
        }
    }

    #[cfg(test)]
    fn active_session_count(&self) -> usize {
        self.lock().active.len()
    }

    #[cfg(test)]
    pub(crate) fn open_test_session(
        &self,
        subscriptions: &[VNextWsTopicV1],
    ) -> (String, mpsc::Receiver<VNextWsEventV1>) {
        let ticket = self
            .issue_ticket(VNextWsTicketRequestV1 {
                subscriptions: subscriptions.to_vec(),
            })
            .expect("test ticket");
        let accepted = self.accept_ticket(&ticket.ticket).expect("test session");
        (ticket.client_session, accepted.receiver)
    }
}

fn enqueue(session: &mut ActiveSession, draft: EventDraft) -> DeliveryOutcome {
    if !session.subscriptions.contains(&draft.topic) {
        return DeliveryOutcome::NotSubscribed;
    }
    let event = VNextWsEventV1 {
        profile: VNEXT_WS_PROFILE,
        event_type: draft.event_type,
        sequence: session.next_sequence,
        timestamp: now_epoch(),
        lifecycle: draft.lifecycle,
        coverage: draft.coverage,
        limitations: draft.limitations,
        data: draft.data,
    };
    match session.sender.try_send(event) {
        Ok(()) => {
            session.next_sequence = session.next_sequence.saturating_add(1);
            DeliveryOutcome::Sent
        }
        Err(mpsc::error::TrySendError::Full(_)) | Err(mpsc::error::TrySendError::Closed(_)) => {
            DeliveryOutcome::DisconnectedBackpressure
        }
    }
}

fn prune_expired(state: &mut HubState, now: Instant) {
    state
        .pending
        .retain(|_, pending| pending.ticket_expires > now && pending.session_expires > now);
    state
        .active
        .retain(|_, session| session.expires > now && !session.sender.is_closed());
}

fn fresh_token(state: &HubState, ticket: bool) -> Result<[u8; TOKEN_BYTES], VNextHttpError> {
    for _ in 0..8 {
        let mut bytes = [0u8; TOKEN_BYTES];
        OsRng.fill_bytes(&mut bytes);
        let collision = if ticket {
            state.pending.contains_key(&bytes)
        } else {
            state.active.contains_key(&bytes)
                || state
                    .pending
                    .values()
                    .any(|pending| pending.client_session == bytes)
        };
        if bytes != [0; TOKEN_BYTES] && !collision {
            return Ok(bytes);
        }
    }
    Err(VNextHttpError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "could not allocate a unique private WebSocket capability",
        false,
        LifecycleV1::Degraded,
        CoverageV1::LocalOnly,
        ["no_ticket_or_session_was_created"],
    ))
}

fn encode_token(token: [u8; TOKEN_BYTES]) -> String {
    format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(token))
}

fn decode_token(value: &str) -> Option<[u8; TOKEN_BYTES]> {
    if !value.starts_with(TOKEN_PREFIX) || value.len() > 64 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(&value[TOKEN_PREFIX.len()..]).ok()?;
    decoded
        .try_into()
        .ok()
        .filter(|bytes| *bytes != [0; TOKEN_BYTES])
}

fn client_session_from_headers(headers: &HeaderMap) -> Option<[u8; TOKEN_BYTES]> {
    let value = headers.get(VNEXT_WS_CLIENT_SESSION_HEADER)?.to_str().ok()?;
    decode_token(value)
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn create_ticket(
    State(state): State<AppState>,
    body: Result<Json<VNextWsTicketRequestV1>, JsonRejection>,
) -> VNextResult<VNextWsTicketV1> {
    let request = body
        .map(|Json(request)| request)
        .map_err(|error| VNextHttpError::invalid(format!("invalid JSON body: {error}")))?;
    let ticket = state.vnext_ws.issue_ticket(request)?;
    Ok(success(
        ticket,
        active_meta(
            CoverageV1::LocalOnly,
            [
                "ticket_is_single_use_and_short_lived",
                "subscription_scope_is_bound_before_upgrade",
                "websocket_events_are_non_authoritative_refetch_hints",
            ],
            None,
        ),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VNextWsTicketQuery {
    ticket: String,
}

pub async fn vnext_ws_events(
    State(state): State<AppState>,
    query: Result<Query<VNextWsTicketQuery>, QueryRejection>,
    ws: WebSocketUpgrade,
) -> Response {
    let ticket = match query {
        Ok(Query(query)) => query.ticket,
        Err(_) => return websocket_auth_error(),
    };
    let accepted = match state.vnext_ws.accept_ticket(&ticket) {
        Ok(accepted) => accepted,
        Err(()) => return websocket_auth_error(),
    };
    ws.max_message_size(MAX_WS_MESSAGE_BYTES)
        .max_frame_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| websocket_session(socket, state, accepted))
        .into_response()
}

fn websocket_auth_error() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiErrorResponse::new(
            "AUTH_REQUIRED",
            "Missing, invalid, expired, or already-used vNext WebSocket ticket",
        )),
    )
        .into_response()
}

async fn websocket_session(mut socket: WebSocket, state: AppState, mut accepted: AcceptedSession) {
    if send_event(&mut socket, &accepted.ready).await.is_err() {
        state.vnext_ws.remove_session(accepted.client_session);
        return;
    }
    emit_initial_lane_status(&state, accepted.client_session).await;
    let expiry = tokio::time::sleep_until(tokio::time::Instant::from_std(accepted.expires));
    tokio::pin!(expiry);

    loop {
        tokio::select! {
            event = accepted.receiver.recv() => {
                match event {
                    Some(event) if send_event(&mut socket, &event).await.is_ok() => {}
                    _ => {
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => {
                        // Subscription state is immutable and ticket-bound.
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            () = &mut expiry => {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
        }
    }
    state.vnext_ws.remove_session(accepted.client_session);
}

async fn send_event(socket: &mut WebSocket, event: &VNextWsEventV1) -> Result<(), ()> {
    let json = serde_json::to_string(event).map_err(|_| ())?;
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| ())
}

async fn emit_initial_lane_status(state: &AppState, client_session: [u8; TOKEN_BYTES]) {
    let (enabled, killed, standalone) = {
        let node = state.node.lock().await;
        (
            node.config().vnext.enabled,
            node.config().vnext.kill_switches,
            node.vnext_status().reachability.standalone,
        )
    };
    let compiled = cfg!(feature = "vnext-network-runtime");
    #[cfg(feature = "vnext-network-runtime")]
    let runtime_active = state.vnext_product_services().await.is_some();
    #[cfg(not(feature = "vnext-network-runtime"))]
    let runtime_active = false;
    let signer_ready = state.vnext_rest.signer_ready();
    let coverage = if standalone {
        CoverageV1::LocalOnly
    } else {
        CoverageV1::Partial
    };
    let lanes = [
        ("obp_rp", enabled.obp_rp, killed.obp_rp, true),
        (
            "distributed_kql_one_hop",
            enabled.distributed_kql_one_hop,
            killed.distributed_kql_one_hop,
            true,
        ),
        (
            "public_use_evidence_publish",
            enabled.public_use_evidence_publish,
            killed.public_use_evidence_publish,
            signer_ready,
        ),
        (
            "distributed_pomv_view",
            enabled.distributed_pomv_view,
            killed.distributed_pomv_view,
            true,
        ),
    ];
    for (lane, requested, kill_switch, dependency_ready) in lanes {
        state.vnext_ws.publish_lane_state(
            client_session,
            LaneSnapshot {
                lane,
                compiled,
                requested,
                active: compiled && runtime_active && requested && !kill_switch && dependency_ready,
                kill_switch,
                signer_ready,
                coverage,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::{Method, Request};
    use futures::StreamExt;
    use serde_json::{json, Value};
    use tokio_tungstenite::connect_async;
    use tower::ServiceExt;

    use super::*;
    use crate::ApiServer;

    async fn test_node(directory: &tempfile::TempDir) -> onebrain_node::OneBrainNode {
        let config = onebrain_node::NodeConfig {
            port: 0,
            data_dir: directory.path().canonicalize().unwrap(),
            concept_registry_mode: onebrain_node::ConceptRegistryMode::Disabled,
            ..Default::default()
        };
        onebrain_node::OneBrainNode::new(config).await.unwrap()
    }

    fn ticket_request(topics: &[VNextWsTopicV1]) -> VNextWsTicketRequestV1 {
        VNextWsTicketRequestV1 {
            subscriptions: topics.to_vec(),
        }
    }

    fn session_headers(session: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(VNEXT_WS_CLIENT_SESSION_HEADER, session.parse().unwrap());
        headers
    }

    #[test]
    fn tickets_are_single_use_and_bind_an_immutable_unique_scope() {
        let hub = VNextWsHub::default();
        assert!(hub.issue_ticket(ticket_request(&[])).is_err());
        assert!(hub
            .issue_ticket(ticket_request(&[
                VNextWsTopicV1::Matches,
                VNextWsTopicV1::Matches,
            ]))
            .is_err());

        let ticket = hub
            .issue_ticket(ticket_request(&[
                VNextWsTopicV1::Matches,
                VNextWsTopicV1::Runtime,
            ]))
            .unwrap();
        let accepted = hub.accept_ticket(&ticket.ticket).unwrap();
        assert_eq!(
            accepted.ready.data,
            VNextWsEventDataV1::SubscriptionReady {
                subscriptions: vec![VNextWsTopicV1::Matches, VNextWsTopicV1::Runtime],
                session_expires_at: ticket.session_expires_at,
            }
        );
        assert!(hub.accept_ticket(&ticket.ticket).is_err());
    }

    #[test]
    fn topic_and_client_scope_prevent_cross_client_delivery_and_private_fields() {
        let hub = VNextWsHub::default();
        let a = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Matches]))
            .unwrap();
        let b = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Publications]))
            .unwrap();
        let mut accepted_a = hub.accept_ticket(&a.ticket).unwrap();
        let mut accepted_b = hub.accept_ticket(&b.ticket).unwrap();

        hub.publish_bounded_match(&session_headers(&a.client_session), 2);
        let event = accepted_a.receiver.try_recv().unwrap();
        assert_eq!(event.event_type, VNextWsEventTypeV1::BoundedMatchAvailable);
        assert_eq!(event.sequence, 2);
        assert!(accepted_b.receiver.try_recv().is_err());

        let wire = serde_json::to_string(&event).unwrap();
        for forbidden in [
            "standing_need_id",
            "query_definition_cid",
            "target_cid",
            "proposal_cid",
            "single_use_receipt",
            "UniquePrivateMarker",
        ] {
            assert!(!wire.contains(forbidden));
        }
        assert!(wire.contains("\"state\":\"quarantined\""));
        assert!(wire.contains("\"executable\":false"));
    }

    #[test]
    fn bounded_backpressure_disconnects_only_the_slow_session() {
        let hub = VNextWsHub::with_limits(2);
        let a = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Matches]))
            .unwrap();
        let b = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Matches]))
            .unwrap();
        let mut accepted_a = hub.accept_ticket(&a.ticket).unwrap();
        let mut accepted_b = hub.accept_ticket(&b.ticket).unwrap();

        hub.publish_bounded_match(&session_headers(&a.client_session), 1);
        hub.publish_bounded_match(&session_headers(&a.client_session), 1);
        hub.publish_bounded_match(&session_headers(&a.client_session), 1);
        assert_eq!(hub.active_session_count(), 1);

        hub.publish_bounded_match(&session_headers(&b.client_session), 1);
        assert!(accepted_b.receiver.try_recv().is_ok());
        assert!(accepted_a.receiver.try_recv().is_ok());
    }

    #[test]
    fn publication_and_view_events_are_typed_fail_closed_and_deduplicated() {
        let hub = VNextWsHub::default();
        let (session, mut receiver) =
            hub.open_test_session(&[VNextWsTopicV1::Publications, VNextWsTopicV1::Views]);
        let headers = session_headers(&session);

        hub.publish_publication_state(&headers, "pending", 4, false);
        hub.publish_publication_state(&headers, "delivered", 5, false);
        let publication = receiver.try_recv().unwrap();
        assert_eq!(
            publication.event_type,
            VNextWsEventTypeV1::PublicationQueued
        );
        assert!(receiver.try_recv().is_err());

        let conflicts = vec!["private-event-id-is-not-exported".to_string()];
        hub.publish_view_state(&headers, [0x71; 32], 8, &conflicts);
        hub.publish_view_state(&headers, [0x71; 32], 8, &conflicts);
        let view = receiver.try_recv().unwrap();
        assert_eq!(view.event_type, VNextWsEventTypeV1::ViewConflict);
        assert!(receiver.try_recv().is_err());
        let wire = serde_json::to_value(view).unwrap();
        assert_eq!(wire["data"]["establishes_truth"], false);
        assert_eq!(wire["data"]["establishes_benefit"], false);
        assert_eq!(wire["data"]["authorizes_reward"], false);
        assert_eq!(wire["data"]["claims_global_completion"], false);
        assert!(!wire
            .to_string()
            .contains("private-event-id-is-not-exported"));
    }

    #[tokio::test]
    async fn ticket_route_requires_bearer_authentication() {
        let directory = tempfile::tempdir().unwrap();
        let node = test_node(&directory).await;
        let router = ApiServer::new(node, "p3-ws-token".into(), 0).build_router();
        let body = Body::from(serde_json::to_vec(&json!({"subscriptions": ["matches"]})).unwrap());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/vnext/ws/tickets")
                    .header("Content-Type", "application/json")
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/vnext/ws/tickets")
                    .header("Authorization", "Bearer p3-ws-token")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({"subscriptions": ["matches"]})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let response: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["ok"], true);
        assert!(response["data"]["ticket"]
            .as_str()
            .unwrap()
            .starts_with(TOKEN_PREFIX));
    }

    #[tokio::test]
    async fn two_real_websockets_receive_only_their_client_scoped_event() {
        let directory = tempfile::tempdir().unwrap();
        let node = test_node(&directory).await;
        let server = ApiServer::new(node, "p3-ws-token".into(), 0);
        let hub = server.vnext_ws_hub();
        let a = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Matches]))
            .unwrap();
        let b = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Matches]))
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, server.build_router()).await.unwrap();
        });

        let (mut socket_a, _) =
            connect_async(format!("ws://{address}/api/vnext/ws?ticket={}", a.ticket))
                .await
                .unwrap();
        let (mut socket_b, _) =
            connect_async(format!("ws://{address}/api/vnext/ws?ticket={}", b.ticket))
                .await
                .unwrap();
        let ready_a = socket_a.next().await.unwrap().unwrap();
        let ready_b = socket_b.next().await.unwrap().unwrap();
        assert!(ready_a.to_text().unwrap().contains("subscription_ready"));
        assert!(ready_b.to_text().unwrap().contains("subscription_ready"));

        hub.publish_bounded_match(&session_headers(&a.client_session), 1);
        let event_a = tokio::time::timeout(Duration::from_secs(1), socket_a.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(event_a
            .to_text()
            .unwrap()
            .contains("bounded_match_available"));
        assert!(
            tokio::time::timeout(Duration::from_millis(150), socket_b.next())
                .await
                .is_err()
        );

        let reused =
            connect_async(format!("ws://{address}/api/vnext/ws?ticket={}", a.ticket)).await;
        assert!(reused.is_err());
        task.abort();
    }

    #[tokio::test]
    async fn runtime_subscription_receives_bounded_disabled_lane_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let node = test_node(&directory).await;
        let server = ApiServer::new(node, "p3-ws-token".into(), 0);
        let hub = server.vnext_ws_hub();
        let ticket = hub
            .issue_ticket(ticket_request(&[VNextWsTopicV1::Runtime]))
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, server.build_router()).await.unwrap();
        });
        let (mut socket, _) = connect_async(format!(
            "ws://{address}/api/vnext/ws?ticket={}",
            ticket.ticket
        ))
        .await
        .unwrap();
        let ready = socket.next().await.unwrap().unwrap();
        assert!(ready.to_text().unwrap().contains("subscription_ready"));

        let mut lanes = BTreeSet::new();
        for _ in 0..4 {
            let event = tokio::time::timeout(Duration::from_secs(1), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_str(event.to_text().unwrap()).unwrap();
            assert_eq!(value["event_type"], "lane_disabled");
            assert_eq!(value["lifecycle"], "disabled");
            lanes.insert(value["data"]["lane"].as_str().unwrap().to_string());
            let wire = value.to_string();
            assert!(!wire.contains("standing_need_id"));
            assert!(!wire.contains("query_definition_cid"));
            assert!(!wire.contains("target_cid"));
        }
        assert_eq!(lanes.len(), 4);
        task.abort();
    }

    #[tokio::test]
    async fn legacy_websocket_path_keeps_its_existing_query_token_contract() {
        let directory = tempfile::tempdir().unwrap();
        let node = test_node(&directory).await;
        let server = ApiServer::new(node, "legacy-ws-token".into(), 0);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, server.build_router()).await.unwrap();
        });

        let accepted =
            connect_async(format!("ws://{address}/ws/events?token=legacy-ws-token")).await;
        assert!(accepted.is_ok());
        let rejected = connect_async(format!("ws://{address}/ws/events")).await;
        assert!(rejected.is_err());
        task.abort();
    }
}
