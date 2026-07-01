# §5. CRDT Integration for Decentralized Knowledge Convergence

Conflict-Free Replicated Data Types (CRDTs) form the consistency substrate upon which the OneBrain network achieves eventual convergence of mutable knowledge metadata without centralized coordination. This section presents the five CRDT primitives implemented in the Knowledge Unit core, provides formal proofs of their convergence properties, maps each primitive to specific KU fields, and details the integration between CRDT merge semantics and the Proof-of-Metabolic-Value (PoMV) metabolism system.

## 5.1 Motivation: Why CRDTs for Knowledge

The OneBrain network operates as a fully decentralized, peer-to-peer system in which nodes may be offline for extended periods, experience network partitions, and process knowledge updates asynchronously. This operational model directly precludes the use of centralized conflict resolution mechanisms—no single node possesses authoritative state, and no global total ordering of updates can be assumed.

Yet knowledge metadata is inherently mutable. A Knowledge Unit's trust score evolves as corroborations and challenges accumulate. Its epistemic status may be upgraded from `Hypothesis` to `Established` as evidence accrues. Usage statistics—query hits, citation counts, dwell times—grow continuously as the KU participates in the network's knowledge economy. These mutable fields must converge to a consistent state across all replicas, even when updates arrive out of order, are duplicated, or are applied during network partitions.

CRDTs provide a mathematically rigorous solution to this challenge through **Strong Eventual Consistency (SEC)**: any two nodes that have received the same set of updates—regardless of reception order—are guaranteed to reach identical states. This guarantee requires no consensus protocol, no leader election, and no global coordination. It derives purely from the algebraic properties of the data types themselves.

The SEC property is particularly valuable for knowledge systems because it preserves the following invariant: *if two nodes have ingested the same knowledge updates, they will render identical knowledge graphs*. This eliminates the class of consistency anomalies (stale reads, conflicting writes, lost updates) that plague eventually consistent systems lacking formal convergence guarantees.

## 5.2 CRDT Primitives

The `ku-core` module implements five CRDT primitives, each selected for its alignment with specific KU metadata access patterns. All implementations reside in `ku-core/src/crdt.rs` and are generic over their element types where applicable.

### 5.2.1 GCounter (Grow-only Counter)

The GCounter is a state-based CRDT that models a monotonically increasing counter in a distributed setting. Each node maintains its own local count, and the global value is the sum of all per-node counts.

**Structure:**

```rust
struct GCounter {
    counts: BTreeMap<u64, u64>,  // node_id → local_count
}
```

The use of `BTreeMap` (rather than `HashMap`) ensures deterministic iteration order, which is essential for reproducible serialization and debugging.

**Operations:**

- `increment(node_id: u64)`: Increments the count for the specified node by 1.
- `increment_by(node_id: u64, amount: u64)`: Increments the count for the specified node by the given amount.
- `value() → u64`: Returns the sum of all per-node counts.

**Merge:**

$$\text{merge}(G_1, G_2) = \{(n, \max(G_1[n], G_2[n])) \mid n \in \text{keys}(G_1) \cup \text{keys}(G_2)\}$$

where $G[n] = 0$ if $n \notin \text{keys}(G)$.

**Value:**

$$\text{value}(G) = \sum_{n \in \text{keys}(G)} G[n]$$

**Properties:**

- *Monotonicity:* The `value()` function is monotonically non-decreasing under any sequence of `increment` and `merge` operations.
- *Commutativity:* $\text{merge}(G_1, G_2) = \text{merge}(G_2, G_1)$, since $\max$ is commutative.
- *Associativity:* $\text{merge}(\text{merge}(G_1, G_2), G_3) = \text{merge}(G_1, \text{merge}(G_2, G_3))$, since $\max$ is associative.
- *Idempotency:* $\text{merge}(G, G) = G$, since $\max(x, x) = x$.

**Application:** The GCounter is used for `corroboration_count`, `query_hits`, `citation_count`, `retrieval_count`, `derivative_count`, and `dwell_time_ms`—all metrics that only increase over the lifetime of a KU.

