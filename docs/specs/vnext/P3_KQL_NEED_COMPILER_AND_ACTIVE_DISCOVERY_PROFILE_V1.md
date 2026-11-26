# Pillar 3 Specification — Deterministic `FIND → NeedIR` Compiler, Direct KU Matching, and Active Discovery (`M6A`) Profile v1

> - **Profile ID:** `P3-KQL-COMPILER-M6A-001`
> - **Pillar:** `P3` (Knowledge Query Language — KQL)
> - **Milestones / Tasks covered:** `ARCH-11` (`FIND → NeedIR` compiler), direct `SemanticFrameSet` matching, and Distributed Runtime `M6A.1–M6A.3`
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [KQL_QUERY_BOUNDARY_PROFILE_V1](KQL_QUERY_BOUNDARY_PROFILE_V1.md), [KQL_SEMANTIC_INDEX_PROFILE_V1](KQL_SEMANTIC_INDEX_PROFILE_V1.md), [KQL_EXACT_TYPED_MATCHER_PROFILE_V1](KQL_EXACT_TYPED_MATCHER_PROFILE_V1.md), [PRIVATE_KQL_PERSISTENCE_PROFILE_V1](PRIVATE_KQL_PERSISTENCE_PROFILE_V1.md), [DISCLOSURE_POLICY_SANITIZER_V1](DISCLOSURE_POLICY_SANITIZER_V1.md), [ROUTE_NEED_SKETCH_PACKET_V1](ROUTE_NEED_SKETCH_PACKET_V1.md), [PROGRESSIVE_DISCLOSURE_CAPSULE_V1](PROGRESSIVE_DISCLOSURE_CAPSULE_V1.md), [KQL_PRIVATE_MULTIPATH_PROFILE_V1](KQL_PRIVATE_MULTIPATH_PROFILE_V1.md), [PROVIDER_LEASE_RETIRE_PROFILE_V1](PROVIDER_LEASE_RETIRE_PROFILE_V1.md)
> - **Owner decisions addressed:** `D-051` §3, `ADR-KU-046-05..07`, `ADR-OBP-056-09`

## 1. Purpose and Scope

In `ARCH-04`, `ARCH-08`, and `ARCH-11`, textual KQL was frozen read-only, `vnext_semantic_index` was wired to `/ku` structured search, and `/api/vnext/kql/needs/prepare` was honestly labeled `semantic_mapping_is_hash_derived_not_compiled` because `adapt_local_intent` hashed raw query strings instead of compiling semantic predicates/units. In addition, `ExactTypedMatcher` (`KQL-004`) matched only `KnowledgeAffordance` (kind `5`) objects rather than committed `SemanticFrameSet` KUs (kind `2`), and `M6A` active distributed discovery modules (`KQL-007..012`, `SEC-002/003`, `DHT-001/002`) lacked durable runtime persistence/wire bindings.

This profile specifies:
1. The **Deterministic Bounded `FIND → KnowledgeNeedIr` Semantic Compiler** (eliminating hash-derived synthetic CCIDs and `SelectorCid`s).
2. **Direct Committed `SemanticFrameSet` ↔ `KnowledgeNeedIr` Virtual Affordance Matching**.
3. **`M6A` Active Distributed KQL Runtime Persistence, Wire Closure, and Lease-Bounded `WATCH`**.

---

## 2. Deterministic Bounded `FIND → KnowledgeNeedIr` Compiler (`KQL-CMP-001`)

### 2.1 Closed Compilable Query Subset & Rejected Legacy Fields

The compiler accepts either a typed `StructuredNeedCompileRequestV1` or a bounded read-only `FIND` AST (`ku_kql::read_only`) restricted to `SEMANTIC_IDENTITY` constructs:

| Construct | Compilable Form | Lowered Target in `KnowledgeNeedIr` / `ReceptorDefinition` |
|---|---|---|
| **Target role / predicate** | `predicate = "<label_or_ccid>"` or pattern `(?x)-[:<pred>]->(?y)` | `ReceptorDefinition.role: ConceptCcid` + `goal: SemanticFrameSet` statement predicate |
| **Bound / open concept terms** | Subject/object label, `32-hex` `ConceptCcid`, or variable `?slot` | `TermRef::Concept(ConceptCcid)` or alpha-normalized open binder `TermRef::Slot(VarId)` |
| **Exact quantity & unit constraints** | `quantity <op> <decimal_or_rational> <unit_label_or_ccid>` (`=`, `!=`, `<`, `<=`, `>`, `>=`, `BETWEEN`) | Exact rational `(i64, u64)` `QuantityLiteral` (`FND-003`, IEEE-754 `f64` floats rejected!) + SI `UnitRef`/`DimensionVector` inside `ReceptorDefinition.hard_constraints: Vec<TypedConstraint>` |
| **Qualifiers & applicability** | `WHERE condition = ...`, `time = ...`, `location = ...`, `NOT ...` | Three-state `TypedConstraint` (`UnknownConstraintPolicy::{KeepAsUnknown, RejectUnknown}`) |
| **Multi-statement conjunctive pattern** | Up to `MAX_COMPILED_STATEMENTS = 8`, `MAX_OPEN_SLOTS = 8`, `MAX_CONSTRAINTS = 32` | Alpha-normalized `SemanticFrameSet` (`SEMANTIC_PRIMITIVES_V1` §3) |

