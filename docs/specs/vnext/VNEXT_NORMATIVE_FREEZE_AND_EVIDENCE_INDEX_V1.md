# OneBrain vNext — Normative Freeze and Evidence Index v1

> **Task:** `DOC-001`
> **Status:** Foundation release frozen
> **Freeze date:** 2026-07-22
> **Baseline:** KU v7.1

## 1. Meaning of this freeze

This is a documentation and conformance freeze for the vNext foundation. It is
not a network activation date, global epoch, truth checkpoint or promise that
every optional research lane is implemented. Future revisions remain additive
and require explicit schema/version, migration, downgrade and evidence changes.

Normative precedence remains:

1. founder directives in the research baseline;
2. architecture decisions in baseline sections 46.3 and 56.1;
3. vNext contracts in this directory;
4. legacy documents and current implementation behavior.

## 2. Machine-checked coverage

The [normative coverage manifest](normative_coverage.json) maps every current
uppercase requirement line to an executable evidence needle or an explicit
rationale. The dependency-free [contract validator](../../../scripts/ci/validate_vnext_contracts.py)
also checks 99 unique task IDs, dependency cycles, 18 ADR mappings, 37 negative
assertions, frozen vector counts and local Markdown links.

The semantic guardrails are indexed in the [Normative Vocabulary](NORMATIVE_VOCABULARY_V1.md),
[Negative Assertions Registry](negative_assertions.yaml), [Field Ownership Matrix](FIELD_OWNERSHIP_MATRIX_V1.md),
[Threat Model](THREAT_MODEL_V1.md) and [ADR Traceability Matrix](TRACEABILITY_MATRIX_V1.md).

## 3. Gate evidence

| Gate | Frozen evidence | Principal claim boundary |
|---|---|---|
| M0 | canonical, identity/object and feed/event vectors; cross-crate conformance | deterministic bounded identity, not semantic truth |
| M1 | KU/Receptor/Assembly/Mapping/KQL contracts and exact typed tests | proposal is not durable Mapping; unknown is not false |
| M2 | [Local Vertical Slice](LOCAL_VERTICAL_SLICE_PROFILE_V1.md) and [Additive Workflow Surface](ADDITIVE_KU_WORKFLOW_SURFACE_V1.md) | materialization is not adoption; resolution is relative |
| M3 | [Anti-Gravity Reunion Canary](ANTI_GRAVITY_REUNION_CANARY_V1.md), OBP state machine/journal and multi-bridge merge | partition/reunion without central authority or global completion |
| M4a | fidelity, capability, privacy, provider, PoMV and reward firewall profiles | fidelity is not truth; reward is downstream only |
| M5 | [Multi-Objective Benchmark](M5_MULTI_OBJECTIVE_BENCHMARK_V1.md) | no hiding aggregate score; privacy/consent and long-tail remain visible |
| M6 | checkpoint proofs, shadow compaction, restore drill, local GC and bounded models | no unseen-fork suppression or resurrection; deletion is local policy |
| M7 | mixed-version, security, property, scale and performance suites plus this release pack | bounded evidence, explicit assumptions and optional legacy gateway |

## 4. Release evidence index

