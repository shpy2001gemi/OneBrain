# OneBrain vNext — Migration and Rollback Guide v1

> **Task:** `DOC-001`
> **Status:** Frozen foundation release guide
> **Profiles:** [Additive Migration Storage](ADDITIVE_MIGRATION_STORAGE_PROFILE_V1.md), [Legacy Data Backfill](LEGACY_DATA_BACKFILL_PROFILE_V1.md)

## 1. Safety model

Migration is additive. Legacy rows and exact raw bytes remain immutable input;
vNext objects, events, quarantine records and journals occupy parallel
namespaces. A batch commits atomically or leaves no vNext writes. Derived views
are disposable and rebuildable.

Rollback means disabling vNext reads/writes and returning to preserved legacy
read paths. It does not require rewriting history or deleting migrated data.

## 2. Pre-migration checklist

1. capture an exact backup and verify it can be opened;
2. record source-store roots, row counts, schema distribution and code revision;
3. run the contract validator and migration/backfill tests;
4. keep legacy write behavior unchanged and enable vNext dual-read in shadow;
5. choose bounded batch size, disk ceiling, quarantine ceiling and kill switch;
6. ensure raw-byte, journal and quarantine namespaces are independently durable.

## 3. Backfill classes

The backfill classifies all ten legacy families defined by `LEG-002`: KU rows,
genes/codons, bonds, verification records, PoMV/use-like records, wallet/reward
records, graph/index rows, peer/provider rows, identity/feed-like rows and
configuration/auxiliary rows. Classification is explicit and deterministic.

Where legacy evidence cannot establish an exact author, feed, time, authority,
fidelity assessment, checkpoint or consent, migration records the field as
unknown or quarantines the candidate. It never invents missing identity-bearing
facts to make a row fit a vNext schema.

Reward/wallet history remains outside knowledge-plane authority. Old `FULL` or
`GLOBAL` values are preserved only as raw local evidence and conservative
advisory normalization.

## 4. Batch procedure

For each bounded batch:

1. read exact source bytes and stable source coordinates;
2. classify the row and compute a deterministic migration key;
3. validate any candidate canonical object before durable admission;
4. write valid vNext records or a typed quarantine record;
5. append the exact source-to-result journal entry in the same transaction;
6. commit atomically and verify counts, CIDs and journal continuity;
7. rebuild derived indexes/views from admitted canonical state;
8. compare shadow reads without making vNext authoritative for legacy clients.

Restart uses the journal and idempotency key. Replaying a committed batch does
not duplicate objects or silently reclassify quarantine.

## 5. Cutover

Promote vNext reads only after every required class has a deterministic outcome,
quarantine is within the reviewed ceiling, restart tests pass and sampled plus
root-level parity checks match. Keep the legacy reader available for mixed
versions and operator inspection.

Promote OBP exchange separately from local storage migration. Provider hints,
fidelity workflows, checkpoints, GC and reward export each retain independent
feature gates; successful backfill does not authorize them.

## 6. Rollback

1. activate the affected vNext feature kill switch;
2. stop new vNext writes for the namespace while preserving journals;
3. snapshot vNext, quarantine and failure evidence;
4. point reads back to immutable legacy rows;
5. verify legacy counts/roots and local service health;
6. diagnose from exact journal entries and retry only in a new bounded run;
7. rebuild vNext projections after the defect is corrected.

Raw v1 deletion is outside this release profile and requires a separate explicit
operator policy/action. Remote copies cannot be recalled by a local rollback.

## 7. Mixed-version behavior

During migration, legacy peers use the negotiated adapter. vNext peers exchange
canonical records through OBP. Legacy advisory claims cannot become fidelity,
authority, adoption or completion evidence. Nodes that understand only opaque
future schemas may retain/forward their exact envelopes without asserting
semantic validation.
