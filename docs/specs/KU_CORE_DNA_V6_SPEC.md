# KU Core DNA v6 Specification

> Specification version: 6.0 | Last updated: 2026-06-30

## §1 Philosophy

Knowledge Units (KUs) follow a **biological metaphor**:

| Biology | KU System | Description |
|---------|-----------|-------------|
| DNA | CoreDna | Immutable genetic code — the knowledge itself |
| Epigenetics | Epigenetics | Runtime modifications — trust, bonds, status |
| Protein | Expression | Phenotype rendering — human-readable text |
| Organism | KuRuntime | Living composite of all three layers |

**Core Principle**: CoreDna is immutable once created. Its CID (BLAKE3 hash) is its permanent identity. All changes occur in the Epigenetics layer.

---

## §2 Wire Format

```
MAGIC(0x4B) | VER_META(1B) | INSTRUCTION_STREAM | END(0x1E) | CRC-16(2B)
```

| Field | Size | Description |
|-------|------|-------------|
| MAGIC | 1 byte | `0x4B` ('K') |
| VER_META | 1 byte | bits[7:5]=version(0-7), bits[4:1]=gene_type(0-15), bit[0]=has_qualifiers |
| Instructions | variable | Sequence of opcode+operand instructions |
| END | 1 byte | `0x1E` terminal marker |
| CRC-16 | 2 bytes | CRC-16/CCITT over entire payload |

### Content Identifier (CID)

Every KU is identified by a 32-byte **BLAKE3** hash of its encoded CoreDna bytes:

```rust
let encoded = encode_core_dna(&dna)?;
let cid: [u8; 32] = blake3::hash(&encoded).into();
```

---

## §3 CoreDnaHeader

```rust
pub struct CoreDnaHeader {
    pub version: u8,         // Format version (0-7, current = 1)
    pub gene_type: u8,       // Gene type (0-15)
    pub has_qualifiers: bool, // Whether instructions contain qualifiers
}
```

### Gene Types

| Value | Name | Description |
|-------|------|-------------|
| 0 | Fact | Verified factual statement |
| 1 | Hypothesis | Testable proposition |
| 2 | Experience | First-person experience |
| 3 | Procedure | Step-by-step process |
| 4 | Rule | Conditional rule (if-then) |
| 5 | Definition | Concept definition |
| 6 | Relation | Relationship between concepts |
| 7 | Meta | Meta-knowledge about knowledge |
| 8 | Creative | Creative/artistic content |
| 9 | Belief | Subjective belief |
| 10 | FormalProof | Mathematically proven |

---

## §4 Instruction Set — 32 Opcodes (0x00-0x1F)

Each opcode occupies 5 bits in the OPCODE byte, with 3 modifier bits.

| Opcode | Hex | Name | Operands | Description |
|--------|-----|------|----------|-------------|
| 0 | 0x00 | TRIPLE | S, P, O | Subject-Predicate-Object fact |
| 1 | 0x01 | QUALITY | S, Q | Subject has quality Q |
| 2 | 0x02 | QUANTITY | S, value, unit | Numeric measurement |
| 3 | 0x03 | SEQUENCE | N, items... | Ordered list of concepts |
| 4 | 0x04 | PART_OF | part, whole | Hierarchical containment |
| 5 | 0x05 | LOCATED | S, location | Spatial relation |
| 6 | 0x06 | TEMPORAL | S, time | Time relation |
| 7 | 0x07 | CAUSAL | cause, effect | Causation link |
| 8 | 0x08 | SIMULATES | S, model | Analogy/simulation |
| 9 | 0x09 | CONDITION | if, then | Conditional logic |
| 10 | 0x0A | AGENT | actor, action | Who performs action |
| 11 | 0x0B | TOOL | action, instrument | Action uses tool |
| 12 | 0x0C | RANGE | S, min, max | Value range |
| 13 | 0x0D | TOLERANCE | S, value, ±delta | Value with error margin |
| 14 | 0x0E | CONSTRAINT | source, op, target | Numeric constraint (≤, ≥, =, ≠) |
| 15 | 0x0F | ENUM_VAL | S, N, values... | One of a set |
| 16 | 0x10 | CERTAINTY | level_u16 | Confidence 0-10000 |
| 17 | 0x11 | DIFFICULTY | level_u8 | Difficulty 0-4 |
| 18 | 0x12 | CID_REF | 32 bytes | BLAKE3 content reference |
| 19 | 0x13 | STEP | ord, action, target | Procedure step |
| 20 | 0x14 | PRECOND | concept | Step precondition |
| 21 | 0x15 | EFFECT | concept | Step effect/result |
| 22 | 0x16 | AFFECT | V, A, D (i16) | VAD emotion model |
| 23 | 0x17 | LABEL | key, value | Generic key-value metadata |
| 24 | 0x18 | TEXT_REF | lang, len, bytes | Compressed canonical text |
| 25 | 0x19 | FORMULA | format, len, bytes | LaTeX/MathML notation |
| 26 | 0x1A | WITNESS | count, proximity | Testimony data |
| 27 | 0x1B | MEDIA_REF | system, len, id_bytes | External media reference |
| 28 | 0x1C | COMPOSITE_HDR | type, completeness, ver | Composite header |
| 29 | 0x1D | MEMBER | order, role, required, label, cid | Composite member |
| 30 | 0x1E | END | — | Terminates instruction stream |
| 31 | 0x1F | EXTENDED | ext_byte, ... | Future extension slot |

