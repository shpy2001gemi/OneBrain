#![forbid(unsafe_code)]
//! Paired `obp/companion-hud/1` Smart Glasses / AR HUD adapter (`P10-SURFACES-SYNC-001` §4.3
//! and `P9-BCI-SAFETY-001`).
//!
//! Normative safety & capability invariants enforced by this crate:
//! 1. **Zero At-Rest Master Secrets (`pairing`):** Ephemeral X25519 + RFC 5869 HKDF-SHA256 +
//!    `XChaCha20Poly1305` session keys wrapped in `zeroize::Zeroizing`; zero `ActorId` root
//!    secret, `FeedEventSigner`, or `PrivateVault` master key custody, and zero disk persistence.
//! 2. **`<= 40 ms` Doff-Lock & Tier M0 Zeroization (`hud_state`):** Continuous `20 ms` monotonic
//!    wear-sensor heartbeats; any break or gap `> 40 ms` synchronously blanks the optical HUD,
//!    locks the companion session, zeroizes cached Tier M0 context cards in RAM, zeros Plane B
//!    sensory outputs, and emits a MACed `SomaticAbortTombstone` under `SomaticImmediate/1`.
//! 3. **Bystander Privacy LED Gate & `obc_surface1` Ceiling (`capture_gate`):** Outward camera /
//!    audio `AI-003` capture fails closed unless hardware firmware attests active recording LED
//!    illumination and active wear-contact; `obm1.` admin tokens and bulk `PrivateVault` endpoints
//!    are rejected fail-closed.
//! 4. **Anti-Voice-Spoofing Step-2 `PUBLIC` Confirmation (`confirm_guard`):** Voice-only `PUBLIC`
//!    confirmation fails closed; requires a `>= 500 ms` physical temple/ring dwell gesture
//!    cryptographically bound to a verified visual HUD or bone-conduction summary readback digest
//!    within an unexpired `<= 60 s` window.

pub mod capture_gate;
pub mod client;
pub mod confirm_guard;
pub mod hud_state;
pub mod pairing;

pub use capture_gate::{
    assert_canonical_hud_badge, validate_glasses_surface_token, AuthorizedOutwardCapture,
    CaptureGate, CaptureGateError, FirmwareLedAttestation, GlassesSurfaceClaims,
    GlassesSurfaceToken, OutwardCaptureModality, OutwardCaptureRequest, TaintClass,
};
pub use client::{CompanionHudClient, CompanionHudError, EncryptedHudEnvelope};
pub use confirm_guard::{
    ConfirmError, ConfirmationModality, PhysicalGestureReceipt, PhysicalGestureSource,
    PreparedPublicReceipt, PublicConfirmGuard, ReadbackChannel, SummaryReadbackReceipt,
    VerifiedPublicConfirmation, MAX_CONFIRM_WINDOW_MS, MIN_GESTURE_DWELL_MS,
};
pub use hud_state::{
    BciHalSafetyGate, HudError, HudStateMachine, OpticalHudRenderState, SomaticAbortReason,
    SomaticAbortTombstone, SomaticPlaneEffect, TierM0ContextCard, WearContactReading,
    HEARTBEAT_INTERVAL_MS, MAX_DOFF_LOCK_LATENCY_MS, SOMATIC_IMMEDIATE_PROFILE_V1,
};
pub use pairing::{
    hkdf_sha256, x25519_diffie_hellman, x25519_public_from_secret, CompanionKeyCustodyAudit,
    EphemeralGlassesKeypair, EphemeralHudSession, HostPairingWelcome, PairingError,
    SomaticLeaseTerms, COMPANION_HUD_PROTOCOL_V1, MAX_SOMATIC_LEASE_TTL_MS,
};
