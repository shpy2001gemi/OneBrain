//! M0 two-peer loopback oracle for vNext carrier work.
//!
//! This harness deliberately uses a small loopback socket instead of claiming
//! to be the production QUIC/authenticated-session runtime. Its job is to make
//! future M1-M4 acceptance tests cross a real listener boundary while retaining
//! exact canonical carrier bytes for privacy and replay assertions.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use ku_core::foundation::{DisclosureClass, NamespaceCommitment, SelectorCid};
use ku_net::vnext_carrier::{
    CarrierRecord, DeliveryInjection, DeliveryOrder, DeterministicCarrier, InMemoryCarrier,
};
use ku_net::vnext_carrier_adapter::QuicRecordAdapter;
use ku_net::vnext_reconciliation::{
    BoundPayloadFrame, PayloadIngestOutcome, PayloadSinkOutcome, ReceiverState,
    ValidateThenAcceptSink,
};
use ku_net::vnext_reconciliation_journal::persistent::RedbReconciliationJournalBackend;
use ku_net::vnext_reconciliation_journal::{
    JournaledPayloadOutcome, JournaledReconciliationSession, ReconciliationJournalConfig,
};
use onebrain_protocol::{
    bind_reconciliation_message, decode_reconciliation_message, encode_reconciliation_message,
    ReconcileManifestEntry, ReconcileManifestKind, ReconciliationBody, ReconciliationBudget,
    ReconciliationContext, ReconciliationResumeMode, ReconciliationSummaryMethod,
};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const MAX_TEST_FRAME_BYTES: usize = 4_194_304;
const FRAME_ACCEPTED: u8 = 1;
const FRAME_REJECTED: u8 = 0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CapturedTraffic {
    raw_frames: Vec<Vec<u8>>,
    records: Vec<CarrierRecord>,
    errors: Vec<String>,
}

#[derive(Clone, Default)]
struct SharedValidatedSink {
    records: Arc<Mutex<BTreeMap<(u64, [u8; 32]), Vec<u8>>>>,
    insertions: Arc<Mutex<u64>>,
}

impl ValidateThenAcceptSink for SharedValidatedSink {
    fn validate_then_accept(
        &mut self,
        kind: ReconcileManifestKind,
        cid: [u8; 32],
        bytes: &[u8],
    ) -> Result<PayloadSinkOutcome, String> {
        let mut records = self.records.lock().expect("validated sink lock");
        let key = (kind as u64, cid);
        if records.contains_key(&key) {
            return Ok(PayloadSinkOutcome::AlreadyPresent);
        }
        records.insert(key, bytes.to_vec());
        *self.insertions.lock().expect("insertion lock") += 1;
        Ok(PayloadSinkOutcome::ValidatedStored)
    }
}

struct LoopbackPeer {
    identity: [u8; 32],
    data_dir: TempDir,
    addr: SocketAddr,
    capture: Arc<Mutex<CapturedTraffic>>,
    listener_task: Option<JoinHandle<()>>,
}

impl LoopbackPeer {
    async fn start(identity: [u8; 32]) -> Result<Self, String> {
        let data_dir = tempfile::tempdir().map_err(|error| error.to_string())?;
        let capture = Arc::new(Mutex::new(CapturedTraffic::default()));
        let (addr, listener_task) = spawn_listener(Arc::clone(&capture)).await?;
        Ok(Self {
            identity,
            data_dir,
            addr,
            capture,
            listener_task: Some(listener_task),
        })
    }

    async fn send_to(&self, target: SocketAddr, record: &CarrierRecord) -> Result<(), String> {
        let frame = QuicRecordAdapter::encode(record).map_err(|error| format!("{error:?}"))?;
        let mut stream = TcpStream::connect(target)
            .await
            .map_err(|error| error.to_string())?;
        stream
            .write_all(&frame)
            .await
            .map_err(|error| error.to_string())?;
        stream.flush().await.map_err(|error| error.to_string())?;
        let acknowledgement = stream.read_u8().await.map_err(|error| error.to_string())?;
        if acknowledgement == FRAME_ACCEPTED {
            Ok(())
        } else {
            Err("receiver rejected canonical carrier frame".to_string())
        }
    }

