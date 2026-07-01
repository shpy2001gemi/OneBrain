# 6. Evaluation

## 6.1 Implementation Summary

### 6.1.1 Module Inventory

**KQL Core (ku-kql crate):**

| Module | File | LOC | Purpose |
|--------|------|----:|---------|
| AST | [ast.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/ast.rs) | 361 | 30+ AST node types: Query, Pattern, Condition, Value, etc. |
| Parser | [parser.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/parser.rs) | 1,310 | nom-based recursive descent parser, 6 query types |
| Executor | [executor.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/executor.rs) | 1,124 | Local executor: FIND/CREATE/UPDATE/DEPRECATE/WATCH/EXPLAIN |
| Storage | [storage.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/storage.rs) | 366 | redb-backed ACID persistent storage, BLAKE3 CID indexing |
| lib.rs | [lib.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-kql/src/lib.rs) | 14 | Module exports |
| **Subtotal** | | **3,175** | |

*Table 5: KQL core modules.*

**Distributed Query Engine (ku-net/query):**

| Module | File | LOC | Purpose |
|--------|------|----:|---------|
| ConceptIndex | [index.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/index.rs) | 178 | VacuumFilter + BLAKE3 concept keys, DHT publishing |
| QueryMessages | [messages.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/messages.rs) | 208 | Wire format: QueryForward(0x50), QueryResponse(0x51), QueryCancel(0x52) |
| QueryRouter | [router.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/router.rs) | 417 | 6-layer scope escalation engine |
| ResultMerger | [merger.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/merger.rs) | 252 | Dedup + trust×scope ranking |
| WatchEngine | [watch.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/watch.rs) | 392 | Standing queries, event filter, TTL propagation |
| GapDetector | [gaps.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/gaps.rs) | 303 | Orphan concepts, low confidence, missing evidence |
| BridgeFinder | [bridges.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/bridges.rs) | 198 | Swanson ABC cross-domain bridge detection |
| SerendipityEngine | [serendipity.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/serendipity.rs) | 272 | Unknown unknowns via relevance×novelty scoring |
| QueryCache | [cache.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/cache.rs) | 301 | LRU cache, BLAKE3-keyed normalized KQL, TTL expiration |
| PheromoneLearner | [learning.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/learning.rs) | 314 | ACO-inspired reinforcement for scope routing |
| mod.rs (query) | [mod.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/mod.rs) | 15 | Module re-exports |
| mod.rs (discovery) | [mod.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/query/discovery/mod.rs) | 9 | Discovery module re-exports |
| **Subtotal** | | **~2,859** | |

*Table 6: Distributed query engine modules.*

**Combined total:** ~3,175 LOC (core) + ~2,859 LOC (distributed) ≈ **~6,034 LOC** across 17 modules.

### 6.1.2 Dependencies

| Dependency | Purpose | Pure Rust? |
|-----------|---------|:----------:|
| `nom` 7.x | Parser combinators | ✅ |
| `ku-core` | KU types and codec | ✅ |
| `blake3` 1.x | Content hashing (CID, cache keys) | ✅ |
| `serde` + `ciborium` | CBOR serialization | ✅ |
| `redb` 2.x | ACID persistent storage (feature-gated) | ✅ |

All dependencies are pure Rust — enabling cross-compilation to mobile and WebAssembly targets without C toolchain requirements.

## 6.2 Test Coverage

### 6.2.1 Parser Tests (28 tests)

| Test | Validates |
|------|-----------|
| `test_parse_simple_find` | Basic `FIND (k:KU)` parsing |
| `test_parse_find_with_where` | WHERE condition with comparison |
| `test_parse_find_with_scope_and_limit` | SCOPE and LIMIT clauses |
| `test_parse_find_with_and_condition` | AND boolean composition |
| `test_parse_find_with_return` | RETURN clause with field paths |
| `test_parse_find_with_order` | ORDER BY DESC |
| `test_parse_find_with_properties` | Property map `{key: value}` |
| `test_parse_create` | CREATE with properties and SIGNED BY |
| `test_parse_explain` | EXPLAIN wrapping FIND |
| `test_parse_aggregate` | COUNT/SUM/AVG/MIN/MAX with AS alias |
| `test_parse_error_invalid` | Rejects non-KQL input (e.g., SQL) |
| `test_parse_concept_label` | Concept node label |
| `test_parse_exists_condition` | EXISTS field check |
| `test_parse_negative_number` | Negative integer values |
| `test_parse_float_value` | Float comparison values |
| `test_parse_all_scopes` | All 6 scope keywords |
| `test_parse_watch_simple` | Basic WATCH FIND |
| `test_parse_watch_full` | WATCH with ON CREATE NOTIFY |
| `test_parse_watch_on_update` | WATCH ON UPDATE event |
| `test_parse_watch_on_deprecate` | WATCH ON DEPRECATE event |
| `test_parse_update` | UPDATE SET WHERE SIGNED BY |
| `test_parse_deprecate` | DEPRECATE REASON SIGNED BY |
| `test_parse_or_condition` | OR boolean operator |
| `test_parse_multiple_assignments` | SET a=1, b=2 |
| `test_parse_no_alias` | Node without alias `(:KU)` |
| `test_parse_case_insensitive` | Mixed case `FiNd` |
| `test_parse_trailing_input_rejected` | Rejects trailing garbage |
| `test_parse_multiple_aggregations` | Multiple aggregations in RETURN |

