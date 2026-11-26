//! Anti-Voice-Spoofing Step-2 `PUBLIC` Confirmation Guard (`P10 §4.3`, `P9 §2.1`).
//!
//! Single-utterance voice commands (`ConfirmationModality::VoiceUtteranceOnly`) fail closed.
//! Step-2 `PUBLIC` confirmation requires a `>= 500 ms` physical temple/ring dwell gesture
//! cryptographically bound to a verified visual HUD or bone-conduction summary readback digest
//! within an unexpired `<= 60 s` window.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capture_gate::assert_canonical_hud_badge;
use crate::hud_state::{HudStateMachine, MAX_DOFF_LOCK_LATENCY_MS};
use crate::pairing::EphemeralHudSession;

/// Minimum intentional physical temple/ring gesture dwell & refractory separation (`P9 §2.1`).
pub const MIN_GESTURE_DWELL_MS: u64 = 500;

/// Maximum lifetime of a prepared `PUBLIC` share receipt on `onebrain-glasses` (`60 s`).
pub const MAX_CONFIRM_WINDOW_MS: u64 = 60_000;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfirmError {
    #[error("anti-voice-spoofing (P10 §4.3): voice-only step-2 PUBLIC confirmation is forbidden")]
    VoiceOnlyPublicConfirmForbidden,
    #[error(
        "anti-impulsive guard (P9 §2.1): unverified somatic intent cannot confirm PUBLIC share"
    )]
    ImpulsiveSomaticConfirmForbidden,
    #[error("physical gesture dwell {dwell_ms} ms is below mandatory 500 ms refractory threshold")]
    InsufficientGestureDwell { dwell_ms: u64 },
    #[error("refractory separation {elapsed_ms} ms since receipt display is below 500 ms")]
    RefractoryWindowNotElapsed { elapsed_ms: u64 },
    #[error("summary readback digest does not match prepared receipt digest")]
    SummaryReadbackDigestMismatch,
    #[error("physical gesture receipt digest does not match prepared receipt digest")]
    GestureReceiptDigestMismatch,
    #[error("summary readback occurred after gesture completion")]
    ReadbackOrderedAfterGesture,
    #[error("invalid hardware physical gesture MAC")]
    InvalidGestureMac,
    #[error("prepared PUBLIC confirmation window (> 60s or past expiry) has expired")]
    ConfirmationWindowExpired,
    #[error("HUD companion session is locked or wear-contact is stale")]
    HudSessionLockedOrUnworn,
    #[error("prepared receipt digest was already consumed")]
    ReceiptAlreadyConsumed,
    #[error("non-canonical epistemic badge on prepared PUBLIC receipt: {0}")]
    InvalidEpistemicBadge(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicalGestureSource {
    TempleCapacitiveHold,
    CompanionRingPressHold,
}

impl PhysicalGestureSource {
    pub const fn as_tag(self) -> &'static str {
        match self {
            Self::TempleCapacitiveHold => "temple_capacitive_hold",
            Self::CompanionRingPressHold => "companion_ring_press_hold",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadbackChannel {
    OpticalHudVisualSummary,
    BoneConductionAudioSummary,
}

/// Intentional physical confirmation gesture receipt (`P10 §4.3`, `P9 §2.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalGestureReceipt {
    pub source: PhysicalGestureSource,
    pub dwell_ms: u64,
    pub completed_at_ms: u64,
    pub bound_receipt_digest: [u8; 32],
    pub gesture_mac: [u8; 32],
}

/// Verified visual HUD or bone-conduction summary readback attestation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryReadbackReceipt {
    pub channel: ReadbackChannel,
    pub summary_readback_digest: [u8; 32],
    pub completed_at_ms: u64,
}

/// Confirmation modality presented to `PublicConfirmGuard`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationModality {
    /// Forbidden by `P10 §4.3` (susceptible to ambient/acoustic voice spoofing).
    VoiceUtteranceOnly { utterance: String },
    /// Forbidden by `P9 §2.1` (`unassessed_somatic_decode` cannot execute high-consequence confirm).
    UnverifiedSomaticDecodeOnly,
    /// Mandatory two-step physical gesture + summary readback verification.
    PhysicalGestureWithReadback {
        gesture: PhysicalGestureReceipt,
        readback: SummaryReadbackReceipt,
    },
}

/// Host-prepared `PUBLIC` publication receipt awaiting user step-2 confirmation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedPublicReceipt {
    pub draft_id: String,
    pub prepared_receipt_digest: [u8; 32],
    pub prepared_at_ms: u64,
    pub expires_at_ms: u64,
    pub epistemic_badge: String,
}

/// Cryptographically bound step-2 `PUBLIC` confirmation proof returned to the paired host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedPublicConfirmation {
    pub draft_id: String,
    pub prepared_receipt_digest: [u8; 32],
    pub gesture_source: PhysicalGestureSource,
    pub readback_channel: ReadbackChannel,
    pub dwell_ms: u64,
    pub confirmed_at_ms: u64,
    pub epistemic_badge: String,
    pub confirmation_mac: [u8; 32],
}

