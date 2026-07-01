# 4. Epistemic Status State Machine

This section formalizes the epistemic status system — 9 observable transitions from Rumor to Formally Proven, entirely without voting.

## 4.1 Motivation

Traditional knowledge systems use binary labels: "verified" or "unverified," "true" or "false." This is insufficient for knowledge management:

- A hypothesis is not "true" or "false" — it is a claim awaiting evidence.
- An observation is not "verified" — it is a first-person report awaiting corroboration.
- A consensus is not permanent — it can be overturned by new evidence.

PoMV uses an 11-level epistemic status scale inspired by philosophy (Justified True Belief [1]), law (standards of proof [2]), and science (NASA Technology Readiness Levels [3]):

$$\text{Rumor} \rightarrow \text{Hearsay} \rightarrow \text{Testimony} \rightarrow \text{Observation} \rightarrow \text{Hypothesis} \rightarrow \text{Evidence}$$
$$\rightarrow \text{Corroborated} \rightarrow \text{Peer Reviewed} \rightarrow \text{Consensus} \rightarrow \text{Formally Proven} \rightarrow \text{Axiomatic}$$

## 4.2 The Key Innovation: Observable Thresholds

> **Every transition is triggered by observable, CRDT-measurable thresholds. No human votes on epistemic status.**

This is the single most important design decision in PoMV. Each transition has precisely defined conditions that any node can independently verify from its local CRDT state:

```mermaid
graph LR
    R["RUMOR"] -->|"metabolic_rate > 0.001"| H["HEARSAY"]
    H -->|"retrieval_count ≥ 3"| T["TESTIMONY"]
    T -->|"citation_count ≥ 1"| O["OBSERVATION"]
    O -->|"citations ≥ 3 AND<br/>diversity ≥ 3"| HY["HYPOTHESIS"]
    HY -->|"node_diversity ≥ 5"| EV["EVIDENCE"]
    EV -->|"citations ≥ 5"| CO["CORROBORATED"]
    CO -->|"engagement ≥ 50"| PR["PEER_REVIEWED"]
    PR -->|"age ≥ 6 months AND<br/>rate ≥ 1.0"| CON["CONSENSUS"]
    CON -->|"age ≥ 1 year AND<br/>engagement ≥ 200"| FP["FORMALLY_PROVEN"]
    
    style R fill:#ef4444,color:#fff
    style H fill:#f97316,color:#fff
    style T fill:#eab308,color:#000
    style O fill:#84cc16,color:#000
    style HY fill:#22c55e,color:#fff
    style EV fill:#14b8a6,color:#fff
    style CO fill:#06b6d4,color:#fff
    style PR fill:#3b82f6,color:#fff
    style CON fill:#6366f1,color:#fff
    style FP fill:#8b5cf6,color:#fff
```

*Figure 3: Epistemic status state machine with observable transition conditions.*

## 4.3 Transition Specification

### Transition 1: RUMOR → HEARSAY

**Condition:** $\text{metabolic\_rate}(ku, t) > 0.001$

**Meaning:** Someone, somewhere, accessed this knowledge. The KU's "heartbeat" has started.

**Observable metric:** G-Counter `query_hits` or `retrieval_count` > 0 and metabolic rate above the alive threshold.

**Why this threshold:** The alive threshold (0.001) is deliberately low — a single genuine access is enough. The goal is simply to distinguish "has been seen" from "has never been seen."

### Transition 2: HEARSAY → TESTIMONY

**Condition:** $\text{retrieval\_count} \geq 3$

**Meaning:** At least 3 separate retrievals indicate sustained interest. The KU is not a one-time curiosity but something people return to read.

**Observable metric:** G-Counter `retrieval_count` summed across nodes.

**Why 3:** Below 3, a single interested user could generate all retrievals. At 3+, independent interest is more likely (though not guaranteed — hence this is "Testimony," not "Evidence").

### Transition 3: TESTIMONY → OBSERVATION

