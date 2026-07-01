# 7. Discussion, Future Work, and Conclusion

## 7.1 Discussion

### 7.1.1 Key Findings

The design and implementation of KQL reveals several key findings about query languages for decentralized knowledge systems:

**Finding 1: The SCOPE clause is the critical missing abstraction in existing query languages.** SQL, SPARQL, Cypher, and GQL all assume the query engine has complete visibility over the dataset. In a decentralized network, this assumption is fundamentally violated. The SCOPE clause resolves this by making distribution breadth an explicit, declarative parameter — users choose between speed (LOCAL) and completeness (GLOBAL) without knowing the network topology. The AUTO scope further abstracts this choice through progressive escalation, providing "good enough" results in minimal time.

**Finding 2: Trust-aware ranking produces fundamentally different results than score-free ranking.** When multiple sources return results for the same query, traditional ranking (by relevance or recency) may prioritize low-trust or unverified knowledge. KQL's trust×scope ranking ensures that high-trust, nearby results appear first. In a knowledge network where epistemic integrity matters — scientific facts, medical knowledge, legal precedents — trust-aware ranking is not optional but essential.

**Finding 3: DEPRECATE is semantically superior to DELETE for knowledge management.** In traditional databases, DELETE removes data permanently. In a knowledge network, knowledge deprecation must preserve provenance: *who* deprecated it, *why*, and *what supersedes it*. KQL's DEPRECATE with mandatory REASON and SIGNED BY ensures that even deprecated knowledge contributes to the graph's history. This mirrors real-world knowledge management — the geocentric model was deprecated, not deleted, because understanding *why* it was superseded is itself knowledge.

**Finding 4: Standing queries (WATCH) bridge the gap between pull and push models.** Traditional query languages are pull-based — users explicitly request data. In a dynamic knowledge network, users also need push-based notifications when relevant knowledge arrives. WATCH queries provide this without requiring a separate notification infrastructure. The combination of pull (FIND) and push (WATCH) in a single language simplifies application development.

**Finding 5: Discovery engines transform query languages from retrieval to knowledge creation tools.** Traditional query languages retrieve existing data. KQL's GapDetector, BridgeFinder, and SerendipityEngine generate *new knowledge* — identifying missing knowledge, cross-domain connections, and unknown unknowns. This elevates the query language from a retrieval interface to a knowledge amplification system.

**Finding 6: Pheromone learning creates a self-improving query system.** The feedback loop between query results and routing pheromones means that the network becomes better at routing queries over time — without explicit configuration. Popular topics develop strong pheromone trails to expert nodes, while novel queries fall back to broader scopes. This bio-inspired approach avoids the cold-start problem of machine learning-based query optimizers.

### 7.1.2 Design Trade-offs

**SQL-familiar vs. novel syntax.** KQL intentionally mirrors SQL syntax (`FIND ≈ SELECT`, `WHERE`, `ORDER BY`, `LIMIT`) to minimize learning curve. However, this SQL-familiarity may create false expectations — users might expect SQL features KQL doesn't support (JOINs, GROUP BY, HAVING, subqueries). The current v1.0 grammar is deliberately simple; future versions may add these features.

**Typed vs. string-based values.** KQL's AST includes domain-specific types (`EpistemicStatus`, `EvidenceType`) alongside standard types. This provides type safety at parse time but couples the language specification to the KU data model. A more generic approach (string-only values with runtime type checking) would be more flexible but lose compile-time safety.

**Local-first vs. distributed-first.** KQL's `AUTO` scope starts with local execution and escalates. This is optimal for queries answerable locally but adds latency for queries requiring global reach (the engine must exhaust lower scopes first). An alternative "hinted" approach — where the parser estimates scope from query semantics — could skip unnecessary local checks.

