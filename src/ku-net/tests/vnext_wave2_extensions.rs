//! Wave 2 (`P2` + `D-012`) end-to-end integration tests:
//! - Negotiated `RIBLT-1` fast-path + Merkle root gate (`NEG-NET-004`) + seamless Radix fallback.
//! - Paginated `FrontierSummaryRequest` (`29`) / `FrontierSummaryPage` (`30`) with MAC cursors,
//!   conflicting head preservation (`NEG-NET-006`), and checkpoint verification gate.
//! - Live QUIC `obp/blob-chunks/1` (`fetch_blob_over_quic`) & `D-012` `obp/registry-chunks/1`.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    CheckpointCid, DisclosureClass, EventCid, FeedId, InventoryRecordKind, NamespaceCommitment,
    SelectorCid,
};
use ku_net::vnext_inventory_forest::{FeedPrefixInventory, HybridInventoryForest, InventoryLeaf};
use ku_net::vnext_reconciliation::{
    deterministic_diff_ranges, evaluate_riblt_sketch_or_fallback,
    finalize_riblt_or_fallback_to_radix, frontier_summary_page_message,
    frontier_summary_request_message, inventory_summary_message, reconcile_frontier_page,
    riblt_sketch_message, BoundPayloadFrame, FastPathDiffResolution, PayloadSinkOutcome,
    ReconciliationError, ReconciliationReceiver, ValidateThenAcceptSink,
};
use ku_net::vnext_riblt::{
    check_riblt_eligibility, RibltFallbackReason, RibltRuntimeGate, RibltSessionMemoryBudget,
};
use ku_net::vnext_session::{authenticate_session, create_finish, create_hello, create_welcome};
use onebrain_protocol::{
    bind_reconciliation_message, reconcile_frontier_capability, reconcile_riblt_capability,
    reconciliation_capability, reconciliation_profile, ReconcileManifestEntry,
    ReconcileManifestKind, ReconciliationBody, ReconciliationBudget, ReconciliationContext,
    ReconciliationResumeMode, ReconciliationSummaryMethod,
};

#[derive(Default)]
struct InMemorySink {
    records: BTreeMap<(u64, [u8; 32]), Vec<u8>>,
}

impl ValidateThenAcceptSink for InMemorySink {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        canonical_bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        match self.records.entry((kind as u64, cid)) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(canonical_bytes.to_vec());
                Ok(PayloadSinkOutcome::ValidatedStored)
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Ok(PayloadSinkOutcome::AlreadyPresent)
            }
        }
    }
}

fn make_context(
    session_id: [u8; 32],
    method: ReconciliationSummaryMethod,
) -> ReconciliationContext {
    ReconciliationContext {
        authenticated_transcript: session_id,
        selector: SelectorCid::from_bytes([0x21; 32]),
        namespace: NamespaceCommitment::from_bytes([0x22; 32]),
        disclosure: DisclosureClass::Public,
        summary_method: method,
        budget: ReconciliationBudget {
            max_summary_nodes: 256,
            max_diff_ranges: 256,
            max_manifest_entries: 256,
            max_payload_bytes: 65_536,
        },
        resume_mode: ReconciliationResumeMode::BoundTokenV1,
    }
}

