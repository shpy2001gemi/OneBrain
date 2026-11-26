#![forbid(unsafe_code)]

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use onebrain_glasses::hud_state::{SensoryCueKind, SensoryFeedbackFrameV1};
use onebrain_glasses::pairing::build_pairing_transcript;
use onebrain_glasses::{
    validate_glasses_surface_token, CaptureGateError, CompanionHudClient, CompanionHudError,
    ConfirmError, ConfirmationModality, EphemeralGlassesKeypair, FirmwareLedAttestation,
    HostPairingWelcome, HudError, OpticalHudRenderState, OutwardCaptureModality,
    OutwardCaptureRequest, PhysicalGestureReceipt, PhysicalGestureSource, PreparedPublicReceipt,
    ReadbackChannel, SomaticAbortReason, SomaticLeaseTerms, SomaticPlaneEffect,
    SummaryReadbackReceipt, TaintClass, TierM0ContextCard, WearContactReading,
    COMPANION_HUD_PROTOCOL_V1, HEARTBEAT_INTERVAL_MS, MAX_DOFF_LOCK_LATENCY_MS,
};

fn make_glasses_surface_token(surface_kind: &str, permissions: &[&str], exp_ms: u64) -> String {
    let claims = serde_json::json!({
        "surface_kind": surface_kind,
        "protocol": COMPANION_HUD_PROTOCOL_V1,
        "permissions": permissions,
        "exp_ms": exp_ms,
    });
    let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("json"));
    let mac_hex = blake3::hash(format!("obc_surface1.{claims_b64}").as_bytes())
        .to_hex()
        .to_string();
    format!("obc_surface1.{claims_b64}.{mac_hex}")
}

fn make_paired_client(now_ms: u64) -> CompanionHudClient {
    let host_signing = SigningKey::from_bytes(&[0x55; 32]);
    let host_verifying = host_signing.verifying_key().to_bytes();
    let host_ephemeral = EphemeralGlassesKeypair::from_ephemeral_secret_for_test([0x22; 32]);
    let glasses_ephemeral = EphemeralGlassesKeypair::from_ephemeral_secret_for_test([0x33; 32]);

    let session_id = [0x99; 32];
    let surface_token = make_glasses_surface_token(
        "glasses",
        &[
            "glasses:hud_cards:read",
            "glasses:capture:write",
            "glasses:public_confirm:two_step",
            "p9:somatic_immediate:paired",
        ],
        now_ms + 300_000,
    );
    let lease = SomaticLeaseTerms {
        issued_at_ms: now_ms,
        expires_at_ms: now_ms + 300_000,
        onward_delegation: false,
        delete_after_task: true,
        training_permission: false,
    };

    let transcript = build_pairing_transcript(
        &session_id,
        glasses_ephemeral.public_bytes(),
        host_ephemeral.public_bytes(),
        &surface_token,
        &lease,
    );
    let sig = host_signing.sign(&transcript).to_bytes().to_vec();

    let welcome = HostPairingWelcome {
        session_id,
        host_device_verifying_key: host_verifying,
        host_ephemeral_x25519_pub: *host_ephemeral.public_bytes(),
        surface_token,
        lease,
        host_transcript_signature: sig,
    };

    CompanionHudClient::pair_with_host(glasses_ephemeral, &welcome, &host_verifying, now_ms)
        .expect("pairing succeeds")
}

