# KQL: A Declarative Query Language for Decentralized Knowledge Graphs

**Authors:** OneBrain Project Contributors  
**Contact:** shpy2001@gmail.com  
**Date:** June 2026  
**Version:** 1.0

---

## Abstract

Querying distributed knowledge requires fundamentally different abstractions than querying relational databases or document stores. Existing query languages — SQL [1], SPARQL [2], Cypher [3], and GraphQL [4] — were designed for centralized data stores with complete visibility over the dataset. In a decentralized peer-to-peer knowledge network, no single node holds the complete graph; queries must be routed across heterogeneous nodes with varying capabilities, trust levels, and connectivity. No existing query language provides native constructs for **scoped distributed execution**, **trust-aware result ranking**, **standing reactive queries**, **knowledge lifecycle management** (creation, deprecation), or **query plan introspection** within a single, coherent language.

This paper presents the **Knowledge Query Language (KQL)**, a declarative query language purpose-built for the OneBrain decentralized knowledge network. KQL provides six query types — `FIND`, `CREATE`, `UPDATE`, `DEPRECATE`, `WATCH`, and `EXPLAIN` — operating over structured Knowledge Units (KUs) with bio-inspired metadata. The language introduces: (1) a `SCOPE` clause for explicit distributed execution control across 6 escalation levels (Local → Neighbors → Cluster → DHT → Semantic → Global); (2) graph pattern matching with typed nodes (`KU`, `Concept`) and directed edge patterns; (3) trust-aware filtering and aggregation (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`) over epistemic metadata; (4) reactive `WATCH` queries with event-driven notifications; (5) `DEPRECATE` for knowledge lifecycle management with provenance; and (6) `EXPLAIN` for query plan transparency.

The implementation comprises a **nom-based recursive descent parser** (~1,310 LOC Rust) producing a typed Abstract Syntax Tree (AST) with 30+ node types, a **local executor** (~1,124 LOC) supporting all 6 query types with aggregation and ordering including `encoding_status` filtering for Encoding Consensus lifecycle queries, **ACID-compliant persistent storage** via redb with BLAKE3-keyed content indexing, and a **distributed query engine** (~2,860 LOC) featuring 6-layer scope escalation, trust×proximity result ranking, LRU query caching, pheromone-based routing reinforcement, and three novel discovery engines (Knowledge Gap Detector, Swanson ABC Bridge Finder, Serendipity Engine). The complete implementation spans **~3,175 LOC** across 5 core modules and **~2,860 LOC** across 12 distributed query modules, validated by **66+ tests** including integration and stress tests.

**Keywords:** Query language, knowledge graphs, decentralized systems, peer-to-peer, distributed query processing, graph pattern matching, CRDT, standing queries, reactive queries, trust-aware ranking, parser combinators, bio-inspired computing, knowledge discovery, Bloom filters
