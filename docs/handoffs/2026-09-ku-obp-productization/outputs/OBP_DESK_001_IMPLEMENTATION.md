# OBP-DESK-001 — Desktop integration evidence

2026-09-22. Original workspace `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-desk-001-networking`, clean starting main `636c113`. Web merge
`7f49eeb`, CLI merge `04bcb30` and API merge `7d37a30` remain ancestors.
D-029 is satisfied. Merged under D-034: implementation `410a6a0`, merge
`7e4fc14`. Earlier local/uncommitted descriptions below are historical.

## Implementation

[Exact Desktop interactions](../../../specs/vnext/OBP_LOCAL_DESKTOP_PROJECTION_V1.md)
were registered before implementation. The shell supervises one supplied
OneBrainNode, the unchanged ApiServer router and one legacy event bridge.
`run_with_host` is an in-process host assembly port; custody, policy, discovery,
management and explicit outbound execution remain host responsibilities.
The stock host does not fabricate them. Compiling the additive outbound-first
feature forwards both shared crates but starts no OBP runtime by itself.

The loopback listener binds before readiness/credential publication. Occupied
ports fail closed, with no fallback to an unrelated host. The per-process
256-bit token stays in memory and is handed only to the local main WebView.
Frontend startup waits for readiness; private calls recheck IPC readiness.
Neither stored browser credentials nor an empty-token fallback is used.
Packaged navigation is restricted to local origins, with a separate debug origin;
Desktop CORS admits the packaged origins without modifying the API crate.
The embedded `/network` retains all 13 accepted operations and private WS.

Windows power and interface callbacks synchronously fence Desktop admission and
withdraw the execution grant. One shutdown task drains the node and owned tasks.
Resume/network change requires the explicit tray Restart action, reconstructing
the process and host dependencies. No automatic replay, re-enable or catch-up
is added. IPC quit, tray quit, restart and application ExitRequested all use the
same exit path. HTTP admission stops first; node shutdown drains admitted work;
cancellation reaches accepted HTTP and upgraded WS sockets, and tasks are joined.
Durable stores, identity and generation ownership remain in the node.

The tray Network action opens the canonical page and derives finite scoped
tooltip text from the same node service status used by the API. Lifecycle changes
clear stale Web details, previews and in-memory management credentials while
retaining the original pending command. Status does not infer global delivery.

Desktop recovery persists one bounded, closed-schema original command record
beside Desktop config, before dispatch, using a synced temporary file and
no-clobber publication. It stores no credentials, rejects overwrite of an
unresolved record, and fails closed on corruption/storage errors. Restart loads
recovery only; existing dataset/origin/key checks and metadata reconciliation
remain authoritative. Browser sessionStorage behavior is unchanged.

Packaging retains one programmatic tray and the same built Web assets. Tauri
build hooks run from `src`, verified by the CLI, so their original `onebrain-web`
prefix is retained. The CSP includes IPC and loopback HTTP/WS. No installer or
application was launched against the owner configuration.

## Verification

All runtime fixtures use temporary datasets, deterministic test custody and
loopback listeners. No OneBrainLocal, Registry, jobs, production keys or Ollama
was accessed or changed.

```powershell
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-desktop --features vnext-outbound-first --test lifecycle -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-desktop --no-default-features --test lifecycle -q
cargo check --offline --locked --manifest-path src/Cargo.toml -p onebrain-desktop --features vnext-network-runtime -q
cargo fmt --manifest-path src/Cargo.toml -p onebrain-desktop -- --check
npm run test:ku --prefix src/onebrain-web
npm run test:vnext --prefix src/onebrain-web
npm run build --prefix src/onebrain-web
npm run lint --prefix src/onebrain-web
python -m unittest scripts.ci.test_obp_desktop_packaging scripts.ci.test_validate_obp_local_api scripts.ci.test_validate_obp_product_contract scripts.ci.test_validate_vnext_cli_profile scripts.ci.test_validate_vnext_desktop_web_ux_profile
python scripts/ci/validate_vnext_contracts.py
git diff --check
```

