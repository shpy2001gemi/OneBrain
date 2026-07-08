# OneBrain AI Layer: A Decentralized, Device-Aware Artificial Intelligence Framework for Progressive Knowledge Encoding in Peer-to-Peer Networks

> *"A computer would deserve to be called intelligent if it could deceive a human into believing that it was human."*
> — Alan Turing, *Computing Machinery and Intelligence* (1950)

---

**Authors:** OneBrain Project Contributors
**Contact:** shpy2001@gmail.com
**Date:** July 2026
**Version:** 1.0
**Pillar:** P6 — AI Layer

---

## Abstract

The transformation of unstructured human knowledge into machine-processable, verifiable, and composable representations remains one of the most challenging problems in knowledge engineering. Existing approaches to automated knowledge encoding suffer from four systemic limitations that render them inadequate for decentralized knowledge networks: (1) **cloud dependency**, requiring persistent connectivity to centralized API endpoints (OpenAI, Google, Anthropic) that introduce latency, cost, privacy risks, and single points of failure; (2) **model-centric architectures** that tightly couple knowledge extraction pipelines to specific model versions, creating brittle systems that break upon model updates or deprecation; (3) **absence of provenance preservation**, where extracted knowledge loses all traceability to its original source, author, evidence type, and epistemic status during the encoding process; and (4) **no hardware adaptation**, treating a smartphone with 4 GB RAM identically to a GPU workstation with 128 GB, resulting in either exclusion of resource-constrained devices or gross underutilization of capable hardware.

We present the **OneBrain AI Layer**, a decentralized, device-aware artificial intelligence framework designed as the cognitive encoding engine of the OneBrain knowledge network. The AI Layer introduces a four-component architecture — AI Runtime Engine, Encoding Pipeline, Model Management, and Personal AI Mediator — that composes over the existing OneBrain pillars (P1–P5, P7–P8) without requiring any modification to their codebases. We contribute seven key innovations: (1) a **three-tier progressive encoding pipeline** (rule-based → small model → large model) that optimizes the cost-quality tradeoff by routing knowledge of varying complexity to appropriate encoding tiers; (2) a **tool-calling paradigm** with 15 structured tools and grammar-constrained generation (GBNF) that makes invalid Knowledge Unit output impossible at the token sampling level; (3) a **7-tier device classification system** (T0–T6) with automatic model selection based on hardware profiling, memory budgeting, and thermal constraints; (4) **content-addressed model distribution** via the existing Kademlia DHT, enabling peer-to-peer model sharing with BLAKE3 integrity verification; (5) **metabolism-aware AI scheduling** that integrates AI resource consumption into the Proof-of-Metabolic-Value (PoMV) consensus framework; (6) a **Personal AI Mediator (PAM)** with hybrid retrieval-augmented generation combining semantic search, structured KQL queries, and graph traversal; and (7) an **encode-decode-compare verification pipeline** that validates AI-generated encodings through round-trip binary serialization and semantic similarity comparison. Our implementation comprises 3 Rust crates (`ku-ai`, `ku-encoder`, `ku-mediator`) totaling 6,158 lines of code, plus 1,729 LOC of AI tool infrastructure in `ku-core` (33 tests), for a grand total of **7,887 LOC** — integrated into a working CLI demo (`onebrain`) that performs end-to-end AI encoding via local Ollama models, P2P broadcasting, and peer verification.

---

## Keywords

artificial intelligence, knowledge encoding, decentralized AI, local-first inference, tool calling, function calling, grammar-constrained generation, GBNF, progressive encoding, device-aware computing, model management, personal AI, retrieval-augmented generation, knowledge extraction, GGUF, quantization, bio-inspired computing, privacy-preserving AI, on-device inference, Rust, CRDT, peer-to-peer, OneBrain

---

