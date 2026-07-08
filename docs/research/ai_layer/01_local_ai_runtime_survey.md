# Local AI Runtime Survey — OneBrain Pillar 6

> **Research Date:** July 2026  
> **Author:** OneBrain Research Team  
> **Status:** Research Complete — Ready for Architecture Decision  
> **Scope:** Local-only AI model runtimes for encoding human knowledge into structured Knowledge Units (KU)

---

## Executive Summary

OneBrain requires a **local-only, no-cloud-dependency AI inference layer** that runs across diverse hardware (mobile → server), integrates with the existing Tokio async runtime, and supports structured output generation for Knowledge Unit encoding. This survey evaluates four runtime approaches, the GGUF model format, tool/function calling capabilities, embedding models, and the Rust AI ecosystem.

### Key Findings

| Criteria | Recommended Approach |
|:---|:---|
| **Primary Runtime** | **Ollama** (process-based, via REST API) for Phase 1; **Candle/mistral.rs** (in-process) for Phase 2 |
| **Model Format** | **GGUF** with Q4_K_M quantization as the default |
| **Structured Output** | Grammar-constrained generation (GBNF) or JSON schema enforcement |
| **Tool Calling** | Qwen3 or Llama 3.x family models (7B+ for reliability) |
| **Embeddings** | Dedicated embedding model (nomic-embed-text) — NOT from chat model |
| **Rust Integration** | `reqwest` for HTTP API; `candle` / `mistral.rs` for future in-process |

### Recommended Architecture

```text
┌─────────────────────────────────────────────┐
│              OneBrain Node                   │
│                                              │
│  ┌──────────────────────────────────────┐   │
│  │      ModelBackend Trait (Rust)        │   │
│  │  ┌────────┐  ┌────────┐  ┌────────┐ │   │
│  │  │ Ollama  │  │Candle/ │  │ ONNX   │ │   │
│  │  │Backend  │  │mistral │  │Backend │ │   │
│  │  │ (HTTP)  │  │(native)│  │(native)│ │   │
│  │  └────┬───┘  └────┬───┘  └────┬───┘ │   │
│  └───────┼───────────┼───────────┼──────┘   │
│          │           │           │            │
│    ┌─────▼───┐  ┌────▼────┐  ┌──▼─────┐    │
│    │ Ollama  │  │ GGUF    │  │ ONNX   │    │
│    │ Server  │  │ Model   │  │ Model  │    │
│    │(process)│  │(in-proc)│  │(in-proc│    │
│    └─────────┘  └─────────┘  └────────┘    │
└─────────────────────────────────────────────┘
```

---

## Table of Contents

