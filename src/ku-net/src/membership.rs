//! # SWIM Membership Protocol & Node Fitness — SPEC B §1-2
//!
//! SWIM-based membership with Lifeguard extensions, 7-tier node model,
//! and fitness scoring.

use crate::identity::NodeId;
use crate::messages::NetworkAddress;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ─── Constants (SPEC B §1.2) ──────────────────────────────────────────────

/// Protocol period between probe rounds (ms).
pub const T_PERIOD_MS: u64 = 1000;
/// Direct probe timeout (ms).
pub const T_DIRECT_MS: u64 = 200;
/// Indirect probe timeout (ms).
pub const T_INDIRECT_MS: u64 = 500;
/// Number of peers for indirect probing.
pub const K_INDIRECT: usize = 3;
/// Base suspect timeout before marking Dead (ms).
pub const T_SUSPECT_BASE_MS: u64 = 5000;
/// Maximum piggybacked updates per message.
pub const MAX_PIGGYBACK: usize = 6;
/// Maximum entries in local membership list.
pub const MAX_MEMBERS: usize = 10_000;
/// Maximum Local Health Awareness scaling factor.
pub const LHA_MAX: u32 = 8;

// ─── Node Tier Model (SPEC B §2.4) ───────────────────────────────────────

/// 7-tier node classification based on capabilities and fitness.
///
/// SPEC B §2.4:
/// - T0 Leaf: Minimal participation (2-3 connections)
/// - T1 Contributor: Active participant (8-15 connections)
/// - T2 Local SP: Local super-peer (50-200 connections)
/// - T3 Regional SP: Regional coordinator
/// - T4 Country SP: National-level hub
/// - T5 Continental SP: Continental coordinator
/// - T6 Global Backbone: Planetary-scale root
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum NodeTier {
    Leaf = 0,
    Contributor = 1,
    LocalSP = 2,
    RegionalSP = 3,
    CountrySP = 4,
    ContinentalSP = 5,
    GlobalBackbone = 6,
}

impl NodeTier {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Leaf),
            1 => Some(Self::Contributor),
            2 => Some(Self::LocalSP),
            3 => Some(Self::RegionalSP),
            4 => Some(Self::CountrySP),
            5 => Some(Self::ContinentalSP),
            6 => Some(Self::GlobalBackbone),
            _ => None,
        }
    }

    /// Minimum fitness score to enter this tier (SPEC B §2.5).
    pub fn promotion_threshold(&self) -> f32 {
        match self {
            Self::Leaf => 0.0,
            Self::Contributor => 0.3,
            Self::LocalSP => 0.6,
            Self::RegionalSP => 0.75,
            Self::CountrySP => 0.85,
            Self::ContinentalSP => 0.92,
            Self::GlobalBackbone => 0.97,
        }
    }

    /// Demotion threshold (with hysteresis — SPEC B §2.5).
    pub fn demotion_threshold(&self) -> f32 {
        match self {
            Self::Leaf => 0.0, // Can't demote below Leaf
            Self::Contributor => 0.2,
            Self::LocalSP => 0.5,
            Self::RegionalSP => 0.65,
            Self::CountrySP => 0.78,
            Self::ContinentalSP => 0.87,
            Self::GlobalBackbone => 0.93,
        }
    }

    /// Convert from OBT tier number (u8).
    /// OBT tiers: 0=Leaf, 1=Contributor, 2=LocalSP, 3=RegionalSP,
    /// 4=CountrySP, 5=ContinentalSP, 6=GlobalBackbone
    pub fn from_obt_tier_u8(tier: u8) -> Self {
        match tier {
            0 => Self::Leaf,
            1 => Self::Contributor,
            2 => Self::LocalSP,
            3 => Self::RegionalSP,
            4 => Self::CountrySP,
            5 => Self::ContinentalSP,
            _ => Self::GlobalBackbone,
        }
    }

    /// Convert to OBT tier number (u8).
    pub fn to_obt_tier_u8(&self) -> u8 {
        match self {
            Self::Leaf => 0,
            Self::Contributor => 1,
            Self::LocalSP => 2,
            Self::RegionalSP => 3,
            Self::CountrySP => 4,
            Self::ContinentalSP => 5,
            Self::GlobalBackbone => 6,
        }
    }
}

// ─── Member Status (SPEC B §1.3) ─────────────────────────────────────────

