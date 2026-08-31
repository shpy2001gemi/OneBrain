# OneBrain Base v1 Production-Qualified Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Status:** Approved by owner for execution; Base-first ordering is mandatory.

**Goal:** Complete, qualify, and freeze one production-ready `base-v1.0.0` backend before any new Desktop or Mobile product behavior is implemented.

**Architecture:** vNext canonical records remain the only Base v1 write authority. A durable dataset-generation layer owns canonical records, encrypted private state, blobs, journals, and migration state; every KQL, graph, search, or retriever projection is versioned and rebuildable. `OneBrainNode` owns one typed Base service facade. REST, CLI, C ABI, TypeScript, native-host, and Dart files are generated or adapted projections of one machine-readable semantic contract. Concept Registry and real-QUIC/P5 qualification bind the same exact Base candidate, while all distributed lanes remain runtime-default-off.

**Tech Stack:** Rust 2021 workspace, redb 2, BLAKE3, XChaCha20-Poly1305, Argon2id, Ed25519, QUIC, Tokio, Axum, Python 3 contract/qualification tooling, GitHub Actions, TypeScript, C ABI, Dart contract generation.

**Approved design:** [`2026-08-06-onebrain-base-v1-program-design.md`](../specs/2026-08-06-onebrain-base-v1-program-design.md)

## Global Constraints

- `BASE-GATE-V1` blocks new Desktop and Mobile behavior. Generated contract artifacts may be produced before the gate, but product wiring waits until the gate passes.
- vNext object/event/feed bytes are authoritative. Legacy Core DNA/KU data is read-only migration evidence; no new Base v1 command silently writes legacy state.
- The approved recovery choice is the versioned encrypted recovery package. Base v1 does not implement mnemonic derivation.
- Archive password mode is Argon2id v1 (`m=65536 KiB`, `t=3`, `p=1`, 16-byte random salt, 32-byte output). Recovery-key mode uses a separately verified 32-byte key and BLAKE3 `derive_key`; both feed XChaCha20-Poly1305 with distinct domains.
- Node transport, Actor-root, and Feed key domains remain separate. An unavailable non-exportable signer becomes `ReprovisionRequired`; an archive must never claim that it restored such a key.
- Every mutation follows red-green-refactor: add one failing test, run it and observe the expected failure, add the minimum implementation, rerun the focused test, then run the affected crate suite.
- Never treat a specification, target vector, or CI workflow as implementation evidence. Evidence must bind the tested commit, artifact digests, environment, command, result, and limitations.
- Candidate-bound Registry, P5, soak, CI, SBOM, and security evidence lives outside the candidate source tree under `target/base-v1/evidence/` or immutable workflow storage. It may be generated only for the unchanged Task 27 commit; no source commit may contain a report that claims to bind itself.
- Do not weaken an existing frozen vNext invariant to make Base pass. A canonical document conflict is a stop-the-line event and requires owner resolution.
- Do not stage `.superpowers/`. Stage only the explicit paths listed in each task.
- Mobile implementation rules remain in force. If execution changes `onebrain-mobile-core`, `onebrain-mobile-bridge`, native adapters, Flutter, packaging, or mobile evidence, first run `python scripts/ci/validate_mobile_build_contracts.py`, read the complete manifest `required_read_set`, name the active `MOB-00..09` package and affected IDs, update `src/onebrain-mobile/compliance/mobile_build_evidence_v1.json`, and rerun the validator. This plan keeps the pre-gate Dart output under `onebrain-base-contract/generated/` so no product behavior is added.
- Do not create `base-v1.0.0`, publish artifacts, or mark the evidence manifest `qualified` until Task 28 passes every gate.

## File Structure Map

### Existing implementation to reuse

| Concern | Existing authority/implementation |
|---|---|
| Canonical records, validation, public store, Vault, Quarantine | `src/ku-core/src/foundation/{canonical,content_id,storage,vault,event,feed,object}.rs` |
| Legacy KU storage and graph projections | `src/ku-kql/src/{storage,graph_storage}.rs` |
| Blob CID and hybrid store | `src/ku-core/src/blob_store.rs`, `src/ku-kql/src/blob_storage.rs` |
| Retriever projection | `src/ku-mediator/src/retriever.rs` |
| Node composition and vNext facade | `src/onebrain-node/src/{node,vnext_product_runtime,vnext_local_runtime,vnext_legacy_migration}.rs` |
| Existing encrypted stream archive | `src/onebrain-mobile-core/src/archive.rs` |
| P5 offline manifest and preflight | `src/onebrain-node/src/vnext_p5_operations.rs` |
| Registry release/runtime/qualification | `src/ku-core/src/{concept_registry_release,indexed_concept_registry}.rs`, `src/onebrain-node/src/concept_registry_runtime.rs`, `scripts/concept_registry/` |
| REST/WS and CLI projections | `src/onebrain-api/src/{server,vnext_api,vnext_ws}.rs`, `src/onebrain-cli/src/{main,cli/vnext}.rs` |
| Contract validator and CI | `scripts/ci/validate_vnext_contracts.py`, `.github/workflows/vnext-foundation.yml` |

### New focused modules and artifacts

```text
docs/specs/vnext/
  BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE.md
  BASE_V1_STORAGE_INTEGRITY_PROFILE.md
  BASE_V1_RUNTIME_INTERFACE_PROFILE.md
  CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md
  P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md
  BASE_V1_FREEZE_AND_EVIDENCE_PROFILE.md
src/test-vectors/vnext/
  base-v1-authority-recovery-v1.json
  base-v1-storage-integrity-v1.json
  base-v1-derived-projection-v1.json
  base-v1-runtime-interface-v1.json
  base-v1-runtime-interface-history-v1.json
  base-v1-archive-v1.json
  concept-registry-production-qualification-v1.json
  p5-multi-host-production-qualification-v1.json
  base-v1-freeze-v1.json
src/onebrain-archive/
  Cargo.toml
  src/{lib,container,crypto,dataset,limits,manifest,restore,verified}.rs
  tests/{container_vectors,dataset_roundtrip,materialization_failpoints}.rs
src/onebrain-base-contract/
  Cargo.toml
  build.rs
  src/{lib,compatibility,generated,negotiation,operation}.rs
  generated/{typescript/base_v1.ts,dart/base_v1.dart}
  conformance/typescript/{package.json,package-lock.json,tsconfig.json,test_base_v1.ts}
  conformance/dart/{pubspec.yaml,pubspec.lock,test/base_v1_test.dart}
  tests/{generation_drift,negotiation_vectors,projection_conformance}.rs
src/onebrain-base-abi/
  Cargo.toml
  cbindgen.toml
  include/onebrain_base_v1.h
  src/lib.rs
src/ku-kql/src/
  blob_layout.rs
  derived_index.rs
src/onebrain-node/src/
  activation_journal.rs
  archive.rs
  archive_capabilities.rs
  base_operation_store.rs
  base_runtime.rs
  blob_authority.rs
  canonical_exchange.rs
  dataset_generation.rs
  derived_index.rs
  derived_projection.rs
  identity_recovery.rs
  signer_ports.rs
  source_capture_transaction.rs
  text.rs
scripts/base/
  generate_contract.py
  qualify_base.py
  test_generate_contract.py
  test_qualify_base.py
scripts/concept_registry/
  production_qualification.py
  test_production_qualification.py
scripts/runner/
  onebrain-p5-multi-host.py
  test_onebrain_p5_multi_host.py
scripts/toolchains/
  base-v1-tools.lock.json
docs/operations/
  ONEBRAIN_BASE_V1_ARCHIVE_RESTORE_GUIDE.md
  ONEBRAIN_BASE_V1_P5_MULTI_HOST_GUIDE.md
target/base-v1/evidence/                 # immutable workflow/release artifacts; not source input
  manifest.json
  qualification/{registry,p5,soak}/...
```

IDL-generated Rust/TypeScript/Dart files carry this first line and are never edited by hand; the Task 18 C header instead carries cbindgen provenance and is owned only by the ABI crate:

```text
// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
```

## Git, Ownership, and Parallel Execution

| Branch | Tasks | May start after | High-conflict files owned |
|---|---:|---|---|
| `codex/base-v1-authority` | 1-3 | approved program design | vNext profiles, machine contracts, traceability |
| `codex/base-v1-storage` | 4-8 | Task 3 | `ku-kql` storage/blob modules, retriever |
| `codex/base-v1-archive` | 9-13 | Tasks 1 and 3; Task 10 waits for Task 7; Task 11 also waits for Task 8 | `onebrain-archive`, Node archive/recovery modules |
| `codex/base-v1-contract` | 14-18 | Task 1; Task 15 waits for the pinned Task 14 baseline receipt; Task 16 waits for Tasks 7, 11-13, and 19; Task 17 waits for Task 16 | Base contract/ABI and facade modules |
| `codex/base-v1-registry` | 19-21 | Task 1; may run beside storage | Registry profile and qualification scripts |
| `codex/base-v1-p5` | 22-24 | Task 22 after Task 1; Task 23 waits for Tasks 17 and 21; Task 24 waits for Task 23; exact production run waits for Task 27 | multi-host runner and P5 workflow preparation |
| `codex/base-v1-freeze` | 25-28 | Tasks 8, 13, 18, 21, and 24 | workspace manifests, `Cargo.lock`, CI, evidence, tag |

One integration owner serializes edits to `src/Cargo.toml`, `src/Cargo.lock`, `src/onebrain-node/src/lib.rs`, `src/onebrain-node/src/node.rs`, `scripts/ci/validate_vnext_contracts.py`, and `.github/workflows/vnext-foundation.yml`. Workstream branches keep logic in focused modules and make export/delegation changes in a final, small commit.

Execution waves:

1. **Wave A — Authority:** Tasks 1-3, serial.
2. **Wave B — Durable substrate:** Tasks 4-8; Tasks 9 and 19 may proceed in parallel, but Task 10 starts only after Task 7 freezes the validated-store/Vault snapshot ports.
3. **Wave C — Recovery and interface:** Tasks 11-18; Registry Tasks 20-21 continue independently.
4. **Wave D — Qualification harness preparation:** Tasks 22-24 after the facade and Registry harness contracts are fixed; dry-runs remain explicitly unqualified.
5. **Wave E — Integration/freeze:** Tasks 25-27 create successive integration commits; Task 27 alone yields the exact candidate SHA, and Task 28 only generates external evidence and tags that unchanged SHA.

## Phase 0 — Authority and Frozen Machine Contracts

### Task 1: Freeze Base authority, recovery, archive, Registry, network, and delete semantics

**Branch:** `codex/base-v1-authority`

**Files:**

- Create: `docs/specs/vnext/BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE.md`
- Create: `src/test-vectors/vnext/base-v1-authority-recovery-v1.json`
- Create: `scripts/ci/test_validate_vnext_base_authority.py`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `docs/specs/vnext/TRACEABILITY_MATRIX_V1.md`
- Modify: `docs/specs/vnext/README.md`

**Frozen contract excerpt:**

```json
{
  "format": "onebrain/base-v1-authority-recovery/1",
  "canonical_write_path": "vnext-object-event-feed",
  "legacy_boundary": "explicit-read-only-migration",
  "recovery_profile": "encrypted-recovery-package-v1",
  "archive_profiles": ["password-argon2id-v1", "recovery-key-v1"],
  "registry_required_states": ["registry-dependent-encoding", "ready-offline"],
  "network_default_active_lane_count": 0,
  "delete_semantics": "event-or-local-retention-never-history-rewrite"
}
```

- [ ] Add tests that mutate each field above and assert `ContractError`; also assert Node/Actor/Feed recovery domains are distinct and non-exportable signers require reprovisioning.
- [ ] Run `python -m unittest scripts.ci.test_validate_vnext_base_authority -v`. Expected: failure because `validate_base_v1_authority_recovery` and its contract do not exist.
- [ ] Add `BASE_V1_AUTHORITY_RECOVERY_PROFILE` and `validate_base_v1_authority_recovery()` to `validate_vnext_contracts.py`; require exact KDF parameters, XChaCha20-Poly1305, domain strings, archive scope, Registry fail-closed states, zero active default lanes, and immutable-history delete semantics.
- [ ] Write the focused profile with explicit authority order: distributed-runtime plan, mobile architecture constraints, this Base profile, then product projections. Record that approval selected encrypted recovery packages and rejected the mnemonic alternative.
- [ ] Add the machine contract to `normative_coverage.json`, fix the `FND-010` traceability/status drift, and link the profile from the vNext README.
- [ ] Run `python -m unittest scripts.ci.test_validate_vnext_base_authority -v`. Expected: all tests `OK`.
- [ ] Run `python scripts/ci/validate_vnext_contracts.py`. Expected: `PASS` with the Base authority profile counted.
- [ ] Commit:

```powershell
git add docs/specs/vnext/BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE.md docs/specs/vnext/normative_coverage.json docs/specs/vnext/TRACEABILITY_MATRIX_V1.md docs/specs/vnext/README.md src/test-vectors/vnext/base-v1-authority-recovery-v1.json scripts/ci/test_validate_vnext_base_authority.py scripts/ci/validate_vnext_contracts.py
git commit -m "docs(base): freeze v1 authority and recovery profile"
```

### Task 2: Freeze the storage, blob, index, import/export, and crash contract

**Branch:** `codex/base-v1-authority`

**Files:**

- Create: `docs/specs/vnext/BASE_V1_STORAGE_INTEGRITY_PROFILE.md`
- Create: `src/test-vectors/vnext/base-v1-storage-integrity-v1.json`
- Create: `src/test-vectors/vnext/base-v1-derived-projection-v1.json`
- Create: `scripts/ci/test_validate_vnext_base_storage.py`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md`

**Required machine fields:** full 68-hex CID path, two digest-byte shards, chunk and full-payload hashes, declared type/length, per-object/total/free-space limits, journaled filesystem commit, source-root/index-root binding, rebuild-on-corruption, update/delete parity, character-safe preview, exact canonical import/export, the frozen five-phase failpoint vocabulary, and child-process kill/reopen oracles. The derived-projection vector freezes each accepted vNext object/event kind, reducer version, graph/search key/output, branch/tombstone handling, exclusion rule, and projection-root domain. The storage profile also freezes `OwnedBlobReferenceV1` as the only Base blob-reference authority, including owner `ObjectReference`, role, retention state, and terminal-event semantics.

**Closed storage/archive owner IDs:** `0x0001 canonical`, `0x0002 vault`, `0x0003 quarantine`, `0x0004 blob`, `0x0005 pending_blob_intent`, `0x0006 source_capture_intent`, `0x0007 reconciliation`, `0x0008 inventory`, `0x0009 outbox`, `0x000A provenance`, `0x000B private_kql`, `0x000C private_pomv`, `0x000D operational`, `0x000E rollout`, `0x000F optional_network`, `0x0010 migration`, `0x0011 base_operations`, `0x0012 interpretation_config`, `0x0013 identity`, `0x0014 registry_metadata`, `0x0015 derived_index`, and `0x0016 retriever_projection`. `0x0000` and `0x0017..0xFFFF` are reserved in Base v1. `BaseStorageOwnerId` and `ArchiveOwner` encode the identical big-endian `u16`; the Node adapter performs the only one-to-one conversion, and unknown/reserved/reused IDs fail closed. Projection-owner paths are generation-owned but their disposable bytes remain explicitly excluded from archive manifests.

- [ ] Add negative tests for a short path, missing full-read hash, missing total quota, graph “best effort” without a dirty generation, an unknown/vacuous derived projection mapping, a blob reference sourced from legacy KU metadata, a corrupt retriever fatal startup, absent update/delete parity, and byte-sliced UTF-8.
- [ ] Run `python -m unittest scripts.ci.test_validate_vnext_base_storage -v`. Expected: failure because the profile is absent.
- [ ] Add owner-table vectors for every exact code and reject a missing, duplicate, reused, reserved, endian-swapped, or non-bijective `BaseStorageOwnerId`/`ArchiveOwner` mapping.
- [ ] Implement `validate_base_v1_storage_integrity()` and make its return value report the number of authoritative boundaries and negative oracles.
- [ ] Write the focused storage profile and derived-projection vector. Classify redb secondary tables as same-transaction indexes and graph/retriever/search as disposable generation-swapped projections; bind every projection row to a frozen mapping/reducer ID rather than accepting root equality alone.
- [ ] Add `TX-BLOB-001`, `TX-IDX-001`, `TX-ARCH-001`, `TX-RESTORE-001`, and `TX-RECOVERY-001` to the transaction inventory using exactly `before_begin_write`, `after_begin_write_before_mutation`, `after_mutation_before_commit`, `after_commit_before_next_side_effect`, and `after_next_side_effect_before_ack`; add child-process reopen as an oracle, not as a sixth phase or renamed vocabulary.
- [ ] Run the focused test and `python scripts/ci/validate_vnext_contracts.py`; expect both to pass.
- [ ] Commit:

```powershell
git add docs/specs/vnext/BASE_V1_STORAGE_INTEGRITY_PROFILE.md docs/specs/vnext/normative_coverage.json docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md src/test-vectors/vnext/base-v1-storage-integrity-v1.json src/test-vectors/vnext/base-v1-derived-projection-v1.json scripts/ci/test_validate_vnext_base_storage.py scripts/ci/validate_vnext_contracts.py
git commit -m "docs(base): freeze storage integrity contract"
```

### Task 3: Add a machine-readable Base profile registry and cross-crate conformance gate

**Branch:** `codex/base-v1-authority`

**Files:**

- Create: `src/ku-core/src/foundation/base_profile.rs`
- Create: `src/ku-core/tests/base_profile_conformance.rs`
- Modify: `src/ku-core/src/foundation/mod.rs`
- Modify: `src/ku-core/examples/foundation_vector_digests.rs`
- Modify: `.github/workflows/vnext-foundation.yml`

**Interface:**

```rust
pub struct BaseProfileRegistry {
    pub profile_major: u16,
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_owner_registry_digest: [u8; 32],
}

pub fn base_v1_profile_registry() -> BaseProfileRegistry;
pub fn base_v1_profile_digest() -> [u8; 32];
```

- [ ] Add `base_profile_conformance.rs` that loads the two Base machine contracts, recomputes sorted canonical digests including the closed storage/archive owner table, and asserts the Rust registry matches them.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p ku-core --test base_profile_conformance`. Expected: compile failure because `base_profile` is missing.
- [ ] Implement `base_profile.rs` using typed constants from `schema_registry.rs` and domain/resource profiles; no runtime JSON parsing in the canonical library.
- [ ] Export the module and extend `foundation_vector_digests` with `base_profile_digest`.
- [ ] Run the focused test, then `cargo test --locked --manifest-path src/Cargo.toml -p ku-core foundation` and `cargo check --locked --manifest-path src/Cargo.toml -p ku-core -p ku-kql -p ku-net -p onebrain-node`.
- [ ] Add the focused test to `vnext-foundation.yml` and run `python scripts/ci/validate_vnext_contracts.py` to prove the workflow reference is frozen.
- [ ] Commit:

```powershell
git add src/ku-core/src/foundation/base_profile.rs src/ku-core/tests/base_profile_conformance.rs src/ku-core/src/foundation/mod.rs src/ku-core/examples/foundation_vector_digests.rs .github/workflows/vnext-foundation.yml
git commit -m "feat(base): add frozen profile registry"
```

## Phase 1 — Durable Storage, Blob, and Derived Projections

### Task 4: Replace short blob directories with a full-CID v2 layout and collision-blocking migration

**Branch:** `codex/base-v1-storage`

**Files:**

- Create: `src/ku-kql/src/blob_layout.rs`
- Create: `src/ku-kql/tests/blob_layout_migration.rs`
- Modify: `src/ku-kql/src/lib.rs`
- Modify: `src/ku-kql/src/blob_storage.rs`

**Interface:**

```rust
pub const BLOB_LAYOUT_VERSION: u16 = 2;

pub struct BlobLayoutMigrationReport {
    pub migrated: u64,
    pub already_v2: u64,
    pub collision_groups: Vec<String>,
    pub corrupt_cids: Vec<BlobCid>,
}

pub fn blob_relative_dir(cid: &BlobCid) -> PathBuf;
pub fn migrate_blob_layout_v2(root: &Path, metas: &[BlobMeta])
    -> Result<BlobLayoutMigrationReport, BlobStorageError>;
```

`blob_relative_dir()` returns `v2/<hash[0..2]>/<hash[2..4]>/<full-68-hex-cid>`; the two version/type bytes are not used for sharding.