**Condition:** $\text{citation\_count} \geq 1$

**Meaning:** Another KU has cited this one. The knowledge has entered the citation network — it is being used as a building block for new knowledge.

**Observable metric:** G-Counter `citation_count`.

**Why 1:** A single inbound citation demonstrates that the KU's creator is not the only person who considers this knowledge worth referencing. The transition from "someone read it" to "someone built on it" is qualitatively significant.

### Transition 4: OBSERVATION → HYPOTHESIS

**Condition:** $\text{citation\_count} \geq 3$ AND $\text{node\_diversity} \geq 3$

**Meaning:** Multiple citations from diverse sources. The knowledge is being referenced by multiple independent actors.

**Observable metrics:** G-Counter `citation_count` + G-Counter `unique_nodes`.

**Why the diversity requirement:** 3 citations from a single node could be self-citation. Requiring diversity ≥ 3 ensures that at least 3 different nodes have interacted with this KU, making Sybil gaming more expensive.

### Transition 5: HYPOTHESIS → EVIDENCE

**Condition:** $\text{node\_diversity} \geq 5$

**Meaning:** The knowledge has been accessed by at least 5 distinct nodes in the network. This broadening of exposure increases the probability that the knowledge has been independently evaluated.

**Observable metric:** G-Counter `unique_nodes`.

**Why 5:** At diversity ≥ 5, the cost of Sybil gaming becomes significant — an attacker would need 5 distinct nodes, each with their own identity, computational resources, and plausible usage patterns.

### Transition 6: EVIDENCE → CORROBORATED

**Condition:** $\text{citation\_count} \geq 5$

**Meaning:** Strong citation evidence. Five independent KUs reference this knowledge as a foundation.

**Observable metric:** G-Counter `citation_count`.

**Analogy:** In academic publishing, a paper with 5+ citations is considered to have made a recognized contribution to the field.

### Transition 7: CORROBORATED → PEER REVIEWED

**Condition:** $\text{total\_engagement} \geq 50$

**Meaning:** Massive engagement — the sum of all usage counters (queries + retrievals + citations + derivatives + refutations + corroborations + downstream) exceeds 50.

**Observable metric:** Sum of all G-Counters.

**Why 50:** This threshold ensures broad community interaction, not just passive reading. At 50+ total engagement events, the knowledge has been thoroughly examined by the community.

### Transition 8: PEER REVIEWED → CONSENSUS

**Condition:** $\text{age} \geq 15{,}552{,}000\ \text{s}$ (6 months) AND $\text{metabolic\_rate} \geq 1.0$

**Meaning:** The knowledge has maintained high metabolism for at least 6 months. This is the "time test" — not just popularity but **sustained** value.

**Observable metrics:** Creation timestamp + current metabolic rate.

**Why 6 months:** Quick viral content can generate high short-term engagement but lacks lasting value. The 6-month requirement filters for knowledge that provides sustained value — like a scientific finding that continues to be cited months after publication.

### Transition 9: CONSENSUS → FORMALLY PROVEN

**Condition:** $\text{age} \geq 31{,}536{,}000\ \text{s}$ (1 year) AND $\text{total\_engagement} \geq 200$

**Meaning:** One full year of sustained high engagement. This is the highest achievable non-axiomatic status.

**Observable metrics:** Creation timestamp + total engagement sum.

**Why these thresholds:** At 1 year and 200+ engagement events, the knowledge has been continuously used, cited, and referenced over an extended period. This corresponds to knowledge that has become foundational in its domain.

### Terminal States

**FORMALLY PROVEN** and **AXIOMATIC** are terminal — no further transitions occur. AXIOMATIC is reserved for mathematical and logical truths (e.g., $1 + 1 = 2$) that are set at creation time, not earned through metabolism.

## 4.4 Formal Properties

### 4.4.1 Monotonicity

The status can only **increase** — once a KU reaches a status, it cannot be demoted back to a lower status through this state machine alone.

