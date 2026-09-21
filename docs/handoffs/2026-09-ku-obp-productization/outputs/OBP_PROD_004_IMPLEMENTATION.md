# OBP-PROD-004 — Local routing and durable delivery evidence

Date: 2026-09-21. Original workspace: `C:/Users/shpy2/Documents/OneBrain`.
Branch: `codex/obp-prod-004-routing`; base: local merge
`3216f1ddf0cd72098924ccdf9fb0ef6b30186b78` (D-027). Implementation committed as `8f7d327`, merged as `2c39117` and pushed to
`origin/main` under D-028, together with the routing branch and retained baseline.
No live-host activation, model tuning or rollout change.

## Implementation

- The existing product owner installs routing before starting the existing outbox
  scheduler. It shares the node identity, QUIC transport, resource admission,
  verified discovery, reservation manager and network generation. Inbound relay
  acceptance runs within the existing owner worker with four bounded candidates.
- Domain-consented Public Use publications request their exact peer from this
  owner, then export their immutable feed/object/event bytes to the existing
  durable network outbox after authentication. No caller address or fresh consent
  is inferred. Missing discovery/grant leaves existing network intent pending
  without dialing or resetting its retry counter.
- Capability-supplied admitted LAN/private direct, public direct and coordinated
  hole-punch paths precede relay candidates. Available UDP relay paths precede TCP
  on the initial attempt; the previously selected relay is deprioritized for the
  next session. Alternatives use standing reservations, signed associations and
  fresh expected-peer authentication. No raw-address fallback bypasses the owner.
  Shared outer relays demultiplex ConnectRequests by peer and reservation pair
  with a bounded inbox; Association responses are keyed by request ID. These
  routing hints grant no authority before validation. Opaque demultiplexing wakes
  the matching session, and releasing an inner socket releases its waiting pump.
- Signed association lifetime bounds the derived outbound candidate, including
  the shorter outer-session lifetime. Inbound candidates bind the initiator's
  reservation. These fixes retain the frozen wire and signature checks.
- Delivery uses the same reconciliation receiver, canonical validation, scoped
  manifests, provenance and resource admission as existing network delivery.
  Receipt binding is checked before acknowledgement. Accepted receipts and durable
  checkpoints commit atomically; new sessions resend only nonterminal intents.
  Receiver canonical storage deduplicates repeated content.
- An internal transactional guard binds the immutable acknowledged checkpoint to
  subsequent legitimate outbox changes. Enqueue/retry/compaction cannot silently
  repair a corrupt checkpoint. Archive/restore includes checkpoint, sequence and
  guard rows. Compaction pins the latest acknowledged record per peer.
- Startup validates checkpoint state and its exact historical route-journal root
  before binding transport. Journal sequence allocation and append are atomic.
  Restart restores no session authority. Route address changes retain attempt
  counts and cannot resurrect terminal work.
- Existing retry/backoff, per-batch deadlines and shutdown cancellation bound work.
  Session checks fence grant revoke/re-enable and network epoch changes. Status
  reports routed authentication counts and partial reservation coverage without
  global-completion or reward claims.

## Verification

From the repository root, with Cargo locked/offline and temporary test stores:

```text
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --lib --test vnext_node_runtime --test vnext_reachability_manager --test vnext_outbound_first_runtime --test vnext_relay_matrix_live
```

242 unit tests and 22 integration tests passed (7 + 9 + 4 + 2).
`cargo test --offline --locked --manifest-path src/Cargo.toml -p ku-net --features outbound-first --lib`: 315 tests passed. Additional ku-net tests cover concurrent peers, cancellation, bounded inbox
retention, Association response routing and pump release on socket drop. Focused routing tests cover stale address bypass, same-peer restart with a new
socket/session, exact checkpoint progression, wrong-peer rejection, bounded retry,
missing discovery/grant, and old-session fencing after grant/epoch changes.
The two-relay fixture uses real TLS relay services, inner QUIC and OBP handshake
on loopback: both alternatives are reserved before selected relay loss; pending
work succeeds on the alternate with a fresh session and no duplicate feed branch.
A third batch reuses that outer relay with another fresh inner session.
Its endpoint-resolution/possession adapters are controlled test fixtures.

Outbox tests verify exact checkpoint preservation through enqueue, retry, archive,
compaction and reopen, plus corrupt-guard rejection on read/write/reopen. Existing
lifecycle and carrier tests remain included. Six OBP contract mutation tests and
`python scripts/ci/validate_vnext_contracts.py` passed.

The merge exposed Windows `core.autocrlf` conversion of generated semantic schema
artifacts. `.gitattributes` now pins their existing content to LF; no schema or
model behavior was changed. Scoped formatting and `git diff --check` pass. Offline/locked node checks pass
with no default features, with only `vnext-network-runtime`, and with
`vnext-outbound-first`; the disabled builds retain existing ku-net dead-code
warnings. The final product-focused rerun passed 29 tests.

## Evidence boundary and remaining scope

- This is local product integration, not Internet/NAT/platform qualification.
  Optional LAN/hole-punch execution requires a host-supplied admitted capability
  port; absent capabilities skip those optimizations. No host capability is
  enabled by this task. Existing frozen carrier implementations remain owners.
- Peer advertisements come from task-003 verified manual invitations and must
  remain fresh. No automatic new discovery source, mailbox, push-wake, background
  networking grant or infinite retry was added.
- Exact resume here is the durable acknowledged-intent boundary with fresh scoped
  reconciliation. It does not persist or reuse an old transport-bound resume token
  and does not claim a live session survives restart. Loss after remote acceptance
  but before receipt may resend the same immutable content for canonical dedupe.
- The relay-loss test is between acknowledged batches. It is not a new exhaustive
  packet drop/delay/reorder or multi-host canary qualification campaign.
- Public API/UI/operator surfaces belong to OBP-API-001 and later tasks. Task 004
  is merged and published under D-028; OBP-API-001 is dependency-ready, not implemented.


## Merge verification — 2026-09-21, D-028

Fresh pre-merge rerun: 242 node unit tests, 22 node integration tests, 315 ku-net
tests, six OBP contract mutation tests, aggregate vNext validator and staged
whitespace checks all passed. Merge was conflict-free in the original workspace.
Atomic push published main and the routing branch; baseline 3216f1d is retained
in merge ancestry. Earlier feature-build evidence above remains applicable;
no implementation changes were made during this merge action.