- [ ] Add a test with two `BlobCid` values whose `short_hex()` is identical but whose full digests differ; assert distinct paths containing both complete 68-character CIDs.
- [ ] Add migration tests for one valid v1 directory, an already-v2 blob, a prefix-collision group, corrupt chunks, an interrupted `.migrating` directory, and idempotent rerun.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p ku-kql --features storage --test blob_layout_migration`. Expected: compile failure because `blob_layout` does not exist.
- [ ] Implement path validation and a staged `v1 -> .migrating -> v2` move. Reassemble and hash old chunks before moving. If two metadata records claim one v1 path or any CID fails validation, return the report with `MigrationBlocked` and do not delete or overwrite v1 data.
- [ ] Route filesystem spill reads/writes through `blob_relative_dir`; keep `BlobCid::short_hex()` display-only.
- [ ] Run the focused test and `cargo test --locked --manifest-path src/Cargo.toml -p ku-kql --features storage blob_storage -- --test-threads=1`.
- [ ] Commit:

```powershell
git add src/ku-kql/src/blob_layout.rs src/ku-kql/tests/blob_layout_migration.rs src/ku-kql/src/lib.rs src/ku-kql/src/blob_storage.rs
git commit -m "fix(storage): migrate blobs to full cid paths"
```

### Task 5: Enforce blob integrity, total quota, free-space admission, and crash recovery

**Branch:** `codex/base-v1-storage`

**Files:**

- Create: `src/ku-kql/tests/blob_integrity.rs`
- Create: `src/ku-kql/tests/blob_metadata_migration.rs`
- Create: `src/ku-core/src/foundation/blob_reference.rs`
- Create: `src/onebrain-node/src/blob_authority.rs`
- Create: `src/onebrain-node/src/dataset_path.rs`
- Create: `src/onebrain-node/tests/blob_upload_gc_race.rs`
- Modify: `src/ku-core/src/blob_store.rs`
- Modify: `src/ku-core/src/foundation/mod.rs`
- Modify: `src/ku-core/src/foundation/schema_registry.rs`
- Modify: `src/ku-kql/src/blob_storage.rs`
- Modify: `src/ku-kql/Cargo.toml`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md`
- Modify: `src/Cargo.lock`

**Interface:**

```rust
pub struct BlobStorageConfig {
    pub total_quota_bytes: u64,
    pub free_space_reserve_bytes: u64,
}

pub enum BlobReadError {
    LengthMismatch,
    ChunkDigestMismatch { index: u32 },
    ContentDigestMismatch,
    TypeMismatch,
}

pub fn open_with_config(
    path: &Path,
    config: BlobStorageConfig,
    references: Arc<dyn BlobReferenceOracle>,
)
    -> Result<BlobStorage, BlobStorageError>;
pub fn recover_pending_filesystem_intents(&self) -> Result<u64, BlobStorageError>;
pub fn migrate_blob_metadata_v2(&self) -> Result<BlobMetadataMigrationReport, BlobStorageError>;

pub trait BlobReferenceOracle {
    fn referencing_records(&self, cid: &BlobCid)
        -> Result<Vec<ObjectReference>, BlobStorageError>;
}

pub struct PendingBlobUploadId([u8; 32]);
pub struct DatasetGenerationId([u8; 32]);
pub struct BaseStorageOwnerId(u16); // closed owner table frozen by Task 2
pub trait DatasetPathResolver: Send + Sync {
    fn current_generation(&self) -> DatasetGenerationId;
    fn owner_path(&self, owner: BaseStorageOwnerId) -> Result<PathBuf, BlobStorageError>;
}

pub struct PendingOwnedBlobUpload {
    pub id: PendingBlobUploadId,
    pub intended_owner: ObjectReference,
    pub expected_blob: BlobCid,
    pub expected_type: BlobType,
    pub expected_length: u64,
    pub dataset_generation: DatasetGenerationId,
}
```

- [ ] Extend `BlobMeta` with `meta_version: u16` and `chunk_blake3: Vec<String>` using decode-only backward-compatible serde defaults; missing digests are typed `MigrationRequired`, never trusted as verified metadata. Add failing metadata round-trip and legacy migration tests.
- [ ] Add tests for exact 68-character CID parsing, corrupt inline chunk, corrupt spilled chunk, truncated/extended payload, declared type mismatch, total quota across two blobs, `BLOB_MAX_PER_KU`, reserve-space rejection through an injected `AvailableSpace` test port, interrupted stage/delete/activation, reference-parity mismatch, a forged/legacy blob reference, and idempotent recovery.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p ku-kql --features storage --test blob_integrity`. Expected: compile failure for `BlobStorageConfig` and failing corruption behavior.
- [ ] Add `fs2` for production free-space readings and a private injectable port for tests. Admission uses checked addition/subtraction for `current_owned_bytes + incoming_unique_bytes <= total_quota_bytes` and `available_bytes - incoming_unique_bytes >= reserve_bytes`; overflow/underflow rejects admission, and deduplicated bytes do not consume quota twice.
- [ ] Add a workspace-pinned, non-optional OS CSPRNG dependency to `onebrain-node`; local/default Base uses it for pending-upload IDs and later archive/process capability IDs even when network features are absent. Entropy failure is typed and leaves no intent/blob; deterministic collision injection is rejected without overwriting an existing ID.
- [ ] Freeze `DatasetGenerationId` and the minimal owner-scoped `DatasetPathResolver` port in `dataset_path.rs`. Task 5 supplies an owner-validating bootstrap resolver for the current Base-owned root so Tasks 5-10 compile without anticipating activation; Task 11 replaces that bootstrap composition with the dual-slot generation store without changing the port or IDs.
- [ ] Implement an idempotent metadata-v2 migration that opens the metadata database, handles inline and spilled blobs, reassembles bytes, verifies exact CID/type/length/full digest, computes every chunk BLAKE3, and commits `meta_version` plus digests atomically. Corrupt or ambiguous legacy metadata blocks migration while preserving the original record and bytes.
- [ ] Reject non-exact CID strings. Inspect file metadata and enforce `BLOB_MAX_SIZE` before allocating; stream hashing/chunking rather than reading an oversized file into memory.
- [ ] Add a redb `blob_fs_intents` table. Filesystem writes/deletes go through journaled staging, file/directory sync, atomic rename, and completion. Startup deterministically completes or rolls back incomplete states.
- [ ] Make `get_chunk()` verify its stored chunk digest. Make `read_full_blob()` and `export_to_file()` verify chunk digests, exact total length, full BLAKE3, Blob CID version/type, and expected type before returning or publishing bytes.
- [ ] Implement `OwnedBlobReferenceV1` from the Task 2 schema and a `CanonicalBlobReferenceOracle` that scans only validated vNext object/event bytes, recomputes their CIDs, applies frozen terminal/retention reducers, and returns full `ObjectReference` owners. Inject this oracle at Node composition; legacy KU blob metadata remains migration evidence and never authorizes Base retention or deletion.
- [ ] Add a bounded durable pending-upload intent beneath the current `DatasetPathResolver` before accepting blob bytes. The intent binds a cryptographically random ID, exact future canonical `ObjectReference`, expected blob CID/type/length, and dataset generation; it protects only that CID from GC until the matching owner record commits, explicit abort completes, or deterministic reopen reconciliation proves the owner can no longer commit. Do not use wall-clock expiry as the sole deletion authority.
- [ ] Add `TX-BLOB-UPLOAD-001` to the frozen transaction inventory with exactly `before_begin_write`, `after_begin_write_before_mutation`, `after_mutation_before_commit`, `after_commit_before_next_side_effect`, and `after_next_side_effect_before_ack`. Child-process tests kill at every phase and cover upload-versus-GC, owner-event rejection, abort, duplicate intent, wrong owner/CID, and reopen; every outcome is either an owned live blob or a reconciled orphan that becomes collectable, never premature deletion.
- [ ] Fence legacy `add_ku_reference`, `remove_ku_reference`, and `set_pinned` metadata from Base admission and GC authority. Compatibility import may record them only as untrusted migration evidence; it cannot create or extend a Base retention lease.
- [ ] Reconcile mutable reference metadata through the injected `BlobReferenceOracle`; GC and destructive delete fail closed while canonical reference parity is dirty or unknown. Deduplicate/sort references and enforce the per-owner blob bound.
- [ ] Run the focused tests, `cargo test --locked --manifest-path src/Cargo.toml -p ku-core blob_store`, and `cargo test --locked --manifest-path src/Cargo.toml -p ku-kql --features storage blob_storage -- --test-threads=1`.
- [ ] Commit:

```powershell
git add src/ku-core/src/blob_store.rs src/ku-core/src/foundation/blob_reference.rs src/ku-core/src/foundation/mod.rs src/ku-core/src/foundation/schema_registry.rs src/ku-kql/src/blob_storage.rs src/ku-kql/tests/blob_integrity.rs src/ku-kql/tests/blob_metadata_migration.rs src/ku-kql/Cargo.toml src/onebrain-node/src/blob_authority.rs src/onebrain-node/src/dataset_path.rs src/onebrain-node/tests/blob_upload_gc_race.rs src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-node/Cargo.toml docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md src/Cargo.lock
git commit -m "fix(storage): make blob commits bounded and verifiable"
```

### Task 6: Keep legacy KU read-only and make vNext-derived indexes parity-checked and rebuildable

**Branch:** `codex/base-v1-storage`

**Files:**

- Create: `src/onebrain-node/src/derived_index.rs`
- Create: `src/onebrain-node/tests/vnext_index_parity.rs`
- Modify: `src/ku-core/src/foundation/storage.rs`
- Modify: `src/onebrain-node/src/vnext_validated_sink.rs`
- Modify: `src/onebrain-node/src/vnext_legacy_migration.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/ku-kql/src/storage.rs`
- Modify: `src/onebrain-node/src/lib.rs`

**Interface:**

```rust
pub const VNEXT_DERIVED_INDEX_PROFILE: &str = "onebrain/base-derived-index/1";

pub struct VNextIndexParityReport {
    pub source_root: [u8; 32],
    pub secondary_root: [u8; 32],
    pub graph_root: [u8; 32],
    pub accepted_record_count: u64,
    pub mismatch_count: u64,
}

pub enum DerivedIndexOpenState { Ready, Rebuilt, Degraded }

impl VNextDerivedIndexManager {
    pub fn verify_parity(&self, source: &dyn AcceptedRecordScan)
        -> Result<VNextIndexParityReport, DerivedIndexError>;
    pub fn rebuild(&self, source: &dyn AcceptedRecordScan)
        -> Result<VNextIndexParityReport, DerivedIndexError>;
}
```

- [ ] Add a Base-mode test proving every `KuStorage::put/update_epi/delete` entry point is unreachable or returns `LegacyReadOnly`; only `vnext_legacy_migration` may scan legacy primary rows as migration evidence. A canonical terminal event drives Base delete/retention semantics; it never mutates legacy authority.
- [ ] Load the frozen Task 2 projection vector in tests. Prove the same vNext accepted-record transaction owns mandatory feed/authority lookup rows and that every derived graph/search row names the exact accepted-record source root plus mapping/reducer digest. Reject an unknown mapping, missing expected row, unexpected extra row, or vacuous empty projection. No derived row may become write authority.
- [ ] Add tests that delete, truncate, or corrupt a derived generation, reopen, rebuild exclusively from validated vNext object/event/feed bytes, and reproduce the same projection root. Include competing branches and tombstone/retention events so rebuild cannot select a winner or resurrect retired state.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test vnext_index_parity`. Expected: compile failures for the manager and Base legacy-write fence.
- [ ] Extend the accepted-record scan to recompute each canonical CID and fail closed or deterministically quarantine corruption before a row influences any source root. Same-transaction mandatory indexes must delete/replace stale keys atomically when their canonical transaction semantics require it.
- [ ] Implement only the Task 2 object/event→graph/search mappings in a generation manager that builds `derived/<profile>/<mapping-digest>/<source-root>/...`, verifies row coverage and the projection root, atomically swaps a small generation pointer, and retires the old generation only after reader leases drain. Publication failure leaves canonical reads available with `DerivedIndexOpenState::Degraded`.
- [ ] Keep `ku-kql` storage and graph APIs available only to the explicit read-only legacy migration adapter; do not call them from Base command, query, graph, retriever, or facade paths.
- [ ] Run the focused suite, vNext validated-store/feed tests, migration tests, and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib vnext_validated_sink -- --test-threads=1`.
- [ ] Commit:

```powershell
git add src/ku-core/src/foundation/storage.rs src/onebrain-node/src/derived_index.rs src/onebrain-node/tests/vnext_index_parity.rs src/onebrain-node/src/vnext_validated_sink.rs src/onebrain-node/src/vnext_legacy_migration.rs src/onebrain-node/src/node.rs src/ku-kql/src/storage.rs src/onebrain-node/src/lib.rs
git commit -m "fix(storage): bind derived indexes to canonical roots"
```

### Task 7: Persist retriever sources, atomically snapshot the projection, and self-heal startup

**Branch:** `codex/base-v1-storage`

**Files:**

- Create: `src/onebrain-node/src/derived_projection.rs`
- Create: `src/onebrain-node/src/source_capture_transaction.rs`
- Create: `src/onebrain-node/tests/durable_data_recovery.rs`
- Create: `src/ku-core/src/foundation/source_text.rs`
- Modify: `src/ku-core/src/foundation/mod.rs`
- Modify: `src/ku-core/src/foundation/schema_registry.rs`
- Modify: `src/ku-core/src/foundation/vault.rs`
- Modify: `src/ku-mediator/src/retriever.rs`
- Modify: `src/ku-mediator/src/mediator.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md`

**Source ownership decision:** exact local source text is durable private source data stored through the Vault and keyed by a validated vNext `ObjectReference`. The token index is derived. A legacy KU may contribute only read-only migration evidence; when its original source was not retained, the migration record carries `legacy_source_text_unavailable` and reconstructed prose is never presented as original text.

**Interface:**

```rust
pub const RETRIEVER_INDEX_PROFILE: &str = "onebrain/retriever-index/2";

pub struct RetrieverSourceRecord {
    pub subject: ObjectReference,
    pub source_text: String,
    pub source_digest: [u8; 32],
}

pub struct RetrieverIndexEnvelope {
    pub profile: String,
    pub source_root: [u8; 32],
    pub entries: Vec<RetrieverIndexEntryV1>,
}

pub struct RetrieverIndexEntryV1 {
    pub subject: ObjectReference,
    pub source_record: ObjectCid,
    pub source_digest: [u8; 32],
    pub expression: BoundedUtf8,
}

pub enum SourceCaptureRecoveryState {
    Complete,
    FinishVaultBinding,
    QuarantineOrphanSource,
    SourceCaptureIncomplete,
}

pub struct VaultStagingId([u8; 32]);
pub struct EncryptedSourceCaptureIntentV1 {
    pub subject: ObjectReference,
    pub source_digest: [u8; 32],
    pub vault_staging_id: VaultStagingId,
    pub dataset_generation: DatasetGenerationId,
    /* authenticated metadata only; exact source bytes remain Vault-key encrypted */
}

impl KuRetriever {
    pub fn upsert_source(&mut self, subject: ObjectReference, expression: String);
    pub fn remove_source(&mut self, subject: &ObjectReference) -> bool;
    pub fn save_atomic(&self, path: &Path, source_root: [u8; 32]) -> Result<(), RetrieverError>;
    pub fn load_envelope(path: &Path) -> Result<RetrieverIndexEnvelope, RetrieverError>;
}
```

- [ ] Add mediator unit tests proving upsert does not duplicate an `ObjectReference`, remove clears it, envelopes reject unknown versions/duplicate subjects/root mismatch, and a failed temp write preserves the previous complete file.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p ku-mediator retriever::tests`. Expected: compile failures for `upsert_source`, `remove_source`, and `save_atomic`.
- [ ] Implement `LocalSourceTextRecordV1` as a bounded, typed, private-only record whose subject is a validated vNext object reference and whose digest binds exact UTF-8 bytes. Persist it through the encrypted Vault during successful canonical encode/import before acknowledging retriever availability; legacy migration uses a separate limitation-bearing record.
- [ ] Add a durable Node-owned `TX-SOURCE-001` intent beneath the current `DatasetPathResolver`. It binds canonical object/event digest, exact source digest, Vault-encrypted staging ID, target Vault record, and dataset generation before either store mutates; exact plaintext exists only in a zeroizing buffer and Vault-key-encrypted staging, never in the journal. Authenticate intent metadata, use exactly the frozen five phases from Task 2, and add child-process reopen, wrong-key/ciphertext-tamper, cleanup, and plaintext-zeroization tests for every phase; do not pretend the canonical store and Vault share one physical transaction.
- [ ] Reconcile the source intent deterministically on startup: finish a proven Vault binding, quarantine an unreferenced Vault source, or expose typed `SourceCaptureIncomplete` when canonical acceptance is durable but exact source cannot be proven. Never invent/reconstruct prose as original source, and never acknowledge retriever availability until canonical acceptance plus the Vault binding are durable.
- [ ] Add deterministic Vault enumeration for source-text records and a `vault_source_root`; Task 10 may generalize this port for archives. Key the retriever map by the complete `ObjectReference` and bind its source root as `H(profile_digest, accepted_vNext_root, vault_source_root)`.
- [ ] Replace the retriever `Vec` with a CID-keyed deterministic map, implement the versioned envelope, and save through temp file, flush, file sync, rename, and parent-directory sync.
- [ ] Add a Node integration test: create validated vNext object/event/feed rows plus Vault source records, write truncated retriever JSON, restart, and assert startup succeeds with `DerivedIndexOpenState::Rebuilt` and the expected object-reference set.
- [ ] Add tests for missing source text, root mismatch, unwritable projection path, update, delete, additional-encode output, and remote KU input using the CID recomputed from validated bytes rather than a peer-supplied string.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test durable_data_recovery`. Expected: failure because startup currently propagates corrupt JSON.
- [ ] Implement one node-owned retriever service in `derived_projection.rs`; inject that service into `Mediator` instead of constructing a second empty retriever. On missing/corrupt/root-mismatched snapshots, quarantine the old file by digest and rebuild. If snapshot publication fails, start in typed degraded mode while canonical reads remain available.
- [ ] Run both focused suites and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib`.
- [ ] Commit:

```powershell
git add src/ku-core/src/foundation/source_text.rs src/ku-core/src/foundation/mod.rs src/ku-core/src/foundation/schema_registry.rs src/ku-core/src/foundation/vault.rs src/ku-mediator/src/retriever.rs src/ku-mediator/src/mediator.rs src/onebrain-node/src/derived_projection.rs src/onebrain-node/src/source_capture_transaction.rs src/onebrain-node/tests/durable_data_recovery.rs src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md
git commit -m "fix(storage): rebuild retriever from durable sources"
```

### Task 8: Make previews Unicode-safe and add exact canonical public exchange

**Branch:** `codex/base-v1-storage`

**Files:**

- Create: `src/onebrain-node/src/text.rs`
- Create: `src/onebrain-node/src/canonical_exchange.rs`
- Create: `src/onebrain-node/tests/canonical_exchange.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-api/src/handlers.rs`
- Modify: `src/onebrain-api/src/types.rs`
- Modify: `src/onebrain-cli/src/cli/data.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

**Interface:**

```rust
pub fn truncate_preview(input: &str, max_graphemes: usize) -> String;

pub enum BaseExchangeEntryV1 {
    VNextPublic {
        kind: StoredRecordKind,
        cid: [u8; 32],
        canonical_bytes: Vec<u8>,
    },
    LegacyReadOnlyEvidence {
        cid: [u8; 32],
        wire_bytes: Vec<u8>,
        epigenetics_json: Vec<u8>,
    },
}

pub fn write_canonical_exchange<W: Write>(entries: &[BaseExchangeEntryV1], output: W)
    -> Result<ExchangeReceipt, ExchangeError>;
pub fn read_canonical_exchange<R: Read>(input: R)
    -> Result<Vec<BaseExchangeEntryV1>, ExchangeError>;
