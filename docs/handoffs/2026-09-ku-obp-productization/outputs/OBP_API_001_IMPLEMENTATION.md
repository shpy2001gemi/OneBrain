# OBP-API-001 — Local implementation evidence

2026-09-22. Original workspace `C:/Users/shpy2/Documents/OneBrain`, branch
`codex/obp-api-001-network-api`, baseline `836af91`. D-029 accepts the complete
[REST/WS transport](../../../specs/vnext/OBP_LOCAL_API_PROFILE_V1.md).
Task 004 is merged under D-028. This API task is merged as `7d37a30` under D-030.
Implementation commit `de04a57` is pushed to `origin/codex/obp-api-001-network-api`.
Reviewed handoff tip `c4aafb9` and the complete implementation are included in that merge.

## Implementation

The additive API feature `vnext-outbound-first` registers six loopback routes
and maps all 13 accepted operations to `VNextProductServices`. The node owns a
bounded redb command journal, durable dataset identity, fresh process fence,
revocable host control, scoped short-lived management grants and typed input
references. HTTP never mints host authority or accepts raw addresses, paths or
source bytes. API server creation installs no OBP binding automatically.

Mutation admission is serialized, bounded at 4096 records/16 MiB, and committed
before effects. Known exact replay precedes the old-generation rejection;
principal/payload conflicts cannot redispatch. Reopen converts unfinished
admissions to `reconcile_required`. Metadata reconciliation never executes an
effect. Source policy and requested configuration survive restart; missing host
source callbacks are projected unavailable/disabled. No live session is restored.

Sources reuse discovery admission and replay floors; disable removes affected
relay candidates and closes reservations/routes. Routing uses the existing
expected-peer connector; intent retry wakes the existing outbox without resetting
payload, counters or checkpoint. Read pages are immutable, bounded and scoped to
principal, process/dataset, generation and query kind with keyed continuation
tokens. Strict JSON rejects duplicates, unknown fields, null optionals and limits
bytes/depth before dispatch. Persisted outcomes are checked against the parent DTOs.

OBP WebSocket tickets and vocabulary are isolated from the older profile while
sharing its ticket/session ceilings. Only readiness and aggregate network hints
are emitted; queue overflow disconnects the slow session. Targeted REST refresh
and initial subscription generate hints; there is no autonomous notification worker.

## Verification

Executed offline with locked Rust dependencies and temporary data directories:

```powershell
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --lib -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-node --features vnext-outbound-first --test vnext_node_runtime --test vnext_reachability_manager --test vnext_outbound_first_runtime --test vnext_relay_matrix_live -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-api --features vnext-outbound-first --lib -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-api --features vnext-outbound-first --test base_contract -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p onebrain-api --no-default-features --lib -q
cargo test --offline --locked --manifest-path src/Cargo.toml -p ku-net --features outbound-first --lib -q
python -m unittest scripts.ci.test_validate_obp_local_api scripts.ci.test_validate_obp_product_contract
python scripts/ci/validate_vnext_contracts.py
git diff --cached --check
```

- Node library with `vnext-outbound-first`: final full rerun 252 passed, including
  ten façade tests and capacity/expiry checks. The 4096-record ceiling rejects a
  new kill command without changing the network generation.
- Node integration targets `vnext_node_runtime`, `vnext_reachability_manager`,
  `vnext_outbound_first_runtime`, `vnext_relay_matrix_live`: 22 passed.
- API library with `vnext-outbound-first`: 44 passed, one existing opt-in test
  ignored. Real loopback WebSocket test checks endpoint separation, single-use
  ticket consumption, readiness/status and private-field exclusion.
- API library with `--no-default-features`: 16 passed. Existing feature-off
  unused/dead-code warnings remain.
- API `base_contract` integration with `vnext-outbound-first`: eight passed.
- `ku-net --features outbound-first --lib`: 315 passed.
- Python OBP parent/transport unit tests: 19 passed. Aggregate vNext contract
  validator passes with six routes, 13 operations and 35 transport fixtures.

Node façade tests exercise all operation names, exact replay, conflict, stale
generation, expired/revoked capabilities, source intake/toggle/refresh, paging
binding/tampering, pending-intent retry and policy restart. Real subprocesses exit
at four journal boundaries: before admission, after admission, after owner effect
and after result commit. Recovery never blind-replays unknown effects. The
connected-carrier/checkpoint/relay evidence remains the existing task-004 tests;
the new façade route test exercises a bounded path-limited attempt.

## Limits

- These are local integration results, not Internet/NAT or platform qualification.
  No live OneBrainLocal host, model, Registry, jobs or rollout settings changed.
- A host must explicitly supply principal/control/management and discovery ports.
  The API cannot rehydrate callbacks or authority from cached bytes.
- Cross-store effects are not atomic with command completion. Unknown outcomes
  remain for explicit future resolution, with no automatic retry/eviction.
- An acknowledged intent exposes a checkpoint only when the existing owner retains
  that exact intent's checkpoint. An older acknowledged intent whose checkpoint
  was superseded returns `dependency_unavailable`; no historical proof is invented.
- Notifications are hints, not a reliable event stream. Refetch REST after gaps.
- The journal has no compaction/reset endpoint. At capacity REST commands fail
  before effects; the trusted host retains its existing emergency kill control.

The prior transport-review report is a historical specification checkpoint;
`handlers_registered=true` now records registration, not deployment or rollout.

Development corrections: the first API test run had one redaction-test failure
because it searched for `PRIVATE`, which is also in the public profile name.
The test now checks the injected private value `PRIVATE_ADDRESS`; full reruns
pass. Final strict GET/session-shape checks passed in both feature modes.
The final full API rerun also passed after tightening WebSocket shutdown:
overflow retains its admission slot until actual socket exit, sends have bounded
deadlines, and a test verifies slot release only after receiver teardown.

## Continuation verification — 2026-09-22

Reopened the original workspace at `bcca300` with a clean working tree.
`git ls-remote` confirmed the API branch at `bcca300` and main at `836af91`;
the published `2c39117` merge is an ancestor of main. Implementation `de04a57`
remains intact and unmerged. D-029 transport approval remains satisfied.

Fresh offline/locked reruns of the commands above passed: node 252 unit and
22 integration tests; API 44 unit (one existing real-Ollama opt-in test ignored),
eight Base integration tests and 16 feature-off tests; ku-net 315 tests.
The 19 Python contract
tests and aggregate vNext validator also passed. Feature-off builds retain the
existing unused/dead-code warnings.

Corrected the stale current `Next action` in PROGRESS, which still requested
transport review and implementation. OBP-API-001 remains Review pending owner
merge direction under D-010/D-029; dependent CLI/Web tasks remain Planned.
This continuation changes handoff documentation only. No model inference,
live-host change, network activation, rollout or qualification was performed.


## Merge closure — 2026-09-22, D-030

The owner requested "merge và push tất cả" after the fresh continuation checks
above. Fetch confirmed clean synchronized API/main tips before merge. Reviewed
API tip `c4aafb9` merged without conflicts as
`7d37a3066a561a38867352b3fab2e08387940ba6`; runtime content is unchanged.
Main and the retained API branch are published. The handoff advances to
OBP-CLI-001, Planned; no dependent implementation began during closure.
Earlier pending-merge statements in the dated verification section are historical.
