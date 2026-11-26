//! D-045 experimental disclosure: a new public manual kernel, never a private CID.
//! The host supplies an authorized saved KU view and owns the explicit gesture.
use base64::{engine::general_purpose::STANDARD, Engine};
use ku_core::foundation::semantic::{LiteralValue, SemanticFrameSet, TermRef};
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_knowledge_object, DisclosureClass, KnownObjectKind, NamespaceCommitment, NodeId,
    ObjectCid, ObjectKind, ObjectSemantics, ResourceProfile, SelectorCid,
};
use onebrain_base_contract::ku::{
    ArtifactDisclosure, ArtifactValidity, KuGetV1, KuRequestV1, KuResponseV1, KuViewV1, ObjectCID,
};
use onebrain_base_contract::ku_payload::{decode_hex, hex};
use onebrain_base_contract::ResourceBudgetV1;
use onebrain_protocol::ReconcileManifestKind;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::ku_product::KuServices;
pub use crate::ku_product::{
    inspect_public_ku, list_accepted_public_kus, KuPublicReader, MAX_PUBLIC_KU_LIST_ITEMS,
    MAX_PUBLIC_KU_QUERY_BYTES,
};
use crate::vnext_network_runtime::VNextNetworkRuntime;
use crate::vnext_outbox::{OutboundTransferIntent, OutboxEnqueueOutcome};

pub const PUBLIC_CONSEQUENCE: &str = "This new object and its semantic content become Public. The recipient may retain and forward it permanently. The private source and original private object are not included. Provenance is authenticated transport only, not original-source, author or fidelity verification.";
pub const MAX_PREPARED_PUBLIC_KU_SHARES: usize = 16;

/// D-063: which peer addresses a host may prepare a Public KU share to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PeerAddressScope {
    /// D-045 default (and the isolated peer demo): loopback peers only.
    #[default]
    LoopbackOnly,
    /// D-063 DEV-ONLY: loopback or a private-LAN peer; only for a host whose
    /// live share runtime was started with the explicit LAN opt-in.
    PrivateLan,
}

pub use crate::ku_product::is_private_lan_ip;

impl PeerAddressScope {
    /// A non-zero port and a loopback address, or (LAN scope only) a
    /// private-LAN address.
    pub fn allows(self, address: SocketAddr) -> bool {
        address.port() != 0
            && (address.ip().is_loopback()
                || (self == Self::PrivateLan && is_private_lan_ip(address.ip())))
    }
}

type LanRuntimes = Mutex<Vec<Weak<VNextNetworkRuntime>>>;
static PRIVATE_LAN_SHARE_RUNTIMES: OnceLock<LanRuntimes> = OnceLock::new();

/// D-063: mark a share runtime the host started with the explicit LAN opt-in.
/// Ignored unless the runtime is actually bound to a private-LAN address.
pub fn register_private_lan_share_runtime(runtime: &Arc<VNextNetworkRuntime>) {
    if !is_private_lan_ip(runtime.local_addr().ip()) {
        return;
    }
    if let Ok(mut list) = PRIVATE_LAN_SHARE_RUNTIMES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
    {
        list.retain(|weak| weak.strong_count() > 0);
        list.push(Arc::downgrade(runtime));
    }
}

/// D-063: the peer scope for a host's current live share runtime; loopback
/// only unless that exact runtime was registered with the LAN opt-in.
pub fn peer_scope_for_runtime(runtime: Option<&Arc<VNextNetworkRuntime>>) -> PeerAddressScope {
    let Some(runtime) = runtime else {
        return PeerAddressScope::LoopbackOnly;
    };
    let registered = PRIVATE_LAN_SHARE_RUNTIMES
        .get()
        .and_then(|list| list.lock().ok())
        .is_some_and(|list| {
            list.iter()
                .filter_map(Weak::upgrade)
                .any(|lan| Arc::ptr_eq(&lan, runtime))
        });
    if registered {
        PeerAddressScope::PrivateLan
    } else {
        PeerAddressScope::LoopbackOnly
    }
}

/// No Clone/Serialize/Debug: possession of this host-local prepared capability
/// cannot be fabricated from a response or used twice. The struct itself is
/// never persisted; a D-062 host keeps only its sealed prepare inputs and
/// re-derives it after restart (`PublicKuShareCoordinator::restore_view`),
/// accepting it only when the exact preview digest matches.
pub struct PreparedPublicKu {
    intent: OutboundTransferIntent,
    expires_at: u64,
    semantic_cid: [u8; 32],
    private_cid: [u8; 32],
}

impl PreparedPublicKu {
    pub fn preview(&self) -> &[u8] {
        &self.intent.canonical_bytes
    }
    pub fn public_cid(&self) -> [u8; 32] {
        self.intent.cid
    }
    pub fn semantic_cid(&self) -> [u8; 32] {
        self.semantic_cid
    }
    pub fn intent_id(&self) -> [u8; 32] {
        self.intent.id
    }
    pub fn expires_at(&self) -> u64 {
        self.expires_at
    }
    /// Must be called only for an authenticated explicit operator gesture,
    /// after rechecking access to the exact saved private view. No new fields
    /// can replace the prepared bytes, peer, selector, namespace or address.
    pub fn confirm(
        self,
        current: &KuViewV1,
        now: u64,
    ) -> Result<OutboundTransferIntent, &'static str> {
        if now >= self.expires_at {
            return Err("public_ku_consent_expired");
        }
        if current.canonical_bytes.len() > 350000 {
            return Err("saved_access_changed");
        }
        let bytes = STANDARD
            .decode(&current.canonical_bytes)
            .map_err(|_| "saved_access_changed")?;
        if current.object_cid.0 != self.private_cid
            || current.disclosure_class != ArtifactDisclosure::LOCALONLY
            || current.artifact_validity != ArtifactValidity::AcceptedKnown
            || ku_core::foundation::ReservedDomain::Object.digest(&bytes) != self.private_cid
        {
            return Err("saved_access_changed");
        }
        Ok(self.intent)
    }
}

