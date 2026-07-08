# 2. Related Work

> *"To understand where we are going, we must first understand where others have been."*

This section surveys existing knowledge graph systems, graph database technologies, decentralized graph architectures, bio-inspired graph computing, knowledge graph embeddings, and temporal knowledge graph models. For each domain, we identify the specific design choices, strengths, and limitations that motivate OBKG's architecture. We organize our analysis around a central question: *Can any existing system simultaneously provide decentralized operation, epistemic grading, biological bond dynamics, and federated embedding learning?*

## §2.1 Knowledge Graphs at Scale

**Knowledge graphs** (KGs) represent structured world knowledge as entities and relations, typically stored as triples *(head, relation, tail)*. Over the past two decades, several large-scale knowledge graphs have been constructed — each embodying different design philosophies regarding openness, schema expressiveness, and quality assurance. We survey eight prominent systems and extract lessons for OBKG.

### §2.1.1 Google Knowledge Graph

The **Google Knowledge Graph** [1] underpins Google Search, Assistant, and multiple internal products. As of 2024, it contains an estimated 500 billion facts about 5 billion entities [2], making it the largest proprietary knowledge graph. Google's system excels at entity disambiguation via Knowledge Panels and cross-lingual entity resolution. However, it is entirely **proprietary** — no public API exposes the full graph, no external party can contribute directly, and there is no published mechanism for epistemic grading or provenance tracking. Knowledge quality is maintained through automated extraction pipelines and manual curation, but the specific trust signals remain opaque. *Lesson for OBKG*: scale alone does not ensure openness or verifiability; OBKG must be fully open-source and decentralized from inception.

### §2.1.2 Wikidata

**Wikidata** [3] is the largest open, collaboratively edited knowledge graph, containing over 110 million items and 1.6 billion statements as of 2025. Its data model centers on *statements* augmented with **qualifiers** (key-value metadata such as `valid-from`, `valid-until`, `sourced-from`) and **references** (provenance chains). Wikidata introduces three **deprecation ranks** — preferred, normal, and deprecated — allowing outdated statements to coexist with current ones without deletion. This design directly inspires OBKG's `QualifiedBond` system (§3.5) and the `Rank` qualifier key. However, Wikidata remains centralized on Wikimedia servers, relies on human consensus for quality control, and provides no mechanism for automatic bond decay or embedding-based anomaly detection. *Lesson for OBKG*: qualifier-augmented statements and deprecation ranks are essential for epistemic management; OBKG extends these with automated decay curves and confidence tracking.

### §2.1.3 DBpedia

**DBpedia** [4] extracts structured data from Wikipedia infoboxes, generating approximately 6.6 million entities with 580 million facts across 125 languages. DBpedia pioneered the use of **IRIs** (Internationalized Resource Identifiers) for entity linking across datasets and established the Linked Open Data paradigm. Its primary limitation is **Wikipedia-dependency**: knowledge coverage mirrors Wikipedia's biases (over-representation of Western, English-language topics) and update latency tracks Wikipedia's editorial cycle. Furthermore, DBpedia's extraction pipeline introduces mapping errors that propagate unchecked due to the absence of epistemic scoring. *Lesson for OBKG*: automated extraction must be paired with quality signals; OBKG's PoMV integration (§3.8) provides per-bond metabolic validation.

### §2.1.4 YAGO

**YAGO** [5] introduced the **SPOTL quintuple** model: *(Subject, Predicate, Object, Time, Location)* — extending traditional triples with temporal and spatial qualifiers. YAGO 4 contains approximately 49 million entities derived from Wikidata and schema.org, with strict type-checking against the schema.org ontology. YAGO's temporal annotations (e.g., `valid-during: [1990, 2005]`) demonstrated the value of time-aware knowledge representation. However, YAGO's qualifiers are static once assigned, with no mechanism for automatic temporal decay or bond strength evolution. *Lesson for OBKG*: temporal and spatial qualifiers should be first-class citizens; OBKG implements both through `QualifierKey::ValidFrom`, `ValidUntil`, and `Location`, while extending the temporal model with exponential decay dynamics.

### §2.1.5 ConceptNet

**ConceptNet** [6] is a multilingual **commonsense knowledge graph** containing 8 million nodes connected by 34 relation types (e.g., `IsA`, `UsedFor`, `CapableOf`). Each assertion carries a static numerical weight reflecting contributor confidence at creation time. ConceptNet has been widely used for commonsense reasoning in NLP pipelines (e.g., pre-training BERT variants). Its key limitations include: (1) **static weights** — assertion confidence is fixed at creation and never updated; (2) **no provenance** — sources are recorded but not used for trust computation; and (3) **centralized hosting**. *Lesson for OBKG*: bond weights must be dynamic and responsive to usage patterns; OBKG's STDP mechanism (§3.6) continuously adjusts bond weights based on co-access timing.

### §2.1.6 Freebase

