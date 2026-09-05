# KU runtime and product gap map — KU-REV-002

> Audited source: `80119e1311b1e95171e5613e0335ad3ef69fa2a4`.
> Date: 2026-09-05. Branch: `codex/ku-rev-002-runtime-map`.
> Scope: read-only source review and existing focused tests; no application changes.
> Direction: [authority audit](KU_AUTHORITY_AUDIT.md) and [D-011–D-014](../DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).

## 1. Findings first

1. **The normal encode path is not a working canonical KU creation service.**
   `OneBrainNode::new` opens legacy `KuStorage` read-only. `encode_and_store`
   still calls an Ollama encoder and then `storage.put`, which rejects writes.
   Its later broadcast/verification calls do not establish a working publish
   lane: the primary write fails before that point. The correct next step is
   to connect a canonical service, not remove the legacy fence.
   [node](../../../../src/onebrain-node/src/node.rs) (`new`,
   `encode_and_store_with_progress`), [storage](../../../../src/ku-kql/src/storage.rs)
   (`open_base_read_only`, `put`, `ensure_writable`).
2. **A shared Base facade exists, but generic KU query/mutation adapters are
   missing in the inspected normal hosts.** `compiled_base_runtime_config`
   installs `UnavailableBaseLocalOperationAdapter`. The API host replaces the
   authorizer, not this adapter. `VNextProductServices` has a transitional
   adapter that only answers `vnext.runtime.status.v1` and rejects mutations.
   The Base facade integration tests use echo/test adapters, so passing them
   does not prove KU create/save/revise works.
   [Base runtime](../../../../src/onebrain-node/src/base_runtime.rs),
   [API host](../../../../src/onebrain-api/src/server.rs),
   [transitional adapter](../../../../src/onebrain-node/src/vnext_product_runtime.rs),
   [facade tests](../../../../src/onebrain-node/tests/base_runtime_facade.rs).
3. **D-011 is not met merely by sharing a Registry.** T1 uses a local numeric
   ConceptDict, v2 uses CCIDs but selects the first ambiguous match and creates
   a name-derived fallback, and v2 includes extracted certainty in Core DNA.
   The canonical SemanticFrameSet is a separate CCID-based IR with alpha
   normalization. No inspected normal encode path routes both T1 and AI through
   that common normalized semantic identity boundary.
   [T1](../../../../src/ku-core/src/text_parser.rs),
   [resolver](../../../../src/ku-encoder/src/concept_resolver.rs),
   [builder](../../../../src/ku-encoder/src/builder.rs),
   [semantic IR](../../../../src/ku-core/src/foundation/semantic.rs).
4. **D-012–D-014 have reusable components, but no complete product job/reward
   loop.** Signed Registry activation and reader leases exist; automatic
   publisher/peer acquisition is not wired into the normal node. Blind fidelity
   coordination is in-memory and is not called by the inspected node encode
   service. Wallet values remain an explicit KU-count-based simulation.
   [Registry runtime](../../../../src/onebrain-node/src/concept_registry_runtime.rs),
   [blind workflow](../../../../src/ku-ai/src/vnext_fidelity.rs),
   [node wallet](../../../../src/onebrain-node/src/node.rs) (`get_balance`).

These are implementation gaps, not a reversal of the approved requirements.
In particular D-014 requires direct work-based OBT issuance without a later
BenefitEvent. The old no-mint implementation is recorded as current behavior,
not imposed as the future requirement.

## 2. Evidence labels and search boundary

- **Implemented component:** executable code and focused tests, within the
  stated backend/fixture/feature boundary.
- **Partially wired:** component exists, but the full node/service/surface
  connection or durable lifecycle is missing.
- **Legacy:** compatibility implementation with different semantics.
- **Absent in inspected product path:** no caller/integration found in the
  named node/API/CLI/Web/Desktop modules; not a claim about every possible
  external embedder or plugin.

Search started in `ku-core`, `onebrain-node`, `onebrain-api`, `onebrain-cli`,
`onebrain-web` and `onebrain-desktop`, then followed direct references into
`ku-encoder`, `ku-ai`, `ku-kql`, `ku-net` and associated contract/test files.
No mobile implementation or mobile evidence was changed.

