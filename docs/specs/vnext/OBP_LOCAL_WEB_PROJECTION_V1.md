# Local OBP Web projection v1

OBP-WEB-001 registers the following interactions before implementation. This
adapts D-029; no transport, authority, host provisioning or protocol is added.

The existing `/network` page uses only authenticated local OBP REST and the
separate `OBP_PRIVATE_WEBSOCKET_PROFILE_V1` stream. Reuse `index.css` Web tokens,
glass-card, input, button and badge components; follow the scoped-state rules in
VNEXT_DESKTOP_WEB_UX_PROFILE_V1 and the labelled forms, explicit confirmation,
inline error/status and wrapping private identifiers of the current KU workflow.
There is no separate Web component/token catalog in this tree; mobile design
documents do not govern this task. Use responsive cards, visible keyboard focus,
semantic headings/labels and textual state independently of color.

| Interaction | Existing operation |
|---|---|
| Read local state / refresh observations | GET status; never discovery refresh |
| Sources / reservations: load first page, next page | source_list / reservation_list, limit 32 |
| Import host-issued reference with selected source kind | source_admit |
| Enable/disable exact source | source_set_enabled; no deletion |
| Request bounded discovery refresh | refresh |
| Connect full expected NodeID | route_request; no address or application payload |
| Inspect exact route ID / intent ID | route_status / intent_status |
| Schedule retry of inspected pending intent | intent_retry; preserve counters and checkpoint |
| Save two independent opt-ins | configure |
| Kill / explicitly re-enable generation | network_kill / network_reenable |
| Recover retained command key | fresh status, same dataset, metadata reconcile |

All eight mutations require a preview of exact immutable operation/payload,
generation and key followed by typing the exact key. Configuration begins with
both checkboxes off. Advertising additionally requires acknowledgement that the
trusted host supplied its public-fields/expiry preview. Five management actions
require a host-issued scoped capability entered into an in-memory password field;
never persist, log, preview or put it in a URL. A supplied token does not prove
authority; only the node can accept it. Missing control/intake/management binding
is explicit. No upload/file/QR intake, capability minting or raw-address fallback.

Persist only the pending command's original operation/payload/session and API
origin in this tab's sessionStorage before dispatch; storage failure prevents
dispatch. No credentials, client-session capabilities or WS tickets persist.
Reload restores recovery only, never dispatch. Unknown outcomes lock further
mutations until resolved; metadata completed does not fabricate the result.
Keep unresolved/local_not_found records; dataset replacement blocks reconciliation
against that dataset. Process restart uses fresh status with the original key.
No automatic replay, replacement key, changed generation or retry loop. Explicit
acknowledgement may clear completed/failed_no_effect recovery, never unknown.

Pagination retains its exact session and generation. Context changes invalidate
the page; expiry/conflict asks for an explicit first-page reload, never appends a
different snapshot. Clear detailed observations after context changes and WS
gaps. Stream hints trigger only REST read refresh; no inferred completion or
mutation replay. Reconnect explicitly mints a new single-use ticket. Show stream
loss separately from node/network state. No background polling is required.

Private route/intent/source IDs stay in authenticated responses and this local
page; they never enter URLs, debug logs, analytics or aggregate notifications.
Validate bounded response shapes, exact target identities and authenticated
route/checkpoint invariants before display. Reject integers beyond JS safe range
rather than round a generation. No discovery/relay/command result claims global
absence, delivery, trust, truth, publication or reward. Candidate paths are the
accepted route state/path kind only; no peer-directory, outbox-list or raw attempt
history exists. Failover belongs to the shared node, never a browser planner.
