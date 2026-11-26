//! Bystander Privacy Recording-LED Gate (`AI-003`) & `obc_surface1` (`Glasses`) Ceiling (`P10 §4.3`).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hud_state::{HudStateMachine, MAX_DOFF_LOCK_LATENCY_MS};
use crate::pairing::EphemeralHudSession;

const ALLOWED_GLASSES_PERMISSIONS: &[&str] = &[
    "glasses:hud_cards:read",
    "glasses:capture:write",
    "glasses:public_confirm:two_step",
    "p9:somatic_immediate:paired",
];

const PROHIBITED_BULK_VAULT_PREFIXES: &[&str] = &[
    "/api/kus",
    "/api/export",
    "/api/backup",
    "/api/restore",
    "/api/base/v1",
    "/api/archive",
    "/api/vnext/archive",
    "/api/obm",
];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CaptureGateError {
    #[error("bystander privacy violation: hardware recording-indicator LED is inactive")]
    RecordingIndicatorLedInactive,
    #[error("firmware LED attestation MAC invalid or stale")]
    InvalidFirmwareLedAttestation,
    #[error("outward capture denied: HUD session is locked or unworn")]
    HudSessionLockedOrUnworn,
    #[error("NEG-P10-002/§4: obm1 management/archive tokens are prohibited on onebrain-glasses")]
    Obm1AdminTokenProhibited,
    #[error("invalid obc_surface1 token: {0}")]
    InvalidSurfaceToken(&'static str),
    #[error("surface ceiling exceeded: bulk PrivateVault or archive access is prohibited")]
    BulkVaultAccessProhibited,
    #[error("NEG-BCI-002: SomaticBiometricRaw taint is permanently non-sanitizable and cannot cross wire/mesh")]
    SomaticBiometricRawExportProhibited,
    #[error("RUN-004 / P10 §2.2: invalid or legacy epistemic badge: {0}")]
    InvalidEpistemicBadge(String),
}

/// Data taint classes enforced at the companion boundary (`P9 §3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaintClass {
    SymbolicDraft,
    MediaCapture,
    SomaticBiometricRaw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutwardCaptureModality {
    OutwardCameraFrame,
    OutwardAudioClip,
    OutwardVideoSnippet,
}

/// Hardware firmware attestation proving outward recording-indicator LED activation (`P10 §4.3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareLedAttestation {
    pub recording_led_active: bool,
    pub led_circuit_closed: bool,
    pub attested_at_ms: u64,
    pub attestation_mac: [u8; 32],
}

/// Outward `AI-003` sensor capture request from `onebrain-glasses`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutwardCaptureRequest {
    pub modality: OutwardCaptureModality,
    pub requested_at_ms: u64,
    pub taint_class: TaintClass,
    pub payload_digest_blake3: [u8; 32],
    pub payload_len: usize,
    pub firmware_led_attestation: FirmwareLedAttestation,
}

