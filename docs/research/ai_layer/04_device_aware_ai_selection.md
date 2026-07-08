# Device-Aware AI Selection — OneBrain Pillar 6

> **Research Date:** July 2026  
> **Author:** OneBrain Research Team  
> **Status:** Research Complete  
> **Scope:** Hardware detection, device classification, and automatic AI model selection

---

## Executive Summary

OneBrain requires a **device-aware AI selection system** that automatically detects each node's hardware capabilities and selects the optimal AI model. This system must work across desktop (Windows/macOS/Linux), mobile (Android/iOS), and server environments.

### Key Architecture Decisions

| Decision | Choice | Rationale |
|:---|:---|:---|
| **System info** | `sysinfo` crate | Cross-platform, mature, RAM/CPU detection |
| **GPU detection** | `nvml-wrapper` + `gfxinfo` | Layered: NVIDIA-specific → cross-vendor fallback |
| **Selection strategy** | Conservative (leave headroom) | Prevent OOM crashes on all platforms |
| **Detection timing** | Once at startup + periodic monitoring | Balance accuracy with performance |
| **Mobile approach** | RAM-based tiers only | No reliable GPU VRAM query on mobile |

> **Key Finding:** No existing app auto-selects models based on hardware — this is a genuine OneBrain differentiator.

---

## 1. System Resource Detection in Rust

### 1.1 The `sysinfo` Crate

```rust
use sysinfo::System;

pub struct SystemProfile {
    pub total_ram_bytes: u64,
    pub available_ram_bytes: u64,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub cpu_arch: &'static str,
    pub os_name: Option<String>,
}

impl SystemProfile {
    pub fn detect() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        SystemProfile {
            total_ram_bytes: sys.total_memory(),
            available_ram_bytes: sys.available_memory(),
            physical_cores: System::physical_core_count(),
            logical_cores: sys.cpus().len(),
            cpu_arch: std::env::consts::ARCH,
            os_name: System::name(),
        }
    }

    pub fn total_ram_gb(&self) -> f64 {
        self.total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}
```

### 1.2 How Existing Apps Detect Hardware

| App | Detection Strategy |
|:---|:---|
| **Ollama** | Auto-detects GPU via backend; calculates layer offload from VRAM; falls back to CPU |
| **LM Studio** | Visual hardware monitor; compatibility indicators per quantization |
| **GPT4All** | Lists RAM requirements per model; user selects |
| **PocketPal AI** | CPU-based + optional GPU offload; user picks model |

---

## 2. GPU Detection and VRAM Measurement

### 2.1 Layered Detection Strategy

```
Layer 1: Platform-specific (NVML, Metal)        → Most detailed
Layer 2: Cross-platform graphics (Vulkan/wgpu)  → Good coverage
Layer 3: Cross-vendor utility (gfxinfo)         → Simple fallback
Layer 4: No GPU detected                       → CPU-only mode
```

### 2.2 NVIDIA: NVML via `nvml-wrapper`

```rust
use nvml_wrapper::Nvml;

pub fn detect_nvidia_gpu() -> Option<NvidiaGpuInfo> {
    let nvml = Nvml::init().ok()?; // Runtime-loaded, no crash on non-NVIDIA
    let device = nvml.device_by_index(0).ok()?;
    let mem = device.memory_info().ok()?;
    Some(NvidiaGpuInfo {
        name: device.name().unwrap_or_default(),
        total_vram_bytes: mem.total,
        free_vram_bytes: mem.free,
        cuda_available: true,
    })
}
```

### 2.3 Apple Silicon: Unified Memory

Apple Silicon has **no separate VRAM** — CPU and GPU share physical memory.

```rust
#[cfg(target_os = "macos")]
fn detect_apple_silicon() -> Option<AppleSiliconInfo> {
    let sys = System::new_all();
    let total = sys.total_memory();
    Some(AppleSiliconInfo {
        total_unified_memory: total,
        estimated_gpu_available: (total as f64 * 0.75) as u64,
        is_unified: true,
    })
}
```