/// MVP support is deliberately one resolved predicate + one text literal.
/// Unknown extensions, dependencies, spans and advanced semantics fail closed
/// rather than being stripped from a saved object. The peer address must be
/// allowed by `scope` (D-045 loopback; D-063 private LAN only when opted in).
pub fn prepare_public_ku(
    view: &KuViewV1,
    expected_peer: NodeId,
    address: SocketAddr,
    selector: SelectorCid,
    namespace: NamespaceCommitment,
    scope: PeerAddressScope,
    now: u64,
) -> Result<PreparedPublicKu, &'static str> {
    if view.disclosure_class != ArtifactDisclosure::LOCALONLY
        || view.artifact_validity != ArtifactValidity::AcceptedKnown
        || view.canonical_bytes.len() > 350000
        || expected_peer.as_bytes() == &[0; 32]
        || selector.as_bytes() == &[0; 32]
        || namespace.as_bytes() == &[0; 32]
        || !scope.allows(address)
    {
        return Err("unsupported_public_ku");
    }
    let bytes = STANDARD
        .decode(&view.canonical_bytes)
        .map_err(|_| "invalid_saved_ku")?;
    let object = decode_knowledge_object(
        &bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(ObjectKind(2), 1)],
        &[],
    )
    .map_err(|_| "invalid_saved_ku")?;
    if object.cid().into_bytes() != view.object_cid.0 {
        return Err("invalid_saved_ku");
    }
    let ObjectSemantics::Known(envelope) = object.semantics() else {
        return Err("unsupported_public_ku");
    };
    if envelope.kind != ObjectKind(2)
        || envelope.disclosure != DisclosureClass::LocalOnly
        || !envelope.references.is_empty()
        || !envelope.extensions.is_empty()
        || !envelope.critical_extensions.is_empty()
        || envelope.limits.is_some()
    {
        return Err("unsupported_public_ku");
    }
    let semantic = SemanticFrameSet::from_canonical_value(&envelope.payload)
        .map_err(|_| "unsupported_public_ku")?;
    if semantic.statements.len() != 1 {
        return Err("unsupported_public_ku");
    }
    let statement = &semantic.statements[0];
    if statement.arguments.len() != 1
        || !matches!(
            &statement.arguments[0],
            TermRef::Literal(LiteralValue::Text(_))
        )
        || !statement.constraints.is_empty()
        || statement.qualifiers != Default::default()
    {
        return Err("unsupported_public_ku");
    }
    let normalized = normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE)
        .map_err(|_| "unsupported_public_ku")?;
    if view.semantic_content_cid.map(|cid| cid.0) != Some(normalized.cid.into_bytes()) {
        return Err("invalid_saved_ku");
    }
    let (private, _) = normalized
        .semantic
        .to_knowledge_object(DisclosureClass::LocalOnly)
        .map_err(|_| "invalid_saved_ku")?
        .encode(ResourceProfile::ObjectV1)
        .map_err(|_| "invalid_saved_ku")?;
    if private != bytes {
        return Err("unsupported_public_ku");
    }
    let (public, _) = normalized
        .semantic
        .to_knowledge_object(DisclosureClass::Public)
        .map_err(|_| "invalid_saved_ku")?
        .encode(ResourceProfile::ObjectV1)
        .map_err(|_| "invalid_saved_ku")?;
    let intent = OutboundTransferIntent::new(
        expected_peer,
        address,
        selector,
        namespace,
        DisclosureClass::Public,
        ReconcileManifestKind::Object,
        public,
    )
    .map_err(|_| "invalid_public_ku_intent")?;
    Ok(PreparedPublicKu {
        intent,
        expires_at: now
            .checked_add(PUBLIC_KU_CONSENT_SECONDS)
            .ok_or("invalid_clock")?,
        semantic_cid: normalized.cid.into_bytes(),
        private_cid: view.object_cid.0,
    })
}

/// Read an authorized saved KU view directly through the normal host's
/// authenticated `KuServices` handle (`KuRequestV1::Get`).
pub async fn saved_ku_view(
    services: &KuServices,
    object_cid: &str,
) -> Result<KuViewV1, &'static str> {
    let cid = ObjectCID(decode_hex::<32>(object_cid).map_err(|_| "invalid_saved_ku")?);
    let budget =
        ResourceBudgetV1::try_new(256, 1_032_192, 1_000_000).map_err(|_| "ku_host_unavailable")?;
    match services
        .invoke(KuRequestV1::Get(KuGetV1 { object_cid: cid }), budget)
        .await
    {
        Ok(KuResponseV1::Get(view)) => Ok(view),
        _ => Err("saved_ku_not_accessible"),
    }
}