/// Validated outward `AI-003` capture ready to transmit over `obp/companion-hud/1` (`LOCAL_ONLY`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizedOutwardCapture {
    pub modality: OutwardCaptureModality,
    pub authorized_at_ms: u64,
    pub payload_digest_blake3: [u8; 32],
    pub payload_len: usize,
    pub disclosure_badge: String,
    pub recording_led_verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlassesSurfaceClaims {
    pub surface_kind: String,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub exp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlassesSurfaceToken {
    pub raw: String,
    pub claims: GlassesSurfaceClaims,
}

/// Validates canonical `RUN-004` / `P0-ADR-001` surface badges and rejects `FULL`/`GLOBAL`/`CLOSED`.
pub fn assert_canonical_hud_badge(badge: &str) -> Result<(), CaptureGateError> {
    let upper = badge.to_ascii_uppercase();
    for forbidden in ["FULL", "GLOBAL", "CLOSED"] {
        if upper.contains(forbidden) {
            return Err(CaptureGateError::InvalidEpistemicBadge(badge.to_string()));
        }
    }
    let valid_static = matches!(
        badge,
        "[Private · Local Only]"
            | "[Personal Mesh · Encrypted]"
            | "[Public · Unassessed]"
            | "[Local Store Only]"
    );
    let valid_corroborated = badge
        .strip_prefix("[Public · Fidelity Corroborated (")
        .and_then(|rest| rest.strip_suffix(" groups)]"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let valid_sampled = badge
        .strip_prefix("[Sampled Reachable Peers: ")
        .and_then(|rest| rest.strip_suffix(']'))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));

    if valid_static || valid_corroborated || valid_sampled {
        Ok(())
    } else {
        Err(CaptureGateError::InvalidEpistemicBadge(badge.to_string()))
    }
}

/// Validates `obc_surface1.<claims>.<mac>` scoped strictly to `surface_kind == "glasses"`.
pub fn validate_glasses_surface_token(
    raw_token: &str,
    now_ms: u64,
) -> Result<GlassesSurfaceToken, CaptureGateError> {
    let trimmed = raw_token.trim();
    if trimmed.is_empty() {
        return Err(CaptureGateError::InvalidSurfaceToken("empty surface token"));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "obm1" || lower.starts_with("obm1.") {
        return Err(CaptureGateError::Obm1AdminTokenProhibited);
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() != 3 || parts[0] != "obc_surface1" {
        return Err(CaptureGateError::InvalidSurfaceToken(
            "expected obc_surface1.<claims>.<mac>",
        ));
    }

    let claims_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| CaptureGateError::InvalidSurfaceToken("invalid base64url claims"))?;
    let claims: GlassesSurfaceClaims = serde_json::from_slice(&claims_bytes)
        .map_err(|_| CaptureGateError::InvalidSurfaceToken("malformed JSON claims"))?;

    if claims.surface_kind != "glasses" {
        return Err(CaptureGateError::InvalidSurfaceToken(
            "surface_kind MUST equal \"glasses\"",
        ));
    }
    if let Some(proto) = &claims.protocol {
        if proto != "obp/companion-hud/1" {
            return Err(CaptureGateError::InvalidSurfaceToken(
                "protocol claim MUST equal \"obp/companion-hud/1\"",
            ));
        }
    }
    if claims.permissions.is_empty() {
        return Err(CaptureGateError::InvalidSurfaceToken(
            "permissions list MUST NOT be empty",
        ));
    }
    for perm in &claims.permissions {
        if !ALLOWED_GLASSES_PERMISSIONS.contains(&perm.as_str()) {
            return Err(CaptureGateError::BulkVaultAccessProhibited);
        }
    }
    if let Some(exp) = claims.exp_ms {
        if now_ms >= exp {
            return Err(CaptureGateError::InvalidSurfaceToken("token expired"));
        }
    }

    let mac_len = if parts[2].len() == 64 && parts[2].chars().all(|c| c.is_ascii_hexdigit()) {
        32
    } else {
        URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|_| CaptureGateError::InvalidSurfaceToken("invalid MAC encoding"))?
            .len()
    };
    if mac_len != 32 {
        return Err(CaptureGateError::InvalidSurfaceToken(
            "BLAKE3 MAC must decode to 32 bytes",
        ));
    }

    Ok(GlassesSurfaceToken {
        raw: trimmed.to_string(),
        claims,
    })
}

/// Enforces bystander LED privacy, taint rules, and `obc_surface1` endpoint ceilings.
pub struct CaptureGate;

impl CaptureGate {
    /// Rejects bulk `PrivateVault`, archive, or `obm1` routes on `onebrain-glasses`.
    pub fn assert_route_permitted(path: &str) -> Result<(), CaptureGateError> {
        let lower = path.trim().to_ascii_lowercase();
        for forbidden in PROHIBITED_BULK_VAULT_PREFIXES {
            if lower == *forbidden || lower.starts_with(&format!("{forbidden}/")) {
                return Err(CaptureGateError::BulkVaultAccessProhibited);
            }
        }
        let allowed = lower == "/api/vnext/glasses/hud-cards"
            || lower == "/api/vnext/glasses/capture"
            || lower == "/api/vnext/glasses/confirm-public";
        if !allowed {
            return Err(CaptureGateError::BulkVaultAccessProhibited);
        }
        Ok(())
    }

    /// Authorizes an outward `AI-003` camera/audio capture iff hardware LED attestation & wear contact pass.
    pub fn authorize_outward_capture(
        req: &OutwardCaptureRequest,
        session: &EphemeralHudSession,
        hud_state: &HudStateMachine,
    ) -> Result<AuthorizedOutwardCapture, CaptureGateError> {
        if req.taint_class == TaintClass::SomaticBiometricRaw {
            return Err(CaptureGateError::SomaticBiometricRawExportProhibited);
        }
        if hud_state.is_locked() || session.is_powered_off() {
            return Err(CaptureGateError::HudSessionLockedOrUnworn);
        }
        let last_worn_ms = hud_state
            .last_contact_ms()
            .ok_or(CaptureGateError::HudSessionLockedOrUnworn)?;
        if req.requested_at_ms < last_worn_ms
            || req.requested_at_ms - last_worn_ms > MAX_DOFF_LOCK_LATENCY_MS
        {
            return Err(CaptureGateError::HudSessionLockedOrUnworn);
        }

        let att = &req.firmware_led_attestation;
        if !att.recording_led_active || !att.led_circuit_closed {
            return Err(CaptureGateError::RecordingIndicatorLedInactive);
        }
        if req.requested_at_ms < att.attested_at_ms
            || req.requested_at_ms - att.attested_at_ms > MAX_DOFF_LOCK_LATENCY_MS
        {
            return Err(CaptureGateError::InvalidFirmwareLedAttestation);
        }

        let expected_mac = session
            .compute_led_attestation_mac(
                att.recording_led_active,
                att.led_circuit_closed,
                att.attested_at_ms,
            )
            .map_err(|_| CaptureGateError::HudSessionLockedOrUnworn)?;
        if att.attestation_mac != expected_mac {
            return Err(CaptureGateError::InvalidFirmwareLedAttestation);
        }

        Ok(AuthorizedOutwardCapture {
            modality: req.modality,
            authorized_at_ms: req.requested_at_ms,
            payload_digest_blake3: req.payload_digest_blake3,
            payload_len: req.payload_len,
            disclosure_badge: "[Private · Local Only]".to_string(),
            recording_led_verified: true,
        })
    }
}
