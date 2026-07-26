# OneBrain vNext Runtime Concurrency Profile v1

> **Work package:** DR-P2.5
>
> **Status:** Frozen and implemented — 2026-07-26
>
> **Code:** [`onebrain-node::vnext_product_runtime`](../../../src/onebrain-node/src/vnext_product_runtime.rs) and [`onebrain-api::server`](../../../src/onebrain-api/src/server.rs)

## 1. Cloneable service boundary

`VNextProductServices` MUST be a cloneable, `Send + Sync + 'static` typed
handle.

The service handle MUST NOT borrow `VNextProductRuntime`,
`OneBrainNode`, or the aggregate `Arc<Mutex<OneBrainNode>>`.

API, CLI and Desktop integrations MUST snapshot the service handle while
holding the aggregate node mutex only briefly.

The aggregate node mutex MUST be released before a service performs a network
wait.

The aggregate node mutex MUST be released before a service scans or mutates a
Redb-backed product store.

The aggregate node mutex MUST be released before a service invokes a caller
owned signer.

The aggregate node mutex MUST be released before a service materializes a
distributed view.

A background product worker MUST own only the lane-specific handles and
cancellation state required for its bounded task.

## 2. Operation leases

Every typed service operation MUST acquire an admitted in-flight lease before
accessing a runtime subsystem.

The lifecycle gate MUST be held only while admitting or releasing a lease and
MUST NOT remain held during network, storage, signer or materialization work.

A successfully admitted operation MUST keep the owning subsystem core alive
until that operation returns.

Cloneable service handles MUST retain only weak ownership and MUST NOT prolong
listener or durable-store lifetime after aggregate shutdown.

## 3. Shutdown interaction

Shutdown MUST fence admission before waiting for in-flight service leases.

A service request arriving after the fence MUST fail with the typed `Stopped`
error.

An operation admitted before the fence MUST be allowed to drain before its
subsystem is stopped or closed.

Background workers MUST be cancelled and joined before their network and
publication handles are released.

The network listener MUST stop only after admitted service work and background
workers have drained.

Durable KQL, publication and PoMV stores MUST close after all admitted service
work has drained.

An already cloned service handle MUST observe the stopped network snapshot and
MUST reject further product operations after shutdown.

## 4. Scope

This concurrency boundary MUST NOT change wallet state, OBT state, validation
rules, authority decisions, incremental cursor semantics or network-completion
claims.

Legacy short local node operations MAY continue to use the aggregate mutex;
new vNext product endpoints MUST cross the cloneable service boundary.

## 5. Executable evidence

Focused tests hold aggregate owner mutexes while independent service handles
complete authenticated QUIC connection, Redb status inspection, caller-owned
signing and PoMV materialization. Additional tests prove the static handle
traits, admission fencing, in-flight draining, stopped-handle behavior and
ordered listener/store shutdown.
