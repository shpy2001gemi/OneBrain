# OneBrain vNext — Negotiated Legacy Adapter Profile v1

> **Task:** `LEG-001`  
> **Status:** Complete  
> **Code:** [`onebrain-protocol::legacy_adapter`](../../../src/onebrain-protocol/src/legacy_adapter.rs)

## 1. Isolation and negotiation

`LegacyAdapter` has no public unchecked constructor. It is created only when
the local feature is enabled, both peers offer profile major `1`, both cap
outbound encoding status at `PART=2`, both accept reachable/partial responses
only, and a non-zero authenticated transcript binding is present.

Legacy JSON/TCP remains in `onebrain-protocol::legacy`. It cannot serialize a
vNext logical type and negotiation grants no feed, knowledge, mapping,
execution, fidelity or completion authority.

## 2. Raw evidence preservation

Every normalized inbound record creates a canonical `legacy-evidence` object
(kind `1`, LOCAL_ONLY) containing the exact original wire bytes, transcript
binding and migration EventCID. The returned `original_wire_ref` is the real
ObjectCID of that envelope—not a padded legacy ID or an untyped hash.

Normalization provenance additionally carries the exact assessed frontier,
migration event and adapter-profile commitment. Disabling the adapter leaves
this local evidence and all vNext operation usable.

## 3. Alias downgrade

Inbound query scope `GLOBAL=5` becomes:

- caller-named SelectorCID;
- `CoverageBasis::Sampled`;
- `CoverageStatus::Partial`;
- `PathLimited` and `FrontierIncomplete`; and
- exact local assessed frontier.

It never becomes selector completion or global absence/completeness.

Inbound encoding `FULL=3` or `PART=2` becomes a normalized
`LegacyEncodingClaim`. The claim contains no legacy token, cannot establish
corroborated fidelity, cannot choose/delete alternate encodings and cannot
create reward.

## 4. Outbound firewall

The only adapter serializer emits `REACHABLE_PARTIAL`,
`coverage_complete=false` and clamps requested encoding status to at most
`PART=2`. It never emits `GLOBAL`, `FULL`, status `3`, a vNext object/event or a
network-wide completion statement.

## 5. Executable evidence

Five tests prove explicit safe negotiation, GLOBAL downgrade, FULL isolation,
outbound alias/status firewall and fail-closed unknown-token/missing-frontier
behavior. Raw bytes round-trip exactly and their evidence ObjectCID verifies.