#[test]
fn zero_at_rest_master_secrets_and_power_off_ram_zeroization() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let mut client = make_paired_client(1_000);

    let initial_audit = client.audit_key_custody();
    assert!(!initial_audit.retains_actor_id_secret);
    assert!(!initial_audit.retains_feed_event_signer);
    assert!(!initial_audit.retains_private_vault_key);
    assert_eq!(initial_audit.persisted_at_rest_bytes, 0);
    assert!(!initial_audit.active_ephemeral_keys_zeroized);
    assert_eq!(client.session().short_auth_string().len(), 6);

    let card = TierM0ContextCard::new(
        "card_secret_1",
        b"Confidential Meeting Agenda",
        b"Discuss zero-key-reuse personal mesh epoch rotation",
        "[Private · Local Only]",
        "bafkreicard1",
    )
    .expect("valid card");
    client.receive_hud_card(card).expect("push card");
    assert_eq!(
        client.render_state(),
        &OpticalHudRenderState::Active { card_count: 1 }
    );

    // Power off: blanks HUD, zeroizes Tier M0 cards, and zeroizes all session keys in RAM.
    let tombstone = client.power_off(1_015).expect("abort tombstone emitted");
    assert_eq!(tombstone.reason, SomaticAbortReason::HardwareKillSwitch);
    assert_eq!(tombstone.zeroized_card_count, 1);
    assert!(tombstone.zeroized_tier_m0_bytes > 0);
    assert_eq!(client.render_state(), &OpticalHudRenderState::BlankedLocked);
    assert!(client.is_locked());

    let post_poweroff_audit = client.audit_key_custody();
    assert!(post_poweroff_audit.active_ephemeral_keys_zeroized);
    assert_eq!(post_poweroff_audit.persisted_at_rest_bytes, 0);

    // Confirm zero files written anywhere in temp directory.
    assert_eq!(
        std::fs::read_dir(temp_dir.path())
            .expect("read_dir")
            .count(),
        0
    );
}

#[test]
fn doff_lock_within_40ms_blanks_hud_zeroizes_tier_m0_and_blocks_replay() {
    assert_eq!(HEARTBEAT_INTERVAL_MS, 20);
    assert_eq!(MAX_DOFF_LOCK_LATENCY_MS, 40);

    // Direct buffer zeroization check on TierM0ContextCard.
    let mut standalone_card = TierM0ContextCard::new(
        "c0",
        b"Sensitive Title",
        b"Sensitive Summary Body",
        "[Personal Mesh · Encrypted]",
        "bafkreic0",
    )
    .expect("card");
    standalone_card.zeroize_in_place();
    assert!(standalone_card.title_bytes().iter().all(|&b| b == 0));
    assert!(standalone_card.summary_bytes().iter().all(|&b| b == 0));

    // Case A: Explicit wear-sensor break at +18 ms (<= 40 ms).
    let mut client = make_paired_client(10_000);
    let expected_mac_key_check = {
        let card = TierM0ContextCard::new(
            "c1",
            b"Project Horizon",
            b"Private local observation summary",
            "[Private · Local Only]",
            "bafkreic1",
        )
        .expect("card");
        client.receive_hud_card(card).expect("receive card");
        true
    };
    assert!(expected_mac_key_check);

    // Valid 20 ms heartbeat keeps HUD active.
    let hb1 = client
        .on_wear_heartbeat(WearContactReading {
            seq: 1,
            monotonic_ms: 10_020,
            worn_contact_active: true,
            hardware_nonce: [0x01; 16],
        })
        .expect("hb1");
    assert!(hb1.is_none());
    assert!(!client.is_locked());

    // Exact 40 ms gap (10_020 -> 10_060) with contact active still passes boundary (`<= 40 ms`).
    let hb2 = client
        .on_wear_heartbeat(WearContactReading {
            seq: 2,
            monotonic_ms: 10_060,
            worn_contact_active: true,
            hardware_nonce: [0x02; 16],
        })
        .expect("hb2");
    assert!(hb2.is_none());
    assert!(!client.is_locked());

    // Physical removal (`worn_contact_active: false`) immediately blanks & zeroizes!
    let tomb_break = client
        .on_wear_heartbeat(WearContactReading {
            seq: 3,
            monotonic_ms: 10_072,
            worn_contact_active: false,
            hardware_nonce: [0x03; 16],
        })
        .expect("hb3")
        .expect("tombstone on wear break");
    assert_eq!(tomb_break.profile, "SomaticImmediate/1");
    assert_eq!(tomb_break.reason, SomaticAbortReason::WearContactLost);
    assert_eq!(tomb_break.zeroized_card_count, 1);
    assert_eq!(tomb_break.zeroized_tier_m0_bytes, 48);
    assert!(tomb_break.plane_b_outputs_zeroed);
    assert_eq!(client.render_state(), &OpticalHudRenderState::BlankedLocked);
    assert_eq!(client.hud_state().active_cards().len(), 0);

    // Subsequent card pushes fail closed while locked.
    let post_lock_card = TierM0ContextCard::new(
        "c2",
        b"Blocked",
        b"Blocked",
        "[Private · Local Only]",
        "bafkreic2",
    )
    .expect("card");
    assert!(matches!(
        client.receive_hud_card(post_lock_card),
        Err(CompanionHudError::Hud(HudError::SessionLocked))
    ));

    // Case B: Missing 2 consecutive heartbeats (elapsed 41 ms > 40 ms) trips watchdog cut-off.
    let mut client_timeout = make_paired_client(20_000);
    let tomb_timeout = client_timeout
        .poll_watchdog(20_041)
        .expect("poll")
        .expect("watchdog trips at 41 ms");
    assert_eq!(
        tomb_timeout.reason,
        SomaticAbortReason::HeartbeatTimeoutExceeded
    );
    assert_eq!(
        client_timeout.render_state(),
        &OpticalHudRenderState::BlankedLocked
    );

    // Case C: Sensor counter replay (`T-BCI-05`) trips immediate cut-off.
    let mut client_replay = make_paired_client(30_000);
    assert!(client_replay
        .on_wear_heartbeat(WearContactReading {
            seq: 10,
            monotonic_ms: 30_020,
            worn_contact_active: true,
            hardware_nonce: [0xAA; 16],
        })
        .expect("hb")
        .is_none());
    let tomb_replay = client_replay
        .on_wear_heartbeat(WearContactReading {
            seq: 10, // replayed counter!
            monotonic_ms: 30_030,
            worn_contact_active: true,
            hardware_nonce: [0xAA; 16],
        })
        .expect("hb replay")
        .expect("replay trips abort");
    assert_eq!(
        tomb_replay.reason,
        SomaticAbortReason::SensorReplayOrClockRegression
    );
}

