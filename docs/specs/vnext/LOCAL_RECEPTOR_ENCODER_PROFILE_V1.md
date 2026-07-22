# OneBrain vNext — Local Receptor Encoder Profile v1

> **Task:** `AI-001`  
> **Status:** Executable local contract — frozen 2026-07-20  
> **Code:** [`ku-encoder::vnext_receptor_encoder`](../../../src/ku-encoder/src/vnext_receptor_encoder.rs)  
> **Corpus:** [`ReceptorEncodingCorpus/1`](../../../src/test-vectors/vnext/ai/receptor-encoding-v1.json)

## 1. Boundary

The Receptor encoder is a deterministic, model-independent boundary between a
local extractor and an immutable `ReceptorDefinition`. An AI may propose a
draft, but the boundary accepts only full resolved CCIDs, immutable object
references, typed constraints and an explicit acceptance-policy reference.

Encoding validates whether the structure faithfully represents the supplied
source and provenance. It does not decide whether the knowledge is true,
useful, globally accepted or valuable.

## 2. No fabrication on omission

`role` and `acceptance` are optional in the draft so omission can be represented
honestly. If either is absent, or declared provenance has no non-empty source
span, the result is `Incomplete` and no object bytes/CID are produced. The
encoder never invents a CCID, policy or source range.

Empty/reversed spans and spans referencing objects outside the declared origin
are adversarial structural errors. Expected types may remain empty, but the
result retains `ExpectedTypesUnresolved`. Partial or unknown constraint coverage
is likewise retained; it is not silently reported as complete.

## 3. Provenance

| Origin | Required to encode | Trace behavior |
|---|---|---|
| Declared | Source object, statement index and non-empty span over that object | Exact span retained locally. |
| Derived | Derivation-rule reference and at least one immutable input | Supplied spans must point into the inputs; absence is an explicit limitation. |
| Emergent | Detector reference and at least one immutable observation | Supplied spans must point into observations; absence is an explicit limitation. |

Trace spans and limitations are deterministically sorted/deduplicated. They are
local encoding evidence, not mutable fields injected into the Receptor object.

## 4. Disclosure firewall

- All origins default to `LOCAL_ONLY`.
- A declared receptor may be explicitly encoded as `PUBLIC` or
  `NEGOTIATED_ENCRYPTED`.
- Derived and emergent receptors remain `LOCAL_ONLY` even if public output is
  requested, and the trace records `DisclosureDowngradedToLocal`.
- `ROUTE_MINIMAL` is not a Knowledge Object storage destination and is rejected.

Later publication must cross a separate explicit policy/materialization
boundary. Local extraction alone cannot publish inferred gaps.

## 5. Executable evidence

The frozen corpus covers declared default-private, declared explicit-public,
derived and emergent encodings. Every result decodes through the generic object
validator as known Receptor kind/major and reproduces its CID. Negative tests
cover missing role, missing declared span, empty span, unrelated evidence span,
privacy downgrade and limitation deduplication.