### 6.2.2 Executor Tests (23 tests)

| Test | Validates |
|------|-----------|
| `test_find_all` | FIND without WHERE returns all |
| `test_find_where_gt` | Greater-than comparison filtering |
| `test_find_where_and` | AND condition evaluation |
| `test_find_with_limit` | LIMIT truncation after total count |
| `test_find_order_by_desc` | Descending sort by field |
| `test_find_exists_trust` | EXISTS condition on optional field |
| `test_find_scope_local` | Scope correctly set to Local |
| `test_empty_result` | Empty result on no matches |
| `test_aggregation_count` | COUNT function |
| `test_aggregation_avg` | AVG function (float result) |
| `test_aggregation_sum` | SUM function |
| `test_aggregation_min_max` | MIN and MAX functions |
| `test_create_execution` | CREATE inserts KU with default gene |
| `test_create_procedure` | CREATE with gene_type="Procedure" |
| `test_update_basic` | UPDATE modifies all KUs |
| `test_update_with_where` | UPDATE only matching KUs |
| `test_deprecate_basic` | DEPRECATE zeroes trust |
| `test_deprecate_with_where` | DEPRECATE with WHERE filter |
| `test_watch_register` | WATCH returns WatchId |
| `test_watch_check_match` | check_watches matches correctly |
| `test_unwatch` | unwatch removes registration |
| `test_explain_find` | EXPLAIN returns QueryPlan |
| `test_explain_auto_scope` | EXPLAIN with AUTO scope |

### 6.2.3 Storage Tests (6 tests)

| Test | Validates |
|------|-----------|
| `test_open_create_db` | Database creation |
| `test_put_and_get` | Insert + retrieve roundtrip |
| `test_has` | Existence check (present/absent) |
| `test_delete` | Delete + verify removal |
| `test_count_and_get_all` | Count + full scan |
| `test_deterministic_cid` | Same content → same CID (idempotent) |

### 6.2.4 Distributed Query Tests (66+ total across all modules)

| Module | Tests | Key Scenarios |
|--------|:-----:|---------------|
| ConceptIndex | 7 | Insert, lookup, VacuumFilter integration, DHT publish |
| QueryMessages | 5 | Wire format roundtrip, header encoding, scope encoding |
| QueryRouter | 6 | Scope escalation, fanout control, TTL decrement |
| ResultMerger | 7 | Dedup, trust ranking, scope proximity, multi-source aggregation |
| WatchEngine | 9 | Register, match, unregister, TTL expiry, event filtering |
| GapDetector | 6 | Orphan detection, low-confidence flagging, suggestion generation |
| BridgeFinder | 3 | Cross-domain bridge detection, scoring, Swanson ABC model |
| SerendipityEngine | 6 | Interest profile matching, novelty scoring, sweet-spot detection |
| QueryCache | 9 | Insert, hit, miss, LRU eviction, TTL expiry, stats, normalization |
| PheromoneLearner | 8 | Reinforce, penalize, scope preference, decay, engagement signals |

### 6.2.5 Integration and Stress Tests (13 tests)

The integration test suite ([query_integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/tests/query_integration.rs), 330 LOC) validates end-to-end pipeline behavior:

| Test | Scenario | Scale |
|------|----------|-------|
| `test_pipeline_find_local` | KQL → parse → execute → result | 10 KUs |
| `test_pipeline_create_and_find` | CREATE then FIND | 5 KUs |
| `test_pipeline_update_verify` | UPDATE then verify changes | 20 KUs |
| `test_pipeline_deprecate_verify` | DEPRECATE then verify zeroed | 10 KUs |
| `test_pipeline_watch_trigger` | WATCH → insert → check_watches | 50 KUs |
| `test_pipeline_explain_accuracy` | EXPLAIN matches actual execution | 100 KUs |
| `test_pipeline_aggregation_accuracy` | Aggregation matches manual calc | 1K KUs |
| `stress_10k_concepts` | ConceptIndex with 10,000 concepts | 10K |
| `stress_1000_watches` | WatchEngine with 1,000 standing queries | 1K watches |
| `stress_500_kus_insert_query` | Insert 500 KUs, run 100 queries | 500 KUs |
| `stress_cache_eviction` | Fill cache beyond capacity, verify LRU | 2K queries |
| `stress_bridge_finder` | BridgeFinder across 20 domains | 500 KUs |
| `stress_concurrent_queries` | 50 concurrent queries | 50 parallel |

## 6.3 Comparison with Existing Query Languages

| Feature | SQL | SPARQL | Cypher | **KQL** |
|---------|-----|--------|--------|---------|
| **LOC** | N/A (spec) | N/A (spec) | N/A (spec) | ~3,175 (core) + ~2,859 (distributed) |
| **Data model** | Relational | RDF triples | Property graph | Knowledge Units |
| **Parser** | Yacc/Bison | Custom | ANTLR | nom (Rust) |
| **Query types** | SELECT/INSERT/UPDATE/DELETE | SELECT/CONSTRUCT/ASK/DESCRIBE | MATCH/CREATE/MERGE/DELETE | FIND/CREATE/UPDATE/DEPRECATE/WATCH/EXPLAIN |
| **Distribution** | Federated SQL | SERVICE | None | SCOPE (6 levels) |
| **Standing queries** | Triggers (limited) | None | None | WATCH (first-class) |
| **Trust-aware** | No | No | No | Yes (trust_score, epistemic) |
| **Deprecation** | DELETE (permanent) | DELETE | DELETE | DEPRECATE (reversible, provenance) |
| **Discovery** | None | None | None | 3 engines (Gap/Bridge/Serendipity) |
| **Learning** | Query optimizer hints | None | Query profiler | Pheromone reinforcement |
| **Cache** | Buffer pool | None built-in | Page cache | BLAKE3-keyed LRU |

*Table 7: Comprehensive comparison of KQL with existing query languages.*

## 6.4 Performance Characteristics

### 6.4.1 Parser Performance

The nom-based parser operates in **linear time** O(n) where n is the query string length. Typical query parse times:

| Query Complexity | Length | Expected Parse Time |
|-----------------|:------:|:-------------------:|
| Simple FIND | ~20 chars | <10 μs |
| FIND + WHERE + SCOPE + LIMIT | ~80 chars | <30 μs |
| Complex AND/OR conditions | ~200 chars | <80 μs |
| WATCH + WHERE + ON + NOTIFY | ~150 chars | <50 μs |

Parser performance is bounded by the zero-copy design of nom — no string allocations during tokenization.

### 6.4.2 Executor Performance

Local executor operations scale with the KU store size:

| Operation | Complexity | Notes |
|-----------|:----------:|-------|
| FIND (no index) | O(N) | Full scan with condition eval |
| FIND (with LIMIT) | O(N) | Full scan, truncate result |
| CREATE | O(1) | Append to store |
| UPDATE | O(N) | Full scan + mutation |
| DEPRECATE | O(N) | Full scan + mutation |
| WATCH register | O(1) | Append to watch list |
| check_watches | O(W × C) | W=watches, C=condition complexity |

For large stores (>100K KUs), index-based lookup (via redb `index_trust` and `index_concept` tables) reduces FIND to O(log N) for indexed fields.

### 6.4.3 Distributed Query Latency

End-to-end latency for distributed queries:

| Scope | Local Parse | Network RTT | Remote Exec | Merge | Total |
|-------|:----------:|:-----------:|:-----------:|:-----:|:-----:|
| LOCAL | <0.1ms | 0 | <1ms | 0 | ~1ms |
| NEIGHBORS | <0.1ms | ~50ms | <1ms | <1ms | ~52ms |
| CLUSTER | <0.1ms | ~100ms | <1ms | <2ms | ~103ms |
| DHT | <0.1ms | ~200ms | <5ms | <5ms | ~210ms |
| SEMANTIC | <0.1ms | ~150ms | <5ms | <5ms | ~160ms |
| GLOBAL | <0.1ms | ~500ms | <10ms | <10ms | ~520ms |

*Table 8: Expected distributed query latency by scope level.*

---
