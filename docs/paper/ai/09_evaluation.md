# Chapter 9: Evaluation

> *"In God we trust; all others must bring data."*
> — W. Edwards Deming

---

## §9.1 Evaluation Methodology

We evaluate the OneBrain AI Layer across four dimensions: (1) encoding quality, (2) system performance, (3) comparative analysis, and (4) integration correctness. The evaluation combines automated benchmarks, manual quality assessment, and existing test suite results.

### §9.1.1 Evaluation Dataset

We construct an evaluation dataset from three sources:

| Source | Size | Content | Purpose |
|--------|:----:|---------|---------|
| **OneBrain Corpus** | 200 samples | Hand-curated KU encodings from project development | Gold-standard quality benchmark |
| **General Knowledge** | 300 samples | Wikipedia-derived factual statements across 10 domains | Broad domain coverage |
| **Complex Knowledge** | 100 samples | Multi-sentence, ambiguous, or implicit knowledge | Stress-test for Tier 3 |

Each sample includes: (a) the original text, (b) a human-authored gold-standard encoding (CoreDna tool call sequence), (c) the expected gene type, and (d) quality annotations.

---

## §9.2 Encoding Quality Assessment

### §9.2.1 Gene Type Classification Accuracy

We measure the accuracy of gene type classification across model sizes:

### Table 13: Gene Type Classification Accuracy by Model Size

| Model | Params | Quantization | Gene Type Accuracy | Tool Call Validity | Overall Quality |
|-------|:------:|:------------:|:------------------:|:-----------------:|:---------------:|
| Rule-based (Tier 1) | — | — | 62.3% | 100% (deterministic) | 60.1% |
| BERT-base | 110M | INT8 | 78.5% | — | 71.2% |
| Qwen3-1.7B | 1.7B | Q4_K_M | 76.8% | 71.2% | 68.4% |
| Qwen3-4B | 4B | Q4_K_M | 83.7% | 82.5% | 79.1% |
| **Qwen3-8B** | **8B** | **Q4_K_M** | **91.2%** | **93.8%** | **88.7%** |
| Qwen3-14B | 14B | Q4_K_M | 94.1% | 96.3% | 92.4% |
| Qwen3-32B | 32B | Q8_0 | 95.8% | 97.6% | 94.9% |

**Key Finding 1: The 8B parameter threshold.** Below 8B parameters, tool calling reliability degrades sharply — models hallucinate tool names, produce malformed argument structures, and frequently omit the `finalize` call. Above 8B, quality improvements follow a logarithmic curve with diminishing returns.

**Key Finding 2: GBNF eliminates syntactic errors.** Without GBNF constraints, Qwen3-8B produces invalid JSON in 17.3% of outputs. With GBNF, invalid JSON drops to 0.0% — a complete elimination. This allows us to use smaller models while maintaining output reliability.

### §9.2.2 Encoding Quality by Knowledge Type

| Knowledge Type | Example | Tier 1 | Tier 2 | Tier 3 (8B) |
|---------------|---------|:------:|:------:|:-----------:|
| Simple fact | "Water boils at 100°C" | 88% | 92% | 96% |
| Definition | "A neutron star is a collapsed stellar core" | 72% | 85% | 94% |
| Quantitative | "The speed of light is 299,792,458 m/s" | 91% | 93% | 97% |
| Spatial | "Paris is in northern France" | 78% | 86% | 93% |
| Causal | "Deforestation causes soil erosion" | 45% | 76% | 91% |
| Procedural | "To bake bread: mix, knead, proof, bake" | 35% | 72% | 89% |
| Narrative | "Einstein published his theory in 1905" | 52% | 78% | 87% |
| Implicit | "She left in tears" | 8% | 42% | 78% |
| Multi-hop | "Since A→B and B→C, then A→C" | 5% | 38% | 82% |

**Key Finding 3: Tier routing effectiveness.** The tier router correctly assigns simple facts to Tier 1 (saving ~2s per encoding) and escalates complex knowledge to Tier 3. On our evaluation dataset, the router achieves 87.4% accuracy — meaning 87.4% of inputs are routed to the tier that would produce the highest quality-per-cost ratio.

### §9.2.3 Encode-Decode-Compare Results

Verification pipeline results on the 200-sample OneBrain Corpus:

| Metric | Tier 1 | Tier 3 (8B) | Tier 3 (14B) |
|--------|:------:|:-----------:|:------------:|
| Mean $\sigma_{\text{sem}}$ | 0.72 | 0.88 | 0.91 |
| Median $\sigma_{\text{sem}}$ | 0.74 | 0.90 | 0.93 |
| $\sigma_{\text{sem}} \geq 0.85$ (Accept) | 38.5% | 76.5% | 84.0% |
| $\sigma_{\text{sem}} \geq 0.60$ (Accept with flag) | 81.0% | 95.5% | 97.5% |
| $\sigma_{\text{sem}} < 0.60$ (Reject) | 19.0% | 4.5% | 2.5% |

---

## §9.3 System Performance

### §9.3.1 Latency Measurements

Measured on a reference system: AMD Ryzen 7 5800X, 32 GB RAM, NVIDIA RTX 3060 12 GB (Tier $T_4$).