/// Membership status of a remote node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberStatus {
    /// Node is confirmed alive.
    Alive,
    /// Node is suspected dead, pending confirmation.
    Suspect { since: Instant },
    /// Node confirmed dead.
    Dead { since: Instant },
    /// Node gracefully departed.
    Left,
}

impl MemberStatus {
    /// Encode to wire format byte.
    pub fn to_wire(&self) -> u8 {
        match self {
            Self::Alive => 0,
            Self::Suspect { .. } => 1,
            Self::Dead { .. } => 2,
            Self::Left => 3,
        }
    }

    /// Decode from wire format byte.
    pub fn from_wire(val: u8) -> Self {
        match val {
            0 => Self::Alive,
            1 => Self::Suspect {
                since: Instant::now(),
            },
            2 => Self::Dead {
                since: Instant::now(),
            },
            3 => Self::Left,
            _ => Self::Dead {
                since: Instant::now(),
            },
        }
    }
}

// ─── Member Entry (SPEC B §1.3) ──────────────────────────────────────────

/// A single entry in the local membership list.
#[derive(Debug, Clone)]
pub struct MemberEntry {
    /// 32-byte node identifier (BLAKE3).
    pub node_id: NodeId,
    /// Network address (IPv4 or IPv6).
    pub address: NetworkAddress,
    /// Incarnation number (monotonically increasing for refutation).
    pub incarnation: u32,
    /// Current membership status.
    pub status: MemberStatus,
    /// Node's tier classification.
    pub tier: NodeTier,
    /// Last confirmed contact time.
    pub last_seen: Instant,
    /// Fitness score (0.0–1.0).
    pub fitness_score: f32,
    /// 128-bit topic interest fingerprint.
    pub topic_vector: [u8; 16],
}

// ─── Fitness Score (SPEC B §2.3) ──────────────────────────────────────────

/// Components for calculating node fitness score.
///
/// SPEC B §2.3: Weighted formula combining 7 factors.
/// `fitness = Σ(w_i × normalize(component_i))`
#[derive(Debug, Clone)]
pub struct FitnessComponents {
    /// Uptime fraction (0.0–1.0). Weight: 0.20
    pub uptime: f32,
    /// Battery level (0.0–1.0) or 1.0 if plugged in. Weight: 0.15
    pub battery: f32,
    /// Bandwidth capacity normalized (0.0–1.0). Weight: 0.15
    pub bandwidth: f32,
    /// Storage available normalized (0.0–1.0). Weight: 0.10
    pub storage: f32,
    /// CPU headroom (0.0–1.0). Weight: 0.10
    pub cpu: f32,
    /// Network type quality (0.0–1.0): WiFi=1.0, 5G=0.8, 4G=0.5, 3G=0.2. Weight: 0.15
    pub network_quality: f32,
    /// Reputation/trust from EigenTrust (0.0–1.0). Weight: 0.15
    pub reputation: f32,
}

impl FitnessComponents {
    /// Calculate weighted fitness score (SPEC B §2.3).
    pub fn score(&self) -> f32 {
        let s = 0.20 * self.uptime
            + 0.15 * self.battery
            + 0.15 * self.bandwidth
            + 0.10 * self.storage
            + 0.10 * self.cpu
            + 0.15 * self.network_quality
            + 0.15 * self.reputation;
        s.clamp(0.0, 1.0)
    }

    /// Map OBT penalty tier to fitness multiplier.
    /// Tier 0 (None): 1.0, Tier 1 (Warning): 0.9, Tier 2 (TrustReduction): 0.5,
    /// Tier 3 (Jail): 0.1, Tier 4 (Tombstone): 0.0
    pub fn obt_penalty_to_factor(penalty_tier: u8) -> f32 {
        match penalty_tier {
            0 => 1.0,
            1 => 0.9,
            2 => 0.5,
            3 => 0.1,
            _ => 0.0,
        }
    }

    /// Determine appropriate tier for this fitness level.
    pub fn recommended_tier(&self) -> NodeTier {
        let s = self.score();
        if s >= NodeTier::GlobalBackbone.promotion_threshold() {
            NodeTier::GlobalBackbone
        } else if s >= NodeTier::ContinentalSP.promotion_threshold() {
            NodeTier::ContinentalSP
        } else if s >= NodeTier::CountrySP.promotion_threshold() {
            NodeTier::CountrySP
        } else if s >= NodeTier::RegionalSP.promotion_threshold() {
            NodeTier::RegionalSP
        } else if s >= NodeTier::LocalSP.promotion_threshold() {
            NodeTier::LocalSP
        } else if s >= NodeTier::Contributor.promotion_threshold() {
            NodeTier::Contributor
        } else {
            NodeTier::Leaf
        }
    }
}

