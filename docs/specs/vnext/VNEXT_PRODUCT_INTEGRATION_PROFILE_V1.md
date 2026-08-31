# OneBrain vNext Product Integration Profile v1

> **Work package:** `DR-P1.1`
> **Status:** Frozen
> **Profile ID:** `VNEXT_PRODUCT_INTEGRATION_PROFILE_V1`
> **Version:** 1.1
> **Freeze date:** 2026-07-26
> **Machine contract:** [`product-integration-profile-v1.json`](../../../src/test-vectors/vnext/product-integration-profile-v1.json)
> **Implementation:** the twelve routes marked `reserved` at freeze time are
> implemented by [vNext REST API Profile v1](VNEXT_REST_API_PROFILE_V1.md);
> the table below retains its historical freeze-state column.

## 1. Scope and compatibility boundary

This profile freezes the additive product contract through which later P2/P3
runtime work becomes visible. It does not activate a runtime, publish a Need,
record consent, materialize or adopt a Mapping, grant authority, or connect
PoMV evidence to OBT.

Every vNext product endpoint MUST remain below `/api/vnext/...`. `/api/kql`,
`/api/watch`, `pomv`, and `pomv_breakdown` MUST retain the legacy meanings
frozen by the [legacy/vNext boundary ADR](LEGACY_VNEXT_PRODUCT_BOUNDARY_ADR_V1.md).

The two workflow endpoints are the only implemented product-contract views at
freeze time. All other endpoints below are reserved contracts for P2/P3 and
return no capability merely because they appear in this inventory.

Every endpoint requires the local Bearer-auth boundary. Need management and
Public Use prepare/confirm endpoints are additionally classified
`authenticated_local_private`; their private identifiers are not eligible for
WebSocket broadcast, telemetry, public inventory, or peer payloads.

Profile minor `1` additively reserves the product projection of Base
negotiation at `/api/vnext/base/negotiate`. It does not change any earlier
endpoint, mint management authority, or activate a runtime. The product route
projects only the bounded, product-neutral machine interface frozen by the
[Base v1 Runtime Interface Profile](BASE_V1_RUNTIME_INTERFACE_PROFILE.md).

## 2. Wire representation

REST payload field names use `snake_case`. JSON is a product projection only;
canonical objects/events remain deterministic CBOR under
[Canonical Profile v1](CANONICAL_PROFILE_V1.md).

- Typed 32-byte CIDs MUST be encoded as exactly 64 lowercase hexadecimal
  characters without `0x`, padding, truncation, or a textual type prefix.
- A field name retains its typed role (`object_cid`, `event_cid`,
  `selector_cid`, `policy_cid`, and so on); equal-width digests are not
  interchangeable.
- Pagination continuations MUST use the opaque
  `obc1.<base64url-without-padding>` representation, remain bound to their
  original query/context, and stay at or below 2,048 characters.
- Clients MUST treat continuation bytes as opaque and must not derive
  authority, completeness, ordering, identity, or reward from them.

## 3. Common response envelope

Successful vNext responses contain:

| Field | Meaning |
|---|---|
| `ok` | `true` |
| `profile` | `VNEXT_PRODUCT_INTEGRATION_PROFILE_V1` |
| `data` | Endpoint-specific DTO |
| `meta` | `VNextMetaV1` |

Errors contain the same `ok`, `profile`, and `meta` positions plus a
`VNextErrorV1` in `error`. `VNextMetaV1` always exposes `lifecycle`,
`coverage`, `limitations`, and nullable `continuation`.

The lifecycle vocabulary is exactly `disabled`, `requested`, `active`, and
`degraded`. Coverage is `local_only` or `partial`; work may additionally be
`pending`, `deferred`, `quarantined`, or `conflict`. A zero-result response
MUST preserve its assessed scope and MUST NOT claim network-wide absence.

## 4. Frozen endpoint inventory

| Method | Endpoint | Request DTO | Response DTO | Freeze state |
|---|---|---|---|---|
| GET | `/api/vnext/workflow` | — | `WorkflowStagePageV1` | implemented, read-only |
| GET | `/api/vnext/workflow/{stage}` | — | `WorkflowStageViewV1` | implemented, read-only |
| POST | `/api/vnext/kql/needs/prepare` | `NeedPrepareRequestV1` | `PreparedNeedV1` | reserved |
| POST | `/api/vnext/kql/needs` | `NeedActivationRequestV1` | `NeedViewV1` | reserved |
| GET | `/api/vnext/kql/needs` | — | `NeedPageV1` | reserved |
| GET | `/api/vnext/kql/needs/{id}` | — | `NeedViewV1` | reserved |
| GET | `/api/vnext/kql/needs/{id}/matches` | — | `MatchPageV1` | reserved |
| POST | `/api/vnext/kql/needs/{id}/scan` | `NeedScanRequestV1` | `NeedViewV1` | reserved |
| DELETE | `/api/vnext/kql/needs/{id}` | — | `NeedViewV1` | reserved |
| POST | `/api/vnext/pomv/public-use/prepare` | `PublicUsePrepareRequestV1` | `PreparedPublicUseV1` | reserved |
| POST | `/api/vnext/pomv/public-use/confirm` | `PublicUseConfirmRequestV1` | `PublicationViewV1` | reserved |
| GET | `/api/vnext/pomv/publications/{id}` | — | `PublicationViewV1` | reserved |
| GET | `/api/vnext/pomv/views/{target}` | — | `MetabolicEvidenceViewV1` | reserved |
| POST | `/api/vnext/base/negotiate` | `BaseNegotiationRequestV1` | `BaseNegotiationViewV1` | reserved |
| GET | `/api/vnext/runtime/status` | — | `RuntimeStatusV1` | reserved |

