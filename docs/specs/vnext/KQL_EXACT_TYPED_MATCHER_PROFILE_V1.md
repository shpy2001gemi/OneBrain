# OneBrain vNext — KQL Exact Typed Matcher Profile v1

> **Task:** `KQL-004`  
> **Status:** Normative local-runtime contract — frozen 2026-07-20  
> **Code:** [`ku-kql::vnext_matcher`](../../../src/ku-kql/src/vnext_matcher.rs)

## 1. Decision boundary

Candidate generation may use indexes, embeddings or local AI, but
`ExactTypedMatcher` is the v1 symbolic decision boundary between a candidate
Affordance and a BindingProposal. Its only positive output is a validated,
private BindingProposal plus local explanatory checks. It cannot materialize a
Mapping, adopt a receptor, mutate OBKG or invoke an action.

## 2. Checks

The matcher evaluates:

- exact offered Receptor role;
- predicate and arity structure;
- ordered argument direction and typed term compatibility;
- negation;
- modality and required time qualifier;
- quantity unit/dimension compatibility;
- Receptor/statement TypedConstraints;
- Affordance preconditions against local applicability context.

Known role, structure, direction, negation, term type, dimension or required
TypedConstraint contradiction is `VIOLATED` and emits no proposal. Missing
time, modality equivalence or applicability evidence is `UNKNOWN`, never
rewritten to `VIOLATED` or `SATISFIED`.

## 3. Mapping construction

For a non-violated candidate, the matcher constructs:

- MappingKernel source/target object references;
- statement/argument TermCorrespondences;
- identity or exact affine unit transforms;
- local-context assumptions;
- typed constraint regions with `SATISFIED | VIOLATED | UNKNOWN`;
- explicit unmapped source regions;
- MappingEnvelope generator/rule/evidence provenance.

Unit conversion uses reduced checked rational arithmetic only. It does not use
floating point; dimension mismatch is a hard violation.

The BindingProposal binds each ConstraintObservation to a unique Kernel
constraint-region index and carries two independent exact score components
(structural and constraint fit). Scores are descriptive/vector inputs, not a
scalar winner or truth value.

## 4. Open-world behavior

Absence of applicability/time evidence is an unresolved observation in the
current local context. It may keep a proposal for later evidence, but an action
policy can require all relevant unknowns to be resolved before materialization
or adoption. Hard mismatch rejects only this candidate-to-need mapping; it does
not mark the source KU false or delete it.

## 5. Acceptance evidence

- Equivalent Celsius/Kelvin values produce an exact affine transform and a
  validated proposal.
- Reversed argument direction and opposite negation emit no proposal.
- Missing time, non-identical modality and missing precondition evidence remain
  `UNKNOWN` while preserving a candidate proposal.
- Offered-role and unit-dimension mismatch emit no proposal.
- A required typed constraint known to be violated blocks proposal output.