### 2.4 AMD: Vulkan Detection

```rust
// AMD vendor ID = 0x1002
// Linux fallback: /sys/class/drm/card0/device/mem_info_vram_total
```

### 2.5 Intel: Integrated vs Discrete

| GPU Type | VRAM Behavior | OneBrain Strategy |
|:---|:---|:---|
| **Intel Arc (Discrete)** | Dedicated GDDR | Use Vulkan DEVICE_LOCAL heap |
| **Intel iGPU** | Shared RAM (reported as "128MB") | **Treat as CPU-tier** — ignore VRAM |
| **Intel NPU** | N/A (accelerator only) | Record as metadata for future |

### 2.6 Unified GPU Detection

```rust
pub enum GpuBackend {
    NvidiaCuda { vram_bytes: u64, name: String },
    AmdRocm { vram_bytes: u64, name: String },
    AppleMetal { unified_memory_bytes: u64 },
    IntelArc { vram_bytes: u64 },
    IntelIntegrated,
    Vulkan { vram_bytes: u64, name: String },
    None,
}

pub fn detect_gpu() -> GpuBackend {
    if let Some(nvidia) = detect_nvidia_gpu() {
        return GpuBackend::NvidiaCuda { vram_bytes: nvidia.total_vram_bytes, name: nvidia.name };
    }
    #[cfg(target_os = "macos")]
    if let Some(apple) = detect_apple_silicon() {
        return GpuBackend::AppleMetal { unified_memory_bytes: apple.total_unified_memory };
    }
    // Try AMD, Intel, generic Vulkan...
    GpuBackend::None
}
```

---

## 3. Mobile Device Classification

### 3.1 Android Device Tiers

| Tier | RAM | AI Capability | Recommended Model |
|:---|:---|:---|:---|
| **Budget** | ≤4 GB | Very basic (0.5-1B) | Phi-4-mini Q2_K (~1.2 GB) |
| **Mid-Range** | 6-8 GB | Small SLMs (1-3B) | Phi-4-mini Q3_K_M (~2 GB) |
| **High-End** | 8-12 GB | 3-4B with quantization | Qwen3-4B Q4_K_M (~2.8 GB) |
| **Flagship** | 12-16+ GB | 4-8B models | Qwen3-8B Q3_K_M (~3.8 GB) |

### 3.2 iOS Device Tiers

| Device | Chip | RAM | AI Capability |
|:---|:---|:---|:---|
| iPhone 13 mini | A15 | 4 GB | Very limited (~2 GB usable) |
| iPhone 14 | A15 | 6 GB | Small SLMs (~3 GB usable) |
| iPhone 15/16 Pro | A17/A18 Pro | 8 GB | 3-4B models |
| iPad Pro M4 | M4 | 16 GB | Near-laptop capability |

### 3.3 Mobile RAM Available for Apps

**Rule: Apps can safely use ~40-50% of total RAM**

| Total RAM | Safe App Usage | Max Model Size |
|:---|:---|:---|
| 4 GB | ~1.5-2 GB | ~1 GB |
| 6 GB | ~2.5-3 GB | ~1.8 GB |
| 8 GB | ~3.5-4 GB | ~2.8 GB |
| 12 GB | ~5-6 GB | ~4.5 GB |
| 16 GB | ~7-8 GB | ~6 GB |

### 3.4 Thermal Throttling & Battery

| Metric | Peak (1-2 min) | Sustained (10+ min) |
|:---|:---|:---|
| **Throughput** | 10-15 t/s (flagship) | 3-5 t/s (throttled) |
| **Performance drop** | — | 40-70% degradation |
| **Battery drain** | — | 15-25% per hour |

### 3.5 Mobile Memory Killers

**Android LMKD**: Kills background processes based on PSI (Pressure Stall Information).
- **Survival**: Run as Foreground Service; leave ~50% RAM free

