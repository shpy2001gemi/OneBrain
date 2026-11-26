# Pillar 1 Specification — Concept Lifecycle, Registry Distribution, and Rich Semantic Lowering Profile v1

> - **Profile ID:** `P1-KU-LIFECYCLE-001`
> - **Pillar:** `P1` (Knowledge Unit & Concept Registry)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [IDENTITY_OBJECT_PROFILE_V1](IDENTITY_OBJECT_PROFILE_V1.md), [SEMANTIC_PRIMITIVES_V1](SEMANTIC_PRIMITIVES_V1.md), [KU_PRODUCT_WORKFLOW_PROFILE_V1](KU_PRODUCT_WORKFLOW_PROFILE_V1.md), [KU_SEMANTIC_SELECTION_PROFILE_V2](KU_SEMANTIC_SELECTION_PROFILE_V2.md), [CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1](CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md), [MAPPING_PROFILE_V1](MAPPING_PROFILE_V1.md)
> - **Owner decisions addressed:** `D-011`, `D-012`, `D-015`

## 1. Purpose and Scope

Existing P1 foundation profiles freeze canonical CBOR envelopes (`OBJ-001/002`), `SemanticFrameSet` primitives (`KU-001`), local KU workflows (`KU-CON-001`), and single-host Concept Registry activation (`CR-OPS-01`). Four specification gaps remained open in P1:

1. **Upstream Ontology Lifecycle & CCID Collision Quarantine:** Handling Wikidata/NCBI/ChEBI concept merges, splits, deprecations, and theoretical 128-bit `ConceptCcid` collisions without mutating immutable stored KUs.
2. **Decentralized Novel-Concept (`Definition`) Authoring & Absorption:** Allowing partitioned or domain-specialized nodes to coin scoped concepts and later bridge them into standard Registry releases.
3. **Node-Assisted Content-Addressed Concept Registry Distribution (`D-012`):** Distributing signed multi-gigabyte `.obr` releases across publisher mirrors and opted-in peers without violating `MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP`.
4. **Rich Semantic Draft (`KU_SEMANTIC_SELECTION_PROFILE_V2`) Lowering & Replicated Lineage (`D-011`):** Compiling validated alternatives, comparisons, references, and qualifiers deterministically into canonical `SemanticFrameSet`s and representing signed cross-peer author revisions without global supersession winners.

---

## 2. Concept Lifecycle, Alias Equivalence, and CCID Collision Handling

### 2.1 Immutable `ConceptCcid` & Lifecycle Records in `.obr`

1. Once assigned in any signed Concept Registry release, a 16-byte `ConceptCcid` (`[u8; 16]`) **MUST** remain permanently bound to its canonical origin tuple `(source_namespace, source_identifier)` and **MUST NOT** be reassigned to a different meaning in later releases.
2. To represent upstream ontology evolution (merged QIDs, split taxa, obsoleted terms) without invalidating historical KUs, Concept Registry releases introduce an append-only `ConceptLifecycleTable` (`concepts.lifecycle.cbor`, covered by the signed release manifest) with three deterministic record kinds:
   - `ConceptRedirect { deprecated_ccid: ConceptCcid, canonical_ccid: ConceptCcid, since_release_seq: u64, provenance_ref: String }` (1-to-1 upstream merge/redirect; acyclic across transitive closure).
   - `ConceptSplit { parent_ccid: ConceptCcid, branch_ccids: Vec<ConceptCcid>, disambiguation_role_ccid: ConceptCcid, since_release_seq: u64 }` (`2 <= branch_ccids.len() <= 16`; historical `parent_ccid` remains a broader umbrella concept, never silently rewritten to one arbitrary child).
   - `ConceptRetire { retired_ccid: ConceptCcid, reason_code: ConceptRetireReason, since_release_seq: u64 }` (upstream retraction; historical KUs remain decodable and searchable with an explicit `[retired_concept]` annotation).
3. **Compiler vs. Index Asymmetry:**
   - New KU authoring (`KU_PRODUCT_WORKFLOW_PROFILE_V1` / `P6` compiler) **MUST** emit `canonical_ccid` when resolving a `ConceptRedirect` and **MUST** return `needs_disambiguation` when input resolves to a `ConceptSplit` parent without explicit umbrella selection.
   - Stored `SemanticFrameSet` and legacy `CoreDna` bytes **MUST NOT** be rewritten on Registry upgrade (`FND-001` §15). Instead, `RebuildableSemanticIndex` (`P3`) and `ObkgProjectionSnapshot` (`P7`) expand `ConceptRedirect` equivalence classes deterministically relative to the active `registry_release_root`, recording `lifecycle_redirect_applied` in match provenance.

### 2.2 128-bit `ConceptCcid` Collision Detection & Disambiguation