**Hard Ownership Rejection (`FND-001`, `NEG-KU-003`, `NEG-REC-001`):** Any query predicate referencing runtime, availability, legacy, or economic fields (`pomv`, `trust_score`, `metabolic_rate`, `obt_balance`, `peer_count`, `wire_size`, `encoding_status`, `gene_type` numeric legacy IDs, or `SCOPE GLOBAL`) **MUST** be rejected at compile time with error code `non_semantic_field_forbidden_in_need_ir` rather than hashed or ignored.

### 2.2 Pinned Signed `ConceptRegistry` Resolution (Zero Fabricated CCIDs)

1. Compilation **MUST** execute against one pinned, verified active `ConceptRegistry` generation (`registry_release_root`), recording that root in `PrivateNeedBundle.compiler_provenance`.
2. Every textual concept, predicate, and unit token **MUST** resolve via `ConceptLookup::resolve_checked` to **exactly one** canonical `ConceptCcid` (applying `P1` `ConceptRedirect` canonically; units must match the unit category and host SI transform table).
3. If the Registry is unavailable, a token returns `NotFound`, or a token returns `Ambiguous(candidates)`, compilation **MUST** fail closed (`422 Unprocessable Entity` with `unresolved_terms` + up to 16 candidate `(ConceptCcid, canonical_label)` pairs for explicit user disambiguation). The compiler **MUST NEVER** hash raw strings (`derived_concept` / `blake3(name)`) to invent a `ConceptCcid` (`ADR-KU-046-01`, `ENC §2`).

### 2.3 Canonical Identity Convergence & Privacy-Safe Selector Binding

1. Two queries differing only in whitespace, variable names (`?a` vs `?x`), or commutative constraint order **MUST** compile after `SEMANTIC_PRIMITIVES_V1` §3 alpha-normalization to byte-identical `ReceptorDefinition` (`receptor-definition/1` `ObjectCid`) and `QueryDefinition` (`query-definition/1` `ObjectCid`).
2. **Selector Firewall (`NEG-KQL-001`, `NEG-KQL-005`):**
   - `StandingNeed.selector` **MUST** bind to an allow-listed public `SelectorCid` from the local `PolicyRegistry` (e.g., `public-knowledge-exchange-v1` or a domain-wide public selector) — **never** derived from `NeedPrepareRequest` or private query bytes!
   - Raw query text (if supplied) is zeroized in memory once sealed inside `RedbPrivateNeedVault` (`LocalNeedVaultKey`, `DR-P1.4`).
   - Once compiled under this profile, `PreparedNeedV1.limitations` omits `"semantic_mapping_is_hash_derived_not_compiled"` and reports `compiler_profile = "kql-need-compiler/1"`. Legacy hash-derived bundles from pre-compiler runs remain readable with their explicit limitation badge and **MUST NOT** be silently reinterpreted as compiled semantic needs.

---

## 3. Direct Committed `SemanticFrameSet` ↔ `KnowledgeNeedIr` Matching

To unify local `/ku` search (`ARCH-08`) and typed 3-state matching (`KQL-004`) without requiring separate manual `KnowledgeAffordance` authoring for every saved KU:

1. **Deterministic Virtual Affordance Projection:**
   - Every committed canonical `SemanticFrameSet` KU (`kind=2`, whether `Public` in `ValidatedStore` or caller-authorized `LocalOnly` in `PrivateVault`) projects deterministically through `AI-005` (`RuleBasedAffordanceExtractor` rule version `virtual-sfs-affordance/1`) into an in-memory/indexed `DerivedAffordanceEntry` exposing its predicates as offered roles, bound `QuantityLiteral`/`DimensionVector`s as typed properties, and `StatementQualifier`s as operating conditions.
