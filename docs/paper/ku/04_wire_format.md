# 4. Binary Encoding Specification

The Knowledge Unit's wire format constitutes the physical representation through which semantic knowledge traverses network boundaries, persists on storage media, and achieves content-addressable identity. This chapter presents the Core DNA binary encoding specification—a custom instruction-stream format that achieves binary representations consistently *smaller* than the equivalent natural-language text. We detail the complete wire layout, the 32-opcode instruction set, 13 gene types with their wire encoding, 7 inline numeric value types, the 5-tier variable-length integer encoding aligned with Zipfian frequency classes, and the CRC-16/CCITT integrity mechanism.

---

## 4.1 Wire Format Overview

The Core DNA wire format is a compact, self-delimiting binary structure with a fixed overhead of 5 bytes:

```
┌────────┬──────────┬──────────────────┬────────────────────────────┬──────────┬──────────┐
│ MAGIC  │ VER_META │  CONCEPT TABLE   │     INSTRUCTION STREAM     │   END    │  CRC-16  │
│  (1B)  │  (1B)    │  (variable, opt) │     (variable length)      │  (1B)    │  (2B)    │
├────────┼──────────┼──────────────────┼────────────────────────────┼──────────┼──────────┤
│  0x4B  │ see §4.2 │  if flag set     │  op(1B) + operands...  ×N  │  0x1E    │ BE u16   │
└────────┴──────────┴──────────────────┴────────────────────────────┴──────────┴──────────┘
 ◄──────────────────────────── CRC covers this range ──────────────────────────►
```

> **Definition 4.1 (Minimum wire size).** An empty Core DNA with zero data instructions and no concept table encodes to exactly 5 bytes: MAGIC(1) + VER\_META(1) + END(1) + CRC-16(2). This represents the irreducible fixed overhead of the format.

### 4.1.1 Component Summary

| Component | Offset | Size | Value | Description |
|---|---|---|---|---|
| MAGIC | 0 | 1 byte | `0x4B` | ASCII `'K'` — rapid format identification |
| VER\_META | 1 | 1 byte | (computed) | Version, gene type, concept table flag |
| Concept Table | 2 | variable | (optional) | Local-to-global concept identity mappings |
| Instructions | variable | variable | (opcodes) | Sequence of typed opcode instructions |
| END | variable | 1 byte | `0x1E` | Instruction stream terminator |
| CRC-16 | final 2 | 2 bytes | (computed) | CRC-16/CCITT integrity checksum, big-endian |

### 4.1.2 Hex Diagram — Complete Encoding Example

Encoding a Fact gene with `TRIPLE(1, 2, 3)` and `CERTAINTY(9500)`:

```
Offset  Hex     Description
──────  ──────  ──────────────────────────────────────────────
 00     4B      MAGIC byte ('K')
 01     40      VER_META: version=2(010), gene_type=0(Fact), has_concept_table=0
                  = (0b010_0000_0) = 0x40
 02     00      OPCODE: Triple (op=0x00, mod=0) → byte = 0x00
 03     01      Varint: s=1 (Tier 0, 1 byte)
 04     02      Varint: p=2 (Tier 0, 1 byte)
 05     03      Varint: o=3 (Tier 0, 1 byte)
 06     80      OPCODE: Certainty (op=0x10, mod=0) → byte = 0x10<<3 = 0x80
 07     25      CERTAINTY u16 BE high byte: 9500 = 0x251C
 08     1C      CERTAINTY u16 BE low byte
 09     1E      END marker (Op::End = 0x1E → opcode byte = 0x1E)
 0A-0B  XX XX   CRC-16/CCITT over bytes [00..09]
```

Total: **12 bytes** for a complete, CRC-protected fact with confidence level.

---

## 4.2 VER_META Byte Layout

The second byte of the wire format packs three fields into a single octet using bit-level multiplexing:

```
Bit:  7   6   5   4   3   2   1   0
      ├───────────┤├──────────────┤├┤
       version       gene_type     has_concept_table
       (3 bits)      (4 bits)      (1 bit)
```

### 4.2.1 Field Definitions