**iOS Jetsam**: SIGKILL (uncatchable) when app exceeds memory limit.
- **Survival**: Request `increased-memory-limit` entitlement; keep model < 50% device RAM
- **Critical**: Use `mmap` for GGUF loading (only active pages consume RAM)

---

## 4. Automatic Model Selection Algorithm

### 4.1 Memory Budget Calculation

```rust
fn calculate_memory_budget(
    system: &SystemProfile,
    gpu: &GpuBackend,
    platform: Platform,
) -> u64 {
    let total_memory = match gpu {
        GpuBackend::NvidiaCuda { vram_bytes, .. } => *vram_bytes,
        GpuBackend::AppleMetal { unified_memory_bytes } => *unified_memory_bytes,
        _ => system.total_ram_bytes,
    };
    
    let os_reserve = match platform {
        Platform::Mobile => total_memory * 55 / 100, // 55% for OS on mobile
        Platform::Desktop => {
            let reserve = 4u64 * 1024 * 1024 * 1024; // 4 GB
            std::cmp::min(reserve, total_memory * 30 / 100)
        },
        Platform::Server => total_memory * 15 / 100,
    };
    
    let kv_cache_reserve = 1_500_000_000u64; // 1.5 GB for KV cache
    total_memory.saturating_sub(os_reserve).saturating_sub(kv_cache_reserve)
}
```

### 4.2 Selection Algorithm

```rust
pub fn select_model(
    system: &SystemProfile,
    gpu: &GpuBackend,
    registry: &ModelRegistry,
    platform: Platform,
) -> SelectedModel {
    let budget = calculate_memory_budget(system, gpu, platform);
    
    let candidates: Vec<_> = registry.models()
        .filter(|m| m.total_memory_required() <= budget)
        .collect();
    
    // Select BEST model that fits (larger params > higher quant > newer)
    let selected = candidates.iter()
        .max_by_key(|m| (m.param_count, m.quantization_quality_score()))
        .expect("At least one model must fit");
    
    SelectedModel {
        model: selected.clone(),
        backend: select_backend(gpu, selected),
        estimated_tps: estimate_performance(selected, gpu),
    }
}
```

### 4.3 Decision Tree

```
IS MOBILE?
├─ YES → Use total RAM as primary signal
│         ├─ ≤4 GB  → Phi-4-mini Q2_K (1.2 GB)
│         ├─ 6 GB   → Phi-4-mini Q3_K_M (2 GB)
│         ├─ 8 GB   → Qwen3-4B Q4_K_M (2.8 GB)
│         ├─ 12 GB  → Qwen3-8B Q3_K_M (3.8 GB)
│         └─ 16+ GB → Qwen3-8B Q4_K_M (4.9 GB)
└─ NO (Desktop/Server)
    ├─ HAS DISCRETE GPU?
    │   ├─ 6-8 GB VRAM  → Qwen3-8B Q4_K_M (4.9 GB)
    │   ├─ 12 GB VRAM   → Qwen3-8B Q8_0 (8.5 GB)
    │   ├─ 16 GB VRAM   → Qwen3-14B Q4_K_M
    │   └─ 24+ GB VRAM  → Qwen3-32B Q4_K_M (20 GB)
    └─ CPU ONLY (or Apple Silicon)
        ├─ 8 GB RAM   → Phi-4-mini Q4_K_M (~2.5 GB)
        ├─ 16 GB RAM  → Qwen3-8B Q4_K_M (4.9 GB)
        ├─ 32 GB RAM  → Qwen3-8B Q8_0 (8.5 GB)
        └─ 64+ GB RAM → Qwen3-32B Q4_K_M (20 GB)
```

### 4.4 Conservative Selection

**OneBrain MUST use conservative selection:**
- OOM crash on mobile = silent app kill (Jetsam/LMKD)
- Users run other apps simultaneously
- KV cache grows during long conversations
- **Partial GPU offload is dramatically slower** than full offload — prefer smaller model that fits entirely

---

## 5. Performance Benchmarks

### 5.1 Inference Performance (8B Q4_K_M)

