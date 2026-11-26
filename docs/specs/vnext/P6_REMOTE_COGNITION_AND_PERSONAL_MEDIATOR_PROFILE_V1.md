# Pillar 6 Specification — Remote Cognition, Delegated Work, and Sovereign Personal AI Mediator Profile v1

> - **Profile ID:** `P6-AI-MEDIATOR-RUN003-001`
> - **Pillar:** `P6` (AI Layer)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [CAPABILITY_LAYER_PROFILE_V1](CAPABILITY_LAYER_PROFILE_V1.md), [SIGNED_CAPABILITY_OFFER_PROFILE_V1](SIGNED_CAPABILITY_OFFER_PROFILE_V1.md), [DELEGATION_PERMIT_VALIDATION_PROFILE_V1](DELEGATION_PERMIT_VALIDATION_PROFILE_V1.md), [TYPED_COGNITIVE_EXECUTOR_PROFILE_V1](TYPED_COGNITIVE_EXECUTOR_PROFILE_V1.md), [AI_LOCAL_OBSERVATION_INTAKE_V1](AI_LOCAL_OBSERVATION_INTAKE_V1.md), [AI_MODEL_RECALL_FIREWALL_V1](AI_MODEL_RECALL_FIREWALL_V1.md), [AI_LOCAL_KNOWLEDGE_COMPANION_V1](AI_LOCAL_KNOWLEDGE_COMPANION_V1.md), [KU_EXTRACTION_FRAMEWORK_PROFILE_V1](KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md), [KU_SEMANTIC_SELECTION_PROFILE_V2](KU_SEMANTIC_SELECTION_PROFILE_V2.md), [BLIND_ENCODING_FIDELITY_WORKFLOW_V1](BLIND_ENCODING_FIDELITY_WORKFLOW_V1.md), [PROGRESSIVE_DISCLOSURE_CAPSULE_V1](PROGRESSIVE_DISCLOSURE_CAPSULE_V1.md), [REVOCATION_FRESHNESS_POLICY_V1](REVOCATION_FRESHNESS_POLICY_V1.md), [VALIDATED_STORAGE_PROFILE_V1](VALIDATED_STORAGE_PROFILE_V1.md)
> - **Owner decisions addressed:** `D-011`, `D-013`, `D-014`, `D-017`, `D-051`

## 1. Purpose and Scope

Existing P6 foundation profiles define capability schemas (`CAP-001..005`), local observation intake (`AI-003`), the model recall firewall (`AI-002`), bounded local extraction (`KU-ENC-001`, `KU-SEM-001`), and the local companion (`AI-004`). Three specification gaps remained open in P6:

1. **`RUN-003` (`M4b`) Optional Remote Cognition Protocol (`obp/cognition/1`):** End-to-end network protocol binding `CAP-003` capability offers, `CAP-004` permits, `REV-001` freshness, `SEC-003` disclosure capsules, remote cancellation, and receiver quarantine.
2. **`D-013` Delegated Encoding & Autonomous Blind Verification Jobs:** Bounded job-claim leases, source-access scoping (`worker access != publication`), `FID-002` commit-before-reveal enforcement across peers, and post-commit work-evidence emission to `P5` (`D-014`).
3. **Sovereign Personal AI Mediator (`PAM`) vNext Bridge:** Upgrading legacy `ku-mediator` SQLite/CoreDNA coupling to a unified `PrivateVault`-backed mediator that orchestrates `AI-001..005`, `KU-SEM-001` v2 drafts, and `P7` read-only views without leaking private context or forging `UseEvidence`.

---

## 2. `RUN-003` Optional Remote Cognition Protocol (`obp/cognition/1`)

### 2.1 Independent Default-Off Runtime Gates

1. Remote cognitive execution **MUST** be gated behind two independent default-off runtime flags in `FND-006`:
   - `remote_cognition_requester = false` (governs outbound `CognitiveTask` dispatch),
   - `remote_cognition_executor = false` (governs advertising `SignedCapabilityOffer`s and accepting inbound `obp/cognition/1` streams).
2. Disabling either flag **MUST** immediately reject new tasks and cancel in-flight `CAP-005` execution tokens while leaving all local `P1`/`P3`/`P6`/`P8` workflows fully operational.

### 2.2 Offer Discovery, Permit Binding, and Capsule Transport

1. **Offer Selection (`CAP-002` / `CAP-003` / `DHT-002`):**
   - Before dispatching a remote task, the requester **MUST** validate the provider's `SignedCapabilityOffer` against its `CapabilityManifest` commitment (`CAP-002`), checking `capability_definition_cid`, `sandbox_isolation_class`, `max_input_bytes`, `lease_expiry`, and `ProviderRetirementFloor` (`DHT-001`).
