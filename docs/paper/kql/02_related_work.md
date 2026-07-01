# 2. Related Work

This section surveys existing query languages and distributed query processing systems, identifying their limitations for decentralized knowledge graph applications and positioning KQL's contributions.

## 2.1 Relational Query Languages

**SQL** [1] has been the dominant query language for over four decades. Its declarative SELECT-FROM-WHERE structure, aggregation functions (COUNT, SUM, AVG), and ordering clauses directly inspire KQL's syntax design. However, SQL fundamentally assumes:
- A centralized relational schema visible to the query optimizer
- ACID transactions with serializability guarantees
- A single query engine with complete data access

**Distributed SQL** systems (CockroachDB [2], Google Spanner [3], TiDB) extend SQL to distributed settings but rely on consensus protocols (Paxos, Raft) for strong consistency — fundamentally incompatible with the eventual consistency model of a permissionless P2P knowledge network.

**KQL inherits** SQL's declarative syntax (FIND ≈ SELECT, WHERE, ORDER BY, LIMIT) but replaces centralized execution with scope-based distributed routing.

## 2.2 Graph Query Languages

**Cypher** [4] (Neo4j) introduced the ASCII-art graph pattern syntax `(a)-[r:TYPE]->(b)` for intuitive graph traversal. KQL adopts this pattern syntax for node matching (`(k:KU)`) and edge patterns (`-[r:BondType]->`). However, Cypher:
- Targets a single-server property graph database
- Has no concept of distributed execution scoping
- Lacks trust-aware ranking or knowledge-specific types
- Does not support standing queries or knowledge deprecation

**SPARQL** [5] provides powerful graph pattern matching over RDF triples with OPTIONAL, UNION, FILTER, and federated query (SERVICE). SPARQL's federated extensions [6] enable cross-endpoint queries, but require explicit endpoint URLs — impractical in a P2P network where endpoints are anonymous, dynamic, and potentially millions in number.

**GQL** [7] (ISO/IEC 39075:2024), the forthcoming ISO standard for graph query languages, unifies concepts from Cypher, PGQL, and G-CORE. GQL addresses property graph querying in centralized databases but does not consider decentralized execution, trust metadata, or knowledge lifecycle management.

**Gremlin** [8] (Apache TinkerPop) provides an imperative graph traversal language. While powerful for complex graph algorithms, Gremlin's imperative nature makes it unsuitable as a declarative knowledge query interface.

| Feature | SQL | SPARQL | Cypher | GQL | Gremlin | **KQL** |
|---------|-----|--------|--------|-----|---------|---------|
| Paradigm | Declarative | Declarative | Declarative | Declarative | Imperative | Declarative |
| Data model | Relational | RDF triples | Property graph | Property graph | Property graph | Knowledge Units |
| Graph patterns | No | Yes (BGP) | Yes (ASCII-art) | Yes | Yes (traversal) | Yes (Cypher-style) |
| Distributed | Federated SQL | SERVICE clause | No | No | No | SCOPE clause (6 levels) |
| Trust-aware | No | No | No | No | No | Yes (trust_score, epistemic) |
| Standing queries | No | No | No | No | No | WATCH + event filter |
| Deprecation | DELETE | DELETE | DELETE | DELETE | DROP | DEPRECATE + REASON |
| Query plan | EXPLAIN | No | EXPLAIN | EXPLAIN | explain() | EXPLAIN |
| Aggregation | Full | Full | Full | Full | fold/unfold | COUNT/SUM/AVG/MIN/MAX |

*Table 1: Comparison of query languages across key features. KQL uniquely combines distributed scoping, trust-awareness, and knowledge lifecycle management.*

## 2.3 Distributed Query Processing

**Federated query processing** [9] decomposes queries across multiple autonomous data sources. Techniques include query decomposition, subquery routing, and result integration. Key challenges:
- **Source selection**: Which sources can answer which subqueries?
- **Query optimization**: How to minimize inter-source communication?
- **Result integration**: How to merge heterogeneous results?

KQL addresses these through its 6-layer scope escalation (§4.2), Vacuum Bloom filter-based source capability assessment (§5.1), and trust×proximity result ranking (§5.2).

**CQL** (Continuous Query Language) [10] extends SQL with windows and streaming operators for continuous query processing over data streams. KQL's WATCH queries serve a similar purpose — providing event-driven notifications when matching knowledge arrives — but operate over a P2P knowledge graph rather than a centralized stream.

**Linked Data Fragments** [11] (TPF, brTPF) distribute query processing between client and server by providing minimal server-side capabilities (e.g., triple pattern lookup) and pushing complex processing to clients. This philosophy aligns with KQL's scope escalation, where local execution handles the "easy" cases and network queries handle the "hard" cases.

## 2.4 Knowledge Graph Query Systems

**Wikidata Query Service** [12] provides SPARQL access to a centralized knowledge graph with ~100 billion triples. While demonstrating the value of structured knowledge querying, its centralized architecture creates single points of failure and control.

**Google Knowledge Graph** [13] powers search through an enormous proprietary knowledge graph with internal query interfaces. The centralized, proprietary nature prevents external use and inspection.

**Amazon Neptune**, **Microsoft Azure Cosmos DB** (Gremlin API), and **ArangoDB** (AQL) provide cloud-hosted graph query services with proprietary or standard query languages. All assume a cloud-hosted centralized deployment.

**RDF4J**, **Apache Jena**, and **Stardog** provide SPARQL endpoints for RDF data. While mature and standards-compliant, they target single-site or federated (explicit endpoint) deployments.

