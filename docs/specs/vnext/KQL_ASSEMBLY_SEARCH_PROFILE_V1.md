# KQL Assembly Search Profile v1

> **Task:** `KQL-009`  
> **Status:** Complete  
> **Depends on:** `KQL-008`, `KQL-003`, `KQL-004`

## 1. Purpose

`AssemblySearcher` finds bounded combinations of two through four candidate
fragments that may fill a local Assembly's stable Receptor placements. It uses
a beam-scheduled, weighted three-state constraint problem and returns a Pareto
portfolio rather than one scalar winner.

The result remains a local search proposal. It cannot materialize a Mapping,
adopt an Assembly revision, activate an OBKG edge, or judge a KU true/false.

## 2. Inputs and identity

Each request binds:

- stable `PlacementId`, required/optional state and a local policy weight;
- candidate ObjectReference and stable local candidate commitment;
- an optional exact KQL-004/KQL-008 Mapping proposal commitment;
- per-placement `SATISFIED`, `UNKNOWN` or `VIOLATED` fit observations;
- systematicity, supporting-evidence and evidence-domain components;
- explicit pairwise three-state compatibility evidence; and
- size, beam, per-page expansion and portfolio bounds.

The context root commits to the normalized semantic inputs, size bounds and
beam width. Page-size and work budgets may change during resume without
changing the search space. Input order does not change the root or output.

## 3. Beam and weighted-CSP boundary

Candidate-level hard violations are excluded before scheduling. Candidates
that offer neither a satisfied nor unknown fit for any requested placement are
also excluded. The remaining candidates are deterministically ordered using
weighted required coverage, unknown coverage, optional coverage,
systematicity, supporting evidence and finally candidate commitment. Only the
bounded beam pool enters combination search.

For each size 2--4 combination, the CSP computes placement coverage and
pairwise compatibility. A violated **required** pair relation is a hard
violation and the whole combination is excluded. A missing pair observation is
`UNKNOWN`, never `false` or implicitly satisfied.

## 4. Pareto portfolio

Every retained candidate carries the full objective vector:

- required and optional satisfied weight (higher is better);
- required unknown and unmet weight (lower is better);
- unknown compatibility and soft-conflict counts (lower is better);
- systematic connections, supporting evidence and evidence-domain diversity
  (higher is better); and
- fragment count (lower is better).

Dominance requires being no worse on every component and strictly better on at
least one. Consequently, a smaller composition and a larger but more
systematic composition may coexist. `AssemblyParetoPortfolio` merges paged
results under the same context root and removes only dominated alternatives.

`ReadyForExactValidation` means that required placement coverage and pair
evidence are fully satisfied within this sampled search. It does not mean
materialized, adopted, useful, beneficial, rewarded, or scientifically true.

## 5. Honest continuation and coverage

Combination enumeration is deterministic and lexicographic inside the chosen
beam. A continuation carries the context root and the exact next combination
indices. Resume rejects a changed context or malformed/non-increasing indices.

`ExhaustedSelectedBeam` only means every configured size combination inside
that local beam was evaluated. It is never a global completeness claim. The
response separately reports hard-blocked inputs/combinations, irrelevant
inputs and candidates pruned by beam width.

## 6. Distributed and partition behavior

Every node can execute this search with its currently available local objects,
constraints and policy. Different partitions may legitimately produce
different Pareto portfolios. After reunion, new candidates or compatibility
evidence create a new context root and a new local search; no coordinator or
network-wide winner is required.

## 7. Boundaries

Assembly search does not:

- turn proposal evidence into canonical Mapping or Assembly state;
- bypass KQL-004 exact typed validation or KU-006 materialization authority;
- infer incompatibility from absent evidence;
- use popularity, trust, benefit, reward or OBT as an eligibility cutoff;
- claim to search all KUs, nodes, partitions or future evidence; or
- introduce a Core DNA Gene or execution opcode.

## 8. Executable evidence

Six tests prove:

- required hard pair violations never enter the portfolio;
- smaller and more-systematic alternatives remain as a Pareto trade-off;
- continuation resumes at the exact next combination and pages can be merged;
- absent compatibility remains `UNKNOWN` and is not validation-ready;
- input reordering preserves the context root and result; and
- emitted composition sizes stay within two through four, without global
  completeness authority.