### Operand Encoding

- **ConceptId**: Varint-encoded `u64` (1-10 bytes depending on value)
- **NumericValue**: Type prefix (0xFA-0xFF) + big-endian value

| Prefix | Type | Size |
|--------|------|------|
| 0xFA | u8 | 1+1 bytes |
| 0xFB | u16 | 1+2 bytes |
| 0xFC | i16 | 1+2 bytes |
| 0xFD | u32 | 1+4 bytes |
| 0xFE | i32 | 1+4 bytes |
| 0xFF | f32 | 1+4 bytes |

### ConstraintOp Encoding

| Value | Operator |
|-------|----------|
| 0 | == |
| 1 | != |
| 2 | < |
| 3 | <= |
| 4 | > |
| 5 | >= |

---

## §5 Epigenetics Layer

```rust
pub struct Epigenetics {
    pub trust: TrustSection,
    pub bonds: Vec<Bond>,
    pub epistemic_status: EpistemicStatus,
    pub first_seen: u64,
    pub last_accessed: u64,
    pub access_count: u32,
}
```

### TrustSection

7 signal fields updated by PomvRuntime:

```rust
pub struct TrustSection {
    pub epistemic_status: EpistemicStatus,
    pub metabolic_rate: u16,        // [0, 10000]
    pub prediction_score: u16,      // [0, 10000]
    pub entropy_at_creation: u16,   // [0, 10000]
    pub survival_score: u16,        // [0, 10000]
    pub synaptic_centrality: u16,   // [0, 10000]
    pub niche_fitness: u16,         // [0, 10000]
}
```

### Bond

```rust
pub struct Bond {
    pub target_cid: [u8; 32],
    pub bond_type: BondType,
    pub strength: f32,
    pub created_at: u64,
}
```

---

## §6 Expression Layer

```rust
pub struct Expression {
    pub text: String,                        // Rendered natural language text
    pub lang: String,                        // Language code (e.g., "vi", "en")
    pub concept_names: Vec<(ConceptId, String)>, // Name mappings used
}
```

**Lazy rendering**: Expression is computed on-demand via `KuRuntime::expression(lang, dict)`.
Cached until language changes.

### Rendering Rules

| Instruction | Rendering |
|-------------|-----------|
| Triple(s,p,o) | "{s} {p} {o}" |
| Quality(s,q) | "{s}: {q}" |
| Quantity(s,v,u) | "{s} = {v} {u}" |
| Step(n,a,t) | "Step {n}: {a} {t}" |
| Precond(c) | "Requires: {c}" |
| Effect(c) | "Effect: {c}" |
| PartOf(p,w) | "{p} ⊂ {w}" |
| Located(s,l) | "{s} @ {l}" |
| Temporal(s,t) | "{s} → {t}" |
| Causal(c,e) | "{c} → {e}" |
| Certainty(l) | "Certainty: {l/100}%" |
| Tolerance(s,v,d) | "{s} = {v} ± {d}" |
| Range(s,min,max) | "{s} ∈ [{min}, {max}]" |
| Constraint(s,op,t) | "{s} {op} {t}" |

---

## §7 KuRuntime — Primary v6 Type

```rust
pub struct KuRuntime {
    pub dna: CoreDna,           // Layer 1: Immutable gene code
    pub epi: Epigenetics,       // Layer 2: Mutable runtime state
    pub expr: Option<Expression>, // Layer 3: Lazy-rendered text
    pub cid: [u8; 32],          // BLAKE3 content ID
}
```

### Key Methods

| Method | Description |
|--------|-------------|
| `from_dna(dna) → Result<KuRuntime>` | Create from CoreDna (computes CID) |
| `expression(&mut self, lang, dict) → &Expression` | Lazy render + cache |
| `apply_pomv_update(&mut self, update)` | Apply PoMV tick results |
| `cid_bytes() → [u8; 32]` | Get CID for PomvRuntime key |
| `extract_field(field) → ExtractedValue` | Extract field for KQL queries |

---

## §8 ConceptDict — Bilingual Concept Dictionary

### v6 ConceptDict

```rust
pub struct ConceptDict {
    by_id: HashMap<ConceptId, ConceptEntry>,
    by_name: HashMap<String, ConceptId>,
    next_id: ConceptId,
}

pub struct ConceptEntry {
    pub id: ConceptId,
    pub name: String,
    pub name_vi: Option<String>,
    pub name_en: Option<String>,
    pub tier: u8,        // Varint tier (0-9)
    pub category: u8,    // Semantic category
}
```

### PersistentConceptDict (redb)

Feature-gated (`#[cfg(feature = "persist")]`):

```rust
pub struct PersistentConceptDict {
    db: Database,  // redb ACID database
}
```

Three tables: `concepts` (name→JSON), `ids` (u64→name), `meta` (next_id).

### text_parser::ConceptDict

Simpler version for Tier 1 rule-based parsing:

```rust
pub struct ConceptDict {
    map: HashMap<String, ConceptId>,
    next_id: ConceptId,
}
```

---

## §9 Varint Encoding

ConceptIds use variable-length integer encoding:

| Range | Bytes | Tier |
|-------|-------|------|
| 0 – 127 | 1 | Tier 0: Universal primitives |
| 128 – 16,383 | 2 | Tier 1: Common concepts |
| 16,384 – 2,097,151 | 3 | Tier 2: Domain-specific |
| 2,097,152+ | 4-10 | Tier 3+: Rare/generated |
