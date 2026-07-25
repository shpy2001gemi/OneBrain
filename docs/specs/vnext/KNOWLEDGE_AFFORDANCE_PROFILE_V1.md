# OneBrain vNext — Knowledge Affordance Profile v1

> **Task:** `KU-008`  
> **Status:** Normative object profile — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::affordance`](../../../src/ku-core/src/foundation/affordance.rs)

## 1. Purpose

A Knowledge Affordance states what a KU/object can explicitly offer to a task,
assembly or receptor under stated conditions. It is the semantic bridge between
“this object exists” and “this object may be usable in this role.” It is not a
runtime capability advertisement, route/provider lease, embedding similarity,
ranking score or proof of benefit.

The profile is immutable generic object kind `5`, major `1`.

## 2. Owned fields

`KnowledgeAffordance` owns:

- canonical source object references;
- explicitly offered role CCIDs;
- accepted inputs, each referencing a ReceptorDefinition, exact role CCID and
  required/optional status;
- alpha-normalized SemanticFrameSets for preconditions, outputs, effects,
  properties, invariants, operating conditions and limits;
- canonical abstraction patterns expressed in the same network-safe IR;
- explicit or derived provenance.

Set-like sources, roles, inputs, patterns and derivation inputs are canonical
sorted and duplicate-rejected. Semantic frame order remains governed by the
Semantic Primitives profile.

## 3. Provenance and version

An explicit affordance names exact source statement locators. A derived
affordance names immutable derivation-engine, derivation-rule and input object
references. The enclosing object kind major/minor versions the profile.

Derived and explicit affordances are both legitimate objects, but derivation
provenance is part of identity: changing engine/rule/input changes CID. Creating
an affordance never changes the source KU/object CID and never writes fields
back into Core DNA.

## 4. No capability inference by embedding

`supports_role(role)` performs exact membership over declared role CCIDs. There
is deliberately no embedding/model-score field and no fallback that turns
similarity into support. KQL may use embeddings to generate a proposal, but the
proposal remains unsupported until validated against explicit semantics or is
materialized as a new provenance-preserving affordance.

Likewise, runtime availability/tool implementation belongs to Capability Offer
and ImplementationManifest layers, not this semantic object.

## 5. Acceptance evidence

- Source/role insertion order does not change object CID.
- Duplicate sources are rejected rather than inflating provenance.
- Explicit versus derived origin (including engine/rule/input) changes identity.
- An undeclared role remains unsupported even if a model would rank it similar.
- Preconditions, outputs, effects, properties, invariants, operating conditions,
  limits and abstraction patterns all use CCID-only alpha-normalized frames.
- A validated known object is accepted by the typed runtime decoder only when
  every field decodes and the reconstructed affordance re-encodes to the exact
  canonical payload; missing fields, unknown fields and alternate set/frame
  representations are rejected.
