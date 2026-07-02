# Temporal, Streaming & Dynamic Knowledge Graphs — Survey for ONKG

> **Author**: OneBrain Research Team  
> **Date**: 2026-07-02  
> **Purpose**: Survey temporal KG models, event sourcing, streaming algorithms, and decay models for ONKG

---

## Executive Summary

This survey covers temporal knowledge graph models, event sourcing patterns, streaming graph algorithms, graph versioning, temporal query languages, knowledge evolution patterns, decay/reinforcement models, and causal reasoning. OneBrain's temporal model is **richer than standard TKGs** because it combines point timestamps, continuous decay functions, discrete state transitions, and causal ordering (VectorClocks).

> [!IMPORTANT]
> **Key Design Decisions:**
> 1. Adopt event sourcing for all bond lifecycle operations
> 2. Use VectorClocks (existing) as primary ordering mechanism
> 3. Implement snapshot + delta graph versioning
> 4. Calibrate decay per RelationType (Categories B, F, G never decay)
> 5. Unify decay framework via a `Decayable` trait
> 6. Start temporal KQL with `AT TIME` and `FIND HISTORY`
> 7. Leverage GCounters as implicit time-series data

---

## 1. Temporal Knowledge Graph Models

TKGs extend triples `(s, r, o)` to quadruples `(s, r, o, t)`.

| Model | Year | Core Approach | Strengths |
|-------|------|--------------|-----------|
| T-TransE | 2018 | TransE + temporal translation | Simple, interpretable |
| HyTE | 2018 | Temporal hyperplane projection | Good temporal granularity |
| DE-SimplE | 2020 | Diachronic entity embeddings | Captures entity evolution |
| TComplEx | 2020 | Complex + temporal tensor decomposition | Strong interpolation |
| TNTComplEx | 2020 | Time-modulated ComplEx | Handles static + dynamic |
| RE-NET | 2020 | RNN + neighborhood aggregation | Structural + temporal evolution |
| CyGNet | 2021 | Copy-generation mechanism | Excels at repetitive events |

### Time Representation Strategies

| Strategy | Description | ONKG Mapping |
|----------|-------------|-------------|
| Point-based | Single timestamp t | `created_at: u32` |
| Interval-based | Valid during [t_start, t_end] | Bond `created_at` + `last_reinforced` |
| Temporal scope | Reification with temporal annotation | VectorClock ordering |
| Event sequence | Ordered events without absolute time | CRDT causal ordering |

---

## 2. Event Sourcing for Knowledge Graphs

### 2.1 BondEvent Types

```rust
enum BondEvent {
    BondCreated {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        weight: u16,
        creator: Creator,
        evidence: Vec<Vec<u8>>,
        timestamp: u64,
        vector_clock: VectorClock,
    },
    BondReinforced {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        new_weight: u16,
        timestamp: u64,
    },
    BondWeakened {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        new_weight: u16,
        reason: WeakeningReason,
        timestamp: u64,
    },
    BondDeprecated {
        source_cid: [u8; 32],
        target_cid: [u8; 32],
        relation: RelationType,
        superseded_by: Option<[u8; 32]>,
        timestamp: u64,
    },
    EpistemicStatusChanged {
        ku_cid: [u8; 32],
        old_status: EpistemicStatus,
        new_status: EpistemicStatus,
        evidence: Vec<Vec<u8>>,
        timestamp: u64,
    },
}
```

### 2.2 Similarity to OBT Account-Chain

| Property | OBT Account-Chain | ONKG Edge Events |
|----------|------------------|------------------|
| Immutability | Blocks append-only | Events append-only |
| Ordering | VectorClock + sequence | VectorClock + timestamp |
| Verification | BLAKE3 hash chain | Content-addressed event CIDs |
| Fork detection | Sequence monotonicity | VectorClock dominance |
| Replay | Rebuild balance from genesis | Rebuild graph from event log |

