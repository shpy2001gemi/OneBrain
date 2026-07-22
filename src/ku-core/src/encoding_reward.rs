//! # Encoding Reward — OBT Token Rewards for Encoding Participation
//!
//! Calculates rewards for nodes that participate in encoding verification.
//! Rewards are paid in OBT tokens, proportional to raw text complexity.
//!
//! ## Roles and Reward Multipliers
//! - **Contributor**: Rewarded later through PoMV (not encoding reward)
//! - **FirstEncoder**: Base × 2 + first encoder bonus (encoded first)
//! - **Verifier**: Base × 1 (confirmed/re-encoded)
//! - **Corrector**: Base × 3 (found and fixed encoding errors)
//! - **ProBono**: Base × 2 + pro-bono bonus (helped someone without AI)
//!
//! ## Reference
//! See `docs/specs/ENCODING_CONSENSUS_SPEC.md` §9 for full reward model.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Base OBT reward per 1KB of raw text.
pub const BASE_OBT_PER_KB: u64 = 1;

/// Bonus for the first encoder (SELF status).
pub const FIRST_ENCODER_BONUS: u64 = 5;

/// Bonus for pro-bono encoding (helping someone without AI).
pub const PRO_BONO_BONUS: u64 = 10;

/// Multiplier for correctors (found encoding errors).
pub const CORRECTOR_MULTIPLIER: u64 = 3;

// ═══════════════════════════════════════════════════════════════════════════
// Verifier Role
// ═══════════════════════════════════════════════════════════════════════════

/// Role of a participant in the encoding consensus process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifierRole {
    /// The person who contributed the raw knowledge.
    /// Rewarded through PoMV lifecycle, not encoding rewards.
    Contributor,

    /// The first AI to encode (owner's local AI or first volunteer).
    FirstEncoder,

    /// An AI that verified/confirmed an existing encoding.
    Verifier,

    /// An AI that found errors and submitted a corrected encoding.
    Corrector,

    /// An AI that encoded for someone without AI (pro-bono service).
    ProBono,
}

impl VerifierRole {
    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Contributor => "Contributor",
            Self::FirstEncoder => "First Encoder",
            Self::Verifier => "Verifier",
            Self::Corrector => "Corrector",
            Self::ProBono => "Pro-Bono Encoder",
        }
    }
}

impl std::fmt::Display for VerifierRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Encoding Reward
// ═══════════════════════════════════════════════════════════════════════════

/// Computed reward for a participant in the encoding process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodingReward {
    /// Node ID of the reward recipient.
    pub node_id: u64,

    /// Role in the encoding process.
    pub role: VerifierRole,

    /// Total OBT reward amount.
    pub obt_amount: u64,

    /// Breakdown: base reward (proportional to raw size).
    pub base_reward: u64,

    /// Breakdown: bonus for role.
    pub role_bonus: u64,

    /// Whether this node's encoding was selected as the final one.
    pub encoding_selected: bool,
}

/// Calculate encoding reward for a given role and raw text size.
///
/// Reward is proportional to complexity (measured by raw text size in bytes).
/// Different roles receive different multipliers.
///
/// # Arguments
/// * `raw_size_bytes` — Size of the raw text in bytes
/// * `role` — The participant's role
/// * `encoding_selected` — Whether this node's encoding was chosen as final
pub fn calculate_reward(raw_size_bytes: u32, role: VerifierRole, encoding_selected: bool) -> u64 {
    // Base: 1 OBT per KB (minimum 1 OBT)
    let base = ((raw_size_bytes as u64) / 1024).max(1) * BASE_OBT_PER_KB;

    match role {
        VerifierRole::Contributor => 0, // Rewarded through PoMV, not here

        VerifierRole::FirstEncoder => {
            let bonus = FIRST_ENCODER_BONUS;
            let selection = if encoding_selected { base } else { 0 };
            base * 2 + bonus + selection
        }

        VerifierRole::Verifier => {
            let selection = if encoding_selected { base / 2 } else { 0 };
            base + selection
        }

        VerifierRole::Corrector => {
            // Correctors get higher reward — they found and fixed errors
            base * CORRECTOR_MULTIPLIER
        }

        VerifierRole::ProBono => {
            // Pro-bono encoders help people without AI — community service bonus
            base * 2 + PRO_BONO_BONUS
        }
    }
}

