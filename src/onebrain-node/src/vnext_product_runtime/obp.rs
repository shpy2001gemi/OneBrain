//! Shared local OBP command owner. HTTP never owns effects or replay state.
use super::*;
use crate::vnext_outbound_product::DiscoveryInput;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
pub mod contract;
mod store;
pub use contract::Request;
use contract::{hex, id};
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub reason: &'static str,
}
impl Error {
    pub fn new(reason: &'static str) -> Self {
        Self { reason }
    }
    pub fn invalid() -> Self {
        Self::new("invalid_payload")
    }
    pub fn storage() -> Self {
        Self::new("storage_corrupt")
    }
    pub fn forbidden() -> Self {
        Self::new("forbidden")
    }
    pub fn unknown(&self) -> bool {
        matches!(
            self.reason,
            "outcome_unknown" | "storage_corrupt" | "response_overflow"
        )
    }
}
fn random() -> Result<[u8; 32]> {
    let mut v = [0; 32];
    getrandom::fill(&mut v).map_err(|_| Error::new("dependency_unavailable"))?;
    Ok(v)
}
fn runtime_error(_: VNextProductRuntimeError) -> Error {
    Error::new("dependency_unavailable")
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub process_generation: String,
    pub dataset_generation: String,
}

pub(super) struct Owner {
    store: store::Store,
    process: [u8; 32],
    cursor_key: [u8; 32],
    serial: tokio::sync::Mutex<()>,
    grants: Mutex<BTreeMap<[u8; 32], Grant>>,
    inputs: Mutex<BTreeMap<[u8; 32], Input>>,
    snapshots: Mutex<BTreeMap<[u8; 32], Snapshot>>,
    routes: Mutex<BTreeMap<String, LiveRoute>>,
}
struct Grant {
    principal: [u8; 32],
    scopes: Vec<String>,
    expires: Instant,
}
struct Input {
    principal: [u8; 32],
    expires: Instant,
    input: DiscoveryInput,
    kind: &'static str,
}
struct Snapshot {
    principal: [u8; 32],
    generation: u64,
    operation: String,
    expires: Instant,
    items: Vec<Value>,
    frontier: String,
}
struct LiveRoute {
    principal: [u8; 32],
    generation: u64,
    session: crate::vnext_connection_planner::RoutedVNextSession,
    value: Value,
}

impl Owner {
    pub(super) fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            store: store::Store::open(path)?,
            process: random()?,
            cursor_key: random()?,
            serial: tokio::sync::Mutex::new(()),
            grants: Mutex::new(BTreeMap::new()),
            inputs: Mutex::new(BTreeMap::new()),
            snapshots: Mutex::new(BTreeMap::new()),
            routes: Mutex::new(BTreeMap::new()),
        })
    }
    pub(super) async fn restore_policy(
        &self,
        owner: Option<&Arc<OutboundFirstOwner>>,
    ) -> Result<()> {
        let state = self.store.read()?;
        if let Some(owner) = owner {
            let statuses = owner.source_statuses.read().await.clone();
            self.store.mutate(|s| {
                for source in &statuses {
                    if !s.sources.contains_key(&hex(&source.source_id)) {
                        if s.sources.len() >= 8 {
                            return Err(Error::new("admission_limit"));
                        }
                        s.sources
                            .insert(hex(&source.source_id), (source.kind.into(), true));
                    }
                }
                Ok(())
            })?;
            if let Some((enabled, advertise)) = state.configuration {
                owner.configure_product(enabled, advertise).await;
            }
            for (source, (_, enabled)) in state.sources {
                let id = contract::id(&Value::String(source))?;
                let _ = owner.toggle_product_source(id, enabled).await;
            }
        }
        Ok(())
    }
    fn session(&self) -> Result<Session> {
        Ok(Session {
            process_generation: hex(&self.process),
            dataset_generation: hex(&self.store.read()?.dataset),
        })
    }
    fn check_session(&self, s: &Session) -> Result<()> {
        if *s != self.session()? {
            Err(Error::new("session_conflict"))
        } else {
            Ok(())
        }
    }
    fn management(&self, principal: [u8; 32], token: Option<&str>, operation: &str) -> Result<()> {
        let bytes = token
            .and_then(|v| v.strip_prefix("obm1."))
            .and_then(|v| URL_SAFE_NO_PAD.decode(v).ok())
            .filter(|b| b.len() == 32)
            .ok_or_else(Error::forbidden)?;
        let grants = self.grants.lock().map_err(|_| Error::forbidden())?;
        // Constant-work token comparison across the bounded grant set.
        let found = grants
            .iter()
            .find(|(key, _)| key.iter().zip(&bytes).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0)
            .map(|(_, v)| v);
        match found {
            Some(g)
                if g.principal == principal
                    && g.expires > Instant::now()
                    && g.scopes.iter().any(|s| s == operation) =>
            {
                Ok(())
            }
            _ => Err(Error::forbidden()),
        }
    }
}

