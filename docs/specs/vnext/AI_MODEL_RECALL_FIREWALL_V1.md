# AI Model Recall Firewall Profile v1

> **Task:** `AI-002`  
> **Status:** Complete  
> **Depends on:** `KQL-008`, `CAP-001`

## 1. Purpose

This profile lets local KGE, embedding and LLM implementations improve candidate
recall and ordering without acquiring semantic validity, materialization,
adoption or execution authority.

The adapter boundary is deliberately asymmetric:

```text
deterministic seeds ─┐
                    ├─ candidate union/rank ─ symbolic validator ─ proposal disposition
model recall ───────┘                         (model score is not an input)
```

## 2. Candidate identity and model provenance

`RecallCandidateSeed` derives its ID only from the canonical sorted set of
full-width object references. The same candidate therefore has the same
identity with the model enabled or disabled.

Every model page binds:

- capability definition ObjectCID;
- implementation-manifest reference from the CAP-001 operational layer;
- exact model-version reference;
- invocation commitment;
- query commitment;
- bounded work usage; and
- optional non-zero continuation.

These fields provide operational provenance only. They grant no authority.
Malformed query binding or a budget overrun rejects the page before symbolic
output is produced.

## 3. Recall-only adapter

`CandidateRecallAdapter` can return candidate object sets, exact rational recall
scores and continuation. It has no method for returning Mapping validity,
constraint truth, materialization commands, resolution actions or capability
execution.

The deterministic/offline path passes `None` as the optional adapter and never
calls model code. Model failure therefore cannot remove the baseline candidate
path.

Duplicate candidate identity is merged. Model score may order the local
validation queue, but neither score magnitude nor repeated appearance changes
the validation result.

## 4. Symbolic validity firewall

Every recalled candidate is passed to `SymbolicMappingValidator` with only:

- canonical candidate ID;
- candidate object references; and
- symbolic validation-context commitment.

The model score and model evidence are not part of this request. The returned
assessment must bind the same candidate and context, an exact MappingKernelCID,
a validator-version reference and unique typed checks.

The firewall—not the model—derives disposition:

- any required `VIOLATED` check → `RejectedRequiredViolation`;
- otherwise any required `UNKNOWN` check → `DeferredRequiredUnknown`;
- otherwise → `EligibleProposalCandidate`.

Eligible still means proposal candidate only. It is not materialized, adopted
or executable.

## 5. Ablation invariant

For the same candidate object set and symbolic validation context, model on/off
must yield the same MappingKernelCID, checks and disposition. Enabling a model
may:

- recall additional candidates;
- omit candidates not present in deterministic channels;
- change validation order/rank; or
- provide model provenance for audit.

It may not change the symbolic validity of a Mapping common to both runs.

This invariant makes model/index components disposable: disabling or rebuilding
them can reduce recall quality without losing canonical knowledge or changing
the validity boundary.

## 6. Boundaries

The profile does not:

- treat embedding similarity, LLM confidence or KGE score as a hard check;
- turn `UNKNOWN` into false;
- let a model create an active Mapping edge;
- let adapter availability become query/action authority;
- claim that a symbolically eligible proposal is useful, true or beneficial;
- consume PoMV or OBT as eligibility; or
- introduce a Core DNA Gene or execution opcode.

## 7. Executable evidence

Five tests prove:

- model-on adds/reorders recall while a common Mapping assessment remains byte-
  identical to model-off;
- an arbitrarily high score cannot override a required violation;
- a required unknown is deferred rather than rejected;
- offline model-disabled operation never invokes the adapter; and
- a model page bound to another query fails before validation output.