Tauri asset build from `src/onebrain-desktop`, with `CARGO_NET_OFFLINE=true`:
`cargo tauri build --debug --no-bundle --features vnext-outbound-first`.

- OBP Desktop: six integration tests pass. They cover default zero peer listener,
  authenticated readiness, packaged-origin CORS/no-store, bind failure, permanent
  execution withdrawal, startup fencing, concurrent shutdown and auxiliary joining,
  bounded recovery, corrupt/secret/overflow rejection and no-clobber behavior.
- The real shared-service test checks API/service equality while holding the node
  mutex, explicit kill, actual private WS closure, stopped weak handles, restart
  with unchanged dataset/generation and a fresh process fence, metadata-only
  reconciliation and preserved pending intent bytes/peer/retry counters.
- Web: 89 tests pass, including seven Desktop IPC/recovery cases and all 82 prior
  KU/OBP cases. Two receipt tests, production build and lint pass; lint retains
  eight unrelated existing warnings.
- Python: 35 tests and aggregate vNext validator pass. Source evidence pointers
  were updated for the centralized exit path; frozen semantic profiles are unchanged.
- Native Windows GNU Tauri debug asset build passes, producing
  `src/target/debug/onebrain-desktop.exe`. It was not launched or installed.
- Feature-off Desktop: four integration tests pass. The existing
  `vnext-network-runtime` feature check and Desktop formatting pass. Test-only
  imported modules and feature-reduced dependencies retain unused-code warnings.

Development corrections: Rust compilation caught a native handle cast and Tauri
plugin generic/navigation API mismatch. The first build-hook edit used the wrong
working directory and was reverted after a real CLI check. The first pending-intent
assertion assumed no initial startup drain; the test now captures its actual
counter before restart and proves that a durably killed restart does not change
it. Final checks above supersede those failed development attempts.

## Limits

This is Windows compilation and isolated integration evidence, not live Windows
sleep/network-switch, native WebView end-to-end, signed installer, macOS/Linux,
Internet/NAT or release qualification. The native event APIs are wired, but the
owner machine was not suspended or reconfigured for a test. Without native lifecycle
hooks, the stock host remains local-only and disables peer networking; custom
network hosts fail closed before startup. No cross-platform networking readiness
is claimed.

The packaged default reports OBP unavailable until a trusted host supplies its
existing in-process dependencies/bindings. No GUI for custody provisioning,
capability minting, source upload or advertisement preview is invented. A configured
fixed API port preserves the recovery origin; changing it or the dataset does
not silently authorize reuse of an old command.

The recovery record is private local metadata, protected by the current user's
configuration-directory permissions; it is not an encrypted node outbox or a
multi-user isolation boundary. It contains no token/key material. Abrupt OS power
loss is not claimed as a tested filesystem durability qualification.

API/node/CLI implementations are unchanged. D-024 model tuning stays Deferred;
mobile, v2 host activation, networking opt-in, default rollout, branch publication
and merge are outside this local implementation checkpoint.

## Owner review acceptance — D-033

The owner stated “tôi đồng ý duyệt review”. The implementation and evidence above
are accepted with their stated limits. No code changed during this acceptance
record. Work remains local/uncommitted, with no publication or merge. The task
stays Review pending explicit Git closure direction; OBP-QA-001 is not started.

## Merge closure — D-034

The owner instructed “merge và push nhé” after D-033 acceptance. Fresh verification
passed: six Desktop integration tests, 89 Web tests, 35 Python tests and aggregate
vNext validator. Implementation `410a6a0` merged without conflict as
`7e4fc14e9ba4082e425d474ef9f7da26129106b0`. Main and the retained Desktop branch
are published by this closure. API/node/CLI content is unchanged. Next pointer:
OBP-QA-001, Planned. No QA execution, live-host change, activation or rollout.
