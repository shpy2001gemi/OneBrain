# OBP-WEB-001 — Local Web implementation evidence

2026-09-22. Original workspace `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-web-001-networking`, clean starting main `2d4d5f4`. CLI merge `04bcb30`
and API merge `7d37a30` remain ancestors. D-029 transport approval is satisfied.
Status: Review, local/uncommitted; no push or merge in this continuation.

## Implementation

[Exact interactions](../../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md) were
registered before implementation. `/network` now projects the shared node-owned
service through the six accepted OBP routes, covering all 13 operations and
metadata reconciliation. The old raw-address connect/legacy peer list is removed
from this page. No runtime, protocol, CLI, host configuration or API code changed.

The private adapter shares existing local authentication configuration while
bypassing debug logging. It accepts only numeric loopback HTTP(S) origins,
disallows redirects, omits ambient credentials, bounds requests by a 30-second
deadline and responses by 1 MiB, and rejects duplicate keys, excessive nesting,
unsafe integers, unknown DTO fields and false route/acknowledgement claims.
Parent DTO types/enums and transport error policy come from accepted inventories.
Private IDs remain in POST bodies and authenticated local views.

Source-health and reservation views use bounded snapshot pages, retaining the
exact continuation. Expiry/conflict clears the page and requires an explicit
first-page reload. Expected-peer connection and exact route/intent inspection
project scoped state, path kind, limitations and checkpoint evidence. Pending
intent retry preserves node-owned counters/bytes/consent. No browser planner or
retry scheduler is introduced.

All mutations require an immutable operation/key/generation/payload preview and
typing the exact key. Independent network/advertisement opt-ins start off.
Management requires an in-memory host-issued capability, cleared after dispatch;
advertising also requires acknowledgement of the host's public-fields/expiry
preview. Missing host service/control/intake/grants remain explicit and fail
closed. Source import consumes an existing typed input_ref; no raw upload, QR,
file, URL-fetch or capability-minting port is invented.

Before dispatch, the original operation/payload/session/origin is saved in this
tab's sessionStorage. Failure to save prevents dispatch. Reload restores recovery
only. Unknown results block further commands; recovery reads fresh status then
reconciles the original key within the same dataset. A new process is accepted
only for that read; dataset replacement and local_not_found do not clear unknown
work. Completed metadata is not presented as a full result or delivery. Known
completed/failed_no_effect/not-admitted outcomes require acknowledgement before
preparing another action. No command is automatically replayed.

Private WS uses fresh scoped single-use tickets and an independent in-memory
client-session capability, never the Bearer in a URL. Closed schema/sequence
validation accepts only readiness and aggregate network hints. A gap or invalid
frame closes the stream, clears stale details and triggers REST reads only.
Reconnect is explicit and obtains a fresh ticket; hints are coalesced during
reads. Unmount/session change clears the socket. No peer carrier is implemented.

UI reuses current Web tokens and glass-card/input/button patterns, with labelled
controls, typed state independent of color, focus on command confirmation,
wrapping identifiers, reduced-motion support and a one-column narrow layout.

## Verification

Run from `src/onebrain-web`:

```powershell
npm run test:ku
npm run test:vnext
npm run build
npm run lint
```

Run from repository root:

```powershell
python -m unittest scripts.ci.test_validate_obp_local_api scripts.ci.test_validate_obp_product_contract scripts.ci.test_validate_vnext_cli_profile
python scripts/ci/validate_vnext_contracts.py
git diff --check
```

The Web suite includes accepted parent fixtures, all operation/route mappings,
all 13 error policies, wrong-peer/malformed outcomes, strict JSON/bounds, private
credentials, lost response, process restart, dataset replacement, metadata-only
reconcile, missing outcome, expired pagination, management/advertisement gates,
storage failure and WS sequence/profile/privacy/gap behavior. Existing KU tests
and frozen receipt vectors remain green. Final results: 82 Vitest tests (60 OBP
and 22 existing KU), two receipt tests, 25 Python tests, production build and
aggregate vNext validation pass. Lint exits successfully with only the eight
existing unrelated warnings. Whitespace validation passes.

For reproducible browser QA, run a temporary Vite server with
`npm run dev -- --host 127.0.0.1 --port 5197 --strictPort`, then from the root run
`node src/onebrain-web/tests/obp-browser-check.cjs`. Optional
`OBP_SCREENSHOT_DIR` saves screenshots. The committed harness uses isolated test
fixtures and blocks every external/live-host request. Chromium checks at 1280px
and 390px passed: correct columns, no horizontal overflow, labelled fields,
no page exceptions and axe including color contrast. This tests Web responsiveness,
not a mobile app or a physical platform networking qualification.

Initial test development found a missing reservation-page mock and an assertion
that treated a GET body as a string. Both fixtures were corrected. No production
API/node changes were needed. Lint has eight existing warnings in unrelated
files and no new OBP warnings. Build and vNext contracts pass.

## Limits and retained boundaries

- This is adapter/interaction/browser fixture evidence. It is not a fresh real
  node/browser end-to-end, Internet/NAT, relay failover or platform qualification
  run. Existing real-node API and CLI evidence retains its own scope.
- The live OneBrainLocal service is not activated or rebuilt. Registry, jobs,
  keys, Ollama, models, networking opt-ins and rollout remain unchanged.
- Host-issued intake/grants remain prerequisites. Advertising preview is
  supplied by the trusted host, not discoverable through an invented endpoint.
- There is no peer-directory, outbox-list, source-delete or intent-cancel API.
  Source disable preserves replay floors. Exact route/intent IDs are supplied
  by their originating workflow; candidate attempt history stays private.
- Recovery is scoped to the same browser tab/sessionStorage, not a new durable
  command owner. Retain the original key/context if discarding that tab. Unsafe
  JS integer ranges fail closed rather than rounding a durable generation.
- No mobile/Desktop lifecycle, model tuning, default rollout or automatic
  networking activation is included. The branch awaits owner review.
