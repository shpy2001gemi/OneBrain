# Chapter 2: Related Work

> *"If I have seen further, it is by standing on the shoulders of giants."*
> — Isaac Newton, Letter to Robert Hooke (1675)

---

## §2.1 Overview

The OneBrain AI Layer draws on and extends work from five distinct research traditions: (1) retrieval-augmented generation, (2) knowledge extraction and structuring, (3) on-device AI and edge inference, (4) federated and decentralized learning, and (5) tool-calling and function-calling paradigms. In this chapter, we survey the most relevant contributions from each tradition, identify their limitations in the context of decentralized knowledge networks, and position the OneBrain AI Layer relative to the state of the art.

---

## §2.2 Retrieval-Augmented Generation (RAG)

Retrieval-Augmented Generation (RAG), introduced by Lewis et al. [1], represents the dominant paradigm for grounding LLM outputs in external knowledge. The core idea is elegant: rather than relying solely on the model's parametric memory (which is frozen at training time and prone to hallucination), augment each generation step with relevant documents retrieved from an external corpus. The original RAG architecture combined a BERT-based Dense Passage Retriever (DPR) with a BART-based generator, demonstrating significant improvements on knowledge-intensive NLP tasks.

Subsequent work has refined the RAG paradigm across multiple dimensions:

- **Self-RAG** (Asai et al. [2]) introduces self-reflection tokens that allow the model to decide *when* to retrieve, *what* to retrieve, and *whether* the retrieval was helpful — reducing unnecessary retrieval calls by up to 40%.
- **RAPTOR** (Sarthi et al. [3]) constructs hierarchical tree-structured summaries of documents, enabling retrieval at multiple abstraction levels.
- **Corrective RAG (CRAG)** (Yan et al. [4]) adds a knowledge refinement step that evaluates and filters retrieved documents before generation.
- **Graph RAG** (Microsoft, Edge et al. [5]) applies community detection algorithms to knowledge graphs and generates hierarchical summaries, enabling RAG over structured graph data.
- **Hybrid RAG** combines dense retrieval (embedding similarity) with sparse retrieval (BM25/TF-IDF) for improved recall across diverse query types [6].

**Limitations for Decentralized Knowledge.** Despite their effectiveness, existing RAG systems share three assumptions that make them unsuitable for decentralized knowledge networks:

1. **Centralized vector store.** All production RAG systems — including LangChain [7], LlamaIndex [8], and Haystack [9] — assume a single, centralized vector database (Pinecone, Weaviate, Milvus, Chroma) that holds the entire document corpus. In a decentralized network, knowledge is distributed across thousands of autonomous peers with no central index.

2. **Document-level chunking.** RAG systems chunk documents into fixed-size passages (typically 256–512 tokens) for embedding and retrieval. This chunking is fundamentally incompatible with Knowledge Units, which are already *atomic* knowledge representations — each KU encodes exactly one idea with explicit structure, not a passage of text.

3. **No epistemic grading.** Retrieved passages are ranked by embedding similarity alone. There is no mechanism to distinguish a passage from a peer-reviewed meta-analysis ($\text{EpistemicStatus} = \text{PeerReviewed}$) from an anonymous blog post ($\text{EpistemicStatus} = \text{Rumor}$).

**OneBrain's Position.** The Personal AI Mediator (§7) extends the RAG paradigm in three ways: (a) it replaces centralized vector stores with **distributed KU retrieval** via Kademlia DHT + Vacuum Bloom filters [13]; (b) it eliminates document chunking by treating each KU as a self-contained retrieval unit; and (c) it augments similarity-based ranking with **trust-weighted epistemic scoring** from the PoMV framework [15].

---

## §2.3 Knowledge Extraction and Structuring

The task of automatically extracting structured knowledge from unstructured text has been studied extensively under various names: information extraction [10], knowledge base population [11], relation extraction [12], and knowledge graph construction [13].

### §2.3.1 Classical NLP Approaches

Early knowledge extraction systems relied on hand-crafted patterns and rules:

