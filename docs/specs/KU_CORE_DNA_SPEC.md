# KU Core DNA Specification

> Specification version: 7.1 | Last updated: 2026-07-19

## §1 Philosophy

Knowledge Units (KUs) follow a **biological metaphor**:

| Biology | KU System | Description |
|---------|-----------|-------------|
| DNA | CoreDna | Immutable genetic code — the knowledge itself |
| Epigenetics | Epigenetics | Runtime modifications — trust, bonds, status |
| Protein | Expression | Phenotype rendering — human-readable text |
| Organism | KuRuntime | Living composite of all three layers |

**Core Principle**: CoreDna is immutable once created. Its CID (32-byte BLAKE3 hash of encoded bytes) is its permanent identity. All changes occur in the Epigenetics layer.

**Language-agnostic**: CoreDna contains no natural language text. Concepts are referenced by numeric ConceptIDs, each globally identified by a 16-byte CCID (Content-Addressed Concept Identity). Expression is generated on-demand.

---

## §2 Wire Format

```
MAGIC(0x4B) | VER_META(1B) | [CONCEPT_TABLE] | INSTRUCTION_STREAM | END(0x1E) | CRC-16(2B)
```

| Field | Size | Description |
|-------|------|-------------|
| MAGIC | 1 byte | `0x4B` ('K') |
| VER_META | 1 byte | bits[7:5] = version (current = `010`), bits[4:1] = gene_type (0–15), bit[0] = has_concept_table |
| Concept Table | variable | Present only if `has_concept_table = 1`. See §2.1. |
| Instructions | variable | Sequence of opcode + operand instructions |
| END | 1 byte | `0x1E` terminal marker |
| CRC-16 | 2 bytes | CRC-16/CCITT (poly 0x1021, init 0xFFFF) over everything before CRC |

### §2.1 Concept Table

When `has_concept_table = 1`, the concept table appears immediately after VER_META:

```
COUNT(varint) | ENTRY[0] | ENTRY[1] | ... | ENTRY[COUNT-1]
```

Each ENTRY:

```
LOCAL_ID(varint) | CCID(16 bytes)
```

| Field | Encoding | Description |
|-------|----------|-------------|
| COUNT | varint | Number of entries |
| LOCAL_ID | varint | Local ConceptId used in this KU's instructions |
| CCID | 16 bytes raw | 128-bit truncated BLAKE3 hash of the concept's canonical form |

Only Tier 2+ concepts (ID ≥ 16,512) need entries. Tier 0 and Tier 1 concepts are universally known.

### §2.2 Content Identifier (CID)

Every KU is identified by a 32-byte BLAKE3 hash of its encoded CoreDna bytes:

```rust
let encoded = encode_core_dna(&dna)?;
let cid: [u8; 32] = blake3::hash(&encoded).into();
```

---

## §3 CoreDna Structures

### §3.1 CoreDnaHeader

```rust
pub struct CoreDnaHeader {
    pub version: u8,            // Format version (0–7, current = 2)
    pub gene_type: u8,          // Gene type (0–15)
    pub has_concept_table: bool, // Whether this KU contains a concept table
}
```

### §3.2 CoreDna

```rust
pub struct CoreDna {
    pub header: CoreDnaHeader,
    pub concept_table: ConceptTable,
    pub instructions: Vec<Instruction>,
}
```

### §3.3 ConceptTableEntry

```rust
pub struct ConceptTableEntry {
    pub local_id: ConceptId,  // Local ID used in instruction stream
    pub ccid: [u8; 16],       // 128-bit CCID
}

pub type ConceptTable = Vec<ConceptTableEntry>;
```

---

## §4 Gene Types — 13 Variants

Gene type is encoded in VER_META bits[4:1] (4 bits → 0–15). Types 0–6 fit directly. Types 7+ use the EXTENDED opcode mechanism (`base = 7`, ext byte follows).

| Value | Name | Wire | Description |
|-------|------|------|-------------|
| 0 | Fact | `(0, —)` | Verified factual statement |
| 1 | Procedure | `(1, —)` | Step-by-step process |
| 2 | Experience | `(2, —)` | First-person experience |
| 3 | Creative | `(3, —)` | Creative/artistic content |
| 4 | MediaExperience | `(4, —)` | Multi-sensory media experience |
| 5 | Testimony | `(5, —)` | Witnessed account |
| 6 | Formal | `(6, —)` | Formally proven (math, logic) |
| 7 | Hypothesis | `(7, 0x00)` | Testable proposition |
| 8 | Narrative | `(7, 0x01)` | Story/narrative structure |
| 9 | Sensory | `(7, 0x02)` | Sensory description |
| 10 | Composite | `(7, 0x03)` | Multi-gene composite KU |
| 11 | Normative | `(7, 0x04)` | Prescriptive rule (should/ought) |
| 12 | Definition | `(7, 0x05)` | Concept definition |