| Evidence | Artifact |
|---|---|
| mixed versions and carriers | [QA-004 profile](MIXED_VERSION_CROSS_CARRIER_CONFORMANCE_V1.md) |
| adversarial probes and runtime guards | [QA-005 security suite](VNEXT_SECURITY_SUITE_V1.md) |
| algebraic and trace properties | [QA-006 property suite](ALGEBRAIC_AND_TRACE_PROPERTY_SUITE_V1.md) |
| 10k/100k logical-node simulation and 30B analytical bounds | [QA-007 scale profile](LOGICAL_NODE_SCALE_AND_ANALYTICAL_BOUND_PROFILE_V1.md) |
| correctness-coupled performance budgets | [QA-008 performance profile](PERFORMANCE_REGRESSION_BUDGET_PROFILE_V1.md) |
| optimized soak and release qualification | [M5-07 soak/release profile](SOAK_PERFORMANCE_RELEASE_GATE_PROFILE_V1.md) |
| single-host three-node real-QUIC canary preflight | [P5 canary preflight profile](P5_CANARY_PREFLIGHT_PROFILE_V1.md) |
| signer/disk/slow-peer drills, backup/restore, rollback and operator dashboard | [P5 operations preflight profile](P5_OPERATIONS_PREFLIGHT_PROFILE_V1.md) |
| portable outbound-only reachability, permissionless relays and capability-based platform adapters | [Outbound-first reachability profile](OUTBOUND_FIRST_REACHABILITY_PROFILE_V1.md) |
| three-host mixed-path and relay-failover production reference | [P5 multi-host production qualification V2](P5_MULTI_HOST_PRODUCTION_QUALIFICATION_PROFILE_V2.md) |
| source-free permissionless relay deployment, immutable units and NAT-free node operations | [Outbound-first relay operator guide](../../operations/ONEBRAIN_OUTBOUND_FIRST_RELAY_GUIDE.md) |
| concurrent pinned-SSH orchestration, partial evidence durability and V2 carry-forward | [P5 outbound-first preflight V2](P5_OUTBOUND_FIRST_PREFLIGHT_PROFILE_V2.md) |
| signed Registry release, capacity admission, truncated-index and disk-shortage qualification | [Concept Registry operations profile](CONCEPT_REGISTRY_OPERATIONS_PROFILE_V1.md) |
| operator-visible scope and consent | [Scoped Runtime Status](SCOPED_RUNTIME_STATUS_PROFILE_V1.md) |
| migration and legacy preservation | [Migration profile](ADDITIVE_MIGRATION_STORAGE_PROFILE_V1.md), [backfill profile](LEGACY_DATA_BACKFILL_PROFILE_V1.md), [operator guide](VNEXT_MIGRATION_AND_ROLLBACK_GUIDE_V1.md) |
| interoperable independent implementations | [Interoperability Profile](VNEXT_INTEROPERABILITY_PROFILE_V1.md) |
| operation and incidents | [Operator Runbook](VNEXT_OPERATOR_RUNBOOK_V1.md) |

The 30-billion-node statement is an analytical extrapolation with explicit
assumptions and zero global-population coefficient in local state bounds. It is
not a 30-billion-node execution result.

## 5. Open optional lanes

`RUN-003` remote cognition remains optional and default-off; it does not block
the local knowledge-plane release. `RIB-001` and `RIB-002` remain unimplemented,
optional and default-off because no reproducible benchmark currently justifies
placing a speculative decoder in the trusted reconciliation path. Deterministic
radix/Merkle reconciliation is the required fallback and release path.

OBT/reward export is post-commit and independently disableable. None of these
lanes is a correctness dependency for KU, KQL, OBP, OBKG, PoMV, local AI,
partition operation, reunion or migration.

## 6. Reproduction commands

```text
python scripts/ci/validate_vnext_contracts.py
python -m unittest scripts.ci.test_validate_vnext_outbound_reachability scripts.ci.test_validate_vnext_p5_multi_host -v
python -m unittest scripts.runner.test_onebrain_p5_multi_host_v2 scripts.release.test_validate_evidence_carry_forward -v
cargo fmt --all -- --check
cargo check --workspace
cargo test -p ku-core qa006_ --lib
cargo test -p ku-net qa006_ --lib
cargo test -p onebrain-node run002_ --lib
cargo test -p onebrain-node vnext_security_suite --lib
cargo test -p onebrain-node qa007_ --lib
cargo test -p onebrain-node qa008_ --lib
cargo test -p onebrain-node --features vnext-canary-harness p5_01_ --lib
cargo test -p onebrain-node --features vnext-canary-harness vnext_p5_operations --lib
```

Successful execution establishes the scoped release evidence listed here. It
does not establish proposition truth, universal reachability or final global
closure.
