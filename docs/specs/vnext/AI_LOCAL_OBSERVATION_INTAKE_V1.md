# AI Local Observation Intake Profile v1

> **Task:** `AI-003`  
> **Status:** Complete  
> **Depends on:** `OBS-002`, `AI-001`, `FID-001`

## 1. Purpose

This profile defines the local, offline-first boundary from text, file or sensor
bytes to an encoding proposal with exact provenance:

```text
authorization assessment
  → encrypted LOCAL_ONLY SourceArtifact
  → local adapter + bounded source spans
  → encrypted payload + signed LOCAL_ONLY ObservationEvent
  → non-executable ReceptorEncodingDraft proposal
```

Observation is input evidence for encoding fidelity. It is not itself a KU
truth claim, Use event, benefit event or publication instruction.

## 2. Append-only allocations

AI-003 adds generic object kinds:

- `22` — `source-artifact`;
- `23` — `observation-event-payload`;

and event type:

- `7` — `observation`.

These are vNext object/event schemas, not Core DNA Genes or opcodes.

## 3. Consent and revocation gate

`ObservationGovernance` binds the capture to:

- consent policy and consent-receipt references;
- revocation-policy reference;
- retention-policy reference;
- capture-scope commitment;
- authorization-assessment commitment; and
- assessed authority frontier.

The intake caller must supply a matching local assessment. Only `Authorized`
passes. `Denied`, `Revoked` and `Unresolved` stop before SourceArtifact creation
and before the observation adapter sees raw bytes. A mismatched commitment or
frontier also fails closed.

The profile preserves the revocation path for future authorization decisions;
it does not claim that historical signed observations can be retroactively
erased from a public network. Raw observations avoid that problem by never
entering the public plane.

## 4. Private SourceArtifact

SourceArtifact supports text, file and sensor classes and binds raw bytes,
media-type commitment, capture adapter, local capture sequence and full
governance. Its only constructor emits `DisclosureClass::LocalOnly`.

The artifact is canonical and content-addressed, then passed through the shared
`PrivateVault::put_verified_object` boundary. Plaintext is encrypted before it
reaches the Vault backend. The intake exposes no Public Store, inventory,
network or publish method.

Raw payload is bounded to 786,432 bytes per artifact. Larger sources require
explicit local chunking/manifest policy rather than silently widening the
canonical object profile.

## 5. Signed ObservationEvent

After the source is accepted by the Vault, a local adapter receives a scoped
view containing the artifact reference and raw bytes. It returns:

- resolved observation-kind CCID;
- byte ranges into that exact artifact;
- typed limitations; and
- fields for a candidate Receptor draft.

Every range must be non-empty and within the immutable raw source. The intake
turns ranges into full `SourceSpan`s and rejects fabricated/out-of-range spans.

The derived payload repeats source identity, extractor, limitations, governance
and observed frontier. A feed-authored ObservationEvent references exactly that
payload object; both payload and event are `LOCAL_ONLY`, signature-validated and
stored in the encrypted Vault.

The event is not Use or benefit evidence.

## 6. Encoding proposal

The final `ObservationEncodingProposal` binds SourceArtifact ObjectCID,
observation payload ObjectCID, Observation EventCID and governance. Its
`ReceptorEncodingDraft` uses the AI-001 `Emergent` origin:

- detector = exact adapter reference;
- observations = exact SourceArtifact reference; and
- evidence spans = validated source ranges.

Requested disclosure is forcibly `None`, so the AI-001 encoder selects
`LOCAL_ONLY`. The proposal has no execute/publish method. A later explicit
encoding, fidelity, disclosure and publication workflow remains necessary.

## 7. Failure and offline behavior

No cloud or network service is required. Adapter failure after source capture
may leave the already-authorized raw SourceArtifact safely retained in the
Vault for audit/retry, but cannot create an ObservationEvent or proposal.

An invalid adapter span similarly cannot rewrite or delete its immutable source.
Idempotent replay returns the existing encrypted records by CID/EventCID.

## 8. Boundaries

The profile does not:

- observe before authorized consent/revocation assessment;
- send raw observations to a cloud or network adapter;
- auto-publish raw, event, Receptor or KU bytes;
- let an adapter invent a span outside its source;
- turn an observation into truth, Use, benefit, PoMV or OBT;
- make an encoding proposal executable; or
- introduce a Core DNA Gene or execution opcode.

## 9. Executable evidence

Four tests prove:

- raw bytes, payload and signed event remain private while AI-001 encoding keeps
  exact SourceArtifact span and local disclosure;
- denied, revoked and unresolved authorization stop before adapter invocation;
- an out-of-range adapter span is rejected; and
- text, file and sensor inputs share the same offline/private/non-publishing
  boundary.