#[test]
fn negotiated_riblt_fast_path_verifies_merkle_root_and_rejects_false_completion() {
    let init_key = SigningKey::from_bytes(&[0x01; 32]);
    let resp_key = SigningKey::from_bytes(&[0x02; 32]);
    let profiles = vec![reconciliation_profile()];
    let caps = vec![
        reconciliation_capability(),
        reconcile_riblt_capability(),
        reconcile_frontier_capability(),
    ];
    let binding = [0x03; 32];
    let hello = create_hello(
        &init_key,
        binding,
        [0x04; 32],
        profiles.clone(),
        caps.clone(),
        Vec::new(),
    )
    .unwrap();
    let welcome = create_welcome(
        &hello,
        binding,
        &resp_key,
        [0x05; 32],
        &profiles,
        &caps,
        Vec::new(),
    )
    .unwrap();
    let finish = create_finish(&hello, &welcome, &init_key, binding, &profiles, &caps).unwrap();
    let session =
        authenticate_session(&hello, &welcome, &finish, binding, &profiles, &caps).unwrap();

    let ctx = make_context(session.session_id, ReconciliationSummaryMethod::RibltV1);
    let mut local_forest = HybridInventoryForest::new(ctx.selector);
    let mut remote_forest = HybridInventoryForest::new(ctx.selector);

    for shared_idx in 0..500u32 {
        let shared_frame = BoundPayloadFrame::new(
            &ctx,
            ReconcileManifestKind::Object,
            format!("shared-ku-{shared_idx}").into_bytes(),
        )
        .unwrap();
        let shared_leaf = InventoryLeaf {
            record_kind: InventoryRecordKind::Object,
            cid: shared_frame.cid,
            canonical_length: shared_frame.canonical_bytes.len() as u64,
        };
        local_forest.insert_record(shared_leaf).unwrap();
        remote_forest.insert_record(shared_leaf).unwrap();
    }

    let mut remote_frames = Vec::new();
    for idx in 0..70u32 {
        let bytes = format!("remote-ku-payload-{idx}").into_bytes();
        let frame = BoundPayloadFrame::new(&ctx, ReconcileManifestKind::Object, bytes).unwrap();
        remote_forest
            .insert_record(InventoryLeaf {
                record_kind: InventoryRecordKind::Object,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            })
            .unwrap();
        remote_frames.push(frame);
    }

    let mut budget = RibltSessionMemoryBudget::new();
    // Default-off runtime gate rejects until enabled!
    assert_eq!(
        check_riblt_eligibility(
            RibltRuntimeGate::default(),
            &session,
            &ctx,
            &[0; 32],
            None,
            local_forest.root(),
            remote_forest.root(),
            1,
            &budget,
        )
        .unwrap_err(),
        RibltFallbackReason::FeatureDisabled
    );
    check_riblt_eligibility(
        RibltRuntimeGate::enabled(),
        &session,
        &ctx,
        &[0; 32],
        None,
        local_forest.root(),
        remote_forest.root(),
        1,
        &budget,
    )
    .unwrap();

    let sketch_msg =
        riblt_sketch_message(ctx.clone(), 1, &remote_forest, [0; 32], None, 70).unwrap();
    let FastPathDiffResolution::RibltCandidate {
        remote_only,
        local_only,
        target_subtree_root,
    } = evaluate_riblt_sketch_or_fallback(&ctx, 2, &local_forest, &sketch_msg, &mut budget)
        .unwrap()
    else {
        panic!("expected RibltCandidate");
    };
    assert!(local_only.is_empty());
    assert_eq!(remote_only.len(), 70);

    // Fetch missing items through normal Manifest -> Payload -> ValidateThenAcceptSink
    let mut manifest_entries: Vec<ReconcileManifestEntry> = remote_frames
        .iter()
        .map(|f| ReconcileManifestEntry {
            kind: f.kind,
            cid: f.cid,
            canonical_length: f.canonical_bytes.len() as u64,
        })
        .collect();
    manifest_entries.sort_by_key(|e| (e.kind as u64, e.cid));
    let manifest_msg = bind_reconciliation_message(
        ctx.clone(),
        2,
        ReconciliationBody::Manifest {
            entries: manifest_entries,
        },
    )
    .unwrap();

    let mut receiver = ReconciliationReceiver::new(ctx.clone(), InMemorySink::default()).unwrap();
    receiver.ingest_manifest(&manifest_msg).unwrap();
    let mut validated_leaves = Vec::new();
    for frame in &remote_frames {
        receiver.ingest_payload(frame);
        validated_leaves.push(InventoryLeaf {
            record_kind: InventoryRecordKind::Object,
            cid: frame.cid,
            canonical_length: frame.canonical_bytes.len() as u64,
        });
    }

    let gate_res = finalize_riblt_or_fallback_to_radix(
        &ctx,
        3,
        &local_forest,
        &receiver,
        &local_only,
        &validated_leaves,
        target_subtree_root,
    )
    .unwrap();
    assert!(gate_res.is_ok());

    // Tampered target root triggers fallback to Radix while preserving all 70 accepted CIDs!
    let mut bad_root = target_subtree_root;
    bad_root[31] ^= 1;
    let fallback = finalize_riblt_or_fallback_to_radix(
        &ctx,
        4,
        &local_forest,
        &receiver,
        &local_only,
        &validated_leaves,
        bad_root,
    )
    .unwrap();
    assert!(fallback.is_err());
    assert_eq!(receiver.accepted_cids().len(), 70);
}