/// Calculate all rewards for a finalized encoding consensus.
///
/// Returns a list of rewards for each participant.
pub fn calculate_all_rewards(
    raw_size_bytes: u32,
    participants: &[(u64, VerifierRole)],
    selected_node_id: Option<u64>,
) -> Vec<EncodingReward> {
    participants
        .iter()
        .map(|&(node_id, role)| {
            let selected = selected_node_id == Some(node_id);
            let obt_amount = calculate_reward(raw_size_bytes, role, selected);
            let base = ((raw_size_bytes as u64) / 1024).max(1) * BASE_OBT_PER_KB;

            EncodingReward {
                node_id,
                role,
                obt_amount,
                base_reward: base,
                role_bonus: obt_amount.saturating_sub(base),
                encoding_selected: selected,
            }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contributor_gets_zero() {
        let reward = calculate_reward(2048, VerifierRole::Contributor, false);
        assert_eq!(reward, 0, "Contributor is rewarded via PoMV, not encoding");
    }

    #[test]
    fn test_first_encoder_reward() {
        // 2KB raw text: base = 2 OBT
        let reward = calculate_reward(2048, VerifierRole::FirstEncoder, false);
        // base*2 + bonus = 2*2 + 5 = 9
        assert_eq!(reward, 9);

        // With encoding selected: +base = 9 + 2 = 11
        let reward_selected = calculate_reward(2048, VerifierRole::FirstEncoder, true);
        assert_eq!(reward_selected, 11);
    }

    #[test]
    fn test_verifier_reward() {
        let reward = calculate_reward(2048, VerifierRole::Verifier, false);
        assert_eq!(reward, 2); // base only

        let reward_selected = calculate_reward(2048, VerifierRole::Verifier, true);
        assert_eq!(reward_selected, 3); // base + base/2
    }

    #[test]
    fn test_corrector_reward() {
        let reward = calculate_reward(2048, VerifierRole::Corrector, false);
        assert_eq!(reward, 6); // base * 3 = 2 * 3
    }

    #[test]
    fn test_pro_bono_reward() {
        let reward = calculate_reward(2048, VerifierRole::ProBono, false);
        assert_eq!(reward, 14); // base*2 + bonus = 2*2 + 10
    }

    #[test]
    fn test_minimum_reward() {
        // Very small raw text (100 bytes) → base = max(0, 1) = 1 OBT
        let reward = calculate_reward(100, VerifierRole::Verifier, false);
        assert_eq!(reward, 1);
    }

    #[test]
    fn test_large_raw_text() {
        // 10KB raw text: base = 10 OBT
        let reward = calculate_reward(10240, VerifierRole::FirstEncoder, true);
        // base*2 + bonus + selection = 10*2 + 5 + 10 = 35
        assert_eq!(reward, 35);
    }

    #[test]
    fn test_calculate_all_rewards() {
        let participants = vec![
            (1, VerifierRole::FirstEncoder),
            (2, VerifierRole::Verifier),
            (3, VerifierRole::Verifier),
        ];
        let rewards = calculate_all_rewards(2048, &participants, Some(1));

        assert_eq!(rewards.len(), 3);
        assert_eq!(rewards[0].node_id, 1);
        assert!(rewards[0].encoding_selected);
        assert!(rewards[0].obt_amount > rewards[1].obt_amount);
    }

    #[test]
    fn test_verifier_role_display() {
        assert_eq!(format!("{}", VerifierRole::ProBono), "Pro-Bono Encoder");
        assert_eq!(format!("{}", VerifierRole::Corrector), "Corrector");
    }
}
