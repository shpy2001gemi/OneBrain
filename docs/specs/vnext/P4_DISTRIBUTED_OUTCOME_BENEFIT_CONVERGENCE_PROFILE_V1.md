# Pillar 4 Specification — Distributed Derivation, Outcome, and Benefit Convergence (`M6B`) Profile v1

> - **Profile ID:** `P4-POMV-CONVERGENCE-001`
> - **Pillar:** `P4` (Proof of Metabolic Value & Encoding Fidelity)
> - **Milestone covered:** Distributed Runtime `M6B` (Cross-peer `Use → Derivation → Outcome → Benefit` & Fidelity correlation integration)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [USE_DERIVATION_EVIDENCE_PROFILE_V1](USE_DERIVATION_EVIDENCE_PROFILE_V1.md), [METABOLIC_EVIDENCE_VIEW_V1](METABOLIC_EVIDENCE_VIEW_V1.md), [OUTCOME_BENEFIT_EVIDENCE_PROFILE_V1](OUTCOME_BENEFIT_EVIDENCE_PROFILE_V1.md), [DISTRIBUTED_POMV_RUNTIME_PROFILE_V1](DISTRIBUTED_POMV_RUNTIME_PROFILE_V1.md), [PUBLIC_USE_CONSENT_PROFILE_V1](PUBLIC_USE_CONSENT_PROFILE_V1.md), [ENCODING_FIDELITY_EVIDENCE_PROFILE_V1](ENCODING_FIDELITY_EVIDENCE_PROFILE_V1.md), [FIDELITY_ASSESSMENT_REDUCER_V1](FIDELITY_ASSESSMENT_REDUCER_V1.md), [KNOWLEDGE_REWARD_FIREWALL_V1](KNOWLEDGE_REWARD_FIREWALL_V1.md), [ROUTE_AUTHORITY_BOUNDARY_PROFILE_V1](ROUTE_AUTHORITY_BOUNDARY_PROFILE_V1.md)

## 1. Purpose and Scope

Foundation contracts `POMV-001..004` and `FID-001..003` define canonical local types and reducers for `UseEvidence`, `DerivationEvidence`, `OutcomeObservation`, `BenefitEvidence`, and blind encoding fidelity, while `M4` (`DISTRIBUTED_POMV_RUNTIME_PROFILE_V1`) wired distributed QUIC sync for `Public` `UseEvidence` only.

This specification completes Pillar 4's specification column by defining the **end-to-end distributed convergence, dependency deferral, per-author authority evaluation, multi-dimensional correlation assessment, and post-commit firewall handoff** (`M6B`) across `Use`, `Derivation`, `Outcome`, `Benefit`, and `Fidelity` without ever voting on proposition truth (`NEG-POMV-003`) or coupling to `P5` token state (`NEG-OBT-001`).

---

## 2. Two-Step Consent & Transactional Publishers for `Derivation`, `Outcome`, and `Benefit`

1. **Explicit Public Disclosure Consent (`DR-P1.3` parity):**
   - Any publication of `Public` `DerivationEvidencePayload` (`kind=14`, `evt=3`), `OutcomeObservationPayload` (`kind=19`, `evt=5`), or `BenefitEvidencePayload` (`kind=20`, `evt=6`) **MUST** use the same two-step `Prepare → SingleUseConsentReceipt (TTL <= 900 s) → ExactTypedConfirm` pattern as `PUBLIC_USE_CONSENT_PROFILE_V1`, signed by the external `FeedEventSigner` (`DR-P1.2`) and bound to `TX-PUSE-001/002` failpoint-safe Redb sequence heads.
   - Private observations (`LOCAL_ONLY` or `NEGOTIATED_ENCRYPTED`) stay inside `PrivateVault` (`0x000C`); when a public `OutcomeObservation` references a sensitive beneficiary, it **MUST** use `AffectedPrincipal::Commitment([u8; 32])` with a randomized salt kept in `PrivateVault` (`FND-001` §5.2).
