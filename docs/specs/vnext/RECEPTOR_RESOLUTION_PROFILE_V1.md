# OneBrain vNext — Receptor Resolution Profile v1

> **Tasks:** `KU-004`, `KU-007`  
> **Status:** Normative — frozen 2026-07-20  
> **Code:** [`foundation::resolution`](../../../src/ku-core/src/foundation/resolution.rs)

## 1. Scope and ownership

Resolution belongs to one receptor placement in one immutable Assembly
revision. It is never stored on the reusable ReceptorDefinition and never means
that a problem is globally closed.

The exact target is:

```text
(assembly_lineage_id, assembly_revision_object_cid, placement_id)
```

`ResolutionView` always carries this target, its ResolutionPolicy reference,
the assessed frontier and reducer version. UI/API text must therefore qualify
`SATISFIED_RELATIVE` by Assembly, revision, policy and frontier.

## 2. Signed action binding

Event type `1` is `receptor-resolution`. Its signed generic Knowledge Event
contains exactly one payload reference to generic object kind `12`,
`receptor-resolution-action`. Event signature, author feed, sequence,
authorization reference and causal parents remain owned by the event envelope;
the payload object owns only the immutable action data:

- exact target;
- one of `ADOPT_BINDING`, `REVOKE_ADOPTION`, `WAIVE`, `REOPEN`, `DEFER`;
- MappingKernelCID for adoption or EventCID for revocation;
- optional receptor claim and acceptance-evidence references;
- ResolutionPolicy reference and observed-frontier digest.

Event and action disclosure classes must match. A different action-object CID,
extra payload reference, wrong event type or wrong schema major is rejected
before projection. The action object carries no authority by itself.

## 3. Authority and materialization gates

Signature validity is not authorization. The authority evaluator supplies one
frontier-relative result: `AUTHORIZED`, `UNAUTHORIZED` or `UNRESOLVED`.
Unauthorized and unresolved events are retained as evidence/pending input but
cannot change the authoritative view.

An authorized `ADOPT_BINDING` additionally requires:

1. the exact MappingKernelCID to exist at a durable materialization boundary;
2. an acceptance assessment under the named placement policy, evidence and
   assessed frontier;
3. an outcome of `REJECTED`, `PARTIAL` or `SATISFIED` without coercing unknown
   constraints to false or true.

`REJECTED` leaves the branch open, `PARTIAL` yields
`PARTIALLY_SATISFIED`, and only `SATISFIED` yields
`SATISFIED_RELATIVE`. Materializing a Mapping never emits an adoption event and
never changes Assembly or Resolution state.

## 4. Causal multi-branch reducer

The reducer derives each causal tip from its authorized ancestors. Concurrent
tips are not ordered by advisory time, receipt time, node identity or rank. If
their effective states/bindings differ, the view is `CONCURRENT` and exposes
all branches. A later causally-descendant action may explicitly reopen, waive,
defer, revoke a named adoption or adopt a new binding.

State vocabulary is closed to:

```text
OPEN | PARTIALLY_SATISFIED | SATISFIED_RELATIVE |
WAIVED | DEFERRED | CONCURRENT
```

There is no `CLOSED`, global terminal state or LWW winner. Exact EventCID replay
is idempotent. Reassessment at a newer frontier is a derived-view update, not a
mutation of historical event bytes.

## 5. Acceptance evidence

- A signed event binds exactly one canonical action payload.
- Unauthorized/unresolved waive/defer actions leave the view `OPEN`.
- Partial adoption cannot become `SATISFIED_RELATIVE`.
- Concurrent adopt and reopen preserve two branches and yield `CONCURRENT`.
- A causal revocation removes only its named adoption.
- Authorized adoption without a durable Mapping is rejected at the gate.

