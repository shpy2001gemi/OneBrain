# OneBrain vNext — Field Ownership Matrix v1

> **Task:** `FND-001`  
> **Status:** Normative  
> **Decision sources:** Research Baseline §46.3 and §56.1  
> **Applies to:** KU Object Family, KQL, OBKG projections, PoMV evidence, OBS, OBP-RP and AI capability contracts

## 1. Purpose

This matrix prevents one field from silently serving incompatible roles. In particular, semantic identity, authority, availability, routing, runtime control and derived assessment must not be collapsed into one object or one CID.

The decisive question is not “which crate currently contains the field?” but:

> Which durable contract owns the meaning of this field, and what evidence may change its derived interpretation?

## 2. Ownership domains

| Domain | Owns | Mutability model | May affect semantic CID? | Examples |
|---|---|---|---:|---|
| `SEMANTIC_IDENTITY` | Meaning independent of provider, route, current user and current task run. | Immutable content-addressed object. | Yes | Receptor role/constraints, Mapping correspondences, Capability semantic function. |
| `PROVENANCE_EVIDENCE` | Who/what produced an artifact and evidence supporting or limiting it. | Immutable envelope or signed event. | Envelope/Event CID only; never Kernel CID. | source refs, signer, model/tool commitments, source-span checks. |
| `AUTHORITY` | Permission to disclose, adopt, delegate or cause side effects. | Signed permit/policy/event evaluated at a scoped key-state frontier. | No | `DelegationPermit`, resolution authority, effect-set ceiling. |
| `AVAILABILITY` | Whether a provider/path/implementation appears usable now. | Expiring signed operational record plus local observation. | No | `CapabilityOffer`, `ProviderLease`, endpoint refs, capacity bucket. |
| `RUNTIME_CONTROL` | Budget, cancellation, deadline, continuation and current execution state. | Run/command/session-local record. | No | `QueryRun`, `MaterializeMappingCommand`, reconciliation cursor. |
| `DERIVED_VIEW` | Reducer result relative to policy, frontier and operator versions. | Rebuildable versioned projection. | No; view has its own optional cache CID. | ResolutionView, FidelityAssessment, CoverageStatement, PoMV view. |
| `LOCAL_PRIVATE_STATE` | Private intent, raw context, debt/propensity, local observation and monotonic receipt time. | Encrypted Vault or local database; not replicated by default. | No public CID. | private NeedIR, ClaimEnvelope, `first_seen_monotonic`. |
| `LEGACY_EVIDENCE` | Original legacy bytes and downgraded interpretation. | Immutable preserved bytes plus migration event. | Never redefines a vNext semantic CID. | `LegacyEncodingClaim`, `LegacyIdentityPrefix`. |

### 2.1 Hard separation rules

1. A `SEMANTIC_IDENTITY` object MUST NOT own endpoint, peer, lease, latency, capacity, route, current rank, current trust or current availability.
2. An `AVAILABILITY` record MUST NOT grant authority or establish fidelity/truth.
3. An `AUTHORITY` record MUST NOT change the semantic identity of the object it authorizes.
4. A `DERIVED_VIEW` MUST identify its policy, source frontier and reducer/operator version.
5. `LOCAL_PRIVATE_STATE` MUST NOT enter public inventory, transcript or derived public object without an explicit disclosure transform.
6. OBT state MUST NOT be an input to object validity, KQL eligibility, reconciliation, Mapping adoption or encoding fidelity.

## 3. Cross-family ownership summary

