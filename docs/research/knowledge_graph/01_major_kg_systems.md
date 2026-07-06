# Major Knowledge Graph Systems — Survey for OBKG

> **Author**: OneBrain Research Team  
> **Date**: 2026-07-02  
> **Purpose**: Extract innovations from existing Knowledge Graph systems applicable to OneBrain Knowledge Graph (OBKG)

---

## Executive Summary

This survey examines eight major Knowledge Graph (KG) systems — Google Knowledge Graph, Wikidata, DBpedia, YAGO, ConceptNet, Freebase, Cyc/OpenCyc, and WordNet — to identify architectural patterns, schema designs, and operational strategies that can inform the design of OneBrain's Knowledge Graph (OBKG). Each system represents a distinct philosophy in knowledge representation: from Google's proprietary, AI-driven entity resolution at web scale (~51 billion entities), to Cyc's hand-curated microtheory-based common sense reasoning (~500K concepts with trillions of inferrable facts), to ConceptNet's crowdsourced common-sense network (~8M concepts, 34 relation types).

OneBrain occupies a unique position in this landscape as a **decentralized, binary-encoded, bio-inspired** knowledge network. Its 33 RelationType taxonomy across 8 categories (Epistemic, Structural, Causal, Derivation, Similarity, Temporal, Provenance, Experiential) is more semantically rich than most surveyed systems. Its 11-level EpistemicStatus classification (Rumor → Axiomatic) is unmatched — no other system provides granular epistemic grading at the protocol level. However, several systems offer innovations that OBKG can adopt: Wikidata's statement ranking and deprecation model for conflict management, YAGO's SPOTL quintuple model for spatiotemporal facts, Cyc's microtheory architecture for context-dependent reasoning, Freebase's Compound Value Types for n-ary relations, and ConceptNet's weighted edge model for defeasible common-sense knowledge.

The most critical lessons for OBKG center on four themes: (1) **Conflict handling** — adopting Wikidata's rank-based coexistence rather than deletion, perfectly aligned with PoMV's philosophy; (2) **Spatiotemporal representation** — extending the Temporal category (currently only Precedes/Cooccurs) with YAGO-style location anchoring; (3) **Context scoping** — adapting Cyc's microtheory concept as lightweight "knowledge contexts" within KU metadata; and (4) **Provenance depth** — learning from the PROV-O standard and Freebase's CVT model to enrich the existing Provenance category (Cites/AuthoredBy/ReviewedBy).

---

## System-by-System Analysis

### 1. Google Knowledge Graph

**Architecture:**  
Google KG uses a proprietary graph database structure where entities are identified by unique Machine IDs (MIDs). It follows an Entity-Attribute-Value-Evidence (EAV-E) model. Data is ingested from public datasets (Wikipedia, Wikidata), licensed data, Schema.org structured markup from the web, and internal AI-powered extraction pipelines. Entity reconciliation (deduplication, clustering) is performed via AI engines to merge mentions like "Bob Smith" and "Robert Smith" to a single entity.

**Key Innovation:**  
- **"Strings to Things"** paradigm shift — moving from keyword matching to entity understanding
- **Data River architecture** — continuous, real-time graph updates (replacing batch "data lake" refreshes)
- **Entity reconciliation at web scale** — AI-powered deduplication across billions of entities
- **Generative AI integration** — the KG serves as the factual backbone for Gemini and AI Overviews

**Schema Design:**  
Schema.org vocabulary with JSON-LD for structured data. Uses `sameAs` linking for entity resolution across sources. No fixed ontology — schema is flexible and emergent from web-scale data aggregation.

**Scale:**  
- ~54 billion entities, ~1.6 trillion facts (mid-2024 estimate)
- June 2025 "Clarity Cleanup" removed 3 billion entities (~6.26% reduction) to improve quality for AI grounding
- Official 2020 figure: 500 billion facts about 5 billion entities

