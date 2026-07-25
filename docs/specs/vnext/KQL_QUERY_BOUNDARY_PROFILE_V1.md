# OneBrain vNext — KQL Query Boundary Profile v1

> **Task:** `KQL-001`  
> **Status:** Normative local-runtime contract — frozen 2026-07-20  
> **Code:** [`ku-kql::vnext_query`](../../../src/ku-kql/src/vnext_query.rs)

## 1. Private cognitive boundary

`KnowledgeNeedIR` contains receptor-definition references, desired semantic role
CCIDs, goal frames and local context. Its disclosure class is restricted to
`LOCAL_ONLY` or `NEGOTIATED_ENCRYPTED`. It has no public/network encoder.

Generic object kind `7`, QueryDefinition, wraps the full NeedIR plus query and
exploration policy references. It can only produce a private Knowledge Object
for the Private Vault. QueryDefinitionCID, Receptor/Assembly identity, raw KQL,
exact private context and stable user/node identity are forbidden in ordinary
OBP route payloads.

## 2. Query lifecycle contracts

`QueryRun` is local runtime state bound to one private QueryDefinitionCID and
one validated Selector. Run IDs and work IDs are caller-random, non-zero local
identifiers.

Each `QueryWorkItem` names:

- exact run and work IDs;
- one query channel;
- SelectorCID boundary;
- a budget that may only narrow the parent Selector budget;
- optional continuation.

No work item may expand records, bytes, work units or traversal depth.

## 3. Results, receipts and coverage

Every `QueryResultBatch` and `QueryReceipt` binds the exact run/work,
SelectorCID boundary and assessed frontier. Results are typed as generic Object,
MappingKernel or Event references. Batch validation enforces work budgets,
canonical result-set uniqueness and the shared CoverageStatement contract.

A partial/cancelled/failed result must include a continuation or at least one
explicit limitation. A completed result may only use
`COMPLETE_WITHIN_SELECTOR` from exact inventory. Zero results therefore mean
only zero within the named boundary/frontier; batch/global completion is always
false.

## 4. Route-minimal compiler

`RouteNeedSketch` fields are private and have no public constructor. Only
`DisclosureCompiler::compile_route_minimal` can create its network bytes.
Profile v1 enforces:

- exactly one allowlisted coarse token per sketch;
- estimated support at least `64`, otherwise caller must generalize/suppress;
- at most three independently randomized sketches per local QueryRun;
- non-zero one-time reply capability, replay nonce, expiry, hop and padding
  buckets;
- no reuse of sketch ID, reply capability, replay nonce or commitment salt in
  the same run;
- exact zero-padded network sizes of 512, 1024 or 2048 bytes;
- a salted disclosure commitment rather than a deterministic low-entropy need
  hash.

The outbound schema contains no field for QueryDefinitionCID, run ID,
Receptor/Assembly/Need/User/Node identity, raw text, exact constraints or source
references. Tests additionally scan encoded bytes to ensure the private
definition CID and run ID are absent.

Strict inbound decoding, receiver-relative expiry and replay tombstones are
specified by [RouteNeedSketch Packet v1](ROUTE_NEED_SKETCH_PACKET_V1.md).

## 5. Acceptance evidence

- Public and route-minimal full NeedIR/QueryDefinition construction is rejected.
- Work boundary and all budget dimensions can only narrow the run.
- Partial batches/receipts cannot omit both limitation and continuation.
- Exact zero results are explicitly selector-relative and never global.
- Rare route tokens and a fourth route packet for one run are rejected.

## 6. Implemented M3 network slice

The first live distributed runtime does not transmit a query packet. It
reconciles validated Public `KnowledgeAffordance` objects from authenticated
direct peers and joins them locally against private durable StandingNeeds.
Consequently, peer timeout or absence never blocks local KQL; an empty observed
delta returns zero results with partial, path-limited coverage.

The durable boundary, source provenance and restart behavior are specified in
[Distributed KQL Runtime Profile v1](DISTRIBUTED_KQL_RUNTIME_PROFILE_V1.md).
Route-minimal active discovery remains optional future M6 work.
