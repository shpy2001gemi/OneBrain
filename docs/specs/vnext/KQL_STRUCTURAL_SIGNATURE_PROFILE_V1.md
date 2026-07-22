# KQL Structural Signature Profile v1

> **Task:** `KQL-007`  
> **Status:** Complete  
> **Depends on:** `KQL-002`, `KQL-004`

## 1. Purpose

KQL-007 adds deterministic candidate-recall addresses for knowledge that may be
far apart in vocabulary but close in relational mechanism. It complements the
exact KQL-002 postings; it does not replace KQL-004 typed validation.

This is the foundation for matching, for example, a mechanical observation
about rotating-wheel moment with an unresolved mechanism in a distant research
Assembly without requiring both authors to have used the same words.

## 2. Signature channels

Every signature has a typed kind, digest, vocabulary-sensitivity flag and exact
source region.

### 2.1 Exact CCID-role

`CcidRole` binds full CCID bytes to a role class:

- required Receptor role and expected type;
- offered Affordance role; and
- required/optional accepted-input role.

This channel is deliberately vocabulary-sensitive and provides precise recall
when ontologies align.

### 2.2 Function–Behavior–Structure

Frame sets are classified as:

- Function: outputs and abstraction patterns;
- Behavior: preconditions, effects and operating conditions; or
- Structure: properties, invariants and limits.

The digest preserves bucket, statement/argument topology, qualifier shape and
constraint structure while omitting predicate/concept spellings and CCIDs.

### 2.3 Operator AST

Each typed comparison, dimension and range constraint produces an operator-AST
signature. It preserves operator direction, requiredness, operand shapes,
range inclusivity, exact rational bounds and unit semantics. It does not use
concept names as a shortcut.

### 2.4 Graph shingles

Each statement produces a one-hop shingle over ordered argument roles,
negation/modality/qualifier shape, constraint shapes and referenced-statement
neighbor shapes. Reversing argument direction changes the shingle even when
the vocabulary is identical.

### 2.5 Dimension and unit semantics

Quantities produce separate seven-axis dimension and unit-semantic signatures.
Unit semantics bind exact dimension, scale and offset using reduced rational
integers, not floating point or unit labels. Renaming a unit CCID while
preserving its exact transform therefore does not destroy structural recall.

## 3. Deterministic projection

`StructuralSignatureIndex` is rebuilt only from validated immutable
ReceptorDefinition and KnowledgeAffordance sources. It carries:

- a sorted `(source kind, ObjectCID)` root;
- a deterministic projection root;
- reducer version, source count and signature count;
- per-object descriptors; and
- typed signature-to-ObjectCID postings.

Semantic frame sets are alpha-normalized before extraction. Projection hashing
uses explicit binary region tags and indices; debug strings, arrival order,
local ConceptIDs, geography and node tier are not inputs. Clearing the index
does not mutate source objects, and rebuilding restores the same roots.

## 4. Decision boundary

A signature match is only a candidate hint. It cannot:

- establish a Mapping correspondence;
- satisfy a hard constraint or turn UNKNOWN into SATISFIED;
- materialize/adopt a Mapping or activate an OBKG edge;
- judge truth, encoding fidelity, utility or benefit;
- publish/disclose private knowledge; or
- create reward/OBT, a Core DNA Gene or execution opcode.

All actionable candidates must still pass KQL-004 exact typed matching and the
separate materialization/adoption authority boundaries.

## 5. Executable evidence

Five tests prove:

- rebuild roots are independent of source insertion order;
- a complete vocabulary/CCID rename changes exact-role signatures but preserves
  FBS/operator/graph/dimension/unit structural signatures;
- reversing argument direction changes graph shingles without changing the
  physical dimension channel;
- postings remain non-authoritative candidate hints; and
- clear/rebuild affects only disposable derived state.
