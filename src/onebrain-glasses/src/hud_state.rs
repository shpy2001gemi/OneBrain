//! `<= 40 ms` Doff-Lock, Tier M0 RAM Zeroization, and `BciHalSafetyGate` (`P10 §4.3`, `P9 §2/§4`).

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::capture_gate::assert_canonical_hud_badge;

/// Maximum permitted wear-loss to optical blank + session lock latency (`P10 §4.3`, `P9 §4.2`).
pub const MAX_DOFF_LOCK_LATENCY_MS: u64 = 40;

/// Continuous skin-contact / optical-proximity heartbeat cadence (`P9 §4.2`).
pub const HEARTBEAT_INTERVAL_MS: u64 = 20;

/// Named somatic revocation freshness floor (`P9 §4`).
pub const SOMATIC_IMMEDIATE_PROFILE_V1: &str = "SomaticImmediate/1";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HudError {
    #[error("HUD companion session is locked; re-wearing + host unlock required")]
    SessionLocked,
    #[error("legacy or non-canonical epistemic badge prohibited: {0}")]
    InvalidEpistemicBadge(String),
    #[error("CAPABILITY_PROHIBITED (NEG-BCI-001): Plane C arbitrary neural write is forbidden")]
    ArbitraryNeuralWriteProhibited,
    #[error("HAL_SAFETY_ENVELOPE_EXCEEDED: {0}")]
    HalSafetyEnvelopeExceeded(&'static str),
}

/// Three-plane somatic effect classes (`P9 §2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SomaticPlaneEffect {
    /// Plane A: `EFFECT_NEURAL_READ_INTENT`
    NeuralReadIntent,
    /// Plane B: `EFFECT_SENSORY_FEEDBACK`
    SensoryFeedback,
    /// Plane C: `EFFECT_ARBITRARY_NEURAL_WRITE` (PROHIBITED in v1 — `NEG-BCI-001`)
    ArbitraryNeuralWrite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensoryCueKind {
    HapticPulse,
    TactilePattern,
    BoneConductionTone,
    PeripheralVisualCue,
}

/// Bounded Plane B sensory actuator frame (`P9 §2.2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensoryFeedbackFrameV1 {
    pub cue_kind: SensoryCueKind,
    pub intensity_permille: u16,
    pub frequency_milli_hz: u32,
    pub pulse_width_us: u32,
    pub duration_ms: u16,
    pub min_inter_frame_cooldown_ms: u16,
}

/// Hardware-enforced Plane B safety gate (`P9 §2.2`).
#[derive(Clone, Debug, Default)]
pub struct BciHalSafetyGate {
    last_frame_end_ms: Option<u64>,
    window_start_ms: u64,
    cumulative_active_ms_in_window: u64,
}

impl BciHalSafetyGate {
    pub const MAX_INTENSITY_PERMILLE: u16 = 1_000;
    pub const MAX_DURATION_MS: u16 = 2_000;
    pub const MIN_COOLDOWN_MS: u16 = 100;
    pub const MAX_DUTY_CYCLE_PERMILLE_60S: u64 = 250; // <= 25% of 60_000 ms = 15_000 ms

    pub fn validate_and_record(
        &mut self,
        effect: SomaticPlaneEffect,
        frame: &SensoryFeedbackFrameV1,
        now_ms: u64,
    ) -> Result<(), HudError> {
        if effect == SomaticPlaneEffect::ArbitraryNeuralWrite {
            return Err(HudError::ArbitraryNeuralWriteProhibited);
        }
        if frame.intensity_permille > Self::MAX_INTENSITY_PERMILLE {
            return Err(HudError::HalSafetyEnvelopeExceeded(
                "intensity_permille > 1000",
            ));
        }
        if frame.duration_ms == 0 || frame.duration_ms > Self::MAX_DURATION_MS {
            return Err(HudError::HalSafetyEnvelopeExceeded(
                "duration_ms outside 1..=2000 ms",
            ));
        }
        if frame.min_inter_frame_cooldown_ms < Self::MIN_COOLDOWN_MS {
            return Err(HudError::HalSafetyEnvelopeExceeded(
                "min_inter_frame_cooldown_ms < 100 ms",
            ));
        }
        if let Some(prev_end) = self.last_frame_end_ms {
            if now_ms < prev_end.saturating_add(u64::from(frame.min_inter_frame_cooldown_ms)) {
                return Err(HudError::HalSafetyEnvelopeExceeded(
                    "inter-frame cooldown violated",
                ));
            }
        }
        if now_ms.saturating_sub(self.window_start_ms) >= 60_000 {
            self.window_start_ms = now_ms;
            self.cumulative_active_ms_in_window = 0;
        }
        let max_active_ms = (60_000 * Self::MAX_DUTY_CYCLE_PERMILLE_60S) / 1_000;
        let next_cumulative = self
            .cumulative_active_ms_in_window
            .saturating_add(u64::from(frame.duration_ms));
        if next_cumulative > max_active_ms {
            return Err(HudError::HalSafetyEnvelopeExceeded(
                "60s 25% thermal/coercive duty-cycle budget exceeded",
            ));
        }
        self.cumulative_active_ms_in_window = next_cumulative;
        self.last_frame_end_ms = Some(now_ms.saturating_add(u64::from(frame.duration_ms)));
        Ok(())
    }

