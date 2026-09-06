use ku_core::foundation::semantic::{
    SemanticFrameSet, SourceSpan, StatementId, TermRef, VariableId,
};
use ku_core::foundation::semantic_content::{normalize_semantic_content, SEMANTIC_CONTENT_PROFILE};
use ku_core::foundation::{
    decode_canonical, DisclosureClass, ObjectCid, ObjectReference, ReservedDomain, ResourceProfile,
};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn semantic_content_golden_equality_and_separation_across_processes() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../test-vectors/vnext/ku-semantic-content-v1.json"
    ))
    .unwrap();
    let mut identities = std::collections::BTreeSet::new();
    for row in fixture["vectors"].as_array().unwrap() {
        let bytes = unhex(row["canonical_hex"].as_str().unwrap());
        let semantic = SemanticFrameSet::from_canonical_value(
            &decode_canonical(&bytes, ResourceProfile::ObjectV1).unwrap(),
        )
        .unwrap();
        let normalized = normalize_semantic_content(&semantic, SEMANTIC_CONTENT_PROFILE).unwrap();
        assert_eq!(normalized.canonical_bytes, bytes);
        assert_eq!(
            normalized.cid.to_string(),
            row["semantic_cid"].as_str().unwrap()
        );
        assert!(identities.insert(normalized.cid.to_string()));
        assert_ne!(row["semantic_cid"], row["other_profile_cid"]);
        assert_ne!(row["semantic_cid"], row["object_domain_same_root"]);
        assert!(ObjectCid::compute(ReservedDomain::SemanticContent, &bytes).is_err());
        assert!(normalize_semantic_content(&semantic, "ku-semantic-content/2.0").is_err());
        let mut provenance = semantic.clone();
        for (i, s) in provenance.statements.iter_mut().enumerate() {
            s.qualifiers.source_spans.push(SourceSpan {
                source: ObjectReference::new(1, [i as u8 + 1; 32]),
                start: 3,
                end: 9,
            });
        }
        let with_source =
            normalize_semantic_content(&provenance, SEMANTIC_CONTENT_PROFILE).unwrap();
        assert_eq!(with_source.cid, normalized.cid);
        assert_ne!(
            with_source.private_input_bytes,
            normalized.private_input_bytes
        );
        let private = normalized
            .semantic
            .to_knowledge_object(DisclosureClass::LocalOnly)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap()
            .1;
        let public = normalized
            .semantic
            .to_knowledge_object(DisclosureClass::Public)
            .unwrap()
            .encode(ResourceProfile::ObjectV1)
            .unwrap()
            .1;
        assert_ne!(private, public);
    }
    // A second process evaluates the independent constants with fresh process state.
    if std::env::var_os("ONEBRAIN_KU_GOLDEN_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "semantic_content_golden_equality_and_separation_across_processes",
            ])
            .env("ONEBRAIN_KU_GOLDEN_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}

#[test]
fn alpha_renaming_changes_private_mapping_but_not_semantic_identity() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../test-vectors/vnext/ku-semantic-content-v1.json"
    ))
    .unwrap();
    let bytes = unhex(fixture["vectors"][0]["canonical_hex"].as_str().unwrap());
    let original = SemanticFrameSet::from_canonical_value(
        &decode_canonical(&bytes, ResourceProfile::ObjectV1).unwrap(),
    )
    .unwrap();
    let mut renamed = original.clone();
    renamed.statements[0].statement_id = StatementId(900);
    renamed.statements[1].statement_id = StatementId(123);
    if let TermRef::Variable { id, .. } = &mut renamed.statements[0].arguments[0] {
        *id = VariableId(888);
    }
    renamed.statements[1].arguments[0] = TermRef::Statement(StatementId(900));
    let a = normalize_semantic_content(&original, SEMANTIC_CONTENT_PROFILE).unwrap();
    let b = normalize_semantic_content(&renamed, SEMANTIC_CONTENT_PROFILE).unwrap();
    assert_eq!(a.cid, b.cid);
    assert_eq!(b.original_statement_ids, vec![900, 123]);
    renamed.statements[0]
        .qualifiers
        .source_spans
        .push(SourceSpan {
            source: ObjectReference::new(1, [1; 32]),
            start: 9,
            end: 3,
        });
    assert!(normalize_semantic_content(&renamed, SEMANTIC_CONTENT_PROFILE).is_err());
}
