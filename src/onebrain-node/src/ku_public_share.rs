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

use crate::ku_product::KuServices;
use crate::vnext_network_runtime::VNextNetworkRuntime;
use crate::vnext_outbox::OutboundTransferIntent;

pub const PUBLIC_CONSEQUENCE: &str = "This new object and its semantic content become Public. The recipient may retain and forward it permanently. The private source and original private object are not included. Provenance is authenticated transport only, not original-source, author or fidelity verification.";
pub const MAX_PREPARED_PUBLIC_KU_SHARES: usize = 16;

/// No Clone/Serialize/Debug: possession of this host-local prepared capability
/// cannot be fabricated from a response or used twice. Restart discards it.
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
/// rather than being stripped from a saved object.
pub fn prepare_public_ku(
    view: &KuViewV1,
    expected_peer: NodeId,
    address: SocketAddr,
    selector: SelectorCid,
    namespace: NamespaceCommitment,
    now: u64,
) -> Result<PreparedPublicKu, &'static str> {
    if view.disclosure_class != ArtifactDisclosure::LOCALONLY
        || view.artifact_validity != ArtifactValidity::AcceptedKnown
        || view.canonical_bytes.len() > 350000
        || expected_peer.as_bytes() == &[0; 32]
        || selector.as_bytes() == &[0; 32]
        || namespace.as_bytes() == &[0; 32]
        || !address.ip().is_loopback()
        || address.port() == 0
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
        expires_at: now.checked_add(900).ok_or("invalid_clock")?,
        semantic_cid: normalized.cid.into_bytes(),
        private_cid: view.object_cid.0,
    })
}

/// Render content of a known public kernel for explicit review/inspection.
/// This comparison identity and display grant no authority or source access.
pub fn inspect_public_ku(bytes: &[u8]) -> Result<serde_json::Value, &'static str> {
    let object = decode_knowledge_object(
        bytes,
        ResourceProfile::ObjectV1,
        &[KnownObjectKind::new(ObjectKind(2), 1)],
        &[],
    )
    .map_err(|_| "invalid_public_ku")?;
    let ObjectSemantics::Known(envelope) = object.semantics() else {
        return Err("unsupported_public_ku");
    };
    if envelope.kind != ObjectKind(2)
        || envelope.disclosure != DisclosureClass::Public
        || !envelope.references.is_empty()
        || !envelope.extensions.is_empty()
        || !envelope.critical_extensions.is_empty()
        || envelope.limits.is_some()
    {
        return Err("unsupported_public_ku");
    }
    let semantic = SemanticFrameSet::from_canonical_value(&envelope.payload)
        .map_err(|_| "unsupported_public_ku")?;
    if semantic.statements.len() != 1
        || semantic.statements.iter().any(|s| {
            s.arguments.len() != 1
                || !matches!(&s.arguments[0], TermRef::Literal(LiteralValue::Text(_)))
                || !s.constraints.is_empty()
                || s.qualifiers != Default::default()
        })
    {
        return Err("unsupported_public_ku");
    }
    let normalized = normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE)
        .map_err(|_| "unsupported_public_ku")?;
    let statements: Vec<_> = semantic.statements.iter().map(|s| {
        let arguments: Vec<_> = s.arguments.iter().map(|a| match a {
            TermRef::Literal(LiteralValue::Text(t)) => Some(t.as_str().to_owned()),
            _ => None,
        }).collect();
        serde_json::json!({"predicate_ccid":onebrain_base_contract::ku_payload::hex(s.operator_or_predicate.as_bytes()),"text_arguments":arguments})
    }).collect();
    Ok(
        serde_json::json!({"semantic_content_cid":onebrain_base_contract::ku_payload::hex(normalized.cid.as_bytes()),"statements":statements,"fidelity":"unassessed"}),
    )
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

/// Host-local, in-memory D-045 prepare/confirm state machine shared by the
/// normal host and the isolated peer demo.
#[derive(Default)]
pub struct PublicKuShareCoordinator {
    prepared: BTreeMap<String, (String, PreparedPublicKu)>,
}

impl PublicKuShareCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_capacity(&mut self, now: u64) -> Result<(), &'static str> {
        self.prepared.retain(|_, (_, p)| p.expires_at() > now);
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
        let peer =
            NodeId::from_bytes(decode_hex::<32>(expected_node_id).map_err(|_| "invalid_peer")?);
        let selector_id =
            SelectorCid::from_bytes(decode_hex::<32>(selector).map_err(|_| "invalid_selector")?);
        let namespace_id = NamespaceCommitment::from_bytes(
            decode_hex::<32>(namespace).map_err(|_| "invalid_namespace")?,
        );
        let proposal = prepare_public_ku(view, peer, address, selector_id, namespace_id, now)?;
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| "consent_randomness_unavailable")?;
        let prepared_id = hex(&nonce);
        let preview = serde_json::json!({
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
        });
        self.prepared
            .insert(prepared_id, (object_cid.to_owned(), proposal));
        Ok(preview)
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
            .map(|(cid, _)| cid.as_str())
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
        let (_, proposal) = self
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
            "delivery": "pending; inspect intent for acknowledgement",
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
        prepare_public_ku(
            view,
            NodeId::from_bytes([1; 32]),
            "127.0.0.1:43000".parse().unwrap(),
            SelectorCid::from_bytes([2; 32]),
            NamespaceCommitment::from_bytes([3; 32]),
            now,
        )
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
        assert_eq!(coord.check_capacity(1000), Ok(()));
        assert_eq!(
            coord.prepared_object_cid(&first_id, true),
            Err("prepared_share_missing")
        );
    }
}
