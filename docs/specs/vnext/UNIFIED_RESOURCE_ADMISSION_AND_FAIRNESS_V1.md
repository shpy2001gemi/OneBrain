# Unified Resource Admission and Fairness v1

> Status: Frozen M5-01 contract
> Version: 1.0
> Date: 2026-07-26
> Scope: authenticated QUIC/OBP-RP resource admission, bounded durable state,
> incremental scans, and fair outbound retry

This profile defines one fail-closed resource boundary from untrusted stream
input through application delivery. Passing admission establishes neither
semantic authority nor global completeness.

## Ordered admission pipeline

- Every accepted network record **MUST** cross `stream_read`, `frame`,
  `protocol`, `journal`, and `application` in that order.
- A record **MUST NOT** skip, repeat, or reorder an admission stage.
- Record count and frame bytes **MUST** be reserved at `stream_read`; work units
  **MUST** be charged monotonically at every later stage.
- A rejected or abandoned record **MUST NOT** refund rate-window usage.

The runtime uses one shared controller for inbound and outbound sessions.
Handshake permits promote atomically into session permits only after the signed
peer `NodeId` is known. Dropping either permit releases live concurrency counts.

## Allocation lanes

| Lane | Maximum bytes | Allocation rule |
|---|---:|---|
| Session control | 262,144 | `read_to_end` is bounded by this exact lane cap. |
| Carrier frame | 4,194,304 | The 32-bit big-endian length is checked before payload allocation. |
| Canonical protocol payload | 1,048,576 | Decoded reconciliation and bound payload bytes are checked independently of the carrier envelope. |

- An empty or oversized declared carrier length **MUST** fail before allocating
  the declared payload buffer.
- A carrier envelope **MUST NOT** raise the canonical protocol payload cap.
- Handshake, carrier, and protocol code **MUST** consume the lane values frozen
  in the machine profile rather than a generic 16 MiB transport limit.

## Identity and rate quotas

The safe default profile admits at most 128 live handshakes globally and 8 per
IP; 64 live sessions globally, 8 per IP, and 4 per authenticated `NodeId`.
Each session admits 64 contexts, 4,096 records, 16 MiB of record bytes, and
1,000,000 work units. The replay window retains at most 65,536 session IDs.

The rolling window is 60 seconds. A `NodeId` receives 8,192 records, 16 MiB,
and 2,000,000 work units per window. Per-IP window limits are those values
multiplied by the per-IP session cap; global limits are multiplied by the
global session cap.

- Handshake and session quotas **MUST** be enforced globally and by source IP.
- Authenticated session quotas **MUST** also be enforced by full-width
  `NodeId`.
- Record, byte, and work quotas **MUST** be enforced per session and in global,
  IP, and `NodeId` rolling windows.
- Rejected identities **MUST NOT** create unbounded counter-map entries.
- Session replay and reconciliation context registries **MUST** have finite
  hard caps.

## Bounded durable state and disk pressure

The exact accepted, quarantine, inventory, provenance, KQL-match, outbox, and
tombstone bounds are frozen in
[`dr-m5-resource-admission-v1.json`](../../../src/test-vectors/vnext/dr-m5-resource-admission-v1.json).

- New durable identities **MUST** fail closed when their store reaches its hard
  record cap; idempotent replay of an existing identity remains allowed.
- Provenance **MUST** bound both total observations and source-peer fan-out for
  one record.
- Network validate-and-persist **MUST** check projected `vnext_*` disk use
  against the configured product hard watermark before writing.
- Disk pressure **MUST NOT** be represented as successful application
  admission.

## Incremental KQL and PoMV scans

Validated typed provenance is indexed by
`selector || record-kind || type-id || monotonic-sequence`. KQL and PoMV retain
independent durable sequence cursors.

- KQL and PoMV **MUST** scan only their exact prefix and a page of at most 4,096
  records.
- A non-exhausted page **MUST** expose continuation so later arrivals progress
  without a full accepted-store scan.
- Arrival order or content-CID lexical order **MUST NOT** replace the monotonic
  sequence cursor.

## Fair outbox and terminal retention

The durable outbox states are `Pending`, `Acknowledged`, `DeadLetter`, and
`RetryExhausted`. `transport_attempts` counts consecutive delivery failures;
receipt-producing delivery resets it. `validation_retries` independently
counts non-terminal protocol deferrals.

- Pending selection **MUST** use a persisted round-robin cursor and wrap at
  most once per scheduling quantum.
- `DeadLetter` and `RetryExhausted` records **MUST NOT** remain selectable as
  pending work.
- A retry-exhausted lexical first page **MUST NOT** starve a later healthy
  record.
- Transport failure and validation-deferral counters **MUST NOT** refund or
  overwrite one another.
- Terminal payload compaction **MUST** write a bounded audit tombstone in the
  same transaction before deleting the payload row.
- Terminal compaction **MUST NOT** remove pending work.

## Exit evidence

M5-01 is complete only when flood tests demonstrate bounded controller state,
two identities demonstrate healthy-peer progress, outbox tests demonstrate
finite-quantum progress past exhausted prefixes, durable restart tests preserve
cursor and terminal state, and the feature-enabled real-QUIC CI job passes.
Status and reports remain scoped observations and never claim network-wide
completion.