#[test]
fn paginated_frontier_summary_exchange_verifies_mac_cursor_across_pages() {
    let ctx = make_context([0x33; 32], ReconciliationSummaryMethod::RadixForest256V1);
    let mut source = HybridInventoryForest::new(ctx.selector);
    let mut target = HybridInventoryForest::new(ctx.selector);
    let mut verified_events = BTreeSet::new();
    let mut verified_checkpoints = BTreeSet::new();

    for i in 1..=5u8 {
        let feed = FeedId::from_bytes([i; 32]);
        let head = [i + 0x20; 32];
        let cp = [i + 0x40; 32];
        verified_events.insert(head);
        verified_checkpoints.insert(cp);
        source
            .insert_feed_prefix(FeedPrefixInventory {
                feed,
                through_sequence: u64::from(i),
                head_event: EventCid::from_bytes(head),
                checkpoint_frontier_refs: vec![CheckpointCid::from_bytes(cp)],
            })
            .unwrap();
    }

    let summary = inventory_summary_message(ctx.clone(), 1, &source).unwrap();
    assert_eq!(
        deterministic_diff_ranges(&ctx, &target, &summary).unwrap_err(),
        ReconciliationError::UnexplainedHybridRootDifference
    );

    let cursor_secret = [0xAB; 32];
    let proofs = BTreeMap::new();
    let mut cursor = None;
    let mut seq = 2u64;
    let mut pages = 0usize;
    loop {
        let req = frontier_summary_request_message(ctx.clone(), seq, None, cursor, 2).unwrap();
        seq += 1;
        let page =
            frontier_summary_page_message(ctx.clone(), seq, &source, &req, &proofs, cursor_secret)
                .unwrap();
        seq += 1;
        let outcome = reconcile_frontier_page(
            &ctx,
            &mut target,
            &page,
            &verified_events,
            &verified_checkpoints,
        )
        .unwrap();
        pages += 1;
        if outcome.next_cursor.is_none() {
            assert!(outcome.coverage_complete);
            break;
        }
        assert!(!outcome.coverage_complete);
        cursor = outcome.next_cursor;
    }
    assert_eq!(pages, 3);
    assert_eq!(target.root(), source.root());
}

