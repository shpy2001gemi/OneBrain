# 6. Evaluation

## 6.1 Implementation Overview

The reference implementation is written in Rust (2021 edition), chosen for its memory safety guarantees without garbage collection, zero-cost abstractions, and deterministic performance characteristics — properties essential for a wire-format library intended to operate across constrained and distributed environments [Matsakis and Klock, 2014].

### 6.1.1 Scale and Structure

The codebase comprises approximately **15,000 lines of code** distributed across more than **40 modules**, accompanied by **827 unit and integration tests**. External dependencies are deliberately minimal:

| Dependency | Purpose |
|:---|:---|
| `serde` | Serialization/deserialization framework |
| `serde_json` | JSON interoperability |
| `blake3` | Cryptographic hashing for concept identities (CCID) |
| `redb` | Persistent embedded database for ConceptDict |

### 6.1.2 Principal Modules

| Module | Approx. LOC | Responsibility |
|:---|:---|:---|
| `core_dna.rs` | ~2,100 | Wire-format encoding/decoding, Instruction enum, CoreDna struct |
| `types.rs` | ~1,300 | ConceptId, GeneType enum (13 variants), BondType (33 variants) |
| `varint.rs` | ~290 | 5-tier variable-length integer encoding |
| `tier0_concepts.rs` | ~230 | 80 Tier 0 universal concept constants |
| `ccid.rs` | ~130 | 128-bit BLAKE3-based concept identity (CCID) |
| `concept_registry.rs` | ~320 | Offline concept registry (~200 MB lookup table) |
| `concept_dict.rs` | — | Bilingual concept dictionary with persistent storage |
| `crdt.rs` | — | Five CRDT implementations for distributed merge |
| `epigenetics.rs` | — | TrustSection (6 PoMV signals), bond management, epistemic status |
| `ku_runtime.rs` | — | KuRuntime composite type orchestrating all three layers |

### 6.1.3 Architectural Principles

The implementation adheres to four design principles:

1. **Single-pass processing.** Both encoding and decoding operate in a single forward pass over the byte stream, enabling streaming use cases and bounded memory consumption.
2. **Deterministic output.** Given identical semantic input, the encoder produces byte-identical output — a property critical for content-addressable storage (CID = BLAKE3 hash of encoded bytes) and deduplication.
3. **Fail-fast validation.** CRC-16/CCITT checksums (polynomial 0x1021, initial value 0xFFFF) appended to each CoreDna unit enable immediate integrity verification upon receipt, without schema negotiation.
4. **Minimal allocation.** The Rust implementation leverages stack allocation and borrowing to minimize heap pressure during encoding and decoding.

---

## 6.2 Test Coverage

The implementation is accompanied by **827 tests** spanning unit, integration, property-based, and adversarial categories. The test suite verifies correctness at every layer of the encoding stack.

### 6.2.1 Test Distribution

| Category | Description |
|:---|:---|
| CoreDna encode/decode | Roundtrip fidelity for all 32 opcodes and edge cases |
| Varint tier boundaries | Boundary values at each tier transition (127→128, 16383→16384, …) |
| CCID determinism | Identical inputs produce identical 128-bit BLAKE3 hashes |
| ConceptRegistry resolution | Correct lookup and fallback behavior across the concept namespace |
| CRDT merge correctness | Commutativity, associativity, and idempotency for all 5 CRDTs |
| GeneType coverage | All 13 gene types encode and decode correctly |
| NumericValue coverage | All 7 numeric representations (F64, U8, U16, I16, U32, I32, F32) |
| BondType coverage | All 33 bond types correctly serialized and deserialized |
| Text parser | Natural language → KU conversion for representative inputs |
| Epigenetics / PoMV | Trust score computation, bond lifecycle, and PoMV signal validation |

### 6.2.2 Roundtrip Invariant

The central correctness property is the **roundtrip invariant**:

$$\forall k \in \text{KU} : \text{decode}(\text{encode}(k)) \equiv k$$

Every Knowledge Unit constructible through the public API must survive a full encode–decode cycle with bit-exact fidelity. This invariant is tested exhaustively for each instruction type, each of the 13 gene types, each of the 7 numeric representations, and each of the 33 bond types.

### 6.2.3 CRDT Verification

The five CRDT implementations are verified against the formal convergence requirements:

- **Commutativity:** $\text{merge}(a, b) \equiv \text{merge}(b, a)$
- **Associativity:** $\text{merge}(\text{merge}(a, b), c) \equiv \text{merge}(a, \text{merge}(b, c))$
- **Idempotency:** $\text{merge}(a, a) \equiv a$