    pub fn zero_outputs(&mut self) {
        self.last_frame_end_ms = None;
        self.cumulative_active_ms_in_window = 0;
    }
}

/// Monotonic wear-sensor / proximity heartbeat reading (`P9 §4.2`, `T-BCI-05`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WearContactReading {
    pub seq: u64,
    pub monotonic_ms: u64,
    pub worn_contact_active: bool,
    pub hardware_nonce: [u8; 16],
}

/// Optical waveguide display state (`P10 §4.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpticalHudRenderState {
    BlankedLocked,
    Active { card_count: usize },
}

/// Ephemeral Tier M0 context card cached exclusively in volatile RAM.
pub struct TierM0ContextCard {
    card_id: String,
    title_bytes: Zeroizing<Vec<u8>>,
    summary_bytes: Zeroizing<Vec<u8>>,
    epistemic_badge: String,
    provenance_cid: String,
}

impl TierM0ContextCard {
    pub fn new(
        card_id: impl Into<String>,
        title: impl AsRef<[u8]>,
        summary: impl AsRef<[u8]>,
        epistemic_badge: impl Into<String>,
        provenance_cid: impl Into<String>,
    ) -> Result<Self, HudError> {
        let badge = epistemic_badge.into();
        assert_canonical_hud_badge(&badge)
            .map_err(|_| HudError::InvalidEpistemicBadge(badge.clone()))?;
        Ok(Self {
            card_id: card_id.into(),
            title_bytes: Zeroizing::new(title.as_ref().to_vec()),
            summary_bytes: Zeroizing::new(summary.as_ref().to_vec()),
            epistemic_badge: badge,
            provenance_cid: provenance_cid.into(),
        })
    }

    pub fn card_id(&self) -> &str {
        &self.card_id
    }

    pub fn title_bytes(&self) -> &[u8] {
        &self.title_bytes
    }

    pub fn summary_bytes(&self) -> &[u8] {
        &self.summary_bytes
    }

    pub fn epistemic_badge(&self) -> &str {
        &self.epistemic_badge
    }

    pub fn provenance_cid(&self) -> &str {
        &self.provenance_cid
    }

    pub fn sensitive_byte_len(&self) -> usize {
        self.title_bytes.len() + self.summary_bytes.len()
    }

    /// Explicitly overwrites every byte of `title_bytes` and `summary_bytes` with `0x00` in place.
    pub fn zeroize_in_place(&mut self) {
        self.title_bytes.as_mut_slice().zeroize();
        self.summary_bytes.as_mut_slice().zeroize();
        self.card_id.zeroize();
        self.provenance_cid.zeroize();
    }
}