#[test]
fn bystander_privacy_led_gate_fails_closed_when_led_inactive() {
    let mut client = make_paired_client(50_000);

    // 1. LED inactive (`recording_led_active == false`) -> RecordingIndicatorLedInactive!
    let inactive_mac = client
        .session()
        .compute_led_attestation_mac(false, true, 50_010)
        .expect("mac");
    let bad_req = OutwardCaptureRequest {
        modality: OutwardCaptureModality::OutwardCameraFrame,
        requested_at_ms: 50_012,
        taint_class: TaintClass::MediaCapture,
        payload_digest_blake3: [0x11; 32],
        payload_len: 4096,
        firmware_led_attestation: FirmwareLedAttestation {
            recording_led_active: false,
            led_circuit_closed: true,
            attested_at_ms: 50_010,
            attestation_mac: inactive_mac,
        },
    };
    assert!(matches!(
        client.submit_outward_capture(&bad_req),
        Err(CompanionHudError::Capture(
            CaptureGateError::RecordingIndicatorLedInactive
        ))
    ));

    // 2. SomaticBiometricRaw taint rejected (`NEG-BCI-002`).
    let active_mac = client
        .session()
        .compute_led_attestation_mac(true, true, 50_010)
        .expect("mac");
    let raw_bci_req = OutwardCaptureRequest {
        taint_class: TaintClass::SomaticBiometricRaw,
        firmware_led_attestation: FirmwareLedAttestation {
            recording_led_active: true,
            led_circuit_closed: true,
            attested_at_ms: 50_010,
            attestation_mac: active_mac,
        },
        ..bad_req
    };
    assert!(matches!(
        client.submit_outward_capture(&raw_bci_req),
        Err(CompanionHudError::Capture(
            CaptureGateError::SomaticBiometricRawExportProhibited
        ))
    ));

    // 3. Valid active hardware LED + live wear contact succeeds and seals over XChaCha20Poly1305.
    let valid_req = OutwardCaptureRequest {
        taint_class: TaintClass::MediaCapture,
        ..raw_bci_req
    };
    let (authorized, envelope) = client
        .submit_outward_capture(&valid_req)
        .expect("LED-attested capture succeeds");
    assert!(authorized.recording_led_verified);
    assert_eq!(authorized.disclosure_badge, "[Private · Local Only]");
    assert_eq!(envelope.purpose, "/api/vnext/glasses/capture");
    assert!(!envelope.ciphertext.is_empty());
}

