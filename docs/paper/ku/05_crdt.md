# 5. Decentralized State Convergence

## 5.1 Motivation for CRDTs in Knowledge Networks

Decentralized knowledge systems operate under fundamentally different assumptions than centralized databases. In a peer-to-peer network where Knowledge Units propagate across heterogeneous nodes, several operational realities impose stringent requirements on the consistency model.

**Network partitions are the norm, not the exception.** Nodes join and leave the network unpredictably. A research institution may operate behind restrictive firewalls; a mobile device may lose connectivity for hours; an edge node in a resource-constrained environment may synchronize only during scheduled windows. Any consistency mechanism that requires synchronous coordination — leader election, two-phase commit, Paxos-family consensus — becomes impractical when partitions are frequent and prolonged [Gilbert and Lynch, 2002].

**Concurrent mutations are unavoidable.** Multiple nodes may independently modify the metadata of the same Knowledge Unit. A node in Tokyo increments the access count while a node in Berlin simultaneously updates a trust score. These concurrent modifications must eventually converge to a consistent state without requiring real-time coordination.

**Mutable metadata demands domain-specific conflict resolution.** While the CoreDna of a Knowledge Unit — its content hash, semantic encoding, and structural identity — is immutable and content-addressed, the Epigenetics layer is inherently mutable. Trust scores rise and fall as a KU proves or disproves its utility. Epistemic status transitions from *hypothesis* to *established* as evidence accumulates. Usage counts grow monotonically. Domain classifications expand as a KU finds relevance in new fields. Each metadata category exhibits distinct conflict semantics that a uniform last-writer-wins strategy cannot adequately capture.

**Strong Eventual Consistency (SEC) is the appropriate guarantee.** SEC, as formalized by Shapiro et al. [2011], provides two properties: (1) eventual delivery — if one correct node delivers a message, all correct nodes eventually deliver it; and (2) convergence — correct nodes that have delivered the same set of updates have equivalent state. Conflict-free Replicated Data Types (CRDTs) achieve SEC by construction, encoding conflict resolution directly into the data structure's merge operation [Shapiro et al., 2011]. This eliminates the need for consensus protocols while guaranteeing that all replicas converge to the same state regardless of message ordering or delivery timing.

The KU system therefore adopts CRDTs as the foundational mechanism for replicating and merging mutable metadata across the decentralized network. The CAP theorem [Gilbert and Lynch, 2002] dictates that a distributed system cannot simultaneously guarantee consistency, availability, and partition tolerance. The KU system chooses **AP** (availability and partition tolerance) with eventual consistency, aligning with the design philosophy of Dynamo [DeCandia et al., 2007] while providing stronger convergence guarantees through CRDT semantics.

---

## 5.2 Formal Foundations

### 5.2.1 Join Semi-Lattice

A *join semi-lattice* $(S, \sqcup)$ is a set $S$ equipped with a binary operation $\sqcup : S \times S \rightarrow S$ (called *join* or *merge*) that satisfies three algebraic properties:

1. **Commutativity.** $\forall a, b \in S: a \sqcup b = b \sqcup a$
2. **Associativity.** $\forall a, b, c \in S: (a \sqcup b) \sqcup c = a \sqcup (b \sqcup c)$
3. **Idempotency.** $\forall a \in S: a \sqcup a = a$

These properties jointly induce a partial order $\leq$ on $S$ defined by $a \leq b \iff a \sqcup b = b$, under which $\sqcup$ computes the *least upper bound* of its arguments.

### 5.2.2 Convergence Theorem

**Theorem (Shapiro et al., 2011).** *A state-based object whose states form a join semi-lattice under the merge operation, and whose local update operations are monotonically non-decreasing with respect to the induced partial order, achieves Strong Eventual Consistency: any two replicas that have received the same set of updates — in any order, with any number of duplicates — converge to identical states.*

This theorem provides the formal foundation for all CRDT-based replication in the KU system. Each mutable field in the Epigenetics layer is backed by a CRDT whose merge operation forms a join semi-lattice, thereby guaranteeing SEC by construction.

### 5.2.3 CAP Positioning

