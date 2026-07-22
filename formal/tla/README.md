# OneBrain vNext TLA+ models

Each model has an adjacent TLC configuration:

- `FeedCheckpoint.tla` / `.cfg`
- `ReceptorResolution.tla` / `.cfg`
- `ProviderLease.tla` / `.cfg`
- `PermitRevocationTask.tla` / `.cfg`
- `ReconciliationSession.tla` / `.cfg`

With a local `tla2tools.jar`, run from this directory:

```text
java -cp tla2tools.jar tlc2.TLC -config FeedCheckpoint.cfg FeedCheckpoint.tla
```

Repeat for the remaining module names. The default repository CI runs the
bounded Rust mirror in `onebrain-node::vnext_m6_model`; TLC is an independent
formal verification lane and is not a runtime dependency.