| Hardware | Backend | Tokens/sec | Notes |
|:---|:---|:---|:---|
| RTX 5090 (32GB) | CUDA | 130-215+ t/s | Full VRAM offload |
| RTX 4090 (24GB) | CUDA | 100-150 t/s | Full VRAM offload |
| Apple M4 Max (128GB) | Metal | 70-110 t/s | High bandwidth |
| RTX 4060 (8GB) | CUDA | 40-80 t/s | Tight VRAM fit |
| Apple M2/M3 (16GB) | Metal | 30-50 t/s | Good for daily use |
| Modern Desktop CPU | AVX2/NEON | 10-30 t/s | Acceptable for chat |
| Mobile Flagship | CPU+GPU | 8-15 → 3-5 t/s | Throttles sustained |

### 5.2 CPU-Only Acceptability

- ✅ Acceptable: ≤8B model, interactive chat, no batch processing
- ❌ Not acceptable: >14B model, batch embedding, < 5 t/s throughput

---

## 6. Runtime Resource Monitoring

### 6.1 Memory Pressure Monitor

```rust
pub struct ResourceMonitor {
    system: System,
    check_interval: Duration,
}

impl ResourceMonitor {
    pub fn check_memory_pressure(&mut self) -> MemoryStatus {
        self.system.refresh_memory();
        let available_pct = (self.system.available_memory() as f64 
            / self.system.total_memory() as f64) * 100.0;
        
        match available_pct {
            x if x < 5.0  => MemoryStatus::Critical,
            x if x < 10.0 => MemoryStatus::Warning,
            x if x < 15.0 => MemoryStatus::Low,
            _              => MemoryStatus::Normal,
        }
    }
}
```

### 6.2 Graceful Degradation

```
NORMAL → LOW → WARNING → CRITICAL

LOW:      Log warning, reduce KV cache size
WARNING:  Unload embedding model (~130 MB), notify user
CRITICAL: Unload all models, clear caches, re-detect for smaller model
```

---

## 7. Hardware Tier Classification

### 7.1 Node Tiers

| Tier | Label | RAM | GPU VRAM | LLM | Embedding |
|:---|:---|:---|:---|:---|:---|
| **T0** | Minimal Mobile | ≤4 GB | — | Phi-4-mini Q2_K | all-MiniLM-L6-v2 |
| **T1** | Mobile | 6-8 GB | — | Phi-4-mini Q3_K_M | all-MiniLM-L6-v2 |
| **T2** | High Mobile | 8-12 GB | — | Qwen3-4B Q4_K_M | all-MiniLM-L6-v2 |
| **T3** | Laptop | 16 GB | 0-6 GB | Qwen3-8B Q4_K_M | nomic-embed-text |
| **T4** | Desktop | 32 GB | 8-12 GB | Qwen3-8B Q8_0 | nomic-embed-text |
| **T5** | Workstation | 64 GB | 16-24 GB | Qwen3-14B Q4_K_M | nomic-embed-text |
| **T6** | Server | 128+ GB | 24+ GB | Qwen3-32B Q4_K_M | nomic-embed-text |

### 7.2 Tier Classification Algorithm

```rust
pub fn classify_tier(system: &SystemProfile, gpu: &GpuBackend, platform: Platform) -> u8 {
    let ram_gb = system.total_ram_gb();
    let vram_gb = gpu.vram_gb().unwrap_or(0.0);
    
    match platform {
        Platform::Android | Platform::IOS => {
            match ram_gb as u32 {
                0..=4 => 0,    // T0
                5..=7 => 1,    // T1
                8..=11 => 2,   // T2
                _ => 3,        // T3 max for mobile
            }
        }
        _ => {
            if ram_gb >= 128.0 || vram_gb >= 24.0 { 6 }      // T6: Server
            else if ram_gb >= 64.0 || vram_gb >= 16.0 { 5 }   // T5: Workstation
            else if ram_gb >= 32.0 || vram_gb >= 8.0 { 4 }    // T4: Desktop
            else if ram_gb >= 16.0 { 3 }                       // T3: Laptop
            else if ram_gb >= 8.0 { 2 }                        // T2
            else { 1 }                                         // T1
        }
    }
}
```

