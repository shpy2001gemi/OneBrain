//! # Distributed Watch Engine — Phase E1
//!
//! Manages standing queries across the network.
//! When new KUs are created/updated/deprecated, the WatchEngine
//! evaluates them against all registered watches and fires notifications.
//!
//! ## Design
//! - **Local watches**: Evaluated in-process when `on_ku_event()` is called.
//! - **Remote watches**: Forwarded via WatchRegisterMsg to neighbors within TTL.
//! - **Notification delivery**: Via WatchNotifyMsg back to the originator.

use std::collections::HashMap;
use std::time::Instant;

use ku_core::KuRuntime;
use crate::identity::NodeId;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Unique watch identifier.
pub type WatchId = u64;

/// What event triggers a watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEvent {
    Create    = 0,
    Update    = 1,
    Deprecate = 2,
    Any       = 3,
}

impl WatchEvent {
    /// Check if this event filter matches a given event type.
    pub fn matches(&self, event: WatchEvent) -> bool {
        *self == WatchEvent::Any || *self == event
    }
}

/// A condition for matching KUs against a watch.
/// Simplified evaluator based on field comparisons.
#[derive(Debug, Clone)]
pub enum WatchCondition {
    /// Trust score above threshold.
    TrustAbove(u16),
    /// Trust score below threshold.
    TrustBelow(u16),
    /// Contains a specific concept ID.
    HasConcept(u64),
    /// Has a specific domain code.
    HasDomain(u64),
    /// Always matches.
    Any,
    /// Logical AND of two conditions.
    And(Box<WatchCondition>, Box<WatchCondition>),
}

impl WatchCondition {
    /// Evaluate this condition against a KU.
    pub fn matches(&self, ku: &KuRuntime) -> bool {
        match self {
            WatchCondition::TrustAbove(threshold) => {
                ku.trust_score() >= *threshold
            },
            WatchCondition::TrustBelow(threshold) => {
                ku.trust_score() < *threshold
            },
            WatchCondition::HasConcept(concept_id) => {
                ku.contains_concept(*concept_id)
            },
            WatchCondition::HasDomain(domain_code) => {
                ku.epi.trust.domain_codes.contains(domain_code)
            },
            WatchCondition::Any => true,
            WatchCondition::And(a, b) => a.matches(ku) && b.matches(ku),
        }
    }
}

/// A registered watch subscription.
#[derive(Debug, Clone)]
pub struct WatchRegistration {
    /// Unique watch ID.
    pub id: WatchId,
    /// Who registered this watch.
    pub origin: NodeId,
    /// Event type filter.
    pub event: WatchEvent,
    /// Condition to evaluate.
    pub condition: WatchCondition,
    /// Notification endpoint (e.g., callback URL or topic).
    pub notify_endpoint: String,
    /// When this watch was registered.
    pub registered_at: Instant,
    /// TTL for network propagation (0 = local only).
    pub ttl: u8,
    /// Number of times this watch has fired.
    pub fire_count: u64,
}

/// A notification generated when a watch fires.
#[derive(Debug, Clone)]
pub struct WatchNotification {
    /// Which watch fired.
    pub watch_id: WatchId,
    /// Who to notify.
    pub origin: NodeId,
    /// The KU that triggered the watch.
    pub ku: KuRuntime,
    /// What event type triggered it.
    pub event: WatchEvent,
    /// Where to send the notification.
    pub notify_endpoint: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// Watch Engine
// ═══════════════════════════════════════════════════════════════════════════

/// Distributed watch engine.
///
/// Manages standing queries and evaluates incoming KU events
/// against all registered watches.
pub struct WatchEngine {
    /// Registered watches: id → registration.
    watches: HashMap<WatchId, WatchRegistration>,
    /// Counter for generating watch IDs.
    next_id: WatchId,
    /// Maximum concurrent watches.
    max_watches: usize,
    /// Our node ID.
    #[allow(dead_code)]
    my_id: NodeId,
}

impl WatchEngine {
    /// Create a new watch engine.
    pub fn new(my_id: NodeId) -> Self {
        Self {
            watches: HashMap::new(),
            next_id: 1,
            max_watches: 1000,
            my_id,
        }
    }

