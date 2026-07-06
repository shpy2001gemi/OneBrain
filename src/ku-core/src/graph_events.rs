//! # Graph Events — Event Sourcing for Bond Lifecycle
//!
//! Provides an in-memory event accumulator that tracks all bond lifecycle
//! operations (create, reinforce, weaken, state change) and supports
//! time-travel replay to reconstruct bond states at any point in time.

use crate::graph_types::{BondEvent, BondSnapshot, CompactionReport};
use crate::types::{RelationType, EdgeState};
use std::collections::HashMap;

/// In-memory event accumulator for bond lifecycle tracking.
///
/// Events are stored in append-only order with monotonically increasing
/// sequence numbers. The accumulator supports:
/// - Querying events by sequence range, KU CID, or time range
/// - Time-travel replay to reconstruct bond states at any timestamp
/// - Compaction to remove old events and reduce memory usage
pub struct EventAccumulator {
    events: Vec<BondEvent>,
    next_seq: u64,
}

impl Default for EventAccumulator {
    fn default() -> Self { Self::new() }
}

impl EventAccumulator {
    /// Create a new empty event accumulator.
    pub fn new() -> Self {
        Self { events: Vec::new(), next_seq: 0 }
    }

    /// Append an event, returns its assigned sequence number.
    pub fn append(&mut self, event: BondEvent) -> u64 {
        let seq = self.next_seq;
        self.events.push(event);
        self.next_seq += 1;
        seq
    }

    /// Get a slice of all recorded events.
    pub fn events(&self) -> &[BondEvent] { &self.events }

    /// Get events in the sequence range `[from_seq, to_seq)`.
    ///
    /// Returns an empty slice if `from_seq` is beyond the event list.
    /// Clamps `to_seq` to the actual event count.
    pub fn events_range(&self, from_seq: u64, to_seq: u64) -> &[BondEvent] {
        let start = from_seq as usize;
        let end = (to_seq as usize).min(self.events.len());
        if start >= self.events.len() { return &[]; }
        &self.events[start..end]
    }

    /// Get all events involving a specific KU (as source or target).
    pub fn events_for_ku(&self, cid: &[u8; 32]) -> Vec<&BondEvent> {
        self.events.iter()
            .filter(|e| e.source_cid() == cid || e.target_cid() == cid)
            .collect()
    }

    /// Get all events within the inclusive time range `[from_ts, to_ts]`.
    pub fn events_in_time_range(&self, from_ts: u64, to_ts: u64) -> Vec<&BondEvent> {
        self.events.iter()
            .filter(|e| {
                let t = e.timestamp();
                t >= from_ts && t <= to_ts
            })
            .collect()
    }

    /// Latest sequence number. Returns 0 if empty.
    pub fn latest_seq(&self) -> u64 {
        if self.next_seq == 0 { 0 } else { self.next_seq - 1 }
    }

    /// Number of events currently stored.
    pub fn len(&self) -> usize { self.events.len() }

    /// Whether the accumulator contains no events.
    pub fn is_empty(&self) -> bool { self.events.is_empty() }