Canonical references used alongside the authority audit:
[validated storage](../../../specs/vnext/VALIDATED_STORAGE_PROFILE_V1.md),
[Mapping materialization](../../../specs/vnext/MAPPING_MATERIALIZATION_PROFILE_V1.md),
[private Need persistence](../../../specs/vnext/PRIVATE_KQL_PERSISTENCE_PROFILE_V1.md),
[runtime ownership](../../../specs/vnext/RUNTIME_OWNERSHIP_PROFILE_V1.md),
[REST](../../../specs/vnext/VNEXT_REST_API_PROFILE_V1.md),
[migration](../../../specs/vnext/ADDITIVE_MIGRATION_STORAGE_PROFILE_V1.md),
[legacy backfill](../../../specs/vnext/LEGACY_DATA_BACKFILL_PROFILE_V1.md),
[typed executor](../../../specs/vnext/TYPED_COGNITIVE_EXECUTOR_PROFILE_V1.md),
[fidelity evidence](../../../specs/vnext/ENCODING_FIDELITY_EVIDENCE_PROFILE_V1.md).

## 3. Operation → owner, store, test and surface

| Operation | Current owner / storage | Actual evidence and product connection | Classification / remaining work |
|---|---|---|---|
| Resolve concepts | [ConceptResolver](../../../../src/ku-encoder/src/concept_resolver.rs), `ConceptLookup`; [T1 ConceptDict](../../../../src/ku-core/src/text_parser.rs) | Resolver tests cover deterministic fallback, lookup failure propagation and CCID survival through wire without receiver Registry. T1 uses a separate numeric vocabulary. | Legacy/partial D-011. Resolve ambiguity and missing concepts under a common explicit profile. |
| Build semantic identity | [SemanticFrameSet](../../../../src/ku-core/src/foundation/semantic.rs), canonical profile; [KuBuilder](../../../../src/ku-encoder/src/builder.rs) | Alpha-renamed variables/statements preserve bytes in semantic unit tests. v2 allocates IDs afresh per triple and sorts its table, but also hashes certainty and uses Core DNA bytes. | Implemented components; no cross-T1/AI common semantic compiler proven. Define identity/version/provenance boundaries before changing bytes. |
| Encode / preview | [node encode](../../../../src/onebrain-node/src/node.rs), [API handler](../../../../src/onebrain-api/src/handlers.rs) `encode_knowledge` | API ignores `body.preview` and invokes encode-and-store under the node mutex with a 300s timeout. Encoder unit tests use controlled inputs/backends, not an operational remote worker. | Legacy, fenced write. No canonical side-effect-free preview/prepare service here. |
| Save validated public KU | [ValidatedStore](../../../../src/ku-core/src/foundation/storage.rs), `RedbVerifiedBackend`; [validated sink](../../../../src/onebrain-node/src/vnext_validated_sink.rs) | Validated bytes/CID/signature dependencies and duplicate acceptance are implemented. Node import uses the sink; old `encode_and_store` uses a different read-only store. | Implemented acceptance component, partially wired creation flow. |
| Save private KU/source | [PrivateVault](../../../../src/ku-core/src/foundation/vault.rs), [source capture transaction](../../../../src/onebrain-node/src/source_capture_transaction.rs) | Encrypted source staging and intent reconciliation have recovery tests. Constructor callers found in recovery tests, not normal encode/API handlers. Those source tests use an in-memory Vault with disk staging. | Components; no production create→canonical+Vault source-binding service demonstrated. Do not call the two stores one transaction. |
| Retrieve / list / text search | [node](../../../../src/onebrain-node/src/node.rs) `get_ku`, `list_kus`, `search_text`, `execute_kql`; [derived index](../../../../src/onebrain-node/src/derived_index.rs) | Legacy list/get/KQL scan `KuStorage`; list uses full scan and page offsets. Canonical accepted-record/derived-index components are separate. API and Web call old KU routes. | Divergent read surfaces; new canonical content cannot be assumed visible in legacy lists. |
| Assembly / Receptor | [foundation Assembly](../../../../src/ku-core/src/foundation/assembly.rs), [Receptor](../../../../src/ku-core/src/foundation/receptor.rs), [LocalVerticalSlice](../../../../src/onebrain-node/src/vnext_local_runtime.rs) | Local slice uses exact lineage/revision/placement and policy. Normal OneBrainNode does not own a LocalVerticalSlice; callers found in local/reunion tests. | Implemented library orchestration; product Assembly/placement management absent in inspected routes. |
| Private Need / discovery | [DistributedKqlRuntime](../../../../src/onebrain-node/src/vnext_distributed_kql.rs), [PrivateNeedVault](../../../../src/ku-kql/src/vnext_private_need.rs), [product services](../../../../src/onebrain-node/src/vnext_product_runtime.rs) | Feature-gated node-owned runtime persists encrypted exact target bundles, rehydrates active needs, keeps terminal tombstones and durable match IDs/cursors. REST/CLI/Web have bounded one-hop Need operations. | Implemented bounded lane, not arbitrary KU creation or full local Assembly editing. |
| Proposal | [ProposalQuarantine](../../../../src/ku-kql/src/vnext_proposal.rs), distributed match index | Matcher inserts non-executable proposals. REST returns quarantined matches. Durable match metadata and rebuilt proposals must be distinguished from a durably stored Mapping pair. | Partial product workflow; cannot auto-save or adopt through list/scan. |
| Materialize Mapping | [MappingMaterializer](../../../../src/ku-core/src/foundation/materialization.rs), `AtomicMappingBackend` | Only `InMemoryMappingBackend` implementation found in Rust sources. Explicit command preflights pair/disclosure/idempotency, with collision and unauthorized tests. | Conformance component. Production encrypted atomic pair+idempotency backend and node adapter absent. |
| Adopt / reopen / revise resolution | [resolution](../../../../src/ku-core/src/foundation/resolution.rs), [local slice](../../../../src/onebrain-node/src/vnext_local_runtime.rs) | Local test materializes while resolution remains Open; separately assessed signed adoption is required. Caller supplies authority result at this library boundary. | Component. Product must resolve authority itself and persist/reduce exact-target events; never accept a caller boolean as authority. |
| KU revision / supersession | [CLI edit](../../../../src/onebrain-cli/src/cli/knowledge.rs) `cmd_edit`; [node versions](../../../../src/onebrain-node/src/node.rs) `get_ku_version_chain` | CLI calls old encode, then explicitly reports `prev_cid` link as STUB. Version lookup walks existing legacy metadata. | Legacy/partial; not an immutable canonical revision workflow. |
| Export / import | [node](../../../../src/onebrain-node/src/node.rs) `export_data`, `import_canonical_exchange`; [exchange codec](../../../../src/onebrain-node/src/canonical_exchange.rs) | Public canonical-v1 bytes are validated; legacy evidence skipped, duplicate imports skipped, dependencies retried. JSON/CSV are views. Text import calls old encode. | Canonical exchange implemented; private archive is separate; text-to-canonical import remains incomplete. |
| Local deletion / retention | [node delete](../../../../src/onebrain-node/src/node.rs), legacy storage fence; canonical archive/retention owners | `delete_ku` reaches read-only legacy store and rejects. Existing retention components are not a new product delete contract. | Define explicit local retention/event behavior; do not reopen legacy writes to make button succeed. |
| Workflow inspection | [workflow surface](../../../../src/onebrain-node/src/vnext_workflow_surface.rs), [CLI workflow](../../../../src/onebrain-cli/src/cli/workflow.rs), API `/api/vnext/workflow` | Three node unit tests and CLI stage-name test; static description, no query/materialize/adopt side effects. | Implemented read-only contract view, not operational workflow. |
| Registry updates | [release package](../../../../src/ku-core/src/concept_registry_release.rs), [generation manager](../../../../src/onebrain-node/src/concept_registry_runtime.rs) | Signed artifacts, activation, old-reader pinning and rollback tests. Normal node owns a startup `Box<dyn ConceptLookup>`, not the refresh manager; manager callers found in tests/qualification example. | Partially wired D-012. Publisher/peer discovery/download, regular update policy and per-run release binding absent in normal encode path. |
| Delegated encode / verify | [typed executor](../../../../src/ku-ai/src/vnext_executor.rs), [blind coordinator](../../../../src/ku-ai/src/vnext_fidelity.rs); [legacy verifier](../../../../src/onebrain-node/src/verifier_service.rs) | Typed executor enforces admitted permit/budget and emits provenance. Blind coordinator and alternate archive are BTreeMaps. Legacy verifier sees original bytes first, invokes encoder v1, compares only first output with agreement ≥0.6. | Components + divergent legacy path. No persisted automatic worker queue integrated into normal product services. |
| Direct work reward | [legacy encoding reward](../../../../src/ku-core/src/encoding_reward.rs), [reward firewall](../../../../src/onebrain-node/src/vnext_reward_firewall.rs), node `get_balance` | Legacy calculation/AccountChain modules exist. Firewall notice kinds are Use/Derivation/Outcome/Benefit, not encode/verify mint authorization. Wallet is `ku_count * 25_000` simulation with staking rejected. | D-014 absent as production work-acceptance→issuance pipeline. Formal amendment, issuance bounds and settlement required; no legacy reward reactivation. |

