# Pillar 7 Specification — Unified OBKG Multi-Plane Projection and Firewalled Discovery Profile v1

> - **Profile ID:** `P7-OBKG-UNIFIED-001`
> - **Pillar:** `P7` (OneBrain Knowledge Graph)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [OBKG_DERIVED_PROJECTION_PROFILE_V1](OBKG_DERIVED_PROJECTION_PROFILE_V1.md), [RECEPTOR_RESOLUTION_PROFILE_V1](RECEPTOR_RESOLUTION_PROFILE_V1.md), [MAPPING_MATERIALIZATION_PROFILE_V1](MAPPING_MATERIALIZATION_PROFILE_V1.md), [MAPPING_PROFILE_V1](MAPPING_PROFILE_V1.md), [ASSEMBLY_MANIFEST_PROFILE_V1](ASSEMBLY_MANIFEST_PROFILE_V1.md), [KQL_SEMANTIC_INDEX_PROFILE_V1](KQL_SEMANTIC_INDEX_PROFILE_V1.md), [REUNION_DELTA_JOIN_PROFILE_V1](REUNION_DELTA_JOIN_PROFILE_V1.md), [KQL_STRUCTURAL_SIGNATURE_PROFILE_V1](KQL_STRUCTURAL_SIGNATURE_PROFILE_V1.md), [KQL_RELATIONAL_ALIGNMENT_PROFILE_V1](KQL_RELATIONAL_ALIGNMENT_PROFILE_V1.md), [KQL_ASSEMBLY_SEARCH_PROFILE_V1](KQL_ASSEMBLY_SEARCH_PROFILE_V1.md), [KQL_EXPLORATION_POLICY_V1](KQL_EXPLORATION_POLICY_V1.md), [KQL_QUERY_VIEW_LEARNING_PROFILE_V1](KQL_QUERY_VIEW_LEARNING_PROFILE_V1.md), [AI_MODEL_RECALL_FIREWALL_V1](AI_MODEL_RECALL_FIREWALL_V1.md), [METABOLIC_EVIDENCE_VIEW_V1](METABOLIC_EVIDENCE_VIEW_V1.md), [OUTCOME_BENEFIT_EVIDENCE_PROFILE_V1](OUTCOME_BENEFIT_EVIDENCE_PROFILE_V1.md), [KNOWLEDGE_REWARD_FIREWALL_V1](KNOWLEDGE_REWARD_FIREWALL_V1.md)
> - **Owner decisions addressed:** `D-051`

## 1. Purpose and Scope

`OBKG-001` (`OBKG_DERIVED_PROJECTION_PROFILE_V1.md`) established the core disposability rule for derived graph views. Three major specification gaps remained between legacy `ku-obkg` (`OBKG_WALKTHROUGH.md`) and vNext:

1. **Unified Multi-Plane Graph Schema (`OBKG-002`):** Joining `SemanticFrameSet` statement/concept subgraphs, `AssemblyManifest` hypernodes, `Receptor`/`Affordance` ports, 3-lane `Mapping` lifecycles, and `P4` metabolic/outcome/fidelity overlays while retiring legacy scalar `EpistemicStatus` and single-winner `EdgeState`.
2. **Cross-Domain Structural & Analogical Discovery Pipeline (`P7 ↔ P3`):** Specifying the exact execution order connecting `KQL-002`, `KQL-006`, `KQL-007`, `KQL-008`, `KQL-009`, and `KQL-010/011` over `OBKG-002`.
3. **Bio-Inspired Heuristics & KGE Firewall ("Dream vNext", Salience, `FedR`):** Reconciling legacy spreading activation, decay, STDP, Dream consolidation, KGE, and `FedR` with symbolic validity (`FW-03`), immutable storage (`NEG-KU-002`), and the retirement of `obkg_rewards.rs` (`FW-07`).

---

## 2. Multi-Plane Disposable `ObkgProjectionSnapshot` (`OBKG-002`)

### 2.1 Pure Functional Rebuildability (`FW-09`)

1. `P7` **MUST** hold zero independent canonical truth. Every `ObkgProjectionSnapshot` is a deterministic, disposable pure projection:
   $$\text{ObkgProjectionSnapshot} = \operatorname{Project}\bigl(\text{ValidatedStore}, \text{AuthorizedVaultOverlay}, \text{ObkgSourceFrontier}, \text{ResolutionPolicyRef}, \text{FidelityPolicyRef}, \text{RegistryReleaseRoot}\bigr)$$
