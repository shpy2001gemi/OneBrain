# Encoding Fidelity Evidence Profile v1

> **Task:** `FID-001`  
> **Status:** Complete  
> **Depends on:** `CAP-001`, `EVT-001`

## 1. Purpose

Encoding fidelity asks one narrow question: does a named candidate encoding represent a named source artifact, including its selected genes, concepts and source spans, with the stated limitations? It does not ask whether the source proposition is true, popular or useful.

This profile defines immutable `EncodingAttempt`, categorical `CorrelationEvidence`, `EncodingFidelityAttestation` and `FidelityPolicy/1` contracts. The commit-before-reveal session is owned by `FID-002`; policy/frontier assessment is owned by `FID-003`.

## 2. Encoding attempts

An attempt records source, output commitment, pipeline/model/tool commitments, source-acquisition lineage and ExecutionRecord provenance. Roles are explicit:

- `Publisher` includes its candidate encoding reference.
- `ExternalBlind` must omit the candidate encoding and must include blind-session and challenge-nonce commitments.

This type boundary prevents an external-blind attempt object from carrying the target it was supposed to encode without seeing. It does not by itself prove that the session orchestrator withheld all target information; `FID-002` must prove the transcript order.

Attempts are immutable generic object kind `15`. They establish neither proposition truth nor attester independence.

## 3. Correlation evidence vector

`CorrelationEvidence` contains at most one entry per named dimension:

- administrative principal;
- device/feed;
- pipeline/model lineage;
- prompt template;
- preprocessing;
- source acquisition/derivation;
- execution environment;
- blind session; and
- challenge nonce.

Each entry has an optional value commitment, evidence references and one categorical strength:

- `UNKNOWN`;
- `SELF_CLAIMED`;
- `CRYPTO_BOUND`;
- `EXTERNALLY_ATTESTED`; or
- `EMPIRICALLY_ESTIMATED`.

Strength deliberately has no ordering trait and is not a scalar confidence score. A policy names acceptable categories separately for every required dimension. Different NodeID, DeviceID, FeedID, IP, route, bridge count or self-claimed model label never creates a group by itself.

## 4. Default FidelityPolicy/1 contract

The default policy requires:

1. a publisher attempt;
2. at least two external blind attempts;
3. at least two evidenced-distinct external groups;
4. group derivation over both `AdministrativePrincipal` and `PipelineModelLineage`;
5. for each required dimension, evidence categorized as `CRYPTO_BOUND`, `EXTERNALLY_ATTESTED` or `EMPIRICALLY_ESTIMATED`; and
6. source-span, gene-selection and concept-selection checks.

`UNKNOWN` and `SELF_CLAIMED` cannot create a default group key. Device/feed is intentionally not a required distinct dimension. Thus 100 identities with one evidenced administrative principal and one pipeline lineage remain one correlation group.

The derived group key is a policy-local grouping key, not a boolean or claim of cognitive independence. FID-003 may count evidenced-distinct keys only within a named policy and accepted evidence frontier.

`FidelityPolicy` is immutable object kind `16`.

## 5. Signed attestation

An `EncodingFidelityAttestation` binds:

- exact source and candidate encoding references;
- blind-attempt output commitment;
- attempt and ExecutionRecord references;
- the full correlation evidence vector;
- typed source-span/gene/concept checks;
- limitations; and
- policy reference.

Check status is one of `CONSISTENT_WITH_SOURCE`, `HARD_ENCODING_MISMATCH`, `UNRESOLVED` or `NOT_APPLICABLE`. A hard mismatch says that this encoding does not faithfully represent the named source region under the check; it says nothing about whether the source knowledge is true or false.

The attestation payload is immutable object kind `17` and is signed through generic KnowledgeEvent type `4`. Binding requires exactly one payload reference and matching disclosure.

## 6. Galileo and preservation boundary

No attempt, attestation, correlation group or future fidelity assessment may classify a KU as “wrong.” A hard encoding mismatch can prevent that artifact from being presented as a faithful encoding of that source, but it must not:

- delete or mutate the source or alternate encoding;
- block preservation, publication, KQL discovery or use of the knowledge;
- become a proposition truth vote;
- become realized value, benefit, reward or OBT; or
- select a global winner.

## 7. Executable evidence

Tests prove:

- 100 device/feed commitments sharing one principal and pipeline yield one group;
- two evidenced principal+pipeline pairs yield distinct policy group keys;
- self-claimed dimensions yield no group key;
- an external-blind attempt cannot contain the candidate encoding; and
- a signed attestation with a hard gene mismatch remains non-truth, non-`wrong-KU` and non-blocking for preserve/publish/query/use.