#[test]
fn anti_voice_spoofing_blocks_voice_confirm_and_accepts_two_step_physical_plus_readback() {
    let mut client = make_paired_client(100_000);

    // Keep wear contact fresh across the >= 500 ms refractory window.
    for step in 1..=30u64 {
        let t = 100_000 + step * 20;
        assert!(client
            .on_wear_heartbeat(WearContactReading {
                seq: step,
                monotonic_ms: t,
                worn_contact_active: true,
                hardware_nonce: [step as u8; 16],
            })
            .expect("hb")
            .is_none());
    }

    let receipt_digest = *blake3::hash(b"prepared-public-ku-summary-v1").as_bytes();
    let prepared = PreparedPublicReceipt {
        draft_id: "draft_hud_77".to_string(),
        prepared_receipt_digest: receipt_digest,
        prepared_at_ms: 100_000,
        expires_at_ms: 160_000, // exact 60s window
        epistemic_badge: "[Public · Unassessed]".to_string(),
    };

    // 1. Voice-only confirmation MUST fail closed (`VoiceOnlyPublicConfirmForbidden`).
    let voice_attempt = ConfirmationModality::VoiceUtteranceOnly {
        utterance: "OneBrain publish this note publicly right now".to_string(),
    };
    assert!(matches!(
        client.confirm_public_share(&prepared, &voice_attempt, 100_600),
        Err(CompanionHudError::Confirm(
            ConfirmError::VoiceOnlyPublicConfirmForbidden
        ))
    ));

    // 2. Physical gesture with dwell < 500 ms MUST fail closed (`InsufficientGestureDwell`).
    let short_mac = client
        .session()
        .compute_gesture_mac(
            PhysicalGestureSource::TempleCapacitiveHold.as_tag(),
            320,
            100_600,
            &receipt_digest,
        )
        .expect("mac");
    let short_dwell = ConfirmationModality::PhysicalGestureWithReadback {
        gesture: PhysicalGestureReceipt {
            source: PhysicalGestureSource::TempleCapacitiveHold,
            dwell_ms: 320,
            completed_at_ms: 100_600,
            bound_receipt_digest: receipt_digest,
            gesture_mac: short_mac,
        },
        readback: SummaryReadbackReceipt {
            channel: ReadbackChannel::OpticalHudVisualSummary,
            summary_readback_digest: receipt_digest,
            completed_at_ms: 100_200,
        },
    };
    assert!(matches!(
        client.confirm_public_share(&prepared, &short_dwell, 100_600),
        Err(CompanionHudError::Confirm(
            ConfirmError::InsufficientGestureDwell { dwell_ms: 320 }
        ))
    ));

    // 3. Mismatched summary readback digest MUST fail closed.
    let valid_mac = client
        .session()
        .compute_gesture_mac(
            PhysicalGestureSource::CompanionRingPressHold.as_tag(),
            650,
            100_600,
            &receipt_digest,
        )
        .expect("mac");
    let wrong_readback = ConfirmationModality::PhysicalGestureWithReadback {
        gesture: PhysicalGestureReceipt {
            source: PhysicalGestureSource::CompanionRingPressHold,
            dwell_ms: 650,
            completed_at_ms: 100_600,
            bound_receipt_digest: receipt_digest,
            gesture_mac: valid_mac,
        },
        readback: SummaryReadbackReceipt {
            channel: ReadbackChannel::BoneConductionAudioSummary,
            summary_readback_digest: [0xFF; 32],
            completed_at_ms: 100_250,
        },
    };
    assert!(matches!(
        client.confirm_public_share(&prepared, &wrong_readback, 100_600),
        Err(CompanionHudError::Confirm(
            ConfirmError::SummaryReadbackDigestMismatch
        ))
    ));

    // 4. Valid >= 500 ms physical ring press-hold + verified bone-conduction readback succeeds!
    let valid_two_step = ConfirmationModality::PhysicalGestureWithReadback {
        gesture: PhysicalGestureReceipt {
            source: PhysicalGestureSource::CompanionRingPressHold,
            dwell_ms: 650,
            completed_at_ms: 100_600,
            bound_receipt_digest: receipt_digest,
            gesture_mac: valid_mac,
        },
        readback: SummaryReadbackReceipt {
            channel: ReadbackChannel::BoneConductionAudioSummary,
            summary_readback_digest: receipt_digest,
            completed_at_ms: 100_250,
        },
    };
    let (confirmed, envelope) = client
        .confirm_public_share(&prepared, &valid_two_step, 100_600)
        .expect("two-step physical + readback confirm succeeds");
    assert_eq!(confirmed.draft_id, "draft_hud_77");
    assert_eq!(confirmed.epistemic_badge, "[Public · Unassessed]");
    assert_eq!(envelope.purpose, "/api/vnext/glasses/confirm-public");

    // 5. Replaying the same receipt digest fails closed (`ReceiptAlreadyConsumed`).
    assert!(matches!(
        client.confirm_public_share(&prepared, &valid_two_step, 100_600),
        Err(CompanionHudError::Confirm(
            ConfirmError::ReceiptAlreadyConsumed
        ))
    ));
}