/// Host-only handle: obtainable from the owning node/runtime, never product reads.
pub struct Host {
    services: VNextProductServices,
}
/// Revocable, in-process command authority, bound to this exact owner and principal.
#[derive(Clone)]
pub struct Control {
    core: Weak<VNextProductServiceCore>,
    principal: [u8; 32],
    enabled: Arc<AtomicBool>,
}
impl Control {
    pub fn revoke(&self) {
        self.enabled.store(false, Ordering::Release);
    }
}
impl VNextProductRuntime {
    pub fn obp_host(&self) -> Host {
        Host {
            services: self.services(),
        }
    }
}
impl Host {
    pub fn control(&self, principal: [u8; 32]) -> Result<Control> {
        let lease = self.services.lease().map_err(runtime_error)?;
        Ok(Control {
            core: Arc::downgrade(&lease.core),
            principal,
            enabled: Arc::new(AtomicBool::new(true)),
        })
    }
    pub fn management(
        &self,
        principal: [u8; 32],
        scopes: Vec<String>,
        ttl: Duration,
    ) -> Result<String> {
        if ttl.is_zero()
            || ttl > Duration::from_secs(300)
            || scopes.is_empty()
            || scopes.len() > 5
            || scopes.iter().any(|s| {
                !matches!(
                    s.as_str(),
                    "configure"
                        | "source_admit"
                        | "source_set_enabled"
                        | "network_kill"
                        | "network_reenable"
                )
            })
        {
            return Err(Error::invalid());
        }
        let lease = self.services.lease().map_err(runtime_error)?;
        let mut grants = lease.core.obp.grants.lock().map_err(|_| Error::storage())?;
        grants.retain(|_, g| g.expires > Instant::now());
        if grants.len() >= 32 {
            return Err(Error::new("admission_limit"));
        }
        let token = random()?;
        grants.insert(
            token,
            Grant {
                principal,
                scopes,
                expires: Instant::now() + ttl,
            },
        );
        Ok(format!("obm1.{}", URL_SAFE_NO_PAD.encode(token)))
    }
    pub fn revoke_management(&self, token: &str) -> Result<()> {
        let bytes = URL_SAFE_NO_PAD
            .decode(token.strip_prefix("obm1.").ok_or_else(Error::invalid)?)
            .map_err(|_| Error::invalid())?;
        let id: [u8; 32] = bytes.try_into().map_err(|_| Error::invalid())?;
        self.services
            .lease()
            .map_err(runtime_error)?
            .core
            .obp
            .grants
            .lock()
            .map_err(|_| Error::storage())?
            .remove(&id);
        Ok(())
    }
    pub fn input(&self, principal: [u8; 32], input: DiscoveryInput) -> Result<String> {
        let (_, kind) = input.product_identity().map_err(|_| Error::invalid())?;
        let lease = self.services.lease().map_err(runtime_error)?;
        let mut inputs = lease.core.obp.inputs.lock().map_err(|_| Error::storage())?;
        inputs.retain(|_, i| i.expires > Instant::now());
        // Every typed input is bounded below 128 KiB, including config endpoint.
        if inputs.len() >= 8 {
            return Err(Error::new("admission_limit"));
        }
        let token = random()?;
        inputs.insert(
            token,
            Input {
                principal,
                expires: Instant::now() + Duration::from_secs(300),
                input,
                kind,
            },
        );
        Ok(hex(&token))
    }
}