**redb vs. SQLite.** The storage backend uses redb (pure Rust, ACID, embedded) rather than SQLite (C, widely deployed). redb enables cross-compilation to WebAssembly and mobile without C toolchain dependencies, but sacrifices SQLite's mature query optimizer and full-text search capabilities. For the v1.0 implementation, redb's simplicity and purity outweigh SQLite's features.

**Pheromone decay rate.** The PheromoneLearner uses the same evaporation parameters as the network layer (γ=0.95/hour). This may be suboptimal for query routing, where topic popularity varies more rapidly than network topology. Adaptive decay rates — faster for trending topics, slower for foundational knowledge — are a natural extension.

## 7.2 Limitations

**L1: No query optimizer.** The current executor uses a simple full-scan strategy for FIND queries. A cost-based query optimizer that leverages index statistics, scope cost models, and result cardinality estimation would significantly improve performance for complex queries.

**L2: No JOIN or subquery support.** KQL v1.0 supports single-pattern queries. Multi-pattern queries (JOINs), subqueries, and path expressions are not yet implemented. These are essential for complex knowledge graph traversal (e.g., "Find all KUs that contradict a high-trust fact").

**L3: Limited graph traversal.** While edge patterns are defined in the AST (`EdgePattern` with direction and type), the executor does not yet implement multi-hop graph traversal. Full graph pattern matching with variable-length paths (`-[r:TYPE*1..5]->`) is deferred to v2.0.

**L4: No type system enforcement.** The parser accepts any field path (e.g., `k.nonexistent_field`) without type checking. Invalid field paths are handled at execution time (returning empty results) rather than at parse time. A type system that validates field paths against the KU schema would improve developer experience.

**L5: Distributed query testing at scale.** The stress tests validate up to 10K concepts and 1K standing queries. Real-world deployment would involve millions of concepts and thousands of concurrent queries. Distributed simulation testing is needed.

**L6: Cache invalidation in P2P networks.** The `CacheInvalidate(0x68)` message propagates cache invalidation, but in a partitioned network, stale cache entries may persist longer than the TTL. Consistency guarantees are bounded by the network's CRDT eventual consistency model.

**L7: Serendipity Engine tuning.** The sweet-spot bell curve parameters for serendipity scoring are currently hard-coded. Personalized parameters — calibrated to individual user exploration preferences — would improve recommendation quality.

## 7.3 Future Work

### 7.3.1 Short-term (v1.1)

- **Cost-based query optimizer** using index statistics, scope cost models, and result cardinality estimation to select optimal execution strategies.
- **Full-text search** integration via tantivy (pure Rust full-text search engine) for natural language queries against KU content.
- **Parse-time type checking** against the KU schema to catch invalid field paths before execution.
- **Batch query execution** for multiple queries in a single request, with shared scope escalation to amortize network overhead.

### 7.3.2 Medium-term (v2.0)

- **Multi-pattern queries (JOINs)**: `FIND (a:KU)-[r:Contradicts]->(b:KU) WHERE a.trust > 8000` with full graph traversal.
- **Path expressions**: Variable-length paths `(a:KU)-[:PartOf*1..5]->(b:Concept)` for multi-hop traversal.
- **GROUP BY and HAVING**: Aggregation with grouping for analytical queries.
- **Subqueries**: Nested queries for complex filtering.
- **UPSERT semantics**: Atomic create-or-update operations.
- **Temporal queries**: Time-travel queries over CRDT version history.

### 7.3.3 Long-term (v3.0)

- **Natural language interface**: LLM-powered translation from natural language questions to KQL queries, enabling non-technical users to query the knowledge network.
- **Query federation with SPARQL**: Bidirectional bridge between KQL and SPARQL endpoints for interoperability with the Linked Data ecosystem.
- **Formal semantics**: Denotational semantics for KQL in terms of set operations over the KU knowledge graph, enabling formal verification of query equivalence.
- **Incremental view maintenance**: Materialized views over KQL queries with automatic CRDT-based distributed maintenance.
- **ML-enhanced scope selection**: Neural network trained on historical query logs to predict optimal scope for each query, supplementing the pheromone-based heuristic.

