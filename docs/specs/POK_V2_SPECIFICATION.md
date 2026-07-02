# PoK v2 Technical Specification — PoMV Runtime

> Specification version: 2.0 | Last updated: 2026-06-30

## §1 Scope

This specification defines the runtime behavior of PoK v2 (Proof of Metabolic Value) — the core value assessment mechanism in OneBrain. It covers:

1. Metabolism event recording and CRDT storage
2. Six signal computation
3. Weighted aggregation (PomvScore)
4. Epistemic status transitions
5. KuLifecycle integration

---

## §2 Data Structures

### 2.1 PomvConfig

```rust
pub struct PomvConfig {
    pub weights: PomvWeights,      // Signal weights
    pub half_life_secs: u64,       // Default: 2,592,000 (30 days)
    pub entropy_decay_secs: u64,   // Entropy decay period
    pub node_id: u64,              // Local node identifier
}
```

### 2.2 KUPomvState

Per-KU state maintained by PomvRuntime:

```rust
pub struct KUPomvState {
    pub predictions: PredictionRegistry,
    pub synaptic: SynapticMap,
    pub entropy_at_creation: f32,
    pub bridge_at_creation: f32,
    pub created_at: u64,
    pub attacks_survived: u32,
    pub niches: Vec<NicheId>,
    pub cross_niche_count: usize,
    pub epistemic_status: EpistemicStatus,
}
```

### 2.3 MetabolismEvent

```rust
pub enum MetabolismEvent {
    QueryHit,                    // Search result hit
    Retrieval { dwell_ms: u64 }, // Read with dwell time
    Citation,                    // Cited by another KU
    Derivative,                  // Derived/inspired new KU
    DownstreamUsage,            // Citation chain used
    Corroboration,              // Explicit confirmation
    Refutation,                 // Challenge (positive engagement!)
}
```

### 2.4 GCounter (CRDT)

```rust
pub struct GCounter {
    counts: HashMap<u64, u64>,   // node_id → count
}

// Merge: merged[node] = max(local[node], remote[node])
// Value: sum of all node counts
```

### 2.5 KUMetabolism

```rust
pub struct KUMetabolism {
    // Consumption
    pub query_hits: GCounter,
    pub retrieval_count: GCounter,
    pub dwell_time_ms: GCounter,
    // Transformation
    pub citation_count: GCounter,
    pub derivative_count: GCounter,
    pub downstream_usage: GCounter,
    // Engagement
    pub corroboration_count: GCounter,
    pub refutation_count: GCounter,
    // Temporal
    pub created_at: u64,
    pub last_event_at: u64,
}
```

> **Implementation note**: In the actual code, `last_activity` uses `LWWRegister<u64>` for CRDT
> conflict resolution, and an additional `unique_nodes: GCounter` field tracks distinct node interactions.

```rust
// (continued from above)
```

### 2.6 PomvSignals & PomvScore

```rust
pub struct PomvSignals {
    pub metabolism: f32,     // [0, 1]
    pub prediction: f32,     // [0, 1]
    pub entropy: f32,        // [0, 1]
    pub survival: f32,       // [0, 1]
    pub synaptic: f32,       // [0, 1]
    pub niche_fitness: f32,  // [0, 1]
}

pub struct PomvScore {
    pub total: f32,
    pub contributions: PomvSignals,
}
```

### 2.7 PomvWeights

```rust
pub const DEFAULT_WEIGHTS: PomvWeights = PomvWeights {
    metabolism: 0.35,
    prediction: 0.15,
    entropy: 0.10,
    survival: 0.10,
    synaptic: 0.15,
    niche: 0.15,
};
```

### 2.8 TrustSectionUpdate

Bridge output from PomvRuntime → KuRuntime:

```rust
pub struct TrustSectionUpdate {
    pub epistemic_status: EpistemicStatus,
    pub metabolic_rate: u16,        // [0, 10000]
    pub prediction_score: u16,      // [0, 10000]
    pub entropy_at_creation: u16,   // [0, 10000]
    pub survival_score: u16,        // [0, 10000]
    pub synaptic_centrality: u16,   // [0, 10000]
    pub niche_fitness: u16,         // [0, 10000]
    pub pomv_total: f32,
}
```

