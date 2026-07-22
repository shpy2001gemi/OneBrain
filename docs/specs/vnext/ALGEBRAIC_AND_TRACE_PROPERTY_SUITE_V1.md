# OneBrain vNext — Algebraic and Trace Property Suite v1

> **Task:** `QA-006`  
> **Status:** Complete  
> **Gate:** `cargo test -p ku-core qa006_ --lib` and `cargo test -p ku-net qa006_ --lib`

## 1. Scope

This suite exercises algebra only where the owning contract requires it. It
does not incorrectly require ordered side effects to be commutative. Instead,
it verifies that set/maximum merges and durable reducers converge, and that
different fair delivery schedules for the same validated trace produce the
same scoped view.

| Boundary | Property | Executable evidence |
|---|---|---|
| exact high-water merge | commutative, associative, idempotent and monotonic | `qa006_high_water_merge_is_commutative_associative_idempotent_and_monotonic` |
| Resolution reducer | all six permutations of one causal trace produce the same branch-preserving view; exact replay is idempotent | `qa006_resolution_reducer_trace_permutations_produce_the_same_view` |
| authority filter | unauthorized and unresolved events remain recorded where required but cannot change authoritative resolution state | `qa006_unauthorized_and_unresolved_events_do_not_change_authoritative_state` |
| Mapping materialization | kernel/envelope pair is atomic and idempotent | `qa006_materialization_pair_is_atomic_and_idempotent` |
| materialization/adoption separation | durable Mapping storage alone leaves the Resolution view open | `qa006_materialization_does_not_adopt_or_change_resolution` |
| provider retirement | all six lease/retirement permutations yield the same high-water, exact retirement floor and active lease set; replay is stable | `qa006_retirement_and_lease_trace_permutations_have_one_final_view` |
| reconciliation completion | drop/reorder/duplicate schedules converge after fair redelivery | `qa006_completion_trace_permutations_converge_under_fair_redelivery` |

## 2. Required semantic separations

The properties freeze these non-equivalences:

- delivery or merge does not grant feed authority;
- proposal is not Mapping;
- materialized Mapping is not adopted Mapping;
- provider availability is not content correctness or custody;
- retirement is an exact signed floor, not global deletion;
- manifest-batch completion is selector/session-relative and never global
  completion;
- unauthorized and unresolved are not `false` and are not silently discarded.

## 3. Permutation oracle

For three-record traces the suite exhausts all six permutations. Expected
outcomes are compared as canonical public views or exact sorted identities,
not by arrival-specific status codes. Each resulting state is then replayed to
prove idempotency. Causal relationships remain part of the signed records, so
arrival order cannot replace causality.

## 4. Limits

This gate is deterministic bounded evidence, not a proof over arbitrary trace
length. The M6 bounded model suite covers related state-machine invariants;
QA-007 extends the evidence to large logical populations. Neither gate can
claim global completeness in a partitionable network.