2. **Mandatory Causal Continuity Invariants at Admission:**
   - `DerivationEvidencePayload` **MUST** bind non-empty `inputs: Vec<DerivationInput>` (`ObjectReference`s), `derived_output: ObjectReference`, `transform_rule_commitment`, and `task_context_commitment: [u8; 32]`, rejecting cycles where `derived_output` appears in `inputs`.
   - `OutcomeObservationPayload` **MUST** bind `task_context_commitment: [u8; 32]`, `outcome_class: ConceptCcid`, `affected: AffectedScope`, and mandatory `EvidenceLimitation`s (`UnwitnessedObservation` whenever `measurement_evidence` is empty; `MissingBaseline` whenever `baseline` is absent).
   - `BenefitEvidencePayload` **MUST** cite $\ge 1$ `OutcomeObservation` `ObjectReference` and $\ge 1$ `UseEvidence` or `DerivationEvidence` `EventCid`, all sharing the **exact same** `task_context_commitment: [u8; 32]`. A standalone `UseEvent` or `DerivationEvent` **MUST NOT** be accepted as `BenefitEvidence` (`NEG-POMV-002`).

---

## 3. Out-of-Order Reconciliation & Non-Terminal Dependency Deferral

Over partitioned, multi-bridge `OBP-RP/1` networks (`P2`), a `BenefitEvidence` event (`evt=6`) frequently arrives before its cited `OutcomeObservation` (`kind=19`), `UseEvidence` (`evt=2`), `DerivationEvidence` (`evt=3`), or author `FeedInception` / `AuthorityEvent`.

1. **Bounded `DeferredEvidenceStaging` Store (`0x000C`):**
   - Validated-envelope records whose causal or subject dependencies are not yet present in `ValidatedStore` **MUST** enter a bounded Redb staging table (`MAX_DEFERRED_EVIDENCE_RECORDS = 16_384`, `MAX_DEFERRED_BYTES = 64 MiB`) with status `DeferredMissingDependency { missing_refs }` — **never** rejected as permanently invalid and **never** reduced as negative/opposed evidence.
2. **Monotonic Dependency Wakeup & Exact Deduplication:**
   - Whenever `ValidatedStore` accepts a new `ObjectCid`, `EventCid`, or `AuthorityEvent`, an indexed reverse-dependency lookup `(missing_ref -> waiting_event_cid)` wakes newly satisfied records in causal topological order (`Use / Derivation → OutcomeObservation → BenefitEvidence`).
   - Replaying the same `EventCid` across 1, 2, or 5 carriers/relays **MUST** deduplicate idempotently via `ReceivedEvidenceIdentityIndex` (`[u8; 32]` full `EventCid`). If the same `(FeedId, event_type, idempotency_key)` arrives with different canonical bytes, **all** conflicting variants are marked `EquivocatedExcluded` and excluded from positive metabolic/benefit totals while preserved for audit (`M4` parity).

---

## 4. Per-Author Frontier Authority & Conflict-Preserving Convergence

1. **Independent Multi-Author Frontier Evaluation (`DR-P1.5`):**
   - In a cross-actor chain where Actor $A$ authored the KU, Actor $B$ signed `UseEvidence`, Actor $C$ signed `OutcomeObservation`, and Actor $D$ signed `BenefitEvidence`, the reducer **MUST** evaluate `FeedAuthorityDecision` (`AuthorizedRelative | QuarantinedRevokedRelative | StaleOrUnresolved`) **independently** for $B$, $C$, and $D$ at the local node's validated `authority_frontier`.
   - Revocation of $D$'s feed quarantines $D$'s `BenefitEvidence` relative to that frontier while leaving $B$'s `UseEvidence` and $C$'s `OutcomeObservation` intact. Callers **MUST NOT** inject pre-computed `Authorized` booleans.
