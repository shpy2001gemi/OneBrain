# 7. Implementation and Evaluation

## 7.1 Implementation Summary

### 7.1.1 Module Inventory

PoMV is implemented across **16 modules** in two Rust crates:

**Core Modules (ku-core):**

| # | Module | File | LOC | Structs/Enums | Constants | Tests |
|:-:|--------|------|----:|:------------:|:---------:|:-----:|
| 1 | Metabolism | [metabolism.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism.rs) | 385 | 2 | 8 | 16 |
| 2 | Metabolism Store | [metabolism_store.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/metabolism_store.rs) | 235 | 2 | 3 | 7 |
| 3 | Epistemic Engine | [epistemic_engine.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epistemic_engine.rs) | 300 | 0 | 11 | 10 |
| 4 | Entropy | [entropy.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/entropy.rs) | 280 | 1 | 5 | 15 |
| 5 | Prediction | [prediction.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/prediction.rs) | 350 | 5 | 0 | 12 |
| 6 | Synaptic | [synaptic.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/synaptic.rs) | 382 | 4 | 9 | 14 |
| 7 | Immune | [immune.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/immune.rs) | 389 | 3 | 7 | 11 |
| 8 | Ecosystem | [ecosystem.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ecosystem.rs) | 292 | 3 | 4 | 8 |
| 9 | PoMV Aggregator | [pomv.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv.rs) | 256 | 4 | 1 | 9 |
| 10 | EigenTrust | [eigentrust.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/eigentrust.rs) | 272 | 3 | 5 | 9 |
| 11 | Spread Analysis | [spread_analysis.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/spread_analysis.rs) | 308 | 2 | 4 | 11 |
| 12 | Runtime | [pomv_runtime.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/pomv_runtime.rs) | 466 | 3 | 0 | 9 |
| 13 | KU Lifecycle | [ku_lifecycle.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/ku_lifecycle.rs) | 246 | 1 | 0 | 5 |
| 14 | Epigenetics | [epigenetics.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/epigenetics.rs) | 219 | 2 | 3 | 7 |
| 15 | OBT Integration | [obt_integration.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/obt_integration.rs) | 354 | 1 | 0 | 8 |
| | **Core Subtotal** | | **4,734** | **36** | **60** | **151** |

*Table 12: PoMV core modules with size, complexity, and test metrics.*

**Network Module (ku-net):**

| # | Module | File | LOC | Structs/Enums | Tests |
|:-:|--------|------|----:|:------------:|:-----:|
| 16 | Metabolism Gossip | [metabolism_gossip.rs](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-net/src/metabolism_gossip.rs) | 278 | 4 | 6 |
| | **Network Subtotal** | | **278** | **4** | **6** |

**Grand Total:** 5,012 LOC | 40 types | 60 constants | 157 tests

### 7.1.2 Type Safety

All 40 struct/enum definitions enforce type safety at compile time:

- `MetabolismEvent` (7 variants) — prevents recording undefined event types
- `ResolutionMethod` (4 variants) — constrains prediction resolution modes
- `PredictionOutcome` (4 variants) — bounded outcome classification
- `AntibodyType` (4 variants) — typed immune detection categories
- `BondReason` (3 variants) — synaptic bond causation tracking

### 7.1.3 Dependencies

| Dependency | Purpose | CRDT Used |
|-----------|---------|:---------:|
| G-Counter (ku-core/crdt.rs) | Metabolism counters, engagement | ✅ |
| LWW-Register (ku-core/crdt.rs) | Last activity timestamp, prediction resolution | ✅ |
| ORSet (ku-core/crdt.rs) | Antibody gossip, prediction registry | ✅ |
| BLAKE3 (ku-core) | Pattern hashing, CID computation | — |
| Types (ku-core/types.rs) | EpistemicStatus, TrustSection, Gene | — |

**Zero external dependencies** for PoMV — all components are built on the existing CRDT infrastructure in `ku-core/crdt.rs`.

## 7.2 Test Coverage

### 7.2.1 Unit Tests by Module (157 tests)

**Metabolism (16 tests):**

| Test | Validates |
|------|-----------|
| `test_new_metabolism_starts_at_zero` | Fresh KU has zero counters |
| `test_query_hit_increases_rate` | QueryHit → metabolic_rate > 0 |
| `test_retrieval_with_dwell_time` | Retrieval + dwell time tracking |
| `test_citation_increases_rate` | Citation boosts metabolic rate |
| `test_refutation_is_positive` | **Key: refutation increments, doesn't decrement** |
| `test_metabolic_rate_decays_over_time` | Exponential decay with half-life |
| `test_node_diversity_tracking` | Unique node counter accuracy |
| `test_merge_two_metabolisms` | CRDT merge produces correct state |
| `test_merge_idempotent` | Merging same data twice = no change |
| `test_is_alive_with_no_activity` | Zero activity → not alive |
| `test_is_alive_with_activity` | Positive activity → alive |
| `test_rate_to_u16_normalization` | Sigmoid normalization correctness |
| `test_avg_dwell_no_retrievals` | Edge case: division by zero handled |
| `test_total_engagement_counts_all` | Sum of all 7 counters |
| `test_merge_keeps_earliest_created_at` | Merge preserves birth timestamp |
| `test_zero_metabolism_after_very_long_time` | Decay approaches zero |

