# OneBrain vNext — Interoperability Profile v1

> **Task:** `DOC-001`
> **Status:** Frozen foundation release profile
> **Freeze date:** 2026-07-22
> **Scope:** additive vNext knowledge-plane interoperability

## 1. Profile objective

This profile defines the smallest interoperable stack for independently built
OneBrain nodes. It freezes byte identity, validation boundaries and downgrade
behavior; it does not freeze scientific claims, select a central authority or
declare any network-wide completion state.

Interoperability is evaluated between concrete peers, selectors, namespaces,
frontiers and carriers. A partitioned node remains a valid standalone node and
can later reconcile through one or many bridges without a global epoch.

## 2. Required profile stack

| Layer | Frozen contract | Interoperability role |
|---|---|---|
| canonical bytes | [Canonical Profile v1](CANONICAL_PROFILE_V1.md) | deterministic CBOR, domain-separated BLAKE3 identity and bounded decoding |
| object identity | [Identity/Object Profile v1](IDENTITY_OBJECT_PROFILE_V1.md) | immutable full-width IDs, schema ownership and opaque-safe envelopes |
| causal authority | [Feed/Event Profile v1](FEED_EVENT_PROFILE_V1.md) | signed events, private feed inception and frontier-relative authority |
| authenticated link | [Authenticated Session Profile v1](AUTHENTICATED_SESSION_PROFILE_V1.md) and [Node Identity Key Custody Profile v1](NODE_IDENTITY_KEY_CUSTODY_PROFILE_V1.md) | transcript/channel binding, downgrade defense and non-exportable signer boundary |
| scoped inventory | [Inventory Scope](INVENTORY_SCOPE_PROFILE_V1.md) and [Hybrid Inventory Forest](HYBRID_INVENTORY_FOREST_PROFILE_V1.md) | bounded selector-relative set comparison with full CIDs |
| reconciliation | [OBP Reconciliation Protocol](OBP_RECONCILIATION_PROTOCOL_PROFILE_V1.md) and [State Machine](DETERMINISTIC_RECONCILIATION_STATE_MACHINE_V1.md) | manifest-before-payload, validation-before-accept and resumable convergence |
| carrier adapters | [Deterministic Carrier](DETERMINISTIC_CARRIER_PROFILE_V1.md) and [Cross-Carrier Reconciliation](CROSS_CARRIER_RECONCILIATION_PROFILE_V1.md) | identical canonical records across memory, file, delayed and QUIC framing |
| downgrade boundary | [Negotiated Legacy Adapter](NEGOTIATED_LEGACY_ADAPTER_PROFILE_V1.md) | parse-only legacy custody and conservative normalization |

## 3. Conformance levels

1. **Opaque custody and forwarding:** preserve exact envelope bytes and CID,
   enforce framing/resource limits, and never infer semantic validity.
2. **Validated local storage:** recognize the schema, validate canonical form,
   identity and required signatures, then persist into the correct public,
   private or quarantine class.
3. **OBP exchange:** authenticate the session, bind selector/namespace/budget,
   reconcile deterministic inventories and resume through a persisted journal.
4. **Optional extensions:** private multipath, remote cognition, reward export
   and future set-reconciliation accelerators are negotiated independently and
   cannot change the validated final set or grant authority.

A node may implement a lower level and forward opaque future objects. It may
not label opaque custody as validated understanding.

## 4. Wire and context binding

Canonical payloads are carrier-independent. Session identity, transcript,
selector CID, namespace, disclosure class, inventory roots, checkpoint bounds
and resource budgets are explicit. Truncated display IDs are never accepted as
wire identity. `BoundTokenV1` continuations bind the exact original context.
`PeerBoundTokenV2` may name an origin journal from a fresh authenticated
transcript, but remains MAC-bound to the same ordered peers and exact
selector/namespace/disclosure/method/budget scope. Its checkpoint is consumed
atomically once; changing peer, scope, frontier, key, or sequence fails closed.

Every decoder applies size, depth, count, compression-ratio and expansion
limits before allocation or durable storage. A malformed branch is isolated;
it cannot poison unrelated branches or upgrade an incomplete exchange to a
completed one.

## 5. Partition, carrier and reunion behavior

Memory, reopenable files, delayed store-carry-forward and bounded QUIC streams
transport the same canonical record. Delay or carrier absence is reported as
unknown/partial, never false or complete. Multi-bridge delivery deduplicates by
canonical identity while retaining path observations and conflicting payload
variants for evidence.

Reunion joins observed deltas. It does not elect a component leader, count a
global population or wait for a seed. Seed nodes are optional discovery hints;
they are not part of correctness, authority or availability semantics.

## 6. Legacy and optional extension behavior

Inbound legacy `GLOBAL` is downgraded to sampled partial reachability. Inbound
legacy `FULL` becomes a non-corroborating advisory encoding claim. Exact raw
bytes remain local-only evidence, and unsafe outbound negotiation is rejected.

RIBLT acceleration and remote cognition remain optional and default-off in this
freeze. Reward export is post-commit and cannot enter KU, KQL, OBP, authority or
materialization transactions.

## 7. Conformance evidence

Implementations can reproduce the frozen vectors and gates with:

```text
python scripts/ci/validate_vnext_contracts.py
cargo test -p ku-core foundation --lib
cargo test -p onebrain-protocol foundation --lib
cargo test -p ku-net foundation --lib
cargo test -p onebrain-node vnext_mixed_conformance --lib
cargo test -p onebrain-node vnext_security_suite --lib
```

Passing proves conformance to the bounded fixtures and models, not universal
network correctness or global completeness.
