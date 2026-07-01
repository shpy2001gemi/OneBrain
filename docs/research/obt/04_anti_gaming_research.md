# Research: Anti-Gaming & Rate Limiting for Fee-less Networks

> Researched: 30/06/2026 | Sources: Nano, IOTA, Helium, academic papers

## Comparison Table

| Dimension | Nano (XNO) | IOTA 2.0 | Helium |
|-----------|-----------|----------|--------|
| **Core Mechanism** | Balance-bucket priority + PoW | Mana-based DRR scheduler | Proof-of-Coverage + Denylist |
| **Rate Limiting** | 63 balance buckets, round-robin, LRU | Deficit Round Robin, quantum ∝ mana | PoC challenge frequency limits |
| **Sybil Resistance** | High cost (must hold significant Nano) | Mana tied to holdings | Hardware authentication (ECC/RSA) |
| **Spam Cost** | Computational micro-PoW | Adaptive PoW scales with rate | Economic ($300-500 hardware) |
| **Gaming Detection** | N/A (currency only) | N/A (DAG ledger) | Automated classifiers + scorecards |
| **Punishment** | De-prioritization (low bucket) | Throttled throughput | Denylist (rewards stopped) |
| **Recovery** | Automatic (hold more Nano) | Automatic (more mana) | Manual review with evidence |

**Key Insight**: All use a **resource proxy** instead of fees.
- Nano: balance
- IOTA: mana
- Helium: hardware
- **OBT: Trust (EigenTrust × NodeTier)** ← already built!

## Proposed Rate Limits (Trust-Gated)

```
MAX_KU_PER_HOUR by NodeTier:
  Leaf (T0):        1
  Contributor (T1): 5
  LocalSP+ (T2+):   10

MAX_ENCODINGS_PER_HOUR:
  Leaf:        2
  Contributor: 5
  LocalSP+:    10

COOLDOWN between claims:
  Leaf:        60 min
  Contributor: 12 min
  LocalSP+:    6 min
```

## Global Emission Formula

```
E(epoch) = B × A(epoch) × Q(epoch)

B = base_emission (governance parameter, initial=10,000 OBT)
A = min(active_nodes / 1000, 10.0)  — scales with network
Q = avg_network_pomv_score (0→1)    — quality gates

Per-node cap:
  max_node_reward = E(epoch) / active_nodes × TrustMultiplier
  TrustMultiplier: Leaf=0.1, Contributor=0.5, LocalSP+=1.0
```

**"Near-infinite but flow-controlled"**: No hard total cap (not Bitcoin 21M), but per-epoch cap exists. Like a river — no total water limit, but flow rate controlled.

## KU Quality Gates (4 levels)

1. **Min size**: ≥256 bytes (~50 words), ≥2 genes
2. **Content validation**: Encoding Consensus 3+ AI verify
3. **PoMV threshold**: ≥0.01 after 7 days, ≥0.05 after 30 days
4. **Encoding complexity**: min 100ms encode time, ≥1 bond

## 4 Gaming Pattern Detectors

### Pattern 1: Isolation Attack
- ≥3 nodes offline/online simultaneously within 30s
- Response: elevated scrutiny, 2× witnesses

### Pattern 2: Burst Spam
- >2× tier rate, sizes near minimum, similarity >0.8
- Response: warn → throttle → trust slash (progressive)

### Pattern 3: Circular Transfer (Wash Trading)
- Transfer loop A→B→C→A within 1 epoch, same subnet
- Response: PoMV discounted by isolation_factor

### Pattern 4: Trust Farming (Long Con)
- High trust but low KU quality divergence >0.3
- Response: alert + audit
