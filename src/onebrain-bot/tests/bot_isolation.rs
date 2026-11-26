use std::cell::RefCell;

use onebrain_bot::{
    mint_bot_surface_token, BotAdapterError, BotApiTransport, BotCoverageScope, BotDmzAdapter,
    BotPrincipal, BotPublicHitV1, BotPublicQueryResponseV1, BotSurfaceClaimsV1, ChatPlatform,
    ExternalChatSubmissionInput, QuarantinedBotSubmissionReceiptV1, QuarantinedBotSubmissionV1,
    BOT_QUERY_ENDPOINT, BOT_SUBMISSIONS_ENDPOINT,
};

struct MockDmzTransport {
    calls: RefCell<Vec<(String, String, String)>>,
    next_response: String,
}

impl MockDmzTransport {
    fn new(next_response: serde_json::Value) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            next_response: next_response.to_string(),
        }
    }
}

impl BotApiTransport for MockDmzTransport {
    fn post_json(
        &self,
        endpoint: &str,
        bearer_header: &str,
        body_json: &str,
    ) -> Result<String, String> {
        self.calls.borrow_mut().push((
            endpoint.to_owned(),
            bearer_header.to_owned(),
            body_json.to_owned(),
        ));
        Ok(self.next_response.clone())
    }
}

fn valid_bot_principal(mac_key: &[u8; 32]) -> BotPrincipal {
    let claims = BotSurfaceClaimsV1 {
        version: 1,
        surface_kind: "bot".to_owned(),
        token_id: "tok-bot-01".to_owned(),
        channel_binding: "discord:guild-42".to_owned(),
        permissions: vec![
            "public_validated_read".to_owned(),
            "quarantined_external_submit".to_owned(),
        ],
        issued_at_unix_s: 1_795_700_000,
        expires_at_unix_s: 1_795_800_000,
        revocation_epoch: 1,
    };
    let raw = mint_bot_surface_token(&claims, mac_key).expect("mint token");
    BotPrincipal::from_scoped_token(&raw, "discord:guild-42", 1_795_710_000, Some(mac_key))
        .expect("valid bot principal")
}

#[test]
fn neg_p10_003_rejects_obm1_admin_wrong_surface_and_escalated_permissions() {
    let key = [7u8; 32];
    assert_eq!(
        BotPrincipal::from_scoped_token(
            "obm1.secret-admin-token",
            "discord:guild-42",
            100,
            Some(&key)
        )
        .unwrap_err(),
        BotAdapterError::AdminTokenProhibited
    );

    let mut desktop_claims = BotSurfaceClaimsV1 {
        version: 1,
        surface_kind: "desktop".to_owned(),
        token_id: "tok-1".to_owned(),
        channel_binding: "discord:guild-42".to_owned(),
        permissions: vec!["public_validated_read".to_owned()],
        issued_at_unix_s: 10,
        expires_at_unix_s: 500,
        revocation_epoch: 0,
    };
    let desktop_tok = mint_bot_surface_token(&desktop_claims, &key).unwrap();
    assert_eq!(
        BotPrincipal::from_scoped_token(&desktop_tok, "discord:guild-42", 100, Some(&key))
            .unwrap_err(),
        BotAdapterError::WrongSurfaceKind("desktop".to_owned())
    );

    desktop_claims.surface_kind = "bot".to_owned();
    desktop_claims
        .permissions
        .push("private_vault_read".to_owned());
    let escalated_tok = mint_bot_surface_token(&desktop_claims, &key).unwrap();
    assert_eq!(
        BotPrincipal::from_scoped_token(&escalated_tok, "discord:guild-42", 100, Some(&key))
            .unwrap_err(),
        BotAdapterError::ProhibitedPermission("private_vault_read".to_owned())
    );
}

#[test]
fn neg_p10_003_and_t_bci_04_block_vault_archive_confirm_and_somatic_routes() {
    let key = [9u8; 32];
    let principal = valid_bot_principal(&key);
    let adapter = BotDmzAdapter::new(principal, MockDmzTransport::new(serde_json::json!({})));

    assert_eq!(
        adapter
            .probe_raw_endpoint("POST", "/api/vnext/pomv/public-use/confirm")
            .unwrap_err(),
        BotAdapterError::PublicConfirmProhibited
    );
    assert_eq!(
        adapter
            .probe_raw_endpoint("POST", "/api/vnext/ku/editor?action=share_confirm")
            .unwrap_err(),
        BotAdapterError::PublicConfirmProhibited
    );
    assert_eq!(
        adapter
            .probe_raw_endpoint("POST", "/api/vnext/hud/session/heartbeat")
            .unwrap_err(),
        BotAdapterError::SomaticAccessProhibited
    );
    for forbidden in [
        ("GET", "/api/kus"),
        ("POST", "/api/backup"),
        ("POST", "/api/restore"),
        ("POST", "/api/base/v1/archive/export"),
        ("POST", "/api/vnext/kql/needs/prepare"),
    ] {
        assert!(matches!(
            adapter.probe_raw_endpoint(forbidden.0, forbidden.1),
            Err(BotAdapterError::ForbiddenEndpoint(_))
        ));
    }
}