    /// Register a new watch. Returns the watch ID.
    pub fn register(
        &mut self,
        origin: NodeId,
        event: WatchEvent,
        condition: WatchCondition,
        notify_endpoint: String,
        ttl: u8,
    ) -> Option<WatchId> {
        if self.watches.len() >= self.max_watches {
            return None; // At capacity
        }

        let id = self.next_id;
        self.next_id += 1;

        self.watches.insert(id, WatchRegistration {
            id,
            origin,
            event,
            condition,
            notify_endpoint,
            registered_at: Instant::now(),
            ttl,
            fire_count: 0,
        });

        Some(id)
    }

    /// Unregister a watch. Returns true if it existed.
    pub fn unregister(&mut self, watch_id: WatchId) -> bool {
        self.watches.remove(&watch_id).is_some()
    }

    /// Evaluate a KU event against all registered watches.
    ///
    /// Returns notifications for watches that matched.
    pub fn on_ku_event(
        &mut self,
        ku: &KuRuntime,
        event: WatchEvent,
    ) -> Vec<WatchNotification> {
        let mut notifications = Vec::new();

        for watch in self.watches.values_mut() {
            // Check event type filter
            if !watch.event.matches(event) {
                continue;
            }

            // Check condition
            if !watch.condition.matches(ku) {
                continue;
            }

            // Fire!
            watch.fire_count += 1;
            notifications.push(WatchNotification {
                watch_id: watch.id,
                origin: watch.origin,
                ku: ku.clone(),
                event,
                notify_endpoint: watch.notify_endpoint.clone(),
            });
        }

        notifications
    }

    /// Get watches that should be propagated to neighbors.
    ///
    /// Returns registrations with TTL > 0 (network-propagated watches).
    pub fn propagatable_watches(&self) -> Vec<&WatchRegistration> {
        self.watches.values()
            .filter(|w| w.ttl > 0)
            .collect()
    }

    /// Number of active watches.
    pub fn count(&self) -> usize {
        self.watches.len()
    }

    /// Get a watch by ID.
    pub fn get(&self, watch_id: WatchId) -> Option<&WatchRegistration> {
        self.watches.get(&watch_id)
    }

    /// Get all active watch IDs.
    pub fn active_ids(&self) -> Vec<WatchId> {
        self.watches.keys().copied().collect()
    }

    /// Clean up watches that haven't fired in a long time.
    pub fn cleanup_stale(&mut self, max_age: std::time::Duration) {
        let now = Instant::now();
        self.watches.retain(|_, w| {
            now.duration_since(w.registered_at) < max_age
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use ku_core::{KuRuntime, Epigenetics};
    use ku_core::core_dna::{CoreDna, CoreDnaHeader, Instruction};
    use crate::identity::{generate_node_id, KeyPair, PUZZLE_C_SMALL};

    fn make_node_id() -> NodeId {
        let kp = KeyPair::generate();
        let proof = generate_node_id(&kp.pubkey_bytes(), PUZZLE_C_SMALL);
        proof.node_id
    }

    fn make_ku(trust_score: u16, concept_id: u64) -> KuRuntime {
        let dna = CoreDna {
            header: CoreDnaHeader { version: 2, gene_type: 0, has_concept_table: false },
            concept_table: Vec::new(),
            instructions: vec![
                Instruction::Triple { s: concept_id, p: 133, o: 132 },
                Instruction::Certainty { level: 9500 },
            ],
        };
        let mut ku = KuRuntime::from_dna(dna).unwrap();
        ku.epi = Epigenetics::with_trust(trust_score, 8000);
        ku
    }

    #[test]
    fn test_register_and_fire() {
        let my_id = make_node_id();
        let origin = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        let wid = engine.register(
            origin,
            WatchEvent::Create,
            WatchCondition::TrustAbove(5000),
            "callback://test".to_string(),
            0,
        ).unwrap();

        assert_eq!(engine.count(), 1);

        // High trust KU should trigger
        let ku = make_ku(9000, 1);
        let notifs = engine.on_ku_event(&ku, WatchEvent::Create);
        assert_eq!(notifs.len(), 1);
        assert_eq!(notifs[0].watch_id, wid);

        // Check fire count incremented
        assert_eq!(engine.get(wid).unwrap().fire_count, 1);
    }

    #[test]
    fn test_event_filter() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        // Watch only CREATE events
        engine.register(
            make_node_id(),
            WatchEvent::Create,
            WatchCondition::Any,
            "".to_string(),
            0,
        );

        let ku = make_ku(5000, 1);

        // CREATE should match
        assert_eq!(engine.on_ku_event(&ku, WatchEvent::Create).len(), 1);
        // UPDATE should not match
        assert_eq!(engine.on_ku_event(&ku, WatchEvent::Update).len(), 0);
        // DEPRECATE should not match
        assert_eq!(engine.on_ku_event(&ku, WatchEvent::Deprecate).len(), 0);
    }

    #[test]
    fn test_event_any_matches_all() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        engine.register(
            make_node_id(),
            WatchEvent::Any,
            WatchCondition::Any,
            "".to_string(),
            0,
        );

        let ku = make_ku(5000, 1);
        assert_eq!(engine.on_ku_event(&ku, WatchEvent::Create).len(), 1);
        assert_eq!(engine.on_ku_event(&ku, WatchEvent::Update).len(), 1);
        assert_eq!(engine.on_ku_event(&ku, WatchEvent::Deprecate).len(), 1);
    }

    #[test]
    fn test_condition_trust_above() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        engine.register(
            make_node_id(),
            WatchEvent::Any,
            WatchCondition::TrustAbove(7000),
            "".to_string(),
            0,
        );

        // Below threshold → no match
        assert_eq!(engine.on_ku_event(&make_ku(3000, 1), WatchEvent::Create).len(), 0);
        // Above threshold → match
        assert_eq!(engine.on_ku_event(&make_ku(9000, 1), WatchEvent::Create).len(), 1);
    }

