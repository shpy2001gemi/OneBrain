# OneBrain vNext — Mixed-Version and Cross-Carrier Conformance v1

> **Task:** `QA-004`  
> **Status:** Complete  
> **Code:** [`onebrain-node::vnext_mixed_conformance`](../../../src/onebrain-node/src/vnext_mixed_conformance.rs)

## 1. Executable matrix

The matrix executes five compatibility cells through public contracts:

| Peer pair | Carrier | Expected semantic result | Authority/completion/fidelity |
|---|---|---|---|
| vNext ↔ vNext | in-memory OBP carrier | two validated payload CIDs | none granted by carrier/receipt |
| vNext ↔ vNext | reopenable file bundle | same accepted CID digest | none granted by file custody |
| vNext ↔ vNext | length-delimited QUIC adapter | same accepted CID digest | none granted by framing |
| vNext ↔ vNext | delayed store-carry-forward | unknown pending before release, then same digest | delay is not absence or completion |
| legacy → vNext | negotiated legacy adapter | LOCAL_ONLY raw evidence + scoped advisory normalization | old peer cannot establish authority, completion or fidelity |

All four native carriers pass through the same manifest-before-payload,
validate-then-accept reconciliation receiver and must yield the same digest of
accepted content identities.

## 2. Outage semantics

The delayed carrier models a relay outage: before release it returns no records,
reports the exact unknown-pending count and grants no authority or completion.
After release it converges to the same result as direct memory, file and QUIC
delivery.

Seed outage is modeled independently: a zero-peer node remains
`USABLE_OFFLINE` with `LOCAL_ONLY` coverage, while the file carrier can retain
and later deliver canonical records. Seed availability is therefore discovery
convenience, not a correctness dependency.

## 3. Downgrade firewall

The mixed-version cell normalizes inbound legacy `GLOBAL` to sampled partial
coverage and legacy `FULL` to a non-corroborating `LegacyEncodingClaim` that
preserves alternates. Outbound status contains neither alias. A peer offering
an unsafe outbound maximum is rejected during transcript-bound negotiation.

No cell can turn transport duplication, a receipt, an old enum, an observed
path or seed reachability into feed authority, Mapping adoption, Receptor
resolution, selector completion or fidelity corroboration.

## 4. Evidence

The matrix test asserts five cells, cross-carrier semantic equality, usable
seed outage, exact delayed unknown-pending behavior, unsafe legacy negotiation
rejection and zero authority/completion/fidelity amplification.

