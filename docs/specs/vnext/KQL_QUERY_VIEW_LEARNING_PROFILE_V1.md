# KQL Revisioned QueryView and Exposure Learning Profile v1

> **Task:** `KQL-011`  
> **Status:** Complete  
> **Depends on:** `KQL-010`, `POMV-001`, `POMV-002`

## 1. Purpose

This profile replaces finalize-and-drop result merging with a revisioned local
`QueryView`, and separates exposure-aware learning from validated Use evidence.
It prevents early arrival, path locality, replay and source multiplicity from
becoming semantic or eligibility authority.

## 2. Canonical result identity

Results are identified by typed canonical CID domain:

- ObjectCID plus its declared object-reference kind;
- MappingKernelCID; or
- EventCID.

The CID is the deduplication key. A second batch carrying the same CID adds
only a provenance occurrence commitment. It never creates another result or a
source-count rank boost. Conflicting declared metadata for the same ObjectCID
is rejected transactionally instead of resolving by arrival order.

Batch commitments bind run, work, canonical result set and the full scoped
`CoverageStatement`. Exact batch replay is idempotent.

## 3. Revisioned QueryView

Each accepted batch or work-done receipt produces a new derived revision with:

- run ID, monotonic revision and parent revision root;
- typed reason and open/work-receipt-observed status;
- canonical result set with occurrence roots;
- batch, receipt and coverage-set roots; and
- the retained scoped coverage statements.

A Done receipt closes only its named work observation. A valid batch for that
work arriving later is accepted as `LateBatchAdded` and creates a child
revision. There is no irreversible `finalize()` gate.

The view never claims global completeness or execution authority. Even a
complete `CoverageStatement` remains exact only within its named selector and
frontier.

## 4. Exposure is not Use

The local learner consumes POMV-002 `ExposureObservation`s:

- `QueryHit` and `Retrieval` are attention-only and cannot create a negative,
  engagement, or Use signal;
- `Presented` requires the matching KQL-010 private selection record and exact
  propensity;
- `NoObservedResponse` is learnable only after presentation; and
- `Engaged` is an attention response, not Use or benefit.

Use enters through a cryptographically bound `ValidatedUseEvidenceEvent` only.
Its EventCID is deduplicated and retained in a separate vector. A Use event is
still not an outcome/benefit statement.

## 5. Propensity-aware aggregation

Presented observations use exact rational inverse-propensity weights:

```text
weighted_presentations += 1 / propensity
weighted_engagements   += 1 / propensity   only when engaged
```

The displayed engagement rate is the exact ratio of these aggregates. Invalid
propensity, rational overflow, observation-ID conflict and capacity failure do
not partially mutate learner state. Exact exposure replay contributes zero
additional weight.

These learned signals may inform a local ranking policy, but expose
`is_eligibility_authority = false`. Popularity, aggregate trust, route distance,
provider count, PoMV and OBT are not eligibility inputs.

## 6. Partition and replay behavior

Each partition may derive a different QueryView from locally received batches.
Reunion or delayed carriers append revisions as new validated evidence arrives.
One result replayed across many bridges remains one canonical result. Its
occurrence root may preserve provenance without creating a scalar count for
ranking.

No server, coordinator, final top-k or global query completion is required.

## 7. Boundaries

The profile does not:

- turn exposure, retrieval, dwell or engagement into Use;
- turn Use into benefit, reward or truth;
- learn a negative preference from an item never presented;
- discard a late valid result because an earlier receipt arrived;
- use source multiplicity or route locality as relevance;
- make QueryView results materialized/adopted/actionable; or
- introduce a Core DNA Gene or execution opcode.

## 8. Executable evidence

Seven tests prove:

- a batch after Done creates a linked late-result revision;
- 100 additional batches with one canonical CID retain one result and no rank
  boost, while exact replay is idempotent;
- conflicting object-kind metadata is rejected atomically;
- QueryHit/Retrieval produce no learned presentation or Use signal;
- presented feedback uses exact inverse propensity and replay does not boost;
- invalid propensity does not consume the observation ID; and
- a signed/validated UseEvent is separate and deduplicated by EventCID.
