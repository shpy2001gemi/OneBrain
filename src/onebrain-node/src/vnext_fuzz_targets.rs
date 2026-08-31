//! Shared parser entry points for deterministic corpus smoke and cargo-fuzz.
//!
//! These functions intentionally return no semantic result. A fuzz failure is
//! a panic, abort, sanitizer finding, unbounded allocation, or timeout. When a
//! canonical decoder accepts bytes, the target also checks exact re-encoding.

use ed25519_dalek::SigningKey;
use ku_core::foundation::{
    decode_actor_delegation, decode_actor_revocation, decode_canonical, decode_feed_inception,
    decode_knowledge_event, decode_knowledge_object, DeviceId, EventCid, FeedInception,
    KnownObjectKind, NamespaceCommitment, ObjectReference, ResourceProfile, SelectorCid,
    SignedFeedInception, UseEvidencePayload, ValidatedFeedInception,
    DERIVATION_EVIDENCE_EVENT_TYPE, DERIVATION_EVIDENCE_KIND, USE_EVIDENCE_EVENT_TYPE,
    USE_EVIDENCE_KIND,
};
use ku_net::vnext_carrier_adapter::QuicRecordAdapter;
use onebrain_protocol::{
    decode_reachability_object, decode_reconciliation_message, decode_session_message,
    encode_reachability_object, encode_reconciliation_message, encode_session_message, legacy,
    DirectCandidateKindV1, DirectCandidateV1, HostAddressV1, LegacyAdapter, LegacyAdapterOffer,
    ReachabilityEndpointV1, ReachabilityObjectV1, RoutePlanV1, RouteResourceBudgetV1,
    LEGACY_ENCODING_FULL, LEGACY_SCOPE_GLOBAL,
};

pub const FUZZ_TARGETS: [&str; 7] = [
    "canonical_codec",
    "session_reconciliation_codec",
    "carrier_frame",
    "journal_token_snapshot",
    "domain_records",
    "legacy_adapter",
    "reachability_codec",
];

pub const MAX_FUZZ_INPUT_BYTES: usize = 4_096;

pub fn run_target(target: &str, data: &[u8]) {
    assert!(
        data.len() <= MAX_FUZZ_INPUT_BYTES,
        "fuzz input exceeds the frozen M5-04 cap"
    );
    match target {
        "canonical_codec" => canonical_codec(data),
        "session_reconciliation_codec" => session_reconciliation_codec(data),
        "carrier_frame" => carrier_frame(data),
        "journal_token_snapshot" => journal_token_snapshot(data),
        "domain_records" => domain_records(data),
        "legacy_adapter" => legacy_adapter(data),
        "reachability_codec" => reachability_codec(data),
        other => panic!("unknown frozen fuzz target: {other}"),
    }
}

fn canonical_codec(data: &[u8]) {
    for profile in [
        ResourceProfile::ControlV1,
        ResourceProfile::ObjectV1,
        ResourceProfile::ManifestV1,
    ] {
        if let Ok(value) = decode_canonical(data, profile) {
            let encoded = ku_core::foundation::encode_canonical(&value, profile)
                .expect("accepted canonical value re-encodes");
            assert_eq!(encoded, data);
        }
    }
}

fn session_reconciliation_codec(data: &[u8]) {
    if let Ok(message) = decode_session_message(data) {
        assert_eq!(
            encode_session_message(&message).expect("accepted session message re-encodes"),
            data
        );
    }
    if let Ok(message) = decode_reconciliation_message(data) {
        assert_eq!(
            encode_reconciliation_message(&message)
                .expect("accepted reconciliation message re-encodes"),
            data
        );
    }
}

fn carrier_frame(data: &[u8]) {
    if let Ok(record) = QuicRecordAdapter::decode(data) {
        assert_eq!(
            QuicRecordAdapter::encode(&record).expect("accepted carrier frame re-encodes"),
            data
        );
    }
    let _ = QuicRecordAdapter::decode_payload(data);
}

fn journal_token_snapshot(data: &[u8]) {
    ku_net::vnext_reconciliation_journal::fuzz_decode_journal_token_and_snapshot(data);
}

fn domain_records(data: &[u8]) {
    let author = fixed_author();
    match data.first().copied().unwrap_or(0) % 6 {
        0 => {
            let _ = decode_knowledge_object(data, ResourceProfile::ObjectV1, &[], &[]);
        }
        1 => {
            let _ = decode_knowledge_event(
                data,
                &author,
                &[USE_EVIDENCE_EVENT_TYPE, DERIVATION_EVIDENCE_EVENT_TYPE],
            );
        }
        2 => {
            let _ = decode_feed_inception(data);
        }
        3 => {
            let _ = decode_actor_delegation(data, &author);
        }
        4 => {
            let _ = decode_actor_revocation(data, &author);
        }
        5 => {
            if let Ok(object) = decode_knowledge_object(
                data,
                ResourceProfile::ObjectV1,
                &[
                    KnownObjectKind::new(USE_EVIDENCE_KIND, 1),
                    KnownObjectKind::new(DERIVATION_EVIDENCE_KIND, 1),
                ],
                &[],
            ) {
                let _ = UseEvidencePayload::from_validated_object(&object);
            }
        }
        _ => unreachable!("modulo six"),
    }
}