| Family | Semantic identity owner | Provenance/evidence owner | Authority owner | Availability owner | Runtime owner | Derived view owner |
|---|---|---|---|---|---|---|
| KU | `KnowledgeKernel/CoreDna` and immutable KU objects | Claim/source envelopes, signed derivation/use events | disclosure/use permit when required | provider/custody records | local expression/encode run | expression, epistemic and metabolic views |
| Receptor | `ReceptorDefinition` | `ReceptorClaimEnvelope` | `ResolutionPolicy` + authorized resolution event | none | `KnowledgeNeedIR`, StandingNeed scheduler | `ResolutionView`, ReceptorView |
| Assembly | `FrontierAssemblyManifest` + stable Placement identity | assembly revision event/provenance | assembly authority policy | none | local assembly work state | assembly frontier/resolution projection |
| Affordance | explicit immutable `KnowledgeAffordance` or versioned derived projection | source/derivation trace | none by itself | capability offer only when executable | extractor run | Affordance index/view |
| Mapping | `MappingKernel` | `MappingEnvelope` | `ReceptorResolutionEvent` for adoption | none | `BindingProposal`, `MaterializeMappingCommand` | Candidate/Adopted MappingView |
| Query | `QueryDefinition` when persisted; full NeedIR local/private | result batches and work receipts | disclosure permit/policy | source/provider offers | `QueryRun`, work items, continuation | `QueryView`, `CoverageStatement` |
| Capability | `CapabilityDefinition` | `ImplementationManifest`, `ExecutionRecord` | `DelegationPermit` | `CapabilityOffer`, provider lease | `CognitiveTask` | local conformance/selection view |
| Encoding fidelity | source and encoding artifact CIDs | attempts, attestations, correlation evidence | task/source-access permit | verifier offers | blind attempt task | `FidelityAssessment` |
| Identity/feed | feed inception semantic identity | signed feed events/heads/proofs | key/delegation state | reachability/session observation | reconciliation session | actor/feed union view |
| Checkpoint/GC | checkpoint schema and reducer identity | inclusion/consistency/effect proofs | feed/key state authorizing checkpoint | archive/custody availability | local retention command | restorable checkpoint/retention view |
| Provider discovery | `ProviderTuple` key shape | signed lease/retire records | none | lease record + local observation | lookup/probe | sampled `ProviderLeaseMap` view |
| PoMV/Benefit | evidence schema and event type | signed Use/Derivation/Outcome evidence | none for truth; separate consent for private outcome | none | local observation/correlation run | Metabolic/Benefit assessment by policy/frontier |
| OBKG | no independent canonical knowledge identity | projection provenance | none | none | index/rebuild job | fully derived graph/index views |

## 4. KU and knowledge-object ownership

| Type/field group | Domain | Identity contribution | Notes |
|---|---|---:|---|
| `KnowledgeKernel/CoreDna` instructions, concepts and semantic relations | `SEMANTIC_IDENTITY` | Yes | Existing KU identity remains separate from vNext operational objects. |
| expression text, language, rendering/model version | `DERIVED_VIEW` or `PROVENANCE_EVIDENCE` | No for Kernel CID | Multiple expressions may coexist. |
| source artifact refs and source-span mapping | `PROVENANCE_EVIDENCE` | Envelope/Event only | Used for encoding fidelity, not truth voting. |
| current rank, trust, popularity, PoMV score | `DERIVED_VIEW` | No | MUST carry policy/frontier where persisted. |
| peer/provider/endpoints/storage count | `AVAILABILITY` | No | Never part of KU semantic identity. |
| OBT balance/reward state | Outside knowledge-plane ownership | No | Asynchronous consumer only. |

An artifact may be rejected before persistence for invalid canonical bytes, wrong CID, invalid signature, unsupported critical schema or resource-limit violation. This is artifact validity, not a declaration that its proposition is “wrong.”

## 5. Receptor, Assembly and Resolution

### 5.1 ReceptorDefinition

| Field group | Domain | Rule |
|---|---|---|
| role, expected input/output relation shape | `SEMANTIC_IDENTITY` | Reusable across assemblies and tasks. |
| typed constraints, qualifier pattern, acceptance-profile reference | `SEMANTIC_IDENTITY` | Policy reference is semantic only when it names the acceptance contract, not its current result. |
| current budget, deadline, rank, matched candidates | Forbidden | Belongs to QueryRun/derived view. |
| current OPEN/SATISFIED state | Forbidden | Belongs to Placement-scoped ResolutionView. |

### 5.2 ReceptorClaimEnvelope

| Field group | Domain | Rule |
|---|---|---|
| definition ref, origin kind, blocker context | `PROVENANCE_EVIDENCE` | The envelope explains why/how the receptor arose. |
| raw private goal/context | `LOCAL_PRIVATE_STATE` | Vault only by default. |
| randomized binding-hiding commitment | `PROVENANCE_EVIDENCE` under explicit disclosure policy | A deterministic hash of a private goal is forbidden. Opening/nonce remains in Vault. |
| current route/provider state | Forbidden | Not part of a claim. |

### 5.3 FrontierAssemblyManifest and ReceptorPlacement

| Field group | Domain | Rule |
|---|---|---|
| assembly lineage/revision and member refs | `SEMANTIC_IDENTITY` | Revision is immutable; new revision creates a new object/event. |
| stable `placement_id`, receptor definition/claim refs, cardinality | `SEMANTIC_IDENTITY` | Resolution is scoped to `(lineage, revision, placement)`. |
| resolution-policy ref | `SEMANTIC_IDENTITY`/`AUTHORITY` boundary | The manifest names the policy; signed events prove authorized actions under it. |
| current ResolutionView | Forbidden | Derived from resolution events. |