**Freebase** [7] pioneered the **Compound Value Type (CVT)** for representing n-ary relations — a design challenge that remains unsolved in most triple-based systems. At its peak, Freebase contained 39 million topics and 3 billion facts contributed by a community of over 100,000 editors. Google acquired Freebase in 2010 and archived it in 2016, migrating much of its data to Wikidata. Freebase demonstrated that community-edited knowledge graphs can achieve impressive scale, but also showed that centralized governance creates a single point of failure. *Lesson for OBKG*: n-ary relations require qualifier support (§3.5); community knowledge must survive the disappearance of any single steward through decentralization.

### §2.1.7 Cyc

**Cyc** [8] represents the most ambitious attempt at encoding commonsense knowledge, with approximately 500,000 concepts organized into context-scoped **microtheories**. Each assertion in Cyc is valid only within its declared microtheory, enabling contradictory statements to coexist in different contexts (e.g., "Earth is flat" is valid within `EverydayPerceptionMt`, while "Earth is an oblate spheroid" is valid within `GeophysicsMt`). This context-scoping mechanism directly inspires OBKG's `QualifierKey::Context` and per-bond context tagging. However, Cyc's microtheories are manually curated, making the system difficult to extend and resistant to automated knowledge integration. *Lesson for OBKG*: context scoping is essential for managing conflicting knowledge; OBKG automates context discovery through dream mode association detection (§3.7).

### §2.1.8 WordNet

**WordNet** [9] organizes 155,287 English words into 117,659 **synsets** (sets of cognitive synonyms), linked by semantic relations including hypernymy, hyponymy, meronymy, and antonymy. WordNet's design prioritizes lexical-semantic relationships over factual knowledge, making it complementary to entity-centric knowledge graphs. Its hierarchical structure has proven valuable for word sense disambiguation and ontology construction. *Lesson for OBKG*: lexical-semantic relations are a distinct relation class; OBKG's 33 `RelationType` variants (§3.1) include hierarchical relations (`Extends`, `Contains`, `PartOf`) inspired by WordNet's taxonomy.

### Table 1: Knowledge Graph Systems Comparison

| System | Scale (Entities) | Open Source | Decentralized | Epistemic Grading | Bond Decay | Qualifiers | Temporal | Embeddings |
|--------|:----------------:|:-----------:|:-------------:|:------------------:|:----------:|:----------:|:--------:|:----------:|
| Google KG [1] | ~5B | ✗ | ✗ | ✗ | ✗ | Internal | Internal | Internal |
| Wikidata [3] | 110M+ | ✓ | ✗ | Ranks (3-level) | ✗ | ✓ (rich) | ✓ | ✗ |
| DBpedia [4] | 6.6M | ✓ | ✗ | ✗ | ✗ | Limited | Limited | ✗ |
| YAGO [5] | 49M | ✓ | ✗ | ✗ | ✗ | SPOTL | ✓ (static) | ✗ |
| ConceptNet [6] | 8M | ✓ | ✗ | Static weights | ✗ | ✗ | ✗ | NumberBatch |
| Freebase [7] | 39M | Archived | ✗ | ✗ | ✗ | CVT | Limited | ✗ |
| Cyc [8] | 500K | Partial | ✗ | Microtheories | ✗ | Context | ✗ | ✗ |
| WordNet [9] | 155K words | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| **OBKG** | Unbounded | ✓ | ✓ | 5-level ladder | 4 curves | 8 keys + custom | ✓ (dynamic) | RotatE int8 |

*Table 1: Comparison of 8 knowledge graph systems and OBKG across 8 architectural dimensions. OBKG is unique in combining all eight capabilities.*

## §2.2 Graph Database Technologies

The choice of underlying storage engine profoundly impacts a knowledge graph's performance characteristics, query expressiveness, and scalability model. We survey six graph database technologies and explain why OBKG builds a native engine on **redb** [10].

### §2.2.1 Neo4j

**Neo4j** [11] is the most widely adopted property graph database, using a **labeled property graph** model with native graph storage (index-free adjacency). Neo4j's Cypher query language provides intuitive ASCII-art pattern matching. However, Neo4j's architecture assumes a single-server or clustered deployment model with strong consistency — fundamentally at odds with OBKG's eventual-consistency, peer-to-peer design. Neo4j's enterprise clustering (Neo4j Fabric) distributes reads but maintains centralized write coordination. Furthermore, Neo4j lacks built-in temporal versioning, bond decay, or CRDT-based replication.

### §2.2.2 TigerGraph

**TigerGraph** [12] emphasizes high-performance distributed graph analytics through its GSQL query language and massively parallel processing architecture. TigerGraph can traverse 100+ billion edges in real-time, making it suitable for fraud detection and recommendation systems. However, TigerGraph is proprietary, requires centralized cluster management, and its GSQL language is imperative rather than declarative — limiting adoption for knowledge-centric workloads.

### §2.2.3 SurrealDB

**SurrealDB** [13] is a multi-model database supporting document, graph, and relational paradigms with a unified SurrealQL query language. Its built-in record links enable graph-like traversals. SurrealDB supports real-time subscriptions and multi-tenancy. However, it lacks native CRDT support, does not implement content-addressing, and provides no bio-inspired graph mechanisms such as decay or consolidation.

### §2.2.4 CozoDB

