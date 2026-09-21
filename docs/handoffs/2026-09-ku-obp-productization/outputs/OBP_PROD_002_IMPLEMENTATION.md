# OBP-PROD-002 — local lifecycle implementation

Date: 2026-09-20. Owner acceptance: D-025, “Chấp nhận, triển khai tại local chưa
merge”. Original dirty workspace: `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-prod-002-node-lifecycle`, HEAD
`467e2ad3ada305597b2d1808c03da6190d0643cb`. No commit, push, merge or live-host
activation. All earlier CLI/Web/semantic changes remain in this tree.

## Implemented boundary

- `VNextProductRuntimeDependencies::with_outbound_first` takes explicit trusted
  host ports: candidate gatherer, advertisement publisher, bounded public endpoint
  resolver and current revocable execution grant. No default port, key or network
  activation is invented. Missing feature/grant rejects before runtime artifacts.
- The normal `OneBrainNode` aggregate creates one private `OutboundFirstOwner`.
  It forwards the existing proof-checked session signer through the existing
  reachability signature domain, attaches the already-bound QUIC transport,
  composes the existing manager/reservation/dial/admission types, and uses durable
  redb replay state. No raw owner, signer, transport or store escapes the façade.
- `VNextProductServices::outbound_first_status` is an admitted, weak-handle local
  snapshot. It reports independent requested/active/kill/signer/generation fields,
  existing route/outbox counts, usable reservations and explicit limitations.
  Advertising intent remains pending, never published by this task. No REST/WS,
  numeric Base discriminator or new wire format was registered.
- One reachability worker is registered after existing rehydration/outbox phases,
  inside the eight-worker ceiling. Candidate maintenance uses the frozen 2 s
  cadence and 20 s deadline, capped 2/4/8/16/20 s failure backoff, missed-tick skip,
  current network generation and revocable grant. Stale epoch/generation results
  cannot become successful observations. Recoverable port failure stays degraded.
- Shutdown fences services, drains admitted leases, cancels and joins workers,
  closes ephemeral reservations/carriers, releases the shared network and closes
  stores. Drop also aborts workers and closes carriers. Quinn releases its UDP
  handle asynchronously; tests actually rebind the port within a 2 s bound.
- Rollback now includes newly created reachability replay and existing core route
  journal artifacts. Existing files and durable rollout decisions are retained.
  Failures after network construction explicitly shut it down. Route journal
  recovery now precedes socket binding. Replay recovery streams bounded validation
  using the existing host storage ceiling; corrupt rows and exhausted sequence
  arithmetic fail closed without deleting floors.

The existing network core continues to own its route journal, outbox and
authenticated route directory. This task does not create a competing store or
network runtime. Without outbound-first dependency injection there is no new
reachability replay store or worker; the pre-existing compiled network core's
route journal behavior remains unchanged.

## Validation

Commands run from `src` unless a Python command specifies a repository path.

| Check | Result |
|---|---|
| `cargo test --locked -q -p onebrain-node --features vnext-outbound-first --lib` | 219 passed |
| `cargo test --locked -q -p onebrain-node --features vnext-outbound-first --test vnext_node_runtime --test vnext_reachability_manager --test vnext_outbound_first_runtime` | 7 + 4 + 9 passed |
| `cargo check --locked -q -p onebrain-node --no-default-features` | Pass |
| `cargo check --locked -q -p onebrain-node --features vnext-network-runtime` | Pass |
| `python -m unittest scripts.ci.test_validate_obp_product_contract` | 6 passed; accepted D-025 gate and default-off mutations |
| `python scripts/ci/validate_vnext_contracts.py` | Pass, frozen authority pins unchanged |
| Scoped `rustfmt --check --config skip_children=true`, all nine changed/new Rust files | Pass |
| `git diff --check` | Pass |

Eleven new lifecycle tests cover default-off, request/grant preflight, pending
port cancellation, grant revocation, durable kill/re-enable and nonce replay,
candidate failure isolation, preservation during rollback, corrupt journal
rejection, journal recovery without live routes, stale epoch/generation results,
and drop cleanup. Two replay recovery tests cover corruption/resource limits and
sequence exhaustion. A new real-node integration test proves service access while
holding the node mutex; the TCP-bind failure test now exercises outbound-first
rollback and checks the actual dataset tree.

The first socket assertions assumed synchronous Quinn driver teardown and failed;
bounded real rebind checks now prove release rather than merely inspecting status.
Feature compilation also exposed five old Base adapter initializers missing the
working tree's new `validation_diagnostics` field; these now supply an empty list.

Package-wide `cargo fmt --check -p onebrain-node` still reports pre-existing KU
formatting in `ku_ollama.rs`, `ku_product.rs`, and `ku_product/review_jobs.rs`.
Those semantic-work files were preserved. Existing unused-code/import warnings
in feature-reduced builds remain; no build/test failure remains for this task.

## Remaining product work

OBP-PROD-003 owns trusted-source intake/discovery refresh, signed cache, PEX lease
integration, reservation scheduling and advertisement publication. The lifecycle
owner currently has no configured discovery sources or admitted live sessions in
its discovery registry; status explicitly reports orchestration pending.
OBP-PROD-004 owns automatic routing/failover and durable retry orchestration.
OBP-API-001 owns management capability/idempotency binding and public transport.
No end-to-end internet reachability, source discovery, platform qualification,
rollout or semantic model improvement is claimed here.