Although 128-bit BLAKE3 truncation has birthday collision probability $\approx 1.47 \times 10^{-29}$ at $10^7$ concepts, P1 defines a deterministic fail-closed collision protocol:

1. Registry release builders **MUST** reject any candidate release where two distinct `(source_namespace, source_identifier)` tuples produce the same `ConceptCcid`, quarantining the newer tuple into `collision_quarantine.json` until domain-versioned disambiguation (`onebrain:ccid:disambig:1\0`) is ratified by a new schema version.
2. Runtime nodes encountering a `ConceptLifecycleTable` cycle or duplicate `ConceptCcid` key during staging verification **MUST** abort activation and retain the previous active Registry generation (`CR-OPS-01` §4).

### 2.3 Scoped Novel-Concept Definitions & Registry Absorption

To prevent partitioned or frontier researchers from being blocked when a specialized concept is absent from the quarterly global `.obr` release:

1. A node **MAY** author a canonical `ConceptDefinitionFrame` (`SemanticFrameSet` carrying reserved Tier-0 predicate `DEFINES_SCOPED_CONCEPT`) whose provisional identifier is a namespace-bound `ScopedConceptCcid = BLAKE3_128("onebrain:vnext:scoped-ccid:1\0" || definition_semantic_content_cid)` paired with an explicit `ConceptOrigin::ScopedDefinition(ObjectCid)` in the object's `extensions` map.
2. KUs referencing a `ScopedConceptCcid` **MUST** include its defining `ObjectCid` in `KnowledgeObjectEnvelope.references` so any peer receiving the KU under `OBP-RP/1` validates the dependency before acceptance (`DeferredMissingDependency` if the definition object has not yet arrived).
3. When a subsequent standard Concept Registry release absorbs that concept as a standard `ConceptCcid`, any authorized actor **MAY** publish a canonical `MappingKernel` (`KU-005`) establishing exact `ConceptEquivalence(ScopedConceptCcid <-> StandardConceptCcid)` without mutating historical KUs.

---

## 3. Node-Assisted Content-Addressed Registry Distribution (`D-012`)

### 3.1 Separation from OBP Knowledge Gossip

Per `CR-OPS-01` §8 and `D-012`, multi-gigabyte Registry packages (`1.3–2.5 GiB`) **MUST NOT** enter `HybridInventoryForest` (`OBP-002`) or unsolicited OBP gossip. Distribution uses an opt-in, pull-only content-addressed chunk profile `obp/registry-chunks/1` over `NET-001` authenticated QUIC sessions or HTTPS static mirrors.

### 3.2 Release Manifest, Chunk Merkle Tree, and Pull Protocol

1. **Signed Release Descriptor (`RegistryReleaseDescriptorV1`):**
   - Signed under `onebrain:concept-registry:release-descriptor:1\0` by an approved Registry release signer (`CR-PROD-01`), committing:
     - `release_sequence: u64` (strictly monotonic per release track),
     - `release_root: [u8; 32]` (exact 5-artifact root from `CR-OPS-01`),
     - `chunk_size_bytes: u32` (fixed `4,194,304` = `4 MiB` except final chunk),
     - `chunk_merkle_root: [u8; 32]` over ordered `ChunkCid = BLAKE3("onebrain:registry-chunk:1\0" || u64_be(index) || chunk_bytes)`,
     - `optional_delta_from: Option<(u64, [u8; 32], ManifestCid)>` (deterministic binary patch manifest from the immediately preceding `release_sequence`),
     - `minimum_compatible_reader_version: u32`.
2. **Untrusted Carrier / Mirror Invariant (`FW-05`):**
   - Peers advertising `OfferKind::RegistryRelease(release_root)` (`DHT-001`) and HTTPS mirrors act solely as untrusted byte caches. Every fetched `4 MiB` chunk **MUST** be verified against its `ChunkCid` Merkle branch before writing to `registry/staging/<release_sequence>/`, exhausting at most `max_inflight_chunks = 4` (`16 MiB` RAM cap).
   - Corrupt chunks immediately penalize that peer's local transport score and re-request the chunk from an alternate source without discarding already-verified chunks (crash/restart resumable via `staging.bitmap`).
3. **Delta Reconstruction & Atomic Activation:**
   - If a node holds verified active generation $N-1$ and fetches a signed delta patch for generation $N$, it reconstructs the 5 target files into `registry/staging/<N>/` and **MUST** verify the exact full-file `release_root` and signer signature identically to a full download before appending the atomic activation stamp (`CR-OPS-01` §4).
   - At least one prior verified generation ($N-1$) **SHOULD** be retained locally (subject to disk budget) to guarantee instant local rollback and reproducible verification of KUs compiled against $N-1$.

