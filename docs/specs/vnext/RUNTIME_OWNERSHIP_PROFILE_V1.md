# OneBrain vNext Runtime Ownership Profile v1

> **Work package:** DR-P2.1
>
> **Status:** Frozen and implemented — 2026-07-26
>
> **Code:** [`onebrain-node::vnext_product_runtime`](../../../src/onebrain-node/src/vnext_product_runtime.rs) and [`onebrain-node::OneBrainNode`](../../../src/onebrain-node/src/node.rs)

## 1. Aggregate ownership

`OneBrainNode` MUST own at most one active `VNextProductRuntime`.

The aggregate MUST be the sole product owner of `VNextNetworkRuntime` and of
each enabled `DistributedKqlRuntime`, `PublicUseEvidencePublisher`, and
`DistributedPomvRuntime`.

The aggregate MUST transitively own the authenticated route directory through
its network runtime; a second product route authority is forbidden.

The aggregate MUST NOT expose a public getter returning a raw subsystem
runtime reference.

Product/API integration MUST use `VNextProductServices`, whose fields remain
private and whose methods accept typed domain requests.

## 2. Caller-owned dependencies

An active aggregate MUST require a caller-owned `LocalNeedVaultKey` and an
already validated immutable `LocalPolicyRegistry`.

Missing Vault or Policy dependencies MUST fail before the identity file,
validated network store, or network listener is created.

An injected `SessionIdentitySigner` MUST be forwarded through the aggregate
without exporting private-key bytes or silently falling back.

The Vault key MUST be consumed by the encrypted private store rather than
retained as inspectable product configuration.

The policy registry MUST remain aggregate-owned and callers may select only
its typed allow-listed versions through the façade.

## 3. Cancellation and worker ownership

The aggregate MUST own one cancellation source shared by all future product
background workers.

The product worker registry MUST reject registration beyond eight concurrent
owned tasks.

Aggregate shutdown or drop MUST signal cancellation and abort every remaining
owned product task.

P2.1 MUST NOT start KQL, publication, or PoMV polling loops merely because the
aggregate exists; concrete lane flags, budgets, and polling policy belong to
P2.2 and P2.3.

## 4. Runtime truth

The aggregate MUST enter `running` only after all four durable/runtime owners
open successfully and the authenticated listener starts.

Its typed status MUST report signer mode, route count, active private needs,
durable matches, pending publications, policy versions, worker bounds, and
cancellation state separately.

The integrated status MUST keep wallet mutation, OBT mutation, and
network-completion claims false.

Operations through a stopped façade MUST fail closed rather than reviving a
subsystem.

When vNext is disabled by default, `OneBrainNode` MUST NOT require product
dependencies or create any vNext database, listener, or worker.

## 5. Deferred lifecycle work

This profile freezes ownership, dependency injection, façade access, bounded
worker registration, and owner cancellation. P2.2 adds independent product
lane flags, optional lane owners, and budgets. P2.3 adds the complete startup,
drain, shutdown, and
partial-start rollback protocol. P2.4 replaces full scans with durable
incremental cursors, and P2.5 removes long work from the legacy global node
lock.

## 6. Executable evidence

Focused tests prove:

- both peers open every aggregate-owned durable store and authenticate over
  real QUIC through the typed façade;
- status exposes every owned subsystem without truth, reward, wallet, or
  global-completion promotion;
- the ninth product worker is rejected and owner shutdown records
  cancellation;
- missing Vault/Policy dependencies create neither identity nor validated
  store side effects; and
- the existing feature-disabled build and safe-default tests remain green.