## Table of Contents

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': {'primaryColor': '#1a2332', 'primaryBorderColor': '#4ecdc4', 'lineColor': '#4ecdc4', 'secondaryColor': '#2d1b36', 'tertiaryColor': '#1a2332'}}}%%
graph LR
    A["📄 Ch.1<br/>Introduction"] --> B["📄 Ch.2<br/>Related Work"]
    B --> C["📄 Ch.3<br/>Architecture"]
    C --> D["📄 Ch.4<br/>Encoding Pipeline"]
    D --> E["📄 Ch.5<br/>Tool Framework"]
    E --> F["📄 Ch.6<br/>Runtime & Models"]
    F --> G["📄 Ch.7<br/>Personal Mediator"]
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
3. [The Four-Component AI Layer Architecture](03_architecture.md)
4. [Three-Tier Progressive Encoding Pipeline](04_encoding_pipeline.md)
5. [Tool-Calling Framework and Grammar-Constrained Generation](05_tool_framework.md)
6. [Device-Aware AI Runtime and Model Management](06_runtime_models.md)
7. [Personal AI Mediator](07_personal_mediator.md)
8. [Cross-Pillar Integration](08_cross_pillar.md)
9. [Evaluation](09_evaluation.md)
10. [Conclusion and Future Work](10_conclusion.md)

---

## List of Tables

| # | Title | Chapter |
|---|-------|---------|
| 1 | Comparison of Knowledge Encoding Systems Across 12 Dimensions | §1.4 |
| 2 | Biological Metaphor Mapping to AI Layer Components | §1.5 |
| 3 | Related Work Feature Matrix | §2.7 |
| 4 | Four-Component Architecture Summary | §3.2 |
| 5 | Three-Tier Encoding Characteristics | §4.1 |
| 6 | Encoding Quality by Model Size | §4.3 |
| 7 | Tool Definitions and JSON Schema Summary | §5.2 |
| 8 | Local AI Runtime Framework Comparison | §6.2 |
| 9 | 7-Tier Device Classification | §6.3 |
| 10 | Model Registry Schema | §6.6 |
| 11 | PAM Intent Taxonomy | §7.2 |
| 12 | Cross-Pillar Integration Points | §8.1 |
| 13 | Benchmark Results: Tool Executor Performance | §9.2 |
| 14 | Comprehensive System Comparison | §9.5 |

## List of Figures

| # | Title | Chapter |
|---|-------|---------|
| 1 | Four-Component AI Layer Architecture | §3.2 |
| 2 | End-to-End Data Flow: Text → KU Binary | §3.3 |
| 3 | Three-Tier Encoding Pipeline | §4.1 |
| 4 | Tier Router Decision Logic | §4.5 |
| 5 | Tool-Calling Sequence Diagram | §5.1 |
| 6 | GBNF Grammar-Constrained Token Sampling | §5.4 |
| 7 | Device Tier Classification and Model Mapping | §6.3 |
| 8 | P2P Model Distribution via DHT | §6.7 |
| 9 | PAM Hybrid RAG Pipeline | §7.4 |
| 10 | Cross-Pillar Integration Data Flow | §8.1 |

---

## Notation

| Symbol | Meaning |
|--------|---------|
| $\text{KU}$ | Knowledge Unit — the atomic unit of knowledge in OneBrain |
| $\text{CoreDna}$ | The binary encoding format for a Knowledge Unit (v6, 31 opcodes) |
| $\text{CID}$ | Content Identifier — BLAKE3 hash of serialized KU bytes |
| $\text{DID}$ | Decentralized Identifier — Ed25519-based author identity |
| $T_i$ | Device tier $i$ where $i \in \{0, 1, 2, 3, 4, 5, 6\}$ |
| $\tau_k$ | Encoding tier $k$ where $k \in \{1, 2, 3\}$ |
| $\mathcal{Q}$ | Quantization level $\in \{\text{F32}, \text{F16}, \text{INT8}, \text{INT4}\}$ |
| $\mathcal{M}$ | Model registry $\mathcal{M} = \{m_1, m_2, \ldots, m_n\}$ |
| $\text{GBNF}$ | GGML BNF — grammar format for constrained token generation |
| $c_{\text{score}}$ | Complexity score for tier routing |
| $\sigma_{\text{sem}}$ | Semantic similarity score $\in [0, 1]$ |
| $\phi_{\text{conf}}$ | Encoding confidence $= 0.6 \cdot \sigma_{\text{sem}} + 0.4 \cdot \iota_{\text{comp}}$ |
| $\iota_{\text{comp}}$ | Information completeness score $\in [0, 1]$ |
| PAM | Personal AI Mediator |
| PoMV | Proof-of-Metabolic-Value consensus mechanism |
| OBT | OneBrain Token — utility token for knowledge contribution rewards |
| OBKG | OneBrain Knowledge Graph |
| KQL | Knowledge Query Language |
| OBP | OneBrain Protocol — 9-layer P2P network stack |
