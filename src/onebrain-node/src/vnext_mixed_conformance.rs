//! Executable mixed-version and cross-carrier conformance matrix.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ku_core::foundation::{
    ConceptCcid, DisclosureClass, EventCid, NamespaceCommitment, ObjectReference, SelectorCid,
};
use ku_net::vnext_bridge_merge::{BridgePathId, MultiBridgeInbox};
use ku_net::vnext_carrier::{
    CarrierRecord, DeliveryInjection, DeterministicCarrier, FileBundleCarrier, InMemoryCarrier,
};
use ku_net::vnext_carrier_adapter::{DelayedCarrier, QuicRecordAdapter};
use ku_net::vnext_reconciliation::{
    BoundPayloadFrame, PayloadSinkOutcome, ReceiverState, ValidateThenAcceptSink,
};
use ku_net::vnext_reconciliation_journal::{
    InMemoryReconciliationJournalBackend, JournaledReconciliationSession,
    ReconciliationJournalConfig,
};
use onebrain_protocol::{
    bind_reconciliation_message, encode_reconciliation_message, LegacyAdapter, LegacyAdapterOffer,
    ReconcileManifestEntry, ReconcileManifestKind, ReconciliationBody, ReconciliationBudget,
    ReconciliationContext, ReconciliationResumeMode, ReconciliationSummaryMethod,
    LEGACY_ENCODING_FULL, LEGACY_SCOPE_GLOBAL,
};