```

The format is `OBXV1\n`, followed by compact, field-order-fixed JSON records sorted by `(kind, cid)`, and a footer that binds count, byte length, and a domain-separated BLAKE3 root. It carries only public vNext records and explicit read-only legacy evidence; private source text, Vault data, identities, receipts, and signer material require the encrypted archive.

- [ ] Add tests for precomposed and decomposed Vietnamese clusters, CJK, emoji modifiers, flags, and emoji ZWJ sequences at 0/1/77/80-grapheme boundaries; assert no panic, broken scalar, or split extended grapheme cluster.
- [ ] Add the workspace-pinned `unicode-segmentation` dependency and replace the byte slices currently used for previews in `node.rs` with grapheme-bounded `truncate_preview()`.
- [ ] Add exchange tests for deterministic ordering, exact wire-byte round trip, CID mismatch, malformed hex, duplicate CID, unknown kind/version, trailing bytes, private-class rejection, and one-byte corruption.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test canonical_exchange`. Expected: compile failure because the module is absent.
- [ ] Implement the exchange reader with bounded record count/record bytes, canonical vNext validation before sink admission, and legacy evidence that cannot enter a vNext write namespace.
- [ ] Rename the existing text re-encoding path to `import_text_drafts`; expose `import_canonical_exchange` separately. Rename JSON/CSV output to view exports so clients cannot mistake them for restorable canonical files.
- [ ] Update REST/CLI types to require an explicit `canonical-v1`, `json-view-v1`, `csv-view-v1`, or `text-drafts-v1` mode.
- [ ] Run the focused suite, `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-api`, and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-cli`.
- [ ] Commit:

```powershell
git add src/onebrain-node/src/text.rs src/onebrain-node/src/canonical_exchange.rs src/onebrain-node/tests/canonical_exchange.rs src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-api/src/handlers.rs src/onebrain-api/src/types.rs src/onebrain-cli/src/cli/data.rs src/onebrain-node/Cargo.toml src/Cargo.lock
git commit -m "fix(base): preserve unicode and canonical exchange bytes"
```

## Phase 2 — Encrypted Archive, Dataset Generations, and Identity Recovery

### Task 9: Build the shared Base archive crate from the reviewed mobile primitive

**Branch:** `codex/base-v1-archive`

**Files:**

- Create: `src/onebrain-archive/Cargo.toml`
- Create: `src/onebrain-archive/src/lib.rs`
- Create: `src/onebrain-archive/src/container.rs`
- Create: `src/onebrain-archive/src/crypto.rs`
- Create: `src/onebrain-archive/src/limits.rs`
- Create: `src/onebrain-archive/src/verified.rs`
- Create: `src/onebrain-archive/tests/container_vectors.rs`
- Modify: `src/Cargo.toml`
- Modify: `src/Cargo.lock`

**Mobile boundary:** this task changes no mobile-owned file, bridge, native adapter, package, or evidence row. Existing mobile `OBARV001` encode/decode behavior remains frozen until the post-`BASE-GATE-V1` mobile plan; the mobile contract validator runs only as a regression guard here.

**Interface:**

```rust
pub struct RecoveryKey(Zeroizing<[u8; 32]>);

pub enum ArchiveCredential {
    Password(Zeroizing<Vec<u8>>),
    RecoveryKey(RecoveryKey),
}

pub enum ArchiveCredentialKind { Password, RecoveryKey }

pub struct ArchiveKdfProfileV1 {
    pub memory_kib: u32,      // 65_536
    pub iterations: u32,     // 3
    pub parallelism: u32,    // 1
}

pub struct ArchiveLimits {
    pub max_entries: u32,
    pub max_manifest_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_total_plaintext_bytes: u64,
    pub max_spool_bytes: u64,
}

pub struct LegacyArchiveInspection { /* OBARV001 metadata only; never activatable */ }
pub struct VerifiedDatasetArchiveV2 {
    /* private fields: owned encrypted spool, verified manifest/root, one-shot state */
}

pub trait EncryptedSpoolCapability: Read + Write + Seek + Send {
    fn sync_all(&mut self) -> Result<(), ArchiveError>;
    fn securely_remove(self: Box<Self>) -> Result<(), ArchiveError>;
}

pub trait SecureSpoolFactory: Send + Sync {
    fn create_new(&self, max_bytes: u64)
        -> Result<Box<dyn EncryptedSpoolCapability>, ArchiveError>;
}

pub fn seal_archive<R: Read, W: Write>(
    input: R, output: W, credential: &ArchiveCredential, limits: &ArchiveLimits,
) -> Result<ArchiveInspection, ArchiveError>;
pub fn inspect_legacy_archive_v1<R: Read>(
    input: R, recovery_key: &RecoveryKey, limits: &ArchiveLimits,
) -> Result<LegacyArchiveInspection, ArchiveError>;
pub fn verify_dataset_archive_v2<R: Read + Send + 'static>(
    input: R, spool_factory: &dyn SecureSpoolFactory,
    credential: &ArchiveCredential,
    limits: &ArchiveLimits,
) -> Result<VerifiedDatasetArchiveV2, ArchiveError>;

impl VerifiedDatasetArchiveV2 {
    pub fn materialize_into(self, sink: &mut dyn LogicalRestoreSink)
        -> Result<VerifiedMaterialization, ArchiveError>;
}
```

- [ ] Copy the existing archive positive/negative vectors into shared-crate integration tests and add password/wrong-password/KDF-downgrade/huge-KDF-parameter/entry-limit/spool-limit vectors. Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-archive`; expect package-not-found.
- [ ] Implement `OBARV001` in the new Base crate as recovery-key-authenticated `LegacyArchiveInspection` decode-and-normalize-only support; decrypt and authenticate its existing manifest before returning any normalized metadata. No conversion or trait implementation can produce `VerifiedDatasetArchiveV2`. Do not change or re-export the existing mobile encoder. Add Base-domain `OBARV002` with Argon2id and recovery-key header profiles, an encrypted authenticated manifest, 1 MiB chunks, exact length/digest, no trailing bytes, and bounded resources. Its public header contains only magic/version, frozen KDF parameters, salt/nonce, and bounded ciphertext lengths—never logical keys, recovery policy, or dataset metadata.
- [ ] Validate the exact frozen KDF profile and all size/count limits before allocating Argon2 memory, manifest buffers, chunks, or spool space.
- [ ] Implement two-phase verification by copying every input—seekable or streaming—into a crate-owned/pre-opened encrypted spool created with create-new/no-follow semantics, owner-only permissions, reparse/symlink rejection, bounded same-volume storage, handle+digest binding, sync, and crash-cleanup registration. `VerifiedDatasetArchiveV2` privately owns that immutable spool handle, re-authenticates the complete container immediately before materialization, and is consumed exactly once. No API emits plaintext before the token exists.
- [ ] Add spool tests for pre-existing target, symlink/reparse swap, permission failure, size exhaustion, crash residue cleanup, cleanup failure reporting, and handle replacement. No public API accepts a raw spool path.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-archive` and `python scripts/ci/validate_mobile_build_contracts.py`; separately rerun the unchanged mobile archive suite with `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-mobile-core archive` as a regression check.
- [ ] Commit:

```powershell
git add src/onebrain-archive src/Cargo.toml src/Cargo.lock
git commit -m "feat(base): add encrypted archive container"
```

### Task 10: Implement the versioned dataset manifest and deterministic snapshot

**Branch:** `codex/base-v1-archive`

**Depends on:** Task 7, because this task generalizes the validated-store and deterministic Vault source-scan ports rather than creating a competing snapshot API.

**Files:**

- Create: `src/onebrain-archive/src/manifest.rs`
- Create: `src/onebrain-archive/src/dataset.rs`
- Create: `src/onebrain-archive/tests/dataset_roundtrip.rs`
- Create: `src/test-vectors/vnext/base-v1-archive-v1.json`
- Create: `docs/operations/ONEBRAIN_BASE_V1_ARCHIVE_RESTORE_GUIDE.md`
- Modify: `src/onebrain-archive/src/lib.rs`
- Modify: `src/ku-core/src/foundation/storage.rs`
- Modify: `src/ku-core/src/foundation/vault.rs`
- Modify: `src/ku-core/src/foundation/migration.rs`
- Modify: `src/ku-core/src/foundation/mod.rs`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`

**Interface:**

```rust
pub struct ArchiveEntryId([u8; 32]);
pub struct BoundedBytes<const MAX: usize>(Vec<u8>);
pub enum ArchiveProfileId { ObarV2 }

pub enum ArchiveEntryKind {
    CanonicalObject, CanonicalEvent, FeedInception, AuthorityEvent, AuthorityHighWater,
    VaultRecord, QuarantineRecord, OwnedBlob, IdentityEnvelope,
    ReconciliationJournalRecord, InventoryRecord, OutboxRecord, ProvenanceRecord,
    PrivateNeedRecord, ReceivedUseRecord, OperationalRecord, RolloutRecord,
    BaseOperationRecord, PendingBlobUploadIntent, SourceCaptureIntent,
    MigrationState, InterpretationConfig,
    RegistryHighWater, SignerRecoveryPolicy,
}

pub struct ArchiveOwner(u16); // exact Task 2 owner code; reserved values rejected
pub struct ArchiveLogicalKey {
    pub owner: ArchiveOwner,
    pub namespace: u16,
    pub key: BoundedBytes<256>,
}

pub struct ArchiveEntryV1 {
    pub id: ArchiveEntryId,
    pub kind: ArchiveEntryKind,
    pub logical_key: ArchiveLogicalKey,
    pub length: u64,
    pub blake3: [u8; 32],
    pub required: bool,
}

pub struct PortableProfileVersion { pub major: u16, pub minor: u16 }
pub enum ProducerArtifactIdentityV1 { Known([u8; 32]), Unknown }

pub struct PortableDataCompatibilityV1 {
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_schema_version: u32,
    pub archive_profile: PortableProfileVersion,
    pub migration_profile: PortableProfileVersion,
}

pub struct DatasetManifestV1 {
    pub profile: ArchiveProfileId,
    pub portable_data_compatibility: PortableDataCompatibilityV1,
    pub producer_artifact_identity: ProducerArtifactIdentityV1,
    pub canonical_root: [u8; 32],
    pub object_root: [u8; 32],
    pub blob_root: [u8; 32],
    pub feed_root: [u8; 32],
    pub entries: Vec<ArchiveEntryV1>,
    pub aggregate_root: [u8; 32],
}

pub trait SnapshotSource {
    fn acquire_snapshot(&self) -> Result<SnapshotLease, ArchiveError>;
    fn entries(&self, lease: &SnapshotLease) -> Result<Vec<ArchiveEntryV1>, ArchiveError>;
    fn read_entry(&self, lease: &SnapshotLease, id: ArchiveEntryId)
        -> Result<Box<dyn Read>, ArchiveError>;
}
```

**Dependency direction:** `ku-core` and `ku-net` expose only bounded, substrate-neutral scan/restore ports and DTOs defined in their own crates. They must not depend on `onebrain-archive`. Node-owned adapters depend upward on both sides and map those DTOs to/from `ArchiveEntryV1`; `onebrain-archive` types never flow into the lower foundation or network crates.

- [ ] Add vector tests for stable logical entry IDs, bounded owner/namespace/key values, sorted IDs, duplicate IDs/keys, missing required kind, modified length/hash, source-root/high-water mismatch, and aggregate-root mismatch. No manifest field may contain a filesystem path. Prove the same portable data subtuple restores across target/toolchain changes while canonical/storage/archive/migration mismatches fail.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-archive --test dataset_roundtrip`. Expected: compile failure for dataset types.
- [ ] Generalize the per-entry hash, sorted manifest, and aggregate-root logic currently private to `vnext_p5_operations.rs` around logical IDs rather than paths. Encode the manifest with deterministic canonical bytes and domain `onebrain:base:archive-manifest:1\0`. Authenticate both the portable data subtuple and `ProducerArtifactIdentityV1`; only the former gates cross-machine restore, while the latter is provenance. `Unknown` is truthful for pre-Task-16/development builds and can never support a qualified release claim; Task 16 supplies the one-way known artifact-tuple adapter.
- [ ] Add bounded substrate-neutral accepted/Quarantine enumeration, portable Vault snapshot/validated-restore, and migration-state enumeration/restore ports in `ku-core`. Task 13—not this task—implements the Node-owned `SnapshotVerifiedBackend` adapter over those ports. Vault records are canonical plaintext only inside the authenticated encrypted stream and are re-encrypted under the target Vault key; raw Vault database ciphertext is never portable data.
- [ ] Acquire a `SnapshotLease` only after quiesce. Bind the frozen canonical source root, all high-water marks, dataset generation, and held blob/retention generations. Same-length or any other mutation during capture invalidates the lease.
- [ ] Include all logical kinds above, including canonical `AuthorityEvent` branches needed to rebuild authority decisions and generation-owned pending blob/source intents needed for deterministic reopen reconciliation; exclude feed/index projections, retriever/graph/search generations, and Registry/model payload bytes while preserving signed Registry/high-water metadata. `SignerRecoveryPolicy` is input; a reprovision receipt is restore output, never an archive entry.
- [ ] Verify the snapshot against its roots before feeding it to the encrypted container. Any source mutation during capture invalidates the attempt and releases holds without publishing an archive.
- [ ] Add the archive contract/vector to the validator and normative coverage; write the exact operator sequence and non-exportable signer limitation in the runbook.
- [ ] Run the focused suite, `python scripts/ci/validate_vnext_contracts.py`, `python scripts/ci/validate_mobile_build_contracts.py`, and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-mobile-core`; Task 10 changes a mobile-core dependency but no mobile-owned behavior or evidence row.
- [ ] Commit:

```powershell
git add src/onebrain-archive/src/manifest.rs src/onebrain-archive/src/dataset.rs src/onebrain-archive/src/lib.rs src/onebrain-archive/tests/dataset_roundtrip.rs src/test-vectors/vnext/base-v1-archive-v1.json src/ku-core/src/foundation/storage.rs src/ku-core/src/foundation/vault.rs src/ku-core/src/foundation/migration.rs src/ku-core/src/foundation/mod.rs docs/operations/ONEBRAIN_BASE_V1_ARCHIVE_RESTORE_GUIDE.md scripts/ci/validate_vnext_contracts.py docs/specs/vnext/normative_coverage.json
git commit -m "feat(base): add deterministic archive dataset manifest"
```

### Task 11: Restore into a verified dataset generation and atomically activate it

**Branch:** `codex/base-v1-archive`

**Files:**

- Create: `src/onebrain-archive/src/restore.rs`
- Create: `src/onebrain-archive/tests/materialization_failpoints.rs`
- Create: `src/onebrain-node/src/dataset_generation.rs`
- Create: `src/onebrain-node/src/activation_journal.rs`
- Create: `src/onebrain-node/src/dataset_root_lease.rs`
- Create: `src/onebrain-node/tests/dataset_generation_failpoints.rs`
- Modify: `src/onebrain-archive/src/lib.rs`
- Modify: `src/onebrain-node/src/config.rs`
- Modify: `src/onebrain-node/src/dataset_path.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-node/src/vnext_network_runtime.rs`
- Modify: `src/onebrain-node/src/vnext_distributed_kql.rs`
- Modify: `src/onebrain-node/src/vnext_distributed_pomv.rs`
- Modify: `src/onebrain-node/src/vnext_outbox.rs`
- Modify: `src/onebrain-node/src/vnext_record_provenance.rs`
- Modify: `src/onebrain-node/src/vnext_product_runtime.rs`
- Modify: `src/onebrain-node/src/vnext_runtime_rollout.rs`
- Modify: `src/onebrain-node/src/derived_index.rs`
- Modify: `src/onebrain-node/src/derived_projection.rs`
- Modify: `src/onebrain-node/src/blob_authority.rs`
- Modify: `src/onebrain-node/src/source_capture_transaction.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

**Interface:**

```rust
// DatasetGenerationId and DatasetPathResolver come from Task 5's frozen port.

pub struct DatasetGenerationStore {
    root: PathBuf,
    root_lease: DatasetRootLease,
}

pub struct RestoreOperationBinding {
    pub operation_id: [u8; 32],
    pub idempotency_key: [u8; 32],
}

pub struct StagedDatasetGeneration { /* private staged stores and proofs */ }
pub struct ActivationReadyGeneration { /* private one-shot type-state */ }

pub struct ArchiveRestorePolicyV1 {
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_schema_version: u32,
    pub archive_profile: PortableProfileVersion,
    pub migration_profile: PortableProfileVersion,
    pub max_dataset_bytes: u64,
}

impl DatasetGenerationStore {
    pub fn open_exclusive(root: &Path) -> Result<Self, RestoreError>;
    pub fn stage_verified_restore(
        &self, verified: VerifiedDatasetArchiveV2,
        expected: &ArchiveRestorePolicyV1,
    ) -> Result<StagedDatasetGeneration, RestoreError>;
    pub fn activate(
        &self,
        ready: ActivationReadyGeneration,
        operation: RestoreOperationBinding,
    )
        -> Result<DatasetGenerationReceipt, RestoreError>;
    pub fn recover_activation(&self, operation_id: [u8; 32])
        -> Result<DatasetGenerationReceipt, RestoreError>;
}
```

The layout is `datasets/generations/<manifest-root>/` with Node-owned stores that map logical archive owner/key pairs to fixed local locations; archive bytes never name a filesystem path. Activation uses a cross-platform dual-slot pointer (`current.a.json`, `current.b.json`) plus a checksummed **non-switched control-plane** activation journal and monotonic generation, rather than relying on directory fsync or one rename alone. That journal binds the restore operation/idempotency IDs, old/new generation roots, pointer phase, and terminal or `UnknownOutcome` receipt so reconciliation still works after the dataset pointer changes. A one-time bootstrap may owner-validate and adopt only pre-generation **vNext Base-owned** stores into generation zero without rewriting canonical bytes. Legacy KU files remain outside this path and are visible only through the explicit read-only migration capability.

`open_exclusive()` canonicalizes and owner-validates the durable root, creates the fixed no-follow control-plane lock file only when absent or opens that same verified regular file when present, takes a nonblocking exclusive OS file lock, and holds the open lock handle for the full `DatasetGenerationStore` lifetime **before** opening or recovering any pointer, journal, generation, or store. Contention returns typed `DatasetRootInUse`; process death releases the OS lock, while the inert lock file is never treated as proof of a live/stale owner or deleted to steal authority.

- [ ] Keep archive-crate failpoints in `materialization_failpoints.rs` (token consumption, entry materialization, flush, and cleanup) and Node activation failpoints in `dataset_generation_failpoints.rs` (health/projection checks, pointer/journal, reopen, rollback, and retirement). This preserves the dependency direction and avoids an archive-to-Node dev-dependency cycle. The only process-reopen states allowed are old complete or new complete.
- [ ] Add tests proving a raw reader, path, or credential cannot call `stage_verified_restore`; wrong password/key and corrupt-container failures in Task 9 create no generation. Add missing/extra/duplicate logical entry, downgrade, Registry equivocation, target non-empty, duplicate restore, and non-exportable signer-policy cases.
- [ ] Add Windows tests for reparse-point rejection, case-folded logical-key collisions, cross-volume staging/rename rejection, write-through/flush ordering, a torn newest pointer slot, and recovery from the older valid slot. Run equivalent symlink/mount-boundary cases on Unix.
- [ ] Add child-process root-lease tests on Windows and Unix: a second process cannot open the same root through the same path, case/alias, symlink/reparse alias, or concurrent startup; killing the holder releases only the OS lock and the next process first runs journal recovery. Lock-acquisition failure must occur before any durable byte changes.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-archive --test materialization_failpoints` and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test dataset_generation_failpoints`. Expected: compile failures for restore/generation APIs.
- [ ] Add the path-pinned `onebrain-archive` dependency to `onebrain-node`, update `src/Cargo.lock`, and keep archive types behind Node-owned facade ports rather than re-exporting raw container/store handles.
- [ ] Consume the privately constructed `VerifiedDatasetArchiveV2` exactly once into owner-specific restore sinks. Re-authenticate its pinned spool during the second pass, then run exact entry/root verification and restore-policy checks. A raw reader, legacy inspection token, copied path, or changed/reopened handle can never reach staging.
- [ ] Make `ActivationReadyGeneration` privately constructible only after every canonical store is healthy and Task 6/7 derived index plus retriever projection have been rebuilt against the staged generation roots. Task 12 adds the identity-recovery prerequisite to this constructor; production code cannot bypass it, while crate-private test fixtures may construct explicit proof tokens.
- [ ] Replace every Base-owned direct `config.data_dir` join with an injected `DatasetPathResolver`; update Node construction and the validated store, pending blob/source intent stores, outbox, provenance, distributed KQL/PoMV, product runtime, rollout state, and optional network stores to open only beneath the selected generation.
- [ ] During activation, quiesce admission, drain and close current generation handles, durably publish the new pointer/journal, reopen and health-check every new store, bind both derived services to the new generation, then release admission. If reopen fails, durably restore the old pointer, reopen the old generation, and rebind its index/retriever services; preserve it until retention releases the rollback lease.
- [ ] Kill around every journal/pointer transition, including after the pointer commit but before response. Reopen through a newly acquired service handle and reconcile the original operation ID from the non-switched journal; atomically carry its terminal/`UnknownOutcome` receipt into the selected generation before clearing the journal. Reused operation/idempotency IDs with different archive roots fail closed.
- [ ] Run the focused suite and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib dataset_generation -- --test-threads=1`.
- [ ] Commit:

```powershell
git add src/onebrain-archive/src/restore.rs src/onebrain-archive/tests/materialization_failpoints.rs src/onebrain-archive/src/lib.rs src/onebrain-node/src/dataset_generation.rs src/onebrain-node/src/activation_journal.rs src/onebrain-node/src/dataset_root_lease.rs src/onebrain-node/tests/dataset_generation_failpoints.rs src/onebrain-node/src/config.rs src/onebrain-node/src/dataset_path.rs src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-node/src/vnext_network_runtime.rs src/onebrain-node/src/vnext_distributed_kql.rs src/onebrain-node/src/vnext_distributed_pomv.rs src/onebrain-node/src/vnext_outbox.rs src/onebrain-node/src/vnext_record_provenance.rs src/onebrain-node/src/vnext_product_runtime.rs src/onebrain-node/src/vnext_runtime_rollout.rs src/onebrain-node/src/derived_index.rs src/onebrain-node/src/derived_projection.rs src/onebrain-node/src/blob_authority.rs src/onebrain-node/src/source_capture_transaction.rs src/onebrain-node/Cargo.toml src/Cargo.lock
git commit -m "feat(base): activate verified dataset generations"
```

### Task 12: Implement typed identity recovery and non-exportable signer reprovisioning

**Branch:** `codex/base-v1-archive`

**Files:**

- Create: `src/onebrain-node/src/identity_recovery.rs`
- Create: `src/onebrain-node/src/signer_ports.rs`
- Create: `src/onebrain-node/tests/identity_recovery.rs`
- Create: `src/onebrain-node/src/archive.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-node/src/dataset_generation.rs`
- Modify: `src/onebrain-node/src/vnext_network_runtime.rs`
- Modify: `src/onebrain-api/src/handlers.rs`
- Modify: `src/onebrain-api/src/types.rs`
- Modify: `src/onebrain-cli/src/cli/identity.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

**Interface:**

```rust
pub enum IdentityDomain { NodeTransport, ActorRoot, FeedAuthor }
pub struct SignerProviderId(String); // private field; constructor enforces ASCII <= 64
pub enum SignerCapability { NetworkSessions, ActorAuthority, FeedPublication }
pub struct SignerCapabilitySet(/* sorted unique, maximum three */);

pub struct NodeTransportIdentity {
    pub session_public_key: SessionPublicKey,
    pub principal_node_id: NodeId,
}
pub struct ActorRootIdentity { pub public_key: ActorRootPublicKey }
pub struct FeedAuthorIdentity {
    pub feed_public_key: FeedPublicKey,
    pub feed_id: FeedId,
}
pub enum ExpectedSignerIdentity {
    NodeTransport(NodeTransportIdentity),
    ActorRoot(ActorRootIdentity),
    FeedAuthor(FeedAuthorIdentity),
}

pub struct SignerReprovisionRequirement {
    pub expected: ExpectedSignerIdentity,
    pub provider_id: SignerProviderId,
    pub disabled_capabilities: SignerCapabilitySet,
}

pub enum SignerRecoveryPolicy {
    ExportableSeedEnvelope {
        expected: ExpectedSignerIdentity,
        sealed_seed: Zeroizing<Vec<u8>>,
    },
    ReprovisionRequired {
        expected: ExpectedSignerIdentity,
        provider_id: SignerProviderId,
    },
}

pub struct ActorRootStatementV1 {
    pub dataset_generation: DatasetGenerationId,
    pub canonical_root: [u8; 32],
    pub authority_high_water: u64,
}

pub struct SignerPossessionChallengeV1 {
    pub domain: IdentityDomain,
    pub expected_identity_digest: [u8; 32],
    pub dataset_generation: DatasetGenerationId,
    pub verifier_nonce: [u8; 32],
}

pub trait ActorRootSigner: Send + Sync {
    fn identity(&self) -> Result<ActorRootIdentity, SignerError>;
    fn sign_actor_root(&self, statement: &ActorRootStatementV1)
        -> Result<[u8; 64], SignerError>;
}

pub trait SignerProvider: Send + Sync {
    fn provider_id(&self) -> &SignerProviderId;
    fn session_identity(&self, expected: &NodeTransportIdentity)
        -> Result<Arc<dyn SessionIdentitySigner>, SignerError>;
    fn actor_root(&self, expected: &ActorRootIdentity)
        -> Result<Arc<dyn ActorRootSigner>, SignerError>;
    fn feed_event(&self, expected: &FeedAuthorIdentity)
        -> Result<Arc<dyn FeedEventSigner>, SignerError>;
    fn prove_possession(&self, challenge: &SignerPossessionChallengeV1)
        -> Result<SignerPossessionProof, SignerError>;
}

pub trait SignerProviderRegistry: Send + Sync {
    fn resolve(&self, id: &SignerProviderId)
        -> Result<Arc<dyn SignerProvider>, SignerError>;
}

pub struct IdentityRecoveryReceipt {
    pub restored: BoundedIdentityDomains, // sorted unique, maximum three
    pub reprovision_required: BoundedReprovisionRequirements, // maximum three
    pub dataset_generation: DatasetGenerationId,
}

pub struct DatasetRestoreReceipt {
    pub activation: DatasetGenerationReceipt,
    pub identity: IdentityRecoveryReceipt,
}
```

- [ ] Add golden positive/negative recovery vectors: three domain-separated exportable envelopes; swapped domains; session-public-key versus principal-Node-ID confusion; Feed public-key versus Feed-ID confusion; wrong Actor root key; wrong archive key; duplicate domain; missing Feed capability; unknown/wrong/cross-provider ID; forged/replayed/cross-domain possession proof; and a non-exportable Node signer requiring reprovisioning with the exact disabled capability set.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test identity_recovery`. Expected: compile failure for the recovery module.
- [ ] Add the workspace-pinned `zeroize` dependency to `onebrain-node` and update `src/Cargo.lock`; secret-bearing constructors accept owned zeroizing buffers and never `String`/borrowed password material.
- [ ] Freeze canonical encodings and derivation rules for all three typed identities. Implement key-domain verification before activation for every exportable envelope: derive and compare both Node session key and principal ID, Actor root key, and both Feed public key and Feed ID as applicable. Zeroize plaintext and reject raw signer material from REST/CLI responses and logs. Reuse the existing `FeedEventSigner` and `SessionIdentitySigner` traits and add only the missing Actor-root/provider ports.
- [ ] Make Actor-root signing accept only canonical `ActorRootStatementV1`; the implementation adds fixed domain `onebrain:actor-root-statement:1\0` internally and exposes no arbitrary-byte signing method. Freeze the possession challenge/proof canonical bytes, signature algorithm, public-key encoding, verifier nonce rules, domain separation, dataset-generation binding, and replay cache in vectors.
- [ ] Remove the 24-token stub. The legacy `recover_identity` endpoint returns a typed `unsupported_legacy_recovery` error directing clients to encrypted-package recovery; it never hashes arbitrary words into identity.
- [ ] Treat `SignerRecoveryPolicy` as archive input and `IdentityRecoveryReceipt` as restore output. Resolve non-exportable providers only through `SignerProviderRegistry`; require the resolved provider's own ID, exact typed identity, domain-bound possession proof, and disabled-capability set to match. Only those capabilities remain `ReprovisionRequired`; local status/query/archive and independent domains still open. Do not create `vnext_identity.key` during caller-owned recovery.
- [ ] Extend Task 11's private staging type-state: only identity recovery may turn a healthy `StagedDatasetGeneration` into `ActivationReadyGeneration`, and the resulting public restore response is the composite `DatasetRestoreReceipt`. Add `complete_reprovision(requirement, provider_proof)` as an idempotent lifecycle transition that re-verifies the exact provider/identity/generation, durably clears only the named limitation, and re-enables only the corresponding capabilities.
- [ ] Run the focused test, vNext signer custody tests, API tests, and CLI tests.
- [ ] Commit:

```powershell
git add src/onebrain-node/src/identity_recovery.rs src/onebrain-node/src/signer_ports.rs src/onebrain-node/tests/identity_recovery.rs src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-node/src/dataset_generation.rs src/onebrain-node/src/archive.rs src/onebrain-node/src/vnext_network_runtime.rs src/onebrain-api/src/handlers.rs src/onebrain-api/src/types.rs src/onebrain-cli/src/cli/identity.rs src/onebrain-node/Cargo.toml src/Cargo.lock
git commit -m "fix(base): implement encrypted identity recovery"
```

### Task 13: Complete Node archive adapters and quarantine plaintext backup primitives

**Branch:** `codex/base-v1-archive`

**Files:**

- Modify: `src/onebrain-node/src/archive.rs`
- Create: `src/onebrain-node/src/archive_capabilities.rs`
- Create: `src/onebrain-node/tests/archive_roundtrip.rs`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-node/src/vnext_validated_sink.rs`
- Modify: `src/onebrain-node/src/vnext_outbox.rs`
- Modify: `src/onebrain-node/src/vnext_network_runtime.rs`
- Modify: `src/onebrain-node/src/vnext_distributed_kql.rs`
- Modify: `src/onebrain-node/src/vnext_distributed_pomv.rs`
- Modify: `src/onebrain-node/src/vnext_record_provenance.rs`
- Modify: `src/onebrain-node/src/vnext_operational_compaction.rs`
- Modify: `src/onebrain-node/src/vnext_runtime_rollout.rs`
- Modify: `src/onebrain-node/src/blob_authority.rs`
- Modify: `src/onebrain-node/src/source_capture_transaction.rs`
- Modify: `src/ku-net/src/vnext_reconciliation_journal.rs`
- Modify: `src/ku-net/src/vnext_inventory_forest.rs`

**Interface:**

```rust
pub struct ArchiveCapabilityId([u8; 32]);
pub struct ArchiveOperationReservationId([u8; 32]);
pub struct WritableArchiveSourceHandle(/* private type-state */);
pub struct SealedArchiveSourceHandle(/* id + process generation + exact length */);
pub struct WritableArchiveSinkHandle(/* private type-state + maximum length */);
pub struct ReadableArchiveSinkHandle(/* sealed one-shot output */);
pub struct ArchiveSecretHandle(/* id + process generation + credential kind */);

pub struct ArchiveCapabilityRegistry { /* sole owner of bounded spools/secrets */ }

impl ArchiveCapabilityRegistry {
    pub fn begin_source(&self, owner_reservation: ArchiveOperationReservationId, expected_encrypted_bytes: u64)
        -> Result<WritableArchiveSourceHandle, NodeError>;
    pub fn push_source_chunk(&self, handle: &WritableArchiveSourceHandle, offset: u64, bytes: &[u8])
        -> Result<(), NodeError>;
    pub fn seal_source(&self, handle: WritableArchiveSourceHandle)
        -> Result<SealedArchiveSourceHandle, NodeError>;
    pub fn begin_sink(&self, owner_reservation: ArchiveOperationReservationId, max_encrypted_bytes: u64)
        -> Result<WritableArchiveSinkHandle, NodeError>;
    pub fn read_sink_chunk(&self, handle: &ReadableArchiveSinkHandle, offset: u64, max: u32)
        -> Result<BoundedArchiveChunk, NodeError>;
    pub fn commit_sink(&self, handle: ReadableArchiveSinkHandle) -> Result<(), NodeError>;
    pub fn register_secret(
        &self, owner_reservation: ArchiveOperationReservationId, kind: ArchiveCredentialKind,
        secret: Zeroizing<Vec<u8>>,
    )
        -> Result<ArchiveSecretHandle, NodeError>;
    pub fn abort(&self, id: ArchiveCapabilityId) -> Result<(), NodeError>;
    pub fn destroy(&self, id: ArchiveCapabilityId) -> Result<(), NodeError>;
}

pub struct BaseArchiveReceipt {
    pub readable_sink: ReadableArchiveSinkHandle,
    pub manifest_root: [u8; 32],
}

pub struct BaseArchiveService { /* generation, snapshot ports, quiesce, archive-local policy */ }

impl BaseArchiveService {
    pub async fn create_archive(
        &self, destination: WritableArchiveSinkHandle,
        credential: ArchiveSecretHandle,
        producer: ProducerArtifactIdentityV1,
    ) -> Result<BaseArchiveReceipt, NodeError>;
    pub async fn restore_archive(
        &self, archive: SealedArchiveSourceHandle,
        credential: ArchiveSecretHandle,
        expected: &ArchiveRestorePolicyV1,
    ) -> Result<DatasetRestoreReceipt, NodeError>;
}
```

- [ ] Add an integration fixture containing vNext objects/events/feed/authority events, public store, Vault, Quarantine, owned blob, reconciliation journal, inventory, pending outbox, provenance, KQL/PoMV private state, operational/rollout state, migration evidence, Registry high-water, and one rebuildable projection. Archive it, restore into a clean root, and assert canonical/object/blob/feed/authority and pending-state roots match while the projection is rebuilt.
- [ ] Add negative tests for password ignored, plaintext leak, modified byte, missing blob, unsafe entry, duplicate restore, kill windows, archive creation while quiesce cannot be acquired, and forged/stale/cross-operation/cross-generation capability handles. Exercise handle reuse, offset overlap/gap, size overflow, cancel, caller disconnect, panic/failpoint cleanup, double commit/abort/destroy, and secret zeroization.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test archive_roundtrip`. Expected: failures proving the current product JSON backup does not restore canonical vNext/blob state and ignores the password.
- [ ] Implement Node-owned `SnapshotVerifiedBackend` and enumerable bounded scan/restore adapters for validated canonical storage, Vault/Quarantine, pending blob/source intents, reconciliation journal, inventory, outbox, provenance, distributed KQL/PoMV, operational compaction, rollout/optional network state, and migration/config owners. Map substrate-neutral DTOs to archive entries under the Task 10 lease and restore only through validated target-store ports; lower crates never depend on `onebrain-archive`, and no adapter copies raw Redb files. Reopened intents reconcile before admission and are never blindly resumed.
- [ ] Implement `BaseArchiveService`; reduce `OneBrainNode` methods to delegation. Keep `onebrain/p5-offline-backup/1`, its sorted relative-path manifest, vectors, and `vnext_p5_operations` implementation unchanged as a frozen **preflight-only** contract. Product backup never routes through it; Task 22 defines production P5 recovery using the distinct Base archive profile without renaming or silently superseding P5 v1.
- [ ] Implement `ArchiveCapabilityRegistry` as the only owner of source/sink spools and zeroizing secrets, reusing Task 5's non-optional OS CSPRNG. Every capability has random identity, exact type-state, owner operation, process generation, bounds, and one-shot destruction. `create_archive` consumes a writable sink and returns its readable successor; only that successor can be read and committed. Entropy failure and ID collision publish nothing.
- [ ] Require an explicit `ArchiveCredentialKind` at secret ingress; password and recovery-key bytes can never be inferred by length. Enforce password bounds and exact 32-byte recovery keys before copying once into zeroizing storage. Keep raw paths/readers/writers/passwords outside this internal service. Task 17 binds it to durable prepare/confirm/reconcile; Task 18 alone wires REST, CLI, and C projections.
- [ ] Bind every capability to a pre-existing `ArchiveOperationReservationId`, not to a future/guessed operation ID. Task 13 uses a bounded internal reservation registry for tests; Task 17 makes reservation durable and maps it one-to-one to the generated Base reservation before any source/sink/secret ingress.
- [ ] Remove or decode-only quarantine the legacy plaintext backup format. Restore must reject it by default; an explicit offline migration tool may inspect it as untrusted legacy evidence but cannot activate it.
- [ ] Run the focused test, `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-canary-harness --lib vnext_p5_operations -- --test-threads=1`, API tests, CLI tests, and `python scripts/ci/validate_vnext_contracts.py`; assert P5 v1 vectors are byte-for-byte unchanged.
- [ ] Commit:

```powershell
git add src/onebrain-node/src/archive.rs src/onebrain-node/src/archive_capabilities.rs src/onebrain-node/tests/archive_roundtrip.rs src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-node/src/vnext_validated_sink.rs src/onebrain-node/src/vnext_outbox.rs src/onebrain-node/src/vnext_network_runtime.rs src/onebrain-node/src/vnext_distributed_kql.rs src/onebrain-node/src/vnext_distributed_pomv.rs src/onebrain-node/src/vnext_record_provenance.rs src/onebrain-node/src/vnext_operational_compaction.rs src/onebrain-node/src/vnext_runtime_rollout.rs src/onebrain-node/src/blob_authority.rs src/onebrain-node/src/source_capture_transaction.rs src/ku-net/src/vnext_reconciliation_journal.rs src/ku-net/src/vnext_inventory_forest.rs
git commit -m "feat(base): replace plaintext backup with encrypted archives"
```

## Phase 3 — Product-Neutral Contract, Compatibility, and Facade

### Task 14: Freeze the semantic runtime interface and projection rules

**Branch:** `codex/base-v1-contract`

**Files:**

- Create: `docs/specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md`
- Create: `src/test-vectors/vnext/base-v1-runtime-interface-v1.json`
- Create: `src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json`
- Create: `scripts/ci/test_validate_base_v1_runtime_interface.py`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `docs/specs/vnext/VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md`
- Modify: `src/test-vectors/vnext/product-integration-profile-v1.json`

**Frozen semantic surface:**

```text
open / negotiate(profile, capabilities, compatibility_tuple)
status / snapshot
query(request, opaque_continuation)
reserve_operation(kind) -> OperationReservation
prepare(reservation, command, including CreateArchive/RestoreArchive) -> PreparedIntent
confirm(intent, idempotency_key) -> OperationReceipt
cancel(operation_id)
reconcile(operation_id)
subscribe(topic, cursor) -> SubscriptionHandle
poll_events(subscription, after_cursor, max_items) -> EventBatch(next_cursor, gap)
close_subscription(subscription)
management.open(authorized_grant) -> ManagementHandle
management.archive_source_begin / push_chunk / seal
management.archive_sink_begin / read_chunk / commit
management.archive_secret_register
management.archive_capability_abort / destroy
management.complete_signer_reprovision
management.close
drain / close
```

The management namespace is privileged but product-neutral and belongs to the same generated facade/runtime lifecycle. An ordinary service handle cannot mint or infer management authority: a host-authenticated, unforgeable `BaseManagementGrant` binds principal, exact scopes, process/dataset generations, and expiry/revocation state, and `management.open` consumes it into a scoped handle. Archive create/restore are explicit command discriminators prepared, confirmed, canceled, and reconciled through the ordinary operation state machine; management calls only register, stream, commit/abort, and destroy bounded opaque capabilities. Required cross-projection fields include profile major/minor, process generation, request/operation/idempotency IDs, lifecycle, coverage, limitations, retryability, resource budget, typed payload discriminator, and compatibility digest. Raw paths, runtime/store handles, private keys, authority implementations, borrowed readers/writers, and unbounded strings are forbidden.

- [ ] Add validator tests for every missing operation, absent durable reserve-before-capability flow, missing `CreateArchive`/`RestoreArchive` command, unbounded payload/continuation/archive chunk, absent idempotency or generation fence, missing management scope/principal, ambiguous capability ownership, retry without reconciliation, raw path/key/reader exposure, handwritten projection allowance, and ABI structs without `struct_size`. Include missing subscription handle/poll/close, unbounded batch, cursor regression, retention gap without explicit resync, absent backpressure behavior, and missing archive register/seal/commit/abort/destroy or reprovision lifecycle.
- [ ] Run `python -m unittest scripts.ci.test_validate_base_v1_runtime_interface -v`. Expected: failure because the runtime profile is absent.
- [ ] Write the machine IDL with closed request/response/error discriminators, maximum lengths/counts, ownership/lifetime, async operation state machine, topic vocabulary, and exact projection mapping. Freeze subscription handle ownership, bounded polling, monotonic cursor advancement, gap/resync response, slow-consumer backpressure, archive capability streaming/lifecycle, drain behavior, subscription close, and runtime close authority.
- [ ] Freeze a canonical append-only discriminator history beside the live IDL. Every numeric/name pair is active or has a later tombstone record; existing records are never rewritten. CI verifies the immutable Task 14 receipt and protected `base-v1-idl-baseline` ref, loads that historical file with `git show`, and semantically diffs current history/IDL against it; editing both live files cannot hide drift. Task 27's signed release request later binds the resulting history-chain root. Removal, reuse, retyping, optional-to-required change, bound widening, or ownership change is a breaking-major change.
- [ ] Implement the validator and focused profile. Bump the additive product API profile minor for the Base negotiation/capability endpoint; do not change existing vNext endpoint meaning.
- [ ] Add coverage entries and run the focused test plus `python scripts/ci/validate_vnext_contracts.py`.
- [ ] Commit:

