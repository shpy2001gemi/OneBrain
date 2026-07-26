# Distributed Runtime Hardening Baseline v1

> Status: Frozen M5-00 contract  
> Version: 1.0  
> Date: 2026-07-26  
> Scope: DR-M5 invariant oracle, transaction boundaries, and real-network CI

This profile freezes the inputs to DR-M5 hardening. It does not claim that the
later process-kill, resource, chaos, compaction, rollback, or soak gates have
already passed.

## Runtime change gate

- Every change below `src/**` **MUST** trigger the feature-enabled real-QUIC
  acceptance job.
- The real-QUIC acceptance job **MUST** have a finite job timeout no greater
  than 45 minutes.
- The gate **MUST** compile the feature-enabled product entrypoints and execute
  the frozen M2–M4, P1–P3, and node-lifecycle acceptance steps listed in the
  machine profile.
- Default-feature and Windows smoke jobs **MUST NOT** substitute for the
  feature-enabled real-QUIC gate.

## Frozen transaction inventory

- The 13 IDs in
  [Distributed Runtime Transaction Boundary Inventory v1](DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md)
  **MUST** remain stable across DR-M5.
- A boundary **MUST NOT** be removed or renamed merely because its implementation
  moves to a different module or table.
- Every boundary **MUST** declare at least one durable owner and at least one
  invariant-oracle component.
- Every boundary **MUST** support the five shared failpoint phase names before
  M5-03 can claim process-kill coverage.

## Frozen invariant oracle

- A crash-run oracle snapshot **MUST** use the
  `onebrain/dr-m5-oracle/1` format and the exact field inventory in the machine
  profile.
- Oracle collection **MUST** preserve sorted, duplicate-free semantic
  identities; arrival order, path count, and provider count are not correctness
  evidence.
- Oracle snapshots **MUST** be canonicalized as UTF-8 JSON with recursively
  sorted keys and no insignificant whitespace before SHA-256 hashing.
- Missing, unreadable, or corrupt canonical state **MUST** fail explicitly and
  **MUST NOT** be represented as a new empty oracle.
- Wallet balance, OBT state, inferred Benefit, or global-completion claims
  **MUST NOT** appear in the DR-M5 correctness oracle.

## Machine-readable freeze

The executable profile is
[`dr-m5-baseline-v1.json`](../../../src/test-vectors/vnext/dr-m5-baseline-v1.json).
The contract validator checks its boundary IDs, oracle fields, digest specimen,
workflow path trigger, timeout, and required acceptance-step inventory.

## Exit evidence

M5-00 is complete only when:

1. the profile and transaction inventory agree on all 13 IDs;
2. the frozen empty-oracle specimen hashes reproducibly;
3. both pull-request and branch-push path filters include `src/**`;
4. the real-QUIC job has the required timeout and acceptance steps; and
5. the default, feature-enabled real-QUIC, and Windows CI jobs pass remotely.

