# Pillar 2 Specification — Fast-Path Sketch Reconciliation (`RIBLT-1`), Frontier Diff, E2EE Mailbox, and Chunked Blob Transport Profile v1

> - **Profile ID:** `P2-OBP-EXTENSIONS-001`
> - **Pillar:** `P2` (OneBrain Protocol — OBP Network)
> - **Tasks covered:** `RIB-001`, `RIB-002`, plus P2 reconciliation frontier, mailbox, and blob-transport extensions
> - **Status:** Specified (additive optional vNext contract; runtime gate open)
> - **Reachability:** `spec-only (CI-checked)`
> - **Depends on:** [AUTHENTICATED_SESSION_PROFILE_V1](AUTHENTICATED_SESSION_PROFILE_V1.md), [HYBRID_INVENTORY_FOREST_PROFILE_V1](HYBRID_INVENTORY_FOREST_PROFILE_V1.md), [OBP_RECONCILIATION_PROTOCOL_PROFILE_V1](OBP_RECONCILIATION_PROTOCOL_PROFILE_V1.md), [DETERMINISTIC_RECONCILIATION_STATE_MACHINE_V1](DETERMINISTIC_RECONCILIATION_STATE_MACHINE_V1.md), [OUTBOUND_FIRST_REACHABILITY_PROFILE_V1](OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md), [BASE_V1_STORAGE_INTEGRITY_PROFILE](BASE_V1_STORAGE_INTEGRITY_PROFILE.md)
> - **ADRs addressed:** `ADR-OBP-056-02`, `ADR-OBP-056-03`, `ADR-OBP-056-04` (`NEG-NET-004`)

## 1. Purpose and Scope

Core P2 profiles freeze authenticated QUIC sessions (`NET-001`), 5-lane 256-bit sparse Merkle radix inventories (`OBP-002`), deterministic `obp/reconcile/1` state machines and Redb journals (`OBP-003..007`), and outbound-first relay reachability (`OBP-PROD-001..004`). Four specification seams remained open in P2:

1. **`RIB-001` & `RIB-002` Negotiated `RIBLT-1` Fast-Path Reconciliation:** Rateless IBLT sketch acceleration with hard adversarial decode ceilings and mandatory deterministic Merkle fallback (`ADR-OBP-056-04`).
2. **Feed-Prefix & Checkpoint-Frontier Resolution on `UnexplainedHybridRootDifference`:** Resolving divergent `HybridInventoryForest` roots when all 5 object/event radix lanes match (`OBP-004` §3).
3. **Outbound-First E2EE Intermittent Mailbox (`obp/mailbox/1`):** Bounded ciphertext store-and-forward for sleeping/intermittent nodes (`OfferKind::QueryMailbox`) without granting relay authority or delivery claims.
4. **Chunked `OwnedBlob` Transport (`obp/blob-chunks/1`):** Streaming verified 256 KiB chunks for blobs up to `100 MiB` (`BASE_V1_STORAGE_INTEGRITY_PROFILE` §2) while preserving `OBP-003`'s `1 MiB` frame cap.

---

## 2. Negotiated `RIBLT-1` Fast-Path Sketch Reconciliation (`RIB-001` / `RIB-002`)

### 2.1 Capability Negotiation & Eligibility Gate

1. `RIBLT-1` is negotiated exclusively as optional capability `obp/reconcile-riblt/1` inside the signed `Hello → Welcome → Finish` transcript (`NET-001`), gated behind default-off `FND-006` flag `reconcile_riblt_v1 = false` with an independent runtime kill switch.
2. Within an active `ReconciliationContext` (`OBP-003`), a sub-range is eligible for `SummaryMethod::Riblt1` **only** when all five conditions hold:
   - both peers negotiated `obp/reconcile-riblt/1` in the current session transcript;
   - `SelectorCid`, `NamespaceCommitment`, `DisclosureClass` (`Public` or `RouteMinimal` only), and key-range `[prefix_start, prefix_end)` match identically;
   - the peers' advertised `HybridInventoryForest` subtree roots for that range **differ**;
   - local item cardinality in that range satisfies `1 <= local_range_items <= 65_536` (`RANGE_ITEM_CAP`); and
   - neither peer has tripped its per-session RIBLT budget (`DECODER_MEMORY_CAP = 33_554_432` bytes = `32 MiB`).

### 2.2 Canonical Coded-Symbol Format & Adversarial Peeling Bounds (`RIB-001`)

