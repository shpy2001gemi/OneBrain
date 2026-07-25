# Actor Authority Event Profile v1

> **Task:** `FEED-002 / M2 authority control plane`  
> **Status:** Normative signed-chain subset — frozen 2026-07-25  
> **Code:** `ku-core::foundation::{actor_root, authority_event}`, `onebrain-node::vnext_validated_sink`

## 1. Scope

This profile defines canonical Actor root, child-delegation and revocation
records that cross OBP-RP. A self-certifying root bootstraps exactly one FeedId;
that authorized feed may sign attenuated child grants or authorized
revocations. The profile does not define a global identity directory, global
authority, wall-clock freshness, or production private-key custody.

`AuthorityEvent` is a separate content domain, inventory lane and reconciliation
manifest kind. Authority bytes never enter the Knowledge Event namespace. The
v1 sink accepts only the three schemas frozen below; unknown authority schemas
and ad-hoc serialized projection structs fail closed.

## 2. Self-certifying ActorId

An ActorId is derived from canonical root-key material:

```text
ActorId = H("authority-event/1", {
  purpose: 0,
  root_public_key: Ed25519PublicKey
})
```

The decoder recomputes this identity. A claimed ActorId that does not match the
included root public key is rejected before persistence. Separate personas or
disclosure scopes should use separate root keys.

## 3. ActorRootDelegation/1

The canonical envelope uses schema ID `9`, major `1`, and these body fields:

| Key | Field | Rule |
|---:|---|---|
| `0` | `actor` | Must equal the self-certifying ActorId of field `1`. |
| `1` | `root_public_key` | Ed25519 root verification key. |
| `2` | `subject_feed` | Exact FeedId being authorized; never a wildcard. |
| `3` | `device` | Exact device scope. |
| `4` | `namespace_commitment` | Optional exact namespace attenuation. |
| `5` | `first_generation` | Inclusive lower generation bound. |
| `6` | `last_generation` | Inclusive upper bound; must not be below field `5`. |
| `7` | `signature` | Root-key signature over the complete unsigned canonical envelope. |

The record CID is computed in the `authority-event/1` domain. Its CID becomes
both the delegation reference carried by FeedInception and the proof frontier.
There is no identity cycle: FeedId material deliberately excludes
`actor_delegation_ref`, so the exact FeedId is known before the proof is signed.

## 4. ActorDelegation/1

Schema ID `10`, major `1`, defines a child grant:

| Key | Field | Rule |
|---:|---|---|
| `0` | `actor` | Must equal the parent grant Actor. |
| `1` | `parent_delegation_ref` | Exact accepted parent authority EventCID. |
| `2` | `authorizing_feed` | Must equal the exact FeedId authorized by the parent grant. |
| `3` | `subject_feed` | Exact child FeedId; never a wildcard. |
| `4` | `device` | Exact child device scope. |
| `5` | `namespace_commitment` | Optional namespace attenuation. |
| `6` | `first_generation` | Inclusive lower generation bound. |
| `7` | `last_generation` | Inclusive upper bound. |
| `8` | `signature` | Signature by the validated authorizing feed key. |

Before acceptance, the receiver rebuilds the parent closure, proves the
authorizing FeedInception is `AUTHORIZED_RELATIVE` at that parent, verifies the
signature, and applies the reducer's actor/namespace/generation attenuation
rules. Missing parent or feed material is deferred; expansion or wrong
authority is quarantined.

## 5. ActorRevocation/1

Schema ID `11`, major `1`, defines a revocation:

| Key | Field | Rule |
|---:|---|---|
| `0` | `actor` | Must equal the target and authorizer Actor. |
| `1` | `target_delegation_ref` | Exact grant being revoked. |
| `2` | `target_device` | Must equal the target grant device. |
| `3` | `revoked_from_generation` | Inclusive generation floor. |
| `4` | `authorized_by` | Exact authorizer grant; it must be target's ancestor or self. |
| `5` | `authorizing_feed` | Exact FeedId authorized by field `4`. |
| `6` | `signature` | Signature by that validated authorizing feed key. |

The revocation frontier includes the target chain, authorizer chain and signed
revocation. It affects only projections evaluated at that frontier or a future
frontier that explicitly contains it; an older frontier remains an honest
historical projection.

## 6. Acceptance and projection

The receiver verifies canonical form, schema/version, ActorId derivation,
Ed25519 root signature and claimed CID before an atomic durable write. Invalid
bytes are quarantined outside the executable namespace. Replays are idempotent,
and the accepted record plus selector inventory survive process restart.

`feed_authority_at(feed_id, authority_frontier)` recursively rebuilds only the
canonical ancestor/target closure ending at one exact durable root, child or
revocation event. Unrelated locally known roots and branches are not implicitly
included. The result is always frontier-relative and never claims network-wide
freshness or absence of a later revocation.

Copying the proof CID, DeviceId, namespace and generation into a FeedInception
with another key produces another FeedId and remains `STALE_OR_UNRESOLVED`.

## 7. Policy-reference and fail-closed boundary

Authority schemas `9`, `10`, and `11` are deliberately self-contained. Every
input needed to derive feed authority is inside the signed record or an exact
parent/target/FeedInception CID dependency: Actor, authorizing and subject
feeds, device, optional namespace, generation range, and revocation target.
There is no external policy URI, mutable policy document, ambient role, or
arrival-order rule in authority v1.

`KnowledgeEvent.authorization_ref` is a typed capability `PermitCid`. It is not
an AuthorityEvent dependency, and merely storing an event carrying that
reference never grants feed authority or capability execution. A missing
permit may coexist with immutable event custody, but any executor that requires
the permit must remain blocked until the CAP-004 permit closure validates at
its named frontier.

Adding an external authority-policy dependency would change the signed
authority meaning and therefore requires a new authority schema major,
canonical fields, dependency resolution rules, vectors, and downgrade
behavior. Unknown schemas fail closed; implementations must not reinterpret an
unknown field as v1 authority.

- Production Actor root-key generation, hardware-backed custody, recovery and
  rotation policy remain outside this wire profile. The network runtime only
  verifies public Actor proofs and never stores an Actor root private key.
- Transport NodeID custody follows
  [Node Identity Key Custody Profile v1](NODE_IDENTITY_KEY_CUSTODY_PROFILE_V1.md)
  and is separate from Actor/feed authority.
- These records do not by themselves grant content truth, adoption, capability
  execution, PoMV benefit, reward, or OBT mint authority.

## 8. Executable evidence

- Core tests cover self-certification, signature tamper, wrong ActorId, exact
  FeedId binding, wrong-domain CID, idempotency and redb restart.
- Sink tests prove an unrelated feed cannot replay public scope fields and a
  child arriving before its parent/feed remains deferred. A signature-valid
  Knowledge Event carrying an arbitrary `PermitCid` also leaves that unrelated
  feed `STALE_OR_UNRESOLVED`.
- Real two-runtime QUIC tests reconcile root, parent feed, delayed child,
  child feed and revocation, verify authenticated receipts, preserve historical
  versus revoked frontier decisions, and reproduce them after receiver restart.
- A three-runtime partition/reunion test proves that receivers may retain
  different frontier-relative authority views while disconnected and converge
  after exchanging the same immutable revocation proof, without a seed, leader,
  quorum, or arrival-order authority.