    /// Replay all events up to `target_time` to reconstruct bond states.
    ///
    /// Returns a snapshot of every bond that was created at or before the
    /// target time, with reinforcements, weakenings, and state changes applied
    /// in order.
    pub fn replay_at_time(&self, target_time: u64) -> Vec<BondSnapshot> {
        // Key: (source_cid, target_cid, relation_u8) -> (weight, state)
        let mut bonds: HashMap<([u8; 32], [u8; 32], u8), (u16, EdgeState)> = HashMap::new();

        for event in &self.events {
            // ★ OBKG Fix L1: Use continue (not break) to handle out-of-order events
            if event.timestamp() > target_time { continue; }
            match event {
                BondEvent::Created { source_cid, target_cid, relation, weight, .. } => {
                    bonds.insert(
                        (*source_cid, *target_cid, *relation as u8),
                        (*weight, EdgeState::Active),
                    );
                }
                BondEvent::Reinforced { source_cid, target_cid, relation, new_weight, .. } => {
                    if let Some(entry) = bonds.get_mut(&(*source_cid, *target_cid, *relation as u8)) {
                        entry.0 = *new_weight;
                    }
                }
                BondEvent::Weakened { source_cid, target_cid, relation, new_weight, .. } => {
                    if let Some(entry) = bonds.get_mut(&(*source_cid, *target_cid, *relation as u8)) {
                        entry.0 = *new_weight;
                    }
                }
                BondEvent::StateChanged { source_cid, target_cid, relation, new_state, .. } => {
                    if let Some(entry) = bonds.get_mut(&(*source_cid, *target_cid, *relation as u8)) {
                        entry.1 = *new_state;
                    }
                }
            }
        }

        bonds.into_iter().map(|((src, tgt, rel_byte), (weight, state))| {
            BondSnapshot {
                source_cid: src,
                target_cid: tgt,
                relation: RelationType::from_u8(rel_byte).unwrap_or(RelationType::Extends),
                weight,
                state,
            }
        }).collect()
    }

    /// Compact the event log by removing events with timestamps ≤ `cutoff_timestamp`.
    ///
    /// Returns a `CompactionReport` summarizing what was removed and retained.
    pub fn compact(&mut self, cutoff_timestamp: u64) -> CompactionReport {
        let total_before = self.events.len() as u64;
        // Find the first event *after* the cutoff
        let split_idx = self.events.iter()
            .position(|e| e.timestamp() > cutoff_timestamp)
            .unwrap_or(self.events.len());
        let events_removed = split_idx as u64;
        self.events = self.events.split_off(split_idx);
        let events_retained = self.events.len() as u64;
        CompactionReport {
            snapshot_seq: self.next_seq.saturating_sub(1),
            events_removed,
            events_retained,
            snapshot_size_bytes: total_before * 128, // estimate ~128 bytes per event
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use crate::graph_types::*;

    fn make_cid(seed: u8) -> [u8; 32] { [seed; 32] }

    fn create_event(src: u8, tgt: u8, rel: RelationType, weight: u16, ts: u64) -> BondEvent {
        BondEvent::Created {
            source_cid: make_cid(src),
            target_cid: make_cid(tgt),
            relation: rel,
            weight,
            creator: Creator::Human,
            evidence: vec![],
            timestamp: ts,
        }
    }

    fn reinforce_event(src: u8, tgt: u8, rel: RelationType, new_weight: u16, ts: u64) -> BondEvent {
        BondEvent::Reinforced {
            source_cid: make_cid(src),
            target_cid: make_cid(tgt),
            relation: rel,
            old_weight: 0,
            new_weight,
            timestamp: ts,
        }
    }

    fn weaken_event(src: u8, tgt: u8, rel: RelationType, new_weight: u16, ts: u64) -> BondEvent {
        BondEvent::Weakened {
            source_cid: make_cid(src),
            target_cid: make_cid(tgt),
            relation: rel,
            old_weight: 0,
            new_weight,
            reason: WeakeningReason::Decay,
            timestamp: ts,
        }
    }

    fn state_change_event(src: u8, tgt: u8, rel: RelationType, new_state: EdgeState, ts: u64) -> BondEvent {
        BondEvent::StateChanged {
            source_cid: make_cid(src),
            target_cid: make_cid(tgt),
            relation: rel,
            old_state: EdgeState::Active,
            new_state,
            timestamp: ts,
        }
    }

    // ─── Test 1: New accumulator is empty ─────────────────────────
    #[test]
    fn new_accumulator_is_empty() {
        let acc = EventAccumulator::new();
        assert!(acc.is_empty());
        assert_eq!(acc.len(), 0);
        assert_eq!(acc.latest_seq(), 0);
    }

    // ─── Test 2: Append increments sequence numbers ───────────────
    #[test]
    fn append_increments_seq() {
        let mut acc = EventAccumulator::new();
        let s0 = acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        let s1 = acc.append(create_event(3, 4, RelationType::Causes, 200, 1001));
        let s2 = acc.append(create_event(5, 6, RelationType::Cites, 300, 1002));
        assert_eq!(s0, 0);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(acc.len(), 3);
    }

    // ─── Test 3: events() returns all events ──────────────────────
    #[test]
    fn events_returns_all() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        let all = acc.events();
        assert_eq!(all.len(), 2);
    }

    // ─── Test 4: events_range correct slice ───────────────────────
    #[test]
    fn events_range_correct_slice() {
        let mut acc = EventAccumulator::new();
        for i in 0..5 {
            acc.append(create_event(i, i + 10, RelationType::Extends, 100, i as u64 * 100));
        }
        let range = acc.events_range(1, 4);
        assert_eq!(range.len(), 3);
        // First event in range should have source_cid = [1; 32]
        assert_eq!(range[0].source_cid(), &make_cid(1));
        // Last event in range should have source_cid = [3; 32]
        assert_eq!(range[2].source_cid(), &make_cid(3));
    }

    // ─── Test 5: events_range with out-of-bounds ──────────────────
    #[test]
    fn events_range_out_of_bounds() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        // from_seq beyond length → empty
        let empty = acc.events_range(10, 20);
        assert!(empty.is_empty());
        // to_seq beyond length → clamped
        let clamped = acc.events_range(0, 100);
        assert_eq!(clamped.len(), 1);
    }

