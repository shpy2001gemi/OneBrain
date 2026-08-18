//! Bounded standing relay selection independent of provider or operator names.

use std::collections::BTreeMap;

use ku_core::foundation::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelayReservationBounds {
    minimum: usize,
    target: usize,
    maximum: usize,
}

impl RelayReservationBounds {
    pub fn new(minimum: usize, target: usize, maximum: usize) -> Result<Self, RelaySetError> {
        if minimum == 0 || minimum > target || target > maximum || maximum > 3 {
            return Err(RelaySetError::InvalidBounds);
        }
        Ok(Self {
            minimum,
            target,
            maximum,
        })
    }

    pub fn minimum(self) -> usize {
        self.minimum
    }

    pub fn target(self) -> usize {
        self.target
    }

    pub fn maximum(self) -> usize {
        self.maximum
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RelayEntry {
    heuristic_score: u64,
    in_use: bool,
}

/// A local selection heuristic. Membership confers no identity or route
/// authority; downstream code still requires validated descriptors and
/// reservations.
#[derive(Debug)]
pub struct StandingRelaySet {
    bounds: RelayReservationBounds,
    entries: BTreeMap<NodeId, RelayEntry>,
}

impl StandingRelaySet {
    pub fn new(bounds: RelayReservationBounds) -> Self {
        Self {
            bounds,
            entries: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, relay: NodeId, heuristic_score: u64) -> Result<bool, RelaySetError> {
        if self.entries.contains_key(&relay) {
            return Ok(false);
        }
        if self.entries.len() >= self.bounds.maximum {
            return Err(RelaySetError::Capacity);
        }
        self.entries.insert(
            relay,
            RelayEntry {
                heuristic_score,
                in_use: false,
            },
        );
        Ok(true)
    }

    pub fn ensure_on_demand(
        &mut self,
        relay: NodeId,
        heuristic_score: u64,
    ) -> Result<NodeId, RelaySetError> {
        if self.entries.contains_key(&relay) {
            return Ok(relay);
        }
        if self.entries.len() == self.bounds.maximum {
            let evict = self
                .entries
                .iter()
                .filter(|(_, entry)| !entry.in_use)
                .min_by_key(|(id, entry)| (entry.heuristic_score, **id))
                .map(|(id, _)| *id)
                .ok_or(RelaySetError::AllInUse)?;
            self.entries.remove(&evict);
        }
        self.insert(relay, heuristic_score)?;
        Ok(relay)
    }

    pub fn mark_in_use(&mut self, relay: NodeId, in_use: bool) -> Result<(), RelaySetError> {
        let entry = self
            .entries
            .get_mut(&relay)
            .ok_or(RelaySetError::UnknownRelay)?;
        entry.in_use = in_use;
        Ok(())
    }

    pub fn contains(&self, relay: NodeId) -> bool {
        self.entries.contains_key(&relay)
    }

    pub fn has_minimum(&self) -> bool {
        self.entries.len() >= self.bounds.minimum
    }

    pub fn has_target(&self) -> bool {
        self.entries.len() >= self.bounds.target
    }

    pub fn relay_ids(&self) -> Vec<NodeId> {
        self.entries.keys().copied().collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelaySetError {
    InvalidBounds,
    Capacity,
    AllInUse,
    UnknownRelay,
}
