# OneBrain vNext — ADR Traceability Matrix v1

> **Task:** `FND-010`  
> **Status:** Complete — matrix and automated CI orphan checks are active
> **Coverage:** all architecture decisions in Research Baseline §46.3 and §56.1

## 1. DRI roles

DRI is a responsibility role, not a permanent person or central authority.

| DRI role | Owns review/integration for |
|---|---|
| `DRI-SCHEMA` | canonical object/event/ID schema, codec and domain registry |
| `DRI-KU` | KU Object Family, Receptor, Affordance, Mapping and Assembly semantics |
| `DRI-KQL` | NeedIR, discovery planner, structural mapping, exploration and OBKG views |
| `DRI-NET` | session, feed, inventory, OBP-RP, carrier and provider routing |
| `DRI-SEC` | privacy, authority, capability, remote execution, key/revocation policy |
| `DRI-STORAGE` | Public/Vault/Quarantine stores, checkpoint, retention and restore |
| `DRI-MIGRATION` | legacy adapters, data migration, rollback and mixed-version behavior |
| `DRI-VERIFY` | golden vectors, property/fuzz/formal/security/partition gates |

Every implementation PR names one primary DRI and at least one independent verification reviewer for cross-boundary changes.

## 2. §46.3 decisions

| ADR | Decision | Primary tasks | DRI | Canonical type/contract | Event/command | Reducer/view | Required gate/evidence |
|---|---|---|---|---|---|---|---|
| `ADR-KU-046-01` | Knowledge Receptor is the standard typed open-interface term. | `FND-002`, `KU-002` | `DRI-KU` | `ReceptorDefinition` | — | terminology/schema validator | V0 vocabulary and schema vectors; `NEG-REC-001` |
| `ADR-KU-046-02` | Receptor is immutable/content-addressed; Assembly owns Placement. | `KU-002`, `KU-003`, `KQL-005` | `DRI-KU` | `ReceptorDefinition`, `ReceptorClaimEnvelope`, `FrontierAssemblyManifest`, `ReceptorPlacement` | assembly revision event | ReceptorView | shared Definition/two Placements; standalone publish/watch; restart rebuild |
| `ADR-KU-046-03` | Proposal becomes Mapping only at durable boundary; adoption is separate. | `KU-005`, `KU-006`, `KU-007`, `KQL-013` | `DRI-KU` | `MappingKernel`, `MappingEnvelope`, `BindingProposal` | `MaterializeMappingCommand`, `ReceptorResolutionEvent(ADOPT_BINDING)` | MappingView, ResolutionView | V1 idempotency; `NEG-MAP-001`, `NEG-MAP-002`, `NEG-MAP-003`, `NEG-MAP-004`; partial-vs-satisfied fixtures |
| `ADR-KU-046-04` | Capability Definition, Manifest, Offer and Permit have separate identities. | `CAP-001`, `CAP-002`, `CAP-003`, `CAP-004`, `CAP-005`, `RUN-003` | `DRI-SEC` | `CapabilityDefinition`, `ImplementationManifest`, `CapabilityOffer`, `DelegationPermit`, `ExecutionRecord` | `CognitiveTask` and task/result records | local conformance/offer view | authority attenuation property; Offer-without-Permit negative test |
| `ADR-KU-046-05` | Hybrid analogy uses AI/embedding for recall and symbolic correspondence for validity. | `KQL-004`, `KQL-007`, `KQL-008`, `KQL-009`, `AI-002`, `QA-002` | `DRI-KQL` | structural signatures, `MappingKernel` | proposal generation run | DiscoveryPortfolio | Anti-Gravity vocabulary swap, unit/direction/negation and embedding-off ablation |
| `ADR-KU-046-06` | Exploration has floor/debt and no popularity/trust eligibility cutoff. | `KQL-010`, `KQL-011` | `DRI-KQL` | `ExplorationPolicyV1` | ExposureTelemetry | propensity/debt/revisioned QueryView | statistical bounded-window, restart/partition and long-tail tests |
| `ADR-KU-046-07` | Full NeedIR is private; disclosure uses four explicit modes. | `SEC-001`, `SEC-002`, `SEC-003`, `KQL-012`, `QA-002` | `DRI-SEC` | DisclosurePolicy, `RouteNeedSketchV1`, `DisclosureCapsule` | disclosure session/permit | local disclosure audit view | V4 forbidden-field, dictionary/linkability, expiry/replay and support-threshold tests |
| `ADR-KU-046-08` | No global receptor closure; resolution is placement/policy/frontier-relative. | `KU-004`, `KU-007`, `QA-003` | `DRI-KU` | ResolutionPolicy | `ADOPT_BINDING`, `REVOKE_ADOPTION`, `WAIVE`, `REOPEN`, `DEFER` | `ResolutionView` | concurrency property/formal model; no `CLOSED`; exact placement target |