fn legacy_adapter(data: &[u8]) {
    let _ = legacy::parse_peer_message(data);
    let _ = legacy::parse_seed_message(data);
    let adapter = LegacyAdapter::negotiate(
        true,
        LegacyAdapterOffer::safe_v1(),
        LegacyAdapterOffer::safe_v1(),
        [0x91; 32],
    )
    .expect("safe frozen legacy adapter negotiation");
    let selector = SelectorCid::from_bytes([0x92; 32]);
    let frontier = vec![EventCid::from_bytes([0x93; 32])];
    let migration = EventCid::from_bytes([0x94; 32]);
    let token = data.first().copied().unwrap_or(0);
    let _ = adapter.normalize_query_scope(
        data,
        if token & 1 == 0 {
            LEGACY_SCOPE_GLOBAL
        } else {
            token
        },
        selector,
        frontier.clone(),
        migration,
        data.len() as u64,
        data.len() as u64,
    );
    let _ = adapter.normalize_encoding_status(
        data,
        if token & 2 == 0 {
            LEGACY_ENCODING_FULL
        } else {
            token
        },
        ObjectReference::new(1, [0x95; 32]),
        ObjectReference::new(2, [0x96; 32]),
        frontier,
        migration,
        Vec::new(),
    );
    let response = adapter
        .serialize_reachable_partial_response(data.len() as u64, token)
        .expect("bounded legacy response serialization");
    assert!(!response.windows(8).any(|window| window == b"\"GLOBAL\""));
    assert!(!adapter.grants_vnext_authority());
}

fn reachability_codec(data: &[u8]) {
    if let Ok(value) = decode_reachability_object(data) {
        let encoded = encode_reachability_object(&value)
            .expect("accepted reachability object re-encodes within frozen bounds");
        assert_eq!(encoded, data);
    }
}

/// Frozen invalid classes from the outbound-first profile. These are small on
/// purpose: the decoder must reject them before any profile-sized allocation.
pub const REACHABILITY_INVALID_CORPUS: [(&str, &[u8]); 19] = [
    ("unknown-schema", &[0xff, 0x00]),
    ("unknown-field", br#"{\"unknown\":true}"#),
    ("noncanonical-order", &[0xa2, 0x02, 0x00, 0x01, 0x00]),
    ("oversize-endpoints", br#"{\"endpoints\":9}"#),
    ("expired", br#"{\"expires_at\":0}"#),
    ("replayed-sequence", br#"{\"sequence\":0}"#),
    ("wrong-signature-domain", br#"{\"domain\":\"wrong\"}"#),
    ("wrong-node-id", br#"{\"node_id\":\"00\"}"#),
    ("private-ipv4", br#"{\"host\":\"192.168.1.1\"}"#),
    ("link-local-ipv4", br#"{\"host\":\"169.254.1.1\"}"#),
    ("link-local-ipv6", br#"{\"host\":\"fe80::1\"}"#),
    ("zero-budget", br#"{\"max_probe_bytes\":0}"#),
    ("duplicate-reservation", br#"{\"reservation\":[1,1]}"#),
    ("route-substitution", br#"{\"expected_peer\":\"other\"}"#),
    ("transcript-substitution", br#"{\"binding\":\"other\"}"#),
    ("unknown-transport", br#"{\"transport\":99}"#),
    ("missing-proof", br#"{\"proof\":null}"#),
    ("dns-rebinding", br#"{\"resolved\":\"10.0.0.1\"}"#),
    ("trailing-bytes", &[0xa0, 0x00]),
];

pub fn valid_reachability_corpus_seed() -> Vec<u8> {
    encode_reachability_object(&ReachabilityObjectV1::RoutePlan(RoutePlanV1 {
        expected_peer: ku_core::foundation::NodeId::from_bytes([0x61; 32]),
        direct_candidates: vec![DirectCandidateV1 {
            endpoint: ReachabilityEndpointV1 {
                host: HostAddressV1::Ipv4([8, 8, 4, 4]),
                port: 41_000,
            },
            kind: DirectCandidateKindV1::ServerReflexive,
            priority: 10,
            network_epoch: 1,
            expires_at: 600,
        }],
        relay_candidates: Vec::new(),
        deadline: 500,
        attempt_budget: 4,
        resource_budget: RouteResourceBudgetV1 {
            max_concurrent_checks: 1,
            max_signature_checks: 8,
            max_probe_bytes: 4_096,
        },
        privacy_policy_digest: [0x62; 32],
    }))
    .expect("frozen valid reachability corpus seed encodes")
}

fn fixed_author() -> ValidatedFeedInception {
    let key = SigningKey::from_bytes(&[0x81; 32]);
    let inception = FeedInception::new(
        *key.verifying_key().as_bytes(),
        NamespaceCommitment::derive(b"dr-m5-fuzz-author", [0x82; 32])
            .expect("non-empty fuzz namespace"),
        0,
        DeviceId::from_bytes([0x83; 32]),
    );
    let signed: SignedFeedInception = inception.sign(&key).expect("fixed fuzz feed signs");
    decode_feed_inception(&signed.encode().expect("fixed fuzz feed encodes"))
        .expect("fixed fuzz feed validates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_rejects_or_accepts_bounded_adversarial_bytes_without_panicking() {
        let corpus = [
            Vec::new(),
            vec![0xff; MAX_FUZZ_INPUT_BYTES],
            br#"{"scope":5,"encoding":3}"#.to_vec(),
        ];
        for target in FUZZ_TARGETS {
            for input in &corpus {
                run_target(target, input);
            }
        }
        for (_, input) in REACHABILITY_INVALID_CORPUS {
            run_target("reachability_codec", input);
        }
        run_target("reachability_codec", &valid_reachability_corpus_seed());
    }
}
