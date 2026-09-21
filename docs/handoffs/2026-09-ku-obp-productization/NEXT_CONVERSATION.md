# Next conversation — OBP CLI workflow

Current checkpoint: 2026-09-22. Under D-030, the owner requested merging and
pushing all retained API work. Implementation `de04a57`, reviewed tip `c4aafb9`,
was merged as `7d37a3066a561a38867352b3fab2e08387940ba6` and published on
`origin/main`. The full baseline through task 004 (`2c39117`) remains in ancestry.

## Workspace and read set

Use the original `C:/Users/shpy2/Documents/OneBrain` workspace, now on `main`.
The completed `codex/obp-api-001-network-api` branch is retained. Inspect Git
status and preserve any subsequent changes; do not discard retained work or
substitute another worktree.

1. Repository `AGENTS.md` and applicable subtree rules.
2. [PROGRESS.md](PROGRESS.md), [DECISIONS.md](DECISIONS.md), especially D-024..D-030.
3. [Accepted OBP profile](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md).
4. [Accepted local API transport](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md),
   [transport review](outputs/OBP_API_001_TRANSPORT_REVIEW.md) and
   [implementation evidence and limits](outputs/OBP_API_001_IMPLEMENTATION.md).
5. [OBP-CLI-001](tasks/15-OBP-CLI-001.md) and its required read set, including
   the CLI profile and current transport/auth/output/error conventions.

## Next task and boundaries

OBP-API-001 is Merged under D-030. D-029 transport approval is satisfied;
do not request that approval again. Six routes map 13 unchanged operations.
The shared node façade, durable command/reconciliation journal, host capabilities,
input references, source policy, snapshot paging and thin REST/WS handlers exist.

Fresh pre-merge checks passed: node 252 unit + 22 integration; API 44 unit
(one existing real-Ollama opt-in ignored) + eight Base integration; feature-off
16; ku-net 315; Python 19 and aggregate vNext validation. These are local
integration results, not Internet/NAT or cross-platform qualification.

The current pointer is OBP-CLI-001, Planned, using declared branch
`codex/obp-cli-001-networking` from updated main when implementation is requested.
OBP-WEB-001 is also dependency-ready and remains Planned. Neither task started
in the merge closure. CLI must use authenticated local REST and the node-owned
service, preserving capability, reconciliation and expected-peer boundaries.
Register exact CLI syntax before implementation; do not invent operations beyond
the 13 accepted mappings. Missing host intake/capability transport must be handled
against the accepted contract, never bypassed by raw addresses or REST uploads.

Keep semantic tuning deferred under D-024. Preserve the running
`C:/Users/shpy2/Documents/OneBrainLocal` host, Registry, jobs, keys and Ollama.
Do not enable networking, change models, activate v2 or claim qualification.
Mobile and default rollout remain separate scopes.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
OBP-API-001 đã merge tại 7d37a30 và push origin/main theo D-030.
Transport D-029 đã được chấp thuận; implementation de04a57 được giữ nguyên.
Đọc PROGRESS, task 15-OBP-CLI-001, API profile và implementation evidence;
kiểm tra git status, giữ mọi thay đổi rồi triển khai OBP-CLI-001 đúng scope.
Không hỏi lại transport approval, không tuning model, không tự bật networking
hay thay đổi live host. CLI phải dùng shared node-owned service qua local API.
```