- **NELL** (Never-Ending Language Learning) [14] uses a coupled semi-supervised approach with hundreds of hand-crafted extraction patterns, continuously reading the web since 2010.
- **OpenIE** systems (TextRunner [15], ReVerb [16], OLLIE [17]) extract open-domain relation triples using syntactic patterns and dependency parsing.
- **DeepDive** [18] combines probabilistic inference with distant supervision to extract relations from text, achieving near-human accuracy on specific domains.

### §2.3.2 Neural Approaches

The advent of pre-trained language models transformed knowledge extraction:

- **BERT-based extractors** (SpanBERT [19], KnowBert [20]) achieve state-of-the-art performance on entity recognition and relation classification tasks.
- **GPT-based extraction** (Wadhwa et al. [21]) demonstrates that LLMs can perform zero-shot relation extraction with competitive accuracy when provided with appropriate prompts.
- **Fine-tuned T5/BART models** (GENRE [22], GenIE [23]) generate structured outputs directly from text using sequence-to-sequence architectures.
- **LLM-as-Knowledge-Extractor** (Wei et al. [24]) shows that chain-of-thought prompting can decompose complex knowledge extraction into multi-step reasoning.

### §2.3.3 Knowledge Graph Construction Pipelines

End-to-end knowledge graph construction pipelines combine multiple extraction components:

- **RECON** [25] integrates named entity recognition, entity linking, and relation extraction in a single neural architecture.
- **DeepKE** [26] provides a unified framework for knowledge extraction supporting both supervised and few-shot learning.
- **OneKE** [27] uses LLMs with carefully designed prompts for multi-task knowledge extraction including NER, RE, and event extraction.

**Limitations for Decentralized Knowledge.** Existing knowledge extraction systems produce RDF triples or JSON-LD structured data — formats designed for centralized databases. None produce binary-encoded, self-describing representations with built-in provenance, epistemic status, trust scoring, and CRDT-compatible fields. Furthermore, existing systems assume access to cloud-hosted models and have no mechanism for hardware-adaptive model selection.

**OneBrain's Position.** Our three-tier encoding pipeline (§4) extends neural knowledge extraction by (a) producing **CoreDna binary** rather than RDF/JSON-LD, (b) preserving provenance through author DID and evidence type fields, (c) assigning epistemic status based on source analysis, and (d) routing extraction complexity to appropriate hardware tiers.

---

## §2.4 On-Device AI and Edge Inference

The deployment of large language models on consumer hardware has rapidly evolved from an academic curiosity to a practical reality, driven by advances in model quantization, efficient inference engines, and hardware acceleration.

### §2.4.1 Quantization and Compression

Model quantization reduces the precision of neural network weights to fit larger models into limited memory:

- **GPTQ** (Frantar et al. [28]) applies one-shot weight quantization using approximate second-order methods, compressing 175B-parameter models to 3–4 bits with minimal quality loss.
- **AWQ** (Lin et al. [29]) protects salient weight channels from quantization, achieving better quality preservation than GPTQ at equivalent compression ratios.
- **GGML/GGUF** (Gerganov [30]) introduces a file format and quantization scheme specifically designed for CPU inference, with 17 quantization variants from Q2_K (2-bit) to F32 (32-bit). The **Q4_K_M** variant has emerged as the community consensus sweet spot, preserving ~95% of FP16 quality while enabling 7B-parameter models to run on devices with as little as 6 GB RAM.
- **QLoRA** (Dettmers et al. [31]) enables fine-tuning of quantized models with minimal additional memory, opening the door to on-device model adaptation.

### §2.4.2 Inference Engines

Multiple inference engines compete for the local AI runtime space:

- **llama.cpp** [30] — C/C++ inference engine optimized for CPU execution with SIMD (AVX2, AVX-512, ARM NEON), GPU offloading (CUDA, Metal, Vulkan), and the GGUF model format. The de facto standard for local LLM inference with over 70,000 GitHub stars.
- **Ollama** [32] — REST API wrapper over llama.cpp that simplifies model management with Docker-like `ollama pull/run` semantics. Provides a standardized `/api/chat` endpoint compatible with OpenAI's API format.
- **Candle** [33] — Pure Rust ML framework by Hugging Face, offering zero-copy tensor operations, WebGPU support, and native Rust type safety. Uniquely suited for integration with Rust-native systems like OneBrain.
- **ONNX Runtime** [34] — Microsoft's cross-platform inference engine supporting multiple hardware backends (CPU, CUDA, DirectML, TensorRT, CoreML). Provides model interoperability through the ONNX standard.
- **vLLM** [35] — Python-based serving engine introducing PagedAttention for efficient KV-cache management, achieving 2–4× throughput improvements over vanilla Hugging Face inference.
- **MLC-LLM** [36] — Universal deployment framework using Apache TVM for compilation-optimized inference on smartphones, browsers (WebGPU), and edge devices.