## 3. §56.1 decisions

| ADR | Decision | Primary tasks | DRI | Canonical type/contract | Event/command | Reducer/view | Required gate/evidence |
|---|---|---|---|---|---|---|---|
| `ADR-OBP-056-01` | Reachability is a local scoped view, not Island identity/authority. | `INV-001`, `OBP-001` | `DRI-NET` | Selector/Boundary/Budget | peer/session observations | `ReachabilityView` | single-node/recursive split/seed outage; `NEG-NET-003` |
| `ADR-OBP-056-02` | OBP-RP/1 is the deterministic resumable reconciliation profile. | `OBP-003`, `OBP-004`, `OBP-005`, `OBP-006`, `OBP-007`, `QA-001` | `DRI-NET` | `obp/reconcile/1` message set | reconciliation session records/receipts | session progress/coverage | V2 duplicate/reorder/drop/delay/crash/multi-bridge/cross-carrier |
| `ADR-OBP-056-03` | Inventory is a selector/range Merkle forest plus feed-prefix structure. | `OBP-002`, `OBP-004` | `DRI-NET` | InventoryNode, SelectorRoot, FeedPrefixRoot | inventory update journal | inventory/root view | insertion-order/restart root equality and exact divergent-prefix tests |
| `ADR-OBP-056-04` | RIBLT is optional optimization with exact verification/fallback. | `RIB-001`, `RIB-002` | `DRI-NET` | `RIBLT-1` negotiated profile | decode attempt | fast-path metrics only | optional benchmark; root mismatch fallback; `NEG-NET-004` |
| `ADR-OBP-056-05` | Feed is device-owned, namespace-scoped and single-writer per generation. | `IDN-002`, `FEED-001`, `FEED-002` | `DRI-SCHEMA` | `FeedInception`, `FeedID`, `FeedHead` | signed feed/key/delegation events | feed/key-state view | full-width/unlinkability/gap/equivocation/frontier-relative revocation tests |
| `ADR-OBP-056-06` | Checkpoint/GC uses exact proofs/floors; no global causal-stability assumption. | `CHK-001`, `CHK-002`, `CHK-003`, `CHK-004`, `CHK-005`, `CHK-006`, `QA-003` | `DRI-STORAGE` | `FeedCheckpoint`, retention classes, retirement-root schema | checkpoint/retention audit events | checkpoint/restore/retention views | formal/property tests, shadow soak, restore drill, no unseen-fork suppression |
| `ADR-OBP-056-07` | Attester independence is evidence vector/correlation grouping. | `FID-001`, `FID-002`, `FID-003` | `DRI-SEC` | `CorrelationEvidence`, `EncodingFidelityAttestation`, `FidelityPolicy` | blind attempt/attestation event | `FidelityAssessment` | 100-Sybil same-pipeline test, two evidenced-distinct blind attempts, alternate preservation |
| `ADR-OBP-056-08` | Revocation freshness is risk-tiered and action-scoped. | `REV-001`, `CAP-004`, `QA-003` | `DRI-SEC` | RevocationPolicy/Profile, key-state refs | policy/key/permit events | authorization decision view | R0/R1 no gate; terrestrial/DTN profile tests; stale frontier formal model |
| `ADR-OBP-056-09` | Provider discovery is signed multi-provider max-generation + exact retirement. | `DHT-001`, `DHT-002`, `CHK-003` | `DRI-NET` | `ProviderTuple`, `ProviderLease`, `ProviderRetire` | lease/retire record | `ProviderLeaseMap` sampled DHT view | replay no-renewal, no overwrite/resurrection, hot-key partial coverage |
| `ADR-OBP-056-10` | `GLOBAL/FULL` exist only in isolated legacy adapter with downgraded semantics. | `LEG-001`, `LEG-002`, `MIG-001`, `PROTO-001` | `DRI-MIGRATION` | `LegacyEncodingClaim`, `LegacyIdentityPrefix`, preserved original-wire record | row/batch migration journal | dual-read compatibility view | V5 ten-class idempotent backfill, redb reopen, corrupt quarantine, outbound no FULL, safe rollback |