/// Consent window of one prepared Public KU share, in seconds.
pub const PUBLIC_KU_CONSENT_SECONDS: u64 = 900;

/// D-062: digest binding a durable prepared-share record to the exact preview
/// that `share_prepare` returned. A re-derived preview must match it.
pub fn prepared_preview_digest(preview: &serde_json::Value) -> Result<[u8; 32], &'static str> {
    let bytes = serde_json::to_vec(preview).map_err(|_| "invalid_public_ku_intent")?;
    Ok(
        *blake3::Hasher::new_derive_key("onebrain:ku:public-share-preview:1")
            .update(&bytes)
            .finalize()
            .as_bytes(),
    )
}

fn share_targets(
    expected_node_id: &str,
    selector: &str,
    namespace: &str,
) -> Result<(NodeId, SelectorCid, NamespaceCommitment), &'static str> {
    Ok((
        NodeId::from_bytes(decode_hex::<32>(expected_node_id).map_err(|_| "invalid_peer")?),
        SelectorCid::from_bytes(decode_hex::<32>(selector).map_err(|_| "invalid_selector")?),
        NamespaceCommitment::from_bytes(
            decode_hex::<32>(namespace).map_err(|_| "invalid_namespace")?,
        ),
    ))
}

fn share_preview_json(
    prepared_id: &str,
    proposal: &PreparedPublicKu,
    expected_node_id: &str,
    address: SocketAddr,
    selector: &str,
    namespace: &str,
) -> Result<serde_json::Value, &'static str> {
    Ok(serde_json::json!({
        "event": "share_prepared",
        "prepared_id": prepared_id,
        "intent_id": hex(&proposal.intent_id()),
        "public_object_cid": hex(&proposal.public_cid()),
        "semantic_content_cid": hex(&proposal.semantic_cid()),
        "canonical_base64": STANDARD.encode(proposal.preview()),
        "semantic": inspect_public_ku(proposal.preview())?,
        "expected_node_id": expected_node_id,
        "address": address,
        "selector": selector,
        "namespace": namespace,
        "expires_at": proposal.expires_at(),
        "consequence": PUBLIC_CONSEQUENCE,
        "enqueued": false,
        "payload_sent": false,
        "support": "one predicate and one text literal only"
    }))
}

/// Host-local D-045 prepare/confirm state machine shared by the normal host
/// and the isolated peer demo. It is in memory; the normal host (`KuStore`,
/// D-062) additionally persists sealed prepare inputs and restores entries.
#[derive(Default)]
pub struct PublicKuShareCoordinator {
    /// prepared_id -> (private ObjectCID, proposal, exact preview returned by prepare).
    prepared: BTreeMap<String, (String, PreparedPublicKu, serde_json::Value)>,
    /// D-063 peer address scope for new/restored preparations (default loopback).
    peer_scope: PeerAddressScope,
}