    async fn restart_listener(&mut self) -> Result<(), String> {
        self.stop_listener().await;
        let (addr, listener_task) = spawn_listener(Arc::clone(&self.capture)).await?;
        self.addr = addr;
        self.listener_task = Some(listener_task);
        Ok(())
    }

    fn snapshot(&self) -> CapturedTraffic {
        self.capture.lock().expect("capture lock").clone()
    }

    async fn stop_listener(&mut self) {
        if let Some(task) = self.listener_task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

struct TwoPeerHarness {
    peer_a: LoopbackPeer,
    peer_b: LoopbackPeer,
    partitioned: bool,
}

impl TwoPeerHarness {
    async fn start() -> Result<Self, String> {
        Ok(Self {
            peer_a: LoopbackPeer::start([0xA1; 32]).await?,
            peer_b: LoopbackPeer::start([0xB2; 32]).await?,
            partitioned: false,
        })
    }

    fn partition(&mut self) {
        self.partitioned = true;
    }

    fn reunite(&mut self) {
        self.partitioned = false;
    }

    async fn send_a_to_b(&self, record: &CarrierRecord) -> Result<(), String> {
        if self.partitioned {
            return Err("test peers are partitioned".to_string());
        }
        self.peer_a.send_to(self.peer_b.addr, record).await
    }

    async fn send_b_to_a(&self, record: &CarrierRecord) -> Result<(), String> {
        if self.partitioned {
            return Err("test peers are partitioned".to_string());
        }
        self.peer_b.send_to(self.peer_a.addr, record).await
    }

    async fn send_a_to_b_with_injection(
        &self,
        records: &[CarrierRecord],
        injection: &DeliveryInjection,
    ) -> Result<Vec<CarrierRecord>, String> {
        if self.partitioned {
            return Err("test peers are partitioned".to_string());
        }
        let mut carrier = InMemoryCarrier::default();
        for record in records {
            carrier
                .enqueue(record.clone())
                .map_err(|error| format!("{error:?}"))?;
        }
        let delivered = carrier
            .deliver(injection)
            .map_err(|error| format!("{error:?}"))?;
        for record in &delivered {
            self.peer_a.send_to(self.peer_b.addr, record).await?;
        }
        Ok(delivered)
    }

    async fn stop(mut self) {
        self.peer_a.stop_listener().await;
        self.peer_b.stop_listener().await;
    }
}

async fn spawn_listener(
    capture: Arc<Mutex<CapturedTraffic>>,
) -> Result<(SocketAddr, JoinHandle<()>), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| error.to_string())?;
    let addr = listener.local_addr().map_err(|error| error.to_string())?;
    let listener_task = tokio::spawn(async move {
        while let Ok((mut stream, _remote)) = listener.accept().await {
            if let Err(error) = receive_one(&mut stream, &capture).await {
                capture.lock().expect("capture lock").errors.push(error);
                let _ = stream.write_u8(FRAME_REJECTED).await;
            }
        }
    });
    Ok((addr, listener_task))
}

