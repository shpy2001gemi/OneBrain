# Deterministic Reconciliation State Machine v1

> **Task:** `OBP-004`  
> **Depends on:** `OBP-003`, `OBS-001`  
> **Status:** Normative vNext reference implementation

## 1. Contract

The reference state machine reconciles immutable, content-addressed records within one authenticated `obp/reconcile/1` context. It is deterministic under message duplication and reordering and converges under fair eventual redelivery. It remains usable within an isolated network component and has no dependency on seed servers, global membership, quorum or OBT.

The implementation separates three facts:

1. a manifest declares that a peer intends to send a full CID and canonical byte length;
2. content-domain/CID validation proves that received bytes match that immutable identity;
3. a `ValidateThenAcceptSink` performs schema/signature/policy validation and durable acceptance as one operation.

There is no unchecked sink `accept` method. Bytes received before a manifest are deferred and never reach the sink. Bytes with an undeclared length, wrong CID, wrong SelectorCID or wrong context binding are rejected before the sink.

## 2. Deterministic state

Receiver state is a projection of canonical sets keyed by `(record kind, full CID)`:

- declared length sets;
- validated/accepted record keys;
- currently rejected record keys;
- typed receipt status.

Duplicate manifests and payloads are idempotent. If a peer advertises conflicting lengths, all observed lengths are retained so arrival order cannot choose a winner; full content identity and sink validation remain decisive. A later valid payload can replace a prior rejected attempt for the same CID.

The states are:

- `AwaitingManifest`;
- `ReceivingPayloads { pending }`;
- `PartialInvalid { pending, rejected }`;
- `ManifestBatchComplete`.

`ManifestBatchComplete` is deliberately not `SelectorComplete`. Selector completion requires a later exact selector-scoped inventory-root comparison and known required checkpoint frontiers. Neither state means global closure.

## 3. Merkle planning

The reference planner:

- emits canonical top-level summaries for Object, Event and MappingKernel radix lanes;
- compares each remote node with the same local lane/prefix;
- emits sorted, duplicate-free divergent ranges;
- projects full-CID manifest entries from those ranges under a caller budget;
- rejects selector mismatch and excessive manifest projection.

The current record-lane summary intentionally does not serialize feed-prefix detail. If the hybrid forest root differs while all summarized record lanes match, the planner returns `UnexplainedHybridRootDifference`; it never reports false closure. A subsequent task must exchange the explicit feed-prefix/checkpoint frontier details before selector completion.

Semantic shard hints are rebuildable accelerators and never enter authoritative diff or completion.

## 4. Failure isolation

Each payload branch is processed independently. A corrupt or schema-invalid branch is recorded as rejected and remains pending, while unrelated valid CIDs continue through validation and acceptance. A receipt can therefore contain both `ValidatedStored` and `RejectedInvalid`; it remains a protocol/storage observation only.

Transport loss is not interpreted as falsity. A payload delivered before its manifest is not buffered as accepted state; fair eventual redelivery after the manifest completes that branch. Repeated manifest/payload delivery produces the same accepted CID set and sink contents.

## 5. Executable evidence

- State machine and deterministic planner: `src/ku-net/src/vnext_reconciliation.rs`
- Hybrid radix inventory: `src/ku-net/src/vnext_inventory_forest.rs`
- Canonical message contract: `src/onebrain-protocol/src/reconciliation_codec.rs`
- CI gate: `.github/workflows/vnext-foundation.yml`

Tests cover manifest-before-payload, no sink call before CID checks, drop/reorder/duplicate convergence, corrupt-branch isolation, deterministic summary/diff/manifest, context/selector rejection and explicit refusal to close on an unexplained feed-prefix root difference.
