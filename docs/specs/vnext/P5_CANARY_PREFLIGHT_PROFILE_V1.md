# OneBrain vNext — P5 Canary Preflight Profile v1

> **Work package:** `P5-01`
>
> **Status:** Frozen single-host implementation preflight
>
> **Machine contract:** [`p5-canary-preflight-v1.json`](../../../src/test-vectors/vnext/p5-canary-preflight-v1.json)
>
> **Executable:** [`onebrain-node::vnext_canary_operations`](../../../src/onebrain-node/src/vnext_canary_operations.rs)

## 1. Scope and topology

The preflight MUST create three independent logical nodes with distinct durable
directories and restart-stable full-width principals.

The nodes MUST exchange one canonical FeedInception on every edge of an
authenticated real-QUIC loopback ring and retain at least six authenticated
route observations.

This is deliberately a one-host preflight. Its report MUST keep
`production_canary_qualified=false` and cannot replace multi-host canary
evidence.

## 2. Partition, restart and reunion

The harness MUST stop node B, hold its former UDP address, and prove that the
stale route cannot establish a new authenticated session.

Node B MUST restart from the same durable directory with the same principal but
a new reachable address, and node A must advance the authenticated route
generation to that address.

Replaying the same signed FeedInception after reunion MUST leave exactly one
durable feed branch while recording the already-present acceptance path.

## 3. Safety and qualification boundary

A complete run MUST quiesce every session, stop all three runtimes cleanly, and
report no wallet, OBT, authority, truth, Benefit, or network-completion
amplification.

A non-empty operator node directory MUST fail before any runtime starts and
must preserve every existing byte.

Production rollout remains blocked until the pinned `pre-release-72h` artifact,
multi-host canary, backup/restore, fault, rollback and operator gates pass.

## 4. Production-profile relationship

This single-host profile MUST always emit `multi_host_qualified=false` and MUST
NOT be relabeled as production evidence.

The distinct production contract is
[`P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md`](P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V1.md);
it requires three physical Linux hosts, candidate-bound signed receipts and an
owner-approved role-separated trust policy.
