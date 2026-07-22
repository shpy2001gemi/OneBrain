# KQL Typed Relational Alignment Profile v1

> **Task:** `KQL-008`  
> **Status:** Complete  
> **Depends on:** `KQL-007`, `KU-005`

## 1. Purpose

`TypedRelationalAligner` converts structural-recall candidates into explicit,
bounded partial graph mappings inspired by Structure-Mapping Engine (SME).
It favors systems of corresponding relations over surface vocabulary, while
retaining direction, type, dimension, negation and uncertainty evidence.

The output is a candidate `MappingKernel`, not a materialized/adopted Mapping
and not a truth verdict.

## 2. Inputs and bounded search

An alignment request binds:

- exact source/target ObjectReferences and alpha-normalized SemanticFrameSets;
- their KQL-007 structural-signature descriptors;
- policy-owned assumption and unmapped-reason CCIDs;
- explicit pair-evaluation, result and per-statement diversity caps; and
- an optional row-major continuation cursor.

The aligner evaluates statement pairs within the hard budget. Exhaustion emits
the real next `(source statement, target statement)` cursor and marks unvisited
regions as `MatchBudget`; it never reports global no-match or completeness.

## 3. Pair evidence

Each StatementAlignment retains:

- exact source/target statement indices;
- direct or detected reversed direction;
- exact-predicate status;
- ordered argument correspondences with SATISFIED/UNKNOWN/VIOLATED state;
- same-graph relational connections used as systematicity evidence;
- typed hard violations; and
- typed assumptions.

Hard violations include reversed direction, negation, term-type and physical
dimension conflict. Open variables/Receptors remain UNKNOWN. Predicate changes
create an explicit `PredicateAnalogy` assumption rather than an equality claim.

## 4. Systematicity and many-to-many

Candidate ordering is deterministic over a vector of relational connections,
exact predicates, compatible arguments and hard-conflict count. This is a
bounded policy order, not a universal scalar value.

Per-source and per-target caps may be greater than one. One relation can
therefore align to multiple potential counterparts, and several competing
partial alignments can coexist. Every capped, unmatched, partial-arity or
hard-conflict region remains explicit.

## 5. Reified MappingKernel

The candidate kernel contains:

- statement and argument `TermCorrespondence`s with exact locators;
- Equivalent or Analogous relation kind;
- exact affine unit transforms derived with rational arithmetic;
- policy-typed assumption frames referring to both objects and statement
  indices;
- distinct typed constraints retained as UNKNOWN for KQL-004 evaluation; and
- explicit source/target `UnmappedRegion`s.

The separate evidence vector reports matched pairs, exact predicates,
systematic connections, satisfied/unknown arguments, hard violations and
shared vocabulary-neutral KQL-007 signatures. These components are not folded
into one truth/popularity score.

## 6. Anti-Gravity corpus boundary

The executable `AG-STRUCT-002`-shaped test aligns two relations after a complete
vocabulary/CCID swap using structural signatures, with embeddings and keywords
absent. The same request produces no alignment for the structurally empty
`AG-DISTRACTOR-001`-shaped candidate despite that corpus case's high keyword
overlap.

This demonstrates mechanism-level recall, not that the fictional anti-gravity
scenario is scientifically true.

## 7. Boundaries

Relational alignment does not:

- bypass KQL-004 exact constraint/applicability validation;
- make hard-conflicting output actionable;
- materialize or adopt its MappingKernel;
- activate an OBKG edge;
- judge either source KU true, false, correct or incorrect;
- disclose a private Need/Receptor over OBP;
- create benefit, reward or OBT; or
- introduce a Core DNA Gene or execution opcode.

## 8. Executable evidence

Five tests prove:

- `AG-STRUCT-002` vocabulary-swap structure beats the
  `AG-DISTRACTOR-001` structurally empty keyword distractor;
- reversed direction remains a hard, explainable, non-authoritative result;
- one source relation can align to multiple targets under explicit caps;
- a bounded partial graph returns unmapped regions and a real continuation; and
- Celsius/Kelvin-style unit correspondence is reified as an exact affine
  Mapping transform.
