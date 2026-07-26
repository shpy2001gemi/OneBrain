# OneBrain vNext Private WebSocket Profile v1

> **Work package:** `DR-P3.2`
>
> **Status:** Implemented
>
> **Date:** 2026-07-26
>
> **Parent contract:** [vNext Product Integration Profile v1](VNEXT_PRODUCT_INTEGRATION_PROFILE_V1.md)
>
> **Machine profile:** [`private-websocket-profile-v1.json`](../../../src/test-vectors/vnext/private-websocket-profile-v1.json)
>
> **Code:** [`onebrain-api::vnext_ws`](../../../src/onebrain-api/src/vnext_ws.rs)

## 1. Surface and authentication

P3.2 adds two vNext-local routes without changing the legacy
`/ws/events?token=...` contract:

| Method | Path | Authentication | Purpose |
|---|---|---|---|
| POST | `/api/vnext/ws/tickets` | Existing constant-time Bearer boundary | Mint one short-lived, single-use ticket and one client-session capability for an exact topic set. |
| GET | `/api/vnext/ws?ticket=obw1...` | Consume the minted ticket | Upgrade one private bounded WebSocket session. |

The ticket REST response uses the parent vNext success/error envelope.
WebSocket frames use the private WebSocket profile identifier below.

Ticket minting MUST require the existing local Bearer authentication boundary.

The WebSocket upgrade MUST consume exactly one unexpired `obw1` ticket and
MUST return the same authentication failure for missing, malformed, expired,
unknown and replayed tickets.

Tickets MUST contain 32 random bytes, use unpadded base64url after `obw1.`,
expire after 30 seconds and be single-use.

The client-session capability MUST be independent of the ticket, expire after
15 minutes and never appear in a WebSocket event, log or public projection.

A REST request that wants a targeted event MUST still carry the normal Bearer
credential and MAY additionally carry its client-session capability in
`X-OneBrain-VNext-Client-Session`; that header never grants REST authority.

## 2. Immutable subscription scope

Ticket creation accepts 1–4 unique topics from the exact set:

- `matches`;
- `publications`;
- `views`; and
- `runtime`.

The requested topic set MUST be frozen into the ticket before upgrade and
MUST NOT be expanded or changed by a client WebSocket frame.

Every vNext event MUST be routed to one exact active client session whose
ticket includes the event topic; there is no vNext global broadcast fallback.

Disconnect, expiry, malformed client data or bounded-queue overflow MUST
remove only the affected session and MUST NOT block runtime or another client.

The server MUST bound pending tickets to 128, active sessions to 64, each
event queue to 32 frames and client messages to 4,096 bytes.

## 3. Event envelope and minimum vocabulary

Every event uses `VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1` and contains:

| Field | Meaning |
|---|---|
| `profile` | Exact WebSocket profile identifier. |
| `event_type` | One frozen event type below. |
| `sequence` | Monotonic per-session delivery sequence, not a global order. |
| `timestamp` | Local projection time. |
| `lifecycle` | `disabled`, `requested`, `active` or `degraded`. |
| `coverage` | `local_only` or `partial`. |
| `limitations` | Explicit semantic and scope limitations. |
| `data` | Event-specific bounded projection. |

Sequence MUST be monotonic only within one client session and MUST NOT imply
network-wide order, completeness, finality or authority.

The frozen event vocabulary is:

| Event | Topic | Bounded meaning |
|---|---|---|
| `subscription_ready` | always | Ticket scope accepted; events remain non-authoritative hints. |
| `bounded_match_available` | `matches` | One or more new locally quarantined matches are available through authenticated REST. |
| `publication_queued` | `publications` | A new confirmation reached the authenticated outbox and remains `pending`. |
| `publication_delivered` | `publications` | A durable authenticated delivery acknowledgement was observed. |
| `publication_deferred` | `publications` | Publication exists but route/outbox handoff is deferred. |
| `view_revision` | `views` | The session observed a new local view revision without projected conflict. |
| `view_conflict` | `views` | The session observed unresolved/conflicting local view inputs. |
| `lane_active` | `runtime` | A requested lane and its required local dependencies are active. |
| `lane_disabled` | `runtime` | A lane is not compiled or not requested. |
| `lane_degraded` | `runtime` | A requested lane is killed or lacks an active required dependency. |

A `publication_delivered` event MUST NOT be emitted without a real durable
authenticated acknowledgement; queued work cannot synthesize delivery.

Exact idempotent REST replay MUST NOT emit a duplicate match or publication
event identity.

## 4. Privacy and semantic firewalls

StandingNeed ID, QueryDefinition CID, raw/local query, private target,
proposal CID, single-use consent receipt, ticket and client-session capability
MUST NOT enter any vNext WebSocket event.

`bounded_match_available` MUST expose only a bounded count, partial coverage,
`state = "quarantined"` and `executable = false`; clients refetch the exact
private projection over authenticated REST.

View events MUST expose only revision, conflict count and the four literal
false semantic flags; target, policy, frontier, evidence root and event IDs
remain on authenticated REST.

No WebSocket event MUST materialize or adopt a Mapping, grant authority,
create UseEvidence, establish truth or benefit, authorize reward, mutate a
wallet, or claim global completion.

Loading, reconnecting, replaying or presenting a WebSocket event MUST NOT
create a product-runtime side effect.

Zero events, a closed connection or a missing notification MUST NOT be shown
as network-wide absence; clients refetch local scoped REST state after a gap.

## 5. Delivery and recovery

Events are bounded wake-up hints, not a durable event log. A client reconnects
with a newly minted ticket and refetches REST state. Session sequence restarts
with the new client session.

The send path MUST use non-blocking bounded enqueue; a full or closed queue
disconnects that slow session instead of waiting inside runtime work.

View notifications MUST deduplicate the same target/revision/conflict
fingerprint within one client session.

Match notification MUST occur only when a scan inserts at least one previously
unseen quarantined proposal; a zero-result scan emits no match event.

Publication notification MUST occur only for a newly committed confirmation;
exact confirmation replay returns the REST identity without another event.

Lane status MUST be emitted as a local snapshot after a `runtime` subscription
connects, including compiled/requested/active/kill-switch/signer readiness.

## 6. Executable evidence

`onebrain-api::vnext_ws::tests` proves:

- Bearer authentication is required before ticket issuance;
- tickets are random, single-use and bound to an immutable unique topic set;
- two real WebSocket clients do not receive each other's targeted event;
- topic scope prevents match events from entering a publications session;
- private Need, target, proposal, receipt and session fields are absent;
- queue overflow removes only the slow client;
- publication delivery cannot be projected without acknowledgement;
- view events preserve all truth/benefit/reward/global flags as false; and
- disabled lane snapshots remain bounded and local; and
- the legacy `/ws/events` query-token handshake remains compatible.

The feature-enabled P3.1 Public Use acceptance flow additionally proves that
new confirmation and view work produce one scoped event while exact replay
produces none.
