//! Local lifecycle regression evidence; no remote-host qualification claims.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use onebrain_protocol::{
    decode_reachability_object, encode_reachability_object, reachability_signing_bytes,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1,
};
use onebrain_relay::{
    activate_descriptor, export_candidate_descriptor, generate_identity, initialize_state,
    DurableRelayState, DurableStateError, DurableStateKind, RelayActivationProbeSetV1,
    RelayActivationProbeV1, RelayConfigV1, RelayConfiguredEndpointV1, RuntimeError,
};

struct Fixture {
    dir: tempfile::TempDir,
    config: RelayConfigV1,
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let config = RelayConfigV1 {
            format: 1,
            data_root: dir.path().join("data"),
            signer_locator: dir.path().join("relay.key"),
            udp_bind: Some("127.0.0.1:0".parse().unwrap()),
            tcp443_bind: None,
            advertised_endpoints: vec![RelayConfiguredEndpointV1 {
                transport: "quic-udp".into(),
                host: "1.1.1.1".into(),
                port: 41000,
            }],
            capacity_policy_digest: [7; 32],
            max_reservations: 8,
            max_reservations_per_target: 3,
            max_rendezvous_records: 64,
            descriptor_sequence: 1,
            descriptor_issued_at: now - 10,
            descriptor_expires_at: now + 600,
            log_destination: dir.path().join("relay.log"),
        };
        generate_identity(&config.signer_locator).unwrap();
        let fixture = Self { dir, config, path };
        fixture.save();
        initialize_state(&fixture.path).unwrap();
        fixture
    }

    fn save(&self) {
        std::fs::write(&self.path, serde_json::to_vec(&self.config).unwrap()).unwrap();
    }
    fn output(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn state(&self) -> DurableRelayState {
        DurableRelayState::open(&self.config.data_root.join("relay-state.redb")).unwrap()
    }
    fn bytes(&self) -> Vec<u8> {
        self.state()
            .get(DurableStateKind::DescriptorFloor, b"candidate-v1")
            .unwrap()
            .unwrap()
    }
    fn advance(&mut self) {
        self.config.descriptor_sequence += 1;
        self.config.descriptor_issued_at += 1;
        self.config.descriptor_expires_at += 1;
        self.save();
    }
    fn export(&self, name: &str) -> Result<[u8; 32], RuntimeError> {
        export_candidate_descriptor(&self.path, &self.output(name))
    }
    fn probes(&self, digest: [u8; 32], name: &str) -> PathBuf {
        let path = self.output(name);
        let probes = RelayActivationProbeSetV1 {
            format: 1,
            descriptor_blake3: hex(&digest),
            probes: (0..2)
                .map(|index| RelayActivationProbeV1 {
                    source_host_id: format!("fixture-{index}"),
                    endpoint_index: 0,
                    transport: "quic-udp".into(),
                    success: true,
                    transcript_blake3: hex(&[index + 1; 32]),
                })
                .collect(),
        };
        std::fs::write(&path, serde_json::to_vec(&probes).unwrap()).unwrap();
        path
    }
}

