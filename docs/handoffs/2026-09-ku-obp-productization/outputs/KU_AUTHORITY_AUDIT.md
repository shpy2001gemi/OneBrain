# KU authority audit — KU-REV-001

> Date: 2026-09-05
> Scope: canonical documentation audit; no application/source changes or runtime qualification.
> Starting main: `3704da6b68237f50998f73c02bc5a2c59d27def8`
> Branch: `codex/ku-rev-001-canonical-audit`
> Owner clarification: [D-011 through D-014](../DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).

## 1. Findings first

1. Core DNA content addressing survives vNext, but the mutable legacy
   `KuRuntime` aggregate is not the canonical owner of every KU concern.
   Semantic identity, provenance, authority, availability, runtime and views
   have distinct owners. [FOM §§2–6; SEM §1]
2. Current canonicalization guarantees identical bytes for identical logical
   values under a named profile. It does not guarantee that AI and rules extract
   the same meaning from arbitrary text, or that all equivalent meanings have
   one existing ObjectCID. The owner selected the narrower normalized-semantic
   identity requirement. Its exact product identity/equivalence boundary needs
   contract work. [CAN §§2–4; SEM §§2–4; OBJ §2; D-011]
3. The existing fidelity lane is blind, evidence-based and frontier-relative.
   It preserves alternate encodings and does not elect a global winning KU.
   Legacy `FULL`/verifier-count consensus must not be revived. [VOC §§3.4, 6;
   BLIND §§2–6; ASSESS §§1–5; BASELINE §§52.1, 56.1.7]
4. The owner explicitly requests direct OBT issuance for accepted encode/verify
   work. This is an intentional change from the current benefit-only direction,
   not documentation drift and not permission to reuse legacy reward formulas.
   The direction is settled; versioned economic contracts and settlement gates
   remain outstanding. [D-014; BASELINE §§1.5, 3.1, 3.5; FID §6]
5. Registry release verification/activation, capability contracts and a local
   workflow foundation exist as documented components. They do not establish
   a deployed peer Registry updater or an autonomous rewarded encoding market.
   The existing six-stage workflow endpoint is read-only. [REG §§1–8; CAP
   §§1–6; RUN2 §§1–4; STATUS overview and approved direction]

## 2. Authority and source table

