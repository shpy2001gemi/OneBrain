//! OBT Governance Configuration
//!
//! Provides a runtime-configurable alternative to compile-time constants.
//! `GovernanceConfig` holds the ~15 most important governance-adjustable
//! parameters with defaults matching the compile-time constants in `obt_constants.rs`.
//!
//! # Usage
//!
//! ```rust
//! use ku_core::obt_governance::GovernanceConfig;
//!
//! let config = GovernanceConfig::default(); // Uses compile-time defaults
//!
//! // Override individual parameters
//! let mut custom = GovernanceConfig::default();
//! custom.base_emission_per_epoch = 20_000_000;
//! assert!(custom.validate().is_ok());
//! ```
//!
//! # Future Work
//!
//! - Governance voting protocol for parameter changes
//! - On-chain parameter update transactions
//! - Parameter change cooldown periods
//! - Multi-sig approval for critical parameters

use crate::obt_constants::*;
use serde::{Deserialize, Serialize};

/// Runtime-configurable governance parameters.
///
/// These parameters control the economic behavior of the OBT system.
/// Each field defaults to the corresponding compile-time constant from
/// [`obt_constants`](crate::obt_constants).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    // === Epoch & Timing ===
    /// Duration of one OBT epoch in seconds (default: 3,600 = 1 hour).
    ///
    /// Maps to [`OBT_EPOCH_DURATION_S`].
    pub epoch_duration_s: u64,

    // === Emission Parameters ===
    /// Base emission per epoch in milliOBT (default: 10,000,000 = 10,000 OBT).
    ///
    /// Maps to [`BASE_EMISSION_PER_EPOCH`].
    pub base_emission_per_epoch: u64,

    /// Activity multiplier target (nodes). `A(epoch) = min(active_nodes / target, max)`.
    ///
    /// Maps to [`ACTIVITY_MULTIPLIER_TARGET`].
    pub activity_multiplier_target: u64,

    /// Cap on activity multiplier (default: 10.0).
    ///
    /// Maps to [`ACTIVITY_MULTIPLIER_MAX`].
    pub activity_multiplier_max: f64,

    // === Stream Weights (R1–R4 allocation) ===
    /// R1 Owner/PoMV weight (default: 0.40).
    pub weight_r1_owner: f64,

    /// R2 Encoding weight (default: 0.25).
    pub weight_r2_encoding: f64,

    /// R3 Verification weight (default: 0.15).
    pub weight_r3_verification: f64,

    /// R4 Storage weight (default: 0.20).
    pub weight_r4_storage: f64,

    // === Storage Parameters ===
    /// Base storage reward per KU per epoch in OBT (default: 0.001).
    ///
    /// Maps to [`STORAGE_BASE_RATE`].
    pub storage_base_rate: f64,

    /// DHT replication factor / target replica count (default: 20).
    ///
    /// Maps to [`K_TARGET`].
    pub storage_k_target: u32,

    /// Challenge rate — fraction of stored KUs challenged per epoch (default: 0.10).
    ///
    /// Maps to [`CHALLENGE_RATE`].
    pub challenge_rate: f64,

    // === Trust & Decay Parameters ===
    /// Exponential trust decay rate per hour (default: 0.01).
    /// Half-life ≈ 69.3h ≈ 3 days.
    ///
    /// Maps to [`TRUST_DECAY_LAMBDA`].
    pub trust_decay_lambda: f64,

    // === Penalty Parameters ===
    /// Dispute window in hours (default: 48).
    ///
    /// Maps to [`DISPUTE_WINDOW_HOURS`].
    pub dispute_window_hours: u32,

    // === Quality Gate Parameters ===
    /// Minimum KU raw size in bytes for quality gate (default: 256 ≈ 50 words).
    ///
    /// Maps to [`MIN_KU_RAW_SIZE`].
    pub min_ku_raw_size: usize,

    /// Minimum PoMV score after 30 days (default: 0.05).
    ///
    /// Maps to [`MIN_POMV_30D`].
    pub min_pomv_30d: f32,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            epoch_duration_s: OBT_EPOCH_DURATION_S,
            base_emission_per_epoch: BASE_EMISSION_PER_EPOCH,
            activity_multiplier_target: ACTIVITY_MULTIPLIER_TARGET,
            activity_multiplier_max: ACTIVITY_MULTIPLIER_MAX,
            weight_r1_owner: STREAM_WEIGHTS[0],
            weight_r2_encoding: STREAM_WEIGHTS[1],
            weight_r3_verification: STREAM_WEIGHTS[2],
            weight_r4_storage: STREAM_WEIGHTS[3],
            storage_base_rate: STORAGE_BASE_RATE,
            storage_k_target: K_TARGET,
            challenge_rate: CHALLENGE_RATE,
            trust_decay_lambda: TRUST_DECAY_LAMBDA,
            dispute_window_hours: DISPUTE_WINDOW_HOURS,
            min_ku_raw_size: MIN_KU_RAW_SIZE,
            min_pomv_30d: MIN_POMV_30D,
        }
    }
}

