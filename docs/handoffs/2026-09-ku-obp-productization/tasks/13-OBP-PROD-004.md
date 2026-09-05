# OBP-PROD-004 — Automatic routing, outbox and relay failover

> State: Planned
> Branch: `codex/obp-prod-004-routing`
> Depends on: `OBP-PROD-003` merged

## Objective

Connect durable network intent to expected-peer route planning and existing
direct, hole-punch and outbound relay carriers, including deterministic
alternate-path failover and checkpoint resume.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../CAPABILITY_STATUS.md`, `../PROGRESS.md`
- accepted outputs/evidence from `OBP-PROD-001..003`
- `../../../specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md`
- `../../../specs/vnext/ROUTE_AUTHORITY_BOUNDARY_PROFILE_V1.md`
- applicable outbox, reconciliation, retry and runtime-budget profiles
- current planner, route-journal, session, carrier and relay code/tests

## Deliverable

- Durable product-level network intent/outbox feeding the node-owned planner.
- Expected NodeID binding through carrier selection and handshake.
- Policy-ordered direct/hole-punch/relay attempts without inbound-port
  requirements for the relay path.
- Exact failover checkpoint, bounded retry/backoff and alternate-relay logic.
- Restart tests that neither duplicate accepted effects nor lose nonterminal
  intent.

## Acceptance

- Selected relay loss triggers policy-bounded alternate-path recovery.
- A relay may drop/delay/reorder traffic but cannot forge accepted peer
  identity or canonical content.
- Retry, delivery and reconciliation states are distinct and scope-honest.
- Resource budgets and kill switches remain effective.
- Applicable validators and focused/integration tests pass.

## Excluded

Public API/UI, default rollout, mailbox/push-wake delivery and wire redesign.