## 4. Product divergences and evidence limits

### Shared ownership and surfaces

- [API](../../../../src/onebrain-api/src/server.rs) registers both legacy KU
  routes and additive Base/vNext routes. [Web client](../../../../src/onebrain-web/src/api/client.ts)
  calls `/api/encode`, `/api/kus`, `/api/kql` for existing KU pages;
  [Encode page](../../../../src/onebrain-web/src/pages/Encode.tsx) displays the
  returned result directly. It has no canonical prepare/save separation.
- [CLI knowledge commands](../../../../src/onebrain-cli/src/cli/knowledge.rs)
  call OneBrainNode directly; `--draft` saves a draft rather than completing
  canonical encoding. [vNext CLI](../../../../src/onebrain-cli/src/cli/vnext.rs)
  uses authenticated API operations for its separate Need/Public Use lane.
- [Desktop commands](../../../../src/onebrain-desktop/src/commands.rs) return
  API connection configuration and implement explicit shutdown/restart.
  `export_ku_file` and `import_knowledge_file` return not-implemented strings.
  [Tauri config](../../../../src/onebrain-desktop/tauri.conf.json) embeds the Web
  dist, so Web KU semantics carry into Desktop; the IPC stubs are not evidence
  that REST canonical export is also absent.
- Existing Base operations provide durable reserve/prepare/confirm/reconcile
  and host/capability fencing. Reuse that ownership boundary; do not create a
  second unrestricted service beside it. [base_runtime.rs](../../../../src/onebrain-node/src/base_runtime.rs),
  [base_operation_store.rs](../../../../src/onebrain-node/src/base_operation_store.rs).