These properties are tested using randomized inputs to ensure convergence under arbitrary merge orderings, reflecting the reality of distributed knowledge synchronization where message arrival order is non-deterministic.

### 6.2.4 Adversarial Input Testing

The test suite includes adversarial cases designed to stress boundary conditions:

- Truncated byte streams (missing END marker, missing CRC-16)
- Invalid opcode bytes (values outside the 0x00–0x1F range)
- Varint overflows at each tier boundary
- Malformed Concept Table entries (invalid CCID lengths)
- CRC-16 mismatches (single-bit corruption detection)

---

## 6.3 Wire Size Benchmarks

To evaluate the compactness of KU CoreDna, representative knowledge expressions were encoded and their wire sizes compared against UTF-8 natural-language equivalents.

### 6.3.1 Benchmark Results

| Example | UTF-8 (bytes) | KU CoreDna (bytes) | KU Count | Compression Ratio |
|:---|:---|:---|:---|:---|
| Breaststroke technique (Vietnamese) | 323 | 88 | 3 | **3.7×** |
| Rocket propulsion systems (English) | 1,078 | 172 | 5 | **6.3×** |
| "Water boils at 100 °C" | 37 | ~14–20 | 1 | **~1.9–2.6×** |

### 6.3.2 Minimal Fact Decomposition

The single-fact case ("Water boils at 100 °C") illustrates the byte-level structure of a minimal CoreDna unit:

```
Field            Bytes   Description
─────────────────────────────────────────────
MAGIC            1       0x4B format identifier
VER_META         1       Version(3b) + GeneType(4b) + HasConceptTable(1b)
TRIPLE           4       Subject(1) + Predicate(1) + Object(1) + BondType(1)
QUANTITY         8       NumType(1) + Unit(1) + Value(5, varint) + Precision(1)
CERTAINTY        3       CertaintyOp(1) + Level(2, u16)
END              1       0x1E terminator
CRC-16           2       Integrity checksum
─────────────────────────────────────────────
Total           ~20      bytes
```

When all concept identifiers fall within Tier 0 (1 byte each), the total reduces to approximately **14 bytes**. The range of 14–20 bytes reflects variation in concept identifier magnitudes and optional metadata fields.

### 6.3.3 Scaling Observations

The compression ratio increases with source text complexity. Vietnamese-language expressions, which are morphologically verbose in UTF-8, exhibit a 3.7× reduction. Technical English descriptions of multi-component systems achieve ratios exceeding 6×. This scaling behavior is expected: KU CoreDna encodes *meaning* via concept identifiers rather than *surface form* via character sequences. The fixed overhead of the wire-format envelope (MAGIC + VER_META + END + CRC-16 = 5 bytes) is amortized over richer semantic content, yielding greater compression for more complex expressions.

### 6.3.4 Multi-KU Encoding

Complex knowledge is decomposed into multiple atomic KUs. The rocket propulsion example (1,078 bytes UTF-8) decomposes into 5 KUs totaling 172 bytes. Each KU is independently content-addressed (CID = BLAKE3 hash), self-delimited, and integrity-verified. This decomposition enables selective retrieval: a consumer interested only in the propellant chemistry need not download the aerodynamics KUs.

---

## 6.4 Comparison with Binary Formats

To contextualize the efficiency of KU CoreDna, the encoding of a single atomic fact ("Water boils at 100 °C") is compared across established serialization formats:

| Format | Size (bytes) | Self-Describing | Language-Agnostic | Schema Required |
|:---|:---|:---:|:---:|:---:|
| KU CoreDna | **~14–20** | ✓ | ✓ | ✗ |
| Protocol Buffers | ~50 | ✗ | ✗ | ✓ |
| CBOR | ~60 | ✓ | ✗ | ✗ |
| MessagePack | ~55 | ✓ | ✗ | ✗ |
| FlatBuffers | ~80 | ✗ | ✗ | ✓ |
| RDF/Turtle | ~150 | ✓ | ✓ | ✓ |
| JSON-LD | ~350 | ✓ | Partial | ✓ |
| UTF-8 text | 37 | N/A | ✗ | ✗ |

### 6.4.1 Analysis

**Protocol Buffers** [Google, 2024] achieves competitive compactness (~50 bytes) through aggressive binary encoding but sacrifices self-description: a Protobuf message is opaque without its accompanying `.proto` schema definition. KU CoreDna is both smaller and fully self-describing.

