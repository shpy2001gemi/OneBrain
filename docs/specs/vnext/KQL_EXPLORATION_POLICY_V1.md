# KQL Exploration Policy v1

> **Task:** `KQL-010`  
> **Status:** Complete  
> **Depends on:** `KQL-003`, `KQL-009`

## 1. Purpose

`ExplorationPolicyV1` prevents eligible rare, old, cross-domain and opposing
KUs from being permanently hidden by exploitation ranking. Exploration only
creates a non-zero discovery/exposure opportunity. It does not relax typed
validation, materialize a Mapping, adopt an Assembly, assess truth, or create
benefit/reward/OBT.

The standard policy is immutable generic object kind `21`, major `1`, with
`LOCAL_ONLY` disclosure. Its ObjectCID is recorded with every selection.

## 2. Frozen policy values

| Field/profile | v1 value |
|---|---:|
| floor / urgent latency-bound | 10% |
| ordinary complement | 20% |
| open scientific / high uncertainty | 30% |
| adaptive ceiling | 40% |
| starvation window | 10 completed eligible opportunities |
| maximum exploit streak | 9 |

A stalled search remains at 20% before two unchanged revisions, rises to 30%
at revision two, then by five percentage points per unchanged revision until
the 40% automatic ceiling. A separate explicit pure-explore policy is outside
v1.

## 3. Eligibility boundary

The scheduler receives only facts needed to establish an eligible universe:

- structural decode succeeded;
- at least one compatibility path exists;
- privacy and consent allow consideration;
- the schema is supported; and
- the candidate fits current resource limits.

Popularity, source count, aggregate trust, PoMV, artifact age, realized use,
benefit, reward and OBT are not fields in the eligibility contract. Risk policy
may require stronger validation before action, but cannot rewrite discovery
eligibility through these signals.

If no exploration candidate is eligible, choosing exploitation does not create
a floor violation. Existing debt is preserved so that a later eligible
long-tail candidate receives an opportunity promptly.

## 4. Debt and cohort scheduler

Each completed opportunity with an eligible exploration lane accrues 1,000
basis points of floor debt. An exploration selection pays up to 10,000 basis
points. Projected debt reaching 10,000, or an exploit streak reaching nine,
forces exploration. Fractional quota therefore survives small result pages
instead of rounding to zero.

Exploration uses a rotating cohort cursor:

1. cross-domain/structural;
2. opposition/alternative; and
3. cold/old/low-exposure/long-tail.

When a batch produces at least three exploration slots and all three cohorts
have eligible candidates, rotation gives each cohort at least one slot. Missing
cohorts remain explicit in the result rather than being fabricated.

## 5. Seeded selection and propensity

Random draws use a domain-separated BLAKE3 stream bound to private seed, trace
ID, policy CID, current frontier digest and monotonic RNG counter. Bounded
rejection sampling avoids modulo bias. Candidate propensity is retained as an
exact reduced rational combining lane probability and within-cohort/pool
selection probability. Forced selections use their actual conditional
probability rather than pretending the configured fraction was used.

Every private selection record contains:

- monotonic selection ordinal and candidate commitment;
- selected lane/cohort and reason;
- exact propensity;
- policy ObjectCID;
- assessed local frontier digest; and
- RNG counter at the beginning of the selection.

## 6. Restart and partition behavior

`ExplorationState` serializes into a canonical `LOCAL_ONLY` snapshot intended
for an encrypted private-local backend. It includes debt, streak, cohort cursor,
seed, RNG counter and audit records. Decode rejects mixed policy records,
non-canonical bytes, broken ordinals and invalid counters.

A frontier change caused by a query revision, restart or network partition does
not reset debt. It changes the frontier committed into subsequent records.
Different nodes or partitions keep independent private traces; there is no
network exploration coordinator.

Batch updates are transactional in memory: validation, RNG or log failure does
not partially mutate the caller's state.

## 7. Exact/admin bypass

Exact CID fetch and administrative lookup use canonical candidate order. They
do not consume seeded RNG, exploration debt, exploit streak or completed
exploration opportunities. Their deterministic selection records have
propensity one and an explicit bypass reason.

## 8. Boundaries

The scheduler does not:

- expose private Need, seed, debt, propensity or standing-interest history;
- claim every KU in the network will be seen in one query;
- turn route distance, early arrival or provider count into semantic quality;
- learn negative evidence from an unexposed candidate;
- make a selected candidate executable/actionable; or
- introduce a Core DNA Gene or execution opcode.

## 9. Executable evidence

Seven tests prove:

- the 10/20/30/40 profile and private policy CID are stable;
- no ten-selection exploit streak occurs while exploration is eligible;
- debt survives canonical restart plus revision/partition frontier change;
- exact/admin lookup consumes neither RNG nor debt;
- three exploration slots cover all available cohorts;
- seeded replay preserves policy/frontier/propensity and round-trips the full
  private audit snapshot; and
- a failed batch leaves private state unchanged.