    // ─── Test 6: events_for_ku matching source ────────────────────
    #[test]
    fn events_for_ku_source() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        let results = acc.events_for_ku(&make_cid(1));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_cid(), &make_cid(1));
    }

    // ─── Test 7: events_for_ku matching target ────────────────────
    #[test]
    fn events_for_ku_target() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        let results = acc.events_for_ku(&make_cid(4));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target_cid(), &make_cid(4));
    }

    // ─── Test 8: events_for_ku matching both source and target ────
    #[test]
    fn events_for_ku_both() {
        let mut acc = EventAccumulator::new();
        // CID 2 is target in first event and source in second
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(2, 3, RelationType::Causes, 200, 2000));
        let results = acc.events_for_ku(&make_cid(2));
        assert_eq!(results.len(), 2);
    }

    // ─── Test 9: events_in_time_range ─────────────────────────────
    #[test]
    fn events_in_time_range() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        acc.append(create_event(5, 6, RelationType::Cites, 300, 3000));
        acc.append(create_event(7, 8, RelationType::PartOf, 400, 4000));

        let range = acc.events_in_time_range(1500, 3500);
        assert_eq!(range.len(), 2);
        assert_eq!(range[0].timestamp(), 2000);
        assert_eq!(range[1].timestamp(), 3000);
    }

    // ─── Test 10: replay on empty accumulator ─────────────────────
    #[test]
    fn replay_empty() {
        let acc = EventAccumulator::new();
        let snapshots = acc.replay_at_time(u64::MAX);
        assert!(snapshots.is_empty());
    }

    // ─── Test 11: replay single create ────────────────────────────
    #[test]
    fn replay_single_create() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 500, 1000));
        let snapshots = acc.replay_at_time(1000);
        assert_eq!(snapshots.len(), 1);
        let s = &snapshots[0];
        assert_eq!(s.source_cid, make_cid(1));
        assert_eq!(s.target_cid, make_cid(2));
        assert_eq!(s.relation, RelationType::Extends);
        assert_eq!(s.weight, 500);
        assert_eq!(s.state, EdgeState::Active);
    }

    // ─── Test 12: replay create then reinforce ────────────────────
    #[test]
    fn replay_create_then_reinforce() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 500, 1000));
        acc.append(reinforce_event(1, 2, RelationType::Extends, 800, 2000));
        let snapshots = acc.replay_at_time(2000);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].weight, 800);
    }

    // ─── Test 13: replay at middle time (stops before later events)
    #[test]
    fn replay_at_middle_time() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 500, 1000));
        acc.append(reinforce_event(1, 2, RelationType::Extends, 800, 2000));
        acc.append(reinforce_event(1, 2, RelationType::Extends, 9999, 3000));
        // Replay at t=2000 should NOT include the t=3000 reinforce
        let snapshots = acc.replay_at_time(2000);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].weight, 800);
    }

    // ─── Test 14: replay with state change ────────────────────────
    #[test]
    fn replay_state_change() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 500, 1000));
        acc.append(state_change_event(1, 2, RelationType::Extends, EdgeState::Deprecated, 2000));
        let snapshots = acc.replay_at_time(2000);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].state, EdgeState::Deprecated);
    }

    // ─── Test 15: compact removes old events ──────────────────────
    #[test]
    fn compact_removes_old_events() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        acc.append(create_event(5, 6, RelationType::Cites, 300, 3000));
        let report = acc.compact(2000);
        assert_eq!(report.events_removed, 2);
        assert_eq!(report.events_retained, 1);
        assert_eq!(acc.len(), 1);
    }

    // ─── Test 16: compact retains new events ──────────────────────
    #[test]
    fn compact_retains_new_events() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        acc.append(create_event(5, 6, RelationType::Cites, 300, 3000));
        acc.compact(1500);
        // Only the first event (ts=1000) should be removed
        assert_eq!(acc.len(), 2);
        // Remaining events should be the ones at ts=2000 and ts=3000
        let remaining = acc.events();
        assert_eq!(remaining[0].timestamp(), 2000);
        assert_eq!(remaining[1].timestamp(), 3000);
    }

    // ─── Test 17: latest_seq tracking ─────────────────────────────
    #[test]
    fn latest_seq_tracking() {
        let mut acc = EventAccumulator::new();
        assert_eq!(acc.latest_seq(), 0);
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        assert_eq!(acc.latest_seq(), 0);
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        assert_eq!(acc.latest_seq(), 1);
        acc.append(create_event(5, 6, RelationType::Cites, 300, 3000));
        assert_eq!(acc.latest_seq(), 2);
    }

    // ─── Test 18: default trait ───────────────────────────────────
    #[test]
    fn default_creates_empty() {
        let acc = EventAccumulator::default();
        assert!(acc.is_empty());
        assert_eq!(acc.len(), 0);
    }

    // ─── Test 19: replay with weaken event ────────────────────────
    #[test]
    fn replay_weaken() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 500, 1000));
        acc.append(weaken_event(1, 2, RelationType::Extends, 200, 2000));
        let snapshots = acc.replay_at_time(2000);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].weight, 200);
    }

    // ─── Test 20: replay multiple bonds ───────────────────────────
    #[test]
    fn replay_multiple_bonds() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 500, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 700, 1001));
        acc.append(create_event(5, 6, RelationType::Cites, 300, 1002));
        let snapshots = acc.replay_at_time(1002);
        assert_eq!(snapshots.len(), 3);
    }

    // ─── Test 21: compact all events ──────────────────────────────
    #[test]
    fn compact_all_events() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        let report = acc.compact(u64::MAX);
        assert_eq!(report.events_removed, 2);
        assert_eq!(report.events_retained, 0);
        assert!(acc.is_empty());
    }

    // ─── Test 22: compact nothing when cutoff is before all events ─
    #[test]
    fn compact_nothing() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        let report = acc.compact(500);
        assert_eq!(report.events_removed, 0);
        assert_eq!(report.events_retained, 2);
        assert_eq!(acc.len(), 2);
    }

    // ─── Test 23: events_in_time_range boundary inclusivity ───────
    #[test]
    fn events_in_time_range_boundary() {
        let mut acc = EventAccumulator::new();
        acc.append(create_event(1, 2, RelationType::Extends, 100, 1000));
        acc.append(create_event(3, 4, RelationType::Causes, 200, 2000));
        acc.append(create_event(5, 6, RelationType::Cites, 300, 3000));
        // Exact boundary: from_ts=1000 and to_ts=3000 should include all three
        let range = acc.events_in_time_range(1000, 3000);
        assert_eq!(range.len(), 3);
    }
}
