# OneBrain Base v1 — Storage Integrity Profile

> **Status:** Frozen — Task 2 contract<br>
> **Machine contracts:** [storage integrity](../../../src/test-vectors/vnext/base-v1-storage-integrity-v1.json) and [derived projection](../../../src/test-vectors/vnext/base-v1-derived-projection-v1.json)<br>
> **Scope:** contract and negative-oracle definition; this document is not runtime or qualification evidence.

## 1. Authority boundary

Validated vNext object/event/feed bytes remain the only Base write authority.
Storage adapters MUST NOT derive retention, deletion, graph authority, search
authority, or canonical identity from legacy KU metadata or from a projection.

Redb secondary feed/authority lookup tables are same-transaction indexes. They
MUST commit or abort with the accepted canonical record. Graph, search, and
retriever stores are disposable, generation-swapped projections and MUST NOT
be opened as a source of record.

## 2. Blob namespace and verification

A blob CID is 34 bytes and its storage name is the complete 68-character
lowercase hexadecimal encoding. The filesystem path MUST be
`v2/<digest-byte-0>/<digest-byte-1>/<full-cid>`; version/type bytes do not shard
the namespace, and a short CID is display-only.

Every authoritative read MUST validate declared type and length, every chunk's
BLAKE3 digest, the full-payload BLAKE3 digest, and the full typed CID before
returning any byte. Legacy metadata without chunk digests requires a typed,
atomic metadata migration; it cannot be silently accepted.

One chunk is at most 256 KiB and one blob is at most 100 MiB. Every store MUST
receive nonzero total-quota and free-space-reserve values. Admission uses
checked arithmetic over unique owned physical bytes and MUST reject overflow,
underflow, quota breach, or reserve breach before a write side effect.

Filesystem spill operations MUST use a durable intent, staged create-new
files, fsync, atomic publication, metadata commit, and idempotent cleanup.
Reopen reconciles every nonterminal intent without overwriting a valid target.

## 3. Canonical blob references

`OwnedBlobReferenceV1` is the only Base blob-reference authority. It binds the
full owner `ObjectReference`, blob CID, role, and retention state. A terminal
state additionally binds the full validated `EventCID`; a live state omits it.
The reference MUST be derived by validating canonical vNext object/event bytes
and recomputing their identities. Legacy KU pins or reference counters are
read-only migration evidence and MUST NOT retain or delete Base blobs.

A validated terminal owner event reduces the reference to `terminal-retain` or
`terminal-release` under the frozen reducer. The event MUST NOT rewrite
canonical history, and garbage collection must observe reference parity plus
any durable pending-upload lease before reclaiming bytes.

## 4. Derived projection coverage

The derived-projection contract enumerates all 23 registered vNext object
kinds and all seven registered event kinds. Every output row MUST bind the
canonical source root, full record reference, exact mapping ID, reducer
version, output key/value, per-store index root, and projection-root domain.

Rebuild MUST preserve all canonical branches and apply only validated terminal
reducers; count, arrival order, path count, graph degree, or model score cannot
choose a winner. Unknown kinds follow the frozen opaque/quarantine exclusion
rule and cannot generate ad hoc rows.

A projection may be empty only when coverage names every accepted input and
the frozen mapping proves zero output. Missing rows, unexpected rows, unknown
mapping IDs, and a vacuous mapping MUST fail parity validation.

Corruption or deletion of graph/search/retriever bytes MUST leave canonical
startup available, mark the projection generation dirty/degraded, and rebuild
from validated canonical bytes. Create, update, delete, and rebuild MUST all
reproduce the same mapping-bound projection root.

## 5. Text and canonical exchange

Preview generation MUST decode validated UTF-8 and truncate by Unicode scalar
value, never by byte offset. The v1 display bound is 80 scalar values; invalid
UTF-8 is rejected.

Export MUST emit the exact validated canonical bytes with their full typed
reference. Import MUST decode, validate, canonically re-encode, compare exact
bytes, recompute CID/type/length, and only then commit. An exchange round trip
preserves the same bytes and identity; per-record partial outcomes are explicit.

## 6. Closed storage/archive owner table

`BaseStorageOwnerId` and `ArchiveOwner` use the same big-endian `u16`. Only the
Node adapter performs the one-to-one conversion. Unknown, missing, duplicate,
reused, reserved, endian-swapped, or non-bijective values MUST fail closed.

| Code | Owner |
|---|---|
| `0x0001` | `canonical` |
| `0x0002` | `vault` |
| `0x0003` | `quarantine` |
| `0x0004` | `blob` |
| `0x0005` | `pending_blob_intent` |
| `0x0006` | `source_capture_intent` |
| `0x0007` | `reconciliation` |
| `0x0008` | `inventory` |
| `0x0009` | `outbox` |
| `0x000A` | `provenance` |
| `0x000B` | `private_kql` |
| `0x000C` | `private_pomv` |
| `0x000D` | `operational` |
| `0x000E` | `rollout` |
| `0x000F` | `optional_network` |
| `0x0010` | `migration` |
| `0x0011` | `base_operations` |
| `0x0012` | `interpretation_config` |
| `0x0013` | `identity` |
| `0x0014` | `registry_metadata` |
| `0x0015` | `derived_index` |
| `0x0016` | `retriever_projection` |

`0x0000` and `0x0017..0xFFFF` are reserved. Projection-owner paths belong to
the active dataset generation, while their disposable bytes remain excluded
from archive payloads.

## 7. Crash vocabulary and oracles

`TX-BLOB-001`, `TX-IDX-001`, `TX-ARCH-001`, `TX-RESTORE-001`, and
`TX-RECOVERY-001` use the inventory's exact five phases. Child-process
kill/reopen is an oracle, not a sixth phase.

Every boundary/phase pair MUST run against real files in a child process. The
expected oracle is stored outside the store under test, and reopen MUST yield
the exact pre-state or exact post-state—never a partial state. Evidence records
the boundary, phase, process exit, restart result, and oracle digest.