### Deterministic encoding and fidelity

- v2 builder's fresh per-triple allocator and sorted table are useful existing
  determinism, but its certainty instruction depends on extracted input.
  T1 and AI do not yet share a network-safe normalized semantic IR adapter.
  Equal raw text or equal labels cannot substitute for equal complete
  normalized semantic representation. [builder](../../../../src/ku-encoder/src/builder.rs),
  [analyzer](../../../../src/ku-encoder/src/analyzer.rs),
  [semantic](../../../../src/ku-core/src/foundation/semantic.rs).
- `SemanticFrameSet` retains statement order, source-span qualifiers and source
  units. Generic ObjectCID hashes the full envelope. Do not promise identical
  CID for different disclosure/envelope/profile fields or arbitrary equivalent
  statements. D-011 needs a named supported normalization/equivalence boundary.
  [semantic](../../../../src/ku-core/src/foundation/semantic.rs),
  [object](../../../../src/ku-core/src/foundation/object.rs).
- Legacy `OwnerJobManager` and stigmergy helpers are libraries with in-memory
  state and unit tests. Search found no production caller connecting them to
  the new capability/fidelity/reward workflow. They cannot establish durable
  worker claims, private access, resume or settlement.
  [encoding gossip](../../../../src/ku-net/src/encoding_gossip.rs),
  [stigmergy](../../../../src/ku-net/src/encoding_stigmergy.rs).

### Restart, idempotency and migration

| Boundary | Evidence actually present | Limit |
|---|---|---|
| Base operation lifecycle | `base_runtime_facade` covers child-process crash/reopen and exactly scoped confirmation/reconciliation; `base_operation_store` has durable generation/receipt state. | Test local adapter echoes payload. Generic operation durability is not KU write durability. |
| Public acceptance | Redb validated store and sink preserve accepted bytes, quarantine collisions and defer missing dependencies; canonical exchange codec rejects corruption/duplicates/private records. | No fresh arbitrary text→canonical encode integration test. |
| Source capture | Disk intent/staging reconciliation, wrong-key/orphan handling and recovery tests in `durable_data_recovery`. | Those tests use an in-memory Vault; no normal encode caller found. |
| Local slice | `offline_full_flow_crosses_restart_and_keeps_boundaries_separate` persists StandingNeed, reopens it, then matches/materializes/adopts. | Mapping backend is created in memory after restart. No persistent Mapping reopen proven. |
| Private Need | Encrypted bundle store, rehydration, terminal tombstones and durable match dedup in `vnext_distributed_kql` / `vnext_private_need`. | Feature-gated bounded discovery; quarantined candidate details remain runtime state/rebuilds. |
| Registry | Reader stays pinned; refresh accepts complete newer generation; rollback+reopen preserves roots. | Small signed fixtures and qualification harness are not a deployed update service or a new full-size qualification run. |
| Migration | `vnext_legacy_migration` handles ten data classes and quarantines malformed rows; foundation migration has per-row journal, batch resume, raw rollback and Redb reopen tests. | Library evidence; no migration execution performed by this audit and no invented identity/fidelity/reward authority. |
| Direct OBT work issuance | Legacy wallet test confirms simulation and rejects staking. | No authoritative accepted-work settlement, cross-partition double-mint prevention or live wallet path for D-014. |

