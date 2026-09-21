# Next conversation — OBP Web workflow

OBP-CLI-001 is Merged under D-031: implementation `e0dcc7d`, merge
`04bcb30e42fa2594d0bf11e146fa0e44df0dcf82`. Main and the retained CLI branch
are published. API `de04a57` remains unchanged in merge `7d37a30`.
D-029 transport approval is satisfied; do not request it again.

## Workspace and read set

Use the original `C:/Users/shpy2/Documents/OneBrain` on main. Inspect Git status
and preserve any subsequent changes. Retain the API/CLI branches.

1. Repository AGENTS.md and applicable subtree rules.
2. [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), D-024..D-031,
   [CAPABILITY_STATUS](CAPABILITY_STATUS.md) and [README](README.md).
3. [Task 16 — OBP-WEB-001](tasks/16-OBP-WEB-001.md), including its required
   Web design tokens, components, patterns and current network UI references.
4. Accepted [OBP product](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md)
   and [local API](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md) contracts,
   including the private WS security/backpressure projection.
5. [API evidence](outputs/OBP_API_001_IMPLEMENTATION.md),
   [CLI projection](../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md) and
   [CLI evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md).

## Next task and boundaries

OBP-WEB-001 remains Planned, dependency-ready. Use its declared branch
`codex/obp-web-001-networking` from updated main when implementation is requested.
The current closure implemented no Web UI.

Web must project the shared node-owned service through authenticated REST and
the accepted private aggregate WS stream. Register exact interactions first.
Preserve capability, session/dataset/generation, pagination, expected-peer and
reconcile-before-retry boundaries. A WS gap triggers read refresh, never mutation
replay or inferred completion.

There are 13 accepted operations, with no source-delete, intent-cancel,
peer-directory or outbox-list endpoint. Source disable preserves replay floors.
Host intake and scoped management grants remain in-process prerequisites; no
REST upload, capability minting or raw-address fallback. Missing host bindings
must be explicit in the product, never replaced by UI-generated authority.

Fresh closure checks passed: CLI 51 unit + two integration, Python 25 and vNext
validator. Prior default CLI 42 + two and no-default check also pass. These are
local integration results, not Internet/NAT or platform qualification.

Keep D-024 tuning deferred. Preserve OneBrainLocal, Registry, jobs, keys and
Ollama. No networking activation, model changes, v2 host activation, mobile,
default rollout or qualification claims are included.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
OBP-CLI-001 đã merge tại 04bcb30 và push theo D-031; API de04a57 giữ nguyên
trong merge 7d37a30. D-029 đã chấp thuận, không hỏi lại transport approval.
Đọc PROGRESS, task 16-OBP-WEB-001, API/private-WS profile, design rules và
implementation evidence; kiểm tra git status, giữ mọi thay đổi rồi triển khai
OBP-WEB-001 đúng scope. Không tuning model, không tự bật networking hay thay đổi
live host. Web phải dùng shared node-owned service qua local API/private WS.
```
