# 3. Language Specification

This section presents the complete KQL language specification: syntax, semantics, type system, and operational behavior for all six query types.

## 3.1 Lexical Structure

### 3.1.1 Keywords

KQL keywords are **case-insensitive**: `FIND`, `find`, and `Find` are equivalent. This design choice maximizes accessibility for users unfamiliar with case-sensitive programming languages.

**Reserved keywords** (33):

| Category | Keywords |
|----------|----------|
| Query types | `FIND`, `CREATE`, `UPDATE`, `DEPRECATE`, `WATCH`, `EXPLAIN` |
| Clauses | `WHERE`, `SCOPE`, `RETURN`, `ORDER`, `BY`, `LIMIT`, `SET`, `SIGNED`, `ON`, `NOTIFY`, `REASON`, `AS` |
| Operators | `AND`, `OR`, `NOT`, `EXISTS` |
| Scopes | `LOCAL`, `NEIGHBORS`, `CLUSTER`, `DHT`, `GLOBAL`, `AUTO` |
| Aggregations | `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` |
| Order | `ASC`, `DESC` |
| Events | `CREATE`, `UPDATE`, `DEPRECATE`, `ANY` |
| Booleans | `TRUE`, `FALSE` |

### 3.1.2 Identifiers

Identifiers match `[a-zA-Z0-9_]+`. They serve as node aliases (`k` in `(k:KU)`), field names, and property keys.

### 3.1.3 Literals

| Type | Syntax | Examples | AST Value |
|------|--------|----------|-----------|
| Integer | `-?[0-9]+` | `8000`, `-100`, `0` | `Value::Integer(i64)` |
| Float | `-?[0-9]+\.[0-9]+` | `0.95`, `-3.14` | `Value::Float(f64)` |
| String | `"[^"]*"` | `"Fact"`, `"did:key:z6Mk..."` | `Value::Text(String)` |
| Boolean | `true \| false` | `true`, `FALSE` | `Value::Bool(bool)` |

### 3.1.4 Field Paths

Dotted field paths access nested KU fields: `k.trust_score`, `k.trust.confidence`, `k.epistemic_status`. The FieldPath struct contains a `Vec<String>` of path segments.

**Accessible fields** (28 fields, mapped to KuRuntime structure):

*Core DNA Fields:*

| Field Path | KU Source | Type |
|------------|-----------|------|
| `k.gene_type` | `ku.gene_type()` → name | Text |
| `k.primary_concept` | `ku.primary_concept()` | Integer (u64 → i64) |
| `k.certainty` | `ku.certainty()` | Integer (u16 → i64) |
| `k.difficulty` | `ku.difficulty()` | Integer (u16 → i64) |
| `k.instruction_count` | `ku.instruction_count()` | Integer |
| `k.has_triple` | `ku.has_triple()` | Bool |
| `k.has_step` | `ku.has_step()` | Bool |
| `k.wire_size` | `ku.wire_size()` | Integer |

*Epigenetics Fields:*

| Field Path | KU Source | Type |
|------------|-----------|------|
| `k.trust_score` | `ku.epi.trust.trust_score` | Integer (u16 → i64) |
| `k.confidence` | `ku.epi.trust.confidence` | Integer (u16 → i64) |
| `k.verification_level` | `ku.epi.trust.verification_level` | Integer (u8 → i64) |
| `k.corroboration_count` | `ku.epi.trust.corroboration_count` | Integer (u16 → i64) |
| `k.challenge_count` | `ku.epi.trust.challenge_count` | Integer (u16 → i64) |
| `k.error_susceptibility` | `ku.epi.trust.error_susceptibility` | Integer (u16 → i64) |
| `k.bond_count` | `ku.bond_count()` | Integer |
| `k.epistemic_status` | `ku.epi.epistemic_status` | Text (11 values) |
| `k.evidence_type` | `ku.epi.evidence_type` | Integer (u8 → i64) |

*PoMV Signal Fields:*

| Field Path | KU Source | Type |
|------------|-----------|------|
| `k.metabolic_rate` | `ku.epi.trust.metabolic_rate` | Integer (u16 → i64) |
| `k.prediction_score` | `ku.epi.trust.prediction_score` | Integer (u16 → i64) |
| `k.entropy_at_creation` | `ku.epi.trust.entropy_at_creation` | Integer (u16 → i64) |
| `k.survival_score` | `ku.epi.trust.survival_score` | Integer (u16 → i64) |
| `k.synaptic_centrality` | `ku.epi.trust.synaptic_centrality` | Integer (u16 → i64) |
| `k.niche_fitness` | `ku.epi.trust.niche_fitness` | Integer (u16 → i64) |

*Expression Fields:*