> [!TIP]
> The Account-Chain's existing infrastructure (VectorClocks, BLAKE3 hashing, delta-state sync) can be directly reused. This provides **free time-travel** — replay events to any timestamp to reconstruct historical state.

---

## 3. Streaming Graph Algorithms

OneBrain's **stigmergy-based routing** is already a streaming graph algorithm:

```
Pheromone Table = Streaming Graph Algorithm
├── reinforce() = edge insertion/weight increase
├── evaporate() = sliding window decay (τ = 0.95/hour)
├── sort by strength = online ranking
└── truncate(10) = bounded memory per topic
```

Key algorithms for ONKG:
- **Streaming Connected Components**: Union-Find with rollback
- **Streaming PageRank**: Forward/Reverse Push — localized updates
- **Sliding Window Analysis**: Only edges within recent time window W — maps to bond decay

---

## 4. Graph Versioning & Time Travel

| System | Model | Time Travel | Key Mechanism |
|--------|-------|-------------|---------------|
| TerminusDB | Document-Graph | ✅ Any commit | Immutable delta layers |
| Dolt | Relational SQL | ✅ `AS OF` queries | Prolly Trees + Merkle DAG |
| ONKG (proposed) | KU + Bond | ✅ Replay events | VectorClock-ordered event log |

### Snapshot Strategy

| Approach | Space | Query Speed | ONKG Recommendation |
|----------|-------|-------------|-------------------|
| Full snapshot | O(V+E) per snap | O(1) | ❌ Too expensive |
| Delta chain | O(Δ) per version | O(Δ × versions) | ❌ Slow for old versions |
| **Snapshot + deltas** | O(V+E) periodic + O(Δ) | O(Δ × since_snap) | ✅ Best tradeoff |

---

## 5. Temporal Query Language Extensions for KQL

### 5.1 Point-in-Time Queries

```sql
FIND (ku:KU) AT TIME "2026-01-01"
FIND (ku:KU)-[r:Extends]->(parent:KU) AT TIME "2026-01-01T12:00:00Z"
```

### 5.2 Interval Queries

```sql
FIND (ku:KU)-[r:Extends]->(m:KU) DURING ["2025-06", "2026-06"]
```

### 5.3 History Queries

```sql
FIND HISTORY (ku:KU) WHERE ku.concept_id = 42
FIND HISTORY (ku:KU).epistemic_status WHERE ku.concept_id = 42 ORDER BY timestamp ASC
```

### 5.4 Decay-Aware Queries

```sql
FIND (ku:KU)-[r]->(m:KU) WHERE r.effective_weight(NOW()) > 0.5
FIND (ku:KU)-[r]->(m:KU) WHERE r.effective_weight(NOW() + 30d) < 0.1
```

### 5.5 Implementation Roadmap

| Phase | Feature | Priority | Complexity |
|-------|---------|----------|------------|
| 1 | `AT TIME` snapshot queries | High | Medium |
| 1 | `FIND HISTORY` event log queries | High | Low |
| 2 | `DURING` interval queries | Medium | Medium |
| 2 | `effective_weight(t)` decay-aware | Medium | Low |
| 3 | Allen's temporal predicates | Low | High |
| 3 | `COUNTERFACTUAL` queries | Low | High |

---

## 6. Knowledge Evolution Patterns

### Kuhn's Paradigm Shift Mapping

| Kuhn Phase | ONKG Mapping |
|------------|-------------|
| Pre-paradigm | Many KUs with `EpistemicStatus::Hypothesis` |
| Normal Science | KUs reaching `Corroborated` → `PeerReviewed` |
| Anomaly Detection | New KUs with `Refutes` bonds to established KUs |
| Crisis | Cluster of `Refutes` bonds; trust decaying |
| Revolution | New KU with `Supersedes` bond; old → `DEPRECATED` |
| New Normal | New KU reaches `Consensus` status |

### Supersession Chains