fn descriptor(bytes: &[u8]) -> RelayDescriptorV1 {
    let ReachabilityObjectV1::RelayDescriptor(value) = decode_reachability_object(bytes).unwrap()
    else {
        panic!("not a descriptor")
    };
    value
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn serve_failure(path: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_onebrain-relay"))
        .args(["serve", "--config"])
        .arg(path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn renewal_preserves_all_other_state_and_fences_activation_across_reopen() {
    let mut fixture = Fixture::new();
    let first = fixture.export("first.cbor").unwrap();
    let original = fixture.bytes();
    let old_probes = fixture.probes(first, "old-probes.json");
    activate_descriptor(&fixture.path, &old_probes).unwrap();
    let kinds = [
        DurableStateKind::ControlFloor,
        DurableStateKind::ConsumedNonce,
        DurableStateKind::Reservation,
        DurableStateKind::Revocation,
        DurableStateKind::RendezvousRecord,
        DurableStateKind::DescriptorFloor,
    ];
    for kind in kinds {
        fixture
            .state()
            .create_new(kind, b"retained", b"exact-existing-state")
            .unwrap();
    }
    fixture.advance();
    let second = fixture.export("second.cbor").unwrap();
    let next = descriptor(&fixture.bytes());
    assert_eq!(next.sequence, 2);
    assert_eq!(next.previous_descriptor_blake3, Some(first));
    assert_eq!(
        next.relay_public_key,
        descriptor(&original).relay_public_key
    );
    assert_eq!(
        std::fs::read(fixture.output("first.cbor")).unwrap(),
        original
    );
    assert_eq!(
        fixture
            .state()
            .get(DurableStateKind::ControlFloor, b"descriptor-activation-v1")
            .unwrap(),
        None
    );
    assert!(serve_failure(&fixture.path).contains("NotActivated"));
    assert_eq!(
        activate_descriptor(&fixture.path, &old_probes),
        Err(RuntimeError::ProbeSet)
    );
    for kind in kinds {
        assert_eq!(
            fixture.state().get(kind, b"retained").unwrap(),
            Some(b"exact-existing-state".to_vec())
        );
        assert_eq!(
            fixture.state().create_new(kind, b"retained", b"reset"),
            Err(DurableStateError::Replay)
        );
    }
    let fresh = fixture.probes(second, "new-probes.json");
    let probe_hash = activate_descriptor(&fixture.path, &fresh).unwrap();
    let binding = fixture
        .state()
        .get(DurableStateKind::ControlFloor, b"descriptor-activation-v1")
        .unwrap()
        .unwrap();
    assert_eq!(&binding[..32], &second);
    assert_eq!(&binding[32..], &probe_hash);
}

#[test]
fn publication_failure_recovers_exact_committed_bytes_without_a_fork() {
    let mut fixture = Fixture::new();
    fixture.export("first.cbor").unwrap();
    fixture.advance();
    assert_eq!(
        fixture.export("missing-parent/second.cbor"),
        Err(RuntimeError::Io)
    );
    let committed = fixture.bytes();
    assert_eq!(descriptor(&committed).sequence, 2);
    let digest = fixture.export("recovered.cbor").unwrap();
    assert_eq!(digest, *blake3::hash(&committed).as_bytes());
    assert_eq!(
        std::fs::read(fixture.output("recovered.cbor")).unwrap(),
        committed
    );
    assert_eq!(fixture.bytes(), committed);
    fixture.config.descriptor_expires_at += 1;
    fixture.save();
    assert_eq!(fixture.export("fork.cbor"), Err(RuntimeError::Descriptor));
    assert!(!fixture.output("fork.cbor").exists());
}

#[test]
fn automatic_time_republication_is_byte_identical_and_no_output_is_overwritten() {
    let mut fixture = Fixture::new();
    fixture.config.descriptor_issued_at = 0;
    fixture.config.descriptor_expires_at = 0;
    fixture.save();
    fixture.export("first.cbor").unwrap();
    let original = fixture.bytes();
    fixture.export("copy.cbor").unwrap();
    assert_eq!(
        std::fs::read(fixture.output("copy.cbor")).unwrap(),
        original
    );
    fixture.config.descriptor_sequence = 2;
    fixture.save();
    assert_eq!(
        fixture.export("first.cbor"),
        Err(RuntimeError::OutputExists)
    );
    assert_eq!(fixture.bytes(), original);
}

#[test]
fn gaps_rollback_changed_key_and_changed_signed_config_leave_no_output() {
    let mut fixture = Fixture::new();
    fixture.export("first.cbor").unwrap();
    fixture.advance();
    fixture.export("second.cbor").unwrap();
    let current = fixture.bytes();
    let saved = fixture.config.clone();
    for sequence in [1, 4, u64::MAX] {
        fixture.config.descriptor_sequence = sequence;
        fixture.save();
        assert_eq!(
            fixture.export("rejected.cbor"),
            Err(RuntimeError::Descriptor)
        );
        assert!(!fixture.output("rejected.cbor").exists());
    }
    fixture.config = saved;
    fixture.advance();
    fixture.config.advertised_endpoints[0].host = "8.8.8.8".into();
    fixture.save();
    assert_eq!(
        fixture.export("changed.cbor"),
        Err(RuntimeError::Descriptor)
    );
    fixture.config.advertised_endpoints[0].host = "1.1.1.1".into();
    fixture.config.signer_locator = fixture.output("other.key");
    generate_identity(&fixture.config.signer_locator).unwrap();
    fixture.save();
    assert_eq!(
        fixture.export("changed-key.cbor"),
        Err(RuntimeError::Identity)
    );
    assert_eq!(fixture.bytes(), current);
}

#[test]
fn expired_legacy_descriptor_can_renew_without_resetting_state() {
    let mut fixture = Fixture::new();
    fixture.export("first.cbor").unwrap();
    // Separate old-format database, as produced by the historical export mode.
    let mut old = descriptor(&fixture.bytes());
    old.issued_at = 100;
    old.expires_at = 700;
    let key: [u8; 32] = std::fs::read(&fixture.config.signer_locator)
        .unwrap()
        .try_into()
        .unwrap();
    old.relay_signature = SigningKey::from_bytes(&key)
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(old.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    let old_bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(old)).unwrap();
    fixture.config.data_root = fixture.output("legacy-state");
    fixture.save();
    initialize_state(&fixture.path).unwrap();
    fixture
        .state()
        .create_new(
            DurableStateKind::DescriptorFloor,
            b"candidate-v1",
            &old_bytes,
        )
        .unwrap();
    fixture
        .state()
        .create_new(
            DurableStateKind::ControlFloor,
            b"descriptor-activation-v1",
            &[9; 32],
        )
        .unwrap();
    let fresh_config = fixture.config.clone();
    fixture.config.descriptor_issued_at = 100;
    fixture.config.descriptor_expires_at = 700;
    fixture.save();
    assert!(serve_failure(&fixture.path).contains("Descriptor"));
    let expired_probes =
        fixture.probes(*blake3::hash(&old_bytes).as_bytes(), "expired-probes.json");
    assert_eq!(
        activate_descriptor(&fixture.path, &expired_probes),
        Err(RuntimeError::Descriptor)
    );
    fixture.config = fresh_config;
    fixture.advance();
    fixture.export("renewed.cbor").unwrap();
    assert_eq!(
        descriptor(&fixture.bytes()).previous_descriptor_blake3,
        Some(*blake3::hash(&old_bytes).as_bytes())
    );
    assert!(serve_failure(&fixture.path).contains("NotActivated"));
}

#[test]
fn cli_reopen_and_concurrent_exports_cannot_fork_a_sequence() {
    let mut fixture = Fixture::new();
    fixture.export("first.cbor").unwrap();
    fixture.advance();
    let mut children = Vec::new();
    for name in ["parallel-a.cbor", "parallel-b.cbor"] {
        children.push((
            fixture.output(name),
            Command::new(env!("CARGO_BIN_EXE_onebrain-relay"))
                .args(["export-candidate-descriptor", "--config"])
                .arg(&fixture.path)
                .arg("--output")
                .arg(fixture.output(name))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        ));
    }
    let mut success = 0;
    for (path, child) in children {
        let result = child.wait_with_output().unwrap();
        if result.status.success() {
            success += 1;
            assert_eq!(std::fs::read(path).unwrap(), fixture.bytes());
        } else {
            assert!(!path.exists());
        }
    }
    assert!(success >= 1);
    assert_eq!(descriptor(&fixture.bytes()).sequence, 2);
}

#[test]
fn legacy_activation_and_changed_serve_config_require_fresh_admission() {
    let mut fixture = Fixture::new();
    let digest = fixture.export("first.cbor").unwrap();
    fixture
        .state()
        .create_new(
            DurableStateKind::ControlFloor,
            b"descriptor-activation-v1",
            &[9; 32],
        )
        .unwrap();
    assert!(serve_failure(&fixture.path).contains("NotActivated"));
    let probes = fixture.probes(digest, "fresh.json");
    activate_descriptor(&fixture.path, &probes).unwrap();
    fixture.config.advertised_endpoints[0].host = "8.8.8.8".into();
    fixture.save();
    assert!(serve_failure(&fixture.path).contains("Descriptor"));
    assert_eq!(
        activate_descriptor(&fixture.path, &probes),
        Err(RuntimeError::Descriptor)
    );
}

#[test]
fn activation_requires_two_distinct_observations_for_every_endpoint() {
    let mut fixture = Fixture::new();
    fixture
        .config
        .advertised_endpoints
        .push(RelayConfiguredEndpointV1 {
            transport: "tls-tcp-443".into(),
            host: "1.1.1.1".into(),
            port: 443,
        });
    fixture.save();
    let digest = fixture.export("first.cbor").unwrap();
    let path = fixture.probes(digest, "probes.json");
    let mut probes: RelayActivationProbeSetV1 =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    probes.probes[1].endpoint_index = 1;
    probes.probes[1].transport = "tls-tcp-443".into();
    std::fs::write(&path, serde_json::to_vec(&probes).unwrap()).unwrap();
    assert_eq!(
        activate_descriptor(&fixture.path, &path),
        Err(RuntimeError::ProbeSet)
    );
    let mut other_a = probes.probes[0].clone();
    other_a.endpoint_index = 1;
    other_a.transport = "tls-tcp-443".into();
    other_a.transcript_blake3 = hex(&[3; 32]);
    let mut other_b = probes.probes[1].clone();
    other_b.endpoint_index = 0;
    other_b.transport = "quic-udp".into();
    other_b.transcript_blake3 = hex(&[4; 32]);
    probes.probes.extend([other_a, other_b]);
    let valid = probes.clone();
    probes.probes[3].transcript_blake3 = probes.probes[0].transcript_blake3.clone();
    std::fs::write(&path, serde_json::to_vec(&probes).unwrap()).unwrap();
    assert_eq!(
        activate_descriptor(&fixture.path, &path),
        Err(RuntimeError::ProbeSet)
    );
    probes = valid.clone();
    probes.probes[3].source_host_id = probes.probes[0].source_host_id.clone();
    std::fs::write(&path, serde_json::to_vec(&probes).unwrap()).unwrap();
    assert_eq!(
        activate_descriptor(&fixture.path, &path),
        Err(RuntimeError::ProbeSet)
    );
    std::fs::write(&path, serde_json::to_vec(&valid).unwrap()).unwrap();
    activate_descriptor(&fixture.path, &path).unwrap();
}
