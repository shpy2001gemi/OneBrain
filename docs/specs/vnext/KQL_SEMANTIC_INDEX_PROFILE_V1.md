# OneBrain vNext — KQL Semantic Index Profile v1

> **Task:** `KQL-002`  
> **Status:** Normative rebuildable projection — frozen 2026-07-20  
> **Code:** [`ku-kql::vnext_semantic_index`](../../../src/ku-kql/src/vnext_semantic_index.rs)

## 1. Derived-index boundary

`RebuildableSemanticIndex` is a disposable projection over immutable,
validated ReceptorDefinition and KnowledgeAffordance objects. Source ObjectCIDs
remain the source of record. The index contains CID postings only; it cannot
publish, materialize, adopt, mutate semantic objects or grant authority.

Every snapshot carries:

- sorted source-set root over `(source_kind, full ObjectCID)`;
- deterministic projection root over all posting namespaces;
- reducer version and source count.

The same validated source set must produce the same roots across insertion
order, process restart and machine implementation.

## 2. Posting namespaces

Profile v1 derives postings for:

- receptor required role;
- affordance offered role and accepted-input roles;
- every full CCID appearing as role, expected/evidence type, predicate,
  concept term, variable/receptor type constraint or unit;
- statement predicate/operator CCID;
- comparison operator AST kind;
- quantity unit CCID and exact seven-axis dimension vector;
- relation signature over predicate, arity, negation, modality and ordered term
  shapes.

Affordance preconditions, outputs, effects, properties, invariants, operating
conditions, limits and abstraction patterns are all traversed. Receptor hard
constraints are traversed independently of affordance semantics.

## 3. Relation signature limits

Relation signatures are domain-tagged local BLAKE3 digests for candidate
generation. They preserve typed relational shape but are not MappingKernelIDs,
truth claims or authority. A signature hit may generate a candidate only; later
typed matching and three-state validation remain mandatory.

## 4. Rebuild and failure behavior

Clearing or corrupting this projection may reduce temporary query recall but
cannot delete source knowledge. Rebuild starts from validated immutable sources
and replaces the derived view. No legacy graph/index row is treated as an
authoritative source during v1 rebuild.

## 5. Acceptance evidence

- Role, CCID, predicate, comparison, unit, dimension and relation queries return
  exact source ObjectCIDs in deterministic order.
- Forward and reverse source insertion produce identical source/projection
  roots, modeling restart reconstruction.
- `clear_derived` empties only the index; rebuilding restores the same root and
  the Receptor/Affordance input remains unchanged.
