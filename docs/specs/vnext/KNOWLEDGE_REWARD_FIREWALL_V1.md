# Knowledge-Plane / Reward Firewall v1

> **Task:** `POMV-003`  
> **Status:** Complete  
> **Depends on:** `POMV-002`, `POMV-004`, `FND-006`

## 1. Purpose

This profile freezes a one-way dependency:

```text
committed Use / Derivation / Outcome / Benefit evidence
                         |
                         v  best-effort, post-commit
                optional reward consumer
```

No arrow returns from reward state to KU preservation, KQL querying, OBP sync,
Mapping materialization/adoption or evidence replay. This task does not design
OBT economics; it makes postponing or replacing OBT safe.

## 2. Post-commit notice

`RewardEvidenceNotice` contains only:

- exact EventCID;
- exact payload ObjectCID;
- evidence kind: Use, Derivation, OutcomeObservation or BenefitEvidence; and
- an exact evidence policy/frontier commitment.

It contains no amount, token account, mint authorization, price, ranking or
reward policy. It is not a canonical KU/Event field and cannot mutate the
referenced bytes. Private payload is not copied into the notice; any later
consumer access remains subject to its separate store/disclosure authority.

## 3. Best-effort queue

The knowledge transaction commits first. Only then may
`observe_committed_evidence` enqueue a notice into a bounded local queue.
Duplicate EventCIDs already queued are idempotent. A full queue drops only the
export notice and reports local backpressure; it never rolls back or marks the
knowledge operation failed.

A separate worker drains the queue. Temporary unavailability/backpressure has
a bounded retry count. Exhausted retries and corrupt consumer state move only
the EventCID to a bounded local quarantine list. Drain returns an observability
report, not an error that can enter KU/KQL/OBP control flow.

## 4. Feature and kill switch

`reward_evidence_export` is a new FND-006 feature flag. It is off by default and
requires `object_event_v1` when active. Its independent kill switch can stop
export without stopping the object/event layer or any knowledge operation.

The disabled configuration still accepts, preserves, queries, synchronizes,
adopts and replays knowledge normally.

## 5. Architectural enforcement

The reward consumer trait exists only in `onebrain-node`'s adapter module.
Canonical foundation objects, KQL and OBP do not depend on that trait or on OBT
state. `execute_knowledge_operation` intentionally accepts only the knowledge
operation closure—there is no consumer parameter or reward failure type.

Legacy OBT modules remain legacy code and are not authority for vNext evidence
or knowledge-plane behavior.

## 6. Boundaries

The firewall does not:

- infer benefit from Use or exposure;
- mint, price, transfer or authorize OBT;
- let consumer availability affect KU/KQL/OBP results;
- put token authority into canonical KU/Event bytes;
- grant disclosure, materialization, adoption or execution authority;
- make reward state a source of evidence; or
- introduce a Core DNA Gene or opcode.

## 7. Executable evidence

Six firewall tests plus four feature-config tests prove:

- disabled reward export leaves publish/query/sync/adopt/replay usable;
- unavailable consumers retry then quarantine without knowledge errors;
- corrupt consumer state is isolated;
- backpressure is bounded and drops only export notices;
- queued EventCID replay is idempotent; and
- notices expose no mint/token/reward authority while the new flag remains
  default-off and dependency-checked.
