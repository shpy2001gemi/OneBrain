# Next conversation — OBP CLI review

Current checkpoint: 2026-09-22. OBP-CLI-001 is implemented locally for review in
`C:/Users/shpy2/Documents/OneBrain` on `codex/obp-cli-001-networking`, from clean
`b66780f`. Preserve the complete working tree. This task has not been committed,
pushed or merged. API implementation `de04a57` remains unchanged, merged at
`7d37a30` and published under D-030. D-029 transport approval is already satisfied.

## Read set

1. Repository AGENTS.md and applicable subtree rules.
2. [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), D-024..D-030.
3. [Task 15](tasks/15-OBP-CLI-001.md),
   [CLI projection](../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md),
   [implementation evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md).
4. Accepted [OBP product](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md)
   and [local API](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md) contracts, plus
   [API evidence](outputs/OBP_API_001_IMPLEMENTATION.md).

## Review boundary

The feature-gated `onebrain obp` client maps all 13 accepted operations and
metadata reconciliation through authenticated loopback REST. It uses the existing
node contract validator and preserves exact payload/key/generation/session fences.
Every mutation previews its payload and requires the exact key on stdin.
Management grants come only from the host and are sent only on management calls.
No client-created runtime, capability mint, raw input upload or raw-address dial.

Host intake remains the accepted in-process port. Source admission requires an
already registered input_ref; a separately provisioned host must supply scoped
management authority. Source disable preserves replay floors. There is no accepted
source-delete, intent-cancel, peer-directory or outbox-list endpoint. CLI does not
invent those operations. The evidence lists exact coverage and remaining limits.

Next action: review the local CLI implementation and its evidence. Merge/push
requires owner direction for these retained changes. Do not start another task
or mark this one Merged without an actual merge. OBP-WEB-001 remains Planned
and dependency-ready from the prior API merge.

Keep D-024 semantic tuning deferred. Preserve the running OneBrainLocal host,
Registry, jobs, keys and Ollama. No networking activation, model changes, v2 host
activation, mobile work, default rollout or NAT/platform qualification is included.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
Review OBP-CLI-001 trên codex/obp-cli-001-networking; giữ toàn bộ thay đổi local.
Đọc PROGRESS, task 15, CLI projection và implementation evidence, rồi kiểm tra
code/test đúng scope. API de04a57 đã merge tại 7d37a30; D-029 đã chấp thuận.
Không hỏi lại transport approval, không tuning model, không bật networking hay
thay đổi live host. CLI phải dùng shared node-owned service qua local API.
```
