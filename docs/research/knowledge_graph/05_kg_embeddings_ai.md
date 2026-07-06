# Knowledge Graph Embeddings & AI — Survey for OBKG

> **Author**: OneBrain Research Team  
> **Date**: 2026-07-02  
> **Purpose**: Evaluate KGE models and AI techniques for OBKG edge deployment

---

## Executive Summary

> [!IMPORTANT]
> **Primary Recommendation**: **RotatE with int8 quantization** at dimension 64 is the optimal choice for OBKG. It handles all critical relational patterns in OBKG's 33 bond types (symmetry, antisymmetry, inversion, composition), requires only **64 bytes per entity** in int8, and can be trained incrementally via federated learning.

**Key Constraints:**
- Edge devices: 1-4 GB RAM, ARM CPU, no GPU
- Existing: 512B int8 embeddings, 128B binary, 16B SimHash
- Must support: link prediction, knowledge completion, anomaly detection
- P2P: no single node has the full graph

---

## 1. OneBrain Current State

### 1.1 Existing Embedding Infrastructure

| Component | Size | Purpose |
|-----------|------|---------|
| `embedding` | 512 bytes (int8×512) | Semantic similarity via cosine distance |
| `embedding_binary` | 128 bytes (1024-bit) | Fast Hamming-based screening |
| `simhash` | 16 bytes (128-bit) | Near-duplicate detection (threshold: 10 bits) |
| `lsh_buckets` | 16 bytes | Bridge detection (LSH locality) |
| `embed_version` | 2 bytes | Model version tracking |

**Total per KU**: ~674 bytes for embedding infrastructure.

### 1.2 RelationType Pattern Analysis

| Pattern | Relations | Count | Model Requirement |
|---------|-----------|-------|-------------------|
| **Symmetric** | Duplicates, Translates, Paraphrases, Cooccurs | 4 | RotatE (r = π) |
| **Antisymmetric** | PartOf, InstanceOf, Causes, Enables, Precedes, AuthoredBy | 12 | RotatE |
| **Inverse** | Extends↔DerivedFrom, Specializes↔Generalizes, Causes↔Prevents | 6 pairs | RotatE |
| **Hierarchical** | PartOf, InstanceOf, Specializes, ExampleOf | 5 | HAKE |
| **Composition** | Causes∘Enables, PartOf∘PartOf | many | RotatE |

> [!NOTE]
> TransE cannot handle symmetric relations (would require r = 0) or N-to-N mappings. RotatE handles ALL patterns, making it the strongest candidate.

---

## 2. Translational Models

### 2.1 TransE (Bordes et al., 2013)

**Scoring**: $f(h, r, t) = -\|h + r - t\|$

Simplest, fastest, excellent for 1-to-1 relations. Cannot model symmetric or N-to-N.

### 2.2 TransH (Wang et al., 2014)

**Scoring**: Project entities onto relation-specific hyperplane before translation. Handles N-to-N.

### 2.3 TransR (Lin et al., 2015)

**Scoring**: Project entities into relation-specific space via matrix $M_r$. Most expressive of Trans-family but expensive.

| Model | Params/Rel | Memory (33 rels, d=64) | Symmetric | Edge-feasible |
|-------|-----------|----------------------|-----------|---------------|
| TransE | $d$ | 2.1 KB | ❌ | ✅ Excellent |
| TransH | $2d$ | 4.1 KB | ❌ | ✅ Good |
| TransR | $d_e \times d_r$ | 135 KB | ✅ | ⚠️ Moderate |

---

## 3. Rotational Models

### 3.1 RotatE (Sun et al., 2019) — ★ Recommended

**Scoring**: $f(h, r, t) = -\|h \circ r - t\|$ where $h, r, t \in \mathbb{C}^{d/2}$ and $|r_i| = 1$

Each $r_i = e^{i\theta_i}$, so only $d/2$ angle parameters per relation.

**Pattern coverage:**
- Symmetric: $r_i = e^{i\pi} = -1$ (180° rotation)
- Antisymmetric: $r_i \neq \pm 1$
- Inversion: $r_2 = \bar{r}_1$ (conjugate = inverse)
- Composition: $r_3 = r_1 \circ r_2$ (angle addition)

### 3.2 HAKE (Zhang et al., 2020)

Modulus captures hierarchy level, phase captures position. Explicitly models hierarchical relations. Very relevant for OBKG's 5+ hierarchical relations, but 2× memory.

### 3.3 Mapping to OBKG Relations