impl PublicKuShareCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// D-063: set by the normal host from its live share runtime before each
    /// prepare/restore; never widens an already prepared share.
    pub fn set_peer_scope(&mut self, scope: PeerAddressScope) {
        self.peer_scope = scope;
    }

    /// Whether a live (unconsumed) entry with this id is held in memory.
    pub fn contains(&self, prepared_id: &str) -> bool {
        self.prepared.contains_key(prepared_id)
    }

    /// Drop an entry without confirming it (e.g. its durable record failed).
    pub fn discard(&mut self, prepared_id: &str) {
        self.prepared.remove(prepared_id);
    }

    /// D-062: re-derive a prepared share from its persisted prepare inputs and
    /// the caller's current saved view. Accepted only while unexpired and when
    /// the re-derived preview digest equals the one recorded at prepare time;
    /// never extends the original expiry, enqueues or sends anything.
    #[allow(clippy::too_many_arguments)]
    pub fn restore_view(
        &mut self,
        prepared_id: &str,
        object_cid: &str,
        view: &KuViewV1,
        expected_node_id: &str,
        address: SocketAddr,
        selector: &str,
        namespace: &str,
        prepared_at: u64,
        preview_digest: [u8; 32],
        now: u64,
    ) -> Result<(), &'static str> {
        if self.prepared.contains_key(prepared_id) {
            return Ok(());
        }
        let expires_at = prepared_at
            .checked_add(PUBLIC_KU_CONSENT_SECONDS)
            .ok_or("prepared_share_missing")?;
        if expires_at <= now {
            return Err("prepared_share_missing");
        }
        let (peer, selector_id, namespace_id) =
            share_targets(expected_node_id, selector, namespace)
                .map_err(|_| "prepared_share_missing")?;
        let proposal = prepare_public_ku(
            view,
            peer,
            address,
            selector_id,
            namespace_id,
            self.peer_scope,
            prepared_at,
        )
        .map_err(|_| "saved_access_changed")?;
        let preview = share_preview_json(
            prepared_id,
            &proposal,
            expected_node_id,
            address,
            selector,
            namespace,
        )
        .map_err(|_| "prepared_share_missing")?;
        if prepared_preview_digest(&preview)? != preview_digest {
            return Err("prepared_share_missing");
        }
        self.prepared.insert(
            prepared_id.to_owned(),
            (object_cid.to_owned(), proposal, preview),
        );
        Ok(())
    }

    pub fn check_capacity(&mut self, now: u64) -> Result<(), &'static str> {
        self.prepared.retain(|_, (_, p, _)| p.expires_at() > now);
        if self.prepared.len() >= MAX_PREPARED_PUBLIC_KU_SHARES {
            return Err("prepared_limit");
        }
        Ok(())
    }

    pub fn prepare_view(
        &mut self,
        object_cid: &str,
        view: &KuViewV1,
        expected_node_id: &str,
        address: SocketAddr,
        selector: &str,
        namespace: &str,
        now: u64,
    ) -> Result<serde_json::Value, &'static str> {
        self.check_capacity(now)?;
        let (peer, selector_id, namespace_id) =
            share_targets(expected_node_id, selector, namespace)?;
        let proposal = prepare_public_ku(
            view,
            peer,
            address,
            selector_id,
            namespace_id,
            self.peer_scope,
            now,
        )?;
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| "consent_randomness_unavailable")?;
        let prepared_id = hex(&nonce);
        let preview = share_preview_json(
            &prepared_id,
            &proposal,
            expected_node_id,
            address,
            selector,
            namespace,
        )?;
        self.prepared.insert(
            prepared_id,
            (object_cid.to_owned(), proposal, preview.clone()),
        );
        Ok(preview)
    }

    /// D-059: read back the exact stored preview of a live prepared share
    /// (with its private ObjectCID so the caller can recheck access). Read-only:
    /// never consumes, extends, enqueues or sends. Expired or unknown ids fail
    /// with `prepared_share_missing`.
    pub fn prepared_preview(
        &self,
        prepared_id: &str,
        now: u64,
    ) -> Result<(&str, &serde_json::Value), &'static str> {
        self.prepared
            .get(prepared_id)
            .filter(|(_, p, _)| p.expires_at() > now)
            .map(|(cid, _, preview)| (cid.as_str(), preview))
            .ok_or("prepared_share_missing")
    }

    pub async fn prepare_with_services(
        &mut self,
        services: &KuServices,
        object_cid: &str,
        expected_node_id: &str,
        address: SocketAddr,
        selector: &str,
        namespace: &str,
        now: u64,
    ) -> Result<serde_json::Value, &'static str> {
        self.check_capacity(now)?;
        let view = saved_ku_view(services, object_cid).await?;
        self.prepare_view(
            object_cid,
            &view,
            expected_node_id,
            address,
            selector,
            namespace,
            now,
        )
    }

    pub fn prepared_object_cid(
        &self,
        prepared_id: &str,
        confirm_public: bool,
    ) -> Result<&str, &'static str> {
        if !confirm_public {
            return Err("explicit_public_confirmation_required");
        }
        self.prepared
            .get(prepared_id)
            .map(|(cid, _, _)| cid.as_str())
            .ok_or("prepared_share_missing")
    }

    pub fn confirm_view(
        &mut self,
        prepared_id: &str,
        confirm_public: bool,
        current: &KuViewV1,
        runtime: &VNextNetworkRuntime,
        now: u64,
    ) -> Result<serde_json::Value, &'static str> {
        if !confirm_public {
            return Err("explicit_public_confirmation_required");
        }
        let (_, proposal, _) = self
            .prepared
            .remove(prepared_id)
            .ok_or("prepared_share_missing")?;
        let intent = proposal.confirm(current, now)?;
        let outcome = runtime
            .enqueue_outbound(&intent)
            .map_err(|_| "outbox_unavailable_reprepare_same_intent")?;
        Ok(serde_json::json!({
            "event": "share_confirmed",
            "intent_id": hex(&intent.id),
            "public_object_cid": hex(&intent.cid),
            "outbox": format!("{outcome:?}"),
            "delivery": confirm_delivery_note(outcome),
            "authorizes_reward": false
        }))
    }

    pub async fn confirm_with_services(
        &mut self,
        services: &KuServices,
        prepared_id: &str,
        confirm_public: bool,
        runtime: &VNextNetworkRuntime,
        now: u64,
    ) -> Result<serde_json::Value, &'static str> {
        let cid = self
            .prepared_object_cid(prepared_id, confirm_public)?
            .to_owned();
        let view = saved_ku_view(services, &cid).await?;
        self.confirm_view(prepared_id, confirm_public, &view, runtime, now)
    }
}

/// Receiver-side D-045 inspection of an accepted Public KU and its
/// authenticated sender/selector transport provenance.
pub fn inspect_accepted_public_ku(
    runtime: &VNextNetworkRuntime,
    object_cid: &str,
    selector: &str,
) -> Result<serde_json::Value, &'static str> {
    let cid =
        ObjectCid::from_bytes(decode_hex::<32>(object_cid).map_err(|_| "invalid_public_identity")?);
    let selector =
        SelectorCid::from_bytes(decode_hex::<32>(selector).map_err(|_| "invalid_selector")?);
    let bytes = runtime
        .public_object(cid)
        .map_err(|_| "public_store_unavailable")?
        .ok_or("public_object_not_found")?;
    let semantic = inspect_public_ku(&bytes)?;
    let peers: Vec<_> = runtime
        .record_source_peers(ReconcileManifestKind::Object, cid.into_bytes(), selector)
        .map_err(|_| "provenance_unavailable")?
        .iter()
        .map(|id| hex(id.as_bytes()))
        .collect();
    Ok(serde_json::json!({
        "event": "public_object",
        "object_cid": object_cid,
        "canonical_base64": STANDARD.encode(bytes),
        "semantic": semantic,
        "authenticated_source_peers": peers,
        "provenance": "transport/selector only; original source withheld and unverifiable",
        "fidelity": "unassessed",
        "authorizes_reward": false
    }))
}