---

## §5 Instruction Set — 32 Opcodes (0x00–0x1F)

Each instruction starts with a 1-byte OPCODE. Bits[7:3] = opcode (5 bits), bits[2:0] = modifier (3 bits).

| Opcode | Hex | Name | Operands | Description |
|--------|-----|------|----------|-------------|
| 0 | 0x00 | TRIPLE | S, P, O | Subject-Predicate-Object fact |
| 1 | 0x01 | QUALITY | S, Q | Subject has quality Q |
| 2 | 0x02 | QUANTITY | S, value, unit | Numeric measurement |
| 3 | 0x03 | SEQUENCE | N, items… | Ordered list of N concepts |
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
| 15 | 0x0F | ENUM_VAL | S, N, values… | One of a set |
| 16 | 0x10 | CERTAINTY | level_u16 | Confidence 0–10000 |
| 17 | 0x11 | DIFFICULTY | level_u8 | Difficulty 0–4 |
| 18 | 0x12 | CID_REF | 32 bytes | BLAKE3 content reference |
| 19 | 0x13 | STEP | ord, action, target | Procedure step |
| 20 | 0x14 | PRECOND | concept | Step precondition |
| 21 | 0x15 | EFFECT | concept | Step effect/result |
| 22 | 0x16 | AFFECT | V, A, D (i16×3) | VAD emotion model |
| 23 | 0x17 | LABEL | key, value | Generic key-value metadata |
| 24 | 0x18 | TEXT_REF | lang, len, bytes | Compressed canonical text |
| 25 | 0x19 | FORMULA | format, len, bytes | LaTeX/MathML notation |
| 26 | 0x1A | WITNESS | count, proximity | Testimony data |
| 27 | 0x1B | MEDIA_REF | system, len, id_bytes | External media reference |
| 28 | 0x1C | COMPOSITE_HDR | type, completeness, ver | Composite header |
| 29 | 0x1D | MEMBER | order, role, required, label, cid | Composite member |
| 30 | 0x1E | END | — | Terminates instruction stream |
| 31 | 0x1F | EXTENDED | ext_byte, … | Future extension slot |

### §5.1 Operand Encoding

**ConceptId**: Varint-encoded `u64` (see §7).

**NumericValue**: Type prefix byte + big-endian value:

| Prefix | Type | Total Size |
|--------|------|------------|
| 0xF9 | f64 | 1 + 8 = 9 bytes |
| 0xFA | u8 | 1 + 1 = 2 bytes |
| 0xFB | u16 | 1 + 2 = 3 bytes |
| 0xFC | i16 | 1 + 2 = 3 bytes |
| 0xFD | u32 | 1 + 4 = 5 bytes |
| 0xFE | i32 | 1 + 4 = 5 bytes |
| 0xFF | f32 | 1 + 4 = 5 bytes |

Context disambiguation: When reading an operand that could be either a ConceptId (varint) or a NumericValue, bytes `≥ 0xF9` are parsed as NumericValue prefixes; bytes `< 0xF9` are parsed as varint ConceptIds.

### §5.2 ConstraintOp Encoding

| Value | Operator |
|-------|----------|
| 0 | == |
| 1 | != |
| 2 | < |
| 3 | <= |
| 4 | > |
| 5 | >= |

---

## §6 CCID — Content-Addressed Concept Identity

CCID is a 16-byte (128-bit) truncated BLAKE3 hash used to globally identify concepts across nodes.

```rust
pub type Ccid = [u8; 16];

pub fn ccid(canonical: &[u8]) -> Ccid {
    let hash = blake3::hash(canonical);
    hash.as_bytes()[0..16]
}
```

### §6.1 Canonical Form Priority

When generating a CCID, use the highest-priority canonical form available:

| Priority | Prefix | Example | Source |
|----------|--------|---------|--------|
| 1 | `wd:` | `wd:Q283` (water) | Wikidata QID |
| 2 | `gn:` | `gn:2643743` (London) | GeoNames ID |
| 3 | `ncbi:` | `ncbi:9606` (Homo sapiens) | NCBI Taxonomy |
| 4 | `chebi:` | `chebi:15377` (water) | ChEBI ID |
| 5 | `cas:` | `cas:7732-18-5` (water) | CAS Registry Number |
| 6 | `ob:` | `ob:chemistry/water` | OneBrain namespace |
| 7 | (raw bytes) | BLAKE3 of definition KU | Fallback for novel concepts |