```powershell
git add docs/specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md src/test-vectors/vnext/base-v1-runtime-interface-v1.json src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json scripts/ci/test_validate_base_v1_runtime_interface.py scripts/ci/validate_vnext_contracts.py docs/specs/vnext/normative_coverage.json docs/specs/vnext/VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md src/test-vectors/vnext/product-integration-profile-v1.json
git commit -m "docs(base): freeze product-neutral runtime interface"
```

- [ ] Before Task 15 starts, the integration owner fast-forwards a protected `base-v1-idl-baseline` ref to this exact Task 14 commit and records commit, tree, live-IDL digest, and history-chain root in an immutable CI receipt. Tasks 15-16 receive that receipt/ref as mandatory input and load the baseline with `git show`; a missing, moved, non-ancestor, or digest-mismatched baseline stops execution. The ref is not a release/qualification tag and is never inferred by commit-message search.

### Task 15: Generate one typed contract for Rust, TypeScript, and Dart

**Branch:** `codex/base-v1-contract`

**Files:**

- Create: `src/onebrain-base-contract/Cargo.toml`
- Create: `src/onebrain-base-contract/build.rs`
- Create: `src/onebrain-base-contract/src/lib.rs`
- Create: `src/onebrain-base-contract/src/generated.rs`
- Create: `src/onebrain-base-contract/src/operation.rs`
- Create: `src/onebrain-base-contract/generated/typescript/base_v1.ts`
- Create: `src/onebrain-base-contract/generated/dart/base_v1.dart`
- Create: `src/onebrain-base-contract/tests/generation_drift.rs`
- Create: `scripts/base/generate_contract.py`
- Create: `scripts/base/test_generate_contract.py`
- Modify: `src/Cargo.toml`
- Modify: `src/Cargo.lock`

**Core generated types:**

```rust
pub struct BaseOperationId(pub [u8; 32]);
pub struct BaseOperationReservationId(pub [u8; 32]);
pub struct BaseIdempotencyKey(pub [u8; 32]);
pub struct BaseOpaqueContinuation(BoundedBytes<4096>);

impl BaseOpaqueContinuation {
    pub fn try_from_bytes(bytes: Vec<u8>) -> Result<Self, BaseContractError>;
    pub fn as_bytes(&self) -> &[u8];
}

pub enum BaseCommandV1 {
    ExistingLocalCommand(BaseLocalCommandV1),
    CreateArchive(CreateArchiveCommandV1),
    RestoreArchive(RestoreArchiveCommandV1),
}

pub enum ArchiveCredentialKindV1 { Password, RecoveryKey }
pub struct BoundedSecretIngressV1 {
    pub kind: ArchiveCredentialKindV1,
    pub bytes: SecretBytes<1024>, // one-way ingress; never serialized in a response/log
}
pub struct BaseManagementGrantV1(/* opaque registry ID + scoped principal binding */);

pub enum BaseRequestV1 {
    Status,
    Query(BaseQueryRequestV1),
    ReserveOperation(BaseOperationKindV1),
    Prepare(BasePrepareRequestV1), // reservation + exact command
    Confirm(BaseConfirmRequestV1),
    Cancel(BaseOperationId),
    Reconcile(BaseOperationId),
    Subscribe(BaseSubscriptionRequestV1),
    PollEvents(BasePollEventsRequestV1),
    CloseSubscription(BaseSubscriptionId),
    Drain,
    Close,
}

pub enum BaseManagementRequestV1 {
    ArchiveSourceBegin(ArchiveSourceBeginV1),
    ArchiveSourcePush(ArchiveSourcePushV1),
    ArchiveSourceSeal(ArchiveCapabilityHandleV1),
    ArchiveSinkBegin(ArchiveSinkBeginV1),
    ArchiveSinkRead(ArchiveSinkReadV1),
    ArchiveSinkCommit(ArchiveCapabilityHandleV1),
    ArchiveSecretRegister(BoundedSecretIngressV1),
    ArchiveCapabilityAbort(ArchiveCapabilityHandleV1),
    ArchiveCapabilityDestroy(ArchiveCapabilityHandleV1),
    CompleteSignerReprovision(CompleteSignerReprovisionV1),
    Close,
}

pub enum BaseErrorCodeV1 {
    InvalidRequest, NotFound, Conflict, Expired, RateLimited,
    CapabilityDisabled, DependencyUnavailable, IncompatibleProfile,
    ResourceExhausted, CorruptState, ReprovisionRequired,
    UnknownOutcome, InternalError,
}
```

- [ ] Write Python generator tests with a tiny frozen fixture and assert byte-for-byte Rust/TypeScript/Dart output, sorted discriminators, the valid `// Generated ...` header, private bounded continuation construction, and rejection of duplicate IDs, unsupported types, or unbounded collections.
- [ ] Run `python -m unittest scripts.base.test_generate_contract -v`. Expected: import/file-not-found failure.
- [ ] Implement a dependency-free generator that reads only the machine IDL and writes through temp+atomic replace. It must have `--check`, which first verifies the pinned Task 14 baseline receipt/ref and semantic history diff, then diffs generated bytes without modifying files.
- [ ] Generate the three projections, including archive capability/management and compatibility declarations. Keep handwritten Rust logic in `operation.rs` and later `compatibility.rs`; generated declarations contain no behavior or authority. Task 18 alone owns the C ABI and checked-in C header.
- [ ] Add `generation_drift.rs` to run the generator in `--check` mode and assert Rust serialization matches IDL golden vectors.
- [ ] Run `python scripts/base/generate_contract.py --check` and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-base-contract`.
- [ ] Commit:

```powershell
git add src/onebrain-base-contract scripts/base/generate_contract.py scripts/base/test_generate_contract.py src/Cargo.toml src/Cargo.lock
git commit -m "feat(base): generate cross-language contract projections"
```

### Task 16: Implement the unified compatibility tuple and negotiation policy

**Branch:** `codex/base-v1-contract`

**Files:**

- Create: `src/onebrain-base-contract/src/compatibility.rs`
- Create: `src/onebrain-base-contract/src/negotiation.rs`
- Create: `src/onebrain-base-contract/tests/negotiation_vectors.rs`
- Create: `src/test-vectors/vnext/base-v1-compatibility-v1.json`
- Create: `scripts/ci/test_validate_base_v1_compatibility.py`
- Modify: `src/onebrain-base-contract/src/lib.rs`
- Modify: `src/onebrain-base-contract/src/generated.rs`
- Modify: `src/onebrain-base-contract/generated/typescript/base_v1.ts`
- Modify: `src/onebrain-base-contract/generated/dart/base_v1.dart`
- Modify: `src/test-vectors/vnext/base-v1-runtime-interface-v1.json`
- Modify: `src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json`
- Modify: `scripts/base/generate_contract.py`
- Modify: `src/onebrain-base-contract/Cargo.toml`
- Modify: `src/onebrain-base-contract/build.rs`
- Modify: `src/Cargo.lock`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md`

**Interface:**

```rust
pub struct ProfileVersion { pub major: u16, pub minor: u16 }
pub struct StorageSchemaVersion(pub u32);
pub struct BoundedAscii<const MAX: usize>(String); // constructor enforces ASCII and MAX
pub struct BaseReleaseVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub prerelease: Option<BoundedAscii<32>>,
}
pub enum SourceCommitId { Sha1([u8; 20]), Sha256([u8; 32]) }
pub enum SourceCommitIdentity { Known(SourceCommitId), Unknown }
pub enum ToolchainIdentity { Known([u8; 32]), Unknown }

pub enum BaseQualificationState {
    Unqualified,
    Qualified {
        candidate_commit: SourceCommitId,
        candidate_semantic_digest: [u8; 32],
        evidence_blake3: [u8; 32],
    },
}

pub struct BaseCompatibilityTuple {
    pub base_version: BaseReleaseVersion,
    pub base_commit: SourceCommitIdentity,
    pub canonical_schema_digest: [u8; 32],
    pub domain_registry_digest: [u8; 32],
    pub resource_registry_digest: [u8; 32],
    pub storage_schema: StorageSchemaVersion,
    pub archive_profile: ProfileVersion,
    pub migration_profile: ProfileVersion,
    pub registry_profile: ProfileVersion,
    pub registry_profile_digest: [u8; 32],
    pub wire_session: ProfileVersion,
    pub product_api: ProfileVersion,
    pub c_abi: ProfileVersion,
    pub feature_set_digest: [u8; 32],
    pub target_triple: BoundedAscii<96>,
    pub toolchain: ToolchainIdentity,
}

pub struct BaseVersionStatus {
    pub compatibility: BaseCompatibilityTuple,
    pub candidate_semantic_digest: [u8; 32],
    pub artifact_tuple_digest: [u8; 32],
    pub qualification: BaseQualificationState,
}

pub struct BaseCompatibilityPolicy {
    pub current: BaseCompatibilityTuple,
    pub minimum_additive: NegotiatedVersions,
    pub archive_restore: ArchiveRestorePolicyV1,
}

pub struct NegotiatedVersions {
    pub base_minor: u16,
    pub wire_session_minor: u16,
    pub product_api_minor: u16,
    pub c_abi_minor: u16,
}

pub struct MigrationVectorBindingV1 {
    pub vector_id: BoundedAscii<64>,
    pub vector_blake3: [u8; 32],
    pub trust_policy_digest: [u8; 32],
}

pub enum BaseNegotiationOutcome {
    Compatible { versions: NegotiatedVersions, capabilities: BaseCapabilitySet },
    MigrationRequired {
        from: BaseReleaseVersion,
        to: BaseReleaseVersion,
        vector: MigrationVectorBindingV1,
    },
    Incompatible { reason: BaseCompatibilityError },
}
```

- [ ] Add at least one positive/negative vector for every tuple field: Base major/minor/patch/prerelease, source commit, canonical/domain/resource registry digests, storage, archive, migration, Registry profile/version/digest, wire session, product API, C ABI, feature set, target, and toolchain. Include unsupported required/optional capabilities and development tuples with unknown commit, unknown toolchain, or both. `Unknown` has a canonical encoding but can only produce `Unqualified`; release qualification and artifact publication reject it. A target/toolchain change must preserve the candidate semantic digest but change the artifact tuple digest; a semantic/profile/feature change must change both.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-base-contract --test negotiation_vectors`. Expected: compile failure for compatibility types.
- [ ] Define the public compatibility/qualification declarations in the Task 14 machine IDL and regenerate Rust/TypeScript/Dart; `compatibility.rs` contains only canonicalization, digest, adapter, and negotiation logic over those generated types. Run the semantic history validator before generation, permanently tombstone removed IDs, and reject a regenerated projection that follows a removed/reused/retyped discriminator.
- [ ] Compute two domain-separated digests from deterministic field order and typed binary lengths: the candidate semantic digest covers commit plus all schema/profile/feature fields including `migration_profile`, while the artifact tuple digest additionally covers target triple and toolchain identity. Source schema/domain/resource digests come from Task 3, storage from Tasks 5-7, archive/migration from Tasks 9-13, the Registry profile version/digest frozen by Task 19, and wire version from the existing protocol/session registry. Candidate-bound Registry run evidence is deliberately absent from both.
- [ ] Implement the one-way adapter from `BaseCompatibilityPolicy` to Task 11's `ArchiveRestorePolicyV1`; assert it preserves all canonical/storage/archive fields and cannot relax archive limits.
- [ ] Implement the one-way producer adapter to Task 10's `ProducerArtifactIdentityV1`: a known target/toolchain tuple yields its exact artifact digest; any unknown commit/toolchain yields `Unknown`. Archive code never computes or fabricates the unified tuple independently.
- [ ] Make `build.rs` consume validated `ONEBRAIN_BASE_COMMIT` and `ONEBRAIN_TOOLCHAIN_DIGEST`; missing, malformed, or unverifiable values produce the explicit `Unknown` variants rather than fabricated zero/sample hashes. Qualification is not a tuple field and never participates in either digest: every unknown identity forces separate `Unqualified` status, while Task 28 may attach `Qualified` only after verifying an external manifest that binds the same known commit, semantic digest, and per-artifact tuple digest.
- [ ] Freeze and implement a field-by-field decision table in the profile: Base major, canonical/domain/resource, Registry profile/digest, wire major, and C ABI major are incompatible; storage/archive/migration mismatches are `MigrationRequired` only with an exact signed vector ID/digest/trust-policy binding and otherwise incompatible; Base, wire-session, product-API, and C-ABI additive minors negotiate independently to the lower supported value and are returned together in `NegotiatedVersions`; feature differences use capability intersection but reject any missing required capability; patch/prerelease and known source commit are provenance-only when every semantic field agrees; target/toolchain are artifact provenance only. No unclassified field, implicit lockstep minor, or catch-all equality shortcut is allowed.
- [ ] Add validator tests and run the Rust/Python focused suites plus the vNext mixed-version conformance tests.
- [ ] Commit:

```powershell
git add src/onebrain-base-contract/src/compatibility.rs src/onebrain-base-contract/src/negotiation.rs src/onebrain-base-contract/src/lib.rs src/onebrain-base-contract/src/generated.rs src/onebrain-base-contract/generated/typescript/base_v1.ts src/onebrain-base-contract/generated/dart/base_v1.dart src/onebrain-base-contract/Cargo.toml src/onebrain-base-contract/build.rs src/Cargo.lock src/onebrain-base-contract/tests/negotiation_vectors.rs src/test-vectors/vnext/base-v1-compatibility-v1.json src/test-vectors/vnext/base-v1-runtime-interface-v1.json src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json scripts/base/generate_contract.py scripts/ci/test_validate_base_v1_compatibility.py scripts/ci/validate_vnext_contracts.py docs/specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md
git commit -m "feat(base): enforce one compatibility tuple"
```

### Task 17: Compose the offline-first Base runtime behind one typed service handle

**Branch:** `codex/base-v1-contract`

**Files:**

- Create: `src/onebrain-node/src/base_runtime.rs`
- Create: `src/onebrain-node/src/base_operation_store.rs`
- Create: `src/onebrain-node/tests/base_runtime_facade.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `src/onebrain-node/src/vnext_product_runtime.rs`
- Modify: `src/onebrain-node/src/vnext_status.rs`
- Modify: `src/onebrain-node/src/archive.rs`
- Modify: `src/onebrain-node/src/archive_capabilities.rs`
- Modify: `src/onebrain-node/src/dataset_generation.rs`
- Modify: `src/onebrain-node/src/activation_journal.rs`
- Modify: `src/Cargo.lock`
- Modify: `docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md`
- Modify: `src/test-vectors/vnext/base-v1-archive-v1.json`
- Modify: `scripts/ci/validate_vnext_contracts.py`

**Interface:**

```rust
pub struct BaseRuntime { /* sole aggregate owner */ }
pub struct ProcessGenerationId([u8; 32]);
pub struct BaseManagementGrant { /* private random ID, principal, scopes, generations */ }

#[derive(Clone)]
pub struct BaseServices {
    core: Weak<BaseServiceCore>,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
}

#[derive(Clone)]
pub struct BaseManagementServices {
    core: Weak<BaseServiceCore>,
    process_generation: ProcessGenerationId,
    dataset_generation: DatasetGenerationId,
    principal_scope_digest: [u8; 32],
}

impl BaseServices {
    pub fn negotiate(&self, request: BaseNegotiationRequest)
        -> Result<BaseNegotiationResponse, BaseServiceError>;
    pub fn snapshot(&self) -> Result<BaseStatusV1, BaseServiceError>;
    pub async fn invoke(&self, request: BaseRequestV1)
        -> Result<BaseResponseV1, BaseServiceError>;
    pub async fn poll_events(&self, request: BasePollEventsRequestV1)
        -> Result<BaseEventBatchV1, BaseServiceError>;
    pub async fn close_subscription(&self, id: BaseSubscriptionId)
        -> Result<(), BaseServiceError>;
    pub async fn drain(&self) -> Result<BaseDrainReceiptV1, BaseServiceError>;
    pub async fn close(&self) -> Result<BaseCloseReceiptV1, BaseServiceError>;
    pub fn management(&self, grant: BaseManagementGrant)
        -> Result<BaseManagementServices, BaseServiceError>;
}

impl BaseManagementServices {
    pub async fn invoke(&self, request: BaseManagementRequestV1)
        -> Result<BaseManagementResponseV1, BaseServiceError>;
    pub async fn close(self) -> Result<BaseManagementCloseReceiptV1, BaseServiceError>;
}
```

- [ ] Add tests that one `OneBrainNode` cannot own two Base runtimes, service clones hold only `Weak` ownership and fail with `StaleGeneration` after either captured generation closes, no raw subsystem/store/path/key accessor exists, and the node mutex is released before storage/network/archive waits. Only the injected local host authorizer may issue a random, one-shot `BaseManagementGrant` after principal authentication; ordinary services cannot mint, clone across principals, widen scopes, or reuse a revoked/expired/stale grant.
- [ ] Add lifecycle tests for open, partial-start rollback, admit, cancellation, drain, close, restart, process/dataset generation fencing, unknown outcome/reconcile, idempotent confirm, bounded workers, budget exhaustion, independently disabled network lanes, and the complete archive capability/management lifecycle. Prove stale/forged/cross-generation management handles fail through Rust, REST, and later C projections.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test base_runtime_facade`. Expected: compile failure because `BaseRuntime` is absent.
- [ ] Implement the aggregate around canonical store, Vault, Quarantine, blob store, derived projections, archive/capability service, Registry status, local vNext workflow, and optional `VNextProductRuntime` network lanes. Scoped `BaseServices::management(grant)` is the sole product-neutral privileged facade; it shares the same weak owner/lifecycle, validates principal/scope/revocation on every call, and is not a parallel product contract. Local Base opens even when network support is not compiled or every lane is off.
- [ ] Construct `BaseRuntime` only from Task 11's lifetime-held `DatasetRootLease`; it cannot open any operation/control/store state first or drop the lease while a service handle lives. Extend facade tests with two child processes racing the same root and crash/reopen recovery before admission.
- [ ] Adapt existing `VNextProductServices` operations behind `BaseServices` instead of exposing a second product contract. Preserve existing typed semantics and semantic firewalls.
- [ ] Allocate a cryptographically random 256-bit `ProcessGenerationId` from the OS CSPRNG, persist it with create-new plus file/parent sync in the non-switched control plane before admitting any request, and retain bounded prior-generation tombstones. Entropy failure, duplicate/collision injection, torn persistence, and ID reuse fail startup; the representation has no counter overflow case. Add child-process kill/reopen tests before and after persistence.
- [ ] Implement `base_operation_store.rs` as the durable owner of operation reservations, prepared intents, capability grants, idempotency keys, attempts, receipts, reconciliation state, subscription cursors, and gap markers. `reserve_operation(kind)` durably creates a random one-shot reservation before any archive capability ingress; every registered handle must bind it, and `prepare(reservation, command)` atomically consumes it only when handle kinds/states/owners match. Persist prepare/confirm state before external effects and bind every record to the current dataset/process generation. Archive create/restore use this same prepare/confirm/cancel/reconcile path; capability registration/abort/destruction is journaled and bounded. `UnknownOutcome` is reconciled, never blindly replayed.
- [ ] When negotiation returns `MigrationRequired`, persist the exact `MigrationVectorBindingV1` in the prepared intent, confirmation, activation journal, terminal/unknown-outcome receipt, and reconciliation response. A retry with a different vector ID, digest, or trust policy conflicts rather than selecting a migration implicitly.
- [ ] Inject Task 16's unified compatibility status into `BaseArchiveService`: archive create maps known/unknown producer identity truthfully, and restore uses the one-way `ArchiveRestorePolicyV1` adapter. This is the first task that connects archive operations to the generated command state machine; Tasks 9-13 remain contract-independent substrate.
- [ ] Add `TX-BASE-OPS-001` to the frozen transaction inventory with the exact mandatory five phases and child-process reopen oracle. Map ordinary operation rows to Task 10 `BaseOperationRecord` under `DatasetPathResolver` and extend archive/restore adapters. Bind an in-flight restore's operation/idempotency IDs into Task 11's non-switched activation journal; after a pointer swap, atomically carry a terminal or `UnknownOutcome/ReconcileRequired` receipt into the selected generation before journal cleanup. A newly acquired post-restore service handle must reconcile the original ID, and unresolved archived-generation effects are never resumed as fresh effects.
- [ ] Run the focused suite, `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-network-runtime --lib vnext_product_runtime -- --test-threads=1`, and archive/derived-storage suites.
- [ ] Commit:

```powershell
git add src/onebrain-node/src/base_runtime.rs src/onebrain-node/src/base_operation_store.rs src/onebrain-node/tests/base_runtime_facade.rs src/onebrain-node/Cargo.toml src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-node/src/vnext_product_runtime.rs src/onebrain-node/src/vnext_status.rs src/onebrain-node/src/archive.rs src/onebrain-node/src/archive_capabilities.rs src/onebrain-node/src/dataset_generation.rs src/onebrain-node/src/activation_journal.rs src/Cargo.lock docs/specs/vnext/DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md src/test-vectors/vnext/base-v1-archive-v1.json scripts/ci/validate_vnext_contracts.py
git commit -m "feat(base): compose the product-neutral runtime facade"
```

### Task 18: Add REST/C/CLI projections, cross-language conformance, and Base-default packaging

**Branch:** `codex/base-v1-contract`

**Files:**

- Create: `src/onebrain-base-abi/Cargo.toml`
- Create: `src/onebrain-base-abi/cbindgen.toml`
- Create: `src/onebrain-base-abi/include/onebrain_base_v1.h`
- Create: `src/onebrain-base-abi/src/lib.rs`
- Create: `src/onebrain-base-contract/tests/projection_conformance.rs`
- Create: `src/onebrain-base-contract/conformance/typescript/package.json`
- Create: `src/onebrain-base-contract/conformance/typescript/package-lock.json`
- Create: `src/onebrain-base-contract/conformance/typescript/tsconfig.json`
- Create: `src/onebrain-base-contract/conformance/typescript/test_base_v1.ts`
- Create: `src/onebrain-base-contract/conformance/dart/pubspec.yaml`
- Create: `src/onebrain-base-contract/conformance/dart/pubspec.lock`
- Create: `src/onebrain-base-contract/conformance/dart/test/base_v1_test.dart`
- Create: `src/onebrain-api/tests/base_contract.rs`
- Create: `src/onebrain-cli/tests/version.rs`
- Create: `scripts/ci/validate_base_abi_header.py`
- Create: `scripts/ci/test_validate_base_abi_header.py`
- Create: `scripts/toolchains/base-v1-tools.lock.json`
- Modify: `src/Cargo.toml`
- Modify: `src/Cargo.lock`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/onebrain-api/Cargo.toml`
- Modify: `src/onebrain-api/src/server.rs`
- Modify: `src/onebrain-api/src/vnext_api.rs`
- Modify: `src/onebrain-api/src/handlers.rs`
- Modify: `src/onebrain-api/src/types.rs`
- Modify: `src/onebrain-cli/Cargo.toml`
- Modify: `src/onebrain-cli/src/main.rs`
- Modify: `src/onebrain-cli/src/cli/data.rs`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `.github/workflows/vnext-foundation.yml`

