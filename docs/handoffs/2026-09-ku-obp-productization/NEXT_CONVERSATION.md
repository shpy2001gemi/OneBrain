# Next conversation — Accepted OBP Desktop, Git closure pending

OBP-DESK-001 review is owner-accepted under D-033 and remains locally in Review on
`codex/obp-desk-001-networking`, original `C:/Users/shpy2/Documents/OneBrain`.
Starting main was `636c113`; all changes remain uncommitted. Inspect Git status
and preserve every change and retained branch. No push or merge has occurred.

Web merge `7f49eeb` (D-032), CLI `04bcb30` and API `7d37a30` remain ancestors.
D-029 transport approval is satisfied; do not ask again. API/node/CLI code is unchanged.

## Read set

1. AGENTS.md, [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), [README](README.md).
2. [Task 17](tasks/17-OBP-DESK-001.md),
   [Desktop projection](../../specs/vnext/OBP_LOCAL_DESKTOP_PROJECTION_V1.md),
   [Desktop evidence](outputs/OBP_DESK_001_IMPLEMENTATION.md), and
   [Desktop build/host notes](../../../src/onebrain-desktop/README.md).
3. [Web projection](../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md),
   [Web evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md),
   [API/private WS](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md),
   [product contract](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md),
   [API evidence](outputs/OBP_API_001_IMPLEMENTATION.md),
   [CLI evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md).
4. Runtime ownership, lifecycle, concurrency, budgets and rollback contracts.

## Current result and next action

Owner review is accepted. Git closure awaits explicit owner direction; do not
request another approval of the same implementation or transport. One supervisor owns the
supplied node, shared API router and event bridge. Credential handoff is local,
memory-only and gated on listener readiness. Windows suspend/resume/interface
changes withdraw execution, stop networking and require explicit Restart.
The existing shared node stores retain identity, disabled generations and pending
intent. Desktop persists one bounded credential-free command recovery record;
restart only restores reconciliation, never replay. Tray status remains scoped.

Tests pass: six OBP Desktop integration tests, four feature-off tests, 89 Web
tests, two receipt tests, 35 Python tests, Web build/lint and vNext validator.
Windows Tauri debug asset build passes. See evidence for feature/build details.
Native OS event APIs are wired; real sleep/network-switch, native WebView E2E,
installer signing, macOS/Linux networking and NAT qualification are not claimed.
The stock host remains local-only without native hooks; custom network hosts
fail closed. OBP provisioning remains an explicit in-process host prerequisite.

OBP-QA-001 stays Planned until the Desktop merge prerequisite is met. Do not
start another task or publish/merge these local changes without owner direction.
Preserve OneBrainLocal, Registry, jobs, keys, Ollama and deferred D-024 tuning.
No networking activation, mobile, v2 host activation or default rollout is included.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
OBP-DESK-001 đang Review local trên codex/obp-desk-001-networking; giữ toàn bộ
thay đổi chưa commit. Đọc PROGRESS, task 17, Desktop projection/evidence và
Web/API/private-WS/lifecycle contracts. Review đã được duyệt theo D-033;
không yêu cầu duyệt lại. Git closure chờ chỉ thị rõ ràng.
D-029 đã duyệt, không hỏi lại. Không tuning model, bật networking hay đổi live host.
Desktop phải dùng một shared node-owned service. Chưa merge hoặc bắt đầu OBP-QA-001.
```