The KU system occupies the AP region of the CAP space: it prioritizes availability (every non-failing node can process reads and writes) and partition tolerance (the system continues to operate despite arbitrary network partitions) at the cost of linearizable consistency. CRDTs provide a principled recovery path: once a partition heals and all updates propagate, replicas converge to an identical state without requiring rollback, retry, or conflict resolution protocols.

---

## 5.3 Five CRDT Types

The KU system employs five CRDT primitives, each selected for its algebraic properties and suitability for specific metadata categories.

### 5.3.1 GCounter (Grow-Only Counter)

**Definition.** A GCounter is a state-based CRDT that models a monotonically non-decreasing counter in a distributed system with $n$ identified nodes. It is a map from node identifiers to non-negative integers:

$$G : \text{NodeId} \rightarrow \mathbb{N}_0$$

**Rust Implementation.**

```rust
pub struct GCounter {
    counts: BTreeMap<u64, u64>,
}

impl GCounter {
    pub fn increment(&mut self, node_id: u64) {
        *self.counts.entry(node_id).or_insert(0) += 1;
    }

    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    pub fn merge(&mut self, other: &GCounter) {
        for (&node, &count) in &other.counts {
            let entry = self.counts.entry(node).or_insert(0);
            *entry = (*entry).max(count);
        }
    }
}
```

**KU Use Cases.** The `access_count` field in Epigenetics is backed by a GCounter. Access counts are monotonically non-decreasing — a KU that has been accessed cannot become un-accessed. Each node independently tracks its local access count; the global count is the sum across all nodes.

**Merge Semantics.** Element-wise maximum: $\text{merge}(G_1, G_2)[n] = \max(G_1[n], G_2[n])$ for all $n$.

---

### 5.3.2 PNCounter (Positive-Negative Counter)

**Definition.** A PNCounter extends the GCounter to support both increments and decrements by maintaining two GCounters — one for positive contributions ($P$) and one for negative contributions ($N$):

$$C = (P, N), \quad \text{value}(C) = \text{value}(P) - \text{value}(N)$$

**Rust Implementation.**

```rust
pub struct PNCounter {
    positive: GCounter,
    negative: GCounter,
}

impl PNCounter {
    pub fn increment(&mut self, node_id: u64) {
        self.positive.increment(node_id);
    }

    pub fn decrement(&mut self, node_id: u64) {
        self.negative.increment(node_id);
    }

    pub fn value(&self) -> i64 {
        self.positive.value() as i64 - self.negative.value() as i64
    }

    pub fn merge(&mut self, other: &PNCounter) {
        self.positive.merge(&other.positive);
        self.negative.merge(&other.negative);
    }
}
```

**KU Use Cases.** The six PoMV trust signals — `metabolic_rate`, `prediction_score`, `entropy_at_creation`, `survival_score`, `synaptic_centrality`, and `niche_fitness` — are each backed by a PNCounter. Trust assessments are inherently revisable: a KU may demonstrate high prediction accuracy in one context and subsequently fail in another.

**Merge Semantics.** Component-wise GCounter merge: $\text{merge}(C_1, C_2) = (\text{merge}(P_1, P_2), \text{merge}(N_1, N_2))$.

---

### 5.3.3 LWWRegister (Last-Writer-Wins Register)

**Definition.** An LWWRegister stores a single value with an associated timestamp. Concurrent writes are resolved by selecting the write with the highest timestamp:

$$R = (v, t), \quad \text{merge}(R_1, R_2) = \begin{cases} R_1 & \text{if } t_1 > t_2 \\ R_2 & \text{if } t_2 > t_1 \\ \max_{\prec}(R_1, R_2) & \text{if } t_1 = t_2 \end{cases}$$

where $\prec$ is a deterministic total order on values (e.g., lexicographic comparison) serving as a tiebreaker.

**Rust Implementation.**

```rust
pub struct LWWRegister<T: Ord + Clone> {
    value: T,
    timestamp: u64,
}

impl<T: Ord + Clone> LWWRegister<T> {
    pub fn write(&mut self, value: T, timestamp: u64) {
        if timestamp > self.timestamp
            || (timestamp == self.timestamp && value > self.value)
        {
            self.value = value;
            self.timestamp = timestamp;
        }
    }

    pub fn merge(&mut self, other: &LWWRegister<T>) {
        self.write(other.value.clone(), other.timestamp);
    }
}
```