P2/P3 may implement a reserved endpoint without changing its method, path,
wire encodings, semantic firewalls, or required DTO fields. Additive optional
fields require a profile-minor revision; incompatible changes require a new
major profile.

P3.2 adds ticket minting and WebSocket upgrade paths as an extension contract
under the same `/api/vnext/...` namespace. Their authentication, immutable
subscription, event vocabulary, private-field suppression and backpressure
rules are frozen separately by the
[vNext Private WebSocket Profile v1](VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md).

## 5. DTO boundaries

The machine contract freezes every required field. These groups explain their
security ownership:

| DTO group | Required product meaning |
|---|---|
| `NeedPrepareRequestV1`, `PreparedNeedV1` | Raw/local query input stays local; the response exposes bounded intent, QueryDefinition/Selector CIDs, budget, expiry, and limitations. |
| `NeedViewV1`, `NeedPageV1`, `NeedScanRequestV1` | State, revision, scoped coverage, budget, idempotency, limitation, and opaque continuation remain explicit. |
| `QuarantinedMatchV1`, `MatchPageV1` | Responder scope, selector/frontier, constraint results, and limitations accompany a non-executable quarantined proposal. |
| `PreparedPublicUseV1` | Exact canonical payload preview, target, recipient, selector, namespace, disclosure, idempotency key, and expiry are visible before confirmation. |
| `PublicUseConfirmRequestV1`, `PublicationViewV1` | Confirmation binds a single prepared intent and single-use receipt; publication state/revision is separate from confirmation. |
| `MetabolicEvidenceViewV1` | Target, policy, assessed frontier, revision, evidence root, conflicts, coverage, and limitations remain visible with all truth/benefit/reward/global flags false. |
| `BaseNegotiationRequestV1`, `BaseNegotiationViewV1` | Profile major/minor, bounded capabilities, compatibility tuple/digest, lifecycle, coverage, and limitations are explicit; no raw runtime, store, path, signer, or management handle is projected. |
| `RuntimeStatusV1` | Compiled, requested, active, kill-switch, signer readiness, lifecycle, coverage, and limitations are separate fields. |

The runtime semantics behind `PreparedPublicUseV1` and
`PublicUseConfirmRequestV1` are frozen by the
[Strong Public Use Consent Profile v1](PUBLIC_USE_CONSENT_PROFILE_V1.md).
The node-owned aggregate and typed service boundary behind all reserved
runtime endpoints are frozen by the
[Runtime Ownership Profile v1](RUNTIME_OWNERSHIP_PROFILE_V1.md).

Responses MUST NOT expose raw queries, private targets, signer private keys, or
single-use receipts. Authenticated local-private Need responses may expose
`standing_need_id` and `query_definition_cid` to the requesting local client,
but those fields MUST NOT enter WebSocket, telemetry, public inventory, or peer
payloads. Requests MUST NOT let a client supply `authorized`, an authority
frontier, a policy implementation, or signer private-key material.

## 6. Error and retry semantics

| Code | HTTP | Retryable | Meaning |
|---|---:|---|---|
| `invalid_request` | 400 | no | Malformed or semantically invalid input |
| `not_found` | 404 | no | No local object for the exact typed identifier |
| `conflict` | 409 | no | Concurrent/incompatible state requires explicit resolution |
| `expired` | 410 | no | Prepared intent or receipt is no longer valid |
| `rate_limited` | 429 | yes | A bounded local admission window rejected this attempt |
| `capability_disabled` | 503 | no | Compile/config/kill-switch state deliberately disables the lane |
| `dependency_unavailable` | 503 | yes | A required signer, store, route, or runtime dependency is temporarily unavailable |
| `internal_error` | 500 | no | Unclassified failure; retry is denied unless a narrower code says otherwise |

Retries MUST reuse the original idempotency identity and opaque continuation
where the endpoint accepts them. A retryable transport/dependency failure does
not upgrade `pending` or `deferred` work to accepted, published, or complete.

## 7. Semantic firewalls

1. Every returned proposal remains `quarantined` and `executable = false`.
2. A proposal cannot materialize a Mapping, grant authority, create an active
   OBKG edge, invoke a tool, or alter a wallet.
3. A Metabolic Evidence View always reports
   `establishes_truth = false`, `establishes_benefit = false`,
   `authorizes_reward = false`, and `claims_global_completion = false`.
4. Coverage and limitation fields cannot be omitted to make a partial result
   appear complete.
5. Loading, retrieving, presenting, or paging a result cannot create
   UseEvidence.

These firewalls apply to REST, CLI, Desktop, Web, and the
[private WebSocket extension](VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md) of the
same product contract. The executable command projection is frozen by the
[vNext CLI extension](VNEXT_CLI_PROFILE_V1.md).

## 8. Acceptance evidence

`scripts/ci/validate_vnext_contracts.py` validates the frozen endpoint/DTO
inventory, CID and continuation encodings, lifecycle/error semantics, legacy
meaning, forbidden fields, and fail-closed semantic flags.

`scripts/ci/test_validate_vnext_product_profile.py` proves that namespace
escape, client-supplied authority, executable proposals, reward-authorizing
PoMV views, and legacy meaning changes are rejected.
