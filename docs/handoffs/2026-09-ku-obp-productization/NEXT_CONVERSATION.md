# Next conversation — review routing, then follow dependencies

Current checkpoint: 2026-09-21. Owner said “vây hãy merge rồi làm tiếp”.
D-027 records the local merge and continuation. KU-CLI/Web/semantic baseline and
OBP-PROD-001..003 were preserved in commit `437dba0`, then merged locally as
`3216f1ddf0cd72098924ccdf9fb0ef6b30186b78`. No remote push occurred.

## Correct workspace and baseline

Use `C:/Users/shpy2/Documents/OneBrain`, branch `codex/obp-prod-004-routing`.
Task-004 implementation, tests and evidence are uncommitted in this original tree.
Inspect status and retain every change. Do not substitute main, another worktree,
a fresh checkout or HEAD alone for this workspace. Do not reset, clean or stash
away its changes. D-027 supersedes the earlier prohibition on the specific baseline
merge; it does not authorize remote publication or live networking activation.

## Read set

1. Repository `AGENTS.md` and applicable subtree rules.
2. Current checkpoint and ledger in [PROGRESS.md](PROGRESS.md), D-024..D-027 in
   [DECISIONS.md](DECISIONS.md).
3. Accepted [OBP product profile](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md)
   and [contract evidence](outputs/OBP_PROD_001_CONTRACT.md).
4. [Lifecycle evidence](outputs/OBP_PROD_002_IMPLEMENTATION.md),
   [discovery evidence](outputs/OBP_PROD_003_IMPLEMENTATION.md), and current
   [routing evidence and limits](outputs/OBP_PROD_004_IMPLEMENTATION.md).
5. [Task 004](tasks/13-OBP-PROD-004.md), then the next task's own prerequisite and scope.

## Current work and next gate

OBP-PROD-004 is in Review: the existing product owner feeds expected-peer routing
into durable outbox delivery, with authenticated relay alternatives, atomic
acknowledged checkpoints, restart/compaction/archive preservation, generation
fences and bounded retry. 242 node unit and 22 integration tests passed, as did
six OBP contract tests and the aggregate vNext validator. See evidence for focused
checks, feature builds and the boundary between loopback tests and qualification.

Task 004 is not committed, merged or published. OBP-API-001 follows after task 004
is merged or the owner explicitly grants a local prerequisite exception. Do not
infer that the earlier merge of tasks 001..003 waives this new gate. Public API/UI,
mobile, cross-platform NAT qualification and default rollout remain separate.
Optional LAN/hole-punch paths require admitted host capabilities; fresh verified
manual invitations remain the peer source. No live host has been activated.

KU semantic foundation remains accepted for this development stage under D-024;
known quality gaps remain in the [contributor backlog](outputs/KU_SEM_001_CONTRIBUTOR_BACKLOG.md).
Do not run model experiments or reopen semantic tuning. Preserve the running
`C:/Users/shpy2/Documents/OneBrainLocal` host, Registry, jobs, keys and Ollama.
Do not enable networking, change models or claim model/platform qualification.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
Baseline đầy đủ qua OBP-PROD-003 đã merge local tại 3216f1d theo D-027, chưa push.
OBP-PROD-004 đã triển khai và kiểm tra local, đang Review; các thay đổi task 004
còn chưa commit trên codex/obp-prod-004-routing. Đọc evidence và PROGRESS hiện tại,
giữ mọi thay đổi và tiếp tục đúng dependency/task scope. OBP-API-001 còn điều kiện
004 merged hoặc ngoại lệ rõ ràng từ owner. Không thay baseline bằng main/worktree
khác, không tuning model, không tự bật networking hay thay đổi live host.
```
