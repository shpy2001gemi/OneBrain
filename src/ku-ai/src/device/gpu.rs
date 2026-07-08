//! # GPU Detection
//!
//! Detect available GPU hardware with graceful fallback.
//! Currently supports Apple Silicon (Metal) detection;
//! CUDA/NVML support can be added as a feature gate.

/// GPU compute backend type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuBackend {
    /// NVIDIA CUDA.
    Cuda,
    /// Apple Metal (unified memory architecture).
    Metal,
    /// No GPU acceleration available.
    None,
}

/// Detected GPU information.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// Human-readable GPU name.
    pub name: String,
    /// GPU compute backend.
    pub backend: GpuBackend,
    /// Dedicated VRAM in bytes (0 for unified memory architectures).
    pub vram_bytes: u64,
}

/// Detect the primary GPU, if available.
///
/// On macOS with Apple Silicon, returns a Metal GPU info entry.
/// On other platforms, returns `None` in the MVP (no NVML probing).
pub fn detect_gpu() -> Option<GpuInfo> {
    // macOS: check for Apple Silicon (ARM64)
    #[cfg(target_os = "macos")]
    {
        if std::env::consts::ARCH == "aarch64" {
            return Some(GpuInfo {
                name: "Apple Silicon (Unified Memory)".into(),
                backend: GpuBackend::Metal,
                vram_bytes: 0, // Unified memory — shared with system RAM
            });
        }
    }

    // Windows/Linux MVP: no NVML probing yet
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_backend_equality() {
        assert_eq!(GpuBackend::Cuda, GpuBackend::Cuda);
        assert_ne!(GpuBackend::Cuda, GpuBackend::Metal);
        assert_ne!(GpuBackend::Metal, GpuBackend::None);
    }

    #[test]
    fn test_detect_gpu_does_not_panic() {
        // Just ensure it doesn't crash — result depends on host hardware
        let _gpu = detect_gpu();
    }

    #[test]
    fn test_gpu_info_clone() {
        let info = GpuInfo {
            name: "Test GPU".to_string(),
            backend: GpuBackend::Cuda,
            vram_bytes: 8 * 1024 * 1024 * 1024,
        };
        let cloned = info.clone();
        assert_eq!(cloned.name, "Test GPU");
        assert_eq!(cloned.backend, GpuBackend::Cuda);
    }
}
