# OneBrain vNext — M6 Bounded Formal Model Profile v1

> **Task:** `QA-003`  
> **Status:** Executable bounded model; TLC source included  
> **Code:** [`onebrain-node::vnext_m6_model`](../../../src/onebrain-node/src/vnext_m6_model.rs)  
> **TLA+:** [`formal/tla`](../../../formal/tla)

## 1. Model set

Five independent finite-state models cover:

| Model | Safety invariant |
|---|---|
| FeedCheckpoint | Suppression needs valid proof, covers only known exact events and never hides an unseen fork; checkpoint high-water cannot regress. |
| ReceptorResolution | Materialization requires proposal; adoption/active mapping requires materialization. |
| ProviderLease | Max generation and retirement floor never regress; a retired high-water generation cannot become active again. |
| PermitRevocationTask | Execution requires accepted exact scope and no revocation at execution time; later revocation blocks future action without rewriting history. |
| ReconciliationSession | Selector completion requires exact context binding, equal roots and zero remaining ranges. |

The models contain no wall-clock, majority, leader, central server, global
membership or global completeness state.

## 2. Two synchronized forms

Human/tool-independent TLA+ modules and TLC configs live under `formal/tla`.
`run_m6_bounded_models()` is an executable Rust mirror so normal CI explores
the bounded state sets even when TLC is unavailable. Each model emits the count
and deterministic state-set root; the aggregate report also has a deterministic
root.

TLC was not assumed to be installed by the runtime. A deployment that installs
TLA+ tools can run every adjacent `.cfg` without changing production behavior.

## 3. Counterexamples found during implementation

The first explorer run exposed and corrected two design errors:

1. accepting a later checkpoint with a lower covered position could invalidate
   prior suppression assumptions; checkpoint coverage is now monotonic;
2. evaluating past task execution against a revocation learned later rewrote
   history; the model now records authority at execution time while the new
   revocation blocks future execution.

These are model corrections, not claims that unseen revocation was absent.

## 4. Executable evidence

Three tests verify that all five reachable bounded models have no
counterexample, explicit forbidden states are rejected by the invariant
oracles, and repeated exploration produces identical state-set/report roots.

