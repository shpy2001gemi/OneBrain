//! Anti-gaming guard for the OneBrain node.
//!
//! Wraps `ku_core::obt_anti_gaming` types to provide a simple API for
//! rate-limiting KU creation and enforcing quality gates before storage.

use ku_core::obt_anti_gaming::{
    gate_1_min_size, AntiGamingPolicy, RateLimitTracker, MIN_GENE_COUNT, MIN_KU_RAW_BYTES,
};
use ku_core::obt_constants::NodeTier;

/// Guard that enforces anti-gaming rules before KU storage.
///
/// Wraps:
/// - **Rate limiting**: Tier-based sliding-window rate control via `RateLimitTracker`.
/// - **Quality gates**: Minimum payload size and instruction count checks.
pub struct AntiGamingGuard {
    rate_tracker: RateLimitTracker,
    node_tier: NodeTier,
    policy: AntiGamingPolicy,
}

impl AntiGamingGuard {
    /// Create a production guard for a Leaf-tier node (epoch 0).
    pub fn new() -> Self {
        Self::with_policy(AntiGamingPolicy::Production)
    }

    /// Create a guard with an explicit local admission policy.
    ///
    /// Development policy can relax local admission only. Consensus, cooldown,
    /// mint, and reward checks remain bound to production constants.
    pub fn with_policy(policy: AntiGamingPolicy) -> Self {
        Self {
            rate_tracker: RateLimitTracker::new(0),
            node_tier: NodeTier::Leaf,
            policy,
        }
    }

    /// Check if we can create a KU right now (rate limiting).
    ///
    /// Returns `Ok(())` if allowed, or `Err(message)` describing
    /// how long until the next slot opens.
    pub fn check_rate_limit(&self) -> Result<(), String> {
        let now = current_unix_secs();
        if self
            .rate_tracker
            .check_ku_rate_with_policy(now, self.node_tier, self.policy)
        {
            Ok(())
        } else {
            // Calculate approximate wait time
            let tier_label = match self.node_tier {
                NodeTier::Leaf => "Leaf",
                NodeTier::Contributor => "Contributor",
                _ => "SuperPeer+",
            };
            let max_per_hour = self
                .policy
                .local_admission_limits(self.node_tier)
                .max_ku_per_hour;
            Err(format!(
                "Rate limited: {} tier allows {} KU/hour. Try again later.",
                tier_label, max_per_hour
            ))
        }
    }

    /// Run quality gates on wire bytes.
    ///
    /// Gate 1: minimum raw byte size (`MIN_KU_RAW_BYTES` = 256) and
    /// minimum instruction count (`MIN_GENE_COUNT` = 2).
    ///
    /// Note: Gates 2-4 (encoding consensus, PoMV, complexity) require
    /// network verification and are not enforced locally.
    pub fn check_quality(&self, wire_bytes: &[u8], instruction_count: usize) -> Result<(), String> {
        if !gate_1_min_size(wire_bytes.len(), instruction_count) {
            let mut reasons = Vec::new();
            if wire_bytes.len() < MIN_KU_RAW_BYTES {
                reasons.push(format!(
                    "minimum {} bytes required (got {})",
                    MIN_KU_RAW_BYTES,
                    wire_bytes.len()
                ));
            }
            if instruction_count < MIN_GENE_COUNT {
                reasons.push(format!(
                    "minimum {} instructions required (got {})",
                    MIN_GENE_COUNT, instruction_count
                ));
            }
            return Err(format!("Gate 1 failed: {}", reasons.join("; ")));
        }
        Ok(())
    }

    /// Record a successful KU creation in the rate tracker.
    pub fn record_creation(&mut self) {
        let now = current_unix_secs();
        self.rate_tracker.record_ku(now);
    }
}

/// Get the current Unix timestamp in seconds.
fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_defaults_to_production_policy() {
        let mut guard = AntiGamingGuard::new();
        assert!(guard.check_rate_limit().is_ok());
        guard.record_creation();
        assert!(guard.check_rate_limit().is_err());
    }

    #[test]
    fn development_guard_is_an_explicit_local_only_override() {
        let mut guard = AntiGamingGuard::with_policy(AntiGamingPolicy::Development);
        guard.record_creation();
        assert!(guard.check_rate_limit().is_ok());
        assert_eq!(
            AntiGamingPolicy::Development
                .economic_limits(NodeTier::Leaf)
                .max_ku_per_hour,
            1
        );
    }
}
