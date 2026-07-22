# OneBrain vNext — Receptor Profile v1

> **Task:** `KU-002`  
> **Status:** Normative object profile — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::receptor`](../../../src/ku-core/src/foundation/receptor.rs)

## 1. Receptor meaning

A Receptor is a typed open semantic role: it says what kind of missing binding
could make a knowledge structure more usable. It is not proof that a missing
piece exists, not a query result, not a ranking score, and not a mutable empty
field inside a source KU.

`ReceptorDefinition` is immutable generic object kind `3`, major `1`. It owns:

- role CCID and a canonical set of expected type CCIDs;
- alpha-normalized hard typed constraints from the Semantic Primitives profile;
- minimum/optional maximum cardinality;
- declared, derived or emergent origin with immutable provenance references;
- an immutable acceptance-policy object reference, required evidence-kind
  CCIDs, and explicit behavior for `UNKNOWN` constraints.

Runtime budget, rank, current candidate list, provider/route state, mutable
resolution status and model confidence are forbidden from the Definition. They
belong to KQL runtime, proposals, events or derived views.

## 2. Origin

| Origin | Required provenance |
|---|---|
| `DECLARED` | Source object plus local normalized statement index that explicitly contains the open role. |
| `DERIVED` | Immutable derivation-rule reference and canonical set of input object references. |
| `EMERGENT` | Detector/model/rule artifact reference and canonical set of observations from which the local system detected the gap. |

These origins describe how the receptor was produced, not whether a proposed
binding is correct. Changing origin changes Definition bytes and CID.

## 3. Acceptance profile

The profile points to an immutable policy object; it does not inline mutable
thresholds from a node. Required evidence kinds are CCIDs. Unknown constraint
handling is one of:

- `REJECT_BINDING` for safety-critical requirements that explicitly demand
  evidence before adoption;
- `KEEP_UNRESOLVED` (default-friendly) so missing evidence is not falsehood;
- `ALLOW_WITH_EXPLICIT_WAIVER`, requiring a later authorized resolution event.

Candidate generation/ranking cannot change this policy and cannot directly mark
a receptor satisfied.

## 4. Private ReceptorClaimEnvelope

`ReceptorClaimEnvelope` is immutable object kind `11`, major `1`, and contains a
Definition reference, a concrete concept/literal/object candidate, and canonical
evidence references. It accepts only `LOCAL_ONLY` or
`NEGOTIATED_ENCRYPTED` disclosure and therefore routes through Private Vault.

Variables, statement-local references and open receptor slots are not legal
claim values: a claim proposes a concrete binding, while partial/open proposals
remain KQL runtime proposals.

Normal construction and canonical encoding emit no commitment field and no
network artifact. This is the required M3 privacy behavior.

## 5. Optional randomized commitment

Only an explicit call supplying both a fresh 32-byte random opening and an
immutable disclosure-policy reference can derive a claim commitment. The
domain-tagged preimage binds:

```text
purpose/version + random opening + disclosure policy + canonical private claim
```

The opening is caller-owned and absent from the returned commitment record.
Different openings unlink identical private claims; the same opening/claim/policy
reproduces the same commitment. A commitment proves only later opening/binding,
not claim correctness, benefit or authorization.

## 6. Acceptance evidence

- Expected-type/evidence/reference set insertion order does not change CID;
  duplicate members are rejected.
- Declared, derived and emergent origins have distinct canonical payloads.
- Private claim round-trips through encrypted Private Vault.
- Public claim construction is rejected.
- Ordinary claim encoding contains no commitment.
- Explicit equal opening reproduces commitment; different opening produces an
  unlinkable commitment.