```
KU_v1 (Hypothesis) 
  └─[Supersedes]─> KU_v2 (Evidence)
                     └─[Supersedes]─> KU_v3 (PeerReviewed)
                                       └─[Supersedes]─> KU_v4 (Consensus)
```

Each version preserved (never deleted). `EdgeState::Deprecated` on superseded bonds. Analogous to Git commits.

---

## 7. Decay & Reinforcement Models

### 7.1 Ebbinghaus Forgetting Curve

$$R(t) = e^{-t/S}$$

Where R(t) = retrievability, S = stability, t = time since last review.

### 7.2 OneBrain's Current Decay System

```rust
// Trust decay (obt_penalty.rs)
fn compute_trust_decay(trust: f64, offline_hours: f64) -> f64 {
    if offline_hours < TRUST_GRACE_PERIOD_HOURS { return trust; }
    let decayed = trust * (-TRUST_DECAY_LAMBDA * offline_hours).exp();
    decayed.max(TRUST_FLOOR)
}
// λ = 0.01, grace period = 1 hour

// Pheromone decay (stigmergy.rs)
let decay = self.decay_rate.powf(hours);  // τ = 0.95

// Bond DecayRate enum
enum DecayRate { None = 0, Slow = 1, Med = 2, Fast = 3 }
```

### 7.3 Recommended Decay per RelationType

$$w_{eff}(t) = w_0 \times e^{-\lambda \cdot (t - t_{last\_reinforced})}$$

| Category | Relations | λ (per day) | Half-life |
|----------|----------|-------------|-----------|
| **B: Structural** | PartOf, InstanceOf, Specializes | **0** | ∞ (never) |
| **F: Temporal** | Precedes, Cooccurs | **0** | ∞ (never) |
| **G: Provenance** | Cites, AuthoredBy, ReviewedBy | **0** | ∞ (never) |
| A: Epistemic | Extends, Corroborates | 0.0019 | 365 days |
| A: Epistemic | Supersedes | **0** | ∞ (permanent) |
| A: Epistemic | Supplements, Refutes, Qualifies | 0.0077 | 90 days |
| C: Causal | Causes, Enables, Prevents, DependsOn | 0.0019 | 365 days |
| D: Derivation | ExampleOf, AnalogyOf | 0.0077 | 90 days |
| D: Derivation | AppliesTo, DerivedFrom | 0.0019 | 365 days |
| E: Similarity | Duplicates | **0** | ∞ (permanent) |
| E: Similarity | Translates, Paraphrases | 0.0019 | 365 days |
| E: Similarity | Inspires | 0.0077 | 90 days |
| H: Experiential | FormallyProves | **0** | ∞ (permanent) |
| H: Experiential | ReactionTo, SensoryEvidenceFor | 0.099 | 7 days |
| H: Experiential | EvolvesInto, VariantOf | 0.0019 | 365 days |

### 7.4 Unified Decay Framework

```rust
trait Decayable {
    fn decay_lambda(&self) -> f64;
    fn last_reinforced(&self) -> u64;
    fn grace_period_hours(&self) -> f64 { 0.0 }
    fn floor(&self) -> f64 { 0.0 }
    
    fn effective_value(&self, current_value: f64, now: u64) -> f64 {
        let hours = (now - self.last_reinforced()) as f64 / 3600.0;
        if hours < self.grace_period_hours() { return current_value; }
        let decayed = current_value * (-self.decay_lambda() * hours).exp();
        decayed.max(self.floor())
    }
}

impl Decayable for Bond { /* per-RelationType λ from table */ }
impl Decayable for TrustScore { /* λ=0.01, grace=1h, floor=0.01 */ }
impl Decayable for PheromoneHop { /* λ=0.0513 (from τ=0.95) */ }
```

---

## 8. Causal Knowledge Graphs

### Pearl's Ladder of Causation

