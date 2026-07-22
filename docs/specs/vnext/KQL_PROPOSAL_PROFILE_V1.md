# OneBrain vNext — KQL Proposal Profile v1

> **Task:** `KQL-013`  
> **Status:** Normative local-runtime contract — frozen 2026-07-20  
> **Code:** [`ku-kql::vnext_proposal`](../../../src/ku-kql/src/vnext_proposal.rs)

## 1. Proposal firewall

`BindingProposal`/`DiscoveryProposal` is an ephemeral local candidate. It is not
a KU, fact, Mapping object, active OBKG edge, receptor resolution or proof of
benefit. The API intentionally exposes no `to_knowledge_object`, event-authoring
or materialization method.

`ProposalQuarantine` stores proposals in a non-executable local namespace. KQL
validators/rankers can read candidates, but graph/tool/profile projectors cannot
consume it as accepted knowledge.

## 2. Required content

A proposal binds:

- proposed semantic MappingKernel and MappingEnvelope;
- exact candidate object references;
- optional index, model and rule artifact commitments;
- a multi-component score vector whose metrics are CCIDs and whose values are
  exact rationals with independent direction;
- required/optional three-state constraint observations, each bound to one
  unique MappingKernel constraint-region index;
- evaluation-count expiry and source EventCID frontier;
- `LOCAL_ONLY` or `NEGOTIATED_ENCRYPTED` privacy.

The envelope's MappingKernelID must equal the proposed kernel's recomputed ID.
Public proposal construction is rejected. Scores are not collapsed into a
canonical scalar winner; duplicate metric CCIDs are rejected.

## 3. Disposition

| Disposition | Meaning |
|---|---|
| `CANDIDATE_ONLY` | Within local expiry and no required violated constraint. Still not actionable/materialized. |
| `BLOCKED_HARD_VIOLATION` | At least one required constraint is violated. Preserve for explanation/audit but action policy cannot accept it. |
| `EXPIRED` | Local evaluation budget/window ended. Remove proposal only. |

`UNKNOWN` required constraints remain candidate-only for later policy/validator
handling; they are not changed to false or satisfied. Expiry never deletes or
mutates source/candidate KUs.

## 4. Local ProposalID

ProposalID is a domain-tagged local BLAKE3 digest over bounded canonical proposal
bytes. It is a dedup key inside ProposalQuarantine, not a network ObjectCID and
not evidence of semantic truth.

## 5. Acceptance evidence

- Proposal store always reports non-executable.
- Hard violation blocks action while preserving the proposal.
- Expiry removes only ephemeral proposal state, not source references.
- Public privacy and kernel/envelope mismatch are rejected.
- Duplicate score metric is rejected; score vector remains multidimensional.
- Duplicate constraint-region observations are rejected instead of being
  double-counted or left semantically ambiguous.