/// Stateful confirmation guard tracking consumed receipts to defeat confirmation replay.
#[derive(Default)]
pub struct PublicConfirmGuard {
    consumed_digests: BTreeSet<[u8; 32]>,
}

impl PublicConfirmGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn verify_step2_public_confirm(
        &mut self,
        prepared: &PreparedPublicReceipt,
        modality: &ConfirmationModality,
        session: &EphemeralHudSession,
        hud_state: &HudStateMachine,
        now_ms: u64,
    ) -> Result<VerifiedPublicConfirmation, ConfirmError> {
        let (gesture, readback) = match modality {
            ConfirmationModality::VoiceUtteranceOnly { .. } => {
                return Err(ConfirmError::VoiceOnlyPublicConfirmForbidden);
            }
            ConfirmationModality::UnverifiedSomaticDecodeOnly => {
                return Err(ConfirmError::ImpulsiveSomaticConfirmForbidden);
            }
            ConfirmationModality::PhysicalGestureWithReadback { gesture, readback } => {
                (gesture, readback)
            }
        };

        assert_canonical_hud_badge(&prepared.epistemic_badge)
            .map_err(|_| ConfirmError::InvalidEpistemicBadge(prepared.epistemic_badge.clone()))?;

        if hud_state.is_locked() || session.is_powered_off() {
            return Err(ConfirmError::HudSessionLockedOrUnworn);
        }
        let last_worn = hud_state
            .last_contact_ms()
            .ok_or(ConfirmError::HudSessionLockedOrUnworn)?;
        if now_ms < last_worn || now_ms - last_worn > MAX_DOFF_LOCK_LATENCY_MS {
            return Err(ConfirmError::HudSessionLockedOrUnworn);
        }

        if prepared.expires_at_ms <= prepared.prepared_at_ms
            || prepared.expires_at_ms - prepared.prepared_at_ms > MAX_CONFIRM_WINDOW_MS
            || now_ms < prepared.prepared_at_ms
            || now_ms > prepared.expires_at_ms
            || now_ms - prepared.prepared_at_ms > MAX_CONFIRM_WINDOW_MS
        {
            return Err(ConfirmError::ConfirmationWindowExpired);
        }

        if self
            .consumed_digests
            .contains(&prepared.prepared_receipt_digest)
        {
            return Err(ConfirmError::ReceiptAlreadyConsumed);
        }

        if readback.summary_readback_digest != prepared.prepared_receipt_digest {
            return Err(ConfirmError::SummaryReadbackDigestMismatch);
        }
        if gesture.bound_receipt_digest != prepared.prepared_receipt_digest {
            return Err(ConfirmError::GestureReceiptDigestMismatch);
        }
        if readback.completed_at_ms < prepared.prepared_at_ms
            || gesture.completed_at_ms < readback.completed_at_ms
            || gesture.completed_at_ms > now_ms
        {
            return Err(ConfirmError::ReadbackOrderedAfterGesture);
        }

        if gesture.dwell_ms < MIN_GESTURE_DWELL_MS {
            return Err(ConfirmError::InsufficientGestureDwell {
                dwell_ms: gesture.dwell_ms,
            });
        }
        let refractory_elapsed = gesture
            .completed_at_ms
            .saturating_sub(prepared.prepared_at_ms);
        if refractory_elapsed < MIN_GESTURE_DWELL_MS {
            return Err(ConfirmError::RefractoryWindowNotElapsed {
                elapsed_ms: refractory_elapsed,
            });
        }

        let expected_gesture_mac = session
            .compute_gesture_mac(
                gesture.source.as_tag(),
                gesture.dwell_ms,
                gesture.completed_at_ms,
                &gesture.bound_receipt_digest,
            )
            .map_err(|_| ConfirmError::HudSessionLockedOrUnworn)?;
        if gesture.gesture_mac != expected_gesture_mac {
            return Err(ConfirmError::InvalidGestureMac);
        }

        let mut confirm_hasher = blake3::Hasher::new_keyed(
            session
                .somatic_mac_key()
                .map_err(|_| ConfirmError::HudSessionLockedOrUnworn)?,
        );
        confirm_hasher.update(b"onebrain:glasses:step2-public-confirm:v1");
        confirm_hasher.update(session.session_id());
        confirm_hasher.update(prepared.draft_id.as_bytes());
        confirm_hasher.update(&prepared.prepared_receipt_digest);
        confirm_hasher.update(&gesture.dwell_ms.to_be_bytes());
        confirm_hasher.update(&now_ms.to_be_bytes());
        let confirmation_mac = *confirm_hasher.finalize().as_bytes();

        self.consumed_digests
            .insert(prepared.prepared_receipt_digest);

        Ok(VerifiedPublicConfirmation {
            draft_id: prepared.draft_id.clone(),
            prepared_receipt_digest: prepared.prepared_receipt_digest,
            gesture_source: gesture.source,
            readback_channel: readback.channel,
            dwell_ms: gesture.dwell_ms,
            confirmed_at_ms: now_ms,
            epistemic_badge: prepared.epistemic_badge.clone(),
            confirmation_mac,
        })
    }
}
