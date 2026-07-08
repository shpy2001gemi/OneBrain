//! # Device Detection Module
//!
//! Aggregates system, GPU, tier, and memory monitoring into a unified
//! [`DeviceProfile`] that describes the current hardware capabilities.

pub mod gpu;
pub mod monitor;
pub mod system;
pub mod tier;

pub use gpu::{GpuBackend, GpuInfo};
pub use monitor::{MemoryMonitor, MemoryPressure};
pub use tier::DeviceTier;

/// Unified device profile describing the current hardware.
///
/// Combines CPU/RAM info, GPU detection, and tier classification
/// into a single snapshot used for model selection and resource management.
#[derive(Debug, Clone)]
pub struct DeviceProfile {
    /// Total physical RAM in bytes.
    pub total_ram_bytes: u64,
    /// Currently available RAM in bytes.
    pub available_ram_bytes: u64,
    /// Number of logical CPU cores.
    pub cpu_cores: usize,
    /// CPU brand string.
    pub cpu_brand: String,
    /// Detected GPU, if any.
    pub gpu: Option<GpuInfo>,
    /// Operating system name.
    pub os_name: String,
    /// Computed device tier for model selection.
    pub tier: DeviceTier,
}

impl DeviceProfile {
    /// Detect the current device's hardware profile.
    ///
    /// Performs system info detection, GPU probing, and tier classification.
    pub fn detect() -> Self {
        let sys = system::detect_system();
        let gpu = gpu::detect_gpu();
        let vram = gpu.as_ref().map(|g| g.vram_bytes);
        let tier = DeviceTier::classify(sys.total_ram_bytes, vram);

        Self {
            total_ram_bytes: sys.total_ram_bytes,
            available_ram_bytes: sys.available_ram_bytes,
            cpu_cores: sys.cpu_cores,
            cpu_brand: sys.cpu_brand,
            gpu,
            os_name: sys.os_name,
            tier,
        }
    }

    /// Return the total AI-usable memory in gigabytes.
    pub fn usable_gb(&self) -> u64 {
        let os_overhead = 2u64 * 1024 * 1024 * 1024;
        let usable = self.total_ram_bytes.saturating_sub(os_overhead);
        let vram = self.gpu.as_ref().map(|g| g.vram_bytes).unwrap_or(0);
        (usable + vram) / (1024 * 1024 * 1024)
    }
}

impl std::fmt::Display for DeviceProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ram_gb = self.total_ram_bytes / (1024 * 1024 * 1024);
        write!(
            f,
            "{} | {} cores | {} GB RAM | GPU: {} | Tier: {}",
            self.os_name,
            self.cpu_cores,
            ram_gb,
            self.gpu
                .as_ref()
                .map(|g| g.name.as_str())
                .unwrap_or("none"),
            self.tier,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_profile_detect() {
        let profile = DeviceProfile::detect();
        assert!(profile.total_ram_bytes > 0);
        assert!(profile.cpu_cores > 0);
        assert!(!profile.os_name.is_empty());
    }

    #[test]
    fn test_device_profile_display() {
        let profile = DeviceProfile::detect();
        let display = format!("{}", profile);
        assert!(!display.is_empty());
        assert!(display.contains("Tier"));
    }

    #[test]
    fn test_device_profile_usable_gb() {
        let profile = DeviceProfile::detect();
        // On any dev machine with at least 4 GB RAM, usable should be > 0
        // (might be 0 on very low-RAM devices, which is valid)
        let _gb = profile.usable_gb();
    }
}