1. [Ollama](#1-ollama)
2. [llama.cpp / llama-server](#2-llamacpp--llama-server)
3. [Candle (HuggingFace Rust ML Framework)](#3-candle-huggingface-rust-ml-framework)
4. [ONNX Runtime](#4-onnx-runtime)
5. [GGUF Format Deep Dive](#5-gguf-format-deep-dive)
6. [Tool/Function Calling in Local Models](#6-toolfunction-calling-in-local-models)
7. [Embedding Models for Local Use](#7-embedding-models-for-local-use)
8. [Rust Ecosystem for AI](#8-rust-ecosystem-for-ai)
9. [Runtime Comparison Matrix](#9-runtime-comparison-matrix)
10. [Recommendations for OneBrain](#10-recommendations-for-onebrain)
11. [Risk Assessment](#11-risk-assessment)

---

## 1. Ollama

### 1.1 Overview

Ollama is a local LLM server that wraps llama.cpp in a user-friendly daemon with Docker-inspired model management. It listens on port `11434` by default and provides two API surfaces: a native API (`/api/*`) and an OpenAI-compatible API (`/v1/*`).

**Current State (mid-2026):**
- Mature, production-stable project with massive community
- Built on llama.cpp as the inference backend
- ~8–14% performance overhead vs raw llama.cpp (the "convenience tax")
- Cross-platform: Windows, macOS, Linux
- Now also supports cloud-hosted models via `:cloud` suffixes (irrelevant for OneBrain)

### 1.2 REST API Specification

#### Native API Endpoints

| Endpoint | Method | Purpose |
|:---|:---|:---|
| `/api/generate` | POST | Single-turn text completion |
| `/api/chat` | POST | Multi-turn conversation (chat) |
| `/api/embed` | POST | Generate embeddings |
| `/api/pull` | POST | Download a model |
| `/api/push` | POST | Push a model to registry |
| `/api/tags` | GET | List local models |
| `/api/show` | POST | Show model information/metadata |
| `/api/delete` | DELETE | Delete a model |

#### OpenAI-Compatible API Endpoints

| Endpoint | Method | Purpose |
|:---|:---|:---|
| `/v1/chat/completions` | POST | Chat completions (OpenAI format) |
| `/v1/embeddings` | POST | Embeddings (OpenAI format) |
| `/v1/models` | GET | List models |

#### Chat Request Example

```bash
curl http://localhost:11434/api/chat -d '{
  "model": "qwen3:8b",
  "messages": [
    {"role": "system", "content": "You are a knowledge encoder."},
    {"role": "user", "content": "Encode this fact: Water boils at 100°C at sea level."}
  ],
  "stream": false,
  "format": {
    "type": "object",
    "properties": {
      "title": {"type": "string"},
      "content": {"type": "string"},
      "category": {"type": "string"},
      "confidence": {"type": "number"}
    },
    "required": ["title", "content", "category", "confidence"]
  }
}'
```

#### Chat Response Format

```json
{
  "model": "qwen3:8b",
  "created_at": "2026-07-07T02:00:00.000Z",
  "message": {
    "role": "assistant",
    "content": "{\"title\":\"Boiling Point of Water\",\"content\":\"Water boils at 100 degrees Celsius (212°F) at standard atmospheric pressure at sea level.\",\"category\":\"physics/thermodynamics\",\"confidence\":0.98}"
  },
  "done": true,
  "total_duration": 1234567890,
  "load_duration": 100000000,
  "prompt_eval_count": 45,
  "prompt_eval_duration": 200000000,
  "eval_count": 82,
  "eval_duration": 900000000
}
```

> **Note:** All durations in the Ollama API are returned in **nanoseconds**.

#### Embedding Request/Response

```bash
# Request
curl http://localhost:11434/api/embed -d '{
  "model": "nomic-embed-text",
  "input": "Water boils at 100 degrees Celsius"
}'

# Response
{
  "embeddings": [
    [-0.012345, 0.023456, 0.098765, ...]
  ]
}
```

### 1.3 Tool/Function Calling

Ollama supports native tool calling in both `/api/chat` and `/v1/chat/completions` endpoints.

**How it works:**
1. Client defines `tools` array with JSON Schema parameter definitions
2. Model analyzes the prompt and decides whether to call a tool
3. If tool call needed, model returns `tool_calls` array instead of text
4. Client executes the function and sends result back with `role: "tool"`
5. Model generates final response using tool output

**Tool Call Request:**
```json
{
  "model": "qwen3:8b",
  "messages": [{"role": "user", "content": "Classify this knowledge: 'Photosynthesis converts light to energy'"}],
  "tools": [{
    "type": "function",
    "function": {
      "name": "classify_knowledge",
      "description": "Classify a piece of knowledge into a category",
      "parameters": {
        "type": "object",
        "properties": {
          "category": {"type": "string", "enum": ["science", "math", "history", "art"]},
          "subcategory": {"type": "string"},
          "confidence": {"type": "number", "minimum": 0, "maximum": 1}
        },
        "required": ["category", "subcategory", "confidence"]
      }
    }
  }],
  "stream": false
}
```

**Models with best tool calling support:** Llama 3.1/3.2/3.3, Qwen 2.5/3.x, Mistral-Nemo

### 1.4 Model Management Internals

Ollama uses a **content-addressed storage system** inspired by Docker:

```
~/.ollama/models/
├── blobs/                          # Raw data chunks (GGUF weights)
│   ├── sha256-abc123...            # Named by SHA-256 hash
│   └── sha256-def456...            # Deduplicated across models
└── manifests/                      # Blueprints to assemble models
    └── registry.ollama.ai/
        └── library/
            └── qwen3/
                └── 8b              # JSON manifest → references blob hashes
```

**Key characteristics:**
- **Deduplication:** Shared layers stored once (saves disk space)
- **Immutable blobs:** Named by SHA-256 hash for integrity
- **Custom storage:** Set `OLLAMA_MODELS` env var to change location
- **Portability:** Entire `models/` directory is relocatable

### 1.5 Platform Support

| Platform | Status | Notes |
|:---|:---|:---|
| **Linux** | ✅ Full | Primary platform, CUDA/ROCm/CPU |
| **macOS** | ✅ Full | Metal acceleration on Apple Silicon |
| **Windows** | ✅ Full | CUDA, DirectML, CPU |
| **Android** | ❌ No | Not designed for mobile |
| **iOS** | ❌ No | Not designed for mobile |

### 1.6 Relevance to OneBrain

**Strengths for OneBrain:**
- ✅ Simple HTTP API — easy to integrate with reqwest
- ✅ Self-contained model management (pull, list, delete)
- ✅ Structured output via JSON schema in `format` parameter
- ✅ Native embedding support via `/api/embed`
- ✅ Tokio-compatible (just async HTTP calls)
- ✅ Mature, well-documented, large community

**Limitations for OneBrain:**
- ❌ Requires separate Ollama process (not in-process)
- ❌ ~10% performance overhead vs raw llama.cpp
- ❌ No mobile support (Android/iOS)
- ❌ Cannot be statically linked into the OneBrain binary
- ❌ External dependency management (Ollama install/update)

---

## 2. llama.cpp / llama-server

### 2.1 Overview

llama.cpp is the foundational C/C++ inference engine for running GGUF models locally. It is the backend that Ollama wraps. Using llama-server directly gives maximum control and performance.

**Current State (mid-2026):**
- De facto standard for local LLM inference
- Supports OpenAI-compatible endpoints (`/v1/chat/completions`)
- Tool calling via chat templates
- Grammar-constrained generation for structured output
- Production-ready for low-to-moderate concurrency

### 2.2 Server Mode API

llama-server provides OpenAI-compatible endpoints:

```bash
# Start the server
./llama-server -m model.gguf --port 8080 --n-gpu-layers 35

# Chat completion with tool calling
curl http://localhost:8080/v1/chat/completions -H "Content-Type: application/json" -d '{
  "model": "qwen3-8b",
  "messages": [
    {"role": "user", "content": "Extract the key fact from: Water boils at 100C"}
  ],
  "tools": [...],
  "temperature": 0.1
}'
```

**Key endpoints:**
- `POST /v1/chat/completions` — Chat with tool calling
- `POST /v1/embeddings` — Embeddings
- `GET /v1/models` — List loaded models
- `GET /props` — Server capabilities
- `POST /v1/messages` — Anthropic-compatible

### 2.3 Grammar-Constrained Generation (GBNF)

llama.cpp's killer feature for OneBrain: **GBNF grammars** force the model to output strictly valid structured data.

**How it works:** During token sampling, the grammar masks out invalid tokens at each step, ensuring 100% structural validity.

**Method 1: JSON Schema (Recommended)**
```bash
curl http://localhost:8080/v1/chat/completions -d '{
  "messages": [{"role": "user", "content": "Encode this fact..."}],
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "knowledge_unit",
      "schema": {
        "type": "object",
        "properties": {
          "title": {"type": "string"},
          "content": {"type": "string"},
          "tags": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["title", "content", "tags"]
      }
    }
  }
}'
```

**Method 2: Custom GBNF Grammar**
```gbnf
# knowledge_unit.gbnf
root   ::= "{" ws "\"title\":" ws string "," ws "\"content\":" ws string "," ws "\"tags\":" ws array "}"
ws     ::= [ \t\n]*
string ::= "\"" [^"\\]* "\""
array  ::= "[" ws (string ("," ws string)*)? ws "]"
```

### 2.4 GGUF Quantization Performance

| Quant | Bits/Weight | Model Size (8B) | Speed (relative) | Quality |
|:---|:---|:---|:---|:---|
| **F16** | 16 | ~16 GB | Baseline | Original |
| **Q8_0** | 8 | ~8.5 GB | 0.7x (slower) | Near-lossless |
| **Q5_K_M** | ~5.5 | ~5.7 GB | 0.9x | High |
| **Q4_K_M** | ~4.5 | ~4.9 GB | 1.0x (fastest) | Good (~95%+) |
| **Q3_K_M** | ~3.5 | ~3.9 GB | 1.1x | Acceptable |
| **Q2_K** | ~2.5 | ~2.8 GB | 1.2x | Degraded |

> **Important:** Q4_K_M is faster than Q8_0 because local LLM inference is **memory-bandwidth-bound**. Smaller weights = less data transferred = higher tokens/second.

### 2.5 Rust Bindings

| Crate | Type | Status | Notes |
|:---|:---|:---|:---|
| `llama-cpp-2` | Safe Rust bindings | Active | Direct C API wrapper, well-maintained |
| `llama-cpp-rs` | Older bindings | Maintained | Alternative, slightly less current |

### 2.6 Mobile Platform Support

| Platform | Method | GPU Acceleration | Notes |
|:---|:---|:---|:---|
| **Android** | NDK cross-compile | Vulkan, OpenCL | Production-ready |
| **iOS** | XCFramework | Metal | First-class Apple Silicon |

### 2.7 Relevance to OneBrain

**Strengths:**
- ✅ Maximum performance (baseline, no overhead)
- ✅ Grammar-constrained generation — perfect for KU structured output
- ✅ Mobile support (Android + iOS)
- ✅ Rust bindings available (`llama-cpp-2`)
- ✅ Granular control over GPU offloading, threading, memory

**Limitations:**
- ❌ Requires C/C++ compilation toolchain
- ❌ Model management must be built separately
- ❌ More complex setup than Ollama

---

## 3. Candle (HuggingFace Rust ML Framework)

### 3.1 Overview

Candle is HuggingFace's minimalist, pure-Rust machine learning framework. It provides tensor operations, model loading, and inference without Python or C++ dependencies.

**Current State (mid-2026):**
- Production-ready for specific inference use cases
- Native GGUF format support via `candle-transformers`
- GPU: CUDA, Metal backends; CPU: MKL, Accelerate
- WASM support for browser deployment
- Foundation for `mistral.rs` (production inference engine)

### 3.2 Capabilities and Model Support

**Supported architectures (GGUF):**
- LLaMA (v1, v2, v3, v3.1)
- Mistral, Mixtral
- Phi (1, 1.5, 2, 3, 4)
- Gemma (1, 2)
- Qwen (2, 2.5, 3)

**Core crates:**
- `candle-core` — Tensor operations, device management
- `candle-nn` — Neural network layers
- `candle-transformers` — Pre-built model architectures, GGUF loading
- `candle-flash-attn` — Flash Attention support

### 3.3 GGUF Model Support

```rust
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama;

// Load GGUF model
let device = Device::Cpu; // or Device::new_cuda(0)?
let model = quantized_llama::ModelWeights::from_gguf(
    "model-q4_k_m.gguf",
    &device
)?;

// Run inference
let input_ids = Tensor::new(&[token_ids], &device)?;
let logits = model.forward(&input_ids, 0)?;
```

### 3.4 Performance vs llama.cpp

| Aspect | Candle | llama.cpp |
|:---|:---|:---|
| **Raw throughput** | Good, improving | Best-in-class |
| **CUDA optimization** | Basic kernels | Highly optimized custom kernels |
| **Metal** | Supported | Highly optimized |
| **CPU (quantized)** | Good | Best (handwritten SIMD) |
| **Compilation** | Pure Rust (cargo build) | Requires C++ toolchain |
| **Binary size** | Smaller | Larger (C++ runtime) |

> **Note:** Candle's raw inference throughput is typically 10-30% slower than llama.cpp for GGUF models due to llama.cpp's heavily hand-optimized SIMD kernels. However, Candle's pure-Rust nature makes it far easier to integrate, deploy, and maintain.

### 3.5 mistral.rs — Production Engine on Candle

**mistral.rs** is a high-level inference engine built on top of Candle that adds:

| Feature | Description |
|:---|:---|
| **OpenAI API** | Drop-in compatible REST API |
| **PagedAttention** | Efficient memory management |
| **Continuous batching** | Higher throughput |
| **Speculative decoding** | Faster generation |
| **ISQ** | In-Situ Quantization (load FP16 → quantize at runtime) |
| **Tool calling** | Native support |
| **GGUF + GPTQ + AWQ** | Multiple quantization formats |

### 3.6 Maturity Assessment

| Criteria | Rating | Notes |
|:---|:---|:---|
| **API stability** | ⚠️ Medium | Breaking changes still occur |
| **Documentation** | ⚠️ Medium | Improving but gaps exist |
| **Community** | ✅ Strong | HuggingFace backing |
| **Production use** | ✅ Yes | Via mistral.rs |
| **Model coverage** | ✅ Good | All major architectures |
| **Mobile/embedded** | ⚠️ Possible | WASM yes; native mobile limited |

### 3.7 Relevance to OneBrain

**Strengths:**
- ✅ Pure Rust — links directly into OneBrain binary
- ✅ GGUF support — same models as Ollama/llama.cpp
- ✅ No external process needed
- ✅ WASM support (future browser nodes?)
- ✅ Via mistral.rs: production-grade with OpenAI API compatibility

**Limitations:**
- ❌ Lower raw performance than llama.cpp
- ❌ API stability concerns (still evolving)
- ❌ No built-in model management
- ❌ Grammar-constrained generation less mature than llama.cpp

---

## 4. ONNX Runtime

### 4.1 Overview

ONNX Runtime (ORT) is Microsoft's cross-platform inference engine. The `ort` Rust crate provides safe bindings.

**Current State (mid-2026):**
- Excellent for classical ML, vision, and embedding models
- LLM support functional but not optimal vs specialized engines
- Strong mobile/NPU support via execution providers
- The `ort` crate is well-maintained and production-ready

### 4.2 Rust Bindings (`ort` crate)

```toml
[dependencies]
ort = { version = "2", features = ["cuda"] }
```

```rust
use ort::{Session, Value};

let session = Session::builder()?
    .with_execution_providers([CUDAExecutionProvider::default().build()])?
    .commit_from_file("model.onnx")?;

let input = Value::from_array(input_tensor)?;
let outputs = session.run(ort::inputs![input]?)?;
```

### 4.3 Mobile/NPU Execution Providers

| Provider | Platform | Hardware |
|:---|:---|:---|
| **QNN** | Android (Snapdragon) | Qualcomm NPU |
| **CoreML** | iOS/macOS | Apple Neural Engine |
| **XNNPACK** | Android/iOS | CPU (optimized) |
| **NNAPI** | Android | Various NPUs |
| **DirectML** | Windows | GPU/NPU |

### 4.4 Suitability for LLM Inference

| Aspect | Assessment |
|:---|:---|
| **Embedding models** | ✅ Excellent — many optimized for ONNX |
| **LLM text generation** | ⚠️ Functional but suboptimal |
| **Dynamic shapes (tokens)** | ⚠️ NPUs struggle with variable-length generation |
| **Quantization** | INT4/INT8 supported but different from GGUF quants |

### 4.5 Relevance to OneBrain

**Strengths:**
- ✅ Best mobile NPU support (QNN, CoreML, XNNPACK)
- ✅ Excellent for embedding models specifically
- ✅ Mature Rust bindings (`ort` crate)

**Limitations:**
- ❌ Not ideal for LLM text generation
- ❌ Different model format from GGUF ecosystem
- ❌ Conversion complexity

> **For OneBrain:** ONNX Runtime is best positioned as a secondary backend specifically for embedding models on mobile devices, not as the primary LLM inference engine.

---

## 5. GGUF Format Deep Dive

### 5.1 File Structure

GGUF (GPT-Generated Unified Format) is a binary format designed for `mmap`-based loading:

```
┌─────────────────────────────────┐
│           HEADER                │
│  • Magic: "GGUF"               │
│  • Version: 3                   │
│  • Tensor count                 │
│  • Metadata KV count            │
├─────────────────────────────────┤
│     METADATA KEY-VALUE STORE    │
│  • Architecture (llama, qwen..) │
│  • n_layers, n_embd, n_head     │
│  • Rope frequency parameters    │
│  • Tokenizer vocabulary         │
│  • Quantization type            │
│  • Context length               │
├─────────────────────────────────┤
│        TENSOR INFO              │
│  • Name, dimensions, type       │
│  • Offset into tensor data      │
├─────────────────────────────────┤
│        TENSOR DATA              │
│  • Raw quantized weights        │
│  • Aligned for mmap efficiency  │
└─────────────────────────────────┘
```

### 5.2 Quantization Types Comparison

| Type | Bits/Weight | Size (7B model) | Quality vs FP16 | Speed | Best For |
|:---|:---|:---|:---|:---|:---|
| **F16** | 16.0 | ~14 GB | 100% | Slowest | Reference only |
| **Q8_0** | 8.0 | ~7.5 GB | ~99%+ | Slow | Accuracy-critical |
| **Q6_K** | 6.5 | ~5.9 GB | ~98% | Medium | High-quality |
| **Q5_K_M** | 5.5 | ~5.1 GB | ~97% | Fast | Quality-focused |
| **Q4_K_M** | 4.5 | ~4.3 GB | ~95% | Fastest | **Sweet spot** |
| **Q4_K_S** | 4.5 | ~4.1 GB | ~93% | Fastest | Space-constrained |
| **Q3_K_M** | 3.5 | ~3.4 GB | ~88% | Very fast | Mobile |
| **Q2_K** | 2.5 | ~2.5 GB | ~75% | Very fast | Ultra-constrained |

**K-Quants explained:**
- **K** = "K-quant" family — uses mixed precision per layer
- **S/M/L** = Small/Medium/Large — how many bits allocated to sensitive layers
- **imatrix** = Importance Matrix quantization — analyzes activations to optimize quality loss

### 5.3 Memory Requirements Formula

```
Total RAM/VRAM ≈ Model Weights + KV Cache + Overhead

Model Weights (GB) = (Parameters × Bits_per_Weight) / 8 / 1024³
KV Cache (GB)      = f(context_length, n_layers, n_heads, head_dim, batch_size)
Overhead           = 1.15 – 1.2× multiplier
```

**Quick Reference (8B model):**

| Quantization | Weight Size | + 4K Context | + 8K Context | Recommended RAM |
|:---|:---|:---|:---|:---|
| Q4_K_M | ~4.9 GB | +0.5 GB | +1.0 GB | 8 GB |
| Q5_K_M | ~5.7 GB | +0.5 GB | +1.0 GB | 10 GB |
| Q8_0 | ~8.5 GB | +0.5 GB | +1.0 GB | 12 GB |

### 5.4 Model Sources

| Source | URL | Notes |
|:---|:---|:---|
| **HuggingFace** | huggingface.co | Primary source; TheBloke, bartowski, unsloth profiles |
| **Ollama Library** | ollama.com/library | Curated, ready to use |
| **llama.cpp quantize** | Build from source | Convert any model to GGUF yourself |

### 5.5 Verification and Integrity

- **SHA-256 checksums:** All HuggingFace files have SHA-256 hashes
- **Ollama blobs:** Named by SHA-256 (inherent verification)
- **File validation:** GGUF header contains magic number + version for format checking
- **OneBrain should:** Verify SHA-256 after download, store hash in manifest

### 5.6 OneBrain Model Selection by Device

| Device Tier | RAM | Recommended Model | Quantization |
|:---|:---|:---|:---|
| **Mobile (low)** | 4 GB | Phi-4-mini (3.8B) | Q3_K_M |
| **Mobile (high)** | 8 GB | Qwen3-4B | Q4_K_M |
| **Laptop** | 16 GB | Qwen3-8B | Q4_K_M |
| **Desktop** | 32 GB | Qwen3-8B | Q8_0 |
| **Server** | 64 GB+ | Qwen3-32B | Q4_K_M |

---

## 6. Tool/Function Calling in Local Models

### 6.1 How Tool Calling Works (Protocol Level)

```
┌──────────┐     ┌────────────┐     ┌──────────┐
│  Client  │────▶│   Model    │────▶│  Client  │
│          │     │            │     │          │
│ 1. Send  │     │ 2. Decide  │     │ 4. Execute│
│    prompt │     │    if tool │     │    function│
│    +tools │     │    needed  │     │           │
│          │     │            │     │ 5. Send   │
│          │     │ 3. Return  │     │    result  │
│          │     │    tool_call│     │    back    │
│          │     │    JSON    │     │           │
│          │◀────│            │◀────│           │
│ 7. Get   │     │ 6. Generate│     │           │
│  answer  │     │  final     │     │           │
│          │     │  response  │     │           │
└──────────┘     └────────────┘     └──────────┘
```

**The model does NOT execute tools.** It only outputs structured JSON describing what tool to call. The client code executes the function and returns results.

### 6.2 Model Support Matrix

| Model Family | Tool Calling | Reliability (7B) | Reliability (30B+) | Notes |
|:---|:---|:---|:---|:---|
| **Qwen 3.x** | ✅ Native | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | Best out-of-box tool priors |
| **Llama 3.1/3.2/3.3** | ✅ Native | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 70B excellent; 8B needs prompt engineering |
| **Phi-3/4** | ✅ Supported | ⭐⭐⭐ | N/A (small models) | Great for edge, strong reasoning |
| **Mistral-Nemo** | ✅ Native | ⭐⭐⭐ | ⭐⭐⭐⭐ | Good reliability |
| **Gemma 2** | ⚠️ Limited | ⭐⭐ | ⭐⭐⭐ | Less trained for tool use |

### 6.3 Reliability by Model Size

| Size Range | Tool Selection | Schema Compliance | Multi-Step | Restraint |
|:---|:---|:---|:---|:---|
| **<4B** | ⚠️ Unreliable | ⚠️ Often fails | ❌ Poor | ❌ Poor |
| **4B–8B** | ✅ Good | ✅ Good (with grammar) | ⚠️ Limited | ⚠️ Variable |
| **8B–30B** | ✅ Reliable | ✅ Reliable | ✅ Good | ✅ Good |
| **30B+** | ✅ Excellent | ✅ Excellent | ✅ Excellent | ✅ Excellent |

> **Important:** For OneBrain, **Q4_K_M is the minimum viable quantization for tool calling.** Going below Q4 (Q3, Q2) disproportionately degrades tool-calling logic before it impacts general chat quality.

### 6.4 Best Practices for OneBrain

1. **Use grammar-constrained generation** — Force valid JSON output via GBNF/JSON schema instead of hoping the model gets it right
2. **Low temperature (0.1–0.3)** — Reduces hallucinated function names and invalid JSON
3. **Use Guided-Structured Templates** — Instead of free-form CoT, force models to map intent to schema step-by-step
4. **Implement retry logic** — Parse output, validate against schema, retry up to 3 times on failure
5. **Negative testing** — Verify model correctly refuses to call tools when input doesn't warrant it

### 6.5 Error Handling Strategy

```rust
// Pseudocode for robust tool calling in OneBrain
async fn invoke_with_retry(
    backend: &dyn ModelBackend,
    prompt: &str,
    tools: &[ToolDef],
    max_retries: u32,
) -> Result<ToolResponse> {
    for attempt in 0..max_retries {
        let response = backend.chat(prompt, tools).await?;
        
        match validate_tool_call(&response) {
            Ok(valid) => return Ok(valid),
            Err(ValidationError::InvalidJson) => {
                // Retry with explicit "output valid JSON" instruction
                continue;
            }
            Err(ValidationError::WrongTool) => {
                // Retry with clarified tool descriptions
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::MaxRetriesExceeded)
}
```

---

## 7. Embedding Models for Local Use

### 7.1 Model Comparison

| Model | Params | Dimensions | Context | Quality | Speed | VRAM |
|:---|:---|:---|:---|:---|:---|:---|
| **all-MiniLM-L6-v2** | 22M | 384 | 512 | ⭐⭐ | ⚡⚡⚡⚡⚡ | ~90 MB |
| **nomic-embed-text** | 137M | 768 | 8,192 | ⭐⭐⭐⭐ | ⚡⚡⚡⚡ | ~300 MB |
| **mxbai-embed-large** | 335M | 1024 | 512 | ⭐⭐⭐⭐ | ⚡⚡⚡ | ~670 MB |
| **embeddinggemma-300m** | 300M | 768 | 2048 | ⭐⭐⭐⭐ | ⚡⚡⚡ | ~600 MB |
| **Qwen3-Embedding** | Varies | 1024+ | 8,192 | ⭐⭐⭐⭐⭐ | ⚡⚡ | ~1 GB+ |
| **BGE-M3** | 568M | 1024 | 8,192 | ⭐⭐⭐⭐⭐ | ⚡⚡ | ~1.1 GB |

### 7.2 Can a Chat Model Generate Embeddings?

**Technically yes, but strongly NOT recommended for OneBrain:**

| Aspect | Chat Model Embeddings | Dedicated Embedding Model |
|:---|:---|:---|
| **Quality** | ⚠️ Suboptimal for retrieval | ✅ Optimized for semantic similarity |
| **Speed** | ❌ 10-100x slower | ✅ Purpose-built, fast |
| **Memory** | ❌ 8B model = 5-16 GB | ✅ 22-570 MB |
| **Training** | Optimized for next-token prediction | Optimized for distance metrics |

### 7.3 OneBrain's "Single Model Per Node" Constraint

The requirement of "one model handles all tasks" creates a tension with embedding quality:

**Option A: Use chat model for everything (simple but compromised)**
- Run Qwen3-8B for chat, classification, AND embeddings
- Simpler deployment, worse embedding quality

**Option B: Chat model + tiny embedding model (recommended)**
- Primary: Qwen3-8B for encoding, classification, tool calling
- Secondary: nomic-embed-text (137M, ~300MB) for embeddings
- The embedding model is so small it barely impacts total resource usage
- Much better duplicate detection and similarity search

> **Recommendation for OneBrain:** Adopt Option B. The "single model" constraint should be interpreted as "single primary LLM" plus a lightweight embedding companion. A 300MB embedding model alongside a 5GB LLM is a negligible overhead increase but a massive quality improvement for duplicate detection.

### 7.4 Embeddings for Duplicate Detection

```rust
// Pseudocode: Using embeddings for KU deduplication
async fn is_duplicate(
    backend: &dyn ModelBackend,
    new_ku_text: &str,
    existing_embeddings: &[(CidHash, Vec<f32>)],
    threshold: f32, // typically 0.85-0.95
) -> Option<CidHash> {
    let new_embedding = backend.embed(new_ku_text).await?;
    
    for (cid, existing) in existing_embeddings {
        let similarity = cosine_similarity(&new_embedding, existing);
        if similarity > threshold {
            return Some(*cid); // Duplicate found
        }
    }
    None
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (mag_a * mag_b)
}
```

---

## 8. Rust Ecosystem for AI

### 8.1 Key Crates

| Crate | Purpose | Relevance to OneBrain |
|:---|:---|:---|
| **reqwest** | HTTP client (async) | Talk to Ollama/llama-server API |
| **candle-core** | Tensor operations | In-process inference |
| **candle-transformers** | Model architectures, GGUF | Load and run models directly |
| **mistral.rs** | Production inference engine | Alternative to Ollama (pure Rust) |
| **ort** | ONNX Runtime bindings | Embedding models, mobile NPU |
| **llama-cpp-2** | llama.cpp Rust bindings | Direct C API access |
| **sysinfo** | System info (RAM, CPU, GPU) | Device capability detection |
| **dirs** | Platform-specific directories | Model storage paths |
| **serde** / **serde_json** | JSON serialization | API communication |
| **tokio** | Async runtime | Already used by quinn |
| **async-trait** | Async trait support | ModelBackend trait definition |

### 8.2 HTTP Client Integration (reqwest)

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
    format: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ResponseMessage,
    done: bool,
    total_duration: u64, // nanoseconds
    eval_count: u32,
}

pub struct OllamaBackend {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaBackend {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
        }
    }
    
    pub async fn chat(&self, messages: Vec<Message>) -> Result<ChatResponse> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            format: None,
        };
        
        let response = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?
            .json::<ChatResponse>()
            .await?;
        
        Ok(response)
    }
}
```

### 8.3 Device Detection (sysinfo)

```rust
use sysinfo::System;

pub struct DeviceCapabilities {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_cores: usize,
    pub has_gpu: bool,
    pub gpu_vram_gb: Option<f64>,
}

pub fn detect_capabilities() -> DeviceCapabilities {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    DeviceCapabilities {
        total_ram_gb: sys.total_memory() as f64 / 1_073_741_824.0,
        available_ram_gb: sys.available_memory() as f64 / 1_073_741_824.0,
        cpu_cores: sys.cpus().len(),
        has_gpu: detect_gpu(),
        gpu_vram_gb: detect_gpu_vram(),
    }
}

pub fn recommend_model(caps: &DeviceCapabilities) -> ModelRecommendation {
    match caps.total_ram_gb as u64 {
        0..=4 => ModelRecommendation {
            model: "phi-4-mini".into(),
            quantization: "Q3_K_M".into(),
            expected_size_gb: 2.5,
        },
        5..=8 => ModelRecommendation {
            model: "qwen3:4b".into(),
            quantization: "Q4_K_M".into(),
            expected_size_gb: 2.8,
        },
        9..=16 => ModelRecommendation {
            model: "qwen3:8b".into(),
            quantization: "Q4_K_M".into(),
            expected_size_gb: 4.9,
        },
        17..=32 => ModelRecommendation {
            model: "qwen3:8b".into(),
            quantization: "Q8_0".into(),
            expected_size_gb: 8.5,
        },
        _ => ModelRecommendation {
            model: "qwen3:32b".into(),
            quantization: "Q4_K_M".into(),
            expected_size_gb: 20.0,
        },
    }
}
```

### 8.4 Model Storage Paths (dirs)

```rust
use dirs::data_local_dir;
use std::path::PathBuf;

pub fn model_storage_path() -> PathBuf {
    let base = data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("onebrain")
        .join("models");
    
    // Platform-specific defaults:
    // Windows: C:\Users\<user>\AppData\Local\onebrain\models
    // macOS:   ~/Library/Application Support/onebrain/models
    // Linux:   ~/.local/share/onebrain/models
    
    std::fs::create_dir_all(&base).ok();
    base
}
```

### 8.5 Notable Rust AI Projects

| Project | Description | Relevance |
|:---|:---|:---|
| **mistral.rs** | Production LLM engine (Candle-based) | Direct competitor to Ollama, pure Rust |
| **kalosm** | High-level LLM/audio/image framework | Easiest Rust LLM integration |
| **candelabra** | Higher-level Candle wrapper | Async GGUF downloads, token streaming |
| **async-llm** | Async OpenAI-compatible client | Model-agnostic API abstraction |

---

## 9. Runtime Comparison Matrix

### 9.1 Feature Comparison

| Feature | Ollama | llama.cpp | Candle/mistral.rs | ONNX Runtime |
|:---|:---|:---|:---|:---|
| **Integration** | HTTP API | C API / HTTP | Native Rust | Native Rust |
| **Setup complexity** | ⭐ Easy | ⭐⭐⭐ Complex | ⭐⭐ Medium | ⭐⭐ Medium |
| **Performance** | Good (90%) | Best (100%) | Good (70-90%) | Good (for embeddings) |
| **Model management** | ✅ Built-in | ❌ Manual | ❌ Manual | ❌ Manual |
| **Structured output** | ✅ JSON schema | ✅ GBNF grammar | ⚠️ Developing | ❌ N/A |
| **Tool calling** | ✅ Native | ✅ Via templates | ✅ Via mistral.rs | ❌ N/A |
| **Embeddings** | ✅ /api/embed | ✅ /v1/embeddings | ✅ Direct | ✅ Excellent |
| **Mobile** | ❌ No | ✅ Android/iOS | ⚠️ WASM only | ✅ QNN/CoreML |
| **In-process** | ❌ External | Via bindings | ✅ Native | ✅ Native |
| **Async (Tokio)** | ✅ HTTP | ⚠️ spawn_blocking | ✅ Native | ⚠️ spawn_blocking |

### 9.2 OneBrain Fit Score

| Criterion (Weight) | Ollama | llama.cpp | Candle/mistral.rs | ONNX |
|:---|:---|:---|:---|:---|
| **Ease of integration** (25%) | 10 | 5 | 7 | 6 |
| **Performance** (20%) | 7 | 10 | 7 | 5 |
| **Structured output** (20%) | 9 | 10 | 6 | 2 |
| **Cross-platform** (15%) | 6 | 9 | 7 | 9 |
| **Model management** (10%) | 10 | 2 | 3 | 3 |
| **In-process** (10%) | 1 | 7 | 10 | 10 |
| **Weighted Total** | **7.5** | **7.4** | **6.7** | **5.5** |

---

## 10. Recommendations for OneBrain

### 10.1 Phased Implementation Strategy

#### Phase 1: Ollama Backend (MVP)

**Rationale:** Fastest path to a working system. Ollama handles model management, provides a clean REST API, and supports structured output.

```rust
/// Phase 1: ModelBackend trait definition
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Generate structured output from a prompt
    async fn generate_structured(
        &self,
        prompt: &str,
        schema: &serde_json::Value,
    ) -> Result<serde_json::Value>;
    
    /// Generate embeddings for text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    
    /// Check if the backend is available and healthy
    async fn health_check(&self) -> Result<BackendStatus>;
    
    /// Get information about the loaded model
    async fn model_info(&self) -> Result<ModelInfo>;
}
```

#### Phase 2: In-Process Backend (Self-Contained)

**Rationale:** Eliminate Ollama dependency for self-contained deployment. Use Candle + mistral.rs for native Rust inference.

#### Phase 3: Mobile/Edge Backend

**Rationale:** Extend to mobile devices using llama.cpp (via FFI) or ONNX Runtime for NPU support.

### 10.2 ModelBackend Trait Design (Full)

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Core trait for all AI model backends
#[async_trait]
pub trait ModelBackend: Send + Sync + 'static {
    // ─── Text Generation ───
    async fn chat(&self, messages: &[ChatMessage]) -> Result<ChatResponse>;
    async fn generate_structured(
        &self,
        messages: &[ChatMessage],
        schema: &JsonSchema,
    ) -> Result<serde_json::Value>;
    
    // ─── Tool Calling ───
    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Result<ChatOrToolResponse>;
    
    // ─── Embeddings ───
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    
    // ─── Management ───
    async fn health_check(&self) -> Result<BackendStatus>;
    async fn model_info(&self) -> Result<ModelInfo>;
    fn embedding_dimensions(&self) -> usize;
}

/// Model metadata
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub family: String,       // "qwen3", "llama3", "phi4"
    pub parameter_count: u64,
    pub quantization: String, // "Q4_K_M", "Q8_0"
    pub context_length: u32,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Capability {
    Chat,
    ToolCalling,
    StructuredOutput,
    Embedding,
    Vision,
}

/// Response that may be either text or a tool call
#[derive(Debug)]
pub enum ChatOrToolResponse {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function_name: String,
    pub arguments: serde_json::Value,
}
```

### 10.3 Model Selection Strategy

```rust
pub fn select_model(caps: &DeviceCapabilities) -> ModelConfig {
    let available_gb = caps.total_ram_gb * 0.6; // Reserve 40% for OS/OneBrain
    
    let candidates = vec![
        ModelConfig::new("qwen3:32b", "Q4_K_M", 20.0, Tier::Server),
        ModelConfig::new("qwen3:8b",  "Q8_0",   8.5,  Tier::Desktop),
        ModelConfig::new("qwen3:8b",  "Q4_K_M", 4.9,  Tier::Laptop),
        ModelConfig::new("qwen3:4b",  "Q4_K_M", 2.8,  Tier::Mobile),
        ModelConfig::new("phi4-mini",  "Q3_K_M", 2.0,  Tier::Embedded),
    ];
    
    candidates
        .into_iter()
        .find(|c| c.estimated_ram_gb <= available_gb)
        .unwrap_or_else(|| ModelConfig::minimum_viable())
}
```

---

## 11. Risk Assessment

### 11.1 Risk Matrix

| Risk | Probability | Impact | Mitigation |
|:---|:---|:---|:---|
| **Ollama API changes** | Low | Medium | Pin version; abstract behind trait |
| **Model quality insufficient for KU encoding** | Medium | High | Test multiple models; implement quality scoring |
| **Small model tool calling unreliable** | Medium | High | Grammar-constrained output; retry logic |
| **Candle breaking API changes** | Medium | Medium | Pin version; Phase 2 can wait |
| **GGUF format changes** | Very Low | Low | Format is stable; backward compatible |
| **Memory pressure on low-end devices** | High | Medium | Device detection → automatic model selection |
| **External Ollama dependency** | Medium | Medium | Phase 2 eliminates with in-process backend |
| **Model download size/bandwidth** | Medium | Low | Incremental downloads; content-addressed storage |
| **Embedding quality from chat models** | High | Medium | Use dedicated embedding model |

### 11.2 Critical Decisions Confirmed

1. ✅ **Dual model approach:** Primary LLM + lightweight embedding model (nomic-embed-text ~300MB)
2. ✅ **Start with Ollama** for development velocity, plan Candle/mistral.rs migration
3. ✅ **Minimum 8GB RAM** for quality; 4GB as degraded mode with Q3_K_M
4. ✅ **Version-pinned model manifests** with opt-in updates

---

## Appendix A: Cargo.toml Dependencies (Proposed for ku-ai)

```toml
[package]
name = "ku-ai"
version = "0.1.0"
edition = "2021"
description = "AI Layer — Local model runtime, KU encoding, Personal AI"

[features]
default = []
ollama = ["dep:reqwest"]
# candle = ["dep:candle-core", "dep:candle-transformers", "dep:candle-nn"]
# onnx = ["dep:ort"]

[dependencies]
# Core
ku-core = { path = "../ku-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
thiserror = "1"

# Async runtime (same as ku-net)
tokio = { version = "1", features = ["rt-multi-thread", "sync", "time"] }

# Phase 1: Ollama HTTP backend
reqwest = { version = "0.12", features = ["json", "rustls-tls"], optional = true }

# Device detection
sysinfo = "0.32"
dirs = "5"

# Content verification
blake3 = "1"

# Future: In-process inference
# candle-core = { version = "0.8", optional = true }
# candle-transformers = { version = "0.8", optional = true }
# candle-nn = { version = "0.8", optional = true }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

---

*Document version: 1.0 | Next review: After Phase 1 MVP completion*
