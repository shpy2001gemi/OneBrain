# Blind Encoding Fidelity Workflow v1

> **Task:** `FID-002`  
> **Status:** Complete  
> **Depends on:** `FID-001`, `CAP-005`

## 1. Purpose

FID-001 defines evidence shapes. This profile defines the local commit-before-reveal workflow that produces an external blind attempt, reveals the candidate only after commitment, runs explicit source-span/gene/concept checks and preserves all validated alternate encodings.

`ku-ai::vnext_fidelity` is a local coordinator. It does not require a central scheduler, network quorum or global session clock.

## 2. Commit-before-reveal state machine

A session progresses monotonically:

```text
AwaitingAttemptCommit
  -> AwaitingTargetReveal
  -> ReadyForChecks
  -> Completed
```

`BlindEncodingRequest` contains source reference/input commitment, blind-session commitment, challenge-nonce commitment and policy reference. Its type contains no candidate target field.

The coordinator accepts only a CAP-005 result that:

- terminated `Completed`;
- has an output commitment matching the exact output bytes;
- includes the request's source input commitment; and
- has a canonical ExecutionRecord body.

It then emits an immutable external `EncodingAttempt` whose candidate field is absent. Correlation evidence must crypto-bind both the blind session and challenge nonce. Revealing a candidate before this object is content-addressed is rejected.

This proves only ordering and binding inside this workflow. It does not claim that two models are cognitively independent or that an external environment leaked no information outside the transcript.

## 3. Exact fidelity checks

`FidelityCheckPlan` is source-specific verification evidence. It names the expected sets of:

- source-span commitments;
- existing gene-selection commitments; and
- Concept CCIDs.

It also names the provenance reference for each expectation. No new Core DNA gene or opcode is introduced.

`CandidateEncodingInspection` carries independently decoded observed sets and a completeness flag per channel. Each check is deterministic:

- incomplete inspection -> `UNRESOLVED`;
- exact set equality -> `CONSISTENT_WITH_SOURCE`;
- complete unequal sets -> `HARD_ENCODING_MISMATCH`.

The check commits both expected and observed canonical sets. “Hard mismatch” is strictly a source-to-encoding fidelity result; it is not a truth verdict about the source knowledge.

## 4. Two-group external contract

`BlindAttemptPortfolio` indexes completed attestations by exact source/candidate pair. Under `FidelityPolicy/1`, an external contract is met only when:

- at least two hard-mismatch-free external attestations are present; and
- their policy-derived correlation keys contain at least two evidenced-distinct administrative-principal + pipeline/model-lineage groups.

Repeated events, DeviceID/NodeID changes or multiple bridges do not create groups. The portfolio explicitly does not establish “cognitive independence”; it only demonstrates the named correlation evidence contract.

Publisher-attempt inclusion and frontier-relative final status remain FID-003 responsibilities.

## 5. Immutable alternate archive

`AlternateEncodingArchive` accepts only already validated KnowledgeObjects and stores their original canonical bytes under `(source, ObjectCID)`. Exact replay is idempotent; a CID/byte conflict is rejected. There is no winner-selection or deletion API.

Therefore an encoding that receives a hard mismatch attestation can remain preserved as an alternate, regression fixture or historical artifact. A later encoding does not overwrite it.

## 6. Interpretation boundary

Completing the workflow does not:

- classify a KU as true, false or wrong;
- delete an alternate;
- block publish/query/use of knowledge;
- auto-adopt or materialize an encoding;
- prove cognitive independence; or
- create benefit, reward or OBT.

## 7. Executable evidence

Tests prove:

- target reveal is impossible before output commitment;
- the external attempt object contains no candidate target;
- exact span/gene/concept mismatch is explicit without truth classification;
- two external attempts require two evidenced principal+pipeline groups;
- incomplete execution cannot become a blind attempt; and
- two validated alternate encodings remain archived without winner cleanup.