## 7.4 Conclusion

This paper presented **KQL (Knowledge Query Language)**, a declarative query language designed for decentralized knowledge graphs. Unlike existing query languages that assume centralized data stores, KQL provides native constructs for scoped distributed execution, trust-aware ranking, standing reactive queries, and knowledge lifecycle management.

Our six principal contributions are:

1. **A declarative query language for decentralized knowledge graphs** with 6 query types (`FIND`, `CREATE`, `UPDATE`, `DEPRECATE`, `WATCH`, `EXPLAIN`), graph pattern matching with typed nodes, trust-aware filtering, and 5 aggregation functions — providing a complete knowledge management interface in a single, coherent language.

2. **A SCOPE clause for explicit distributed execution control** across 6 escalation levels (Local → Neighbors → Cluster → DHT → Semantic → Global), enabling users to declaratively trade latency for completeness. The AUTO scope provides progressive escalation without manual tuning.

3. **Standing reactive queries (WATCH)** with event-driven notifications (`CREATE`, `UPDATE`, `DEPRECATE`, `ANY`), filter propagation across super-peers, and TTL-based lifecycle management — the first standing query mechanism integrated into a graph query language.

4. **A nom-based recursive descent parser** producing a typed AST with 30+ node types, supporting case-insensitive keywords, nested boolean conditions with AND/OR/NOT/EXISTS, 5 aggregation functions, graph edge patterns, and 8 value types including domain-specific EpistemicStatus and EvidenceType.

5. **Three novel knowledge discovery engines** — the Knowledge Gap Detector (identifying orphan concepts, low-confidence KUs, and untested hypotheses), the Swanson ABC Bridge Finder (cross-domain undiscovered public knowledge), and the Serendipity Engine (surfacing unknown unknowns via relevance×novelty scoring) — transforming the query language from a retrieval interface into a knowledge amplification system.

6. **Pheromone-based query routing reinforcement** that closes the feedback loop between query results and network routing: successful queries reinforce pheromone trails (+0.1), failed queries penalize them (−0.2), and unused trails evaporate (×0.95/hour). This bio-inspired approach creates a self-improving query system that adapts to knowledge demand patterns without explicit configuration.

The implementation spans **~3,175 LOC** (core) + **~2,860 LOC** (distributed) across 17 Rust modules, validated by **66+ tests** including 13 integration and stress tests at scales up to 10K concepts. All dependencies are pure Rust, enabling cross-compilation to mobile and WebAssembly.

KQL demonstrates that decentralized knowledge networks require purpose-built query languages — not adaptations of centralized database query languages. By integrating distribution control, trust awareness, reactive monitoring, lifecycle management, and knowledge discovery into a single declarative language, KQL provides the query interface that decentralized knowledge systems need.

---

## References

[1] ISO/IEC 9075:2023, "Information technology — Database languages — SQL," 2023.

[2] W3C, "SPARQL 1.1 Query Language," W3C Recommendation, Mar. 2013.

[3] N. Francis *et al.*, "Cypher: An Evolving Query Language for Property Graphs," in *Proc. ACM SIGMOD '18*, pp. 1433–1445, 2018.

[4] Facebook, "GraphQL: A Query Language for APIs," 2015.

[5] W3C, "XQuery 3.1: An XML Query Language," W3C Recommendation, Mar. 2017.

[6] S. Ceri, G. Gottlob, and L. Tanca, "What You Always Wanted to Know About Datalog," *IEEE TKDE*, vol. 1, no. 1, pp. 146–166, 1989.

[7] OneBrain Project, "Knowledge Unit: A Bio-Inspired Knowledge Representation for Decentralized Knowledge Networks," 2026 (companion paper).

[8] O. Görlitz and S. Staab, "SPLENDID: SPARQL Endpoint Federation Exploiting VOID Descriptions," in *Proc. COLD '11*, 2011.

