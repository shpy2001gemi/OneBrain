# Research: Penalty & Slashing Mechanisms

> Researched: 30/06/2026 | Sources: Ethereum 2.0, Cosmos, EigenLayer, Helium

## Comparison Table

| Dimension | Ethereum 2.0 | Cosmos | EigenLayer | Helium | **OBT** |
|-----------|-------------|--------|-----------|--------|---------|
| **At stake** | 32 ETH staked | Bonded tokens | Allocated Unique Stake | Reward stream | Trust Score |
| **Lightest** | Inactivity leak | 0.01% slash + 10min jail | Per-AVS configurable | Warning/flagging | Trust reduction |
| **Medium** | ~1 ETH slash | N/A | Partial stake slash | Denylist | Trust slash + temp ban |
| **Heaviest** | Correlation penalty (100%) | 5% slash + Tombstone | Full stake loss | Permanent denylist | Trust zero + Tombstone |
| **Correlation** | ✅ More slashings = harsher | ❌ Fixed | ❌ Per-AVS | ❌ Per-device | ✅ Adopted |
| **Appeals** | None (deterministic) | Governance | ✅ Veto Committee | ✅ Community review | ✅ 4-layer appeal |
| **Permanent ban** | Forced exit | Tombstoning | Depends | Permanent denylist | Tombstone |

## Key Findings

### Ethereum 2.0 Slashing
- **3 offenses**: Double Proposal, Double Vote, Surround Vote
- **Correlation Penalty** (Day 18 of 36-day exit): scales with simultaneous slashings
  - Isolated incident → small penalty
  - Mass slashing → lose ALL staked ETH
- **Design Principle**: Isolated accidents forgiven; coordinated attacks harsh

### Cosmos Tombstoning
- **Two-tier**: Downtime (0.01% slash, temp jail) vs Double-sign (5% slash, permanent Tombstone)
- **Tombstone** = permanent ban, can never rejoin with same key
- **Governance-adjustable** parameters

### EigenLayer
- **Programmable slashing** per AVS
- **Veto Committee** — reputation-based, not stake-weighted
- **Intersubjective faults** — for things code can't prove (oracle manipulation, AI quality)

### Helium Denylist
- **Automated classifiers** + community reporting + appeal process
- Gaming types: location spoofing, clustering, DC farming, hardware manipulation

## Critical Insight: OBT vs Trust Separation

```
OBT (G-Counter, increment-only) = reward for past value → NON-PUNITIVE
Trust (PN-Counter, can decrease) = current reputation → CAN BE SLASHED

"We don't take back past salary. We revoke your medical license."
```

## Proposed 5-Tier Penalty System

| Tier | Name | Trigger | Trust Formula | Duration |
|------|------|---------|--------------|---------|
| 0 | Natural Decay | Low quality, offline | Organic decay | Continuous |
| 1 | Warning | 1 antibody, suspicious pattern | No trust reduction | 90 days |
| 2 | Trust Reduction | ≥2 antibodies, conf>0.7 | trust × (1-severity×0.3) | Permanent |
| 3 | Jail | Collusion ≥3 nodes, isolation | trust × 0.2 | 7-30 days |
| 4 | Trust Zero | Proven fraud + economic gain | trust = 0.001 | 180 days |
| 5 | Tombstone | Systematic ring leader, forgery | trust = 0 | PERMANENT |

## Correlation Penalty

```
multiplier = 1 + log₂(simultaneous_nodes_penalized)
1 node: ×1.0    4 nodes: ×3.0    16 nodes: ×5.0
```

## 4-Layer Appeal Process

1. **Auto**: Quarantine ≥2 antibodies + conf>0.7 (reduce false positives)
2. **Dispute Window**: 48h before slash, submit counter-evidence
3. **Retrospective**: Appeal within 30 days, K random high-trust evaluators
4. **Tombstone Appeal**: >80% top-tier nodes + cryptographic evidence
