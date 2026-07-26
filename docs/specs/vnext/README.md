# OneBrain vNext Foundation Contracts

> **Status:** Foundation release frozen; optional lanes tracked separately  
> **Plan:** [OneBrain Foundation Implementation Plan — KU v7.1](../../research/ONEBRAIN_FOUNDATION_IMPLEMENTATION_PLAN_V7_1.md)  
> **Decision source:** [OneBrain Research Baseline — KU v7.1](../../research/ONEBRAIN_RESEARCH_BASELINE_V7_1.md)  
> **Started:** 2026-07-20

This directory owns the cross-crate contracts that must be stable before vNext runtime code is implemented. It does not replace Core DNA, add a new OneBrain pillar, or define network-wide truth.

## Contract index

| Task | Contract | Status | Evidence |
|---|---|---|---|
| `FND-001` | [Field Ownership Matrix v1](FIELD_OWNERSHIP_MATRIX_V1.md) | Complete | Ownership domains, object/event/view split, storage classes and CID-cycle rules. |
| `FND-002` | [Normative Vocabulary v1](NORMATIVE_VOCABULARY_V1.md) | Complete | Scoped terminology and testable negative assertions. |
| `FND-002` | [Negative Assertions Registry](negative_assertions.yaml) | Complete | Machine-readable seed registry for the future conformance runner. |
| `FND-003` | [Canonical Codec and Domain Profile v1](CANONICAL_PROFILE_V1.md) | Complete | Frozen deterministic CBOR restrictions, BLAKE3 domains, versioning and resource limits. |
| `FND-004` | [Golden/invalid foundation vectors](../../../src/test-vectors/vnext/foundation/canonical-v1.json) | Complete | 55 fixed vectors across CBOR, NFC, envelopes, signatures and 21 domains plus exact-boundary/property tests. |
| `FND-005` | [Cross-crate conformance runner](../../../src/ku-core/src/foundation/conformance.rs) | Complete | The same fixture passes in `ku-core`, `onebrain-protocol` and `ku-net`. |
| `FND-006` | [Safe feature/kill-switch configuration](../../../src/onebrain-node/src/vnext_config.rs) | Complete | Default-off gates and dependency validation before runtime side effects. |
| `FND-007` | [Foundation CI](../../../.github/workflows/vnext-foundation.yml) | Complete | Format/check/lint/tests plus contract, graph, vector and link validation. |
| `FND-008` | [Foundation Threat Model v1](THREAT_MODEL_V1.md) | Complete | Assets, adversaries, trust boundaries, threat controls, abuse budgets and kill switches. |
| `FND-009` | [Anti-Gravity Reunion Corpus v1](corpus/README.md) | Complete | Typed positive, partial, hard-negative, unknown, opposition, distractor and privacy fixtures. |
| `FND-010` | [ADR Traceability Matrix v1](TRACEABILITY_MATRIX_V1.md) | Complete | All 18 ADRs mapped; CI rejects task cycles, undefined dependencies, broken links and vector drift. |
| `P0-ADR-001` | [Legacy and vNext Product Boundaries ADR v1](LEGACY_VNEXT_PRODUCT_BOUNDARY_ADR_V1.md) | Complete | Freezes additive endpoint semantics and marks legacy PoMV/wallet values as local non-economic compatibility projections. |
| `DR-P1.1` | [vNext Product Integration Profile v1](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md) · [ADR](VNEXT_PRODUCT_INTEGRATION_ADR_V1.md) | Frozen | Fourteen additive endpoint contracts, eighteen DTO field sets, typed CID/opaque continuation encoding, scoped lifecycle/error semantics and fail-closed proposal/PoMV firewalls. |
| `DR-P1.2` | [Feed Event Signer Custody Profile v1](FEED_SIGNER_CUSTODY_PROFILE_V1.md) | Frozen | Independent private-key-free feed signer, proof-of-possession, pre-encode feed/key binding, fail-closed remote/HSM errors and pre-side-effect integration tests. |
| `DR-P1.3` | [Strong Public Use Consent Profile v1](PUBLIC_USE_CONSENT_PROFILE_V1.md) | Frozen | Two-step canonical prepare/confirm, exact intent binding, private single-use receipt capability, bounded expiry and atomic replay-safe publication. |
| `DR-P1.4` | [Private KQL Persistence Profile v1](PRIVATE_KQL_PERSISTENCE_PROFILE_V1.md) | Frozen | Caller-keyed encrypted exact target bundles, deterministic local-intent adapter, startup rehydration, reversible pause and terminal cancel/retire tombstones. |
| `DR-P1.5` | [Route and Authority Boundary Profile v1](ROUTE_AUTHORITY_BOUNDARY_PROFILE_V1.md) | Frozen | Handshake-only route learning, immutable allow-listed policy versions, validated-local authority-frontier resolution and caller authority-injection closure. |
| `DR-P2.1` | [Runtime Ownership Profile v1](RUNTIME_OWNERSHIP_PROFILE_V1.md) | Frozen | One node-owned aggregate for network/KQL/Public Use/PoMV, caller-owned Vault/Policy dependencies, typed service façade, bounded workers and owner cancellation. |
| `DR-P2.2` | [Runtime Feature and Budget Profile v1](RUNTIME_FEATURE_BUDGET_PROFILE_V1.md) | Frozen | Independent KQL/Public Use/PoMV kill switches plus hard KQL, PoMV, publication, worker, peer and storage bounds. |
| `DR-P2.3` | [Runtime Lifecycle Profile v1](RUNTIME_LIFECYCLE_PROFILE_V1.md) | Frozen | Ordered startup, outbox recovery, bounded lane workers, fenced shutdown, store closure, and partial-start rollback. |
| `DR-P2.4` | [Runtime Incremental Processing Profile v1](RUNTIME_INCREMENTAL_PROCESSING_PROFILE_V1.md) | Frozen | Monotonic selector/type indexes, durable KQL and PoMV cursors, exactly-once match notifications, changed-input PoMV cache, and restart-safe continuation. |
| `DR-P2.5` | [Runtime Concurrency Profile v1](RUNTIME_CONCURRENCY_PROFILE_V1.md) | Frozen | Cloneable static service handles, short operation leases, aggregate-lock exclusion, shutdown fencing and in-flight drain. |
| `M5-00` | [Distributed Runtime Transaction Boundary Inventory v1](DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md) | Inventory frozen | Stable failpoint IDs, durable owners, downstream side effects and restart oracles for M2–M4 crash harness work. |
| `IDN-001` / `OBJ-001` / `OBJ-002` | [Identity and Knowledge Object Profile v1](IDENTITY_OBJECT_PROFILE_V1.md) | Complete | Full-width role IDs, CRDT dot/clock, schema registry and opaque-safe immutable object envelope. |
| `IDN-002` / `EVT-001` | [Feed, Authority and Event Profile v1](FEED_EVENT_PROFILE_V1.md) | Complete | Namespace-private FeedInception, frontier-relative delegation/revocation decisions and signed causal event envelope. |
| `OBS-001` / `OBS-002` | [Validated Storage Profile v1](VALIDATED_STORAGE_PROFILE_V1.md) | Complete | Validate-before-persist Public Store, XChaCha20-Poly1305 Private Vault, encrypted private quarantine and atomic memory/redb backends. |
| `FEED-001` / `FEED-002` | [Feed State and Key-State Profile v1](FEED_STATE_PROFILE_V1.md) | Complete | Rebuildable gap/branch/equivocation projection and frontier-scoped delegation attenuation/revocation reducer. |
| `INV-001` | [Inventory Scope Profile v1](INVENTORY_SCOPE_PROFILE_V1.md) | Complete | Content-addressed selectors, bounded budgets/carriers and explicitly scoped non-global coverage statements. |
| `KU-001` | [Semantic Primitives Profile v1](SEMANTIC_PRIMITIVES_V1.md) | Complete | CCID-only TermRef/StatementFrame, alpha normalization, typed three-state constraints and exact unit/dimension algebra. |
| `KU-002` | [Receptor Profile v1](RECEPTOR_PROFILE_V1.md) | Complete | Immutable declared/derived/emergent definitions, acceptance policy and Vault-only claim envelopes with explicit randomized commitment. |
| `KU-008` | [Knowledge Affordance Profile v1](KNOWLEDGE_AFFORDANCE_PROFILE_V1.md) | Complete | Explicit offered roles, typed inputs, full operating semantics, abstraction patterns and immutable derivation provenance. |
| `KU-003` | [Frontier Assembly Manifest Profile v1](ASSEMBLY_MANIFEST_PROFILE_V1.md) | Complete | Versioned lineage, immutable sources, stable receptor placements/cardinality and resolution policy references. |
| `KU-005` | [Mapping Kernel and Envelope Profile v1](MAPPING_PROFILE_V1.md) | Complete | Stable semantic correspondence/transform KernelID separated from generator/evidence provenance envelopes. |
| `KU-004` / `KU-007` | [Receptor Resolution Profile v1](RECEPTOR_RESOLUTION_PROFILE_V1.md) | Complete | Exact placement actions, materialized-adoption prerequisite and causal multi-branch frontier-relative resolution. |
| `KU-006` | [Mapping Materialization Profile v1](MAPPING_MATERIALIZATION_PROFILE_V1.md) | Complete | Explicit authorized command, disclosure firewall and atomic/idempotent Kernel+Envelope durable boundary. |
| `KQL-013` | [KQL Proposal Profile v1](KQL_PROPOSAL_PROFILE_V1.md) | Complete | Private ephemeral Mapping proposals, artifact commitments, vector scores, three-state constraints, expiry and non-executable quarantine. |
| `KQL-001` | [KQL Query Boundary Profile v1](KQL_QUERY_BOUNDARY_PROFILE_V1.md) | Complete | Private full NeedIR, scoped run/work/batch/receipt contracts and disclosure-compiled route-minimal sketches. |
| `KQL-002` | [KQL Semantic Index Profile v1](KQL_SEMANTIC_INDEX_PROFILE_V1.md) | Complete | Rebuildable CID postings for roles, semantic structure, typed constraints, units/dimensions and relation signatures. |
| `KQL-003` | [KQL Complement Planner Profile v1](KQL_COMPLEMENT_PLANNER_PROFILE_V1.md) | Complete | Independent candidate channels, bounded validation, cancellation/continuation and non-scalar proposal portfolio. |
| `KQL-004` | [KQL Exact Typed Matcher Profile v1](KQL_EXACT_TYPED_MATCHER_PROFILE_V1.md) | Complete | Exact role/relation matching, three-state qualifier/applicability checks and rational unit transforms into proposal-only output. |
| `KQL-005` | [Standing Need and Minimal View Profile v1](STANDING_NEED_MINIMAL_VIEW_PROFILE_V1.md) | Complete | Local-only durable standing needs and rebuildable Receptor/Mapping views sharing the canonical Resolution reducer contract. |
| `KQL-006` | [Reunion Delta Join Profile v1](REUNION_DELTA_JOIN_PROFILE_V1.md) | Complete | Frontier-delta Affordance/Receptor joins over bounded local candidates; [M3 runtime evidence](DISTRIBUTED_KQL_RUNTIME_PROFILE_V1.md) adds real one-hop QUIC/OBP-RP delivery, durable peer provenance and restart-safe private proposal deduplication. |
| `KQL-007` | [KQL Structural Signature Profile v1](KQL_STRUCTURAL_SIGNATURE_PROFILE_V1.md) | Complete | Exact CCID-role plus vocabulary-neutral FBS, operator-AST, graph-shingle and rational dimension/unit channels in a deterministic rebuildable candidate index. |
| `KQL-008` | [KQL Typed Relational Alignment Profile v1](KQL_RELATIONAL_ALIGNMENT_PROFILE_V1.md) | Complete | Bounded SME-style partial/many-to-many statement graph alignment with direction, systematicity, vector evidence, assumptions, unmapped regions and candidate MappingKernel output. |
| `KQL-009` | [KQL Assembly Search Profile v1](KQL_ASSEMBLY_SEARCH_PROFILE_V1.md) | Complete | Beam-scheduled weighted three-state CSP over Assembly sizes 2--4, hard-violation isolation, Pareto page merger and context-bound exact continuation. |
| `KQL-010` | [KQL Exploration Policy v1](KQL_EXPLORATION_POLICY_V1.md) | Complete | Frozen 10/20/30/40 profile, persistent starvation debt, three long-tail cohorts, seeded exact propensity audit and deterministic exact/admin bypass. |
| `KQL-011` | [KQL Revisioned QueryView and Exposure Learning Profile v1](KQL_QUERY_VIEW_LEARNING_PROFILE_V1.md) | Complete | Canonical-CID result dedup, late-result child revisions, source-count-neutral provenance roots and private inverse-propensity learning separated from validated UseEvent. |
| `KQL-012` | [KQL Private Multipath Query Profile v1](KQL_PRIVATE_MULTIPATH_PROFILE_V1.md) | Complete | Up to three schema-unlinkable route sketches, SEC-003-opened one-time replies, canonical local union, partial path coverage and encrypted exactly-once StandingNeed mailbox. |
| `OBKG-001` | [OBKG Derived Projection Profile v1](OBKG_DERIVED_PROJECTION_PROFILE_V1.md) | Complete | Disposable frontier/version-bound Receptor/Affordance/Mapping/Use views; only materialized+adopted mappings become active and exposure cannot become Use. |
| `AI-001` | [Local Receptor Encoder Profile v1](LOCAL_RECEPTOR_ENCODER_PROFILE_V1.md) | Complete | Deterministic model-independent encoding, exact provenance spans, explicit limitations, no fabricated CCIDs and private-by-default derived/emergent receptors. |
| `AI-002` | [AI Model Recall Firewall Profile v1](AI_MODEL_RECALL_FIREWALL_V1.md) | Complete | Optional CAP-001-bound model recall/ranking whose scores never enter the independent symbolic Mapping validity request; offline/model-off ablation preserves common validity. |
| `AI-003` | [AI Local Observation Intake Profile v1](AI_LOCAL_OBSERVATION_INTAKE_V1.md) | Complete | Consent/revocation-gated text/file/sensor input into encrypted local SourceArtifact and signed ObservationEvent, yielding only an exact-span private encoding proposal. |
| `AI-004` | [AI Local Knowledge Companion Profile v1](AI_LOCAL_KNOWLEDGE_COMPANION_V1.md) | Complete | Offline private NeedIR/StandingNeeds and bounded local recommendations; KQL-012 is optional proposal-only, while route/share/materialize remain behind exact policy/consent/authority executors. |
| `QA-002` | [M5 Multi-Objective Benchmark Profile v1](M5_MULTI_OBJECTIVE_BENCHMARK_V1.md) | Complete | Reproducible exact metric/gate vector for gap fill, useful Assemblies, hard violations, long-tail exposure, privacy, consent and model validity ablation—without a hiding aggregate scalar. |
| `CHK-001` / `CHK-002` | [Feed Checkpoint and Proof Profile v1](FEED_CHECKPOINT_PROOF_PROFILE_V1.md) | Complete | Signed schema-4 per-feed checkpoint, exact Merkle inclusion/prefix consistency/reducer-effect proofs, frontier key-state binding and branch-preserving checkpoint conflicts; signature alone never suppresses or deletes. |
| `CHK-003` / `CHK-004` / `CHK-005` / `CHK-006` | [Checkpoint Compaction and Local GC Profile v1](CHECKPOINT_COMPACTION_AND_LOCAL_GC_PROFILE_V1.md) | Complete | Exact lease/retire/permit/key/checkpoint high-water anchors, dry-run shadow manifests, signed custody and restore drill, then audit-first local eviction behind policy/consent/soak/kill-switch gates. |
| `QA-003` | [M6 Bounded Formal Model Profile v1](M6_BOUNDED_FORMAL_MODEL_PROFILE_V1.md) | Complete | Five TLA+ models plus deterministic executable bounded explorer for checkpoint, resolution, provider retirement, permit revocation and scoped reconciliation safety invariants. |
| `LEG-001` | [Negotiated Legacy Adapter Profile v1](NEGOTIATED_LEGACY_ADAPTER_PROFILE_V1.md) | Complete | Transcript-negotiated parse/normalize firewall: GLOBAL becomes sampled partial reachable coverage, FULL becomes non-corroborating LegacyEncodingClaim, exact raw bytes remain LOCAL_ONLY evidence and outbound is capped at PART=2. |
| `AI-005` | [Local Affordance Extractor Profile v1](LOCAL_AFFORDANCE_EXTRACTOR_PROFILE_V1.md) | Complete | Offline rule projection of explicit or evidenced affordances, immutable engine/rule provenance, no evidence expansion and deterministic rebuild. |
| `RUN-001` | [Local Vertical Slice Profile v1](LOCAL_VERTICAL_SLICE_PROFILE_V1.md) | Complete | Offline Assembly-to-Need-to-Proposal-to-Materialize-to-signed-Adopt flow across a durable StandingNeed restart boundary without collapsing authority. |
| `RUN-002` | [Additive KU Workflow Surface v1](ADDITIVE_KU_WORKFLOW_SURFACE_V1.md) | Complete | Shared read-only API/CLI contract for Assembly, Receptor, Discover, Proposal, Mapping and relative Resolution with explicit assumptions, violated/unknown constraints, scope and next boundary. |
| `PROTO-001` | [Protocol Codec and Legacy Isolation Profile v1](PROTOCOL_CODEC_ISOLATION_PROFILE_V1.md) | Complete | One canonical typed vNext message codec/wire-ID owner with exact payload CID binding and parse-only legacy byte preservation. |
| `POMV-001` | [Use and Derivation Evidence Profile v1](USE_DERIVATION_EVIDENCE_PROFILE_V1.md) · [Distributed runtime](DISTRIBUTED_POMV_RUNTIME_PROFILE_V1.md) | Complete | Signed, typed and authority-assessed use records travel over authenticated QUIC; replay across one/two/five paths deduplicates by EventCID without truth, benefit, ranking or reward semantics. |
| `POMV-002` | [Metabolic Evidence View v1](METABOLIC_EVIDENCE_VIEW_V1.md) · [Distributed runtime](DISTRIBUTED_POMV_RUNTIME_PROFILE_V1.md) | Complete | Durable policy/frontier-relative EventCID roots and linked revisions survive restart; conflicts have no arrival-order winner and wallet/OBT remain isolated. |
| `POMV-004` | [Outcome Observation and Benefit Evidence Profile v1](OUTCOME_BENEFIT_EVIDENCE_PROFILE_V1.md) | Complete | Signed outcome/benefit objects separate observed valence from attribution, require explicit UNKNOWN/counterfactual limitations and preserve contradictory branches without reward semantics. |
| `POMV-003` | [Knowledge-Plane / Reward Firewall v1](KNOWLEDGE_REWARD_FIREWALL_V1.md) | Complete | Default-off, bounded post-commit evidence export whose unavailable/backpressured/corrupt consumer cannot enter KU/KQL/OBP transactions or canonical authority. |
| `OBP-002` | [Hybrid Inventory Forest Profile v1](HYBRID_INVENTORY_FOREST_PROFILE_V1.md) | Complete | Selector-scoped full-CID radix lanes plus branch-preserving feed prefixes, restart-stable roots and checkpoint-limited coverage. |
| `NET-001` | [Authenticated Session Profile v1](AUTHENTICATED_SESSION_PROFILE_V1.md) | Complete | Canonical signed Hello/Welcome/Finish with channel/transcript binding, full NodeID principals, downgrade defense and selective non-authoritative feed proofs. |
| `NET-002` | [Node Identity Key Custody Profile v1](NODE_IDENTITY_KEY_CUSTODY_PROFILE_V1.md) | Complete | Caller-owned OS/HSM/remote signer boundary, pre-side-effect proof of possession, no private-key export or fallback, and restart-stable external-signature resume evidence. |
| `CAR-001` | [Deterministic Carrier Profile v1](DETERMINISTIC_CARRIER_PROFILE_V1.md) | Complete | Shared canonical records through memory and reopenable file bundles with controlled drop/duplicate/reorder injection and no semantic authority. |
| `OBP-007` | [Cross-Carrier Reconciliation Profile v1](CROSS_CARRIER_RECONCILIATION_PROFILE_V1.md) | Complete | Same inbox/journal/state-machine outcome through memory, file, delayed and bounded QUIC stream framing; delayed absence stays partial/unknown. |
| `OBP-001` | [Reachability View Profile v1](REACHABILITY_VIEW_PROFILE_V1.md) | Complete | Local authenticated peer/selector/carrier observations with scoped limitations, no island/component authority and always-usable standalone mode. |
| `OBP-003` | [OBP Reconciliation Protocol Profile v1](OBP_RECONCILIATION_PROTOCOL_PROFILE_V1.md) | Complete | Canonical transcript/selector/namespace/disclosure/budget-bound Hello-to-Resume schema, including peer-bound cross-session rebinding, full-CID manifests, caps and no authority/global-completion semantics. |
| `OBP-004` | [Deterministic Reconciliation State Machine v1](DETERMINISTIC_RECONCILIATION_STATE_MACHINE_V1.md) | Complete | Deterministic radix diff and manifest-before-payload/validate-then-accept receiver; fair redelivery converges and corrupt branches remain isolated. |
| `OBP-005` | [Persisted Reconciliation Journal v1](PERSISTED_RECONCILIATION_JOURNAL_V1.md) | Complete | Canonical memory/Redb journal, peer/scope/checkpoint/MAC-bound single-use continuation, cross-session manifest rebinding, crash repair and durable retry/backpressure limits. |
| `OBP-006` | [Multi-Bridge Merge Profile v1](MULTI_BRIDGE_MERGE_PROFILE_V1.md) | Complete | Canonical message/payload-variant dedup, retained path observations, deterministic conflict delivery and bridge-count-independent semantic state. |
| `QA-001` | [Anti-Gravity Reunion Canary v1](ANTI_GRAVITY_REUNION_CANARY_V1.md) | Complete | M3 cross-pillar gate: partition/restart, public reconciliation, private delta match, proposal/materialize/adopt boundaries, replay and Use evidence. |
| `CAP-001` | [Capability Layer and Field Ownership Profile v1](CAPABILITY_LAYER_PROFILE_V1.md) | Complete | Canonical Definition/Manifest/Offer/Permit/Execution bodies with semantic, availability, authority and provenance identities kept separate. |
| `CAP-002` | [Local Manifest Builder and Conformance Profile v1](LOCAL_MANIFEST_CONFORMANCE_PROFILE_V1.md) | Complete | Reproducible typed implementation commitments, coarse public sketch firewall and bounded vector conformance report. |
| `CAP-003` | [Signed Capability Offer Profile v1](SIGNED_CAPABILITY_OFFER_PROFILE_V1.md) | Complete | Exact feed-provider signature binding, bounded leases and generation high-water reducer that prevents stale resurrection. |
| `CAP-004` | [Delegation Permit Validation Profile v1](DELEGATION_PERMIT_VALIDATION_PROFILE_V1.md) | Complete | Frontier-relative issuer authentication and fail-closed child attenuation across every authority dimension. |
| `CAP-005` | [Typed Cognitive Executor Profile v1](TYPED_COGNITIVE_EXECUTOR_PROFILE_V1.md) | Complete | Permit-gated typed steps with deterministic logical deadline/cancellation, partial commitments and provenance-only results. |
| `FID-001` | [Encoding Fidelity Evidence Profile v1](ENCODING_FIDELITY_EVIDENCE_PROFILE_V1.md) | Complete | Immutable attempts/attestations, categorical per-dimension correlation evidence and default two-group blind-attempt policy contract without truth voting. |
| `FID-002` | [Blind Encoding Fidelity Workflow v1](BLIND_ENCODING_FIDELITY_WORKFLOW_V1.md) | Complete | Commit-before-reveal external attempts, exact source-span/gene/concept checks, two-group portfolio and immutable alternate preservation. |
| `FID-003` | [Fidelity Assessment Reducer v1](FIDELITY_ASSESSMENT_REDUCER_V1.md) | Complete | Policy/frontier-relative deterministic assessment, retained mismatch evidence and normalized-legacy isolation without `FULL`. |
| `SEC-001` | [Disclosure Policy and Sanitizer v1](DISCLOSURE_POLICY_SANITIZER_V1.md) | Complete | Private-default four-mode policy, scoped consent, local taint audit and deterministic generalize-or-suppress projections with no stable IDs/raw/private refs. |
| `SEC-002` | [RouteNeedSketch Packet v1](ROUTE_NEED_SKETCH_PACKET_V1.md) | Complete | At most three fixed-size one-token packets with distinct per-run entropy, one-time reply capabilities and receiver-relative replay/expiry enforcement. |
| `SEC-003` | [Progressive Disclosure Capsule v1](PROGRESSIVE_DISCLOSURE_CAPSULE_V1.md) | Complete | CAP-004-bound XChaCha20-Poly1305 capsules with Affordance-first bilateral approval, fixed encrypted padding, replay/TTL/ceiling/cancellation checks. |
| `REV-001` | [Revocation Freshness Policy v1](REVOCATION_FRESHNESS_POLICY_V1.md) | Complete | Action-tiered R0–R4 local freshness decisions, exact frontier scopes, named terrestrial windows and permit-bound task-specific DTN profiles without global liveness. |
| `DHT-001` | [Provider Lease and Retirement Profile v1](PROVIDER_LEASE_RETIRE_PROFILE_V1.md) | Complete | Signed multi-provider tuples, immutable generation forks, exact retirement floors and local first-seen lease age with replay no-renewal/no resurrection. |
| `DHT-002` | [Bounded Provider Discovery View v1](BOUNDED_PROVIDER_DISCOVERY_VIEW_V1.md) | Complete | Deterministic bounded merge of direct/PEX/cache LeaseCIDs, local TTL probes, provider diversity and honest sampled pagination with no global-completeness claim. |
| `MIG-001` | [Additive Migration Storage Profile v1](ADDITIVE_MIGRATION_STORAGE_PROFILE_V1.md) | Complete | Immutable legacy rows, atomic per-batch vNext writes, quarantine, exact journals, kill/restart idempotency and rollback-safe dual-read. |
| `LEG-002` | [Legacy Data Backfill Profile v1](LEGACY_DATA_BACKFILL_PROFILE_V1.md) | Complete | Ten-class §17 backfill preserving exact raw bytes and LOCAL_ONLY provenance without inventing identity, authorship, time, authority, fidelity or checkpoint validity. |
| `RUN-004` | [Scoped Runtime Status Profile v1](SCOPED_RUNTIME_STATUS_PROFILE_V1.md) | Complete | Honest local usability, reachability, coverage/frontier, fidelity, legacy and consent status without FULL/GLOBAL/CLOSED aliases. |
| `QA-004` | [Mixed-Version and Cross-Carrier Conformance v1](MIXED_VERSION_CROSS_CARRIER_CONFORMANCE_V1.md) | Complete | Same validate-then-accept outcome across memory, file, QUIC and delayed carriers, plus downgrade-isolated legacy behavior. |
| `QA-005` | [vNext Security Suite v1](VNEXT_SECURITY_SUITE_V1.md) | Complete | Six adversarial probes plus runtime decompression admission and cognitive task replay guards, with zero authority amplification. |
| `QA-006` | [Algebraic and Trace Property Suite v1](ALGEBRAIC_AND_TRACE_PROPERTY_SUITE_V1.md) | Complete | Seven named properties for merge, reducer, materialization/adoption, authority, retirement and scoped completion under permutation and replay. |
| `QA-007` | [Logical-Node Scale and Analytical Bound Profile v1](LOGICAL_NODE_SCALE_AND_ANALYTICAL_BOUND_PROFILE_V1.md) | Complete | Streaming 10k/100k split-operate-reunite simulation plus local-cap state/bandwidth analysis; 30B is explicitly an assumption-bound extrapolation, not a simulated claim. |
| `QA-008` | [Performance Regression Budget Profile v1](PERFORMANCE_REGRESSION_BUDGET_PROFILE_V1.md) | Complete | Versioned correctness-coupled budgets for object bytes, inventory update/diff, duplicate bridges, hot provider hints and restore time. |
| `DOC-001` | [Normative Freeze and Evidence Index v1](VNEXT_NORMATIVE_FREEZE_AND_EVIDENCE_INDEX_V1.md) | Complete | Frozen interoperability profile, operator runbook, migration/rollback guide, gate evidence and explicit optional/default-off lanes. |

