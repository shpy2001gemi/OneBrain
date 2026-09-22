# Local OBP Desktop projection v1

OBP-DESK-001 adapts the accepted Web/API contracts. No new OBP operation,
transport, grant issuer, signer or runtime is introduced.

* One supervisor owns one supplied OneBrainNode, its loopback API listener and
  the legacy event bridge. Host dependencies are supplied in-process before
  startup; the packaged default supplies none and starts no peer networking.
  An explicitly configured legacy auto_start remains separate from OBP opt-in.
* get_api_config is restricted to the local main WebView. It returns credentials
  only after successful listener bind, and fails closed during shutdown/failure.
  No fallback port, stored browser credential or remote navigation is accepted.
  Tokens are fresh per process, memory-only and never logged or placed in URLs.
  The embedded surface uses the six unchanged REST/private-WS routes, including
  the existing host management/input prerequisites and exact confirmations.
* Quit, tray quit, exit and restart fence API admission and host execution,
  await node shutdown, then join owned auxiliary tasks. Restart rebuilds the
  process; no stopped dependencies or ephemeral sessions are reused.
* Native Windows suspend/resume and interface changes fence host execution and
  stop networking. Resume/change leaves a visible restart-required state; only
  the existing explicit Restart action reconstructs dependencies. No automatic
  replay, re-enable, catch-up, discovery or model work occurs. Other platforms
  remain explicitly unqualified until their native event adapters are supplied.
  Missing native hooks permit only the stock local-only host with peer networking
  disabled; custom network-host startup fails closed.
  Durable identity, disabled generations, replay floors and intents remain under
  their existing node owners. Lifecycle events never delete or rewrite them.
* Tray “Network status” explicitly reads the canonical OBP status through the
  shared local API, displaying only unavailable/disabled/active/degraded and
  local/partial scope. No background polling, IDs, addresses or success counts
  are promoted to global reachability/delivery.
* Desktop pending command recovery stores one bounded validated original Web
  recovery record in the Desktop configuration directory before dispatch. IPC
  load/save/clear never dispatches commands or stores credentials. Existing
  unresolved records cannot be overwritten with another command. Reload/restart
  restores reconciliation only; the original origin/dataset/key remains binding.
  Storage failure blocks dispatch. Clearing follows explicit acknowledgement of
  a resolved outcome in the accepted Web workflow. Browser sessionStorage is
  unchanged. This journal is a client recovery aid, not a second outbox owner.

Packaging uses the same built Web assets, explicit opt-in Cargo feature forwarding,
local-only main-window capabilities and CSP. Development origins are distinct
from packaged origins. Tests use temporary stores/listeners and never the live host.
