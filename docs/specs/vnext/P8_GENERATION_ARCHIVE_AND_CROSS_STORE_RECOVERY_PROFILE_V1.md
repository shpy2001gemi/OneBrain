# Pillar 8 Specification — Generation-Scoped Archive, Cross-Store Recovery, and Tombstone Epoch Profile v1

> - **Profile ID:** `P8-OBS-RECOVERY-001`
> - **Pillar:** `P8` (OneBrain Storage)
> - **Status:** Specified (additive vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [VALIDATED_STORAGE_PROFILE_V1](VALIDATED_STORAGE_PROFILE_V1.md), [BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE](BASE_V1_AUTHORITY_AND_RECOVERY_PROFILE.md), [BASE_V1_STORAGE_INTEGRITY_PROFILE](BASE_V1_STORAGE_INTEGRITY_PROFILE.md), [DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1](DISTRIBUTED_RUNTIME_TRANSACTION_BOUNDARY_INVENTORY_V1.md), [PRIVATE_KQL_PERSISTENCE_PROFILE_V1](PRIVATE_KQL_PERSISTENCE_PROFILE_V1.md), [OPERATIONAL_COMPACTION_PROFILE_V1](OPERATIONAL_COMPACTION_PROFILE_V1.md), [CHECKPOINT_COMPACTION_AND_LOCAL_GC_PROFILE_V1](CHECKPOINT_COMPACTION_AND_LOCAL_GC_PROFILE_V1.md), [FEED_CHECKPOINT_PROOF_PROFILE_V1](FEED_CHECKPOINT_PROOF_PROFILE_V1.md)
> - **Owner decisions addressed:** `D-045`, `D-062`

## 1. Purpose and Scope

Existing P8 contracts freeze `ValidatedStore` and `PrivateVault` (`OBS-001/002`), Base v1 `OBARV002` archive custody (`BASE-V1`), operational compaction (`M5-05`), and checkpoint-anchored local GC (`CHK-003..006`). Three specification gaps remained open in `OBS_SPEC.md §7.4` and runtime storage lifecycle:

1. **Complete `OBARV002` Archive Class Closure (`0x0002` & `0x000B`–`0x000F`):** Defining exact serialization, key-wrapping, anti-equivocation sequence high-water floors, and restore fences for the five deferred distributed-runtime archive classes plus `ku-product-v1/journal.redb`.
2. **Deterministic Multi-Redb Crash Recovery Ordering:** Establishing a single canonical four-phase startup reconciliation sequence across the node's isolated Redb databases so process kills between cross-store steps never leave orphaned vaults, resurrected tombstones, or stale indexes.
3. **Proof-Anchored Terminal Tombstone Epoch Rollover (`ARCH-03` / `ARCH-15` / `M5-05`):** Resolving the `65,536` terminal receipt/tombstone ceiling without weakening anti-replay or violating `CHK-003..006` retention anchors.

---

## 2. Complete `OBARV002` Archive & Restore Closure (`0x0002`, `0x000B`–`0x000F`)

### 2.1 Shared Generation-Scoped Envelope Rules

1. Every archive class **MUST** seal inside the authenticated `OBARV002` container (`BASE_V1_STORAGE_INTEGRITY_PROFILE.md`) using XChaCha20-Poly1305 per-chunk AEAD bound to `(archive_id, archive_generation_id, archive_class_id, chunk_seq)`.
2. Raw private signing keys (`ActorRootKey`, `FeedEventSigner` Ed25519 secret key, `SessionIdentitySigner` secret key) **MUST NOT** be exported into any `OBARV002` payload (`DR-P1.2`, `NET-002`).
3. Restoring an archive onto a host **MUST** advance `local_restore_epoch + 1` atomically before any network or API surface opens.

### 2.2 Normative Class Specifications

| Archive Class ID | Backing Store / Domain | Exported Durable Records | Mandatory Restore Invariant |
|---|---|---|---|
| **`0x0002` (`ku_product_journal`)** | `ku-product-v1/journal.redb` (`purpose = 4`) | Private KU revision lineage (`KU-PC-C`), `D-062` share-prepare records, `ARCH-03` terminal receipts | In-flight uncommitted `D-062` share-prepare previews **MUST** transition to `InvalidatedAcrossRestore` (requiring fresh user prepare/confirm); committed revision lineage and terminal receipts verify 1:1 against `PrivateVault` / `ValidatedStore` CIDs. |
| **`0x000B` (`private_kql`)** | `RedbPrivateNeedVault` (`DR-P1.4`) | Encrypted `PrivateNeedBundle`s, `StandingNeed` pause flags, match cursors, and **all** cancel/retire tombstones | `LocalNeedVaultKey` **MUST** be wrapped under `onebrain:archive:need-vault-wrap:1\0` inside the archive recovery header; restore **MUST** load cancel/retire tombstones *before* active Needs so retired Needs never resurrect. |
| **`0x000C` (`private_pomv`)** | `TX-PUSE-001` & `M6B` PoMV journals | Per-`FeedId` `highest_issued_seq` floor map, `PublicUseConsent` single-use consumption tombstones, `DeferredEvidenceStaging` records, PoMV view watermarks | **Anti-Equivocation Floor (`NEG-P8-001`):** Restore **MUST** install `max(local_seq_floor, archive_highest_issued_seq)` for every local `FeedId` before `FeedEventSigner` accepts any new signing request, preventing post-restore feed sequence fork/equivocation. |
| **`0x000D` (`operational`)** | Operational journal & `M5-05` metadata | Command idempotency tombstones, `TombstoneEpochAccumulatorV1` / `OverflowSummaryRoot`s, `D-013` job claim leases, `P5` `NoticeOutbox` watermarks | Active pre-backup `D-013` worker leases restore as `RecoveryInspectionRequired` (never silently double-dispatched). |
| **`0x000E` (`rollout`)** | `TX-ROL-001` durable rollout state | Per-lane kill switches, schema-generation high-water marks, `M5-06` N-1 downgrade fences | Restore **MUST** compute the conservative union of active kill switches and maximum schema-generation fences; restoring an older backup **MUST NOT** silently re-enable a locally killed feature lane or bypass `M5-06` version guards. |
| **`0x000F` (`optional_network`)** | Route, retirement & replay floors | `ProviderRetirementFloor` (`DHT-001`), `RoutePacketReplayGuard` epoch floor (`M6A`), `CognitiveTaskReplayGuard` watermark (`QA-005`), OBP journal continuation salt | Pre-backup OBP session MAC continuation tokens **MUST** be invalidated by rotating `reconciliation_restore_salt` on restore so peers cleanly re-run `Hello`/manifest rebinding (`OBP-005`) instead of replaying stale cursors. |

---

## 3. Deterministic Four-Phase Cross-Store Crash Recovery

Because OneBrain isolates failure domains across dedicated Redb files (`ValidatedStore`, `PrivateVault`, `QuarantineStore`, `BlobCatalog`, `ku-product/journal`, `PrivateNeedVault`, `PoMVJournal`, `ReconciliationJournal`, `OperationalStore`, `RebuildableIndexes`), node startup and post-crash recovery **MUST** execute in four strict sequential phases before serving requests (`DR-P2.3`):

1. **Phase R1 — Rollout Fences, Authority Frontiers & Sequence Floors:**
   - Open `0x000E` rollout/version fences, `FeedState` key-state/equivocation frontiers (`FEED-001/002`), and `0x000C`/`0x000F` monotonic sequence/retirement/replay high-water floors. Any CRC, AEAD, or schema-downgrade violation **MUST** halt startup fail-closed (`STORAGE_INTEGRITY_FAILURE`).
2. **Phase R2 — Canonical Custody, Vaults, Blobs & Quarantine:**
   - Open and verify `ValidatedStore` (`OBS-001`), `PrivateVault` (`OBS-002` `purpose` separation), `QuarantineStore`, and `BlobCatalog` (`P2` 34-byte `BlobCid` Merkle root & staging bitmaps). Unverified staging chunks remain isolated from readers.
3. **Phase R3 — Cross-Store Journal & Outbox Reconciliation:**
   - Reconcile `ku-product/journal`, `PublicUseConsent` / `M6B` journals, `PrivateNeedVault` tombstones, `ReconciliationJournal`, and `P5` `NoticeOutbox` against Phase R2 custody:
     - If a journal entry is in `Prepared` state and its target `ObjectCid`/`EventCid` is **already durable** in Phase R2, roll the journal entry forward to `Committed`.
     - If a `Prepared` consent/share receipt has **no** committed Phase R2 artifact and its TTL or restore generation has elapsed, transition it deterministically to `AbortedExpired` without partial residue.
4. **Phase R4 — Disposable Derived Indexes & Views:**
   - Resume or rebuild `P3` `RebuildableSemanticIndex`, `P7` `ObkgProjectionSnapshot`, and `P4` `Metabolic`/`Outcome`/`Fidelity` views from monotonic commit sequence cursors (`DR-P2.4`). Corrupt derived view files **MUST** be deleted and rebuilt automatically from Phase R2 stores.

---

## 4. Proof-Anchored Terminal Tombstone Epoch Rollover & Local GC

### 4.1 Resolving the `65,536` Terminal Receipt/Tombstone Ceiling (`ARCH-03` / `ARCH-15`)

To prevent bounded terminal tables (`65,536` entries in `ku-product`, `PublicUseConsent`, `PrivateNeedVault`, and operational idempotency) from either forgetting anti-replay state or permanently rejecting new user work:

1. **Watermark-Triggered Epoch Seal (`49,152 / 65,536`):** When terminal tombstones in a table reach `75%` capacity (`49,152` entries), `CompactionCoordinator` (`M5-05`) selects the oldest `32,768` entries whose retention age exceeds `min_hot_tombstone_window` (`7 days` logical/monotonic floor) and constructs a canonical `TombstoneEpochAccumulatorV1`:
   - `epoch_seq: u64` (strictly monotonic),
   - `cutoff_sequence_floor: u64` (monotonic local op sequence covered by the epoch),
   - `tombstone_count: u32`,
   - `sorted_tombstone_merkle_root: [u8; 32]` (`BLAKE3("onebrain:obs:tombstone-epoch:1\0" || ...)`),
   - `bloom_filter_256kb: Bytes` (zero-false-negative fast non-membership prefilter for cold CIDs).
2. **Atomic Rollover & Two-Tier Anti-Replay Verification:**
   - Within a single Redb write transaction, the store persists the cold sorted tombstone page to `blobs/cold-tombstones/<epoch_seq>.cbor`, commits `TombstoneEpochAccumulatorV1` to `0x000D` `OverflowSummaryRoot`, and prunes the `32,768` compacted rows from the hot Redb B-tree.
   - Any incoming command with `op_seq <= cutoff_sequence_floor` rejects in $O(1)$ (`BELOW_COMPACTED_EPOCH_FLOOR`). Any exact-CID idempotency check that hits `bloom_filter_256kb` verifies exact inclusion against the cold Merkle page before accepting, preserving **100% lossless anti-replay** with bounded hot RAM/B-tree size.

### 4.2 Coordination with `CHK-003..006` Local Payload GC

1. Compaction of operational journals (`M5-05`) and tombstone epochs **MUST** remain strictly separate from canonical knowledge eviction (`CHK-003..006`).
2. Local canonical `ValidatedStore` / `BlobCatalog` eviction (`CHK-006`, default-off) **MUST NOT** delete any object or blob pinned by: (a) local user collections/bookmarks, (b) an unexpired `DHT-001` `ProviderLease` advertised by this node, (c) an active `AssemblyManifest` / `ReceptorResolutionEvent` frontier, or (d) an unfinalized `P4`/`P5` challenge window (`NEG-GC-001..003`).

---

## 5. Cross-Pillar Boundaries & Negative Assertions

- **`P8 ↔ P1 / P3 / P4 / P6`:** `STORE_CLASS_MISMATCH` fails closed before disk write; `LOCAL_ONLY` and `SomaticBiometricRaw` (`P9`) objects **MUST NOT** enter `ValidatedStore` or public archive exports.
- **`P8 ↔ P2`:** `0x000F` restore rotates reconciliation continuation salts so restored nodes never emit inconsistent OBP-RP journal offsets.
- **`P8 ↔ P5`:** `P5` `onebrain-obt` ledger files live in an isolated store directory (`obt-ledger-v1/`) whose corruption or fullness **MUST NOT** abort Phase R1–R4 knowledge-store startup (`FW-07`).