## Foundation release pack

- [Interoperability Profile v1](VNEXT_INTEROPERABILITY_PROFILE_V1.md)
- [Operator Runbook v1](VNEXT_OPERATOR_RUNBOOK_V1.md)
- [Migration and Rollback Guide v1](VNEXT_MIGRATION_AND_ROLLBACK_GUIDE_V1.md)
- [Normative Freeze and Evidence Index v1](VNEXT_NORMATIVE_FREEZE_AND_EVIDENCE_INDEX_V1.md)
- [vNext Product Integration Profile v1](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md)

## Executable foundation gate

- [Frozen canonical vectors](../../../src/test-vectors/vnext/foundation/canonical-v1.json)
- [Frozen identity/object vectors](../../../src/test-vectors/vnext/foundation/identity-object-v1.json)
- [Frozen feed/event vectors](../../../src/test-vectors/vnext/foundation/feed-event-v1.json)
- [Frozen public knowledge exchange selector](../../../src/test-vectors/vnext/inventory/public-knowledge-exchange-v1.json)
- [Reference Rust foundation](../../../src/ku-core/src/foundation)
- [vNext contract validator](../../../scripts/ci/validate_vnext_contracts.py)
- [Foundation CI workflow](../../../.github/workflows/vnext-foundation.yml)

## Normative precedence

If documents disagree, use this order until replaced by a newer signed/merged ADR:

1. Founder directives in the Research Baseline.
2. Architecture decisions in §46.3 and §56.1 of the Research Baseline.
3. Contracts in this directory.
4. Legacy specs and current implementation behavior.

Legacy code is evidence of the current state, not authority to override a vNext decision.

## Change control

A change to a public field, ownership domain, canonical token or negative assertion must:

1. identify the affected ADR and Task ID;
2. state migration and downgrade behavior;
3. add or revise its golden/invalid vector once `FND-004` exists;
4. preserve original legacy bytes when the change is a migration;
5. avoid adding OBT, seed, bridge, provider or route state to knowledge authority.

## Completion evidence for WP-001

- Every required family in `FND-001` has separate semantic, authority, availability, runtime and derived-view ownership.
- The CID graph rules prohibit self-reference and object↔view identity cycles.
- Every required ambiguous term in `FND-002` has mandatory qualifiers or a canonical replacement.
- `GLOBAL` and `FULL` appear only as quoted legacy input aliases, never as canonical vNext enum values.
- The negative-assertion registry has unique IDs and is intended to become direct input to `FND-004/FND-005`.
