# OneBrain vNext Mixed-Version Runtime Rollback Profile v1

> **Work package:** DR-M5 / M5-06
>
> **Status:** Frozen and implemented — 2026-07-27
>
> **Machine contract:** [`dr-m5-mixed-rollback-v1.json`](../../../src/test-vectors/vnext/dr-m5-mixed-rollback-v1.json)
>
> **Code:** [`vnext_runtime_rollout.rs`](../../../src/onebrain-node/src/vnext_runtime_rollout.rs), [`vnext_product_runtime.rs`](../../../src/onebrain-node/src/vnext_product_runtime.rs), [`vnext_network_runtime.rs`](../../../src/onebrain-node/src/vnext_network_runtime.rs), and [`vnext_mixed_conformance.rs`](../../../src/onebrain-node/src/vnext_mixed_conformance.rs)

## 1. Mixed-version transport contract

The node-owned legacy TCP/JSON listener and authenticated vNext QUIC listener
MUST run concurrently on real sockets. Failure or rollback of vNext MUST NOT
disable legacy TCP, local KQL, or offline knowledge access.

The checked-in N-1 corpus freezes the exact four-byte big-endian frame prefix
and JSON payload bytes. Every fixture MUST deserialize through the current
legacy `NetMessage` parser and serialize back byte-for-byte. A legacy record
MUST NOT gain vNext authority, global completion, corroborated fidelity, or
wallet/OBT meaning.

## 2. Durable generation fence

The runtime owns four independent durable lanes:

1. authenticated network session admission;
2. distributed one-hop KQL;
3. Public UseEvidence publication; and
4. distributed PoMV view materialization.

Each lane stores an enabled bit and a monotonically increasing generation in
`vnext_runtime_rollout.redb`. A kill transaction MUST persist the disabled
generation before returning. Repeating the same kill MUST be idempotent.

Startup configuration MAY move a lane toward disabled. Startup configuration
MUST NOT move a durably disabled lane toward enabled. Re-enable MUST be an
explicit operator action and MUST advance the generation.

An operation admitted before a generation change MAY drain. New operations
after the kill commit MUST fail closed. An authenticated session MUST recheck
its generation before every later record, so an old session cannot be used to
start a new record or side effect after its generation is killed.

Provisioned owners and databases remain open while a lane is killed. This is
required for a bounded explicit re-enable without deleting and recreating
durable state. A lane never requested by configuration remains unprovisioned.

## 3. Rollback and restart

Runtime rollback MUST atomically disable all four lanes at `TX-ROL-001`.
Rollback MUST NOT delete or rewrite raw accepted records, reconciliation
journals, pending outbox work, quarantine, provenance, KQL/PoMV stores,
wallet state, or OBT state.

After process restart, the durable disabled generations win over an old
enabled configuration. The operator MUST explicitly re-enable each intended
lane. Re-enable of one product lane MUST NOT enable any other lane.

`TX-ROL-001` MUST pass every standard DR-M5 process-kill phase with a real Redb
reopen and idempotent retry. The resulting generation and enabled state MUST
be exact regardless of the kill point.

## 4. Executable evidence

Acceptance requires:

- byte-exact N-1 fixture decoding and reserialization;
- simultaneous legacy TCP and authenticated QUIC exchanges on loopback;
- outbound and inbound QUIC rejection after network kill;
- independent KQL, publication, and PoMV fences;
- upgrade → run → kill → rollback → restart → explicit re-enable;
- protected durable files present and unchanged by rollback; and
- five-phase child-process kill/reopen coverage for `TX-ROL-001`.
