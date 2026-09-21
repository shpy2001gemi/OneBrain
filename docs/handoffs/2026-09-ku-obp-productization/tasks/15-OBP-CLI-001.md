# OBP-CLI-001 — Networking CLI workflow

> State: Review (local, uncommitted)
> Branch: `codex/obp-cli-001-networking`
> Depends on: `OBP-API-001` merged

## Objective

Provide a scriptable CLI workflow for configuring bootstrap inputs, observing
discovery/connectivity and initiating bounded expected-peer operations through
the local API.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../PROGRESS.md`
- accepted OBP product and API contracts
- `../../../specs/vnext/VNEXT_CLI_PROFILE_V1.md`
- current CLI transport, auth, output and error-convention files

## Deliverable

Commands, exact flags and stable output for importing/listing/removing approved
bootstrap inputs, inspecting sources/relays/peers/paths/outbox, connecting to an
expected peer, retrying/cancelling eligible intent and operating feature/kill
controls allowed by the frozen contract.

## Acceptance

- Commands use authenticated REST; no direct socket/runtime bypass exists.
- Machine-readable output uses the API's exact states and identifiers.
- Human output never implies global discovery, guaranteed delivery or trust.
- Destructive/capability-bearing actions require the specified confirmation.
- CLI unit/contract/integration tests pass.

## Excluded

New protocol semantics, UI, default enablement and raw private-key handling.

## Registered projection

[Exact CLI syntax and boundaries](../../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md)
map the accepted 13 API operations plus metadata reconciliation. Under the accepted
API, removal is source disable preserving replay floors; intent cancellation,
peer-directory and outbox-list have no operation and are not invented here.
Host intake/grants remain in-process prerequisites; no REST upload or raw dial.

[Implementation evidence and verification](../outputs/OBP_CLI_001_IMPLEMENTATION.md).