**Epistemic Engine (10 tests):**

| Test | Validates |
|------|-----------|
| `test_rumor_stays_without_activity` | No activity → stays RUMOR |
| `test_rumor_to_hearsay` | metabolic_rate > 0.001 → HEARSAY |
| `test_hearsay_to_testimony` | retrieval_count ≥ 3 → TESTIMONY |
| `test_testimony_to_observation` | citation_count ≥ 1 → OBSERVATION |
| `test_observation_to_hypothesis` | citations ≥ 3 + diversity ≥ 3 → HYPOTHESIS |
| `test_hypothesis_to_evidence` | node_diversity ≥ 5 → EVIDENCE |
| `test_evidence_to_corroborated` | citations ≥ 5 → CORROBORATED |
| `test_formally_proven_is_terminal` | No transition from FORMALLY_PROVEN |
| `test_evaluate_max_status_jumps` | Multi-step jump when thresholds met |
| `test_consensus_requires_time` | Time gate: 6 months + high rate |

**Immune Engine (11 tests):**

| Test | Validates |
|------|-----------|
| `test_healthy_spread_no_antibodies` | Organic spread → no detection |
| `test_temporal_burst_detected` | 50+/hour → antibody fired |
| `test_source_concentration_detected` | >80% single source → detected |
| `test_low_engagement_detected` | High replication, low usage → detected |
| `test_diversity_deficit_detected` | Few sources, many replications → detected |
| `test_bot_spread_multiple_signals` | Bot behavior triggers multiple antibodies |
| `test_quarantine_requires_multiple_types` | Single antibody insufficient for quarantine |
| `test_survival_score_anti_fragile` | 10 attacks → score = 1.0 |
| `test_dead_ku_no_survival_bonus` | Dead KU gets 0 survival |
| `test_survival_to_u16` | u16 normalization accuracy |
| `test_too_few_replications_no_flags` | Below threshold → no false positives |

**Spread Analysis (11 tests):**

| Test | Validates |
|------|-----------|
| `test_organic_high_score` | Natural spread → high organicity |
| `test_bot_low_score` | Bot spread → low organicity |
| `test_organic_beats_bot` | Organic always scores higher than bot |
| `test_temporal_regular_intervals_bot` | Regular timing → bot detection |
| `test_temporal_varied_intervals_organic` | Varied timing → organic classification |
| `test_source_diversity_high` | Many unique sources → high score |
| `test_source_diversity_low` | Few sources → low score |
| `test_engagement_bot_dwell` | <1s dwell → bot engagement |
| `test_engagement_real_user` | >5s dwell → real engagement |
| `test_organicity_multiplier` | Multiplier formula correctness |
| `test_empty_metrics_neutral` | Edge case: empty input → neutral |

### 7.2.2 Integration Properties Verified

| Property | Test Evidence |
|----------|-------------|
| **CRDT convergence** | `test_merge_idempotent`, `test_merge_two_metabolisms`, `test_merge_registries`, `test_merge_synaptic_maps` |
| **Monotonic status** | `test_formally_proven_is_terminal`, `test_evaluate_max_status_jumps` |
| **Non-punitive** | `test_refutation_is_positive`, G-Counter increment-only semantics |
| **Content-agnostic** | All immune tests use `SpreadObservation` (behavioral data), never content |
| **Antifragile** | `test_survival_score_anti_fragile` — 10 attacks → max bonus |
| **False positive prevention** | `test_quarantine_requires_multiple_types`, `test_too_few_replications_no_flags` |
| **Full lifecycle** | `test_full_lifecycle` — creation → events → tick → score |

## 7.3 Comparison: PoMV vs PoK v1

PoMV is a complete redesign of the original PoK v1, which used a vote-based architecture:

| Dimension | PoK v1 (Vote-based) | PoMV v2 (Observation-based) |
|-----------|---------------------|---------------------------|
| **Who decides value** | Community voters | No one — usage is objective |
| **Subjective knowledge** | Cannot evaluate ("Is sunset beautiful?") | Full support (metabolism = usage) |
| **Architecture** | 5 layers (Identity → Screening → Evaluation → Trust → Evolution) | 6 signals + immune system |
| **Anti-manipulation** | 7 mechanisms (quadratic voting, commit-reveal, PoU quiz, staking, collusion detection, EigenTrust, AI screening) | 4 antibodies + spread analysis + EigenTrust + immune memory |
| **Reward model** | Asymmetric staking (1:3 risk ratio) + clawback | Linear PoMV share, **no clawback** |
| **Epistemic transitions** | Vote-based quorum | Observable CRDT thresholds |
| **Complexity** | 11+ defense layers | 6 signals + 4 antibodies |
| **Decentralization** | Needs quorum (partial) | Each node evaluates independently (full) |
| **Philosophical alignment** | "Is this knowledge correct?" | "Is this knowledge used?" |
| **Code complexity** | Not implemented | 5,012 LOC, 157 tests, fully implemented |