**CozoDB** [14] is an embedded graph database implementing Datalog as its query language, with built-in support for vector search (HNSW), full-text search, and graph algorithms (shortest path, PageRank). CozoDB stores data as relations and evaluates recursive Datalog queries efficiently. Its embedded architecture aligns with OBKG's local-first philosophy, but CozoDB does not support CRDT-based replication, federated training, or temporal bond dynamics.

### §2.2.5 TerminusDB

**TerminusDB** [15] provides **Git-like version control** for graph data, with branch, merge, diff, and time-travel capabilities using an append-only immutable store. TerminusDB's temporal model enables querying any historical graph state — a capability that inspired OBKG's `EventAccumulator.replay_at_time()` (§3.3). However, TerminusDB's version control model assumes a centralized repository, uses WOQL (a JSON-based query language) rather than a graph-native syntax, and does not support decentralized replication.

### §2.2.6 Oxigraph

**Oxigraph** [16] is a lightweight, embedded RDF triple store written in Rust, supporting SPARQL queries and various serialization formats. Its Rust implementation and embedded architecture align with OBKG's technology choices. However, Oxigraph's RDF data model lacks property graph expressiveness, does not support qualified edges, and provides no built-in embedding or decay mechanisms.

### Table 2: Graph Database Technology Comparison

| System | Language | Distributed | Index Model | Query Language | Time-Travel | CRDT Support |
|--------|:--------:|:-----------:|:-----------:|:--------------:|:-----------:|:------------:|
| Neo4j [11] | Java | Clustered | Index-free adjacency | Cypher | ✗ | ✗ |
| TigerGraph [12] | C++ | MPP cluster | Compressed adjacency | GSQL | ✗ | ✗ |
| SurrealDB [13] | Rust | Multi-node | Document + links | SurrealQL | ✗ | ✗ |
| CozoDB [14] | Rust | Embedded | Relational (Datalog) | CozoScript | ✗ | ✗ |
| TerminusDB [15] | Prolog/Rust | Centralized | Immutable append | WOQL | ✓ (Git-style) | ✗ |
| Oxigraph [16] | Rust | Embedded | B-tree (RDF) | SPARQL | ✗ | ✗ |
| **OBKG** | Rust | P2P (gossip) | 6 redb tables | KQL (§4) | ✓ (event replay) | ✓ (5 types) |

*Table 2: Graph database technology comparison. OBKG's native engine on redb uniquely combines P2P distribution, event-sourced time-travel, and CRDT-based replication — capabilities absent from all surveyed systems.*

**Why OBKG builds a native engine on redb.** Existing graph databases assume centralized or clustered deployments with strong consistency guarantees. OBKG's requirements — (1) embedded operation on edge devices, (2) CRDT-based eventual consistency across gossip networks, (3) per-relation-type decay curves, (4) event-sourced time-travel queries, and (5) RotatE embedding storage — are not supported by any existing system. The **redb** [10] embedded key-value store provides pure-Rust ACID transactions, zero-copy reads, and sub-millisecond latency, making it the ideal foundation for OBKG's six specialized index tables (§3.2).

## §2.3 Decentralized Graph Systems

OBKG operates in a fully decentralized, peer-to-peer environment. We survey existing decentralized data systems and assess their suitability for knowledge graph workloads.

### §2.3.1 The Graph

**The Graph** [17] is a decentralized indexing protocol for querying blockchain data using GraphQL. Indexers stake GRT tokens to operate subgraphs, and curators signal which subgraphs are valuable. The Graph demonstrates that decentralized indexing is viable, but its design is specialized for blockchain event data rather than general knowledge representation. It does not support property graphs, bond dynamics, or knowledge-specific query operations.

### §2.3.2 IPLD (InterPlanetary Linked Data)

**IPLD** [18] provides a content-addressed data model for linking data across distributed systems. Every IPLD node is identified by its **CID** (Content Identifier) — a self-describing hash. IPLD's Merkle-DAG structure ensures data integrity and enables deduplication. OBKG adopts CID-based content addressing for Knowledge Units (§3.1), ensuring that every KU has a globally unique, verifiable identifier. However, IPLD provides no graph query capabilities, no temporal model, and no embedding support.

### §2.3.3 AT Protocol (Merkle Search Trees)

The **AT Protocol** [19] (Bluesky) uses **Merkle Search Trees (MSTs)** for authenticated data structures, enabling users to self-certify their data repositories. MSTs provide $O(\log n)$ lookup with cryptographic verification at every node. OBKG's authenticated bond storage draws inspiration from MST-style Merkle proofs, but extends the model with temporal qualifiers and bond-weight authentication.

### §2.3.4 OrbitDB and Merkle-CRDTs

**OrbitDB** [20] builds databases on top of IPFS using **Merkle-CRDTs** — CRDTs whose operations are stored in Merkle-DAGs for verifiable, append-only replication. Merkle-CRDTs combine the consistency guarantees of CRDTs with the integrity guarantees of content-addressed storage. OBKG's CRDT design (G-Counter for metabolism, LWW-Register for bond state, OR-Set for qualifiers) follows this philosophy, extending it with graph-specific CRDT operations for bond lifecycle management.

### §2.3.5 Holochain

