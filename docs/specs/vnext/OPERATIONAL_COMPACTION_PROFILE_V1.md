# Operational Compaction Profile v1

Status: normative DR-M5 M5-05 contract.

Machine-readable contract:
[`dr-m5-operational-compaction-v1.json`](../../../src/test-vectors/vnext/dr-m5-operational-compaction-v1.json).

## Feature and generation firewall

- The acceptance harness MUST remain behind the default-off `vnext-compaction-harness` feature.
- Operational compaction MUST start disabled and require an explicit enable operation.
- Every destructive or replaceable commit MUST carry a permit for the current generation and execute while holding the shared commit gate.
- Disable or re-enable MUST invalidate every permit acquired under an earlier generation.
- A stale or disabled permit MUST fail before durable commit.

## Reconciliation journal

- Compaction MUST remove payload bytes only for manifests whose entries are all durably accepted with their exact canonical lengths.
- Completed manifest bytes MUST be replaced atomically by their full-width manifest digest.
- Pending, retrying, inflight, and missing-dependency work MUST remain durable.
- Accepted identities and canonical lengths MUST survive reopen after compaction.
- The journal semantic root MUST be identical before and after compaction.
- The compacted journal snapshot MUST be smaller when eligible manifest payloads exist.

## Outbox audit-first deletion

- Outbox compaction MUST select only `Acknowledged`, `DeadLetter`, or `RetryExhausted` records.
- A `Pending` record MUST never be selected or deleted.
- Before a terminal payload disappears, the same Redb transaction MUST write its audit tombstone.
- An audit tombstone MUST bind intent ID, terminal state, terminal sequence, transport attempts, validation retries, CID, and payload BLAKE3 digest.
- Terminal deletion and its tombstone MUST commit atomically.
- Tombstone retention MUST remain bounded to 65,536 records.
- Redb page compaction MUST demonstrate that a payload-heavy outbox file physically decreases in bytes.

## Bounded quarantine and provenance

- Quarantine and provenance MUST use independent configurable record caps.
- Each configured cap MUST be positive and no larger than the frozen 4,096-record hard cap.
- Each raw evidence record MUST be non-empty and no larger than 1,048,576 bytes.
- Once a lane reaches its cap, further raw payloads MUST NOT be retained.
- Overflow evidence MUST retain a deterministic chain root, dropped-record count, dropped-byte count, and last dropped ID.
- Retrying the same immediately preceding overflow record after a crash MUST be idempotent.

## KQL and PoMV derived snapshots

- KQL and PoMV MUST use distinct snapshot lanes.
- Snapshot rows MUST be sorted, duplicate-free, bounded to 65,536 rows, and encoded canonically.
- A snapshot MUST be bounded to 16 MiB and MUST reject trailing or corrupt bytes.
- Each snapshot MUST bind its lane, reducer version, source root, projection root, and exact rows.
- Store followed by restore MUST reproduce the exact canonical bytes and roots.
- The profile vector's KQL and PoMV roots MUST remain frozen executable evidence.

## Crash matrix

- M5-05 MUST expose exactly five authenticated failpoint phases for each transaction boundary.
- The frozen boundaries MUST be journal compaction, outbox audit-first deletion, quarantine overflow, provenance overflow, and derived-index replacement.
- Acceptance MUST kill a separate process at all 25 boundary/phase combinations.
- Recovery MUST reopen the real Redb file and retry the interrupted operation.
- Recovery MUST produce the same journal semantic root, outbox audit root, evidence oracle root, pending set, counters, and derived snapshot as the uninterrupted execution.
- Failpoint injection MUST remain inert unless the authenticated harness feature and enable environment are both present.

## Exit

- Every crash point MUST restore the exact expected root.
- Pending and missing-dependency work MUST continue after recovery.
- Compaction MUST leave the semantic result unchanged.
- Logical compaction MUST remove eligible payload bytes.
- Physical Redb compaction MUST reduce measured disk usage for the frozen payload-heavy case.