**Handling of Special Concerns:**

| Concern | Approach |
|---------|----------|
| **Conflicts** | Entity reconciliation + source ranking. Authoritative sources prioritized. No public conflict visibility |
| **Uncertainty** | Implicit — confidence scores internal, not exposed. Source authority serves as proxy |
| **Provenance** | "Evidence" in EAV-E model links to authoritative external sources (LinkedIn, Wikipedia, Crunchbase) |
| **Temporal** | Dynamic "data river" updates; historical versions not publicly accessible |

**Lesson for OBKG:**  
- **Adopt EAV-E model concept** → OBKG's Bond already has `relation` and `target_id`; adding an "Evidence" field pointing to supporting KUs would strengthen provenance (partially addressed by `Cites` and `Corroborates` RelationTypes)
- **Data River approach** → OBKG's Hebbian learning and pheromone decay already implement continuous evolution; validate this as aligned with industry best practice
- **Entity reconciliation** → OBKG needs a `Duplicates` (0x40) bond detection mechanism at the protocol level — consider adding a deduplication service in the stigmergy routing layer

---

### 2. Wikidata

**Architecture:**  
Document-oriented knowledge base built on Wikibase software. Each entity is an "Item" (Q-prefixed ID) or "Property" (P-prefixed ID). Data is organized as Statements, each containing a main claim (property-value pair), optional Qualifiers (contextual annotations like start/end time), and References (provenance citations). Exported as RDF for SPARQL querying via the Wikidata Query Service (WDQS).

**Key Innovation:**  
- **Statement Ranking system** — Preferred / Normal / Deprecated ranks for managing conflicting information
- **Reification-native model** — statements are first-class objects with qualifiers and references, not just simple triples
- **Community-driven consensus** — 100M+ items maintained by a global community
- **Multilingual by design** — every item has labels in hundreds of languages

**Scale:**  
- 122+ million items (mid-2025)
- ~1.65 billion item statements
- ~20 billion RDF triples (including reified statement structure)

**Handling of Special Concerns:**

| Concern | Approach |
|---------|----------|
| **Conflicts** | **Rank system**: Preferred/Normal/Deprecated. Deprecated statements preserved with `reason for deprecated rank` (P2241). Multiple conflicting values can coexist |
| **Uncertainty** | No formal uncertainty scores. Rank serves as implicit confidence. References provide evidence basis |
| **Provenance** | First-class References on every statement. Each reference contains property-value pairs citing sources |
| **Temporal** | Qualifiers like `start time` (P580), `end time` (P582), `point in time` (P585) on statements |

**Lesson for OBKG:**  
- **★ CRITICAL: Adopt Rank-based conflict model** → Map directly to PoMV: PoMV's usage-based value is analogous to Wikidata's community-driven ranking. OBKG should support a "Deprecated" status for KUs that doesn't delete but marks as superseded — the `Supersedes` (0x05) RelationType partially handles this, but a first-class deprecation flag in TrustSection would be valuable
- **Qualifier model for Bonds** → OBKG Bonds currently have `relation`, `target_id`, and `weight`. Consider adding qualifiers (key-value pairs) to bonds to capture context like time range, confidence, scope — similar to Wikidata's qualifier mechanism
- **Reference tracking** → OBKG's `Cites` (0x60) is already aligned, but Wikidata's model of multiple references per statement suggests OBKG should support multiple provenance bonds per claim

---

### 3. DBpedia

**Architecture:**  
Automated extraction framework that converts Wikipedia infobox templates into RDF triples. Community-curated ontology with mapping wiki that defines how templates map to classes and properties. DBpedia-Live provides real-time synchronization with Wikipedia changes.

**Key Innovation:**  
- **Community-curated ontology mapping** — human-defined mappings from heterogeneous Wikipedia templates to a unified schema
- **DBpedia Spotlight** — entity linking service that annotates free text with KG entities
- **DBpedia Databus** — dataset publication and consumption platform for Linked Open Data
- **Live synchronization** — priority-queue processing of Wikipedia edits

