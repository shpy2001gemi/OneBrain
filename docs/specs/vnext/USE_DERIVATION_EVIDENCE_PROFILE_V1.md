# OneBrain vNext — Use and Derivation Evidence Profile v1

> **Task:** `POMV-001`  
> **Status:** Executable evidence contract — frozen 2026-07-20  
> **Code:** [`ku-core::foundation::use_evidence`](../../../src/ku-core/src/foundation/use_evidence.rs)

## 1. Meaning

A signed Use/Derivation event is evidence that a signer exercised identified
knowledge in a stated task context and causal role. Signature verifies authorship
and byte integrity. A separate frontier-relative authority assessment determines
whether the local evidence path accepts it.

It does not establish:

- proposition truth or falsity;
- benefit to a person, Assembly or task;
- reward/OBT entitlement;
- popularity, rank or universal value.

These negative properties are executable API constants on `ExerciseEvidence`.

## 2. Use evidence

Immutable object kind `13` contains a canonical subject set, one realized-use
mode, actor-class CCID, private task-context commitment, causal-role CCID,
optional exact Assembly/Mapping context, optional reference to separately
modeled outcome observation, policy and observed frontier.

Use modes cover application, transformation, epistemic comparison/refutation,
transfer, discovery, Receptor/candidate/constraint/gap activity, Assembly use,
analogical transfer and capability-result use. `QueryHit`, retrieval and
exposure are deliberately absent because attention is not necessarily use.

Optional outcome reference means only “this event cites another artifact.” The
UseEvent still cannot assert benefit by itself.

## 3. Derivation evidence

Immutable object kind `14` contains a canonical non-empty set of exact input
references paired with causal-role CCIDs, one output reference, immutable
derivation-rule reference, task-context commitment, policy and frontier.

Changing an input, output, role or rule changes payload identity. Derivation is
a transformation-use path, not proof that its output is correct.

## 4. Signed event binding and record path

Event types `2` and `3` bind exactly one corresponding payload ObjectCID and
must use the same disclosure class. Generic event validation verifies the feed
signature before typed binding.

`ExerciseEvidencePath` deduplicates/reassesses by full EventCID. Replaying one
event through any number of bridges produces one record. Unauthorized records
do not enter the authorized evidence set; unresolved records remain explicit.

There is no ranking, reward, token, graph-materialization or KU-deletion API in
this path. Turning OBT off therefore cannot change record validity.

## 5. Executable evidence

Tests cover signed typed binding, EventCID replay deduplication, separate
authority reassessment, exact derivation inputs/output/roles, absence of
exposure-only modes and mismatch rejection.