### 5.2.2 PNCounter (Positive-Negative Counter)

The PNCounter extends the GCounter to support both increment and decrement operations by maintaining two independent GCounters: one for positive contributions and one for negative contributions.

**Structure:**

```rust
struct PNCounter {
    positive: GCounter,
    negative: GCounter,
}
```

**Operations:**

- `increment(node_id: u64)`: Increments the positive GCounter for the specified node.
- `decrement(node_id: u64)`: Increments the negative GCounter for the specified node.
- `value() → i64`: Returns the difference between the positive and negative counter values.

**Value:**

$$\text{value}(PN) = \text{value}(PN.P) - \text{value}(PN.N)$$

**Merge:**

$$\text{merge}(PN_1, PN_2) = (\text{merge}(PN_1.P, PN_2.P), \text{merge}(PN_1.N, PN_2.N))$$

The PNCounter inherits all convergence properties from the GCounter, since its merge operation is defined component-wise over two GCounters, each of which independently satisfies commutativity, associativity, and idempotency.

**Application:** The PNCounter is used for `trust_score` derivation, where corroborations increment the positive counter and challenges increment the negative counter, yielding a net trust value that can increase or decrease over time.

### 5.2.3 LWWRegister\<T\> (Last-Writer-Wins Register)

The LWWRegister is a state-based CRDT that stores a single value of type `T`, resolving concurrent writes by selecting the value with the highest timestamp.

**Structure:**

```rust
struct LWWRegister<T> {
    value: T,
    timestamp: u64,
    node_id: u64,
}
```

**Merge:**

$$\text{merge}(R_1, R_2) = \begin{cases} R_1 & \text{if } R_1.\text{ts} > R_2.\text{ts} \\ R_2 & \text{if } R_2.\text{ts} > R_1.\text{ts} \\ R_1 & \text{if } R_1.\text{ts} = R_2.\text{ts} \wedge R_1.\text{node\_id} \geq R_2.\text{node\_id} \\ R_2 & \text{otherwise} \end{cases}$$

The tie-breaking rule on `node_id` ensures determinism when two nodes write at the exact same logical timestamp. The choice of `≥` (rather than `>`) for the tie-break is arbitrary but fixed, ensuring that all nodes apply the same deterministic rule.

**Properties:**

- *Commutativity:* The merge function produces the same result regardless of argument order, since the timestamp comparison and tie-breaking rule are symmetric.
- *Idempotency:* $\text{merge}(R, R) = R$, since the timestamp and node_id comparisons yield equality.
- *Associativity:* For any three registers, the merge is associative because the total order induced by (timestamp, node_id) pairs is transitive.

**Application:** The LWWRegister is used for `epistemic_status` and `verification_level`, both of which represent single authoritative values that should reflect the most recent assessment.

**Limitation:** The LWWRegister assumes loosely synchronized clocks. In practice, the OneBrain network uses hybrid logical clocks (HLCs), which combine physical timestamps with logical counters to ensure causal consistency even under clock skew.

### 5.2.4 ORSet\<T\> (Observed-Remove Set)

The ORSet (Observed-Remove Set) is a state-based CRDT that supports both add and remove operations on a set, with **add-wins semantics**: if one node adds an element concurrently with another node removing it, the add takes precedence.

**Structure:**

```rust
struct ORSet<T> {
    elements: BTreeMap<T, BTreeSet<u64>>,  // element → set of unique tags
    tombstones: BTreeSet<u64>,              // tags of removed elements
}
```

Each add operation generates a globally unique tag (constructed as `node_id << 32 | local_counter`), which is associated with the added element. A remove operation moves all of the element's current tags to the tombstone set, effectively marking those specific add observations as removed.

**Operations:**

- `add(element: T, node_id: u64)`: Generates a new unique tag, associates it with the element. Any tags for this element that are in the tombstone set remain there (they record historical removals).
- `remove(element: T)`: Moves all current tags associated with the element to the tombstone set.
- `contains(element: &T) → bool`: Returns `true` if the element has at least one tag not in the tombstone set.
- `elements() → Vec<T>`: Returns all elements with at least one live (non-tombstoned) tag.