| OBKG Category | Best Model | Why |
|---------------|-----------|-----|
| B: Structural (PartOf, InstanceOf) | **HAKE** | Explicit hierarchy |
| C: Causal (Causes, Enables) | **RotatE** | Antisymmetry + composition |
| E: Similarity (Duplicates, Translates) | **RotatE** | Symmetry (r = π) |
| A: Epistemic (Extends, Refutes) | **RotatE** | Antisymmetry |

---

## 4. Semantic Matching Models

| Model | Params/Rel | Symmetric | Antisymmetric | Edge |
|-------|-----------|-----------|---------------|------|
| RESCAL | $d^2$ | ✅ | ✅ | ❌ (132KB) |
| DistMult | $d$ | ✅ | ❌ | ✅ |
| ComplEx | $2d$ | ✅ | ✅ | ✅ |
| TuckER | core tensor | ✅ | ✅ | ❌ |

---

## 5. Graph Neural Networks for KG

| GNN Model | Params/Layer (d=64) | Inductive | Relation-Aware | Edge Verdict |
|-----------|-------------------|-----------|----------------|--------------|
| R-GCN | 132 KB (33 rels) | ❌ | ✅ | ⚠️ With basis decomposition |
| CompGCN | 12 KB (shared) | ❌ | ✅ | ✅ Best for KG |
| GAT | 24 KB (4 heads) | ❌ | ❌ | ❌ Too expensive |
| GraphSAGE | 4 KB | ✅ | ❌ | ✅ Best for edge |

> [!TIP]
> Hybrid approach: **RotatE embeddings** (pre-computed) with **GraphSAGE-style inductive aggregation** for on-the-fly predictions on new KUs.

---

## 6. Knowledge Completion & Link Prediction

### 6.1 Benchmark Results (FB15k-237)

| Model | MRR | Hits@10 | Dim |
|-------|-----|---------|-----|
| TransE | 0.294 | 0.465 | 512-1000 |
| DistMult | 0.241 | 0.419 | 200 |
| ComplEx | 0.247 | 0.428 | 200 |
| **RotatE** | **0.338** | **0.533** | 500-1000 |
| HAKE | 0.346 | 0.542 | 500 |
| TuckER | 0.358 | 0.544 | 200 |

### 6.2 OBKG Application

| Task | Example | Impact |
|------|---------|--------|
| Missing bonds | (KU_quantum, Extends, ?) → discover KU_physics | Auto-link related KUs |
| Relation prediction | (KU_A, ?, KU_B) → predict "Causes" | Suggest bond type |
| Knowledge gaps | Low-scoring predictions in domain | Identify under-explored areas |
| Anomaly detection | Very low score for existing bond | Flag wrong bonds |

---

## 7. Lightweight / Edge-Deployable Models

### 7.1 Quantization Table

| Precision | Bytes/dim | Memory for d=64 | Accuracy Retention |
|-----------|----------|-----------------|-------------------|
| float32 | 4 | 256 bytes | 100% (baseline) |
| float16 | 2 | 128 bytes | ~99% |
| **int8** | **1** | **64 bytes** | **95-98%** |
| int4 | 0.5 | 32 bytes | 88-92% |
| binary | 0.125 | 8 bytes | 90-95% |

### 7.2 Pure Rust RotatE Scoring

```rust
fn rotate_score(h: &[i8; 64], r_re: &[i8; 32], r_im: &[i8; 32], t: &[i8; 64]) -> i32 {
    let mut score: i64 = 0;
    for i in 0..32 {
        let h_re = h[2*i] as i64;
        let h_im = h[2*i+1] as i64;
        let rr = r_re[i] as i64;
        let ri = r_im[i] as i64;
        let res_re = h_re * rr - h_im * ri;
        let res_im = h_re * ri + h_im * rr;
        let t_re = t[2*i] as i64;
        let t_im = t[2*i+1] as i64;
        let d_re = res_re - t_re * 127;
        let d_im = res_im - t_im * 127;
        score += d_re * d_re + d_im * d_im;
    }
    -(score as i32)
}
```

---

## 8. Anomaly Detection with KGE

### Integration with immune.rs

```
                ┌──────────────────────────┐
                │     OBKG Anomaly Layer    │
                ├──────────────┬────────────┤
                │ Behavioral   │ Structural │
                │ (immune.rs)  │ (KGE-new)  │
                ├──────────────┼────────────┤
                │ TemporalBurst│ LowScore   │
                │ SourceConc   │ ClusterOut │
                │ LowEngagement│ TempDrift  │
                │ DiversityDef │ InverseViol│
                └──────────────┴────────────┘
```

---

## 9. Federated KG Embedding

### FedR Protocol (Best Match for OBKG)