| Field | Bits | Width | Range | Description |
|---|---|---|---|---|
| `version` | 7–5 | 3 bits | 0–7 | Format version; current = **2** (binary `010`) |
| `gene_type` | 4–1 | 4 bits | 0–15 | Gene type index (0–6 direct; 7 = extended) |
| `has_concept_table` | 0 | 1 bit | 0–1 | `1` if a Concept Table follows this byte |

### 4.2.2 Encoding and Decoding

$$\text{VER\_META} = (\text{version} \wedge \texttt{0x07}) \ll 5 \;\big|\; (\text{gene\_type} \wedge \texttt{0x0F}) \ll 1 \;\big|\; \text{has\_concept\_table}$$

$$\text{version} = (\text{VER\_META} \gg 5) \wedge \texttt{0x07}$$

$$\text{gene\_type} = (\text{VER\_META} \gg 1) \wedge \texttt{0x0F}$$

$$\text{has\_concept\_table} = \text{VER\_META} \wedge \texttt{0x01}$$

### 4.2.3 Worked Examples

| Gene Type | Version | Has Table | VER\_META Hex | Binary |
|---|---|---|---|---|
| Fact (0) | 2 | No | `0x40` | `010_0000_0` |
| Fact (0) | 2 | Yes | `0x41` | `010_0000_1` |
| Procedure (1) | 2 | No | `0x42` | `010_0001_0` |
| Testimony (5) | 2 | No | `0x4A` | `010_0101_0` |
| Formal (6) | 2 | No | `0x4C` | `010_0110_0` |
| Hypothesis (7→ext) | 2 | No | `0x4E` | `010_0111_0` |

---

## 4.3 Concept Table Encoding

When `has_concept_table = 1`, the Concept Table appears immediately after the VER_META byte, before the instruction stream:

```
COUNT(varint) | ENTRY[0] | ENTRY[1] | ... | ENTRY[COUNT-1]
```

Each entry:

```
LOCAL_ID(varint) | CCID(16 bytes raw)
```

| Field | Encoding | Size | Description |
|---|---|---|---|
| COUNT | varint | 1–5 bytes | Number of entries |
| LOCAL\_ID | varint | 1–5 bytes | Local ConceptId used in instructions |
| CCID | raw bytes | 16 bytes | 128-bit truncated BLAKE3 hash |

Only **Tier 2+ concepts** (ConceptId ≥ 16,512) require entries. Tier 0 (0–127) and Tier 1 (128–16,511) concepts are universally known and do not appear in the Concept Table. This threshold ensures minimal overhead: a typical KU referencing only common concepts has an empty Concept Table (flag = 0, zero bytes).

**Example.** A KU referencing two domain concepts (IDs 20000 and 50000):

```
Offset  Hex                              Description
──────  ──────                           ──────────────────────────────
 02     02                               COUNT = 2 (varint, 1 byte)
 03     C0 0D D0                         LOCAL_ID = 20000 (varint Tier 2, 3 bytes)
 06     [16 bytes CCID]                  CCID for concept 20000
 16     C0 82 30                         LOCAL_ID = 50000 (varint Tier 2, 3 bytes)
 19     [16 bytes CCID]                  CCID for concept 50000
```

Total Concept Table size: 1 + (3+16) + (3+16) = **39 bytes**.

---

## 4.4 Instruction Set

The Core DNA instruction set comprises **32 opcodes** (values `0x00`–`0x1F`): 30 data instructions, the `END` terminator (`0x1E`), and the `EXTENDED` escape (`0x1F`).

### 4.4.1 Opcode Byte Format

Each opcode is a 5-bit value. The opcode byte encodes two fields:

$$\text{opcode\_byte} = (\text{op} \ll 3) \mid (\text{modifier} \wedge \texttt{0x07})$$

where **op** (bits[7:3]) is the 5-bit opcode value and **modifier** (bits[2:0]) is a 3-bit field reserved for future use (currently `0`).

### 4.4.2 Complete Opcode Table

**Table 4.1.** All 32 Core DNA opcodes with wire byte mappings and operand layouts.