**Scale:**  
- ~6 million entities (English), 850 million+ core semantic triples
- Billions of triples across all language editions, 125+ languages

**Lesson for OBKG:**  
- **Linked Data URI pattern** → OBKG's ConceptId (varint u64 with 4-tier resolution) provides compact addressing. Consider a URI scheme that maps ConceptIds to HTTP URIs for interoperability: `obp://onebrain.network/ku/{concept_id}`
- **Entity linking service** → A "KU Spotlight" equivalent that annotates free text with KU references would be valuable for ingestion pipelines
- **Automatic extraction pipeline** → While OBKG is human-created, automated KU extraction from text could bootstrap the network

---

### 4. YAGO

**Architecture:**  
YAGO merges structured data from Wikipedia, WordNet (for taxonomic backbone), and GeoNames (for spatial data). YAGO 4.5 shifted to Wikidata + Schema.org as primary sources. Uses OWL 2 with logical constraints to maintain consistency. ~95% manually verified accuracy.

**Key Innovation:**  
- **SPOTL quintuple model** — extends SPO triples to `Subject-Predicate-Object-Time-Location`, making time and space first-class citizens
- **WordNet taxonomic backbone** — maps Wikipedia categories to WordNet synsets for rigorous hierarchical classification
- **High-precision extraction** — ~95% accuracy through constraint checking
- **Logical consistency** — OWL 2 constraints ensure type disjointness and cardinality

**Scale:**  
- ~49 million entities, 132 million – 2 billion triples

**Handling of Special Concerns:**

| Concern | Approach |
|---------|----------|
| **Conflicts** | Constraint checking rejects inconsistent facts. Strong typing prevents type violations |
| **Uncertainty** | No probabilistic model, but high precision (~95%) through rigorous extraction |
| **Provenance** | Each fact traces to a specific Wikipedia source |
| **Temporal** | **★ First-class**: SPOTL model with explicit time intervals on facts |

**Lesson for OBKG:**  
- **★ CRITICAL: Extend Temporal category** → OBKG's Category F (Temporal) only has `Precedes` (0x50) and `Cooccurs` (0x51). YAGO's SPOTL model suggests adding:
  - `ValidDuring` (0x52) — fact valid during time range
  - `ValidAt` (0x53) — fact valid at specific location
  - Or alternatively, add temporal qualifiers to Bond struct (start_time, end_time fields)
- **Constraint checking at protocol level** → OBKG could implement type constraints in the KU validation layer
- **Spatial anchoring** → Consider adding a Location category or extending Temporal to "Spatiotemporal"

---

### 5. ConceptNet

**Architecture:**  
Multilingual semantic network of nodes (concepts = words/phrases) connected by labeled, weighted edges (assertions). Knowledge aggregated from crowdsourcing (Open Mind Common Sense), games with a purpose, and expert-curated resources (WordNet, Wiktionary). Produces ConceptNet Numberbatch embeddings for NLP tasks.

**Key Innovation:**  
- **Defeasible common-sense knowledge** — captures "typically true" facts, not absolute truths
- **Weighted edges** — reliability/strength scores on every assertion
- **Multi-source aggregation** — "graph of graphs" merging crowdsourced, NLP-extracted, and curated data
- **Numberbatch embeddings** — KG-enhanced word embeddings for downstream NLP

**Schema Design:** ~34 relation types across categories: Taxonomic (IsA, InstanceOf), Part-Whole (PartOf, HasA, MadeOf), Spatial (AtLocation), Causal (Causes, CausesDesire, ObstructedBy), Functional (UsedFor, CapableOf, HasProperty), Temporal (HasPrerequisite, HasSubevent), Desire (MotivatedByGoal), Lexical (Synonym, Antonym, DerivedFrom).

