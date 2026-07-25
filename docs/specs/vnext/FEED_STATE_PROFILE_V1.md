# OneBrain vNext — Feed State and Key-State Profile v1

> **Tasks:** `FEED-001`, `FEED-002`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`foundation::feed_store`](../../../src/ku-core/src/foundation/feed_store.rs), [`foundation::key_state`](../../../src/ku-core/src/foundation/key_state.rs)

## 1. Rebuildable single-writer feed projection

The durable source is the accepted signed event namespace from `OBS-001`.
`ValidatedFeedStore` is a deterministic projection over those already validated
events; it is safe to rebuild after restart or partition reunion.

Events are grouped by `(full FeedId, author_sequence)`. Exact `EventCid` replay
is idempotent. If two different, validly signed event CIDs occupy one feed
position, both are retained and a `FeedEquivocationProof` names the feed,
sequence and sorted CIDs. This proves a cryptographic same-feed/same-position
conflict; it does not infer human intent or invalidate either knowledge payload.

A contiguous branch begins at sequence `0`. At sequence `n`, an event advances a
branch only when its causal parents include a reachable event at `n - 1`.
Equivocation creates multiple reachable branch tips; a later event may select
one or more of them. Each accepted link emits a `FeedSuccessorProof`.

Missing positions are compact inclusive ranges. A present event without locally
available predecessor consistency is `unresolved_consistency`, not an accusation
and not global invalidity. Arrival of missing/reordered events recomputes the
projection and can close gaps. Sparse input at `u64::MAX` is handled by ordered
key iteration, never by expanding all intermediate positions.

## 2. Frontier-scoped key state

`KeyStateReducer` evaluates only evidence accepted at one named `EventCid`
frontier. Advancing the frontier is explicit; wall time and network majority do
not create freshness.

A `ScopedDelegation` binds an accepted grant to an optional parent delegation.
Root admission is a separate explicit operation representing a previously
verified principal/root proof. A submitted child cannot promote itself to root.
Every grant binds one exact initial `FeedId`; the other public inception fields
are scope checks, not substitutes for feed-key identity.

Child delegation attenuation is monotonic:

- actor identity must equal the parent actor;
- the granted subject FeedID is explicit and cannot be inferred from a copied
  delegation reference;
- generation range must be wholly inside the parent range;
- if the parent binds a namespace commitment, the child must bind the same one;
- a child may narrow an unbound parent to one namespace but cannot widen a bound
  namespace back to unrestricted;
- missing parent evidence remains pending `STALE_OR_UNRESOLVED` and is
  reevaluated when its parent arrives.

Accepted revocation must target an accepted delegation and be authorized by
that delegation or an accepted ancestor. Ancestor revocation cascades to child
delegations at the same local frontier. Missing authorizer/target evidence stays
unresolved; it cannot become fresh negative authority.

The reducer materializes the same three outcomes frozen by the feed authority
profile: `AUTHORIZED_RELATIVE`, `STALE_OR_UNRESOLVED`, and
`QUARANTINED_REVOKED_RELATIVE`. A structurally committed feed-key rotation can
remain authorized inside the inherited generation/namespace scope.

## 3. Reunion semantics

Disconnected partitions may have different gaps, branch tips, pending
delegations and accepted revocations. Their durable signed bytes are unioned by
CID. Each node then rebuilds feed and key-state projections at an explicitly
chosen accepted frontier.

Reunion therefore never requires a central feed server, global clock or
destructive last-writer-wins merge. New evidence can resolve an old pending
state; it does not retroactively erase the bytes or observations made by the
isolated partition.

## 4. Acceptance evidence

- Out-of-order successor is unresolved until predecessor arrival, then becomes
  contiguous with an explicit successor proof.
- Exact replay is idempotent and insertion order converges.
- Same position/different CID retains both branches and emits equivocation proof.
- Missing consistency emits unresolved state and no equivocation accusation.
- Sparse maximum sequence cannot trigger an unbounded loop/allocation.
- Child-before-parent delegation remains stale, then reconciles automatically.
- Generation/namespace widening is rejected by attenuation.
- A second feed key copying all public delegation scope fields remains
  `STALE_OR_UNRESOLVED`.
- Accepted ancestor revocation cascades; unauthorized/missing revocation proof
  never becomes fresh authority.
- A precommitted, in-scope key rotation remains authorized.
