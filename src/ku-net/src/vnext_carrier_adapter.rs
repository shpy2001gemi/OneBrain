//! Carrier-neutral OBP-RP adapters and shared conformance path.

use crate::vnext_carrier::{CarrierError, CarrierRecord, DeliveryInjection, DeterministicCarrier};

const MAX_QUIC_FRAME_BYTES: usize = 4_194_304;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelayedDelivery {
    pub records: Vec<CarrierRecord>,
    pub reachable_now: bool,
    pub unknown_pending: u64,
}

impl DelayedDelivery {
    pub const fn grants_authority(&self) -> bool {
        false
    }

    pub const fn is_globally_complete(&self) -> bool {
        false
    }
}

pub struct DelayedCarrier<C> {
    inner: C,
    release_epoch: u64,
}

impl<C: DeterministicCarrier> DelayedCarrier<C> {
    pub const fn new(inner: C, release_epoch: u64) -> Self {
        Self {
            inner,
            release_epoch,
        }
    }

    pub fn deliver_at(
        &self,
        epoch: u64,
        injection: &DeliveryInjection,
    ) -> Result<DelayedDelivery, CarrierError> {
        if epoch < self.release_epoch {
            Ok(DelayedDelivery {
                records: Vec::new(),
                reachable_now: false,
                unknown_pending: self.inner.record_count() as u64,
            })
        } else {
            Ok(DelayedDelivery {
                records: self.inner.deliver(injection)?,
                reachable_now: true,
                unknown_pending: 0,
            })
        }
    }

    pub const fn inner(&self) -> &C {
        &self.inner
    }
}

/// Length-delimited canonical frame used by a QUIC bidirectional or
/// unidirectional stream. Session authentication and socket lifecycle remain
/// owned by the QUIC transport; this adapter cannot enter a reducer.
pub struct QuicRecordAdapter;

impl QuicRecordAdapter {
    pub fn encode(record: &CarrierRecord) -> Result<Vec<u8>, CarrierAdapterError> {
        let payload = record.canonical_bytes()?;
        if payload.is_empty() || payload.len() > MAX_QUIC_FRAME_BYTES {
            return Err(CarrierAdapterError::FrameLimit);
        }
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        Ok(frame)
    }

    pub fn decode(frame: &[u8]) -> Result<CarrierRecord, CarrierAdapterError> {
        if frame.len() < 4 {
            return Err(CarrierAdapterError::Truncated);
        }
        let length = u32::from_be_bytes(frame[..4].try_into().expect("four bytes")) as usize;
        if length == 0 || length > MAX_QUIC_FRAME_BYTES {
            return Err(CarrierAdapterError::FrameLimit);
        }
        if frame.len() != length + 4 {
            return Err(CarrierAdapterError::LengthMismatch);
        }
        CarrierRecord::decode(&frame[4..]).map_err(Into::into)
    }

    pub const fn grants_authority() -> bool {
        false
    }
}

#[derive(Debug)]
pub enum CarrierAdapterError {
    Carrier(CarrierError),
    Truncated,
    LengthMismatch,
    FrameLimit,
}