### §2.4.3 Structured Output Generation

A critical capability for knowledge encoding is the ability to force LLMs to produce output conforming to a predefined schema:

- **GBNF (GGML BNF)** grammars in llama.cpp use finite-state machines to mask invalid tokens at the logit level during decoding, making schema-violating output *impossible* rather than merely *unlikely* [30].
- **Outlines** [37] compiles JSON Schema or regular expressions into finite-state automata that constrain the sampling process.
- **Instructor** [38] uses Pydantic models to enforce structured output from LLMs, with automatic retry on validation failure.
- **OpenAI Function Calling** [39] defines tool schemas in JSON Schema format and relies on the model's instruction-following capability (soft constraint, not guaranteed).

**Limitations for Decentralized Knowledge.** Existing on-device AI frameworks focus on *running* models efficiently but provide no mechanism for *selecting* models based on available hardware, *distributing* model weights across a P2P network, or *tracking* AI resource consumption for incentive systems.

**OneBrain's Position.** The AI Runtime and Model Management components (§6) extend on-device inference by introducing (a) a **pluggable `ModelBackend` trait** that abstracts over Ollama, Candle, and ONNX backends; (b) a **7-tier device classification** ($T_0$–$T_6$) with automatic model selection; (c) **CID-based model distribution** via Kademlia DHT; and (d) **GBNF grammar integration** for guaranteed-valid KU output.

---

## §2.5 Federated and Decentralized Learning

Federated Learning (FL), introduced by McMahan et al. [40], enables multiple devices to collaboratively train a shared model without exchanging raw data — communicating only model updates (gradients or parameter deltas). This privacy-preserving paradigm has seen extensive adoption:

- **FedAvg** [40] — The original federated averaging algorithm: each client trains locally for $E$ epochs, then sends model updates to a central server for aggregation.
- **FedProx** [41] — Addresses heterogeneous data distributions by adding a proximal term to the local objective, improving convergence on non-IID data.
- **Personalized FL** (Per-FedAvg [42], pFedMe [43]) — Allows each client to maintain a personalized model layer while sharing a global backbone.
- **Decentralized FL** (D-PSGD [44], GossipFL [45]) — Eliminates the central server by having peers exchange updates directly via gossip protocols.

### §2.5.1 Federated Knowledge Graph Learning

The intersection of federated learning and knowledge graphs has received growing attention:

- **FedE** [46] — Federates knowledge graph embedding training across multiple knowledge graph owners, sharing only embedding updates while keeping raw triples private.
- **FKGE** [47] — Uses adversarial generation to create synthetic triples that augment each client's local graph, improving embedding quality without sharing real data.
- **FedEC** [48] — Combines federated embedding with entity-level contrastive learning for cross-silo knowledge graph completion.

**Limitations for Decentralized Knowledge.** Existing federated KG learning systems assume (a) a central aggregation server, (b) homogeneous model architectures across clients, and (c) the existence of pre-structured knowledge graphs as input. None address the upstream problem of *creating* structured knowledge from unstructured text in a federated manner.

**OneBrain's Position.** The AI Layer complements the existing FedR protocol (P7, OBKG) [17] by providing the **upstream encoding capability** that feeds structured Knowledge Units into the federated knowledge graph. Additionally, the model distribution system (§6.7) enables decentralized sharing of model weights — a form of *model federation* rather than *gradient federation*.

---

## §2.6 Tool-Calling and Function-Calling Paradigms

The ability of LLMs to interact with external systems through structured tool calls has emerged as a critical capability for agentic AI:

- **Toolformer** (Schick et al. [49]) demonstrated that LLMs can be fine-tuned to generate API calls within their output, learning when and how to invoke external tools through self-supervised training on the LLM's own generated examples.
- **Gorilla** (Patil et al. [50]) fine-tuned LLaMA on a large corpus of API documentation, achieving state-of-the-art accuracy on tool-use benchmarks while reducing hallucinated API calls.
- **ToolBench** (Qin et al. [51]) introduced a comprehensive benchmark with 16,000+ real-world APIs across 49 categories, along with the ToolLLaMA model trained on this data.
- **ReAct** (Yao et al. [52]) interleaves reasoning traces with action execution, enabling LLMs to plan, execute tools, observe results, and adapt their strategy — a pattern directly relevant to multi-step KU encoding.
- **OpenAI Function Calling** [39] established the de facto standard API format for tool calling: the model receives tool definitions as JSON Schema, generates structured tool calls in its response, and the client executes them and returns results.

### §2.6.1 Tool Calling for Knowledge Tasks

Several works have applied tool calling specifically to knowledge-related tasks:

- **ChatKBQA** (Luo et al. [53]) uses LLMs to generate SPARQL queries as tool calls for knowledge base question answering.
- **KnowAgent** (Zhu et al. [54]) equips LLMs with knowledge graph search tools for multi-hop reasoning.
- **StructGPT** (Jiang et al. [55]) provides LLMs with structured data interfaces (tables, KGs, databases) through tool calling.

**Limitations for Decentralized Knowledge.** Existing tool-calling frameworks focus on *querying* existing knowledge bases, not *creating* new structured knowledge. Furthermore, they assume cloud-hosted models with unlimited context windows, and provide no mechanism for grammar-constrained output that guarantees schema conformance.

**OneBrain's Position.** Our tool-calling framework (§5) inverts the typical direction: instead of using tools to *read* from knowledge bases, we use tools to *write* into the Knowledge Unit store. The 15 tools (`new_ku`, `finalize`, `lookup`, `lookup_or_create`, `add_triple`, `add_part_of`, `add_quality`, `add_quantity`, `add_tolerance`, `add_enum_val`, `add_causal`, `add_located`, `add_step`, `set_certainty`, `set_difficulty`) collectively define a **knowledge encoding DSL** that any tool-calling-capable LLM can speak. Combined with GBNF grammar constraints, this achieves *guaranteed-valid* structured output — a property no existing tool-calling framework provides.

---

## §2.7 Summary and Positioning

### Table 3: Related Work Feature Matrix

| Feature | RAG Systems | KG Construction | On-Device AI | Federated KG | Tool-Calling | **OneBrain AI** |
|:---|:---:|:---:|:---:|:---:|:---:|:---:|
| **Decentralized** | ✗ | ✗ | ⚠️ | ⚠️ | ✗ | ✓ |
| **Local-first** | ✗ | ✗ | ✓ | ⚠️ | ✗ | ✓ |
| **Binary KU output** | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **Provenance preserved** | ⚠️ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **Epistemic grading** | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **Device-adaptive** | ✗ | ✗ | ⚠️ | ✗ | ✗ | ✓ |
| **Grammar-constrained** | ✗ | ✗ | ⚠️ | ✗ | ✗ | ✓ |
| **Model-agnostic** | ⚠️ | ✗ | ⚠️ | ✗ | ✓ | ✓ |
| **P2P model sharing** | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **Round-trip verification** | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **Privacy-preserving** | ✗ | ✗ | ✓ | ✓ | ✗ | ✓ |
| **Incentive-integrated** | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |

*Legend: ✓ = full support; ⚠️ = partial/limited support; ✗ = not supported.*

**Observation 1: No existing system addresses knowledge *creation* in a decentralized, device-aware manner.** While RAG systems retrieve knowledge and KG construction systems extract it, none produce compact binary-encoded knowledge representations with built-in provenance, epistemic grading, and CRDT compatibility — the properties required for a decentralized knowledge network.

**Observation 2: Grammar-constrained generation for knowledge encoding is unexplored.** Although GBNF and similar techniques have been applied to JSON Schema conformance in general-purpose settings, no prior work has applied grammar-constrained decoding specifically to the problem of producing valid structured knowledge representations. The combination of tool calling + GBNF constraints constitutes a novel approach to guaranteed-valid knowledge encoding.