**C ABI boundary:** opaque ordinary and scoped-management handles; `{abi_major, abi_minor, struct_size}` on every public struct; explicit pointer+length buffers; asynchronous operation IDs; process-generation fence; no Rust enum layout, path, raw pointer lifetime, or runtime reference crosses the boundary. The sole secret exception is bounded caller-owned credential ingress with explicit password/recovery-key discriminator: the call copies once into zeroizing registry storage and never returns/logs it. Raw signer/private keys and all secret outputs are forbidden. Caller-supplied output buffers use two-call sizing and are never freed by the library; separately tagged library-allocated event/error buffers must be released exactly once with `ob_base_buffer_free_v1`.

`ob_base_open_v1` receives an immutable host-authorizer trust configuration, never an ambient callback pointer. `ob_base_management_open_v1` consumes a bounded, single-use signed grant envelope issued by that host authorizer and returns a separately typed scoped handle; `ob_base_management_close_v1` revokes/releases only that handle. `ob_base_close_v1` drains/closes the runtime and all remaining handles. REST and CLI obtain the same semantic grant only after their host authentication layer succeeds.

**Required C symbols:** `ob_base_open_v1`, `ob_base_management_open_v1`, `ob_base_management_close_v1`, `ob_base_negotiate_v1`, `ob_base_snapshot_v1`, `ob_base_query_v1`, `ob_base_reserve_operation_v1`, `ob_base_prepare_v1`, `ob_base_confirm_v1`, `ob_base_cancel_v1`, `ob_base_reconcile_v1`, `ob_base_subscribe_v1`, `ob_base_poll_events_v1`, `ob_base_close_subscription_v1`, `ob_base_archive_source_begin_v1`, `ob_base_archive_source_push_v1`, `ob_base_archive_source_seal_v1`, `ob_base_archive_sink_begin_v1`, `ob_base_archive_sink_read_v1`, `ob_base_archive_sink_commit_v1`, `ob_base_archive_secret_register_v1`, `ob_base_archive_capability_abort_v1`, `ob_base_archive_capability_destroy_v1`, `ob_base_complete_reprovision_v1`, `ob_base_drain_v1`, `ob_base_close_v1`, and `ob_base_buffer_free_v1`.

- [ ] Add ABI tests for undersized structs; oversized same-major structs whose tail is ignored; null/length mismatch; invalid UTF-8; output-too-small/two-call caller-buffer sizing; tagged library allocation/free ownership; double free; ordinary/runtime versus management-handle close; close versus in-flight call; concurrent handle calls; stale generation; cursor gap/backpressure; reserve/prepare/cancel/reconcile; every archive capability transition; and ABI-major mismatch. Prove an ordinary/unprivileged, wrong-principal, wrong-scope, revoked, forged, or stale management grant cannot open/use the C management handle. Inject a panic/failpoint in every extern entry family and prove `catch_unwind` maps it to typed `InternalError`, cleans owned capabilities, and lets no unwind or abort cross the ABI.
- [ ] Configure `onebrain-base-abi` with crate types `cdylib`, `staticlib`, and `rlib`. Commit one exact reviewed `cbindgen` semver plus per-host executable hash/install recipe in `scripts/toolchains/base-v1-tools.lock.json`; CI installs it under the session tool directory, never from ambient `PATH`, and the validator rejects any version/hash mismatch. Generate `include/onebrain_base_v1.h` only from this crate with the pinned `cbindgen.toml`.
- [ ] Extend the header validator with an IDL-to-ABI descriptor pass: independently derive every operation/discriminator, field width/bound/ownership, `struct_size`, error, lifecycle transition, and required symbol from the Task 14 machine IDL, then compare that descriptor to the Rust ABI and header. Regenerate to a temp file and fail on checked-in drift, missing/extra symbols, or a header that agrees with Rust but not the IDL.
- [ ] Add one conformance corpus that invokes Rust facade, Axum router, and C ABI with the same negotiation, status, query, reserve, prepare, confirm, cancel, reconcile, subscribe, poll, subscription-close, archive register/stream/seal/commit/abort/destroy, complete-reprovision, management-close, drain, runtime-close, and every typed error/lifecycle vector. Include an authenticated full `reserve -> capability ingress -> CreateArchive -> readable sink -> commit -> reserve -> source upload -> RestoreArchive -> reconcile` round trip plus wrong reservation/credential, unprivileged management, and kill/reopen cases. Compare normalized semantic responses. The dedicated TypeScript and Dart harnesses encode/decode every command/receipt/error in that corpus, assert bounds/gaps/ownership, and run outside the mobile tree.
- [ ] Run the exact conformance commands below. Expected initially: package/module failure. Pin Node/npm/Dart versions in CI; generated Dart remains a pre-gate contract artifact and is not imported into Flutter.

```powershell
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-base-abi
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-base-contract --test projection_conformance
npm ci --prefix src/onebrain-base-contract/conformance/typescript
npm test --prefix src/onebrain-base-contract/conformance/typescript
Push-Location src/onebrain-base-contract/conformance/dart
dart pub get --enforce-lockfile
dart test
Pop-Location
```
- [ ] Implement `/api/base/v1/capabilities`, `/api/base/v1/status`, and `/api/base/v1/operations` as authenticated local projections. Authenticated host policy maps principals to explicit management scopes before issuing a one-shot grant; archive multipart/CLI paths use that scoped handle and always destroy it. Existing `/api/vnext/...` endpoints delegate to the same service semantics; REST remains noncanonical.
- [ ] Add `onebrain --version --verbose` and a `base status` command that emit the complete tuple/digest plus the separately derived qualification state without starting a node.
- [ ] Define `base-v1` as the default feature for Node/API/CLI. It includes the local facade and persistence; `vnext-network-runtime` remains separately compiled and all distributed lanes remain requested/active false by default.
- [ ] Add `legacy-read-compat` as an explicit feature/runtime flag. Without it, legacy view/migration routes are absent and legacy writes return `capability_disabled`; no automatic backend fallback remains.
- [ ] Add black-box Node/REST/CLI tests with `legacy-read-compat` compiled on but runtime flag off/on. Off means route/command absence plus no backend open; on means bounded read-only view/migration only. Both modes reject legacy writes, automatic fallback, and a runtime attempt to enable code that was compiled out.
- [ ] Add a packaging validator and CI matrix for: each Node/API/CLI package with `--no-default-features --features base-v1`; `base-v1,legacy-read-compat`; Base without legacy; default features; all three network feature namespaces; and every forbidden combination (including `legacy-read-compat` without `base-v1`). Inspect the release feature manifest and symbols to prove `vnext-canary-harness`, test failpoints, and preflight-only P5 code are absent from production artifacts.
- [ ] Run all focused suites, both TypeScript/Dart harnesses, `python -m unittest scripts.ci.test_validate_base_abi_header -v`, `python scripts/ci/validate_base_abi_header.py`, and the full packaging matrix. Assert every distributed lane still reports requested/active false until runtime-enabled.

```powershell
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node -p onebrain-api -p onebrain-cli
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node -p onebrain-api -p onebrain-cli
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node --no-default-features --features base-v1
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-api --no-default-features --features base-v1
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-cli --no-default-features --features base-v1
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node --no-default-features --features base-v1,legacy-read-compat
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node -p onebrain-api -p onebrain-cli --features onebrain-node/vnext-network-runtime,onebrain-api/vnext-network-runtime,onebrain-cli/vnext-network-runtime
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node -p onebrain-api -p onebrain-cli --features onebrain-node/vnext-network-runtime,onebrain-api/vnext-network-runtime,onebrain-cli/vnext-network-runtime
```
- [ ] Commit:

```powershell
git add src/onebrain-base-abi src/onebrain-base-contract/tests/projection_conformance.rs src/onebrain-base-contract/conformance src/onebrain-api/tests/base_contract.rs src/onebrain-cli/tests/version.rs scripts/ci/validate_base_abi_header.py scripts/ci/test_validate_base_abi_header.py scripts/toolchains/base-v1-tools.lock.json scripts/ci/validate_vnext_contracts.py .github/workflows/vnext-foundation.yml src/Cargo.toml src/Cargo.lock src/onebrain-node/Cargo.toml src/onebrain-api/Cargo.toml src/onebrain-api/src/server.rs src/onebrain-api/src/vnext_api.rs src/onebrain-api/src/handlers.rs src/onebrain-api/src/types.rs src/onebrain-cli/Cargo.toml src/onebrain-cli/src/main.rs src/onebrain-cli/src/cli/data.rs
git commit -m "feat(base): ship generated projections and default facade"
```

## Phase 4 — Concept Registry Production Kernel

### Task 19: Freeze full-size Registry production qualification

**Branch:** `codex/base-v1-registry`

**Files:**

- Create: `docs/specs/vnext/CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md`
- Create: `src/test-vectors/vnext/concept-registry-production-qualification-v1.json`
- Create: `scripts/ci/test_validate_concept_registry_production_qualification.py`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `docs/specs/vnext/README.md`
- Modify: `src/test-vectors/vnext/concept-registry-operations-v1.json`

**Frozen reference rules:**

- The release package has exactly five payload artifacts: `concepts.obr`, `concepts.obr.labels.idx`, `concepts.obr.ccids.idx`, `concepts.obr.manifest.json`, and `sbom.spdx.json`, plus the separate `release.stamp.json` verification stamp.
- The release aggregate root covers the ordered five-artifact set by kind, filename, exact length, and BLAKE3. `release.stamp.json` signs/binds that root and activation metadata but is not included in the bytes it attests, avoiding self-reference.
- `concepts.obr` is 2.2 GB-class when `2_200_000_000 <= obr_bytes <= 2_500_000_000`.
- Cold-cache: ready ≤180 s, p95 lookup ≤250 ms, peak RSS ≤512 MiB.
- Low-RAM: ready ≤300 s, p95 lookup ≤500 ms, peak RSS ≤256 MiB under the frozen address-space limit.
- SSD: ready ≤120 s, p95 lookup ≤100 ms, peak RSS ≤512 MiB.
- Rotational HDD: ready ≤300 s, p95 lookup ≤750 ms, peak RSS ≤512 MiB.
- Production evidence additionally includes truncated index, disk shortage, update interruption, live-reader swap, rollback, CCID stability, and one complete signed release-cycle drill.
- Storage class is supported by OS evidence, not a free-form operator label.
- The Base v1 reference Registry gate uses one pinned `x86_64-unknown-linux-gnu` release target, Rust toolchain digest, runner-image digest, and byte-identical signed probe across cold-cache/low-RAM/SSD/HDD hosts. Windows/macOS collectors remain portability/preflight coverage; the separate Task 26 matrix proves those OS builds.
- `release.stamp.json` and every Registry evidence receipt must verify against an owner-approved Ed25519 Registry signer allowlist and trust-policy digest frozen in this profile. A valid signature from an unlisted key is invalid evidence; Task 19 stops until real signer public keys/fingerprints are owner-approved rather than inserting a sample value.
- Every producer consumes a closed `QualificationRunContextV1`: `Prequalification { closure_digest }` always emits `base_candidate_bound=false`; `Release { release_request_digest, qualification_session_id, candidate_commit, candidate_tree }` must match the verified signed request and is required for any production aggregate. Missing, mixed, or caller-overridden context fails closed.

- [ ] Add negative validator tests for undersized/oversized OBR, changed budgets, missing SSD/HDD OS evidence, wrong target/toolchain/probe hash, fixture evidence marked production, mismatched roots, missing/mixed release-request digest or session, a valid-but-unlisted signer, changed trust-policy digest, absent kill/live-reader gate, and quarterly update incorrectly used as a signed release cycle.
- [ ] Run `python -m unittest scripts.ci.test_validate_concept_registry_production_qualification -v`. Expected: failure because the profile/validator is missing.
- [ ] Write the focused profile and exact machine contract. Preserve the existing small-fixture and preflight profiles as non-production evidence.
- [ ] Implement the validator, link coverage, and replace the old “remaining gate” list with explicit references to this production profile without claiming completion.
- [ ] Run the focused test and `python scripts/ci/validate_vnext_contracts.py`.
- [ ] Commit:

```powershell
git add docs/specs/vnext/CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md src/test-vectors/vnext/concept-registry-production-qualification-v1.json scripts/ci/test_validate_concept_registry_production_qualification.py scripts/ci/validate_vnext_contracts.py docs/specs/vnext/normative_coverage.json docs/specs/vnext/README.md src/test-vectors/vnext/concept-registry-operations-v1.json
git commit -m "docs(registry): freeze production qualification profile"
```

### Task 20: Extend Registry resource, failure, generation-swap, and release-cycle harnesses

**Branch:** `codex/base-v1-registry`

**Files:**

- Create: `src/onebrain-node/examples/concept_registry_production_qualification.rs`
- Create: `scripts/concept_registry/release_cycle_qualification.py`
- Create: `scripts/concept_registry/test_release_cycle_qualification.py`
- Create: `scripts/concept_registry/production_qualification.py`
- Create: `scripts/concept_registry/test_production_qualification.py`
- Modify: `scripts/concept_registry/resource_qualification.py`
- Modify: `scripts/concept_registry/test_resource_qualification.py`
- Modify: `scripts/concept_registry/test_failure_qualification.py`
- Modify: `src/ku-core/examples/registry_probe.rs`
- Modify: `src/ku-core/examples/concept_registry_failure_qualification.rs`
- Modify: `src/ku-core/src/concept_registry_release.rs`
- Modify: `src/onebrain-node/src/concept_registry_runtime.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

**Aggregator rule:** `registry_production_qualified` is a Registry-only subgate derived only in `QualificationRunContextV1::Release` when every fresh report directly binds the identical signed release-request digest, qualification-session ID, final Base candidate commit/tree, candidate semantic digest, artifact tuple digest, release aggregate root, Registry generation, profile/trust-policy digest, allowlisted signer, byte-identical probe/executable hash, and candidate artifact/stamp hashes. No carry-forward wrapper is accepted for Base v1, and this subgate never implies `BASE-GATE-V1` by itself.

- [ ] Add Python tests for `ssd`/`hdd`, candidate-size bounds, missing volume evidence, report/profile/root mismatch, false subreport, duplicate report, fixture evidence, and prequalification/missing/wrong/mixed release-request/session context. Run the resource/failure tests; expect unsupported production fields.
- [ ] Add Rust tests for kill before/during/after release publication and activation-state append, old readers pinned during swap, new readers seeing only the complete new generation, rollback with active readers, and exact active root after reopen.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p ku-core concept_registry_release -- --test-threads=1` and `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --lib concept_registry_runtime -- --test-threads=1`; new tests must initially fail.
- [ ] Extend resource qualification with candidate/root/commit/host/filesystem/volume fields and platform collectors: Linux rotational sysfs, Windows physical-disk media type, and macOS storage protocol/solid-state report. Unknown production storage class fails closed.
- [ ] Implement immutable generation swap with reader leases and process-kill receipts. An interruption leaves the old or new complete generation, never mixed files/state.
- [ ] Implement `release_cycle_qualification.py` to package, verify, activate, query, build a new signed generation, perform CCID diff, activate, rollback, reactivate, and aggregate exact roots. Do not reuse `quarterly_update.py` as evidence.
- [ ] Implement `production_qualification.py` as a pure verifier/aggregator; it does not run probes and cannot turn a false report true.
- [ ] Run all focused Python/Rust suites and `python scripts/ci/validate_vnext_contracts.py`.
- [ ] Commit:

```powershell
git add src/onebrain-node/examples/concept_registry_production_qualification.rs scripts/concept_registry/release_cycle_qualification.py scripts/concept_registry/test_release_cycle_qualification.py scripts/concept_registry/production_qualification.py scripts/concept_registry/test_production_qualification.py scripts/concept_registry/resource_qualification.py scripts/concept_registry/test_resource_qualification.py scripts/concept_registry/test_failure_qualification.py src/ku-core/examples/registry_probe.rs src/ku-core/examples/concept_registry_failure_qualification.rs src/ku-core/src/concept_registry_release.rs src/onebrain-node/src/concept_registry_runtime.rs src/onebrain-node/Cargo.toml src/Cargo.lock
git commit -m "feat(registry): qualify immutable full-size generations"
```

### Task 21: Prepare and prequalify the 2.2 GB-class Registry workflow

**Branch:** `codex/base-v1-registry`

**Files:**

- Create: `.github/workflows/concept-registry-production.yml`
- Create: `scripts/runner/onebrain-registry-runner.sh`
- Create: `scripts/ci/test_validate_concept_registry_runner.py`
- Create: `docs/operations/ONEBRAIN_REGISTRY_RUNNER_GUIDE_V1.md`
- Modify: `.github/workflows/vnext-foundation.yml`
- Modify: `scripts/ci/validate_vnext_contracts.py`

**Preconditions:** the signed Registry component candidate is staged at `target/base-v1/registry/candidate/`; previous signed input/release is at `target/base-v1/registry/previous/`; SSD, HDD, and low-RAM self-hosted runners have immutable environment receipts. Compute a `registry_closure_digest` over Registry source/profile/vector/dependencies; old and new input bytes; all five previous and candidate payload artifacts; both `release.stamp.json` files; old/new release roots and generations; Registry signer keys/fingerprints; trust-policy digest; lockfile/toolchain/target/runner image; probe/runner bytes; and every release-cycle input. This digest supports prequalification comparison only and never substitutes for Task 28's fresh candidate-bound run.

- [ ] Add workflow-structure tests that require manual/self-hosted production jobs, explicit prequalification versus release mode, exact candidate and Registry-closure inputs, immutable runner labels, artifact hashing, no fixture fallback, and retention of raw reports. A release dispatch must verify a signed release request and pass its digest, qualification-session ID, candidate commit, and candidate tree as non-overridable producer inputs.
- [ ] Run `python -m unittest scripts.ci.test_validate_concept_registry_runner -v`. Expected: failure because the workflow/runner is absent.
- [ ] Implement the runner and workflow. Keep the existing fixture job in `vnext-foundation.yml`; it must continue reporting only fixture qualification.
- [ ] Build the exact release probe:

```powershell
cargo build --release --locked --manifest-path src/Cargo.toml -p ku-core --example registry_probe
```

- [ ] Run an optional full-size component prequalification for cold-cache, low-RAM, SSD, and HDD profiles. Each command writes under `target/base-v1/evidence/prequalification/registry/` and receives the same release root and `registry_closure_digest`; all production-reference hosts use the frozen target and byte-identical signed probe. It must emit `base_candidate_bound=false` and cannot satisfy Task 28 by itself.
- [ ] Run the existing failure qualification, the new process-kill/live-reader example, and exact CCID stability diff:

```powershell
python scripts/concept_registry/ccid_stability_diff.py --old-input target/base-v1/registry/previous/input.jsonl --old-obr target/base-v1/registry/previous/concepts.obr --old-manifest target/base-v1/registry/previous/concepts.obr.manifest.json --new-input target/base-v1/registry/candidate/input.jsonl --new-obr target/base-v1/registry/candidate/concepts.obr --new-manifest target/base-v1/registry/candidate/concepts.obr.manifest.json --work-dir target/base-v1/work/ccid --output target/base-v1/evidence/prequalification/registry/ccid-stability.json
```

- [ ] Run the signed release-cycle drill and `production_qualification.py` in prequalification mode. Expected: one release root, zero component mismatches, all raw report digests listed, `component_qualified=true`, and `registry_production_qualified=false` because no final Base commit is bound.
- [ ] Run validator/tests and commit only the runner, workflow, guide, and validator references. All measured reports remain external immutable artifacts; never commit an aggregate that claims to bind the commit containing itself.

```powershell
git add .github/workflows/concept-registry-production.yml .github/workflows/vnext-foundation.yml scripts/runner/onebrain-registry-runner.sh scripts/ci/test_validate_concept_registry_runner.py docs/operations/ONEBRAIN_REGISTRY_RUNNER_GUIDE_V1.md scripts/ci/validate_vnext_contracts.py
git commit -m "test(registry): prepare full-size production workflow"
```

## Phase 5 — Multi-Host P5 and Exact-Candidate Soak

### Task 22: Freeze the P5 production multi-host profile

**Branch:** `codex/base-v1-p5`

**Files:**

- Create: `docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md`
- Create: `src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json`
- Create: `scripts/ci/test_validate_vnext_p5_multi_host.py`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `docs/specs/vnext/P5_CANARY_PREFLIGHT_PROFILE_V1.md`
- Modify: `docs/specs/vnext/P5_OPERATIONS_PREFLIGHT_PROFILE_V1.md`

**Frozen topology/control:** three physical `x86_64-unknown-linux-gnu` hosts with one pinned toolchain/runner image and a byte-identical signed release agent, plus three independent durable roots/principals; A→B→C→A authenticated real-QUIC ring; SSH stdio is control-only with pinned host keys, inventory-signed TCP ports, and signed agent receipts; the inventory pins each host receipt key, orchestrator key/role, and trust-policy digest, so a valid signature from an unlisted key fails. A default-off application fault proxy injects transport faults without granting knowledge authority. Single-host/three-process runs remain preflight and always emit `multi_host_qualified=false`; Linux/Windows/macOS portability remains Task 26, not this topology.

**P5 evidence identity:** the profile/vector maps `p5-host:<host-id>` and `p5-orchestrator` roles to exact owner-approved public-key fingerprints and one trust-policy digest. Every child receipt binds its role, physical host, signed release-request digest, qualification-session ID, candidate commit/tree, candidate semantic digest, frozen Linux artifact tuple digest, agent binary digest, Registry root, profile digest, runner identity, command, and result. The aggregate root covers only canonical ordered child-receipt bytes; the aggregate report and its detached signature are excluded from the root they attest.

- [ ] Add validator tests for one physical host, shared durable root/principal, model transport, missing signed control receipt, missing/wrong/mixed release-request digest, valid-but-unlisted host/orchestrator signer, wrong role, cross-host key reuse, trust-policy/session mismatch, self-including aggregate root, fewer than three hosts, missing fault, missing before/after roots, absent resource bounds, and a report that claims production from preflight.
- [ ] Run `python -m unittest scripts.ci.test_validate_vnext_p5_multi_host -v`. Expected: failure because the profile is absent.
- [ ] Write the profile with the exact target/toolchain/image/agent identity and fault matrix: partition, drop, reorder, duplicate, restart, address change, seed outage, signer outage, disk pressure, slow peer, Base `OBARV002` archive restore, rollback, and explicit re-enable. Preserve `onebrain/p5-offline-backup/1` unchanged and explicitly classify it as preflight-only.
- [ ] Freeze exit oracles: durable reunion/idempotency, principal preservation, canonical/journal/outbox/operational roots, quiescence, bounded memory/disk/tasks, local network-off KQL, and zero truth/authority/completion/reward/wallet amplification.
- [ ] Implement the validator/coverage and run the focused test plus `validate_vnext_contracts.py`.
- [ ] Commit:

```powershell
git add docs/specs/vnext/P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md src/test-vectors/vnext/p5-multi-host-production-qualification-v1.json scripts/ci/test_validate_vnext_p5_multi_host.py scripts/ci/validate_vnext_contracts.py docs/specs/vnext/normative_coverage.json docs/specs/vnext/P5_CANARY_PREFLIGHT_PROFILE_V1.md docs/specs/vnext/P5_OPERATIONS_PREFLIGHT_PROFILE_V1.md
git commit -m "docs(p5): freeze multi-host production profile"
```

### Task 23: Build the default-off P5 host agent, fault proxy, and orchestrator

**Branch:** `codex/base-v1-p5`

**Files:**

- Create: `src/onebrain-node/src/vnext_p5_multi_host.rs`
- Create: `src/onebrain-node/src/vnext_p5_fault_proxy.rs`
- Create: `src/onebrain-node/examples/p5_multi_host_agent.rs`
- Create: `scripts/runner/onebrain-p5-multi-host.py`
- Create: `scripts/runner/test_onebrain_p5_multi_host.py`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/Cargo.toml`
- Modify: `src/Cargo.lock`

**Feature:** `vnext-production-canary-harness = ["vnext-canary-harness"]`; it is never in default features or product artifacts.

- [ ] Add Rust tests for signed control commands, replayed/stale command rejection, independent roots, exact release-request/session/Registry/compatibility binding, fault-proxy bounds, graceful quiescence, and a three-process single-host report that cannot claim multi-host qualification.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-production-canary-harness vnext_p5_multi_host -- --test-threads=1`. Expected: missing feature/module failure.
- [ ] Add Python tests with fake SSH processes for host-key mismatch, wrong executable hash, timeout, partial host result, reordered receipts, duplicate host/principal/root, fault omission, resource overflow, and aggregate-root mismatch.
- [ ] Run `python -m unittest scripts.runner.test_onebrain_p5_multi_host -v`. Expected: import/file-not-found failure.
- [ ] Implement ordinary runtime work through `BaseServices` and privileged `OBARV002` create/restore through a host-authorized, scope-limited `BaseManagementServices` handle; use the pinned Registry candidate, real QUIC, per-host durable root, signed bounded JSON control protocol, and fault proxy. Every producer/receipt carries the verified release-request digest and session in release mode. The proxy changes delivery conditions only; it cannot fabricate validation, authority, or completion. No P5 path receives raw archive paths, readers, writers, or secrets.
- [ ] Implement the orchestrator to hash the exact executable, validate inventory and pinned SSH host keys, drive the complete matrix, collect signed receipts, recompute roots, and derive `multi_host_qualified`.
- [ ] Run the Rust/Python focused suites and existing P5 preflight examples to prove preflight meaning did not change.
- [ ] Commit:

```powershell
git add src/onebrain-node/src/vnext_p5_multi_host.rs src/onebrain-node/src/vnext_p5_fault_proxy.rs src/onebrain-node/examples/p5_multi_host_agent.rs src/onebrain-node/src/lib.rs src/onebrain-node/Cargo.toml src/Cargo.lock scripts/runner/onebrain-p5-multi-host.py scripts/runner/test_onebrain_p5_multi_host.py
git commit -m "feat(p5): add signed multi-host canary harness"
```

### Task 24: Prepare the exact-candidate multi-host and 72-hour soak workflow

**Branch:** `codex/base-v1-p5`

**Files:**

- Create: `.github/workflows/vnext-p5-production-canary.yml`
- Create: `scripts/release/validate_evidence_carry_forward.py`
- Create: `scripts/release/test_validate_evidence_carry_forward.py`
- Create: `docs/operations/ONEBRAIN_BASE_V1_P5_MULTI_HOST_GUIDE.md`
- Create: `docs/specs/vnext/BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md`
- Create: `src/test-vectors/vnext/base-v1-exact-candidate-soak-v1.json`
- Create: `scripts/ci/test_validate_base_v1_soak_profile.py`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`

**Base v1 soak rule:** Task 24 retains a carry-forward analyzer to demonstrate why older evidence is stale, but Task 28 must run a fresh 72-hour soak on the exact Task 27 commit. No prior M5-07 or synthetic “unchanged closure” report can qualify Base v1.

**Soak evidence identity:** the machine profile maps `soak-runner:<runner-id>` and `soak-aggregator` roles to exact owner-approved signer fingerprints and a trust-policy digest. Each child receipt binds the release request and qualification-session ID, candidate commit/tree, semantic and frozen-target artifact digests, Registry/P5 roots, executable/SBOM/provenance digests, runner image/identity, monotonic interval, command/result, and limitations. Its aggregate root covers only canonical ordered interval/fault child receipts; the aggregate report and detached signature are outside that root.

- [ ] Add analyzer/profile tests for filename-only identity, short commit, changed archive/facade/Registry code, changed lockfile/toolchain, missing runner identity, a synthetically unchanged transitive closure, valid-but-unlisted signer, wrong/cross-runner role, changed trust policy, mixed qualification session, and self-including aggregate root. Even an unchanged closure is analytically reusable but rejected by the Base v1 fresh-soak policy.
- [ ] Run `python -m unittest scripts.release.test_validate_evidence_carry_forward scripts.ci.test_validate_base_v1_soak_profile -v`. Expected: missing validator/profile failure.
- [ ] Implement the analyzer, guide, and manual/self-hosted workflow. At production dispatch, the workflow verifies the exact signed Task 28 release request and derives its digest, qualification-session ID, candidate commit/tree, candidate semantic digest, frozen-target artifact tuple digest, and final candidate-bound Registry root as immutable inputs; callers cannot override derived identity. It checks out/builds that commit and never assumes Task 25 is final.
- [ ] Add workflow tests that require identical release executable hashes on every host, signed raw-receipt retention, candidate-commit equality, and separate outputs for multi-host and soak qualification.
- [ ] Dry-run the orchestration with three local processes. Expected: control/fault protocol passes but `multi_host_qualified=false` and `production_qualified=false`.
- [ ] Run carry-forward analysis against the existing 72-hour report as a test fixture and record rejection. Do not start or claim the real multi-host/fresh-soak run until Task 27 produces the only eligible candidate SHA.
- [ ] Run all P5 validator tests, existing preflights, and `python scripts/ci/validate_vnext_contracts.py`.
- [ ] Commit:

```powershell
git add .github/workflows/vnext-p5-production-canary.yml scripts/release/validate_evidence_carry_forward.py scripts/release/test_validate_evidence_carry_forward.py docs/operations/ONEBRAIN_BASE_V1_P5_MULTI_HOST_GUIDE.md docs/specs/vnext/BASE_V1_EXACT_CANDIDATE_SOAK_PROFILE.md src/test-vectors/vnext/base-v1-exact-candidate-soak-v1.json scripts/ci/test_validate_base_v1_soak_profile.py scripts/ci/validate_vnext_contracts.py docs/specs/vnext/normative_coverage.json
git commit -m "test(p5): prepare exact-candidate production workflow"
```

## Phase 6 — Integration, Qualification, and Freeze

### Task 25: Integrate all Base workstreams and prove cross-lane root/tuple parity

**Branch:** `codex/base-v1-freeze`

**Files:**

- Create: `src/onebrain-node/tests/base_gate_integration.rs`
- Create: `src/onebrain-base-contract/tests/cross_consumer_tuple.rs`
- Modify: `src/Cargo.toml`
- Modify: `src/Cargo.lock`
- Modify: `src/onebrain-node/src/lib.rs`
- Modify: `src/onebrain-node/src/node.rs`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `.github/workflows/vnext-foundation.yml`

**Integration order:** authority/contracts → storage → archive → Registry → Base contract/facade → P5 harness. Resolve conflicts by preserving the frozen machine contracts; never choose a workstream implementation over a newer authoritative invariant.

**Receipt:**

```rust
pub struct BaseIntegrationReceipt {
    pub candidate_semantic_digest: [u8; 32],
    pub artifact_tuple_digest: [u8; 32],
    pub canonical_root_before_restart: [u8; 32],
    pub canonical_root_after_restart: [u8; 32],
    pub archive_restore_root: [u8; 32],
    pub registry_release_root: [u8; 32],
    pub default_active_network_lanes: u16,
    pub legacy_write_enabled: bool,
}
```

- [ ] In a clean integration worktree, merge completed branch tips in the order above using non-fast-forward merge commits; record the exact tip SHA for every input branch in the integration commit message.
- [ ] Resolve `Cargo.toml`, `Cargo.lock`, exports, `node.rs` delegation, validator, and workflow conflicts under the single-owner rule. Run `cargo metadata --locked --manifest-path src/Cargo.toml --no-deps` after each manifest resolution.
- [ ] Add an end-to-end fixture: create/validate vNext records, private source/Vault state, blob, graph/retriever projections, pending operation/outbox, signed Registry generation, and all-off network status; restart, archive, restore, rebuild projections, and compare roots/tuple.
- [ ] Add cross-consumer tests that compare the tuple from Rust services, API capabilities, CLI verbose version, C ABI, and generated TypeScript/Dart fixture decoding.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node --test base_gate_integration`. Expected on the first integrated tree: failures for any unjoined ownership or root mismatch; fix only the integration boundary causing each mismatch.
- [ ] Run `cargo test --locked --manifest-path src/Cargo.toml -p onebrain-base-contract --test cross_consumer_tuple`, default Node/API/CLI tests, feature-enabled runtime tests, Registry tests, archive tests, and both contract validators.
- [ ] Assert the final receipt has three equal canonical/restore roots, zero default-active network lanes, and `legacy_write_enabled=false`.
- [ ] Commit only integration changes:

```powershell
git add src/Cargo.toml src/Cargo.lock src/onebrain-node/src/lib.rs src/onebrain-node/src/node.rs src/onebrain-node/tests/base_gate_integration.rs src/onebrain-base-contract/tests/cross_consumer_tuple.rs scripts/ci/validate_vnext_contracts.py .github/workflows/vnext-foundation.yml
git commit -m "feat(base): integrate qualification candidate"
```

### Task 26: Add three-OS candidate CI, dependency triage, SBOM, and provenance gates

**Branch:** `codex/base-v1-freeze`

**Files:**

- Create: `.github/workflows/base-v1-candidate.yml`
- Create: `scripts/release/generate_base_sbom.py`
- Create: `scripts/release/test_generate_base_sbom.py`
- Create: `scripts/release/verify_base_provenance.py`
- Create: `scripts/release/test_verify_base_provenance.py`
- Create: `docs/security/BASE_V1_DEPENDENCY_AUDIT.md`
- Create: `docs/operations/ONEBRAIN_BASE_V1_CANDIDATE_RUNBOOK.md`
- Modify: `scripts/ci/validate_vnext_contracts.py`

**Required matrix:** Linux, Windows, and macOS; default Base and `vnext-network-runtime`; format, check, clippy with denied warnings, tests, generated-contract drift, vNext/mobile validators, archive/recovery, Registry fixtures, P5 preflights, and legacy-disabled scan. Release-mode dispatch verifies the signed Task 27 request and derives its request digest, qualification-session ID, candidate commit, and tree; jobs cannot accept independently supplied identity fields.

- [ ] Add SBOM tests that feed synthetic `cargo metadata --locked --manifest-path src/Cargo.toml` and npm lock data; assert deterministic SPDX package IDs, licenses, checksums, dependency edges, target/toolchain/candidate binding, and rejection of missing/duplicate packages.
- [ ] Add provenance tests for wrong/mixed release-request digest or session, wrong commit/tree, dirty tracked/ignored source tree, mutable/unknown action reference, mismatched executable/SBOM digest, missing OS lane, and untriaged P0/P1 audit item.
- [ ] Run the two Python suites. Expected: import/file-not-found failures.
- [ ] Implement deterministic SPDX generation and provenance verification. Preserve raw `cargo audit --file src/Cargo.lock --json`, npm audit, compiler, target, runner image, workflow, and artifact digest data; classification is documented in `BASE_V1_DEPENDENCY_AUDIT.md`.
- [ ] Create the matrix workflow with least permissions, concurrency control, locked dependency commands, immutable artifacts, explicit timeouts, and a required signed-release-request artifact/input for release mode. Resolve each third-party action tag to a reviewed full commit SHA during implementation and record the tag-to-SHA mapping in the runbook.
- [ ] Require all three OS jobs to emit one identical candidate semantic digest and their own target/toolchain-bound artifact tuple digest; reject an OS artifact that copies another target's tuple. Run the pinned TypeScript and Dart conformance harnesses in the matrix as non-mobile contract tests.
- [ ] Include these core commands on every supported OS where applicable:

```powershell
cargo fmt --all --manifest-path src/Cargo.toml -- --check
cargo check --workspace --locked --manifest-path src/Cargo.toml
cargo clippy --workspace --all-targets --locked --manifest-path src/Cargo.toml -- -D warnings
cargo test --workspace --locked --manifest-path src/Cargo.toml
cargo check --locked --manifest-path src/Cargo.toml -p onebrain-node -p onebrain-api -p onebrain-cli --features onebrain-node/vnext-network-runtime,onebrain-api/vnext-network-runtime,onebrain-cli/vnext-network-runtime
cargo test --locked --manifest-path src/Cargo.toml -p onebrain-node -p onebrain-api -p onebrain-cli --features onebrain-node/vnext-network-runtime,onebrain-api/vnext-network-runtime,onebrain-cli/vnext-network-runtime
python scripts/ci/validate_vnext_contracts.py
python scripts/ci/validate_mobile_build_contracts.py
python scripts/base/generate_contract.py --check
```

- [ ] Add focused workflow-structure validation to `validate_vnext_contracts.py`; run all local tests and a workflow lint/parse check.
- [ ] Commit:

```powershell
git add .github/workflows/base-v1-candidate.yml scripts/release/generate_base_sbom.py scripts/release/test_generate_base_sbom.py scripts/release/verify_base_provenance.py scripts/release/test_verify_base_provenance.py docs/security/BASE_V1_DEPENDENCY_AUDIT.md docs/operations/ONEBRAIN_BASE_V1_CANDIDATE_RUNBOOK.md scripts/ci/validate_vnext_contracts.py
git commit -m "ci(base): add candidate supply-chain gates"
```

### Task 27: Implement the Base evidence manifest, freeze policy, and release documents

**Branch:** `codex/base-v1-freeze`

**Files:**

- Create: `docs/specs/vnext/BASE_V1_FREEZE_AND_EVIDENCE_PROFILE.md`
- Create: `src/test-vectors/vnext/base-v1-freeze-v1.json`
- Create: `scripts/base/qualify_base.py`
- Create: `scripts/base/test_qualify_base.py`
- Create: `docs/security/BASE_V1_RELEASE_SIGNER_POLICY.md`
- Create: `src/test-vectors/vnext/base-v1-release-signers-v1.json`
- Create: `scripts/release/create_base_release_request.py`
- Create: `scripts/release/test_create_base_release_request.py`
- Create: `scripts/release/prepare_clean_candidate.py`
- Create: `scripts/release/test_prepare_clean_candidate.py`
- Create: `scripts/release/create_verified_base_release.py`
- Create: `scripts/release/test_create_verified_base_release.py`
- Create: `docs/operations/ONEBRAIN_BASE_V1_MIGRATION_GUIDE.md`
- Create: `docs/operations/ONEBRAIN_BASE_V1_ROLLBACK_GUIDE.md`
- Create: `docs/operations/ONEBRAIN_BASE_V1_CHANGELOG.md`
- Modify: `scripts/ci/validate_vnext_contracts.py`
- Modify: `docs/specs/vnext/normative_coverage.json`
- Modify: `src/onebrain-base-contract/src/compatibility.rs`