| Op | Hex | Wire Byte | Name | Operands | Description |
|---|---|---|---|---|---|
| 0x00 | 0x00 | `0x00` | `TRIPLE` | varint(S), varint(P), varint(O) | Subject-Predicate-Object fact |
| 0x01 | 0x01 | `0x08` | `QUALITY` | varint(S), varint(Q) | Subject has quality Q |
| 0x02 | 0x02 | `0x10` | `QUANTITY` | varint(S), numeric(value), varint(unit) | Numeric measurement |
| 0x03 | 0x03 | `0x18` | `SEQUENCE` | u8(N), varint(item₁), …, varint(itemₙ) | Ordered list of N concepts |
| 0x04 | 0x04 | `0x20` | `PART_OF` | varint(part), varint(whole) | Hierarchical containment |
| 0x05 | 0x05 | `0x28` | `LOCATED` | varint(S), varint(location) | Spatial relation |
| 0x06 | 0x06 | `0x30` | `TEMPORAL` | varint(S), varint(time) | Time relation |
| 0x07 | 0x07 | `0x38` | `CAUSAL` | varint(cause), varint(effect) | Causation link |
| 0x08 | 0x08 | `0x40` | `SIMULATES` | varint(S), varint(model) | Analogy/simulation |
| 0x09 | 0x09 | `0x48` | `CONDITION` | varint(if), varint(then) | Conditional logic |
| 0x0A | 0x0A | `0x50` | `AGENT` | varint(actor), varint(action) | Who performs action |
| 0x0B | 0x0B | `0x58` | `TOOL` | varint(action), varint(instrument) | Action uses tool |
| 0x0C | 0x0C | `0x60` | `RANGE` | varint(S), numeric(min), numeric(max) | Value range |
| 0x0D | 0x0D | `0x68` | `TOLERANCE` | varint(S), numeric(value), numeric(±δ) | Value with error margin |
| 0x0E | 0x0E | `0x70` | `CONSTRAINT` | varint(source), u8(op\_code), varint(target) | Numeric constraint |
| 0x0F | 0x0F | `0x78` | `ENUM_VAL` | varint(S), u8(N), varint(val₁), …, varint(valₙ) | One of a set |
| 0x10 | 0x10 | `0x80` | `CERTAINTY` | u16\_be(level) | Confidence 0–10,000 |
| 0x11 | 0x11 | `0x88` | `DIFFICULTY` | u8(level) | Difficulty 0–4 |
| 0x12 | 0x12 | `0x90` | `CID_REF` | raw(32 bytes BLAKE3 hash) | Content ID reference |
| 0x13 | 0x13 | `0x98` | `STEP` | u8(ord), varint(action), varint(target) | Procedure step |
| 0x14 | 0x14 | `0xA0` | `PRECOND` | varint(concept) | Step precondition |
| 0x15 | 0x15 | `0xA8` | `EFFECT` | varint(concept) | Step effect/result |
| 0x16 | 0x16 | `0xB0` | `AFFECT` | i16\_be(V), i16\_be(A), i16\_be(D) | VAD emotion model |
| 0x17 | 0x17 | `0xB8` | `LABEL` | varint(key), varint(value) | Generic key-value metadata |
| 0x18 | 0x18 | `0xC0` | `TEXT_REF` | u8(lang), u16\_be(len), raw(bytes) | Compressed canonical text |
| 0x19 | 0x19 | `0xC8` | `FORMULA` | u8(format), u16\_be(len), raw(bytes) | LaTeX/MathML notation |
| 0x1A | 0x1A | `0xD0` | `WITNESS` | u16\_be(count), u8(proximity) | Testimony data |
| 0x1B | 0x1B | `0xD8` | `MEDIA_REF` | u8(system), u8(len), raw(id\_bytes) | External media reference |
| 0x1C | 0x1C | `0xE0` | `COMPOSITE_HDR` | u8(type), u8(completeness), u32\_be(ver) | Composite header |
| 0x1D | 0x1D | `0xE8` | `MEMBER` | u16\_be(order), u8(role), u8(required), varint(label), raw(32B cid) | Composite member |
| 0x1E | 0x1E | `0xF0` | `END` | *(none)* | Terminates instruction stream |
| 0x1F | 0x1F | `0xF8` | `EXTENDED` | u8(ext\_byte), … | Future extension slot |

### 4.4.3 Instruction Categories

The 30 data instructions are organized into five functional categories:

**Relational instructions** (opcodes 0x00–0x0B) encode semantic relationships between concepts. The foundational `TRIPLE` instruction represents subject-predicate-object facts; `QUALITY`, `QUANTITY`, `PART_OF`, `LOCATED`, `TEMPORAL`, `CAUSAL`, `SIMULATES`, `CONDITION`, `AGENT`, and `TOOL` provide specialised relationship types that would otherwise require verbose triple chains. `SEQUENCE` encodes ordered lists of up to 255 concepts.

**Quantitative instructions** (opcodes 0x0C–0x0F, 0x02) encode numeric measurements and constraints. `QUANTITY` attaches a typed numeric value and unit to a concept; `RANGE` and `TOLERANCE` express measurement bounds and precision margins; `CONSTRAINT` applies relational operators ($=$, $\neq$, $<$, $\leq$, $>$, $\geq$); `ENUM_VAL` enumerates valid value sets.

**Metadata instructions** (opcodes 0x10–0x12, 0x17) encode KU-level metadata. `CERTAINTY` expresses confidence as a scaled integer 0–10,000; `DIFFICULTY` rates complexity on a 5-level ordinal scale; `CID_REF` embeds a 32-byte BLAKE3 hash reference to another KU; `LABEL` provides a generic key-value mechanism.

**Procedural instructions** (opcodes 0x13–0x15) encode step-by-step processes. `STEP` carries an ordinal, action concept, and target concept; `PRECOND` and `EFFECT` attach prerequisite and outcome concepts to the preceding step.

**Gene-specific instructions** (opcodes 0x16–0x1D) encode data structures unique to particular gene types. `AFFECT` stores VAD emotional dimensions as signed 16-bit integers; `TEXT_REF` and `FORMULA` embed compressed text blobs; `WITNESS` carries testimony metadata; `MEDIA_REF` references external media; `COMPOSITE_HDR` and `MEMBER` define composite KU structures.

---

## 4.5 Gene Type Encoding

Gene types are encoded in the `VER_META` byte's bits[4:1] (4 bits, range 0–15). Types 0–6 are encoded directly with their numeric value. Types 7–12 share the base value `7` in bits[4:1] and are disambiguated by an extension byte within the instruction stream.

**Table 4.2.** Gene type wire encoding.

| Value | Name | Wire Encoding | Description |
|---|---|---|---|
| 0 | Fact | `(0, —)` | Verified factual statement |
| 1 | Procedure | `(1, —)` | Step-by-step process |
| 2 | Experience | `(2, —)` | First-person experience with VAD affect |
| 3 | Creative | `(3, —)` | Creative/artistic content |
| 4 | MediaExperience | `(4, —)` | Multi-sensory media experience |
| 5 | Testimony | `(5, —)` | Witnessed account |
| 6 | Formal | `(6, —)` | Formally proven (mathematics, logic) |
| 7 | Hypothesis | `(7, 0x00)` | Testable proposition |
| 8 | Narrative | `(7, 0x01)` | Story/narrative structure |
| 9 | Sensory | `(7, 0x02)` | Sensory description |
| 10 | Composite | `(7, 0x03)` | Multi-gene composite KU |
| 11 | Normative | `(7, 0x04)` | Prescriptive rule (should/ought) |
| 12 | Definition | `(7, 0x05)` | Concept definition |

The Rust implementation encodes this as:

```rust
impl GeneType {
    pub fn wire_encoding(&self) -> (u8, Option<u8>) {
        match self {
            Self::Fact            => (0, None),
            Self::Procedure       => (1, None),
            Self::Experience      => (2, None),
            Self::Creative        => (3, None),
            Self::MediaExperience => (4, None),
            Self::Testimony       => (5, None),
            Self::Formal          => (6, None),
            Self::Hypothesis      => (7, Some(0x00)),
            Self::Narrative       => (7, Some(0x01)),
            Self::Sensory         => (7, Some(0x02)),
            Self::Composite       => (7, Some(0x03)),
            Self::Normative       => (7, Some(0x04)),
            Self::Definition      => (7, Some(0x05)),
        }
    }
}
```

---

## 4.6 NumericValue Encoding

Numeric values within `QUANTITY`, `RANGE`, and `TOLERANCE` instructions are encoded inline using a 1-byte type prefix followed by the value in big-endian byte order.