**Merge:**

$$\text{merge}(S_1, S_2).\text{elements} = \{(e, T_1[e] \cup T_2[e]) \mid e \in \text{dom}(T_1) \cup \text{dom}(T_2)\}$$
$$\text{merge}(S_1, S_2).\text{tombstones} = S_1.\text{tombstones} \cup S_2.\text{tombstones}$$

An element is considered present in the merged set if and only if it has at least one tag that is not in the merged tombstone set:

$$e \in \text{merge}(S_1, S_2) \iff \exists\, t \in (T_1[e] \cup T_2[e]) : t \notin (S_1.\text{tombstones} \cup S_2.\text{tombstones})$$

**Add-wins semantics:** If node A adds element $e$ (generating tag $t_{\text{new}}$) concurrently with node B removing element $e$ (tombstoning tags $\{t_1, t_2\}$), the merged state contains $e$ because $t_{\text{new}} \notin \{t_1, t_2\}$—the new tag was not observed by the removing node and therefore cannot be tombstoned.

**Application:** The ORSet is used for `domain_codes` (the set of domain classifications for a KU), `verifications` (the set of verification CIDs), and `challenges` (the set of challenge CIDs). These fields require both addition and removal of elements with well-defined concurrent semantics.

### 5.2.5 VectorClock

The VectorClock is not a CRDT in the strict sense but serves as a causal ordering mechanism that complements the CRDT primitives.

**Structure:**

```rust
struct VectorClock {
    clocks: BTreeMap<u64, u64>,  // node_id → logical_timestamp
}
```

**Operations:**

- `tick(node_id: u64)`: Increments the logical timestamp for the specified node.
- `merge(other: &VectorClock)`: Per-node maximum, identical to GCounter merge.
- `dominates(other: &VectorClock) → bool`: Returns `true` if this clock is ≥ the other clock for all nodes, and strictly > for at least one node.
- `is_concurrent(other: &VectorClock) → bool`: Returns `true` if neither clock dominates the other.
- `covers(other: &VectorClock) → bool`: Returns `true` if this clock is ≥ the other clock for all nodes.

**Application:** VectorClocks are used to establish causal ordering of KU updates, enabling nodes to determine whether two updates are causally related (one happened-before the other) or concurrent (neither is aware of the other). This information is critical for the LWWRegister's timestamp comparison and for detecting concurrent modifications that require CRDT merge resolution.

## 5.3 Formal Properties

### 5.3.1 Join Semi-Lattice Structure

All five CRDT primitives form **join semi-lattices** under their respective merge operations. A join semi-lattice $(S, \sqcup)$ is a partially ordered set in which every pair of elements has a least upper bound (join). The merge operation corresponds to the join:

$$\text{merge}(a, b) = a \sqcup b$$

The partial order is defined by the "is a predecessor of" relation:

$$a \leq b \iff \text{merge}(a, b) = b$$

**Theorem 1 (Convergence).** *Any state-based CRDT whose states form a join semi-lattice with a monotonically increasing merge function achieves Strong Eventual Consistency.*

This theorem, due to Shapiro et al. (2011), guarantees that any two replicas that have received the same set of updates (in any order, with any number of duplicates) converge to the same state.

### 5.3.2 GCounter Convergence Proof

**Claim:** The GCounter with per-node-max merge forms a join semi-lattice.

**Proof sketch:**

1. *Partial order:* Define $G_1 \leq G_2 \iff \forall n \in \text{keys}(G_1) \cup \text{keys}(G_2): G_1[n] \leq G_2[n]$. This is reflexive ($G \leq G$), antisymmetric ($G_1 \leq G_2 \wedge G_2 \leq G_1 \implies G_1 = G_2$), and transitive.

