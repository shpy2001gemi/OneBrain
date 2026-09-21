# OBP product orchestration profile v1

> OBP-PROD-001 — **Accepted for local implementation, D-025 (2026-09-20)**.
> Machine inventory: [obp-product-orchestration-v1.json](../../../src/test-vectors/vnext/obp-product-orchestration-v1.json).
> No activation, endpoint registration, canonical schema ID or signature domain is allocated here.

## 1. Composition and authority

This proposal composes the already implemented
[outbound-first core](OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md) under
[node ownership](RUNTIME_OWNERSHIP_PROFILE_V1.md),
[lifecycle](RUNTIME_LIFECYCLE_PROFILE_V1.md),
[budgets](RUNTIME_FEATURE_BUDGET_PROFILE_V1.md),
[route authority](ROUTE_AUTHORITY_BOUNDARY_PROFILE_V1.md) and
[durable rollback](MIXED_VERSION_RUNTIME_ROLLBACK_PROFILE_V1.md).
Those frozen contracts retain their authority. Existing authenticated-session,
reconciliation, inventory, outbox, receipt and canonical bytes are unchanged.

OneBrainNode owns one VNextProductRuntime, which owns the network runtime and
its Reachability Manager, discovery/admission state, reservations and route
journal. VNextProductServices exposes typed operations; applications never
construct a second network owner or obtain a raw transport/store/signer.
The same transport and authenticated route directory serve every product lane.
The existing local QUIC socket startup requirement remains; no public inbound
port, port forwarding, UPnP or router changes are required of an ordinary node.

KU remains usable locally with networking disabled. Discovery or connection
never saves/publishes a KU, creates UseEvidence, adopts a Mapping, establishes
truth/fidelity, authorizes a reward or mutates OBT/wallets. An independently
authorized domain operation creates its own outbox intent; this profile adds
no general-purpose raw payload enqueue or disclosure-grant endpoint.

## 2. Explicit operator configuration and bootstrap

The proposed host configuration has `outbound_first_requested=false` and
`advertise_reachability=false`. It is not a new replicated feature bit. It
requires compiled `vnext-outbound-first` plus active `object_event_v1` and
`obp_rp`; it cannot override the existing durable network kill generation.
Opting into outbound reachability permits bounded relay/discovery traffic.
Advertising is a separately explicit local setting, with a preview of the
public fields and expiry. It authorizes only the target's short-lived signed
reachability advertisement, never KU/source disclosure or permanent Public Use.

Trusted-local host input ports supply these finite input kinds:

| Input | Admission |
|---|---|
| DNS/IP relay endpoint plus signed descriptor/invitation | DNS/IP is location only. Full self-certifying relay identity, descriptor signature/freshness/replay and live endpoint possession must validate before use. A bare endpoint remains an unresolved hint and is never dialable authority. |
| Configured bootstrap source | Reuse `ConfiguredBootstrapSource::load_from_trusted_local_file`: exact bounded local config, pinned source public key, transport/host/port/path. Fetched bytes cannot construct this local trust binding. |
| Signed manifest bytes | Verify the existing BootstrapManifestV1 against an independently admitted source key; do not trust a key supplied only inside the manifest. |
| Manual relay/peer invitation | Use existing canonical `onebrain://relay/v1/` or `onebrain://peer/v1/` envelopes, whether pasted or admitted from file/QR. Verify every contained object and the exact target identity. |
| Learned PEX/rendezvous/cache | PEX requires the live authenticated opposite-peer lease. Other sources contribute signed bytes only. No learned record installs a new bootstrap trust key or bypasses source budgets. |

The typed product source-admission request selects a host-owned opaque input
reference; it does not pass raw filesystem paths, private keys, `authorized`,
caller-selected authorities or arbitrary network URLs into shared services.
The host intake adapter must specify its exact bounded transport before API/UI
implementation. No new raw configuration/upload endpoint is implied here.

DNS is bounded, public global-unicast only and revalidated at dial time to the
exact admitted set. Reuse sealed dial tokens; no ambient DNS/proxy/redirect
fallback. Private/LAN candidates remain in target-specific authenticated
signaling and never enter public discovery.

## 3. Lifecycle, refresh and failure isolation

