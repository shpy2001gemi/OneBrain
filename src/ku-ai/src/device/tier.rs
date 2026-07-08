//! # Device Tier Classification
//!
//! Classifies hardware into tiers (T0–T6) based on total available memory
//! for AI workloads. This determines which models can be loaded.

use serde::{Deserialize, Serialize};

/// Hardware capability tier for model selection.
///
/// Tiers range from T0 (mobile/embedded, ≤3 GB usable) to T6 (server, 96+ GB).
/// Classification considers total system RAM minus OS overhead, plus dedicated VRAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DeviceTier {
    /// Mobile/Embedded — ≤3 GB usable.
    T0,
    /// Low-End — 4–7 GB usable.
    T1,
    /// Mid-Range — 8–11 GB usable.
    T2,
    /// Standard — 12–23 GB usable.
    T3,
    /// High-End — 24–47 GB usable.
    T4,
    /// Workstation — 48–95 GB usable.
    T5,
    /// Server — 96+ GB usable.
    T6,
}

impl DeviceTier {
    /// Classify hardware into a device tier.
    ///
    /// Subtracts a 2 GB OS overhead estimate from total RAM, then adds
    /// any dedicated VRAM to compute the total AI-usable memory.
    ///
    /// # Arguments
    /// * `total_ram_bytes` — Total system RAM in bytes.
    /// * `vram_bytes` — Dedicated GPU VRAM in bytes, if any.
    pub fn classify(total_ram_bytes: u64, vram_bytes: Option<u64>) -> Self {
        let os_overhead = 2u64 * 1024 * 1024 * 1024; // 2 GB
        let usable = total_ram_bytes.saturating_sub(os_overhead);
        let total_ai = usable + vram_bytes.unwrap_or(0);
        let gb = total_ai / (1024 * 1024 * 1024);

        match gb {
            0..=3 => Self::T0,
            4..=7 => Self::T1,
            8..=11 => Self::T2,
            12..=23 => Self::T3,
            24..=47 => Self::T4,
            48..=95 => Self::T5,
            _ => Self::T6,
        }
    }

    /// Human-readable display name for this tier.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::T0 => "Mobile/Embedded",
            Self::T1 => "Low-End",
            Self::T2 => "Mid-Range",
            Self::T3 => "Standard",
            Self::T4 => "High-End",
            Self::T5 => "Workstation",
            Self::T6 => "Server",
        }
    }
}

impl std::fmt::Display for DeviceTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} ({})", self, self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn test_tier_4gb_system() {
        // 4 GB total - 2 GB overhead = 2 GB usable → T0
        let tier = DeviceTier::classify(4 * GB, None);
        assert_eq!(tier, DeviceTier::T0);
    }

    #[test]
    fn test_tier_8gb_system() {
        // 8 GB total - 2 GB overhead = 6 GB usable → T1
        let tier = DeviceTier::classify(8 * GB, None);
        assert_eq!(tier, DeviceTier::T1);
    }

    #[test]
    fn test_tier_16gb_system() {
        // 16 GB total - 2 GB overhead = 14 GB usable → T3
        let tier = DeviceTier::classify(16 * GB, None);
        assert_eq!(tier, DeviceTier::T3);
    }

    #[test]
    fn test_tier_32gb_system() {
        // 32 GB total - 2 GB overhead = 30 GB usable → T4
        let tier = DeviceTier::classify(32 * GB, None);
        assert_eq!(tier, DeviceTier::T4);
    }

    #[test]
    fn test_tier_128gb_server() {
        // 128 GB total - 2 GB overhead = 126 GB usable → T6
        let tier = DeviceTier::classify(128 * GB, None);
        assert_eq!(tier, DeviceTier::T6);
    }

    #[test]
    fn test_tier_with_vram() {
        // 8 GB RAM - 2 GB overhead = 6 GB + 8 GB VRAM = 14 GB → T3
        let tier = DeviceTier::classify(8 * GB, Some(8 * GB));
        assert_eq!(tier, DeviceTier::T3);
    }

    #[test]
    fn test_tier_ordering() {
        assert!(DeviceTier::T0 < DeviceTier::T1);
        assert!(DeviceTier::T1 < DeviceTier::T2);
        assert!(DeviceTier::T2 < DeviceTier::T3);
        assert!(DeviceTier::T3 < DeviceTier::T4);
        assert!(DeviceTier::T4 < DeviceTier::T5);
        assert!(DeviceTier::T5 < DeviceTier::T6);
    }

    #[test]
    fn test_tier_display_names() {
        assert_eq!(DeviceTier::T0.display_name(), "Mobile/Embedded");
        assert_eq!(DeviceTier::T3.display_name(), "Standard");
        assert_eq!(DeviceTier::T6.display_name(), "Server");
    }

    #[test]
    fn test_tier_serde_roundtrip() {
        let tier = DeviceTier::T3;
        let json = serde_json::to_string(&tier).unwrap();
        let back: DeviceTier = serde_json::from_str(&json).unwrap();
        assert_eq!(back, DeviceTier::T3);
    }

    #[test]
    fn test_tier_zero_ram() {
        let tier = DeviceTier::classify(0, None);
        assert_eq!(tier, DeviceTier::T0);
    }

    #[test]
    fn test_tier_boundary_48gb() {
        // 50 GB total - 2 GB overhead = 48 GB → T5
        let tier = DeviceTier::classify(50 * GB, None);
        assert_eq!(tier, DeviceTier::T5);
    }
}
