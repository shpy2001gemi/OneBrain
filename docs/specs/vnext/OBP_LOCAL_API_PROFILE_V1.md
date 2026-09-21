# Local OBP API transport v1

Task: OBP-API-001. Status: **accepted under D-029; implementation merged under D-030 (`7d37a30`)**.
This document completes the transport specification prerequisite of the accepted
[OBP composition](OBP_PRODUCT_ORCHESTRATION_PROFILE_V1.md), not its implementation.
The [machine inventory](../../../src/test-vectors/vnext/obp-local-api-v1.json)
and offline checker describe this proposal. They do not prove runtime behavior.

The existing OBP profile allocates no REST routes or WS events. Its section 5
requires exact transport registration before handlers and a separately reviewed
notification projection. The owner accepted this extension under D-029 (2026-09-21). Existing REST/WS
profiles and the accepted 13 operations retain their existing meanings.

## 1. Ownership and scope

Use the existing OneBrainNode / VNextProductRuntime owner. One shared OBP service
owns command admission, operation records, generation checks, input references,
configuration and pagination. REST and future CLI/UI adapters parse, authenticate
and project; they must not own a second planner, signer, relay or outbox.

Compile execution behind an additive `onebrain-api/vnext-outbound-first` feature
forwarding to `onebrain-node/vnext-outbound-first` and the existing API network
feature. It is not a default feature. Feature-off status can report an unavailable
session and compiled=false; it cannot fabricate a dataset, generation or owner.
Read-only requests never construct runtime resources or opt into networking.

No new peer protocol, Base discriminator, canonical hash domain, reward,
disclosure consent, model experiment, live-host activation or rollout is added.
The task's “discovered peer” surface means expected-peer route status within the
13 accepted operations. There is no newly invented peer-directory or raw-dial API.

## 2. Exact route inventory

All ordinary routes require the existing constant-time local Bearer check and
return `Cache-Control: no-store`. Serve on loopback only; this extension is not
remote administration. Missing/malformed Bearer returns 401; wrong Bearer returns
403. Neither case dispatches, parses a command, or mints a ticket.

| Method | Path | Request | Result / authority |
|---|---|---|---|
| GET | `/api/vnext/obp/status` | No body or query | Current session and ObpStatusV1, or explicit unavailable session; product_read |
| POST | `/api/vnext/obp/query` | `{session, operation, payload}` | One of four exact read operations below; product_read |
| POST | `/api/vnext/obp/commands` | `{session, operation, payload}` | One of eight exact mutations; operation-specific authority |
| POST | `/api/vnext/obp/reconcile` | `{session, idempotency_key}` | Read the caller's durable operation outcome; no redispatch |
| POST | `/api/vnext/obp/ws/tickets` | `{session, subscriptions:["network"]}` | Single-use OBP-scoped ticket and independent client-session capability |
| GET | `/api/vnext/obp/ws` | Exactly `?ticket=obw1...` | Consume one OBP-scoped ticket; no Bearer in URL |

The WS upgrade is an exact-route exception to the Bearer middleware, protected
by ticket consumption. It does not create a prefix-based authentication bypass.
OBP tickets must be scoped to this endpoint and cannot upgrade the older WS route,
or vice versa. Reuse the existing bounded hub and aggregate admission ceilings;
do not create another unbounded session/ticket pool. Frozen old topics and event
vocabulary remain unchanged on the existing endpoints.

Session is `{process_generation, dataset_generation}`, both 64 lowercase hex.
The shared node service generates the process fence at startup and persists the
dataset identity with its operation records. They are public fences, not tokens.
Every POST requires the exact current session. An unavailable runtime returns no
session and a disabled envelope; it does not emit fake generation=1 status.
GET status returns `data={available:false, compiled:false|true}` when no shared
OBP service exists, and `data={available:true, session, status}` otherwise.

## 3. Operation mapping and strict payloads

Reuse every accepted DTO without relaxing its fields or meaning:

| Route class | Operation | Request DTO | Result DTO | Access |
|---|---|---|---|---|
| status | status | ObpStatusRequestV1 | ObpStatusV1 | product_read |
| query | source_list | ObpPageRequestV1 | ObpSourcePageV1 | product_read |
| query | reservation_list | ObpPageRequestV1 | ObpReservationPageV1 | product_read |
| query | route_status | ObpRouteRefV1 | ObpRouteV1 | product_read |
| query | intent_status | ObpIntentRefV1 | ObpIntentV1 | product_read |
| commands | configure | ObpConfigureV1 | ObpStatusV1 | host_management |
| commands | source_admit | ObpSourceAdmitV1 | ObpSourceV1 | host_management |
| commands | source_set_enabled | ObpSourceToggleV1 | ObpSourceV1 | host_management |
| commands | refresh | ObpRefreshV1 | ObpStatusV1 | product_control |
| commands | route_request | ObpRouteRequestV1 | ObpRouteV1 | product_control |
| commands | intent_retry | ObpIntentRetryV1 | ObpIntentV1 | product_control |
| commands | network_kill | ObpNetworkFenceV1 | ObpStatusV1 | host_management |
| commands | network_reenable | ObpNetworkFenceV1 | ObpStatusV1 | host_management |

Every mutation carries its existing 64-hex idempotency key and expected durable
network generation. No raw address, input path, signer, principal, capability
scope, `authorized` or transport-bound resume token is accepted in a body.
Private peer/intent/source IDs are carried only in authenticated POST bodies.

Require JSON objects, exact field sets, no duplicate keys at any depth, no null
optional fields, finite integers, depth <=16 and UTF-8 JSON <=1 MiB before DTO
allocation/dispatch. Content-Type must be application/json (optional UTF-8
charset). Reject unexpected GET bodies/query fields. Responses including envelope
are <=1 MiB; limitations are <=64 allow-listed entries of <=128 UTF-8 bytes.
No arbitrary upstream error text, resolver result or path enters the response.

## 4. Host authentication and input intake

The API Bearer maps to a host-installed local principal, never a caller-provided
principal. In the initial single-Bearer server all callers share that principal;
this is not per-human isolation. Product reads use the weak node service handle.
Product control additionally requires a host-installed, revocable control grant.
No control grant is installed implicitly by creating an API server.

The five host_management operations also require
`X-OneBrain-OBP-Management: obm1.<32 random bytes in unpadded base64url>`.
The trusted host installs this bounded capability in-process; this API has no
management-token minting route. Its node-owned scope binds principal, dataset,
process, permitted management operations and monotonic expiry (at most 300 s).
Cap at 32 active grants; recheck expiry/revocation before effect, not only at
HTTP admission. No capability is serialized to durable storage, returned in
status, accepted from a query string, or inferred from the ordinary Bearer.
Missing, wrong, expired, revoked or wrong-scope capability returns the same 403.
The optional WS client-session header is never management/control authority.

Input transport is deliberately in-process: the trusted host registers a bounded
typed DiscoveryInput with the node, receiving the accepted 64-hex InputRef. Max
eight references and 1 MiB retained input bytes, with a 300 s unconsumed lifetime.
No upload, filesystem read or arbitrary URL fetch endpoint is added. A future
file/QR UI resolves its input through this same trusted host port, not REST paths.
The existing canonical per-object size/signature/expiry/identity limits apply.
References bind principal, dataset, process, SourceKind and immutable input.
source_admit revalidates the binding and freshness and never consumes an unrelated
reference; successful idempotent replay does not require resurrecting the token.
Expired/unavailable refs fail explicitly; they cannot fall back to addresses.

Host configuration is the authority for persisted source rehydration. Persist
source policy and bounded signed cache under existing node owners, retaining
replay floors. Never serialize a PEX lease, callback, signer or bearer grant.
On restart, missing host bindings project unavailable/disabled; cached bytes do
not manufacture live authority. Source enable/disable cannot bypass fresh crypto
admission, revive expired records or erase replay floors.