**Observation 3: The intersection of on-device AI and knowledge graph construction is largely vacant.** On-device AI research focuses on inference efficiency, while KG construction research assumes cloud compute. The AI Layer bridges this gap by bringing knowledge extraction to the edge while maintaining quality through progressive tier escalation.

---

## References

[1] P. Lewis et al., "Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks," in *Proc. NeurIPS*, 2020.

[2] A. Asai et al., "Self-RAG: Learning to Retrieve, Generate, and Critique through Self-Reflection," arXiv preprint arXiv:2310.11511, 2023.

[3] P. Sarthi et al., "RAPTOR: Recursive Abstractive Processing for Tree-Organized Retrieval," arXiv preprint arXiv:2401.18059, 2024.

[4] S. Yan et al., "Corrective Retrieval Augmented Generation," arXiv preprint arXiv:2401.15884, 2024.

[5] D. Edge et al., "From Local to Global: A Graph RAG Approach to Query-Focused Summarization," arXiv preprint arXiv:2404.16130, 2024.

[6] W. X. Zhao et al., "Dense Text Retrieval Based on Pretrained Language Models: A Survey," ACM Computing Surveys, vol. 56, no. 9, 2024.

[7] H. Chase, "LangChain: Building Applications with LLMs through Composability," [Online]. Available: https://github.com/langchain-ai/langchain, 2022.

[8] J. Liu, "LlamaIndex: A Data Framework for LLM Applications," [Online]. Available: https://github.com/run-llama/llama_index, 2022.

[9] deepset, "Haystack: LLM Orchestration Framework," [Online]. Available: https://github.com/deepset-ai/haystack, 2020.

[10] R. Grishman, "Information Extraction: Capabilities and Challenges," Lecture Notes in Computer Science, 2012.

[11] H. Ji and R. Grishman, "Knowledge Base Population: Successful Approaches and Challenges," in *Proc. ACL*, 2011.

[12] S. Pawar, G. K. Palshikar, and P. Bhattacharyya, "Relation Extraction: A Survey," arXiv preprint arXiv:1712.05191, 2017.

[13] OneBrain Project, "OneBrain Protocol: A Bio-Inspired 9-Layer P2P Network Stack," OneBrain Technical Paper (P2), 2026.

[14] T. Mitchell et al., "Never-Ending Learning," in *Proc. AAAI*, 2015.

[15] M. Banko et al., "Open Information Extraction from the Web," in *Proc. IJCAI*, 2007.

[16] A. Fader, S. Soderland, and O. Etzioni, "Identifying Relations for Open Information Extraction," in *Proc. EMNLP*, 2011.

[17] M. Schmitz et al., "Open Language Learning for Information Extraction," in *Proc. EMNLP-CoNLL*, 2012.

[18] C. De Sa et al., "DeepDive: Declarative Knowledge Base Construction," ACM SIGMOD Record, vol. 45, no. 1, 2016.

[19] M. Joshi et al., "SpanBERT: Improving Pre-training by Representing and Predicting Spans," TACL, vol. 8, 2020.

[20] M. E. Peters et al., "Knowledge Enhanced Contextual Word Representations," in *Proc. EMNLP*, 2019.

[21] S. Wadhwa, S. Amir, and B. Wallace, "Revisiting Relation Extraction in the Era of Large Language Models," in *Proc. ACL*, 2023.

[22] N. De Cao et al., "Autoregressive Entity Retrieval," in *Proc. ICLR*, 2021.

[23] M. Josifoski et al., "GenIE: Generative Information Extraction," in *Proc. NAACL*, 2022.

[24] J. Wei et al., "Chain-of-Thought Prompting Elicits Reasoning in Large Language Models," in *Proc. NeurIPS*, 2022.

[25] A. Bastos et al., "RECON: Relation Extraction using Knowledge Graph Context," in *Proc. WWW*, 2021.

[26] N. Zhang et al., "DeepKE: A Deep Learning Based Knowledge Extraction Toolkit for Knowledge Base Population," in *Proc. EMNLP (Demo)*, 2022.

[27] Y. Xiao et al., "OneKE: A Dockerized Schema-Guided LLM Agent for Knowledge Extraction," arXiv preprint arXiv:2412.20005, 2024.

[28] E. Frantar et al., "GPTQ: Accurate Post-Training Quantization for Generative Pre-trained Transformers," in *Proc. ICLR*, 2023.

