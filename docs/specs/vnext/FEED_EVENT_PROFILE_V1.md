# OneBrain vNext — Feed, Authority and Event Profile v1

> **Tasks:** `IDN-002`, `EVT-001`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::{feed, authority, event}`](../../../src/ku-core/src/foundation)  
> **Vectors:** [`feed-event-v1.json`](../../../src/test-vectors/vnext/foundation/feed-event-v1.json)

## 1. Feed identity and inception

`FeedId` is the domain-separated digest of canonical identity material:

```text
FeedId = H("feed-inception/1", {
  purpose: 1,
  feed_public_key,
  randomized_namespace_commitment,
  generation
})
```

It excludes `ActorId`, `DeviceId`, `NodeId`, route, endpoint, predecessor and
provenance. Those values can authorize or describe a feed but cannot silently
change its identity. In particular, there is no actor-wide sequence.

`FeedInception` schema ID is `2`, major `1`. Its signed body contains:

| Key | Field | Rule |
|---:|---|---|
| `0` | `feed_public_key` | Ed25519 public key; owns this single-writer feed. |
| `1` | `namespace_commitment` | Randomized binding-hiding commitment; raw namespace is never published. |
| `2` | `generation` | Feed-local rotation generation, not a global clock. |
| `3` | `owner_device` | Full-width device identity; excluded from `FeedId` material. |
| `4` | `actor_delegation_ref` | Optional authority-event reference. Missing proof is unresolved. |
| `5` | `predecessor_feed` | Optional explicit rotation predecessor. |
| `6` | `pre_rotation_commitment` | Predecessor commitment to one exact successor `FeedId`. |
| `7` | `signature` | Signature by `feed_public_key` over the exact unsigned canonical root. |

The namespace commitment includes a caller-owned random 32-byte opening. Two
feeds for the same private namespace therefore do not self-link by default.
Explicit predecessor or authority evidence can reveal a relationship only when
the publishing policy chooses to disclose it.

## 2. Delegation, rotation and revocation

Authority is evaluated relative to an accepted local event frontier. There is
no network-wide authority oracle and no wall-clock freshness rule.

The authority projection consumes accepted `DelegationGrant` and
`AcceptedRevocation` evidence. Each item binds actor, device, delegation-event
reference and generation range, and retains the `EventCid` of its proof. A
`DelegationGrant` additionally binds the exact initially authorized `FeedId`;
copying a public delegation reference, DeviceId, namespace commitment, and
generation into an unrelated feed key never grants authority. These
projection records are not independent authority. The root-only
`ActorRootDelegation/1` derives one exact initial grant from a self-certifying
root-key proof. `ActorDelegation/1` and `ActorRevocation/1` are canonical signed
wire inputs whose authorizing feed, parent/target references and attenuation
rules are checked before conversion into these projections.

An authority decision has exactly these outcomes:

| Outcome | Meaning |
|---|---|
| `AUTHORIZED_RELATIVE` | A matching accepted grant covers this device, delegation and generation at this frontier. |
| `STALE_OR_UNRESOLVED` | The delegation reference or accepted proof is unavailable locally. Retry after reconciliation; do not declare global invalidity. |
| `QUARANTINED_REVOKED_RELATIVE` | A matching revocation already accepted by this frontier covers the generation. Preserve bytes/evidence but do not execute it. |

A structural key successor additionally requires the exact predecessor,
generation `previous + 1`, same owner device and a matching pre-rotation
commitment. A device change is a separately delegated feed, not an implicit key
rotation. Structural failure rejects that claimed successor relationship; it
does not make the carried knowledge proposition false.

Revocation wins over an older matching grant only inside a view whose accepted
frontier contains that revocation. A disconnected partition without the proof
must remain unresolved/locally stale and converge by reevaluation after reunion.

## 3. Signed Knowledge Event envelope

`KnowledgeEventEnvelope` uses schema ID `3`, major `1`, and an `event/1`
domain-separated `EventCid`. The signed body contains:

| Key | Field | Rule |
|---:|---|---|
| `0` | `event_type` | Stable unsigned type. Unknown type is opaque, not semantic truth or falsehood. |
| `1` | `payload_refs` | Canonical sorted unique set of immutable object references. |
| `2` | `author_feed` | Full-width `FeedId`; must match the verified inception/key. |
| `3` | `author_sequence` | Feed-local sequence only. |
| `4` | `device_delegation_ref` | Optional authority evidence reference. |
| `5` | `causal_parents` | Canonical sorted unique set of `EventCid` parents. |
| `6` | `authorization_ref` | Optional typed `PermitCid`; reference alone grants no authority. |
| `7` | `disclosure` | Public, negotiated encrypted, route-minimal or local-only. |
| `8` | `advisory_time` | Metadata only; never causal or authority ordering. |
| `9` | `idempotency_key` | Non-zero operation key; exact event replay also deduplicates by `EventCid`. |
| `10` | `signature` | Feed-key signature over the exact unsigned canonical root. |

Missing causal parents produce `MissingParents`, not invalidity. Unknown event
types that pass bounded canonical/signature validation are preserved as opaque
original bytes. An unsupported root schema major is rejected before execution.
Duplicate set members are rejected; changing insertion order does not change
canonical bytes, signature or `EventCid`.

### Policy-reference boundary

The authority dependency of a feed is the exact
`FeedInception.actor_delegation_ref`, resolved through the self-contained
signed AuthorityEvent schemas defined by
[Actor Authority Event Profile v1](ACTOR_ROOT_AUTHORITY_PROFILE_V1.md).
Knowledge Event field `authorization_ref` belongs to the capability/execution
plane instead. A `PermitCid` reference alone does not grant feed authority,
does not make an event executable, and does not become valid merely because
the event bytes are accepted into immutable storage.

Authority v1 has no mutable external policy document or URI. Any future
external authority-policy mechanism requires a new schema major and must fail
closed on v1 implementations.

## 4. Acceptance evidence

- Different feed keys/devices do not collide; randomized namespace commitments
  for the same namespace do not link.
- Feed inception and events preserve their exact validated bytes.
- Tamper and wrong-author cases fail signature/feed binding.
- Missing authority proof remains `STALE_OR_UNRESOLVED`.
- A copied delegation reference cannot authorize a different FeedID/key.
- An arbitrary Knowledge Event `authorization_ref` cannot change feed
  authority.
- Accepted grant authorizes relative to its frontier; accepted covered
  revocation quarantines relative to that same frontier.
- Rotation commitment binds one exact successor and malformed successor claims
  receive stable structural outcomes.
- Duplicate, reorder, missing-parent, exact-replay, unknown event type and
  unsupported schema-major paths all have deterministic outcomes.
- Frozen feed/event vectors pass `ku-core`, `onebrain-protocol` and `ku-net`.