**Holochain** [21] implements an agent-centric distributed computing framework where each agent maintains their own source chain (append-only hashchain) and shares data through a validating DHT. Holochain's **intrinsic data integrity** model — data is validated by its DNA rules, not by global consensus — aligns with OBKG's per-node validation philosophy. However, Holochain does not provide graph-native data structures, embedding support, or knowledge-specific operations.

### §2.3.6 GossipSub

**GossipSub** [22] is a topic-based pub/sub protocol used by libp2p, Ethereum 2.0, and Filecoin for reliable message dissemination across P2P networks. GossipSub combines mesh-based eager push with gossip-based lazy pull, achieving both reliability and scalability. OBKG's gossip layer for FedR delta exchange (§3.9) and metabolism propagation builds on GossipSub-style hybrid dissemination.

### §2.3.7 Merkle-CRDTs (Formal Model)

**Merkle-CRDTs** [23] formalize the combination of Merkle-DAGs with CRDTs, proving that CRDTs replicated over Merkle-DAG transports inherit both causal consistency from CRDTs and tamper-evidence from Merkle structures. This theoretical framework validates OBKG's design choice of using CRDT-based bond state management over content-addressed storage.

### Table 3: Decentralized Graph System Comparison

| System | Content-Addressed | P2P Native | CRDT Support | Graph-Native | KG Features | Privacy Model |
|--------|:------------------:|:----------:|:------------:|:------------:|:-----------:|:-------------:|
| The Graph [17] | ✗ (blockchain) | Via staking | ✗ | GraphQL only | ✗ | Public chain |
| IPLD [18] | ✓ (CID) | Via IPFS | ✗ | DAG only | ✗ | Content-hash |
| AT Protocol [19] | ✓ (MST) | ✓ | ✗ | ✗ | ✗ | DID-based |
| OrbitDB [20] | ✓ (Merkle-CRDT) | Via IPFS | ✓ | ✗ | ✗ | Encryption |
| Holochain [21] | ✓ (hashchain) | ✓ | ✗ (DHT validation) | ✗ | ✗ | Agent-centric |
| GossipSub [22] | ✗ | ✓ | ✗ | ✗ | ✗ | Topic-based |
| **OBKG** | ✓ (BLAKE3 CID) | ✓ (OBP) | ✓ (5 types) | ✓ (property graph) | ✓ (full KG) | Per-bond context |

*Table 3: Decentralized system comparison. No existing decentralized system provides native property graph support with KG-specific features. OBKG combines content-addressing, P2P gossip, CRDTs, and a full knowledge graph model.*

## §2.4 Bio-Inspired Graph Computing

OBKG draws heavily on biological metaphors for its dynamic mechanisms. We survey the biological models that inspire each component.

### §2.4.1 Small-World Networks and Connectomics

**Watts and Strogatz** [24] demonstrated that many real-world networks exhibit the **small-world property**: high clustering coefficient combined with short average path length. Neural connectomes in particular show small-world topology, enabling efficient information propagation with minimal wiring. OBKG's graph topology emerges organically through bond creation and STDP strengthening, and dream mode association discovery (§3.7) specifically targets cross-cluster connections — maintaining short path lengths between knowledge domains.

### §2.4.2 Ant Colony Optimization and Stigmergy

**Dorigo and Stützle** [25] formalized **Ant Colony Optimization (ACO)**, where artificial ants deposit pheromone on solution components, guiding subsequent ants toward better solutions through indirect coordination (**stigmergy**). In OBKG, the `SynapticMap` (§3.6) implements digital stigmergy: when two Knowledge Units are co-accessed, their connecting bond is strengthened — creating "pheromone trails" that guide future knowledge traversal. The PoMV Synaptic signal (co-retrieval patterns creating emergent learning paths) directly parallels ant pheromone-based path optimization.

### §2.4.3 Danger Theory and Immune System Models

**Matzinger's Danger Theory** [26] proposed that the immune system responds to **danger signals** from damaged cells rather than distinguishing "self" from "non-self." This insight reframes immune detection from identity-based to behavior-based. OBKG's immune system (§3.4) implements this principle through **content-agnostic spread analysis**: rather than judging *what* knowledge says, the immune engine detects *how* knowledge spreads. Four structural antibody types — `LowTripleScore`, `ClusterOutlier`, `TemporalDrift`, and `InverseViolation` — detect anomalous structural patterns without examining content, analogous to danger-signal recognition.

### §2.4.4 Ecological Carrying Capacity

The **Lotka-Volterra equations** [27] model population dynamics through carrying capacity — the maximum population size an environment can sustain. When a niche reaches carrying capacity, additional organisms provide diminishing marginal benefit. OBKG applies this ecological metaphor through the PoMV **Niche signal**: the 1,001st Knowledge Unit about "how to boil water" has near-zero marginal value, while the first KU about a novel topic has maximum entropy. Density-dependent bond scoring prevents knowledge graph bloat while preserving diversity.

### §2.4.5 Spike-Timing-Dependent Plasticity (STDP)

**Markram et al.** [28] discovered that synaptic strength between neurons depends on the precise timing of pre- and post-synaptic spikes. When a pre-synaptic spike precedes a post-synaptic spike (causal timing, $\Delta t > 0$), the synapse strengthens (**Long-Term Potentiation**, LTP); when the order is reversed ($\Delta t < 0$), the synapse weakens (**Long-Term Depression**, LTD). The weight change follows:

$$\Delta w = A_{\pm} \times \exp\left(-\frac{|\Delta t|}{\tau}\right)$$

OBKG's `StdpEngine` (§3.6) implements this mechanism directly: when a user accesses KU $A$ and then KU $B$ within a time window, the bond $A \rightarrow B$ is strengthened (LTP), reinforcing observed knowledge navigation patterns. Reversed access order triggers LTD, weakening bonds that do not reflect actual usage.

### §2.4.6 Memory Consolidation

**Rasch and Born** [29] established that memory consolidation during sleep involves two complementary processes: (1) **replay** of recent experiences in the hippocampus, strengthening neural pathways; and (2) **association discovery** through pattern completion across stored memories. OBKG's `DreamEngine` (§3.7) implements both processes:

- **Replay Phase**: bonds accessed during waking hours are revisited and strengthened during dream cycles, analogous to hippocampal replay.
- **Association Phase**: RotatE embedding similarity is used to discover latent connections between knowledge units that were never explicitly linked, analogous to cross-cortical association formation.
- **Pruning Phase**: dream-discovered bonds that are never subsequently accessed are removed after a 7-day grace period, analogous to synaptic pruning during development.

## §2.5 Knowledge Graph Embeddings

**Knowledge graph embedding** (KGE) models learn low-dimensional vector representations of entities and relations, enabling link prediction, entity classification, and knowledge completion. We survey the major families of KGE models and justify OBKG's choice of RotatE with int8 quantization.

### §2.5.1 Translational Models

**TransE** [30] models relations as translations in embedding space: $\mathbf{h} + \mathbf{r} \approx \mathbf{t}$, where $\mathbf{h}$, $\mathbf{r}$, $\mathbf{t}$ are head, relation, and tail embeddings respectively. TransE is simple and effective for one-to-one relations but cannot model symmetric, antisymmetric, inverse, or compositional relation patterns.

### §2.5.2 Bilinear Models

**DistMult** [31] uses a diagonal bilinear scoring function: $f(h, r, t) = \mathbf{h}^T \text{diag}(\mathbf{r}) \mathbf{t}$. DistMult naturally models symmetric relations but cannot model antisymmetric or inverse relations. **ComplEx** [32] extends DistMult to the complex domain, using Hermitian dot products to model both symmetric and antisymmetric relations through the imaginary components.

### §2.5.3 Rotational Models

**RotatE** [33] models relations as **rotations in complex space**: $\mathbf{t} = \mathbf{h} \circ \mathbf{r}$, where $\circ$ denotes the Hadamard (element-wise) product and $|r_i| = 1$ for all components. RotatE can model all four fundamental relation patterns:

| Pattern | Definition | RotatE Mechanism |
|---------|-----------|-----------------|
| Symmetric | $r(x, y) \Rightarrow r(y, x)$ | $\mathbf{r} = \pm 1$ (180° or 0°) |
| Antisymmetric | $r(x, y) \Rightarrow \neg r(y, x)$ | $\mathbf{r} \neq \pm 1$ (arbitrary angle) |
| Inverse | $r_1(x, y) \Rightarrow r_2(y, x)$ | $\mathbf{r}_2 = \overline{\mathbf{r}_1}$ (conjugate) |
| Composition | $r_1(x, y) \wedge r_2(y, z) \Rightarrow r_3(x, z)$ | $\mathbf{r}_3 = \mathbf{r}_1 \circ \mathbf{r}_2$ |

### §2.5.4 Hierarchical Models

**HAKE** [34] extends rotational embeddings with a modulus component for modeling hierarchical relations, distinguishing entities at different levels of a hierarchy. HAKE achieves state-of-the-art performance on datasets with rich hierarchical structure.

### §2.5.5 Graph Neural Network Approaches

**R-GCN** [35] applies Graph Convolutional Networks to knowledge graphs with relation-specific weight matrices, enabling inductive learning over graph structure. **GraphSAGE** [36] introduces an inductive learning framework that samples and aggregates features from local neighborhoods, enabling generalization to unseen nodes. While GNN-based approaches achieve strong performance, they require centralized training on the full graph — incompatible with OBKG's decentralized architecture.

### Table 4: Knowledge Graph Embedding Model Comparison

| Model | Scoring Function | FB15k-237 MRR | FB15k-237 Hits@10 | Symmetric | Antisymmetric | Inverse | Composition |
|-------|:----------------:|:-------------:|:------------------:|:---------:|:-------------:|:-------:|:-----------:|
| TransE [30] | $\|\mathbf{h} + \mathbf{r} - \mathbf{t}\|$ | 0.294 | 0.465 | ✗ | ✓ | ✗ | ✗ |
| DistMult [31] | $\mathbf{h}^T \text{diag}(\mathbf{r}) \mathbf{t}$ | 0.241 | 0.419 | ✓ | ✗ | ✗ | ✗ |
| ComplEx [32] | $\text{Re}(\mathbf{h}^T \text{diag}(\mathbf{r}) \overline{\mathbf{t}})$ | 0.247 | 0.428 | ✓ | ✓ | ✓ | ✗ |
| RotatE [33] | $\|\mathbf{h} \circ \mathbf{r} - \mathbf{t}\|$ | 0.338 | 0.533 | ✓ | ✓ | ✓ | ✓ |
| HAKE [34] | Modulus + Phase | 0.346 | 0.542 | ✓ | ✓ | ✓ | ✓ |
| R-GCN [35] | GNN aggregation | 0.249 | 0.417 | ✓ | ✓ | ✓ | ✗ |
| GraphSAGE [36] | Neighborhood sampling | — | — | ✓ | ✓ | ✗ | ✗ |

