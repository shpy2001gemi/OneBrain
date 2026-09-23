# OBP-QA-001 — local relay renewal correction

2026-09-23. Reviewable local correction on
`codex/obp-qa-001-nat-canary`, original working tree, base `fc65f08`.
No commit, push, merge, remote installation or signed P5 dispatch occurred.
Task 18 remains **Blocked** on independent-network and exact-candidate evidence.

## Defect and scope

The [retained blocker](OBP_QA_001_P5_RENEWAL_BLOCKER.md) showed expired relay
descriptors and a first-export-only runtime. The requested continuation addresses
that concrete D-002 defect using the existing canonical successor rule in
[outbound-first profile section 4](../../../specs/vnext/OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md).
No public command, wire object, signature domain or product API is added.
The task file records this bounded local correction scope.

Changed [runtime](../../../../src/onebrain-relay/src/runtime.rs),
[durable state](../../../../src/onebrain-relay/src/state.rs),
[lifecycle regression tests](../../../../src/onebrain-relay/tests/descriptor_lifecycle.rs)
and the [operator procedure](../../../operations/ONEBRAIN_OUTBOUND_FIRST_RELAY_GUIDE.md#existing-relay-descriptor-renewal).
Every product surface continues to use the existing node-owned service.

## Resulting behavior

- Existing `export-candidate-descriptor` creates sequence 1 or the explicitly
  configured contiguous successor. It verifies the stored canonical bytes and
  signature, same key/NodeID and signed endpoint/configuration fields. A successor
  embeds the exact stored predecessor digest and advances sequence by one.
  Gaps, rollback, key/config substitution and non-advancing validity reject.
- A REDb compare-and-swap commits the candidate and removes only its old lifecycle
  activation marker in one transaction. Other descriptor/control floors, consumed
  nonces, reservations, revocations and rendezvous records are preserved.
- Commit precedes publication. A synced private sibling and no-clobber hard link
  publish complete bytes; retained outputs are never overwritten. If publication
  fails after commit, equal-sequence export to a new path republishes the exact
  stored bytes. It never changes timestamps/signature or creates a fork.
- Activation checks the current signed descriptor/configuration/freshness and
  requires distinct source hosts and nonzero distinct transcripts per endpoint.
  Its durable marker binds both candidate and probe-set digests. Stale probes and
  old unbound activation markers cannot authorize normal startup.
- Serve uses the persisted candidate and retains the same exclusive database
  owner from activation validation through listener shutdown. Renewal cannot race
  a live relay process. Offline renewal/probe/activation ordering is documented.

## Verification

All Rust runs use `--offline --locked`. Stores, fixture keys and listeners are
temporary. These are local regression tests, including synthetic activation
summaries, not fresh remote possession or physical-host qualification evidence.

| Check | Result |
|---|---|
| Windows `cargo test -p onebrain-relay` | 21 unit + 8 lifecycle + 11 data-plane tests passed |
| Linux container, same relay suite | 21 unit + 8 lifecycle + 11 data-plane tests passed |
| Node outbound-first integration/matrix, five targets | 32 passed |
| P5 V1/V2 controller, P5 contract and local collector Python tests | 87 run: 86 passed, one existing Linux-only test skipped on Windows |
| Aggregate vNext contracts | Passed |
| Relay format and Git whitespace | Passed |

The nine added tests cover concurrent CAS with one winner, stale activation CAS,
real CLI process concurrency, reopen/replay protection of retained state, an
expired historical-format descriptor, publication failure/recovery, exact
automatic-time republication, invalid succession/key/configuration and per-endpoint
activation coverage. The existing lifecycle unit test additionally checks that
serve uses exact persisted signed bytes and holds the database lock.

Linux ran locally with network disabled, repository and existing dependency cache
mounted read-only, and a separate new target volume. Builder image:
`sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922`
(Rust 1.98.0). No old build target, immutable source checkout, upload artifact or
remote staging directory was replaced.

Final logs and reports, with exact argv, source SHA-256 and log commitments:

- `target/obp-qa-001/relay-renewal-check-01/windows-report.json`
  SHA-256 `ae406e7b2ccbd8de5099f0c4f920c3ff4622b86e37d0dd461a2ad0dc6998c4fc`.
- `target/obp-qa-001/relay-renewal-check-02/linux-report.json`
  SHA-256 `27af1f8a69edded7694101478885293f2ff8f0e3d9fad9857af4f5a373b84141`.

Both final reports have `source_unchanged=true`; their source and log hashes were
independently rechecked. Linux attempt 01 also passed, but a parent-directory sync
correction was made during that build. Its original report/log are preserved with
`linux-source-drift.json`; it is superseded by attempt 02 for final-source evidence.
No failed or superseded attempt is removed or relabeled.

## Remaining gate and limits

This dirty-tree correction cannot qualify `fc65f08`, be installed under its old
generation, or inherit its signed Base/Registry bindings. The current handoff
explicitly forbids Git commit/push/merge without owner direction. The next owner
step is review of this concrete correction and authorization to create a new
immutable candidate on the retained QA branch. No D-029, SSH, key-location or
September 23–30 policy-renewal approval is being reopened.

After a new candidate exists, rebuild and reverify the bundle and all exact-candidate
Base/Registry/P5 bindings; collect fresh host exports, descriptor chains and actual
remote possession probes; satisfy current topology/provider authority; only then
enter canonical signed bootstrap/prepare/session/fault/cleanup. Preserve both old
staging generations, existing identities/state and all historical runs. Do not
reuse old provider attestations as new measurements.

The local activation summary remains a trusted operator file; these tests do not
prove remote host identities, genuine possession transcripts or physical
independence. The canonical V2 verifier/controller still owns those gates.
Publication requires filesystem hard-link support. A crash may retain a private
temporary descriptor sibling; it cannot publish uncommitted bytes. Recovery must
use the committed sequence before advancing again. Key or endpoint changes and
automatic online renewal are outside this correction.

Two independent consumer networks/two relays and all ten product scenarios remain
outstanding. The separate P5 production-reference lane still requires three
physical Linux hosts, thirteen faults and ten exit oracles. Networking stays
default-off, D-024 tuning stays Deferred and OBP-MIG-001 remains unstarted.