### 4.6.1 Seven Numeric Types

**Table 4.3.** NumericValue types with wire encoding.

| Prefix | Type | Payload Size | Total Size | Value Range |
|---|---|---|---|---|
| `0xF9` | F64 | 8 bytes | 9 bytes | IEEE 754 double-precision |
| `0xFA` | U8 | 1 byte | 2 bytes | 0–255 |
| `0xFB` | U16 | 2 bytes BE | 3 bytes | 0–65,535 |
| `0xFC` | I16 | 2 bytes BE signed | 3 bytes | −32,768–32,767 |
| `0xFD` | U32 | 4 bytes BE | 5 bytes | 0–4,294,967,295 |
| `0xFE` | I32 | 4 bytes BE signed | 5 bytes | −2,147,483,648–2,147,483,647 |
| `0xFF` | F32 | 4 bytes BE IEEE 754 | 5 bytes | $\pm 3.4 \times 10^{38}$ |

### 4.6.2 Disambiguation Rule

At operand positions where either a `NumericValue` or a varint `ConceptId` may appear, the first byte determines the interpretation:

> **Definition 4.2 (Numeric/varint disambiguation).** Given a byte $b$ at a position where either a NumericValue or a varint ConceptId may appear:
> - If $b \geq \texttt{0xF9}$: decode as NumericValue (prefix + payload).
> - If $b < \texttt{0xF9}$: decode as varint ConceptId.

This rule is well-defined because varint first bytes use the range `0x00`–`0xF8` (tiers 0–4 plus reserved), and all `NumericValue` prefix bytes occupy `0xF9`–`0xFF`. The byte `0xFF` is explicitly the sentinel in the varint scheme and is repurposed here as the F32 prefix. No ambiguity arises because the varint sentinel is never a valid ConceptId encoding.

### 4.6.3 Encoding Examples

```
NumericValue::U8(42)       → [0xFA, 0x2A]                               (2 bytes)
NumericValue::U16(9500)    → [0xFB, 0x25, 0x1C]                         (3 bytes)
NumericValue::I16(-500)    → [0xFC, 0xFE, 0x0C]                         (3 bytes)
NumericValue::U32(100000)  → [0xFD, 0x00, 0x01, 0x86, 0xA0]             (5 bytes)
NumericValue::F32(35.2)    → [0xFF, 0x42, 0x0C, 0xCC, 0xCD]             (5 bytes)
NumericValue::F64(3.14159) → [0xF9, 0x40, 0x09, 0x21, 0xF9, 0xF0, 0x1B, 0x86, 0x6E]  (9 bytes)
```

---

## 4.7 Variable-Length Integer Encoding

### 4.7.1 Motivation

Concept identifiers follow a Zipfian frequency distribution: a small core of universal primitives appears with high frequency, while the vast majority of domain-specific concepts are rare. An optimal encoding should assign shorter representations to frequent concepts, following Shannon's source coding theorem [Shannon, 1948]. We employ a **5-tier variable-length integer encoding** that partitions the concept namespace into semantically aligned frequency classes, enables $O(1)$ length determination from the first byte's prefix pattern, and supports branchless decoding on modern processors.

### 4.7.2 Encoding Structure

The encoding uses a prefix-free code inspired by UTF-8's leading-byte structure:

**Table 4.4.** 5-tier varint encoding specification.

| Tier | Bytes | First Byte Prefix | Data Bits | Encoded Range | Capacity |
|---|---|---|---|---|---|
| 0 | 1 | `0xxxxxxx` | 7 | 0–127 | 128 |
| 1 | 2 | `10xxxxxx xxxxxxxx` | 14 | 128–16,511 | 16,384 |
| 2 | 3 | `110xxxxx xxxxxxxx xxxxxxxx` | 21 | 16,512–2,113,663 | 2,097,152 |
| 3 | 4 | `1110xxxx` + 3 bytes | 28 | 2,113,664–270,549,119 | 268,435,456 |
| 4 | 5 | `11110xxx` + 4 bytes | 35 | 270,549,120–34,628,173,487 | ~34.6 billion |

Higher tiers are reserved for future expansion:

| Tier | Bytes | Prefix | Status |
|---|---|---|---|
| 5 | 6 | `111110xx` | Reserved |
| 6 | 7 | `1111110x` | Reserved |
| 7 | 8 | `11111110` | Reserved |
| — | 1 | `11111111` | Sentinel (`0xFF`, reserved forever) |

### 4.7.3 Tier Boundary Constants

Each tier's range is contiguous and non-overlapping. The offset for tier $k$ equals the cumulative capacity of all lower tiers:

```
TIER0_MAX       =         127
TIER1_OFFSET    =         128       TIER1_MAX    =      16,511
TIER2_OFFSET    =      16,512       TIER2_MAX    =   2,113,663
TIER3_OFFSET    =   2,113,664       TIER3_MAX    = 270,549,119
TIER4_OFFSET    = 270,549,120       TIER4_MAX    ≈ 34,628,173,487
```

### 4.7.4 Semantic Alignment

The tier structure constitutes a deliberate architectural alignment between byte-width and conceptual frequency class:

| Tier | Bytes | Capacity | Semantic Stratum |
|---|---|---|---|
| 0 | 1 | 128 | Universal primitives (actions, objects, spatial relations, units) |
| 1 | 2 | ~16K | Common everyday concepts (emotions, objects, activities) |
| 2 | 3 | ~2M | Domain-specific terminology (medical, legal, engineering) |
| 3 | 4 | ~268M | Extended concepts (chemical compounds, gene variants) |
| 4 | 5 | ~34.6B | Community and rare concepts (neologisms, ephemeral terms) |

### 4.7.5 Encode Algorithm

```
function encode(value: u64) → bytes:
    if value ≤ 127:
        return [value as u8]                           // 0xxxxxxx

    elif value ≤ 16,511:
        v = value - 128
        return [0x80 | (v >> 8) as u8,                 // 10xxxxxx
                (v & 0xFF) as u8]                      // xxxxxxxx

    elif value ≤ 2,113,663:
        v = value - 16,512
        return [0xC0 | (v >> 16) as u8,                // 110xxxxx
                ((v >> 8) & 0xFF) as u8,               // xxxxxxxx
                (v & 0xFF) as u8]                      // xxxxxxxx

    elif value ≤ 270,549,119:
        v = value - 2,113,664
        return [0xE0 | (v >> 24) as u8,                // 1110xxxx
                ((v >> 16) & 0xFF) as u8,              // xxxxxxxx
                ((v >> 8) & 0xFF) as u8,               // xxxxxxxx
                (v & 0xFF) as u8]                      // xxxxxxxx

    else:
        v = value - 270,549,120
        return [0xF0 | (v >> 32) as u8,                // 11110xxx
                ((v >> 24) & 0xFF) as u8,              // xxxxxxxx
                ((v >> 16) & 0xFF) as u8,              // xxxxxxxx
                ((v >> 8) & 0xFF) as u8,               // xxxxxxxx
                (v & 0xFF) as u8]                      // xxxxxxxx
```

### 4.7.6 Decode Algorithm

```
function decode(buf: &[u8]) → (value: u64, bytes_consumed: usize):
    let first = buf[0]

    if first & 0x80 == 0:                              // Tier 0: 0xxxxxxx
        return (first as u64, 1)

    elif first & 0xC0 == 0x80:                         // Tier 1: 10xxxxxx
        v = ((first & 0x3F) as u64) << 8
            | buf[1] as u64
        return (v + 128, 2)

    elif first & 0xE0 == 0xC0:                         // Tier 2: 110xxxxx
        v = ((first & 0x1F) as u64) << 16
            | (buf[1] as u64) << 8
            | buf[2] as u64
        return (v + 16,512, 3)

    elif first & 0xF0 == 0xE0:                         // Tier 3: 1110xxxx
        v = ((first & 0x0F) as u64) << 24
            | (buf[1] as u64) << 16
            | (buf[2] as u64) << 8
            | buf[3] as u64
        return (v + 2,113,664, 4)

    elif first & 0xF8 == 0xF0:                         // Tier 4: 11110xxx
        v = ((first & 0x07) as u64) << 32
            | (buf[1] as u64) << 24
            | (buf[2] as u64) << 16
            | (buf[3] as u64) << 8
            | buf[4] as u64
        return (v + 270,549,120, 5)

    else:
        error("Invalid varint prefix")
```

