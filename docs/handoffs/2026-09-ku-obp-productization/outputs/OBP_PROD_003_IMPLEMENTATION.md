# OBP-PROD-003 — Local discovery and reservation orchestration

Date: 2026-09-21. Owner authorization: D-026 (task 003 local before task 002
merge). Original working tree: `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-prod-003-discovery`, HEAD
`467e2ad3ada305597b2d1808c03da6190d0643cb`. All prior CLI/Web/semantic and
task-001/002 changes remain uncommitted in this tree. No commit, push, merge,
model experiment or live-host activation occurred.

## Implemented behavior

- The existing node-owned worker now schedules bounded discovery and standing
  reservations. Trusted host inputs include pinned DNS/IP bootstrap sources,
  signed manifests, canonical manual relay/peer invitations, signed relay
  descriptors, rendezvous/DHT byte sources and PEX bound to a live authenticated
  connection. Network bytes cannot install trust keys or mint session authority.
- Bootstrap source loss preserves fresh learned endpoints and admitted records.
  Each source and learned endpoint has a bounded share of the refresh deadline;
  one hung endpoint does not consume every other source's opportunity. Invalid
  sources are isolated. Refresh uses the frozen record, byte and signature
  ceilings, at most eight configured sources, and bounded backoff.
- Core admission still verifies canonical signatures, freshness, public endpoint
  policy and relay possession. Generation/grant checks fence stage and commit.
  PEX requires its live sealed connection; cached data never restores a session.
- The same redb store persists bounded canonical cache entries and durable replay
  floors. Restart recovery permits only exact locally cached bytes matching the
  current floor, followed by fresh validation and possession. It never resets a
  floor or labels persisted reservations/carriers as live.
- Standing orchestration targets three distinct admitted relay identities,
  renews within the existing 180-second margin and maintains the 20-second
  keepalive cadence. Fewer than two usable grants reports partial coverage.
  Distinct NodeIDs do not certify independent operators or public availability.
- Reserve and advertisement emission sequences and exact pending bytes are
  checkpointed atomically before I/O. Unknown reserve results remain pending
  with `reservation_reconcile_required`; restart does not blindly replay them.
  Checkpoint corruption fails closed. Advertisements require separate explicit
  opt-in and a host-supplied capability ceiling, contain current grants and no
  private candidates, and project bounded publisher acknowledgement only.
- Read-only local Rust service snapshots expose redacted bounded source and
  reservation health without waiting on discovery I/O. Public management/API
  transport and automatic routing remain subsequent tasks.

Implementation: `onebrain-node/src/vnext_outbound_product/{discovery,cache,standing}.rs`,
the existing product runtime, reachability manager/replay store and bootstrap
client; small supporting changes in `ku-net` and the relay reservation store.
Paths in this paragraph are relative to repository `src/`.

## Defects found by real TLS verification

The new loopback integration test uses the production authenticated TLS relay,
dual-signed reservations and durable relay store. It found and now covers:

1. Node keepalive must continue from the signed reserve sequence, as the relay
   already requires, rather than start at zero.
2. Durable keepalive keys must include their reservation scope. Sharing the
   per-target reserve key caused the next renewal to collide with a keepalive.
   Existing reserve keys/floors remain intact; no durable state is deleted or
   migrated. Previously colliding legacy state continues to fail closed.
3. Closing a TLS carrier must release its write half even while another caller
   retains the connection object. Close cancels an in-flight writer and drops
   the half through a guard; no detached cleanup worker is added.

Wire objects, domains, target/relay signatures and exact successor semantics are
unchanged. Uncertain keepalive completion closes the carrier, including when
the owning maintenance future is cancelled.

## Validation

All Cargo commands used `--offline --locked` from `src/`. Tests used temporary
stores, deterministic signed fixtures and isolated loopback listeners; the
existing OneBrainLocal host and external services were not changed.

| Check | Result |
|---|---|
| Node `--features vnext-outbound-first --lib` | 234 passed |
| Node lifecycle/reachability/outbound integration suites | 7 + 4 + 9 passed |
| Node `vnext_relay_matrix_live` | 2 passed; three-host/two-relay authentication and reservation renewal/keepalive/carrier closure |
| ku-net `--features outbound-first --lib` | 311 passed |
| ku-net discovery/crypto/resolver/reservation integration suites | 4 + 10 + 2 + 2 passed |
| Relay `--lib` | 20 passed |
| Node `--no-default-features` and `--features vnext-network-runtime` checks | Pass; existing feature-reduced unused-code warnings |
| OBP contract mutation tests | 6 passed |
| Aggregate vNext contract validator | Pass; 13 operations, 18 DTOs, 26 fixtures and authority pins unchanged |
| Scoped Rust formatting and `git diff --check` | Pass |

The 15 added node unit tests cover DNS/IP seed loss, exact cache restart with
fresh possession, cancellation during admission, invalid/expired input,
durable unknown emission, source bounds, endpoint fairness, source isolation,
dual-signed manual peer recovery, reservation target/renewal, separate publication
opt-in, exact failed-publication replay, revoked work, nonblocking snapshots and
corrupt pending checkpoints.

## Evidence limits and next boundary

- These are local implementation tests, not public relay deployment, NAT-canary
  qualification, independent-operator evidence or a global discovery claim.
  DNS, possession and publisher ports in scheduler tests are controlled fixtures;
  the separate TLS integration uses real loopback transport and production relay.
- Rendezvous/DHT/PEX byte adapters and publisher are supplied explicitly by the
  trusted host. The HTTPS adapter fetches one existing canonical object; it does
  not invent a new wire envelope or server endpoint. Peer invitations are
  validated reachability data, not completed target-peer handshakes.
- Unknown reservation effects and expired pending advertisements require explicit
  reconciliation; this task does not add a management mutation/reconcile API.
  Stale pending advertisements stay degraded rather than silently advancing a
  sequence. Crash gaps between admission-floor and cache writes fail closed.
- Source management REST/WS, public API idempotency/pagination and automatic
  outbox routing are not claimed. Networking and advertising remain opt-in.
- Task 004 still requires task 003 merged or another explicit owner exception.
  D-026 authorizes task 003 only. This result is ready for local review, unmerged.
