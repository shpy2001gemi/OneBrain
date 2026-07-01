# KU Core DNA v6 — Wire Format Specification

> **Version**: 1.0  
> **Status**: Definitive Technical Reference  
> **Date**: 2026-06-29  
> **Source of Truth**: [`core_dna.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/core_dna.rs), [`varint.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/varint.rs), [`types.rs`](file:///c:/Users/shpy2/Documents/OneBrain/src/ku-core/src/types.rs)

---

## Table of Contents

1. [Overview](#1-overview)
2. [3-Layer Architecture](#2-3-layer-architecture)
3. [Wire Format Specification](#3-wire-format-specification)
4. [Op Enum — All 32 Opcodes](#4-op-enum--all-32-opcodes)
5. [Instruction Set — 30 Typed Variants](#5-instruction-set--30-typed-variants)
6. [NumericValue — Inline Numeric Literals](#6-numericvalue--inline-numeric-literals)
7. [Varint Encoding — ConceptId Wire Format](#7-varint-encoding--conceptid-wire-format)
8. [CRC-16/CCITT Algorithm](#8-crc-16ccitt-algorithm)
9. [Auto-detect — v4/v5 CBOR vs v6 Core DNA](#9-auto-detect--v4v5-cbor-vs-v6-core-dna)
10. [Size Comparisons — Measured Results](#10-size-comparisons--measured-results)
11. [Backward Compatibility — decode_any](#11-backward-compatibility--decode_any)
12. [Bridge — KU ↔ CoreDna Conversion](#12-bridge--ku--coredna-conversion)

---

## 1. Overview

### What is Core DNA?

Core DNA is an ultra-compact, language-agnostic binary encoding format for Knowledge Units (KUs) in the OneBrain knowledge system. It is the **v6 wire format**, replacing the v4/v5 CBOR-based encoding with a custom instruction stream that is **smaller than natural language text**.

### Why was it created?

The v4/v5 CBOR format suffered from severe size inflation:

| Problem | Measurement |
|---------|-------------|
| v5 CBOR encoding of "bơi ếch" (breaststroke) | **1,053 bytes** |
| Original Vietnamese text (UTF-8) | **323 bytes** |
| Ratio | **3.3× LARGER than text** |

This meant that the "compact binary" format was actually **larger** than writing the same knowledge in plain text — defeating the purpose of binary encoding entirely.

Core DNA v6 solves this by using a custom instruction stream:

| Metric | v6 Core DNA |
|--------|-------------|
| "bơi ếch" in Core DNA | **88 bytes** |
| Original text | **323 bytes** |
| Ratio | **3.7× SMALLER than text** |

### Design Goals

1. **Smaller than text** — Binary encoding must beat natural language in size
2. **Language-agnostic** — Only ConceptIds, no natural language strings in the core
3. **Machine-queryable** — Structured instructions, not opaque byte blobs
4. **Precise** — IEEE 754 floats, typed numerics, constraint operators
5. **CRC-protected** — Bit-flip detection via CRC-16/CCITT
6. **Backward-compatible** — Auto-detect and decode both CBOR v4/v5 and Core DNA v6

### Biological Analogy

The naming follows a biological metaphor:

| Layer | Biology | Knowledge System |
|-------|---------|-----------------|
| **Core DNA** | Nucleotide sequence | Compact, immutable, stored binary |
| **Epigenetics** | Histone modifications | Runtime-only metadata (CBOR), not persisted in DNA |
| **Expression** | Protein synthesis | Generated on-demand from DNA (text output) |

---

## 2. 3-Layer Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Expression Layer                      │
│          (Text output — generated on-demand)             │
│                                                         │
│   "Breaststroke is a basic swimming style that           │
│    simulates frog movement in water."                    │
│                                                         │
│   Generated from Core DNA + locale rules.               │
│   NOT stored. Recreated each time.                       │
├─────────────────────────────────────────────────────────┤
│                   Epigenetics Layer                       │
│           (Runtime CBOR — not persisted in DNA)          │
│                                                         │
│   • Trust metadata (epistemic status, evidence type)     │
│   • Bond metadata (weight, decay, creator, timestamps)   │
│   • Qualifier details (key-value pairs on codons)        │
│   • Edge state, bidirectional flags, context concepts    │
│                                                         │
│   Serialized as CBOR when needed. Dropped during         │
│   KU→CoreDna conversion. Defaults restored on inflate.   │
├─────────────────────────────────────────────────────────┤
│                    Core DNA Layer                         │
│        (Primary binary — the stored wire format)         │
│                                                         │
│   0x4B 0x21 [INSTRUCTION STREAM] 0xF0 [CRC-16]         │
│                                                         │
│   • Opcodes + varint operands                            │
│   • Gene type, version, qualifier flag in header         │
│   • Instruction set: 30 typed variants, 32 opcodes       │
│   • CRC-16/CCITT integrity check                         │
│                                                         │
│   This is what gets persisted to disk/network.           │
└─────────────────────────────────────────────────────────┘
```

### Layer Responsibilities

| Layer | Stored? | Format | Contains |
|-------|---------|--------|----------|
| Core DNA | ✅ Yes | Custom binary | Concept IDs, triples, quantities, procedures, constraints |
| Epigenetics | ❌ Runtime only | CBOR (v4/v5) | Trust, bonds, qualifiers, edge metadata |
| Expression | ❌ Generated | Natural language | Human-readable text, locale-specific |

---

## 3. Wire Format Specification

### Complete Wire Layout

```
┌────────┬──────────┬──────────────────────────────┬──────────┬──────────┐
│ MAGIC  │ VER_META │      INSTRUCTION STREAM      │   END    │  CRC-16  │
│  (1B)  │  (1B)    │      (variable length)       │  (1B)    │  (2B)    │
├────────┼──────────┼──────────────────────────────┼──────────┼──────────┤
│  0x4B  │ see §3.2 │ op(1B) + operands...  × N    │  0xF0    │ BE u16   │
└────────┴──────────┴──────────────────────────────┴──────────┴──────────┘
 ◄──────────────────── CRC covers this range ───────────────────►
```

> [!IMPORTANT]
> Minimum wire size is **5 bytes**: MAGIC(1) + VER_META(1) + END(1) + CRC(2). An empty Core DNA with zero instructions encodes to exactly 5 bytes.

### 3.1 Magic Byte

```
Offset: 0
Size:   1 byte
Value:  0x4B (ASCII 'K')
```

The magic byte is `0x4B` — a single byte. This distinguishes v6 Core DNA from v4/v5 CBOR, which uses the **two-byte** magic `0x4B 0x44` (ASCII `"KD"`).

### 3.2 VER_META Byte

```
Offset: 1
Size:   1 byte
Layout:
  ┌───────────┬────────────────┬──────────────────┐
  │ Bits 7-5  │   Bits 4-1     │     Bit 0        │
  │ version   │   gene_type    │ has_qualifiers   │
  │ (3 bits)  │   (4 bits)     │   (1 bit)        │
  └───────────┴────────────────┴──────────────────┘
```

**Encoding formula:**

```
VER_META = (version & 0x07) << 5
         | (gene_type & 0x0F) << 1
         | (has_qualifiers as u8)
```

**Decoding formula:**

```
version        = (VER_META >> 5) & 0x07
gene_type      = (VER_META >> 1) & 0x0F
has_qualifiers = (VER_META & 0x01) != 0
```

**Field details:**

| Field | Bits | Range | Description |
|-------|------|-------|-------------|
| `version` | 7-5 | 0-7 | Format version. Current = **1** (binary `001`). |
| `gene_type` | 4-1 | 0-15 | Gene type index. See table below. |
| `has_qualifiers` | 0 | 0-1 | `1` if any instructions contain qualifiers. |

**Gene Type Values (4-bit, stored directly):**

| Value | GeneType | Description |
|-------|----------|-------------|
| 0 | Fact | Factual knowledge with triples |
| 1 | Procedure | Step-by-step process |
| 2 | Experience | Experiential with VAD affect |
| 3 | Creative | Creative content |
| 4 | MediaExperience | Media-based experience (★v4) |
| 5 | Testimony | Witness testimony (★v4) |
| 6 | Formal | Mathematical/logical notation (★v4) |
| 7 | Hypothesis | Hypothesis with confidence (★v4) |
| 8 | Narrative | Narrative structure (★v4) |
| 9 | Sensory | Sensory perception (★v4) |
| 10 | Composite | Composite KU with members (★v5) |
| 11-15 | *Reserved* | Future use |

> [!NOTE]
> Core DNA stores all 11 gene types directly in 4 bits (0-10). This differs from v4/v5 wire encoding, where types 7+ use an EXTENDED mechanism with a separate ext byte. The Core DNA approach is simpler and uses less space.

**Example:** A Fact gene (type 0), version 1, no qualifiers:

```
VER_META = (1 << 5) | (0 << 1) | 0 = 0x20
Full header: 0x4B 0x20
```

**Example:** A Procedure gene (type 1), version 1, no qualifiers:

```
VER_META = (1 << 5) | (1 << 1) | 0 = 0x22
Full header: 0x4B 0x22
```

### 3.3 Instruction Stream

The instruction stream is a sequence of variable-length instructions. Each instruction consists of:

```
┌──────────────────┬────────────────────────────┐
│   OPCODE BYTE    │        OPERANDS            │
│     (1 byte)     │   (variable, 0-N bytes)    │
├──────────────────┼────────────────────────────┤
│  [op:5][mod:3]   │  varint / numeric / raw    │
└──────────────────┴────────────────────────────┘
```

**Opcode byte layout:**

```
  Bit:  7  6  5  4  3  2  1  0
       ├──op(5 bits)──┤├mod(3)┤
```

```
opcode_byte = (op << 3) | (modifier & 0x07)
op_value    = opcode_byte >> 3
modifier    = opcode_byte & 0x07
```

- **op** (bits 7-3): The 5-bit opcode value (0x00-0x1F). See [§4](#4-op-enum--all-32-opcodes).
- **modifier** (bits 2-0): 3-bit modifier field. Currently always 0 in v1. Reserved for future use.

### 3.4 END Marker

```
Offset: After last instruction
Size:   1 byte
Value:  Opcode byte for Op::End (0x1E << 3 = 0xF0)
```

The END marker terminates the instruction stream. Its opcode byte value is `0xF0` (opcode `0x1E` shifted left by 3 bits, with modifier 0).

### 3.5 CRC-16/CCITT

```
Offset: Last 2 bytes of wire
Size:   2 bytes (big-endian u16)
Input:  All bytes BEFORE the CRC (MAGIC + VER_META + INSTRUCTION_STREAM + END)
```

The CRC is computed over all bytes in the wire data **excluding** the CRC itself. It is stored as a big-endian `u16`. See [§8](#8-crc-16ccitt-algorithm) for the algorithm.

### 3.6 Complete Hex Diagram — Worked Example

Encoding `Triple(s=1, p=2, o=3)` as a Fact gene:

```
Offset  Hex    Description
──────  ─────  ──────────────────────────────────────
 00     4B     MAGIC byte ('K')
 01     20     VER_META: version=1, gene_type=0(Fact), qualifiers=false
                 (0b001_0000_0 = 0x20)
 02     00     OPCODE: Triple (op=0x00, mod=0) → byte = 0x00<<3 | 0 = 0x00
 03     01     Varint: s=1 (Tier 0, 1 byte)
 04     02     Varint: p=2 (Tier 0, 1 byte)
 05     03     Varint: o=3 (Tier 0, 1 byte)
 06     F0     END marker (Op::End = 0x1E, byte = 0x1E<<3 = 0xF0)
 07-08  XX XX  CRC-16/CCITT over bytes [00..06]
```

Total: **9 bytes** for a complete, CRC-protected knowledge unit.

---

## 4. Op Enum — All 32 Opcodes

Each opcode is a 5-bit value (0x00-0x1F). The opcode byte on wire = `(op << 3) | modifier`.

| Op | Hex | Wire Byte | Name | Operand Layout |
|----|-----|-----------|------|----------------|
| 0x00 | `0x00` | `0x00` | `TRIPLE` | varint(S), varint(P), varint(O) |
| 0x01 | `0x01` | `0x08` | `QUALITY` | varint(S), varint(Q) |
| 0x02 | `0x02` | `0x10` | `QUANTITY` | varint(S), numeric(value), varint(unit) |
| 0x03 | `0x03` | `0x18` | `SEQUENCE` | u8(N), varint(item₁), …, varint(itemₙ) |
| 0x04 | `0x04` | `0x20` | `PART_OF` | varint(part), varint(whole) |
| 0x05 | `0x05` | `0x28` | `LOCATED` | varint(S), varint(location) |
| 0x06 | `0x06` | `0x30` | `TEMPORAL` | varint(S), varint(time) |
| 0x07 | `0x07` | `0x38` | `CAUSAL` | varint(cause), varint(effect) |
| 0x08 | `0x08` | `0x40` | `SIMULATES` | varint(S), varint(model) |
| 0x09 | `0x09` | `0x48` | `CONDITION` | varint(if), varint(then) |
| 0x0A | `0x0A` | `0x50` | `AGENT` | varint(actor), varint(action) |
| 0x0B | `0x0B` | `0x58` | `TOOL` | varint(action), varint(instrument) |
| 0x0C | `0x0C` | `0x60` | `RANGE` | varint(S), numeric(min), numeric(max) |
| 0x0D | `0x0D` | `0x68` | `TOLERANCE` | varint(S), numeric(value), numeric(±delta) |
| 0x0E | `0x0E` | `0x70` | `CONSTRAINT` | varint(source), u8(op_code), varint(target) |
| 0x0F | `0x0F` | `0x78` | `ENUM_VAL` | varint(S), u8(N), varint(val₁), …, varint(valₙ) |
| 0x10 | `0x10` | `0x80` | `CERTAINTY` | u16_be(level) |
| 0x11 | `0x11` | `0x88` | `DIFFICULTY` | u8(level) |
| 0x12 | `0x12` | `0x90` | `CID_REF` | raw(32 bytes BLAKE3 hash) |
| 0x13 | `0x13` | `0x98` | `STEP` | u8(ord), varint(action), varint(target) |
| 0x14 | `0x14` | `0xA0` | `PRECOND` | varint(concept) |
| 0x15 | `0x15` | `0xA8` | `EFFECT` | varint(concept) |
| 0x16 | `0x16` | `0xB0` | `AFFECT` | i16_be(V), i16_be(A), i16_be(D) |
| 0x17 | `0x17` | `0xB8` | `LABEL` | varint(key), varint(value) |
| 0x18 | `0x18` | `0xC0` | `TEXT_REF` | u8(lang), u16_be(len), raw(bytes) |
| 0x19 | `0x19` | `0xC8` | `FORMULA` | u8(format), u16_be(len), raw(bytes) |
| 0x1A | `0x1A` | `0xD0` | `WITNESS` | u16_be(count), u8(proximity) |
| 0x1B | `0x1B` | `0xD8` | `MEDIA_REF` | u8(system), u8(len), raw(id_bytes) |
| 0x1C | `0x1C` | `0xE0` | `COMPOSITE_HDR` | u8(type), u8(completeness), u32_be(version) |
| 0x1D | `0x1D` | `0xE8` | `MEMBER` | u16_be(order), u8(role), u8(required), varint(label), raw(32B cid) |
| 0x1E | `0x1E` | `0xF0` | `END` | *(none — terminates stream)* |
| 0x1F | `0x1F` | `0xF8` | `EXTENDED` | u8(ext_byte), … *(future extension)* |

> [!NOTE]
> The "Wire Byte" column shows the actual byte on wire when `modifier = 0`. In practice, all current v1 instructions use `modifier = 0`.

### Constraint Operators (for Op 0x0E)

The `op_code` operand of `CONSTRAINT` is a single u8:

| Value | Operator | Symbol |
|-------|----------|--------|
| 0 | Equal | `==` |
| 1 | Not Equal | `!=` |
| 2 | Less Than | `<` |
| 3 | Less or Equal | `<=` |
| 4 | Greater Than | `>` |
| 5 | Greater or Equal | `>=` |

---

## 5. Instruction Set — 30 Typed Variants

Each instruction maps 1:1 to an `Op` enum value (excluding `END` and `EXTENDED` which are not data instructions). Below is every instruction variant with its exact encoding layout.

### 5.1 Triple — `Op::Triple` (0x00)

Subject-Predicate-Object fact.

```
┌──────┬───────────┬───────────┬───────────┐
│ 0x00 │ varint(S) │ varint(P) │ varint(O) │
└──────┴───────────┴───────────┴───────────┘
```

- **S**: Subject ConceptId
- **P**: Predicate ConceptId
- **O**: Object ConceptId

### 5.2 Quality — `Op::Quality` (0x01)

Subject has quality Q.

```
┌──────┬───────────┬───────────┐
│ 0x08 │ varint(S) │ varint(Q) │
└──────┴───────────┴───────────┘
```

### 5.3 Quantity — `Op::Quantity` (0x02)

Subject has a numeric measurement with unit.

```
┌──────┬───────────┬────────────────┬──────────────┐
│ 0x10 │ varint(S) │ numeric(value) │ varint(unit) │
└──────┴───────────┴────────────────┴──────────────┘
```

The `numeric(value)` is a [NumericValue](#6-numericvalue--inline-numeric-literals) (prefix byte + payload).

### 5.4 Sequence — `Op::Sequence` (0x03)

Ordered list of N ConceptIds.

```
┌──────┬───────┬────────────┬─────┬────────────┐
│ 0x18 │ u8(N) │ varint(i₁) │ ... │ varint(iₙ) │
└──────┴───────┴────────────┴─────┴────────────┘
```

- **N**: Number of items (u8, max 255)
- Followed by N varint-encoded ConceptIds

### 5.5 PartOf — `Op::PartOf` (0x04)

Part belongs to whole (hierarchical containment).

```
┌──────┬──────────────┬──────────────┐
│ 0x20 │ varint(part) │ varint(whole)│
└──────┴──────────────┴──────────────┘
```

### 5.6 Located — `Op::Located` (0x05)

Subject located at spatial location.

```
┌──────┬───────────┬─────────────────┐
│ 0x28 │ varint(S) │ varint(location)│
└──────┴───────────┴─────────────────┘
```

### 5.7 Temporal — `Op::Temporal` (0x06)

Subject has temporal relation.

```
┌──────┬───────────┬──────────────┐
│ 0x30 │ varint(S) │ varint(time) │
└──────┴───────────┴──────────────┘
```

### 5.8 Causal — `Op::Causal` (0x07)

Causation: cause leads to effect.

```
┌──────┬──────────────┬───────────────┐
│ 0x38 │ varint(cause)│ varint(effect)│
└──────┴──────────────┴───────────────┘
```

### 5.9 Simulates — `Op::Simulates` (0x08)

Subject simulates/mimics a model (analogy).

```
┌──────┬───────────┬──────────────┐
│ 0x40 │ varint(S) │ varint(model)│
└──────┴───────────┴──────────────┘
```

### 5.10 Condition — `Op::Condition` (0x09)

If condition then result.

```
┌──────┬──────────────┬───────────────┐
│ 0x48 │ varint(cond) │ varint(result)│
└──────┴──────────────┴───────────────┘
```

### 5.11 Agent — `Op::Agent` (0x0A)

Actor performs action.

```
┌──────┬──────────────┬───────────────┐
│ 0x50 │ varint(actor)│ varint(action)│
└──────┴──────────────┴───────────────┘
```

### 5.12 Tool — `Op::Tool` (0x0B)

Action uses instrument.

```
┌──────┬───────────────┬────────────────────┐
│ 0x58 │ varint(action)│ varint(instrument) │
└──────┴───────────────┴────────────────────┘
```

### 5.13 Range — `Op::Range` (0x0C)

Subject has value range [min, max].

```
┌──────┬───────────┬──────────────┬──────────────┐
│ 0x60 │ varint(S) │ numeric(min) │ numeric(max) │
└──────┴───────────┴──────────────┴──────────────┘
```

### 5.14 Tolerance — `Op::Tolerance` (0x0D)

Subject has value ± delta (precision with error margin).

```
┌──────┬───────────┬────────────────┬──────────────┐
│ 0x68 │ varint(S) │ numeric(value) │ numeric(±δ)  │
└──────┴───────────┴────────────────┴──────────────┘
```

### 5.15 Constraint — `Op::Constraint` (0x0E)

Numeric constraint between source and target concepts.

```
┌──────┬───────────────┬────────────┬───────────────┐
│ 0x70 │ varint(source)│ u8(op_code)│ varint(target)│
└──────┴───────────────┴────────────┴───────────────┘
```

`op_code` values: 0=`==`, 1=`!=`, 2=`<`, 3=`<=`, 4=`>`, 5=`>=`.

### 5.16 EnumVal — `Op::EnumVal` (0x0F)

Subject is one of a set of values.

```
┌──────┬───────────┬───────┬─────────────┬─────┬─────────────┐
│ 0x78 │ varint(S) │ u8(N) │ varint(v₁)  │ ... │ varint(vₙ)  │
└──────┴───────────┴───────┴─────────────┴─────┴─────────────┘
```

### 5.17 Certainty — `Op::Certainty` (0x10)

Confidence level, scaled integer 0-10,000 (represents 0.0000-1.0000).

```
┌──────┬──────────────┐
│ 0x80 │ u16_be(level)│
└──────┴──────────────┘
```

- A level of `9900` = 99.00% certainty.

### 5.18 Difficulty — `Op::Difficulty` (0x11)

Difficulty scale 0-4.

```
┌──────┬───────────┐
│ 0x88 │ u8(level) │
└──────┴───────────┘
```

| Value | Meaning |
|-------|---------|
| 0 | Trivial |
| 1 | Easy |
| 2 | Medium |
| 3 | Hard |
| 4 | Expert |

### 5.19 CidRef — `Op::CidRef` (0x12)

32-byte BLAKE3 content hash reference to another KU.

```
┌──────┬──────────────────────────┐
│ 0x90 │ raw(32 bytes BLAKE3 CID) │
└──────┴──────────────────────────┘
```

Fixed 33 bytes total (1 opcode + 32 hash).

### 5.20 Step — `Op::Step` (0x13)

Procedure step with order, action, and target.

```
┌──────┬──────────┬───────────────┬───────────────┐
│ 0x98 │ u8(ord)  │ varint(action)│ varint(target)│
└──────┴──────────┴───────────────┴───────────────┘
```

- **ord**: Step order (0-based, u8)

### 5.21 Precond — `Op::Precond` (0x14)

Step precondition concept. Associates with the preceding `STEP` instruction.

```
┌──────┬────────────────┐
│ 0xA0 │ varint(concept)│
└──────┴────────────────┘
```

### 5.22 Effect — `Op::Effect` (0x15)

Step effect/result concept. Associates with the preceding `STEP` instruction.

```
┌──────┬────────────────┐
│ 0xA8 │ varint(concept)│
└──────┴────────────────┘
```

### 5.23 Affect — `Op::Affect` (0x16)

VAD emotional model (Valence, Arousal, Dominance). Each is a signed 16-bit integer.

```
┌──────┬───────────┬───────────┬───────────┐
│ 0xB0 │ i16_be(V) │ i16_be(A) │ i16_be(D) │
└──────┴───────────┴───────────┴───────────┘
```

Fixed 7 bytes total. Values typically range from -10,000 to +10,000.

### 5.24 Label — `Op::Label` (0x17)

Generic key-value metadata pair.

```
┌──────┬────────────┬──────────────┐
│ 0xB8 │ varint(key)│ varint(value)│
└──────┴────────────┴──────────────┘
```

Used as a catch-all for semantic roles that don't have dedicated opcodes. Special key values:

| Key | Meaning |
|-----|---------|
| `0xF000` | Domain marker (for Formal genes) |
| `0xF001` | Summary marker (for Composite genes) |
| `0xFFFF` | Fallback primary subject |
| `0x01`-`0x0E` | RoleId as u64 (maps codon roles) |

### 5.25 TextRef — `Op::TextRef` (0x18)

Canonical text reference (compressed byte blob).

```
┌──────┬──────────┬──────────────┬─────────────────┐
│ 0xC0 │ u8(lang) │ u16_be(len)  │ raw(bytes[len]) │
└──────┴──────────┴──────────────┴─────────────────┘
```

- **lang**: Language code (u8)
- **len**: Data length in bytes (u16, big-endian)

### 5.26 Formula — `Op::Formula` (0x19)

Mathematical or logical formula (LaTeX, MathML).

```
┌──────┬────────────┬──────────────┬─────────────────┐
│ 0xC8 │ u8(format) │ u16_be(len)  │ raw(bytes[len]) │
└──────┴────────────┴──────────────┴─────────────────┘
```

- **format**: Notation format (u8). 0=LaTeX, other values TBD.

### 5.27 Witness — `Op::Witness` (0x1A)

Witness/testimony data.

```
┌──────┬────────────────┬───────────────┐
│ 0xD0 │ u16_be(count)  │ u8(proximity) │
└──────┴────────────────┴───────────────┘
```

- **count**: Number of witnesses (u16)
- **proximity**: Proximity level (u8)

### 5.28 MediaRef — `Op::MediaRef` (0x1B)

External media reference.

```
┌──────┬────────────┬──────────┬──────────────────┐
│ 0xD8 │ u8(system) │ u8(len)  │ raw(id_bytes)    │
└──────┴────────────┴──────────┴──────────────────┘
```

- **system**: Media system identifier (u8)
- **len**: ID byte length (u8, max 255)

### 5.29 CompositeHdr — `Op::CompositeHdr` (0x1C)

Composite KU header metadata.

```
┌──────┬───────────┬─────────────────┬────────────────┐
│ 0xE0 │ u8(type)  │ u8(completeness)│ u32_be(version)│
└──────┴───────────┴─────────────────┴────────────────┘
```

Fixed 8 bytes total.

### 5.30 Member — `Op::Member` (0x1D)

Composite member entry.

```
┌──────┬──────────────┬──────────┬──────────────┬──────────────┬──────────────────┐
│ 0xE8 │ u16_be(order)│ u8(role) │ u8(required) │ varint(label)│ raw(32B cid)     │
└──────┴──────────────┴──────────┴──────────────┴──────────────┴──────────────────┘
```

- **order**: Ordering position (u16, 0-based)
- **role**: Structural role (u8)
- **required**: 0=optional, 1=required
- **label**: ConceptId for member label
- **cid**: 32-byte BLAKE3 content hash of the member KU

---

## 6. NumericValue — Inline Numeric Literals

Numeric values are encoded inline in the instruction stream using a 1-byte type prefix followed by the value in big-endian byte order. The prefix bytes (`0xFA`-`0xFF`) are chosen to be **outside the varint range** (varints use `0x00`-`0xF7` as first bytes), so the decoder can unambiguously distinguish numeric literals from varint ConceptIds.

### Prefix Table

| Prefix | Type | Payload | Total Bytes | Value Range |
|--------|------|---------|-------------|-------------|
| `0xFA` | `U8` | 1 byte (unsigned) | 2 | 0 – 255 |
| `0xFB` | `U16` | 2 bytes BE | 3 | 0 – 65,535 |
| `0xFC` | `I16` | 2 bytes BE (signed) | 3 | -32,768 – 32,767 |
| `0xFD` | `U32` | 4 bytes BE | 5 | 0 – 4,294,967,295 |
| `0xFE` | `I32` | 4 bytes BE (signed) | 5 | -2,147,483,648 – 2,147,483,647 |
| `0xFF` | `F32` | 4 bytes BE (IEEE 754) | 5 | ±3.4 × 10³⁸ |

### Encoding Examples

```
NumericValue::U8(42)       → [0xFA, 0x2A]                   (2 bytes)
NumericValue::U16(1000)    → [0xFB, 0x03, 0xE8]             (3 bytes)
NumericValue::I16(-500)    → [0xFC, 0xFE, 0x0C]             (3 bytes)
NumericValue::U32(100000)  → [0xFD, 0x00, 0x01, 0x86, 0xA0] (5 bytes)
NumericValue::I32(-100000) → [0xFE, 0xFF, 0xFE, 0x79, 0x60] (5 bytes)
NumericValue::F32(35.2)    → [0xFF, 0x42, 0x0C, 0xCC, 0xCD] (5 bytes)
```

### Decoding Algorithm

```
function decode_numeric(data, pos):
    prefix = data[pos]
    switch prefix:
        case 0xFA: return (U8(data[pos+1]),  2)
        case 0xFB: return (U16(BE16(data[pos+1..pos+3])), 3)
        case 0xFC: return (I16(BE16(data[pos+1..pos+3])), 3)
        case 0xFD: return (U32(BE32(data[pos+1..pos+5])), 5)
        case 0xFE: return (I32(BE32(data[pos+1..pos+5])), 5)
        case 0xFF: return (F32(BE32(data[pos+1..pos+5])), 5)
        default:   ERROR("Invalid numeric prefix")
```

### Numeric vs. Varint Disambiguation

When an operand could be either a numeric value or a varint (e.g., in `QUANTITY`, `RANGE`, `TOLERANCE`), the decoder checks the first byte:

```
if data[pos] >= 0xFA:
    → decode as NumericValue (prefix + payload)
else:
    → decode as varint ConceptId (then wrap as U16 or U32)
```

This works because:
- Varint first bytes: `0x00`–`0xF7` (Tier 3+ max prefix is `0xF0 | 0x07 = 0xF7`)
- Numeric prefixes: `0xFA`–`0xFF`
- Gap `0xF8`–`0xF9` is unused (available for future extensions)

---

## 7. Varint Encoding — ConceptId Wire Format

ConceptIds are encoded as variable-length integers using OneBrain's tier-based varint scheme. This is **not** standard protobuf varint — it uses prefix bits to determine length and applies offsets for each tier.

### Tier Table

| Tier | Bytes | Prefix Bits | Data Bits | Offset | Range | Count |
|------|-------|-------------|-----------|--------|-------|-------|
| 0 | 1 | `0xxxxxxx` | 7 | 0 | 0 – 127 | 128 |
| 1 | 2 | `10xxxxxx` | 14 | 128 | 128 – 16,511 | 16,384 |
| 2 | 3 | `110xxxxx` | 21 | 16,512 | 16,512 – 2,113,663 | 2,097,152 |
| 3 | 4 | `1110xxxx` | 28 | 2,113,664 | 2,113,664 – 270,549,119 | 268,435,456 |
| 3+ | 5 | `11110xxx` | 35 | 270,549,120 | 270,549,120 – 34,628,173,567 | ~34.4B |

### Encoding Algorithm (Pseudocode)

```
function encode_varint(value):
    if value <= 127:
        return [value]                    // Tier 0: 1 byte

    else if value <= 16,511:
        v = value - 128
        return [0x80 | (v >> 8),          // Tier 1: 2 bytes
                v & 0xFF]

    else if value <= 2,113,663:
        v = value - 16,512
        return [0xC0 | ((v >> 16) & 0x1F),  // Tier 2: 3 bytes
                (v >> 8) & 0xFF,
                v & 0xFF]

    else if value <= 270,549,119:
        v = value - 2,113,664
        return [0xE0 | ((v >> 24) & 0x0F),  // Tier 3: 4 bytes
                (v >> 16) & 0xFF,
                (v >> 8) & 0xFF,
                v & 0xFF]

    else if value <= 34,628,173,567:
        v = value - 270,549,120
        return [0xF0 | ((v >> 32) & 0x07),  // Tier 3+: 5 bytes
                (v >> 24) & 0xFF,
                (v >> 16) & 0xFF,
                (v >> 8) & 0xFF,
                v & 0xFF]

    else:
        ERROR("Value exceeds maximum")
```

### Decoding Algorithm (Pseudocode)

```
function decode_varint(bytes):
    first = bytes[0]

    if (first & 0x80) == 0:            // 0xxxxxxx
        return (first, 1)              // Tier 0

    else if (first & 0xC0) == 0x80:    // 10xxxxxx
        adjusted = ((first & 0x3F) << 8) | bytes[1]
        return (adjusted + 128, 2)     // Tier 1

    else if (first & 0xE0) == 0xC0:    // 110xxxxx
        adjusted = ((first & 0x1F) << 16) | (bytes[1] << 8) | bytes[2]
        return (adjusted + 16512, 3)   // Tier 2

    else if (first & 0xF0) == 0xE0:    // 1110xxxx
        adjusted = ((first & 0x0F) << 24) | (bytes[1] << 16)
                 | (bytes[2] << 8) | bytes[3]
        return (adjusted + 2113664, 4) // Tier 3

    else if (first & 0xF8) == 0xF0:    // 11110xxx
        adjusted = ((first & 0x07) << 32) | (bytes[1] << 24)
                 | (bytes[2] << 16) | (bytes[3] << 8) | bytes[4]
        return (adjusted + 270549120, 5) // Tier 3+

    else:
        ERROR("Invalid varint prefix")
```

### Worked Examples

```
Value 1     → [0x01]                           (1 byte, Tier 0)
Value 127   → [0x7F]                           (1 byte, Tier 0)
Value 128   → [0x80, 0x00]                     (2 bytes, Tier 1: (128-128)=0)
Value 300   → [0x80, 0xAC]                     (2 bytes, Tier 1: (300-128)=172=0x00AC)
Value 500   → [0x81, 0x74]                     (2 bytes, Tier 1: (500-128)=372=0x0174)
Value 16511 → [0xBF, 0xFF]                     (2 bytes, Tier 1: max)
Value 16512 → [0xC0, 0x00, 0x00]               (3 bytes, Tier 2: (16512-16512)=0)
Value 100000→ [0xC1, 0x45, 0xE0]               (3 bytes, Tier 2: (100000-16512)=83488)
```

### Concept ID Tiers — Semantic Meaning

| Tier | Range | Purpose |
|------|-------|---------|
| 0 (1B) | 0-127 | 128 universal primitives (DO, BE, HAVE, NOT, AND, …) |
| 1 (2B) | 128-16,511 | ~16K common concepts (WATER, FIRE, HUMAN, ANIMAL, …) |
| 2 (3B) | 16,512-2,113,663 | ~2M standard concepts (PHOTOSYNTHESIS, MITOCHONDRIA, …) |
| 3 (4B) | 2,113,664-270,549,119 | ~268M extended/domain concepts |
| 3+ (5B) | 270,549,120+ | ~34.4B community/user-defined concepts |

---

## 8. CRC-16/CCITT Algorithm

### Parameters

| Parameter | Value |
|-----------|-------|
| Algorithm | CRC-16/CCITT (XMODEM) |
| Polynomial | `0x1021` (x¹⁶ + x¹² + x⁵ + 1) |
| Init value | `0xFFFF` |
| Input reflected | No |
| Output reflected | No |
| Final XOR | None (`0x0000`) |
| Check value | `0x29B1` (for ASCII `"123456789"`) |
| Wire encoding | Big-endian u16 |

### Pseudocode Implementation

```
function crc16_ccitt(data: byte[]):
    crc = 0xFFFF

    for byte in data:
        crc = crc XOR (byte << 8)

        repeat 8 times:
            if (crc AND 0x8000) != 0:
                crc = (crc << 1) XOR 0x1021
            else:
                crc = crc << 1

        crc = crc AND 0xFFFF      // Keep 16 bits

    return crc
```

### Usage in Wire Format

1. **Encoding**: Compute CRC over `MAGIC + VER_META + INSTRUCTION_STREAM + END`, then append CRC as 2 bytes big-endian.
2. **Decoding**: Extract last 2 bytes as stored CRC. Compute CRC over all preceding bytes. Compare. Reject on mismatch.

### Verification Example

```
Input:  b"123456789"  (9 bytes: 0x31 0x32 0x33 0x34 0x35 0x36 0x37 0x38 0x39)
Output: 0x29B1

This is the standard CRC-16/CCITT check value.
Implementations MUST pass this test.
```

---

## 9. Auto-detect — v4/v5 CBOR vs v6 Core DNA

### Detection Algorithm

The wire format can be determined by examining the **first 2 bytes**:

```
function detect_wire_format(data):
    if data.length < 2:
        return UNKNOWN

    if data[0] == 0x4B AND data[1] == 0x44:
        return CBOR_V4V5        // Two-byte magic "KD" (0x4B44)

    else if data[0] == 0x4B:
        return CORE_DNA_V6      // Single-byte magic 'K', second byte is VER_META

    else:
        return UNKNOWN
```

### Why This Works

The key insight is that v4/v5 CBOR uses `0x4B 0x44` as its magic (ASCII `"KD"`), while v6 Core DNA uses `0x4B` followed by a VER_META byte.

The VER_META byte **cannot** be `0x44` (`0b01000100`) in any valid v6 encoding because:
- `0x44` means version=2, gene_type=2, has_qualifiers=false
- While theoretically valid, in practice version=1 is current
- More importantly: `0x44` has bit 6 set and bit 7 clear, which would parse as version=2 — the detection logic simply checks if the second byte is exactly `0x44` before falling through to the Core DNA path

**Decision tree:**

```
data[0] == 0x4B?
├── YES → data[1] == 0x44?
│         ├── YES → v4/v5 CBOR (magic "KD")
│         └── NO  → v6 Core DNA (magic 'K' + VER_META)
└── NO  → Unknown format
```

> [!WARNING]
> If a future Core DNA version uses `gene_type=2` and `version=2`, the VER_META byte would be `0x44`, colliding with the CBOR magic. The detection code handles this by checking for `0x44` first (CBOR takes priority). This means Core DNA `version=2, gene_type=2, qualifiers=false` is effectively a **reserved** combination.

---

## 10. Size Comparisons — Measured Results

### Test Case 1: "Bơi ếch" (Breaststroke)

Vietnamese text describing breaststroke swimming technique decomposed into 3 Knowledge Units.

| Encoding | Size | Ratio vs. Text |
|----------|------|----------------|
| **Original Vietnamese text (UTF-8)** | **323 bytes** | 1.0× (baseline) |
| Core DNA KU#1 (Fact: Definition) — 4 instructions | ~20 bytes | — |
| Core DNA KU#2 (Procedure: Swimming Cycle) — 9 instructions | ~38 bytes | — |
| Core DNA KU#3 (Fact: Properties) — 3 instructions | ~14 bytes | — |
| **Core DNA total (3 KUs)** | **88 bytes** | **3.7× smaller** |
| CBOR v5 total | 1,053 bytes | 3.3× LARGER |

### Test Case 2: Rocket Systems ("Tên lửa")

1,078-byte Vietnamese text about rocket systems, decomposed into 5 Knowledge Units.

| Encoding | Size | Ratio vs. Text |
|----------|------|----------------|
| **Original Vietnamese text (UTF-8)** | **1,078 bytes** | 1.0× (baseline) |
| Core DNA KU#1 (Body & Shell) — 8 instructions | ~40 bytes | — |
| Core DNA KU#2 (Liquid Fuel Engine) — 8 instructions | ~38 bytes | — |
| Core DNA KU#3 (Solid Fuel) — 4 instructions | ~20 bytes | — |
| Core DNA KU#4 (Guidance & Control) — 6 instructions | ~30 bytes | — |
| Core DNA KU#5 (Payload Bay) — 4 instructions | ~20 bytes | — |
| **Core DNA total (5 KUs)** | **~172 bytes** | **~6.3× smaller** |

### Test Case 3: Airplane Wing Design

131-byte English text describing wing parameters with 10 measurements, 1 constraint.

| Encoding | Size | Ratio vs. Text |
|----------|------|----------------|
| **English text** | **131 bytes** | 1.0× (baseline) |
| **Core DNA (1 KU)** — 12 instructions | **~118 bytes** | **1.1× smaller** |

> [!NOTE]
> The airplane test case shows the lower bound of compression advantage. Highly numeric, short descriptions with many floats approach text size because each F32 value requires 5 bytes (prefix + 4 bytes IEEE 754). The advantage is still positive, and Core DNA preserves machine-queryable structure that text does not.

### Summary: v5 CBOR vs v6 Core DNA

| Metric | v5 CBOR | v6 Core DNA | Improvement |
|--------|---------|-------------|-------------|
| "Bơi ếch" encoding | 1,053 B | 88 B | **12× smaller** |
| vs. text (323 B) | 3.3× LARGER | 3.7× SMALLER | — |
| Overhead per KU | ~100-300 B | ~5-40 B | ~10× less |
| Minimum wire size | ~50 B | 5 B | 10× smaller |

---

## 11. Backward Compatibility — decode_any

The `decode_any` function provides a unified decoder that accepts **both** v4/v5 CBOR and v6 Core DNA wire formats and returns a `KnowledgeUnit`.

### Algorithm

```
function decode_any(data):
    format = detect_wire_format(data)    // See §9

    switch format:
        case CBOR_V4V5:
            return decode_cbor_v4v5(data)      // Existing CBOR decoder

        case CORE_DNA_V6:
            dna = decode_core_dna(data)        // Core DNA binary decoder
            ku  = core_dna_to_ku(dna)          // Bridge to KnowledgeUnit
            return ku

        case UNKNOWN:
            ERROR("Unknown wire format")
```

### Guarantees

1. **Transparent upgrade**: Existing v4/v5 CBOR data continues to decode without modification
2. **Format negotiation**: Consumers call `decode_any` without knowing the format
3. **Gene type preservation**: The gene type survives the Core DNA → KU bridge
4. **Data fidelity for core data**: Triples, steps, quantities, constraints roundtrip perfectly
5. **Epigenetic loss accepted**: Trust, bonds, qualifiers are absent from Core DNA (see §12)

---

## 12. Bridge — KU ↔ CoreDna Conversion

### 12.1 KU → CoreDna (`ku_to_core_dna`)

This is a **LOSSY** conversion. The Core DNA layer stores only the structural knowledge "genes" — all runtime metadata is intentionally dropped.

#### What is preserved

| KU Component | Core DNA Encoding |
|-------------|-------------------|
| Gene type | `gene_type` field in VER_META |
| Codons (by role) | Mapped to instruction types (Quality, Agent, Located, Label) |
| Triples (Fact) | `TRIPLE` instructions |
| Certainty | `CERTAINTY` instruction |
| Steps (Procedure) | `STEP` + `PRECOND` + `EFFECT` instructions |
| Difficulty | `DIFFICULTY` instruction |
| Scene + Affect (Experience) | `LABEL` + `AFFECT` instructions |
| Body + Confidence (Hypothesis) | `LABEL` + `CERTAINTY` + `DIFFICULTY` instructions |
| Formula (Formal) | `FORMULA` + `LABEL(0xF000)` instructions |
| Witness data (Testimony) | `TRIPLE` + `WITNESS` instructions |
| Members (Composite) | `COMPOSITE_HDR` + `MEMBER` + `LABEL(0xF001)` instructions |

#### What is dropped (lives in Epigenetics layer)

| KU Component | Status |
|-------------|--------|
| Trust section | ❌ Dropped |
| Epigenetic section | ❌ Dropped |
| Bonds (edges to other KUs) | ❌ Dropped |
| Qualifier details on codons | ❌ Dropped |
| Epistemic status | ❌ Dropped |
| Evidence type | ❌ Dropped |
| Edge state, decay, reinforcement | ❌ Dropped |

#### Codon-to-Instruction Mapping

```
Codon Role           → Instruction Type
─────────────────────────────────────────
Object               → (skipped — used as primary subject S)
Quality              → Quality(s=primary, q=concept)
Agent                → Agent(actor=concept, action=primary)
Location             → Located(s=primary, location=concept)
Manner/Purpose/      → Label(key=role_as_u64, value=concept)
Condition/Cause/
Result/Time/
Quantity/Tool/
CompoundHead/
CompoundMod
```

### 12.2 CoreDna → KU (`core_dna_to_ku`)

This is an **INFLATABLE** conversion. The Core DNA is decoded and expanded back into a full `KnowledgeUnit` struct, with missing epigenetic data set to defaults.

#### Inflation Rules

| KU Field | Inflated Value |
|----------|---------------|
| `bonds` | `[]` (empty) |
| `flags` | `HeaderFlags::default()` |
| `epistemic_status` | `None` |
| `evidence_type` | `None` |
| `trust` | `None` |
| `epigenetic` | `None` |

#### Primary Subject Recovery

The decoder finds the primary subject by scanning instructions for the first `Triple`, `Quality`, or `Quantity` instruction and extracting its `s` field. This is added as an `Object`-role codon.

#### Two-Pass Decode

1. **Pass 1**: Find primary subject from first Triple/Quality/Quantity
2. **Pass 2**: Collect all instructions into codons, triples, steps, etc.

#### Gene Reconstruction by Type

| Gene Type | Reconstructed From |
|-----------|-------------------|
| Fact | Triples + Certainty |
| Procedure | Steps (with Precond/Effect attached) + Difficulty |
| Experience | Scene codons + Affect(V,A,D) |
| Hypothesis | Body codons + Certainty + Difficulty (maturity) |
| Formal | Formula data + Domain label |
| Testimony | Triples + Witness data |
| Composite | CompositeHdr + Members + Summary labels |
| *Other* | Fallback to Fact(triples, certainty) |

### 12.3 Roundtrip Fidelity

```
KU ──ku_to_core_dna──► CoreDna ──encode──► bytes ──decode──► CoreDna ──core_dna_to_ku──► KU'
         LOSSY                  LOSSLESS         LOSSLESS            INFLATABLE
```

- **KU → CoreDna**: Lossy (epigenetics dropped)
- **CoreDna → bytes → CoreDna**: Lossless (bit-perfect roundtrip, CRC-verified)
- **CoreDna → KU'**: Inflatable (defaults restored, structure preserved)
- **KU' ≠ KU**: The reconstructed KU lacks trust, bonds, qualifiers, and epigenetic metadata

> [!TIP]
> For full-fidelity persistence of a KnowledgeUnit (including bonds, trust, and epigenetics), use the CBOR v4/v5 encoder. Core DNA is designed for compact, network-efficient transmission of the essential knowledge content.

---

## Appendix A: Quick Reference — Byte Cheat Sheet

```
MAGIC:      0x4B
VER_META:   [VVV GGGG Q]  (V=version, G=gene_type, Q=has_qualifiers)
OPCODE:     [OOOOO MMM]   (O=opcode, M=modifier)
END:        0xF0           (Op::End=0x1E, shifted: 0x1E<<3=0xF0)
CRC:        2 bytes big-endian, CRC-16/CCITT(poly=0x1021, init=0xFFFF)

NUMERIC PREFIXES:
  0xFA = U8   (1 byte payload)
  0xFB = U16  (2 bytes BE)
  0xFC = I16  (2 bytes BE, signed)
  0xFD = U32  (4 bytes BE)
  0xFE = I32  (4 bytes BE, signed)
  0xFF = F32  (4 bytes BE, IEEE 754)

VARINT PREFIXES:
  0xxxxxxx = 1 byte  (0-127)
  10xxxxxx = 2 bytes  (128-16,511)
  110xxxxx = 3 bytes  (16,512-2,113,663)
  1110xxxx = 4 bytes  (2,113,664-270,549,119)
  11110xxx = 5 bytes  (270,549,120-34,628,173,567)

FORMAT DETECTION:
  0x4B 0x44 → CBOR v4/v5
  0x4B 0x??  → Core DNA v6 (if ?? ≠ 0x44)
```

## Appendix B: Worked Example — Full Wire Decode

Input hex (a Fact KU with one Triple and Certainty):

```
4B 20 00 01 02 03 80 26 AC F0 XX XX
```

Step-by-step decode:

| Offset | Bytes | Decoded |
|--------|-------|---------|
| 0 | `4B` | MAGIC = 0x4B ✓ |
| 1 | `20` | VER_META: version=1 (`001`), gene_type=0 (`0000`), qualifiers=false (`0`) |
| 2 | `00` | OPCODE: op=0x00 (Triple), mod=0 |
| 3 | `01` | Varint Tier 0: S = 1 |
| 4 | `02` | Varint Tier 0: P = 2 |
| 5 | `03` | Varint Tier 0: O = 3 |
| 6 | `80` | OPCODE: op=0x10 (Certainty), mod=0 |
| 7-8 | `26 AC` | u16 BE: level = 9900 |
| 9 | `F0` | END marker |
| 10-11 | `XX XX` | CRC-16/CCITT over bytes [0..9] |

Result: `CoreDna { version=1, gene_type=Fact, instructions=[Triple(1,2,3), Certainty(9900)] }`