| Field Path | KU Source | Type |
|------------|-----------|------|
| `k.text` | `ku.expr.text` | Text (Option) |

*System Fields:*

| Field Path | KU Source | Type |
|------------|-----------|------|
| `k.epi` | always present (v6) | Bool |
| `k.expression` | `ku.expr.is_some()` | Bool |
| `k.encoding_status` | `ku.encoding_status` | Text (Raw/Self/Part/Full) |
| `k.cid` | `ku.cid` | Text (hex) |

## 3.2 Query Types

### 3.2.1 FIND — Read Queries

```
FIND <pattern>
  [WHERE <condition>]
  [SCOPE <scope>]
  [RETURN <return_exprs>]
  [ORDER BY <order_exprs>]
  [LIMIT <n>]
```

**Semantics:** Match all KUs satisfying the pattern and conditions, optionally aggregate, order, and limit results. This is the primary read operation.

**Examples:**

```sql
-- Simple: find all KUs
FIND (k:KU)

-- Filtered: high-trust knowledge
FIND (k:KU) WHERE k.trust_score > 8000 SCOPE CLUSTER LIMIT 10

-- Aggregation: count and average trust
FIND (k:KU) RETURN COUNT(k.id), AVG(k.trust_score)

-- Complex: compound conditions with ordering
FIND (k:KU) WHERE k.trust_score > 5000 AND k.certainty >= 9000
  ORDER BY k.trust_score DESC LIMIT 20

-- Property matching
FIND (k:KU {gene_type: "Fact", certainty: 9500})

-- Existence check
FIND (k:KU) WHERE EXISTS k.trust

-- Concept queries
FIND (c:Concept)
```

### 3.2.2 CREATE — Knowledge Creation

```
CREATE <pattern>
  [SIGNED BY <signer>]
```

**Semantics:** Create a new Knowledge Unit with the specified properties. The `SIGNED BY` clause identifies the author (DID format). Default gene type is Fact; supported types include Fact, Procedure, and Narrative.

```sql
-- Create a fact
CREATE (k:KU {body: "Water boils at 100°C"}) SIGNED BY "did:key:z6Mk..."

-- Create a procedure
CREATE (k:KU {gene_type: "Procedure"}) SIGNED BY "author_id"
```

**Execution:** The executor constructs a `KnowledgeUnit` from properties, assigns default trust metadata (epistemic_status = Observation, evidence_type = Anecdotal, trust_score = 1000, confidence = 5000), and inserts into the local store.

### 3.2.3 UPDATE — Knowledge Modification

```
UPDATE <pattern>
  SET <assignments>
  [WHERE <condition>]
  SIGNED BY <signer>
```

**Semantics:** Modify fields of existing KUs matching the condition. The `SIGNED BY` clause is **mandatory** — all modifications must be attributable.

```sql
-- Update trust score for a specific concept
UPDATE (k:KU) SET k.trust_score = 9000
  WHERE k.concept_id = 42 SIGNED BY "did:ob:abc"

-- Update multiple fields
UPDATE (k:KU) SET k.trust_score = 8500, k.confidence = 9000
  WHERE k.trust_score < 5000 SIGNED BY "did:ob:reviewer"
```

**Execution:** The executor iterates all KUs, evaluates the WHERE condition, and applies assignments to matching KUs. Returns `affected_count`.

### 3.2.4 DEPRECATE — Knowledge Retirement

```
DEPRECATE <pattern>
  [WHERE <condition>]
  REASON <reason_string>
  SIGNED BY <signer>
```

**Semantics:** Mark KUs as deprecated — not deleted, but flagged as no longer authoritative. Both `REASON` and `SIGNED BY` are **mandatory**, ensuring deprecation provenance.

```sql
-- Deprecate superseded knowledge
DEPRECATE (k:KU) WHERE k.concept_id = 42
  REASON "Superseded by newer research" SIGNED BY "did:ob:abc"
```

**Execution:** The executor sets `trust_score = 0`, `verification_level = 0`, and `epistemic_status = Rumor` for matching KUs. The KU remains in storage with its deprecation metadata — enabling historical analysis and potential undeprecation.

**Design rationale:** DEPRECATE rather than DELETE because knowledge provenance matters. A deprecated KU still contributes to the knowledge graph's history and can be referenced in "superseded by" bonds.

### 3.2.5 WATCH — Standing Reactive Queries

```
WATCH FIND <pattern>
  [WHERE <condition>]
  [ON <event>]
  [NOTIFY <endpoint>]
```

**Semantics:** Register a persistent query that fires notifications when matching KUs arrive. This is fundamentally different from a point-in-time FIND — WATCH is reactive, event-driven, and persistent.

**Events:**

