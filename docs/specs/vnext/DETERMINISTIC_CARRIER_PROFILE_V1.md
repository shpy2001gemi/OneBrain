# Deterministic Carrier Profile v1

> **Task:** `CAR-001`  
> **Depends on:** `NET-001`, `OBS-001`  
> **Status:** Normative vNext reference implementation

## Contract

The carrier layer transports canonical reconciliation messages and context-bound payload frames without interpreting semantic meaning, authority or completion. The same canonical `CarrierRecord/1` is used by:

- an in-memory deterministic carrier;
- an offline file-bundle carrier that survives close/reopen.

`CarrierRecord/1` wraps either exact canonical `obp/reconcile/1` bytes or a payload frame containing context binding, full SelectorCID, record kind, full claimed CID and exact bytes. Decode validates canonical structure and re-encodes byte-for-byte before delivery.

File bundles use a bounded canonical manifest containing at most 65,536 records. Delivery is non-destructive, enabling store-carry-forward and repeated import. Corrupt bundles fail before returning partial records.

## Controlled fault injection

The test carrier supports a deterministic delivery plan:

- canonical or reverse-canonical order;
- 1–1,000 copies per retained record;
- exact dropped ordinals over the canonical list.

The same plan always produces the same output. These controls are transport observations only and never enter reconciliation reducers or authority.

## Evidence

- in-memory and reopened file bundle emit the same record digests;
- duplicate/reverse/drop injection is exact and repeatable;
- malformed record/bundle input produces no partial delivery.

Implementation: `src/ku-net/src/vnext_carrier.rs`.