async fn receive_one(
    stream: &mut TcpStream,
    capture: &Arc<Mutex<CapturedTraffic>>,
) -> Result<(), String> {
    let mut length_bytes = [0u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .await
        .map_err(|error| error.to_string())?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_TEST_FRAME_BYTES {
        return Err("test carrier frame exceeds the bounded length".to_string());
    }

    let mut payload = vec![0u8; length];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|error| error.to_string())?;
    let mut frame = Vec::with_capacity(length + 4);
    frame.extend_from_slice(&length_bytes);
    frame.extend_from_slice(&payload);
    let record = QuicRecordAdapter::decode(&frame).map_err(|error| format!("{error:?}"))?;

    {
        let mut captured = capture.lock().expect("capture lock");
        captured.raw_frames.push(frame);
        captured.records.push(record);
    }
    stream
        .write_u8(FRAME_ACCEPTED)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn context() -> ReconciliationContext {
    ReconciliationContext {
        authenticated_transcript: [3; 32],
        selector: SelectorCid::from_bytes([4; 32]),
        namespace: NamespaceCommitment::from_bytes([5; 32]),
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

fn record(marker: u8) -> CarrierRecord {
    CarrierRecord::BoundPayload(
        BoundPayloadFrame::new(&context(), ReconcileManifestKind::Object, vec![marker; 64])
            .expect("bounded payload frame"),
    )
}

fn reconciliation_records(marker: u8) -> (Vec<CarrierRecord>, BoundPayloadFrame) {
    let frame = BoundPayloadFrame::new(&context(), ReconcileManifestKind::Object, vec![marker; 64])
        .expect("bounded payload frame");
    let manifest = bind_reconciliation_message(
        context(),
        1,
        ReconciliationBody::Manifest {
            entries: vec![ReconcileManifestEntry {
                kind: frame.kind,
                cid: frame.cid,
                canonical_length: frame.canonical_bytes.len() as u64,
            }],
        },
    )
    .expect("manifest");
    (
        vec![
            CarrierRecord::reconciliation_message(
                &encode_reconciliation_message(&manifest).expect("manifest bytes"),
            )
            .expect("manifest carrier record"),
            CarrierRecord::BoundPayload(frame.clone()),
        ],
        frame,
    )
}

fn journal_config() -> ReconciliationJournalConfig {
    ReconciliationJournalConfig {
        max_retries_per_record: 2,
        max_inflight_bytes: 4096,
    }
}

#[tokio::test]
async fn two_loopback_peers_cross_real_listener_capture_partition_and_restart_boundaries() {
    let mut harness = TwoPeerHarness::start().await.expect("two-peer harness");
    assert_ne!(harness.peer_a.identity, harness.peer_b.identity);
    assert_ne!(
        harness.peer_a.data_dir.path(),
        harness.peer_b.data_dir.path()
    );
    assert_ne!(harness.peer_a.addr, harness.peer_b.addr);

    let first = record(11);
    harness.send_a_to_b(&first).await.expect("A to B");
    let first_snapshot = harness.peer_b.snapshot();
    assert_eq!(first_snapshot.records, vec![first.clone()]);
    assert_eq!(first_snapshot.errors, Vec::<String>::new());
    assert_eq!(
        first_snapshot.raw_frames,
        vec![QuicRecordAdapter::encode(&first).unwrap()]
    );
    let private_marker = b"private-standing-need-id-must-not-cross-wire";
    assert!(first_snapshot.raw_frames.iter().all(|frame| !frame
        .windows(private_marker.len())
        .any(|window| window == private_marker)));

    harness.partition();
    assert!(harness.send_b_to_a(&record(22)).await.is_err());
    assert!(harness.peer_a.snapshot().records.is_empty());
    harness.reunite();

    let data_dir_before_restart = harness.peer_b.data_dir.path().to_path_buf();
    let old_addr = harness.peer_b.addr;
    harness
        .peer_b
        .restart_listener()
        .await
        .expect("restart B listener");
    assert_eq!(harness.peer_b.data_dir.path(), data_dir_before_restart);
    assert_ne!(harness.peer_b.addr, old_addr);

    let second = record(33);
    harness
        .send_a_to_b(&second)
        .await
        .expect("A to restarted B");
    let final_snapshot = harness.peer_b.snapshot();
    assert_eq!(final_snapshot.records, vec![first, second]);
    assert!(final_snapshot.errors.is_empty());

    // The loopback oracle captures exact bytes but never grants authority.
    assert!(!QuicRecordAdapter::grants_authority());
    harness.stop().await;
}

#[tokio::test]
async fn injected_drop_duplicate_and_reverse_order_cross_the_listener_exactly() {
    let harness = TwoPeerHarness::start().await.expect("two-peer harness");
    let records = vec![record(41), record(42), record(43)];
    let injection = DeliveryInjection {
        order: DeliveryOrder::ReverseCanonical,
        copies_per_record: 2,
        dropped_ordinals: BTreeSet::from([1]),
    };

    let delivered = harness
        .send_a_to_b_with_injection(&records, &injection)
        .await
        .expect("injected delivery");
    assert_eq!(delivered.len(), 4);

    let snapshot = harness.peer_b.snapshot();
    assert_eq!(snapshot.records, delivered);
    assert_eq!(snapshot.raw_frames.len(), 4);
    assert!(snapshot.errors.is_empty());
    for (frame, record) in snapshot.raw_frames.iter().zip(&snapshot.records) {
        assert_eq!(frame, &QuicRecordAdapter::encode(record).unwrap());
    }

    harness.stop().await;
}

#[tokio::test]
async fn redb_journal_reopens_after_loopback_delivery_without_duplicate_acceptance() {
    let mut harness = TwoPeerHarness::start().await.expect("two-peer harness");
    let (records, frame) = reconciliation_records(77);
    for record in &records {
        harness
            .send_a_to_b(record)
            .await
            .expect("canonical loopback delivery");
    }
    let captured = harness.peer_b.snapshot();
    assert_eq!(captured.records, records);

    let journal_path = harness.peer_b.data_dir.path().join("reconciliation.redb");
    let sink = SharedValidatedSink::default();
    let token_key = [0x91; 32];
    let resume_token;
    {
        let backend = RedbReconciliationJournalBackend::open(&journal_path).expect("redb journal");
        let mut session = JournaledReconciliationSession::open(
            backend,
            context(),
            journal_config(),
            sink.clone(),
        )
        .expect("journaled session");
        for record in &captured.records {
            match record {
                CarrierRecord::ReconciliationMessage(bytes) => {
                    let message = decode_reconciliation_message(bytes).expect("manifest decode");
                    session
                        .ingest_manifest(&message)
                        .expect("manifest persisted");
                }
                CarrierRecord::BoundPayload(payload) => assert_eq!(
                    session.ingest_payload(payload).expect("payload persisted"),
                    JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::ValidatedStored)
                ),
            }
        }
        assert_eq!(session.state(), ReceiverState::ManifestBatchComplete);
        assert_eq!(session.accepted_cids(), vec![frame.cid]);
        resume_token = session
            .issue_resume_token(2, token_key)
            .expect("resume token");
    }

    let data_dir_before_restart = harness.peer_b.data_dir.path().to_path_buf();
    harness
        .peer_b
        .restart_listener()
        .await
        .expect("restart receiver listener");
    assert_eq!(harness.peer_b.data_dir.path(), data_dir_before_restart);

    let backend = RedbReconciliationJournalBackend::open(&journal_path).expect("reopen journal");
    let mut reopened =
        JournaledReconciliationSession::open(backend, context(), journal_config(), sink.clone())
            .expect("reopen journaled session");
    assert_eq!(reopened.state(), ReceiverState::ManifestBatchComplete);
    assert_eq!(reopened.accepted_cids(), vec![frame.cid]);
    reopened
        .validate_resume_token(&resume_token, token_key)
        .expect("resume token survives restart");

    let replay = CarrierRecord::BoundPayload(frame.clone());
    harness
        .send_a_to_b(&replay)
        .await
        .expect("replay crosses restarted listener");
    assert_eq!(
        reopened
            .ingest_payload(&frame)
            .expect("replay journal outcome"),
        JournaledPayloadOutcome::Delivered(PayloadIngestOutcome::AlreadyPresent)
    );
    assert_eq!(*sink.insertions.lock().expect("insertion lock"), 1);

    harness.stop().await;
}
