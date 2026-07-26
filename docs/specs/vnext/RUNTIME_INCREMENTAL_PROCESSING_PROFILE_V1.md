# OneBrain vNext Runtime Incremental Processing Profile v1

> **Work package:** DR-P2.4
>
> **Status:** Frozen and implemented — 2026-07-26
>
> **Code:** [`onebrain-node::vnext_record_provenance`](../../../src/onebrain-node/src/vnext_record_provenance.rs), [`onebrain-node::vnext_distributed_kql`](../../../src/onebrain-node/src/vnext_distributed_kql.rs), and [`onebrain-node::vnext_distributed_pomv`](../../../src/onebrain-node/src/vnext_distributed_pomv.rs)

## 1. Selector-scoped typed admission index

Every validated Object or Event accepted from an authenticated peer MUST be
classified by its canonical typed discriminator before it enters the product
incremental index.

The index key MUST bind the exact selector, manifest record kind and typed kind.

Each new typed record under one bound key MUST receive a monotonically
increasing durable sequence independent of CID ordering.

An exact record replay MUST retain its original sequence and canonical bytes.

Authenticated source-peer observations MUST be retained as a deduplicated set
without assigning a new record sequence.

A CID mapped to different canonical bytes under the same typed key MUST fail
closed as an index conflict.

Incremental reads MUST use a bounded prefix range after a durable sequence
cursor and MUST NOT scan the complete accepted Object or Event store.

## 2. Distributed KQL continuation

Each exact KQL selector MUST own a durable processed sequence cursor.

Registering or resuming an active private need MUST reset only that selector
cursor so historical typed affordances can be evaluated for the changed need
set.

A KQL cursor MUST advance only after proposal joining and durable match writes
complete successfully.

A match notification MUST be returned only when its durable
`StandingNeedID + ProposalID` identity is first recorded.

Crash replay of an already durable match MUST NOT emit a second match
notification.

The KQL continuation token MUST commit the exact selector and durable sequence,
not an in-memory collection position.

## 3. Distributed PoMV continuation

Each PoMV selector and typed Object/Event lane MUST own an independent durable
sequence cursor.

PoMV materialization MUST discover changed UseEvidence Objects and Events only
through bounded typed deltas.

Validated changed inputs MUST be merged into a selector-scoped durable input
cache before later requests depend on the advanced cursor.

PoMV input cache and both lane cursors MUST commit atomically after identity
deduplication and view materialization succeed.

A request with no changed typed inputs MUST report zero changed records and
MUST NOT re-observe a durable EventCID identity as a new event.

Authority or feed frontier changes MUST still be allowed to rebuild the view
from the bounded selector-scoped cache and advance lineage only when the view
root changes.

Restart with no changed typed inputs MUST preserve the view root, revision,
previous-root link and exactly-once EventCID identity state.

## 4. Scope and migration boundary

Typed cursors and input caches MUST remain local derived indexes and MUST NOT
claim truth, benefit, wallet state, OBT state or network-wide completion.

Existing pre-profile accepted stores MUST use explicit re-reconciliation or a
separate bounded migration; a product request MUST NOT silently fall back to a
full accepted-store scan.

## 5. Executable evidence

Focused tests prove sequence ordering independent of CID ordering, exact replay,
selector/type isolation, bounded continuation, KQL exactly-once notifications,
PoMV zero-change re-materialization, multi-path source aggregation, authority
frontier revision, publication crash recovery, and restart-stable cursors,
matches, identities and views.
