# KU-DESK-001 — Desktop KU integration

> State: Planned
> Branch: `codex/ku-desk-001-workflow`
> Depends on: `KU-WEB-001` merged

## Objective

Integrate the accepted Web KU workflow into the Tauri Desktop lifecycle using
the same embedded node/API and durable shutdown/restart boundaries.

## Required read set

- package README/decisions/progress
- KU product/API/Desktop-Web profiles
- accepted Web implementation
- Desktop bootstrap, sidecar, tray, shutdown and event-bridge modules/tests

## Acceptance

- Desktop embeds/reuses the Web surface; it does not fork KU semantics.
- Startup dependency failures are visible and local reads remain available when safe.
- Quit/restart drains or preserves nonterminal durable work per contract.
- No private keys or local capabilities cross the Web bridge.
- Desktop check/build and focused lifecycle tests pass on the available host;
  missing platform evidence remains explicit.

## Excluded

Installer/store release, mobile and OBP network UI.