| Event | Fires when... |
|-------|-------------|
| `CREATE` | A new KU matching the filter is created |
| `UPDATE` | An existing KU matching the filter is modified |
| `DEPRECATE` | A matching KU is deprecated |
| `ANY` | Any of the above (default) |

```sql
-- Watch for new high-trust knowledge
WATCH FIND (k:KU) WHERE k.trust_score > 7000
  ON CREATE NOTIFY "callback://agent"

-- Watch for any changes to a concept
WATCH FIND (k:KU) WHERE k.concept_id = 42
  ON ANY NOTIFY "ws://localhost:8080/updates"

-- Watch on specific event
WATCH FIND (c:Concept) ON UPDATE
```

**Execution:** The executor stores the WATCH registration (returning a `WatchId`). On each subsequent `insert()` or `update()`, the executor evaluates all registered watches against the affected KU and fires notifications for matches.

### 3.2.6 EXPLAIN — Query Plan Introspection

```
EXPLAIN <any_query>
```

**Semantics:** Instead of executing the query, return its execution plan — scope, strategy, estimated results, and indexes used. Essential for query optimization and debugging in a distributed environment.

```sql
-- Explain a local find
EXPLAIN FIND (k:KU) WHERE k.confidence > 50 SCOPE DHT

-- Explain a watch registration
EXPLAIN WATCH FIND (k:KU) WHERE k.trust_score > 8000 ON CREATE
```

**Query plan output:**

| Field | Description | Example |
|-------|-------------|---------|
| `scope` | Execution scope | `Dht` |
| `strategy` | Execution strategy | `kademlia_lookup` |
| `estimated_results` | Estimated match count | `1,247` |
| `indexes_used` | Indexes consulted | `["trust_score_index", "concept_id_index"]` |

**Strategy mapping:**

| Scope | Strategy |
|-------|----------|
| `LOCAL` | `local_scan` |
| `NEIGHBORS` | `neighbor_broadcast` |
| `CLUSTER` | `super_peer_route` |
| `DHT` | `kademlia_lookup` |
| `GLOBAL` | `global_flood` |
| `AUTO` | `auto_escalation` |

## 3.3 Scope System

The SCOPE clause is KQL's most distinctive feature — a first-class mechanism for controlling query distribution:

| Scope | Level | TTL | Strategy | Latency | Completeness |
|-------|:-----:|:---:|----------|:-------:|:------------:|
| `LOCAL` | 0 | 0 | Execute on self only | <1ms | Lowest |
| `NEIGHBORS` | 1 | 1 | 1-hop SWIM peers (fanout=5) | ~50ms | Low |
| `CLUSTER` | 2 | 3 | Route via super-peers | ~100ms | Medium |
| `DHT` | 3 | 8 | Kademlia concept key lookup | ~200ms | High |
| `SEMANTIC` | 4 | 5 | Stigmergy pheromone trails | ~150ms | High* |
| `GLOBAL` | 5 | 12 | Random walk + TTL flooding | ~500ms+ | Highest |
| `AUTO` | — | — | Progressive escalation L0→L5 | Varies | Adaptive |

*Table 3: KQL scope levels. SEMANTIC (L4) has variable completeness — high for well-trodden topics, low for novel queries.*

**`AUTO` scope** (default): The query engine starts at LOCAL and progressively escalates until sufficient results are found or all scopes are exhausted. This is the recommended scope for most queries — users get the fastest possible response without manually tuning distribution.

## 3.4 Conditions (WHERE Clause)

KQL conditions form a boolean expression tree:

```
Condition ::= Comparison | And | Or | Not | Exists | Contains

Comparison: field op value
  field:  dotted path (e.g., k.trust_score)
  op:     = | != | > | >= | < | <=
  value:  integer | float | string | boolean

And:      condition AND condition
Or:       condition OR condition  
Not:      NOT condition
Exists:   EXISTS field
Contains: field CONTAINS value
```

**Operator precedence** (high to low): NOT > AND > OR. Conditions associate right: `A AND B AND C` parses as `A AND (B AND C)`.

**Evaluation**: The `evaluate_condition(ku, condition)` function recursively evaluates the condition tree against a KU, extracting field values via `extract_field_value(ku, field_path)` and comparing with the specified operator.

## 3.5 Aggregation Functions

KQL supports 5 aggregation functions, computed during FIND execution:

| Function | Syntax | Input | Output | Description |
|----------|--------|-------|--------|-------------|
| `COUNT` | `COUNT(k.field)` | Any | Integer | Count non-null values |
| `SUM` | `SUM(k.field)` | Numeric | Integer/Float | Sum all values |
| `AVG` | `AVG(k.field)` | Numeric | Float | Arithmetic mean |
| `MIN` | `MIN(k.field)` | Numeric | Integer/Float | Minimum value |
| `MAX` | `MAX(k.field)` | Numeric | Integer/Float | Maximum value |

