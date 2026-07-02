# Bio-Inspired & Novel Graph Architectures for ONKG

> **Author**: OneBrain Research Team  
> **Date**: 2026-07-02  
> **Purpose**: Survey bio-inspired paradigms and extensions for ONKG's biological knowledge model

---

## Executive Summary

OneBrain already implements 5 bio-inspired mechanisms (Stigmergy, Hebbian bonds, Immune system, Ecological niche, Metabolic value). This research identifies **extensions, new paradigms, and specific algorithms** to evolve ONKG from a bio-inspired knowledge graph into a **living knowledge organism**.

---

## 1. Brain Connectome Models

### 1.1 Key Principles

**Small-World Networks (Watts-Strogatz, 1998):**
- High local clustering + short average path lengths
- Average path length scales as L ∝ log(N)

**Clustering Coefficient:**
$$C_i = \frac{2 \times \text{triangles}_i}{\text{degree}_i \times (\text{degree}_i - 1)}$$

**Small-world coefficient:** σ = (C/C_random) / (L/L_random) — σ >> 1 = small-world

### 1.2 ONKG Integration

| Brain Feature | ONKG Implementation |
|:---|:---|
| Small-world topology | Monitor σ coefficient across KU bond graph |
| Hierarchical organization | Map 11 Gene types to hierarchical levels |
| Semantic memory | KU retrieval via spreading activation from query seeds |
| Cortical columns | KU clusters sharing domain codes |
| Hub nodes | High-PageRank KUs (from `synaptic.rs`) as cortical hubs |

---

## 2. Evolutionary Knowledge Networks

### 2.1 Memetic Algorithm for KU Evolution

| Genetic Evolution | Memetic (Knowledge) | ONKG Mechanism |
|:---|:---|:---|
| Gene | Meme (KU content) | KU Gene payload |
| Biological reproduction | Imitation, citation | Derivative KUs, citations |
| Slow (generations) | Rapid (hours-days) | Version chains via `prev_cid` |
| Natural selection | Usage-based selection | PoMV metabolism signal |
| Genetic drift | Knowledge drift | Entropy decay over time |

### 2.2 Knowledge Mutation Operators

```rust
enum KnowledgeMutation {
    Refinement,        // Improve precision (local search)
    Generalization,    // Abstract to broader principle
    Specialization,    // Narrow to specific context
    Combination,       // Merge two KUs into derivative
    Contradiction,     // Challenge existing KU (adversarial)
}
```

---

## 3. Immune-Inspired Graph Filtering

### 3.1 Current System (immune.rs, 440 LOC)

OneBrain has 4 antibody types: TemporalBurst, SourceConcentration, LowEngagement, DiversityDeficit. Content-agnostic pattern detection. Quarantine requires ≥2 antibody types AND avg confidence > 0.7.

### 3.2 Proposed Extensions

**Negative Selection Algorithm (NSA):** Generate detectors, filter against "self" (legitimate patterns). Remaining detectors recognize only "non-self" (anomalous patterns).

**Clonal Selection Algorithm (CSA):** When antibody matches antigen with sufficient affinity → clone + mutate. Higher affinity antibodies preferentially cloned (affinity maturation).

**Danger Theory (Matzinger, 2002):** Immune response triggered by *danger signals* (cell damage/stress), not foreignness. Lower false-alarm rates than pure self/non-self.

```rust
enum DangerSignal {
    TemporalBurst { cv: f32 },
    SourceConcentration { ratio: f32 },
    GeographicAnomaly { entropy: f32 },
    MetabolicAnomaly { rate: f32 },
}

fn danger_assessment(ku: &KU, signals: &[DangerSignal]) -> ThreatLevel {
    let danger_score: f32 = signals.iter()
        .map(|s| s.weight() * s.intensity()).sum();
    let safe_score: f32 = ku.positive_interactions();
    if danger_score > safe_score * DANGER_THRESHOLD {
        ThreatLevel::Quarantine
    } else { ThreatLevel::Safe }
}
```

---

## 4. Ant Colony Optimization for Graph Queries

### 4.1 Enhanced ACO

OneBrain already has stigmergy.rs (251 LOC): reinforce/evaporate/best_hop, ConceptID-based trails, τ=0.95 decay.

**Pheromone Update Rule:**
$$\tau_{ij}(t+1) = (1-\rho) \cdot \tau_{ij}(t) + \sum_{k=1}^{m} \Delta\tau_{ij}^k$$

### 4.2 Proposed Extensions

1. **Hierarchical Pheromone**: 3-level routing (domain → concept → node)
2. **ACO + Embedding Hybrid**: Initialize pheromone from embedding similarity
3. **Anti-pheromone**: Negative trails for paths leading to poor results
4. **Pheromone Manipulation Detection**: Immune system detects artificial inflation

