# OneBrain vNext — Strong Public Use Consent Profile v1

> **Work package:** `DR-P1.3`
> **Status:** Frozen — 2026-07-26
> **Code:** [`onebrain-node::vnext_distributed_pomv`](../../../src/onebrain-node/src/vnext_distributed_pomv.rs)
> **Product contract:** [vNext Product Integration Profile v1](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md)
> **Replaces:** arbitrary non-zero `ExplicitUseConfirmation`

## 1. Scope and security objective

This profile freezes the runtime-core boundary for publishing a Public
`UseEvidence` object. Consent is a two-step, expiring, exact-intent state
transition. It is not inferred from a query, retrieval, presentation, non-zero
commitment, signer availability, or possession of an idempotency key.

Only `DisclosureClass::Public` is accepted. This profile does not grant Feed,
Actor, transport, policy, truth, benefit, reward, wallet, or OBT authority.
Product surfaces remain reserved for P3 and must preserve the separate
`prepare` and `confirm` actions frozen by the product integration profile.

## 2. Prepare contract

`PreparePublicUseEvidenceRequest` carries the full typed `UseEvidencePayload`
and these review fields:

| Field | Exact meaning |
|---|---|
| `exact_target` | Typed object reference that must occur in `payload.subjects`. |
| `expected_peer` | Exact recipient `NodeID`, not a route address. |
| `selector` | Exact `SelectorCID` used for reconciliation. |
| `namespace` | Exact namespace commitment. |
| `disclosure` | Exactly `Public`. |
| `idempotency_key` | Non-zero operation identity scoped by author `FeedID`. |
| `expires_at` | Unix seconds, strictly future and at most 900 seconds from the trusted clock. |

Prepare MUST canonicalize the complete Public `UseEvidence` object and return
those exact canonical bytes as the payload preview.

Prepare MUST reject a target absent from `payload.subjects`, any non-Public
disclosure, zero security identifiers, an expired request, or an expiry beyond
the 900-second ceiling.

The returned `PreparedPublicUseIntent` exposes the preview, exact target,
recipient, selector, namespace, disclosure, idempotency key, expiry, and typed
`PublicUseIntentCid`. The receipt remains a private typed capability: it has
no public constructor, byte getter, serializer, response field, `Clone`
implementation, or unredacted `Debug` representation.

The durable store MUST retain only a domain-separated receipt commitment,
never the receipt plaintext.

## 3. Exact intent binding

`intent_cid` is BLAKE3-256 over this unambiguous, domain-separated preimage:

```text
"onebrain:vnext:public-use-consent-intent:1" || 0x00
|| author FeedID
|| u64be(canonical_payload_preview.length)
|| canonical_payload_preview
|| u64be(exact_target.reference_kind)
|| exact_target CID
|| exact_recipient NodeID
|| SelectorCID
|| namespace commitment
|| u64be(Public disclosure)
|| idempotency_key
|| u64be(expires_at)
```

Confirmation MUST recompute this binding from durable state; caller-supplied
preview, target, recipient, selector, namespace, disclosure, idempotency, or
expiry substitutions are not accepted.

`last_known_addr` is deliberately excluded. It is an unauthoritative,
replaceable availability hint; outbound QUIC still authenticates the exact
prepared `NodeID`. P1.5 owns the authenticated `NodeID ↔ SocketAddr` directory.

## 4. Receipt lifecycle and idempotency

Receipt material is 32 non-zero bytes from the operating-system CSPRNG. Its
durable commitment binds both `intent_cid` and the receipt under
`onebrain:vnext:public-use-consent-receipt:1`.

An exact, unconsumed prepare retry MUST keep the same intent and rotate the
receipt commitment, invalidating every earlier receipt.

Reusing `(FeedID, idempotency_key)` for different bound content MUST fail with
an idempotency conflict. Re-preparing an already consumed operation MUST fail
as already confirmed.

## 5. Confirm transaction

Confirm consumes `PreparedPublicUseIntent` into a typed
`ConfirmPublicUseEvidenceRequest`; arbitrary external 32-byte values cannot
construct that request through the public API.

Before publication, confirm MUST validate the exact intent, author `FeedID`,
unexpired trusted time, receipt commitment, canonical Public object, exact
target membership, and Feed signer proof-of-possession.

One Redb write transaction MUST atomically commit the signed publication, the
next Feed head, and `consumed = true` on the prepared intent.

A receipt can cause at most one publication/sequence transition. An exact
retry returns the same publication; it does not allocate another sequence,
event, or consent transition. A route-only retry may replace the unauthoritative
address and requeue the same peer-bound publication.

Wrong, forged, rotated, intent-swapped, missing, corrupt, or expired receipt
state MUST fail closed before publication side effects.

Expiry is checked on every confirm, including exact retry after restart.
Expiration MUST NOT be bypassed by an existing publication lookup.

## 6. Durable ownership and compatibility

| State | Redb table | Rule |
|---|---|---|
| Prepared intent | `vnext_prepared_public_use_v1` | Exact fields, canonical preview, receipt commitment, consumed flag. |
| Operation index | `vnext_prepared_public_use_by_operation_v1` | `(FeedID, idempotency_key) → intent_cid`. |
| Publication | `vnext_public_use_publications_v1` | Schema 2 binds intent and receipt commitment to the existing signed object/event record. |
| Feed head | `vnext_public_use_feed_heads_v1` | Next sequence and causal parent, atomically advanced with confirmation. |

Prepared intent count and publication count are bounded at 65,536 each.
Unsupported or corrupt stored schemas fail validation rather than being
silently accepted or replaced.

## 7. Product integration rule

P3 CLI/Desktop/Web integration MUST place
`PreparedPublicUseIntent::confirm` behind an authenticated, explicit user
gesture that displays the exact preview, recipient, and Public/permanent
consequence. There is no default `--yes`, auto-confirm, background-confirm, or
conversion from a generic non-zero commitment.

The receipt and private capability MUST NOT enter logs, telemetry, WebSocket
events, public inventory, peer payloads, or general response DTOs.

## 8. Executable acceptance evidence

The feature-gated `vnext_distributed_pomv` suite proves:

- forged non-zero receipts and cross-intent receipt swaps fail;
- wrong target, non-Public disclosure, expired prepare, and excessive TTL fail;
- exact re-prepare rotates the receipt and invalidates the old capability;
- prepared/confirm debug output redacts the complete receipt;
- one confirmation creates one publication and one Feed sequence;
- exact confirmation retry and process reopen preserve the same publication;
- confirmation after expiry fails after process reopen;
- signer mismatch still fails before publication state; and
- real loopback QUIC publishes only the exact prepared recipient/selector/
  namespace record.
