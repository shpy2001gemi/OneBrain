# Real Redb Process Crash Harness v1

> Status: Frozen M5-03 contract
> Version: 1.0
> Date: 2026-07-26
> Scope: child-process kill, Redb reopen, idempotent recovery, and corrupt-store failure

This profile completes the process-kill coverage reserved by the
[Distributed Runtime Hardening Baseline v1](DISTRIBUTED_RUNTIME_HARDENING_BASELINE_V1.md)
and its
[Transaction Boundary Inventory v1](DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md).
It does not claim network completeness, Outcome, Benefit, reward, or global
finality.

## Activation firewall

- The crash harness **MUST** compile only through the explicit
  `vnext-crash-harness` feature.
- The crash feature **MUST NOT** be enabled by default.
- A failpoint **MUST** remain a no-op unless the explicit kill-switch,
  boundary/phase selector, fresh marker path, and per-case token are all
  present.
- A production/default-feature build **MUST NOT** be stoppable through DR-M5
  environment variables.

## Real process kill

- The parent **MUST** execute a child test process against a real Redb file.
- The child **MUST** fsync a boundary/phase/token marker before it waits.
- The parent **MUST** verify the marker and then terminate the child process.
- A child that exits successfully before the parent kill **MUST** fail the
  case.
- Marker wait **MUST** have a finite timeout no greater than ten seconds.
- Recovery **MUST** use `Database::open`; it **MUST NOT** create a replacement
  database.

## Boundary matrix

- All 13 IDs frozen by M5-00 **MUST** have hooks in their real durable-owner
  source path.
- Every boundary **MUST** execute all five frozen failpoint phases.
- The acceptance matrix **MUST** therefore contain exactly 65 process-kill
  cases.
- Public Use preparation, publication, and network-outbox handoff **MUST**
  remain distinct boundaries.
- Outbox attempt/enqueue and receipt application **MUST** remain distinct
  boundaries.
- Validated storage, selector inventory, authority projection, journal, KQL
  vault/match, and PoMV identity/lineage **MUST** retain separate hooks.

## Recovery oracle

- Restart **MUST** compare the exact 11-field `onebrain/dr-m5-oracle/1`
  inventory.
- Oracle lists **MUST** be sorted and duplicate-free.
- Oracle JSON **MUST** use recursively sorted keys, no insignificant whitespace,
  UTF-8, and SHA-256.
- Recovery replay **MUST** leave canonical, next-side-effect, and ack row counts
  unchanged.
- Recovery replay **MUST** reproduce the same oracle digest.
- Authority recovery **MUST** remain fail-closed and **MUST NOT** amplify
  authority.
- Status/report output **MUST NOT** claim network completeness.

## Storage faults

- Disk-full and read-only injection **MUST** fail explicitly before canonical
  mutation and leave the oracle unchanged.
- A corrupt or truncated Redb file **MUST** fail explicit preflight/open.
- Corrupt/truncated recovery **MUST NOT** rewrite, truncate further, delete, or
  recreate the input file.
- Pending or missing-dependency state **MUST NOT** be compacted as part of crash
  recovery.

## Machine-readable freeze

The executable profile is
[`dr-m5-crash-harness-v1.json`](../../../src/test-vectors/vnext/dr-m5-crash-harness-v1.json).
It freezes the feature firewall, owner-hook mapping, 13×5 matrix, storage-fault
inventory, oracle/report schemas, complete-fixture digest, and crash-report
digest.

## Exit evidence

M5-03 is complete only when:

1. all 65 child processes are killed after a verified fsynced marker;
2. every restart converges to the exact frozen oracle;
3. a second recovery replay changes neither row counts nor digest;
4. disk-full/read-only/corrupt/truncated cases fail explicitly without data
   loss or database recreation; and
5. the feature-enabled remote CI gate passes.