## 4. Cross-cutting source-of-record map

| Source-of-record | Derived consumers | Forbidden reverse dependency |
|---|---|---|
| canonical object/event bytes | KQL indexes, OBKG, Fidelity, PoMV, API | canonical bytes MUST NOT include view CID/rank/status |
| feed/key/permit events | authority evaluator, capability task gate | transport/route/provider MUST NOT rewrite authority |
| Receptor/Mapping objects + resolution events | ResolutionView, Assembly/OBKG projection | rank/proposal/view MUST NOT synthesize adoption |
| Use/Derivation/Outcome evidence | Metabolic/Benefit views; optional OBT consumer | OBT MUST NOT gate evidence creation/use |
| lease/retire records | ProviderLeaseMap/DHT view | DHT arrival order MUST NOT rewrite signed record semantics |
| checkpoint/proof/floor anchors | compacted/restored view | local GC MUST NOT manufacture global deletion |
| exact read-only legacy row + migration journal | downgraded vNext migration artifact | backfill MUST NOT invent full identity, authority, fidelity, authorship, time or checkpoint validity |

## 5. Verification ownership

| Gate | DRI | Required artifacts |
|---|---|---|
| V0 Canonical/wire | `DRI-SCHEMA`, `DRI-VERIFY` | profile, domain/schema registry, golden/invalid vectors, cross-crate runner |
| V1 Algebraic/property | semantic/reducer DRI + `DRI-VERIFY` | merge/reduce/idempotency/authority properties |
| V2 Partition/reunion | `DRI-NET`, `DRI-VERIFY` | deterministic trace oracle, multi-carrier/multi-bridge simulator |
| V3 Formal | state-machine DRI + `DRI-VERIFY` | TLA+/PlusCal models and invariant results |
| V4 Security/privacy | `DRI-SEC`, `DRI-VERIFY` | threat-model cases, fuzz/resource caps, taint/linkability tests |
| V5 Compatibility/scale | `DRI-MIGRATION`, `DRI-NET`, `DRI-VERIFY` | mixed-version/carrier matrix, migration reports, analytical bounds |

## 6. Automation contract for `FND-007`

The future CI checker must fail when:

1. an ADR in §46.3/§56.1 has no row here;
2. a Task ID referenced here is absent from the implementation plan;
3. a negative assertion ID is absent from `negative_assertions.yaml`;
4. a completed schema task has no vector/evidence link;
5. a row has no DRI or required gate;
6. a public type/reducer is introduced without an ADR or explicit foundation task.

## 7. Current completion

- [x] All 18 ADRs are mapped to tasks, DRI roles, contracts, reducers/views and gates.
- [x] Cross-cutting source-of-record direction is explicit.
- [x] Verification DRI and required evidence are explicit.
- [ ] Automated orphan/vector/evidence check is implemented in CI (`FND-007`).