### §6.2 Collision Resistance

128-bit → birthday bound ≈ 2⁶⁴ ≈ 1.8 × 10¹⁹. With 50 billion concepts (2526 projection), collision probability ≈ 3.67 × 10⁻¹⁸.

---

## §7 Varint Encoding

ConceptIds use variable-length integer encoding with prefix-based tier detection:

| Tier | Bytes | Prefix | Range | Capacity | Purpose |
|------|-------|--------|-------|----------|---------|
| 0 | 1 | `0xxxxxxx` | 0 – 127 | 128 | Universal primitives |
| 1 | 2 | `10xxxxxx` | 128 – 16,511 | 16,384 | Common concepts |
| 2 | 3 | `110xxxxx` | 16,512 – 2,113,663 | 2,097,152 | Domain-specific |
| 3 | 4 | `1110xxxx` | 2,113,664 – 270,549,119 | 268,435,456 | Extended concepts |
| 4 | 5 | `11110xxx` | 270,549,120 – 34,628,173,487 | ~34.6B | Community concepts |
| 5 | 6 | `111110xx` | — | — | RESERVED |
| 6 | 7 | `1111110x` | — | — | RESERVED |
| 7 | 8 | `11111110` | — | — | RESERVED |
| — | 1 | `11111111` | — | — | SENTINEL (0xFF, reserved forever) |

### §7.1 Encoding Algorithm

```
if value ≤ 127:             → [value]
if value ≤ 16,511:          → [0x80 | hi6, lo8]         where (hi6, lo8) = value - 128
if value ≤ 2,113,663:       → [0xC0 | hi5, mid8, lo8]   where ... = value - 16,512
if value ≤ 270,549,119:     → [0xE0 | hi4, ...]         where ... = value - 2,113,664
if value ≤ 34,628,173,487:  → [0xF0 | hi3, ...]         where ... = value - 270,549,120
else:                       → Error (exceeds 5-tier max)
```

---

## §8 Tier 0 — Universal Concept Constants

74 hardcoded concept IDs (0–79), 47 reserved slots (80–126), 1 sentinel (127). These are fixed, universal, and never change — analogous to the genetic code.

### Structural Predicates (0–15)

| ID | Constant | Semantics |
|----|----------|-----------|
| 0 | SELF_REF | Self-reference / identity |
| 1 | IS_A | Taxonomy: X is a Y |
| 2 | HAS_PART | Meronymy: X has part Y |
| 3 | RELATED_TO | Generic relation (fallback) |
| 4 | INSTANCE_OF | X is instance of class Y |
| 5 | SUBCLASS_OF | X is subclass of Y |
| 6 | OPPOSITE_OF | Antonymy |
| 7 | SIMILAR_TO | Analogy / synonymy |
| 8 | DERIVES_FROM | Origin / etymology |
| 9 | IMPLIES | Logical implication |
| 10 | EQUIVALENT | Equivalence / identity |
| 11 | DISTINCT | Distinctness |
| 12 | PROPERTY_OF | X is property of Y |
| 13 | VALUE_OF | X is value of property Y |
| 14 | MADE_OF | Material composition |
| 15 | USED_FOR | Purpose / function |

### Causal & Temporal (16–27)

| ID | Constant | Semantics |
|----|----------|-----------|
| 16 | CAUSES | X causes Y |
| 17 | PREVENTS | X prevents Y |
| 18 | ENABLES | X enables Y |
| 19 | PRECEDES | X before Y |
| 20 | FOLLOWS | X after Y |
| 21 | DURING | X during Y |
| 22 | BEGINS | Start point |
| 23 | ENDS | End point |
| 24 | SIMULTANEOUS | Co-occurrence |
| 25 | CORRELATES | Correlation (not causation) |
| 26 | REQUIRES | Prerequisite |
| 27 | PRODUCES | Production / output |

### Spatial (28–35)

| ID | Constant | Semantics |
|----|----------|-----------|
| 28 | AT | Location: X at Y |
| 29 | CONTAINS | X contains Y |
| 30 | ABOVE | Spatial above |
| 31 | BELOW | Spatial below |
| 32 | NEAR | Spatial proximity |
| 33 | INSIDE | Spatial inside |
| 34 | BETWEEN | Spatial between |
| 35 | ADJACENT | Spatial adjacency |

