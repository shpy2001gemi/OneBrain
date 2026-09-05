# OBP-MIG-001 — Retire the legacy seed product path

> State: Planned
> Branch: `codex/obp-mig-001-retire-legacy-seed`
> Depends on: `OBP-QA-001` merged

## Objective

Remove the legacy TCP/JSON `SeedClient`/`onebrain-seed` path from the normal
product workflow only after vNext product parity is accepted, while retaining
an explicit compatibility/rollback path for the approved window.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../PROGRESS.md`
- accepted `OBP-QA-001` evidence and the migration/rollback section of the OBP
  product contract
- `../../../specs/vnext/LEGACY_VNEXT_PRODUCT_BOUNDARY_ADR_V1.md`
- legacy seed/client call sites, packaging, docs and feature flags
- current rollback and release evidence requirements

## Deliverable

- Complete legacy-path inventory and migration map.
- Normal product path switched to the accepted vNext orchestration when its
  explicit feature is enabled.
- Legacy client/daemon isolated behind an accurately named compatibility flag
  or removed only where the approved migration says it is safe.
- Rollback test/evidence and corrected README/operator documentation.

## Acceptance

- No documentation calls `onebrain-seed` a secure vNext seeder/relay.
- No silent data/config loss and no automatic trust migration occurs.
- Rollback is tested and does not silently enable networking.
- Dead legacy entry points are removed only after call-site and packaging audit.
- Applicable validators, builds and migration tests pass.

## Excluded

Remote Git branch deletion, default-on rollout, wire redesign and mobile.
