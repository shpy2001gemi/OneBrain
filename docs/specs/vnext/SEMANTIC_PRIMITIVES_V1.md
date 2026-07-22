# OneBrain vNext — Semantic Primitives Profile v1

> **Task:** `KU-001`  
> **Status:** Normative object-IR profile — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::semantic`](../../../src/ku-core/src/foundation/semantic.rs)

## 1. Relationship to Concept, Gene and Core DNA

This profile does not replace the biological design language and does not add a
new Core DNA opcode/Gene Type.

| Layer | Role after `KU-001` |
|---|---|
| Concept | Language-independent semantic atom identified across nodes by 16-byte CCID. Labels, media and embeddings remain expressions/lookup aids. |
| local `ConceptId u64` | Compression code scoped to one legacy/current Core DNA artifact and resolved through its Concept Table. It is not accepted by this network-safe IR. |
| Instruction/Core DNA | Existing compact immutable KnowledgeKernel artifact. It remains side-by-side and byte-preserved during vNext work. |
| Gene / `gene_type` | Structural/interpretive discriminator for a Core DNA instruction stream; no new number is allocated here. |
| `SemanticFrameSet` | Network-safe object payload for variables, statement identity, qualifiers, typed constraints and exact units that current operands cannot express losslessly. |

`SemanticFrameSet` is carried by generic immutable object kind `2`
(`semantic-kernel`), kind major `1`. Corpus and multi-encoder evidence must decide
which primitives later compile into Core DNA extensions. Until then, no silent
downgrade to `TRIPLE` and no second authoritative KU wire format is allowed.

## 2. Typed terms and statements

`TermRef` is a closed tagged union:

- `Concept(ConceptCcid[16])`;
- lexically bound `Variable(id, optional_type_ccid)`;
- `Literal(Boolean | NFC Text | exact Quantity | Bytes)`;
- local `Statement(id)` reference;
- typed immutable `KnowledgeObject` reference;
- lexically bound `Receptor(slot, optional_expected_type_ccid)`.

`StatementFrame` contains a local statement ID, predicate CCID, ordered
arguments, typed constraints and qualifiers. Statement IDs, variable IDs and
receptor slot IDs are binders inside the immutable payload; they are not node,
concept or global authority identities.

Qualifiers preserve negation, modality, statement-level condition, time,
location, perspective, tolerance and source spans. A condition therefore binds
the whole referenced proposition rather than pretending a single ConceptId is a
compound statement. Source spans bind typed immutable source-object references
and inclusive/exclusive byte or token offsets selected by the owning source
profile; `start > end` is invalid.

## 3. Alpha normalization

Before canonical encoding:

1. statement IDs are renumbered by frame order;
2. variables are renumbered by first semantic occurrence;
3. receptor slots are renumbered by first semantic occurrence;
4. every statement/condition reference is rewritten to the normalized ID;
5. compatible variable type hints are unified; conflicting non-empty type CCIDs
   are rejected.

Consequently, changing only author-local names such as `M=7` to `M=900` cannot
change canonical bytes or the enclosing object CID. Statement order is retained
because order independence is not assumed without a schema-specific proof.

## 4. Exact quantity and dimension algebra

Numeric semantics use normalized exact rational values `(i64 numerator, u64
non-zero denominator)`, never binary float. Arithmetic is checked; overflow is
an explicit error.

`DimensionVector` uses the seven SI base dimensions in this fixed order:

```text
[length, mass, time, electric_current, temperature,
 amount_of_substance, luminous_intensity]
```

Multiplication adds exponents and division subtracts them with checked `i8`
arithmetic. A `UnitRef` retains the source unit CCID plus its dimension and exact
affine transform:

```text
coherent_base_value = source_value × scale_to_base + offset_to_base
```

This supports scale-only units such as mg/g and affine units such as
Celsius/Kelvin while retaining the source unit. Quantity comparison first
requires identical physical dimensions and then compares exact coherent values.
Dimension mismatch/insufficient binding yields `UNKNOWN`, never coerced `false`.

## 5. Typed constraints

Profile v1 supports:

- term comparison: equal, unequal, `<`, `≤`, `>`, `≥`;
- expected physical dimension;
- exact quantity range with explicit inclusive/exclusive bounds.

Each constraint declares whether it is required. Evaluation has three outcomes:
`SATISFIED`, `VIOLATED`, `UNKNOWN`. These are query/mapping evaluation states,
not a judgment that the containing KU is true or false.

## 6. Canonical/resource rules

- Concept/predicate/type/unit identity is always CCID bytes, never local u64.
- Semantic profile root is `{0: major=1, 1: minor=0, 2: statements[]}` under
  `onebrain/canonical/1` and object resource limits.
- At most 4,096 statements, 1,024 arguments per statement and 1,024 constraints
  per statement are accepted in v1.
- NFC text is rejected rather than silently rewritten.
- Unknown generic object kind/major behavior remains governed by the immutable
  object envelope profile.

## 7. Acceptance evidence

- Alpha-renamed variable and statement IDs produce identical canonical bytes.
- Encoded concepts/predicates are exactly 16-byte CCIDs; the API has no local
  `ConceptId` operand.
- Celsius `0` and Kelvin `273.15` compare exactly equal; velocity dimension is
  `[1, 0, -1, 0, 0, 0, 0]`.
- Incompatible mass/length comparison produces `UNKNOWN`, not `VIOLATED`.
- Variable type conflict and unknown statement condition are rejected.
- A semantic frame wraps and validates as known immutable object kind `2` with a
  stable domain-separated `ObjectCid`.