### Logical & Modal (36–43)

| ID | Constant | Semantics |
|----|----------|-----------|
| 36 | NOT | Negation |
| 37 | AND | Conjunction |
| 38 | OR | Disjunction |
| 39 | IF_THEN | Conditional |
| 40 | POSSIBLE | Possibility |
| 41 | NECESSARY | Necessity |
| 42 | EXISTS | Existential quantifier |
| 43 | FOR_ALL | Universal quantifier |

### SI Base Units (44–50)

| ID | Constant | Unit |
|----|----------|------|
| 44 | UNIT_METER | Length (m) |
| 45 | UNIT_KILOGRAM | Mass (kg) |
| 46 | UNIT_SECOND | Time (s) |
| 47 | UNIT_AMPERE | Electric current (A) |
| 48 | UNIT_KELVIN | Temperature (K) |
| 49 | UNIT_MOLE | Amount of substance (mol) |
| 50 | UNIT_CANDELA | Luminous intensity (cd) |

### Common Derived Units (51–63)

| ID | Constant | Unit |
|----|----------|------|
| 51 | UNIT_HERTZ | Frequency (Hz) |
| 52 | UNIT_NEWTON | Force (N) |
| 53 | UNIT_PASCAL | Pressure (Pa) |
| 54 | UNIT_JOULE | Energy (J) |
| 55 | UNIT_WATT | Power (W) |
| 56 | UNIT_VOLT | Voltage (V) |
| 57 | UNIT_DEGREE | Angle (°) |
| 58 | UNIT_RADIAN | Angle (rad) |
| 59 | UNIT_PERCENT | Percentage (%) |
| 60 | UNIT_BYTE | Digital storage (byte) |
| 61 | UNIT_BIT | Digital information (bit) |
| 62 | UNIT_LITER | Volume (L) |
| 63 | UNIT_DIMENSIONLESS | Dimensionless |

### Epistemological (64–69)

| ID | Constant | Semantics |
|----|----------|-----------|
| 64 | TRUE_VAL | Truth |
| 65 | FALSE_VAL | Falsehood |
| 66 | UNKNOWN_VAL | Unknown |
| 67 | APPROXIMATE | Approximate value |
| 68 | EXACT | Exact value |
| 69 | MEASURED | Measured value |

### Agentive / Thematic Roles (70–79)

| ID | Constant | Semantics |
|----|----------|-----------|
| 70 | AGENT | Who does (actor) |
| 71 | PATIENT | Who receives (affected) |
| 72 | INSTRUMENT | With what (tool) |
| 73 | BENEFICIARY | For whom |
| 74 | SOURCE | From where |
| 75 | GOAL | To where/what |
| 76 | PURPOSE | Why |
| 77 | METHOD | How |
| 78 | RESULT | Outcome |
| 79 | CONDITION | Under what condition |

### Reserved & Sentinel

| Range | Purpose |
|-------|---------|
| 80–126 | Reserved for future universal concepts |
| 127 | UNKNOWN_CONCEPT — sentinel / fallback |

---

## §9 Concept Registry (.obr)

Offline concept lookup file shipped with every node. Binary format loaded at startup for O(1) name → CCID resolution.

### §9.1 Specifications

| Property | Value |
|----------|-------|
| File format | `.obr` (OneBrain Registry) |
| Initial size | ~200 MB |
| Capacity | ~8 million concepts |
| Coverage target | 99.9% of general-domain knowledge |
| Lookup | O(1) hash table (String → CCID) |
| Update cycle | Quarterly |
| Output path | `onebrain_data/concepts.obr` (same directory as `.redb` files) |

### §9.2 Concept Sources

4 primary data sources, following CCID canonical form priority (§6.1):

| # | Source | Coverage | Canonical Form | Est. entries | Fetch method |
|---|--------|----------|----------------|-------------|--------------|
| 1 | **Wikidata** | Entities, properties, general concepts | `wd:Q{id}` | ~5M | SPARQL endpoint (batched 10K/query) |
| 2 | **GeoNames** | Geographic features (cities, countries, regions) | `gn:{id}` | ~1.5M | Dump file `allCountries.zip` (~400MB) |
| 3 | **NCBI Taxonomy** | Species, organisms | `ncbi:{taxid}` | ~1M | FTP dump `taxdump.tar.gz` (~70MB) |
| 4 | **ChEBI** | Chemical compounds | `chebi:{id}` | ~500K | TSV/SDF dump from `ftp.ebi.ac.uk` |

