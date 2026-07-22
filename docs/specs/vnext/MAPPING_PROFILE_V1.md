# OneBrain vNext — Mapping Kernel and Envelope Profile v1

> **Task:** `KU-005`  
> **Status:** Normative semantic profile — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::mapping`](../../../src/ku-core/src/foundation/mapping.rs)

## 1. Why mapping is a first-class KU primitive

The anti-gravity reunion case is unlikely to share the same words, Gene Type or
local ConceptIds. A material observation about wheel moment, an old alloy study
and an unfinished propulsion design may become useful only after OneBrain can
state *how particular semantic roles correspond*, what transform is needed, and
what remains unknown.

Mapping is therefore explicit knowledge. Similarity can propose it but cannot
silently install it as an OBKG edge or claim equivalence.

## 2. MappingKernel

`MappingKernel` contains only semantic mapping content:

- canonical source and target object sets;
- exact statement/argument locators;
- correspondence kinds: equivalent, broader, narrower, analogous, causal role
  or structural role;
- identity, exact affine-unit or explicit-rule transforms;
- alpha-normalized assumptions;
- typed constraint regions with `SATISFIED`, `VIOLATED` or `UNKNOWN` state;
- explicit source/target unmapped regions and reason CCIDs.

Canonical bytes hash under reserved `mapping-kernel/1` into typed
`MappingKernelCid`. Generator, model, evidence, score, route, runtime and reward
are excluded. Correspondence/set insertion order cannot change KernelID;
duplicates are rejected.

Unknown, violated and unmapped regions are identity-bearing rather than omitted.
This prevents a partial analogy from being mistaken for complete equivalence.

## 3. MappingEnvelope

`MappingEnvelope` is immutable generic object kind `6`, major `1`. It references
one MappingKernelID and carries:

- generator/implementation artifact reference;
- optional derivation-rule reference;
- canonical evidence references;
- optional signed source EventCID.

Two people/models may independently produce envelopes around the same semantic
KernelID. Their provenance/evidence CIDs remain distinct without fragmenting the
shared mapping identity.

## 4. Unit and direction safety

Affine-unit transform binds source and target DimensionVectors plus exact scale
and offset. Changing Celsius/Kelvin offset, dimension, direction or any mapping
locator changes KernelID. A model score alone cannot create or erase the
transform.

## 5. Lifecycle firewall

A candidate MappingKernel/Envelope remains an ephemeral KQL proposal until the
durable materialization boundary (`KU-006`). Materialization creates immutable
objects but does not adopt them into an Assembly; adoption requires the separate
authorized resolution event path (`KU-007`).

## 6. Acceptance evidence

- Correspondence insertion order gives the same MappingKernelID.
- Generator/evidence changes MappingEnvelope CID, not KernelID.
- Unknown→violated and mapped→unmapped changes KernelID.
- Affine dimension/scale/offset transform is exact and identity-bearing.
- Duplicate correspondence is rejected.

