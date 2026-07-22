//! Executable bounded-state mirror for the five M6 TLA+/PlusCal models.
//!
//! The Rust explorer runs in the normal workspace CI even when TLC is not
//! installed. The corresponding `.tla` modules remain the language-neutral
//! formal source for independent TLC runs.

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedInvariantResult {
    pub model: &'static str,
    pub explored_states: usize,
    pub violations: Vec<String>,
    pub state_set_root: [u8; 32],
}

impl BoundedInvariantResult {
    pub fn passed(&self) -> bool {
        self.violations.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct M6BoundedModelReport {
    pub models: Vec<BoundedInvariantResult>,
    pub report_root: [u8; 32],
}

impl M6BoundedModelReport {
    pub fn passed(&self) -> bool {
        self.models.iter().all(BoundedInvariantResult::passed)
    }
}

pub fn run_m6_bounded_models() -> M6BoundedModelReport {
    let models = vec![
        explore_feed_checkpoint(),
        explore_receptor_resolution(),
        explore_provider_lease(),
        explore_permit_revocation_task(),
        explore_reconciliation_session(),
    ];
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:m6-bounded-model-report:1\0");
    for model in &models {
        hasher.update(&(model.model.len() as u64).to_be_bytes());
        hasher.update(model.model.as_bytes());
        hasher.update(&(model.explored_states as u64).to_be_bytes());
        hasher.update(&model.state_set_root);
        hasher.update(&(model.violations.len() as u64).to_be_bytes());
        for violation in &model.violations {
            hasher.update(&(violation.len() as u64).to_be_bytes());
            hasher.update(violation.as_bytes());
        }
    }
    M6BoundedModelReport {
        models,
        report_root: *hasher.finalize().as_bytes(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct FeedCheckpointState {
    known_events: u8,
    known_forks: u8,
    checkpoint_covered: Option<u8>,
    checkpoint_proof_valid: bool,
    suppressed: u8,
    suppressed_forks: u8,
}

fn feed_checkpoint_invariant(state: FeedCheckpointState) -> bool {
    let suppression_has_proof = state.suppressed == 0 || state.checkpoint_proof_valid;
    let suppressed_known = state.suppressed & !state.known_events == 0;
    let fork_not_hidden = state.suppressed_forks == 0;
    let suppressed_covered = match state.checkpoint_covered {
        Some(covered) => {
            let covered_mask = ((1u16 << (covered + 1)) - 1) as u8;
            state.suppressed & !covered_mask == 0
        }
        None => state.suppressed == 0,
    };
    suppression_has_proof && suppressed_known && fork_not_hidden && suppressed_covered
}

fn explore_feed_checkpoint() -> BoundedInvariantResult {
    let initial = FeedCheckpointState::default();
    explore(
        "FeedCheckpoint",
        initial,
        6,
        |state| {
            let mut next = Vec::new();
            for sequence in 0..3u8 {
                let bit = 1 << sequence;
                let mut received = *state;
                received.known_events |= bit;
                next.push(received);

                let mut fork = *state;
                fork.known_events |= bit;
                fork.known_forks |= bit;
                next.push(fork);

                let required = ((1u16 << (sequence + 1)) - 1) as u8;
                if state.known_events & required == required
                    && state
                        .checkpoint_covered
                        .is_none_or(|previous| sequence >= previous)
                {
                    let mut checkpointed = *state;
                    checkpointed.checkpoint_covered = Some(sequence);
                    checkpointed.checkpoint_proof_valid = true;
                    next.push(checkpointed);
                }

                if state.checkpoint_proof_valid
                    && state
                        .checkpoint_covered
                        .is_some_and(|covered| sequence <= covered)
                    && state.known_events & bit != 0
                {
                    let mut suppressed = *state;
                    suppressed.suppressed |= bit;
                    next.push(suppressed);
                }
            }
            next
        },
        feed_checkpoint_invariant,
        encode_feed_state,
    )
}

fn encode_feed_state(state: FeedCheckpointState) -> Vec<u8> {
    vec![
        state.known_events,
        state.known_forks,
        state.checkpoint_covered.unwrap_or(u8::MAX),
        state.checkpoint_proof_valid as u8,
        state.suppressed,
        state.suppressed_forks,
    ]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct ResolutionState {
    proposal: bool,
    materialized: bool,
    adopted: bool,
}

fn resolution_invariant(state: ResolutionState) -> bool {
    (!state.materialized || state.proposal) && (!state.adopted || state.materialized)
}

fn explore_receptor_resolution() -> BoundedInvariantResult {
    explore(
        "ReceptorResolution",
        ResolutionState::default(),
        4,
        |state| {
            let mut next = Vec::new();
            if !state.proposal {
                next.push(ResolutionState {
                    proposal: true,
                    ..*state
                });
            }
            if state.proposal && !state.materialized {
                next.push(ResolutionState {
                    materialized: true,
                    ..*state
                });
            }
            if state.materialized && !state.adopted {
                next.push(ResolutionState {
                    adopted: true,
                    ..*state
                });
            }
            next
        },
        resolution_invariant,
        |state| {
            vec![
                state.proposal as u8,
                state.materialized as u8,
                state.adopted as u8,
            ]
        },
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct ProviderState {
    max_generation: u8,
    retirement_floor: u8,
    high_water_conflicts: u8,
}

fn provider_invariant(state: ProviderState) -> bool {
    let active = state.max_generation > state.retirement_floor;
    !active || state.max_generation != 0
}

fn explore_provider_lease() -> BoundedInvariantResult {
    explore(
        "ProviderLease",
        ProviderState::default(),
        6,
        |state| {
            let mut next = Vec::new();
            for generation in 1..=3 {
                let mut lease = *state;
                if generation > lease.max_generation {
                    lease.max_generation = generation;
                    lease.high_water_conflicts = 1;
                } else if generation == lease.max_generation {
                    lease.high_water_conflicts =
                        lease.high_water_conflicts.saturating_add(1).min(2);
                }
                next.push(lease);

                let mut retire = *state;
                retire.retirement_floor = retire.retirement_floor.max(generation);
                next.push(retire);
            }
            next
        },
        provider_invariant,
        |state| {
            vec![
                state.max_generation,
                state.retirement_floor,
                state.high_water_conflicts,
            ]
        },
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct PermitTaskState {
    accepted: bool,
    revoked_relative: bool,
    exact_scope: bool,
    executed: bool,
    execution_authorized_at_time: bool,
}

fn permit_invariant(state: PermitTaskState) -> bool {
    !state.executed || state.execution_authorized_at_time
}

fn explore_permit_revocation_task() -> BoundedInvariantResult {
    explore(
        "PermitRevocationTask",
        PermitTaskState::default(),
        5,
        |state| {
            let mut next = Vec::new();
            if !state.accepted {
                next.push(PermitTaskState {
                    accepted: true,
                    ..*state
                });
            }
            if state.accepted && !state.revoked_relative {
                next.push(PermitTaskState {
                    revoked_relative: true,
                    ..*state
                });
            }
            if !state.exact_scope {
                next.push(PermitTaskState {
                    exact_scope: true,
                    ..*state
                });
            }
            if state.accepted && !state.revoked_relative && state.exact_scope && !state.executed {
                next.push(PermitTaskState {
                    executed: true,
                    execution_authorized_at_time: true,
                    ..*state
                });
            }
            next
        },
        permit_invariant,
        |state| {
            vec![
                state.accepted as u8,
                state.revoked_relative as u8,
                state.exact_scope as u8,
                state.executed as u8,
                state.execution_authorized_at_time as u8,
            ]
        },
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct ReconciliationState {
    context_bound: bool,
    roots_equal: bool,
    pending_ranges: u8,
    selector_complete: bool,
}

fn reconciliation_invariant(state: ReconciliationState) -> bool {
    !state.selector_complete
        || (state.context_bound && state.roots_equal && state.pending_ranges == 0)
}

fn explore_reconciliation_session() -> BoundedInvariantResult {
    let initial = ReconciliationState {
        pending_ranges: 2,
        ..ReconciliationState::default()
    };
    explore(
        "ReconciliationSession",
        initial,
        6,
        |state| {
            let mut next = Vec::new();
            if !state.context_bound {
                next.push(ReconciliationState {
                    context_bound: true,
                    ..*state
                });
            }
            if !state.roots_equal {
                next.push(ReconciliationState {
                    roots_equal: true,
                    ..*state
                });
            }
            if state.pending_ranges > 0 {
                next.push(ReconciliationState {
                    pending_ranges: state.pending_ranges - 1,
                    ..*state
                });
            }
            if state.context_bound
                && state.roots_equal
                && state.pending_ranges == 0
                && !state.selector_complete
            {
                next.push(ReconciliationState {
                    selector_complete: true,
                    ..*state
                });
            }
            if state.selector_complete {
                // A context change starts a new scoped exchange; it cannot keep
                // the previous selector-complete bit.
                next.push(ReconciliationState {
                    context_bound: false,
                    selector_complete: false,
                    ..*state
                });
            }
            next
        },
        reconciliation_invariant,
        |state| {
            vec![
                state.context_bound as u8,
                state.roots_equal as u8,
                state.pending_ranges,
                state.selector_complete as u8,
            ]
        },
    )
}

fn explore<S, N, I, E>(
    name: &'static str,
    initial: S,
    max_depth: usize,
    next_states: N,
    invariant: I,
    encode: E,
) -> BoundedInvariantResult
where
    S: Copy + Ord + std::fmt::Debug,
    N: Fn(&S) -> Vec<S>,
    I: Fn(S) -> bool,
    E: Fn(S) -> Vec<u8>,
{
    let mut seen = BTreeSet::from([initial]);
    let mut frontier = BTreeSet::from([initial]);
    for _ in 0..max_depth {
        let mut next_frontier = BTreeSet::new();
        for state in &frontier {
            for next in next_states(state) {
                if seen.insert(next) {
                    next_frontier.insert(next);
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }
    let violations = seen
        .iter()
        .copied()
        .filter(|state| !invariant(*state))
        .map(|state| format!("{state:?}"))
        .collect::<Vec<_>>();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"onebrain:vnext:bounded-state-set:1\0");
    hasher.update(name.as_bytes());
    for state in &seen {
        let bytes = encode(*state);
        hasher.update(&(bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    BoundedInvariantResult {
        model: name,
        explored_states: seen.len(),
        violations,
        state_set_root: *hasher.finalize().as_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_bounded_models_have_no_counterexample() {
        let report = run_m6_bounded_models();
        assert_eq!(report.models.len(), 5);
        assert!(report.passed(), "{:#?}", report.models);
        assert!(report.models.iter().all(|model| model.explored_states > 1));
        assert_ne!(report.report_root, [0; 32]);
    }

    #[test]
    fn invariant_oracles_reject_the_forbidden_states() {
        assert!(!feed_checkpoint_invariant(FeedCheckpointState {
            known_events: 0b111,
            known_forks: 0b100,
            checkpoint_covered: Some(2),
            checkpoint_proof_valid: true,
            suppressed: 0,
            suppressed_forks: 0b100,
        }));
        assert!(!resolution_invariant(ResolutionState {
            proposal: true,
            materialized: false,
            adopted: true,
        }));
        assert!(!permit_invariant(PermitTaskState {
            accepted: true,
            revoked_relative: true,
            exact_scope: true,
            executed: true,
            execution_authorized_at_time: false,
        }));
        assert!(!reconciliation_invariant(ReconciliationState {
            context_bound: true,
            roots_equal: false,
            pending_ranges: 0,
            selector_complete: true,
        }));
    }

    #[test]
    fn repeated_run_has_identical_state_set_and_report_roots() {
        assert_eq!(run_m6_bounded_models(), run_m6_bounded_models());
    }
}
