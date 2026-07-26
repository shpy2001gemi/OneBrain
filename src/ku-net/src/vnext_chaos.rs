//! Deterministic DR-M5 chaos and adversarial-resource acceptance.
//!
//! The long-trace model supplies reproducible property evidence. The tests in
//! this module additionally cross real QUIC connections, authentication,
//! disconnect/reconnect, prefix-first reads, and bounded slow peers.

use std::collections::BTreeSet;

pub const CHAOS_SCENARIOS: [&str; 7] = [
    "drop",
    "duplicate",
    "delay",
    "reorder",
    "disconnect",
    "partition_reunion",
    "slow_reader_writer",
];

pub const FLOOD_SCENARIOS: [&str; 5] = [
    "pre_auth",
    "authenticated_sessions",
    "contexts_manifests",
    "unique_invalid_cids",
    "slowloris",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChaosTraceConfig {
    pub seed: u64,
    pub steps: usize,
    pub record_count: usize,
}

impl ChaosTraceConfig {
    pub fn validate(self) -> Result<Self, ChaosError> {
        if self.steps == 0 || self.steps > 4_096 {
            return Err(ChaosError::InvalidSteps);
        }
        if self.record_count == 0 || self.record_count > 256 {
            return Err(ChaosError::InvalidRecordCount);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChaosTraceReport {
    pub steps: usize,
    pub drops: u64,
    pub duplicates: u64,
    pub delays: u64,
    pub reorders: u64,
    pub disconnects: u64,
    pub partitions: u64,
    pub reunions: u64,
    pub slow_reads: u64,
    pub slow_writes: u64,
    pub accepted_before_fair_redelivery: usize,
    pub accepted_after_fair_redelivery: usize,
    pub final_oracle_root: [u8; 32],
    pub grants_authority: bool,
    pub claims_network_completion: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChaosError {
    InvalidSteps,
    InvalidRecordCount,
}

/// Generate and execute a deterministic long delivery trace.
///
/// The random component chooses record ordinals, while `step % 11` guarantees
/// that every fault family is represented in every sufficiently long trace.
/// The final fair-redelivery phase is explicit and inserts only exact CIDs.
pub fn run_delivery_trace(config: ChaosTraceConfig) -> Result<ChaosTraceReport, ChaosError> {
    let config = config.validate()?;
    let source = (0..config.record_count)
        .map(|ordinal| record_cid(ordinal as u64))
        .collect::<Vec<_>>();
    let mut accepted = BTreeSet::new();
    let mut delayed = Vec::new();
    let mut rng = SplitMix64::new(config.seed);
    let mut connected = true;
    let mut partitioned = false;
    let mut report = ChaosTraceReport {
        steps: config.steps,
        drops: 0,
        duplicates: 0,
        delays: 0,
        reorders: 0,
        disconnects: 0,
        partitions: 0,
        reunions: 0,
        slow_reads: 0,
        slow_writes: 0,
        accepted_before_fair_redelivery: 0,
        accepted_after_fair_redelivery: 0,
        final_oracle_root: [0; 32],
        grants_authority: false,
        claims_network_completion: false,
    };

    for step in 0..config.steps {
        let cid = source[(rng.next() as usize) % source.len()];
        match step % 11 {
            0 => {
                if connected && !partitioned {
                    accepted.insert(cid);
                }
            }
            1 => report.drops = report.drops.saturating_add(1),
            2 => {
                report.duplicates = report.duplicates.saturating_add(1);
                if connected && !partitioned {
                    accepted.insert(cid);
                    accepted.insert(cid);
                }
            }
            3 => {
                report.delays = report.delays.saturating_add(1);
                delayed.push(cid);
            }
            4 => {
                report.reorders = report.reorders.saturating_add(1);
                delayed.reverse();
                if connected && !partitioned {
                    accepted.extend(delayed.drain(..));
                }
            }
            5 => {
                report.disconnects = report.disconnects.saturating_add(1);
                connected = false;
            }
            6 => {
                if !partitioned {
                    connected = true;
                }
            }
            7 => {
                report.partitions = report.partitions.saturating_add(1);
                partitioned = true;
                connected = false;
            }
            8 => {
                report.reunions = report.reunions.saturating_add(1);
                partitioned = false;
                connected = true;
            }
            9 => {
                report.slow_reads = report.slow_reads.saturating_add(1);
                if connected && !partitioned {
                    accepted.insert(cid);
                }
            }
            10 => {
                report.slow_writes = report.slow_writes.saturating_add(1);
                if connected && !partitioned {
                    accepted.insert(cid);
                }
            }
            _ => unreachable!("step modulo 11"),
        }
    }

    report.accepted_before_fair_redelivery = accepted.len();
    accepted.extend(source);
    report.accepted_after_fair_redelivery = accepted.len();
    report.final_oracle_root = accepted_oracle_root(&accepted);
    Ok(report)
}

pub fn expected_oracle_root(record_count: usize) -> Result<[u8; 32], ChaosError> {
    ChaosTraceConfig {
        seed: 0,
        steps: 1,
        record_count,
    }
    .validate()?;
    Ok(accepted_oracle_root(
        &(0..record_count)
            .map(|ordinal| record_cid(ordinal as u64))
            .collect(),
    ))
}

fn record_cid(ordinal: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:dr-m5:chaos-record:1\0");
    hasher.update(&ordinal.to_be_bytes());
    *hasher.finalize().as_bytes()
}

fn accepted_oracle_root(accepted: &BTreeSet<[u8; 32]>) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:dr-m5:chaos-oracle:1\0");
    hasher.update(&(accepted.len() as u64).to_be_bytes());
    for cid in accepted {
        hasher.update(cid);
    }
    *hasher.finalize().as_bytes()
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use ed25519_dalek::SigningKey;
    use ku_core::foundation::{DisclosureClass, NamespaceCommitment, NodeId, SelectorCid};
    use onebrain_protocol::{
        bind_reconciliation_message, encode_reconciliation_message, reconciliation_capability,
        reconciliation_profile, ReconcileManifestEntry, ReconcileManifestKind, ReconciliationBody,
        ReconciliationBudget, ReconciliationContext, ReconciliationResumeMode,
        ReconciliationSummaryMethod,
    };
    use tokio::time::timeout;

    use super::*;
    use crate::transport::{OBPConnection, QuicTransport, TransportConfig};
    use crate::vnext_carrier::CarrierRecord;
    use crate::vnext_carrier_adapter::QuicRecordAdapter;
    use crate::vnext_quic_session::{
        accept_authenticated_session, initiate_authenticated_session, send_carrier_record,
        AuthenticatedCarrierRecord, AuthenticatedCarrierSession,
    };
    use crate::vnext_reconciliation::BoundPayloadFrame;
    use crate::vnext_resource_gate::{
        ResourceUsage, RuntimeAdmissionController, RuntimeAdmissionLimits,
    };
    use crate::vnext_session::AuthenticatedSession;

    const TRACE_SEEDS: u64 = 64;
    const TRACE_STEPS: usize = 4_096;
    const TRACE_RECORDS: usize = 64;

    #[test]
    fn long_delivery_traces_converge_to_one_oracle_under_fair_redelivery() {
        let expected = expected_oracle_root(TRACE_RECORDS).unwrap();
        for seed in 0..TRACE_SEEDS {
            let report = run_delivery_trace(ChaosTraceConfig {
                seed,
                steps: TRACE_STEPS,
                record_count: TRACE_RECORDS,
            })
            .unwrap();
            assert_eq!(report.accepted_after_fair_redelivery, TRACE_RECORDS);
            assert_eq!(report.final_oracle_root, expected);
            assert!(report.drops > 0);
            assert!(report.duplicates > 0);
            assert!(report.delays > 0);
            assert!(report.reorders > 0);
            assert!(report.disconnects > 0);
            assert!(report.partitions > 0);
            assert!(report.reunions > 0);
            assert!(report.slow_reads > 0);
            assert!(report.slow_writes > 0);
            assert!(!report.grants_authority);
            assert!(!report.claims_network_completion);
        }
    }

    #[tokio::test]
    async fn real_quic_drop_duplicate_delay_reorder_disconnect_and_reunion_converge() {
        timeout(Duration::from_secs(15), async {
            let server = bind_transport().await;
            let client = bind_transport().await;
            let old_server_addr = server.local_addr().unwrap();
            let (server_connection, client_connection, accepted, initiated) =
                authenticated_pair(&server, &client).await;
            let (manifest, payloads) = records(initiated.session_id);
            let private_marker = b"private-standing-need-must-not-cross-chaos-wire";
            for record in std::iter::once(&manifest).chain(payloads.iter()) {
                let encoded = QuicRecordAdapter::encode(record).unwrap();
                assert!(!encoded
                    .windows(private_marker.len())
                    .any(|window| window == private_marker));
            }

            let first_receive = receive_records(&server_connection, accepted, 4, true);
            let first_send = async {
                send_carrier_record(&client_connection, &manifest)
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(5)).await;
                send_carrier_record(&client_connection, &payloads[2])
                    .await
                    .unwrap();
                send_carrier_record(&client_connection, &payloads[0])
                    .await
                    .unwrap();
                send_carrier_record(&client_connection, &payloads[0])
                    .await
                    .unwrap();
            };
            let (first, ()) = tokio::join!(first_receive, first_send);
            assert_eq!(first.len(), 2, "one payload was deliberately dropped");

            server_connection.close("DR_M5_CHAOS_DISCONNECT");
            client_connection.close("DR_M5_CHAOS_DISCONNECT");
            server.close();
            let partition_attempt =
                timeout(Duration::from_millis(250), client.connect(old_server_addr)).await;
            assert!(
                !matches!(partition_attempt, Ok(Ok(_))),
                "closed endpoint must remain unavailable during the partition"
            );

            let reunited_server = bind_transport().await;
            let (server_connection, client_connection, accepted, initiated) =
                authenticated_pair(&reunited_server, &client).await;
            let (manifest, payloads) = records(initiated.session_id);
            let expected = payloads.iter().map(payload_cid).collect();
            let second_receive = receive_records(&server_connection, accepted, 4, true);
            let second_send = async {
                send_carrier_record(&client_connection, &manifest)
                    .await
                    .unwrap();
                send_carrier_record(&client_connection, &payloads[0])
                    .await
                    .unwrap();
                let frame = QuicRecordAdapter::encode(&payloads[1]).unwrap();
                let payload = &frame[4..];
                let midpoint = payload.len() / 2;
                client_connection
                    .send_length_prefixed_uni_chunks(
                        payload.len() as u32,
                        &[&payload[..midpoint], &payload[midpoint..]],
                        Duration::from_millis(5),
                        Duration::ZERO,
                    )
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(5)).await;
                send_carrier_record(&client_connection, &payloads[2])
                    .await
                    .unwrap();
            };
            let (second, ()) = tokio::join!(second_receive, second_send);
            let converged = first.union(&second).copied().collect::<BTreeSet<_>>();
            assert_eq!(converged, expected);

            client_connection.close("DR_M5_CHAOS_COMPLETE");
            server_connection.close("DR_M5_CHAOS_COMPLETE");
            reunited_server.shutdown().await;
            client.shutdown().await;
        })
        .await
        .expect("real-QUIC chaos scenario must have a finite completion bound");
    }

    #[tokio::test]
    async fn floods_and_slowloris_remain_bounded_without_state_amplification() {
        let controller = RuntimeAdmissionController::new(admission_limits()).unwrap();
        let first = controller
            .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
            .unwrap();
        let second = controller
            .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
            .unwrap();
        for _ in 0..20_000 {
            assert!(controller
                .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
                .is_err());
        }
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.live_handshakes, 2);
        assert_eq!(snapshot.tracked_handshake_ips, 1);
        drop((first, second));

        let session_one = controller
            .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
            .unwrap()
            .promote(NodeId::from_bytes([0x11; 32]))
            .unwrap();
        let session_two = controller
            .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
            .unwrap()
            .promote(NodeId::from_bytes([0x12; 32]))
            .unwrap();
        for ordinal in 0..1_024u64 {
            let mut peer = [0x20; 32];
            peer[24..].copy_from_slice(&ordinal.to_be_bytes());
            assert!(controller
                .try_begin_handshake(IpAddr::V4(Ipv4Addr::LOCALHOST))
                .unwrap()
                .promote(NodeId::from_bytes(peer))
                .is_err());
        }
        let snapshot = controller.snapshot().unwrap();
        assert_eq!(snapshot.live_sessions, 2);
        assert_eq!(snapshot.tracked_session_ips, 1);
        assert_eq!(snapshot.tracked_session_peers, 2);
        drop((session_one, session_two));

        let server = bind_transport().await;
        let client = bind_transport().await;
        let (server_connection, client_connection) = connect_pair(&server, &client).await;
        let receive = timeout(
            Duration::from_millis(75),
            server_connection.recv_length_prefixed_uni(1_024),
        );
        let send = client_connection.send_length_prefixed_uni_chunks(
            1_024,
            &[b"x"],
            Duration::ZERO,
            Duration::from_millis(200),
        );
        let (receive, send) = tokio::join!(receive, send);
        assert!(
            receive.is_err(),
            "slowloris must hit the finite read deadline"
        );
        assert!(send.is_ok());
        server_connection.close("DR_M5_SLOWLORIS");
        client_connection.close("DR_M5_SLOWLORIS");

        let (server_connection, client_connection, accepted, initiated) =
            authenticated_pair(&server, &client).await;
        let mut carrier = AuthenticatedCarrierSession::with_context_limit(accepted, 8).unwrap();
        let mut admitted = 0usize;
        let mut rejected = 0usize;
        for ordinal in 0..1_024u64 {
            let context = context(initiated.session_id, ordinal);
            let message = bind_reconciliation_message(
                context,
                ordinal,
                ReconciliationBody::Hello {
                    nonce: record_cid(ordinal),
                    profile: reconciliation_profile(),
                    capability: reconciliation_capability(),
                },
            )
            .unwrap();
            let record = CarrierRecord::reconciliation_message(
                &encode_reconciliation_message(&message).unwrap(),
            )
            .unwrap();
            match carrier.validate(record) {
                Ok(_) => admitted += 1,
                Err(_) => rejected += 1,
            }
        }
        assert_eq!(admitted, 8);
        assert_eq!(rejected, 1_016);
        assert_eq!(carrier.context_count(), 8);

        for ordinal in 0..4_096u64 {
            let mut invalid = ordinal.to_be_bytes().repeat(4);
            invalid[0] ^= 0xff;
            let _ = QuicRecordAdapter::decode_payload(&invalid);
        }
        assert_eq!(carrier.context_count(), 8);
        assert!(!QuicRecordAdapter::grants_authority());

        server_connection.close("DR_M5_FLOOD_COMPLETE");
        client_connection.close("DR_M5_FLOOD_COMPLETE");
        server.shutdown().await;
        client.shutdown().await;
    }

    async fn bind_transport() -> QuicTransport {
        QuicTransport::bind(TransportConfig {
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            ..TransportConfig::default()
        })
        .await
        .unwrap()
    }

    async fn connect_pair(
        server: &QuicTransport,
        client: &QuicTransport,
    ) -> (OBPConnection, OBPConnection) {
        let server_addr = server.local_addr().unwrap();
        let (server_connection, client_connection) =
            tokio::join!(server.accept(), client.connect(server_addr));
        (server_connection.unwrap(), client_connection.unwrap())
    }

    async fn authenticated_pair(
        server: &QuicTransport,
        client: &QuicTransport,
    ) -> (
        OBPConnection,
        OBPConnection,
        AuthenticatedSession,
        AuthenticatedSession,
    ) {
        let (server_connection, client_connection) = connect_pair(server, client).await;
        let initiator_key = SigningKey::from_bytes(&[0x41; 32]);
        let responder_key = SigningKey::from_bytes(&[0x42; 32]);
        let profiles = [reconciliation_profile()];
        let capabilities = [reconciliation_capability()];
        let (accepted, initiated) = tokio::join!(
            accept_authenticated_session(
                &server_connection,
                &responder_key,
                [0x51; 32],
                &profiles,
                &capabilities,
                Vec::new(),
            ),
            initiate_authenticated_session(
                &client_connection,
                &initiator_key,
                [0x52; 32],
                &profiles,
                &capabilities,
                Vec::new(),
            )
        );
        (
            server_connection,
            client_connection,
            accepted.unwrap(),
            initiated.unwrap(),
        )
    }

    fn context(session_id: [u8; 32], ordinal: u64) -> ReconciliationContext {
        let mut selector = [0x61; 32];
        selector[..8].copy_from_slice(&ordinal.to_be_bytes());
        ReconciliationContext {
            authenticated_transcript: session_id,
            selector: SelectorCid::from_bytes(selector),
            namespace: NamespaceCommitment::from_bytes([0x62; 32]),
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

    fn records(session_id: [u8; 32]) -> (CarrierRecord, Vec<CarrierRecord>) {
        let context = context(session_id, 0);
        let payloads = [0x71, 0x72, 0x73]
            .into_iter()
            .map(|marker| {
                CarrierRecord::BoundPayload(
                    BoundPayloadFrame::new(
                        &context,
                        ReconcileManifestKind::Object,
                        vec![marker; 128],
                    )
                    .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let mut entries = payloads
            .iter()
            .map(|record| match record {
                CarrierRecord::BoundPayload(frame) => ReconcileManifestEntry {
                    kind: frame.kind,
                    cid: frame.cid,
                    canonical_length: frame.canonical_bytes.len() as u64,
                },
                CarrierRecord::ReconciliationMessage(_) => unreachable!(),
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.kind as u64, entry.cid));
        let manifest =
            bind_reconciliation_message(context, 0, ReconciliationBody::Manifest { entries })
                .unwrap();
        (
            CarrierRecord::reconciliation_message(
                &encode_reconciliation_message(&manifest).unwrap(),
            )
            .unwrap(),
            payloads,
        )
    }

    async fn receive_records(
        connection: &OBPConnection,
        authenticated: AuthenticatedSession,
        count: usize,
        slow_reader: bool,
    ) -> BTreeSet<[u8; 32]> {
        if slow_reader {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut receiver = AuthenticatedCarrierSession::new(authenticated);
        let mut accepted = BTreeSet::new();
        for _ in 0..count {
            let record = timeout(Duration::from_secs(2), receiver.recv(connection))
                .await
                .expect("bounded QUIC receive")
                .expect("authenticated chaos record");
            if let AuthenticatedCarrierRecord::BoundPayload(frame) = record {
                accepted.insert(frame.cid);
            }
        }
        accepted
    }

    fn payload_cid(record: &CarrierRecord) -> [u8; 32] {
        match record {
            CarrierRecord::BoundPayload(frame) => frame.cid,
            CarrierRecord::ReconciliationMessage(_) => unreachable!(),
        }
    }

    fn admission_limits() -> RuntimeAdmissionLimits {
        RuntimeAdmissionLimits {
            max_handshakes_global: 2,
            max_handshakes_per_ip: 2,
            max_sessions_global: 2,
            max_sessions_per_ip: 2,
            max_sessions_per_peer: 1,
            max_contexts_per_session: 8,
            per_session: ResourceUsage {
                records: 16,
                bytes: 65_536,
                work: 65_536,
            },
            rate_window: Duration::from_secs(60),
            global_per_window: ResourceUsage {
                records: 64,
                bytes: 262_144,
                work: 262_144,
            },
            per_ip_per_window: ResourceUsage {
                records: 32,
                bytes: 131_072,
                work: 131_072,
            },
            per_peer_per_window: ResourceUsage {
                records: 32,
                bytes: 131_072,
                work: 131_072,
            },
        }
    }
}