impl From<CarrierError> for CarrierAdapterError {
    fn from(error: CarrierError) -> Self {
        Self::Carrier(error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
    use onebrain_protocol::{
        bind_reconciliation_message, encode_reconciliation_message, ReconcileManifestEntry,
        ReconcileManifestKind, ReconciliationBody, ReconciliationBudget, ReconciliationContext,
        ReconciliationResumeMode, ReconciliationSummaryMethod,
    };

    use super::*;
    use crate::vnext_bridge_merge::{BridgePathId, MultiBridgeInbox};
    use crate::vnext_carrier::{FileBundleCarrier, InMemoryCarrier};
    use crate::vnext_reconciliation::{
        BoundPayloadFrame, PayloadSinkOutcome, ReceiverState, ValidateThenAcceptSink,
    };
    use crate::vnext_reconciliation_journal::{
        InMemoryReconciliationJournalBackend, JournaledReconciliationSession,
        ReconciliationJournalConfig,
    };

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<BTreeMap<(u64, [u8; 32]), Vec<u8>>>>);

    impl ValidateThenAcceptSink for Sink {
        fn validate_then_accept(
            &mut self,
            kind: ReconcileManifestKind,
            cid: [u8; 32],
            bytes: &[u8],
        ) -> Result<PayloadSinkOutcome, String> {
            let mut records = self.0.lock().unwrap();
            let key = (kind as u64, cid);
            if records.contains_key(&key) {
                Ok(PayloadSinkOutcome::AlreadyPresent)
            } else {
                records.insert(key, bytes.to_vec());
                Ok(PayloadSinkOutcome::ValidatedStored)
            }
        }
    }

    fn context() -> ReconciliationContext {
        ReconciliationContext {
            authenticated_transcript: [1; 32],
            selector: SelectorCid::from_bytes([2; 32]),
            namespace: NamespaceCommitment::from_bytes([3; 32]),
            disclosure: DisclosureClass::Public,
            summary_method: ReconciliationSummaryMethod::RadixForest256V1,
            budget: ReconciliationBudget {
                max_summary_nodes: 32,
                max_diff_ranges: 32,
                max_manifest_entries: 32,
                max_payload_bytes: 4096,
            },
            resume_mode: ReconciliationResumeMode::BoundTokenV1,
        }
    }

    fn records() -> Vec<CarrierRecord> {
        let first = BoundPayloadFrame::new(
            &context(),
            ReconcileManifestKind::Object,
            b"carrier-alpha".to_vec(),
        )
        .unwrap();
        let second = BoundPayloadFrame::new(
            &context(),
            ReconcileManifestKind::Object,
            b"carrier-beta".to_vec(),
        )
        .unwrap();
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
                .unwrap();
        vec![
            CarrierRecord::reconciliation_message(
                &encode_reconciliation_message(&manifest).unwrap(),
            )
            .unwrap(),
            CarrierRecord::BoundPayload(first),
            CarrierRecord::BoundPayload(second),
        ]
    }

    fn accepted(records: Vec<CarrierRecord>, path: u8) -> Vec<[u8; 32]> {
        let mut inbox = MultiBridgeInbox::new(context());
        let path = BridgePathId::from_bytes([path; 32]);
        for record in records {
            match record {
                CarrierRecord::ReconciliationMessage(bytes) => {
                    inbox.ingest_message(path, &bytes).unwrap();
                }
                CarrierRecord::BoundPayload(payload) => {
                    inbox.ingest_payload(path, payload).unwrap();
                }
            }
        }
        let sink = Sink::default();
        let mut session = JournaledReconciliationSession::open(
            InMemoryReconciliationJournalBackend::default(),
            context(),
            ReconciliationJournalConfig {
                max_retries_per_record: 4,
                max_inflight_bytes: 4096,
            },
            sink,
        )
        .unwrap();
        let report = inbox.deliver(&mut session);
        assert_eq!(report.errors, 0);
        assert_eq!(session.state(), ReceiverState::ManifestBatchComplete);
        session.accepted_cids()
    }

    #[test]
    fn memory_file_delayed_and_quic_frames_have_same_outcome() {
        let source = records();
        let plan = DeliveryInjection::default();

        let mut memory = InMemoryCarrier::default();
        for record in &source {
            memory.enqueue(record.clone()).unwrap();
        }
        let memory_records = memory.deliver(&plan).unwrap();

        let directory = tempfile::tempdir().unwrap();
        let mut file = FileBundleCarrier::open(directory.path().join("bundle.obp")).unwrap();
        for record in &source {
            file.enqueue(record.clone()).unwrap();
        }
        let file_records = file.deliver(&plan).unwrap();

        let mut delayed_memory = InMemoryCarrier::default();
        for record in &source {
            delayed_memory.enqueue(record.clone()).unwrap();
        }
        let delayed = DelayedCarrier::new(delayed_memory, 10);
        let before = delayed.deliver_at(9, &plan).unwrap();
        assert!(!before.reachable_now);
        assert_eq!(before.unknown_pending, 3);
        assert!(!before.grants_authority());
        assert!(!before.is_globally_complete());
        let delayed_records = delayed.deliver_at(10, &plan).unwrap().records;

        let quic_records = source
            .iter()
            .map(|record| {
                QuicRecordAdapter::decode(&QuicRecordAdapter::encode(record).unwrap()).unwrap()
            })
            .collect();

        let expected = accepted(memory_records, 1);
        assert_eq!(accepted(file_records, 2), expected);
        assert_eq!(accepted(delayed_records, 3), expected);
        assert_eq!(accepted(quic_records, 4), expected);
        assert!(!QuicRecordAdapter::grants_authority());
    }

    #[test]
    fn quic_frame_limits_and_exact_length_are_enforced() {
        let record = records().remove(0);
        let mut frame = QuicRecordAdapter::encode(&record).unwrap();
        frame.pop();
        assert!(matches!(
            QuicRecordAdapter::decode(&frame),
            Err(CarrierAdapterError::LengthMismatch)
        ));
        assert!(matches!(
            QuicRecordAdapter::decode(&[0, 0, 0]),
            Err(CarrierAdapterError::Truncated)
        ));
    }
}
