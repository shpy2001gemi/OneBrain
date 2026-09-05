# OBP-PROD-002 — Node-owned outbound-first lifecycle

> State: Planned
> Branch: `codex/obp-prod-002-node-lifecycle`
> Depends on: `OBP-PROD-001` merged

## Objective

Give the normal `OneBrainNode` aggregate sole lifecycle ownership of the
existing outbound-first runtime components, with deterministic startup,
shutdown, rollback and restart recovery.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../PROGRESS.md`
- the accepted `OBP-PROD-001` product profile and vectors
- `../../../specs/vnext/RUNTIME_OWNERSHIP_PROFILE_V1.md`
- `../../../specs/vnext/RUNTIME_LIFECYCLE_PROFILE_V1.md`
- `../../../specs/vnext/RUNTIME_CONCURRENCY_PROFILE_V1.md`
- `../../../specs/vnext/RUNTIME_FEATURE_BUDGET_PROFILE_V1.md`
- current `onebrain-node` vNext networking owner, Reachability Manager,
  redb replay and route-journal implementation files

## Deliverable

- Node-owned construction and typed service façade for Reachability Manager,
  shared authenticated transport, reservation state and route journal.
- Ordered start/stop, bounded worker ownership, partial-start rollback and
  restart replay.
- Focused lifecycle, concurrency, kill-switch and failure-injection tests.

## Acceptance

- No application surface constructs an independent network runtime.
- Partial startup leaves no worker, lease or socket orphaned.
- Shutdown fences new work and drains/cancels in-flight work as specified.
- Restart restores only validated durable state and nonterminal intent.
- Disabled/default-off mode performs no background network work.
- Applicable validators and focused Rust tests pass.

## Excluded

Discovery orchestration, automatic route planning, public API/UI and rollout.
