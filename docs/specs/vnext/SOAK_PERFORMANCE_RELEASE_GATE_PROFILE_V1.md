# OneBrain vNext — Soak, Performance and Release Gate Profile v1

> **Work package:** `M5-07`
>
> **Status:** Frozen implementation profile
>
> **Machine contract:** [`dr-m5-soak-release-v1.json`](../../../src/test-vectors/vnext/dr-m5-soak-release-v1.json)
> **Executable:** [`onebrain-node::vnext_soak_release`](../../../src/onebrain-node/src/vnext_soak_release.rs)

## 1. Evidence classes

The release harness MUST run as an optimized Cargo release build and MUST use
authenticated real QUIC on loopback. `smoke`, `nightly-24h` and
`pre-release-72h` use the same executable and frozen budgets.

A smoke result MUST NOT claim nightly or pre-release qualification. Nightly
qualification requires at least 86,400 monotonic elapsed seconds. Release
qualification requires at least 259,200 monotonic elapsed seconds and all
other gates in the same report.

GitHub-hosted jobs have a six-hour execution limit. Long runs therefore use a
dedicated self-hosted Linux runner labeled `onebrain-soak`; splitting a short
run or editing a report MUST NOT satisfy the duration gate.

## 2. Measured release signals

Every run records authenticated QUIC connect p50/p95/p99 and 4 KiB
write-plus-fsync p50/p95/p99. It also samples process RSS, recursive runtime
disk bytes and OS task count before, during and after the fault cycles.

Growth gates include a hard cap, endpoint growth cap and positive
bytes/tasks-per-cycle slope. A missing platform signal fails closed. A task or
session that remains active after shutdown is a leak.

KQL and PoMV measurements use the durable selector/type sequence index consumed
by both incremental runtimes. The first bounded scan MUST observe the exact
typed fixture within the record and latency budgets; a second scan from the
returned durable cursor MUST observe zero records.

## 3. Repeated fault cycle

Each three-cycle window contains:

1. an authenticated slow peer held open and closed cleanly;
2. the configured per-peer session cap plus one rejected flood session; and
3. a real endpoint shutdown, failed connection during partition, durable
   receiver restart and authenticated reunion.

Every cycle also runs the deterministic drop/duplicate/delay/reorder trace.
Fair redelivery MUST converge to the frozen oracle root and MUST NOT grant
authority or claim network completion.

## 4. Semantic and operational release gate

M3 reunion identity MUST remain exact. M4 evidence MUST NOT become truth,
Benefit, wallet or OBT state. The report exposes finite rollback reason codes
for latency, growth, leak, incremental scan, reunion, semantic amplification
and incomplete duration evidence.

An operator SHOULD retain the JSON artifact with the exact commit, runner and
workflow metadata. Any rollback reason blocks release and directs the operator
to the durable four-lane generation fence defined by the M5-06 profile.

`pre_release_qualified=true` is valid only for the 72-hour profile after every
budget, fault, semantic, shutdown and duration oracle passes.