2. Deleting all `P7` on-disk caches **MUST** lose zero canonical objects, zero private drafts, zero `StandingNeed`s, and zero authority/evidence events, rebuilding to the exact same `SnapshotCommitmentCid` from `P8` inputs.

### 2.2 The Five Coordinated Graph Planes

`OBKG-002` projects five strongly typed, non-conflated planes sharing immutable `ObjectCid` / `ConceptCcid` anchors:

| Plane | Projected Entities & Hyperedges | Canonical Invariant |
|---|---|---|
| **1. Semantic Statement Plane** | `StatementFrame` hyperedges, `ConceptCcid` nodes (with `P1` `ConceptRedirect` equivalence classes), rational SI dimension/unit channels | Never rewrites stored `P1` bytes; marks `[retired_concept]` and redirect provenance relative to `RegistryReleaseRoot`. |
| **2. Gap & Affordance Plane** | `FrontierAssemblyManifest` `(lineage_id, revision, placement_id)` hypernodes, `ReceptorDefinition` ports, `KnowledgeAffordance` role offers | Preserves exact placement cardinality and 3-state (`Satisfied` / `Violated` / `Unknown`) constraint annotations. |
| **3. Three-Lane Mapping & Resolution Plane** | Lane A: `QuarantinedProposalOverlay` (`LOCAL_ONLY`, `KQL-013`); Lane B: `MaterializedInactiveEdge` (`KU-006`); Lane C: `AdoptedActiveRelativeEdge` (`KU-007`) | `is_active_edge() == true` **iff** the `MappingKernel` is materialized (`KU-006`) **and** adopted by an authorized `ReceptorResolutionEvent` at the snapshot frontier (`NEG-MAP-001..004`). Concurrent competing bindings coexist (`CONCURRENT`) without legacy single-winner `EdgeState`. |
| **4. Evidence & Fidelity Overlay Plane** | `MetabolicEvidenceView` (`Use`/`Derivation` counts & lineages), `OutcomeBranchView` (`OutcomeCaseId` branches), `FidelityAssessment` badges | Overlays **MUST** preserve zero-use, disputed, and refuted subgraphs (`NEG-KU-002`, `NEG-KU-004`, `NEG-POMV-003`), replacing legacy scalar `EpistemicStatus` with multi-dimensional provenance tuples. |
| **5. Frontier-Anchored Historical Plane** | `AT FRONTIER <ObkgSourceFrontier \| FeedCheckpointCid>` and `DIFF FRONTIERS (F_a, F_b)` | Historical graph inspection **MUST** bind to cryptographic feed/checkpoint frontiers (`CHK-001`), **never** unverified wall-clock arrival timestamps (`NEG-P7-001`). |

---

## 3. Cross-Domain Discovery & Assembly Orchestration (`P7 ↔ P3`)

When `P3` executes open-world complement discovery or reunion delta evaluation over `OBKG-002`, candidate generation **MUST** traverse five deterministic bounded stages:

1. **Stage D1 — Exact & Reunion Delta Posting Filter (`KQL-002` + `KQL-006`):** Intersects exact `ConceptCcid`, relation-role, and rational SI unit postings; on peer sync deltas, `ReunionFrontier` computes incremental `ΔAffordance ⋈ Receptor` and `ΔReceptor ⋈ Affordance` joins without full-store rescans.
2. **Stage D2 — Vocabulary-Neutral Structural Signatures (`KQL-007`):** Recalls cross-domain candidates whose surface `ConceptCcid`s differ completely (e.g., biology vs. materials physics) via Functional Basis Signatures (FBS), operator-AST skeletons, graph-shingle hashes, and dimensionless/SI scaling invariants.
3. **Stage D3 — Bounded SME Relational Alignment (`KQL-008`):** Performs Structure-Mapping Engine (SME) partial/many-to-many graph isomorphism between `NeedIR` / `Receptor` graphs and candidate `SemanticFrameSet`s, emitting explicit role correspondences, systematicity depth, unmapped residual predicates, and required domain-transfer assumptions.
4. **Stage D4 — Multi-KU Assembly Search (`KQL-009`):** Runs beam-scheduled weighted 3-state CSP search across Assembly sizes `2..4`, isolating hard constraint violations (`Violated`) from open gaps (`Unknown`) and returning a non-scalar Pareto frontier.
5. **Stage D5 — Exploration Portfolio & Propensity Accounting (`KQL-010` + `KQL-011`):** Allocates presentation slots across the frozen `10 / 20 / 30 / 40` cohort profile with persistent `starvation_debt` so newly authored, zero-use, and cross-domain long-tail candidates cannot be starved by popular hubs, logging local inverse-propensity `ExposureTelemetry` (`KQL-011`).

