# KU-DESK-001 — Desktop KU integration evidence

2026-09-24. Review branch: `codex/ku-desk-001-workflow`, based on clean
`main == origin/main` at `073e141790266df13500bc4b42f104655fc1098b`.
Task 08 is the only active work package. This is a local Windows build and
service-lifecycle review, not owner acceptance, merge or release.

## Integration and authority

- The packaged Desktop uses the accepted Web `/ku` page and the same
  `ApiServer::with_shared_node` router as the Web host. There is one
  `OneBrainNode` and one Base KU runtime; the WebView calls the existing
  authenticated `/api/vnext/ku/*` routes. No KU semantics or editor operation
  was implemented in Tauri IPC.
- `onebrain-api::ku_host` now prepares the same operator-controlled KU custody
  inputs for the opt-in Web example and Desktop. Desktop takes an optional
  `[ku_host]` section in its TOML config: signed Registry root/trust key,
  canonical private Text SourceArtifact files, stable 32-byte Vault key file,
  and optional installed Ollama artifacts. Host files never enter the Web
  bridge. The process API token is generated in memory before Base admission
  and bound to that same Base authorizer and listener; only the local main
  WebView receives the normal API handoff. Custom OBP host embedding keeps its
  existing two-argument port, with a separate token-aware port available for
  trusted KU hosts.
- KU host input errors are static bounded codes. Missing/invalid Vault key
  leaves KU unavailable while legacy local API reads remain available.
  Missing Registry or source admission with a valid key installs a read-only
  KU owner, preserving exact saved local get/list; editor and new preparation
  remain unavailable. A failed optional experimental model is reported while
  manual KU remains usable. Malformed/unreadable Desktop config stops startup
  with a visible code instead of silently using another data directory.
- The Desktop supervisor fences new API admission, stops the node-owned
  network, closes and drains Base in-flight KU work, then releases the
  listener. Quit and
  restart both use this path. Prepared encrypted work and its original
  operation ID survive a process restart; the Web page requires explicit
  reconciliation and never reruns extraction on its own. The stock host still
  installs no OBP networking grants and does not turn networking on by default.
- The Tauri event bridge no longer forwards legacy KU source text. It emits
  no KU private key, source file path, Registry trust input or management
  capability. Host-issued capabilities remain behind the existing authenticated
  service boundary; the API Bearer token remains a per-process WebView handoff.

## Focused evidence

| Check | Result / boundary |
|---|---|
| Desktop KU lifecycle test | Actual signed test Registry and canonical private Text source through the shared API. Wrong token and a Web-supplied `vault_key`/`authorized` field are rejected without echo. Manual draft prepares, shutdown closes Base, restart changes process generation, original operation reconciles and previews, then saves privately and exact get succeeds. A later restart without Registry still reads the saved KU; editor refuses work. |
| Desktop KU startup failure test | Missing Vault key produces a bounded host issue. Legacy `/api/status` remains readable, while KU status returns a typed dependency failure without echoing the path. |
| Existing Desktop lifecycle | Listener bind failure has no fallback; fence is idempotent; auxiliary tasks join; feature-enabled OBP durable pending and killed-generation restart checks remain intact. |
| Existing API/Web checks | API tests cover typed errors, manual editor, private save, review-job drain/restart and model-unqualified boundaries. Web KU tests cover page and transport behavior; Web production build packages `/ku`. |

## Verification commands

Run from `src` unless noted. Final result counts are recorded in
[PROGRESS](../PROGRESS.md). Warnings about the duplicate Web example target,
pre-existing `ku-net` unused methods, and test-only unused module methods do
not affect these gates. Workspace-wide `cargo fmt --all -- --check` reports
pre-existing formatting differences outside task files; touched Rust files
were formatted directly.

| Command | Result |
|---|---|
| `cargo check --locked -p onebrain-desktop` | Pass, Windows default build graph. |
| `cargo check --locked -p onebrain-desktop --features vnext-outbound-first` | Pass, opt-in feature graph. |
| `cargo build --locked -p onebrain-desktop --features vnext-outbound-first` | Pass, Windows Desktop executable build. |
| `cargo test --locked -p onebrain-desktop --test lifecycle -- --test-threads=2` | 6 passed. |
| `cargo test --locked -p onebrain-desktop --test lifecycle --features vnext-outbound-first -- --test-threads=2` | 8 passed. |
| `cargo check --locked -p onebrain-api --example ku_local_web` | Pass after shared host provisioning refactor. |
| `cargo test --locked -p onebrain-api --lib -- --test-threads=2` | 37 passed; one explicit real-model opt-in test ignored. |
| `npm run build` and `npm run test:ku` in `src/onebrain-web` | Build passes; 89 tests in 7 files pass. |
| `python scripts/ci/validate_vnext_contracts.py`; `git diff --check` | Pass. The validator and normative evidence map now point at the composed Base/network shutdown path. |

## Limits retained for review

No real operator Registry/source/key installation, live Tauri WebView run,
installer signing, macOS/Linux lifecycle qualification, real OS sleep test or
real-model quality result was performed. `model_qualified` remains false.
`KU-ENC-003` remains Blocked; this evidence does not claim `KU-QA-001` or
`INT-KU-OBP-001`. OBP-MIG-001 remains merged with legacy rollback/data intact.
OBP-QA-001 remains functionally accepted with
`consumer_nat_qualified=false`; Linux three-host P5 qualifies only candidate
`c453f3e`. Old remote staging, runs and durable state were untouched; OBP
networking remains default-off. D-010 keeps this branch for owner review and
requires explicit direction before merge.
