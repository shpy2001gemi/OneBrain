# OneBrain Knowledge Graph: A Bio-Inspired, Decentralized Knowledge Graph with Federated Embeddings and Epistemic-First Bonds

> *"The brain is a world consisting of a number of unexplored continents and great stretches of unknown territory."*
> — Santiago Ramón y Cajal

---

**Authors:** OneBrain Project Contributors
**Contact:** shpy2001@gmail.com
**Date:** July 2026
**Version:** 1.0

---

## Abstract

Knowledge graphs have become foundational infrastructure for structured knowledge representation, powering search engines, recommendation systems, and question-answering pipelines across industry and academia. Yet the dominant paradigms — exemplified by Google Knowledge Graph, Wikidata, and DBpedia — suffer from four systemic limitations that constrain their utility for truly intelligent systems: (1) **static triple stores** that capture knowledge as immutable snapshots rather than living, temporally evolving structures; (2) **centralized ownership models** that concentrate control over knowledge curation and access, creating single points of failure and censorship; (3) the **absence of epistemic grading**, treating a rigorously peer-reviewed scientific finding identically to an unverified rumor; and (4) **no biological adaptation mechanisms**, rendering these graphs incapable of learning from usage patterns, forgetting stale knowledge, or consolidating memories during idle periods. We present the **OneBrain Knowledge Graph (OBKG)**, a bio-inspired, decentralized knowledge graph layer designed as the cognitive backbone of the OneBrain system. OBKG introduces a four-layer architecture — Foundation, Intelligence, Advanced, and Adapter — that composes over the existing OneBrain pillars (P1–P5) without requiring any modification to their codebases. We contribute seven key innovations: (1) a taxonomy of 34 `RelationType` variants with first-class **Epistemic** and **Experiential** bond categories embedding an 11-level epistemic ladder directly into the wire format; (2) **RotatE-based knowledge graph embeddings** quantized to int8 achieving 64 bytes per entity; (3) **Spike-Timing-Dependent Plasticity (STDP)** for adaptive bond weight modulation; (4) **Dream Mode** for offline graph restructuring and memory consolidation; (5) **FedR**, a federated embedding training protocol requiring only 1,056 bytes per communication round; (6) **Unified Decayable** bond decay with per-`RelationType` decay constants $\lambda$; and (7) **Wikidata-inspired qualifiers** for temporal scoping and provenance annotation. Our implementation comprises 13 Rust modules totaling 8,515 lines of code with 217 tests and a 6-table `redb` persistent index, all integrated through an adapter pattern that achieves zero modifications to the P1–P5 foundation layers.

---

## Keywords

knowledge graph, bio-inspired computing, spike-timing-dependent plasticity, dream mode, RotatE, knowledge graph embeddings, federated learning, FedR, decentralized knowledge, CRDT, epistemic status, bond decay, event sourcing, spreading activation, memory consolidation, Rust, peer-to-peer, OneBrain

---

## Table of Contents

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': {'primaryColor': '#1a2332', 'primaryBorderColor': '#4ecdc4', 'lineColor': '#4ecdc4', 'secondaryColor': '#2d1b36', 'tertiaryColor': '#1a2332'}}}%%
graph LR
    A["📄 Ch.1<br/>Introduction"] --> B["📄 Ch.2<br/>Related Work"]
    B --> C["📄 Ch.3<br/>Architecture"]
    C --> D["📄 Ch.4<br/>Bio Mechanisms"]
    D --> E["📄 Ch.5<br/>Embeddings & FedR"]
    E --> F["📄 Ch.6<br/>Temporal & Qualifiers"]
    F --> G["📄 Ch.7<br/>Storage & Query"]
    G --> H["📄 Ch.8<br/>Cross-Pillar"]
    H --> I["📄 Ch.9<br/>Evaluation"]
    I --> J["📄 Ch.10<br/>Conclusion"]

    style A fill:#1a2332,stroke:#4ecdc4,stroke-width:2px,color:#e0e0e0
    style B fill:#1a2332,stroke:#4ecdc4,stroke-width:2px,color:#e0e0e0
    style C fill:#2d1b36,stroke:#ff6b9d,stroke-width:2px,color:#e0e0e0
    style D fill:#2d1b36,stroke:#ff6b9d,stroke-width:2px,color:#e0e0e0
    style E fill:#2d1b36,stroke:#ff6b9d,stroke-width:2px,color:#e0e0e0
    style F fill:#1a2332,stroke:#4ecdc4,stroke-width:2px,color:#e0e0e0
    style G fill:#1a2332,stroke:#4ecdc4,stroke-width:2px,color:#e0e0e0
    style H fill:#2d1b36,stroke:#ff6b9d,stroke-width:2px,color:#e0e0e0
    style I fill:#1a2332,stroke:#4ecdc4,stroke-width:2px,color:#e0e0e0
    style J fill:#1a2332,stroke:#4ecdc4,stroke-width:2px,color:#e0e0e0
```

1. [Introduction](01_introduction.md)
2. [Related Work](02_related_work.md)
3. [The Four-Layer OBKG Architecture](03_architecture.md)
4. [Bio-Inspired Graph Mechanisms](04_bio_mechanisms.md)
5. [Knowledge Graph Embeddings and Federated Training](05_embeddings_fedr.md)
6. [Temporal Knowledge and Bond Qualifiers](06_temporal_qualifiers.md)
7. [Persistent Storage and Graph Query Extensions](07_storage_query.md)
8. [Cross-Pillar Integration](08_cross_pillar.md)
9. [Evaluation](09_evaluation.md)
10. [Conclusion and Future Work](10_conclusion.md)

---

## List of Tables

| # | Title | Chapter |
|---|-------|---------|
| 1 | Comparison of Knowledge Graph Systems Across 12 Dimensions | §1.2 |
| 2 | Biological Metaphor Mapping to OBKG | §1.3 |
| 3 | Related Work Feature Matrix | §2 |
| 4 | Four-Layer Architecture Summary | §3 |
| 5 | RelationType Taxonomy (34 variants) | §3 |
| 6 | EpistemicStatus 11-Level Ladder | §3 |
| 7 | STDP Parameter Configuration | §4 |
| 8 | Dream Mode Operations | §4 |
| 9 | RotatE Quantization Comparison | §5 |
| 10 | FedR Communication Costs | §5 |
| 11 | Qualifier Schema | §6 |
| 12 | redb Table Layout | §7 |
| 13 | Cross-Pillar Integration Points | §8 |
| 14 | Benchmark Results | §9 |

## List of Figures

| # | Title | Chapter |
|---|-------|---------|
| 1 | OBKG Four-Layer Architecture | §3 |
| 2 | Bond Lifecycle State Machine | §4 |
| 3 | STDP Weight Modulation Curve | §4 |
| 4 | Dream Mode Pipeline | §4 |
| 5 | RotatE Embedding Space | §5 |
| 6 | FedR Communication Protocol | §5 |
| 7 | Temporal Qualifier Timeline | §6 |
| 8 | redb Index Architecture | §7 |
| 9 | Cross-Pillar Data Flow | §8 |
| 10 | Performance Benchmarks | §9 |