**Scale:** ~8 million nodes, ~21 million edges, 80+ languages

**Lesson for OBKG:**  
- **Edge weight model validation** → OBKG's Bond weight system (u16 initial_weight with decay) is MORE sophisticated than ConceptNet's static weights. OBKG's Hebbian reinforcement through usage is a clear innovation
- **Common-sense relation gaps** → ConceptNet has `UsedFor`, `CapableOf`, `HasProperty`, `AtLocation` — OBKG may want to add these to a future Category I (Functional)

---

### 6. Freebase (Archived)

**Architecture:**  
Graph database built on custom Graphd engine. Schema-flexible design with Topics (entities), Types (groupings), Properties (relationships), and Domains (namespaces). Community-edited with permissionless schema extensions. Acquired by Google (2010), shut down 2016, data migrated to Wikidata.

**Key Innovation:**  
- **Compound Value Types (CVTs) / Mediator Nodes** — intermediary nodes for n-ary relations. A CVT acts as a hub connecting multiple attributes of a complex fact (e.g., a "film performance" CVT connecting actor, film, character, and year)
- **Schema-free design** — anyone could create new types and properties
- **Open, permissionless editing** — Wikipedia-like community contribution model

**Scale:** 39 million topics, 1.9 billion facts (at shutdown)

**Lesson for OBKG:**  
- **★ CRITICAL: CVT / Mediator Node pattern for n-ary relations** → Options for OBKG:
  1. **Composite KU gene** (already exists as GeneType::Composite = 10) — use as mediator node
  2. **Bond qualifiers** — add key-value qualifier fields to Bond struct
  3. Both approaches can coexist
- **Anti-vandalism** → Freebase's spam problems validate OBKG's immune system / anti-gaming mechanisms as essential

---

### 7. Cyc / OpenCyc

**Architecture:**  
Hand-curated knowledge base with CycL (formal language based on first-order predicate calculus). Knowledge organized into microtheories (contextual partitions). Inference engine with Heuristic Level modules. Supports defeasible (non-monotonic) reasoning.

**Key Innovation:**  
- **★ Microtheories (Contexts)** — knowledge partitioned into locally consistent contexts that may globally contradict each other. A "Mythology" microtheory can assert "unicorns are animals" while a "Real World" microtheory denies it. Microtheories are hierarchical — specialized contexts inherit from general ones
- **Transparent reasoning** — every conclusion has a traceable logical proof chain
- **40,000+ predicates** — extremely fine-grained relation vocabulary
- **Trillions of inferrable facts** — from 7-25M explicit axioms

**Scale:** 500,000+ concepts, 7-25 million assertions, 40,000+ predicates, 23,000+ microtheories

**Handling of Special Concerns:**

| Concern | Approach |
|---------|----------|
| **Conflicts** | **★ Microtheories**: contradictions allowed BETWEEN microtheories but forbidden WITHIN a single microtheory |
| **Uncertainty** | Defeasible reasoning — conclusions can be retracted |
| **Provenance** | Every assertion traceable to ontological engineers or inference chains |
| **Temporal** | Temporal microtheories — different time periods are separate contexts |

**Lesson for OBKG:**  
- **★ CRITICAL: Microtheory concept for context scoping** → Cyc's microtheory model can be adapted as lightweight "Knowledge Contexts" (KCs):
  - Each KU could carry an optional `context_id` grouping it into a specific domain/perspective/time
  - KUs in different contexts can contradict without violating consistency
  - Context hierarchies enable inheritance (e.g., "Physics" inherits from "Science")
  - Maps naturally to `CulturallyContextualizes` (0x76) relation
- **Predicate richness** → Cyc's 40,000 vs OBKG's 33. The tradeoff is valid for binary protocol, but consider domain-specific sub-relations via Bond qualifiers

---

### 8. WordNet

**Architecture:**  
Lexical database organized by word senses, not alphabetical order. Core unit is the **synset** (synonym set) — a group of words sharing the same meaning. Synsets connected by semantic and lexical relations.