    #[test]
    fn test_condition_has_concept() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        engine.register(
            make_node_id(),
            WatchEvent::Any,
            WatchCondition::HasConcept(42),
            "".to_string(),
            0,
        );

        // Wrong concept → no match
        assert_eq!(engine.on_ku_event(&make_ku(5000, 99), WatchEvent::Create).len(), 0);
        // Right concept → match
        assert_eq!(engine.on_ku_event(&make_ku(5000, 42), WatchEvent::Create).len(), 1);
    }

    #[test]
    fn test_condition_and() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        engine.register(
            make_node_id(),
            WatchEvent::Any,
            WatchCondition::And(
                Box::new(WatchCondition::TrustAbove(5000)),
                Box::new(WatchCondition::HasConcept(42)),
            ),
            "".to_string(),
            0,
        );

        // High trust, wrong concept → no match
        assert_eq!(engine.on_ku_event(&make_ku(9000, 99), WatchEvent::Create).len(), 0);
        // Low trust, right concept → no match
        assert_eq!(engine.on_ku_event(&make_ku(1000, 42), WatchEvent::Create).len(), 0);
        // High trust, right concept → match!
        assert_eq!(engine.on_ku_event(&make_ku(9000, 42), WatchEvent::Create).len(), 1);
    }

    #[test]
    fn test_unregister() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        let wid = engine.register(
            make_node_id(), WatchEvent::Any, WatchCondition::Any, "".to_string(), 0,
        ).unwrap();

        assert_eq!(engine.count(), 1);
        assert!(engine.unregister(wid));
        assert_eq!(engine.count(), 0);
        assert!(!engine.unregister(wid)); // Already removed
    }

    #[test]
    fn test_multiple_watches_fire() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        // Two watches, both should match
        engine.register(make_node_id(), WatchEvent::Any, WatchCondition::Any, "a".to_string(), 0);
        engine.register(make_node_id(), WatchEvent::Any, WatchCondition::Any, "b".to_string(), 0);

        let ku = make_ku(5000, 1);
        let notifs = engine.on_ku_event(&ku, WatchEvent::Create);
        assert_eq!(notifs.len(), 2);
    }

    #[test]
    fn test_propagatable_watches() {
        let my_id = make_node_id();
        let mut engine = WatchEngine::new(my_id);

        // Local-only watch (TTL=0)
        engine.register(make_node_id(), WatchEvent::Any, WatchCondition::Any, "".to_string(), 0);
        // Network watch (TTL=3)
        engine.register(make_node_id(), WatchEvent::Any, WatchCondition::Any, "".to_string(), 3);

        let propagatable = engine.propagatable_watches();
        assert_eq!(propagatable.len(), 1);
        assert_eq!(propagatable[0].ttl, 3);
    }
}