*Table 13: PoK v1 vs PoMV v2 comparison.*

The fundamental shift: PoK v1 tried to answer an unanswerable question ("Is this knowledge correct?"). PoMV v2 answers an empirically observable question ("Is this knowledge used?").

## 7.4 Threat Analysis

### 7.4.1 Attack Scenarios and Defenses

| # | Attack | Mechanism | Defense | Cost to Attack | Outcome |
|:-:|--------|-----------|---------|:--------------:|---------|
| 1 | **Query bombing** | Bot sends 10K queries for target KU | Temporal burst antibody + source concentration | Low | Quarantined; organicity ≈ 0 |
| 2 | **Citation ring** | 5 Sybil nodes cite each other | Diversity deficit + EigenTrust low trust | Medium (5 S/Kademlia puzzles) | Low PoMV; low node trust |
| 3 | **Dwell time inflation** | Bot reports long dwell times | Engagement authenticity (action_ratio check) | Low | Low engagement score |
| 4 | **Entropy gaming** | Submit bizarre content for entropy bonus | 7-day decay → 0 metabolism → natural death | Low | Temporary boost, no lasting reward |
| 5 | **Flash mob** | 100 nodes coordinate 1-hour campaign | All 4 antibodies fire; quarantine | Very high (100 puzzles) | Quarantined within 1 epoch |
| 6 | **Slow manipulation** | 10 nodes, organic-looking, sustained | Passes spread analysis | Extremely high | Succeeds — but cost ≈ real value delivery |
| 7 | **Eclipse attack** | Isolate a node to control its view | SWIM protocol (not PoMV's domain) | Network-level attack | Handled by transport layer |

### 7.4.2 The "Slow Manipulation" Argument

Attack #6 (slow, organic-looking manipulation by real nodes with sustained engagement) is the only attack that PoMV cannot automatically detect. This is **by design**:

> If 10 real nodes spend months creating real content, generating real citations, sustaining real dwell times, and maintaining diverse sources — they have **actually delivered value** to the network. The "attack" is indistinguishable from genuine contribution because it IS genuine contribution.

The cost of sustained organic-looking manipulation at scale exceeds the reward from PoMV, making it economically irrational. This is analogous to the "51% attack" argument in Bitcoin — theoretically possible but economically irrational because the cost of attacking exceeds the benefit.

## 7.5 Performance Characteristics

### 7.5.1 Per-Module Computational Cost

| Module | Operation | Complexity | Memory |
|--------|-----------|:----------:|:------:|
| Metabolism | record_event() | O(1) | O(N_nodes) per KU |
| Metabolism | metabolic_rate() | O(1) | — |
| Metabolism Store | tick() all KUs | O(N_KUs) | O(N_KUs) |
| Epistemic Engine | evaluate_max_status() | O(1) | — |
| Entropy | cosine_distance() | O(D) where D=embedding dim | — |
| Entropy | novelty_score() | O(K × D) where K=neighbors | — |
| Prediction | prediction_score() | O(P × R) where P=predictions, R=resolutions | O(P + R) |
| Synaptic | reinforce() | O(1) amortized | O(B) per KU, B≤100 |
| Synaptic | centrality (PageRank) | O(I × N × B) where I=10 iterations | O(N) |
| Immune | analyze() | O(1) | O(1) |
| Spread Analysis | analyze() | O(T) where T=timestamps | O(T) |
| EigenTrust | compute_global() | O(I × N²) where I=10, N=nodes | O(N²) |
| PoMV Aggregator | compute() | O(1) | O(1) |
| Runtime | tick() | O(N_KUs × (PageRank + EigenTrust)) | O(N_KUs + N²) |

*Table 14: Computational complexity of PoMV operations.*

### 7.5.2 Expected Performance at Scale

| Scale | KU Count | Node Count | tick() Time | Memory |
|-------|:--------:|:----------:|:-----------:|:------:|
| Small | 1,000 | 100 | <10ms | ~10MB |
| Medium | 100,000 | 10,000 | ~1s | ~1GB |
| Large | 10,000,000 | 1,000,000 | ~100s (batch) | ~100GB |

At large scale, the tick() function would be batched and parallelized. The PageRank centrality computation and EigenTrust iteration are the bottlenecks — both can be approximated for scalability (Monte Carlo sampling for PageRank, local-only EigenTrust).

---
