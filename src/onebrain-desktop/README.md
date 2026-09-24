# onebrain-desktop

> OneBrain Desktop — native desktop application.

| Field       | Value                                     |
|-------------|-------------------------------------------|
| **Tech**    | Tauri + React / onebrain-node embedded    |
| **Phase**   | Phase 8                                   |
| **Status**  | OBP Desktop integration, local review      |

## Overview

Native desktop application for Windows, macOS, and Linux. Embeds
`onebrain-node` directly via Tauri's Rust backend for maximum
performance and offline capability.

## Spec

See [OneBrain Architecture Spec](../../docs/ARCHITECTURE.md) for details.

## OBP-DESK-001

The shell embeds the accepted `/network` Web workflow with one supervised
`OneBrainNode` and the unchanged shared API router. Build from the repository:

```powershell
npm run build --prefix src/onebrain-web
cargo build --offline --locked --manifest-path src/Cargo.toml -p onebrain-desktop --features vnext-outbound-first
```

For Tauri asset packaging, run `cargo tauri build --debug --no-bundle --features
vnext-outbound-first` from `src/onebrain-desktop`. This builds a local executable;
it neither runs that executable nor installs or modifies the live host.

The feature is opt-in and does not enable networking. Default `auto_start=false`;
existing explicit legacy auto-start configuration remains separate from OBP.
The stock host supplies no OBP custody/policy/control/intake ports, so the page
reports unavailable. A trusted embedding host uses `run_with_host` to return one
configured `HostNode`, installs the existing node dependencies and supplies the
API binding in-process. For outbound-first it must explicitly call
`Supervisor::grant_execution()` and pass `Supervisor::execution()` to the existing
outbound-first dependency constructor. It must never create a second runtime or
reuse stopped dependencies. Source intake, management grants and advertisement
preview remain the host's existing responsibilities, with no new IPC minting API.

Credentials are per-process and memory-only; a local main WebView receives them
after bind. Bind failure has no fallback port. The same fixed configured port is
recommended across restarts because recovery binds the original API origin.
No secrets are written into URLs, browser storage or recovery files.

Windows power/interface callbacks withdraw execution and stop the owned service.
The tray Restart action reconstructs the process explicitly. Sleep/resume never
re-enables a killed generation or replays a command. Close hides to tray; Quit
and Restart await shutdown. The tray Network action opens the canonical page and
sets an aggregate, scoped tooltip from the shared service snapshot.

One bounded pending command record is stored as `obp-pending-v1.json` beside the
Desktop config, before dispatch. Unknown outcomes survive process restart and
require the accepted metadata reconciliation flow; do not delete this file to
retry unknown work. Corruption/storage failure blocks commands. Browser recovery
continues to use sessionStorage.

Native lifecycle adapters currently cover Windows. Without an adapter, the stock
host remains local-only with peer networking disabled; a custom network host fails
closed at startup. macOS/Linux networking support and real OS sleep/network
qualification remain outstanding. No installer signing, public NAT test or default
rollout is claimed. See the [registered projection](../../docs/specs/vnext/OBP_LOCAL_DESKTOP_PROJECTION_V1.md)
and [implementation evidence](../../docs/handoffs/2026-09-ku-obp-productization/outputs/OBP_DESK_001_IMPLEMENTATION.md).

## KU-DESK-001

The packaged `/ku` page uses the same in-process node, authenticated local API
and node-owned KU service as the accepted Web workflow. KU custody is an optional
operator configuration in the Desktop `config.toml`; the WebView cannot submit
Registry roots, source paths, Vault keys or model installation grants. Add the
following to an existing Desktop config after provisioning real host inputs:

```toml
[ku_host]
registry_root = "C:/OneBrainLocal/registry"
registry_public_key = "<trusted Registry public key: 64 lowercase hex characters>"
vault_key_file = "C:/OneBrainLocal/secrets/vault.key"

[[ku_host.sources]]
label = "My admitted source"
canonical_file = "C:/OneBrainLocal/custody/source.canonical"
```

The Registry must be a verified signed release. Each admitted source is an
existing canonical private Text SourceArtifact, not raw text; the Vault key is
exactly 32 binary bytes. Keep the same data directory, key and admitted source
files across restart. The Desktop generates a fresh in-memory API token for
each process. The manual editor is available without a model. To admit an
experimental installed Ollama model, add `[ku_host.ollama]` with `executable`,
`models_dir`, `models = ["qwen3:8b"]` and `memory_limit_bytes`; this does not
qualify model output or enable AI by default.

Invalid KU inputs show a bounded dependency code in the Desktop lifecycle
status while the legacy local API remains usable. A malformed/unreadable
Desktop config stops startup with a visible code, avoiding a silent switch to
the default data directory. Quit/restart fences API admission, stops the
node-owned network, closes Base and waits for in-flight KU work, then releases
the listener. Prepared KU operations
remain durable; after restart, recover with the original operation ID and
explicitly reconcile before saving. The Web page keeps unsaved IDs in memory
only, so record an operation ID before leaving it. No KU source or key is
forwarded through the native event bridge.

Build the Web assets and Desktop crate from the repository root:

```powershell
npm run build --prefix src/onebrain-web
cargo build --locked --manifest-path src/Cargo.toml -p onebrain-desktop
```

No installer, live native WebView run, macOS/Linux lifecycle qualification,
model qualification or default OBP networking is claimed. See the
[implementation evidence](../../docs/handoffs/2026-09-ku-obp-productization/outputs/KU_DESK_001_IMPLEMENTATION.md).