impl GovernanceConfig {
    /// Creates a new config with all defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates that all parameters are within acceptable ranges.
    ///
    /// # Checks
    /// - Stream weights R1–R4 must sum to ≈1.0 (tolerance: ±0.001)
    /// - Base emission must be positive
    /// - Activity multiplier max must be positive
    /// - Trust decay lambda must be positive
    /// - Challenge rate must be in \[0.0, 1.0\]
    /// - Epoch duration must be at least 60 seconds
    /// - Min KU raw size must be positive
    /// - PoMV threshold must be non-negative
    pub fn validate(&self) -> Result<(), GovernanceError> {
        // Stream weights must sum to ~1.0
        let weight_sum = self.weight_r1_owner
            + self.weight_r2_encoding
            + self.weight_r3_verification
            + self.weight_r4_storage;
        if (weight_sum - 1.0).abs() > 0.001 {
            return Err(GovernanceError::InvalidWeightSum { sum: weight_sum });
        }

        // Emission must be positive
        if self.base_emission_per_epoch == 0 {
            return Err(GovernanceError::ZeroEmission);
        }

        // Activity multiplier max must be positive
        if self.activity_multiplier_max <= 0.0 {
            return Err(GovernanceError::InvalidActivityFactor);
        }

        // Trust decay must be positive
        if self.trust_decay_lambda <= 0.0 {
            return Err(GovernanceError::InvalidDecayRate);
        }

        // Challenge rate must be in [0.0, 1.0]
        if !(0.0..=1.0).contains(&self.challenge_rate) {
            return Err(GovernanceError::InvalidChallengeRate);
        }

        // Epoch duration must be at least 60 seconds
        if self.epoch_duration_s < 60 {
            return Err(GovernanceError::EpochTooShort);
        }

        // Min KU raw size must be positive
        if self.min_ku_raw_size == 0 {
            return Err(GovernanceError::InvalidMinKuSize);
        }

        // PoMV threshold must be non-negative
        if self.min_pomv_30d < 0.0 {
            return Err(GovernanceError::InvalidPomvThreshold);
        }

        Ok(())
    }

    /// Returns stream weights as an array \[R1, R2, R3, R4\].
    pub fn stream_weights(&self) -> [f64; 4] {
        [
            self.weight_r1_owner,
            self.weight_r2_encoding,
            self.weight_r3_verification,
            self.weight_r4_storage,
        ]
    }

    /// Returns the dispute window converted to seconds.
    pub fn dispute_window_s(&self) -> u64 {
        self.dispute_window_hours as u64 * 3_600
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Error Type
// ═══════════════════════════════════════════════════════════════════════════

/// Errors produced when validating a [`GovernanceConfig`].
#[derive(Debug, Clone, PartialEq)]
pub enum GovernanceError {
    /// Stream weights R1–R4 do not sum to 1.0.
    InvalidWeightSum { sum: f64 },
    /// Base emission per epoch is zero.
    ZeroEmission,
    /// Activity multiplier max is not positive.
    InvalidActivityFactor,
    /// Trust decay lambda is not positive.
    InvalidDecayRate,
    /// Challenge rate is outside \[0.0, 1.0\].
    InvalidChallengeRate,
    /// Epoch duration is less than 60 seconds.
    EpochTooShort,
    /// Minimum KU raw size is zero.
    InvalidMinKuSize,
    /// PoMV 30-day threshold is negative.
    InvalidPomvThreshold,
}

impl std::fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWeightSum { sum } => {
                write!(f, "Stream weights must sum to 1.0, got {sum:.6}")
            }
            Self::ZeroEmission => write!(f, "Base emission per epoch must be positive"),
            Self::InvalidActivityFactor => {
                write!(f, "Activity multiplier max must be positive")
            }
            Self::InvalidDecayRate => write!(f, "Trust decay lambda must be positive"),
            Self::InvalidChallengeRate => {
                write!(f, "Challenge rate must be between 0.0 and 1.0")
            }
            Self::EpochTooShort => {
                write!(f, "Epoch duration must be at least 60 seconds")
            }
            Self::InvalidMinKuSize => write!(f, "Minimum KU raw size must be positive"),
            Self::InvalidPomvThreshold => {
                write!(f, "PoMV 30-day threshold must be non-negative")
            }
        }
    }
}

