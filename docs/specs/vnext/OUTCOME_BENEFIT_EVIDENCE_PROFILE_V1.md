# Outcome Observation and Benefit Evidence Profile v1

> **Task:** `POMV-004`  
> **Status:** Complete  
> **Depends on:** `POMV-001`, `EVT-001`, `KU-007`

## 1. Purpose

This profile separates three statements that must never collapse:

1. a signed Use/Derivation event says knowledge was exercised;
2. an `OutcomeObservation` says a change was observed in a task/context; and
3. `BenefitEvidence` assesses outcome valence and causal attribution under an
   exact policy and frontier.

None of these is proposition truth or reward authorization. A signature proves
authorship and byte integrity, not that an observation or attribution is true.

## 2. OutcomeObservation

Generic object kind `19`, signed by event type `5`, binds:

- a private task-context commitment;
- an extensible outcome-class CCID;
- observed valence: beneficial, harmful, mixed, no observed change or unknown;
- an affected ActorID/principal commitment and/or exact Assembly reference;
- bounded measurement-evidence references;
- an optional baseline;
- explicit limitations;
- an observation-policy reference; and
- the exact observed frontier.

An observation with no measurement reference must say
`UnwitnessedObservation`; one with no baseline must say `MissingBaseline`.
These fields allow self-reports and partial observations to be preserved
without silently upgrading them.

`OutcomeCaseId` derives from task commitment, outcome class, affected scope and
observation policy—not the observed valence. The branch reducer therefore keeps
beneficial, harmful, mixed and unknown observations about the same case under
their exact EventCIDs. It has no arrival winner or destructive conflict merge.

## 3. BenefitEvidence

Generic object kind `20`, signed by event type `6`, requires at least one exact
OutcomeObservation reference. A UseEvent alone cannot construct it. The body
also owns:

- exact cited Use EventCIDs and knowledge subjects;
- assessed outcome valence;
- attribution status: supported, opposed, contested or unknown;
- causal/attribution and counterfactual evidence references;
- explicit limitations;
- a benefit-policy reference; and
- the exact assessment frontier.

Missing counterfactual evidence requires `MissingCounterfactual`. Unknown
attribution requires `AttributionUnknown`; it is retained as UNKNOWN and cannot
be coerced to supported attribution. Non-unknown attribution requires both a
knowledge subject and causal evidence.

Outcome references are resolved against validated OutcomeObservation object
CIDs. Missing referenced observations leave the benefit artifact unresolved;
they do not become negative evidence.

## 4. Refutation, conflict and benefit meaning

Comparison, opposition and refutation remain valid POMV-001 Use modes. They may
lead to evidentiary benefit—for example discovering that another hypothesis
does not apply—without making either KU “wrong.”

`establishes_attributed_benefit()` is true only for the narrow conjunction of a
beneficial assessment and supported attribution in this artifact's named
policy/frontier. It still does not establish KU truth, universal value or OBT
entitlement. Harmful, contested, no-change and unknown branches remain useful
evidence and are never deleted by a preferred branch.

## 5. Privacy and decentralization

Affected principals can be encoded as full ActorIDs or opaque commitments.
Disclosure class remains event/object-bound. Nodes can retain, exchange and
reconcile signed branches during partition/reunion without a central outcome
oracle, global clock or global finality claim.

## 6. Boundaries

Outcome/Benefit evidence does not:

- turn Use alone into benefit;
- judge a KU true, false, correct or incorrect;
- select one conflicting observation by arrival order;
- convert missing attribution or counterfactuals into positive evidence;
- materialize, adopt, publish or execute knowledge;
- mint, price, allocate or authorize OBT;
- use geography, fiat, node tier or bridge count as benefit; or
- introduce a Core DNA Gene or execution opcode.

## 7. Executable evidence

Six tests prove:

- signed OutcomeObservation preserves task, affected scope and valence;
- UseEvent-only construction is rejected;
- UNKNOWN attribution cannot establish attributed benefit, truth or reward;
- signed BenefitEvidence binds outcomes, Use EventCIDs, limitations, policy and
  frontier and resolves exact outcome object CIDs;
- comparison/refutation Use remains admissible; and
- contradictory outcomes coexist deterministically as explicit branches.