**KQL's differentiation**: No existing knowledge graph query system provides a query language designed for permissionless P2P networks with anonymous nodes, eventual consistency, trust-annotated data, and bio-inspired knowledge structures.

## 2.5 Parser Combinators

**Parser combinators** [14] compose small parsing functions into complex parsers. The approach originated in functional programming (Haskell's Parsec [15]) and has been adopted in systems languages:

- **nom** [16] (Rust): Zero-copy, streaming-capable parser combinators. KQL uses nom for its parser, achieving ~1,310 LOC for the complete grammar.
- **pest** (Rust): PEG-based parser generator. Offers simpler grammar definition but less runtime flexibility.
- **LALRPOP** (Rust): LR parser generator. More suitable for complex grammars but generates less readable code.
- **ANTLR** [17] (Java/multi-language): LL(*) parser generator widely used in language implementation.

KQL chose **nom** for three reasons: (1) zero-copy parsing minimizes memory allocation; (2) combinator composition enables incremental grammar extension; (3) native Rust integration without build-time code generation.

## 2.6 Query Caching in Distributed Systems

**Materialized views** [18] precompute query results for fast access. In a distributed setting, maintaining view consistency is challenging.

**Query result caching** [19] stores recent query results keyed by normalized query strings. Cache invalidation strategies include TTL-based expiration, event-driven invalidation, and hybrid approaches.

KQL's query cache (§5.3) uses BLAKE3-hashed normalized query strings as keys, LRU eviction with configurable capacity, and TTL-based expiration. The `CacheInvalidate(0x68)` network message enables distributed cache coherence.

## 2.7 Summary and Positioning

Table 2 summarizes KQL's position relative to existing systems:

| System | Decentralized | Trust-Aware | Standing Queries | Lifecycle Mgmt | Knowledge Types |
|--------|:------------:|:-----------:|:----------------:|:--------------:|:---------------:|
| SQL | No | No | No | No | No |
| SPARQL | Federated | No | No | No | RDF types |
| Cypher | No | No | No | No | No |
| GQL | No | No | No | No | No |
| Gremlin | No | No | No | No | No |
| Wikidata SPARQL | No | No | No | No | Wikidata types |
| CQL (streams) | No | No | Windows | No | No |
| **KQL** | **Yes (6 scopes)** | **Yes** | **Yes (WATCH)** | **Yes (DEPRECATE)** | **Yes (10 genes)** |

*Table 2: Positioning of KQL relative to existing query languages and systems.*

---

## References

[1] ISO/IEC 9075:2023, "Information technology — Database languages — SQL," 2023.

[2] R. Taft *et al.*, "CockroachDB: The Resilient Geo-Distributed SQL Database," in *Proc. ACM SIGMOD '20*, pp. 1493–1509, 2020.

[3] J. C. Corbett *et al.*, "Spanner: Google's Globally-Distributed Database," in *Proc. OSDI '12*, pp. 251–264, 2012.

[4] N. Francis *et al.*, "Cypher: An Evolving Query Language for Property Graphs," in *Proc. ACM SIGMOD '18*, pp. 1433–1445, 2018.

[5] W3C, "SPARQL 1.1 Query Language," W3C Recommendation, Mar. 2013.

[6] O. Görlitz and S. Staab, "SPLENDID: SPARQL Endpoint Federation Exploiting VOID Descriptions," in *Proc. COLD '11*, 2011.

[7] ISO/IEC 39075:2024, "Information technology — Database languages — GQL," 2024.

[8] M. A. Rodriguez, "The Gremlin Graph Traversal Machine and Language," in *Proc. DBPL '15*, pp. 1–10, 2015.

[9] D. Kossmann, "The State of the Art in Distributed Query Processing," *ACM Computing Surveys*, vol. 32, no. 4, pp. 422–469, 2000.

[10] A. Arasu, S. Babu, and J. Widom, "The CQL Continuous Query Language: Semantic Foundations and Query Execution," *VLDB Journal*, vol. 15, no. 2, pp. 121–142, 2006.

[11] R. Verborgh *et al.*, "Triple Pattern Fragments: A Low-Cost Knowledge Graph Interface for the Web," *Journal of Web Semantics*, vol. 37, pp. 184–206, 2016.

[12] D. Vrandečić and M. Krötzsch, "Wikidata: A Free Collaborative Knowledgebase," *Communications of the ACM*, vol. 57, no. 10, pp. 78–85, 2014.

[13] A. Singhal, "Introducing the Knowledge Graph: Things, Not Strings," Google Blog, May 2012.

[14] G. Hutton, "Higher-Order Functions for Parsing," *Journal of Functional Programming*, vol. 2, no. 3, pp. 323–343, 1992.

[15] D. Leijen and E. Meijer, "Parsec: Direct Style Monadic Parser Combinators for the Real World," *Technical Report UU-CS-2001-27*, Utrecht University, 2001.

[16] G. Couprie, "nom: A Byte-Oriented, Streaming, Zero-Copy Parser Combinators Library in Rust," in *Proc. IEEE SecDev '15*, pp. 1–6, 2015.

[17] T. Parr, "ANTLR (ANother Tool for Language Recognition)," 2023. [Online]. Available: https://www.antlr.org/

[18] A. Gupta and I. S. Mumick, "Maintenance of Materialized Views: Problems, Techniques, and Applications," *IEEE Data Engineering Bulletin*, vol. 18, no. 2, pp. 3–18, 1995.

[19] Q. Luo, J. F. Naughton *et al.*, "Form-Based Proxy Caching for Database-Backed Web Sites," in *Proc. VLDB '01*, pp. 191–200, 2001.