### 5.4 ReceptorResolutionEvent and ResolutionView

| Owner | Fields/state | Rule |
|---|---|---|
| `ReceptorResolutionEvent` | event kind, exact placement target, MappingCID when applicable, authority ref, causal parents, signer | Event kinds are `ADOPT_BINDING`, `REVOKE_ADOPTION`, `WAIVE`, `REOPEN`, `DEFER`. |
| `ResolutionView` | `OPEN`, `PARTIALLY_SATISFIED`, `SATISFIED_RELATIVE`, `WAIVED`, `DEFERRED`, `CONCURRENT` | MUST include assembly revision, policy, assessed frontier and reducer version. |

`ADOPT_BINDING` does not imply satisfaction. The reducer evaluates the accepted Mapping against the placement acceptance profile and observed evidence/frontier.

## 6. KnowledgeAffordance, BindingProposal and Mapping

### 6.1 KnowledgeAffordance

Semantic fields include source refs, offered roles, accepted inputs, preconditions, outputs/effects/properties, invariants, operating conditions, limits and abstraction patterns. Provenance fields include derivation trace and extractor/operator version.

- An explicit public Affordance is an immutable object with its own CID.
- A derived Affordance is a rebuildable view with source roots and operator version.
- Neither form may invent a capability unsupported by its source KU/assembly/capability.
- Provider availability belongs to Offer/Lease, not the Affordance.

### 6.2 Mapping split

| Owner | Domain | Owned fields | MUST NOT own |
|---|---|---|---|
| `MappingKernel` | `SEMANTIC_IDENTITY` | receptor ref, candidate refs, correspondences, transforms, applicability, constraint states, assumptions, unmapped regions, derived questions | generator identity, current rank, adoption state, endpoint, permit |
| `MappingEnvelope` | `PROVENANCE_EVIDENCE` | kernel ref, proposal/source ref, generator/index/model/rule commitments, explanation/evidence refs, signer | assembly resolution or availability |
| `BindingProposal` | `RUNTIME_CONTROL`/Quarantine | proposed kernel/envelope, score vector, validation state, expiry, privacy class | canonical truth, active OBKG edge, adoption authority |
| `MaterializeMappingCommand` | `RUNTIME_CONTROL` | mapping refs, durability intent, destination storage/disclosure, authority ref, idempotency key | assembly state transition |
| `ReceptorResolutionEvent(ADOPT_BINDING)` | `AUTHORITY` + event provenance | exact assembly/placement target and MappingCID | Mapping semantic fields |
| Mapping views | `DERIVED_VIEW` | candidate/adopted projections, policy/frontier/reducer versions | source-of-record status |

## 7. KQL and distributed discovery

| Owner | Domain | Owned fields | Boundary |
|---|---|---|---|
| `KnowledgeNeedIR` | `LOCAL_PRIVATE_STATE` by default | typed need, receptors, constraints, local context | Never an OBP payload. |
| `QueryDefinition` | `SEMANTIC_IDENTITY` when persisted | normalized need reference, result/disclosure policy | Does not include a current route or budget consumption. |
| `QueryRun` | `RUNTIME_CONTROL` | run ID, boundary, budgets, causal start, continuation | Scoped execution, never global search identity. |
| `RouteNeedSketch` | disclosure-derived operational message | coarse token, one-time reply key, expiry, padding | At most three unlinkable packets; no stable Receptor/Assembly/Need/User/Node ID. |
| `ResultBatch`/`WorkReceipt` | `PROVENANCE_EVIDENCE` | source frontier, result refs, work status, continuation | A response cannot establish global absence. |
| `CoverageStatement`/`QueryView` | `DERIVED_VIEW` | searched boundary/frontiers/channels, unobserved regions, limitations | `exact` is always relative to a named closed boundary. |
| `StandingNeed` | `LOCAL_PRIVATE_STATE` | durable local watch, mailbox/dedup state | Outbound publication requires a separate disclosure object. |

## 8. Capability and local/remote AI