impl std::error::Error for GovernanceError {}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_valid() {
        let config = GovernanceConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_matches_constants() {
        let config = GovernanceConfig::default();
        assert_eq!(config.base_emission_per_epoch, BASE_EMISSION_PER_EPOCH);
        assert_eq!(config.epoch_duration_s, OBT_EPOCH_DURATION_S);
        assert_eq!(
            config.activity_multiplier_target,
            ACTIVITY_MULTIPLIER_TARGET
        );
        assert!((config.activity_multiplier_max - ACTIVITY_MULTIPLIER_MAX).abs() < f64::EPSILON);
        assert!((config.weight_r1_owner - STREAM_WEIGHTS[0]).abs() < f64::EPSILON);
        assert!((config.weight_r2_encoding - STREAM_WEIGHTS[1]).abs() < f64::EPSILON);
        assert!((config.weight_r3_verification - STREAM_WEIGHTS[2]).abs() < f64::EPSILON);
        assert!((config.weight_r4_storage - STREAM_WEIGHTS[3]).abs() < f64::EPSILON);
        assert!((config.storage_base_rate - STORAGE_BASE_RATE).abs() < f64::EPSILON);
        assert_eq!(config.storage_k_target, K_TARGET);
        assert!((config.challenge_rate - CHALLENGE_RATE).abs() < f64::EPSILON);
        assert!((config.trust_decay_lambda - TRUST_DECAY_LAMBDA).abs() < f64::EPSILON);
        assert_eq!(config.dispute_window_hours, DISPUTE_WINDOW_HOURS);
        assert_eq!(config.min_ku_raw_size, MIN_KU_RAW_SIZE);
        assert!((config.min_pomv_30d - MIN_POMV_30D).abs() < f32::EPSILON);
    }

    #[test]
    fn test_invalid_weight_sum() {
        let config = GovernanceConfig {
            weight_r1_owner: 0.90, // Sum will be 0.90 + 0.25 + 0.15 + 0.20 = 1.50
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(matches!(err, GovernanceError::InvalidWeightSum { .. }));
    }

    #[test]
    fn test_zero_emission_rejected() {
        let config = GovernanceConfig {
            base_emission_per_epoch: 0,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::ZeroEmission);
    }

    #[test]
    fn test_invalid_activity_factor() {
        let config = GovernanceConfig {
            activity_multiplier_max: -1.0,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::InvalidActivityFactor);
    }

    #[test]
    fn test_invalid_decay_rate() {
        let config = GovernanceConfig {
            trust_decay_lambda: 0.0,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::InvalidDecayRate);
    }

    #[test]
    fn test_invalid_challenge_rate() {
        let config = GovernanceConfig {
            challenge_rate: 1.5,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::InvalidChallengeRate);
    }

    #[test]
    fn test_negative_challenge_rate() {
        let config = GovernanceConfig {
            challenge_rate: -0.1,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::InvalidChallengeRate);
    }

    #[test]
    fn test_epoch_too_short() {
        let config = GovernanceConfig {
            epoch_duration_s: 30,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::EpochTooShort);
    }

    #[test]
    fn test_epoch_minimum_boundary() {
        let config = GovernanceConfig {
            epoch_duration_s: 60, // Exactly at minimum
            ..GovernanceConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_stream_weights_array() {
        let config = GovernanceConfig::default();
        let weights = config.stream_weights();
        assert_eq!(weights.len(), 4);
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_dispute_window_s() {
        let config = GovernanceConfig::default();
        assert_eq!(config.dispute_window_s(), 48 * 3_600);
    }

    #[test]
    fn test_custom_config() {
        let config = GovernanceConfig {
            base_emission_per_epoch: 20_000_000,
            challenge_rate: 0.20,
            ..GovernanceConfig::default()
        };
        assert!(config.validate().is_ok());
        assert_eq!(config.base_emission_per_epoch, 20_000_000);
        assert!((config.challenge_rate - 0.20).abs() < f64::EPSILON);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = GovernanceConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GovernanceConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.base_emission_per_epoch,
            deserialized.base_emission_per_epoch
        );
        assert_eq!(config.epoch_duration_s, deserialized.epoch_duration_s);
        assert_eq!(config.storage_k_target, deserialized.storage_k_target);
        assert_eq!(
            config.dispute_window_hours,
            deserialized.dispute_window_hours
        );
        assert_eq!(config.min_ku_raw_size, deserialized.min_ku_raw_size);
        assert!((config.min_pomv_30d - deserialized.min_pomv_30d).abs() < f32::EPSILON);
    }

    #[test]
    fn test_new_equals_default() {
        let from_new = GovernanceConfig::new();
        let from_default = GovernanceConfig::default();
        assert_eq!(
            from_new.base_emission_per_epoch,
            from_default.base_emission_per_epoch
        );
        assert_eq!(from_new.epoch_duration_s, from_default.epoch_duration_s);
    }

    #[test]
    fn test_error_display() {
        let err = GovernanceError::InvalidWeightSum { sum: 1.5 };
        let msg = format!("{err}");
        assert!(msg.contains("1.5"));
        assert!(msg.contains("sum to 1.0"));

        let err = GovernanceError::ZeroEmission;
        assert!(!format!("{err}").is_empty());
    }

    #[test]
    fn test_invalid_min_ku_size() {
        let config = GovernanceConfig {
            min_ku_raw_size: 0,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::InvalidMinKuSize);
    }

    #[test]
    fn test_negative_pomv_threshold() {
        let config = GovernanceConfig {
            min_pomv_30d: -0.1,
            ..GovernanceConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert_eq!(err, GovernanceError::InvalidPomvThreshold);
    }
}
