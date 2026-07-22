# Multi-Bridge Merge Profile v1

> **Task:** `OBP-006`  
> **Depends on:** `OBP-004`, `OBP-005`  
> **Status:** Normative vNext reference implementation

## Contract

Multiple bridges may carry the same canonical OBP message or payload variant. The merge inbox keys state by canonical logical identity, retains a set of observed bridge paths separately and never chooses the first arrival as winner.

- canonical reconciliation messages are deduplicated by a domain-separated digest of exact canonical bytes;
- payloads are keyed by kind, full claimed CID and exact-byte variant digest;
- duplicate delivery on one path and duplicate delivery across paths have distinct telemetry outcomes but identical knowledge-plane meaning;
- conflicting byte variants for one claimed CID are retained and delivered deterministically; content validation decides each variant;
- all canonical manifests are applied before payload variants, independent of carrier arrival order;
- adding bridge paths does not change `semantic_delivery_digest` and grants no authority or fidelity amplification.

The inbox delivers into the crash-resumable journaled state machine. Replaying the same merged inbox repairs incomplete journal state and remains idempotent at the validated sink.

## Evidence

Tests cover:

- identical semantic delivery and accepted set through 1, 2 and 5 bridges;
- 1,000 repeated deliveries across five paths with one durable sink insertion;
- conflicting valid/corrupt payload variants ingested in opposite orders with the same final accepted set;
- bridge-count-independent semantic digest and negative authority assertion.

Implementation: `src/ku-net/src/vnext_bridge_merge.rs`.