Before side effects, validate feature/dependency presence, proof-checked signer,
Vault/policy ports, trusted-local inputs, budgets and current execution grant.
Never-requested owners open no new stores, sockets or workers. Ordered startup:

1. Reuse the aggregate's configuration/signer/store/listener startup phases.
2. Recover bounded replay floors, admitted signed cache, route journal and
   nonterminal outbox/checkpoints; validate before projection. Never restore a
   live carrier, PEX lease or reservation merely from persisted bytes.
3. Rehydrate enabled lanes and perform the existing bounded outbox drain; lack
   of a route retains pending work and does not fail all local startup.
4. Register one reachability scheduling worker under aggregate cancellation,
   within the existing eight-worker ceiling. This worker schedules bounded
   discovery, reservation, keepalive and advertisement work; no detached task.
5. Mark the aggregate running only after required phases complete. Reachability
   can separately remain partial/degraded while local services are available.

Use frozen core ceilings and intervals without increasing them: 8 source keys,
64 records/1 MiB per source, 256 total records/signature checks, 4 concurrent
checks, 12 route attempts, 20 s route deadline, 2/3/3 minimum/target/maximum relay
reservations, 20 s keepalive, 180 s reservation refresh margin and 2 s probe
cadence. Device grants may reduce permitted work, never invent successful
coverage. Budget and storage pressure stop new admission before consuming input.

Each source projects `configured`, `refreshing`, `usable`, `expired`,
`unavailable`, `rejected` or `disabled`. These are local views, not signed wire
states. Retain still-valid learned records after one source fails. A source
key's replay floor survives disable/re-enable. Refresh before the earliest
admitted expiry using the existing scheduler; repeated failures use bounded
backoff (2, 4, 8, 16, then 20 s) inside grant/deadline ceilings. No unbounded
catch-up burst after suspend. Expired data is not used while awaiting refresh.

Reservations project `pending`, `active`, `refreshing`, `expired`, `denied` or
`revoked`; active means a currently usable dual-signed reservation and live
outer carrier, not an authenticated session with a target peer. Renew only
while requested/current-generation/granted, maintaining independent available
relays under local diversity policy. Below two usable reservations is explicit
partial coverage; labels do not prove independent operators.

Advertisements project `disabled`, `pending`, `published`, `expired` or
`degraded`. Sign only the existing minimal public object, advance the durable
sequence, and remove stale reservation/candidate references before refresh.
`published` means a bounded source-write acknowledgement, not global discovery,
peer reachability or payload delivery. If no live reservation exists, do not
advertise it. Revocation/withdrawal is best effort; expiry remains authoritative.

Shutdown fences new operations, cancels and joins workers, settles safe metadata,
closes ephemeral reservations/carriers, then stops the shared network and closes
stores in the frozen order. Partial startup rolls back only newly created
artifacts; pre-existing files and durable disabled generations survive.

## 4. Route and durable-intent state

Preserve capability-aware order: direct/LAN, public/reflexive direct,
coordinated hole punch, relay UDP, relay TCP-443. The four public path kinds
remain `direct`, `hole-punched`, `relay-udp`, `relay-tcp-443`.
Web/mobile carrier implementation and qualification remain outside this task.

Route state is the existing finite planner projection: `discovering`,
`direct_checking`, `hole_punching`, `relay_connecting`, `peer_authenticating`,
`connected`, or `path_limited`. Only connected has an authenticated peer and
route receipt, with exact expected-peer equality and a fresh transport-bound
Hello/Welcome/Finish transcript. Intermediate carrier success is not connected.
Wrong peer rejects that path without replacing the expected peer or route.

Retain exact failures `NoBootstrapReachable`, `CandidateExpired`,
`DirectTimeout`, `HolePunchFailed`, `RelayDenied`, `RelayUnavailable`,
`PeerIdentityMismatch`, `NetworkChanged`, `BudgetExceeded`, `PathLimited`.
Other host admission/configuration failures use existing typed service errors;
they cannot masquerade as signed route failures. No path within budget means
path-limited, not globally offline or absent.