impl Zeroize for TierM0ContextCard {
    fn zeroize(&mut self) {
        self.zeroize_in_place();
        self.title_bytes.zeroize();
        self.summary_bytes.zeroize();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SomaticAbortReason {
    WearContactLost,
    HeartbeatTimeoutExceeded,
    SensorReplayOrClockRegression,
    HardwareKillSwitch,
    UserSomaticAbortGesture,
    ValidatedErrpVeto,
    LeaseExpired,
}

/// Durable-ready abort tombstone MACed under `SomaticImmediate/1` (`P9 §4.3`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SomaticAbortTombstone {
    pub profile: String,
    pub session_id: [u8; 32],
    pub abort_seq: u64,
    pub triggered_at_ms: u64,
    pub elapsed_since_last_contact_ms: u64,
    pub reason: SomaticAbortReason,
    pub zeroized_card_count: usize,
    pub zeroized_tier_m0_bytes: usize,
    pub plane_b_outputs_zeroed: bool,
    pub mac_blake3: [u8; 32],
}

impl SomaticAbortTombstone {
    pub fn verify_mac(&self, somatic_mac_key: &[u8; 32]) -> bool {
        let expected = compute_tombstone_mac(
            somatic_mac_key,
            &self.session_id,
            self.abort_seq,
            self.triggered_at_ms,
            self.elapsed_since_last_contact_ms,
            self.reason,
            self.zeroized_card_count,
            self.zeroized_tier_m0_bytes,
        );
        self.mac_blake3 == expected
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_tombstone_mac(
    somatic_mac_key: &[u8; 32],
    session_id: &[u8; 32],
    abort_seq: u64,
    triggered_at_ms: u64,
    elapsed_ms: u64,
    reason: SomaticAbortReason,
    zeroized_cards: usize,
    zeroized_bytes: usize,
) -> [u8; 32] {
    let mut h = blake3::Hasher::new_keyed(somatic_mac_key);
    h.update(b"onebrain:somatic-abort-tombstone:v1");
    h.update(SOMATIC_IMMEDIATE_PROFILE_V1.as_bytes());
    h.update(session_id);
    h.update(&abort_seq.to_be_bytes());
    h.update(&triggered_at_ms.to_be_bytes());
    h.update(&elapsed_ms.to_be_bytes());
    h.update(&[reason as u8]);
    h.update(&(zeroized_cards as u64).to_be_bytes());
    h.update(&(zeroized_bytes as u64).to_be_bytes());
    *h.finalize().as_bytes()
}

/// Real-time HUD state machine enforcing `<= 40 ms` doff-lock and Tier M0 RAM zeroization.
pub struct HudStateMachine {
    session_id: [u8; 32],
    lease_expires_at_ms: u64,
    render_state: OpticalHudRenderState,
    locked: bool,
    last_contact_ms: Option<u64>,
    last_sensor_seq: Option<u64>,
    tier_m0_cards: Vec<TierM0ContextCard>,
    hal_gate: BciHalSafetyGate,
    plane_b_outputs_zeroed: bool,
    abort_seq: u64,
}

impl HudStateMachine {
    pub fn new_paired(
        session_id: [u8; 32],
        initial_worn_ms: u64,
        lease_expires_at_ms: u64,
    ) -> Self {
        Self {
            session_id,
            lease_expires_at_ms,
            render_state: OpticalHudRenderState::Active { card_count: 0 },
            locked: false,
            last_contact_ms: Some(initial_worn_ms),
            last_sensor_seq: None,
            tier_m0_cards: Vec::new(),
            hal_gate: BciHalSafetyGate::default(),
            plane_b_outputs_zeroed: false,
            abort_seq: 0,
        }
    }

    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn render_state(&self) -> &OpticalHudRenderState {
        &self.render_state
    }

    pub fn active_cards(&self) -> &[TierM0ContextCard] {
        &self.tier_m0_cards
    }

    pub const fn last_contact_ms(&self) -> Option<u64> {
        self.last_contact_ms
    }

    pub const fn plane_b_outputs_zeroed(&self) -> bool {
        self.plane_b_outputs_zeroed
    }

    /// verifies live wear continuity at `now_ms`; if stale (`> 40 ms`) or expired, trips doff-lock.
    pub fn evaluate_watchdog(
        &mut self,
        now_ms: u64,
        somatic_mac_key: &[u8; 32],
    ) -> Option<SomaticAbortTombstone> {
        if self.locked {
            return None;
        }
        if now_ms >= self.lease_expires_at_ms {
            return Some(self.trip_synchronous_cutoff(
                now_ms,
                SomaticAbortReason::LeaseExpired,
                somatic_mac_key,
            ));
        }
        if let Some(last_ms) = self.last_contact_ms {
            if now_ms.saturating_sub(last_ms) > MAX_DOFF_LOCK_LATENCY_MS {
                return Some(self.trip_synchronous_cutoff(
                    now_ms,
                    SomaticAbortReason::HeartbeatTimeoutExceeded,
                    somatic_mac_key,
                ));
            }
        } else {
            return Some(self.trip_synchronous_cutoff(
                now_ms,
                SomaticAbortReason::WearContactLost,
                somatic_mac_key,
            ));
        }
        None
    }

    /// Processes a `<= 20 ms` wear-sensor heartbeat (`WearContactReading`).
    pub fn ingest_wear_heartbeat(
        &mut self,
        reading: WearContactReading,
        somatic_mac_key: &[u8; 32],
    ) -> Option<SomaticAbortTombstone> {
        if let Some(prev_seq) = self.last_sensor_seq {
            if reading.seq <= prev_seq {
                return Some(self.trip_synchronous_cutoff(
                    reading.monotonic_ms,
                    SomaticAbortReason::SensorReplayOrClockRegression,
                    somatic_mac_key,
                ));
            }
        }
        if let Some(last_ms) = self.last_contact_ms {
            if reading.monotonic_ms < last_ms {
                return Some(self.trip_synchronous_cutoff(
                    last_ms,
                    SomaticAbortReason::SensorReplayOrClockRegression,
                    somatic_mac_key,
                ));
            }
            if reading.monotonic_ms - last_ms > MAX_DOFF_LOCK_LATENCY_MS {
                return Some(self.trip_synchronous_cutoff(
                    reading.monotonic_ms,
                    SomaticAbortReason::HeartbeatTimeoutExceeded,
                    somatic_mac_key,
                ));
            }
        }
        if reading.monotonic_ms >= self.lease_expires_at_ms {
            return Some(self.trip_synchronous_cutoff(
                reading.monotonic_ms,
                SomaticAbortReason::LeaseExpired,
                somatic_mac_key,
            ));
        }
        if !reading.worn_contact_active {
            return Some(self.trip_synchronous_cutoff(
                reading.monotonic_ms,
                SomaticAbortReason::WearContactLost,
                somatic_mac_key,
            ));
        }

        if self.locked {
            return None;
        }
        self.last_sensor_seq = Some(reading.seq);
        self.last_contact_ms = Some(reading.monotonic_ms);
        None
    }

    /// Loads a Tier M0 context card onto the optical HUD while worn and unlocked.
    pub fn push_context_card(&mut self, card: TierM0ContextCard) -> Result<(), HudError> {
        if self.locked {
            return Err(HudError::SessionLocked);
        }
        self.tier_m0_cards.push(card);
        self.render_state = OpticalHudRenderState::Active {
            card_count: self.tier_m0_cards.len(),
        };
        Ok(())
    }

    /// Emits bounded Plane B sensory cue through `BciHalSafetyGate`, rejecting Plane C fail-closed.
    pub fn emit_sensory_feedback(
        &mut self,
        effect: SomaticPlaneEffect,
        frame: &SensoryFeedbackFrameV1,
        now_ms: u64,
    ) -> Result<(), HudError> {
        if self.locked {
            return Err(HudError::SessionLocked);
        }
        self.hal_gate.validate_and_record(effect, frame, now_ms)
    }

    /// Executes synchronous (`0 ms` software scheduling delay) cut-off (`P9 §4.3`, `P10 §4.3`).
    pub fn trip_synchronous_cutoff(
        &mut self,
        triggered_at_ms: u64,
        reason: SomaticAbortReason,
        somatic_mac_key: &[u8; 32],
    ) -> SomaticAbortTombstone {
        let elapsed_ms = self
            .last_contact_ms
            .map_or(0, |last| triggered_at_ms.saturating_sub(last));

        let zeroized_cards = self.tier_m0_cards.len();
        let mut zeroized_bytes = 0usize;
        for card in &mut self.tier_m0_cards {
            zeroized_bytes += card.sensitive_byte_len();
            card.zeroize_in_place();
        }
        self.tier_m0_cards.clear();

        self.render_state = OpticalHudRenderState::BlankedLocked;
        self.locked = true;
        self.hal_gate.zero_outputs();
        self.plane_b_outputs_zeroed = true;

        self.abort_seq = self.abort_seq.saturating_add(1);
        let mac_blake3 = compute_tombstone_mac(
            somatic_mac_key,
            &self.session_id,
            self.abort_seq,
            triggered_at_ms,
            elapsed_ms,
            reason,
            zeroized_cards,
            zeroized_bytes,
        );

        SomaticAbortTombstone {
            profile: SOMATIC_IMMEDIATE_PROFILE_V1.to_string(),
            session_id: self.session_id,
            abort_seq: self.abort_seq,
            triggered_at_ms,
            elapsed_since_last_contact_ms: elapsed_ms,
            reason,
            zeroized_card_count: zeroized_cards,
            zeroized_tier_m0_bytes: zeroized_bytes,
            plane_b_outputs_zeroed: true,
            mac_blake3,
        }
    }
}
