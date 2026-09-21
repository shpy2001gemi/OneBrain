# OBP-API-001 — Transport prerequisite review

Historical checkpoint, superseded for implementation status by
[implementation evidence](OBP_API_001_IMPLEMENTATION.md). The owner accepted
this complete transport under D-029; handler registration is now true.

2026-09-21. Original workspace `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-api-001-network-api`, baseline `836af91`. Task 004's merge dependency
is satisfied at `2c39117`. This checkpoint is a local uncommitted specification
and validation change; no new API handler or runtime implementation is claimed.

## Concrete proposal

[Local OBP API profile](../../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md) defines:

- Six exact routes: status, query, commands, reconcile, WS ticket and WS upgrade.
- All 13 accepted operations with unchanged request/result DTO and access class.
- Host-installed principal/control grant, independently scoped management
  capability and in-process typed host input references; no REST raw upload.
- Node-owned durable operation admission/recovery, exact replay, unknown effects,
  generation fences and bounded immutable pagination.
- A separate WS profile on separate routes, sharing existing hub resource caps;
  only readiness and redacted aggregate network state, never private identifiers.
- Finite error details under the existing outer codes, strict JSON parsing,
  explicit resource ceilings and implementation acceptance tests.

No previously accepted parent inventory, DTO, wire signature or WS vocabulary was
edited. The machine proposal pins five parent inputs with normalized-text SHA-256
and keeps handlers_registered=false; this is a checker pin, not a canonical CID.

## Source findings

Inspected the existing API router/auth middleware, KU transport, vNext REST/WS
DTOs and hub, plus node product façade, outbound owner, discovery inputs, rollout
and outbox ownership. The existing façade exposes redacted status/source/
reservation snapshots. It does not yet provide all 13 admitted operations,
the new command journal or a host-scoped OBP management capability.

Consequently, mapping JSON directly to existing low-level kill/dial methods
would bypass required capability, replay and generation semantics. The proposal
requires completing the common node façade before thin handlers. Runtime
integration and crash/auth/backpressure tests remain implementation work.

## Verification

- `python -m unittest scripts.ci.test_validate_obp_local_api scripts.ci.test_validate_obp_product_contract -v`
  — 19 tests passed (13 transport proposal checks and six accepted-parent checks).
- `python scripts/ci/validate_obp_local_api.py` — six proposed routes, 13 operations,
  35 positive/negative request, outcome and event fixtures passed.
- `python scripts/ci/validate_vnext_contracts.py` — passed, including the new checker.
- `git diff --check` — passed.

Mutations cover privilege escalation, raw input transport, duplicate nested JSON,
nonfinite numbers, wrong operation route, stale shape/field injection, resource
bounds, unknown-outcome retry, false active state, WS identity/address leaks and
false reward claims. These tests validate contract structure, not a live API,
node crash recovery, token expiry or actual networking behavior. No Rust changes
were made, so previously recorded runtime results were not rerun or relabeled.

## Review boundary

The accepted OBP product profile section 5 says:

> aggregate counts alone are eligible for a separately reviewed notification projection.

It also requires exact routes, capability binding, bounded failure DTO and
reconciliation transport before handlers. The separate notification review is an
explicit requirement. Presenting the complete REST/WS transport together for that
review is the implementation sequencing choice here; the text does not separately
require a second approval for each REST route.

Owner review should accept or amend this concrete transport proposal. Do not
record a new owner decision or enable handlers merely because proposal checks pass.
After acceptance, implement and test the shared façade, then REST/WS projection.
Model tuning, live-host changes, networking activation, mobile and default rollout
remain excluded. No permission to merge task OBP-API-001 is inferred from D-028.
