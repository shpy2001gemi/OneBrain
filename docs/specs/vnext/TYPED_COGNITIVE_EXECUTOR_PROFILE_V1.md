# Typed Cognitive Executor Profile v1

> **Task:** `CAP-005`  
> **Status:** Complete  
> **Depends on:** `CAP-002`, `CAP-004`

## 1. Purpose

Local and eventually remote cognition needs a typed execution boundary, not an overloaded raw chat endpoint. `ku-ai::vnext_executor` accepts one capability-scoped task, validates it against an admitted Permit, drives a cooperative bounded backend and returns committed output plus immutable execution provenance.

The executor does not select or advertise a provider; CAP-002 Manifest/Conformance and CAP-003 Offer retain those responsibilities.

## 2. Typed task boundary

`CognitiveTask` binds:

- full-width task, Permit, Offer and ImplementationManifest references;
- CapabilityDefinition ObjectCID;
- private typed payload and its domain-separated input commitment;
- schema/prompt/parameter commitments rather than raw public prompt metadata;
- requested effect classes, purpose, budget and retention;
- optional deterministic seed; and
- an exclusive logical local deadline.

The payload commitment must occur in the permit-scoped input set. Duplicate commitments/effects, empty typed input, unbounded budget and a non-advancing deadline fail before backend invocation.

`PermitValidator::authorize_scope` checks the concrete task again against the admitted permit: capability and purpose equality, input/effect subset, component-wise budget, retention attenuation and local lease state. The task deadline cannot extend beyond permit expiry.

## 3. Backend contract

`TypedCapabilityBackend` receives `CognitiveStepRequest`, not chat messages. Each cooperative step sees the typed input, commitments, optional continuation and exact remaining record/byte/work/step ceilings. It returns:

- one output fragment;
- `None` for completion or bounded opaque continuation bytes;
- measured work and logical elapsed ticks; and
- explicit limitation concepts.

Zero-tick steps and oversized continuation state are protocol violations. Output fragments, not backend claims, are measured by the executor.

## 4. Deterministic cancellation and deadline precedence

Cancellation is observed at step boundaries. A step already returned is admitted atomically, then cancellation is observed before the next call. This makes partial-output behavior deterministic and prevents half-admitted fragments.

The deadline is an exclusive caller-supplied logical tick. If a reported step would finish after it, that late fragment is discarded and the result finishes exactly at the deadline. No wall-clock, Earth-time or global-time authority is implied.

Record, byte, work and step ceilings are checked independently. Exhaustion with prior output returns a committed partial result; the executor never silently continues outside the Permit budget.

## 5. Result and provenance

Every termination, including cancellation before the first step, commits the complete accepted output (the empty byte string is also commit-able). The result includes:

- domain-separated output commitment;
- termination reason and consumed resource counters;
- optional private backend-error commitment, not raw error leakage; and
- canonical `CapabilityExecutionRecordBody` with input/parameter/output references, logical interval, limitations and deterministic trace digest.

Completed, partial, cancelled and failed records remain provenance. They do not establish correctness, publish output, mutate tools/profile/OBKG or materialize a Mapping/KU. Those actions require their own policy and durable-boundary commands.

## 6. Executable evidence

Tests prove:

- typed successful execution produces stable committed provenance only;
- pre-cancel invokes no backend;
- cancellation at the next step boundary preserves exactly the prior committed partial output;
- a deadline-crossing fragment is deterministically discarded;
- step-budget exhaustion returns a committed partial result; and
- task effect expansion is rejected before backend invocation.
