# Next conversation — OBP API implementation

Current checkpoint: 2026-09-22. Under D-028, the owner requested merging 004
and pushing all retained work. Implementation `8f7d327` was merged into main as
`2c39117183f167ef4d333757b050d6306043e068`. Main and the routing branch were
pushed atomically to origin, including the full D-027 baseline at `3216f1d`.

## Workspace and read set

Use the original `C:/Users/shpy2/Documents/OneBrain` workspace, now on
`codex/obp-api-001-network-api`, based on `836af91`. Implementation `de04a57`
is committed and pushed; the latest branch tip also records this handoff.
Retain all API implementation, transport, checker and handoff changes.
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

OBP-API-001 transport is accepted under D-029; implementation is in Review. Read
[proposal and verification](outputs/OBP_API_001_TRANSPORT_REVIEW.md) and
[exact transport](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md).
Six registered routes map 13 unchanged operations. Node-owned command journal,
host capabilities/input references, source policy, snapshot pagination and thin
REST/WS handlers are implemented locally. The owner accepted the complete REST/WS
extension; do not request transport approval again. Read
[implementation evidence](outputs/OBP_API_001_IMPLEMENTATION.md), including the
final passing checks and limits, before advancing a dependency.
Task-004 publication permission does not authorize merging this API task.
The next action is owner review/merge direction for API-001. Keep CLI/Web
dependent tasks Planned until the API merge prerequisite is actually satisfied.

Keep semantic tuning deferred under D-024. Preserve the running
`C:/Users/shpy2/Documents/OneBrainLocal` host, Registry, jobs, keys and Ollama.
Do not enable networking, change models, activate v2 or claim qualification.
Mobile and default rollout remain separate scopes.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
Baseline đầy đủ qua OBP-PROD-004 đã merge tại 2c39117 và push origin/main theo D-028.
OBP-API-001 có transport được chấp thuận theo D-029, implementation de04a57 đã push trên codex/obp-api-001-network-api và đang Review.
Đọc PROGRESS, OBP_API_001_TRANSPORT_REVIEW.md và OBP_LOCAL_API_PROFILE_V1.md; giữ mọi thay đổi.
Handlers và shared façade đã triển khai local; kiểm tra evidence, test và git status rồi tiếp tục đúng scope. Không hỏi lại transport approval.
Không tuning model, không tự bật networking hay thay đổi live host.
```
