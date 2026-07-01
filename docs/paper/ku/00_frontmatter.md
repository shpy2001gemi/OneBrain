# Knowledge Unit: A Bio-Inspired Knowledge Representation with Core DNA Encoding for Decentralized Knowledge Networks

---

**Authors:** OneBrain Project Contributors  
**Contact:** shpy2001@gmail.com  
**Affiliation:** OneBrain Open Source Project  
**Date:** June 2026  
**Version:** 2.0 (v6 Core DNA — Preprint)

---

## Abstract

The fragmentation of human knowledge across individual minds, languages, geographies, and temporal boundaries remains one of the most significant unsolved problems in knowledge engineering. Existing knowledge management systems — from centralized encyclopedias such as Wikipedia to proprietary knowledge graphs like Google's Knowledge Graph — suffer from a fundamental set of limitations: they lack epistemic metadata describing *how well* a piece of knowledge is established, they provide no economic incentive layer for knowledge contribution, and they impose centralized control that creates single points of failure and censorship vulnerability. Meanwhile, artificial intelligence systems routinely share learned representations across network boundaries with near-zero latency, a capability that remains unavailable to human knowledge contributors.

This paper introduces the **Knowledge Unit (KU)**, a bio-inspired knowledge representation that serves as the atomic unit of a fully decentralized knowledge-sharing network called OneBrain. Drawing a deep structural analogy from molecular biology, the KU architecture organizes knowledge into a **3-layer model**: (1) **Core DNA** — a compact binary instruction stream that is always persisted; (2) **Epigenetics** — rich runtime metadata (trust, bonds, metabolism) that emerges from network interaction; and (3) **Expression** — natural language rendering generated on demand.

The Core DNA layer encodes knowledge using a custom binary format with **32 opcodes** and variable-length operands, achieving language-agnostic representation through numeric ConceptIDs with a novel 5-tier variable-length integer encoding. The instruction set covers 30 typed knowledge primitives organized across 7 categories: Relationship (Triple, PartOf, Quality, Quantity, Tolerance, Range, EnumVal, Formula), Procedural (Step, Precond, Effect, Tool, Duration), Causal-Spatial (Causal, Temporal, Located, SpatialRel), Meta (Certainty, Difficulty, Importance, Context, Source, Timestamp), Experiential (Affect, Sensory, Witness), Structural (Analogy, Contrast, Example, Composite, Constraint), and Control (End, Nop). Each instruction is encoded as a single opcode byte followed by varint-encoded operands, producing a wire format of `MAGIC(1B) | VER_META(1B) | INSTRUCTIONS(var) | END(1B) | CRC-16(2B)`.

The Epigenetics layer preserves the full richness of the epistemic framework — 11 levels of knowledge maturity (from `Rumor` to `Axiomatic`), 9 evidence types aligned with the Cochrane/GRADE hierarchy, 33 directed bond types, and 16-bit error susceptibility flags — as runtime-only CBOR overlays that are not persisted in the primary wire format.

A **3-tier encoding pipeline** converts natural language text into Core DNA: **Tier 1** applies rule-based pattern matching (offline, no AI required, ~60–70% accuracy); **Tier 2** employs a local AI model via 15 JSON-schema function-calling tools (pluggable runtime — Gemma 4, Qwen, Phi-3, or any model supporting tool calling); **Tier 3** verifies encoding fidelity through a distributed Encoding Consensus Protocol with a 4-state lifecycle (RAW → SELF → PART → FULL), 2-phase verification, and weighted consensus scoring.

The KU integrates five Conflict-Free Replicated Data Type (CRDT) primitives — G-Counter, PN-Counter, LWW-Register, OR-Set, and Vector Clock — enabling fully decentralized eventual consistency without any coordination or consensus protocol.

The Core DNA wire format achieves approximately **16 bytes** for a minimal Fact-type KU — a **16.5× reduction** from the prior CBOR-based v5 format (~264 bytes). Real-world benchmarks demonstrate that Core DNA is consistently **smaller than the original natural-language text**: a Vietnamese breaststroke swimming description (323 bytes UTF-8) encodes to **88 bytes** across 3 KUs (3.7× compression), while a comprehensive rocket systems description (1,078 bytes) encodes to **172 bytes** across 5 KUs (6.3× compression). Backward compatibility with the prior v4/v5 CBOR format is maintained through automatic wire format detection via magic byte inspection.

The reference implementation, written in Rust, comprises approximately **10,000+ lines of code** across 27 modules with **267 unit and integration tests**, covering Core DNA encode/decode roundtrips, bridge conversion, text parser patterns, AI tool executor workflows, CRDT merge semantics, varint boundary conditions, epistemic engine computations, and metabolic value scoring.

We present eight novel contributions: (1) a bio-inspired 3-layer knowledge representation (Core DNA / Epigenetics / Expression) with deep structural parallels to DNA encoding and gene expression; (2) a custom binary instruction set with 32 opcodes achieving wire sizes consistently smaller than natural-language text; (3) a semantically-tiered 5-tier variable-length integer scheme for concept identifiers; (4) integration of 5 CRDT types for coordination-free decentralized consistency; (5) a content-agnostic epistemic framework spanning 11 maturity levels with error susceptibility tracking; (6) a 3-tier encoding pipeline from rule-based text parsing through AI function calling to distributed Encoding Consensus with 2-phase verification and OBT token rewards; (7) backward-compatible wire format evolution via automatic format detection; and (8) a fully open-source Rust implementation with comprehensive test coverage.

**Keywords:** knowledge representation, decentralized systems, bio-inspired computing, Core DNA, opcode instruction set, CRDT, conflict-free replicated data types, knowledge graph, epistemic metadata, variable-length encoding, function calling, AI encoding, Rust, content-addressable storage, knowledge unit, OneBrain
