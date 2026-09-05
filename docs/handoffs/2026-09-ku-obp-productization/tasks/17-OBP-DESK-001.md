# OBP-DESK-001 — Desktop networking integration

> State: Planned
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
