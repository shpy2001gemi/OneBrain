# Metabolic Evidence View v1

> **Task:** `POMV-002`  
> **Status:** Complete  
> **Depends on:** `POMV-001`, `FEED-001`, `KQL-001`
> **Live runtime:** [Distributed Public PoMV Runtime Profile v1](DISTRIBUTED_POMV_RUNTIME_PROFILE_V1.md)

## 1. Purpose

`MetabolicEvidenceView` is a versioned, policy- and frontier-relative derived
view answering a narrow question: which accepted signed Use/Derivation events
in this local evidence scope exercised one knowledge object?

It is not a truth score, popularity score, benefit decision, reward
instruction, universal rank or global view. Knowledge used for comparison,
opposition or refutation is valid exercise evidence in exactly the same way as
other declared Use modes.

## 2. Three strictly separated layers

### 2.1 Cumulative exercise evidence

Authorized POMV-001 records are deduplicated by full EventCID and rooted as a
canonical sorted set. The cumulative set has no time or activity decay. One
event arriving through one bridge or twenty bridges therefore contributes one
EventCID.

Authority reassessment may change whether a record is admitted at a later
frontier. This is a new evidence projection, not decay or deletion of the
signed event. Derivation events attach to exact input/output references;
comparison/opposition Use attaches to its exact subject without asserting that
either side is true or false.

### 2.2 Recent exercise activity

Recent activity is a separate projection over the same admitted EventCIDs. Its
relative display weight may fade linearly from `1_000_000` to zero across a
positive policy horizon measured only in later accepted events of the same
author Feed.

This avoids Earth time, synchronized clocks and invalid ordering across
independent feeds. The weight is not cumulative value, benefit, eligibility or
reward. Once an event leaves the recent window, its EventCID remains unchanged
in the cumulative evidence root.

### 2.3 ExposureTelemetry

`QueryHit`, retrieval and presentation/exposure are local-private telemetry.
They have a separate bounded store and cannot be submitted to the metabolic
reducer. Telemetry records explicitly report `counts_as_use() = false`.

This prevents ranking feedback, repeated delivery or aggressive routing from
manufacturing Use evidence.

## 3. Policy and frontier

Every view binds:

- an exact target ObjectReference;
- an exact view-policy reference and a bounded canonical set of accepted
  evidence-policy references;
- a positive recent-event horizon;
- an exact authority-frontier commitment; and
- accepted per-Feed sequence positions.

An event beyond or absent from the local accepted frontier remains excluded
with `EvidenceBeyondFrontier`. Unauthorized, unresolved and policy-excluded
records remain explicit limitations rather than implicit negative claims.

The evidence root, frontier root and deterministic view root are separate. A
new late/reunion EventCID or frontier/authority change creates a locally linked
view revision. Rebuilding the same set in another arrival order produces the
same roots; local geography and node tier are not projection inputs.

## 4. Bounded retention and honest coverage

Reducers and telemetry stores have positive hard capacities. A full evidence
store rejects a new record rather than evicting accumulated evidence and marks
future views `LocalEvidenceRetentionBound`. Every view also states
`LocalFrontierOnly` and returns `is_globally_complete() = false`.

The derived root is not a Knowledge Object CID or a new canonical authority.
It is reproducible evidence-view identity inside the named policy/frontier.

## 5. Boundaries

The view does not:

- turn retrieval, exposure, ranking or delivery into Use;
- decay or delete cumulative signed evidence or any KU;
- label a KU correct, incorrect, true or false;
- infer benefit from Use alone;
- compare feeds using geography, wall time, bridge count or node tier;
- mint, price, allocate or authorize OBT;
- grant publish, adopt, materialize or execution authority; or
- introduce a Core DNA Gene or opcode.

## 6. Executable evidence

The foundation tests prove:

- QueryHit/retrieval/presentation telemetry cannot change a metabolic view;
- one EventCID observed through many bridges counts once;
- late reunion evidence creates a linked revision;
- recent activity decays by same-feed event distance while cumulative evidence
  root remains unchanged;
- arrival order, geography and node tier are absent from the projection; and
- opposition/refutation Use accumulates evidence without truth, benefit or
  reward claims.

The distributed M4 acceptance test adds real authenticated QUIC evidence: the
same EventCID delivered through one, two and five source NodeIDs still
contributes once; an authority-frontier revocation excludes it without
deleting the immutable record; an idempotency conflict has no arrival-order
winner; and the exact view root, revision and previous-root lineage survive
receiver restart. The report also proves that no wallet, OBT, truth, benefit
or network-completion state is produced.
