# Next conversation — OBP product acceptance

OBP-DESK-001 is accepted and merged under D-034: implementation `410a6a0`,
merge `7e4fc14`. Main and the retained Desktop branch are published in this closure.
Web `7f49eeb`, CLI `04bcb30` and API `7d37a30` remain ancestors. D-029 transport
approval remains satisfied; do not ask again. API/node/CLI code is unchanged.

Use original `C:/Users/shpy2/Documents/OneBrain` on main. Inspect Git status and
preserve subsequent changes and all retained branches.

## Read set

1. AGENTS.md, [PROGRESS](PROGRESS.md), [DECISIONS](DECISIONS.md), [README](README.md).
2. [Task 18 — OBP-QA-001](tasks/18-OBP-QA-001.md) and
   [capability status](CAPABILITY_STATUS.md).
3. [Desktop projection](../../specs/vnext/OBP_LOCAL_DESKTOP_PROJECTION_V1.md),
   [Desktop evidence](outputs/OBP_DESK_001_IMPLEMENTATION.md),
   [Desktop host/build notes](../../../src/onebrain-desktop/README.md).
4. [Web projection](../../specs/vnext/OBP_LOCAL_WEB_PROJECTION_V1.md),
   [Web evidence](outputs/OBP_WEB_001_IMPLEMENTATION.md),
   [API/private WS](../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md),
   [product contract](../../specs/vnext/OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md),
   [API evidence](outputs/OBP_API_001_IMPLEMENTATION.md),
   [CLI evidence](outputs/OBP_CLI_001_IMPLEMENTATION.md).
5. Applicable runtime ownership/lifecycle/concurrency/budget/rollback contracts,
   outbound-first acceptance and P5/production evidence rules named by task 18.

## Next task and boundaries

OBP-QA-001 is Planned, dependency-ready. When requested, use
`codex/obp-qa-001-nat-canary` from updated main. This closure starts no QA work.
Single-machine or loopback simulation is preflight, never independent-network
production evidence. Follow the task's exact host/network/provider requirements;
report missing evidence without inventing qualification.

Desktop supervises one node-owned service with memory-only local credentials.
Windows lifecycle events fence execution; explicit Restart reconstructs the
process. Durable identity, disabled generations and pending intent remain with
the node. Unknown commands retain their original recovery context; restart and
WS hints never replay mutations. Host provisioning stays explicitly in-process.

Fresh closure checks: six Desktop integration tests, 89 Web tests, 35 Python tests
and vNext validator. Earlier feature-off, receipt, Web build/lint and Windows Tauri
debug asset evidence retains its scope. Native OS hooks are wired; real sleep/
network-switch, native WebView E2E, signed installers, other-OS networking and NAT
qualification remain unclaimed. Read the Desktop evidence limits before QA.

Preserve OneBrainLocal, Registry, jobs, keys and Ollama. Keep D-024 tuning Deferred.
Do not activate networking or alter a live host merely to advance this handoff.
Mobile, v2 host activation, default rollout and legacy removal are outside scope.

## Copy into a new conversation

```text
Hãy đọc AGENTS.md và docs/handoffs/2026-09-ku-obp-productization/NEXT_CONVERSATION.md
trên working tree gốc C:/Users/shpy2/Documents/OneBrain.
OBP-DESK-001 đã merge tại 7e4fc14 và push theo D-034; API/node/CLI giữ nguyên.
Đọc PROGRESS, task 18, Desktop/Web/API/private-WS và các acceptance/P5 contracts,
kiểm tra git status, giữ mọi thay đổi rồi thực hiện OBP-QA-001 đúng scope.
Phân biệt preflight local với evidence hai host/network độc lập; không tự nhận
qualification. D-029 đã duyệt, không hỏi lại. Không tuning model, tự bật networking
hay thay đổi live host. Mọi surface dùng chung node-owned service.
```