**Key Innovation:**  
- **Synset model** — grouping words by meaning rather than form
- **Taxonomic hierarchy** — deep hypernymy trees provide semantic distance computation
- **Multiple relation types per POS** — different relations for nouns vs verbs

**Schema Relations:** Hypernymy (IS-A), Hyponymy, Meronymy (PART-OF), Holonymy (HAS-PART), Antonymy, Troponymy (Manner-of for verbs), Entailment, Synonymy.

**Scale:** 155,287 unique words, 117,659 synsets, 206,941 word-sense pairs

**Lesson for OBKG:**  
- **Synset concept for ConceptId disambiguation** → Multiple surface forms should map to the same ConceptId. Aligns with `Translates` (0x41) and `Paraphrases` (0x42)
- **Meronymy/Holonymy refinement** → OBKG has `PartOf` (0x10) but no explicit `HasPart` inverse. Consider bidirectional support
- **Troponymy for procedures** → For `GeneType::Procedure` KUs, a "manner-of" relation would be valuable

---

## Cross-System Comparison

| Feature | Google KG | Wikidata | DBpedia | YAGO | ConceptNet | Freebase | Cyc | WordNet | **OBKG** |
|---------|-----------|----------|---------|------|------------|----------|-----|---------|----------|
| **Scale (entities)** | ~51B | 122M | 6M | 49M | 8M | 39M† | 500K | 117K | — (new) |
| **Scale (facts)** | ~1.6T | 1.65B | 850M+ | 132M-2B | 21M | 1.9B† | 7-25M | 206K | — |
| **Data Model** | EAV-E | Stmt+Qual+Ref | RDF | SPOTL | Weighted edges | Topics+CVTs | CycL | Synsets | KU+Bonds |
| **Conflict Handling** | Source ranking | ★ Rank system | None | Constraint | Weighted | Last edit | ★ Microtheories | N/A | PoMV+Refutes |
| **Uncertainty** | Internal scores | Rank implicit | None | None | ★ Weights | None | Defeasible | N/A | ★ 11 EpistemicStatus |
| **Provenance** | EAV-E evidence | ★ First-class | Wikipedia | Wikipedia | Source list | Contributor | Proof chains | Expert | ★ Cites/AuthoredBy |
| **Temporal** | Data river | Qualifiers | Date props | ★ SPOTL Time | HasPrereq | CVT dates | Temporal Mt | None | Precedes/Cooccurs |
| **Spatial** | Attributes | Qualifiers | Geo-coords | ★ SPOTL Loc | AtLocation | CVT location | Spatial Mt | None | — (gap) |
| **Relation Types** | Flexible | ~9000+ | ~1600 | Schema.org | ~34 | Schema-free | 40,000+ | ~15 | ★ 33 (8 cats) |
| **Openness** | Proprietary | ★ Open | Open | Open | Open | Archived | Proprietary | Open | Open (P2P) |
| **Decentralized** | No | No | No | No | No | No | No | No | ★ Yes |

† At time of shutdown (2016)

---

## Key Innovations to Adopt for OBKG

### Priority 1: Critical Adoptions

1. **Statement Deprecation Model (from Wikidata)**  
   Add a `deprecated` flag or `KUStatus` enum to TrustSection: `Active | Deprecated(reason) | Superseded(successor_id)`. Preserves knowledge history while guiding queries to prefer current knowledge.

2. **Bond Qualifiers (from Wikidata + Freebase CVTs)**  
   Extend the Bond struct to include optional key-value qualifiers. Enables temporal scoping, spatial scoping, confidence annotation, and n-ary relation context.

3. **Knowledge Context Scoping (from Cyc Microtheories)**  
   Add optional `context_id: Option<ConceptId>` to KU metadata. Enables cultural contexts, domain scoping, temporal contexts, and perspective contexts.

