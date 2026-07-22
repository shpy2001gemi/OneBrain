# Capability Layer and Field Ownership Profile v1

> **Task:** `CAP-001`  
> **Status:** Complete  
> **ADR:** `ADR-KU-046-04`

## 1. Required identity split

OneBrain shares bounded cognitive functions without sharing a whole model, private memory, profile or internal reasoning state. Five layers MUST retain different identities:

| Layer | Ownership domain | Meaning | Never implies |
|---|---|---|---|
| `CapabilityDefinition` | semantic identity | what the bounded cognitive function means | provider availability, implementation identity or permission |
| `CapabilityImplementationManifest` | immutable operational artifact | which model/tool/runtime commitments can implement it | a provider is online, conformance is correctness, or authority |
| `CapabilityOfferBody` | availability claim | a principal/feed advertises a coarse, expiring implementation route | authority, correctness or evidenced fidelity independence |
| `DelegationPermitBody` | unvalidated authority claim | requested executor, purpose, effects, inputs, budget, retention and delegation ceiling | authority before signature/key-state/parent attenuation validation |
| `CapabilityExecutionRecordBody` | execution provenance | task inputs, implementation, outputs, state, limitations, log and retention claim | result correctness, publication, adoption, tool effect or materialization |

Only `CapabilityDefinition` is stable semantic knowledge. The generic Knowledge Object envelope used to encode an ImplementationManifest is a content-addressed container; it does not promote the manifest to semantic KU authority.

## 2. CapabilityDefinition canonical payload

| Key | Field |
|---:|---|
| `0`, `1` | profile major/minor |
| `2` | semantic function CCID |
| `3`, `4` | canonical input/output schema reference sets |
| `5` | preconditions |
| `6` | postconditions and effect classes |
| `7` | accepted KU forms, roles and modalities |
| `8` | deterministic / seeded / stochastic declaration |
| `9` | allowed behavior classes |
| `10` | side-effect class ceiling |
| `11` | failure taxonomy |
| `12` | verification profile references |
| `13` | composition contract |
| `14` | conformance vector reference |

The Rust type has no endpoint, route, current load, latency, provider, exact model, device, ABI or runtime field. Implementation and availability therefore cannot alter the Definition CID. Set ordering is canonical and duplicate members fail canonical-set validation.

## 3. ImplementationManifest canonical payload

| Key | Field |
|---:|---|
| `0`, `1` | profile major/minor |
| `2` | CapabilityDefinition ObjectCID |
| `3` | model/tool/runtime commitments |
| `4` | ABI/codec/protocol support commitments |
| `5` | static resource requirements |
| `6` | determinism and limit declarations |
| `7` | sandbox profile |
| `8` | supply-chain provenance references |
| `9` | conformance evidence references |

Changing a model, tool, runtime, ABI or evidence byte changes the manifest identity without changing the Definition. Static requirements are not current capacity; current capacity belongs only to an expiring Offer.

Model/tool/runtime/build and ABI/codec/protocol entries are typed `OperationalCommitment { kind, digest }` values, not invented ObjectReferences. `CAP-002` defines the reproducible local commitment builder and public-sketch firewall.

## 4. Offer body boundary

The unsigned canonical Offer body contains a typed Actor-or-Feed provider principal, Definition ObjectCID, implementation commitment or coarse class, supported privacy modes, four bounded resource buckets, a self-claimed correlation hint, route/carrier handles, generation and bounded lease interval.

Resource fields are coarse buckets `1..=256`; an Offer cannot publish arbitrary exact hardware numbers through this profile. The maximum profile lease interval is bounded. `CAP-003` owns the signed event/feed wrapper and stale-generation reducer.

Even a valid signature proves only who made the availability claim. `self_claimed_correlation_hint` is routing input only and MUST NOT create or increase an encoding-fidelity group.

## 5. Permit body boundary

The canonical Permit claim binds issuer, executor, Definition ObjectCID, input commitments, allowed effect-class subset, purpose, bounded budget, retention rule, onward-delegation bit, optional parent PermitCID, lease and nonce. The domain-separated claimed PermitCID is not authorization by itself.

`CAP-004` MUST add signature/key-state validation and prove child attenuation for effects, purpose, input scope, budget, retention, lifetime and onward-delegation. Offer, trust, conformance and ExecutionRecord inputs cannot substitute for that validation.

## 6. ExecutionRecord boundary

The canonical execution provenance body binds task, Offer reference, ImplementationManifest ObjectCID, input and prompt/schema/parameter commitments, output references/commitments, partial/completed/cancelled/failed state, start/finish ticks, limitations, log digest, optional attestation and retention claim.

It exposes no API that writes a KU, Mapping, OBKG edge, profile, tool state or adoption event. A completed record and successful conformance still require local quarantine/evaluation and the existing explicit durable-boundary and adoption commands.

## 7. Negative assertions

- Definition and Manifest grant no authority and make no current-availability claim.
- Offer and conformance grant no authority.
- Offer correlation hints are not `CorrelationEvidence`.
- An unsigned or merely content-addressed Permit body grants no authority.
- Execution provenance establishes neither correctness nor benefit.
- Execution output is never auto-materialized, auto-published or auto-adopted.
- None of these layers introduces a Core DNA Gene or opcode.
- None of these layers introduces OBT, seed, bridge, route or geography into semantic identity or authority.

## 8. Executable evidence

`ku-core::foundation::capability` tests canonical set-order stability, Definition/Manifest identity separation, coarse resource and lease limits, Actor/Feed provider representation, Offer/conformance negative authority, unvalidated PermitCID behavior, and ExecutionRecord non-correctness/non-materialization.

Signing and reducers remain deliberately assigned to dependent tasks `CAP-003` and `CAP-004`; this profile freezes the bodies and ownership boundary they MUST preserve.