**KU Use Cases.** The `epistemic_status` field — taking values from the 11-level enumeration (e.g., *rumor*, *hypothesis*, *emerging*, *established*, *axiom*, *deprecated*) — is backed by an LWWRegister. Epistemic status is a singular categorical state; the most recent authoritative assessment should prevail.

**Merge Semantics.** Maximum under the total order $(t, v)$: the entry with the higher timestamp wins; ties are broken by value comparison.

---

### 5.3.4 ORSet (Observed-Remove Set)

**Definition.** An ORSet implements a set with both add and remove operations, using unique tags to disambiguate concurrent modifications. The ORSet provides *add-wins* semantics: if an add and a remove for the same element are concurrent, the add takes effect [Bieniusa et al., 2012].

$$S : E \rightarrow \mathcal{P}(\text{UniqueTag})$$

An element $e$ is present if and only if $S[e] \neq \emptyset$.

**Rust Implementation.**

```rust
pub struct ORSet<T: Eq + Hash + Clone> {
    elements: HashMap<T, HashSet<(u64, u64)>>, // (node_id, seq_num) tags
    seq: u64,
}

impl<T: Eq + Hash + Clone> ORSet<T> {
    pub fn add(&mut self, element: T, node_id: u64) {
        self.seq += 1;
        self.elements
            .entry(element)
            .or_default()
            .insert((node_id, self.seq));
    }

    pub fn remove(&mut self, element: &T) {
        self.elements.remove(element);
    }

    pub fn merge(&mut self, other: &ORSet<T>) {
        for (elem, tags) in &other.elements {
            self.elements
                .entry(elem.clone())
                .or_default()
                .extend(tags);
        }
    }
}
```

**KU Use Cases.** The `domain_codes` and `tags` fields in Epigenetics are backed by ORSets. A KU may be independently tagged by different nodes: one adds "neuroscience" while another removes "psychology." The add-wins semantics ensure that concurrent additions are never silently lost.

**Merge Semantics.** Per-element tag union: $\text{merge}(S_1, S_2)[e] = S_1[e] \cup S_2[e]$ for all $e$.

---

### 5.3.5 VectorClock

**Definition.** A VectorClock captures causal ordering of events in a distributed system [Lamport, 1978; Mattern, 1989]. It is a map from node identifiers to logical timestamps:

$$V : \text{NodeId} \rightarrow \mathbb{N}_0$$

**Rust Implementation.**

```rust
pub struct VectorClock {
    clocks: BTreeMap<u64, u64>,
}

impl VectorClock {
    pub fn tick(&mut self, node_id: u64) {
        *self.clocks.entry(node_id).or_insert(0) += 1;
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (&node, &time) in &other.clocks {
            let entry = self.clocks.entry(node).or_insert(0);
            *entry = (*entry).max(time);
        }
    }

    pub fn partial_cmp(&self, other: &VectorClock) -> Option<std::cmp::Ordering> {
        let all_keys: BTreeSet<_> =
            self.clocks.keys().chain(other.clocks.keys()).collect();
        let mut less = false;
        let mut greater = false;
        for &k in &all_keys {
            let a = self.clocks.get(k).copied().unwrap_or(0);
            let b = other.clocks.get(k).copied().unwrap_or(0);
            if a < b { less = true; }
            if a > b { greater = true; }
        }
        match (less, greater) {
            (true, true) => None,      // concurrent
            (true, false) => Some(std::cmp::Ordering::Less),
            (false, true) => Some(std::cmp::Ordering::Greater),
            (false, false) => Some(std::cmp::Ordering::Equal),
        }
    }
}
```

**KU Use Cases.** VectorClocks are attached to Epigenetics updates to establish causal ordering. When a node updates a KU's metadata, it ticks its local entry and attaches the resulting clock. Receiving nodes can distinguish causally related updates from concurrent ones ($V_1 \| V_2 \iff \neg(V_1 \leq V_2) \land \neg(V_2 \leq V_1)$), enabling informed merge decisions.

**Merge Semantics.** Element-wise maximum — identical to GCounter merge: $\text{merge}(V_1, V_2)[n] = \max(V_1[n], V_2[n])$.

---

## 5.4 KU-Specific Merge Semantics

### 5.4.1 Field-Level Independent Merge