The ordering below follows [INDEX, Normative precedence](../../../specs/vnext/README.md#normative-precedence).
The latest owner choices are recorded separately as requested amendments so
future work neither ignores them nor claims old contracts already implement
them. This audit allocates no protocol field, opcode, domain or reward amount.

| Ref | Document and relevant sections | Authority/use in this audit |
|---|---|---|
| BASELINE | [Research Baseline v7.1](../../../research/ONEBRAIN_RESEARCH_BASELINE_V7_1.md), §§1.5, 3.1–3.5, 52.1, 56.1.7 | Founder direction first; architecture decisions precede vNext and legacy. Full relevant sections read for fidelity/reward conflicts. |
| INDEX | [vNext index](../../../specs/vnext/README.md), precedence, change control, KU/RUN task rows | Contract ownership and precedence; task completion claims are bounded to their named evidence. |
| FOM | [Field Ownership Matrix](../../../specs/vnext/FIELD_OWNERSHIP_MATRIX_V1.md), §§2–7, 9, 13–16 | Normative ownership, storage and identity direction. |
| VOC | [Normative Vocabulary](../../../specs/vnext/NORMATIVE_VOCABULARY_V1.md), §§2–7 | Required qualifiers, legacy aliases and negative assertions. |
| OBJ | [Identity and Knowledge Object](../../../specs/vnext/IDENTITY_OBJECT_PROFILE_V1.md), §§1–3 | Typed principals; exact immutable object bytes and append-only registry. |
| CAN | [Canonical Profile](../../../specs/vnext/CANONICAL_PROFILE_V1.md), §§1–8, 11 | Deterministic encoding, digest domains and preservation of legacy bytes. |
| SEM | [Semantic Primitives](../../../specs/vnext/SEMANTIC_PRIMITIVES_V1.md), §§1–7 | CCID-based IR, qualifiers, alpha normalization and exact quantities. |
| RUN1 | [Local Vertical Slice](../../../specs/vnext/LOCAL_VERTICAL_SLICE_PROFILE_V1.md), §§1–4 | Offline orchestration and separately authorized durable boundaries. |
| RUN2 | [Additive KU Workflow Surface](../../../specs/vnext/ADDITIVE_KU_WORKFLOW_SURFACE_V1.md), §§1–4 | Shared six-stage description; read-only, not operational CRUD. |
| BOUNDARY | [Legacy/vNext Product ADR](../../../specs/vnext/LEGACY_VNEXT_PRODUCT_BOUNDARY_ADR_V1.md), decision, matrix and product rules | Additive/default-off product behavior; legacy PoMV/wallet labels. |
| BASE | [Base v1 Authority and Recovery](../../../specs/vnext/BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE.md), §5 | Exact signed active Registry release required for Registry-dependent encoding. |
| REG | [Registry Operations](../../../specs/vnext/CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md), §§1–5, 8 | Signed immutable releases, activation, CCID stability and distribution boundary. |
| FID | [Encoding Fidelity Evidence](../../../specs/vnext/ENCODING_FIDELITY_EVIDENCE_PROFILE_V1.md), §§1–6 | Attempts, diversity evidence, signed attestations and current no-reward boundary. |
| BLIND | [Blind Fidelity Workflow](../../../specs/vnext/BLIND_ENCODING_FIDELITY_WORKFLOW_V1.md), §§1–6 | Commit before reveal, exact checks and alternate archive. |
| ASSESS | [Fidelity Assessment](../../../specs/vnext/FIDELITY_ASSESSMENT_REDUCER_V1.md), §§1–5 | Deterministic policy/frontier view; no global winner or mint effect. |
| CAP | [Capability Layer](../../../specs/vnext/CAPABILITY_LAYER_PROFILE_V1.md), §§1–7 | Definition/Manifest/Offer/Permit/Execution separation for delegated work. |
| ENC | [Local Receptor Encoder](../../../specs/vnext/LOCAL_RECEPTOR_ENCODER_PROFILE_V1.md), §§1–5 | Model-independent resolved-input boundary; no invented CCID; limited to Receptors. |
| FIREWALL | [Knowledge/Reward Firewall](../../../specs/vnext/KNOWLEDGE_REWARD_FIREWALL_V1.md), §§1–6 | Reward consumer cannot determine KU validity/availability of local operations; current exporter has no encode/verify reward kind. |
| STATUS | [Project status](../../../PROJECT_STATUS.vi.md), overview, approved direction and remaining priorities | Repository/product evidence summary; not protocol authority or proof of every production lane. |
| D | [Owner decisions](../DECISIONS.md), D-001–D-014 | Workstream scope and latest owner-approved direction, including explicit economic amendment request. |
| LEG-A | [Legacy KU Architecture](../../../specs/KU_ARCHITECTURE.md), full document | Historical design/current-code descriptions, subordinate to canonical contracts. |
| LEG-C | [Legacy Core DNA](../../../specs/KU_CORE_DNA_SPEC.md), full document | Existing byte-family documentation, not authority for new semantic/operational behavior. |
| LEG-P | [Legacy Encoding Pipeline](../../../specs/KU_ENCODING_PIPELINE.md), full document | Historical encoder behavior and claims to audit, not an approved vNext job/reward design. |

All task-required handoff files, STATUS, FOM, VOC, OBJ, RUN1, RUN2 and BOUNDARY
were read completely. INDEX was read for its specified precedence and KU/RUN
sections. Additional documents above were read only to address the implicated
encoding, identity, Registry, delegation and reward boundaries. No mobile
implementation evidence was created or changed.

## 3. Canonical KU statements

### Identity and ownership

- A KnowledgeKernel/Core DNA artifact is immutable. Existing Core DNA bytes
  and CIDs remain preserved; vNext does not reinterpret their legacy hash as
  a generic `object/1` digest. [FOM §4; CAN §§6, 11; SEM §1]
- Generic ObjectCID commits the complete accepted canonical root, including
  object kind/version, disclosure, references, payload and present extensions
  or limits. Equal payload alone does not prove equal ObjectCID. [OBJ §2]
- Semantic terms use full 16-byte CCIDs. Legacy local numeric ConceptIDs are
  artifact-scoped compression, not portable semantic identity. Alpha-renaming
  binders does not change the normalized object; statement order is retained.
  Exact unit comparison is not a claim that distinct source-unit encodings
  already have identical CIDs. [SEM §§1–4, 6]
- Generator/model/source evidence belongs to appropriate provenance records;
  endpoints, popularity, reward balances and current trust cannot become
  semantic identity. Some current IR qualifiers retain source-span references;
  D-011 must therefore define exactly which identity excludes which provenance
  rather than silently stripping fields from existing objects. [FOM §§2–6;
  SEM §2; OBJ §2; D-011]
- NodeId, ActorId, DeviceId and FeedId have separate 32-byte roles. A transport
  peer does not acquire the author's rights or prove verifier diversity by
  possessing a different NodeId. [OBJ §1; VOC NEG-NET-002; FID §§3–4]

### Lifecycle

- **encode ≠ publish**: an encoding artifact and its disclosure action have
  different owners. Default private inputs do not become public merely because
  an encoder or remote worker processed them. [FOM §§2, 7–9; VOC §5.1;
  ENC §4; CAP §§5–7]
- **proposal ≠ materialize**: a candidate is non-executable quarantine state;
  materialization requires an explicit authorized command, destination and
  idempotency boundary. [RUN1 §§1–2; FOM §6.2]
- **materialize ≠ adopt**: durable Mapping creation does not update Assembly
  resolution. Adoption requires a separately signed, authority-assessed event
  targeting the exact lineage, revision and placement. [RUN1 §§2–3]
- Adoption does not force satisfaction. Canonical views retain `OPEN`,
  `PARTIALLY_SATISFIED`, `SATISFIED_RELATIVE`, `WAIVED`, `DEFERRED` and
  `CONCURRENT`, qualified by policy and evidence frontier. Event kinds remain
  separate from those view states. [VOC §§3.1, 5.1; FOM §5.4]
- Artifact validity, source fidelity, applicability, use and benefit remain
  different questions. A correct mismatch report can be useful verification;
  a reward acceptance policy must not require agreement with the submitted
  encoding. The latter is a proposed acceptance requirement for D-014, not an
  existing mint contract. [VOC §§2, 4; FID §5; D-014]

### Storage and local usefulness

- Public validated objects/events, encrypted Private Vault, Quarantine,
  rebuildable derived views and preserved legacy storage are separate classes.
  Private NeedIR and raw private goals do not enter public inventory. Unknown
  kinds may be bounded opaque bytes, not executable semantic content. [FOM
  §14; OBJ §2; VOC NEG-KQL-001/004]
- OBKG and assessment indexes are projections, not independent knowledge
  authority. Revisions/events preserve predecessors; a local eviction does
  not mean knowledge has disappeared globally. [FOM §§3, 5, 14–15; VOC §5.4]
- The local slice documents a durable StandingNeed restart with subsequent
  in-process Mapping materialization. Its conformance in-memory backend is
  not evidence of production encrypted Mapping persistence. [RUN1 §4]
- Local KU remains useful without OBP or a reward service. A node with no
  adequate local encoder still needs a reachable authorized worker for that
  particular delegated job; offline autonomy does not invent absent compute.
  This last limitation follows from CAP's separation of Definition and Offer.
  [D-001, D-013–014; RUN1 §1; CAP §§1, 4; FIREWALL §§1, 4–5]

## 4. Legacy-to-vNext meaning map

| Legacy term/claim | Current interpretation | Product consequence |
|---|---|---|
| `KuRuntime` as universal owner | Semantic object + evidence + authority + runtime + views [FOM §§2–6] | Do not serialize the whole aggregate as a new canonical KU contract. |
| Mutable Epigenetics/trust/status | Versioned policy/frontier-derived assessments and separate evidence [FOM §§4, 9, 13] | No authoritative scalar truth ladder. |
| ConceptDict/local numeric ID | Legacy compression/lookup; CCID in portable semantic IR [SEM §§1–2] | Equal local numbers do not establish equal concepts across nodes. |
| `FULL=3` | Preserved `LegacyEncodingClaim`; outbound legacy capped at `PART=2` [VOC §6] | No new `FULL` field or global winning encoding. |
| Encoding consensus/verifier count | Signed blind evidence with evidenced correlation groups [FID §§3–5] | Identity or delivery counts do not establish independent verification. |
| Receptor closed | Placement/policy/frontier-relative resolution [VOC §3.1] | No `CLOSED`/`FINAL` canonical state. |
| Candidate retrieved/ranked | Proposal only [FOM §6.2] | No durable Mapping or active adoption side effect. |
| `GLOBAL` search/zero results | Scoped/partial coverage; absence remains unknown [VOC §§3.5, 6] | Do not claim the whole network was searched. |
| PoMV composite | Labeled `legacy_local_pomv_scalar_v1` compatibility view [BOUNDARY] | Do not rename it vNext fidelity, Benefit or authority. |
| Legacy balance | `simulated_non_economic` [BOUNDARY] | D-014 does not convert placeholders into issued OBT. |
| Encode/verify reward by text length/agreement | Historical behavior, not accepted economics [LEG-P §5.3; D-014] | New direct-work issuance needs its own admission/acceptance/settlement contract. |

## 5. Exact contradictions and dispositions

### Resolvable documentation drift: apply existing precedence

| ID | Exact legacy/current source conflict | Disposition |
|---|---|---|
| A-01 | LEG-P §5.1: `PART --> FULL : Threshold reached (≤3 verifiers, score ≥ 0.7)`; VOC §3.4: `FULL is not a canonical vNext state`; BASELINE §56.1.7 requires evidenced groups. | Legacy-only interpretation. Preserve alternatives and use current fidelity assessments. |
| A-02 | LEG-A §7 describes an epistemic ladder driven by PoMV; VOC §4 forbids an existence-level `KU_CORRECT/KU_WRONG`; FOM §4 puts current trust/rank/PoMV outside semantic identity. | Preserve compatibility fields, with adjacent legacy labels; do not infer truth or adoption. |
| A-03 | LEG-A §8 `gc`: “Xóa KU đã chết khỏi cả hai stores”; VOC NEG-KU-002 prohibits deletion/rejection merely for missing use and §5.4 limits eviction to local retention. | The biological lifecycle wording cannot authorize semantic deletion. Audit actual GC paths in KU-REV-002. |
| A-04 | LEG-P §3.3 Step 5: `Ambiguous` -> `Pick first`, `NotFound` -> hash normalized name; ENC §2: encoder never invents a CCID; SEM §§1–2 requires resolved CCIDs. | Historical fallback is not proof of canonical resolution. ENC is a Receptor contract, so its no-fabrication rule must not be overclaimed as an already implemented generic text encoder. D-011 needs generic scope. |
| A-05 | LEG-P §§1, 4 allow missing Registry -> v1 fallback; BASE §5 requires an exact signed active release for Registry-dependent encoding/ReadyOffline. REG §3 also documents an optional legacy fallback. | Optional compatibility fallback may exist, but cannot be represented as Base canonical Registry-ready encoding. Actual surface gating is for KU-REV-002. |
| A-06 | LEG-C §1 says Core DNA contains no natural-language text, while its §5 defines `TEXT_REF` and `FORMULA`; SEM §2 permits NFC Text literals. | Do not promise all knowledge is text-free. New lossless IR follows SEM; legacy operand bytes remain preserved. |
| A-07 | LEG-C §2 uses a 4-bit gene field while §4 also describes EXTENDED handling; LEG-A §6 describes 3-bit direct encoding. | Intra-legacy layout inconsistency. Do not choose/rewrite a byte format in this audit; preserve accepted bytes and map actual decoder/vectors in KU-REV-002. New byte behavior requires a versioned contract. |
| A-08 | LEG-A §11 proposes DHT/PubSub jobs and legacy message IDs `0x90..0x95`; CAP §§4–7 separates availability, signed authority and execution provenance. | Legacy ClaimToken/job gossip cannot stand in for the vNext delegated-work authority contract. No wire IDs allocated here. |

### Owner-directed changes and specification gaps

| ID | Current boundary versus requested outcome | Resolution/status |
|---|---|---|
| R-01 | CAN guarantees equal logical values -> equal bytes; SEM retains statement order/source units/source-span qualifiers; OBJ hashes the full envelope. Owner selects normalized semantics -> same CID. | Direction approved in D-011. Specify the semantic identity target and finite normalization equivalence set; no claim of cross-envelope or cross-profile identity today. |
| R-02 | REG §8: `MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP`. Owner wants releases available from publisher servers and peers, regularly updated. | Direction approved in D-012. A separately specified content-addressed chunk path is allowed by §8; peer acquisition does not require turning large Registry packages into OBP gossip. Cadence, release discovery and upgrade/reproducibility contract remain open. |
| R-03 | BLIND §1 explicitly describes a local coordinator; CAP defines bounded delegation. Owner wants autonomous eligible workers to claim encode/verify jobs. | Direction approved in D-013. Scheduling, durable job recovery, source-access consent and integration evidence remain specification/runtime gaps; not established by “Complete” on FID/CAP. |
| R-04 | BASELINE §3.1: OBT must not mint merely from encode/verify; §3.5: contribution receipts vest only after final BenefitEvent. FID §6, BLIND §6 and ASSESS §5 prohibit creating reward/OBT. Owner expressly selects direct issuance from encode/verify. | Owner direction resolved by D-014. A formal versioned amendment is required; current contracts/ledger remain unchanged by this audit. Do not ask the owner to select the same direction again or substitute a bounty. |
| R-05 | FIREWALL §2 exports only Use/Derivation/Outcome/Benefit notices; no work-acceptance mint contract exists there. | Open design item: define separate accepted-work evidence/authorization and settlement before implementation. Retain one-way isolation from KU truth, CID and local usefulness. [D-014; FOM §2] |

There is no unresolved choice here to make `FULL`, consensus, popularity, PoMV
or OBT into KU truth/authority. The owner's new reward trigger does not request
that change. Economic amounts, supply limits, finality and detailed worker
admission are genuinely open design items; this audit does not invent them.

## 6. Proposed KU-CON-001 scope and sequencing

This is a scope recommendation, not API/UI design or an amendment to the task
dependency graph. KU-REV-002 must first map concrete runtime paths. [D-008,
D-010; handoff PROGRESS; task KU-REV-001 acceptance]

1. Define exactly what the product calls a KU, semantic identity, revision,
   source, encoding attempt and assessment; distinguish Kernel identity from
   generic ObjectCID and legacy wire CID. [FOM §§4–6; OBJ §2; D-011]
2. Preserve explicit encode/publish, proposal/materialize and materialize/adopt
   actions, exact placement targets, partial/unknown results and shared
   node-owned surface semantics. [RUN1; RUN2; D-008]
3. Specify Registry/profile provenance and deterministic compiler requirements
   for both AI and rule-generated resolved drafts. State only the tested
   normalization equivalences; keep raw source bytes intact. Handle unresolved
   concepts and incompatible Registry/profile inputs explicitly. [CAN §§3–4;
   SEM §§2–4; D-011–012]
4. Define user-visible encode/verify evidence and remote-work handoff boundaries
   using CAP/FID concepts. Do not claim a remote job market or release updater
   is implemented merely by adding status fields. [CAP; FID; D-013]
5. Carry D-014 as a mandatory linked economic-specification dependency. The
   existing KU-only contract task is not enough to implement minting, a ledger,
   job market or new Registry transport. Explicitly scope those specification
   and implementation tasks before changing their public behavior. Do not
   reactivate legacy economics to fill the gap. [D-014; BOUNDARY; INDEX change
   control; task KU-REV-001 exclusions]

Recommended acceptance evidence for the later scoped work (not tests claimed
to run in this audit):

| Requirement | Evidence to demand |
|---|---|
| Same normalized semantic content | AI/rule adapters feed equivalent normalized drafts to the same compiler; byte/CID equality across processes/platforms; negative cases preserve negation, conditions, order and units. |
| Versioned identity | Registry additions preserve established CCIDs; unchanged normalized inputs retain the specified identity; incompatible profiles and provenance/envelope changes are explicitly distinguished. |
| Registry exchange | Publisher and peer copies verify to the same signed release root; truncated/tampered releases never activate; interruption and rollback preserve a valid active release. |
| No-local-AI node | An authorized reachable worker completes a job; absent worker remains pending/unavailable; rule-supported local work and existing KU use remain usable. |
| Verification | Commit-before-reveal, exact source/concept/gene checks, retained disagreements, correlation-group dedup and no automatic publish/adopt. |
| Direct work reward | Accepted encode/verify work can reach a separate issuance authorization without a later BenefitEvent; replay/concurrent claims cannot double-mint; fabricated work, agreement-only incentives and partition settlement are explicitly tested. Amounts and finality require the future contract. |
| Reward isolation | Disabled/unavailable reward processing does not block local KU preservation, search, use or authorized adoption. |

The table translates approved requirements into proposed evidence boundaries;
it allocates no new DTO, endpoint, event kind or token formula. [D-011–014]

## 7. Validation boundary

Required audit commands are `python scripts/ci/validate_vnext_contracts.py`
and `git diff --check`. Their actual results and branch checkpoint are recorded
in [PROGRESS](../PROGRESS.md). No runtime/build/test result from an older
document is presented as freshly executed here. No production capability,
wallet issuance, default rollout or canonical-spec migration is claimed.
