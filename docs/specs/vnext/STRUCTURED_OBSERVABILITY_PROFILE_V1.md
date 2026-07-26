# Structured Observability Profile v1

> Status: Frozen M5-02 contract
> Version: 1.0
> Date: 2026-07-26

This profile freezes the operator-visible telemetry boundary for the distributed
runtime. It is observability only: counters and logs do not grant authority,
prove truth, acknowledge delivery, complete a selector, or expose private
identifiers. The machine-readable source is
[`dr-m5-observability-v1.json`](../../../src/test-vectors/vnext/dr-m5-observability-v1.json).

## Normative requirements

M5-02-001 MUST use the frozen `onebrain/dr-m5-observability/1` machine profile.

M5-02-002 MUST expose exactly the 22 typed reason codes in the machine profile.

M5-02-003 MUST NOT accept a free-form reason, label name, or label value.

M5-02-004 MUST count accepted-new, already-present, and replayed as distinct outcomes.

M5-02-005 MUST count missing-dependency and budget deferrals as non-terminal outcomes.

M5-02-006 MUST count quarantine separately from protocol, authority, storage, and resource rejection.

M5-02-007 MUST record admitted bytes, work units, and rate-limit transitions.

M5-02-008 MUST use the frozen finite bucket boundaries for record bytes and work.

M5-02-009 MUST expose active journal depth and observe journal lifetime on close.

M5-02-010 MUST expose pending and retry-exhausted outbox depth.

M5-02-011 MUST derive oldest pending outbox age from durable enqueue time.

M5-02-012 MUST report unknown outbox age rather than inventing age for legacy rows.

M5-02-013 MUST decode durable outbox schema v1 and v2 after the timestamped schema upgrade.

M5-02-014 MUST expose reconciliation lag with the frozen finite record buckets.

M5-02-015 MUST count selector scans, partial continuations, and assessed frontier items.

M5-02-016 MUST expose PoMV identity conflict count and latest view revision separately.

M5-02-017 MUST expose registry state as UNKNOWN, DISABLED, LOADED, or FALLBACK_V1.

M5-02-018 MUST emit one typed REGISTRY_FALLBACK transition per state transition, not per status read.

M5-02-019 MUST emit structured adversarial logs with a typed reason code.

M5-02-020 MUST NOT put NodeID, peer ID, selector, FeedID, CID, or private Need into metric labels.

M5-02-021 MUST NOT serialize local query text or a standing Need identifier in the operator snapshot.

M5-02-022 MUST expose the snapshot only through the authenticated local runtime-status surface.

M5-02-023 MUST keep `claims_network_completion=false` in both runtime and observability status.

M5-02-024 MUST test exact counter transitions, privacy, registry idempotence, and a real-QUIC rejection.

## Counter transition model

Each call records one frozen reason and an explicit count. Aggregate outcome
counters are derived from that reason class in the same in-memory transition.
Histogram buckets are cumulative only through their sample counts and sum;
bucket boundaries cannot be configured by peer input.

The durable outbox stores enqueue/update Unix seconds in schema v3. Legacy v1
and v2 rows decode with unknown age and acquire a timestamp only on a later
durable transition. Unknown age is serialized as `null`.

## Privacy and status semantics

The snapshot contains only fixed field names, typed reason codes, bounded
numeric measurements, and a four-state registry gauge. It has no map keyed by
runtime identity or user content. Structured logs use the fixed target
`onebrain::vnext::observability`; callers may add fixed operation names but not
identity-bearing values.

The REST status is display-only and local-authenticated. A partial scan, zero
lag, empty outbox, loaded registry, or absence of rejection counters never
means global completeness.

## Exit evidence

The work package exits only when mutation tests reject reason inventory drift,
dynamic/private labels, mutable buckets, swallowed-error policy changes, API
snapshot removal, and completeness amplification. Rust tests must prove exact
transitions, schema compatibility, honest age, fixed privacy flags, and one
typed outcome for an invalid record over a real authenticated QUIC session.