### 7.3 NPU Future-Proofing

| NPU | Platform | LLM Usable? | Status |
|:---|:---|:---|:---|
| Apple Neural Engine | iOS/macOS | CoreML only, not GGUF | Record as metadata |
| Qualcomm Hexagon | Android | QNN models only | Record as metadata |
| Intel NPU | Windows/Linux | OpenVINO only | Record as metadata |

> NPUs don't work with GGUF/llama.cpp today. Record presence for future support.

---

## 8. Cross-Platform Considerations

### 8.1 Memory Management

| | Windows | macOS | Linux | Android | iOS |
|:---|:---|:---|:---|:---|:---|
| **OS overhead** | 3-5 GB | 2-4 GB | 0.3-1 GB | 1.5-3 GB | 1-2 GB |
| **Swap** | pagefile | compression | configurable | zRAM | none (compress) |
| **OOM handling** | Slow swap | Compression | OOM killer | LMKD | Jetsam (SIGKILL) |
| **Kill risk** | Low | Low | Medium | High | Very High |

> **Swap is NOT acceptable for LLM inference.** Always ensure model fits in physical RAM/VRAM.

### 8.2 Permissions

| Platform | RAM/CPU | GPU VRAM |
|:---|:---|:---|
| Windows/macOS/Linux | None needed | NVML: user-level; Metal: none |
| Android | ActivityManager (no perm) | Sandboxed — no access |
| iOS | ProcessInfo (no perm) | Not exposed |

---

## 9. Risk Assessment

| Risk | Severity | Mitigation |
|:---|:---|:---|
| OOM crash on mobile | 🔴 Critical | Conservative selection; leave 50%+ RAM free |
| Thermal throttling | 🟡 Medium | Adaptive thread count; inform user |
| Incorrect VRAM detection | 🟡 Medium | Fallback to RAM-based; validate with test |
| Intel iGPU fake VRAM | 🟡 Medium | Detect IntegratedGpu; ignore VRAM value |
| Model too slow on CPU | 🟡 Medium | Min 5 t/s threshold; suggest smaller model |
| NPU not usable with GGUF | 🟠 Low | Document; plan for future CoreML/QNN |

---

## Appendix: Quick Reference

```
┌─────────────────────────────────────────────────────────┐
│           OneBrain Model Selection Quick Reference       │
├─────────────┬──────────────┬─────────────┬──────────────┤
│ Total RAM   │ Platform     │ Model       │ Size         │
├─────────────┼──────────────┼─────────────┼──────────────┤
│ ≤4 GB       │ Mobile       │ Phi-4-mini  │ Q2_K  ~1.2GB │
│ 6-8 GB      │ Mobile       │ Phi-4-mini  │ Q3_K_M ~2GB  │
│ 8 GB        │ Mobile       │ Qwen3-4B    │ Q4_K_M ~2.8GB│
│ 8 GB        │ Desktop/CPU  │ Phi-4-mini  │ Q4_K_M ~2.5GB│
│ 16 GB       │ Desktop      │ Qwen3-8B    │ Q4_K_M ~4.9GB│
│ 32 GB       │ Desktop      │ Qwen3-8B    │ Q8_0  ~8.5GB │
│ 64+ GB      │ Server       │ Qwen3-32B   │ Q4_K_M ~20GB │
│ 8 GB VRAM   │ GPU Desktop  │ Qwen3-8B    │ Q4_K_M ~4.9GB│
│ 12 GB VRAM  │ GPU Desktop  │ Qwen3-8B    │ Q8_0  ~8.5GB │
│ 24+ GB VRAM │ GPU Desktop  │ Qwen3-32B   │ Q4_K_M ~20GB │
└─────────────┴──────────────┴─────────────┴──────────────┘
```

---

*Document version: 1.0 | Next review: After device detection implementation*
