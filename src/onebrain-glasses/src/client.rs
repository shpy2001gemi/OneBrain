//! High-level `obp/companion-hud/1` Smart Glasses Companion Client (`P10 §4.3`).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capture_gate::{
    validate_glasses_surface_token, AuthorizedOutwardCapture, CaptureGate, CaptureGateError,
    GlassesSurfaceToken, OutwardCaptureRequest,
};
use crate::confirm_guard::{
    ConfirmError, ConfirmationModality, PreparedPublicReceipt, PublicConfirmGuard,
    VerifiedPublicConfirmation,
};
use crate::hud_state::{
    HudError, HudStateMachine, OpticalHudRenderState, SomaticAbortReason, SomaticAbortTombstone,
    TierM0ContextCard, WearContactReading,
};
use crate::pairing::{
    CompanionKeyCustodyAudit, EphemeralGlassesKeypair, EphemeralHudSession, HostPairingWelcome,
    PairingError,
};

#[derive(Debug, Error)]
pub enum CompanionHudError {
    #[error(transparent)]
    Pairing(#[from] PairingError),
    #[error(transparent)]
    Hud(#[from] HudError),
    #[error(transparent)]
    Capture(#[from] CaptureGateError),
    #[error(transparent)]
    Confirm(#[from] ConfirmError),
    #[error("JSON serialization over obp/companion-hud/1 failed: {0}")]
    Serialization(String),
}

/// Encrypted `XChaCha20Poly1305` wire envelope sent over `obp/companion-hud/1`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedHudEnvelope {
    pub seq: u64,
    pub nonce: [u8; 24],
    pub purpose: String,
    pub ciphertext: Vec<u8>,
}

/// Paired Smart Glasses / AR HUD client combining ephemeral session crypto, `<= 40 ms`
/// wear-sensor doff-lock, bystander LED capture gate, and two-step physical confirmation.
pub struct CompanionHudClient {
    session: EphemeralHudSession,
    surface_token: GlassesSurfaceToken,
    hud_state: HudStateMachine,
    confirm_guard: PublicConfirmGuard,
}

impl CompanionHudClient {
    pub fn pair_with_host(
        glasses_ephemeral: EphemeralGlassesKeypair,
        welcome: &HostPairingWelcome,
        expected_host_verifying_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<Self, CompanionHudError> {
        let surface_token = validate_glasses_surface_token(&welcome.surface_token, now_ms)?;
        let session = EphemeralHudSession::complete_handshake(
            glasses_ephemeral,
            welcome,
            expected_host_verifying_key,
            now_ms,
        )?;
        let hud_state =
            HudStateMachine::new_paired(*session.session_id(), now_ms, welcome.lease.expires_at_ms);
        Ok(Self {
            session,
            surface_token,
            hud_state,
            confirm_guard: PublicConfirmGuard::new(),
        })
    }

    pub const fn session(&self) -> &EphemeralHudSession {
        &self.session
    }

    pub const fn hud_state(&self) -> &HudStateMachine {
        &self.hud_state
    }

    pub const fn surface_token(&self) -> &GlassesSurfaceToken {
        &self.surface_token
    }

    pub fn render_state(&self) -> &OpticalHudRenderState {
        self.hud_state.render_state()
    }

    pub fn is_locked(&self) -> bool {
        self.hud_state.is_locked() || self.session.is_powered_off()
    }

    pub fn audit_key_custody(&self) -> CompanionKeyCustodyAudit {
        self.session.audit_key_custody()
    }

    /// Processes a `20 ms` wear-sensor heartbeat and emits a MACed `SomaticAbortTombstone` on break/timeout.
    pub fn on_wear_heartbeat(
        &mut self,
        reading: WearContactReading,
    ) -> Result<Option<SomaticAbortTombstone>, CompanionHudError> {
        let mac_key = *self.session.somatic_mac_key()?;
        Ok(self.hud_state.ingest_wear_heartbeat(reading, &mac_key))
    }

    /// Evaluates monotonic watchdog at `now_ms`, tripping `<= 40 ms` doff-lock if overdue.
    pub fn poll_watchdog(
        &mut self,
        now_ms: u64,
    ) -> Result<Option<SomaticAbortTombstone>, CompanionHudError> {
        let mac_key = *self.session.somatic_mac_key()?;
        Ok(self.hud_state.evaluate_watchdog(now_ms, &mac_key))
    }

    /// Displays a Tier M0 context card on the optical HUD while worn and unlocked.
    pub fn receive_hud_card(&mut self, card: TierM0ContextCard) -> Result<(), CompanionHudError> {
        CaptureGate::assert_route_permitted("/api/vnext/glasses/hud-cards")?;
        self.hud_state.push_context_card(card)?;
        Ok(())
    }

    /// Enforces `obc_surface1` route firewall before any companion request.
    pub fn check_endpoint(&self, path: &str) -> Result<(), CompanionHudError> {
        CaptureGate::assert_route_permitted(path)?;
        Ok(())
    }

    /// Authorizes and encrypts an outward `AI-003` capture frame (`recording_led_active == true` required).
    pub fn submit_outward_capture(
        &mut self,
        req: &OutwardCaptureRequest,
    ) -> Result<(AuthorizedOutwardCapture, EncryptedHudEnvelope), CompanionHudError> {
        CaptureGate::assert_route_permitted("/api/vnext/glasses/capture")?;
        validate_glasses_surface_token(&self.surface_token.raw, req.requested_at_ms)?;
        let authorized =
            CaptureGate::authorize_outward_capture(req, &self.session, &self.hud_state)?;
        let encoded = serde_json::to_vec(&authorized)
            .map_err(|e| CompanionHudError::Serialization(e.to_string()))?;
        let (seq, nonce, ciphertext) = self
            .session
            .seal_to_host("/api/vnext/glasses/capture", &encoded)?;
        Ok((
            authorized,
            EncryptedHudEnvelope {
                seq,
                nonce,
                purpose: "/api/vnext/glasses/capture".to_string(),
                ciphertext,
            },
        ))
    }

    /// Executes two-step physical+readback `PUBLIC` confirmation and seals it for the paired host.
    pub fn confirm_public_share(
        &mut self,
        prepared: &PreparedPublicReceipt,
        modality: &ConfirmationModality,
        now_ms: u64,
    ) -> Result<(VerifiedPublicConfirmation, EncryptedHudEnvelope), CompanionHudError> {
        CaptureGate::assert_route_permitted("/api/vnext/glasses/confirm-public")?;
        validate_glasses_surface_token(&self.surface_token.raw, now_ms)?;
        let verified = self.confirm_guard.verify_step2_public_confirm(
            prepared,
            modality,
            &self.session,
            &self.hud_state,
            now_ms,
        )?;
        let encoded = serde_json::to_vec(&verified)
            .map_err(|e| CompanionHudError::Serialization(e.to_string()))?;
        let (seq, nonce, ciphertext) = self
            .session
            .seal_to_host("/api/vnext/glasses/confirm-public", &encoded)?;
        Ok((
            verified,
            EncryptedHudEnvelope {
                seq,
                nonce,
                purpose: "/api/vnext/glasses/confirm-public".to_string(),
                ciphertext,
            },
        ))
    }

    /// Powers off the glasses: blanks HUD, zeroizes Tier M0 cards, and zeroizes all session keys.
    pub fn power_off(&mut self, now_ms: u64) -> Option<SomaticAbortTombstone> {
        let tombstone = self.session.somatic_mac_key().ok().copied().map(|mac_key| {
            self.hud_state.trip_synchronous_cutoff(
                now_ms,
                SomaticAbortReason::HardwareKillSwitch,
                &mac_key,
            )
        });
        self.session.power_off();
        tombstone
    }
}