### 2.9 NicheStats

```rust
pub struct NicheStats {
    pub population: usize,
    pub total_metabolic_rate: f32,
    pub avg_metabolic_rate: f32,
    pub source_diversity: usize,
}
```

---

## §3 Runtime Lifecycle

### 3.1 Initialization

```rust
let config = PomvConfig::default();
let mut runtime = PomvRuntime::new(config);
```

### 3.2 KU Registration

```rust
runtime.register_ku(
    cid,            // [u8; 32] — BLAKE3 content ID
    created_at,     // u64 — timestamp
    vec![1, 5],     // NicheId list
    0.7,            // novelty score
    0.3,            // bridge score
);
```

> **Encoding Status**: Newly registered KUs start with an `encoding_status` determined by their creation method:
> - `CREATE` (Tier 1 structured) → `encoding_status = SELF` (locally encoded with explicit instructions)
> - `CREATE FROM TEXT` (Tier 2 AI-assisted) → `encoding_status = SELF` (AI-encoded locally)
> - Received via network sync → `encoding_status = RAW` (not yet verified by this node)
>
> The encoding status lifecycle (RAW → SELF → PART → FULL) progresses independently via the Encoding Consensus Protocol (see OBP_SPEC §4.5).

### 3.3 Event Recording

Events flow in from user interaction, peer gossip, and automated processes:

```rust
runtime.record_event(cid, MetabolismEvent::QueryHit, now);
runtime.record_event(cid, MetabolismEvent::Retrieval { dwell_ms: 5000 }, now);
runtime.record_event(cid, MetabolismEvent::Citation, now);
```

### 3.4 Tick Cycle

Periodic computation (recommended: every 60-300 seconds):

```rust
let results: Vec<([u8; 32], PomvScore, TrustSectionUpdate)> =
    runtime.tick(now, &niche_stats);

// Apply to KuRuntimes
for (cid, _score, update) in &results {
    if let Some(ku) = kus.get_mut(cid) {
        ku.apply_pomv_update(update);
    }
}
```

### 3.5 CRDT Gossip Merge

When receiving metabolism data from peer nodes:

```rust
runtime.merge_remote_metabolism(cid, &remote_metabolism);
```

### 3.6 Maintenance

```rust
// Garbage-collect dead KUs (metabolic rate = 0)
let removed = runtime.gc(now);

// Decay synaptic bonds (call periodically, e.g., daily)
runtime.evaporate_bonds();
```

### 3.7 Relationship to Encoding Consensus

The PoMV epistemic lifecycle (Rumor → … → Axiomatic) and the Encoding Consensus lifecycle (RAW → SELF → PART → FULL) are **parallel but independent** processes:

- **PoMV epistemic status** tracks the *knowledge credibility* of a KU — how well it has been observed, corroborated, and metabolized by the network.
- **Encoding status** tracks the *structural encoding fidelity* of a KU — whether its CoreDna binary representation has been verified by distributed consensus.

Both lifecycles run concurrently on each KU. A KU may reach `Corroborated` epistemic status while still at `SELF` encoding status (awaiting network verification), or vice versa. The two lifecycles inform but do not gate each other.

**OBT Rewards**: OBT tokens are **utility tokens** that reward knowledge contribution.
The PoMV score feeds directly into the OBT minting formula:
- R1 (Owner reward) = pomv_score × max_reward_per_epoch
- Quality Gate 3 requires minimum PoMV score for reward eligibility
See `docs/specs/obt/03_MINTING.md` for the complete reward formula.

Encoding participants (Verifiers, Correctors, Pro-Bono encoders) receive separate encoding rewards via `encoding_reward.rs` — proportional to raw text complexity. Contributors (people who provide knowledge) are rewarded through PoMV lifecycle. See [ENCODING_CONSENSUS_SPEC §9](ENCODING_CONSENSUS_SPEC.md) for reward model.

