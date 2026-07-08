//! # System Information Detection
//!
//! Uses the `sysinfo` crate to detect hardware capabilities at runtime.

use sysinfo::System;

/// Detected system hardware information.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// Total physical RAM in bytes.
    pub total_ram_bytes: u64,
    /// Currently available RAM in bytes.
    pub available_ram_bytes: u64,
    /// Number of logical CPU cores.
    pub cpu_cores: usize,
    /// CPU brand string (e.g. "Apple M2 Pro", "AMD Ryzen 9 7950X").
    pub cpu_brand: String,
    /// Operating system name.
    pub os_name: String,
}

/// Detect the current system's hardware information.
///
/// This refreshes all system information and returns a snapshot.
pub fn detect_system() -> SystemInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    SystemInfo {
        total_ram_bytes: sys.total_memory(),
        available_ram_bytes: sys.available_memory(),
        cpu_cores: sys.cpus().len(),
        cpu_brand: sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default(),
        os_name: System::name().unwrap_or_else(|| "unknown".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_system_has_ram() {
        let info = detect_system();
        assert!(info.total_ram_bytes > 0, "total RAM must be positive");
    }

    #[test]
    fn test_detect_system_has_cpu_cores() {
        let info = detect_system();
        assert!(info.cpu_cores > 0, "must detect at least one CPU core");
    }

    #[test]
    fn test_available_ram_le_total() {
        let info = detect_system();
        assert!(
            info.available_ram_bytes <= info.total_ram_bytes,
            "available RAM must not exceed total RAM"
        );
    }
}