configure commits requested configuration durably, separately from actual active
state. If startup ownership cannot apply it live, return requested/degraded with
`restart_required`; do not start another runtime. Disabling fences new work before
acknowledgement. Advertising remains separately opted in. reenable changes the
existing network generation and cannot enable unrelated lanes or grant execution.

## 5. Durable commands and recovery

Before any side effect, serialize command admission under the node owner. Bind
the idempotency key to principal, dataset, operation and exact typed payload.
Whitespace/key order are immaterial; compare a deterministic typed encoding, not
raw HTTP bytes or an unkeyed truncated fingerprint. Expected generation is part
of this binding. Same key with another payload/operation is conflict; never run it.

For a new command, check current session, capability, generation, resource and
storage admission before durably recording `admitted`. Persist admission before
performing I/O. Cap retained operation records at 4096 and 16 MiB within existing
host storage budgets, with at most four concurrent executions. At capacity reject
new work before effects; do not silently evict replay protection. A storage full
condition must leave an independently bounded path to the existing network kill
control available to the trusted host; REST admission is not that emergency path.

Command outcomes use ObpOperationV1: idempotency_key, operation, state and
reconcile_before_retry. State is one of admitted, completed, failed_no_effect,
reconcile_required. completed has exactly one result of the mapped DTO type;
failed_no_effect has exactly one bounded failure and certifies no effect;
admitted/reconcile_required have no result or delivery claim. These are command
record states, not new outbox, route or reservation states.

Successful completed and failed_no_effect records replay their recorded result
without re-execution, after current identity/capability checks. Lookup occurs
before rejecting an old expected_generation on an exact known replay, so replay
of an acknowledged kill does not kill again or conflict solely with its own
generation advance. Unknown keys still require current expected_generation.
New process/session requires GET status then reconcile of the original key.
Dataset replacement rejects the old key/context; it is never treated as a new
command automatically. Expired/revoked management authority prevents dispatch
and result replay, but product_read may read redacted outcome metadata.
Reconcile returns a distinct ObpReconcileV1 with exactly idempotency_key,
operation, state and reconcile_before_retry; it never includes result/failure.
The full ObpOperationV1 result is available only through an authorized exact
command replay. Thus a metadata-only completed record does not violate the
full command result's required result field.

Reconcile only reads/derives effects already evidenced by their owners; it never
dials, republishes, enables a lane, clears a checkpoint or resets a retry budget.
On restart, an admitted command with no atomic completion proof becomes
reconcile_required. Across stores, missing proof is unknown, not failed_no_effect.
Atomic local effects/result records are preferred; exact owner receipts may prove
an effect after a crash. This proposal does not claim cross-database atomicity.
Keep unknown outcomes until explicit future resolution; never automatically retry.
A missing key returns local_not_found, which describes the current dataset only.
After a transport gap or dataset replacement that response does not prove that
the original effect never happened and does not authorize submitting a new key.

route_request authenticates the exact expected peer using task-004 routing,
without sending application payload. A path-limited route is a completed bounded
attempt, not global absence. Restored route metadata cannot report connected
without a fresh live authenticated session. intent_retry schedules an existing
pending intent only; preserve bytes, counters, consent and acknowledged checkpoint.
It cannot resurrect dead_letter/retry_exhausted or repeat model extraction.

## 6. Envelopes, errors and paging

Reuse VNEXT_PRODUCT_INTEGRATION_PROFILE_V1 success metadata and its eight outer
error codes/HTTP mappings. Add an OBP-only `error.obp` detail to error responses
on these new routes, never existing routes. It contains reason, outcome and
reconcile_before_retry; idempotency_key is optional and only echoed after valid
parsing/authentication. reason is a finite machine enum, not arbitrary text.
Auth failures retain a minimal uniform 401/403 response outside that vocabulary.

