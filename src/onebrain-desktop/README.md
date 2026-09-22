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
