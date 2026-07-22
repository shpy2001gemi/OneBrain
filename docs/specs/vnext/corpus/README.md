# Anti-Gravity Reunion Corpus v1

> **Task:** `FND-009`  
> **Fixture file:** [anti_gravity_v1.yaml](anti_gravity_v1.yaml)  
> **Purpose:** deterministic receptor↔affordance, Mapping and partition/reunion tests

## Important scientific boundary

“Anti-Gravity” is the narrative name inherited from the Founder’s thought experiment. The corpus does **not** assert that an anti-gravity engine exists or that any fixture proves new physics.

The benchmark tests a more general OneBrain capability:

> Can a private knowledge gap in one component discover and explain a structurally relevant public observation/research artifact from another component, while preserving partiality, opposition, units, conditions and privacy boundaries?

## Corpus principles

- Terms use stable fixture IDs, never local `u64 ConceptId`.
- Expected structural correspondences and constraint states are explicit.
- Embedding/KGE/LLM output is never the expected oracle.
- Positive cases include both partial and policy-relative satisfaction.
- Negative cases distinguish hard mismatch from `UNKNOWN`.
- Opposing claims remain present and may be used as evidence/counterexample.
- The reunion case uses a bounded public selector configured independently from the private Receptor.

## Expected runner behavior

The future `FND-004`, `KQL-004`, `KQL-008` and `QA-001` runners should:

1. load the YAML without assigning local ConceptIDs to wire fixtures;
2. construct typed Receptor, Affordance and constraint objects;
3. generate proposals through the requested channels;
4. compare symbolic correspondence/constraint output to `expected`;
5. treat ranking order as policy output, not truth;
6. fail if a partial case becomes `SATISFIED_RELATIVE`;
7. fail if private fields enter the public selector/transcript fixture;
8. pass the vocabulary-swap cases with latent/embedding channels disabled.

## Case groups

| Group | Purpose |
|---|---|
| `AG-STRUCT-*` | Positive structural matching despite vocabulary changes. |
| `AG-PARTIAL-*` | Partial binding and unresolved placement behavior. |
| `AG-ASSEMBLY-*` | Multiple complementary candidates satisfy different placements. |
| `AG-HARD-*` | Unit, direction and negation hard mismatches. |
| `AG-UNKNOWN-*` | Missing applicability evidence remains unknown. |
| `AG-OPPOSE-*` | Conflicting claims coexist and support epistemic use. |
| `AG-DISTRACTOR-*` | Keyword similarity without structural fit. |
| `AG-PRIVACY-*` | Private Need remains local during bounded public reconciliation. |

## Acceptance checklist

- [x] Case IDs are unique.
- [x] Every case has an explicit expected discovery/proposal count and resolution outcome.
- [x] Positive, partial, hard-negative, unknown, opposition, distractor and privacy cases exist.
- [x] Vocabulary-swap cases require no embedding model.
- [x] The fixture makes no scientific truth claim about anti-gravity.
