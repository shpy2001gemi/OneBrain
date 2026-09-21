# Next conversation — OBP API prerequisites

Current checkpoint: 2026-09-21. Under D-028, the owner requested merging 004
and pushing all retained work. Implementation `8f7d327` was merged into main as
`2c39117183f167ef4d333757b050d6306043e068`. Main and the routing branch were
pushed atomically to origin, including the full D-027 baseline at `3216f1d`.

## Workspace and read set

Use the original `C:/Users/shpy2/Documents/OneBrain` workspace, now on `main`.
Inspect status and preserve any subsequent changes; do not substitute another
worktree or discard the retained baseline.

1. Repository `AGENTS.md` and applicable subtree rules.
2. [PROGRESS.md](PROGRESS.md), [DECISIONS.md](DECISIONS.md), especially D-024..D-028.
3. [Accepted OBP profile](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md).
4. [Routing evidence and limits](outputs/OBP_PROD_004_IMPLEMENTATION.md), and the
   task-001..003 evidence it builds on.
5. [OBP-API-001](tasks/14-OBP-API-001.md) and its own required read set.

## Next task and boundaries

OBP-PROD-004 is Merged. Fresh verification passed: 242 node unit tests,
22 integration tests, 315 ku-net tests, six OBP contract tests and aggregate
vNext validation. These are local integration results, not Internet/NAT or
cross-platform qualification.

OBP-API-001 remains Planned. Its task-004 merge dependency is now satisfied;
its exact routes, authentication/capability binding, host intake and reconciliation
transport prerequisites still apply before handlers. API implementation was not
started as part of the merge/push action.

Keep semantic tuning deferred under D-024. Preserve the running
`C:/Users/shpy2/Documents/OneBrainLocal` host, Registry, jobs, keys and Ollama.
Do not enable networking, change models, activate v2 or claim qualification.
Mobile and default rollout remain separate scopes.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
Baseline đầy đủ qua OBP-PROD-004 đã merge tại 2c39117 và push origin/main theo D-028.
Đọc PROGRESS và evidence hiện tại, giữ mọi thay đổi phát sinh.
Tiếp tục OBP-API-001 theo đúng prerequisite và task scope; điều kiện 004 merged đã đạt.
Không tuning model, không tự bật networking hay thay đổi live host.
```