// ─── Membership State ────────────────────────────────────────────────────

/// Local SWIM membership state (SPEC B §1.3).
///
/// Each node maintains a LOCAL membership list of its cluster
/// (~5K–10K entries), NOT a global view.
pub struct MembershipState {
    /// Our own node ID.
    pub my_id: NodeId,
    /// Our current incarnation number.
    pub my_incarnation: u32,
    /// Our current tier.
    pub my_tier: NodeTier,
    /// Our current fitness components.
    pub my_fitness: FitnessComponents,
    /// Local Health Awareness multiplier (Lifeguard extension).
    pub lha_multiplier: u32,
    /// Membership list: node_id -> entry.
    members: HashMap<NodeId, MemberEntry>,
    /// Pending probes awaiting response (used in Phase 6: full SWIM probe cycle).
    #[allow(dead_code)]
    pending_probes: Vec<PendingProbe>,
}

/// A probe awaiting response in the SWIM cycle.
#[derive(Debug)]
#[allow(dead_code)]
struct PendingProbe {
    target: NodeId,
    probe_type: ProbeType,
    sent_at: Instant,
}

#[derive(Debug)]
#[allow(dead_code)]
enum ProbeType {
    Direct,
    Indirect { via: NodeId },
}

impl MembershipState {
    /// Create new membership state for a fresh node.
    pub fn new(my_id: NodeId, fitness: FitnessComponents) -> Self {
        let tier = fitness.recommended_tier();
        MembershipState {
            my_id,
            my_incarnation: 0,
            my_tier: tier,
            my_fitness: fitness,
            lha_multiplier: 1,
            members: HashMap::new(),
            pending_probes: Vec::new(),
        }
    }

    /// Number of known members.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Add or update a member.
    pub fn upsert_member(&mut self, entry: MemberEntry) {
        // Enforce MAX_MEMBERS cap
        if self.members.len() >= MAX_MEMBERS && !self.members.contains_key(&entry.node_id) {
            // Evict oldest Dead member
            let oldest_dead = self
                .members
                .iter()
                .filter(|(_, e)| matches!(e.status, MemberStatus::Dead { .. }))
                .min_by_key(|(_, e)| e.last_seen)
                .map(|(id, _)| *id);
            if let Some(id) = oldest_dead {
                self.members.remove(&id);
            } else {
                return; // Can't add: at capacity with no dead members to evict
            }
        }
        self.members.insert(entry.node_id, entry);
    }

    /// Get a member by NodeId.
    pub fn get_member(&self, id: &NodeId) -> Option<&MemberEntry> {
        self.members.get(id)
    }

    /// Get all alive members.
    pub fn alive_members(&self) -> Vec<&MemberEntry> {
        self.members
            .values()
            .filter(|e| matches!(e.status, MemberStatus::Alive))
            .collect()
    }

    /// Process a received SWIM PING (SPEC B §1.5 Algorithm 1).
    ///
    /// Returns updates to piggyback on the ACK.
    pub fn handle_ping(&mut self, sender: &NodeId) -> Vec<MemberEntry> {
        // Mark sender as alive
        if let Some(entry) = self.members.get_mut(sender) {
            entry.status = MemberStatus::Alive;
            entry.last_seen = Instant::now();
        }
        // Return recent updates for piggybacking
        self.recent_updates(MAX_PIGGYBACK)
    }

    /// Mark a node as suspect (SPEC B §1.4 State Machine).
    pub fn mark_suspect(&mut self, node_id: &NodeId) {
        if let Some(entry) = self.members.get_mut(node_id) {
            if matches!(entry.status, MemberStatus::Alive) {
                entry.status = MemberStatus::Suspect {
                    since: Instant::now(),
                };
            }
        }
    }

    /// Mark a node as dead (SPEC B §1.4 State Machine).
    pub fn mark_dead(&mut self, node_id: &NodeId) {
        if let Some(entry) = self.members.get_mut(node_id) {
            entry.status = MemberStatus::Dead {
                since: Instant::now(),
            };
        }
    }

