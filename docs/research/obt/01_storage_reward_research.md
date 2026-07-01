# Research: Storage Reward Mechanisms

> Researched: 30/06/2026 | Sources: Filecoin, Arweave, Sia

## Comparison Table

| Dimension | Filecoin | Arweave | Sia |
|-----------|---------|---------|-----|
| **Proof** | Proof-of-Spacetime (WinningPoSt + WindowPoSt) | SPoRA (Succinct Proofs of Random Access) | Merkle Proofs within contract windows |
| **Challenge** | Random parts of sealed data, strict time limits | Mining hash selects random recall chunks | Network requests Merkle proof of segment |
| **Data Unit** | 32/64 GiB sectors | 256 KB chunks | File shards (erasure-coded) |
| **Storage Model** | Deal-based (clients pay providers) | Endowment (one-time fee → 200yr storage) | Contract-based (renter deposits, host collateral) |
| **Reward Source** | Block rewards + deal fees | Block mining rewards | Renter payment on successful proof |
| **Penalty** | Fault fee: ~3.51 days rewards/day; Termination: 90 days cap | No slashing, miss mining opportunities | Collateral slashing |
| **Collateral** | Yes (FIL pledge per sector) | No | Yes (Siacoins locked) |
| **Sealing Cost** | Very high (GPU, SNARKs) | High (RandomX packing) | Low (encryption + erasure coding) |
| **Small Objects** | Poor (aggregate into 32GB sectors) | Good (native 256KB chunks) | Good (flexible sizing) |
| **Complexity** | Very high | Medium | Low |

## Recommendation for OBT

**Sia-inspired Merkle + Arweave random recall, simplified for KU objects.**

Filecoin too heavy (32GB sectors, GPU sealing) for 16-172 byte KU wire format.

## PoS-KU Challenge Protocol

```
Per epoch:
  1. challenge_seed = BLAKE3(epoch || node_id) → deterministic
  2. Select 5-10 random KUs from stored set
  3. Three challenge types:
     Type A: FULL HASH — return BLAKE3(wire_bytes) of CID X
     Type B: BYTE RANGE — return bytes[offset..offset+len]
     Type C: FIELD EXTRACT — return GeneType + first ConceptID
  4. Response within 30 seconds
  5. K=3 witnesses verify (DHT-selected)
```

## Storage Factor Formula

```
storage_reward(node, epoch) = Σ per stored KU:
    base_rate × size_w × rarity_w × demand_w × duration_f × trust_f

Where:
  base_rate = 0.001 OBT/KU/epoch
  size_w = clamp(wire_bytes/1024, 0.1, 10.0)
  rarity_w = clamp(20/actual_replicas, 0.5, 3.0)
  demand_w = clamp(metabolism/median, 0.1, 5.0)
  duration_f = min(epochs_stored/100, 2.0)
  trust_f = eigentrust_score (0-1)
```

## Anti-Gaming Storage (5 layers)

1. size_weight floor — tiny KU earn 1/10 vs 1KB KU
2. Challenge scales with KU count
3. Max 10 OBT/node/epoch cap
4. KU must be FULL encoding status
5. demand_weight — unused KU → reward ≈ 0