| Owner | Domain | Owned fields | MUST NOT imply |
|---|---|---|---|
| `CapabilityDefinition` | `SEMANTIC_IDENTITY` | semantic function, typed IO, behavior/effect ceiling, conformance contract | implementation availability or permission |
| `ImplementationManifest` | immutable operational artifact/provenance | model/tool/build/ABI/runtime commitments and conformance results | current provider availability or authority |
| `CapabilityOffer` | `AVAILABILITY` | scoped provider principal/feed, coarse resources, capacity/latency bucket, route handles, generation/expiry | authority, correctness or attester independence |
| `DelegationPermit` | `AUTHORITY` | principal, purpose, effect subset, budget, retention, deadline, onward-delegation caveat | semantic identity or availability |
| `CognitiveTask` | `RUNTIME_CONTROL` | input refs/capsules, expected output, deadline/cancel/budget | permission beyond its Permit |
| `ExecutionRecord` | `PROVENANCE_EVIDENCE` | task/implementation refs, input/output commitments, limitations, logs/receipts | correctness, fidelity or automatic materialization |

Remote output enters Quarantine. Local validation success alone does not publish, adopt, update a profile/graph or execute a tool side effect.

## 9. Encoding fidelity

| Owner | Domain | Rule |
|---|---|---|
| `EncodingAttempt` | `PROVENANCE_EVIDENCE` | Identifies source, candidate encoding, blinded output commitment, pipeline/model/tool commitments. |
| `CorrelationEvidence` | `PROVENANCE_EVIDENCE` | Per-dimension evidence strength; no scalar/boolean `independent`. |
| `EncodingFidelityAttestation` | signed event/evidence | States source↔encoding fidelity checks and limitations; not proposition truth. |
| `FidelityPolicy` | policy identity | Names required attempts, evidenced-distinct dimensions and hard mismatch rules. |
| `FidelityAssessment` | `DERIVED_VIEW` | Includes policy, assessed frontier, accepted attestation-set root and limitations. |
| `LegacyEncodingClaim` | `LEGACY_EVIDENCE` | May be consumed conservatively; never auto-upgraded to corroborated fidelity. |

NodeID, IP address, route count, repeated delivery or a self-claimed model family never owns attester independence.

## 10. Identity, feed and reachability

| Owner | Domain | Rule |
|---|---|---|
| `ActorID` | principal identity | Union root for delegated device/feed views; not a sequence clock. |
| `DeviceID` | device identity | Does not replace scoped FeedID or transport NodeID. |
| `FeedID`/`FeedInception` | feed semantic identity | Scoped by public key, namespace commitment and generation; no actor-wide sequence. |
| signed feed event/head/proof | `PROVENANCE_EVIDENCE` | Carries event position, parent/successor evidence and signer. |
| key/delegation state | `AUTHORITY` | Evaluated relative to an observed feed frontier. Missing proof yields STALE/UNRESOLVED. |
| transport/session principal | transport authentication | Bound to transcript/profile; feed proof disclosed only when required. |
| `ReachabilityView` | `DERIVED_VIEW`/local observation | Peer digest, selector frontier, carrier paths, interval, budgets and limitations. No IslandID/leader/epoch. |

Transport identity, feed identity and author authority are separate. A bridge authenticated for transport gains no authority over the content it carries.

## 11. Checkpoint, retention and GC

| Field group | Domain | Rule |
|---|---|---|
| feed/position, covered root, state CID, reducer version | checkpoint semantic/provenance | Identifies the claimed reduced prefix and reducer. |
| `last_event_cid`, `previous_checkpoint_cid` | consistency evidence | Enables prefix/successor verification. |
| `retirement_floor_root` | exact suppression evidence | Must preserve high-water semantics; never probabilistic. |
| `key_state_root` | authority evidence | Required when checkpoint authority depends on delegated/rotated keys. |
| `archive_manifest_ref` | custody/provenance | Does not imply current provider availability. |
| local retention class, dry-run decision | `RUNTIME_CONTROL`/local policy | Not replicated as global deletion. |
| checkpoint/restore view | `DERIVED_VIEW` | Carries proof status and unresolved forks. |

An old event may be suppressed only with inclusion, consistency and effect proof appropriate to the reducer. Missing proof is unresolved, not covered.

## 12. Provider discovery

| Owner | Domain | Rule |
|---|---|---|
| `ProviderTuple(index_key, provider_principal, offer_kind)` | operational key identity | Separates providers and offer kinds; not a knowledge identity. |
| `ProviderLease` | signed `AVAILABILITY` record | Owns generation, selector/content root, capabilities, advisory times, duration and key-state ref. |
| `ProviderRetire` | signed suppression event | Owns exact `retire_through_generation`. |
| `LeaseObservation.first_seen_monotonic(record_cid)` | `LOCAL_PRIVATE_STATE` | Local-only; MUST NOT be signed, copied or refreshed by replay. |
| `ProviderLeaseMap` | `DERIVED_VIEW` | Max-generation + exact retirement floor over observed records. |
| DHT response | sampled routing view | Returns continuation when a real page exists or `coverage=sampled` after local sampling/eviction. |

