# OneBrain vNext — Operator Runbook v1

> **Task:** `DOC-001`
> **Status:** Frozen foundation release runbook
> **Freeze date:** 2026-07-22

## 1. Operating principle

A node is useful without peers. Network connectivity increases the observed
frontier but never determines whether local KU/KQL work is valid. Operators
manage explicit local policies and kill switches; they do not coordinate a
global epoch or wait for a central controller.

## 2. Preflight

From the repository root, run the contract validator, then from `src/` run the
workspace checks and focused release gates:

```text
python scripts/ci/validate_vnext_contracts.py
cargo check --workspace
cargo test -p onebrain-node vnext_mixed_conformance --lib
cargo test -p onebrain-node vnext_security_suite --lib
cargo test -p ku-core qa006_ --lib
cargo test -p ku-net qa006_ --lib
cargo test -p onebrain-node qa007_ --lib
cargo test -p onebrain-node qa008_ --lib
cargo run --release -p onebrain-node --features vnext-soak-harness \
  --example dr_m5_soak_release -- --profile smoke \
  --output target/m5-07/soak-report.json
```

Record the code revision, configuration, platform and benchmark profile root.
Timing results are regression evidence for that environment, not a universal
throughput promise.

The smoke report validates the release binary and fault-cycle harness only; it
does not qualify a release. The scheduled `vNext soak and release gate`
workflow runs `nightly-24h` on a dedicated Linux runner labeled
`onebrain-soak`. Before canary or release, manually run `pre-release-72h` on
the same pinned runner class and retain its JSON artifact with commit SHA and
workflow metadata. Any `rollback_reasons` entry blocks release.

## 3. Safe enablement order

All additive vNext feature flags begin disabled. Enable one bounded lane at a
time:

1. canonical object/event validation and durable local storage;
2. inventory shadow calculation, with no payload or deletion side effect;
3. authenticated OBP reconciliation and persisted journal;
4. provider discovery and fidelity workflows after their validation gates;
5. checkpoint shadow planning, custody and restore drills;
6. local GC only after proof, parity, soak, operator approval and recovery gates;
7. reward evidence export independently, if desired, after knowledge commit.

Keep speculative RIBLT and remote cognition disabled unless their separate
profiles, benchmarks and operational controls are completed. Enable the legacy
adapter only for known mixed-version peers.

## 4. Status interpretation

`USABLE_OFFLINE` with `LOCAL_ONLY` coverage is healthy standalone operation.
An observed peer set upgrades reachability information, not completeness.
`PARTIAL`, an exact assessed frontier and explicit limitations are expected in
a distributed partition-tolerant network.

Fidelity status describes evidence that a source was encoded as claimed; it
does not vote on proposition truth. Consent remains denied or unconfigured
until a named auditable grant exists. The workflow view displays assumptions,
violated/unknown constraints and “Satisfied relative to…” scope.

## 5. Incident procedures

### Seed or peer outage

Keep local services available, retain pending journals/bundles and report the
frontier as partial or unavailable. Do not clear inventories or infer that
unreachable knowledge is absent. Reconcile when any bridge returns.

### Partition and reunion

Allow every island to accept locally valid objects/events and preserve causal
branches. On reunion, authenticate each bridge, exchange scoped inventories,
deduplicate canonical identities and retain conflicting variants. Compare the
result to deterministic reconciliation traces before increasing budgets.

### Malformed or adversarial payload

Stop the affected session or feature lane, retain bounded quarantine evidence,
and preserve unaffected branches. Check decompression limits, signature/context
binding, replay guards and durable admission logs. Never promote quarantine
because multiple carriers repeated the same bytes.

### Privacy or consent incident

Disable the affected disclosure lane, remote route and observation intake.
Preserve encrypted local audit evidence, revoke exact grants/permits and avoid
broadcasting stable private identifiers. Local deletion follows its own policy;
it cannot promise removal from remote custody.

### Checkpoint or GC incident

Activate the GC kill switch immediately. Keep canonical payloads, checkpoint
conflicts and protected anchors. Restore from exact archives, verify custody
receipts and rebuild the derived view root before considering another dry run.

### Fidelity dispute

Retain every encoding attempt, mismatch dimension and alternate. Recompute the
assessment under the named policy/frontier. Do not delete a KU or label it
false; lower use can follow naturally from poor task fit or fidelity evidence.

### Soak or performance regression

Inspect QUIC and fsync percentiles, then RSS/disk/task growth and the KQL/PoMV
incremental scan fields. A positive unbounded slope, non-zero active session
after shutdown, task leak, reunion-root mismatch or semantic amplification is
a rollback trigger. Do not loosen a frozen budget to make an existing report
green; create a reviewed profile revision and rerun the complete duration.

## 6. Rollback trigger and order

Rollback when a new lane changes validated sets across carriers, expands
authority/disclosure, loses a causal branch, cannot resume after crash, or
breaks restore parity. Disable the narrow feature first, stop new writes for
that namespace, preserve raw/journal/quarantine evidence, reopen the prior
reader and rebuild projections. The additive migration guide defines exact
storage rollback; no rollback step deletes legacy source rows.