The merge of two KU replicas operates at the *field level*: each Epigenetics field is merged independently using its corresponding CRDT merge function. If $\mathcal{E} = (f_1, f_2, \ldots, f_k)$ is a product type where each $f_i$ is a CRDT with merge function $\sqcup_i$, then:

$$\text{merge}(\mathcal{E}_a, \mathcal{E}_b) = (f_{1,a} \sqcup_1 f_{1,b}, \; f_{2,a} \sqcup_2 f_{2,b}, \; \ldots, \; f_{k,a} \sqcup_k f_{k,b})$$

This field-level composition guarantees isolation (a conflict in one field does not affect resolution of others) and minimality (only changed fields need to be transmitted for delta-based replication [Almeida et al., 2015]).

### 5.4.2 Epigenetics Field Mapping

The complete mapping from Epigenetics fields to CRDT types is driven by the semantic requirements of each field:

| **Field** | **CRDT Type** | **Rationale** |
|---|---|---|
| `access_count` | GCounter | Monotonically non-decreasing; sum of per-node counts |
| `metabolic_rate` | PNCounter | Bidirectional: rises with active use, falls during dormancy |
| `prediction_score` | PNCounter | Fluctuates as predictions are validated or refuted |
| `entropy_at_creation` | PNCounter | May be recalculated if creation context is re-evaluated |
| `survival_score` | PNCounter | Evolves as a KU withstands or fails challenges |
| `synaptic_centrality` | PNCounter | Changes as knowledge graph connections form and dissolve |
| `niche_fitness` | PNCounter | Fluctuates with domain landscape evolution |
| `epistemic_status` | LWWRegister | Singular categorical state; latest assessment prevails |
| `domain_codes` | ORSet | Set with add-wins semantics; prevents accidental classification loss |
| `tags` | ORSet | User-assigned tags follow add-remove set pattern |
| Event ordering | VectorClock | Causal ordering for distinguishing concurrent updates |

### 5.4.3 Trust Score Convergence (PoMV Signals)

The six Proof of Metabolic Value (PoMV) signals are each represented as `u16` values in $[0, 10000]$, encoding a fixed-point decimal with four significant digits. All six are backed by PNCounters, reflecting the fundamental characteristic that knowledge assessments are revisable. After merge, the signal value is clamped:

$$\text{signal\_value} = \text{clamp}\left(\sum_{n} P[n] - \sum_{n} N[n], \; 0, \; 10000\right)$$

This ensures the result remains within the valid `u16` range while preserving the causal contributions of each evaluating node.

### 5.4.4 Bond Semantics

The 33 bond types governing inter-KU relationships (e.g., `supports`, `contradicts`, `extends`, `cites`) are managed through ORSet semantics at the collection level. A bond between two KUs is identified by the tuple (source CID, target CID, bond type); concurrent creation of the same bond at different nodes is idempotent, while removal requires observation of the existing bond. Bond strength values, where applicable, use LWWRegister semantics.

### 5.4.5 Epistemic Status Transitions

The 11 epistemic status levels form a lattice of evidential certainty. While the LWWRegister ensures convergence on a single status value, the system imposes application-level invariants on transitions. For example, a KU's status cannot regress from *axiom* to *rumor* without an explicit deprecation event. These invariants are enforced post-merge as defense-in-depth constraints.

### 5.4.6 Immutability of CoreDna

The CoreDna of a Knowledge Unit — comprising the content hash (CID), semantic encoding, and structural metadata — is immutable. Two replicas of the same KU (identified by the same CID) are guaranteed to have identical CoreDna. If the content changes, a new KU with a new CID is created. This clean separation confines all CRDT machinery to the Epigenetics layer, reducing the surface area for consistency-related defects.

---

## 5.5 Convergence Guarantees

### 5.5.1 Semi-Lattice Proofs

Each of the five CRDT primitives satisfies the three join semi-lattice properties:

**GCounter.** The merge function is element-wise $\max$. Since $\max$ over $\mathbb{N}_0$ is commutative, associative, and idempotent, GCounter merge inherits all three properties. $\square$

**PNCounter.** The merge is a pair of GCounter merges applied independently to the positive and negative components. Component-wise application of commutative, associative, and idempotent functions preserves all three properties. $\square$