| Level | Question | ONKG Capability |
|-------|----------|----------------|
| 1. Association | "What does X tell me about Y?" | Bond traversal (existing) |
| 2. Intervention | "What if I do X?" | Causal bonds (Causes, Enables, Prevents) |
| 3. Counterfactual | "What if X hadn't happened?" | Event replay with modified history |

ONKG's Category C (Causal) relations: `Causes (0x20)`, `Enables (0x21)`, `Prevents (0x22)`, `DependsOn (0x23)`.

With event sourcing, ONKG can answer counterfactuals by replaying events while filtering specific operations.

---

## 9. Integration with OneBrain

### Existing Temporal Infrastructure

| Component | Location | Temporal Extension |
|-----------|----------|-------------------|
| VectorClock | crdt.rs | Graph event ordering |
| GCounter | crdt.rs / metabolism.rs | Time-series analysis |
| LWWRegister | crdt.rs | Timestamp-based conflict resolution |
| ORSet | crdt.rs | Add/remove with causal tracking |
| DecayRate | types.rs | Per-RelationType calibration |
| EdgeState | types.rs | Event-sourced state machine |
| PheromoneTable | stigmergy.rs | Streaming graph analytics |
| Trust decay | obt_penalty.rs | Unified decay framework |

### GCounter as Time-Series Data

Each GCounter in metabolism tracking is implicitly a time series:
- **Trend detection**: Is a KU gaining or losing traction?
- **Velocity tracking**: Rate of citation growth
- **Anomaly detection**: Sudden spike in usage (viral knowledge)

---

## 10. Design Proposals

### 10.1 Event-Sourced ONKG Architecture

```
Write Path:  KQL Command → Validator → Event Store (append-only) → VectorClock
Event Bus:   Delta-State CRDT Sync (Layer 8)
Read Projections:
  ├── Graph Projection (current state materialized view)
  ├── Temporal Projection (time-indexed events for time-travel)
  ├── Search Projection (full-text + vector search index)
  └── Analytics Projection (streaming metrics, PageRank)
```

### 10.2 Graph Versioning Design

```rust
struct GraphVersion {
    version_id: [u8; 32],            // BLAKE3 hash
    parent_version: Option<[u8; 32]>,
    timestamp: u64,
    vector_clock: VectorClock,
    event_range: (u64, u64),          // [from, to)
    snapshot_cid: Option<[u8; 32]>,
    metadata: VersionMetadata,
}

fn query_at_time(query: &KQLQuery, target_time: u64) -> QueryResult {
    let snapshot = find_snapshot_before(target_time);
    let events = event_store.range(snapshot.event_seq..);
    let graph = snapshot.materialize();
    for event in events {
        if event.timestamp > target_time { break; }
        graph.apply(event);
    }
    graph.execute(query)
}
```

---

## References

1. Zhang et al. (2024). "A Survey on Temporal Knowledge Graph." arXiv.
2. Jin et al. (2020). "RE-NET: Recurrent Event Network." USC.
3. Zhu et al. (2021). "CyGNet: Learning Copy-Generation Networks." AAAI.
4. Dasgupta et al. (2018). "HyTE: Hyperplane-based Temporally aware KGE." EMNLP.
5. Microsoft (2023). "Event Sourcing pattern." Azure Architecture Center.
6. TerminusDB (2025). Documentation. terminusdb.org.
7. DoltHub (2025). "Dolt: Git for Data." dolthub.com.
8. Allen, J.F. (1983). "Maintaining Knowledge about Temporal Intervals." CACM.
9. Kuhn, T. (1962). *The Structure of Scientific Revolutions*. UChicago Press.
10. Ebbinghaus, H. (1885). *Über das Gedächtnis*.
11. Ye, J. (2022). "Free Spaced Repetition Scheduler (FSRS)."
12. Pearl, J. (2009). *Causality: Models, Reasoning, and Inference*. Cambridge.

---

> **Last updated**: 2026-07-02  
> **Status**: Survey complete — ready for implementation planning