#[test]
fn rejects_obm1_tokens_bulk_vault_routes_legacy_badges_and_plane_c_neural_write() {
    assert_eq!(
        validate_glasses_surface_token("obm1.admin.secret", 1_000).unwrap_err(),
        CaptureGateError::Obm1AdminTokenProhibited
    );

    let ext_token = make_glasses_surface_token("extension", &["glasses:hud_cards:read"], 9_000);
    assert!(matches!(
        validate_glasses_surface_token(&ext_token, 1_000),
        Err(CaptureGateError::InvalidSurfaceToken(_))
    ));

    let client = make_paired_client(5_000);
    for forbidden_route in [
        "/api/kus",
        "/api/export",
        "/api/backup",
        "/api/restore",
        "/api/base/v1/ku",
        "/api/archive",
    ] {
        assert!(matches!(
            client.check_endpoint(forbidden_route),
            Err(CompanionHudError::Capture(
                CaptureGateError::BulkVaultAccessProhibited
            ))
        ));
    }

    for legacy_badge in ["[FULL]", "[GLOBAL]", "[CLOSED]"] {
        assert!(TierM0ContextCard::new("id", b"t", b"s", legacy_badge, "cid").is_err());
    }

    // Plane C arbitrary neural write fails closed (`NEG-BCI-001`).
    let mut hud = onebrain_glasses::HudStateMachine::new_paired([0x11; 32], 1_000, 50_000);
    let cue = SensoryFeedbackFrameV1 {
        cue_kind: SensoryCueKind::HapticPulse,
        intensity_permille: 500,
        frequency_milli_hz: 100_000,
        pulse_width_us: 200,
        duration_ms: 150,
        min_inter_frame_cooldown_ms: 100,
    };
    assert_eq!(
        hud.emit_sensory_feedback(SomaticPlaneEffect::ArbitraryNeuralWrite, &cue, 1_100),
        Err(HudError::ArbitraryNeuralWriteProhibited)
    );
    assert!(hud
        .emit_sensory_feedback(SomaticPlaneEffect::SensoryFeedback, &cue, 1_100)
        .is_ok());
}
