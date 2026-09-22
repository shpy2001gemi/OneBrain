# OBP-WEB-001 — Local Web networking workflow

> State: Merged — `7f49eeb`, owner acceptance D-032
> Branch: `codex/obp-web-001-networking`
> Depends on: `OBP-API-001` merged

## Objective

Implement a comprehensible local Web workflow for bootstrap, discovery,
outbound reachability, connection state and durable network intent using only
the accepted local API/private event stream.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../CAPABILITY_STATUS.md`, `../PROGRESS.md`
- accepted OBP product/API contracts
- current Web design tokens, component/pattern rules and network-related UI
- private WebSocket security/backpressure profile

Registered interactions: [Web projection](../../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md).
Implementation and verification: [evidence](../outputs/OBP_WEB_001_IMPLEMENTATION.md).
API/node/CLI implementation remains unchanged. Host provisioning, raw import,
peer-directory/outbox-list and live-host activation remain outside this adapter.

## Deliverable

- Bootstrap/import and source-health views.
- Relay reservation, learned-peer and candidate-path views.
- Expected-peer connect and outbox/retry/failover workflow.
- Clear disabled/partial/degraded/error/kill states.
- In-product explanation of DNS/IP location, cryptographic peer identity,
  relay limitations and no-global-completeness guarantees.

## Acceptance

- UI uses shared REST/WS state and survives refresh/restart coherently.
- No private Need/query/Vault/key/capability material leaks into public views,
  logs or analytics.
- Accessibility, responsive layout and focused interaction tests pass.
- Product copy stays within `DECISIONS.md` claim boundaries.

## Excluded

Desktop process lifecycle, mobile/browser transport and protocol changes.
