# KU-API-001 implementation evidence

The local authenticated REST adapter now projects all eleven registered KU
operations through the existing node-owned service. It adds no encoder,
semantic compiler, storage authority, Registry fallback or private WS events.
This is the API prerequisite for the early local Web MVP under D-021; it is
not yet an end-user application or a qualified arbitrary-text AI encoder.

## Delivered boundary

The additive [REST contract](../../../specs/vnext/KU_LOCAL_REST_PROFILE_V1.md)
defines three routes, compiled with the default `base-v1` feature:

| Route | Use |
|---|---|
| `GET /api/vnext/ku/status` | Read service readiness and current generation fences. |
| `POST /api/vnext/ku/reservations` | Reserve a node-owned operation with the current session. |
| `POST /api/vnext/ku/operations` | Prepare, preview, save, get, list, search, revise, export, status, cancel or reconcile using generated DTOs. |

The adapter validates JSON, budgets and generation fences; the service retains
principal/custody checks, exact canonical bytes and IDs, atomic save, revision
frontiers, opaque continuations and replay/reconciliation semantics. Requests
are bounded to 1 MiB, and operation budgets reserve space for the response
envelope. Unknown, duplicate, malformed and explicit-null optional fields fail
without echoing private input. Errors preserve all thirteen Base codes through
the generated failure DTO while retaining the existing outer REST vocabulary.

All KU responses use `Cache-Control: no-store`. IDs and search terms stay in
POST bodies. The node mutex is released before service execution; cancellation
can reach an operation during extraction. KU introduces no broadcast producer
or subscriber queue. Clients poll the authenticated endpoint; existing bounded
private WS behavior remains separate.

Every successful envelope states `model_qualified: false`. Local AI preparation
and revision fail before service dispatch even if a host advertises an AI
implementation. Readiness does not imply model qualification. Resolved drafts
and supported local-rule requests delegate to the existing service unchanged;
unresolved preparation exposes the service's issues and empty artifacts.

## Integration limits and next product step

The default node does not install KU custody inputs, Vault keys or a verified
Registry on behalf of a browser. Without these host dependencies, status returns
a typed dependency failure. The API adds no source-upload or draft-intake
endpoint. A Web MVP still needs an explicitly scoped host intake/editor path,
installation of those dependencies, and its actual screen flow. Credentials,
source references and pinned implementation/Registry values cannot be invented
by a client. The task does not claim a runnable full demo yet.

Export retains owner policy: private KUs cannot become canonical public exchange
through this adapter, and encrypted archives require separate Base-management
authority. Save remains private and does not publish or authorize reward.

KU-ENC-003 remains separate at its preserved branch checkpoint. No private
VI/EN holdout workbook was opened, no model inference or download occurred, and
no quality measurement, qualification completion or rollout change is claimed.

## Verification

Commands run locally on Windows. Cargo commands use the `src` directory.

| Command | Evidence |
|---|---|
| `cargo test --locked -q -p onebrain-api` | 22 library and 8 integration tests pass, including seven new KU tests. |
| `cargo test --locked -q -p onebrain-api --features vnext-network-runtime --lib` | 24 tests pass, including private WS and KU integration under the opt-in build. |
| `cargo check --locked -q -p onebrain-api --no-default-features` | Pass; feature-disabled build remains valid. |
| `cargo fmt --all -- --check` | Pass. |
| `python -m scripts.base.generate_contract --check` | Existing generated Rust, TypeScript and Dart projections unchanged. |
| `python scripts/ci/validate_vnext_contracts.py` | Pass; existing inventories and specification links remain valid. The original 15-route product inventory remains unchanged; the three additive KU routes are specified separately and exercised by API tests. |
| `git diff --check` | Pass. |

The new integration tests use temporary node storage and a test-signed,
one-concept Registry with fixture source custody. They cover exact
prepare/preview/save/get and replay, list/search pagination and context rejection,
revision/predecessor preservation, export refusal, status/cancel/reconcile,
stale session rejection, authentication, malformed/private payloads, revoked
source access, unresolved saves, unqualified AI refusal, response overflow,
all Base retry/reconcile policies and cancellation without retaining the node
mutex. No fixture is a model-quality or production-custody claim.

Existing unused-code/import warnings remain in dependency/feature builds; this
is not a zero-warning claim or a remote CI execution claim.

Closure: the owner accepted reviewed tip `423b7b8` under D-022. Merge `3eba370`
is on `origin/main`; fresh default API tests and contract/format checks pass.
KU-WEB-001 is the next Planned task for the owner's new conversation. These
accepted implementation limits remain in force after merge.
