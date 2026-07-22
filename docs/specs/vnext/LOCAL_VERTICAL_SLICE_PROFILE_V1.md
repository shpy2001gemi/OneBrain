# OneBrain vNext — Local Vertical Slice Profile v1

> **Task:** `RUN-001`  
> **Status:** Executable local MVP — frozen 2026-07-20  
> **Code:** [`onebrain-node::vnext_local_runtime`](../../../src/onebrain-node/src/vnext_local_runtime.rs)

## 1. Composed flow

The local runtime proves that OneBrain remains useful with no network and no
central service:

```text
FrontierAssemblyManifest + placement
  → private KnowledgeNeedIR / QueryDefinition
  → exact typed Affordance match
  → private BindingProposal quarantine
  → explicit authorized materialization
  → separately signed and policy-assessed ADOPT_BINDING event
  → frontier-relative ResolutionView
  → rebuildable MinimalKnowledgeViews
```

The runtime reuses the canonical KU/KQL contracts. It defines orchestration,
not a new object, event, Gene, opcode, authority or global state.

## 2. Boundaries that must not collapse

1. `propose` can only insert into `ProposalQuarantine`, whose executable flag is
   always false. A hard mismatch produces no proposal.
2. `materialize` is a separate call with intent, requester, idempotency key,
   destination, disclosure evidence and an explicit authority result. It does
   not change Receptor resolution.
3. `apply_resolution` accepts only a validated signed resolution event and a
   separate frontier-relative policy decision. Authorized adoption additionally
   requires the referenced MappingKernel to be durably visible to the supplied
   materializer.
4. State is reported as `SatisfiedRelative`, never globally closed or true.

No default or helper silently upgrades an event to authorized.

## 3. Assembly and NeedIR

Construction binds one exact `(assembly lineage, assembly ObjectCID,
PlacementId)` target and resolves the placement-specific policy override before
falling back to the manifest policy. Missing placements are rejected.

The generated full `KnowledgeNeedIR` is `LOCAL_ONLY`, contains the exact
Receptor definition reference and desired role, and carries the placement's
local context. Only later KQL disclosure compilation may derive bounded network
work.

## 4. Restart and offline evidence

The executable integration test persists the StandingNeed in Redb, drops the
first runtime, reopens the same need, then completes matching, quarantine,
materialization and signed adoption with an in-memory carrier and no network.
It finally rebuilds MinimalKnowledgeViews from durable/source records and
confirms the Mapping is adopted by the exact target.

The test also applies the signed event first as `Unauthorized` and observes
`Open`; reassessment as `Authorized` plus `Satisfied` is required before the
view becomes `SatisfiedRelative`. Materialization is observed while the
Receptor is still `Open`, proving that materialize does not auto-adopt.

Production private Mapping persistence must use an encrypted Vault-backed
`AtomicMappingBackend`; the integration test uses the conformance in-memory
backend only within one process after the restart boundary.

