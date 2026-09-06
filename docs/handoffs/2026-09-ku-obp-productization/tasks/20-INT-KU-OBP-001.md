# INT-KU-OBP-001 — Opt-in KU-to-peer product journey

> State: Planned
> Branch: `codex/int-ku-obp-001-product-journey`
> Depends on: `KU-QA-001` and `OBP-QA-001` merged

## Objective

Prove one consistent, explicitly opt-in journey from local KU creation through
authenticated peer reconciliation without weakening local-first operation,
consent, privacy, provenance or authority boundaries.

## Required read set

- `../README.md`, `../DECISIONS.md`, `../PROGRESS.md`
- accepted KU and OBP product/API/surface contracts and QA evidence
- `../../../specs/vnext/ADDITIVE_KU_WORKFLOW_SURFACE_V1.md`
- applicable publication, consent, reconciliation, provenance, route and
  private-field profiles
- exact end-to-end source and harness files changed by the task

## Deliverable

An end-to-end CLI/Web/Desktop journey and evidence for:

1. deterministic local compile of an approved resolved semantic draft and
   exact preview; model-assisted raw-text extraction uses the shared encoder
   and qualified KU-ENC-003 profiles, with its measured convergence limits;
2. private local save without network side effects;
3. explicit prepare/confirm publication intent;
4. durable outbound intent bound to an expected peer;
5. authenticated transport and reconciliation;
6. remote validation/materialization under local policy;
7. scoped delivery/reconciliation/provenance status after restart;
8. negative proof that private data and authority do not cross the boundary.

## Acceptance

- `encode ≠ publish`, `proposal ≠ materialize`, and `materialize ≠ adopt`
  remain true in code, API, UI and tests.
- Local KU use works with zero peers and with networking killed.
- Delivery/path/relay evidence does not become epistemic authority.
- Duplicate/retry/restart paths are idempotent or expose explicit conflict.
- CLI, Web and Desktop report the same canonical identifiers and states.
- All prescribed cross-surface, network, privacy and restart tests pass.

## Excluded

Outcome/Benefit/OBT economics, active multipath KQL, mobile/browser, mailbox
wake and default rollout.