#[test]
fn public_channel_reply_includes_provenance_and_refuses_non_public_leak() {
    let key = [11u8; 32];
    let principal = valid_bot_principal(&key);
    let public_resp = BotPublicQueryResponseV1 {
        coverage: BotCoverageScope::SampledReachablePeers { peer_count: 3 },
        hits: vec![
            BotPublicHitV1 {
                object_cid: "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
                    .to_owned(),
                semantic_cid: "1122334455667788112233445566778811223344556677881122334455667788"
                    .to_owned(),
                summary: "Pure water boils at 373.15 K under standard atmospheric pressure."
                    .to_owned(),
                disclosure_class: "public".to_owned(),
                model_qualified: false,
                corroborated_fidelity_groups: 0,
            },
            BotPublicHitV1 {
                object_cid: "9988776655443322998877665544332299887766554433229988776655443322"
                    .to_owned(),
                semantic_cid: "ffeeddccbbaa9988ffeeddccbbaa9988ffeeddccbbaa9988ffeeddccbbaa9988"
                    .to_owned(),
                summary: "Standard atmospheric pressure equals 101325 Pa.".to_owned(),
                disclosure_class: "public".to_owned(),
                model_qualified: false,
                corroborated_fidelity_groups: 3,
            },
        ],
    };
    let transport = MockDmzTransport::new(serde_json::to_value(&public_resp).unwrap());
    let adapter = BotDmzAdapter::new(principal.clone(), transport);

    let rendered = adapter
        .handle_channel_query(ChatPlatform::Discord, "chan-1", "boiling point water", 5)
        .expect("public reply");
    assert_eq!(adapter.transport().calls.borrow()[0].0, BOT_QUERY_ENDPOINT);
    assert!(rendered.contains("[Sampled Reachable Peers: 3]"));
    assert!(rendered.contains("[Public · Unassessed]"));
    assert!(rendered.contains("[Public · Fidelity Corroborated (3 groups)]"));
    assert!(rendered.contains("`abcdef0123456789`"));
    assert!(!rendered.contains("FULL"));
    assert!(!rendered.contains("GLOBAL"));

    let leaked_private = BotPublicQueryResponseV1 {
        coverage: BotCoverageScope::LocalStoreOnly,
        hits: vec![BotPublicHitV1 {
            object_cid: "00".repeat(32),
            semantic_cid: "11".repeat(32),
            summary: "Private vault note".to_owned(),
            disclosure_class: "local_only".to_owned(),
            model_qualified: false,
            corroborated_fidelity_groups: 0,
        }],
    };
    let bad_adapter = BotDmzAdapter::new(
        principal,
        MockDmzTransport::new(serde_json::to_value(&leaked_private).unwrap()),
    );
    assert_eq!(
        bad_adapter
            .handle_channel_query(ChatPlatform::Discord, "chan-1", "secret", 5)
            .unwrap_err(),
        BotAdapterError::NonPublicDisclosureRefused("local_only".to_owned())
    );
}

#[test]
fn external_chat_submissions_route_exclusively_to_non_executable_quarantine() {
    let key = [15u8; 32];
    let principal = valid_bot_principal(&key);
    let receipt_fixture = QuarantinedBotSubmissionReceiptV1 {
        quarantine_record_id: "qrec-001".to_owned(),
        submission_digest_hex: "aa".repeat(32),
        is_executable: false,
        status_badge: "[Quarantined · Untrusted External Submission]".to_owned(),
    };
    let transport = MockDmzTransport::new(serde_json::to_value(&receipt_fixture).unwrap());
    let adapter = BotDmzAdapter::new(principal, transport);

    let receipt = adapter
        .handle_external_submission(ExternalChatSubmissionInput {
            platform: ChatPlatform::Telegram,
            channel_id: "tg-room-7".to_owned(),
            external_user_id: "tg-user-99".to_owned(),
            message_id: "m-501".to_owned(),
            raw_text: "  \u{200B}Copper conducts electricity at 293 K.  ".to_owned(),
            observed_at_unix_s: 1_795_710_500,
        })
        .expect("quarantined submission");
    assert!(!receipt.is_executable);

    let calls = adapter.transport().calls.borrow().clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, BOT_SUBMISSIONS_ENDPOINT);
    let sent: QuarantinedBotSubmissionV1 = serde_json::from_str(&calls[0].2).unwrap();
    assert_eq!(sent.target_store, "quarantine_only");
    assert!(!sent.is_executable);
    assert!(sent.untrusted_external_provenance);
    assert_eq!(sent.sanitized_text, "Copper conducts electricity at 293 K.");
}