**LWWRegister.** The merge selects the maximum under the total order $(t, v)$. The $\max$ operation over any total order is commutative, associative, and idempotent. $\square$

**ORSet.** The merge is per-element set union. Set union is commutative, associative, and idempotent. $\square$

**VectorClock.** The merge is element-wise $\max$ — structurally identical to GCounter merge. The proof follows by the same argument. $\square$

### 5.5.2 Composite Convergence

Since each Epigenetics field is backed by a CRDT whose merge forms a join semi-lattice, and the composite merge applies each field merge independently (Section 5.4.1), the composite Epigenetics merge is itself a join semi-lattice over the product space. By the convergence theorem (Section 5.2.2), any two replicas that have received the same set of updates converge to identical states.

### 5.5.3 Merge Validity Invariants

A critical invariant is that the merge result is always a valid KU state:

- **GCounter values** are non-negative by construction (sums of non-negative entries).
- **PNCounter values** may be negative arithmetically, but the application clamps them to $[0, 10000]$ after merge.
- **LWWRegister values** are drawn from the `EpistemicStatus` enumeration; only valid values can be written.
- **ORSet elements** are validated at insertion time; union of valid sets produces a valid set.
- **VectorClock entries** are non-negative integers whose element-wise max preserves non-negativity.

Post-merge validation provides defense-in-depth beyond the structural correctness of the CRDT merge.

### 5.5.4 Convergence Under Concurrent Evaluation

Consider three nodes $A$, $B$, $C$ concurrently evaluating a KU's `prediction_score`:

- Node $A$ increments the positive counter by 50 (prediction confirmed).
- Node $B$ increments the negative counter by 30 (prediction partially refuted).
- Node $C$ increments the positive counter by 20 (confirmed in a different context).

Regardless of propagation order, the final state converges to $P = \{A: 50, C: 20, \ldots\}$, $N = \{B: 30, \ldots\}$, yielding $\text{prediction\_score} = (50 + 20 + \ldots) - (30 + \ldots)$. The PNCounter's algebraic properties guarantee this convergence without coordination.

---

## References

- Shapiro, M., Preguiça, N., Baquero, C., and Zawirski, M. (2011). A Comprehensive Study of Convergent and Commutative Replicated Data Types. *INRIA Research Report RR-7506*.

- Shapiro, M., Preguiça, N., Baquero, C., and Zawirski, M. (2011). Conflict-free Replicated Data Types. In *Proceedings of the 13th International Symposium on Stabilization, Safety, and Security of Distributed Systems (SSS 2011)*, Lecture Notes in Computer Science, 6976, 386–400.

- Lamport, L. (1978). Time, Clocks, and the Ordering of Events in a Distributed System. *Communications of the ACM*, 21(7), 558–565.

- Mattern, F. (1989). Virtual Time and Global States of Distributed Systems. In *Proceedings of the International Workshop on Parallel and Distributed Algorithms*, 215–226.

- Bieniusa, A., Zawirski, M., Preguiça, N., Shapiro, M., Baquero, C., Balegas, V., and Duarte, S. (2012). An Optimized Conflict-free Replicated Set. *arXiv preprint arXiv:1210.3368*.

- Gilbert, S. and Lynch, N. (2002). Brewer's Conjecture and the Feasibility of Consistent, Available, Partition-Tolerant Web Services. *ACM SIGACT News*, 33(2), 51–59.

- DeCandia, G., Hastorun, D., Jampani, M., Kakulapati, G., Lakshman, A., Pilchin, A., Sivasubramanian, S., Vosshall, P., and Vogels, W. (2007). Dynamo: Amazon's Highly Available Key-value Store. In *Proceedings of the 21st ACM Symposium on Operating Systems Principles (SOSP 2007)*, 205–220.

- Almeida, P. S., Shoker, A., and Baquero, C. (2015). Efficient State-based CRDTs by Delta-Mutation. In *Proceedings of the International Conference on Networked Systems (NETYS 2015)*, Lecture Notes in Computer Science, 9466, 62–76.

- Kleppmann, M. and Beresford, A. R. (2017). A Conflict-Free Replicated JSON Datatype. *IEEE Transactions on Parallel and Distributed Systems*, 28(10), 2733–2746.