### 4.7.7 Comparison with LEB128

**Table 4.5.** Structural comparison with LEB128 [DWARF Committee, 1992].

| Property | LEB128 / Protocol Buffers | OneBrain 5-Tier Varint |
|---|---|---|
| Length determination | $O(n)$ sequential scan | $O(1)$ from first byte prefix |
| Self-synchronizing | No | Yes (UTF-8-like prefix) |
| Semantic alignment | None | Tier = frequency class |
| Maximum 1-byte range | 0–127 | 0–127 |
| Maximum 2-byte range | 0–16,383 | 128–16,511 |
| Maximum 5-byte range | 0–4,294,967,295 | 270,549,120–~34.6B |
| Branchless decoding | Not feasible | Feasible via prefix lookup |
| Encoding uniqueness | Not canonical | Canonical (unique per value) |

---

## 4.8 CRC-16/CCITT Integrity Check

### 4.8.1 Algorithm Specification

Core DNA uses CRC-16/CCITT (XMODEM variant) for error detection:

**Table 4.6.** CRC-16/CCITT parameters.

| Parameter | Value |
|---|---|
| Algorithm | CRC-16/CCITT (XMODEM) |
| Polynomial | `0x1021` ($x^{16} + x^{12} + x^5 + 1$) |
| Init value | `0xFFFF` |
| Input reflected | No |
| Output reflected | No |
| Final XOR | `0x0000` |
| Check value | `0x29B1` (for ASCII `"123456789"`) |
| Wire encoding | Big-endian u16 |

### 4.8.2 Computation

The CRC is computed over all bytes preceding the CRC itself (MAGIC + VER\_META + [CONCEPT\_TABLE] + INSTRUCTIONS + END):

```
function crc16_ccitt(data: byte[]) → u16:
    crc ← 0xFFFF
    for byte in data:
        crc ← crc ⊕ (byte ≪ 8)
        repeat 8 times:
            if (crc ∧ 0x8000) ≠ 0:
                crc ← (crc ≪ 1) ⊕ 0x1021
            else:
                crc ← crc ≪ 1
        crc ← crc ∧ 0xFFFF
    return crc
```

### 4.8.3 Error Detection Properties

- **Single-bit errors:** 100% detection for any single-bit flip.
- **Burst errors:** 100% detection for burst errors up to 16 bits in length.
- **Random error patterns:** Undetected error probability of approximately $2^{-16} \approx 1.5 \times 10^{-5}$.

The CRC-16 checksum is explicitly *not* a cryptographic integrity mechanism. Tamper detection is provided separately by the 32-byte BLAKE3 Content Identifier (CID), which serves as a cryptographic commitment to the wire bytes.

### 4.8.4 Verification

Implementations MUST pass the standard check value test:

```
Input:  b"123456789"  (9 bytes: 0x31..0x39)
Output: 0x29B1
```

---

## 4.9 Worked Examples

### 4.9.1 Example 1: Minimal Fact — "Water IS_A liquid"

Encoding a single triple asserting that water (ConceptId 42) is a liquid (ConceptId 187) using the IS_A predicate (ConceptId 1):

```
Offset  Hex     Description
──────  ──────  ──────────────────────────────────────────────
 00     4B      MAGIC ('K')
 01     40      VER_META: version=2, gene_type=0(Fact), table=0
 02     00      OPCODE: TRIPLE (op=0x00, mod=0)
 03     2A      Varint: s=42 (Tier 0, 1 byte)
 04     01      Varint: p=1 (IS_A, Tier 0, 1 byte)
 05     80 3B   Varint: o=187 (Tier 1, 2 bytes: 187-128=59 → 0x80|0x00, 0x3B)
 07     1E      END marker
 08-09  XX XX   CRC-16/CCITT
```

Total: **10 bytes**.

### 4.9.2 Example 2: Quantity — "Water boils at 100°C"

Encoding that water (42) has a boiling point of 100°C, using `TRIPLE` + `QUANTITY`:

```
Offset  Hex           Description
──────  ──────        ──────────────────────────────────────────────
 00     4B            MAGIC
 01     40            VER_META: Fact, version=2, no table
 02     00            TRIPLE opcode
 03     2A            s=42 (water, Tier 0)
 04     01            p=1 (IS_A, Tier 0)
 05     80 3B         o=187 (boiling, Tier 1)
 07     10            QUANTITY opcode (op=0x02 → 0x02<<3=0x10)
 08     2A            s=42 (water, Tier 0)
 09     FA 64         NumericValue::U8(100) → [0xFA, 0x64]
 0B     39            unit=57 (UNIT_DEGREE, Tier 0)
 0C     80 25 1C      CERTAINTY opcode (0x80) + u16_be(9500)
 0F     1E            END
 10-11  XX XX         CRC-16/CCITT
```

Total: **~18 bytes** vs. UTF-8 text "Water boils at 100°C" = 22 bytes.

### 4.9.3 Example 3: Procedure Step

Encoding a procedure step "Step 1: heat the pan" for a Procedure gene:

```
Offset  Hex     Description
──────  ──────  ──────────────────────────────────────────────
 00     4B      MAGIC
 01     42      VER_META: version=2, gene_type=1(Procedure), table=0
 02     98      STEP opcode (op=0x13 → 0x13<<3=0x98)
 03     01      ord=1 (u8)
 04     80 XX   action ConceptId (varint, Tier 1, "heat")
 06     80 YY   target ConceptId (varint, Tier 1, "pan")
 08     1E      END
 09-0A  XX XX   CRC-16/CCITT
```

Total: **~11 bytes**.

---

## 4.10 Size Comparison

### 4.10.1 Benchmark Results

**Table 4.7.** Measured encoding sizes from the reference Rust implementation.

| Test Case | UTF-8 Text | Core DNA | Compression |
|---|---|---|---|
| Breaststroke technique ("Bơi ếch") | 323 bytes | 88 bytes (3 KUs) | **3.7× smaller** |
| Rocket systems ("Tên lửa") | 1,078 bytes | 172 bytes (5 KUs) | **6.3× smaller** |
| Water boiling point | 37 bytes | ~14 bytes (1 KU) | **2.6× smaller** |

### 4.10.2 Format Comparison

**Table 4.8.** Encoding "Water boils at 100°C at sea level" across formats.

| Format | Size | Notes |
|---|---|---|
| UTF-8 text | 37 bytes | Raw natural language |
| JSON-LD | ~350 bytes | Standard linked data serialisation |
| KU Core DNA | **~14 bytes** | Binary instruction stream |

### 4.10.3 Minimal Encoding

The smallest meaningful KU—a single `TRIPLE(s, p, o)` with all three ConceptIds in Tier 0—encodes to:

$$|\text{wire}|_{\min} = \underbrace{1}_{\text{MAGIC}} + \underbrace{1}_{\text{VER\_META}} + \underbrace{1}_{\text{opcode}} + \underbrace{3}_{3 \times \text{varint(Tier 0)}} + \underbrace{1}_{\text{END}} + \underbrace{2}_{\text{CRC-16}} = 9 \text{ bytes}$$

### 4.10.4 Overhead Budget

**Table 4.9.** Fixed wire format overhead.

| Component | Size | Notes |
|---|---|---|
| MAGIC | 1 byte | Fixed: `0x4B` |
| VER\_META | 1 byte | Version + gene type + concept table flag |
| END marker | 1 byte | Fixed: `0x1E` |
| CRC-16 | 2 bytes | Fixed trailer |
| **Total overhead** | **5 bytes** | Constant regardless of instruction count |

For a typical 88-byte KU, the 5-byte overhead represents 5.7% of the total wire size. For the minimal 9-byte KU, the overhead is 55.6%.

### 4.10.5 Bandwidth Implications

At the typical KU size of 20–172 bytes (median ~60 bytes), a 2G cellular connection (50 Kbps) transmits approximately 104 KUs per second; a 4G connection (10 Mbps) achieves approximately 20,800 KUs per second. These throughput figures confirm that Core DNA is viable for real-time knowledge synchronisation on bandwidth-constrained mobile devices.

---

## References

- DWARF Committee (1992). *DWARF Debugging Information Format*, Version 2. UNIX International.
- Shannon, C. E. (1948). A mathematical theory of communication. *Bell System Technical Journal*, 27(3), 379–423.