[29] J. Lin et al., "AWQ: Activation-aware Weight Quantization for LLM Compression and Acceleration," in *Proc. MLSys*, 2024.

[30] G. Gerganov, "llama.cpp: LLM Inference in C/C++," [Online]. Available: https://github.com/ggerganov/llama.cpp, 2023.

[31] T. Dettmers et al., "QLoRA: Efficient Finetuning of Quantized Language Models," in *Proc. NeurIPS*, 2023.

[32] Ollama, "Ollama: Get up and running with large language models," [Online]. Available: https://ollama.com, 2023.

[33] L. Delemotte et al., "Candle: Minimalist ML Framework for Rust," Hugging Face, [Online]. Available: https://github.com/huggingface/candle, 2023.

[34] ONNX Runtime Team, "ONNX Runtime: Cross-platform Machine Learning Inference," Microsoft, [Online]. Available: https://onnxruntime.ai, 2019.

[35] W. Kwon et al., "Efficient Memory Management for Large Language Model Serving with PagedAttention," in *Proc. SOSP*, 2023.

[36] MLC Team, "MLC-LLM: Universal LLM Deployment Engine," [Online]. Available: https://llm.mlc.ai, 2023.

[37] B. Willard and R. Louf, "Efficient Guided Generation for Large Language Models," arXiv preprint arXiv:2307.09702, 2023.

[38] J. Liu, "Instructor: Structured Outputs for LLMs," [Online]. Available: https://github.com/jxnl/instructor, 2023.

[39] OpenAI, "Function Calling and Other API Updates," OpenAI Blog, 2023.

[40] B. McMahan et al., "Communication-Efficient Learning of Deep Networks from Decentralized Data," in *Proc. AISTATS*, 2017.

[41] T. Li et al., "Federated Optimization in Heterogeneous Networks," in *Proc. MLSys*, 2020.

[42] A. Fallah, A. Mokhtari, and A. Ozdaglar, "Personalized Federated Learning with Moreau Envelopes," in *Proc. NeurIPS*, 2020.

[43] C. T. Dinh et al., "Personalized Federated Learning with Moreau Envelopes," in *Proc. NeurIPS*, 2020.

[44] X. Lian et al., "Can Decentralized Algorithms Outperform Centralized Algorithms? A Case Study for Decentralized Parallel Stochastic Gradient Descent," in *Proc. NeurIPS*, 2017.

[45] Z. Li et al., "GossipFL: A Gossip-Based Federated Learning Framework," in *Proc. ICDCS*, 2022.

[46] H. Chen et al., "FedE: Embedding Knowledge Graphs in Federated Setting," in *Proc. IJCKG*, 2021.

[47] M. Peng et al., "Differentially Private Federated Knowledge Graph Embedding," in *Proc. CIKM*, 2021.

[48] Z. Chen et al., "Federated Knowledge Graph Completion with Embedding-Contrastive Learning," in *Proc. AAAI*, 2023.

[49] T. Schick et al., "Toolformer: Language Models Can Teach Themselves to Use Tools," in *Proc. NeurIPS*, 2023.

[50] S. G. Patil et al., "Gorilla: Large Language Model Connected with Massive APIs," arXiv preprint arXiv:2305.15334, 2023.

[51] Y. Qin et al., "ToolLLM: Facilitating Large Language Models to Master 16000+ Real-world APIs," in *Proc. ICLR*, 2024.

[52] S. Yao et al., "ReAct: Synergizing Reasoning and Acting in Language Models," in *Proc. ICLR*, 2023.

[53] H. Luo et al., "ChatKBQA: A Generate-then-Retrieve Framework for Knowledge Base Question Answering," in *Proc. ACL Findings*, 2024.

[54] P. Zhu et al., "KnowAgent: Knowledge-Augmented Planning for LLM-Based Agents," arXiv preprint arXiv:2403.03101, 2024.

[55] J. Jiang et al., "StructGPT: A General Framework for Large Language Model to Reason over Structured Data," in *Proc. EMNLP*, 2023.

[15] OneBrain Project, "Proof-of-Metabolic-Value: An Observation-Based Consensus Mechanism," OneBrain Technical Paper (P4), 2026.
