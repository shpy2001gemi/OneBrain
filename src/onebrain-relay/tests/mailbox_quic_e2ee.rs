//! End-to-end QUIC outer-relay E2EE mailbox test (`obp/mailbox/1`, `P2` §4, `FW-05`).

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use ku_net::vnext_mailbox_client::{
    acknowledge_mailbox_over_relay, deposit_mailbox_over_relay, pull_mailbox_over_relay,
    unseal_pulled_mailbox_page, InboundMailboxDisposition, MailboxInboundSink,
    MailboxOutboundTracker,
};
use ku_net::vnext_reachability_crypto::{
    possession_challenge_digest, possession_proof_signing_bytes, InMemoryReachabilityReplayStore,
    PublicEndpointResolver, ReachabilityAdmission, ReachabilityAdmissionPreparer,
    ReachabilityLockFreePreparation, ReachabilityRecordAdmission, RelayAdmissionError,
};
use ku_net::vnext_relay_tunnel::{
    connect_authenticated_outer, AlternateRelayProbeObservation, ValidatedRelayDialRoute,
    ValidatedRelayDialSet,
};
use onebrain_protocol::{
    derive_mailbox_slot_id, encode_reachability_object, mailbox_x25519_public_key,
    reachability_signing_bytes, seal_mailbox_envelope, sign_mailbox_delivery_receipt,
    sign_mailbox_deposit_token, sign_mailbox_pull_request, HostAddressV1, MailboxDeliveryReceiptV1,
    MailboxDepositRequestV1, MailboxDepositToken, MailboxPullRequestV1, ProtocolVersionV1,
    ReachabilityObjectV1, ReachabilitySignatureRoleV1, RelayDescriptorV1, RelayEndpointV1,
    RelayPossessionProofV1, RelayTransportV1,
};
use onebrain_relay::{
    principal_node_id, relay_identity_certificate, DurableRelayState, RelayProductionService,
    UdpRelayListener,
};

struct TestResolver;

impl PublicEndpointResolver for TestResolver {
    fn resolve(
        &self,
        host: &HostAddressV1,
        _deadline: Instant,
    ) -> Result<Vec<IpAddr>, RelayAdmissionError> {
        match host {
            HostAddressV1::Ipv4(value) => Ok(vec![IpAddr::V4(Ipv4Addr::from(*value))]),
            _ => Err(RelayAdmissionError::DnsResolutionFailed),
        }
    }
}

#[derive(Default)]
struct TestQuarantineInbox {
    received: Vec<Vec<u8>>,
}

impl MailboxInboundSink for TestQuarantineInbox {
    fn ingest_unsealed_mailbox_payload(
        &mut self,
        _slot_id: [u8; 32],
        _envelope_id: [u8; 32],
        plaintext: &[u8],
    ) -> Result<InboundMailboxDisposition, String> {
        self.received.push(plaintext.to_vec());
        Ok(InboundMailboxDisposition::QuarantinedOrValidated)
    }
}