2. **Two-Phase Local Evaluation (`Recall → Exact 3-State Verify`):**
   - Phase 1 narrows candidate KU/Affordance CIDs via `RebuildableSemanticIndex` (`KQL-002`: predicate/concept/dimension postings) and optional firewalled `AI-002` recall seeds.
   - Phase 2 evaluates `ExactTypedMatcher` (`KQL-004`) — including rational affine unit conversion (e.g. `km → m`, `°C → K`) and 3-state `SATISFIED | VIOLATED | UNKNOWN` qualifier checks — emitting non-executable `BindingProposal`s (`KQL-013`) with explicit `CoverageStatement`s (`local_validated_and_authorized_vault` vs `public_store_only`).

---

## 4. `M6A` Active Distributed KQL Runtime Persistence & Wire Closure

Activating the `ku-kql/experimental` discovery stack on the product network (`M6A.1–M6A.3` in `WIP_DISTRIBUTED_RUNTIME_IMPLEMENTATION_PLAN_V2` §12) requires four normative runtime contracts:

### 4.1 `M6A.1` Live `ProviderLease` / `ProviderRetire` Wire Lane (`obp/provider-directory/1`)
- Adds bounded `VNextMessage` frames over `NET-001` QUIC for publishing, reconciling, and probing signed `ProviderLease` and `ProviderRetire` records (`DHT-001`), persisted in generation-scoped `Redb` with `LeaseObservation.first_seen_monotonic` kept strictly local-only (`NEG-PRO-001`).

### 4.2 `M6A.2` Durable Anti-Replay & Progressive Capsule State (`obp/private-discovery/1`)
1. **Persistent `RouteNeedSketchV1` Replay Guard:** Upgrades `SEC-002` `RoutePacketReplayGuard` to persist `(packet_tag_blake3, expires_at_unix_s)` tombstones in `vnext_distributed_kql.redb` (bounded at `65,536` entries with expiry eviction; fail-closed when full), guaranteeing zero duplicate reply generation across receiver restarts.
2. ** projection Sanitization Gate:** `DisclosureCompiler::compile_route_minimal` consumes a compiled `KnowledgeNeedIr`, walks the active `ConceptRegistry` ontology hierarchy (`SEC-001`) until estimated concept support $\ge 64$ (suppressing overly specific tokens), pads to `512 / 1024 / 2048` bytes, and emits $\le 3$ schema-unlinkable `RouteNeedSketchV1` packets per `QueryRun`.
3. **Durable `DisclosureCapsuleV1` State & Wire Cancellation:** Persists the 4-stage `SEC-003` state machine (`AffordanceSketch → ConstraintSketch → EvidenceReferences → FullNegotiatedPayload`) in `RedbPrivateNeedVault` with ephemeral X25519/AEAD session key zeroization on terminal transition, and adds authenticated frame `CapsuleCancelV1 { session_id: [u8; 32], permit_cid: PermitCid, reason: CancelReason, signer_proof }` so either peer can abort disclosure mid-flight with immediate key zeroization.

### 4.3 `M6A.3` Domain-Separated `CCID → LeaseCID` Lookup & Lease-Bounded `WATCH`
1. Iterative QUIC DHT lookup keys use `DhtKey = BLAKE3("onebrain:vnext:dht-provider-key:1\0" || selector_cid || target_ccid_or_tuple)` returning signed `LeaseCid` references + `ProviderDiscoveryView` (`coverage = Sampled`, bounded $\alpha = 3$, $k = 20$, hot-key per-prefix quotas; `NEG-PRO-002/003`).
2. **Distributed `WATCH` (`obp/selector-watch/1`):** Replaces legacy unbounded textual `WATCH` with a bounded, expiring (`TTL <= 3600 s`), selector-scoped inventory root push hint (`WatchRootHint { selector_cid, new_root }`) over an authenticated session. Private `StandingNeed` predicates **NEVER** leave the watcher (`NEG-KQL-004`); receiving a `WatchRootHint` merely schedules a normal `OBP-RP/1` delta reconciliation followed by local private matching.

---

## 5. Cross-Pillar Firewalls

- **`P3 ↔ P1 / P7`:** No `BindingProposal` from local matching, `RouteNeedSketch` reply, or `DisclosureCapsule` creates a canonical `Mapping` or active `OBKG` edge without explicit `MaterializeMappingCommand` (`KU-006`) and `ADOPT_BINDING` (`KU-007`).
- **`P3 ↔ P4 / P5`:** Query execution, `WatchRootHint`, and `OpenedMultipathReply` record local `ExposureTelemetry` only — **zero** `UseEvidence` (`NEG-POMV-001`) and **zero** `OBT` eligibility or cost (`NEG-KU-003`, `NEG-OBT-001`).
