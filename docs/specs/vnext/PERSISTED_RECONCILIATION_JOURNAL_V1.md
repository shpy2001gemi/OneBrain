# Persisted Reconciliation Journal v1

> **Task:** `OBP-005`  
> **Depends on:** `OBP-004`  
> **Status:** Normative vNext reference implementation

## Contract

The journal persists one selector-scoped reconciliation session by its full context binding. It stores canonical manifest batches, accepted full-CID identities, retry counters, next sequence and transient backpressure reservation. Payload bytes remain owned by the validated object/event/mapping sink.

The side-effect order is deliberate:

1. a manifest is validated and journaled before the receiver can rely on it;
2. a payload passes context, selector, declared-length and content-CID checks;
3. the sink performs validate-and-accept atomically;
4. the accepted identity is committed to the journal.

If a process crashes between steps 3 and 4, the durable sink already contains the immutable object. Fair redelivery returns `AlreadyPresent`, repairs the journal and creates no duplicate materialization. If it crashes with an in-flight reservation, reopen clears that reservation without inferring payload acceptance.

## Snapshot and resume

Snapshots use restricted canonical CBOR under `manifest/1` limits. Decode requires an exact canonical re-encode and rejects duplicate records, invalid kinds/statuses, excessive retry counts, wrong binding/config and changed manifest bytes.

`BoundTokenV1` binds:

- reconciliation context digest;
- current journal checkpoint digest;
- exact next sequence;
- a keyed local MAC.

Changing the journal, sequence, context or local token key invalidates the token. A token remains operational continuation data and grants no authority, adoption, truth or reward.

## Bounds

- maximum 4,096 canonical manifest batches per journal snapshot;
- maximum 65,536 accepted/retry records;
- retry ceiling is configured per record and limited to 1,024;
- in-flight payload ceiling is configured and limited to 16 MiB;
- records larger than the in-flight ceiling are backpressured before sink invocation;
- retry counters and accepted identities survive restart.

## Backends and evidence

- deterministic in-memory backend for conformance tests;
- atomic Redb backend under the existing `persist` feature;
- crash-on-Nth-transition harness covering manifest commit, reservation, post-sink journal commit and recovery;
- Redb close/reopen test preserving manifest and accepted identity;
- bounded retry/backpressure and continuation-token tests.

Implementation: `src/ku-net/src/vnext_reconciliation_journal.rs`.
