# Fidelity Assessment Reducer v1

> **Task:** `FID-003`  
> **Status:** Complete  
> **Depends on:** `FID-001`, `FID-002`, `FEED-001`

## 1. Purpose

Attempts and attestations are immutable evidence; nodes need a deterministic local view over the evidence they have accepted. `foundation::fidelity_assessment` derives that view under an explicit `FidelityPolicy/1` and exact assessed event frontier.

The assessment is rebuildable. It is not a consensus record, winner election, truth vote or deletion instruction.

## 2. Reducer inputs

The reducer is fixed to one `(policy, source artifact, encoding artifact, assessed frontier)` tuple and accepts only:

- validated publisher `EncodingAttempt` records whose source/candidate match the tuple;
- signed and exactly payload-bound `ValidatedEncodingFidelityAttestationEvent` records whose policy/source/candidate match; and
- normalized `LegacyEncodingClaim` records produced by a separate adapter.

Records are keyed by immutable references/EventCID and exact replay is idempotent. Target mismatch or same-identity/different-body conflict fails closed.

`LegacyEncodingClaim` contains commitments, references and limitations only. It has no legacy wire status field or parser and cannot express `FULL` or `GLOBAL`. Legacy-only evidence cannot create a vNext assessment; adding a legacy claim to vNext evidence cannot enter the accepted-attestation root or upgrade status.

## 3. Eligibility and status

Every signed attestation remains in the accepted-attestation set root, including hard mismatches and unresolved checks. Eligibility for the default corroborated status additionally requires:

- all policy-required span/gene/concept check kinds are present;
- no hard encoding mismatch; and
- a policy-derived correlation group key exists.

The reducer emits only:

- `SELF_ATTESTED`;
- `PARTIALLY_CORROBORATED`; or
- `FIDELITY_CORROBORATED_RELATIVE`.

The relative status requires a publisher attempt plus the policy minimum external blind attempts and evidenced-distinct groups. A group count is derived from administrative-principal + pipeline/model-lineage evidence, never raw event, NodeID, DeviceID, FeedID or bridge count.

No canonical `FULL` state exists.

## 4. Canonical assessment

`FidelityAssessment` canonically binds:

- policy/source/encoding references;
- sorted accepted EventCID set root;
- sorted evidenced correlation group keys;
- blind-attempt count;
- coverage counts for publisher, signed/eligible external, groups, hard mismatches, unresolved checks and legacy claims;
- exact assessed frontier;
- relative status; and
- union of explicit limitations.

BTree-keyed evidence plus canonical sets make rebuild independent of event arrival, carrier and island reunion order. Advancing the frontier changes the named observation boundary but does not mutate evidence.

## 5. Galileo and preservation boundary

Failure to meet policy never blocks preservation, publication, KQL discovery or use. Hard mismatch evidence remains queryable and alternate encodings remain intact. Assessment never:

- establishes proposition truth;
- calls a KU “wrong”;
- selects or deletes an alternate;
- creates availability or authority;
- materializes/adopts an encoding; or
- creates benefit, reward or OBT.

## 6. Executable evidence

Tests prove:

- opposite attestation arrival orders produce byte-identical assessment data;
- two evidenced principal+pipeline groups reach only the policy-relative status;
- 100 signed attestations in one group remain partially corroborated;
- hard mismatch remains counted/rooted without cleanup or knowledge blocking; and
- normalized legacy evidence neither upgrades status nor changes the vNext attestation root.