Every candidate surviving Stages D1–D5 **MUST** terminate as a non-executable `BindingProposal` in `ProposalQuarantine` (`FW-02`).

---

## 4. Firewalled Bio-Inspired Heuristics, "Dream vNext", and Grounded KGE

Legacy `ku-obkg` intertwined biological heuristics directly with graph state and token rewards. vNext preserves their discovery power while enforcing strict isolation:

### 4.1 Local Salience Cache (Decay, Spreading Activation, STDP)

1. Exponential salience decay, spreading activation, and spike-timing-dependent co-exercise weights **MUST** reside exclusively in a disposable `LOCAL_ONLY` `LocalSalienceCache` (`P8`, `purpose = 4`).
2. `LocalSalienceCache` values **MAY** break ties within the `40%` high-utility cohort of `KQL-010`, but **MUST NOT**:
   - evict, hide, or tomb-stone any canonical object in `ValidatedStore` (`NEG-KU-002`),
   - override the `30%` long-tail/novelty exploration floor in `KQL-010`, or
   - be serialized into canonical `P1` objects, `P2` gossip, or `P4` public views.

### 4.2 Offline Consolidation ("Dream vNext")

1. When enabled and device resource budgets permit (`DR-P2.2`), an idle background worker (`DreamConsolidatorV1` inside `CAP-005`) scans `OBKG-002` for transitive structural closures, opposing-claim clusters, and unsatisfied `StandingNeed` multi-hop bridges.
2. Every `DreamConsolidatorV1` candidate **MUST** pass model-free `SymbolicMappingValidator` (`AI-002` / `FW-03`) and **MUST** be written solely into expiring `ProposalQuarantine` (`KQL-013`) with `generator = DreamConsolidatorV1`.
3. `DreamConsolidatorV1` **MUST NOT** auto-execute `MaterializeMappingCommand` (`KU-006`) or `ReceptorResolutionEvent` (`KU-007`) (`NEG-MAP-004`).

### 4.3 CCID-Grounded Embeddings (KGE) & Lineage-Bound `FedR`

1. **Deterministic Concept Grounding:** Legacy `Cid::to_bytes()` hash-seeded embeddings are retired (`RandomHashSeedForbidden`). KGE node initializations **MUST** bind deterministically to `(RegistryReleaseRoot, ConceptCcid, StructuralSignature)` so identical concepts share consistent initialization across nodes without leaking `LOCAL_ONLY` structure.
2. **Recall-Only Firewall (`AI-002`):** KGE link scores (`TransE`/`RotatE`/`ComplEx`) act strictly as candidate recall priors in Stage D2 and **MUST NOT** enter symbolic compatibility or unit checking (`FW-03`).
3. **Optional Lineage-Bound `FedR`:** Federated relation-matrix exchange (`obp/obkg-fedr/1`, default-off) **MAY** share sanitized relation-operator matrices only when tagged with exact `(RegistryReleaseRoot, EmbeddingSpaceSpecCid)` and differential/norm clipping bounds (`SEC-001`), landing inbound matrices in `QuarantineRecord` until local validation passes.
4. **Retirement of `obkg_rewards.rs` (`FW-07`):** Graph topology metrics (`GraphContributionScore`, PageRank, degree centrality) **MUST NOT** emit OBT rewards or `RewardEvidenceNotice`s.

---

## 5. Cross-Pillar Boundaries & Negative Assertions

- **`P7 ← P1 / P4 / P8`:** `P7` is strictly downstream of `P1`, `P4`, and `P8`; no `P7` ID or score may appear inside a `P1` or `P4` canonical envelope (`FND-001`).
- **`P7 ↔ P3`:** `P7` supplies multi-plane views and structural signatures; `P3` enforces `KQL-010` exploration floors and `ProposalQuarantine` output boundaries.
- **`P7 ↛ P5`:** Direct `P7 → P5` reward edges (`obkg_rewards.rs`) are permanently prohibited (`FW-07`).