/// Confirmation-time delivery wording. Only a newly added intent is known to be
/// pending; a re-confirmed identical intent keeps whatever state it already
/// reached (it may already be `Acknowledged`), so the note defers to inspection.
pub fn confirm_delivery_note(outcome: OutboxEnqueueOutcome) -> &'static str {
    match outcome {
        OutboxEnqueueOutcome::Added => "pending; inspect intent for acknowledgement",
        OutboxEnqueueOutcome::Existing => {
            "existing intent unchanged; inspect intent for its current state"
        }
        OutboxEnqueueOutcome::RouteUpdated => {
            "existing intent, new peer address recorded; inspect intent for its current state"
        }
    }
}

/// Sender-side D-045 inspection of a durable outbound transfer intent.
pub fn inspect_outbound_intent(
    runtime: &VNextNetworkRuntime,
    intent_id: &str,
) -> Result<serde_json::Value, &'static str> {
    let id = decode_hex::<32>(intent_id).map_err(|_| "invalid_intent_identity")?;
    let intent = runtime
        .outbound_intent(&id)
        .map_err(|_| "outbox_unavailable")?
        .ok_or("intent_not_found")?;
    Ok(serde_json::json!({
        "event": "intent",
        "intent_id": intent_id,
        "public_object_cid": hex(&intent.cid),
        "state": format!("{:?}", intent.state),
        "terminal_sequence": intent.terminal_sequence,
        "claims_network_completion": false
    }))
}