| Reason | Outer code / HTTP | Outcome | Reconcile |
|---|---|---|---|
| invalid_payload | invalid_request / 400 | not_admitted | false |
| local_not_found | not_found / 404 | not_admitted | false |
| generation_conflict, idempotency_conflict, session_conflict | conflict / 409 | not_admitted | false |
| input_expired, snapshot_expired | expired / 410 | not_admitted | false |
| admission_limit | rate_limited / 429 | not_admitted | false |
| feature_disabled | capability_disabled / 503 | not_admitted | false |
| dependency_unavailable | dependency_unavailable / 503 | not_admitted | false |
| outcome_unknown | dependency_unavailable / 503 | unknown | true |
| storage_corrupt | internal_error / 500 | unknown | true |
| response_overflow | internal_error / 500 | unknown | true |

Outer retryable retains the frozen meaning; reconcile_before_retry takes
precedence for mutations. A connection timeout with no response is unknown to
the client even if the server might have rejected before dispatch. A definitive
failed_no_effect record cannot use an unknown-outcome reason. Redacted reconcile
responses omit result/failure detail if current operation authority is absent.

Pages retain the accepted 1–256 item limit and <=2048-character obc1 token. Store
bounded immutable snapshots in the shared service: at most 32 snapshots, aggregate
1 MiB, 60 s lifetime. Bind token to principal, dataset/process, network generation,
query kind/filter, stable full-ID order, frontier and offset. A changed context
is conflict, evicted/expired snapshot is expired; never silently restart a page.
All generation reads around snapshot acquisition must agree or return conflict.
No raw continuation payload or signer material is exposed in telemetry.

## 7. Accepted private WS extension

New profile: OBP_PRIVATE_WEBSOCKET_PROFILE_V1. Only topic `network`, scoped to
the new ticket/upgrade routes. Ticket TTL=30 s, client-session TTL=900 s, 32 random
bytes each and independently generated. Existing shared limits remain 128 pending
tickets, 64 active sessions, queue=32, client frame<=4096 bytes. Disconnect the
slow session on overflow using nonblocking enqueue; never block node work.

Two events only: subscription_ready and network_state. The latter carries only
compiled, requested, active, kill_switch, signer_ready, source_count,
usable_reservations, authenticated_routes, pending_intents, advertisement_state,
claims_global_completion=false and authorizes_reward=false. Envelope has profile,
event_type, per-session sequence, timestamp, lifecycle, coverage and allow-listed
limitations. No IDs, addresses, route path, attempts, input refs, operation keys,
checkpoint digests, credentials or raw error strings appear in either event.
Ready data is `{subscriptions:["network"], session_expires_at}` only.

Emit a redacted snapshot on subscription. A REST operation can target a refreshed
snapshot to its exact active session through X-OneBrain-VNext-Client-Session;
that session must belong to the same principal/dataset and this profile. Deduplicate
identical snapshot data/metadata per session, excluding envelope time/sequence.
No global broadcast or autonomous polling worker is introduced. These are hints,
not guaranteed push updates: clients refetch REST after a gap or remote change.
Reading status or handling a notification never activates any network lane.

## 8. Implementation acceptance after transport review

1. Strict DTO/auth/capability tests, wrong endpoint ticket and feature-off tests.
2. Shared-service integration for every accepted operation, with real temporary
   stores and existing loopback carriers, not an API-only mock service.
3. Exact replay, conflicting reuse, concurrent key admission, stale generation,
   restart and four crash boundaries: before admission, after admission, after
   owner effect, after result commit. Unknown effects must never blind-replay.
4. No raw-address input, no private fields in WS/log output, principal/dataset
   isolation, pagination expiry/context tests and slow-client isolation.
5. Existing REST/WS/KU suites and compile-feature checks remain green; no live host,
   model, public-network qualification or rollout is involved.

The contract checker verifies the inventory and fixtures, not runtime behavior.
The shared management façade, durable journal and REST/WS handlers are registered
behind the opt-in feature. See task implementation evidence for runtime checks
and limitations; registration does not enable a host or grant execution.