**Proof sketch:** Each G-Counter is monotonically non-decreasing (increment-only). Each threshold condition is a lower bound on a monotonically non-decreasing value. Therefore, once a condition is satisfied, it remains satisfied for all future states. ∎

### 4.4.2 Determinism

Given the same CRDT state, any two nodes will compute the same epistemic status for a KU.

**Proof sketch:** The `evaluate_max_status` function walks the status ladder from the current level upward, checking each threshold in order. All thresholds are deterministic functions of CRDT values. CRDT merge is convergent — eventually all nodes will have the same counter values. Therefore, epistemic status eventually converges across all nodes. ∎

### 4.4.3 Convergence

Epistemic status is eventually consistent across the network. Due to CRDT merge semantics, all nodes will converge to the same status for each KU.

**Caveat:** During network partitions, different nodes may temporarily assign different statuses. This is acceptable because:
1. Status only affects local PoMV scoring, not global state.
2. When partitions heal, CRDT merge will reconcile counters and status will converge.

## 4.5 Addressing the "But Is It True?" Objection

A natural objection: "Epistemic status measures *popularity*, not *truth*. Misinformation could reach CONSENSUS."

PoMV's response is multi-layered:

1. **Philosophically:** PoMV explicitly rejects the claim that any system can determine absolute truth. Even peer-reviewed science has a 60% replication failure rate [4]. What PoMV measures is **sustained utility** — whether knowledge continues to be useful to people.

2. **Prediction Signal:** Misinformation that makes false predictions will have a low Prediction score as predictions are refuted. This doesn't prevent CONSENSUS status but reduces the overall PoMV score.

3. **Natural Selection:** Better knowledge about the same topic will naturally attract metabolism from inferior knowledge. The carrying capacity (Niche signal) limits how many KUs about the same topic can thrive — eventually, the most metabolically active (most useful) KU dominates.

4. **Immune System:** Organized campaigns to artificially inflate metabolism are detected by the content-agnostic Spread Analysis and Immune Engine (§5).

5. **The Fundamental Answer:** If a piece of knowledge is used by thousands of people for years, cited by hundreds of other KUs, and survives adversarial challenges — it has *demonstrated value* regardless of whether a philosopher would call it "true." Newton's mechanics is "wrong" (superseded by Einstein), but it remains enormously valuable and widely used 300+ years later.

## 4.6 Comparison with Traditional Epistemic Systems

| Feature | Academic Peer Review | Wikipedia | PoMV Epistemic Status |
|---------|---------------------|-----------|----------------------|
| **Levels** | Binary (published/not) | Binary (verified/not) | 11 gradual levels |
| **Transition mechanism** | Editor decision | Editor consensus | Observable CRDT thresholds |
| **Reversibility** | Retraction (rare, stigmatized) | Any edit | Monotonic (no demotion) |
| **Time factor** | None (static once published) | None (static once verified) | 6-month and 1-year gates |
| **Subjective knowledge** | Not applicable | "Not notable" deletion | Full lifecycle support |
| **Decentralization** | Centralized (journal editors) | Semi-centralized (admins) | Fully decentralized (CRDT) |
| **Scalability** | Bottlenecked by reviewer pool | Bottlenecked by editor pool | Unlimited (automated) |

*Table 8: Epistemic status comparison across knowledge systems.*

---

## References

[1] E. L. Gettier, "Is Justified True Belief Knowledge?," *Analysis*, vol. 23, no. 6, pp. 121–123, 1963.

[2] J. O. Newman, "Quantifying the Standard of Proof Beyond a Reasonable Doubt," *Law, Probability and Risk*, vol. 5, no. 3–4, pp. 171–186, 2006.

[3] J. C. Mankins, "Technology Readiness Levels," NASA White Paper, 1995.

[4] Open Science Collaboration, "Estimating the Reproducibility of Psychological Science," *Science*, vol. 349, no. 6251, 2015.
