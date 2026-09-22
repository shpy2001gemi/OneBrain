# OBP-DESK-001 — Desktop networking integration

> State: Merged
> Branch: `codex/obp-desk-001-networking`
> Depends on: `OBP-WEB-001` merged

## Objective

Integrate the accepted local Web networking workflow with the Desktop-owned
process lifecycle, local credential boundary and sleep/resume behavior.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../PROGRESS.md`
- accepted `OBP-WEB-001` output and OBP API/lifecycle contracts
- current Desktop shell, process supervision, local auth and packaging files
- applicable lifecycle, concurrency and resource-budget profiles

## Deliverable

- Desktop start/stop supervision of the one node-owned network service.
- Secure local API/session handoff to the embedded Web surface.
- Sleep, resume, network-change and clean-exit handling.
- Bounded Desktop/tray status derived from canonical API state.
- Focused packaging and lifecycle tests.

## Acceptance

- Desktop never spawns a second peer identity or hidden network runtime.
- Restart/resume preserves identity and durable nonterminal intent.
- Disabled/kill state survives Desktop restart and performs no network work.
- Status text remains partial/scope-honest and matches CLI/Web semantics.
- Applicable Desktop build and tests pass.

## Excluded

Mobile, browser/WASM carrier, default enablement and release qualification.

## Registered Desktop interactions

See [Desktop projection](../../../specs/vnext/OBP_LOCAL_DESKTOP_PROJECTION_V1.md).
The existing explicit Restart action reconstructs dependencies after native
lifecycle fencing. Host provisioning remains in-process; no automatic network
activation or new OBP operation is included.

[Implementation evidence and platform limits](../outputs/OBP_DESK_001_IMPLEMENTATION.md).
Owner accepted review under D-033 and authorized merge/publication under D-034.
Implementation `410a6a0` merged as `7e4fc14`; the Desktop branch is retained.