*Table 4: KGE model comparison on FB15k-237 benchmark. RotatE offers the best balance of pattern support, performance, and computational simplicity.*

**Why RotatE with int8 quantization.** OBKG selects RotatE [33] for three reasons: (1) **universal pattern support** — RotatE models all four fundamental relation patterns through the single mechanism of complex rotation; (2) **computational simplicity** — the scoring function requires only element-wise multiplication and L1/L2 distance, avoiding matrix multiplications; (3) **quantization-friendly** — rotation angles can be represented as int8 values ($[-128, 127]$ mapping to $[-\pi, \pi]$) with minimal accuracy loss, reducing per-entity storage from 512 bytes (float32, $d=64$) to 70 bytes (int8, 32 complex dimensions). This 7.3× compression enables embedding storage on resource-constrained edge devices. OBKG further extends RotatE with **federated training** via the FedR protocol (§3.9), where peers exchange compact ~2KB embedding deltas through gossip rather than requiring centralized access to the full graph.

## §2.6 Temporal and Dynamic Knowledge Graphs

Real-world knowledge evolves over time: facts become outdated, relations strengthen or weaken, and new connections emerge. We survey approaches to temporal knowledge representation and identify gaps that motivate OBKG's dynamic bond model.

### §2.6.1 Temporal Knowledge Graph Embedding Models

**T-TransE** [37] extends TransE by incorporating temporal information into the scoring function, learning time-specific relation embeddings. **HyTE** [38] projects entities and relations onto time-specific hyperplanes, enabling temporal link prediction. **RE-NET** [39] uses recurrent event networks to model temporal interactions, capturing both global and local structural dependencies over time. **CyGNet** [40] introduces a copy-generation mechanism for temporal knowledge graphs, predicting future events by combining repetition patterns with global graph structure.

These temporal KGE models treat time as an additional input dimension for prediction. In contrast, OBKG treats temporality as an **intrinsic property of bond dynamics**: bonds decay exponentially with relation-type-specific half-lives, are strengthened through STDP co-activation, and are periodically consolidated during dream cycles. This represents a shift from *temporal prediction* to *temporal evolution*.

### §2.6.2 Allen's Temporal Algebra

**Allen's interval algebra** [41] defines 13 fundamental relations between temporal intervals (before, meets, overlaps, during, starts, finishes, equals, and their inverses). Allen's algebra provides a complete framework for temporal reasoning. OBKG's `QualifiedBond` system supports Allen-compatible temporal queries through `ValidFrom` and `ValidUntil` qualifiers: the `is_valid_at(timestamp)` method resolves interval containment, while the `bonds_in_time_range(from, to)` storage query enables overlap detection.

### §2.6.3 Pearl's Causal Ladder

**Pearl's causal hierarchy** [42] distinguishes three levels of causal reasoning:

1. **Association** (seeing): $P(Y|X)$ — observing correlations.
2. **Intervention** (doing): $P(Y|\text{do}(X))$ — predicting effects of actions.
3. **Counterfactual** (imagining): $P(Y_X|X', Y')$ — reasoning about alternative outcomes.

OBKG's bond model operates primarily at Level 1 (association via co-access patterns and embedding similarity) with emerging Level 2 capabilities through the `Causes` relation type and PoMV Prediction signal (implicit interventional reasoning). Full Level 3 counterfactual reasoning over knowledge graphs remains an open challenge.

## §2.7 Summary and Positioning

Our survey reveals that while individual aspects of OBKG's design have precedents, no existing system combines all of OBKG's capabilities. Table 5 summarizes the specific gaps in existing systems and OBKG's solutions.

### Table 5: Gap Analysis — Existing Systems vs. OBKG