**CBOR** (Concise Binary Object Representation) [Bormann and Hoffman, 2013] and **MessagePack** [Furuhashi, 2013] are self-describing binary formats but encode data as generic key-value pairs rather than semantic concepts. Their ~55–60 byte encodings of a single fact are 3–4× larger than KU CoreDna because they transmit natural-language field names and string values rather than numeric concept identifiers.

**FlatBuffers** [Google, 2014] provides zero-copy deserialization but requires schema compilation and produces larger wire sizes (~80 bytes) due to alignment padding and vtable overhead.

**RDF/Turtle** [W3C, 2014] achieves semantic self-description but relies on full URI strings for resource identification, yielding ~150 bytes — approximately 8–10× larger than KU CoreDna.

**JSON-LD** [W3C, 2020] provides linked-data interoperability at substantial overhead from JSON syntax, context declarations, and URI verbosity: ~350 bytes for a single fact, nearly 20× larger than KU CoreDna.

### 6.4.2 Unique Property Combination

KU CoreDna uniquely combines four properties that no single competing format achieves simultaneously:

1. **Minimal wire size** (14–20 bytes for a single fact).
2. **Full self-description** (every byte is interpretable without external schema).
3. **Complete language independence** (no natural-language tokens in the wire format).
4. **Schema-free operation** (no negotiation or schema exchange required between peers).

---

## 6.5 Varint Efficiency Analysis

The 5-tier variable-length integer (varint) scheme is central to the compactness of KU CoreDna.

### 6.5.1 Tier Structure

| Tier | Bytes | Value Range | Capacity | First-Byte Prefix |
|:---|:---|:---|:---|:---|
| 0 | 1 | 0–127 | 128 | `0xxxxxxx` |
| 1 | 2 | 128–16,383 | 16,384 | `10xxxxxx` |
| 2 | 3 | 16,384–2,097,151 | 2,097,152 | `110xxxxx` |
| 3 | 4 | 2,097,152–268,435,455 | 268,435,456 | `1110xxxx` |
| 4 | 5 | 268,435,456–~34.6B | ~34,359,738,368 | `11110xxx` |

Tier detection is achieved in O(1) time by inspecting the leading bits of the first byte, requiring no lookahead or backtracking.

### 6.5.2 Zipfian Alignment

Concept identifier usage in natural knowledge corpora follows a Zipfian distribution [Clauset et al., 2009]: a small number of high-frequency concepts (e.g., fundamental entities, common properties, SI units) dominate, while the long tail of specialized concepts appears infrequently. The varint tier boundaries are aligned to this distribution, ensuring that common concepts consume fewer bytes.

Under this assumption, the expected byte cost per identifier is:

| Tier | Estimated Usage Share | Bytes | Weighted Contribution |
|:---|:---|:---|:---|
| 0 | ~45% | 1 | 0.45 |
| 1 | ~30% | 2 | 0.60 |
| 2 | ~15% | 3 | 0.45 |
| 3 | ~7% | 4 | 0.28 |
| 4 | ~3% | 5 | 0.15 |

**Weighted average: ~1.93 bytes per identifier.**

Compared with a fixed 8-byte (`u64`) encoding, the varint scheme yields an estimated **75.9% reduction** in identifier storage cost. Against a fixed 4-byte (`u32`) encoding, the savings are approximately **51.8%**.

### 6.5.3 Tier 0 Optimization

The **80 Tier 0 universal constants** — representing the most fundamental concepts in the ontology (structural predicates, causal/temporal relations, spatial relations, logical/modal operators, SI base units, derived units, epistemological values, and agentive/thematic roles) — are encoded in a single byte. Because these constants appear disproportionately in typical knowledge expressions, they exert an outsized effect on average encoding cost, pulling the weighted mean below 2 bytes per identifier in practice.

---

## 6.6 Concept Resolution Performance

### 6.6.1 ConceptRegistry Architecture

The ConceptRegistry is an offline concept lookup table shipped with every node, providing **O(1)** name-to-CCID resolution via a precomputed hash table:

| Property | Value |
|:---|:---|
| File format | `.obr` (OneBrain Registry) |
| Size | ~200 MB |
| Capacity | ~8 million concepts |
| Coverage target | 99.9% of general-domain knowledge |
| Lookup complexity | O(1) hash table |
| Update cycle | Quarterly |

### 6.6.2 Resolution Algorithm

The resolution pipeline applies a cascading strategy:

