//! Separate wire vocabulary with the parent hub's shared admission ceilings.
use super::*;
use futures::SinkExt;
use onebrain_node::vnext_product_runtime::obp::{Error, Session};
use serde_json::{json, Value};
const PROFILE: &str = "OBP_PRIVATE_WEBSOCKET_PROFILE_V1";
#[derive(Default)]
pub(super) struct State {
    pub(super) pending: BTreeMap<[u8; 32], Pending>,
    pub(super) active: BTreeMap<[u8; 32], Active>,
}
pub(super) struct Pending {
    principal: [u8; 32],
    session: Session,
    client: [u8; 32],
    expires: Instant,
    session_expires: Instant,
    epoch: u64,
}
pub(super) struct Active {
    principal: [u8; 32],
    session: Session,
    expires: Instant,
    sender: mpsc::Sender<Value>,
    next: u64,
    last: Option<Value>,
    closing: bool,
}
impl State {
    pub(super) fn prune(&mut self, now: Instant) {
        self.pending
            .retain(|_, p| p.expires > now && p.session_expires > now);
        self.active
            .retain(|_, a| a.expires > now && !a.sender.is_closed());
    }
    pub(super) fn contains(&self, key: &[u8; 32]) -> bool {
        self.pending.contains_key(key)
            || self.active.contains_key(key)
            || self.pending.values().any(|p| p.client == *key)
    }
}
fn event(kind: &str, sequence: u64, data: Value, status: Option<&Value>) -> Value {
    // Project only low-cardinality status hints, never arbitrary error text.
    json!({"profile":PROFILE,"event_type":kind,"sequence":sequence,"timestamp":now_epoch(),
        "lifecycle":status.and_then(|s|s["lifecycle"].as_str()).unwrap_or("active"),
        "coverage":status.and_then(|s|s["coverage"].as_str()).unwrap_or("local_only"),
        "limitations":["local_state_hint_refetch_rest"],"data":data})
}
impl VNextWsHub {
    pub(crate) fn issue_obp(&self, principal: [u8; 32], session: Session) -> Result<Value, Error> {
        let now = Instant::now();
        let mut state = self.lock();
        prune_expired(&mut state, now);
        if state.pending_count() >= self.limits.max_pending_tickets
            || state.active_count() >= self.limits.max_active_sessions
        {
            return Err(Error::new("admission_limit"));
        }
        let ticket = fresh_token(&state, true).map_err(|_| Error::new("admission_limit"))?;
        let mut client = fresh_token(&state, false).map_err(|_| Error::new("admission_limit"))?;
        if client == ticket {
            client = fresh_token(&state, false).map_err(|_| Error::new("admission_limit"))?;
            if client == ticket {
                return Err(Error::new("admission_limit"));
            }
        }
        let epoch = now_epoch() + self.limits.session_ttl.as_secs();
        state.obp.pending.insert(
            ticket,
            Pending {
                principal,
                session,
                client,
                expires: now + self.limits.ticket_ttl,
                session_expires: now + self.limits.session_ttl,
                epoch,
            },
        );
        Ok(
            json!({"ticket":encode_token(ticket),"client_session":encode_token(client),"expires_at":now_epoch()+self.limits.ticket_ttl.as_secs(),"session_expires_at":epoch,"subscriptions":["network"],"limitations":["local_state_hint_refetch_rest"]}),
        )
    }
    fn accept_obp(
        &self,
        ticket: &str,
        principal: [u8; 32],
        session: &Session,
    ) -> Result<([u8; 32], mpsc::Receiver<Value>, Instant, Value), ()> {
        let ticket = decode_token(ticket).ok_or(())?;
        let now = Instant::now();
        let mut state = self.lock();
        prune_expired(&mut state, now);
        let pending = state.obp.pending.remove(&ticket).ok_or(())?;
        if pending.principal != principal
            || pending.session != *session
            || state.active_count() >= self.limits.max_active_sessions
        {
            return Err(());
        }
        let (sender, receiver) = mpsc::channel(self.limits.event_queue_capacity);
        let ready = event(
            "subscription_ready",
            1,
            json!({"subscriptions":["network"],"session_expires_at":pending.epoch}),
            None,
        );
        state.obp.active.insert(
            pending.client,
            Active {
                principal,
                session: session.clone(),
                expires: pending.session_expires,
                sender,
                next: 2,
                last: None,
                closing: false,
            },
        );
        Ok((pending.client, receiver, pending.session_expires, ready))
    }
    pub(crate) fn publish_obp(
        &self,
        headers: &HeaderMap,
        principal: [u8; 32],
        session: &Session,
        status: &Value,
    ) {
        let Some(client) = client_session_from_headers(headers) else {
            return;
        };
        self.publish_obp_to(client, principal, session, status);
    }
    fn publish_obp_to(
        &self,
        client: [u8; 32],
        principal: [u8; 32],
        session: &Session,
        status: &Value,
    ) {
        let mut state = self.lock();
        prune_expired(&mut state, Instant::now());
        let Some(active) = state.obp.active.get_mut(&client) else {
            return;
        };
        if active.closing || active.principal != principal || active.session != *session {
            return;
        }
        let mut data = serde_json::Map::new();
        for key in [
            "compiled",
            "requested",
            "active",
            "kill_switch",
            "signer_ready",
            "source_count",
            "usable_reservations",
            "authenticated_routes",
            "pending_intents",
            "advertisement_state",
        ] {
            let Some(value) = status.get(key) else {
                return;
            };
            data.insert(key.into(), value.clone());
        }
        data.insert("claims_global_completion".into(), false.into());
        data.insert("authorizes_reward".into(), false.into());
        let mut value = event("network_state", active.next, data.into(), Some(status));
        let mut fingerprint = value.clone();
        fingerprint.as_object_mut().unwrap().remove("sequence");
        fingerprint.as_object_mut().unwrap().remove("timestamp");
        if active.last.as_ref() == Some(&fingerprint) {
            return;
        }
        if active.sender.try_send(value.take()).is_err() {
            // Keep the admission slot until the socket task actually exits.
            active.closing = true;
        } else {
            active.last = Some(fingerprint);
            active.next = active.next.saturating_add(1);
        }
    }
}
pub(crate) async fn upgrade(
    state: AppState,
    query: BTreeMap<String, String>,
    ws: WebSocketUpgrade,
) -> Response {
    let failure = || crate::obp_api::auth_error(StatusCode::UNAUTHORIZED);
    if query.len() != 1 {
        return failure();
    }
    let Some(ticket) = query.get("ticket") else {
        return failure();
    };
    let Some(binding) = state.obp_binding.clone() else {
        return failure();
    };
    let Some(services) = state.vnext_product_services().await else {
        return failure();
    };
    let Ok(session) = services.obp_session() else {
        return failure();
    };
    let Ok((client, mut rx, expires, ready)) =
        state
            .vnext_ws
            .accept_obp(ticket, binding.principal, &session)
    else {
        return failure();
    };
    let mut response=ws.max_message_size(MAX_WS_MESSAGE_BYTES).max_frame_size(MAX_WS_MESSAGE_BYTES).on_upgrade(move|mut socket|async move{
        let hub=&state.vnext_ws;
        let run=async{
            tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Text(ready.to_string().into()))).await.map_err(|_|())?.map_err(|_|())?;
            if let Ok(status)=services.obp_status().await{hub.publish_obp_to(client,binding.principal,&session,&status);}
            loop {tokio::select!{
                _=tokio::time::sleep_until(expires.into())=>break,
                message=socket.recv()=>match message{Some(Ok(Message::Ping(bytes)))=>{tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Pong(bytes))).await.map_err(|_|())?.map_err(|_|())?;},Some(Ok(Message::Pong(_)))=>{},_=>break},
                value=rx.recv()=>{let Some(value)=value else{break;};if hub.lock().obp.active.get(&client).is_none_or(|a|a.closing)||services.obp_session().ok().as_ref()!=Some(&session){break;}tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Text(value.to_string().into()))).await.map_err(|_|())?.map_err(|_|())?;}
            }}Ok::<(),()>(())
        };
        let _=tokio::time::timeout_at(expires.into(),run).await;let _=tokio::time::timeout(Duration::from_secs(1),socket.close()).await;hub.lock().obp.active.remove(&client);
    }).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        "no-store".parse().unwrap(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session {
            process_generation: "11".repeat(32),
            dataset_generation: "22".repeat(32),
        }
    }
    fn status() -> Value {
        json!({"compiled":true,"requested":false,"active":false,"kill_switch":false,"signer_ready":true,"source_count":0,"usable_reservations":0,"authenticated_routes":0,"pending_intents":0,"advertisement_state":"disabled","lifecycle":"disabled","coverage":"local_only","limitations":["PRIVATE_ADDRESS"],"peer_id":"SECRET_PEER"})
    }
    #[test]
    fn tickets_cannot_cross_profiles_or_principals_and_are_single_use() {
        let hub = VNextWsHub::default();
        let s = session();
        let old = hub
            .issue_ticket(VNextWsTicketRequestV1 {
                subscriptions: vec![VNextWsTopicV1::Runtime],
            })
            .unwrap();
        assert!(hub.accept_obp(&old.ticket, [1; 32], &s).is_err());
        assert!(hub.accept_ticket(&old.ticket).is_ok());
        let ticket = hub.issue_obp([1; 32], s.clone()).unwrap();
        let t = ticket["ticket"].as_str().unwrap();
        assert!(hub.accept_ticket(t).is_err());
        assert!(hub.accept_obp(t, [1; 32], &s).is_ok());
        assert!(hub.accept_obp(t, [1; 32], &s).is_err());
        let ticket = hub.issue_obp([1; 32], s.clone()).unwrap();
        assert!(hub
            .accept_obp(ticket["ticket"].as_str().unwrap(), [2; 32], &s)
            .is_err());
    }
    #[test]
    fn parent_and_obp_share_ticket_ceiling() {
        let hub = VNextWsHub {
            limits: HubLimits {
                max_pending_tickets: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        hub.issue_obp([1; 32], session()).unwrap();
        hub.issue_ticket(VNextWsTicketRequestV1 {
            subscriptions: vec![VNextWsTopicV1::Runtime],
        })
        .unwrap();
        assert!(hub.issue_obp([1; 32], session()).is_err());
        assert!(hub
            .issue_ticket(VNextWsTicketRequestV1 {
                subscriptions: vec![VNextWsTopicV1::Runtime]
            })
            .is_err());
    }
    #[test]
    fn snapshot_redaction_dedup_and_overflow_are_client_local() {
        let hub = VNextWsHub {
            limits: HubLimits {
                event_queue_capacity: 1,
                max_active_sessions: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        let s = session();
        let a = hub.issue_obp([1; 32], s.clone()).unwrap();
        let b = hub.issue_obp([1; 32], s.clone()).unwrap();
        let (a, mut rx, _, _) = hub
            .accept_obp(a["ticket"].as_str().unwrap(), [1; 32], &s)
            .unwrap();
        let (b, mut other, _, _) = hub
            .accept_obp(b["ticket"].as_str().unwrap(), [1; 32], &s)
            .unwrap();
        hub.publish_obp_to(a, [2; 32], &s, &status());
        assert!(rx.try_recv().is_err());
        hub.publish_obp_to(a, [1; 32], &s, &status());
        hub.publish_obp_to(a, [1; 32], &s, &status());
        let value = rx.try_recv().unwrap();
        assert_eq!(value["sequence"], 2);
        assert_eq!(value["data"]["authorizes_reward"], false);
        assert!(!value.to_string().contains("SECRET"));
        assert!(!value.to_string().contains("PRIVATE_ADDRESS"));
        assert!(rx.try_recv().is_err());
        assert!(other.try_recv().is_err());
        let mut changed = status();
        changed["pending_intents"] = 1.into();
        hub.publish_obp_to(a, [1; 32], &s, &changed);
        changed["pending_intents"] = 2.into();
        hub.publish_obp_to(a, [1; 32], &s, &changed);
        assert!(hub.lock().obp.active[&a].closing);
        assert!(hub.lock().obp.active.contains_key(&b));
        assert!(hub.issue_obp([1; 32], s.clone()).is_err());
        drop(rx);
        assert!(hub.issue_obp([1; 32], s).is_ok());
    }
}