use crate::vnext_config::VNextFeatureConfig;
use crate::vnext_status::{CoverageViewStatus, LocalUsability, VNextStatusSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConformancePeerPair {
    VNextToVNext,
    LegacyToVNext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConformanceCarrier {
    InMemory,
    FileBundle,
    QuicFrame,
    DelayedStoreCarryForward,
    NegotiatedLegacyAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceCell {
    pub peer_pair: ConformancePeerPair,
    pub carrier: ConformanceCarrier,
    pub semantic_result_digest: Option<[u8; 32]>,
    pub grants_authority: bool,
    pub claims_network_completion: bool,
    pub establishes_fidelity: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedVersionConformanceReport {
    pub cells: Vec<ConformanceCell>,
    pub cross_carrier_semantics_equal: bool,
    pub usable_during_seed_outage: bool,
    pub relay_delay_preserves_unknown_pending: bool,
    pub unsafe_legacy_offer_rejected: bool,
}

impl MixedVersionConformanceReport {
    pub fn has_authority_amplification(&self) -> bool {
        self.cells.iter().any(|cell| cell.grants_authority)
    }

    pub fn has_network_completion_amplification(&self) -> bool {
        self.cells.iter().any(|cell| cell.claims_network_completion)
    }
}

#[derive(Clone, Default)]
struct MatrixSink(Arc<Mutex<BTreeMap<(u64, [u8; 32]), Vec<u8>>>>);

impl ValidateThenAcceptSink for MatrixSink {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        let mut records = self
            .0
            .lock()
            .map_err(|_| "MIXED_MATRIX_SINK_LOCK".to_string())?;
        let key = (kind as u64, cid);
        if records.contains_key(&key) {
            Ok(PayloadSinkOutcome::AlreadyPresent)
        } else {
            records.insert(key, bytes.to_vec());
            Ok(PayloadSinkOutcome::ValidatedStored)
        }
    }
}

pub fn run_mixed_version_conformance_matrix(
    file_bundle_path: &Path,
) -> Result<MixedVersionConformanceReport, String> {
    let source = canonical_records()?;
    let plan = DeliveryInjection::default();

    let mut memory = InMemoryCarrier::default();
    enqueue_all(&mut memory, &source)?;
    let memory_digest = accepted_digest(memory.deliver(&plan).map_err(debug)?, 1)?;

    let mut file = FileBundleCarrier::open(file_bundle_path).map_err(debug)?;
    if file.record_count() != 0 {
        return Err("MIXED_MATRIX_FILE_NOT_EMPTY".into());
    }
    enqueue_all(&mut file, &source)?;
    let file_digest = accepted_digest(file.deliver(&plan).map_err(debug)?, 2)?;

    let quic_records = source
        .iter()
        .map(|record| {
            QuicRecordAdapter::encode(record)
                .and_then(|frame| QuicRecordAdapter::decode(&frame))
                .map_err(debug)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let quic_digest = accepted_digest(quic_records, 3)?;

    let mut delayed_inner = InMemoryCarrier::default();
    enqueue_all(&mut delayed_inner, &source)?;
    let delayed = DelayedCarrier::new(delayed_inner, 10);
    let before_release = delayed.deliver_at(9, &plan).map_err(debug)?;
    let relay_delay_preserves_unknown_pending = !before_release.reachable_now
        && before_release.records.is_empty()
        && before_release.unknown_pending == source.len() as u64
        && !before_release.grants_authority()
        && !before_release.is_globally_complete();
    let delayed_digest = accepted_digest(delayed.deliver_at(10, &plan).map_err(debug)?.records, 4)?;

    let status = VNextStatusSnapshot::local_runtime(2, 0, &VNextFeatureConfig::default(), true);
    let usable_during_seed_outage = status.usability == LocalUsability::UsableOffline
        && status.coverage.status == CoverageViewStatus::LocalOnly
        && !status.reachability.claims_network_completion;

    let adapter = LegacyAdapter::negotiate(
        true,
        LegacyAdapterOffer::safe_v1(),
        LegacyAdapterOffer::safe_v1(),
        [41; 32],
    )
    .map_err(debug)?;
    let query = adapter
        .normalize_query_scope(
            br#"{"scope":5}"#,
            LEGACY_SCOPE_GLOBAL,
            SelectorCid::from_bytes([42; 32]),
            vec![EventCid::from_bytes([43; 32])],
            EventCid::from_bytes([44; 32]),
            0,
            0,
        )
        .map_err(debug)?;
    let encoding = adapter
        .normalize_encoding_status(
            br#"{"encoding_status":3}"#,
            LEGACY_ENCODING_FULL,
            ObjectReference::new(22, [45; 32]),
            ObjectReference::new(2, [46; 32]),
            vec![EventCid::from_bytes([47; 32])],
            EventCid::from_bytes([48; 32]),
            vec![ConceptCcid::from_bytes([49; 16])],
        )
        .map_err(debug)?;
    let outbound = adapter
        .serialize_reachable_partial_response(0, LEGACY_ENCODING_FULL)
        .map_err(debug)?;
    let outbound = std::str::from_utf8(&outbound).map_err(|error| error.to_string())?;
    let legacy_safe = !adapter.grants_vnext_authority()
        && !query.claims_global_coverage()
        && !query.coverage.is_globally_complete()
        && !encoding.claim.establishes_corroborated_fidelity()
        && !encoding.claim.selects_or_deletes_alternate_encodings()
        && !outbound.contains("GLOBAL")
        && !outbound.contains("FULL");

    let unsafe_offer = LegacyAdapterOffer {
        max_outbound_encoding_status: LEGACY_ENCODING_FULL,
        ..LegacyAdapterOffer::safe_v1()
    };
    let unsafe_legacy_offer_rejected =
        LegacyAdapter::negotiate(true, LegacyAdapterOffer::safe_v1(), unsafe_offer, [50; 32])
            .is_err();

    let native = [
        (ConformanceCarrier::InMemory, memory_digest),
        (ConformanceCarrier::FileBundle, file_digest),
        (ConformanceCarrier::QuicFrame, quic_digest),
        (ConformanceCarrier::DelayedStoreCarryForward, delayed_digest),
    ];
    let cross_carrier_semantics_equal = native.iter().all(|(_, digest)| *digest == memory_digest);
    let mut cells = native
        .into_iter()
        .map(|(carrier, semantic_result_digest)| ConformanceCell {
            peer_pair: ConformancePeerPair::VNextToVNext,
            carrier,
            semantic_result_digest: Some(semantic_result_digest),
            grants_authority: false,
            claims_network_completion: false,
            establishes_fidelity: false,
            warning: None,
        })
        .collect::<Vec<_>>();
    cells.push(ConformanceCell {
        peer_pair: ConformancePeerPair::LegacyToVNext,
        carrier: ConformanceCarrier::NegotiatedLegacyAdapter,
        semantic_result_digest: Some(query.provenance.original_wire_ref.cid),
        grants_authority: !legacy_safe,
        claims_network_completion: !legacy_safe,
        establishes_fidelity: !legacy_safe,
        warning: Some("LEGACY_INPUT_DOWNGRADED_TO_SCOPED_ADVISORY_EVIDENCE".into()),
    });

    Ok(MixedVersionConformanceReport {
        cells,
        cross_carrier_semantics_equal,
        usable_during_seed_outage,
        relay_delay_preserves_unknown_pending,
        unsafe_legacy_offer_rejected,
    })
}

fn context() -> ReconciliationContext {
    ReconciliationContext {
        authenticated_transcript: [31; 32],
        selector: SelectorCid::from_bytes([32; 32]),
        namespace: NamespaceCommitment::from_bytes([33; 32]),
        disclosure: DisclosureClass::Public,
        summary_method: ReconciliationSummaryMethod::RadixForest256V1,
        budget: ReconciliationBudget {
            max_summary_nodes: 32,
            max_diff_ranges: 32,
            max_manifest_entries: 32,
            max_payload_bytes: 4_096,
        },
        resume_mode: ReconciliationResumeMode::BoundTokenV1,
    }
}

fn canonical_records() -> Result<Vec<CarrierRecord>, String> {
    let first = BoundPayloadFrame::new(
        &context(),
        ReconcileManifestKind::Object,
        b"mixed-alpha".to_vec(),
    )
    .map_err(debug)?;
    let second = BoundPayloadFrame::new(
        &context(),
        ReconcileManifestKind::Object,
        b"mixed-beta".to_vec(),
    )
    .map_err(debug)?;
    let mut entries = [&first, &second]
        .into_iter()
        .map(|payload| ReconcileManifestEntry {
            kind: payload.kind,
            cid: payload.cid,
            canonical_length: payload.canonical_bytes.len() as u64,
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
    let manifest =
        bind_reconciliation_message(context(), 1, ReconciliationBody::Manifest { entries })
            .map_err(debug)?;
    Ok(vec![
        CarrierRecord::reconciliation_message(
            &encode_reconciliation_message(&manifest).map_err(debug)?,
        )
        .map_err(debug)?,
        CarrierRecord::BoundPayload(first),
        CarrierRecord::BoundPayload(second),
    ])
}

fn enqueue_all<C: DeterministicCarrier>(
    carrier: &mut C,
    records: &[CarrierRecord],
) -> Result<(), String> {
    for record in records {
        carrier.enqueue(record.clone()).map_err(debug)?;
    }
    Ok(())
}

fn accepted_digest(records: Vec<CarrierRecord>, path: u8) -> Result<[u8; 32], String> {
    let mut inbox = MultiBridgeInbox::new(context());
    let path = BridgePathId::from_bytes([path; 32]);
    for record in records {
        match record {
            CarrierRecord::ReconciliationMessage(bytes) => {
                inbox.ingest_message(path, &bytes).map_err(debug)?;
            }
            CarrierRecord::BoundPayload(payload) => {
                inbox.ingest_payload(path, payload).map_err(debug)?;
            }
        }
    }
    let mut session = JournaledReconciliationSession::open(
        InMemoryReconciliationJournalBackend::default(),
        context(),
        ReconciliationJournalConfig {
            max_retries_per_record: 4,
            max_inflight_bytes: 4_096,
        },
        MatrixSink::default(),
    )
    .map_err(debug)?;
    let report = inbox.deliver(&mut session);
    if report.errors != 0 || session.state() != ReceiverState::ManifestBatchComplete {
        return Err("MIXED_MATRIX_RECONCILIATION".into());
    }
    let accepted = session.accepted_cids();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:mixed-conformance-result/1\0");
    for cid in accepted {
        hasher.update(&cid);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_matrix_converges_without_old_peer_authority_or_seed_dependency() {
        let directory = tempfile::tempdir().unwrap();
        let report =
            run_mixed_version_conformance_matrix(&directory.path().join("matrix.obp")).unwrap();
        assert_eq!(report.cells.len(), 5);
        assert!(report.cross_carrier_semantics_equal);
        assert!(report.usable_during_seed_outage);
        assert!(report.relay_delay_preserves_unknown_pending);
        assert!(report.unsafe_legacy_offer_rejected);
        assert!(!report.has_authority_amplification());
        assert!(!report.has_network_completion_amplification());
        assert!(report.cells.iter().all(|cell| !cell.establishes_fidelity));
    }
}
