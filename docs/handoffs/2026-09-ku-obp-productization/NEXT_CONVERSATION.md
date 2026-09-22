# Next conversation — OBP Desktop integration

OBP-WEB-001 is accepted and merged under D-032: implementation `cade635`,
merge `7f49eeb`. Main and the retained Web branch are published. CLI merge
`04bcb30` and API merge `7d37a30` remain unchanged in ancestry. D-029 transport
approval is satisfied; do not ask again.

Use original `C:/Users/shpy2/Documents/OneBrain` on main. Inspect Git status
and preserve any subsequent changes. Retain Web/CLI/API branches.

## Read set

1. AGENTS.md and applicable subtree rules.
2. [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), D-024..D-032 and [README](README.md).
3. [Task 17 — OBP-DESK-001](tasks/17-OBP-DESK-001.md), including Desktop shell,
   supervision, local authentication, packaging and lifecycle/budget references.
4. [Web projection](../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md),
   [Web evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md),
   [API/private WS](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md) and
   [product contract](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md).
5. [API evidence](outputs/OBP_API_001_IMPLEMENTATION.md),
   [CLI evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md), and applicable runtime
   ownership, lifecycle, concurrency and resource-budget profiles named by task 17.

## Next task and boundaries

OBP-DESK-001 is Planned, dependency-ready. When implementation is requested,
use `codex/obp-desk-001-networking` from updated main. This closure implements
no Desktop changes. Reuse one node-owned service; no second identity/runtime.
Preserve disabled/kill generation, durable intent, local credential handoff,
sleep/resume and clean-exit semantics. Exact interactions must be registered
before adding any new product behavior.

Web supports the 13 accepted operations and metadata reconcile. Host intake,
control and management remain explicit in-process prerequisites. No upload,
capability minting, raw-address fallback, peer-directory, outbox-list,
source-delete or intent-cancel endpoint exists. Unknown outcomes retain the
original key/context; WS gaps only refresh reads. Source disable retains replay
floors. Advertising remains a separate explicit opt-in with host preview.

Fresh closure checks: 82 Web tests, build, 25 Python tests and vNext validator.
Prior receipt, lint and isolated Chromium/axe evidence retains its stated scope;
no new real-node/browser, multi-host/NAT or platform qualification is claimed.

Preserve OneBrainLocal, Registry, jobs, keys and Ollama. Keep D-024 tuning
Deferred. No networking activation, live-host change, mobile, v2 host activation,
default rollout or qualification claim is included.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
OBP-WEB-001 đã merge tại 7f49eeb và push theo D-032; API/CLI giữ nguyên.
D-029 đã chấp thuận, không hỏi lại transport approval. Đọc PROGRESS, task 17,
Web/API/private-WS và lifecycle contracts/evidence; kiểm tra git status, giữ mọi
thay đổi rồi triển khai OBP-DESK-001 đúng scope. Không tuning model, không tự bật
networking hay thay đổi live host. Desktop phải dùng một shared node-owned service.
```