2. **Valence-Neutral `OutcomeCaseId` & Conflict Coexistence (`NEG-KU-004`, `NEG-POMV-003`):**
   - `OutcomeCaseId = BLAKE3("onebrain:vnext:outcome-case:1\0" || task_context_commitment || outcome_class || canonical(affected) || observation_policy)`.
   - Because `valence` (`Beneficial | Harmful | Mixed | NoObservedChange | Unknown`) is excluded from `OutcomeCaseId`, independent observers reporting `Beneficial` and `Harmful` for the same case deterministically merge into one `OutcomeCaseClosureView` with `authorized_conflict = true` and `branches` sorted lexicographically by `EventCid` (zero arrival-order winner!).
   - A `BenefitClosureView` marks `closure_state`:
     - `SupportedUncontested` iff $\ge 1$ authorized `BenefitEvidence` has `assessed_valence == Beneficial && attribution == Supported` and zero authorized `Harmful` / `Opposed` / `Contested` branches exist in the same `OutcomeCaseId`;
     - `ContestedOrMixed` whenever any authorized opposing `OutcomeObservation` or `BenefitEvidence` (`Harmful`, `Opposed`, `Contested`) coexists;
     - `DeferredIncomplete` when dependencies or authority proofs are `StaleOrUnresolved`;
     - `ObservedNonBeneficial` otherwise.
   - Even under `ContestedOrMixed` or `Harmful`, the referenced KU **MUST NOT** be deleted, hidden from KQL, or labeled "false" (`NEG-KU-002`, `NEG-POMV-003`).

---

## 5. Multi-Dimensional Correlation Grouping (`FID-001` × `POMV-004`)

To prevent Sybil rings from fabricating corroborated fidelity or multi-witness benefit by spinning up multiple `NodeId`/`FeedId` keys (`NEG-FID-001`):

1. Both `FidelityAssessmentReducer` (`FID-003`) and `BenefitClosureReducer` (`M6B`) group attestations using the 4-dimensional `CorrelationEvidence` lattice:
   - `AdministrativePrincipal` (`ActorId` root / organizational attestation),
   - `PipelineModelLineage` (`ImplementationManifest` model/weights/prompt family),
   - `ExecutionEnvironment` (hardware/runtime commitment),
   - `CausalStakeholderRelation` (`SelfAuthor | DirectBeneficiary | UnrelatedWitness | Unknown`).
2. Two attestations count toward distinct corroboration groups **only** when evidenced non-overlapping commitments (`CRYPTO_BOUND` or `EXTERNALLY_ATTESTED`, never bare `SELF_CLAIMED` or distinct IP/`NodeId`) establish distinctness under the named `FidelityPolicy` or `BenefitAssessmentPolicy`. Self-authored or `UNKNOWN`-correlation attestations remain visible in `SelfAttested` / `SingleGroup` tiers with explicit `EvidenceLimitation`s.

---

## 6. Post-Commit Handoff to `P7` (OBKG) and `P5` (OBT Firewall)

1. **`P4 → P7` (Read-Only Overlay):** `MetabolicEvidenceView`, `OutcomeCaseClosureView`, `BenefitClosureView`, and `FidelityAssessment` project read-only onto `ObkgProjectionSnapshot` (`P7`), keyed by `(policy_bundle_cid, assessed_frontier_root, reducer_version)`.
2. **`P4 → P5` (`FW-07` One-Way Post-Commit Notice):**
   - Only after `ValidatedStore` and `vnext_distributed_pomv.redb` commit durably may the runtime enqueue a post-commit notice (`POMV-003` / `P5` `RewardEvidenceNoticeV2`) referencing the committed `EventCid`, `ObjectCid`, and `evidence_scope_commitment`.
   - `BenefitClosureView` carries `is_reward_instruction = false`; whether `P5` is disabled, in shadow mode, backpressured, or slashing a fraudulent claimant has **zero** effect on `P4` views or `P1`/`P2`/`P3` behavior.
