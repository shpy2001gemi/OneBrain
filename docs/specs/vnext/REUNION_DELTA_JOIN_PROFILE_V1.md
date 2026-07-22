# Reunion Delta Join Profile v1

> **Task:** `KQL-006`  
> **Status:** Complete  
> **Scope:** local, frontier-relative KQL matching after disconnected components exchange new public knowledge objects

## 1. Purpose

Reunion matching answers a narrow question: can a newly received public knowledge object complete a locally known need or affordance? It does not search for network truth, elect a winning KU, or turn a match into an adopted mapping.

The primary join is:

```text
delta Affordance_remote JOIN indexed Receptor_local -> private Mapping proposal
```

The inverse join is permitted only when the remote receptor is itself an exact, validated, public knowledge object:

```text
delta Receptor_remote JOIN indexed Affordance_local -> private Mapping proposal
```

## 2. Admission boundary

A remote affordance or receptor enters the join only after the Public Store validation boundary. Its decoded value MUST re-encode to the exact original canonical bytes and full CID. Opaque, private, local-only, wrong-kind, or reconstructed look-alike values are rejected.

`StandingNeed`, `NeedIR`, local context, claim envelopes, and proposal quarantine remain local. The join exports none of them and never places them in OBP inventory.

## 3. Delta and frontier semantics

The join consumes only newly received object CIDs relative to a local frontier. The planner and semantic index supply a bounded local candidate set; the join does not traverse the full OBKG. Replayed remote CIDs are counted and skipped.

Frontiers are local implementation state, not a claim that a selector, component, or network is complete. A disconnected component may keep matching locally, then submit only its newly observed public objects after reunion.

## 4. Matching and output

Every candidate pair uses the exact typed matcher from `KQL-004`, including three-state constraints and exact unit/dimension handling. A successful match produces a `KQL-013` proposal in private quarantine with `LocalOnly` disclosure.

A proposal is non-executable. It is not a Mapping Kernel, materialization command, adoption event, truth verdict, or PoMV benefit claim. Materialization and adoption still require their separate authority boundaries.

## 5. Budgets and retry

The caller supplies explicit limits for delta objects, candidate pairs, and proposals. Processing is atomic at a delta-object boundary: if the remaining pair or proposal budget cannot cover its complete candidate set, that object is deferred and MUST NOT be inserted into the processed frontier. This makes a later bounded retry lossless.

## 6. Determinism and negative assertions

- Input delta objects and local candidates are sorted by full-width content identifiers before evaluation.
- Duplicate delivery cannot create a second frontier evaluation.
- Bridge count, carrier path, seed availability, and arrival order are not matching inputs.
- A remote receptor cannot reveal or infer a private local need.
- No join result establishes that a KU is correct or incorrect.
- No join result creates global completeness, network authority, or automatic adoption.

## 7. Executable evidence

`ku-kql::vnext_reunion` covers:

- one newly received Affordance triggering the matching local active StandingNeed once;
- replay deduplication at the local frontier;
- whole-object deferral under pair or proposal budget pressure;
- inverse matching from exact validated public Receptor objects only;
- private remote objects rejected before matching; and
- proposal-only quarantine with no executable output.