fn control(
    core: &Arc<VNextProductServiceCore>,
    principal: [u8; 32],
    grant: &Control,
) -> Result<()> {
    if !grant.enabled.load(Ordering::Acquire)
        || grant.principal != principal
        || !Weak::ptr_eq(&grant.core, &Arc::downgrade(core))
    {
        return Err(Error::forbidden());
    }
    Ok(())
}
fn authorize(
    core: &Arc<VNextProductServiceCore>,
    principal: [u8; 32],
    grant: &Control,
    token: Option<&str>,
    r: &Request,
) -> Result<()> {
    control(core, principal, grant)?;
    if r.management() {
        core.obp.management(principal, token, &r.operation)?;
    }
    Ok(())
}
fn generation(core: &VNextProductServiceCore) -> Result<u64> {
    Ok(core
        .rollout
        .snapshot()
        .map_err(|_| Error::storage())?
        .lane(VNextRuntimeLane::Network)
        .generation)
}
fn owner(core: &VNextProductServiceCore) -> Result<Arc<OutboundFirstOwner>> {
    core.outbound_first
        .lock()
        .map_err(|_| Error::storage())?
        .clone()
        .ok_or_else(|| Error::new("dependency_unavailable"))
}

impl VNextProductServices {
    pub fn obp_session(&self) -> Result<Session> {
        self.lease().map_err(runtime_error)?.core.obp.session()
    }
    pub async fn obp_status(&self) -> Result<Value> {
        let lease = self.lease().map_err(runtime_error)?;
        let s = self.outbound_first_status().await.map_err(runtime_error)?;
        let mut v = serde_json::to_value(&s).map_err(|_| Error::storage())?;
        if let Some((requested, advertise)) = lease.core.obp.store.read()?.configuration {
            v["requested"] = requested.into();
            if !requested {
                v["active"] = false.into();
                v["lifecycle"] = "disabled".into();
                v["advertisement_state"] = "disabled".into();
            } else if !s.requested {
                v["active"] = false.into();
                v["lifecycle"] = if s.kill_switch {
                    "disabled"
                } else {
                    "requested"
                }
                .into();
                v["limitations"]
                    .as_array_mut()
                    .unwrap()
                    .push("restart_required".into());
            }
            if !advertise {
                v["advertisement_state"] = "disabled".into();
            }
        }
        let policies = lease.core.obp.store.read()?.sources;
        let sources = self.outbound_first_sources().await.map_err(runtime_error)?;
        let missing = policies
            .keys()
            .filter(|key| !sources.iter().any(|s| hex(&s.source_id) == **key))
            .count();
        if missing > 0 {
            v["source_count"] = (s.source_count + missing).into();
            v["limitations"]
                .as_array_mut()
                .unwrap()
                .push("host_source_binding_unavailable".into());
        }
        if s.active {
            let live = lease
                .core
                .obp
                .routes
                .lock()
                .map_err(|_| Error::storage())?
                .values()
                .filter(|r| r.generation == s.generation && r.session.is_live())
                .count();
            v["authenticated_routes"] = (s.authenticated_routes + live).into();
        }
        contract::validate("ObpStatusV1", &v).map_err(|_| Error::new("response_overflow"))?;
        Ok(v)
    }
    pub fn obp_reconcile(
        &self,
        principal: [u8; 32],
        session: &Session,
        key: [u8; 32],
    ) -> Result<Value> {
        let lease = self.lease().map_err(runtime_error)?;
        lease.core.obp.check_session(session)?;
        let state = lease.core.obp.store.read()?;
        let record = state
            .records
            .get(&hex(&key))
            .filter(|r| r.principal == principal)
            .ok_or_else(|| Error::new("local_not_found"))?;
        let mut value = record.outcome.clone();
        value.as_object_mut().unwrap().remove("result");
        value.as_object_mut().unwrap().remove("failure");
        Ok(value)
    }
    pub async fn obp_command(
        &self,
        principal: [u8; 32],
        session: &Session,
        grant: &Control,
        token: Option<&str>,
        request: Request,
    ) -> Result<Value> {
        let lease = self.lease().map_err(runtime_error)?;
        let core = &lease.core;
        core.obp.check_session(session)?;
        authorize(core, principal, grant, token, &request)?;
        // A single bounded slot keeps command mutation and generation admission ordered.
        let _serial = core
            .obp
            .serial
            .try_lock()
            .map_err(|_| Error::new("admission_limit"))?;
        core.obp.check_session(session)?;
        authorize(core, principal, grant, token, &request)?;
        let key = request.payload["idempotency_key"]
            .as_str()
            .ok_or_else(Error::invalid)?;
        if let Some(record) = core.obp.store.read()?.records.get(key) {
            if record.principal != principal
                || record.operation != request.operation
                || record.payload != request.payload
            {
                return Err(Error::new("idempotency_conflict"));
            }
            let mut outcome = record.outcome.clone();
            if request.operation == "route_request"
                && outcome["state"] == "completed"
                && outcome["result"]["state"] == "connected"
            {
                let query = Request::new(
                    "route_status",
                    json!({"route_id":outcome["result"]["route_id"]}),
                    false,
                )?;
                outcome["result"] = self.obp_query(principal, session, query).await?;
            }
            return Ok(outcome);
        }
        if request.payload["expected_generation"].as_u64() != Some(generation(core)?) {
            return Err(Error::new("generation_conflict"));
        }
        core.ensure_storage_writable()
            .map_err(|_| Error::new("admission_limit"))?;
        // Missing dependencies and invalid references reject before durable admission.
        match request.operation.as_str() {
            "refresh" | "route_request" | "source_admit" | "source_set_enabled" => {
                let o = owner(core)?;
                if !o.granted() {
                    return Err(Error::new("feature_disabled"));
                }
            }
            _ => {}
        }
        if request.operation == "source_admit" {
            let sid = {
                let input = id(&request.payload["input_ref"])?;
                let inputs = core.obp.inputs.lock().map_err(|_| Error::storage())?;
                let entry = inputs
                    .get(&input)
                    .filter(|i| i.principal == principal && i.expires > Instant::now())
                    .ok_or_else(|| Error::new("input_expired"))?;
                if request.payload["kind"] != entry.kind {
                    return Err(Error::invalid());
                }
                entry
                    .input
                    .product_identity()
                    .map_err(|_| Error::invalid())?
                    .0
            };
            let o = owner(core)?;
            let sources = o.source_statuses.read().await;
            if sources.iter().any(|s| s.source_id == sid) {
                return Err(Error::new("generation_conflict"));
            }
            let stored = core.obp.store.read()?;
            if sources.len() >= 8
                || (stored.sources.len() >= 8 && !stored.sources.contains_key(&hex(&sid)))
            {
                return Err(Error::new("admission_limit"));
            }
        }
        if request.operation == "source_set_enabled" {
            let sid = id(&request.payload["source_id"])?;
            let o = owner(core)?;
            if !o
                .source_statuses
                .read()
                .await
                .iter()
                .any(|s| s.source_id == sid)
            {
                return Err(Error::new("local_not_found"));
            }
        }
        if request.operation == "intent_retry" {
            let intent = core
                .network()
                .map_err(runtime_error)?
                .outbound_intent(&id(&request.payload["intent_id"])?)
                .map_err(|_| Error::storage())?
                .ok_or_else(|| Error::new("local_not_found"))?;
            if intent.state.is_terminal() {
                return Err(Error::new("generation_conflict"));
            }
        }
        authorize(core, principal, grant, token, &request)?;
        crash_point("before_admission");
        core.obp
            .store
            .admit(principal, &request.operation, &request.payload)?;
        crash_point("after_admission");
        let guard = || {
            authorize(core, principal, grant, token, &request).is_ok()
                && generation(core).ok() == request.payload["expected_generation"].as_u64()
        };
        let result = {
            let effect = self.obp_effect(core, principal, &request, &guard);
            tokio::pin!(effect);
            loop {
                tokio::select! {
                    result=&mut effect=>break result,
                    _=tokio::time::sleep(Duration::from_millis(10))=>{if !guard(){break Err(Error::forbidden());}}
                }
            }
        };
        crash_point("after_effect");
        let outcome = core.obp.store.finish(key, result)?;
        crash_point("after_result_commit");
        Ok(outcome)
    }
    async fn obp_effect(
        &self,
        core: &Arc<VNextProductServiceCore>,
        principal: [u8; 32],
        r: &Request,
        current: &(dyn Fn() -> bool + Send + Sync),
    ) -> Result<Value> {
        if !current() {
            return Err(Error::forbidden());
        }
        match r.operation.as_str() {
            "network_kill" => {
                core.rollout
                    .kill(VNextRuntimeLane::Network)
                    .map_err(|_| Error::storage())?;
                close_routes(&core.obp);
                self.obp_status().await
            }
            "network_reenable" => {
                core.rollout
                    .reenable(VNextRuntimeLane::Network)
                    .map_err(|_| Error::storage())?;
                self.obp_status().await
            }
            "configure" => {
                let requested = r.payload["outbound_first_requested"].as_bool().unwrap();
                let advertise = r.payload["advertise_reachability"].as_bool().unwrap();
                core.obp.store.mutate(|s| {
                    s.configuration = Some((requested, advertise));
                    Ok(())
                })?;
                if let Ok(o) = owner(core) {
                    o.configure_product(requested, advertise).await;
                }
                if !requested {
                    close_routes(&core.obp);
                }
                self.obp_status().await
            }
            "source_admit" => {
                let o = owner(core)?;
                let token = id(&r.payload["input_ref"])?;
                let entry = core
                    .obp
                    .inputs
                    .lock()
                    .map_err(|_| Error::storage())?
                    .remove(&token)
                    .ok_or_else(|| Error::new("input_expired"))?;
                if !current() || entry.principal != principal || entry.expires <= Instant::now() {
                    return Err(Error::forbidden());
                }
                let (sid, kind) = entry
                    .input
                    .product_identity()
                    .map_err(|_| Error::invalid())?;
                core.obp.store.mutate(|s| {
                    if s.sources.len() >= 8 && !s.sources.contains_key(&hex(&sid)) {
                        return Err(Error::new("admission_limit"));
                    }
                    s.sources.insert(hex(&sid), (kind.into(), true));
                    Ok(())
                })?;
                let status = o
                    .admit_product_source(entry.input, current)
                    .await
                    .map_err(|_| Error::new("dependency_unavailable"))?;
                source_value(status)
            }
            "source_set_enabled" => {
                let sid = id(&r.payload["source_id"])?;
                let enabled = r.payload["enabled"].as_bool().unwrap();
                let o = owner(core)?;
                let prior = o
                    .source_statuses
                    .read()
                    .await
                    .iter()
                    .find(|s| s.source_id == sid)
                    .cloned()
                    .ok_or_else(|| Error::new("local_not_found"))?;
                if !current() {
                    return Err(Error::forbidden());
                }
                core.obp.store.mutate(|s| {
                    if s.sources.len() >= 8 && !s.sources.contains_key(&hex(&sid)) {
                        return Err(Error::new("admission_limit"));
                    }
                    s.sources.insert(hex(&sid), (prior.kind.into(), enabled));
                    Ok(())
                })?;
                let status = o
                    .toggle_product_source_guarded(sid, enabled, current)
                    .await
                    .map_err(|_| Error::new("outcome_unknown"))?;
                close_routes(&core.obp);
                source_value(status)
            }
            "refresh" => {
                let g = core
                    .rollout
                    .acquire(VNextRuntimeLane::Network)
                    .map_err(|_| Error::new("feature_disabled"))?;
                owner(core)?
                    .refresh_product(&|| current() && g.is_current())
                    .await
                    .map_err(|_| Error::new("outcome_unknown"))?;
                self.obp_status().await
            }
            "intent_retry" => {
                let network = core.network().map_err(runtime_error)?;
                network.wake_product_outbox();
                self.intent_value(core, id(&r.payload["intent_id"])?)
            }
            "route_request" => {
                let peer = NodeId::from_bytes(id(&r.payload["expected_peer"])?);
                let gen = generation(core)?;
                let route_id = hex(&random()?);
                let mut value = json!({"route_id":route_id,"expected_peer":hex(peer.as_bytes()),"state":"path_limited","generation":gen,"coverage":"partial","limitations":["route_attempt_path_limited"],"failure":"PathLimited","claims_global_completion":false,"authorizes_reward":false});
                let network = core.network().map_err(runtime_error)?;
                let result = tokio::time::timeout(
                    Duration::from_secs(20),
                    network.connect_product_route(peer),
                )
                .await;
                if let Ok(Ok(session)) = result {
                    if !current() || generation(core)? != gen {
                        session.close();
                        return Err(Error::new("outcome_unknown"));
                    }
                    value["state"] = "connected".into();
                    value["limitations"] = json!([]);
                    value.as_object_mut().unwrap().remove("failure");
                    value["authenticated_peer"] = hex(peer.as_bytes()).into();
                    value["route_receipt_digest"] = hex(&session.route_receipt_digest()).into();
                    value["path_kind"] = match session.carrier().path_kind() {
                        onebrain_protocol::RoutePathKindV1::Direct => "direct",
                        onebrain_protocol::RoutePathKindV1::HolePunched => "hole-punched",
                        onebrain_protocol::RoutePathKindV1::RelayUdp => "relay-udp",
                        onebrain_protocol::RoutePathKindV1::RelayTcp443 => "relay-tcp-443",
                    }
                    .into();
                    let mut routes = core.obp.routes.lock().map_err(|_| Error::storage())?;
                    routes.retain(|_, r| {
                        let keep = r.generation == gen && r.session.is_live();
                        if !keep {
                            r.session.close();
                        }
                        keep
                    });
                    if routes.len() >= 64 {
                        session.close();
                        return Err(Error::new("admission_limit"));
                    }
                    routes.insert(
                        route_id,
                        LiveRoute {
                            principal,
                            generation: gen,
                            session,
                            value: value.clone(),
                        },
                    );
                }
                Ok(value)
            }
            _ => Err(Error::invalid()),
        }
    }
    fn intent_value(&self, core: &VNextProductServiceCore, key: [u8; 32]) -> Result<Value> {
        let network = core.network().map_err(runtime_error)?;
        let i = network
            .outbound_intent(&key)
            .map_err(|_| Error::storage())?
            .ok_or_else(|| Error::new("local_not_found"))?;
        use crate::vnext_outbox::OutboundIntentState::*;
        let state = match i.state {
            Pending => "pending",
            Acknowledged => "acknowledged",
            DeadLetter => "dead_letter",
            RetryExhausted => "retry_exhausted",
        };
        let mut value = json!({"intent_id":hex(&key),"expected_peer":hex(i.expected_peer.as_bytes()),"state":state,"transport_attempts":i.transport_attempts,"validation_retries":i.validation_retries,"coverage":"partial","limitations":[],"claims_global_completion":false,"authorizes_reward":false});
        if state == "acknowledged" {
            let c = network
                .outbound_checkpoint(i.expected_peer)
                .map_err(|_| Error::storage())?
                .filter(|c| c.acknowledged_intent_id() == key)
                .ok_or_else(|| Error::new("dependency_unavailable"))?;
            value["acknowledged_sequence"] = c.acknowledged_sequence().into();
            value["checkpoint_digest"] = hex(&c.checkpoint_digest()).into();
        }
        Ok(value)
    }
    pub async fn obp_query(
        &self,
        principal: [u8; 32],
        session: &Session,
        r: Request,
    ) -> Result<Value> {
        let lease = self.lease().map_err(runtime_error)?;
        let core = &lease.core;
        core.obp.check_session(session)?;
        match r.operation.as_str() {
            "intent_status" => self.intent_value(core, id(&r.payload["intent_id"])?),
            "route_status" => {
                let key = r.payload["route_id"].as_str().unwrap();
                let gen = generation(core)?;
                let routes = core.obp.routes.lock().map_err(|_| Error::storage())?;
                if let Some(route) = routes.get(key).filter(|r| r.principal == principal) {
                    if route.generation == gen
                        && route.session.is_live()
                        && owner(core).is_ok_and(|o| o.granted())
                    {
                        return Ok(route.value.clone());
                    }
                }
                let state = core.obp.store.read()?;
                let old = state
                    .records
                    .values()
                    .filter(|r| r.principal == principal)
                    .find_map(|r| r.outcome.get("result").filter(|v| v["route_id"] == key))
                    .ok_or_else(|| Error::new("local_not_found"))?;
                let mut v = old.clone();
                if v["state"] != "connected" {
                    return Ok(v);
                }
                for field in ["authenticated_peer", "path_kind", "route_receipt_digest"] {
                    v.as_object_mut().unwrap().remove(field);
                }
                v["state"] = "path_limited".into();
                v["failure"] = "NetworkChanged".into();
                v["limitations"] = json!(["fresh_session_required"]);
                Ok(v)
            }
            "source_list" | "reservation_list" => self.page(core, principal, &r).await,
            _ => Err(Error::invalid()),
        }
    }
    async fn page(
        &self,
        core: &VNextProductServiceCore,
        principal: [u8; 32],
        r: &Request,
    ) -> Result<Value> {
        let gen = generation(core)?;
        let limit = r.payload["limit"].as_u64().unwrap() as usize;
        let (key, offset) = if let Some(token) = r.payload.get("continuation") {
            let token = token.as_str().ok_or_else(Error::invalid)?;
            let bytes = URL_SAFE_NO_PAD
                .decode(token.strip_prefix("obc1.").ok_or_else(Error::invalid)?)
                .map_err(|_| Error::invalid())?;
            if bytes.len() != 72 || format!("obc1.{}", URL_SAFE_NO_PAD.encode(&bytes)) != token {
                return Err(Error::invalid());
            }
            let tag = blake3::keyed_hash(&core.obp.cursor_key, &bytes[..40]);
            if tag
                .as_bytes()
                .iter()
                .zip(&bytes[40..])
                .fold(0u8, |a, (x, y)| a | (x ^ y))
                != 0
            {
                return Err(Error::invalid());
            }
            let key = bytes[..32].try_into().unwrap();
            let offset = u64::from_be_bytes(bytes[32..40].try_into().unwrap()) as usize;
            (key, offset)
        } else {
            let items = if r.operation == "source_list" {
                self.outbound_first_sources()
                    .await
                    .map_err(runtime_error)?
                    .into_iter()
                    .map(source_value)
                    .collect::<Result<Vec<_>>>()?
            } else {
                self.outbound_first_reservations().await.map_err(runtime_error)?.into_iter().map(|s|json!({"relay_node_id":hex(s.relay_node_id.as_bytes()),"state":s.state,"expires_at_unix_seconds":s.expires_at_unix_seconds,"limitations":s.limitations})).collect()
            };
            if generation(core)? != gen {
                return Err(Error::new("generation_conflict"));
            }
            let mut items = items;
            if r.operation == "source_list" {
                for (key, (kind, enabled)) in core.obp.store.read()?.sources {
                    if !items.iter().any(|v| v["source_id"] == key) {
                        items.push(json!({"source_id":key,"kind":kind,"state":if enabled{"unavailable"}else{"disabled"},"admitted_records":0,"limitations":["host_source_binding_unavailable"]}));
                    }
                }
            }
            items.sort_by_key(|v| {
                v[if r.operation == "source_list" {
                    "source_id"
                } else {
                    "relay_node_id"
                }]
                .as_str()
                .unwrap_or("")
                .to_owned()
            });
            let bytes = serde_json::to_vec(&items).map_err(|_| Error::storage())?;
            let frontier = hex(blake3::hash(&bytes).as_bytes());
            let key = random()?;
            let mut snapshots = core.obp.snapshots.lock().map_err(|_| Error::storage())?;
            snapshots.retain(|_, s| s.expires > Instant::now());
            let size: usize = snapshots
                .values()
                .map(|s| {
                    serde_json::to_vec(&s.items)
                        .map(|b| b.len())
                        .unwrap_or(1_048_576)
                })
                .sum();
            if snapshots.len() >= 32 || size + bytes.len() > 1_048_576 {
                return Err(Error::new("admission_limit"));
            }
            snapshots.insert(
                key,
                Snapshot {
                    principal,
                    generation: gen,
                    operation: r.operation.clone(),
                    expires: Instant::now() + Duration::from_secs(60),
                    items,
                    frontier,
                },
            );
            (key, 0)
        };
        let snapshots = core.obp.snapshots.lock().map_err(|_| Error::storage())?;
        let s = snapshots
            .get(&key)
            .filter(|s| s.expires > Instant::now())
            .ok_or_else(|| Error::new("snapshot_expired"))?;
        if s.principal != principal
            || s.generation != gen
            || s.operation != r.operation
            || generation(core)? != gen
        {
            return Err(Error::new("generation_conflict"));
        }
        if offset > s.items.len() {
            return Err(Error::invalid());
        }
        let end = (offset + limit).min(s.items.len());
        let mut value = json!({"items":&s.items[offset..end],"snapshot_frontier":s.frontier,"coverage":"partial","limitations":[],"claims_global_completion":false,"authorizes_reward":false});
        if end < s.items.len() {
            let mut bytes = key.to_vec();
            bytes.extend_from_slice(&(end as u64).to_be_bytes());
            let tag = blake3::keyed_hash(&core.obp.cursor_key, &bytes);
            bytes.extend_from_slice(tag.as_bytes());
            value["continuation"] = format!("obc1.{}", URL_SAFE_NO_PAD.encode(bytes)).into();
        }
        Ok(value)
    }
}
fn source_value(s: crate::vnext_outbound_product::DiscoverySourceStatus) -> Result<Value> {
    let mut v = json!({"source_id":hex(&s.source_id),"kind":s.kind,"state":s.state,"admitted_records":s.admitted_records,"limitations":s.limitations});
    if let Some(exp) = s.expires_at_unix_seconds {
        v["expires_at_unix_seconds"] = exp.into();
    }
    Ok(v)
}
fn close_routes(owner: &Owner) {
    if let Ok(mut routes) = owner.routes.lock() {
        for r in routes.values() {
            r.session.close();
        }
        routes.clear();
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        close_routes(self);
    }
}

#[cfg(test)]
mod tests;

fn crash_point(stage: &str) {
    #[cfg(test)]
    if std::env::var("OBP_API_TEST_CRASH").ok().as_deref() == Some(stage) {
        std::process::exit(73);
    }
    #[cfg(not(test))]
    let _ = stage;
}
