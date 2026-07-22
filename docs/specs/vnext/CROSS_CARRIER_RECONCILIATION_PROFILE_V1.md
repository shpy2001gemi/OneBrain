# Cross-Carrier Reconciliation Profile v1

> **Task:** `OBP-007`  
> **Depends on:** `CAR-001`, `OBP-005`, `OBP-006`  
> **Status:** Normative vNext reference implementation

## Contract

One canonical `CarrierRecord/1` feeds the same multi-bridge inbox, persisted journal and reconciliation receiver through:

- deterministic in-memory delivery;
- reopenable file-bundle delivery;
- delayed/offline store-carry-forward delivery;
- length-delimited QUIC stream framing.

Carrier type and path metadata do not enter semantic reducers, authority, fidelity, adoption or completion. The QUIC adapter owns only bounded stream framing; authenticated session and socket lifecycle remain transport concerns.

A delayed carrier before its release point reports `reachable_now=false` and an `unknown_pending` upper bound. It never converts absence into falsity or global completion. When released, the exact same canonical records flow through the standard inbox and receiver.

QUIC frames have a four-byte network-order length followed by one canonical carrier record, with a 4 MiB ceiling and exact-length enforcement.

## Evidence

The shared conformance test runs one manifest and two payloads through memory, file, delayed and QUIC-framed adapters. All four produce the same sorted accepted CID set and `ManifestBatchComplete` state. Tests also cover delayed partial state, negative authority assertions, truncation and length mismatch.

Implementation: `src/ku-net/src/vnext_carrier_adapter.rs`.