---

## §4 Epistemic Engine

### 4.1 Status Enum

```rust
#[repr(u8)]
pub enum EpistemicStatus {
    Rumor          = 0x00,
    Hearsay        = 0x01,
    Testimony      = 0x02,
    Observation    = 0x03,
    Hypothesis     = 0x04,
    Evidence       = 0x05,
    Corroborated   = 0x06,
    PeerReviewed   = 0x07,
    Consensus      = 0x08,
    FormallyProven = 0x09,
    Axiomatic      = 0x0A,
}
```

### 4.2 Transition Logic

```rust
// epistemic_engine.rs
pub fn evaluate_transition(
    current: EpistemicStatus,
    metabolism: &KUMetabolism,
    now: u64,
    half_life_secs: u64,
) -> Option<EpistemicStatus>

// Multi-jump: advance as far as possible in one tick
pub fn evaluate_max_status(
    current: EpistemicStatus,
    metabolism: &KUMetabolism,
    now: u64,
    half_life_secs: u64,
) -> EpistemicStatus
```

### 4.3 Transition Rules

Transitions require minimum metabolic activity thresholds. Each level has progressively higher requirements for metabolic_rate, citation_count, corroboration, etc.

Key principle: **Only forward transitions** during normal operation. Backward transitions occur only through `gc()` (death) or manual override.

---

## §5 Integration with KuRuntime

### 5.1 Bridge Methods

On `KuRuntime`:

```rust
/// Apply PoMV tick results to this KU's epigenetics
pub fn apply_pomv_update(&mut self, update: &TrustSectionUpdate)

/// Return raw CID bytes for PomvRuntime key lookup
pub fn cid_bytes(&self) -> [u8; 32]
```

### 5.2 TrustSection Fields

Updated by `apply_pomv_update()`:

```rust
pub struct TrustSection {
    pub epistemic_status: EpistemicStatus,
    pub metabolic_rate: u16,
    pub prediction_score: u16,
    pub entropy_at_creation: u16,
    pub survival_score: u16,
    pub synaptic_centrality: u16,
    pub niche_fitness: u16,
}
```

### 5.3 KuLifecycle

Full integration point:

```rust
pub struct KuLifecycle {
    pub kus: HashMap<[u8; 32], KuRuntime>,
    pub pomv: PomvRuntime,
}

// Unified methods:
// ingest() → register in both stores
// record_event() → metabolism tracking
// tick() → PoMV computation + apply updates
// gc() → remove dead KUs from both stores
```

---

## §6 Test Coverage

| Module | Tests |
|--------|-------|
| metabolism.rs | GCounter CRDT, metabolic rate, event recording |
| metabolism_store.rs | Multi-KU store, gc_dead |
| pomv.rs | Signal computation, weighted aggregation |
| pomv_runtime.rs | Full tick cycle, epistemic transitions |
| epistemic_engine.rs | Threshold transitions, multi-jump |
| entropy.rs | Novelty decay, bridge bonus |
| prediction.rs | Score tracking |
| synaptic.rs | Bond strength, evaporation |
| immune.rs | Survival scoring |
| ecosystem.rs | Niche fitness |
| ku_lifecycle.rs | Full lifecycle integration |
| **Total** | 375+ tests across ku-core |

---

## §7 PoMV ↔ OBT Epoch Alignment

The OBT token system uses 1-hour epochs (3,600 seconds) for reward settlement.
PoMV runtime runs independently with configurable tick intervals (recommended: 60-300 seconds).

At each OBT epoch boundary, the epoch settlement process:
1. Collects PoMV scores from all active KUs
2. Computes `avg_pomv_score` across the epoch
3. Uses this average in the emission formula's Q factor
4. Distributes R1 (owner) rewards proportional to individual PoMV scores

This design keeps PoMV computation independent from OBT economics while
providing clean integration via the `obt_integration.rs` bridge layer.