---

## 5. Ecological Models for Knowledge

### 5.1 Lotka-Volterra Competition

$$\frac{dN_1}{dt} = r_1 N_1 \left(1 - \frac{N_1 + \alpha_{12} N_2}{K_1}\right)$$

Where N₁ = population of knowledge type 1, r₁ = growth rate, K₁ = carrying capacity, α₁₂ = competition coefficient.

### 5.2 Ecological Succession

| Stage | Ecology | Knowledge Analog |
|:---|:---|:---|
| Pioneer | r-strategists, fast-growing | Rumors, early hypotheses |
| Early succession | Competition begins | Evidence accumulates, hypotheses compete |
| Climax | K-strategists, specialized, stable | Established theories, axioms |
| Disturbance | Fire, flood | Paradigm shift, new discovery |

### 5.3 Biodiversity Indices

- **Shannon**: H = -Σ(pᵢ × ln(pᵢ))
- **Simpson**: D = 1 - Σ(pᵢ²)

OneBrain already has: niche fitness (ecosystem.rs), carrying capacity, competitive exclusion via PoMV.

---

## 6. Neuromorphic Graph Processing

### 6.1 Event-Driven KU Activation

```rust
struct SpikeEvent {
    ku_id: KuId,
    timestamp: Instant,
    activation_type: ActivationType, // Query, Citation, CoRetrieval
    strength: f32,
}

fn process_spike(event: SpikeEvent, graph: &mut KuGraph) {
    let ku = graph.get_mut(event.ku_id);
    ku.membrane_potential += event.strength;
    if ku.membrane_potential > FIRING_THRESHOLD {
        ku.membrane_potential = RESET_POTENTIAL;
        for bond in ku.hebbian_bonds() {
            let propagated = event.strength * bond.weight * DECAY_FACTOR;
            if propagated > MIN_PROPAGATION {
                emit_spike(SpikeEvent { ku_id: bond.target, strength: propagated, .. });
            }
        }
    }
}
```

### 6.2 STDP for Hebbian Bond Updates

```rust
fn stdp_update(pre: &SpikeEvent, post: &SpikeEvent, bond: &mut Bond) {
    let dt = (post.timestamp - pre.timestamp).as_millis() as f32;
    if dt > 0.0 {
        // Pre before post → LTP (strengthening)
        bond.weight += A_PLUS * (-dt / TAU_PLUS).exp();
    } else {
        // Post before pre → LTD (weakening)
        bond.weight += A_MINUS * (dt.abs() / TAU_MINUS).exp();
    }
}
// A_PLUS = 0.01, A_MINUS = -0.012, TAU = 20.0
```

---

## 7. Morphogenetic Knowledge Structures

### 7.1 Reaction-Diffusion on Networks

Based on Turing Patterns (1952): activator (short-range activation) + inhibitor (long-range inhibition) → spontaneous pattern formation.

```rust
fn knowledge_diffusion_step(graph: &mut KuGraph, dt: f32) {
    for ku in graph.nodes() {
        let reaction = ACTIVATION_RATE * ku.activation
            * (1.0 - ku.activation / SATURATION)
            - INHIBITION_RATE * ku.inhibition;
        let diffusion: f32 = ku.neighbors().iter()
            .map(|(n, w)| DIFFUSION_COEFF * w * (n.activation - ku.activation))
            .sum();
        ku.activation += (reaction + diffusion) * dt;
    }
}
```

### 7.2 Self-Organized Criticality (SOC)

Systems naturally evolve toward critical state. Power-law distributions. "Sandpile model": small perturbations → occasional large avalanches. Monitor if ONKG operates near SOC — too subcritical = static, too supercritical = chaotic.

---

## 8. Mycelium Networks (Fungal Inspiration)

### 8.1 Key Mechanisms

| Biological | Function | ONKG Analog |
|:---|:---|:---|
| Hyphal branching | Explore new territory | Speculative bonds to distant clusters |
| Anastomosis (fusion) | Redundant pathways | Cross-domain bond reinforcement |
| Nutrient translocation | Route resources | PoMV metabolic flow |
| Hyphal retreat | Abandon unproductive areas | Bond decay + pruning |
| Self-healing | Bridge gaps when damaged | Automatic bypass on node failure |

### 8.2 Self-Healing Algorithm

```rust
fn self_heal(graph: &mut MyceliumGraph, failed_node: KuId) {
    let orphaned = graph.connections_to(failed_node);
    for conn in orphaned {
        let alternative = graph.find_bridge(conn.source, failed_node, MAX_DETOUR);
        if let Some(bridge) = alternative {
            graph.create_bypass(conn.source, bridge, conn.target_behind_failure);
        }
    }
}
```

