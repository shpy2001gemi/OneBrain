# onebrain-glasses

> Paired OneBrain AR / Smart Glasses Companion HUD Adapter (`P10-SURFACES-SYNC-001` §4.3 & `P9-BCI-SAFETY-001`).

| Field | Value |
|---|---|
| **Tech** | `#![forbid(unsafe_code)]` Rust (`obp/companion-hud/1`) |
| **Session Crypto** | Ephemeral X25519 + RFC 5869 HKDF-SHA256 + `XChaCha20Poly1305` (`Zeroizing` RAM-only) |
| **Doff-Lock Ceiling** | `<= 40 ms` (`HEARTBEAT_INTERVAL_MS = 20`, `SomaticImmediate/1`) |
| **Status** | Implemented (`P10 §4.3`, `P9 §2/§4`) |

## Normative Safety & Capability Invariants

1. **Zero At-Rest Master Secrets (`src/pairing.rs`):**
   - Pairs as an ephemeral companion over `obp/companion-hud/1` to `onebrain-mobile` or `onebrain-desktop`.
   - Retains **zero** `ActorId` secret, `FeedEventSigner`, or `PrivateVault` master keys, exposes **no** `persist_to_disk()` API, and zeroizes all session keys on power-off or `Drop`.
2. **`<= 40 ms` Doff-Lock & Tier M0 Zeroization (`src/hud_state.rs`):**
   - Verifies continuous `20 ms` monotonic wear-sensor heartbeats (`WearContactReading`).
   - Any explicit wear-sensor break, heartbeat gap `> 40 ms`, or replayed sensor counter (`T-BCI-05`) synchronously (`0 ms` scheduling delay) blanks the optical HUD (`OpticalHudRenderState::BlankedLocked`), locks the session, zeroizes cached Tier M0 context cards in RAM, zeros Plane B sensory outputs, and emits a BLAKE3-MACed `SomaticAbortTombstone`.
3. **Bystander Privacy LED Gate & `obc_surface1` Ceiling (`src/capture_gate.rs`):**
   - Outward camera/audio `AI-003` capture fails closed (`CaptureGateError::RecordingIndicatorLedInactive`) unless hardware firmware attests active LED illumination (`recording_led_active == true`) while worn.
   - Enforces `obc_surface1` (`surface_kind == "glasses"`), rejecting `obm1.` admin/archive tokens, bulk `PrivateVault` routes, and `SomaticBiometricRaw` projection (`NEG-BCI-002`).
4. **Anti-Voice-Spoofing Step-2 `PUBLIC` Confirmation (`src/confirm_guard.rs`):**
   - Voice-only confirmation (`ConfirmationModality::VoiceUtteranceOnly`) fails closed (`ConfirmError::VoiceOnlyPublicConfirmForbidden`).
   - Requires a `>= 500 ms` physical temple/ring dwell gesture bound to a verified visual HUD or bone-conduction summary readback digest (`summary_readback_digest == prepared_receipt_digest`) within `<= 60 s`.

## Verification

```bash
cargo clippy --manifest-path src/Cargo.toml --offline -p onebrain-glasses --all-targets --no-deps -- -D warnings
cargo test --manifest-path src/Cargo.toml --offline -p onebrain-glasses
```