#[cfg(feature = "outbound-first")]
#[tokio::test]
async fn quic_blob_and_registry_chunk_streams_verify_hashes_and_merkle_proofs() {
    use ku_core::blob_store::{BlobCid, BlobType};
    use ku_core::foundation::{
        BlobRetentionState, NodeId, ObjectReference, OwnedBlobReferenceV1, OwnedBlobRole,
    };
    use ku_net::transport::{QuicTransport, TransportConfig};
    use ku_net::vnext_chunk_streams::{
        fetch_blob_over_quic, request_registry_message_over_quic,
        serve_one_blob_chunk_request_over_quic, serve_one_registry_chunk_request_over_quic,
        BlobChunkProvider, CanonicalBlobReferenceOracle, PeerTransportPenaltyHook,
        RegistryChunkProvider, RegistryChunkPullSession,
    };
    use onebrain_protocol::{
        compute_registry_chunk_cid, compute_registry_chunk_merkle_root,
        sign_registry_release_descriptor, RegistryChunkWireMessageV1, RegistryReleaseDescriptorV1,
        BLOB_CHUNK_BYTES, REGISTRY_CHUNK_BYTES,
    };

    struct AllowBlob(OwnedBlobReferenceV1);
    impl CanonicalBlobReferenceOracle for AllowBlob {
        fn prove_accepted_reference(&self, blob_cid: &BlobCid) -> Option<OwnedBlobReferenceV1> {
            (self.0.blob_cid == *blob_cid).then(|| self.0.clone())
        }
    }

    struct NoopPenalty;
    impl PeerTransportPenaltyHook for NoopPenalty {
        fn penalize_corrupt_registry_chunk(&mut self, _: NodeId, _: [u8; 32], _: u32) {}
    }

    let dir = tempfile::tempdir().unwrap();
    let blob_bytes = vec![0x5Eu8; BLOB_CHUNK_BYTES * 3 + 9_999];
    let blob_cid = BlobCid::from_content(BlobType::Image, &blob_bytes);
    let mut blob_provider = BlobChunkProvider::default();
    blob_provider
        .insert_blob(blob_cid, blob_bytes.clone())
        .unwrap();

    let signer = SigningKey::from_bytes(&[0x77; 32]);
    let reg_bytes = vec![0xA5u8; REGISTRY_CHUNK_BYTES + 16_384];
    let reg_root = *blake3::hash(&reg_bytes).as_bytes();
    let reg_cids: Vec<[u8; 32]> = reg_bytes
        .chunks(REGISTRY_CHUNK_BYTES)
        .enumerate()
        .map(|(i, c)| compute_registry_chunk_cid(i as u64, c))
        .collect();
    let mut descriptor = RegistryReleaseDescriptorV1 {
        release_sequence: 1,
        release_root: reg_root,
        total_bytes: reg_bytes.len() as u64,
        chunk_size_bytes: REGISTRY_CHUNK_BYTES as u32,
        total_chunks: 2,
        chunk_merkle_root: compute_registry_chunk_merkle_root(&reg_cids),
        optional_delta_from: None,
        minimum_compatible_reader_version: 1,
        signer_public_key: [0; 32],
        signature: [0; 64],
    };
    sign_registry_release_descriptor(&mut descriptor, &signer).unwrap();
    let reg_provider = RegistryChunkProvider::new(descriptor.clone(), &reg_bytes).unwrap();

    let server = QuicTransport::bind(TransportConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..TransportConfig::default()
    })
    .await
    .unwrap();
    let server_addr = server.local_addr().unwrap();
    let client = QuicTransport::bind(TransportConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        ..TransportConfig::default()
    })
    .await
    .unwrap();

    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(async move {
        let conn = server.accept().await.unwrap();
        // 1 manifest + 2 batches (3 chunks + 1 chunk)
        for _ in 0..3 {
            serve_one_blob_chunk_request_over_quic(&conn, &blob_provider)
                .await
                .unwrap();
        }
        // 2 registry chunk requests
        for _ in 0..2 {
            serve_one_registry_chunk_request_over_quic(&conn, &reg_provider)
                .await
                .unwrap();
        }
        let _ = done_rx.await;
        server.shutdown().await;
    });

    let conn = client.connect(server_addr).await.unwrap();
    let oracle = AllowBlob(
        OwnedBlobReferenceV1::new(
            ObjectReference::new(0, [0x44; 32]),
            blob_cid,
            OwnedBlobRole::OwnedOriginal,
            BlobRetentionState::Live,
            None,
        )
        .unwrap(),
    );
    let promoted = fetch_blob_over_quic(
        &conn,
        &oracle,
        blob_cid,
        &dir.path().join("blob_stage"),
        &dir.path().join("blob_store"),
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&promoted).unwrap(), blob_bytes);

    let mut reg_session = RegistryChunkPullSession::open_verified(
        descriptor,
        &[*signer.verifying_key().as_bytes()],
        &dir.path().join("registry"),
    )
    .unwrap();
    let peer = NodeId::from_bytes([0x88; 32]);
    let mut penalty = NoopPenalty;
    while let Some(req) = reg_session.schedule_next_chunk_request(peer).unwrap() {
        let delivery = request_registry_message_over_quic(&conn, &req)
            .await
            .unwrap();
        assert!(matches!(
            delivery,
            RegistryChunkWireMessageV1::ChunkDelivery { .. }
        ));
        reg_session
            .ingest_chunk_delivery(peer, &delivery, &mut penalty)
            .unwrap();
    }
    let final_obr = reg_session
        .finalize_staged_release(|p| Ok(*blake3::hash(&std::fs::read(p).unwrap()).as_bytes()))
        .unwrap();
    assert_eq!(std::fs::read(final_obr).unwrap(), reg_bytes);

    let _ = done_tx.send(());
    client.shutdown().await;
    server_task.await.unwrap();
}