## 5. Dependency and risk order for implementation

This order refines the gap map without inventing public contracts or changing
the approved one-task/one-branch merge sequence.

1. **Contract before identity changes:** KU-CON-001 must explicitly cover
   D-011's semantic identity/profile boundary, private provenance, exact Registry
   release binding, unresolved concepts and legacy migration. Preserve the
   existing byte families and acceptance validators.
2. **Canonical local service before interfaces:** adapt the existing Base
   facade to typed KU prepare/preview/save/retrieve/search/revise operations;
   connect canonical store, caller-owned Vault, source capture reconciliation
   and derived indexes. Preserve idempotency/restart and fail-closed authority.
3. **Durable Mapping before adoption UI:** implement the encrypted atomic
   Mapping pair/idempotency backend and authorized exact-target resolution
   events; verify reopen before presenting durable success.
4. **One API contract before CLI/Web/Desktop:** migrate projections onto the
   same node-owned operations. Replace ignored preview, stub revision linkage
   and legacy success wording. Preserve `encode ≠ publish`,
   `proposal ≠ materialize`, `materialize ≠ adopt`.
5. **Explicit Registry distribution specification:** D-012 needs cadence,
   signed release discovery, publisher/peer chunk acquisition, interrupted
   transfer, activation, compatibility and historical reproducibility rules.
   Existing `MIRROR_OR_OFFLINE_ONLY_NO_OBP_GOSSIP` is not permission to flood
   Registry payloads over gossip.
6. **Explicit delegated-work specification:** D-013 needs capability fit,
   source consent/permit, bounded automatic claims, timeout/reassignment,
   private durable attempts, commit-before-reveal, signed acceptance evidence
   and worker restart/outage tests. A missing local AI plus missing reachable
   worker remains pending/unavailable, not success.
7. **Economic amendment and work settlement:** D-014 requires direct issuance
   from accepted work, without a later BenefitEvent. Specify admission and
   supply bounds, rewarded correct mismatch reports, replay/duplicate job
   handling, correlation abuse, disputes and partition-safe settlement. Rewards
   never enter semantic identity or grant knowledge authority. This must be
   scoped as linked specification/implementation work; it does not fit inside
   the existing local-CRUD KU-CON-001 alone. No amount/formula invented here.
8. **Cross-surface and multi-node QA:** exercise matching normalized AI/rule
   drafts, no-local-AI delegation, publisher/peer Registry updates, blind verify,
   reward replay isolation and offline KU usefulness before any rollout change.

D-011–D-014 therefore require additional scoped specification/implementation
work beyond the original 20-task local-KU/OBP package. KU-CON-001 should link
those dependencies rather than imply this audit or a DTO completes them.

## 6. Focused validation

Fresh commands/results and their limits are recorded in the review evidence
in [PROGRESS](../PROGRESS.md). All Cargo commands use `--locked` from `src/`;
the Web test runs from `src/onebrain-web`. Network-feature tests use isolated
local fixtures. No live model service, private user input, external-network
publication, real OBT issuance, mobile build or Desktop GUI session was used.

Recommended later acceptance commands, after their owning changes:

```text
cargo test --locked -p ku-core --features persist --lib foundation::semantic
cargo test --locked -p ku-core --features persist --lib foundation::materialization
cargo test --locked -p ku-core --features persist --lib foundation::vault
cargo test --locked -p ku-core --features persist --lib foundation::migration
cargo test --locked -p ku-encoder --lib
cargo test --locked -p ku-ai --lib vnext_
cargo test --locked -p onebrain-node --lib concept_registry_runtime
cargo test --locked -p onebrain-node --test base_runtime_facade --test durable_data_recovery --test vnext_index_parity
cargo test --locked -p onebrain-node --features vnext-network-runtime --lib vnext_distributed_kql
cargo test --locked -p onebrain-api --features vnext-network-runtime --lib vnext_api
cargo test --locked -p onebrain-cli --bin onebrain
npm run test:vnext
python scripts/ci/validate_vnext_contracts.py
git diff --check
```

Those commands alone do not create missing end-to-end tests: the later task
must add the scoped D-011–D-014 acceptance cases in §5 and show actual eligible
test counts, not merely a successful filter matching zero tests.
