# Workstream progress

> This is the authoritative execution ledger for this folder.
> Allowed states: `Planned`, `In progress`, `Review`, `Merged`, `Blocked`, `Deferred`.

## Current checkpoint

<!-- CURRENT_CHECKPOINT_START -->
**Cập nhật: 19/10/2026 11:05 (Asia/Saigon). ARCH-06 — một mô hình KU hướng người dùng & feature `legacy`: Done `2a38443`, committed/published theo D-050. Tiếp theo: ARCH-07 (D-051).**

- **Mục tiêu hiện tại:** ARCH-07 — hạ các bản nháp AI review đơn giản (concept–predicate–concept, quantity) xuống canonical draft thông qua `compiler.rs` có phân giải CCID (`semantic_selection_v2.rs`, `compiler.rs`); một ví dụ VI và một ví dụ EN lưu thành canonical KU sau khi review tường minh, phần chưa phân giải giữ trạng thái không thể lưu. MVP giữ 22 Merged / 2 Deferred.
- **Vừa xong (`2a38443`):** hoàn thành ARCH-06: tách `legacy-read` (cho KQL read-only đóng băng và `onebrain-mobile-core` không sửa đổi) và `legacy` (không nằm trong `default`) trên `ku-core`, `ku-encoder`, `ku-net`, `ku-mediator`, `onebrain-node`, `onebrain-api`, `onebrain-cli`, `ku-demo`; đưa `obt_*`, `encoding_reward`/`verifier`, `ku_tools`/`executor`/`system_prompt`, `obkg_*`, `graph_bio`/`decay`/`dream`/`embeddings`/`fedr`, `metabolism*`, `pomv_runtime`, `ku_lifecycle`, `eigentrust`, `immune`, `prediction`, `spread_analysis`, `synaptic`, `ecosystem`, `crdt`, `obs_cache`, `persistent_concept_dict`, `epistemic_engine`, `AiEncoder`/Core DNA v1–v2 pipeline, DHT/SWIM/gossip/OBT/query/sync/vacuum, `seed_client`, `verifier_service`, `anti_gaming_guard` ra sau `#[cfg(feature = "legacy")]`; `ku-mediator` trong bản build mặc định điều hướng `Intent::Encode` và proactive detection sang luồng vNext KU (`/ku`, `/api/vnext/ku`, `onebrain ku`) với `encode_suggestion`; Web `Encode.tsx`, `Explorer.tsx`, `Chat.tsx` gắn nhãn rõ bề mặt tương thích legacy Core DNA v7, điều hướng tạo KU sang `/ku` (`/api/vnext/ku`, loại bỏ gọi `api.encode` từ Chat) và nối `kqlResults` vào `displayKus` của `Explorer.tsx`.
- **Còn lại / blocker:** ARCH-07..14 theo MASTER_PLAN; ARCH-15 (Base operation store 4.096, chưa được owner xếp ưu tiên). Cột Reachability trong `docs/specs/vnext/README.md` là audit tĩnh, cần cập nhật khi consumer đổi (ARCH-08 đổi KQL-002, ARCH-11 đổi các dòng “not wired”). `cargo fmt -p onebrain-node --check` vẫn có lệch sẵn ở `examples/p5_multi_host_agent_v2.rs`. Registry package/public key vẫn do operator cung cấp.
- **Bước tiếp theo:** đọc dòng ARCH-07 trong MASTER_PLAN, [ARCHITECTURE_REVIEW](outputs/ARCHITECTURE_REVIEW_2026-10-18.md) §3.3 mục 3, `src/ku-encoder/src/extraction/semantic_selection_v2.rs`, `src/ku-encoder/src/extraction/compiler.rs` và `src/onebrain-node/src/ku_product.rs` để thiết kế đường hạ (lowering) từ `ReviewClaim` đơn giản (concept–predicate–concept và quantity đã đủ role/unit/CCID) sang canonical draft qua `compiler.rs` khi review hoàn tất, giữ nguyên yêu cầu không lưu nếu còn term chưa phân giải CCID/unit.
- **Đọc tiếp:** [MASTER_PLAN — ARCH-07](MASTER_PLAN.md#architecture-remediation-backlog-d-051), [ARCHITECTURE_REVIEW](outputs/ARCHITECTURE_REVIEW_2026-10-18.md) §3.3 mục 3, `src/ku-encoder/src/extraction/semantic_selection_v2.rs`, `src/ku-encoder/src/extraction/compiler.rs`, `src/onebrain-node/src/ku_product.rs`.
- **Workspace/Git:** root `C:/Users/shpy2/Documents/OneBrain`, main; base `03376a3` (`origin/main`); ARCH-06 `2a38443`; docs closure là HEAD cuối (`git log -1`), push và kiểm tra trực tiếp `git ls-remote` cùng lượt. PROGRESS exact 108 prep additions + ghi chú tra cứu SSH (local) + task23 dirty, 15 untracked giữ ngoài commit (stage bằng blob = working copy trừ đúng 2 khối local). Detached `798eabf` giữ nguyên.
- **Cần giữ:** 16 retained hashes và exact prep; backup tại `%LOCALAPPDATA%/Temp/onebrain_preserved_backup_fe220690/`. Qualification `4a8f29d` giữ 3 unmerged commits; Registry `6bc146c`, secrets `f3780c7`, source `4d01775`, MVP `0b050a4`, first-run `1c4c78a`, detached `798eabf` và mọi fixture/demo/backup cũ giữ nguyên.
- **Kiểm tra:** 19/10 11:04 (Windows, base `03376a3` + ARCH-06 `2a38443`): `cargo check` workspace (default + `--features legacy`) PASS; `cargo test -p ku-core --lib` 249/249 PASS; `cargo test -p ku-net --lib` 65/65 PASS; `cargo test -p ku-mediator --lib` 96/96 PASS (default + `--features legacy`); `cargo test -p ku-encoder --lib` 92/92 PASS (default) + 207/207 PASS (`--features legacy`); `cargo test -p onebrain-node --lib` 126/126 PASS; `npm run test:ku` 119/119 PASS; `npm run lint` 0 errors; `npm run build` PASS; `python scripts/ci/validate_vnext_contracts.py` PASS (530 local links); `python scripts/ci/validate_mobile_build_contracts.py` PASS; `git diff --check` PASS.
- **Quyết định/giới hạn:** D-051 mục 2 (một mô hình KU hướng người dùng, di sản sau feature `legacy`), D-050 standing merge/push, D-052, D-044/D-045 giữ nguyên. Không đụng `src/onebrain-mobile*`; giữ `legacy-read` tối thiểu cho KQL read-only đóng băng và mobile core.
<!-- CURRENT_CHECKPOINT_END -->

Quy trình cập nhật và prompt cố định: [NEXT_CONVERSATION](NEXT_CONVERSATION.md).
Agent mới dừng đọc phần trạng thái ở marker END; chỉ mở inventory/ledger/lịch sử
bên dưới khi cần đối chiếu. Khối trên được thay thế sau mỗi checkpoint có ý nghĩa.

## Git and retained-work inventory — audited 2026-09-28; scoped D-047 recheck 2026-10-03

Fresh `git fetch origin` and `git ls-remote --heads origin` completed. Compared
all 24 local branches and all 58 actual remote branch tips against origin/main,
including refs without a configured upstream. Both registered worktrees and
stash were inspected. This full-ref inventory describes the September audit. Scoped fresh fetch,
branch/worktree and dirty-state checks on 2026-10-02 followed the owner-approved
MVP merge `8d064c4` and verified it directly on origin/main. Qualification
`4a8f29d` and the retained worktree remain unchanged. The accompanying docs
closure commit updates these rows; no full remote-tip audit was repeated.

Scoped checks on 2026-10-03 fetched main at `49e529f`, directly checked main and
qualification tips, and preserved both worktrees and incoming dirty files.
D-047 merge `cbb7d58` was pushed and directly verified; its tree matches `1c4c78a`.
The following docs closure changes only coordination state and is pushed with main.

| Location | Actual state | How it affects the plan |
|---|---|---|
| `main`, `origin/main` | D-050 merge `2b14b23`, `e77202c`, `832425b`, normal-host share seam `2eb30d4`, registered editor share projection `bef0d28`, `/ku` Web share UI `25b89c5`, `ku_local_web` opt-in loopback network config `5dfc6bd`, `VNextProductRuntime` `CanonicalPublicReader` binding `fbcda30`, contributor entry refresh `ef9b459`, stale share preview feedback/cleanup `0f5d27d`, remaining D-045 share error feedback `1855e8f`, D-045 storage/outbox/input share error feedback `c31b3a2`, D-045 validation/system share error feedback & stale view cleanup `04f74cd`, saved-artifact inspection D-045 state transitions `a27ffee`, share input edit cleanup & `share_confirm` inspection prefill `2a4da76`, ARCH-01 relay `1d85082`, ARCH-02 legacy listener `6700281`, `share_prepare` cleanup & Base pending isolation `d1a762c`, ARCH-03 KU journal compaction `34c45f2`, ARCH-04 legacy KQL read-only freeze `06414e3`, ARCH-05 spec Reachability/doc refresh `e76576c`, ARCH-06 single user-facing KU model & `legacy` feature gate `2a38443` + docs closure at HEAD; closure pushed/verified in same run | Accepted MVP, first-run/manual/secrets/Registry diagnostics, normal-host share seam, registered editor share projection, `/ku` Web share UI, `ku_local_web` opt-in loopback network config, `VNextProductRuntime` `CanonicalPublicReader` binding, contributor entry refresh, stale share preview feedback/cleanup, remaining D-045 share error feedback, D-045 storage/outbox/input share error feedback, D-045 validation/system share error feedback & stale view cleanup, saved-artifact inspection D-045 state transitions share input edit cleanup & `share_confirm` inspection prefill, ARCH-01/02/03/04/05/06 and `share_prepare` cleanup & Base pending isolation Merged/published |
| `codex/ku-registry-public-key-diagnostics` | Retained at `6bc146c`; `645c6bc` + scoped docs integrated through `2b14b23` under D-050; root on main | No branch deletion; standing merge/push authority recorded; preparation outside commits |
| `codex/ku-new-dataset-secrets` | Retained at `f3780c7`; `b73e684` + reviewed docs integrated through `ea92e8a` under D-049; root now on main | No source branch deletion/publication; 17 incoming paths outside integration commits |
| `codex/ku-manual-provisioning-diagnostics` | Retained at `4d01775`; scoped commits merged through `403bbbb` under D-048; root now on main | No source branch deletion/publication; task23/untracked/108 preparation lines outside integration commits |
| `codex/ku-first-run-diagnostics` | Retained at `1c4c78a`, base `49e529f`; reviewed `f2c0baf` + ledger `1c4c78a` integrated through `cbb7d58` | D-047; no branch deletion, all incoming preparation remains outside integration commits |
| `codex/ku-enc-003-handoff` | Retained at `0b050a4`, now an ancestor of origin/main | Source branch kept; no remote branch was created/deleted |
| `codex/ku-enc-003-model-qualification` local and remote | `4a8f29d`, 3 commits not in main; local and remote tip unchanged | Preserve for later qualification; do not overwrite current docs or switch the dirty tree to it |
| Other branches / remote tips from the September audit | All audited tips already ancestors of main | Their old names do not represent missing implementation; do not redo them |
| MVP publication | `8de723d`, `0b050a4` and merge `8d064c4` reachable from origin/main; docs closure accompanies them | No unpublished MVP commits after closure push; dirty preparation is still local |
| Original working tree | Incoming handoff edits, six ENC-003 preparation reports, four `qualification_*` Python modules plus four test modules, and the overview report | Preserve; qualification preparation is useful later, not an MVP blocker or completed model evaluation |
| Second worktree `.codex/worktrees/3bbf/OneBrain` | Clean detached `798eabf`, already in main | No unique implementation found; untouched, with no cleanup/reuse claim |
| Stash | 0 entries | No stashed task found |

The unmerged qualification commits are `89c5f33` (preflight/independent evaluation
instructions), `6e4df3a` (published checkpoint) and `4a8f29d` (early MVP priority).
Its unique deliverables are `scripts/encoder/qualification_preflight.py`, its
unit tests and `KU_ENC_003_DATA_GUIDE.vi.md` / `KU_ENC_003_QUALIFICATION.md` in
outputs, plus older coordination edits. The preflight is an artifact inventory,
not an inference runner. Its MVP direction is already represented by D-021 and
D-044. No automatic full-branch merge is necessary to start the demo; recover
useful tooling selectively if a later task needs it.

Incoming local preparation modules are `qualification_package_audit`,
`qualification_source_export`, `qualification_attempt_ledger` and
`qualification_analysis`, each with a matching test module. They and the private
200-source export remain intact. Local-only reports
`outputs/KU_ENC_003_SOURCE_ACCOUNTING.md` and `outputs/KU_ENC_003_RESUME.md`
locate retained preparation; neither report is included in the MVP commit.

The prior `KU-SEM-001` ledger text saying its whole implementation was unmerged
was stale: `semantic_selection_v2.rs` entered through `437dba0`, included by main's
`3216f1d` baseline merge. Remaining fidelity and activation work is still deferred.

## Historical checkpoints — not current execution instructions

### 2026-10-19 — ARCH-06 single user-facing KU model and `legacy` feature gating, published

`2a38443`: `ku-core` splits `legacy-read` from `legacy` (`obt_*`, `encoding_reward`/
`verifier`, `ku_tools`/`executor`/`system_prompt`, `obkg_*`, `graph_bio`/`decay`/
`dream`/`embeddings`/`fedr`, `metabolism*`, `pomv_runtime`, `ku_lifecycle`,
`eigentrust`, `immune`, `prediction`, `spread_analysis`, `synaptic`, `ecosystem`,
`crdt`, `obs_cache`, `persistent_concept_dict`, `epistemic_engine`); `ku-encoder`
gates `AiEncoder`/Core DNA v1–v2 pipeline behind `legacy`; `ku-net` gates DHT/SWIM/
gossip/OBT/query/sync/vacuum behind `legacy`; `ku-mediator` routes chat encode to
`/ku` / `/api/vnext/ku` / `onebrain ku` in the default build; `onebrain-node`/API/
CLI/demo gate `seed_client`, `verifier_service`, `anti_gaming_guard` and legacy
`/api/encode` behind `legacy`; Web `Encode.tsx`, `Explorer.tsx`, `Chat.tsx` label
legacy surfaces, link/route to `/ku` and wire `kqlResults` into `Explorer.tsx`.
`ku-core` 249/249, `ku-net` 65/65, `ku-mediator` 96/96, `ku-encoder` 92/92 + 207/207,
`onebrain-node` 126/126, Web 119/119, validators PASS. ARCH-07 next.
[MASTER_PLAN row](MASTER_PLAN.md#architecture-remediation-backlog-d-051).

### 2026-10-19 — ARCH-05 spec Reachability column and drifted-doc refresh, published

`e76576c`: vNext contract index gains a `Reachability` column (default-build /
feature-gated / library-only / spec-only) for all 112 rows plus legend and method;
KQL_SPEC, `docs/references/index.html`, STUB_TRACKING, PILLAR_REVIEW, `Cargo.toml`
and `node.rs` session-state comments match current behavior. vNext/mobile validators
PASS; docs/comments only. ARCH-06 next. [MASTER_PLAN row](MASTER_PLAN.md#architecture-remediation-backlog-d-051).

### 2026-10-19 — ARCH-04 legacy textual KQL frozen as local read-only, published

`06414e3`: `ku_kql::read_only` gates `/api/kql` and CLI `kql`; write/standing forms
and clauses the executor ignored are refused with explanatory 400s. Parser gets
`NOT` > `AND` > `OR`, parentheses, whole-keyword matching and depth/term/path bounds;
EXPLAIN reports `local_scan`; CLI help and Web autocomplete use tested examples.
`ku-kql` 190/190, node 1/1, Web 118/118, validator PASS. ARCH-05 next.
[MASTER_PLAN row](MASTER_PLAN.md#architecture-remediation-backlog-d-051).

### 2026-10-19 — ARCH-03 KU journal compaction and non-retryable `storage_full`, published

`34c45f2`: committed/canceled bundles move atomically into bounded encrypted
`ku_private_terminal_v1`; the 1,024 / 64 MiB journal bound covers pending work only.
Canceled staging kept; committed drops only Vault-held source bytes; legacy journals
migrate on open; `ku_storage_full` → 507 `storage_full` (API/REST profile/Web).
Node lib 128/128, API `ku_api` 22/22, Web 115/115, build/lint/validator PASS. ARCH-15
(Base store 4,096 cap) recorded; ARCH-04 next. [MASTER_PLAN row](MASTER_PLAN.md#architecture-remediation-backlog-d-051).

### 2026-10-18 — D-052 records the mixed-version profile scope for ARCH-02

Owner delegated the decision; [D-052](DECISIONS.md#d-052--mixed-version-legacyvnext-concurrency-applies-to-legacy-compat-opt-in-nodes) scopes frozen
`MIXED_VERSION_RUNTIME_ROLLBACK_PROFILE_V1` §1 to legacy-compat opt-in nodes, keeps
concurrency/rollback/N-1 invariants for them and local KQL/offline access for all.
Docs only (DECISIONS, MASTER_PLAN ARCH-02 row, checkpoint); frozen file unchanged.
N-1 corpus test rerun; vNext validator and whitespace PASS.

### 2026-10-18 — D-045 `share_prepare` pre-call cleanup and Base `pending` isolation in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--d-045-shareprepare-pre-call-cleanup-and-base-pending-isolation-in-ku-web-2026-10-18):
`d1a762c` clears the previous share preview/confirmation before each re-prepare and
flags D-045 share errors (`isShareError`) so they never mark a pending Base draft
`uncertain` or show `Check recorded outcome`. `npm run test:ku` 114/114 (3 new
tests, shown failing on the old code), build, lint (0 errors), vNext validator and
whitespace PASS; retained work unchanged. ARCH-03 next.

### 2026-10-18 — ARCH-02 legacy TCP listener made opt-in and bounded, published

`6700281`: `NodeConfig::legacy_tcp_listener` (disabled by default, loopback or
all interfaces when opted in), `start_network()` binds legacy TCP only on opt-in,
CLI `--legacy-tcp-listener` with `--legacy-seed-compat`, Desktop auto-start only
when requested, 64-connection cap, 10 s first-message deadline, incremental receive
buffer, 2 concurrent remote verifications, labelled `/api/peers/connect`. Node
5/5 + 7/7 + 8/8, desktop 8/8 + 11/11, `ku_local_web` 7/7 + 8/8, OBP API 3/3,
CLI/API check and vNext validator PASS. Task 20 Web follow-up resumes next.

### 2026-10-18 — ARCH-01 relay outer-auth lifecycle fixed and published

`1d85082` bounds `OuterClientAuthenticator`: expiry pruning of pending challenges
and consumed nonces, single-use removal on every `authenticate` outcome,
connection-scoped `cancel_challenge` with a drop guard on the production
handshake paths, and expiring durable `ConsumedNonce` rows with periodic GC.
`cargo test -p onebrain-relay` 27/27 (5 new) + 8/8 + 11/11, clippy (no new
warnings), new-hunk rustfmt and whitespace PASS. No live relay run; ARCH-02 next.

### 2026-10-18 — Architecture review and D-051 remediation backlog published

Read-only review of KU/OBP/KQL/foundation in
[ARCHITECTURE_REVIEW_2026-10-18](outputs/ARCHITECTURE_REVIEW_2026-10-18.md):
four code-verified defects (relay auth lifecycle, unauthenticated legacy TCP on
`0.0.0.0`, KU journal 1,024-operation ceiling reported as retryable 429, legacy
KQL lost writes/precedence/EXPLAIN). Owner D-051: ARCH-01/02 first, mobile shares
the node core after ARCH-09, KQL direction chosen by agent (legacy text frozen and
corrected; `vnext_semantic_index` behind KU search; rest `experimental`). Backlog
ARCH-01..14 added to MASTER_PLAN; task 20 Web follow-up paused, not dropped. No
code change; vNext validator PASS (529 local links), relative links/anchors in the four changed handoff files and `git diff --check` PASS; retained work unchanged.

### 2026-10-18 — D-045 share input edit cleanup and `share_confirm` inspection prefill in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--d-045-share-input-edit-cleanup-and-share_confirm-inspection-prefill-in-ku-web-2026-10-18):
`2a4da76` clears stale `shareConfirmed` (`Confirmed public share intent`) in
`src/onebrain-web/src/pages/KuWorkflow.tsx` when editing `sharePeer`,
`shareAddress`, `shareSelector`, or `shareNamespace`, and when `shareConfirm`
succeeds, prefills `shareInspectCid` with `result.data.payload.public_object_cid`
and `shareInspectSelector` with `sharePrepared.selector` while clearing stale
`shareInspectView`. `npm run test:ku` 111/111 (`kuWorkflow.test.tsx` 36/36),
`npm run build`, `npm run lint`, vNext validator and whitespace PASS; retained
work unchanged.

### 2026-10-18 — Saved-artifact inspection D-045 state transitions and PUBLIC guidance/prefill in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--saved-artifact-inspection-d-045-state-transitions-and-public-guidanceprefill-in-ku-web-2026-10-18):
`a27ffee` clears stale `shareConfirmed` (`Confirmed public share intent`) in
`src/onebrain-web/src/pages/KuWorkflow.tsx` when inspecting any artifact in
`Saved local knowledge`, and when `inspected.disclosure_class === "PUBLIC"`,
surfaces explicit D-045 guidance that only `LOCAL_ONLY` artifacts can prepare a
new Public share preview while prefilling `shareInspectCid` with
`inspected.object_cid` and clearing stale `shareInspectView`. `npm run test:ku`
110/110 (`kuWorkflow.test.tsx` 35/35), `npm run build`, `npm run lint`, vNext
validator and whitespace PASS; retained work unchanged.

### 2026-10-18 — Remaining D-045 validation/system share error feedback and stale inspect view cleanup in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--remaining-d-045-validationsystem-share-error-feedback-and-stale-inspect-view-cleanup-in-ku-web-2026-10-18):
`04f74cd` defines `SHARE_ERROR_FEEDBACK` for all 25 D-045 share error codes in
`src/onebrain-web/src/pages/kuFeedback.ts` (adding the 12 validation/system
codes, prioritizing `shareFeedback?.recovery` ahead of
`failure?.reconcile_before_retry` for `consent_randomness_unavailable` and
`invalid_clock`, and keeping `isStaleSharePreparationError` false for
`explicit_public_confirmation_required` and `invalid_clock`), and clears stale
`shareIntentView` and `shareInspectView` cards in
`src/onebrain-web/src/pages/KuWorkflow.tsx` when lookup inputs change or a
subsequent lookup fails. `npm run test:ku` 109/109 (`kuWorkflow.test.tsx` 34/34),
`npm run build`, `npm run lint`, vNext validator and whitespace PASS; retained
work unchanged.

### 2026-10-18 — D-045 storage/outbox/input share error feedback and consumed preview cleanup in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--d-045-storageoutboxinput-share-error-feedback-and-consumed-preview-cleanup-in-ku-web-2026-10-18):
`c31b3a2` explains `saved_ku_not_accessible` (`404 NotFound`),
`outbox_unavailable_reprepare_same_intent` (`503 DependencyUnavailable`),
`outbox_unavailable` (`503 DependencyUnavailable`), `public_store_unavailable`
(`503 DependencyUnavailable`) and `provenance_unavailable` (`503 DependencyUnavailable`)
in `src/onebrain-web/src/pages/kuFeedback.ts` (prioritizing the four
`DependencyUnavailable` codes ahead of `failure?.reconcile_before_retry`), and
adds `outbox_unavailable_reprepare_same_intent` to `isStaleSharePreparationError`
so `KuWorkflow.tsx` clears the consumed `sharePrepared` preview card. `npm run test:ku`
99/99 (`kuWorkflow.test.tsx` 24/24), `npm run build`, `npm run lint`, vNext
validator and whitespace PASS; retained work unchanged.

### 2026-10-18 — Remaining D-045 share error feedback and invalidated preview cleanup in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--remaining-d-045-share-error-feedback-and-invalidated-preview-cleanup-in-ku-web-2026-10-18):
`1855e8f` explains `unsupported_public_ku` (`400 InvalidRequest`),
`prepared_limit` (`429 ResourceExhausted`, prioritized ahead of
`failure?.reconcile_before_retry`), `saved_access_changed` (`409 Conflict`),
`public_object_not_found` (`404 NotFound`) and `intent_not_found` (`404 NotFound`)
in `src/onebrain-web/src/pages/kuFeedback.ts`, and adds `saved_access_changed`
to `isStaleSharePreparationError` so `KuWorkflow.tsx` clears the consumed
`sharePrepared` preview card. `npm run test:ku` 97/97 (`kuWorkflow.test.tsx`
22/22), `npm run build`, `npm run lint`, vNext validator and whitespace PASS;
retained work unchanged.

### 2026-10-18 — Stale D-045 share preview feedback and cleanup in `/ku` Web published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--stale-d-045-share-preview-feedback-and-cleanup-in-ku-web-2026-10-18):
`0f5d27d` explains `prepared_share_missing` (`404 NotFound`) and
`public_ku_consent_expired` (`410 Expired`) on `share_confirm` in
`src/onebrain-web/src/pages/kuFeedback.ts` and clears the stale `sharePrepared`
preview card in `src/onebrain-web/src/pages/KuWorkflow.tsx` while preserving
`sharePrepared` on `503 capability_disabled` (`ku_public_share_network_unavailable`).
`npm run test:ku` 94/94 (`kuWorkflow.test.tsx` 19/19), `npm run build`,
`npm run lint`, vNext validator and whitespace PASS; retained work unchanged.

### 2026-10-18 — Contributor entry refresh for D-045 share paths published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--contributor-entry-refresh-for-d-045-share-paths-2026-10-18):
`ef9b459` updates `CONTRIBUTING.md` and `### Run the local-to-peer manual concept
(D-045)` in `tasks/20-INT-KU-OBP-001.md` so contributors see both the direct
`/ku` Web + `ku_local_web` (`--features vnext-network-runtime`, opt-in loopback
`"network"`) / `OneBrainNode` / `onebrain-desktop` (`VNextProductRuntime`) share
path and the isolated `ku_peer_demo` helper path. vNext validator, local links
and whitespace PASS; retained work unchanged.

### 2026-10-18 — `VNextProductRuntime` `CanonicalPublicReader` binding and network lane fence published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--vnextproductruntime-canonicalpublicreader-binding-and-network-lane-fence-2026-10-18):
`fbcda30` registers `Arc<VNextNetworkRuntime>` with `CanonicalPublicReader` for
both `paths.network.canonical` and `paths.operational` in
`VNextProductRuntime::start_with_paths`, checks `is_network_lane_active()`
(`VNextRuntimeLane::Network`) on active network lookup, and wakes the product
outbox on `Network` lane re-enable. `onebrain-desktop --test lifecycle` 8/8 +
9/9 `vnext-network-runtime` + 11/11 `vnext-outbound-first`, `ku_product` 19/19,
`ku_host::tests` 3/3, Base/vNext/rustfmt PASS; retained work unchanged.

### 2026-10-18 — Opt-in loopback network runtime config for `ku_local_web` published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--opt-in-loopback-network-runtime-config-for-ku_local_web-2026-10-18):
`5dfc6bd` adds `KuHostNetworkConfig` and `start_host_loopback_network_runtime` in
`ku_host.rs` and optional `network` config in `examples/ku_local_web.rs` (default
off; loopback + `network_opt_in: true` validated before Registry/dataset access).
`ku_local_web` 7/7 + 8/8 feature, `ku_host::tests` 3/3, KU/vNext/rustfmt PASS;
retained work unchanged.

### 2026-10-17 — `/ku` Web Public KU share and peer inspection UI published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--ku-web-public-ku-share-and-peer-inspection-ui-2026-10-17):
`25b89c5` wires `sharePrepare`, `shareConfirm`, `shareIntent` and `shareInspect`
into `src/onebrain-web/src/api/ku.ts` and `KuWorkflow.tsx` (`kuFeedback.ts`),
keeping preview → confirm separate (`confirm_public: true`), preserving `prepared_id`
on `503 capability_disabled`, hiding private CIDs/sources in share panels, and
distinguishing pending intent from delivery. `npm run test:ku` 92/92, `npm run build`,
`npm run lint`, KU/vNext PASS; retained work unchanged.

### 2026-10-17 — Registered editor Public KU share projection published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--registered-editor-public-ku-share-projection-2026-10-17):
`bef0d28` projects `share_prepare`, `share_confirm`, `share_intent` and
`share_inspect` onto `POST /api/vnext/ku/editor` (`KuServices::editor`), gating
network-dependent share actions on an active opt-in `VNextNetworkRuntime` with
`503 capability_disabled` (`ku_public_share_network_unavailable`). `onebrain-api`
39/39 + 42/42 feature (`ku_host::tests` 3/3), `ku_public_share` 4/4, `ku_product`
19/19, `ku_local_web` 6/6, `ku_peer_demo` check, KU/vNext/rustfmt PASS; retained
work unchanged.

### 2026-10-17 — Normal-host Public KU reader and D-045 share seam published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-follow-up--normal-host-public-reader-and-d-045-share-seam-2026-10-17):
`2eb30d4` wires `CanonicalPublicReader::for_data_dir` into `ku_host` (`get` +
`CanonicalPublicExchange` `export` with matching `semantic_content_cid`) and
shares `PublicKuShareCoordinator` + host prepare/confirm helpers across
`ku_public_share.rs`, `ku_host.rs` and `ku_peer_demo.rs`. `ku_public_share` 4/4,
`ku_product` 1/1, `ku_host` 1/1 + 2/2 feature, `ku_local_web` 6/6,
`ku_peer_demo` check, KU/vNext/rustfmt PASS; retained work unchanged.

### 2026-10-04 — PowerShell composed-host config published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-first-run-follow-up--composed-host-config-2026-10-04):
`832425b` accepts one UTF-8 BOM with strict schema/byte bounds; actual PS5.1 guide
composition and two isolated host starts PASS. Example 6/6, saved-read restart 1/1,
build/vNext/rustfmt PASS; retained work unchanged. Normal host sharing remains next.

### 2026-10-04 — Received signed-package setup guidance published

[Same parent result](tasks/20-INT-KU-OBP-001.md#latest-first-run-follow-up--received-registry-package-2026-10-04):
`e77202c` committed/pushed on main under D-050; read-only verify I/O/file-set guidance
and exact six-file acquisition layout. Rust 3/3 + signer/artifact 1/1, build/12 CLI
calls/vNext PASS; retained preparation excluded. Next: bounded host config composition.

### 2026-10-04 — Registry merged/published; standing merge/push permission

[Same parent closure](tasks/20-INT-KU-OBP-001.md#owner-directed-registry-integration--2026-10-04):
D-050 integrates `645c6bc` + `6bc146c` through `2b14b23`; push/direct remote verified.
Exact tree/parents, post-merge vNext/whitespace and retained-work PASS; prior tests reused.
Owner grants standing merge/push; first-run broader setup remains existing backlog.

### 2026-10-04 — Registry integration handoff recheck / awaiting owner scope

[Same parent review](tasks/20-INT-KU-OBP-001.md#latest-first-run-follow-up--registry-public-key-input-2026-10-04):
`645c6bc` + `6bc146c` ready, five-file scope/tested bytes/retained-work PASS.
Direct remote main `3de6a11`, qualification `4a8f29d`; Registry branch unpublished.
Owner merge/push scope requested under D-010; no implementation rerun or publication.

### 2026-10-04 — Registry public-key slice reviewed/committed locally

[Same parent review](tasks/20-INT-KU-OBP-001.md#latest-first-run-follow-up--registry-public-key-input-2026-10-04):
`645c6bc` + scoped docs review, no further code change; main `3de6a11` unchanged.
Scope/links/whitespace, 16 hashes and exact 108 prep lines PASS; reuse focused tests.
Ready for separate owner merge/push scope; broader Registry setup remains open.

### 2026-10-04 — Task 20 Registry public-key input / Review local

Reproduced the operator CLI's OS-only missing-key error; bounded diagnostics and
[existing guide](outputs/KU_WEB_001_IMPLEMENTATION.md#resolve-a-registry-startup-failure)
now distinguish independent trust, file-path/inline input and verify/status/activation.
Rust 2/2 + signer/artifact 1/1, build/vNext/format and 4 executable refusals PASS.
Custody retained; review/scoped local commit remains, no publication or host change.

### 2026-10-04 — Task 20 owner-directed new-dataset secrets merge (D-049)

[Same parent closure](tasks/20-INT-KU-OBP-001.md#owner-directed-new-dataset-secrets-merge--2026-10-04):
`ea92e8a` integrates `b73e684` + review `f3780c7`, exact reviewed tree; pushed/verified.
Post-merge vNext/whitespace and 17-file byte restore PASS; prior tests reused.
Registry/composition/share remain backlog; 108 prep additions stay outside commits.

### 2026-10-04 — Task 20 new-dataset secrets integration review / ready local

[Same parent review](tasks/20-INT-KU-OBP-001.md#new-dataset-secrets-integration-review--2026-10-04):
7-file `04906dd..b73e684` review found no scoped integration blocker; no code change.
Direct remote/ancestry/whitespace/docs links and retained hash/prep checks PASS;
prior helper/host tests reused. Merge/push awaits separate owner authorization.

### 2026-10-04 — Task 20 new-dataset secret helper / Review local

[Same parent slice](tasks/20-INT-KU-OBP-001.md#new-dataset-secret-provisioning--2026-10-04):
OS-random fresh key/token provisioning; no existing-custody overwrite or host action.
Python 4/4, 3 CLI + 1 host smoke, build/vNext/whitespace/links PASS; retained work intact.
Local scoped commit only; Registry/composition and owner integration remain open.


### 2026-10-04 — Task 20 owner-directed manual provisioning merge (D-048)

[Same parent closure](tasks/20-INT-KU-OBP-001.md#owner-directed-manual-provisioning-merge--2026-10-04):
merge `403bbbb` exactly matches `4d01775`; pushed and directly verified on main.
Post-merge vNext/whitespace and 17-file byte restoration PASS; prior tests reused.
Three diagnostics slices Merged; broader setup/product-share remains backlog.

### 2026-10-03 — Task 20 manual provisioning integration review / local

[Owner review scope](tasks/20-INT-KU-OBP-001.md#manual-provisioning-integration-review--2026-10-03):
cumulative six-path package + later-write delta reviewed; no code changes.
Retained 16 hashes/108 preparation lines, whitespace/ancestry PASS; prior tests reused.
Remote main remains `ac83f2a`; merge/push authorization and broader setup remain open.

### 2026-10-03 — Task 20 manual provisioning later writes / committed local

[Same parent result](tasks/20-INT-KU-OBP-001.md#manual-provisioning-later-write-diagnostics--2026-10-03):
`2c09839` adds bounded write-stage guidance + custody-preserving new-dir retry.
Rust 6/6, build, 3 CLI smoke calls, KU/vNext/format PASS; preparation retained.
No merge/push; writes remain non-transactional, broader setup remains backlog.

### 2026-10-03 — Task 20 manual provisioning combined review / committed local

[Cumulative result](tasks/20-INT-KU-OBP-001.md#cumulative-manual-provisioning-review--2026-10-03):
reviewed both follow-ups; corrected overview; scoped `2cd6ffe` + ledger, no push/merge.
Rust 5/5, KU/vNext/format/index links PASS; retained files/history SHA256 unchanged.
Later write errors and broader setup remain open; reproduce one synthetic write failure next.

### 2026-10-03 — Task 20 manual provisioning request / Review local

[Same parent result](tasks/20-INT-KU-OBP-001.md#manual-provisioning-request-diagnostics--2026-10-03):
fixed serde input-value disclosure with bounded request diagnostics; Rust 5/5,
build, 14 executable calls, KU/vNext/format/links PASS. Two manual follow-ups
uncommitted; incoming preparation retained. Next: review the combined local slice.

### 2026-10-03 — Task 20 manual provisioning source/output / Review local

[Same parent result](tasks/20-INT-KU-OBP-001.md#manual-provisioning-sourceoutput-diagnostics--2026-10-03):
source/output setup diagnostics and custody-preserving retry guide; Rust 3/3,
build, 9 executable calls, KU/vNext/format/whitespace PASS. Incoming preparation
retained. Uncommitted/unpublished on `ac83f2a`; request-loader diagnostics next.

### 2026-10-03 — Owner-approved first-run merge and publication

[D-047](DECISIONS.md#d-047--merge-and-publish-the-reviewed-first-run-fixes) integrates
`f2c0baf` + `1c4c78a` as `cbb7d58`, exact reviewed tree, verified on origin/main.
Post-merge vNext/whitespace PASS; 17 retained paths unchanged before closure.
Task/checklist/overview synchronized; broader provisioning/composition stays open.


### 2026-10-03 — Task 20 cumulative first-run review / local commit

[Review result](tasks/20-INT-KU-OBP-001.md#cumulative-first-run-review--2026-10-03):
`f2c0baf` isolates six paths, correcting Registry-root and optional-source guidance.
Example 5/5, Web 16/16, KU/vNext/format and index links PASS; preparation retained
outside commits. Ledger synchronized separately; publication/merge needs owner scope.

### 2026-10-03 — Task 20 empty-catalog guidance / Review local

[Same parent follow-up](tasks/20-INT-KU-OBP-001.md#contributor-follow-up--first-run-experience)
adds explicit operator provisioning guidance and a manual-only gate. Example 5/5,
Web component 16/16, Web/staging builds and KU/vNext checks PASS; omitted/empty
catalog restarts preserve exact private bytes/receipt. Preparation retained;
five slices uncommitted/unpublished, integration and broader provisioning remain open.

### 2026-10-03 — Task 20 source-input diagnostics / Review local

[Same parent follow-up](tasks/20-INT-KU-OBP-001.md#contributor-follow-up--first-run-experience)
adds source-field/format/retry guidance while preserving shared fallback. Example 5/5,
staging build and KU/vNext/format checks PASS; synthetic save + four fallback/restarts
retain exact private bytes/receipt, deny wrong-token reads and block partial editor admission.
Empty-catalog/provisioning remain open; retained preparation unchanged, no commit/push/merge.

### 2026-10-02 — Task 20 Vault-key diagnostics / Review local

[Same parent follow-up](tasks/20-INT-KU-OBP-001.md#contributor-follow-up--first-run-experience)
adds example-only file/length/recovery guidance; shared custody/Desktop behavior unchanged.
Example 4/4, staging build, 14 executable key cases and KU/vNext/format checks PASS;
saved/key/token files unchanged, exact binary acceptance retained. Source/provisioning
remain open; prior fixes and incoming preparation retained, no commit/push/merge.

### 2026-10-02 — Task 20 config/token diagnostics / Review local

[Same parent follow-up](tasks/20-INT-KU-OBP-001.md#contributor-follow-up--first-run-experience)
adds safe, actionable config/token errors before dataset initialization; operator guide,
checklist and overview synchronized. Example 3/3, staging build, 13 executable cases,
KU/vNext/format checks PASS. Registry fix and incoming preparation retained;
custody-input diagnostics/provisioning remain, with no commit/push/merge.

### 2026-10-02 — Task 20 first-run Registry diagnostics / Review local

[Same parent follow-up](tasks/20-INT-KU-OBP-001.md#contributor-follow-up--first-run-experience)
now reports unavailable Registry setup before creating a dataset, with trusted-release
retry guidance. Example regression, staging build, real missing/unsigned startup and
KU/vNext/format checks PASS. Change is local on `codex/ku-first-run-diagnostics`;
config/token onboarding and broader provisioning remain, with incoming preparation retained.

### 2026-10-02 — Owner acceptance, main merge and publication

- Owner accepted task 09/20 under [D-046](DECISIONS.md#d-046--accept-merge-and-publish-the-concept-mvp-tasks-0920); merged reviewed commits `8de723d` + `0b050a4` as `8d064c4` and verified the push directly on origin/main.
- Merge tree exactly matches the reviewed tip; post-merge vNext validator and whitespace PASS. Existing scoped happy-path/build/test evidence reused.
- All 17 incoming dirty/untracked hashes and retained branch tips unchanged through merge; preparation/history stays outside Git publication. Ledger/overview now 22 Merged / 2 Deferred.
- MVP is complete within its declared limits. Next backlog: [task 20 first-run experience](tasks/20-INT-KU-OBP-001.md#contributor-follow-up--first-run-experience), not started; model/NAT qualification remains false.

### 2026-10-02 — MVP diff review and local Git closure

- Reviewed and committed tasks 09/20 plus D-044/D-045 coordination as `8de723d`; code matches the previously tested slice. [Task 20 review and limits](tasks/20-INT-KU-OBP-001.md#git-review--2026-10-02).
- Removed the private saved CID from result docs; 265 index-only file links, KU/vNext validators, focused rustfmt and whitespace PASS. Existing happy-path/build/test evidence reused.
- ENC-003 preparation reports/scripts/tests/history remain outside the commit and unchanged; ledger/overview now distinguish local commits from main/publication.
- No push/merge or host action. Next: owner review of the committed MVP and accompanying ledger; integration requires owner direction.

### 2026-10-01 — INT-KU-OBP-001 local concept complete / Review

- D-045 owner-approved Public KU boundary implemented in [shared share service](../../../src/onebrain-node/src/ku_public_share.rs) and the opt-in peer helper; private save remains separate.
- Actual authorized API → preview/confirm → durable QUIC exchange → identical receiver CID/bytes/semantic identity and transport provenance → both-peer restart/retry PASS; private KU/source/config retained.
- Helper build, 3 disclosure regressions, 1 existing expected-peer regression, KU/vNext validators and focused formatting PASS. [Commands, topology, result and limits](tasks/20-INT-KU-OBP-001.md#actual-local-result--2026-10-01).
- Task 20 A–D/overview now Review local; no commit/push/merge. Contributor instructions/backlog updated; next is a bounded MVP diff/commit, retaining all qualification work.

### 2026-09-30 — KU-QA-001 local MVP complete / Review

- Real Web/API manual save/search/restart without AI preserved exact IDs/bytes/receipt; source provisioning fixed the empty catalog without changing keys/data.
- Five admitted-Qwen3 VI/EN drafts: 3 extracted, 2 needs-review, 47–83 s; reads/repeated start did not resample or save. [Results and commands](outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md#ku-qa-001-small-experimental-check--2026-09-30).
- Focused tests/build/contracts PASS; unchanged workspace formatting drift and historical native-host limits retained. [Local result](outputs/KU_WEB_001_IMPLEMENTATION.md#ku-qa-001-local-mvp-checkpoint--2026-09-30).
- All incoming/unmerged work retained; changes uncommitted on `689efd5`. Next: task 20.A, explicit two-node composition under D-044.

### 2026-09-30 — reusable handoff

Reused NEXT_CONVERSATION for one update template and stable prompt; PROGRESS now
has a bounded checkpoint payload. AGENTS requires updating it at meaningful task/
checkpoint boundaries. Task states and runtime are unchanged; no Git publication.

### 2026-09-28–30 — retained planning and overview observations

- 2026-09-30 overview-only refresh: updated the existing [task index](tasks/README.md)
  into a Vietnamese 24-task overview with status, MVP substeps and deferred lanes.
  Ledger states remain 20 Merged / 2 Planned / 2 Deferred. Fresh fetch and local
  inspection retain main `689efd5`, three qualification commits outside main,
  zero local-branch commits absent from origin refs, and the clean historical
  detached worktree. Dirty preparation/plan files remain local. No demo run,
  implementation acceptance, commit/push/merge or new parallel plan was created.

**2026-09-28 — D-044: runnable concept MVP, qualification deferred.**

- **Current task: `KU-QA-001`, Planned and ready to start under its revised MVP
  acceptance.** No new demo run or implementation is claimed by this planning update.
- Next action: task 09.A/B, use the existing local Web/shared node setup and find
  the first actual obstacle to preview → explicit save → search → restart/read.
  Then do the small experimental AI check and proceed to task 20's two-node demo
  and contributor entry. Do not wait for ENC-003 reviewers/holdout/run lock.
- `KU-ENC-003` is **Deferred**, retaining unfinished qualification inputs/harness
  and `model_qualified=false`. `KU-SEM-001` quality remains Deferred; its baseline
  source is already in main, although v2 activation/fidelity remain unaccepted.
- Use [MASTER_PLAN](MASTER_PLAN.md) for scope/backlog and the existing task 09/20
  files for substeps. New branch/attempt/conversation does not get a new plan.
- This update changes coordination/acceptance only. No source/runtime/mobile
  evidence change, model run, host action, commit, merge or push was performed.
- Documentation checks: aggregate vNext validator **PASS**; all **271 local file
  links across 21 changed/new Markdown files resolve**; `git diff --check` **PASS**.
  No new tests or runtime build were needed for this planning-only change.

### Earlier dated observations

Dated notes below retain their original observations, including old Blocked,
current-task and next-action wording. D-044, the Current checkpoint above and
Task ledger below govern current work. Do not resume an old blocker automatically.

The entries below retain their original observation dates. Older `Current task`,
branch and next-action statements are historical; use this checkpoint and the
task ledger for current state.

- 2026-09-24 D-043 KU-DESK-001 closure: the owner approved the exact review
  tip `bf3edb1` and explicitly directed merge and main publication under
  D-010, retaining the branch. Merge
  `6449446c3e538b8c97dbed2fdfb15c138e68a0c8` is verified on
  `origin/main`; the review branch remains at `bf3edb1` on origin. On the
  merged tree, Windows Desktop lifecycle tests pass 8/8 default and 10/10
  with `vnext-outbound-first`; the feature Desktop build and vNext contract
  validator pass. Task 08 is `Merged`. Live WebView, real OS sleep,
  macOS/Linux lifecycle and model quality remain unqualified; the separate
  Desktop lib test executable limitation remains documented in the
  [evidence](outputs/KU_DESK_001_IMPLEMENTATION.md). `KU-ENC-003` is
  `Blocked`; no KU-QA/INT acceptance, mobile work, OBP default change or
  remote host action occurred.

- 2026-09-24 KU-DESK-001 owner review correction — the owner accepted the
  review finding and directed implementation on the retained task branch.
  Shutdown errors now release the API listener, retain the node for retry,
  and publish a distinct `desktop-lifecycle` error even when KU was already
  degraded. Lifecycle status gives shutdown and fatal startup errors priority
  over a prior read-only KU issue. Default/feature Windows lifecycle suites
  pass 8/8 and 10/10; feature Desktop build passes. A separate Desktop lib
  test binary compiled but could not start on this host
  (`STATUS_ENTRYPOINT_NOT_FOUND`); the new status case passes in the lifecycle
  integration binary. [Updated evidence](outputs/KU_DESK_001_IMPLEMENTATION.md).
  Task remains `Review` pending D-010 Git closure; no KU-QA/INT acceptance,
  mobile work, OBP networking change or remote host action occurred.

- 2026-09-24 KU-DESK-001 — `Review` on
  `codex/ku-desk-001-workflow` from clean, synchronized main `073e141`.
  [Desktop integration evidence](outputs/KU_DESK_001_IMPLEMENTATION.md):
  packaged Web `/ku` uses the embedded shared node/API with trusted host KU
  inputs; node network shutdown retains its order and Base drains before the
  listener is released. Valid-key
  Registry/source failure retains read-only saved KU; missing Vault key is a
  visible typed dependency failure while legacy local reads remain available.
  Windows default/feature lifecycle suites pass 6/6 and 8/8, including actual
  prepared-work reconciliation, private save and read across restart.
  Desktop feature build/default check, Web build and 89 tests, API 37 tests
  (one real-model test ignored), shared Web-host example check, vNext contract
  validator and whitespace check pass. Native WebView, other OS lifecycle and
  model quality remain unqualified. No mobile or OBP network/default change,
  remote operation, merge or KU-QA/INT acceptance occurred. D-010 requires
  owner direction to merge; retain this branch for review.

- 2026-09-24 D-042 publication verified: merge `0604b55` and handoff
  closure `e7a3c5a` were pushed to `origin/main`. The retained migration
  branch remains at `c608069` on origin. OBP-MIG-001 is `Merged`.

- 2026-09-24 D-042 OBP-MIG-001 closure: owner accepted the reviewed
  implementation and its D-010 merge/publication step. Branch
  `codex/obp-mig-001-retire-legacy-seed` remains published at `c608069`;
  merge `0604b55aab6341a65eb1e782e85d8eb812351aa1` is on local main.
  Focused node runtime 8/8, CLI default-start process 1/1 and seed daemon
  compatibility gate 1/1 passed again before merge. Task 19 is `Merged`;
  no live host, old durable data, default networking or consumer-NAT claim
  changed. Main publication is recorded after push completes.

- 2026-09-24 OBP-MIG-001 review publication: validated implementation
  `1d649ce` was committed and published on
  `origin/codex/obp-mig-001-retire-legacy-seed`. The retained branch is
  `Review`; no merge, default rollout, live host action or legacy data
  removal occurred. D-010 keeps merge as a separate owner direction.

- 2026-09-24 OBP-MIG-001 — `Review`: final local verification passed after
  the [migration map](outputs/OBP_MIG_001_IMPLEMENTATION.md) was completed.
  CLI feature-on 52+2 and base-only 43+2, plus one real-process default-start
  test in each mode; seed integration 1, node runtime 8,
  node library 253 (two threads), Desktop lifecycle 6 and no-default vNext
  CLI check passed. The aggregate vNext validator, focused Rust format,
  whitespace and link checks passed. A first parallel node-library run had
  one unrelated PoMV timeout; focused and bounded two-thread reruns passed.
  No default OBP opt-in, live host or durable data was modified. All
  changes remain local and uncommitted on the declared migration branch;
  owner review and D-010 Git closure are separate steps.

- 2026-09-24 OBP-MIG-001 — `In progress` on
  `codex/obp-mig-001-retire-legacy-seed` from published main `940fa61`.
  [Migration inventory, rollback map and tests](outputs/OBP_MIG_001_IMPLEMENTATION.md):
  legacy seed/mDNS/UPnP startup and REPL connect now require explicit
  `--legacy-seed-compat`; vNext opt-in uses node-owned vNext-only startup;
  standalone `onebrain-seed` refuses to bind without the flag. Old config,
  peer memory, daemon package and legacy listener compatibility remain. CLI
  feature-on 52+2, base-only 43+2, seed integration and node runtime 8 pass.
  Node library passes 253 with two threads after one unrelated PoMV timeout
  in a parallel run. No live host, default OBP activation or remote state
  changed. Final contract/format/packaging checks remain before Review.

- 2026-09-24 main publication: D-041 QA merge `d695e4a` and handoff closure
  commit `940fa619925c6b1903525141d2244eb6d4e19009` were published to
  `origin/main`; the retained QA branch remains published at `fb5b71b`.
  Task 18 is `Merged`; its consumer-NAT gate was skipped and remains
  unqualified. Task 19 is dependency-ready.

- 2026-09-24 D-041 Git closure: validated QA tip `fb5b71b` was published on
  `origin/codex/obp-qa-001-nat-canary` and merged locally without conflicts as
  `d695e4ae976c53fceffdc103580c9462997a5389`. Task 18 is `Merged`
  under D-039/D-040 functional acceptance, with D-041's consumer-NAT gate
  explicitly skipped (`consumer_nat_qualified=false`). The separate Linux P5
  qualification on `c453f3e` is unchanged. Main publication is recorded after
  it completes; task 19 becomes dependency-ready but has not started at this
  checkpoint.

- 2026-09-24 D-041: owner directed task 18 closure and progression to task 19,
  skipping consumer-NAT qualification because the existing VPS cannot prove
  ordinary consumer NAT or run the supported product host. D-039/D-040
  functional review is accepted; `consumer_nat_qualified=false` and all ten
  independent-network product scenarios remain unrun. This is an explicit
  skipped gate, not qualification. QA branch publication and merge are
  directed under D-010 but have not yet been recorded at this checkpoint.
  Linux P5 qualification on `c453f3e` remains valid and separate.

- 2026-09-24 D-040: owner accepted the OBP-QA-001 functional review under
  D-039. Task 18 remains `Review` with the tested local scope and unclaimed
  consumer-NAT/native-platform qualification stated in
  [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md).
  The QA branch and its local changes remain intact; no commit, merge, push,
  remote operation, default networking change, tuning or task 19 start.

- 2026-09-24 continuation: re-ran the renamed bidirectional two-relay node
  test and aggregate vNext validator; both passed. Updated the acceptance-v1
  test pointer and clarified historical `Blocked` text in task 18. Functional
  Review under D-039 and its qualification limits are unchanged. All edits
  remain local on `codex/obp-qa-001-nat-canary`; no remote action, Git
  publication, default networking change, tuning or task 19 start.

- 2026-09-23 16:09 UTC D-039 [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md):
  owner accepts correct code and functional tests without strict external
  evidence as the OBP-QA-001 review criterion. Extended the live two-relay
  integration test to verify a separate B→A domain intent, authenticated
  relay path, durable receiver state and sender checkpoint after the existing
  A→B failover journey. Fresh runs passed: node 253, Desktop 6, API 44/one
  ignored, CLI 51 + 2, Web 89 and vNext contracts. Task 18 advances from
  `Blocked` to `Review` for functional QA. The consumer-network scenarios
  remain unrun and carry no NAT/platform qualification claim. The separate
  Linux P5 exact-candidate qualification remains valid. No remote state,
  networking default, tuning or task 19 changed; work remains local on the
  retained QA branch.

- 2026-09-23 15:44 UTC [two-VPS product admission](outputs/OBP_QA_001_VPS_PRODUCT_ADMISSION_20260923.md):
  owner selected two of the approved VPS as application-node roles and asserted
  their networks are independent. Read-only pinned-SSH checks on a/c and relay
  host b passed. The P5 bundle has no product host, and Linux custom Desktop
  network startup is deliberately fenced by the missing native lifecycle
  adapter. No OBP-QA-V1-01..10 product scenario was executed or relabeled from
  P5. VPS topology does not prove ordinary consumer NAT. Task 18 remains
  `Blocked`; Linux P5 qualification remains valid and task 19 unstarted.

- 2026-09-23 15:26 UTC [Linux P5 production-reference qualification](outputs/OBP_QA_001_P5_PRODUCTION_20260923.md):
  exact `c453f3e` bundle installed on a/b/c; fresh signed Base/Registry/P5 V2
  authority, sequence-7 relay history and four cross-host probes verified.
  Signed relay-only ring, selected-relay shutdown/alternate recovery and all
  13 faults with before/during/after markers completed. Independent verifier
  reports `multi_host_qualified=true` with 332 signed child receipts and 387
  raw objects; public aggregate privacy scan is clean. Signed cleanup and
  finalization passed, and direct three-host checks found no active P5 session
  or TCP 443 listener. Old runs, staging and archived fixtures/cursors remain.
  Task 18 stays `Blocked` on separate two-consumer-network/two-relay product
  scenarios and native surface/lifecycle evidence; task 19 is unstarted.

- 2026-09-23 14:34 UTC [signed V2 happy case completed](outputs/OBP_QA_001_SIGNED_HAPPY_SUCCESS_20260923.md):
  immutable `5b8dc3b` installed on a/b/c; fresh Base/Registry/P5 authority
  verified, sequence-4 history and four cross-host probes passed. Signed
  reservations, relay-only A→B→C→A ring and three marker exchanges completed,
  followed by cleanup/finalization. Direct a/b/c checks found no session or
  network state, active P5 agents/signers or TCP 443 listener; normal relays
  remain fenced. Earlier generations, failures and cursor archives remain.
  P5 13-fault/ten-oracle qualification and the two independent consumer-network
  product lane remain outstanding. Task 18 Blocked, task 19 unstarted.

- 2026-09-23 14:05 UTC [signed V2 happy-case attempt](outputs/OBP_QA_001_SIGNED_HAPPY_ATTEMPT_20260923.md):
  immutable `85adf65` installed on a/b/c, exact Base/Registry/bundle/P5
  authority verified, four sequence-3 cross-host probes passed. Signed
  bootstrap/prepare/reachability/matrix passed, but reservations failed closed
  at host-b. Signed cleanup and finalization succeeded on all three hosts;
  relay listeners are stopped and the prior cursor files preserved in typed
  maintenance archives. A durable relay-sequence and stable node-cursor
  correction is in progress locally; no full happy case, fault qualification,
  independent consumer-network evidence or OBP-MIG-001 start exists.

- 2026-09-23 13:11 UTC [installed candidate and successor blocker](outputs/OBP_QA_001_INSTALLED_SUCCESSOR_BLOCKER.md):
  all three immutable installations/public exports match `2dc5581`; identities
  preserved. D-036 placement wrappers retained with historical signature binding.
  Host-b's future timesyncd timestamp corrected; bounded skew now near zero, NTP
  still unsynchronized. Both relays durably renewed to sequence 2. Four actual
  restricted-SSH probes fail `SequenceRollback` because cold admission has no
  predecessor-history input. Relay-b/c and candidate-only listeners are stopped;
  new units/configs are safely fenced by `NotActivated`, state/backups retained.
  47 evidence file commitments and identity preservation verified. Versioned
  history-admission correction proposed for review; no new runtime edit/commit,
  complete V2 request, signed session/fault or qualification. Task 18 Blocked,
  task 19 unstarted. Earlier no-installation/clock-unmodified entries are historical.

- 2026-09-23 D-036: owner explicitly reaffirmed the already supplied/accepted
  placement of the three VPS on three separate physical machines. New placement
  collection is no longer a prerequisite; reuse retained evidence with original
  dates/provenance and provider-document-pending status. Earlier external topology
  blocker entries are superseded. Remaining work: clock maintenance, installed
  candidate/public exports, fresh relay descriptors/probes and complete V2
  admission/execution. Task 18 is still incomplete; task 19 remains unstarted.

- 2026-09-23 08:39 UTC [admission continuation](outputs/OBP_QA_001_ADMISSION_RECHECK.md):
  new read-only SSH collection confirms host-b remains about 25,388 seconds ahead,
  NTP off and relay running. Host-a search remains permission-limited; b/c still
  expose August placement receipts. All three collected file hashes verify.
  Added canonical V2 input completion order; current physical placement evidence
  remains an external dependency. No host mutation or signed session/fault.
  Task 18 remains Blocked; task 19 unstarted. Existing approvals remain accepted.

- 2026-09-23 08:35 UTC [retained authority recheck](outputs/OBP_QA_001_AUTHORITY_RECHECK.md):
  all five local checks pass for clean candidate `2dc5581`, signed Base request,
  rehashed Registry binding, canonical native bundle and approved policy identity/
  interval. Existing WSL dependency environment used after a retained system-Python
  import failure. No SSH, signing, host mutation or session/fault operation.
  Current topology, host-b clock and complete V2 admission remain unresolved;
  task 18 remains Blocked and task 19 unstarted. This is not qualification.

- 2026-09-23 08:30 UTC admission recheck: [fresh read-only SSH diagnostics](outputs/OBP_QA_001_ADMISSION_RECHECK.md)
  confirm host-b remains ~25,388 seconds ahead. Timesyncd is installed but
  disabled; VMware time sync is disabled. Relay remains running; P5 agent/signers
  are inactive. Host-a/c report NTP synchronized. Bounded host search found no
  new provider receipt; host-a search is partial/permission-limited. Initial SSH
  timeouts and successful bounded retries are retained separately. No host clock,
  service, runtime, signed session or fault mutation. Task 18 remains Blocked.

- 2026-09-23 D-035 execution: candidate `2dc5581e74387953b21051c930e52a8044ce0503`
  (tree `f08092d800615181da4ccc50db997606edb6f03b`) committed locally on the
  retained QA branch. [Rebuild/staging evidence](outputs/OBP_QA_001_CANDIDATE_2DC5581.md):
  fresh Base/Registry bindings verified; rebuilt bundle passed canonical checks
  and all three remote staging verifiers; six native loopback preflights passed.
  Fresh guest evidence does not prove physical independence. Found remote b/c
  placement receipts are still August 13 telephone attestations. New measured
  blocker: host-b clock ahead ~25,388 seconds, NTP off. No installed service,
  identity/state, clock, signed session or fault changed. Task 18 remains Blocked;
  next work is admissible topology/clock prerequisites and complete fresh V2
  admission. User requested autonomous investigation; no repeat approval request.

- 2026-09-23 D-035: owner approved the reviewed relay renewal correction and
  creation of a new local immutable QA candidate, followed by rebuild and fresh
  exact-candidate authority preparation. The reviewed source hashes still match
  both final verification reports. Keep the retained QA branch and all old runs;
  no push/merge or task 19. Candidate/build outcomes follow only after execution.

- 2026-09-23 QA continuation: [relay renewal correction](outputs/OBP_QA_001_RELAY_RENEWAL_FIX.md)
  is locally implemented for review on the retained QA branch. Canonical same-key
  successor, atomic candidate/activation fencing, exact-byte publication recovery
  and persisted-descriptor startup are covered. Windows and local Linux: 40 relay
  tests each; node integration/matrix 32; Python 86 passed/one skipped; vNext,
  format and whitespace pass. All source/log commitments verify. No commit,
  push, merge, remote replacement or signed P5 session. Next: review/new immutable
  candidate, then fresh exact-candidate V2 authority and independent-network
  evidence. OBP-QA-001 remains `Blocked`; OBP-MIG-001 is unstarted.

- 2026-09-23 approved P5 follow-up: owner accepted all proposals; same-key policy
  renewal applied. Fresh `fc65f08` Base request and remeasured Registry binding
  signed/verified; operational bundle 02 verified on all three hosts. The current
  blocker is [relay descriptor renewal](outputs/OBP_QA_001_P5_RENEWAL_BLOCKER.md):
  existing descriptors expired, while exact-candidate CLI only supports first
  export and cannot preserve the descriptor chain on renewal. Binary repro fails
  closed; no state reset/service replacement or production fault performed.
  OBP-QA-001 remains `Blocked`. Earlier approval-pending entries are historical.

- 2026-09-23 P5 follow-up: owner authorized reuse of the existing three hosts,
  SSH upload and remote execution. [Upload/execution evidence](outputs/OBP_QA_001_P5_UPLOAD.md):
  exact `fc65f08` source-free bundle built and canonical manifest verified;
  pinned SSH, upload and artifact checks passed on all three hosts. Both native
  single-host preflights passed on each host (six runs), qualification false.
  Installed candidates differ; no service/generation/identity was replaced.
  [V2 authority recovered](outputs/OBP_QA_001_RECOVERED_P5_AUTHORITY.md) from the
  sibling release archive; old signatures/keys verify. The policy expired on
  September 17; a same-key September 23–30 renewal proposal awaits approval.
  OBP-QA-001 remains `Blocked`; no production fault or Git publication occurred.

- 2026-09-23 OBP-QA-001 — `Blocked` on
  `codex/obp-qa-001-nat-canary`, original clean main `fc65f08` verified equal
  to remote main. All retained branches preserved. Versioned
  [acceptance scenarios](outputs/OBP_QA_001_ACCEPTANCE_V1.md) and fixed local
  preflight collector added. [Local evidence](outputs/OBP_QA_001_PREFLIGHT.md):
  all 13 groups pass, with one Linux-only Python skip and one ignored model test;
  22 supplemental P5 tests and report integrity verification pass. Independent
  consumer networks, relay-host faults and exact-candidate P5 evidence are not
  supplied or executed. No qualification, live-host activation or Git publication.
  The D-034 Planned pointer below is historical.

- D-034 closure: accepted Desktop implementation `410a6a0` merged without
  conflicts as `7e4fc14e9ba4082e425d474ef9f7da26129106b0`. Fresh checks passed:
  six Desktop integration tests, 89 Web tests, 35 Python tests and vNext validator.
  Main and the retained Desktop branch are published in this closure.
  Current task: OBP-QA-001, Planned. No QA execution or live-host activation.
  Earlier local/uncommitted Desktop Review entries below are historical.

- D-033: owner accepted OBP-DESK-001 review (“tôi đồng ý duyệt review”).
  Implementation and evidence are unchanged. State remains Review with owner
  acceptance recorded, pending explicit Git closure direction. Original working
  tree and all local changes retained; no commit/push/merge or next task started.

- 2026-09-22 OBP-DESK-001 — `Review`, local implementation on
  `codex/obp-desk-001-networking` from clean main `636c113` in the original tree.
  One shared node/API supervisor, local credential handoff, Windows lifecycle
  fencing, scoped tray status and durable client recovery are implemented.
  [Evidence and limits](outputs/OBP_DESK_001_IMPLEMENTATION.md): six feature-enabled
  and four feature-off integration tests, 89 Web tests, two receipt tests,
  Web build/lint, 35 Python tests and vNext validator pass. Windows Tauri debug
  asset build passes. API/node/CLI and live host remain unchanged. Work remains
  uncommitted; no push/merge, networking activation or platform qualification.
  Earlier Desktop Planned/main pointers below are historical.

- D-032 closure: owner accepted Web review. Implementation `cade635` merged
  without conflicts as `7f49eeb`. Fresh checks passed: 82 Web tests, production
  build, 25 Python tests and aggregate vNext validator. API/node/CLI unchanged.
  Main and retained Web branch are published in this closure. Current task:
  OBP-DESK-001, Planned. No Desktop implementation or live-host activation.
  Earlier local/uncommitted Web entries below are historical.

- 2026-09-22 OBP-WEB-001 — `Review`, local implementation on
  `codex/obp-web-001-networking` from clean main `2d4d5f4` in the original tree.
  All 13 accepted operations plus metadata reconcile use the shared local API;
  private aggregate WS hints only trigger reads. Exact interactions are in the
  [Web projection](../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md).
  [Evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md): 82 Web tests, two receipt
  tests, production build, lint (eight existing unrelated warnings), 25 Python
  tests and vNext validator pass. Isolated Chromium at 1280/390px passes layout
  and axe including contrast. API/node/CLI and the live host remain unchanged.
  Work is local/uncommitted; no push, merge, networking activation or model work.
  The earlier Web Planned pointers are historical; next action is Web review.

- 2026-09-22 D-031: owner approved the complete CLI implementation and requested
  continuation. Implementation `e0dcc7d` merged without conflicts as
  `04bcb30e42fa2594d0bf11e146fa0e44df0dcf82`. Fresh checks passed: CLI 51 unit +
  2 integration, Python 25 and aggregate vNext validation. API/node content
  remains unchanged. Main and the retained CLI branch are published in this
  closure. Current pointer: OBP-WEB-001, Planned; no Web implementation started.
  Earlier local/uncommitted CLI Review entries below are historical.

- 2026-09-22 OBP-CLI-001 — `Review`, local implementation on
  `codex/obp-cli-001-networking` from clean `b66780f` in the original workspace.
  All 13 accepted operations and metadata reconcile use authenticated local REST;
  exact syntax, confirmations and host-input limits are registered in the
  [CLI projection](../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md).
  [Implementation evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md): feature CLI
  51 unit + 2 integration, default CLI 42 + 2, no-default check, 25 Python contract
  tests, vNext validation and format/whitespace checks pass. API/node code remains
  unchanged. All work is retained locally, uncommitted; no push/merge, live-host,
  networking activation, model, mobile or rollout changes.

- 2026-09-22 D-030: owner instructed "merge và push tất cả". Reviewed API tip
  `c4aafb9` (implementation `de04a57`) merged without conflicts as
  `7d37a3066a561a38867352b3fab2e08387940ba6` and published on `origin/main`.
  All retained baseline work remains in ancestry. Verification immediately before
  merge passed: node 252 + 22, API 44 + 8 (one opt-in ignored), feature-off 16,
  ku-net 315, Python 19 and vNext validator. Implementation was unchanged.
  Current pointer advances to OBP-CLI-001, Planned; OBP-WEB-001 is also dependency-ready.
  No next-task implementation, host activation, model tuning or rollout occurred.
  Earlier Review entries below are historical checkpoints superseded by D-030.

- 2026-09-22 OBP-API-001 continuation verification at `bcca300`: original tree
  initially clean; remote API/main tips verified directly. Fresh offline/locked
  reruns passed: node 252 unit + 22 integration; API 44 unit/one ignored + eight
  Base integration; feature-off 16; ku-net 315; Python 19 and vNext validator.
  [Updated evidence](outputs/OBP_API_001_IMPLEMENTATION.md#continuation-verification--2026-09-22).
  Corrected the stale next-action pointer. Implementation remains unchanged in
  Review; API merge requires owner direction, CLI/Web remain Planned.

- 2026-09-22 OBP-API-001 — `Review`, implementation under accepted D-029 transport.
  Shared node façade, durable command/reconciliation journal, host grants/input
  references and six REST/WS routes are implemented in the original workspace.
  [Implementation evidence and limits](outputs/OBP_API_001_IMPLEMENTATION.md).
  Verification passed: node 252 unit + 22 integration; API 44 unit/one ignored
  + eight Base integration; API feature-off 16; ku-net 315; Python 19 and vNext
  validator. Implementation `de04a57` is published on
  `origin/codex/obp-api-001-network-api` under D-010. No merge is authorized by
  D-029; dependent tasks remain Planned.
  The transport-review entry below is historical; approval is already satisfied.

- 2026-09-21 OBP-API-001 — `Review` of the transport prerequisite only.
  [Concrete proposal and evidence](outputs/OBP_API_001_TRANSPORT_REVIEW.md):
  six routes, 13 operations, management/input/reconciliation boundaries and a
  separately scoped private WS projection. 19 contract tests and aggregate
  vNext validation passed; 35 transport fixtures. No handlers implemented.
  Original tree retained on `codex/obp-api-001-network-api`, baseline `836af91`;
  these changes are local and uncommitted. Notification transport review remains
  required by the accepted OBP profile before this complete extension is implemented.

- 2026-09-21 D-028 — `OBP-PROD-004` Merged and pushed to `origin/main`.
  Implementation `8f7d327`; merge `2c39117183f167ef4d333757b050d6306043e068`.
  Atomic push published main and `codex/obp-prod-004-routing`, including the
  complete retained D-027 baseline at `3216f1d`. Original workspace preserved.
  Fresh verification passed: 242 node unit + 22 integration, 315 ku-net,
  six OBP contract tests, aggregate vNext validator and whitespace checks.
  [Evidence and limitations](outputs/OBP_PROD_004_IMPLEMENTATION.md).
  OBP-API-001 is now dependency-ready and remains Planned. No model tuning,
  live-host changes, networking activation or default rollout occurred.

- 2026-09-21 D-027: owner authorized merge and continuation. Complete retained
  baseline committed as `437dba0`, merged into local main as
  `3216f1ddf0cd72098924ccdf9fb0ef6b30186b78`. No remote push. Original workspace
  retained; task 004 now In progress on `codex/obp-prod-004-routing`.
  Fresh verification: node 234 unit + 22 integration; CLI 42 unit + 2 integration;
  encoder extraction 70 passed/1 opt-in ignored; KU API 21 passed/1 opt-in ignored;
  Web 22 tests and build; six OBP contract tests, aggregate validator and diff check.
  Task 003 merge prerequisite is satisfied. Historical unmerged checkpoints below
  describe their original observation dates, not the current Git state.

- 2026-09-21 OBP-PROD-003 â€” `Review`: discovery, signed cache recovery,
  standing reservations/renewal and separately opted-in advertisements are
  implemented locally under D-026. 234 node unit + 22 integration tests pass,
  including real loopback TLS renewal/keepalive/closure. Core tests, feature
  checks and validators pass. [Evidence and limits](outputs/OBP_PROD_003_IMPLEMENTATION.md).
  Original dirty tree retained on `codex/obp-prod-003-discovery`; no merge,
  publication, model tuning or live networking activation. Task 004's merge
  prerequisite is not waived by D-026.

- 2026-09-20 D-026: owner explicitly authorized OBP-PROD-003 local work before
  task 002 merge. Implementation is in progress on `codex/obp-prod-003-discovery`
  in the original dirty tree. Fresh task-002 verification: 219 unit + 20
  integration tests, six contract tests and aggregate vNext validation pass.
  No Git publication, model tuning or live network activation is authorized.

- 2026-09-20 D-025: owner accepted OBP-PROD-001 and explicitly authorized
  OBP-PROD-002 local implementation before merge. Work remains on the original
  dirty tree under `codex/obp-prod-002-node-lifecycle`; no Git publication or
  live-host activation. Task 002 is locally implemented/tested: 219 unit and
  20 integration tests passed, feature checks and contract validators passed.
  [Lifecycle evidence](outputs/OBP_PROD_002_IMPLEMENTATION.md).

- 2026-09-20 OBP-PROD-001 â€” `Review`: [orchestration proposal](outputs/OBP_PROD_001_CONTRACT.md)
  defines 13 logical operations, 18 DTOs and 26 fixtures. Six mutation tests and
  aggregate vNext validation pass. Original dirty tree, including completed CLI,
  retained on `codex/obp-prod-001-product-contract`; no commits/push/merge.
  Historical review gate superseded by D-025: contract accepted, local task 002 authorized before merge.

- 2026-09-20 KU-CLI-001 â€” `Review`: all eleven shared KU operations plus
  reservation implemented in `onebrain ku`. 42 CLI unit and 2 integration tests
  pass, including the isolated real API/node workflow; default/no-default checks,
  format and vNext contracts pass. [Evidence](outputs/KU_CLI_001_IMPLEMENTATION.md).
  Original dirty baseline retained on `codex/ku-cli-001-workflow`; uncommitted,
  no model tuning, live-host change, push or merge. Next lane: OBP-PROD-001 contract.

- 2026-09-20 D-024: owner accepts the current semantic foundation for this stage.
  Further fidelity work is Deferred to the [contributor backlog](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md).
  Next: KU-CLI-001, then the dependency-ready OBP framework lane. No merge or v2
  host activation is claimed. [New-conversation handoff](NEXT_CONVERSATION.md).

- 2026-09-20 shared semantic selection continuation â€” `Review`: common Rust host assembly, sparse LLM choices and mechanically constrained repair are implemented and activated locally. Qwen3 8b passed 5/5 through the actual Web API (52.32 s mean; admission <0.1 s); Gemma E4B passed 4/5 (43.19 s), 12B passed 5/5 (78.58 s) via the development adapter. Fifteen selection tests, five API tests, twenty Web tests, build and contract validation passed. Three legacy jobs remain readable; new drafts are explicitly unassessed and no KU save/share occurred. Ollama 0.34.2; earlier baseline 0.33.3. See [selection implementation and limits](outputs/KU_SEMANTIC_SELECTION_IMPLEMENTATION.md).

- 2026-09-09 shared semantic selection â€” historical checkpoint (superseded by September 20 `Review`): owner approved sparse LLM choices and host mechanics consistently across platforms. Common architecture, Rust assembler/targeted repair and unassessed draft state are implemented; 45 encoder, five API and 20 Web tests passed. Real-model comparisons and local host activation are pending. See [selection implementation](outputs/KU_SEMANTIC_SELECTION_IMPLEMENTATION.md).
- 2026-09-09 owner-requested Gemma 4 comparison â€” `Review`: development-only chat transport ran the same native DraftJob/prompt/schema flow on five public cases per model. E4B passed 3/5 meanings (66.57 s mean); 12B passed 5/5 (179.78 s mean). E4B missed a preposition and linked the wrong contrast endpoints despite mechanical readiness. All 20 calls had thinking disabled. No Web provider admission or canonical KU behavior changed; read-only live-host check passed. See [Gemma evidence and limits](outputs/KU_GEMMA4_DEVELOPMENT_PROBE.md).
- 2026-09-08 staged-draft follow-up â€” `Review`: shared draft workflow, encrypted background jobs and Web interface are implemented and activated on the local owner host. Qwen3 8b preserved all five inspected development meanings; 1.7b retained review issues on all five. Actual Web API originals passed (water 170.95 s, car 126.06 s; start replies 0.085/0.071 s), without resampling on reads/repeated start or any KU save/share. See [implementation and limitations](outputs/KU_REVIEW_DRAFT_IMPLEMENTATION.md). Complete canonical lowering and broad model portability remain unfinished; draft readiness does not claim those outcomes.
- 2026-09-08 owner follow-up: [headless tests and portable proposal experiments](outputs/KU_ENCODER_MODEL_PORTABILITY_EXPERIMENT.md); two requested sources tested locally with Qwen3 8b/1.7b and a separate 3b reviewer. Semantic defects remain explicitly reported; prototype proposals are not canonical KU or integrated Web behavior. Shared preflight now brings span/number/structural compilation errors into the bounded repair allowance.
- Local follow-up: [schema diagnostics and repair feedback](outputs/KU_WEB_001_SCHEMA_DIAGNOSTICS.md) implemented and tested after owner authorization; these follow-up edits remain uncommitted.
- Real Vietnamese follow-up remains unsuccessful on the current Web/host Candidate path: the two exact owner sources failed `duplicate_id` (379.26s, water) and `oneof` (451.29s, car) in the 2026-09-08 rebuilt-host API tests. Both test reservations were canceled; neither yielded a validated preview or saved/shared KU. Number/span preflight now reaches the existing repair allowance; earlier `unsupported_number` evidence remains historical. See the [model-portability report](outputs/KU_ENCODER_MODEL_PORTABILITY_EXPERIMENT.md) for separate proposal results and remaining semantic representation gaps.
- Current task: `OBP-QA-001` — `Blocked` (local preflight passed; independent-network evidence missing).
- Current branch: `codex/obp-qa-001-nat-canary` in the original workspace; prior branches retained.
- Completed review branch: `codex/obp-prod-004-routing`, implementation `8f7d327`.
- Retained baseline: `437dba0`, merged at `3216f1d` under D-027; now published.
- Last merge: `7e4fc14` (OBP-DESK-001, D-034).
- Next action: resolve current topology/clock admission prerequisites, then complete
  candidate-bound host/relay evidence and V2 authority for `2dc5581`; collect the
  separate independent-consumer-network evidence. Review/commit/rebuild is complete.
  Keep task 18 current, networking default-off and task 19 unstarted.
- Encoder framework direction and sequence: owner accepted at `e513552` under D-018; no further direction approval needed
- Default rollout change authorized: **no**
- Mobile work authorized by this package: **no**

## Task ledger

| Order | Task | State | Branch | Dependency | Merge commit/evidence |
|---:|---|---|---|---|---|
| 1 | `KU-REV-001` | Merged | `codex/ku-rev-001-canonical-audit` | â€” | `25d008d211f450d15ba1a63cacc0368298ed3e7a` on `origin/main`; [authority audit](outputs/KU_AUTHORITY_AUDIT.md), D-011â€“D-014. |
| 2 | `KU-REV-002` | Merged | `codex/ku-rev-002-runtime-map` | `KU-REV-001` | [Runtime gap map](outputs/KU_RUNTIME_GAP_MAP.md), including owner D-011â€“D-014; merge `b872263` on `origin/main`. |
| 3 | `KU-CON-001` | Merged | `codex/ku-con-001-product-contract` | `KU-REV-002` | [Approved contract](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md); KU-PC-A/B/C accepted under D-015; merge `2cbc8f2` on `origin/main`. |
| 4 | `KU-RUN-001` | Merged | `codex/ku-run-001-shared-service` | `KU-CON-001` | Owner-authorized merge `d141701` on `origin/main`; [implementation evidence](outputs/KU_RUN_001_IMPLEMENTATION.md). |
| 5 | `KU-API-001` | Merged | `codex/ku-api-001-local-api` | `KU-RUN-001`, `KU-ENC-002` | D-022; merge `3eba370` on `origin/main`; [API implementation and verification](outputs/KU_API_001_IMPLEMENTATION.md). |
| 6 | `KU-CLI-001` | Merged | `codex/ku-cli-001-workflow` | `KU-API-001` | [Local implementation and tests](outputs/KU_CLI_001_IMPLEMENTATION.md); local merge `3216f1d`, D-027. |
| 7 | `KU-WEB-001` | Merged | `codex/ku-web-001-workflow` | `KU-API-001` | [Manual implementation](outputs/KU_WEB_001_IMPLEMENTATION.md), [Ollama integration and run instructions](outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md); D-023; local merge `3216f1d`, D-027. |
| 8 | `KU-DESK-001` | Merged | `codex/ku-desk-001-workflow` | `KU-WEB-001` | D-043 merge `6449446` on `origin/main`; review branch retained at `bf3edb1`; [Desktop integration evidence](outputs/KU_DESK_001_IMPLEMENTATION.md); Windows default/feature lifecycle 8/10. |
| 9 | `KU-QA-001` | Merged | Retained `codex/ku-enc-003-handoff` at `0b050a4`; integrated in main | Available accepted KU CLI/Web/Desktop and shared service; no ENC-003 gate (D-044) | D-046 merge `8d064c4` on origin/main; A–D accepted: real private save/search/restart and five bounded AI drafts. Focused results reused; broader qualification stays deferred. |
| 10 | `OBP-PROD-001` | Merged | `codex/obp-prod-001-product-contract` | `KU-CON-001` | [Contract proposal and tests](outputs/OBP_PROD_001_CONTRACT.md); accepted under D-025; local merge `3216f1d`, D-027. |
| 11 | `OBP-PROD-002` | Merged | `codex/obp-prod-002-node-lifecycle` | `OBP-PROD-001` accepted, D-025 local waiver | [Lifecycle evidence](outputs/OBP_PROD_002_IMPLEMENTATION.md); 219 unit + 20 integration tests; no activation; local merge `3216f1d`, D-027. |
| 12 | `OBP-PROD-003` | Merged | `codex/obp-prod-003-discovery` | `OBP-PROD-002` local evidence, D-026 | [Discovery evidence](outputs/OBP_PROD_003_IMPLEMENTATION.md); 234 node unit + 22 integration tests; local merge `3216f1d`, D-027. |
| 13 | `OBP-PROD-004` | Merged | `codex/obp-prod-004-routing` | `OBP-PROD-003` | [Routing evidence](outputs/OBP_PROD_004_IMPLEMENTATION.md); merge `2c39117` on `origin/main`, D-028. |
| 14 | `OBP-API-001` | Merged | `codex/obp-api-001-network-api` | `OBP-PROD-004` merged `2c39117` | [Implementation evidence](outputs/OBP_API_001_IMPLEMENTATION.md); merge `7d37a30` on `origin/main`, D-030. |
| 15 | `OBP-CLI-001` | Merged | `codex/obp-cli-001-networking` | `OBP-API-001` | [Evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md); implementation `e0dcc7d`, merge `04bcb30`, D-031. |
| 16 | `OBP-WEB-001` | Merged | `codex/obp-web-001-networking` | `OBP-API-001` | [Evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md); implementation `cade635`, merge `7f49eeb`, D-032. |
| 17 | `OBP-DESK-001` | Merged | `codex/obp-desk-001-networking` | `OBP-WEB-001` | [Evidence](outputs/OBP_DESK_001_IMPLEMENTATION.md); implementation `410a6a0`, merge `7e4fc14`, D-034. |
| 18 | `OBP-QA-001` | Merged | `codex/obp-qa-001-nat-canary` | `OBP-CLI-001`, `OBP-DESK-001` | D-041 merge `d695e4a`; [functional acceptance v2](outputs/OBP_QA_001_FUNCTIONAL_ACCEPTANCE_V2.md); consumer NAT skipped/unqualified; Linux P5 `c453f3e` qualified separately. |
| 19 | `OBP-MIG-001` | Merged | `codex/obp-mig-001-retire-legacy-seed` | `OBP-QA-001` merged `d695e4a` | D-042 merge `0604b55`; retained implementation `1d649ce`; [migration map and tests](outputs/OBP_MIG_001_IMPLEMENTATION.md). |
| 20 | `INT-KU-OBP-001` | Merged | MVP `0b050a4`, first-run `1c4c78a` and manual provisioning `4d01775` integrated in main | KU-QA-001 MVP and accepted OBP-QA-001; D-044/D-045 | D-046 accepts [A–D](tasks/20-INT-KU-OBP-001.md#actual-local-result--2026-10-01); D-047 publishes five fixes (`cbb7d58`); D-048 publishes [three manual diagnostics](tasks/20-INT-KU-OBP-001.md#owner-directed-manual-provisioning-merge--2026-10-04) through `403bbbb`, tree matches reviewed tip. Post-merge vNext/whitespace PASS; prior Rust 6/6/build/smoke/save/share checks reused. Preparation retained outside integration commits; writes non-transactional, broader setup/product-share open. Additional [new-dataset secrets](tasks/20-INT-KU-OBP-001.md#new-dataset-secret-provisioning--2026-10-04) helper Merged/published through `ea92e8a` under D-049 from `f3780c7`; exact tree/retained work/post-merge vNext/whitespace PASS; Python 4/4 + 3 CLI/1 host/build and save/share checks reused. Additional [Registry public-key input](tasks/20-INT-KU-OBP-001.md#owner-directed-registry-integration--2026-10-04) Merged/published through `2b14b23` under D-050 from `6bc146c` (implementation/guide `645c6bc`), based on `3de6a11`; exact tree/parents/post-merge vNext/whitespace/retained-work PASS, example 2/2 + signer/artifact 1/1/build/4 CLI refusals reused. D-050 grants standing merge/push permission; broader package acquisition/composition stays open. Additional [received-package guidance](tasks/20-INT-KU-OBP-001.md#latest-first-run-follow-up--received-registry-package-2026-10-04) committed/pushed `e77202c` under D-050: example 3/3 + signer/artifact 1/1, build/12 CLI calls/vNext PASS; shared authority/activation unchanged. Additional [composed-host config](tasks/20-INT-KU-OBP-001.md#latest-first-run-follow-up--composed-host-config-2026-10-04) committed/pushed `832425b` under D-050: one UTF-8 BOM, strict byte/schema bounds, PS5.1 NoClobber composition and two isolated host starts; example 6/6 + saved-read restart 1/1/build/vNext PASS. Additional [normal-host public reader and D-045 share seam](tasks/20-INT-KU-OBP-001.md#latest-follow-up--normal-host-public-reader-and-d-045-share-seam-2026-10-17) committed/pushed `2eb30d4` under D-050: `CanonicalPublicReader` in `ku_host` + shared `PublicKuShareCoordinator` (`ku_public_share` 4/4, `ku_product` 1/1, `ku_host` 1/1 + 2/2 feature, `ku_local_web` 6/6, `ku_peer_demo` check, KU/vNext/rustfmt PASS). Additional [registered editor Public KU share projection](tasks/20-INT-KU-OBP-001.md#latest-follow-up--registered-editor-public-ku-share-projection-2026-10-17) committed/pushed `bef0d28` under D-050: `share_prepare`, `share_confirm`, `share_intent`, `share_inspect` on `POST /api/vnext/ku/editor` (`KuServices::editor`, `onebrain-api` 39/39 + 42/42 feature `ku_host::tests` 3/3, `ku_public_share` 4/4, `ku_product` 19/19, `ku_local_web` 6/6, `ku_peer_demo` check, KU/vNext/rustfmt PASS). Additional [`/ku` Web Public KU share & peer inspection UI](tasks/20-INT-KU-OBP-001.md#latest-follow-up--ku-web-public-ku-share-and-peer-inspection-ui-2026-10-17) committed/pushed `25b89c5` under D-050 (`npm run test:ku` 92/92, `npm run build`, `npm run lint`, KU/vNext PASS). Additional [`ku_local_web` opt-in loopback network config](tasks/20-INT-KU-OBP-001.md#latest-follow-up--opt-in-loopback-network-runtime-config-for-ku_local_web-2026-10-18) committed/pushed `5dfc6bd` under D-050 (`ku_local_web` 7/7 + 8/8 feature, `ku_host::tests` 3/3, KU/vNext/rustfmt PASS). Additional [`VNextProductRuntime` `CanonicalPublicReader` binding and network lane fence](tasks/20-INT-KU-OBP-001.md#latest-follow-up--vnextproductruntime-canonicalpublicreader-binding-and-network-lane-fence-2026-10-18) committed/pushed `fbcda30` under D-050 (`onebrain-desktop --test lifecycle` 8/8 + 9/9 `vnext-network-runtime` + 11/11 `vnext-outbound-first`, `ku_product` 19/19, `ku_host::tests` 3/3, Base/vNext/rustfmt PASS). Additional [contributor entry refresh for D-045 share paths](tasks/20-INT-KU-OBP-001.md#latest-follow-up--contributor-entry-refresh-for-d-045-share-paths-2026-10-18) committed/pushed `ef9b459` under D-050 (vNext validator, local links and whitespace PASS). Additional [stale D-045 share preview feedback and cleanup in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--stale-d-045-share-preview-feedback-and-cleanup-in-ku-web-2026-10-18) committed/pushed `0f5d27d` under D-050 (`npm run test:ku` 94/94, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Additional [remaining D-045 share error feedback and invalidated preview cleanup in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--remaining-d-045-share-error-feedback-and-invalidated-preview-cleanup-in-ku-web-2026-10-18) committed/pushed `1855e8f` under D-050 (`npm run test:ku` 97/97, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Additional [D-045 storage/outbox/input share error feedback and consumed preview cleanup in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--d-045-storageoutboxinput-share-error-feedback-and-consumed-preview-cleanup-in-ku-web-2026-10-18) committed/pushed `c31b3a2` under D-050 (`npm run test:ku` 99/99, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Additional [remaining D-045 validation/system share error feedback and stale inspect view cleanup in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--remaining-d-045-validationsystem-share-error-feedback-and-stale-inspect-view-cleanup-in-ku-web-2026-10-18) committed/pushed `04f74cd` under D-050 (`npm run test:ku` 109/109, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Additional [saved-artifact inspection D-045 state transitions and PUBLIC guidance/prefill in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--saved-artifact-inspection-d-045-state-transitions-and-public-guidanceprefill-in-ku-web-2026-10-18) committed/pushed `a27ffee` under D-050 (`npm run test:ku` 110/110, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Additional [D-045 share input edit cleanup and `share_confirm` inspection prefill in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--d-045-share-input-edit-cleanup-and-share_confirm-inspection-prefill-in-ku-web-2026-10-18) committed/pushed `2a4da76` under D-050 (`npm run test:ku` 111/111, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Additional [D-045 `share_prepare` pre-call cleanup and Base `pending` isolation in `/ku` Web](tasks/20-INT-KU-OBP-001.md#latest-follow-up--d-045-shareprepare-pre-call-cleanup-and-base-pending-isolation-in-ku-web-2026-10-18) committed/pushed `d1a762c` under D-050 (`npm run test:ku` 114/114, `npm run build`, `npm run lint`, vNext validator and whitespace PASS). Registry distribution remains backlog; D-051 ARCH-03 is next. |
| 21 | `KU-ENC-001` | Merged | `codex/ku-enc-001-framework-contract` | `KU-RUN-001` | Owner-authorized merge `22599d0` on `origin/main`; [contract evidence](outputs/KU_ENC_001_CONTRACT.md), D-019. |
| 22 | `KU-ENC-002` | Merged | `codex/ku-enc-002-shared-encoder` | `KU-ENC-001`, `KU-RUN-001` | Owner accepted under D-020; merge `dc04b71` on `origin/main`; [implementation and verification](outputs/KU_ENC_002_IMPLEMENTATION.md). |
| 23 | `KU-ENC-003` | Deferred | Retained `codex/ku-enc-003-model-qualification` at `4a8f29d`; newer preparation retained uncommitted in the original working tree | KU-ENC-002; formal qualification inputs when resumed | D-044 removes this from MVP critical path. 200-source export and 85-test preparation retained; full harness/reviewer/run lock unfinished; model unqualified. |
| 24 | `KU-SEM-001` | Deferred | Foundation in main through `437dba0` / `3216f1d`; no new quality branch | Shared selection follow-up, outside MVP gate | D-024/D-044 defer remaining [fidelity gaps](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md); source integrated, v2 host activation not accepted. |

## Historical per-task results

### KU-WEB-001 reserved reconciliation feedback â€” 2026-09-07

Owner reported that checking an error's recorded outcome cleared the error and
only displayed `reserved`. Web now retains the original error during reconcile,
explains each recorded state, and places explicit reservation cancellation beside
the reserved outcome. Reserved is not interpreted as running inference or a
successful preparation. Thirteen Web tests pass, including reserved recovery,
editor unlocking after cancellation, and no automatic inference/save replay.
This UI correction does not resolve the separate model `oneof` schema failure.

### KU-WEB-001 semantic guidance and timeout â€” 2026-09-07

Owner approved semantic-analysis guidance and requested a higher experimental
timeout. The local Ollama policy now allows 10 minutes, retains standard token
limits, and preserves elapsed-time accounting through preparation. The provider
adds a pinned semantic guide while retaining the reviewed system prompt.
See [implementation and async follow-up scope](outputs/KU_WEB_001_SEMANTIC_TIMEOUT.md).
The owner-described async encoding/sharing direction remains a follow-up design
requirement; the current host still awaits preparation and does not publish.
Validation passed: 20 shared extraction tests, 12 KU API tests, 22 node KU tests,
18 Python contract tests, 12 Web tests, build and vNext validator. Real qwen3:8b
development preview/save/restart-read passed with 139.519s inference/validation
using an isolated test Registry. Local owner host rebuilt/restarted and checked
ready. Changes remain in the current review working tree; no merge or push.

### KU-WEB-001 timeout feedback â€” 2026-09-07

Owner local testing reported `ResourceExhausted / deadline`. Web feedback now
explains the 120-second workflow timeout, displays browser-observed progress and
elapsed time, and offers reconciliation without replay. Twelve Web tests and
the production build pass; backend limits and model quality status are unchanged.
See [timeout feedback evidence](outputs/KU_WEB_001_TIMEOUT_FEEDBACK.md).
This follow-up remains in the current task working tree for review.

### KU-WEB-001 experimental Ollama review evidence â€” 2026-09-06

Owner D-023 approved the addition. Implementation `5e540e5` adds governed text
intake, installed Qwen3 selection, the shared extraction workflow and an isolated
Windows Ollama worker with exact tokenizer/artifact and resource controls.
The actual `qwen3:8b` development probe passed intake â†’ ready preview â†’ explicit
private save â†’ restart/read, with inference/validation taking 109.037 seconds.
The test Registry is synthetic and confined to temporary integration tests.

Verification: 27 API library + 8 integration, 22 node KU, 18 encoder extraction,
10 Web component/transport tests and the real owned-worker shutdown probe pass.
Web build/lint, host example, feature-disabled API, generated Base projections,
vNext contracts and format/whitespace checks pass. Existing unrelated warnings
remain. See [run instructions, pins and precise evidence limits](outputs/KU_WEB_001_OLLAMA_IMPLEMENTATION.md).
AI remains unqualified; no holdout, mobile, rollout or merge work was performed.

### KU-ENC-002 review evidence â€” 2026-09-06

Closure: the owner reviewed, accepted and requested completion of KU-ENC-002,
reserving the next task for a new conversation (D-020). Clean reviewed tip
`a687600` matched its fetched remote. Fresh extraction (16), node KU (19),
bounded HTTP (3), format, generated-bundle, global vNext and diff checks passed.
Merge `dc04b71b48b27588800b682ef1e71d4506945db1` was pushed to `origin/main`.
KU-ENC-003 remains Planned; its branch and conversation were not created.
Task branches are retained; no cleanup was requested.

- Shared native strict validator/compiler, 48 oracle cases, two complete/partial
  multi-chunk jobs, bounded provider and durable node integration implemented.
- Native suites: 153 encoder, 123 node, 109 AI and seven existing SEM tests pass.
  KU coverage includes four real extraction process-kill phases plus the existing
  save crash matrix. Inference/token manifests are explicitly fixtures.
- Workspace check, format, generated bundle, 62 Python tests and global vNext
  validator pass. Clippy uses the existing warning policy, not zero-warning gates.
- [Evidence](outputs/KU_ENC_002_IMPLEMENTATION.md) records the first planner's
  single whole-source scope, unavailable unit/review authority and unqualified
  real-model/device behavior. KU-ENC-003 carries those qualification inputs.
- Review-stage scope included no new Base IDL/canonical semantics, default
  rollout, mobile implementation or model download. The subsequent merge is
  recorded above.

### KU-REV-001 review evidence â€” 2026-09-05

- Starting main: `3704da6b68237f50998f73c02bc5a2c59d27def8`, clean and equal to
  fetched `origin/main` before branching.
- Deliverable: [KU_AUTHORITY_AUDIT.md](outputs/KU_AUTHORITY_AUDIT.md), including
  authority/ownership/lifecycle/storage, legacy contradictions, owner-resolved
  direction and outstanding specification work.
- Owner clarification: [D-011â€“D-014](DECISIONS.md#d-011--deterministic-identity-after-semantic-normalization).
  Direct encode/verify OBT issuance is the requested new direction; the earlier
  benefit-only restriction is not to be silently retained in future design.
  Reward amounts, admission, replay prevention and settlement remain unspecified.
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS (99 tasks, 18 ADRs,
  37 negative assertions; existing canonical contracts unchanged).
- `git diff --check` â€” PASS.
- Local file-link check â€” PASS (47 links across the four handoff deliverables).
- No source/application/mobile changes. No runtime tests or production
  encode/verify/Registry-sync/reward implementation claims.
- Owner explicitly authorized merge and KU-REV-002 in this conversation.
  Revalidated the clean, pushed audit tip, merged and pushed `25d008d` to main;
  README pointer now advances to `KU-REV-002`. No branch deletion requested.
- Audited content checkpoint: `c89f848`, pushed successfully to
  `origin/codex/ku-rev-001-canonical-audit`. The final branch tip also contains
  the ledger-only follow-up recording this checkpoint; resolve the tracked
  branch for that tip rather than treating this content hash as a merge commit.

### KU-REV-002 review evidence â€” 2026-09-05

- Starting main: `80119e1311b1e95171e5613e0335ad3ef69fa2a4`, clean and equal
  to `origin/main`. This includes the authorized KU-REV-001 merge `25d008d`
  and its handoff pointer update.
- Deliverable: [KU_RUNTIME_GAP_MAP.md](outputs/KU_RUNTIME_GAP_MAP.md).
  Maps semantic identity, Registry, canonical/public/private storage, Base
  operations, Mapping/adoption, existing interfaces, migration and delegated
  work/reward gaps. D-011â€“D-014 remain required future direction.
- Changed only handoff documentation. No source fixes, public contract
  changes, rollout changes, minting, mobile implementation or migrations.
- Fresh focused results (Cargo run from `src/`, all with `--locked`):

| Command | Result | Evidence boundary |
|---|---|---|
| `cargo test --locked -p onebrain-node --lib vnext_local_runtime` | PASS: 1 | Local slice fixture; persistent Need reopen, in-memory Mapping after reopen. |
| `cargo test --locked -q -p onebrain-node --lib` | PASS: 104 | Default-feature node components; includes the preceding local test, Registry, migration, workflow and reward firewall. |
| `cargo test --locked -q -p onebrain-node --test base_runtime_facade --test canonical_exchange --test durable_data_recovery --test p0_capability_truth` | PASS: 9 + 5 + 4 + 1 | Base uses test adapters; source recovery uses in-memory Vault plus disk staging. |
| `cargo test --locked -q -p onebrain-node --test vnext_index_parity` | PASS: 3 | Canonical index parity and legacy mutation fence. |
| `cargo test --locked -q -p onebrain-api --test base_contract` | PASS: 8 | Base HTTP contract; test local adapter does not implement KU operations. |
| `cargo test --locked -q -p onebrain-api --features vnext-network-runtime --lib vnext_api` | PASS: 7 | Feature-enabled Need and explicit Public Use HTTP fixtures; 10 other tests filtered out. |
| `cargo test --locked -q -p onebrain-node --features vnext-network-runtime --lib vnext_distributed_kql` | PASS: 3 | Two local test peers, private match/restart dedup, lifecycle tombstones and legacy plaintext rejection; 167 other tests filtered out. |
| `cargo test --locked -q -p ku-encoder --lib` | PASS: 137 | Controlled encoder/resolver/builder tests; no live Ollama or shared T1/AI canonical identity conformance. |
| `cargo test --locked -q -p ku-ai --lib vnext_` | PASS: 21 | Executor/fidelity component tests; no durable distributed worker settlement. |
| `npm run test:vnext` (from `src/onebrain-web`) | PASS: 2 | Receipt tests only; no full browser or Desktop GUI run. |

- `python scripts/ci/validate_vnext_contracts.py` â€” PASS (99 tasks, 18 ADRs,
  37 negative assertions; existing canonical contracts unchanged).
- `git diff --check` â€” PASS. Local file-link check â€” PASS (104 links across
  the four changed handoff files).
- Existing compiler dead-code/unused-import warnings were observed; no fixes made within
  this read-only audit. No claim of whole-workspace, live multi-node
  qualification, full-size Registry qualification or D-011â€“D-014 completion.
- After this review was reported, the owner requested "lÃ m task káº¿ tiáº¿p".
  Treated this as authorization for the prerequisite merge and KU-CON-001;
  rechecked the clean/pushed branch and contract/diff gates, then merged and
  pushed `b872263` to main. No branch deletion requested.
- Audited content checkpoint: `496d340`, pushed successfully to
  `origin/codex/ku-rev-002-runtime-map`. The branch tip additionally contains
  the ledger-only commit recording this checkpoint; resolve that tracked
  branch for its final tip. This content checkpoint is not a merge commit.

### KU-CON-001 review evidence â€” 2026-09-05

- Starting main: `d8effb772b0cb7766e91b799dd598061a81a9df5`, clean, pushed
  and synchronized after KU-REV-002 merge `b872263` and handoff update.
- Deliverables: [owner-review profile](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md),
  [machine inventory](../../../src/test-vectors/vnext/ku-product-workflow-v1.json),
  [focused validator](../../../scripts/ci/validate_ku_product_contract.py),
  [mutation tests](../../../scripts/ci/test_validate_ku_product_contract.py).
  The global vNext validator includes the candidate check; normative coverage
  and specification index mark this as contract evidence only.
- Proposed review items are KU-PC-A (separate finite normalized semantic
  identity/domain), KU-PC-B (11 local operations and 18 typed bounded DTOs),
  KU-PC-C (private local revision journal). New domains/command IDs/routes are
  not allocated or activated. Owner acceptance plus explicit registration,
  compatibility and golden-vector gates precede runtime dispatch.
- D-012 publisher/peer Registry distribution, D-013 durable delegated work and
  blind verification, and D-014 direct work-based issuance remain mandatory
  linked specification/implementation dependencies. D-014 does not wait for
  BenefitEvent and does not fall back to bounty or simulated legacy balances.
- `python scripts/ci/validate_ku_product_contract.py` â€” PASS:
  11 operations, 18 DTOs, 11 valid/invalid DTO fixtures. Fixture byte/hash values
  are shape examples, not golden canonical/hash conformance evidence.
- `python -m unittest scripts.ci.test_validate_ku_product_contract
  scripts.ci.test_validate_vnext_product_profile
  scripts.ci.test_validate_vnext_ws_profile
  scripts.ci.test_validate_vnext_cli_profile
  scripts.ci.test_validate_vnext_desktop_web_ux_profile
  scripts.ci.test_validate_base_v1_runtime_interface` â€” PASS: 101 tests,
  including 33 new KU tests with mutation subcases and 68 existing contract tests.
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS: the candidate plus
  existing 99 tasks, 18 ADRs, 37 negative assertions, 841 normative lines and
  479 specification links. Existing canonical profile/domain/IDL inventories
  and generated runtime declarations unchanged.
- `git diff --check` â€” PASS. Only candidate specification/inventory,
  contract validators/tests and handoff/index/coverage files changed. No
  application runtime, screen, mobile, networking or rollout implementation.
- Local file-link check â€” PASS: 53 links across the candidate profile and
  changed handoff README/PROGRESS files, in addition to the global link gate.
- Current pointer remains KU-CON-001 pending owner review and merge. This is
  completion of the owner-reviewable deliverables, not approval/freeze of new
  public behavior and not implementation of KU-RUN-001.
- Reviewed content checkpoint: `8b6aa9d`, pushed successfully to
  `origin/codex/ku-con-001-product-contract`. The final tracked tip additionally
  contains the ledger-only commit recording this checkpoint. This is not a
  merge commit or owner acceptance of KU-PC-A/B/C.

### KU-CON-001 owner acceptance â€” 2026-09-05

- Owner answered "Ä‘á»“ng Ã½" after the completed review at `b5956e8`.
  [D-015](DECISIONS.md#d-015--ku-product-contract-accepted) records acceptance
  of KU-PC-A/B/C and merge of the reviewed task. The preceding review section
  is historical evidence of the candidate before this acceptance.
- Profile and machine inventory now report owner approval with registration
  pending. Existing byte formats and numeric domain/payload inventories remain
  unchanged; no runtime or rollout was enabled.
- Added a mutation test binding acceptance to D-015, the reviewed commit and
  all three approved items. Approval cannot erase technical dispatch gates.
- Approved contract tip `9f67022` was clean, pushed and synchronized before
  merge. Merged and pushed `2cbc8f263961d5a6368ef2c7bdc5a77f209d5b21`
  to main. Current task advances to KU-RUN-001, which has not started.
  Preserve the registration/vector gates before dispatch; no branch deletion
  or runtime/default-rollout change was requested or performed.
- Re-ran the same six-module unittest command from the review evidence:
  PASS, 102 tests (34 KU contract tests and 68 existing contract tests).
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS, including 480
  specification links and unchanged canonical inventories.
- `git diff --check` â€” PASS.

### KU-RUN-001 registration preflight â€” 2026-09-05

- Started from `91cc715b547b71941f6d66fea2093fc2326eb481`, clean and equal
  to freshly fetched `origin/main`; created the exact task branch
  `codex/ku-run-001-shared-service`. State moved through `In progress` during
  preflight to `Blocked`. Current task remains KU-RUN-001.
- Read the start/resume prompts, repository instructions, handoff
  README/decisions/progress, task, authority audit, runtime gap map and approved
  KU profile. Inspected the implicated Base interface/ownership/storage/canonical
  profiles, inventories, validator and local adapter dispatch boundary.
  This is prerequisite evidence, not the completed runtime required-read or
  implementation acceptance gate.

#### Missing gate and scope boundary

1. [KU profile Â§1](../../specs/vnext/KU_PRODUCT_WORKFLOW_PROFILE_V1.md#1-authority-and-approval-boundary)
   requires domain registration and golden equality/separation vectors before
   production hashing, and generated Base payload registration/compatibility
   revision before dispatch. D-015 accepts KU-PC-A/B/C but expressly retains
   those gates; their approval is not being reopened.
2. The [KU inventory](../../../src/test-vectors/vnext/ku-product-workflow-v1.json)
   still has `owner_approved_pending_registration`,
   `implementation_enabled: false`, an empty `base_local_command_ids`,
   `domain_registry_allocated: false`, and null operation wire IDs. The
   [KU validator](../../../scripts/ci/validate_ku_product_contract.py)
   explicitly requires these values. Its successful result proves contract
   consistency in that pending state, not readiness to dispatch.
3. [Canonical Â§6.2](../../specs/vnext/CANONICAL_PROFILE_V1.md#62-reserved-v1-domains)
   calls addition of a domain a contract change. The current canonical
   inventory has no `semantic-content` entry. The
   [Base IDL](../../../src/test-vectors/vnext/base-v1-runtime-interface-v1.json)
   has no generated KU operation DTOs. Its generic `BaseLocalCommandV1.kind`
   is not an allocated KU discriminator. [Base Â§Â§8â€“9](../../specs/vnext/BASE_V1_RUNTIME_INTERFACE_PROFILE.md#8-generated-projections)
   require generated projections and append-only history with a profile-minor
   increment for additive registration.
4. The selected task scopes runtime service implementation. The handoff
   [working rules](README.md#working-rules) state: "Any wider change requires
   an explicit scope revision." The task does not explicitly assign the
   outstanding canonical/IDL registration changes. Treating implementation
   as permission to change those frozen inventories would silently expand
   that scope. This is a missing prerequisite/scope assignment, not a new
   conflict with the owner's approved semantic design.
5. [The Base adapter](../../../src/onebrain-node/src/base_runtime.rs) currently
   defaults to `UnavailableBaseLocalOperationAdapter`; local confirmation
   passes a `BaseLocalCommandV1` to that adapter. Installing a hand-written KU
   dispatcher with invented IDs would bypass the gate and would not satisfy
   the approved service contract.

#### Concrete proposed scope extension, pending owner direction

Extend KU-RUN-001 with a prerequisite registration phase, then continue its
existing runtime objective on the same branch:

1. Register the already approved `semantic-content/1` domain in the canonical
   contract/inventory and typed core domain declarations; add golden canonical
   byte/hash equality and separation vectors for the finite approved
   normalization. Preserve existing domains, IDs and original artifact bytes.
2. Register the approved eleven operation payload mappings and eighteen DTOs
   in the Base machine IDL; append discriminator history, advance the additive
   profile/compatibility declarations and regenerate affected projections.
   Add old-host rejection and generation/history drift checks. Do not allocate
   REST routes, CLI commands or WS events in this phase.
3. Update the KU inventory/validator to recognize registered state only when
   those exact registrations and vector gates pass. Then implement the
   node-owned service, encrypted atomic/recoverable save and all existing
   KU-RUN-001 acceptance cases. Keep API/UI, OBP orchestration, mobile
   implementation and D-012â€“D-014 distribution/work/reward changes excluded.

Owner direction is requested only for this scope extension. No request to
approve KU-PC-A/B/C again, merge, delete branches or enable default rollout.

#### Fresh validation and evidence limit

- `python scripts/ci/validate_ku_product_contract.py` â€” PASS: 11 operations,
  18 DTOs, 11 fixtures; tool explicitly reports registration pending.
- `python -m unittest scripts.ci.test_validate_ku_product_contract
  scripts.ci.test_validate_base_v1_runtime_interface` â€” PASS: 74 tests.
- `python scripts/ci/validate_vnext_contracts.py` â€” PASS: existing contract
  inventories, including 21 foundation domains and 27 Base runtime operations.
- `git diff --check` â€” PASS. Local file-link check for README/PROGRESS â€” PASS.
- Preflight content checkpoint `8464770` was pushed to
  `origin/codex/ku-run-001-shared-service`; the branch also includes the
  ledger-only follow-up recording this checkpoint. No merge or branch deletion.
- Source, canonical contracts, generated declarations, tests and rollout state
  are unchanged. Only README/PROGRESS handoff records changed. Runtime tests,
  workspace check and Rust format are not claimed; KU-RUN-001 remains
  incomplete and cannot be marked Review on this evidence.

### KU-RUN-001 review evidence â€” 2026-09-06

- Implementation checkpoint `b608a82` is pushed to
  `origin/codex/ku-run-001-shared-service`. The branch tip also contains this
  ledger-only follow-up recording that checkpoint; neither commit is a merge.
- D-016 supersedes the historical preflight blocker above. The owner approved
  its registration scope and reported `onebrain.live`; no DNS/deployment work
  was needed or performed.
- Delivered registered semantic identity/goldens, Base 1.2 payload/DTO/history
  generation, authenticated node-owned KU service, encrypted recoverable
  private save, snapshot index/revisions and typed lifecycle/recovery fences.
  Full evidence and integration limits:
  [KU_RUN_001_IMPLEMENTATION.md](outputs/KU_RUN_001_IMPLEMENTATION.md).
- Validation: node library **117 PASS**, core foundation **196 PASS**, semantic
  golden suite **2 PASS** plus child run, Base contract **21 PASS**, Base facade/
  exchange/recovery/capability/index integration **22 PASS**. Six real process
  kills verify partial-save visibility and exact recovery without model replay.
- Whole-workspace `cargo check --workspace --locked -q` and
  `cargo fmt --all -- --check` pass. Generated `--check`, global vNext contract
  validator and **85 Python tests** pass. Existing TypeScript conformance and
  both Dart conformance tests pass outside the mobile subtree.
- Host input/Registry/public-read ports are explicit. Test encoders are
  controlled fixtures; no live AI, automatic Registry synchronization, remote
  work, minting or product UI qualification is claimed. Private export is a
  Base management reservation; portable KU metadata/archive round-trip remains
  unqualified. Network and default rollout are unchanged.
- Current task remains KU-RUN-001 for owner review. Do not merge/delete the
  branch or start KU-API-001 without the corresponding instruction.

### Encoder framework research and task amendment â€” 2026-09-06

- Owner accepted KU-RUN-001 and requested the D-017 framework research and
  backlog amendment. Runtime remains unchanged and unmerged.
- [Research output](outputs/KU_ENCODER_FRAMEWORK_RESEARCH.md) records source
  findings from legacy tool and v2 extraction paths, unsafe semantic defaults,
  available structured adapter, overlap with AI-001/AI-003/FID/MOB-06 and the
  shared framework gap. Official Ollama/llama.cpp/Anthropic references were
  checked; no live model or latency/accuracy benchmark was run.
- Added KU-ENC-001/002/003; supplemented API, QA and integration acceptance,
  dependency graph and indexes. Research includes a candidate workflow,
  prompt guidance, data responsibilities, resource policy and evaluation plan.
  New machine schema and production implementation remain future task outputs.
- This documentation-only amendment stays on the accepted task's handoff
  branch. It grants no mobile implementation, runtime rollout or merge action.
- Validation passed: 200 local links, 23 task files with an acyclic dependency
  graph and the new API/QA gates, global vNext contract validator and
  `git diff --check`. All changes are handoff Markdown; no runtime test rerun
  or live-model/mobile performance claim is needed for this documentation edit.

### KU-RUN-001 merge and encoder handoff â€” 2026-09-06

- Owner explicitly authorized merging KU-RUN-001 into main and starting
  KU-ENC-001. Fresh generated/global contract checks, 13 KU runtime tests,
  workspace check and format passed on the clean synchronized branch.
- Merged tip `13e03f3` with merge `d1417018a236798a910ceb625fbe5fd0b10dc406`
  and pushed `origin/main`. No branch deletion or rollout change.
- README/progress pointer advances to KU-ENC-001; the accepted direction is
  D-018. No further merge or framework-direction approval is required here.

### KU-ENC-001 contract review â€” 2026-09-06

- Started from synchronized main `5d8fba077076597da163d5f17b8e290f28eb12c9`.
- Reviewed contract commit: `a6f0a00`. All 18 manifest artifact hashes were
  compared against exact Git index blobs to verify LF portability before commit.
- [Framework contract](../../specs/vnext/KU_EXTRACTION_FRAMEWORK_PROFILE_V1.md)
  and [review evidence](outputs/KU_ENC_001_CONTRACT.md) define one host-controlled
  extraction path, six DTO schemas, vi/en prompts, eight generated artifacts,
  48 cases/two jobs, explicit unsupported semantics and qualification gates.
- Validation: 18 encoder tests, 44 KU/product regression tests, seven existing
  Rust SEM tests, generated bundle
  and Base checks, global vNext validator, independent Draft202012 comparison
  and diff/link/dependency checks. Commands and evidence boundaries are recorded
  in the review output.
- No production inference/compiler, real model/hardware qualification, mobile
  implementation/evidence, accepted bytes, IDL registrations or rollout changes.
- Task remains Review on its own branch; no merge/deletion or KU-ENC-002 start
  is included in this contract handoff.

### KU-ENC-001 owner acceptance

Owner acceptance update: the owner reviewed and accepted KU-ENC-001 at handoff
`e4c1bb6`; D-019 records the concrete accepted contract. This update changes only
handoff metadata, preserving the validated bundle. No repeat contract approval
is needed. Merge remains a separate explicit instruction under D-010; do not
start the dependent KU-ENC-002 branch before that merge.
Generated bundle integrity, global vNext validation and diff checks pass for
this handoff-only update.

### KU-API-001 review evidence â€” 2026-09-06

- Implementation checkpoint `29c34d1` was pushed successfully. The following
  ledger-only commit records that content checkpoint; neither is a merge.
- D-021 owner follow-up authorized the early local API/Web MVP path. API work
  starts from `ca1d4c22f61864a5bde53edf5c399e6ffdbc1943` on its own branch;
  the qualification branch remains preserved separately at `4a8f29d`.
- Added the versioned local KU REST projection and three authenticated routes
  for all eleven registered operations. No new Base/DTO IDs, canonical bytes,
  extraction implementation, Registry authority or WS vocabulary.
- [Implementation evidence](outputs/KU_API_001_IMPLEMENTATION.md) records
  API behavior, host integration needs, bounded/private envelopes and the
  explicit unqualified-model status. Host intake and Web UI remain the next
  product integration step; a complete runnable Web demo is not claimed here.
- Default API suite: 22 library plus 8 integration tests pass. Opt-in network
  library suite: 24 pass. Feature-disabled check, workspace format, generated
  Base projections, global vNext contracts and diff checks pass.
- New tests exercise exact save/read/replay, pagination and revisions,
  cancellation without the node mutex, auth/generation/privacy rejection,
  unavailable dependencies, export boundaries, unresolved and unqualified AI
  requests, response overflow and all Base error policies.
- Owner's new holdout files were not opened; no inference, download, rollout,
  merge or branch deletion. Current pointer remains KU-API-001 for review;
  KU-WEB-001 follows the accepted API merge.

### KU-API-001 merge and Web handoff â€” 2026-09-06

- Owner reviewed and accepted tip `423b7b8`, authorizing merge and handoff
  closure under D-022. The task branch was clean, pushed and synchronized.
- Fresh `cargo test --locked -q -p onebrain-api`: 22 library and 8 integration
  tests pass. Generated Base check, global vNext contracts, workspace format
  and diff checks also pass. No runtime changes were made during closure.
- Merged and pushed `3eba370df1df91627595e0acbf7645d94ea75276` to main.
  API task is Merged; current pointer advances to KU-WEB-001, still Planned.
- The next conversation should build the small local Web journey under D-021,
  accounting for API host-intake/Registry dependencies and unqualified AI.
  It must not invent client-side authority or treat fixture evidence as a
  usable production source pipeline.
- No Web branch or implementation started, holdout opened, model run, rollout
  enabled or branch deleted during closure. KU-ENC-003 remains separate.

### KU-WEB-001 Ollama follow-up â€” 2026-09-06

- Owner requests actual Ollama inference, model selection (`qwen3:8b`) and text
  encoding through the previously designed shared framework. This expands the
  manual MVP; it does not accept the existing manual-only deliverable as final.
- [Concrete amendment and exact conflict](outputs/KU_WEB_001_OLLAMA_AMENDMENT.md)
  proposes a local experimental lane with `model_qualified: false`, preserving
  technical admission/custody/validation/recovery controls. The existing REST
  contract explicitly rejects AI before full qualification; owner direction is
  required before changing that activation rule. No request to reapprove the
  shared architecture or to merge/delete the branch is being made.
- Installed `qwen3:8b` was confirmed by `ollama list`; no inference/model download,
  holdout read or runtime change. Qualification branch and corpus stay separate.
- The reviewed manual implementation remains at `ac5ce80` plus handoff `f316e5a`.
  This follow-up records the requested change; it does not claim AI completion.

### KU-WEB-001 implementation review â€” 2026-09-06

- Implementation checkpoint `ac5ce80` was pushed successfully. The branch tip
  additionally contains this ledger-only record; neither commit is a merge.
- Started from clean `main` at `798eabfee8acaa8cb473aa99fa52ccfd6f124f7e`,
  equal to fetched `origin/main`; exact task branch `codex/ku-web-001-workflow`.
- Implemented the local `/ku` manual create/preview/validate/save/search/inspect/
  revise journey, plus the scoped opt-in host editor and loopback launch example.
  [Evidence and run instructions](outputs/KU_WEB_001_IMPLEMENTATION.md) specify
  real host prerequisites and the finite predicate/text editor's limitations.
- Added the versioned editor transport contract before implementation. Native
  source custody, signed Registry selection, generated KU payloads, canonical
  validation, encrypted persistence and Base generation fences retain authority.
  No new Base IDs, canonical bytes, network lane, model or default rollout.
- API default suite: 24 library + 8 integration tests pass; node KU suite: 19
  pass; opt-in network-feature API library suite: 26 pass. Host example and
  feature-disabled API compile. Web: 8 component/transport
  tests, 2 existing receipt tests, automated accessibility, production build and
  lint pass under the existing non-KU warning policy. Generated Base, global
  vNext and format/diff checks pass. Full details and evidence limits are linked.
- Operator Registry/source/Vault provisioning is required. No production dataset
  or full physical-browser/assistive-technology run is claimed; source/Registry
  fixtures appear only in tests. New VI/EN holdouts and KU-ENC-003 remain untouched.
- Ready for owner review. Keep this task current and preserve its branch;
  no merge, deletion, deployment or next-task start is included.

## Update protocol — D-044

Use the bounded checkpoint template and update procedure in
[NEXT_CONVERSATION](NEXT_CONVERSATION.md) after every meaningful task/checkpoint.
Replace the current payload; add only a short historical result, not another full
handoff snapshot. Read the marked payload first when resuming a new conversation.

1. Before a task starts, record the parent task, actual branch/base and relevant
   dirty/unmerged dependencies in Current checkpoint. Reuse existing work.
2. Keep substep checklists in that task. Record a brief command/result and next
   action here; use its existing output only for a substantive reusable explanation.
3. Mark local implementation `In progress` or `Review` honestly; no mandatory push,
   clean-main reset or extra document is needed merely to continue local MVP work.
4. Record `Merged` only after an actual merge and state publication separately.
   Apply owner-authorized Git actions to concrete reviewed content; do not silently
   discard, delete or copy old branch ledgers over current state.
5. Name only blockers of the scoped MVP as current blockers. Keep model/production
   qualification and later research gaps in the central deferred backlog.

## Historical blocker records

### 2026-09-23 — OBP-QA-001

Local preflight is complete; product acceptance is not. The current task has no
verified two-consumer independent-network/two-relay environment or scoped test
host assembly/execution authority. No independent-host fault or native GUI/OS
journey was run. P5 production-reference also lacks this candidate's signed
request/inventory, three physical hosts, provider/topology evidence, receipts
and complete fault/oracle collection. Historical Linux qualification cannot
promote this candidate/platform. See [scenarios](outputs/OBP_QA_001_ACCEPTANCE_V1.md)
and [measured local results](outputs/OBP_QA_001_PREFLIGHT.md). Do not enable live
networking to fill this gap; task 19 remains Planned. All changes are local,
uncommitted and retained on the QA branch.

### 2026-09-20 â€” KU-SEM-001 (historical gate; D-024 defers further work)

Owner approved the separate draft/selection v2 representation and implementation.
Shared encoder, node dispatch and Web rendering have been added in the original
working tree; [implementation evidence](outputs/KU_SEM_001_IMPLEMENTATION.md)
records checks and failed development iterations. The activation gate remains
unaccepted because the real-model rocket/reference outputs still lose meaning.
No canonical conflict was silently resolved, no independent verifier is claimed,
and no commit/push/merge or live-host restart was performed. Continue from the
original dirty baseline, preserving failed raw reports and v1 records.

Continuation: predicate/qualifier overlap now dispatches a finite-choice repair.
The real Gemma replay accepted that repair and retained both revisions, but the
rocket case still fails reference/future meaning checks. Encoder checks are now
70 passed/one ignored, API 8 passed; final staging build and validator passed.
The completed Qwen transport retry (1/6) and Gemma comparison (2/3) remain known
development evidence only. See the linked report for commitments and limitations.

Record the exact canonical conflict, missing authority, failing gate or external
dependency here. Do not replace `Blocked` with an inferred product behavior.