### Priority 2: Strong Recommendations

4. **Spatiotemporal Bond Extension (from YAGO SPOTL)**  
   Extend Category F (Temporal) with: `ValidDuring` (0x52), `LocatedAt` (0x53), `ValidAtLocation` (0x54).

5. **Functional/Common-Sense Relations (from ConceptNet)**  
   Add Category I: `UsedFor` (0x80), `CapableOf` (0x81), `HasProperty` (0x82), `AtLocation` (0x83).

6. **Inverse Relation Tracking (from WordNet)**  
   Ensure all structural relations have explicit inverses, or implement automatic inverse bond creation.

### Priority 3: Future Considerations

7. **Entity Linking Service (from DBpedia Spotlight)** — "KU Spotlight" for text annotation
8. **Extended Relation Registry (from Freebase + Cyc)** — Reserve 0x90-0xFF for community-defined relations
9. **KG Embeddings (from ConceptNet Numberbatch)** — Graph-structure embeddings for semantic search
10. **Constraint Validation Engine (from YAGO)** — Type-consistency checks for bond creation

---

## OBKG's Unique Advantages in Relation Design

| Feature | OBKG | Nearest Competitor | Advantage |
|---------|------|-------------------|-----------|
| **Epistemic relations** | 6 types (Extends, Supplements, Refutes, Corroborates, Supersedes, Qualifies) | None | No other KG has first-class epistemic bonds |
| **Experiential relations** | 7 types (ReactionTo, TestimonyAbout, SensoryEvidenceFor, etc.) | ConceptNet (crude) | Subjective/experiential knowledge as first-class |
| **Provenance as relations** | Cites, AuthoredBy, ReviewedBy | Wikidata (references) | Provenance embedded in graph structure |
| **Bond weight decay** | Hebbian learning + exponential decay | ConceptNet (static weights) | Bonds evolve through usage — truly bio-inspired |
| **Binary efficiency** | u8 RelationType codes, varint ConceptIds | RDF URIs (verbose) | Orders of magnitude more compact |

---

## Appendix A: OneBrain's Current Type System Reference

### GeneType (11 types)

| Code | Name | Description |
|------|------|-------------|
| 0 | Fact | Factual assertion |
| 1 | Procedure | How-to / process |
| 2 | Experience | Personal experience |
| 3 | Creative | Creative work |
| 4 | MediaExperience | Media-based knowledge |
| 5 | Testimony | Witness account |
| 6 | Formal | Formal/mathematical |
| 7 | Hypothesis | Unproven conjecture |
| 8 | Narrative | Story/narrative |
| 9 | Sensory | Sensory data |
| 10 | Composite | Compound KU (v5) |

### EpistemicStatus (11 levels)

| Code | Level | Description |
|------|-------|-------------|
| 0x00 | Rumor | Unverified claim |
| 0x01 | Hearsay | Secondhand report |
| 0x02 | Testimony | Firsthand account |
| 0x03 | Observation | Direct observation |
| 0x04 | Hypothesis | Proposed explanation |
| 0x05 | Evidence | Supporting data |
| 0x06 | Corroborated | Independently confirmed |
| 0x07 | PeerReviewed | Peer-reviewed |
| 0x08 | Consensus | Community consensus |
| 0x09 | FormallyProven | Logically/mathematically proven |
| 0x0A | Axiomatic | Self-evident truth |

### EvidenceType (9 types, Cochrane/GRADE pyramid)

| Code | Type |
|------|------|
| 0x00 | None |
| 0x01 | Anecdotal |
| 0x02 | CaseStudy |
| 0x03 | Observational |
| 0x04 | Correlational |
| 0x05 | Experimental |
| 0x06 | MetaAnalysis |
| 0x07 | FormalProof |
| 0x08 | Computational |

---

> **Last updated**: 2026-07-02  
> **Status**: Survey complete — ready for implementation planning