Aggregations are computed over the **filtered result set** (after WHERE, before LIMIT):

$$\text{AVG}(f) = \frac{1}{N} \sum_{i=1}^{N} f(ku_i) \quad \text{where } ku_i \in \text{filtered\_results}$$

```sql
-- Multiple aggregations in one query
FIND (k:KU) WHERE k.trust_score > 1000
  RETURN COUNT(k.id) AS total,
         AVG(k.trust_score) AS avg_trust,
         MIN(k.certainty) AS min_cert,
         MAX(k.certainty) AS max_cert
```

## 3.6 Graph Pattern Matching

KQL supports graph pattern matching with typed nodes and directed edges:

### 3.6.1 Node Patterns

```
NodePattern ::= '(' [alias ':'] label ['{' properties '}'] ')'
```

**Labels:** `KU` (Knowledge Unit) | `Concept`

```sql
-- Named KU node
(k:KU)

-- KU with property filter
(k:KU {gene_type: "Fact", certainty: 9500})

-- Concept node
(c:Concept)
```

### 3.6.2 Edge Patterns

```
EdgePattern ::= '-[' [alias ':'] edge_type ']->' | '<-[' ... ']-' | '-[' ... ']-'
```

**Directions:**
- `->` (Outgoing)
- `<-` (Incoming)  
- `-` (Undirected)

```sql
-- Find KUs connected by a Causes bond
FIND (a:KU)-[r:Causes]->(b:KU) WHERE a.trust_score > 8000

-- Find concept relationships
FIND (c1:Concept)-[r:PartOf]->(c2:Concept)
```

Edge types correspond to KU Bond types — all 33 bond types are queryable, including `PartOf`, `Causes`, `Enables`, `Contradicts`, `AnalogyOf`, `Inspires`, and `EvolvesInto`.

## 3.7 Formal Grammar (EBNF)

```ebnf
query           = explain_query | watch_query | update_query
                | deprecate_query | find_query | create_query ;

find_query      = "FIND" pattern [where_clause] [scope_clause]
                  [return_clause] [order_clause] [limit_clause] ;

create_query    = "CREATE" pattern [signed_clause] ;

update_query    = "UPDATE" pattern "SET" assignments
                  [where_clause] signed_clause ;

deprecate_query = "DEPRECATE" pattern [where_clause]
                  reason_clause signed_clause ;

watch_query     = "WATCH" find_query [on_clause] [notify_clause] ;

explain_query   = "EXPLAIN" (find_query | create_query | update_query
                  | deprecate_query | watch_query) ;

pattern         = "(" [identifier ":"] node_label [property_map] ")" ;
node_label      = "KU" | "Concept" ;
property_map    = "{" property ("," property)* "}" ;
property        = identifier ":" value ;

where_clause    = "WHERE" condition ;
condition       = simple_cond [("AND" | "OR") condition] ;
simple_cond     = "EXISTS" field_path
                | field_path comp_op value ;
comp_op         = "=" | "!=" | ">" | ">=" | "<" | "<=" ;

scope_clause    = "SCOPE" scope ;
scope           = "LOCAL" | "NEIGHBORS" | "CLUSTER" | "DHT"
                | "GLOBAL" | "AUTO" ;

return_clause   = "RETURN" return_expr ("," return_expr)* ;
return_expr     = aggregate_expr | field_path | identifier ;
aggregate_expr  = agg_func "(" field_path ")" ["AS" identifier] ;
agg_func        = "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" ;

order_clause    = "ORDER" "BY" order_expr ("," order_expr)* ;
order_expr      = field_path ["ASC" | "DESC"] ;

limit_clause    = "LIMIT" integer ;
signed_clause   = "SIGNED" "BY" (quoted_string | identifier) ;
reason_clause   = "REASON" quoted_string ;
on_clause       = "ON" watch_event ;
watch_event     = "CREATE" | "UPDATE" | "DEPRECATE" | "ANY" ;
notify_clause   = "NOTIFY" quoted_string ;

assignments     = assignment ("," assignment)* ;
assignment      = field_path "=" value ;
field_path      = identifier ("." identifier)* ;
value           = quoted_string | "true" | "false" | number ;
number          = ["-"] digit+ ["." digit+] ;
quoted_string   = '"' [^"]* '"' ;
identifier      = [a-zA-Z0-9_]+ ;
```

*Figure 2: Complete KQL grammar in Extended Backus-Naur Form (EBNF).*

---

## References

[1] ISO/IEC 9075:2023, "Information technology — Database languages — SQL," 2023.
