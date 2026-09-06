use onebrain_base_contract::{ku::*, ku_payload::KuPayload};
use serde_json::Value;

#[test]
fn approved_positive_and_negative_dto_corpus_matches_generated_rust_validation() {
    let document: Value = serde_json::from_str(include_str!(
        "../../test-vectors/vnext/ku-product-workflow-v1.json"
    ))
    .unwrap();
    for fixture in document["fixtures"].as_array().unwrap() {
        let bytes = serde_json::to_vec(&fixture["value"]).unwrap();
        let valid = match fixture["dto"].as_str().unwrap() {
            "KuPreparedV1" => KuPreparedV1::decode(&bytes).is_ok(),
            "KuGetV1" => KuGetV1::decode(&bytes).is_ok(),
            "KuPrepareV1" => KuPrepareV1::decode(&bytes).is_ok(),
            "KuListV1" => KuListV1::decode(&bytes).is_ok(),
            "KuFailureV1" => KuFailureV1::decode(&bytes).is_ok(),
            "KuExportViewV1" => KuExportViewV1::decode(&bytes).is_ok(),
            other => panic!("add corpus consumer for {other}"),
        };
        assert_eq!(
            valid,
            fixture["valid"].as_bool().unwrap(),
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn every_operation_round_trips_only_under_its_registered_discriminator() {
    let prep = KuPrepareV1 {
        operation_id: OperationId([1; 32]),
        idempotency_key: IdempotencyKey([2; 32]),
        input_mode: InputMode::ResolvedSemanticDraft,
        source_refs: vec![SourceArtifactCID([3; 32])],
        registry_release_root: ReleaseRoot([4; 32]),
        semantic_profile: "ku-semantic-content/1.0".into(),
        implementation_commitment: ImplementationCommitment([5; 32]),
        destination: Disclosure::LOCALONLY,
        draft_ref: Some(ObjectCID([6; 32])),
    };
    let op = KuOperationRefV1 {
        operation_id: prep.operation_id,
    };
    let requests = [
        KuRequestV1::Prepare(prep.clone()),
        KuRequestV1::Preview(op.clone()),
        KuRequestV1::Save(KuSaveV1 {
            operation_id: prep.operation_id,
            idempotency_key: prep.idempotency_key,
            object_cids: vec![ObjectCID([7; 32])],
        }),
        KuRequestV1::Get(KuGetV1 {
            object_cid: ObjectCID([7; 32]),
        }),
        KuRequestV1::List(KuListV1 {
            limit: 1,
            continuation: None,
        }),
        KuRequestV1::Search(KuSearchV1 {
            query: "water".into(),
            limit: 1,
            continuation: None,
        }),
        KuRequestV1::Revise(KuReviseV1 {
            preparation: prep,
            predecessor_object_cid: ObjectCID([7; 32]),
            expected_revision_frontier: RevisionFrontier([8; 32]),
        }),
        KuRequestV1::Export(KuExportV1 {
            mode: ExportMode::CanonicalPublicExchange,
            object_cids: vec![ObjectCID([7; 32])],
        }),
        KuRequestV1::Status(KuStatusRequestV1 { operation_id: None }),
        KuRequestV1::Cancel(op.clone()),
        KuRequestV1::Reconcile(op),
    ];
    let idl: Value = serde_json::from_str(include_str!(
        "../../test-vectors/vnext/base-v1-runtime-interface-v1.json"
    ))
    .unwrap();
    for (request, schema) in requests
        .iter()
        .zip(idl["ku_payloads"]["operations"].as_array().unwrap())
    {
        assert_eq!(
            request.discriminator() as u64,
            schema["wire_id"].as_u64().unwrap()
        );
        let bytes = request.payload_bytes().unwrap();
        assert!(KuRequestV1::decode_for_base_minor(request.discriminator(), &bytes, 1).is_err());
        assert_eq!(
            KuRequestV1::decode_for_base_minor(request.discriminator(), &bytes, 2).unwrap(),
            *request
        );
        assert_eq!(
            KuRequestV1::decode(request.discriminator(), &bytes).unwrap(),
            *request
        );
        assert!(KuRequestV1::decode(0xffff, &bytes).is_err());
        let mut malformed: Value = serde_json::from_slice(&bytes).unwrap();
        malformed["authorized"] = Value::Bool(true);
        assert!(KuRequestV1::decode(
            request.discriminator(),
            &serde_json::to_vec(&malformed).unwrap()
        )
        .is_err());
    }
    assert!(KuListV1::decode(&vec![b' '; 1_048_577]).is_err());
    assert!(KuListV1::decode(br#"{"limit":1,"limit":2}"#).is_err());
    assert!(KuListV1::decode(br#"{"limit":1,"continuation":null}"#).is_err());
    assert!(KuListV1 {
        limit: 1,
        continuation: Some("obc1.not=padded".into())
    }
    .encode()
    .is_err());
}