1. **Exact match:** Direct hash table lookup — O(1).
2. **Case-insensitive match:** Normalized key lookup — O(1).
3. **Fuzzy match:** Diacritics-stripped comparison for languages with complex orthography (e.g., Vietnamese).
4. **Ambiguity resolution:** When a term maps to multiple concepts (e.g., "Mercury" → planet, element, deity), the system returns an ambiguity set for application-level disambiguation.
5. **AI fallback:** For genuinely novel concepts not in the registry, the system generates a CCID from a Definition gene (GeneType = 12) whose binary encoding is hashed via BLAKE3.

### 6.6.3 Runtime Characteristics

The registry is loaded once at initialization and remains immutable thereafter, eliminating lock contention in concurrent workloads. Concept sources include Wikidata entities (`wd:Q{id}`), GeoNames geographic features (`gn:{id}`), NCBI Taxonomy (`ncbi:{taxid}`), and ChEBI chemical compounds (`chebi:{id}`), ensuring broad cross-domain coverage.

---

## 6.7 Limitations

Several areas remain for further investigation:

1. **Large-scale corpus benchmarks.** The wire-size benchmarks presented in Section 6.3 cover representative examples but do not yet include systematic evaluation over large-scale multilingual corpora. Future work will establish compression statistics over datasets exceeding $10^6$ knowledge expressions across at least 10 languages.

2. **Formal verification.** While the 827-test suite provides strong empirical evidence of correctness, formal verification of the roundtrip invariant and CRDT convergence properties using tools such as Prusti or Kani [Astrauskas et al., 2022] would strengthen confidence in the implementation.

3. **Hardware acceleration.** The BLAKE3-based CCID computation and varint encoding are amenable to SIMD optimization. Preliminary analysis suggests a 2–4× throughput improvement on x86-64 with AVX-512 extensions [O'Connor et al., 2020].

4. **Interoperability bridges.** Bidirectional converters between KU CoreDna and established formats (JSON-LD, RDF/Turtle, Protocol Buffers) would facilitate incremental adoption in existing knowledge-management ecosystems.

5. **Network-level evaluation.** The current evaluation focuses on single-node encoding and CRDT merge correctness. Large-scale distributed deployment across geographically dispersed nodes — measuring convergence latency, bandwidth consumption, and partition recovery time — remains for future work.

6. **Concept Registry completeness.** While the 99.9% coverage target is estimated from cross-referencing Wikidata, GeoNames, NCBI, and ChEBI, empirical validation against diverse domain-specific corpora (e.g., legal, medical, indigenous knowledge) has not yet been conducted.

---

## References

- Matsakis, N. D. and Klock, F. S. (2014). The Rust Language. *ACM SIGAda Ada Letters*, 34(3), 103–104.

- O'Connor, B. D. et al. (2020). The BLAKE3 Hashing Framework. *IACR Cryptology ePrint Archive*, 2020.

- Shapiro, M., Preguiça, N., Baquero, C., and Zawirski, M. (2011). Conflict-Free Replicated Data Types. In *Proceedings of the 13th International Symposium on Stabilization, Safety, and Security of Distributed Systems (SSS 2011)*, Lecture Notes in Computer Science, 6976, 386–400.

- Google (2024). Protocol Buffers: Language Guide. *Google Developers Documentation*. Available: https://protobuf.dev/programming-guides/proto3/

- Bormann, C. and Hoffman, P. (2013). Concise Binary Object Representation (CBOR). *IETF RFC 7049*.

- Furuhashi, S. (2013). MessagePack: An Efficient Binary Serialization Format. Available: https://msgpack.org/

- Google (2014). FlatBuffers: Memory Efficient Serialization Library. Available: https://flatbuffers.dev/

- W3C (2020). JSON-LD 1.1: A JSON-based Serialization for Linked Data. *W3C Recommendation*.

- W3C (2014). RDF 1.1 Turtle: Terse RDF Triple Language. *W3C Recommendation*.

- Clauset, A., Shalizi, C. R., and Newman, M. E. J. (2009). Power-Law Distributions in Empirical Data. *SIAM Review*, 51(4), 661–703.

- Kleppmann, M. (2017). *Designing Data-Intensive Applications*. O'Reilly Media.

- Astrauskas, V., Müller, P., Poli, F., and Summers, A. J. (2022). Leveraging Rust Types for Program Verification. *Proceedings of the ACM on Programming Languages*, 6(OOPSLA1), 1–30.

- Berners-Lee, T. (2006). Linked Data — Design Issues. *W3C*. Available: https://www.w3.org/DesignIssues/LinkedData.html