| # | Capability Gap | Existing Systems | OBKG Solution |
|:-:|----------------|-----------------|---------------|
| 1 | **Decentralized KG operation** | All major KGs (Google, Wikidata, DBpedia) are centralized or server-dependent | P2P gossip via OBP network; CRDT-based eventual consistency; no single point of failure |
| 2 | **Dynamic bond decay** | All surveyed KGs use static edge weights or manual deprecation | Per-relation-type exponential decay with 4 curves ($\lambda$ from `decay_lambda()`); automatic weight evolution |
| 3 | **Bio-inspired plasticity** | No KG system implements neural-inspired weight dynamics | STDP for co-activation strengthening/weakening; dream-mode replay and association discovery |
| 4 | **Federated embedding training** | All KGE methods (TransE, RotatE, R-GCN) assume centralized training | FedR protocol: local SGD + compact ~2KB delta gossip + FedAvg aggregation |
| 5 | **Epistemic grading** | Wikidata: 3 static ranks; most KGs: none | 5-level epistemic ladder integrated with PoMV signals and observation-based transitions |
| 6 | **Qualifier-augmented bonds** | Wikidata: rich qualifiers but static; YAGO: SPOTL but no dynamics | 8 typed qualifier keys + custom; temporal validity, confidence, source, context — all CRDT-replicated |
| 7 | **Content-agnostic anomaly detection** | Graph databases provide no built-in anomaly detection | 4 structural antibody types using embedding-based detection; immune memory via VacuumFilter |
| 8 | **Event-sourced time-travel** | TerminusDB: Git-style versioning (centralized) | EventAccumulator with `replay_at_time()`; decentralized event logs via CRDT gossip |
| 9 | **Quantized edge-device embeddings** | All KGE models use float32/float64 | RotatE int8 quantization: 70 bytes/entity (7.3× compression); runs on Raspberry Pi |
| 10 | **Cross-pillar integration** | Graph systems operate as isolated components | OBKG adapts to KU Core (P1), PoMV (P2), KQL (P3), OBP (P4), OBT (P5) without modifying foundation code |

*Table 5: Gap analysis identifying 10 capability gaps in existing systems and OBKG's solutions. To our knowledge, no existing system combines decentralized operation, bio-inspired bond dynamics, federated embedding training, epistemic grading, qualifier-augmented bonds, and content-agnostic anomaly detection within a single coherent knowledge graph architecture.*

---

## References

[1] N. Noy, Y. Gao, A. Jain, A. Naber, A. Patterson, and K. Taylor, "Industry-scale knowledge graphs: Lessons and challenges," *Queue*, vol. 17, no. 2, pp. 48–75, 2019.

[2] Google, "How we help you find information with the Knowledge Graph," Google Blog, 2023. [Online]. Available: https://blog.google/products/search/about-knowledge-graph-and-knowledge-panels/

[3] D. Vrandečić and M. Krötzsch, "Wikidata: A free collaborative knowledgebase," *Commun. ACM*, vol. 57, no. 10, pp. 78–85, 2014.

[4] J. Lehmann *et al.*, "DBpedia — A large-scale, multilingual knowledge base extracted from Wikipedia," *Semantic Web*, vol. 6, no. 2, pp. 167–195, 2015.