Since OBKG has only 33 relation types, nodes can safely share relation embeddings (33 × 32 int8 = **1,056 bytes**) while keeping entity embeddings local.

```
FedR-OBKG Protocol:
1. INIT: Seed 33 relation embeddings (1,056 bytes total)
2. LOCAL TRAIN: SGD on local triples, 10-20 steps
3. GOSSIP: Send Δr to K random peers (~1 KB per round)
4. CONVERGENCE: ~50-100 gossip rounds
5. PRIVACY: NEVER share entity embeddings
```

**Communication**: ~50-100 KB per node per day. Minimal.

---

## 10. Memory Budget

### RotatE (d=64, int8, 33 relations)

| Scale | KU Count | Entity Memory | Relation | Total |
|-------|----------|--------------|----------|-------|
| Small | 1K | 64 KB | 1 KB | **65 KB** |
| Medium | 10K | 640 KB | 1 KB | **641 KB** |
| Large | 100K | 6.1 MB | 1 KB | **6.1 MB** |
| 1M | 1M | 61 MB | 1 KB | **61 MB** |

> [!WARNING]
> At 10M+ KUs, edge devices cannot hold all embeddings. Solutions: disk-backed index, local subgraph only (<100K), binary screening + int8 for top-K.

---

## 11. Recommendation

### RotatE-int8 Summary

| Aspect | Choice | Rationale |
|--------|--------|-----------|
| **Model** | RotatE | Handles all 4 patterns |
| **Dimension** | 64 (32 complex dims) | Accuracy vs. memory balance |
| **Precision** | int8 | Matches existing pipeline |
| **Per-entity** | 64 bytes | Fits in EpigeneticSection |
| **Per-relation** | 32 bytes | 33 × 32 = 1,056 bytes total |
| **Training** | Federated (FedR) | Share only relation embeddings |
| **Inference** | Pure Rust | No ML framework needed |

### Integration with Existing Embeddings

```
EpigeneticSection (current + new):
├── embedding: [u8; 512]          ← EXISTING: content similarity
├── embedding_binary: [u8; 128]   ← EXISTING: fast screening
├── simhash: [u8; 16]             ← EXISTING: near-duplicate
├── lsh_buckets: [u8; 16]         ← EXISTING: bridge detection
├── embed_version: u16            ← EXISTING: versioning
│
├── relational_emb: [u8; 64]      ← NEW: RotatE entity (int8)
├── relational_version: u16       ← NEW: model version
└── relational_timestamp: u32     ← NEW: last updated
```

**New per KU**: +68 bytes → Total epigenetic: ~742 bytes

### 4-Phase Implementation

| Phase | Feature | Priority |
|-------|---------|----------|
| v6.1 | Add relational_embedding, RotatE scoring, offline training | High |
| v6.2 | Link prediction, "Suggested Bonds" UI | Medium |
| v6.3 | FedR protocol, local SGD, convergence monitoring | Medium |
| v6.4 | KGE-based anomaly detection in immune.rs | Low |

---

## References

1. Bordes et al. (2013). "Translating Embeddings for Multi-relational Data." NeurIPS.
2. Wang et al. (2014). "Knowledge Graph Embedding by Translating on Hyperplanes." AAAI.
3. Lin et al. (2015). "Learning Entity and Relation Embeddings for KG Completion." AAAI.
4. Sun et al. (2019). "RotatE: KG Embedding by Relational Rotation in Complex Space." ICLR.
5. Zhang et al. (2019). "Quaternion Knowledge Graph Embeddings." NeurIPS.
6. Zhang et al. (2020). "Learning Hierarchy-Aware KG Embeddings." AAAI.
7. Nickel et al. (2011). "A Three-Way Model for Collective Learning." ICML.
8. Yang et al. (2015). "Embedding Entities and Relations." ICLR.
9. Trouillon et al. (2016). "Complex Embeddings for Simple Link Prediction." ICML.
10. Balažević et al. (2019). "TuckER: Tensor Factorization for KG Completion." EMNLP.
11. Schlichtkrull et al. (2018). "Modeling Relational Data with GCN." ESWC.
12. Vashishth et al. (2020). "Composition-Based Multi-Relational GCN." ICLR.
13. Hamilton et al. (2017). "Inductive Representation Learning on Large Graphs." NeurIPS.
14. FedR (2024). Relation-Aware Federated KG Embedding.
15. FastKGE (2024). Incremental LoRA for Dynamic Knowledge Graphs. IJCAI.

---

> **Last updated**: 2026-07-02  
> **Status**: Survey complete — ready for implementation planning
