# OneBrain vNext — Performance Regression Budget Profile v1

> **Task:** `QA-008`  
> **Status:** Complete  
> **Code:** [`onebrain-node::vnext_performance_budgets`](../../../src/onebrain-node/src/vnext_performance_budgets.rs)  
> **Regenerator:** `cargo run -p onebrain-node --example qa008_performance_report --quiet`

## 1. Versioned workload and budgets

The v1 profile root is
`88588a430487b9ada73db52a9f74b9bcdbabdcf8efc7e3b99449a5cfae40044c`.
Changing a workload size or threshold changes this root and requires review.

| Metric | Fixed workload | Regression ceiling |
|---|---:|---:|
| bytes per representative new canonical object | one canonical public object fixture | 4,096 bytes |
| inventory update | insert 4,096 exact leaves | 2,000,000 µs |
| inventory diff | first divergent prefix over 4,096 vs 4,095 leaves | 2,000,000 µs |
| duplicate bridge overhead | 10 bridges × 1,000 replays of one payload | 5,000,000 µs; one logical variant |
| hot provider observation | merge 100,000 LeaseCID hints into cap 4,096 | 10,000,000 µs; retained ≤ 4,096 |
| inventory restore | canonical snapshot with 4,096 leaves | 2,000,000 µs; exact snapshot parity |

Ceilings are intentionally broad debug-build regression gates. They are not
latency promises, SLOs or cross-machine comparisons.

## 2. Current reproducible sample

One local debug run on 2026-07-22 reported:

| Metric | Measured |
|---|---:|
| object bytes | 57 bytes |
| inventory insert 4,096 | 14,601 µs |
| inventory diff | 826 µs |
| duplicate ingest 10,000 | 189,881 µs |
| hot provider merge 100,000 | 395,148 µs |
| restore 4,096 / 171,950-byte snapshot | 13,818 µs |

Re-running on another machine will produce different wall-clock values. The
checked JSON output is regenerated rather than treated as a canonical semantic
object.

## 3. Optimization discovered by the gate

The first QA-008 run exposed a roughly 18.9-second inventory diff. The old
implementation recomputed Merkle subtrees at every descent even though both
inputs were already exact, canonical, sorted leaf sequences.

`first_divergent_prefix` now compares the exact left/right leaf slices to choose
the same lexicographic branch, eliminating repeated hashing. All existing
inventory root/order/restart/collision/coverage tests pass after the change.
The optimization changes neither inventory roots nor the returned divergence
contract.

## 4. Correctness-coupled gates

Performance alone cannot pass the suite. The same run also requires:

- object decode reproduces the encoded ObjectCID;
- inventory diff actually finds the missing canonical leaf branch;
- snapshot restore reproduces the exact canonical snapshot;
- 10,000 physical duplicate deliveries retain one logical payload variant;
- bridge observations grant no authority; and
- the hot provider view never exceeds its configured retained bound.

Thus an optimization that skips CID validation, changes roots, collapses
variants, enlarges provider state or grants authority fails even if it is
faster.

## 5. Limits and next calibration

This is a deterministic micro-regression profile, not a network capacity test.
It does not measure disk fsync, real QUIC congestion, radio energy, WAN latency,
allocator RSS or archive download time. Release CI should add a pinned-hardware
optimized-build lane and establish p50/p95 baselines before tightening these
portable debug ceilings.