[5] F. M. Suchanek, G. Kasneci, and G. Weikum, "YAGO: A core of semantic knowledge," in *Proc. 16th Int. Conf. World Wide Web (WWW '07)*, 2007, pp. 697–706.

[6] R. Speer and C. Havasi, "ConceptNet 5.5: An open multilingual graph of general knowledge," in *Proc. 31st AAAI Conf. Artificial Intelligence*, 2017, pp. 4444–4451.

[7] K. Bollacker, C. Evans, P. Paritosh, T. Sturge, and J. Taylor, "Freebase: A collaboratively created graph database for structuring human knowledge," in *Proc. ACM SIGMOD Int. Conf. Management of Data*, 2008, pp. 1247–1250.

[8] D. B. Lenat, "CYC: A large-scale investment in knowledge infrastructure," *Commun. ACM*, vol. 38, no. 11, pp. 33–38, 1995.

[9] G. A. Miller, "WordNet: A lexical database for English," *Commun. ACM*, vol. 38, no. 11, pp. 39–41, 1995.

[10] C. Berner, "redb: An embedded key-value store written in pure Rust," 2023. [Online]. Available: https://github.com/cberner/redb

[11] Neo4j, Inc., "The Neo4j graph platform," 2024. [Online]. Available: https://neo4j.com/

[12] A. Deutsch, Y. Xu, M. Wu, and V. Lee, "TigerGraph: A native MPP graph database," *arXiv preprint arXiv:1901.08248*, 2019.

[13] SurrealDB Ltd., "SurrealDB: The ultimate multi-model database," 2024. [Online]. Available: https://surrealdb.com/

[14] Z. Hao, "CozoDB: An embedded transactional graph database," 2023. [Online]. Available: https://github.com/cozodb/cozo

[15] G. Faaborg *et al.*, "TerminusDB: An open-source graph database for collaborative data-intensive applications," in *Proc. ISWC (Posters & Demos)*, 2021.

[16] T. Tanon, "Oxigraph: A SPARQL database and toolkit in Rust," 2023. [Online]. Available: https://github.com/oxigraph/oxigraph

[17] Y. Ramaswamy and J. Barber, "The Graph: An indexing protocol for querying networks like Ethereum and IPFS," The Graph Foundation, Tech. Rep., 2020.

[18] Protocol Labs, "IPLD: InterPlanetary Linked Data," 2021. [Online]. Available: https://ipld.io/

[19] J. Graber, "The AT Protocol specification," Bluesky PBLLC, 2023. [Online]. Available: https://atproto.com/

[20] M. Lanzinger, "OrbitDB: A peer-to-peer database for the decentralized web," 2023. [Online]. Available: https://github.com/orbitdb/orbitdb

[21] A. Brock, E. Harris-Braun, N. Luck, and M. Mealling, "Holochain: A framework for distributed applications," Holo Ltd., White Paper, 2018.

[22] D. Vyzovitis *et al.*, "GossipSub: Attack-resilient message propagation in the Filecoin and ETH2.0 networks," *arXiv preprint arXiv:2007.02754*, 2020.

[23] H. Sanjuán, S. Poyhtari, P. Teixeira, and I. Psaras, "Merkle-CRDTs: Merkle-DAGs meet CRDTs," *arXiv preprint arXiv:2004.00107*, 2020.

[24] D. J. Watts and S. H. Strogatz, "Collective dynamics of 'small-world' networks," *Nature*, vol. 393, no. 6684, pp. 440–442, 1998.

[25] M. Dorigo and T. Stützle, *Ant Colony Optimization*. Cambridge, MA: MIT Press, 2004.

[26] P. Matzinger, "The danger model: A renewed sense of self," *Science*, vol. 296, no. 5566, pp. 301–305, 2002.

[27] J. D. Murray, *Mathematical Biology: I. An Introduction*, 3rd ed. New York: Springer, 2002.

[28] H. Markram, J. Lübke, M. Frotscher, and B. Sakmann, "Regulation of synaptic efficacy by coincidence of postsynaptic APs and EPSPs," *Science*, vol. 275, no. 5297, pp. 213–215, 1997.

[29] B. Rasch and J. Born, "About sleep's role in memory," *Physiological Reviews*, vol. 93, no. 2, pp. 681–766, 2013.

[30] A. Bordes, N. Usunier, A. Garcia-Durán, J. Weston, and O. Yakhnenko, "Translating embeddings for modeling multi-relational data," in *Proc. NIPS*, 2013, pp. 2787–2795.

[31] B. Yang, W.-t. Yih, X. He, J. Gao, and L. Deng, "Embedding entities and relations for learning and inference in knowledge bases," in *Proc. ICLR*, 2015.

[32] T. Trouillon, J. Welbl, S. Riedel, É. Gaussier, and G. Bouchard, "Complex embeddings for simple link prediction," in *Proc. ICML*, 2016, pp. 2071–2080.

[33] Z. Sun, Z.-H. Deng, J.-Y. Nie, and J. Tang, "RotatE: Knowledge graph embedding by relational rotation in complex space," in *Proc. ICLR*, 2019.

[34] Z. Zhang, J. Cai, Y. Zhang, and J. Wang, "Learning hierarchy-aware knowledge graph embeddings for link prediction," in *Proc. AAAI*, 2020, pp. 3065–3072.

[35] M. Schlichtkrull, T. N. Kipf, P. Bloem, R. van den Berg, I. Titov, and M. Welling, "Modeling relational data with graph convolutional networks," in *Proc. ESWC*, 2018, pp. 593–607.

[36] W. L. Hamilton, R. Ying, and J. Leskovec, "Inductive representation learning on large graphs," in *Proc. NIPS*, 2017, pp. 1024–1034.

[37] S. Jiang, D. Lowd, S. Kafle, and D. Dou, "Encoding temporal information for time-aware link prediction," in *Proc. EMNLP*, 2016, pp. 2350–2354.

[38] S. Dasgupta, S. N. Ray, and P. Talukdar, "HyTE: Hyperplane-based temporally aware knowledge graph embedding," in *Proc. EMNLP*, 2018, pp. 2001–2011.

[39] W. Jin, M. Qu, X. Jin, and X. Ren, "Recurrent event network: Autoregressive structure inference over temporal knowledge graphs," in *Proc. EMNLP*, 2020, pp. 6669–6683.

[40] C. Zhu, M. Chen, C. Fan, G. Cheng, and Y. Zhang, "Learning from history: Modeling temporal knowledge graphs with distributed representations," in *Proc. AAAI*, 2021, pp. 4763–4770.

[41] J. F. Allen, "Maintaining knowledge about temporal intervals," *Commun. ACM*, vol. 26, no. 11, pp. 832–843, 1983.

[42] J. Pearl, *Causality: Models, Reasoning, and Inference*, 2nd ed. Cambridge, UK: Cambridge University Press, 2009.

[43] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "A comprehensive study of convergent and commutative replicated data types," INRIA, Res. Rep. RR-7506, 2011.

[44] P. S. Almeida, A. Shoker, and C. Baquero, "Delta state replicated data types," *J. Parallel Distrib. Comput.*, vol. 111, pp. 162–173, 2018.

[45] S. D. Kamvar, M. T. Schlosser, and H. Garcia-Molina, "The EigenTrust algorithm for reputation management in P2P networks," in *Proc. 12th Int. Conf. World Wide Web (WWW '03)*, 2003, pp. 640–651.

[46] S. Vosoughi, D. Roy, and S. Aral, "The spread of true and false news online," *Science*, vol. 359, no. 6380, pp. 1146–1151, 2018.

[47] D. Dasgupta, *Artificial Immune Systems and Their Applications*. Berlin: Springer, 1999.

[48] S. Nakamoto, "Bitcoin: A peer-to-peer electronic cash system," 2008. [Online]. Available: https://bitcoin.org/bitcoin.pdf
