# OBKG Derived Projection Profile v1

> **Task:** `OBKG-001`  
> **Status:** Complete  
> **Depends on:** `KU-007`, `KU-008`, `KQL-005`, `KQL-011`, `POMV-001`, `POMV-002`

## 1. Purpose

This profile defines OBKG as a disposable local projection over canonical
OneBrain objects, already-reduced Receptor resolution state, durable Mapping
materialization, private KQL proposals and signed causal exercise evidence.
It provides Receptor, Affordance, Mapping and Use/Derivation views without
creating another knowledge database or authority plane.

The projection may feed exact graph traversal, learned indexes and legacy OBKG
engines, but those consumers remain derived and non-authoritative.

## 2. One reducer boundary

`ObkgProjection::rebuild` calls the KQL-005 `MinimalKnowledgeViews` contract and
consumes `ResolutionView` outputs carrying the exact
`RESOLUTION_REDUCER_VERSION`. It never consumes raw Resolution actions and never
replays adoption/revocation semantics.

A foreign reducer version fails closed. A ResolutionView naming an active
Mapping without a corresponding durable materialized Mapping record is also
rejected. This prevents incomplete source selection from inventing a graph
edge.

## 3. Explicit source frontier and versions

Every snapshot binds:

- selector-scoped source frontier;
- authority frontier commitment;
- full-width feed positions;
- source root and derived projection root;
- OBKG projection reducer version;
- canonical Resolution reducer version;
- index version; and
- either `DeterministicNoModel` or an exact learned-model object reference.

Changing model identity changes the roots. A learned embedding or score without
its model version therefore cannot silently masquerade as the deterministic
view.

The source frontier is local and scoped. It is not proof of global completeness,
network component membership or absence outside the observed feeds.

## 4. Receptor and Affordance views

The object lane accepts validated, known ReceptorDefinition and
KnowledgeAffordance envelopes. Each record retains full ObjectCID identity,
schema version, disclosure class, declared references and payload commitment.

Receptor records join only to StandingNeed IDs and scoped Resolution states
already emitted by KQL-005. Affordance records remain immutable object
projections. No graph index may rewrite either source object.

Schema-specific semantic validation belongs to the object admission pipeline;
this projection does not reinterpret opaque or unsupported kinds.

## 5. Mapping lifecycle lanes

MappingView deliberately keeps three distinct facts:

1. candidate proposal IDs and dispositions;
2. durable materialized Mapping Kernel/Envelope records; and
3. Assembly targets that adopted the Mapping in a canonical ResolutionView.

An active edge exists only when both durable materialization and at least one
active adopted target are present. Consequently:

- proposal alone is candidate-only;
- materialization alone is durable but inactive; and
- adoption cannot be inferred from rank, retrieval, model score or proposal
  presence.

Several branches adopting one Kernel retain target identity but do not turn
branch/source count into semantic weight.

## 6. Use and Derivation view

The only accepted exercise input type is `AssessedExerciseEvidence`, whose
payload is a cryptographically bound `ValidatedUseEvidenceEvent` or
`ValidatedDerivationEvidenceEvent`. Records retain EventCID, payload ObjectCID,
authority assessment, causal inputs/outputs, optional Mapping and observed
frontier.

Retrieval, QueryHit, presentation, dwell, engagement and other exposure
telemetry have no input path. The projection explicitly reports
`accepts_exposure_telemetry = false`.

Unauthorized and unresolved signed evidence may be retained with its assessment
for audit, while authorized-query helpers return only authorized events. Use and
Derivation still establish neither truth, benefit nor reward.

## 7. Delete and rebuild semantics

All maps and roots are deterministic over canonical full-width identity. Object,
proposal, mapping, Resolution and Event source order cannot change the result.
A node may delete the entire derived store and rebuild the same projection root
from the same source frontier and versions.

Deleting the projection loses no canonical object, event, StandingNeed,
materialized Mapping or Resolution state. OBKG therefore exposes
`is_source_of_record = false` and `is_resolution_reducer = false`.

Different disconnected islands may derive different valid partial roots. Reunion
extends the source frontier and causes a new rebuild; it does not require one
network-global graph.

## 8. Boundaries

The profile does not:

- make legacy OBKG bonds a canonical KU relation or source of truth;
- activate a candidate or merely materialized Mapping;
- infer Use from retrieval/exposure;
- infer benefit, correctness, popularity, PoMV or OBT from graph degree;
- let model/index output change eligibility or adoption;
- replace canonical object/event storage;
- reduce raw Resolution actions a second time; or
- introduce a Core DNA Gene or execution opcode.

## 9. Executable evidence

Five tests prove:

- deleting and rebuilding with reversed source order yields identical frontier,
  source and projection roots plus frozen versions;
- proposal-only and materialized-only Mapping records remain inactive until an
  adopted Resolution binding exists;
- an adoption without its materialized Mapping fails closed;
- signed validated Use and Derivation events populate the exercise view while
  exposure telemetry is not accepted; and
- a foreign Resolution reducer version is rejected.