---

## 9. Hebbian Extensions & Memory Consolidation

### 9.1 STDP Extension

Current: Simple co-retrieval (symmetric). Proposed: Timing-dependent (asymmetric).

```rust
fn stdp_bond_update(ku_a: KuId, ku_b: KuId, time_a: Instant, time_b: Instant, bond: &mut HebbianBond) {
    let dt = (time_b - time_a).as_secs_f32();
    if dt.abs() < STDP_WINDOW_SECS {  // e.g., 300 seconds
        if dt > 0.0 {
            // A before B → strengthen A→B (causal direction)
            bond.forward_weight += A_PLUS * (-dt / TAU_PLUS).exp();
        } else {
            // B before A → weaken A→B
            bond.forward_weight += A_MINUS * (dt.abs() / TAU_MINUS).exp();
        }
    }
}
```

### 9.2 Memory Consolidation (Hippocampus → Neocortex)

```rust
struct KnowledgeMemory {
    working_memory: BTreeMap<KuId, WorkingKU>,  // Last 24-48 hours
    long_term_memory: PersistentStore,           // Permanent storage
}

fn consolidation_cycle(memory: &mut KnowledgeMemory) {
    for (ku_id, working_ku) in memory.working_memory.iter() {
        let score = working_ku.retrieval_count * 0.3
            + working_ku.pomv_score * 0.3
            + working_ku.bond_count * 0.2
            + working_ku.emotional_salience * 0.2;
        if score > CONSOLIDATION_THRESHOLD {
            memory.long_term_memory.consolidate(ku_id, working_ku);
            for bond in working_ku.recent_bonds() {
                bond.weight *= CONSOLIDATION_BONUS; // 1.5x
            }
        }
    }
}
```

### 9.3 Dream Mode (Offline Restructuring)

```rust
fn dream_reorganization(graph: &mut KuGraph) {
    // 1. REPLAY: Re-activate popular query patterns
    let queries = graph.query_log_last_24h();
    for query in queries.sample(DREAM_SAMPLE_SIZE) {
        let activated = spreading_activation(query.seed_kus(), graph);
        // 2. RANDOM ASSOCIATION: Cross-domain connections
        let pairs = activated.random_cross_domain_pairs(5);
        for (a, b) in pairs {
            if embedding_similarity(a, b) > WEAK_THRESHOLD {
                graph.create_speculative_bond(a, b, SPECULATIVE_WEIGHT);
            }
        }
    }
    // 3. ABSTRACTION: Detect patterns → create meta-KUs
    let patterns = graph.detect_frequent_subgraphs(MIN_SUPPORT);
    for pattern in patterns {
        if !graph.has_meta_ku_for(pattern) {
            graph.add_ku(create_meta_ku(pattern));
        }
    }
    // 4. PRUNING: Remove unreinforced speculative bonds
    for bond in graph.speculative_bonds() {
        if bond.age > SPECULATIVE_MAX_AGE && bond.reinforcement_count == 0 {
            graph.remove_bond(bond.id);
        }
    }
}
```

---

## 10. Biological Concept → ONKG Mapping Table

| Biological System | Concept | ONKG Mechanism | Status | Priority |
|:---|:---|:---|:---|:---|
| **Brain** | Neuron | Knowledge Unit (KU) | ✅ Exists | — |
| **Brain** | Synapse | Hebbian bond (synaptic.rs) | ✅ Exists | — |
| **Brain** | Synaptic plasticity (LTP/LTD) | Bond weight reinforcement/decay | ✅ Exists | — |
| **Brain** | STDP | Timing-dependent bond updates | 🆕 Proposed | P1 |
| **Brain** | Anti-Hebbian learning | Novelty detection / redundancy suppression | 🆕 Proposed | P2 |
| **Brain** | Memory consolidation | Working memory → long-term store | 🆕 Proposed | P1 |
| **Brain** | REM sleep | Dream mode (offline restructuring) | 🆕 Proposed | P2 |
| **Brain** | Spreading activation | Query propagation through bonds | 🆕 Proposed | P1 |
| **Brain** | Small-world network | KU graph topology monitoring | 🆕 Proposed | P2 |
| **Evolution** | Natural selection | PoMV fitness-based survival | ✅ Exists | — |
| **Evolution** | Mutation | Version chains (prev_cid) | ✅ Exists | — |
| **Evolution** | Genetic code | 11 Gene types | ✅ Exists | — |
| **Immune** | Antibodies | 4 antibody types | ✅ Exists | — |
| **Immune** | Clonal selection | Adaptive antibody mutation | 🆕 Proposed | P1 |
| **Immune** | Danger Theory / DCA | Multi-signal threat assessment | 🆕 Proposed | P1 |
| **Ant Colony** | Pheromone trails | Stigmergy routing | ✅ Exists | — |
| **Ant Colony** | Hierarchical pheromone | Multi-level routing | 🆕 Proposed | P1 |
| **Ant Colony** | Anti-pheromone | Negative trails | 🆕 Proposed | P2 |
| **Ecology** | Niche | Knowledge domain | ✅ Exists | — |
| **Ecology** | Carrying capacity | Max KUs per niche | ✅ Exists | — |
| **Ecology** | Lotka-Volterra | Continuous competition modeling | 🆕 Proposed | P2 |
| **Ecology** | Biodiversity indices | Shannon/Simpson monitoring | 🆕 Proposed | P2 |
| **Neuromorphic** | Spiking neurons | Event-driven KU activation | 🆕 Proposed | P3 |
| **Morphogenesis** | Turing patterns | Self-organizing topic clusters | 🆕 Proposed | P3 |
| **Morphogenesis** | SOC | Criticality monitoring | 🆕 Proposed | P3 |
| **Mycelium** | Self-healing | Automatic bypass on node failure | 🆕 Proposed | P1 |
| **Mycelium** | Nutrient flow | Metabolic value routing | 🆕 Proposed | P2 |

