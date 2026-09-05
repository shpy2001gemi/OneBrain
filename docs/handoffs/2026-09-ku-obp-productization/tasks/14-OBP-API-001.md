# OBP-API-001 — Local networking API and private event projection

> State: Planned
> Branch: `codex/obp-api-001-network-api`
> Depends on: `OBP-PROD-004` merged

## Objective

Expose the accepted OBP product contract once through the authenticated local
REST API and private WebSocket, without creating a second network runtime or
expanding authority.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../PROGRESS.md`
- accepted OBP product profile and `OBP-PROD-002..004` evidence
- `../../../specs/vnext/VNEXT_REST_API_PROFILE_V1.md`
- `../../../specs/vnext/VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md`
- the exact source handlers/DTOs changed by this task

## Deliverable

Authenticated endpoints/events for the frozen bootstrap, source health,
discovered peer, reservation, path, connection, outbox, retry/failover,
feature/kill and bounded operator-action contract.

## Acceptance

- All mutations call the shared node-owned façade.
- Bearer/private WebSocket, redaction and backpressure rules remain intact.
- No raw address bypasses validated bootstrap or expected-peer binding.
- DTOs distinguish disabled, partial, degraded, pending and failed state.
- Contract, API compatibility, auth and negative-leak tests pass.

## Excluded

CLI/Web/Desktop presentation, mobile, remote administration and rollout.