**Evidence manifest minimum:** candidate commit/tree, candidate semantic digest, per-target full compatibility tuples/artifact digests, schema/domain/resource/storage/archive/Registry/wire/API/ABI versions, feature/default/kill-switch matrix, three-OS job receipts, canonical/vector tests, blob/index tests, archive/recovery/kill tests, transaction boundaries, projection conformance, fresh full-size Registry aggregate, fresh multi-host P5 aggregate, fresh exact-candidate 72-hour soak, dependency audits/triage, SBOM/provenance, migration/rollback/changelog, raw evidence hashes, limitations, child-evidence signer fingerprints/roles/trust-policy digests, and signatures on child evidence. The manifest's own detached signature is an outer envelope and is never a field in the manifest bytes/hash it signs.

```python
def qualify_base(inputs: QualificationInputs) -> BaseEvidenceManifest:
    """Derive qualified; never accept it as an input field."""
```

- [ ] Add tests that remove or falsify every gate, alter an evidence byte, mix candidate/semantic/artifact/Registry roots or qualification-session IDs, duplicate a report/target, omit Linux/Windows/macOS, swap Windows/Linux artifacts, mismatch binary/SBOM/provenance digests, use an unsupported profile, attempt any Registry/soak carry-forward, omit a security lane, use a valid signature from an unlisted/wrong-role key, or set `qualified=true` in an input. Every case must fail.
- [ ] Run `python -m unittest scripts.base.test_qualify_base -v`. Expected: missing qualifier/profile failure.
- [ ] Implement the pure verifier/manifest builder. It recomputes hashes and derives `qualified`; it never shells out to run tests and never trusts a target file's claim.
- [ ] Write the freeze profile and release documents with exact upgrade/downgrade, legacy read-only migration, archive restore, Registry rollback, network kill, signer roles, and Base v2 reopening rules. Populate the signer vector only with real owner-approved full fingerprints for `qualification-approver` and `base-release`; if either is unavailable or unapproved, Task 27 stops rather than using the local default key or a sample fingerprint.
- [ ] Implement and test `create_base_release_request.py`. After Task 27 commits, it creates canonical external bytes containing a random 256-bit qualification-session ID, exact candidate commit/tree/object format, signer/trust-policy digest, required target map, profile/vector and append-only IDL-history roots, creation/expiry policy, evidence-root URI, and BLAKE3 digests of the exact candidate-owned qualifier/request/clean-worktree/release-wrapper/verifier scripts plus signer policy. It signs with the explicit allowlisted `qualification-approver` key. New attempts live under `target/base-v1/release-requests/<request-digest>/`; no file is overwritten. `--resume` accepts only an existing byte/signature-identical request and preserves failed attempts for audit. The verifier parses `VALIDSIG`, and all later reports bind request digest/session/tooling digest.
- [ ] Implement and test `prepare_clean_candidate.py`: verify the signed release request and bound tooling digests first, create a detached linked worktree in a newly created OS-temporary directory outside the source worktree, check out only the requested commit, verify its tree, compare the filesystem against `git ls-tree -r`/`git ls-files`, reject `git status --porcelain --untracked-files=all --ignored=matching` output other than the worktree administrative file, redirect every build/cache/evidence output outside, and make tracked source read-only for the qualification duration. Tests prove dirty/untracked/ignored/generated source files cannot enter or later mutate the candidate and every subprocess exit code is checked.
- [ ] Implement and test `create_verified_base_release.py`: it selects the allowlisted `base-release` fingerprint explicitly, signs the immutable ready manifest to a temporary file, parses GPG `VALIDSIG`, publishes the verified detached signature in a create-new content-addressed release envelope, then atomically publishes a checksummed release-ready pointer without mutating the manifest generation. It constructs the annotated tag bytes with internal name `base-v1.0.0`, signs them, writes an **unreferenced** tag object, verifies that object and all bindings, then uses one `git update-ref` compare-and-swap from an absent final ref to the already verified object. A crash before CAS leaves no tag ref; the script never deletes or overwrites a pre-existing tag. If a retry finds an existing ref, it returns idempotent `AlreadyPublished` only when tag object, request, manifest/release-ready digests, signature, signer, and target commit all match exactly; every other existing ref hard-fails. Tests use isolated repositories/keyrings and inject failures before envelope readiness, before object write, before CAS, and after CAS receipt fsync, plus exact retry, wrong-but-valid key, wrong role, stale digest, and foreign existing-ref cases.
- [ ] Change the candidate tuple from `1.0.0-rc.1` to `1.0.0`. Keep the separate runtime/version status `Unqualified` until a valid external manifest binds this exact commit, candidate semantic digest, and per-target artifact digests; the version change is not a completion claim and does not alter the tuple after Task 27.
- [ ] Run qualifier unit tests, contract validators, and the compatibility/vector suites.
- [ ] Commit; this commit becomes the only eligible candidate SHA for Task 28:

```powershell
git add docs/specs/vnext/BASE_V1_FREEZE_AND_EVIDENCE_PROFILE.md src/test-vectors/vnext/base-v1-freeze-v1.json scripts/base/qualify_base.py scripts/base/test_qualify_base.py docs/security/BASE_V1_RELEASE_SIGNER_POLICY.md src/test-vectors/vnext/base-v1-release-signers-v1.json scripts/release/create_base_release_request.py scripts/release/test_create_base_release_request.py scripts/release/prepare_clean_candidate.py scripts/release/test_prepare_clean_candidate.py scripts/release/create_verified_base_release.py scripts/release/test_create_verified_base_release.py docs/operations/ONEBRAIN_BASE_V1_MIGRATION_GUIDE.md docs/operations/ONEBRAIN_BASE_V1_ROLLBACK_GUIDE.md docs/operations/ONEBRAIN_BASE_V1_CHANGELOG.md scripts/ci/validate_vnext_contracts.py docs/specs/vnext/normative_coverage.json src/onebrain-base-contract/src/compatibility.rs
git commit -m "chore(base): cut v1.0.0 qualification candidate"
```

### Task 28: Qualify the exact commit and create the signed `base-v1.0.0` tag

**Branch:** `codex/base-v1-freeze`

**Generated release artifacts, not candidate-source edits:**

- `target/base-v1/release-requests/<request-digest>/release-request.json`
- `target/base-v1/release-requests/<request-digest>/release-request.json.asc`
- `target/base-v1/evidence/sessions/<qualification-session-id>/manifest-generations/<manifest-digest>/manifest.json`
- `target/base-v1/evidence/sessions/<qualification-session-id>/manifest-generations/<manifest-digest>/manifest.blake3`
- `target/base-v1/evidence/sessions/<qualification-session-id>/manifest.ready.json`
- `target/base-v1/evidence/sessions/<qualification-session-id>/release-envelopes/<manifest-digest>/<signature-digest>/manifest.json.asc`
- `target/base-v1/evidence/sessions/<qualification-session-id>/release.ready.json`
- `target/base-v1/evidence/sessions/<qualification-session-id>/base-v1.spdx.json`
- `target/base-v1/evidence/sessions/<qualification-session-id>/qualification/registry-production.json`
- `target/base-v1/evidence/sessions/<qualification-session-id>/qualification/p5-production.json`
- `target/base-v1/evidence/sessions/<qualification-session-id>/qualification/soak-72h.json`

- [ ] After Task 27 commits, use only minimal Git commands to bootstrap a pristine detached worktree at that commit. Run request/preparation tooling and signer policy by absolute path from the verified bootstrap, create and owner-sign one content-addressed immutable attempt, then create the read-only qualification worktree. A retry either resumes the exact verified request/session or creates a new request/session while preserving the old attempt. Every native invocation checks `$LASTEXITCODE`; the signed request—not ambient scripts or `HEAD`—is authoritative:

```powershell
$sourceRoot = (Get-Location).Path
$task27CandidateCommit = (git -C $sourceRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "Task 27 candidate commit read failed" }
$task27CandidateTree = (git -C $sourceRoot rev-parse 'HEAD^{tree}').Trim()
if ($LASTEXITCODE -ne 0) { throw "Task 27 candidate tree read failed" }
$bootstrapRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("onebrain-base-bootstrap-" + [guid]::NewGuid().ToString("N"))
git -C $sourceRoot worktree add --detach $bootstrapRoot $task27CandidateCommit
if ($LASTEXITCODE -ne 0) { throw "candidate bootstrap worktree failed" }
$bootstrapCommit = (git -C $bootstrapRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $bootstrapCommit -ne $task27CandidateCommit) { throw "bootstrap commit mismatch" }
$bootstrapTree = (git -C $bootstrapRoot rev-parse 'HEAD^{tree}').Trim()
if ($LASTEXITCODE -ne 0 -or $bootstrapTree -ne $task27CandidateTree) { throw "bootstrap tree mismatch" }
$bootstrapStatus = git -C $bootstrapRoot status --porcelain --untracked-files=all --ignored=matching
if ($LASTEXITCODE -ne 0 -or $bootstrapStatus) { throw "bootstrap worktree is not pristine" }
$bootstrapRequestTool = Join-Path $bootstrapRoot "scripts/release/create_base_release_request.py"
$bootstrapPrepareTool = Join-Path $bootstrapRoot "scripts/release/prepare_clean_candidate.py"
$bootstrapSignerPolicy = Join-Path $bootstrapRoot "src/test-vectors/vnext/base-v1-release-signers-v1.json"
$releaseRequestRoot = Join-Path $sourceRoot "target/base-v1/release-requests"
$baseReleaseRequest = (& python $bootstrapRequestTool --new-attempt --candidate-commit $task27CandidateCommit --output-root $releaseRequestRoot --signer-policy $bootstrapSignerPolicy).Trim()
if ($LASTEXITCODE -ne 0) { throw "release request creation failed" }
$baseReleaseRequestSignature = "$baseReleaseRequest.asc"
& python $bootstrapRequestTool --verify $baseReleaseRequest --signature $baseReleaseRequestSignature --signer-policy $bootstrapSignerPolicy
if ($LASTEXITCODE -ne 0) { throw "release request verification failed" }
$baseCandidateCommit = (& python $bootstrapRequestTool --print candidate_commit --request $baseReleaseRequest).Trim()
if ($LASTEXITCODE -ne 0) { throw "candidate extraction failed" }
$baseCandidateTree = (& python $bootstrapRequestTool --print candidate_tree --request $baseReleaseRequest).Trim()
if ($LASTEXITCODE -ne 0) { throw "candidate tree extraction failed" }
$qualificationSessionId = (& python $bootstrapRequestTool --print qualification_session_id --request $baseReleaseRequest).Trim()
if ($LASTEXITCODE -ne 0) { throw "session extraction failed" }
$sessionEvidenceRoot = Join-Path ([System.IO.Path]::GetFullPath("target/base-v1/evidence/sessions")) $qualificationSessionId
if (Test-Path -LiteralPath $sessionEvidenceRoot) { throw "qualification session directory already exists" }
New-Item -ItemType Directory -Path $sessionEvidenceRoot -ErrorAction Stop | Out-Null
$baseCandidateRoot = (& python $bootstrapPrepareTool --source-root $sourceRoot --release-request $baseReleaseRequest --signature $baseReleaseRequestSignature --signer-policy $bootstrapSignerPolicy --read-only).Trim()
if ($LASTEXITCODE -ne 0) { throw "clean candidate preparation failed" }
$actualCandidateCommit = (git -C $baseCandidateRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "candidate commit read failed" }
if ($actualCandidateCommit -ne $baseCandidateCommit) { throw "candidate commit mismatch" }
$actualCandidateTree = (git -C $baseCandidateRoot rev-parse 'HEAD^{tree}').Trim()
if ($LASTEXITCODE -ne 0) { throw "candidate tree read failed" }
if ($actualCandidateTree -ne $baseCandidateTree) { throw "candidate tree mismatch" }
$candidateStatus = git -C $baseCandidateRoot status --porcelain --untracked-files=all --ignored=matching
if ($LASTEXITCODE -ne 0) { throw "candidate status failed" }
if ($candidateStatus) { throw "candidate worktree is not clean" }
$candidateRequestTool = Join-Path $baseCandidateRoot "scripts/release/create_base_release_request.py"
$candidatePrepareTool = Join-Path $baseCandidateRoot "scripts/release/prepare_clean_candidate.py"
$candidateQualifier = Join-Path $baseCandidateRoot "scripts/base/qualify_base.py"
$candidateReleaseTool = Join-Path $baseCandidateRoot "scripts/release/create_verified_base_release.py"
$candidateSignerPolicy = Join-Path $baseCandidateRoot "src/test-vectors/vnext/base-v1-release-signers-v1.json"
& python $candidatePrepareTool --verify-only --candidate-root $baseCandidateRoot --release-request $baseReleaseRequest --signature $baseReleaseRequestSignature --signer-policy $candidateSignerPolicy
if ($LASTEXITCODE -ne 0) { throw "candidate filesystem/tooling verification failed" }
```

- [ ] Run the complete local gate from Tasks 1-27 **inside `$baseCandidateRoot`**, with Cargo/build/tool caches redirected to the session work directory and evidence written only beneath `$sessionEvidenceRoot`. Include `git diff --check`, both validators, generated drift, default/feature Rust matrices, Node/API/CLI black-box tests, archive/recovery failpoints, index/blob tests, Registry fixtures, and P5 preflights. Save command, output, environment, exit code, release-request digest, and session ID as raw receipts; abort on the first nonzero exit.
- [ ] Complete the `base-v1-candidate.yml` Linux/Windows/macOS matrix for the release request's commit/tree/session; verify one semantic digest, the exact three-target artifact map, every job artifact digest, and no skipped required lane.
- [ ] Build the frozen-target Registry probe from `$baseCandidateRoot` and rerun every cold-cache, low-RAM, SSD, HDD, failure, live-reader, release-cycle, and CCID-stability profile into the new session directory. Task 21 prequalification is rehearsal only and is never consumed. Require all fresh reports to bind the release-request digest/session, candidate commit/tree, semantic/artifact digests, allowlisted signer/trust policy, byte-identical probe, and `registry_production_qualified=true`.
- [ ] Build the frozen-target release P5 agent from `$baseCandidateRoot`, distribute the byte-identical signed binary to the three physical hosts, and run the real canary. The pure aggregator must recompute every child/root/signature and bind release request/session, candidate commit/tree, semantic digest, Linux artifact tuple, agent binary digest, Registry root, profile/trust-policy digest, physical runner identities, and authorized host/orchestrator signers before deriving `multi_host_qualified=true`; it never trusts the input boolean.
- [ ] Generate and verify the SBOM/provenance, complete dependency/security review, and freeze their digests before soak admission. Any untriaged Base P0/P1 stops the task; the soak runner receives these already-final digests rather than a future promise.
- [ ] Record the expected rejection from `validate_evidence_carry_forward.py`, then complete a new uninterrupted exact-candidate 72-hour soak in the same session. Require its signed report to bind the release request/session, commit/tree, semantic/artifact digests, Registry/P5 roots, executable/SBOM/provenance digests, runner identities/signers/trust policy, and environment; no old report or carry-forward receipt is accepted for Base v1. After soak, recompute and verify the executable/SBOM/provenance bytes are unchanged before manifest construction.
- [ ] Build and verify the manifest:

```powershell
$manifestGenerationRoot = Join-Path $sessionEvidenceRoot "manifest-generations"
$manifestReady = Join-Path $sessionEvidenceRoot "manifest.ready.json"
& python $candidateQualifier --release-request $baseReleaseRequest --release-request-signature $baseReleaseRequestSignature --evidence-root $sessionEvidenceRoot --output-generation-root $manifestGenerationRoot --ready-output $manifestReady
if ($LASTEXITCODE -ne 0) { throw "qualification failed" }
& python $candidateQualifier --verify-ready $manifestReady --release-request $baseReleaseRequest
if ($LASTEXITCODE -ne 0) { throw "manifest verification failed" }
```

Expected: `BASE-GATE-V1 PASS`, `qualified=true`, candidate commit equals `$baseCandidateCommit`, and every referenced artifact hash resolves.

- [ ] Require the qualifier to create-new exactly two files under `manifest-generations/<digest>/`, fsync each file and the generation directory, verify the sidecar equals a fresh BLAKE3, then atomically create/publish one checksummed `manifest.ready.json` pointer after all bytes are durable. Missing, partial, extra, or mismatched generations are never ready, and no byte/file is ever added there after readiness. The release wrapper resolves only this pointer and re-verifies the immutable generation; no command may use the default GPG key:

```powershell
$releaseEnvelopeRoot = Join-Path $sessionEvidenceRoot "release-envelopes"
$releaseReady = Join-Path $sessionEvidenceRoot "release.ready.json"
& python $candidateReleaseTool --release-request $baseReleaseRequest --release-request-signature $baseReleaseRequestSignature --manifest-ready $manifestReady --release-envelope-root $releaseEnvelopeRoot --release-ready-output $releaseReady --signer-policy $candidateSignerPolicy --signer-role base-release --tag base-v1.0.0
if ($LASTEXITCODE -ne 0) { throw "verified release publication failed" }
```

- [ ] The wrapper writes the detached signature only into a create-new content-addressed `release-envelopes/<manifest-digest>/<signature-digest>/` directory, fsyncs it, and atomically publishes `release.ready.json` before constructing/verifying the unreferenced tag object. It never mutates the manifest generation. Retries accept only the exact ready manifest/envelope pair; partial or foreign envelopes remain non-ready evidence.

- [ ] Before signing, re-run `git -C $baseCandidateRoot status --porcelain --untracked-files=all --ignored=matching`, commit/tree verification, and candidate-owned `prepare_clean_candidate.py --verify-only` filesystem/tooling comparison plus the read-only manifest/release-request binding check; any tracked, untracked, ignored, generated, tooling, policy, or request mutation aborts publication.
- [ ] Accept wrapper success or idempotent `AlreadyPublished` only when the manifest and tag both produce `VALIDSIG` for the same allowlisted `base-release` fingerprint, the tag points to `$baseCandidateCommit`, and a freshly recomputed manifest BLAKE3 equals both the sidecar and tag message. `AlreadyPublished` additionally requires the exact same request/session/tag object/signature; an unrelated existing ref remains a hard failure. Re-run the read-only release-binding verifier and confirm API/CLI emit the candidate commit plus semantic/artifact digests. If any check or signing key is unavailable, the wrapper must leave `base-v1.0.0` absent unless that exact verified tag was already atomically published.
- [ ] Do not push the tag or publish release assets without separate owner publication authorization. When authorized, publish the manifest, signature, SBOM, provenance, aggregate evidence, changelog, migration guide, and rollback guide together.

## Spec-to-Task Coverage

| Program workstream | Implementation tasks | Exit evidence |
|---|---:|---|
| WS-00 Authority closure | 1, 14, 19, 22 | frozen profiles and negative validators |
| WS-01 Canonical freeze audit | 2-3 | Base registry digest and cross-crate conformance |
| WS-10 Storage/blob/index integrity | 4-8 | collision, integrity, quota, parity, rebuild, Unicode, exchange suites |
| WS-11 Recovery/archive | 9-13 | encrypted clean-directory round trip and kill/negative matrix |
| WS-20 Runtime facade | 14-18 | generated projection and N-1 semantic conformance |
| WS-21 Registry kernel | 19-21, 28 | exact 2.2 GB-class candidate-bound aggregate report |
| WS-22 Distributed/P5 | 22-24, 28 | exact-commit three-host report and soak decision |
| WS-23 Integration | 25 | common roots, tuple, lifecycle, and all-off network receipt |
| WS-30 Freeze | 26-28 | three-OS matrix, SBOM, manifest, signed tag |

## Completion Checklist

- [ ] All 28 tasks have focused red/green or qualification evidence; Tasks 1-27 have reviewable commits, and Task 28 has an immutable external manifest plus signed tag on the unchanged Task 27 commit.
- [ ] No unresolved marker, fake recovery, short blob path, plaintext backup, fatal derived-index startup, or silent legacy fallback remains in Base-owned paths.
- [ ] `python scripts/ci/validate_vnext_contracts.py` and `python scripts/ci/validate_mobile_build_contracts.py` pass on the candidate.
- [ ] All `BASE-GATE-V1` criteria from the approved program design are represented by a successful, candidate-bound manifest entry.
- [ ] The signed tag points to the exact source commit tested by Registry, P5, soak, compatibility, security, and three-OS gates.
- [ ] Only after this checklist passes may separate Desktop/Web/CLI product-completion and Mobile Offline RC implementation plans begin.

## Execution Handoff

Recommended execution is **Subagent-Driven Development** in the current task: one fresh implementation agent per task, contract/compliance review after each task, and serial integration at each wave boundary. The alternative is **Inline Execution** in a separate session using `superpowers:executing-plans`, with review checkpoints after Tasks 3, 8, 13, 18, 21, 24, 27, and 28.