**Deduplication rule**: If a concept exists in multiple sources (e.g., "water" = wd:Q283 + chebi:15377), only the entry with the highest canonical form priority is kept. Cross-referencing is done via Wikidata properties (P683→ChEBI, P846→NCBI, P1566→GeoNames). Labels from all sources are merged into the winning entry.

### §9.3 Per-Source Fetch Strategy

#### §9.3.1 Wikidata (~5M concepts)

- **API**: `query.wikidata.org/sparql`
- **Rate limit**: 1 request / 2 seconds (Wikidata policy)
- **Fetch strategy**: Batched by P31 (instance_of) category:
  - Entities (Q35120): ~2M
  - Places (Q515 city, Q6256 country, Q82794 region): ~1M
  - Persons (Q5 human): ~1.5M
  - Properties (P-items): ~12K
  - Top sitelinks (Wikipedia popularity): backfill
- **Labels**: en, vi, fr, de, es, ja, zh, ko (8 languages)
- **Fields**: QID, labels, descriptions, P31 category
- **Output**: `raw/wikidata.jsonl`
- **Estimated time**: 4–8 hours (rate-limited)
- **Quarterly delta**: Filter `schema:dateModified > last_fetch_date`

#### §9.3.2 GeoNames (~1.5M places)

- **Source**: Dump file `download.geonames.org/export/dump/allCountries.zip` (~400MB)
- **No API account needed** (dump is publicly available)
- **Fields**: GeoNames ID, name, alternateNames (multilingual), feature class/code, coordinates, population, country code
- **Filter**: population > 0 OR feature class in {A, P, T, H, L} (admin, populated, terrain, water, parks)
- **Output**: `raw/geonames.jsonl`
- **Estimated time**: ~30 minutes (download + parse)
- **Quarterly delta**: Download `modifications-{date}.txt` daily diff files

#### §9.3.3 NCBI Taxonomy (~1M species)

- **Source**: FTP dump `ftp.ncbi.nih.gov/pub/taxonomy/taxdump.tar.gz` (~70MB)
- **Parse files**: `names.dmp` (names + synonyms) + `nodes.dmp` (rank, division)
- **Filter**: Keep species + genus ranks. Skip strains/subspecies unless widely known.
- **Fields**: Taxon ID, scientific name, common names (multilingual), rank, division
- **Output**: `raw/ncbi_taxonomy.jsonl`
- **Estimated time**: ~15 minutes
- **Quarterly delta**: Compare new taxdump vs cached version (diff by taxid set)

#### §9.3.4 ChEBI (~500K compounds)

- **Source**: TSV/SDF dump from `ftp.ebi.ac.uk/pub/databases/chebi/`
- **Backup**: REST API `www.ebi.ac.uk/webservices/chebi/2.0/`
- **Fields**: ChEBI ID, name, synonyms, InChI, SMILES, CAS number
- **Cross-ref**: Map CAS numbers (canonical form priority 5) into ChEBI entries
- **Output**: `raw/chebi.jsonl`
- **Estimated time**: ~20 minutes
- **Quarterly delta**: REST query for new/modified entries since last fetch

### §9.4 `.obr` Binary Format

```
Header (32 bytes):
  magic:       [u8; 4]  = "OBR1"
  version:     u32      = 1
  entry_count: u64
  label_count: u64
  reserved:    [u8; 8]  = 0

Entry section (variable length, sequential):
  For each concept:
    ccid:               [u8; 16]   # 128-bit CCID (BLAKE3 truncated)
    ext_id:             u32        # Wikidata QID / GeoNames ID / NCBI taxid / ChEBI ID
    source:             u8         # 0=wd, 1=gn, 2=ncbi, 3=chebi
    category:           u8         # ConceptCategory enum (Entity=0..Other=255)
    canonical_name_len: u16
    canonical_name:     [u8; canonical_name_len]
    label_count:        u16
    For each label:
      label_len:        u16
      label:            [u8; label_len]
```

### §9.5 Resolution Algorithm

```
1. Exact match:      "water"    → Found(CCID)
2. Case-insensitive: "Water"    → Found(CCID)
3. Fuzzy match:      "ngua van" → Fuzzy("ngựa vằn", CCID)   # Vietnamese diacritics stripped
4. Ambiguous:        "Mercury"  → Ambiguous([planet, element, god])
5. Not found:                   → AI fallback (generate CCID from context)
```