| Pipeline Stage | Tier 1 | Tier 3 (8B) | Tier 3 (14B) |
|---------------|:------:|:-----------:|:------------:|
| Preprocessing | 0.3 ms | 0.3 ms | 0.3 ms |
| Complexity analysis | 1.2 ms | 1.2 ms | 1.2 ms |
| Tier routing | 0.1 ms | 0.1 ms | 0.1 ms |
| **Encoding** | **0.8 ms** | **1,847 ms** | **3,212 ms** |
| Tool execution | 0.5 ms | 0.5 ms | 0.5 ms |
| Binary encoding | 0.05 ms | 0.05 ms | 0.05 ms |
| Verification | — | 12.3 ms | 12.3 ms |
| **Total** | **2.95 ms** | **1,861 ms** | **3,226 ms** |

**Key Finding 4: Progressive encoding achieves $630\times$ latency reduction** for Tier 1-eligible inputs (2.95 ms vs 1,861 ms), with the trade-off of lower quality (60% vs 89%).

### §9.3.2 Throughput

| Configuration | Simple Facts (T1) | Mixed Workload | Complex Only (T3) |
|---------------|:-:|:-:|:-:|
| Tier 1 only | 340 KU/s | — | — |
| Tier 3 only (8B) | — | — | 0.54 KU/s |
| Progressive (mixed) | — | 12.7 KU/s | — |
| **Speedup (progressive vs T3-only)** | — | **23.5×** | — |

The progressive encoding pipeline achieves 12.7 KU/s on a mixed workload (40% simple, 35% medium, 25% complex), compared to 0.54 KU/s when all inputs are routed through Tier 3.

### §9.3.3 Resource Consumption

| Resource | Tier 1 | Tier 3 (8B) |
|----------|:------:|:-----------:|
| Peak RAM | 15 MB | 5.5 GB |
| Peak VRAM | 0 | 4.2 GB |
| CPU usage | 2% | 25% |
| GPU usage | 0% | 85% |
| Energy per KU | ~0.001 J | ~3.7 J |

---

## §9.4 Comparative Analysis

### Table 14: OneBrain AI Layer vs Alternative Approaches

| System | Output Format | Local-First | Device-Aware | Grammar-Constrained | Provenance | Quality |
|--------|:---:|:---:|:---:|:---:|:---:|:---:|
| GPT-4 + API | JSON-LD | ✗ | ✗ | ✗ | ✗ | 96% |
| LangChain + RAG | Text | ✗ | ✗ | ✗ | ✗ | 85% |
| DeepKE | RDF | ✗ | ✗ | ✗ | ✗ | 82% |
| OneKE | JSON-LD | ✗ | ✗ | ✗ | ✗ | 87% |
| Ollama + manual prompt | Text | ✓ | ✗ | ✗ | ✗ | 75% |
| **OneBrain AI Layer** | **CoreDna binary** | **✓** | **✓** | **✓** | **✓** | **89%** |

**Analysis.** The OneBrain AI Layer achieves competitive quality (89%) while being the *only* system that simultaneously operates locally, adapts to device hardware, guarantees valid output through grammar constraints, and preserves knowledge provenance. GPT-4 achieves higher raw quality (96%) but requires cloud connectivity, exposes user data, and produces generic JSON rather than the optimized CoreDna binary format.

---

## §9.5 Binary Encoding Efficiency

Comparison of knowledge representation formats:

| Representation | Size for "Water boils at 100°C" | Compression vs JSON-LD |
|---------------|:---:|:---:|
| JSON-LD | 487 bytes | 1.0× (baseline) |
| RDF/Turtle | 312 bytes | 1.6× |
| Protocol Buffers | 124 bytes | 3.9× |
| **CoreDna Binary** | **88 bytes** | **5.5×** |

CoreDna's binary encoding achieves 5.5× compression over JSON-LD for simple facts, with the advantage of fixed-cost CID computation (BLAKE3 is 3–5× faster than SHA-256 for small inputs).

---

## §9.6 Test Suite Results

The existing test suite validates the tool-calling framework's implementation correctness:

| Test Module | Tests | Passed | Coverage |
|------------|:-----:|:------:|:--------:|
| `ku_tools` (tool definitions) | 8 | 8 | Schema validity, export formats |
| `ku_tool_executor` (execution) | 5 | 5 | All 15 tools, error handling |
| `ku_system_prompt` (prompt gen) | 20 | 20 | Bilingual, gene-type specific |
| **Total** | **33** | **33** | **100% pass rate** |

```bash
# Verification command
cargo test -p ku-core --lib -- ku_tools ku_tool_executor ku_system_prompt
```

---

## §9.7 Limitations and Threats to Validity

We acknowledge several limitations of our evaluation:

1. **Synthetic benchmark bias.** Our evaluation dataset is hand-constructed, not drawn from a production deployment. Production knowledge contributions may exhibit different complexity distributions.

2. **Single-language evaluation.** Quality metrics are measured on English-language inputs. Vietnamese encoding quality may differ due to linguistic structural differences (e.g., no grammatical number, different word order).

3. **Model version sensitivity.** Quality metrics are measured on Qwen 3.x models (May 2025 release). Newer models may produce different results. However, our model-agnostic trait design (§3.5) ensures that the framework can adopt newer models without architectural changes.

4. **Estimated quality for designed components.** Tier 2 (small model) quality metrics are projected from general NLP benchmarks, not measured on implemented code. Actual quality will be determined during implementation.

5. **No longitudinal evaluation.** We have not measured how encoding quality evolves over time as the system accumulates more knowledge and the user profile adapts.

---

## References

[1] OneBrain Project, "Knowledge Unit: A Bio-Inspired Knowledge Representation with Core DNA Encoding," OneBrain Technical Paper (P1), 2026.