1. Each inventory entry in the scoped range maps to a fixed 40-byte symbol key-sum unit:
   $$K_e = \text{RecordKind}(1\text{B}) \parallel \text{pad}_7(0) \parallel \text{CanonicalCID}(32\text{B}), \quad C_e = \text{BLAKE3\_64}(\text{"onebrain:vnext:riblt-check:1\0"} \parallel K_e)$$
2. Coded symbol index $m \in [0, M_{\max})$ deterministically selects included items via PRNG stream `BLAKE3_XOF("onebrain:vnext:riblt-map:1\0" || context_id || K_e)` using `ADR-OBP-056-04`'s Zipf-like rateless degree distribution, serializing `{ symbol_index: u32, count_delta: i64, key_xor: [u8; 40], checksum_xor: u64 }` in deterministic CBOR (`FND-003`).
3. **Hard Adversarial Resource Bounds:**
   - `MAX_CODED_SYMBOLS_PER_RANGE = 16_384` (at 64 B wire overhead $\le 1\text{ MiB}$);
   - `DECODER_MEMORY_CAP = 32 MiB` pre-allocated/bounded arena (zero unbounded `HashMap` growth from poisoned symbols);
   - `MAX_PEEL_OPERATIONS = 131_072` per range;
   - `STOP_AT_COST_RATIO = 50%`: if streamed symbol bytes reach $50\%$ of the deterministic `RadixForest256V1` subtree summary upper bound for that range without peeling to zero residual cells, symbol streaming **MUST** stop immediately.

### 2.3 Mandatory Root Verification & Seamless Radix Fallback (`RIB-002`, `NEG-NET-004`)

1. **Zero Probabilistic Completion (`NEG-NET-004`):** Peeling to an all-zero residual cell array (`count_delta == 0 && key_xor == 0 && checksum_xor == 0`) is a **candidate symmetric-difference hypothesis**, never proof of completion.
2. Before emitting `SelectorComplete` or advancing a checkpoint root, the receiver **MUST**:
   - validate every decoded remote-only `(RecordKind, CID)` against schema/selector bounds and fetch unknown payloads through normal `Manifest → Payload → ValidateThenAcceptSink` (`OBP-004`); and
   - recompute the simulated union `HybridInventoryForest` subtree root (or verify exact post-sync subtree roots) against the peer's signed/advertised target root.
3. **Deterministic Fallback Transition:** On any `STOP_AT_COST_RATIO` hit, `MAX_PEEL_OPERATIONS` exhaustion, checksum mismatch on a singleton cell (`count_delta == ±1`), or post-peel root mismatch, the state machine **MUST** record low-cardinality metric `RibltFallbackToRadix` (`M5-02`) and transition the exact sub-range to `SummaryMethod::RadixForest256V1` inside the **same** `ReconciliationContext` and `RedbReconciliationJournalBackend` without resetting the QUIC session or losing already-validated payloads.
4. **Shipping Gate (`RIB-001` V5):** Enabling `reconcile_riblt_v1` in any release profile requires benchmark receipts demonstrating strictly lower byte-and-CPU cost than `RadixForest256V1` across $65 \le \Delta \le 4096$ divergent items with zero false-completion acceptances under adversarial symbol injection.

---

## 3. Resolving `UnexplainedHybridRootDifference` via Feed-Prefix Frontier Exchange

In `OBP-004` §3, when all 5 object/event radix lanes report equal roots for a `SelectorCid` yet the composite `HybridInventoryForest` roots differ, the divergence lies in the **Feed-Prefix / Checkpoint Frontier lane** (`OBP-002` §3).

1. Peers negotiating `obp/reconcile-frontier/1` add bounded wire exchange `FrontierSummaryRequest (wire_id = 29)` / `FrontierSummaryPage (wire_id = 30)` carrying up to `MAX_FRONTIER_ENTRIES_PER_PAGE = 512` canonical entries `{ feed_id: FeedId, covered_sequence: u64, head_event_cid: EventCid, checkpoint_cid: Option<CheckpointCid>, equivocation_proof_cid: Option<ObjectCid> }` ordered lexicographically by `FeedId`, with opaque MAC-bound continuation (`OBP-005`).
2. When two peers observe the **same** `(feed_id, covered_sequence)` with **different** `head_event_cid`s, neither head overwrites the other (`NEG-NET-006`). Both heads are fetched via `Manifest → Payload` so `FeedStateReducer` (`FEED-001`) constructs a deterministic `FeedEquivocationProof` and quarantines unauthorized forks without halting non-equivocating feeds.
3. Missing `FeedCheckpoint` proofs referenced in `checkpoint_cid` are fetched and verified under `CHK-001/002` before any covered prefix sequence is treated as reconciled (`NEG-GC-001/002`).