2. **Non-Delegable Scoped Permit (`CAP-004` + `REV-001`):**
   - Every remote task **MUST** carry a requester-signed `SignedDelegationPermit` with:
     - `onward_delegation = false` (provider **MUST NOT** sub-delegate to a third peer),
     - `retention_policy = DeleteAfterTask`,
     - `training_permission = false` (task inputs/outputs **MUST NOT** enter weight updates or persistent provider corpora),
     - explicit `allowed_effect_set`, `max_logical_steps`, `max_wall_ms`, and `max_output_bytes`.
   - Both requester and executor **MUST** evaluate permit validity under `REV-001` tier `R3` (`300 s` terrestrial freshness window, or an explicit permit-bound `TaskSpecificDtn/1` profile for high-latency links).
3. **Encrypted Capsule Framing & Remote Cancellation (`SEC-003`):**
   - Task inputs travel exclusively inside a `DisclosureCapsuleV1` (`SEC-003`) over a `NET-001` mutually authenticated QUIC stream (`ALPN: obp/cognition/1`). Raw `SomaticBiometricRaw` (`P9`) and unsanitized `LOCAL_ONLY` vault keys **MUST NOT** enter any remote capsule.
   - An authenticated `RemoteTaskCancelV1 { task_nonce, permit_cid, reason }` frame from the requester (or session close / permit expiry) **MUST** immediately trip the executor's `CAP-005` `CancellationToken`, zeroize ephemeral scratch buffers, and return a deterministic `CancelledExecutionReceipt` without persisting task residuals.

### 2.3 Sandboxed Execution and Mandatory Receiver Quarantine

1. The executor **MUST** run the task inside `TypedCognitiveExecutor` (`CAP-005`) with `NetworkAccess::None` during inference and emit a signed `CapabilityExecutionRecordBody` committing `(task_nonce, permit_cid, manifest_cid, input_commitment_cid, output_commitment_cid, step_trace_root, resource_usage)`.
2. Upon receipt, the requester **MUST**:
   - verify `CognitiveTaskReplayGuard` (`QA-005`) against `(requester_node_id, task_nonce, permit_cid)` so replayed or delayed responses reject fail-closed,
   - verify `output_commitment_cid` and deterministic schema bounds before decoding, and
   - persist the returned payload **exclusively** into `OBS-002` `QuarantineRecord` with `is_executable = false` and `trust_origin = RemoteCognitiveProposal` (`NEG-CAP-002`, `FW-01`, `FW-02`).
3. Remote execution output **MUST NOT** directly insert into `ValidatedStore`, materialize a `MappingKernel`, adopt a `Receptor` binding, or emit `UseEvidence`.

---

## 3. `D-013` Delegated Encoding and Autonomous Blind Verification Jobs

### 3.1 Delegated Encoding (`DelegatedEncodeJobV1`)

Nodes lacking a local GPU or qualified local LLM **MAY** opt in to `D-013` delegated encoding over `RUN-003`:

1. **Worker Access $\neq$ Publication (`FW-01`):** The requester retains canonical `SourceArtifact` custody in `PrivateVault` (`P8`) and encloses only the user-consented text/document span inside a single-task `DisclosureCapsuleV1` (`DeleteAfterTask`). Granting a worker capsule read access **MUST NOT** classify the source or resulting draft as `PUBLIC`.
2. **Bounded Non-Renewable Job Claim Lease:** Each job issues a `JobClaimLeaseV1 { job_id, worker_node_id, permit_cid, lease_ttl_ms <= 300_000 }` persisted in `0x000D` operational storage. If a worker crashes or misses `lease_ttl_ms`, the requester marks the lease `ExpiredTombstone` and **MAY** re-issue a fresh `job_id` to a distinct provider without ambiguity.
3. **Host-Owned Semantic Compilation (`D-017`):** The worker returns only a grounded `ku-semantic-selection/2.0` proposal (`KU_SEMANTIC_SELECTION_PROFILE_V2.md`) + exact span anchors into `QuarantineRecord`. The requester's own host **MUST** execute closed `ConceptRegistry` CCID lookup, structural validation, and `SemanticFrameSet` lowering (`P1_CONCEPT_LIFECYCLE_AND_REGISTRY_DISTRIBUTION_PROFILE_V1.md`).

### 3.2 Autonomous Blind Verification (`BlindVerifyJobV1`)

To establish multi-party encoding fidelity (`FID-001..003`) without central oracle servers:

1. **Two-Phase Commit-Before-Reveal (`FID-002`):**
   - *Phase 1 (Blind Attempt):* Given a consented `SourceArtifact` span (and **zero** access to the primary encoder's `SemanticFrameSet`), an independent verifier computes its own candidate encoding and signs `BlindAttemptCommitment { job_id, source_span_cid, attempt_semantic_cid_salted_hash, correlation_ declaration }` on its `FeedId`.
   - *Phase 2 (Reveal & Structural Diff):* Only after the requester acknowledges the signed commitment at its local frontier does the verifier reveal its `EncodingFidelityAttempt` and compute deterministic span/CCID/unit/qualifier correspondence (`FID-003`), producing a signed `EncodingFidelityAttestation`.
2. **Independence Enforcement (`CorrelationEvidence`):** A `BlindVerifyJobV1` portfolio **MUST** require distinct `CorrelationEvidence` groups across `(operator_actor_id, model_family_id, prompt_template_cid, runtime_vendor)` before upgrading local `FidelityAssessment` beyond single-group `CORROBORATED_SINGLE_GROUP`.
3. **Post-Commit Work Notice (`D-014`):** Once the requester validates and commits an accepted `DelegatedEncodeJobV1` or `BlindVerifyJobV1` `EncodingFidelityAttestation` (whether matching or reporting an evidenced structural mismatch) into `P8`, the post-commit hook emits an `AcceptedEncodeVerifyWork` notice to `P5` (`P5_OBT_BENEFIT_AND_WORK_ECONOMY_PROFILE_V1.md`) across `FW-07`.

---

## 4. Sovereign Personal AI Mediator (`PAM`) vNext Bridge

### 4.1 `PrivateVault` 4-Tier Memory Architecture (Superseding `ku-mediator` SQLite)

Legacy `ku-mediator` persisted chat logs and profiles in unencrypted SQLite tables. In vNext, `PAM` stores all durable personal state inside `P8` `PrivateVault` (`purpose = 4`, XChaCha20-Poly1305, `LOCAL_ONLY` by default):

| Memory Tier | Custody & Encryption | Retention & Sync Policy |
|---|---|---|
| **Tier M0 (`EphemeralTurnBuffer`)** | In-RAM mlock/zeroize buffer only | Wiped immediately upon turn completion, cancellation, or `SomaticImmediate/1` abort. |
| **Tier M1 (`WorkingSessionThread`)** | `PrivateVault` (`purpose = 4`, `LOCAL_ONLY`) | Bounded rolling window (`<= 256` turns); eligible for same-`ActorId` Personal Mesh sync (`P10`) only when explicitly toggled. |
| **Tier M2 (`EpisodicObservationLinks`)** | `AI-003` `ObservationEvent` + `SourceArtifact` refs | Immutable provenance pointers linking chat/file/sensor captures to reviewed private drafts. |
| **Tier M3 (`SovereignActorPreferences`)** | `PrivateVault` (`purpose = 4`, `LOCAL_ONLY`) | User-inspected domain vocabulary preferences, disclosure defaults (`PrivateOnly`), and exploration appetite; **never** implicitly exported to remote `RUN-003` workers. |

### 4.2 Grounded Recall, RAG Exposure Firewall, and `RecommendationGuard`

1. **Symbolic Firewall (`AI-002`, `FW-03`):** When `PAM` assists query formulation (`AI-004`), it compiles user intent into `KnowledgeNeedIr` (`P3`) and optionally supplies `RecallCandidateSeed`s. Model scores **MUST NOT** bypass `ExactTypedMatcher` (`KQL-004`) or `SymbolicMappingValidator`.
2. **RAG Exposure $\neq$ Use (`NEG-POMV-001`, `FW-06`):** Retrieving or quoting KUs inside a `PAM` conversation **MUST** record at most local `ExposureTelemetry` (`KQL-011`) and **MUST NOT** emit `UseEvidence` (`POMV-001`). Signed `UseEvidence` requires a separate downstream task/derivation commitment.
3. **Non-Executing Recommendations (`AI-004`):** Every side-effecting action proposed by `PAM` (saving a KU, preparing a `PUBLIC` share, opening a `SEC-003` capsule, materializing a `Mapping`, or adopting a `Receptor` binding) **MUST** return a non-executing `RecommendationCard` gated by `RecommendationGuard` until confirmed by an explicit `P10` user gesture.
4. **Honest Qualification Reporting (`KU-ENC-001`):** Until a model passes full conformance and domain qualification gates, all `PAM` and extraction responses **MUST** carry `model_qualified: false` (`unassessed` epistemic label).

---

## 5. Cross-Pillar Boundaries & Negative Assertions

- **`P6 ↔ P1`:** Models propose sparse selections/spans only; host deterministic code owns `ConceptCcid` resolution, unit algebra, and `SemanticFrameSet` byte generation (`D-017`).
- **`P6 ↔ P2 / P8`:** Every `RUN-003` and `D-013` remote payload enters `QuarantineRecord` (`OBS-002`) with `is_executable = false` (`NEG-CAP-002`).
- **`P6 ↔ P5`:** `AcceptedEncodeVerifyWork` notices cross `FW-07` strictly post-commit; OBT balance or stake **MUST NOT** gate access to local `PAM` or `CAP-004` permit validation.
- **`P6 ↔ P9`:** `P9` somatic streams enter `PAM` solely as `LOCAL_ONLY` `AI-003` artifacts and **MUST NOT** be forwarded to `RUN-003` remote executors.