Intent status preserves existing `OutboundIntentState`: `pending`,
`acknowledged`, `dead_letter`, `retry_exhausted`. Retry is only a bounded
scheduling hint for an existing pending intent; it cannot reset counters,
resurrect terminal work, replace exact bytes, change scope or create consent.
Deferred validation remains pending with an explicit limitation; no new durable
state is inserted into the existing outbox format.

Existing intent identity binds expected peer, selector, namespace, disclosure,
record kind and exact content CID. Last-known address is not an identity input.
Domain publication stays pending until its already-required authenticated route
exists; the planner may resolve that exact peer without supplying an address to
Public Use confirmation. This profile does not remove that existing gate.

On relay/carrier loss: stop writes, preserve intent and counters, choose an
admitted alternate, establish a fresh binding, authenticate the same peer,
validate the acknowledged checkpoint, then resume exact remaining work. Persist
acknowledgement and checkpoint atomically via the existing outbox owner before
projecting `acknowledged`. A socket write, relay receipt, route receipt or mailbox
acceptance is insufficient. Corrupt/mismatched checkpoint fails closed and
requires reconciliation, never reconstructed success. Restart loses all
ephemeral sessions; durable intent does not authorize fresh source extraction.

## 5. Typed product projections and operator actions

The machine inventory owns proposed required/optional DTO fields, finite enums,
bounds and fixtures. All IDs use 64 lowercase hex with distinct field roles.
No new canonical hash domain or Base numeric discriminator is allocated.
Optional fields are omitted, not null. Unknown/duplicate fields reject.
Payloads are at most 1 MiB, 256 page items and 2,048-character opaque `obc1`
continuations. Pagination binds local principal, generations, filter, full-ID
ordering and snapshot frontier; expiry/context change requires a fresh query.

Logical service actions: status, configure, source admit/enable/disable, bounded
refresh, source/reservation list, route request/status, intent status/retry,
network kill/re-enable. Exact request/response types are in the inventory.
Every mutation binds a stable idempotency key and expected durable generation;
same key/different request conflicts. Route request may connect but cannot send
an arbitrary application payload. Refresh/status/read cannot enable a lane.
Configure/admission and kill/re-enable are host-management operations; ordinary
product read handles cannot manufacture that capability. A re-enable advances
the existing network generation explicitly and enables no dependent product lane.

The additive authenticated local-private projection keeps compiled/requested/
active/kill/signer-ready independent, with existing envelope lifecycle
`disabled|requested|active|degraded`, coverage `local_only|partial`, limitations
and continuation. Optional successful authentication/acknowledgement fields
cannot be guessed from counts. All completion/authority/reward flags are false.
Do not expose raw addresses, config paths, credentials, LAN topology, private
payloads or candidate attempt history in these DTOs or WS/telemetry. Exact local
peer/intent IDs stay in authenticated local responses; aggregate counts alone
are eligible for a separately reviewed notification projection.

OBP-API-001 must register its exact routes, authentication/capability binding,
bounded failure DTO and operation reconciliation transport before handlers.
No existing REST route/DTO or WS event is silently extended. Existing outer
REST error codes retain their meanings. Unknown outcomes must carry an explicit
reconcile-before-retry projection; transport failure cannot trigger blind replay.
CLI spellings and UI interactions are later adapters of these logical actions.

## 6. Feature and review gates

The existing network kill generation fences discovery, reservations,
advertisement, routing and new record admission. Existing in-flight drain rules
remain; each later record rechecks generation. Kill preserves durable intent,
replay floors, route/checkpoint journals and accepted content. Startup cannot
undo a durable kill. Never auto-enable KQL, publication, PoMV or a mailbox.
Unknown versions fail closed; rollback retains original bytes and pending work.

OBP-PROD-001 is contract evidence only. Owner review was accepted in D-025,
which also permits OBP-PROD-002 local implementation before merge. Other merge
and release prerequisites remain. Implementation acceptance tests
must prove ownership/startup rollback, source independence, restart/expiry,
wrong-peer rejection, UDP-blocked fallback, exact failover checkpoints, bounded
retry, disabled zero-work and redacted cross-surface parity. Windows/macOS and
real multi-host qualification remain separate; Linux evidence does not promote
them. No task here implements mobile or changes its evidence contract.
