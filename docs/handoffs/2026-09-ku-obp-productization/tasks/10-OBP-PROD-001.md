# OBP-PROD-001 — Freeze the OBP product orchestration contract

> State: Planned
> Branch: `codex/obp-prod-001-product-contract`
> Depends on: `KU-CON-001` merged

## Objective

Define the missing product-facing orchestration and status contract around the
already implemented vNext outbound-first core. This task freezes composition;
it does not redesign the wire, session, reconciliation or authority model.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../CAPABILITY_STATUS.md`, `../PROGRESS.md`
- `../../../superpowers/specs/2026-08-14-onebrain-outbound-first-nat-traversal-design.md`
- `../../../specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md`
- `../../../specs/vnext/RUNTIME_OWNERSHIP_PROFILE_V1.md`
- `../../../specs/vnext/RUNTIME_LIFECYCLE_PROFILE_V1.md`
- `../../../specs/vnext/RUNTIME_FEATURE_BUDGET_PROFILE_V1.md`
- existing vNext product/API/status profiles directly touched by the proposal

## Deliverable

Add or amend an owner-reviewable vNext product profile and machine-checked
vectors that define:

- trusted-local bootstrap inputs: DNS/IP relay endpoint, signed manifest and
  manual invitation;
- discovery-source, reservation, advertisement and refresh lifecycle;
- expected-peer authentication and route selection states;
- durable network intent/outbox and retry/failover checkpoints;
- exact partial, disabled, degraded and error statuses;
- operator actions and redacted product projections;
- feature/default/kill/rollback behavior.

## Acceptance

- New public fields, statuses and commands are specified before implementation.
- Bootstrap location is not identity or content authority.
- A relay cannot authenticate as the expected peer or grant KU authority.
- “Connected”, “delivered”, “discovered” and “globally complete” are not
  conflated.
- Existing frozen wire and reconciliation semantics remain unchanged.
- Product enablement remains opt-in/default-off.
- Contract validators and `git diff --check` pass.

## Excluded

Runtime wiring, API handlers, UI, mobile/browser carriers and default rollout.