---

## 4. Outbound-First E2EE Intermittent Mailbox (`obp/mailbox/1`)

To support mobile, laptop-sleep, and high-latency/DTN partitions (`OUTBOUND_FIRST_REACHABILITY_PROFILE_V1` §7, `DHT-001` `OfferKind::QueryMailbox`) without inbound ports:

1. **Zero-Plaintext Relay Staging:** A mailbox provider (`onebrain-relay` opted into `obp/mailbox/1`) stores opaque `MailboxEnvelopeV1` records addressed to an ephemeral rotating `MailboxSlotId = BLAKE3("onebrain:vnext:mailbox-slot:1\0" || recipient_mailbox_pk || epoch_salt)` (preventing stable `NodeId`/`ActorId` inbox correlation by the relay).
2. **Cryptographic Payload & Anti-Spam Permit:**
   - Inner payload is sealed with HPKE / XChaCha20-Poly1305 (`onebrain:vnext:mailbox-aead:1\0`) to `recipient_mailbox_pk` and padded to fixed bucket sizes (`1 KiB`, `4 KiB`, `16 KiB`, `64 KiB` max).
   - Deposit requires a recipient-issued `MailboxDepositToken` (exported inside a signed `ProviderLease` for `OfferKind::QueryMailbox` or bilateral `SEC-003` permit) bounding `max_envelopes`, `max_total_bytes` (`<= 4 MiB` per slot), and `expires_at_unix_s` (`<= 7 days`).
3. **Hard Availability-Only Boundary (`FW-05`):**
   - Mailbox deposit acknowledgement (`MailboxStoredReceipt`) means **staged at untrusted carrier only**. Senders **MUST** keep the outbound intent in `PendingMailboxPickup` (never `Acknowledged` peer delivery, `Use`, or `SelectorComplete`) until an end-to-end authenticated recipient receipt arrives.
   - Upon pulling envelopes, the recipient routes decrypted bytes through the **exact same** `Quarantine` / `ValidateThenAcceptSink` / `RoutePacketReplayGuard` pipelines as live QUIC traffic.

---

## 5. Chunked `OwnedBlob` Transport (`obp/blob-chunks/1`)

Because `OBP-003` caps control/object frames at `1 MiB` while `BASE_V1_STORAGE_INTEGRITY_PROFILE` §2 supports `OwnedBlob`s up to `100 MiB` (`BlobCid` 34B, `256 KiB` chunks):

1. A peer **MUST** first accept a validated canonical `KnowledgeObjectEnvelope` containing an `OwnedBlobReferenceV1` (`CanonicalBlobReferenceOracle` proof that the blob is referenced by an accepted object) before requesting its bytes over `obp/blob-chunks/1` — preventing unsolicited arbitrary-file relay/storage abuse.
2. Transfer operates on capability `obp/blob-chunks/1`:
   - `BlobManifestRequest { blob_cid: BlobCid }` → `BlobChunkManifest { blob_cid: BlobCid, total_bytes: u64, chunk_size: u32 (= 262_144), chunk_hashes: Vec<[u8; 32]> }` where `BLAKE3_ROOT(chunk_hashes)` binds deterministically to `blob_cid`.
   - `BlobChunkFetch { blob_cid: BlobCid, start_chunk: u32, count: u16 (<= 3) }` streams at most `3 × 256 KiB = 768 KiB` per frame (strictly within the `1 MiB` `CarrierLane::BulkPayload` admission cap of `M5-01`).
3. Each `256 KiB` chunk is verified against `chunk_hashes[i]` immediately before staging into `onebrain-blob-store` (`TX-BLOB-001`), supporting crash-safe bitmask resume across carrier failovers and commit-atomic promotion to `v2/<b0>/<b1>/<68-hex-ob-cid>` only after full `BlobCid` digest verification succeeds.

---

## 6. Cross-Pillar Invariants

- **`P2 ↔ P8`:** Neither `RIBLT-1`, `FrontierSummaryPage`, `obp/mailbox/1`, nor `obp/blob-chunks/1` may bypass `RuntimeAdmissionController` (`M5-01`) pre-allocation frame limits or `ValidateThenAcceptSink`.
- **`P2 ↔ P3 / P6`:** `obp/mailbox/1` carries `RouteNeedSketchV1`, `DisclosureCapsuleV1`, or `RUN-003` result notifications under fixed-bucket padding without exposing `NeedIR`, `ActorId`, or `StandingNeedId` to the mailbox host.