---

## 4. Rich Draft Lowering (`D-011`) and Signed Revision Lineage

### 4.1 Deterministic Lowering from `KU_SEMANTIC_SELECTION_PROFILE_V2` to `SemanticFrameSet`

Extending `ARCH-07` (`review_lowering`), a reviewed `ku-semantic-selection/2.0` draft lowers deterministically to `SemanticFrameSet` **only** when every selected role, unit, and structural operator passes closed host resolution:

| `KU_SEMANTIC_SELECTION_PROFILE_V2` Construct | Canonical `SemanticFrameSet` Lowering Rule | Fail-Closed Rejection (`saveable: false`) |
|---|---|---|
| **Disjunctive `alternatives` (`OR` group)** | Lowers only when `exclusivity` is explicitly reviewed (`InclusiveOr` or `ExclusiveXor`) and every branch resolves to a single `ConceptCcid` or exact `QuantityLiteral`; emits canonical `DisjunctiveTermGroup` sorted only when `exclusivity` is commutative and spans carry no ordered discourse dependency, preserving `D-011` normalized `SemanticContentCid` convergence. | `exclusivity: unspecified`, unresolved branch concept, or unrepairable cue/branch overlap. |
| **Bound `ellipses`** | Reconstructs the elided role by binding the exact resolved `TermRef` of the referenced `borrowed` role within the same claim, preserving statement arity without duplicating text literals. | Unresolved `borrowed` span or `self_reference` (unless omitted via `D-068`). |
| **Cross-claim `references`** | Compiles acyclic intra-draft coreference into a shared alpha-normalized binder variable `VarId(k)` (`SEMANTIC_PRIMITIVES_V1` §3) across the referring and target `StatementFrame`s so alpha-equivalent phrasings yield the exact same `SemanticContentCid`. | Reference `cycle` (unless omitted via `D-075`), ambiguous target, or unresolved `via`. |
| **Explicit `comparisons`** | Compiles `property` (`ConceptCcid` dimension/predicate), `direction` (`Greater | Less | Equal | Approximate`), and explicit `target` (`TermRef` or exact rational `QuantityLiteral` converted to canonical SI base + preserved source unit metadata). | `target_kind: unspecified` (`D-067`/`D-069`) — remains a valid reviewable private draft, never lowered as a complete binary comparison KU. |
| **Reviewed `qualifiers`** | Maps `negation`, `condition`, `time`, `location`, `frequency`, `modality`, and `approximation` to typed `StatementQualifier` entries sorted deterministically by `(QualifierKind, canonical_bytes)`. | Qualifier overlapping predicate without `D-074` boundary trim or unresolved temporal/unit reference. |

### 4.2 Replicated Author Revision Lineage (`KnowledgeSupersessionEvent`)

`KU-CON-001` (`KU-PC-C`) defined private local revision journals. For public or personal-mesh (`P10`) multi-node lineage:

1. An author **MAY** publish a signed `KnowledgeSupersessionEvent` (canonical `KnowledgeEventEnvelope` signed by an authorized `FeedId` of the predecessor author's `ActorId`) binding `{ predecessor_object_cid: ObjectCid, successor_object_cid: ObjectCid, relation: RevisionRelation::{Refines, CorrectsEncoding, SplitsScope, RetractsClaim}, rationale_ref: Option<ObjectCid> }`.
2. **No Global Erasure or Single-Winner Collapse (`NEG-KU-004`):**
   - A `KnowledgeSupersessionEvent` **MUST NOT** delete `predecessor_object_cid` from `ValidatedStore` (`P8`), invalidate `Mapping`s citing `predecessor_object_cid`, or overwrite concurrent `successor_object_cid` branches.
   - Third-party critiques or counterclaims signed by a *different* `ActorId` **MUST** use `MappingKernel` (`OPPOSES` / `REFINES`) or `OutcomeObservation`, never `KnowledgeSupersessionEvent` (enforced by `FeedAuthorityDecision` against the predecessor's `ActorId` frontier).

---

## 5. Cross-Pillar Boundaries & Negative Assertions

- **`P1 ↔ P3`:** `ConceptRedirect` expansion occurs in `P3` index/matcher views parameterized by `registry_release_root`, never by mutating `P1` bytes.
- **`P1 ↔ P2 / P8`:** `obp/registry-chunks/1` stages into `P8` `registry/staging/` and verifies the full cryptographic `release_root` before atomic activation; unverified chunks are never readable by `P1`/`P3`/`P6`.
- **`P1 ↔ P6`:** Unassessed `target_unspecified` or `exclusivity_unassessed` v2 drafts (`P6`) **MUST** fail closed before `SemanticFrameSet` compilation unless explicitly resolved by the user.