2. *Least upper bound:* For any $G_1, G_2$, define $G_{\sqcup} = \text{merge}(G_1, G_2)$. Then $G_1 \leq G_{\sqcup}$ and $G_2 \leq G_{\sqcup}$ (since $\max(a, b) \geq a$ and $\max(a, b) \geq b$). For any $G'$ such that $G_1 \leq G'$ and $G_2 \leq G'$, we have $G_{\sqcup} \leq G'$ (since $\max(G_1[n], G_2[n]) \leq G'[n]$ for all $n$). Thus $G_{\sqcup}$ is the least upper bound. $\square$

### 5.3.3 ORSet Convergence Proof

**Claim:** The ORSet with tag-union/tombstone-union merge forms a join semi-lattice.

**Proof sketch:**

1. *State space:* An ORSet state is a pair $(E, T)$ where $E: \text{Element} \to \mathcal{P}(\text{Tag})$ maps elements to tag sets, and $T \subseteq \text{Tag}$ is the tombstone set.

2. *Partial order:* $(E_1, T_1) \leq (E_2, T_2) \iff (\forall e: E_1[e] \subseteq E_2[e]) \wedge (T_1 \subseteq T_2)$.

3. *Least upper bound:* $\text{merge}((E_1, T_1), (E_2, T_2)) = (\lambda e. E_1[e] \cup E_2[e],\; T_1 \cup T_2)$. Set union is the join operation for the subset partial order, so both components form join semi-lattices, and the product of two join semi-lattices is a join semi-lattice. $\square$

The visible set (elements considered present) is derived as $\{e \mid \exists\, t \in E[e] : t \notin T\}$, which is a monotone function of the lattice state with respect to the add-wins interpretation.

## 5.4 Application to KU Fields

The following table maps each mutable KU field to its CRDT type, with rationale for the selection:

| KU Field             | CRDT Type       | Rationale                                                   |
|----------------------|-----------------|-------------------------------------------------------------|
| `corroboration_count`| GCounter        | Corroborations only accumulate; never retracted             |
| `challenge_count`    | GCounter        | Challenges only accumulate; never retracted                 |
| `trust_score`        | PNCounter       | Net trust may increase (corroboration) or decrease (challenge) |
| `epistemic_status`   | LWWRegister     | Single authoritative classification; latest assessment wins |
| `verification_level` | LWWRegister     | Verification upgrades reflect most recent evaluation        |
| `domain_codes`       | ORSet\<u32\>    | Domain classifications may be added or removed              |
| `verifications`      | ORSet\<CID\>    | Set of verification proof CIDs; may be added or invalidated |
| `challenges`         | ORSet\<CID\>    | Set of challenge CIDs; may be added or resolved             |
| `query_hits`         | GCounter        | Query frequency only increases                              |
| `citation_count`     | GCounter        | Citations only accumulate                                   |
| `derivative_count`   | GCounter        | Derivative works only accumulate                            |
| `dwell_time_ms`      | GCounter        | Cumulative reading time across all nodes                    |

This mapping ensures that every mutable field on a KU has well-defined concurrent update semantics. Fields that only grow use GCounters. Fields that can both grow and shrink use PNCounters. Fields requiring a single authoritative value use LWWRegisters. Fields representing mutable sets use ORSets.

## 5.5 Merge Semantics & Conflict Resolution

### 5.5.1 Full KU Merge Procedure

When two nodes exchange KU states during synchronization, the merge proceeds field-by-field according to each field's CRDT type:

```
function merge_ku(local: KuState, remote: KuState) → KuState:
    result.corroboration_count  = GCounter.merge(local.corroboration_count, remote.corroboration_count)
    result.challenge_count      = GCounter.merge(local.challenge_count, remote.challenge_count)
    result.trust_score          = PNCounter.merge(local.trust_score, remote.trust_score)
    result.epistemic_status     = LWWRegister.merge(local.epistemic_status, remote.epistemic_status)
    result.verification_level   = LWWRegister.merge(local.verification_level, remote.verification_level)
    result.domain_codes         = ORSet.merge(local.domain_codes, remote.domain_codes)
    result.verifications        = ORSet.merge(local.verifications, remote.verifications)
    result.challenges           = ORSet.merge(local.challenges, remote.challenges)
    result.query_hits           = GCounter.merge(local.query_hits, remote.query_hits)
    result.citation_count       = GCounter.merge(local.citation_count, remote.citation_count)
    result.derivative_count     = GCounter.merge(local.derivative_count, remote.derivative_count)
    result.dwell_time_ms        = GCounter.merge(local.dwell_time_ms, remote.dwell_time_ms)
    result.vector_clock         = VectorClock.merge(local.vector_clock, remote.vector_clock)
    return result
```

### 5.5.2 Conflict-Free Resolution Guarantees

Each CRDT type resolves concurrent updates without conflicts:

1. **GCounters:** Per-node maximum ensures that the highest observed count for each node is preserved. No information is lost, and no double-counting occurs (each node's count reflects its own local observations).

2. **PNCounters:** Both the positive and negative GCounters are merged independently via per-node maximum. The resulting net value reflects the aggregate of all positive and negative contributions observed by either node.

3. **LWWRegisters:** The timestamp comparison produces a deterministic winner. The tie-breaking rule on `node_id` ensures that even with identical timestamps, exactly one value is selected consistently across all nodes.

4. **ORSets:** The union of element-tag mappings and the union of tombstone sets produce a merged state where: (a) any element added by either node is present unless explicitly removed by a node that observed the specific add, and (b) concurrent add-remove conflicts resolve in favor of the add (add-wins semantics).

5. **VectorClocks:** Per-node maximum produces a clock that dominates both input clocks, correctly reflecting the causal union of both nodes' histories.

### 5.5.3 Merge Scenario Illustration

Consider two nodes, $A$ (node_id = 1) and $B$ (node_id = 2), that diverge after initial synchronization and independently update a KU's metadata:

```
Initial State (both nodes):
  corroboration_count = {1: 3, 2: 5}     → value = 8
  epistemic_status    = {value: Hypothesis, ts: 100, node: 1}
  domain_codes        = {biology: {tag_1}, chemistry: {tag_2}}

Node A updates (offline):
  corroboration_count: increment(1)       → {1: 4, 2: 5}     → value = 9
  epistemic_status: set(Established, 150) → {value: Established, ts: 150, node: 1}
  domain_codes: add(physics, tag_3)       → {biology: {tag_1}, chemistry: {tag_2}, physics: {tag_3}}

Node B updates (offline):
  corroboration_count: increment(2)       → {1: 3, 2: 6}     → value = 9
  corroboration_count: increment(2)       → {1: 3, 2: 7}     → value = 10
  domain_codes: remove(chemistry)         → tombstones += {tag_2}

Merged State (after sync):
  corroboration_count = {1: max(4,3), 2: max(5,7)} = {1: 4, 2: 7}  → value = 11
  epistemic_status    = {value: Established, ts: 150, node: 1}  (ts 150 > ts 100)
  domain_codes        = {biology: {tag_1}, chemistry: {tag_2}∖{tag_2}=∅, physics: {tag_3}}
                      → visible: {biology, physics}
                      (chemistry removed because tag_2 is tombstoned; physics preserved)
```

Both nodes, upon merging, arrive at identical state regardless of which node initiates the merge or the order of message delivery.

## 5.6 Integration with PoMV Metabolism

### 5.6.1 GCounters as Metabolic Signal Accumulators

The Proof-of-Metabolic-Value (PoMV) system quantifies the ongoing utility of each Knowledge Unit through metabolic signals: discrete events that indicate the KU is being actively used within the network's knowledge economy. Each metabolic signal is accumulated via a dedicated GCounter:

| Metabolic Signal    | GCounter Field       | Weight ($\alpha$) | Interpretation                        |
|---------------------|----------------------|-------------------|---------------------------------------|
| Query hit           | `query_hits`         | 0.25              | KU retrieved in response to a query   |
| Retrieval           | `retrieval_count`    | 0.20              | KU actively accessed/read by a node   |
| Citation            | `citation_count`     | 0.25              | KU referenced by another KU's bond    |
| Derivative          | `derivative_count`   | 0.15              | New KU created building on this KU    |
| Dwell/Study         | `dwell_time_ms`      | 0.15              | Cumulative time spent engaging with KU |

The GCounter's per-node-max merge semantics are essential for accurate metabolic accounting across the decentralized network. When node $A$ records 5 query hits and node $B$ independently records 3 query hits for the same KU, the merged GCounter correctly yields 8 total hits (5 from $A$ + 3 from $B$), not 5 (as a simple max would produce) or 11 (as naive addition of both states would produce if applied after duplication). The per-node accounting prevents double-counting: even if node $A$'s state is propagated to nodes $C$, $D$, and $E$ before reaching $B$, the merge at $B$ correctly attributes 5 hits to node $A$ and 3 to node $B$.

### 5.6.2 Metabolic Rate Computation

The metabolic rate of a KU at time $t$ is computed as a weighted sum of signal velocities (rates of change), subject to exponential decay:

$$\text{metabolic\_rate}(t) = \left(\alpha_1 \cdot v_q(t) + \alpha_2 \cdot v_r(t) + \alpha_3 \cdot v_c(t) + \alpha_4 \cdot v_d(t) + \alpha_5 \cdot v_{ds}(t)\right) \times e^{-\lambda \cdot \frac{\text{age}}{T_{1/2}}}$$

where:
- $v_q(t)$, $v_r(t)$, $v_c(t)$, $v_d(t)$, $v_{ds}(t)$ are the signal velocities for query, retrieval, citation, derivative, and dwell/study signals respectively, computed as the rate of GCounter value change over a sliding window.
- $\boldsymbol{\alpha} = (0.25,\, 0.20,\, 0.25,\, 0.15,\, 0.15)$ are the signal weights, reflecting the relative importance of each metabolic signal type.
- $T_{1/2} = 30$ days is the metabolic half-life.
- $\lambda = \ln(2) / T_{1/2}$ is the decay constant.
- $\text{age}$ is the elapsed time since the KU's creation.

The exponential decay factor ensures that knowledge which ceases to be actively used gradually loses metabolic vitality, analogous to biological metabolism where unused cellular components are recycled. The 30-day half-life was chosen empirically to balance between preserving recently relevant knowledge and recycling genuinely obsolete information.

### 5.6.3 CRDT-Decay Interaction

The exponential decay is applied as a **read-time transformation** on top of the CRDT state, not as a mutation to the CRDT itself. This distinction is critical: the GCounter values represent the cumulative, monotonically increasing count of metabolic events, which must never decrease (preserving the GCounter's lattice property). The decay function transforms these raw counts into a time-weighted metabolic rate at query time.

This layered architecture—immutable CRDT accumulation at the storage layer, decay-adjusted computation at the query layer—ensures that:

1. **CRDT convergence is preserved:** The underlying GCounter states always satisfy the semi-lattice properties, regardless of decay computation.
2. **Decay is eventually consistent:** Since all nodes compute decay using the same formula and the same CRDT-derived counts (which converge via SEC), the computed metabolic rates also converge.
3. **Historical accuracy is maintained:** The raw GCounter values serve as an immutable audit log of metabolic activity, enabling retrospective analysis independent of the current decay function.

### 5.6.4 Refutation Signal

One metabolic signal—refutation—operates through the PNCounter (`trust_score`) rather than a GCounter. When a KU is challenged, the trust_score's negative GCounter is incremented. A KU whose metabolic rate falls below a configurable threshold (default: 0.01) and whose trust_score is negative is eligible for garbage collection. This creates a biologically inspired lifecycle: knowledge that is neither used nor trusted is eventually recycled, while actively cited or studied knowledge persists regardless of age.

The integration of CRDTs with the PoMV metabolism system thus establishes a self-regulating knowledge ecosystem: convergent, decentralized accounting of metabolic signals feeds into a decay-adjusted vitality metric that governs knowledge retention, prioritization, and eventual recycling—all without centralized coordination.
