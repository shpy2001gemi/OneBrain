# OneBrain vNext REST API Profile v1

> **Work package:** `DR-P3.1`
>
> **Status:** Implemented
>
> **Date:** 2026-07-26
>
> **Frozen parent contract:** [vNext Product Integration Profile v1](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md)
>
> **Code:** [`onebrain-api::vnext_api`](../../../src/onebrain-api/src/vnext_api.rs)

## 1. Implemented surface

P3.1 implements every previously reserved product endpoint without changing
its method, path, visibility, required request fields, required response
fields, CID encoding, continuation encoding, or semantic firewall:

| Method | Path | Runtime behavior |
|---|---|---|
| POST | `/api/vnext/kql/needs/prepare` | Validate local KQL, derive a private deterministic bundle and retain a short-lived local activation capability. |
| POST | `/api/vnext/kql/needs` | Activate the exact prepared bundle through `VNextProductServices`. |
| GET | `/api/vnext/kql/needs` | Page non-terminal encrypted-Vault projections in stable ID order. |
| GET | `/api/vnext/kql/needs/{id}` | Read one exact local typed StandingNeed. |
| GET | `/api/vnext/kql/needs/{id}/matches` | Page the current runtime quarantine projection. |
| POST | `/api/vnext/kql/needs/{id}/scan` | Run one bounded selector-scoped delta scan with a context-bound continuation. |
| DELETE | `/api/vnext/kql/needs/{id}` | Write a terminal retire tombstone and return an idempotent local projection. |
| POST | `/api/vnext/pomv/public-use/prepare` | Build the exact canonical Public Use payload preview without publishing it. |
| POST | `/api/vnext/pomv/public-use/confirm` | Consume the in-process typed consent capability after exact-intent confirmation. |
| GET | `/api/vnext/pomv/publications/{id}` | Read a durable pending/deferred publication projection. |
| GET | `/api/vnext/pomv/views/{target}` | Materialize/read a policy/frontier-relative partial evidence view. |
| GET | `/api/vnext/runtime/status` | Separate compiled, requested, active, kill-switch and signer readiness. |

All vNext routes MUST remain behind the existing constant-time local Bearer
authentication boundary.

Successful and runtime error responses MUST use the profile envelope, include
`meta.lifecycle`, `meta.coverage`, `meta.limitations` and nullable
`meta.continuation`, and keep legacy API envelopes unchanged.

## 2. Private Need boundary

Need preparation MUST validate the raw KQL locally, retain no raw query, and
return only the bounded intent, private QueryDefinition CID, selector, budget,
expiry and limitations allowed by the parent contract.

Activation MUST bind the exact prepared intent and idempotency key; exact
replay returns the same StandingNeed identity while conflicting reuse fails
closed.

Need list/get/retire operations MUST expose private identifiers only to the
authenticated local caller and never add them to WebSocket, telemetry, public
inventory or peer payloads.

Need scan and match pagination MUST remain one-hop, selector/context-bound,
budgeted and explicitly partial/path-limited.

Every match projection MUST remain `state = "quarantined"` and
`executable = false`; reading it cannot materialize or adopt a Mapping.

## 3. Public Use consent boundary

Public Use preparation MUST display the exact canonical payload bytes, target,
recipient, selector, namespace, Public/permanent acknowledgement, idempotency
identity and expiry before confirmation.

REST confirmation MUST require a context-bound `obc1` interaction receipt for
the exact `intent_cid` and then consume `PreparedPublicUseIntent::confirm`;
constructing that REST value alone cannot construct or replace the core typed
receipt.

The core single-use consent receipt MUST remain non-serializable and
in-process: it cannot enter a DTO, header, log, telemetry event, WebSocket
event, public inventory or peer payload.

Publication lookup and view retrieval MUST NOT create UseEvidence; only the
explicit confirm endpoint may enter the atomic consent/publication
transaction.

## 4. Coverage, policy and status truth

Metabolic Evidence View output MUST include target, allow-listed policy CID,
assessed authority frontier, revision, evidence root, conflicts, partial
coverage and limitations while all truth, benefit, reward and global
completion flags remain false.

Runtime status MUST report compiled, requested, active, kill-switch and
proof-checked Feed signer readiness as separate values with one of
`disabled`, `requested`, `active` or `degraded`.

Zero-result Need or PoMV responses MUST say local-only or partial/path-limited
and cannot claim that an object, match or event does not exist on the network.

## 5. Continuation and interaction receipt

List continuations use `obc1.` plus unpadded base64url over a versioned endpoint
kind, offset, typed context and domain-separated checksum. Scan continuations
project the runtime's selector-bound opaque 32-byte token using the same
wire prefix.

The REST interaction receipt is:

```text
obc1.base64url_no_pad(
  BLAKE3-256(
    "onebrain:vnext:rest-explicit-confirmation:1" || 0x00 ||
    u64be(intent_cid.length) || intent_cid
  )
)
```

It is a confirmation-gesture binding, not the core secret receipt. The local
Bearer-authenticated server still has to possess the unexportable typed
`PreparedPublicUseIntent`; after restart or capability loss, the client must
prepare and review again.

## 6. Executable evidence

`onebrain-api::vnext_api::tests` proves:

- disabled builds return the vNext envelope without starting a runtime;
- lowercase typed CID and context-bound continuation rejection;
- private Need prepare/activate exact replay, local paging, bounded zero-result
  scan, quarantine paging and idempotent retirement over a real node-owned
  runtime;
- Public Use preview contains no receipt, wrong confirmation is rejected,
  exact confirmation replay returns one publication identity, durable lookup
  succeeds, and the target view preserves all four false semantic flags; and
- runtime operations use cloneable service handles after releasing the global
  aggregate node mutex.