### §9.6 Novel Concept Protocol

When a node creates a genuinely novel concept:

1. AI generates a `Definition` gene (GeneType = 12)
2. CCID = `blake3(encoded_definition_ku)[0..16]`
3. The definition KU propagates via gossip protocol
4. Quarterly update absorbs community-validated novel concepts into the global registry

### §9.7 Data Pipeline Scripts

Scripts located at `scripts/concept_registry/`. Python 3.10+.

#### §9.7.1 Initial Fetch (`initial_fetch.py`)

Run once during system bootstrap. Orchestrates 4 source fetchers sequentially:

```
1. Wikidata  → raw/wikidata.jsonl         (~4-8h, SPARQL rate-limited)
2. GeoNames  → raw/geonames.jsonl         (~30min, dump download)
3. NCBI      → raw/ncbi_taxonomy.jsonl    (~15min, dump download)
4. ChEBI     → raw/chebi.jsonl            (~20min, dump download)
5. Dedup     → merged/concepts_deduped.jsonl  (~10min)
6. Build     → onebrain_data/concepts.obr     (~5min)
```

Features:
- Checkpoint/resume (each source saves progress independently)
- `--sources` flag: select specific sources (e.g., `--sources wd,gn`)
- `--quick` flag: fetch only 100K Wikidata concepts (dev/test, ~10 minutes)

#### §9.7.2 Quarterly Update (`quarterly_update.py`)

Incremental delta update, run quarterly:

```
1. Load checkpoint (last_fetch_date per source)
2. Fetch deltas from 4 sources
3. Absorb community-validated novel concepts from gossip log
4. Merge deltas into existing concepts.obr
5. Output: new concepts.obr + changelog.json
```

#### §9.7.3 Dependencies

```
requests          # HTTP client (Wikidata SPARQL, ChEBI REST)
blake3            # CCID computation (match Rust impl)
tqdm              # Progress bars
```

---

## §10 Epigenetics Layer

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

### §10.1 TrustSection — PoMV 6 Signals

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

### §10.2 Bond

```rust
pub struct Bond {
    pub target_cid: [u8; 32],
    pub bond_type: BondType,
    pub strength: f32,
    pub created_at: u64,
}
```

---

## §11 Expression Layer

```rust
pub struct Expression {
    pub text: String,
    pub lang: String,
    pub concept_names: Vec<(ConceptId, String)>,
}
```

**Lazy rendering**: Expression is computed on-demand via `KuRuntime::expression(lang, dict)`. Cached until language changes.

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

## §12 KuRuntime — Primary Type

```rust
pub struct KuRuntime {
    pub dna: CoreDna,
    pub epi: Epigenetics,
    pub expr: Option<Expression>,
    pub cid: [u8; 32],
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

## §13 ConceptDict — Bilingual Concept Dictionary

### §13.1 Full ConceptDict

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
    pub tier: u8,
    pub category: u8,
}
```

### §13.2 PersistentConceptDict

Feature-gated (`#[cfg(feature = "persist")]`), backed by `redb` ACID database. Three tables: `concepts` (name→JSON), `ids` (u64→name), `meta` (next_id).

---

## §14 Size Comparison

Encoding "Water boils at 100°C at sea level" (a simple fact):

| Format | Size |
|--------|------|
| UTF-8 text | 37 bytes |
| JSON-LD | ~350 bytes |
| KU CoreDna | **~14 bytes** |

Breakdown:
```
MAGIC(1) + VER_META(1) + TRIPLE(1+1+1+1) + QUANTITY(1+1+5+1) + CERTAINTY(1+2) + END(1) + CRC(2) = ~20 bytes
```

---

## §15 Implementation Reference

| File | Purpose |
|------|---------|
| `core_dna.rs` | Wire format encode/decode, Instruction enum, CoreDna struct |
| `types.rs` | ConceptId type, GeneType enum |
| `varint.rs` | Variable-length integer encoding |
| `tier0_concepts.rs` | 74 Tier 0 universal concept constants |
| `ccid.rs` | Content-Addressed Concept Identity (128-bit BLAKE3) |
| `concept_registry.rs` | Offline concept name → CCID lookup |
| `epigenetics.rs` | Layer 2: Runtime trust/bond metadata |
| `ku_runtime.rs` | Layer 1+2+3 unified composite |
| `text_parser.rs` | Natural language → CoreDna parser |
| `concept_dict.rs` | Bilingual concept dictionary |
