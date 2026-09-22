# Next conversation — OBP Web review

OBP-WEB-001 is locally implemented in `Review` on
`codex/obp-web-001-networking`, original workspace
`C:/Users/shpy2/Documents/OneBrain`, starting main `2d4d5f4`.
Changes are retained uncommitted; no push/merge occurred in this continuation.
Inspect Git status and preserve every change before further work.

CLI `e0dcc7d` is merged at `04bcb30` and published under D-031. API `de04a57`
remains unchanged in merge `7d37a30`. D-029 transport approval remains satisfied;
do not request it again. Retain both earlier task branches.

## Read set

1. Repository AGENTS.md and applicable subtree rules.
2. [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), D-024..D-031,
   [CAPABILITY_STATUS](CAPABILITY_STATUS.md) and [README](README.md).
3. [Task 16](tasks/16-OBP-WEB-001.md) and registered
   [Web interactions](../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md).
4. Accepted [product](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md),
   [API/private-WS](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md),
   [private WS](../../specs/vnext/VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1.md) and
   [Web UX](../../specs/vnext/VNEXT_DESKTOP_WEB_UX_PROFILE_V1.md) contracts.
5. [Web evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md),
   [API evidence](outputs/OBP_API_001_IMPLEMENTATION.md),
   [CLI projection](../../specs/vnext/OBP_LOCAL_CLI_PROJECTION_V1.md) and
   [CLI evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md).

## Current result and boundaries

The Network page uses the shared node-owned service through authenticated REST
and scoped aggregate private WS. All 13 accepted operations plus metadata
reconcile are covered. Unknown responses retain the original key/context in
this tab's sessionStorage; reload and WS gaps never replay mutations. Dataset,
process, generation, expected-peer, checkpoint and pagination fences are retained.

Host intake/control/management grants remain explicit prerequisites. Import is
host-issued input_ref only; no REST upload, raw address fallback, capability
minting, peer directory, outbox list, source delete or intent cancellation exists.
Source disable preserves replay floors. Advertising requires the host's own
public-fields/expiry preview and separate opt-in.

Checks passed: 82 Web tests (60 OBP/22 KU), two receipt tests, production build,
lint with eight existing unrelated warnings, 25 Python tests and vNext validator.
Isolated Chromium checks at 1280px/390px passed layout and axe including contrast.
The temporary QA server was stopped. Browser fixtures do not claim a fresh
real-node/browser integration, multi-host/NAT or platform qualification run.

Next action is owner review of this local Web implementation. Do not start
OBP-DESK-001 or merge/publish without the applicable owner direction. Preserve
OneBrainLocal, Registry, jobs, keys and Ollama. D-024 model tuning remains Deferred.
No networking activation, live-host change, v2 host activation, mobile, default
rollout or qualification claim is included.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
OBP-WEB-001 đang Review local trên codex/obp-web-001-networking; mọi thay đổi
chưa commit phải được giữ lại. Đọc PROGRESS, task 16, Web projection và Web/API/CLI
evidence rồi review implementation và các ranh giới recovery/private WS.
D-029 đã chấp thuận, không hỏi lại transport approval. Không tuning model,
không bật networking hoặc thay đổi live host. Chưa merge hoặc bắt đầu Desktop.
```