Provider records establish routing hints only. They do not establish content correctness, completeness, custody or authority.

## 13. PoMV, Use and Benefit

| Owner | Domain | Rule |
|---|---|---|
| `UseEvent`/`DerivationEvent` | signed behavioral/causal evidence | Records an exercise/use path; does not by itself prove benefit. |
| `OutcomeObservation`/`BenefitEvidence` | signed evidence/claim | Owns observed outcome, affected context, attribution refs and limitations. Conflicts coexist. |
| `MetabolicEvidenceView` | `DERIVED_VIEW` | Separates cumulative use evidence, recent activity and exposure telemetry. |
| `ExposureTelemetry` | `LOCAL_PRIVATE_STATE` by default | Retrieval/query hit is not Use. |
| OBT reward/mint state | outside knowledge-plane | Asynchronous consumer; never a prerequisite for preserving, finding or using KU. |

## 14. OBS and OBKG storage ownership

| Storage class | May contain | Must exclude |
|---|---|---|
| Public Object/Event Store | validated public canonical objects/events and opaque unknown objects within quota | private NeedIR, raw private goals, unverified executable payload |
| Private Vault | private claims, NeedIR, openings/nonces, observation context, local policy state | plaintext export without explicit transform/consent |
| Quarantine | untrusted/remote proposals, unknown critical objects, failed validation evidence | active graph/profile/tool side effects |
| Derived Index/View Store | OBKG, Receptor/Affordance/Mapping views, coverage/fidelity/PoMV views | source-of-record authority or irreplaceable original bytes |
| Legacy Store | original wire bytes and migration journal | invented vNext identity or silently rewritten provenance |

## 15. CID graph rules

### 15.1 Allowed direction

```text
Semantic Kernel/Object
        ↓ referenced by
Provenance Envelope
        ↓ referenced by
Signed Event / Permit / Operational Record
        ↓ reduced into
Versioned Derived View
```

An arrow means “may reference,” not “grants authority.” A lower layer MUST NOT be referenced back into the identity bytes of an upper layer.

### 15.2 Forbidden cycles

1. An object MUST NOT contain its own CID or a CID computed from bytes containing that CID.
2. `MappingKernel` MUST NOT reference `MappingEnvelope`; the Envelope references the Kernel.
3. `ReceptorDefinition` MUST NOT reference Placement/ResolutionView; Placement references the Definition.
4. A canonical object/event MUST NOT include a derived view CID as semantic identity input.
5. A checkpoint state CID MUST NOT include the checkpoint CID that names it.
6. Provider/Offer/Reachability records MUST NOT be referenced into the semantic CID of the knowledge/capability they advertise.
7. Legacy migration objects MUST preserve `original_wire_ref`; they MUST NOT rewrite original bytes to force a vNext CID.

## 16. Field placement decision procedure

For every new field, answer in order:

1. Would the meaning remain the same if every provider and route disappeared? If no, it is not semantic identity.
2. Does the field permit disclosure/adoption/delegation/side effects? If yes, it belongs to Authority.
3. Does it change with current load, time, endpoint or reachability? If yes, it belongs to Availability/local observation.
4. Does it describe one task/session/budget/deadline? If yes, it belongs to Runtime Control.
5. Can it be recomputed from immutable inputs? If yes, it belongs to a versioned Derived View.
6. Can it reveal a person's intent/context? If yes, it defaults to Local Private State.
7. Is it only present in legacy bytes? If yes, preserve it as Legacy Evidence and migrate explicitly.

If two answers apply, split the type. Do not add a multi-role field.

## 17. Acceptance checklist

- [x] KU, Receptor, Mapping, Query, Capability, Fidelity, Feed, Checkpoint, Provider, PoMV, OBS and OBKG families are covered.
- [x] Semantic identity, authority and availability have different owners in every family.
- [x] Runtime commands/proposals cannot directly create authoritative derived state.
- [x] Private Need/goal/local timing state has an explicit non-public owner.
- [x] Derived views carry policy/frontier/operator or reducer identity.
- [x] CID direction and cycle prohibitions are explicit.
- [x] OBT, route, seed, bridge and provider state cannot enter knowledge authority.

