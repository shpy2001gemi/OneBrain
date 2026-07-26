# OneBrain vNext Runtime Lifecycle Profile v1

> **Work package:** DR-P2.3
>
> **Status:** Frozen and implemented — 2026-07-26
>
> **Code:** [`onebrain-node::vnext_product_runtime`](../../../src/onebrain-node/src/vnext_product_runtime.rs), [`onebrain-node::vnext_network_runtime`](../../../src/onebrain-node/src/vnext_network_runtime.rs), and [`onebrain-node::OneBrainNode`](../../../src/onebrain-node/src/node.rs)

## 1. Ordered startup

Runtime configuration and required dependency presence MUST be validated before
any product listener or durable subsystem store is opened.

An external identity signer MUST pass proof-of-possession before any durable
subsystem store is opened.

The fixed-size caller-owned Vault capability MUST be present before product
startup may continue.

Enabled lane stores MUST open before the authenticated QUIC listener starts.

Disabled lane stores MUST remain unopened throughout startup and recovery.

The authenticated QUIC listener MUST be active before private-needs are
rehydrated into the in-memory KQL target set.

Vault ciphertext and lifecycle invariants MUST be checked while private needs
are rehydrated.

The logical Public Use publication outbox MUST be inspected and a bounded drain
attempt MUST run before background workers start.

Missing authenticated routes during the startup drain MUST preserve the
publication as pending rather than mark it exported or fail startup.

Bounded lane workers MUST start only after store, listener, rehydration, and
logical-outbox recovery phases complete.

The aggregate MUST expose the completed startup phase trace, recovered private
need count, pending publication count, active worker count, and worker ticks.

The aggregate MUST enter `running` only after every required startup phase
completes.

## 2. Workers and recovery

Every active product lane MUST own exactly one bounded scheduling worker.

No disabled product lane MUST create a scheduling worker.

All product workers MUST share the aggregate cancellation source and configured
poll interval.

The publication worker MUST retry a bounded durable-outbox drain without
inventing an authenticated route.

Retryable route absence MUST NOT discard, duplicate, or falsely export a
durable publication.

Restart rehydration MUST NOT duplicate a durable KQL match, resurrect a
terminal private need, or advance a publication feed sequence twice.

## 3. Ordered shutdown

Shutdown MUST fence new typed operations before cancelling workers.

Shutdown MUST wait for cooperative worker cancellation before flushing safe
durable metadata.

Safe metadata flush MUST snapshot private-need, publication, and storage state
without claiming network completion.

The authenticated network MUST stop after workers and safe metadata are
settled.

Lane stores MUST close after the network stops.

The legacy TCP accept loop and integrated vNext aggregate MUST both be owned by
`OneBrainNode` and stopped by explicit node shutdown.

Drop MUST abort any remaining node-owned listener or product worker.

## 4. Partial startup rollback

A failed startup MUST close every owner opened by that startup attempt.

Rollback MUST remove only explicit vNext artifacts that did not exist before
the failed attempt.

Rollback MUST preserve every pre-existing file, including pre-existing vNext
artifacts.

A legacy TCP bind failure after successful vNext startup MUST invoke aggregate
rollback before returning the bind error.

Signer failure before store open and QUIC bind failure after store open MUST
both leave no new vNext artifact.

## 5. Executable evidence

Focused tests prove the exact startup and shutdown traces, one worker per active
lane, cancellation and store closure, signer and post-store bind rollback,
preservation of pre-existing artifacts, node-level TCP-bind rollback, durable
KQL restart idempotence, and Public Use publication replay safety.