[9] ISO/IEC 39075:2024, "Information technology — Database languages — GQL," 2024.

[10] M. A. Rodriguez, "The Gremlin Graph Traversal Machine and Language," in *Proc. DBPL '15*, 2015.

[11] D. Kossmann, "The State of the Art in Distributed Query Processing," *ACM Computing Surveys*, vol. 32, no. 4, pp. 422–469, 2000.

[12] A. Arasu, S. Babu, and J. Widom, "The CQL Continuous Query Language," *VLDB Journal*, vol. 15, no. 2, pp. 121–142, 2006.

[13] R. Verborgh *et al.*, "Triple Pattern Fragments," *Journal of Web Semantics*, vol. 37, pp. 184–206, 2016.

[14] D. Vrandečić and M. Krötzsch, "Wikidata: A Free Collaborative Knowledgebase," *CACM*, vol. 57, no. 10, pp. 78–85, 2014.

[15] A. Singhal, "Introducing the Knowledge Graph," Google Blog, May 2012.

[16] G. Hutton, "Higher-Order Functions for Parsing," *JFP*, vol. 2, no. 3, pp. 323–343, 1992.

[17] D. Leijen and E. Meijer, "Parsec: Direct Style Monadic Parser Combinators," UU-CS-2001-27, 2001.

[18] G. Couprie, "nom: A Byte-Oriented, Streaming, Zero-Copy Parser Combinators Library in Rust," in *Proc. IEEE SecDev '15*, 2015.

[19] T. Parr, "ANTLR," 2023. [Online]. Available: https://www.antlr.org/

[20] A. Gupta and I. S. Mumick, "Maintenance of Materialized Views," *IEEE DE Bulletin*, vol. 18, no. 2, pp. 3–18, 1995.

[21] Q. Luo *et al.*, "Form-Based Proxy Caching for Database-Backed Web Sites," in *Proc. VLDB '01*, pp. 191–200, 2001.

[22] R. Taft *et al.*, "CockroachDB: The Resilient Geo-Distributed SQL Database," in *Proc. ACM SIGMOD '20*, 2020.

[23] J. C. Corbett *et al.*, "Spanner: Google's Globally-Distributed Database," in *Proc. OSDI '12*, 2012.

[24] D. R. Swanson, "Fish Oil, Raynaud's Syndrome, and Undiscovered Public Knowledge," *Perspectives in Biology and Medicine*, vol. 30, no. 1, pp. 7–18, 1986.

[25] C. Olson, "redb: An embedded key-value store written in pure Rust," 2023.

[26] M. Shapiro *et al.*, "A Comprehensive Study of Convergent and Commutative Replicated Data Types," INRIA RR-7506, 2011.

[27] P. Maymounkov and D. Mazières, "Kademlia: A Peer-to-Peer Information System Based on the XOR Metric," in *Proc. IPTPS '02*, pp. 53–65, 2002.

[28] A. Das, I. Gupta, and A. Motivala, "SWIM: Scalable Weakly-consistent Infection-style Process Group Membership Protocol," in *Proc. IEEE/IFIP DSN '02*, 2002.

[29] M. Dorigo and T. Stützle, *Ant Colony Optimization*. MIT Press, 2004.

[30] B. H. Bloom, "Space/Time Trade-offs in Hash Coding with Allowable Errors," *CACM*, vol. 13, no. 7, pp. 422–426, 1970.

[31] P.-P. Grassé, "La reconstruction du nid et les coordinations interindividuelles chez Bellicositermes natalensis," *Insectes Sociaux*, vol. 6, pp. 41–80, 1959.

[32] OneBrain Project, "OneBrain Protocol: A Bio-Inspired 9-Layer P2P Network Stack for Decentralized Knowledge Sharing," 2026 (companion paper).

---

*End of Paper — KQL: A Declarative Query Language for Decentralized Knowledge Graphs*