    /// Mark a node as gracefully departed.
    pub fn mark_left(&mut self, node_id: &NodeId) {
        if let Some(entry) = self.members.get_mut(node_id) {
            entry.status = MemberStatus::Left;
        }
    }

    /// Refute a suspicion about ourselves by incrementing incarnation.
    pub fn refute_suspicion(&mut self) {
        self.my_incarnation += 1;
    }

    /// Check suspect timeouts and transition to Dead (SPEC B §1.4).
    ///
    /// Suspicion timer: `T_SUSPECT_BASE × log(N) × (1 + LHA)`
    pub fn check_suspect_timeouts(&mut self) {
        let n = self.members.len().max(1) as f64;
        let timeout_ms =
            T_SUSPECT_BASE_MS as f64 * n.ln().max(1.0) * (1.0 + self.lha_multiplier as f64);
        let timeout = Duration::from_millis(timeout_ms as u64);
        let now = Instant::now();

        let suspects: Vec<NodeId> = self
            .members
            .iter()
            .filter_map(|(id, e)| {
                if let MemberStatus::Suspect { since } = e.status {
                    if now.duration_since(since) > timeout {
                        return Some(*id);
                    }
                }
                None
            })
            .collect();

        for id in suspects {
            self.mark_dead(&id);
        }
    }

    /// Update fitness and potentially transition tier.
    pub fn update_fitness(&mut self, components: FitnessComponents) {
        let new_tier = components.recommended_tier();
        let current_score = components.score();

        // Check demotion with hysteresis (SPEC B §2.5)
        if (new_tier < self.my_tier && current_score < self.my_tier.demotion_threshold())
            || (new_tier > self.my_tier && current_score >= new_tier.promotion_threshold())
        {
            self.my_tier = new_tier;
        }

        self.my_fitness = components;
    }

    /// Get recent membership updates for piggybacking.
    fn recent_updates(&self, max: usize) -> Vec<MemberEntry> {
        // Prioritize suspect/dead over alive for fast propagation
        let mut updates: Vec<&MemberEntry> = self.members.values().collect();
        updates.sort_by(|a, b| {
            let priority = |e: &MemberEntry| -> u8 {
                match e.status {
                    MemberStatus::Dead { .. } => 0,
                    MemberStatus::Suspect { .. } => 1,
                    MemberStatus::Left => 2,
                    MemberStatus::Alive => 3,
                }
            };
            priority(a)
                .cmp(&priority(b))
                .then(b.last_seen.cmp(&a.last_seen))
        });
        updates.into_iter().take(max).cloned().collect()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_tier_obt_bridge() {
        assert_eq!(NodeTier::from_obt_tier_u8(0), NodeTier::Leaf);
        assert_eq!(NodeTier::from_obt_tier_u8(2), NodeTier::LocalSP);
        assert_eq!(NodeTier::from_obt_tier_u8(6), NodeTier::GlobalBackbone);
        assert_eq!(NodeTier::from_obt_tier_u8(99), NodeTier::GlobalBackbone);
        assert_eq!(NodeTier::Leaf.to_obt_tier_u8(), 0);
        assert_eq!(NodeTier::GlobalBackbone.to_obt_tier_u8(), 6);
    }

    #[test]
    fn test_obt_penalty_factor() {
        assert_eq!(FitnessComponents::obt_penalty_to_factor(0), 1.0);
        assert_eq!(FitnessComponents::obt_penalty_to_factor(1), 0.9);
        assert_eq!(FitnessComponents::obt_penalty_to_factor(2), 0.5);
        assert_eq!(FitnessComponents::obt_penalty_to_factor(3), 0.1);
        assert_eq!(FitnessComponents::obt_penalty_to_factor(4), 0.0);
    }

    #[test]
    fn test_fitness_score_basic() {
        let f = FitnessComponents {
            uptime: 1.0,
            battery: 1.0,
            bandwidth: 1.0,
            storage: 1.0,
            cpu: 1.0,
            network_quality: 1.0,
            reputation: 1.0,
        };
        assert!((f.score() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_node_tier_from_u8() {
        assert_eq!(NodeTier::from_u8(0), Some(NodeTier::Leaf));
        assert_eq!(NodeTier::from_u8(6), Some(NodeTier::GlobalBackbone));
        assert_eq!(NodeTier::from_u8(7), None);
    }
}