---

## 11. Proposed Biological Lifecycle

### Lifecycle Parameters

| Phase | Duration | Transition Trigger |
|:---|:---|:---|
| Conception | Instant | KU creation event |
| Birth | 0-1 hour | First retrieval |
| Infancy | 1h-48h | Consolidation check |
| Juvenile | 48h-30d | PoMV stability threshold |
| Adult | 30d+ | Natural decay unless reinforced |
| Aging | Variable | No activity for >30d |
| Apoptosis | Irreversible | PoMV < extinction threshold |

---

## 12. Implementation Roadmap

### Phase 1 — Quick Wins (P1, 1-2 months)

| Enhancement | Module | Impact |
|:---|:---|:---|
| STDP bond updates | `synaptic.rs` | Directional bonds, better retrieval |
| Memory consolidation (2-tier) | New: `consolidation.rs` | Better knowledge persistence |
| Hierarchical pheromone | `stigmergy.rs` | Faster query convergence |
| Clonal selection | `immune.rs` | Adaptive attack defense |
| Danger Theory (DCA) | `immune.rs` | Fewer false positives |
| Self-healing routing | `stigmergy.rs` | Network resilience |
| Spreading activation | New: `activation.rs` | Associative retrieval |

### Phase 2 — Bio Depth (P2, 3-6 months)

| Enhancement | Module | Impact |
|:---|:---|:---|
| Anti-Hebbian learning | `synaptic.rs` | Novelty promotion |
| Dream mode | New: `dream.rs` | Knowledge reorganization |
| Lotka-Volterra dynamics | `ecosystem.rs` | Dynamic competition |
| Biodiversity indices | New: `diversity.rs` | Ecosystem health monitoring |
| Mycelium-inspired routing | `stigmergy.rs` | Adaptive discovery + retreat |

### Phase 3 — Frontier (P3, 6-12 months)

| Enhancement | Module | Impact |
|:---|:---|:---|
| Event-driven (spiking) activation | New: `neuromorphic.rs` | Energy-efficient processing |
| Reaction-diffusion | New: `morphogenesis.rs` | Self-organizing clusters |
| SOC monitoring | New: `criticality.rs` | Graph health at edge of chaos |

---

## References

1. Watts, D.J. & Strogatz, S.H. (1998). "Collective dynamics of 'small-world' networks." *Nature*.
2. Dawkins, R. (1976). *The Selfish Gene*. Oxford University Press.
3. De Castro, L.N. & Timmis, J. (2002). *Artificial Immune Systems*. Springer.
4. Matzinger, P. (2002). "The Danger Model." *Science*.
5. Dorigo, M. & Stützle, T. (2004). *Ant Colony Optimization*. MIT Press.
6. Lotka, A.J. (1925). *Elements of Physical Biology*.
7. Turing, A.M. (1952). "The Chemical Basis of Morphogenesis." *Phil. Trans. R. Soc.*
8. Bak, P. (1996). *How Nature Works: SOC*. Copernicus.
9. Hebb, D.O. (1949). *The Organization of Behavior*. Wiley.
10. Markram, H. et al. (1997). "Regulation of Synaptic Efficacy." *Science*.
11. Rasch, B. & Born, J. (2013). "About Sleep's Role in Memory." *Physiological Reviews*.

---

> **Last updated**: 2026-07-02  
> **Status**: Survey complete — ready for implementation planning