/// Add `accepted_selectors` (hex, at most 16) to each `share_list` item from
/// the live runtime's transport observations, so step-4 inspection can be
/// prefilled. Unknown/unavailable provenance yields an empty list.
pub fn annotate_accepted_selectors(listed: &mut serde_json::Value, runtime: &VNextNetworkRuntime) {
    let Some(items) = listed["items"].as_array_mut() else {
        return;
    };
    for item in items {
        let selectors: Vec<String> = item["object_cid"]
            .as_str()
            .and_then(|cid| decode_hex::<32>(cid).ok())
            .and_then(|cid| {
                runtime
                    .record_selectors(ReconcileManifestKind::Object, cid)
                    .ok()
            })
            .unwrap_or_default()
            .iter()
            .map(|s| hex(s.as_bytes()))
            .collect();
        item["accepted_selectors"] = serde_json::json!(selectors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::foundation::semantic::{
        ConceptCcid, StatementFrame, StatementId, StatementQualifiers,
    };
    use ku_core::foundation::{NormalizedText, ObjectReference};
    use onebrain_base_contract::ku::{Coverage, ObjectCID, SemanticContentCID};

    fn view() -> KuViewV1 {
        let semantic = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: ConceptCcid::from_bytes([7; 16]),
                arguments: vec![TermRef::Literal(LiteralValue::Text(
                    NormalizedText::new("Water is a liquid.").unwrap(),
                ))],
                constraints: vec![],
                qualifiers: StatementQualifiers::default(),
            }],
        };
        let normalized = normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE).unwrap();
        let (bytes, cid) = semantic
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        KuViewV1 {
            object_cid: ObjectCID(cid.into_bytes()),
            canonical_bytes: STANDARD.encode(bytes),
            semantic_content_cid: Some(SemanticContentCID(normalized.cid.into_bytes())),
            disclosure_class: ArtifactDisclosure::LOCALONLY,
            artifact_validity: ArtifactValidity::AcceptedKnown,
            coverage: Coverage::LocalOnly,
            limitations: vec!["fidelity_unassessed".into()],
            executable: false,
            fidelity_policy_cid: None,
            fidelity_frontier: None,
        }
    }
    fn prepare(view: &KuViewV1, now: u64) -> Result<PreparedPublicKu, &'static str> {
        prepare_at(view, "127.0.0.1:43000", PeerAddressScope::LoopbackOnly, now)
    }
    fn prepare_at(
        view: &KuViewV1,
        address: &str,
        scope: PeerAddressScope,
        now: u64,
    ) -> Result<PreparedPublicKu, &'static str> {
        prepare_public_ku(
            view,
            NodeId::from_bytes([1; 32]),
            address.parse().unwrap(),
            SelectorCid::from_bytes([2; 32]),
            NamespaceCommitment::from_bytes([3; 32]),
            scope,
            now,
        )
    }

    #[test]
    fn peer_address_scope_is_loopback_unless_private_lan_opt_in() {
        let view = view();
        use PeerAddressScope::{LoopbackOnly, PrivateLan};
        for addr in ["127.0.0.1:43000", "[::1]:43000"] {
            assert!(prepare_at(&view, addr, LoopbackOnly, 100).is_ok(), "{addr}");
            assert!(prepare_at(&view, addr, PrivateLan, 100).is_ok(), "{addr}");
        }
        for addr in [
            "10.0.0.5:43000",
            "172.16.4.2:43000",
            "172.31.255.1:43000",
            "192.168.1.20:43000",
            "[fd12:3456::1]:43000",
        ] {
            assert_eq!(
                prepare_at(&view, addr, LoopbackOnly, 100).err(),
                Some("unsupported_public_ku"),
                "{addr}"
            );
            assert!(prepare_at(&view, addr, PrivateLan, 100).is_ok(), "{addr}");
        }
        for addr in [
            "0.0.0.0:43000",
            "[::]:43000",
            "8.8.8.8:43000",
            "172.32.0.1:43000",
            "169.254.1.1:43000",
            "224.0.0.1:43000",
            "255.255.255.255:43000",
            "[fe80::1]:43000",
            "[2001:db8::1]:43000",
            "[::ffff:192.168.1.20]:43000",
            "192.168.1.20:0",
            "127.0.0.1:0",
        ] {
            assert_eq!(
                prepare_at(&view, addr, PrivateLan, 100).err(),
                Some("unsupported_public_ku"),
                "{addr}"
            );
        }
        // Without a live runtime the scope is loopback only.
        assert_eq!(peer_scope_for_runtime(None), LoopbackOnly);
        // The coordinator applies its scope; restore re-derives with the scope
        // current at restore time, so a host restarted without the LAN opt-in
        // fails closed instead of widening.
        let private_cid = hex(&view.object_cid.0);
        let (peer, selector, namespace) = ("01".repeat(32), "02".repeat(32), "03".repeat(32));
        let lan: SocketAddr = "192.168.1.20:43000".parse().unwrap();
        let mut coord = PublicKuShareCoordinator::new();
        assert_eq!(
            coord.prepare_view(&private_cid, &view, &peer, lan, &selector, &namespace, 100),
            Err("unsupported_public_ku")
        );
        coord.set_peer_scope(PrivateLan);
        let preview = coord
            .prepare_view(&private_cid, &view, &peer, lan, &selector, &namespace, 100)
            .unwrap();
        assert_eq!(preview["address"], "192.168.1.20:43000");
        let id = preview["prepared_id"].as_str().unwrap().to_owned();
        let digest = prepared_preview_digest(&preview).unwrap();
        let mut restarted = PublicKuShareCoordinator::new();
        assert_eq!(
            restarted.restore_view(
                &id,
                &private_cid,
                &view,
                &peer,
                lan,
                &selector,
                &namespace,
                100,
                digest,
                200
            ),
            Err("saved_access_changed")
        );
        restarted.set_peer_scope(PrivateLan);
        restarted
            .restore_view(
                &id,
                &private_cid,
                &view,
                &peer,
                lan,
                &selector,
                &namespace,
                100,
                digest,
                200,
            )
            .unwrap();
        assert_eq!(restarted.prepared_preview(&id, 200).unwrap().1, &preview);
    }

    #[test]
    fn disclosure_creates_new_identity_and_confirmation_is_exact_and_idempotent() {
        let view = view();
        let original = view.clone();
        let preview = prepare(&view, 100).unwrap();
        assert_ne!(preview.public_cid(), view.object_cid.0);
        assert_eq!(preview.semantic_cid(), view.semantic_content_cid.unwrap().0);
        let private_cid = view.object_cid.0;
        assert!(!preview.preview().windows(32).any(|b| b == private_cid));
        let expected = preview.preview().to_vec();
        let displayed = inspect_public_ku(&expected).unwrap();
        assert_eq!(
            displayed["statements"][0]["text_arguments"][0],
            "Water is a liquid."
        );
        assert_eq!(
            displayed["semantic_content_cid"],
            onebrain_base_contract::ku_payload::hex(&preview.semantic_cid())
        );
        let intent = preview.confirm(&view, 101).unwrap();
        assert_eq!(intent.canonical_bytes, expected);
        assert_eq!(intent.expected_peer, NodeId::from_bytes([1; 32]));
        let replay = prepare(&view, 200).unwrap().confirm(&view, 201).unwrap();
        assert_eq!(intent.id, replay.id);
        assert_eq!(view, original);
    }

    #[test]
    fn only_a_newly_added_intent_claims_pending_delivery_at_confirmation() {
        assert_eq!(
            confirm_delivery_note(OutboxEnqueueOutcome::Added),
            "pending; inspect intent for acknowledgement"
        );
        for outcome in [
            OutboxEnqueueOutcome::Existing,
            OutboxEnqueueOutcome::RouteUpdated,
        ] {
            let note = confirm_delivery_note(outcome);
            assert!(!note.contains("pending"), "{outcome:?}: {note}");
            assert!(note.starts_with("existing intent"), "{outcome:?}: {note}");
            assert!(note.ends_with("inspect intent for its current state"));
        }
    }

    #[test]
    fn private_dependencies_and_unsupported_extensions_are_not_silently_stripped() {
        let mut view = view();
        let bytes = STANDARD.decode(&view.canonical_bytes).unwrap();
        let object = decode_knowledge_object(
            &bytes,
            ResourceProfile::ObjectV1,
            &[KnownObjectKind::new(ObjectKind(2), 1)],
            &[],
        )
        .unwrap();
        let ObjectSemantics::Known(mut envelope) = object.semantics().clone() else {
            panic!()
        };
        envelope.references.push(ObjectReference::new(1, [9; 32]));
        let (bytes, cid) = envelope.encode(ResourceProfile::ObjectV1).unwrap();
        view.canonical_bytes = STANDARD.encode(bytes);
        view.object_cid = ObjectCID(cid.into_bytes());
        assert!(matches!(prepare(&view, 100), Err("unsupported_public_ku")));
        envelope.references.clear();
        envelope.extensions.push((
            100,
            ku_core::foundation::CanonicalValue::Text("PRIVATE".into()),
        ));
        let (bytes, cid) = envelope.encode(ResourceProfile::ObjectV1).unwrap();
        view.canonical_bytes = STANDARD.encode(bytes);
        view.object_cid = ObjectCID(cid.into_bytes());
        assert!(prepare(&view, 100).is_err());
        envelope.extensions.clear();
        let mut semantic = SemanticFrameSet::from_canonical_value(&envelope.payload).unwrap();
        semantic.statements[0].qualifiers.source_spans.push(
            ku_core::foundation::semantic::SourceSpan {
                source: ObjectReference::new(1, [9; 32]),
                start: 0,
                end: 1,
            },
        );
        let (bytes, cid) = semantic
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap();
        view.canonical_bytes = STANDARD.encode(bytes);
        view.object_cid = ObjectCID(cid.into_bytes());
        assert!(matches!(prepare(&view, 100), Err("unsupported_public_ku")));
    }

    #[test]
    fn expiry_or_changed_access_fails_before_outbound_intent_is_returned() {
        let mut view = view();
        assert!(matches!(
            prepare(&view, 100).unwrap().confirm(&view, 1000),
            Err("public_ku_consent_expired")
        ));
        let prepared = prepare(&view, 100).unwrap();
        view.object_cid.0 = [8; 32];
        assert!(matches!(
            prepared.confirm(&view, 101),
            Err("saved_access_changed")
        ));
    }

    #[test]
    fn coordinator_enforces_explicit_confirmation_capacity_and_expiry() {
        let view = view();
        let private_cid = hex(&view.object_cid.0);
        let peer = "01".repeat(32);
        let selector = "02".repeat(32);
        let namespace = "03".repeat(32);
        let addr: SocketAddr = "127.0.0.1:43000".parse().unwrap();
        let mut coord = PublicKuShareCoordinator::new();
        let mut first_id = String::new();
        for i in 0..MAX_PREPARED_PUBLIC_KU_SHARES {
            let preview = coord
                .prepare_view(&private_cid, &view, &peer, addr, &selector, &namespace, 100)
                .unwrap();
            assert_eq!(preview["event"], "share_prepared");
            assert_eq!(preview["enqueued"], false);
            assert_eq!(preview["payload_sent"], false);
            assert_ne!(preview["public_object_cid"], private_cid);
            assert_eq!(
                preview["semantic_content_cid"],
                hex(&view.semantic_content_cid.unwrap().0)
            );
            assert!(!preview.to_string().contains(&private_cid));
            if i == 0 {
                first_id = preview["prepared_id"].as_str().unwrap().to_owned();
            }
        }
        assert_eq!(
            coord.prepare_view(&private_cid, &view, &peer, addr, &selector, &namespace, 100),
            Err("prepared_limit")
        );
        assert_eq!(
            coord.prepared_object_cid(&first_id, false),
            Err("explicit_public_confirmation_required")
        );
        assert_eq!(
            coord.prepared_object_cid(&first_id, true),
            Ok(private_cid.as_str())
        );
        // D-059 read-back returns the exact stored preview without consuming it.
        let (cid, stored) = coord.prepared_preview(&first_id, 101).unwrap();
        assert_eq!(cid, private_cid);
        assert_eq!(stored["prepared_id"], first_id);
        assert_eq!(stored["enqueued"], false);
        assert_eq!(stored["payload_sent"], false);
        assert!(!stored.to_string().contains(&private_cid));
        assert!(coord.prepared_preview(&first_id, 101).is_ok());
        assert_eq!(
            coord.prepared_preview(&"ff".repeat(32), 101),
            Err("prepared_share_missing")
        );
        assert_eq!(
            coord.prepared_preview(&first_id, 1000),
            Err("prepared_share_missing")
        );
        assert_eq!(coord.check_capacity(1000), Ok(()));
        assert_eq!(
            coord.prepared_object_cid(&first_id, true),
            Err("prepared_share_missing")
        );
    }

    #[test]
    fn restored_prepared_share_matches_exact_preview_and_never_extends_expiry() {
        let view = view();
        let private_cid = hex(&view.object_cid.0);
        let peer = "01".repeat(32);
        let selector = "02".repeat(32);
        let namespace = "03".repeat(32);
        let addr: SocketAddr = "127.0.0.1:43000".parse().unwrap();
        let mut before = PublicKuShareCoordinator::new();
        let preview = before
            .prepare_view(&private_cid, &view, &peer, addr, &selector, &namespace, 100)
            .unwrap();
        let id = preview["prepared_id"].as_str().unwrap().to_owned();
        let digest = prepared_preview_digest(&preview).unwrap();
        let restore = |coord: &mut PublicKuShareCoordinator, v: &KuViewV1, d, now| {
            coord.restore_view(
                &id,
                &private_cid,
                v,
                &peer,
                addr,
                &selector,
                &namespace,
                100,
                d,
                now,
            )
        };
        // A fresh coordinator (host restart) restores the identical preview.
        let mut after = PublicKuShareCoordinator::new();
        assert!(!after.contains(&id));
        restore(&mut after, &view, digest, 500).unwrap();
        let (cid, stored) = after.prepared_preview(&id, 500).unwrap();
        assert_eq!(cid, private_cid);
        assert_eq!(stored, &preview);
        assert_eq!(stored["expires_at"], 100 + PUBLIC_KU_CONSENT_SECONDS);
        // Restoring is idempotent and confirmation still consumes it once.
        restore(&mut after, &view, digest, 501).unwrap();
        assert_eq!(
            after.prepared_object_cid(&id, true),
            Ok(private_cid.as_str())
        );
        after.discard(&id);
        assert!(!after.contains(&id));
        // Mismatched digest, expiry and changed saved access fail closed.
        let mut other = PublicKuShareCoordinator::new();
        assert_eq!(
            restore(&mut other, &view, [9; 32], 500),
            Err("prepared_share_missing")
        );
        assert_eq!(
            restore(&mut other, &view, digest, 100 + PUBLIC_KU_CONSENT_SECONDS),
            Err("prepared_share_missing")
        );
        let mut changed = view.clone();
        changed.disclosure_class = ArtifactDisclosure::PUBLIC;
        assert_eq!(
            restore(&mut other, &changed, digest, 500),
            Err("saved_access_changed")
        );
        assert!(!other.contains(&id));
    }

    #[test]
    fn list_accepted_public_kus_filters_bounds_and_skips_non_ku_records() {
        use crate::vnext_validated_sink::{SharedVNextValidatedSink, VNextValidatedSink};
        use ku_core::foundation::{AcceptedRecordEntry, InMemoryVerifiedBackend, StoredRecordKind};

        let sink = SharedVNextValidatedSink::new(VNextValidatedSink::new(
            InMemoryVerifiedBackend::default(),
        ));
        let make_public = |pred: u8, text: &str| {
            let semantic = SemanticFrameSet {
                statements: vec![StatementFrame {
                    statement_id: StatementId(0),
                    operator_or_predicate: ConceptCcid::from_bytes([pred; 16]),
                    arguments: vec![TermRef::Literal(LiteralValue::Text(
                        NormalizedText::new(text).unwrap(),
                    ))],
                    constraints: vec![],
                    qualifiers: StatementQualifiers::default(),
                }],
            };
            let normalized =
                normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE).unwrap();
            normalized
                .semantic
                .to_knowledge_object(DisclosureClass::Public)
                .unwrap()
                .encode(ResourceProfile::ObjectV1)
                .unwrap()
        };
        let (b1, c1) = make_public(7, "Water is a liquid.");
        let (b2, c2) = make_public(8, "Ice melts at zero.");
        let mut opaque_obj = SemanticFrameSet {
            statements: vec![StatementFrame {
                statement_id: StatementId(0),
                operator_or_predicate: ConceptCcid::from_bytes([9; 16]),
                arguments: vec![],
                constraints: vec![],
                qualifiers: StatementQualifiers::default(),
            }],
        }
        .to_knowledge_object(DisclosureClass::Public)
        .unwrap();
        opaque_obj.kind = ObjectKind(999);
        let (b_opaque, c_opaque) = opaque_obj.encode(ResourceProfile::ObjectV1).unwrap();
        for (cid, bytes) in [(c1, b1), (c2, b2), (c_opaque, b_opaque)] {
            sink.restore_accepted_record(&AcceptedRecordEntry {
                record_kind: StoredRecordKind::Object,
                claimed_cid: cid.into_bytes(),
                canonical_bytes: bytes,
            })
            .unwrap();
        }

        let all = list_accepted_public_kus(&sink, None, None, None).unwrap();
        assert_eq!(all["event"], "public_objects");
        assert_eq!(all["has_more"], false);
        assert_eq!(all["authorizes_reward"], false);
        assert_eq!(all["fidelity"], "unassessed");
        let items = all["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert!(!all.to_string().contains(&hex(c_opaque.as_bytes())));

        let truncated = list_accepted_public_kus(&sink, None, None, Some(1)).unwrap();
        assert_eq!(truncated["items"].as_array().unwrap().len(), 1);
        assert_eq!(truncated["has_more"], true);

        let by_text = list_accepted_public_kus(&sink, None, Some("  LIQUID "), None).unwrap();
        assert_eq!(by_text["items"].as_array().unwrap().len(), 1);
        assert_eq!(by_text["items"][0]["object_cid"], hex(c1.as_bytes()));

        let by_pred = list_accepted_public_kus(&sink, None, Some(&"08".repeat(16)), None).unwrap();
        assert_eq!(by_pred["items"].as_array().unwrap().len(), 1);
        assert_eq!(by_pred["items"][0]["object_cid"], hex(c2.as_bytes()));

        assert_eq!(
            list_accepted_public_kus(&sink, None, None, Some(0)),
            Err("invalid_public_list_limit")
        );
        assert_eq!(
            list_accepted_public_kus(&sink, None, None, Some(65)),
            Err("invalid_public_list_limit")
        );
        assert_eq!(
            list_accepted_public_kus(&sink, None, Some(&"x".repeat(257)), None),
            Err("invalid_public_query")
        );
    }
}