#[tokio::test]
async fn quic_relay_stages_64kib_e2ee_mailbox_and_deletes_immediately_on_delivery_receipt() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let relay_key = SigningKey::from_bytes(&[0x41; 32]);
    let relay_public = *relay_key.verifying_key().as_bytes();
    let sender_node_key = SigningKey::from_bytes(&[0x42; 32]);
    let recipient_node_key = SigningKey::from_bytes(&[0x43; 32]);
    let recipient_inbox_signer = SigningKey::from_bytes(&[0x44; 32]);
    let recipient_x_sk = [0x45; 32];
    let recipient_x_pk = mailbox_x25519_public_key(&recipient_x_sk);

    let mut descriptor = RelayDescriptorV1 {
        format: 1,
        relay_node_id: principal_node_id(&relay_public),
        relay_public_key: relay_public,
        endpoints: vec![RelayEndpointV1 {
            transport: RelayTransportV1::QuicUdp,
            host: HostAddressV1::Ipv4([1, 1, 1, 1]),
            port: 41000,
        }],
        supported_transports: vec![RelayTransportV1::QuicUdp],
        protocol_versions: vec![ProtocolVersionV1 { major: 1, minor: 0 }],
        capacity_policy_digest: [0x51; 32],
        previous_descriptor_blake3: None,
        sequence: 1,
        issued_at: now,
        expires_at: now + 120,
        relay_signature: [0; 64],
    };
    descriptor.relay_signature = relay_key
        .sign(
            &reachability_signing_bytes(
                &ReachabilityObjectV1::RelayDescriptor(descriptor.clone()),
                ReachabilitySignatureRoleV1::RelayDescriptor,
            )
            .unwrap(),
        )
        .to_bytes();
    let descriptor_bytes =
        encode_reachability_object(&ReachabilityObjectV1::RelayDescriptor(descriptor.clone()))
            .unwrap();
    let preparer = ReachabilityAdmissionPreparer::new(Arc::new(TestResolver), 1).unwrap();
    let prepared = preparer
        .prepare_descriptor(
            &descriptor_bytes,
            now,
            Instant::now() + Duration::from_secs(2),
        )
        .await
        .unwrap();
    let mut admission =
        ReachabilityAdmission::new(Arc::new(InMemoryReachabilityReplayStore::default()));
    let pending = admission
        .register_prepared_descriptor(prepared, [0x52; 32], now)
        .unwrap();
    let challenge = pending.challenges()[0].clone();
    let proof = RelayPossessionProofV1 {
        challenge_digest: possession_challenge_digest(&challenge),
        connection_binding_digest: [0x53; 32],
        signature: relay_key
            .sign(&possession_proof_signing_bytes(&challenge, [0x53; 32]))
            .to_bytes(),
    };
    let validated = admission
        .complete_descriptor_admission(pending, &[proof], now)
        .unwrap();

    let identity = relay_identity_certificate(&relay_key, &descriptor).unwrap();
    let listener = UdpRelayListener::bind("127.0.0.1:0".parse().unwrap(), &identity).unwrap();
    let address = listener.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let durable = Arc::new(DurableRelayState::initialize(&dir.path().join("relay.redb")).unwrap());
    let service = Arc::new(RelayProductionService::new(relay_key.clone(), 8, 4, durable).unwrap());

    let server_task = tokio::spawn({
        let service = Arc::clone(&service);
        async move {
            // Serve 2 outer connections sequentially: sender deposits, then waking recipient pulls & acks!
            let _ = listener.serve_production_once(Arc::clone(&service)).await;
            let _ = listener.serve_production_once(service).await;
        }
    });

    let probe = AlternateRelayProbeObservation::new(
        0,
        address,
        RelayTransportV1::QuicUdp,
        relay_public,
        [0x54; 32],
        now,
        now + 30,
    )
    .unwrap();
    let route = ValidatedRelayDialRoute::alternate_from_verified_probe(&validated, probe).unwrap();
    let routes = ValidatedRelayDialSet::from_admitted_descriptor(route, None).unwrap();

    // 1. Sender deposits a 50 KiB payload (padded to 65536-byte bucket!) while recipient is offline.
    let slot_id = derive_mailbox_slot_id(&recipient_x_pk, &[0x61; 32]);
    let mut token = MailboxDepositToken {
        slot_id,
        relay_node_id: descriptor.relay_node_id,
        recipient_signing_pk: [0; 32],
        max_envelopes: 8,
        max_total_bytes: 512 * 1024,
        issued_at_unix_s: now,
        expires_at_unix_s: now + 3_600,
        recipient_signature: [0; 64],
    };
    sign_mailbox_deposit_token(&mut token, &recipient_inbox_signer).unwrap();

    let secret_payload = vec![0xCDu8; 50_000];
    let envelope = seal_mailbox_envelope(
        &recipient_x_pk,
        [0x62; 32],
        slot_id,
        &secret_payload,
        now,
        now + 3_600,
    )
    .unwrap();
    assert_eq!(envelope.ciphertext.len(), 65_536);

    let mut tracker = MailboxOutboundTracker::new(16).unwrap();
    tracker
        .register_outbound(
            &envelope,
            *recipient_inbox_signer.verifying_key().as_bytes(),
        )
        .unwrap();

    let sender_outer = connect_authenticated_outer(
        &routes,
        &sender_node_key,
        now,
        Instant::now() + Duration::from_secs(5),
    )
    .await
    .unwrap();
    let stored_receipt = deposit_mailbox_over_relay(
        &sender_outer,
        [0x71; 16],
        &MailboxDepositRequestV1 {
            token,
            envelope: envelope.clone(),
        },
        &relay_public,
    )
    .await
    .unwrap();
    assert!(stored_receipt.carrier_staged_only);
    let state = tracker
        .record_relay_stored_receipt(&stored_receipt, &relay_public)
        .unwrap();
    assert!(
        !state.is_peer_delivered(),
        "FW-05: staged != peer delivered"
    );
    sender_outer.close();

    // 2. Sleeping recipient wakes up, connects outbound, pulls, unseals into quarantine, and acks!
    let recipient_outer = connect_authenticated_outer(
        &routes,
        &recipient_node_key,
        now,
        Instant::now() + Duration::from_secs(5),
    )
    .await
    .unwrap();

    let mut pull_req = MailboxPullRequestV1 {
        slot_id,
        recipient_signing_pk: [0; 32],
        after_envelope_id: None,
        max_envelopes: 4,
        max_bytes: 200_000,
        issued_at_unix_s: now,
        request_nonce: [0x81; 32],
        recipient_signature: [0; 64],
    };
    sign_mailbox_pull_request(&mut pull_req, &recipient_inbox_signer).unwrap();
    let page = pull_mailbox_over_relay(&recipient_outer, [0x72; 16], &pull_req)
        .await
        .unwrap();
    assert_eq!(page.envelopes.len(), 1);

    let mut inbox = TestQuarantineInbox::default();
    let ack_ids = unseal_pulled_mailbox_page(&recipient_x_sk, &page, &mut inbox).unwrap();
    assert_eq!(inbox.received, vec![secret_payload]);

    let mut delivery_receipt = MailboxDeliveryReceiptV1 {
        slot_id,
        envelope_ids: ack_ids,
        recipient_signing_pk: [0; 32],
        delivered_at_unix_s: now,
        recipient_signature: [0; 64],
    };
    sign_mailbox_delivery_receipt(&mut delivery_receipt, &recipient_inbox_signer).unwrap();
    let ack = acknowledge_mailbox_over_relay(&recipient_outer, [0x73; 16], &delivery_receipt)
        .await
        .unwrap();
    assert_eq!(ack.deleted_envelopes, 1);
    assert_eq!(ack.reclaimed_bytes, 65_536);

    // Subsequent pull observes immediate deletion (0 envelopes remaining).
    let mut pull_after = pull_req;
    pull_after.request_nonce = [0x82; 32];
    sign_mailbox_pull_request(&mut pull_after, &recipient_inbox_signer).unwrap();
    let empty_page = pull_mailbox_over_relay(&recipient_outer, [0x74; 16], &pull_after)
        .await
        .unwrap();
    assert!(empty_page.envelopes.is_empty());

    let delivered = tracker
        .record_recipient_delivery_receipt(&delivery_receipt)
        .unwrap();
    assert!(delivered[0].is_peer_delivered());

    recipient_outer.close();
    let _ = server_task.await;
}
