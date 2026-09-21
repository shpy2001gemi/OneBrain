# Local KU REST projection v1

Task: KU-API-001. This additive transport projects the registered
[KU workflow](KU_PRODUCT_WORKFLOW_PROFILE_V1.md), without new Base IDs or
canonical semantics. Existing REST paths and WS vocabulary remain unchanged.

KU-WEB-001 adds a separately specified opt-in [manual editor transport](KU_LOCAL_EDITOR_PROFILE_V1.md).
The three operations routes below retain their KU-API-001 meanings; the editor
is a host input adapter and is unavailable unless explicitly installed.

## Routes and payloads

All routes require the existing constant-time local Bearer authentication.
They are compiled with `base-v1` (enabled by default) and work with the local
Base owner; network-runtime is not required.

| Method | Path | Body / result |
|---|---|---|
| GET | `/api/vnext/ku/status` | No body. Returns current session generations and `KuStatusV1`. |
| POST | `/api/vnext/ku/reservations` | `{session}`. Reserves one node-owned operation; returns `KuOperationRefV1`. |
| POST | `/api/vnext/ku/operations` | `{session, budget, request: {operation, payload}}`. Invokes one registered KU operation. |

`session` contains `process_generation` and `dataset_generation`, each exactly
64 lowercase hexadecimal characters. Obtain them from status. They are fences,
not credentials. Both POST routes reject stale/malformed generations before
dispatch. After restart, obtain current generations and reconcile the original
operation ID rather than repeating extraction or save blindly. A lost reservation
reply can leave unused reserved work; reservations do not prepare or save data.

`budget` contains integer `max_items` (1–256), `max_bytes` (32768–1048576),
and `max_work_units` (1–1000000). No omitted or null budget fields. REST reserves
16384 bytes from `max_bytes` for response metadata/envelope; the remainder is
passed to the service. The whole request must fit `max_bytes` and 1 MiB.
Responses are bounded to `max_bytes`; post-dispatch overflow reports a typed
reconcile-required failure, never a fabricated rollback or success.

`operation` selects exactly one generated request/response pair:

| Operation | Payload | Result |
|---|---|---|
| prepare | KuPrepareV1 | KuPreparedV1 |
| preview | KuOperationRefV1 | KuPreparedV1 |
| save | KuSaveV1 | KuReceiptV1 |
| get | KuGetV1 | KuViewV1 |
| list | KuListV1 | KuPageV1 |
| search | KuSearchV1 | KuPageV1 |
| revise | KuReviseV1 | KuPreparedV1 |
| export | KuExportV1 | KuExportViewV1 |
| status | KuStatusRequestV1 | KuStatusV1 |
| cancel | KuOperationRefV1 | KuReceiptV1 |
| reconcile | KuOperationRefV1 | KuReceiptV1 |

Transport structs and generated payloads reject unknown/duplicate fields.
Payload identifiers retain their generated lowercase hex types and canonical
previews use canonical padded base64. Optional generated fields are omitted,
not null. Continuations remain unchanged opaque `obc1` tokens, at most 2048
characters; the service owns snapshot/context checks. Search and private IDs
remain in POST bodies, not URLs. No raw path, source text, Registry resolution,
draft intake or provider installation endpoint is introduced here. Prepare uses
previously admitted opaque references through host-installed custody ports.

## Responses and capability meaning

Use the existing vNext success envelope. `data` contains `session`, `payload`
(the generated result), and `model_qualified: false`. This release has no
qualified real-model tuple; AI prepare/revise is rejected before dispatch unless
the host explicitly admits the exact experimental implementation under D-023
and [the experimental Ollama profile](KU_EXPERIMENTAL_OLLAMA_PROFILE_V1.md).
Existing rule and resolved-draft requests use `KuServices::invoke` unchanged.
Qualified/default AI still requires the separate measured qualification integration.

`meta.lifecycle`, `coverage`, `limitations`, and nullable `continuation` preserve
the result's relevant service values. Prepared validity is distinct from Base
operation state. All responses state local scope and unqualified-model limits;
`local_encoder_ready` is service availability, never arbitrary-text model
qualification. A `needs_resolution` result remains HTTP success with its actual
issues/empty artifacts; it is not a saveable complete output.

Errors retain the existing eight REST codes and additionally carry a generated
`KuFailureV1` under `error.failure`, plus numeric Base `discriminator`.
Inside `error.failure`, retryability and `reconcile_before_retry` are copied
from Base policy; the outer error retains the REST table's retry flag. In
particular UnknownOutcome retains `failure.reconcile_before_retry = true`
even though outer `conflict` is not automatically retryable. Request
validation errors use InvalidRequest without reflecting private request text.

| Base code | REST code | HTTP |
|---|---|---|
| InvalidRequest | invalid_request | 400 |
| NotFound | not_found | 404 |
| Conflict / UnknownOutcome | conflict | 409 |
| Expired | expired | 410 |
| RateLimited | rate_limited | 429 |
| CapabilityDisabled / IncompatibleProfile | capability_disabled | 503 |
| ResourceExhausted | rate_limited | 429 |
| DependencyUnavailable | dependency_unavailable | 503 |
| CorruptState / ReprovisionRequired / InternalError | internal_error | 500 |

Authentication failures use invalid_request with HTTP 401 (missing/malformed)
or 403 (incorrect proof); no runtime access occurs. All KU responses include
`Cache-Control: no-store`. These transport errors do not change Base policy.

## Concurrency and privacy

Acquire a cloneable, authenticated KU handle and generation snapshot under the
node mutex, then release the mutex before service work or response transmission.
Base retains resource, principal, generation and durable-operation ownership.
No private source, canonical preview, identifier or capability is logged or
sent on legacy broadcast/private WS. KU clients poll the authenticated API;
no new notification channel or unbounded subscriber queue is introduced.
An API disconnect does not authorize replay; reconcile through the owner.

## Minimal client sequence

1. GET status; retain its session generation fields.
2. POST reservations with that session.
3. POST operations/prepare with the reserved ID, a fresh idempotency key,
   host-admitted references and the exact pinned implementation/Registry values.
4. Read the returned preview and validation issues. Save only a ready preview
   with its exact object set and original operation/idempotency identities.
5. Search/list/get through the same authenticated service. Save is private and
   does not publish, adopt, create UseEvidence or issue OBT.

The API alone does not install source custody, a signed Registry, Vault keys
or a draft editor. Those are explicit host/product integration dependencies,
not values a browser may invent. Integration tests use test-only signed
Registry/custody fixtures; they are not model qualification evidence.
