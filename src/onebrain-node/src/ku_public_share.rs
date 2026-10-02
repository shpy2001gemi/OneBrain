//! D-045 experimental disclosure: a new public manual kernel, never a private CID.
//! The host supplies an authorized saved KU view and owns the explicit gesture.
use base64::{engine::general_purpose::STANDARD, Engine};
use ku_core::foundation::semantic::{LiteralValue, SemanticFrameSet, TermRef};
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_knowledge_object, DisclosureClass, KnownObjectKind, NamespaceCommitment, NodeId,
    ObjectKind, ObjectSemantics, ResourceProfile, SelectorCid,
};
use onebrain_base_contract::ku::{ArtifactDisclosure, ArtifactValidity, KuViewV1};
use onebrain_protocol::ReconcileManifestKind;
use std::net::SocketAddr;

use crate::vnext_outbox::OutboundTransferIntent;

pub const PUBLIC_CONSEQUENCE: &str = "This new object and its semantic content become Public. The recipient may retain and forward it permanently. The private source and original private object are not included. Provenance is authenticated transport only, not original-source, author or fidelity verification.";

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
}
