# OneBrain vNext — Frontier Assembly Manifest Profile v1

> **Task:** `KU-003`  
> **Status:** Normative object profile — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::assembly`](../../../src/ku-core/src/foundation/assembly.rs)

## 1. Assembly versus Composite Gene

A Frontier Assembly Manifest is an immutable structural object that places
source objects and ReceptorDefinitions into a versioned working whole. Generic
object kind is `4`, major `1`.

It may later compile to or coexist with compatible Core DNA
`COMPOSITE_HDR/MEMBER` instructions, but it is not a new GeneType and does not
rewrite member KUs. Assembly identity and receptor resolution stay outside each
member's semantic identity.

## 2. Manifest identity

The manifest owns:

- full 256-bit assembly lineage ID;
- monotonically declared revision and immutable predecessor reference;
- canonical source-object set;
- canonical set of receptor placements;
- default immutable resolution-policy reference.

Revision `0` has no predecessor; later revisions require one. This validates
chain shape only—authorization and branch choice remain feed/event concerns.

## 3. Stable placement

Each `ReceptorPlacement` has its own full 256-bit `PlacementId`, Definition
reference, cardinality, required flag, alpha-normalized local context and
optional policy override.

Two placements may intentionally reference the same ReceptorDefinition. Their
resolution identities are nevertheless distinct:

```text
(assembly_lineage, manifest_revision, placement_id)
```

Changing a placement creates a new manifest identity but never mutates the
Definition or source KU. Placement IDs are unique inside one manifest; duplicate
placement IDs are rejected even when their other fields differ.

Current binding/status, candidate rank, network provider and runtime budget are
not manifest fields. They belong to signed resolution events and derived views.

## 4. Acceptance evidence

- The same Definition at two PlacementIds remains two independently addressable
  slots.
- Placement/source insertion order does not change manifest CID.
- Duplicate PlacementId is rejected.
- Revision `>0` without predecessor and revision `0` with predecessor are
  rejected.
- Changing only placement identity changes manifest CID without changing the
  referenced ReceptorDefinition.

