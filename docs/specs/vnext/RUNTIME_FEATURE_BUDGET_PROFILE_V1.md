# OneBrain vNext Runtime Feature and Budget Profile v1

> **Work package:** DR-P2.2
>
> **Status:** Frozen and implemented — 2026-07-26
>
> **Code:** [`onebrain-node::vnext_config`](../../../src/onebrain-node/src/vnext_config.rs) and [`onebrain-node::vnext_product_runtime`](../../../src/onebrain-node/src/vnext_product_runtime.rs)

## 1. Independent product lanes

`distributed_kql_one_hop`, `public_use_evidence_publish`, and
`distributed_pomv_view` MUST be separate requested feature flags.

Each product lane MUST have an independent emergency kill switch.

Every active product lane MUST require active `object_event_v1`.

Every active product lane MUST require active `obp_rp`.

All three product lanes MUST remain disabled in the default configuration.

A lane never requested by configuration MUST NOT open its lane-specific
durable database. A provisioned lane killed at runtime MUST retain its owner
and database so explicit generation-advancing re-enable does not recreate or
lose durable state.

A typed operation targeting a disabled lane MUST fail closed with the exact
disabled lane identity.

Stopping one product lane MUST NOT implicitly stop either of the other product
lanes.

## 2. Hard runtime budgets

Runtime budget fields MUST reject zero, overflow, inverted, or
implementation-exceeding values during configuration validation.

The KQL policy MUST independently bound scanned records, accepted affordance
objects, candidate pairs, and quarantined proposals.

A caller-provided KQL budget MUST be no larger than every corresponding
configured KQL bound.

PoMV configuration MUST independently bound reducer records and records
returned in one materialized view.

The PoMV view-record limit MUST NOT exceed the reducer-record limit.

Publication outbox draining MUST use a configured maximum flush batch.

Product worker scheduling MUST expose a bounded polling interval.

The polling interval MUST remain between 10 milliseconds and 10 minutes.

Per-peer work and byte policy MUST bound the authenticated network session
record and in-flight byte limits.

Aggregate configured KQL work MUST fit within the per-peer work allowance.

## 3. Storage pressure

The runtime MUST expose separate soft and hard vNext storage watermarks.

The soft watermark MUST be lower than the hard watermark.

Crossing the soft watermark MUST be reported as storage pressure without
inventing data loss or network completion.

At or above the hard watermark, every new lane write MUST fail before its
domain payload is consumed.

Only files in the explicit `vnext_` durable namespace MUST contribute to this
product storage accounting.

## 4. Status and non-goals

Typed runtime status MUST report active lane truth, configured budgets,
accounted storage bytes, and storage-pressure state.

Feature status MUST distinguish a requested lane from a lane active behind a
real listener.

Budget or kill-switch enforcement MUST NOT mutate wallet state, OBT state, or
promote a scoped result to global network completion.

## 5. Executable evidence

Focused tests prove:

- defaults and serialized omissions leave every product lane disabled;
- each kill switch is independent and dependency validation is fail closed;
- never-requested lanes create no KQL, publication, or PoMV database;
- provisioned killed lanes retain their databases for explicit re-enable;
- typed calls into a killed lane return a lane-specific error;
- oversized KQL work and storage above the hard watermark fail before work;
  and
- the all-lanes path still authenticates over real QUIC and opens every
  enabled durable owner.
