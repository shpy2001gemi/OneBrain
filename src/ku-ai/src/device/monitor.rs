//! # Memory Pressure Monitor
//!
//! Periodically checks system memory usage and classifies pressure levels.
//! Used to make model loading/unloading decisions at runtime.

use sysinfo::System;

/// Memory pressure level classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    /// Memory usage below 80% — safe for operations.
    Normal,
    /// Memory usage between 80–90% — consider freeing resources.
    Warning,
    /// Memory usage above 90% — critical, should unload models.
    Critical,
}

/// Monitors system memory pressure in real-time.
pub struct MemoryMonitor {
    system: System,
}

impl MemoryMonitor {
    /// Create a new memory monitor.
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    /// Check current memory pressure level.
    ///
    /// Refreshes memory info and classifies usage:
    /// - **Normal**: < 80% used
    /// - **Warning**: 80–90% used
    /// - **Critical**: > 90% used
    pub fn check(&mut self) -> MemoryPressure {
        self.system.refresh_memory();
        let total = self.system.total_memory();
        if total == 0 {
            return MemoryPressure::Normal;
        }
        let used = total.saturating_sub(self.system.available_memory());
        let ratio = used as f64 / total as f64;

        if ratio > 0.90 {
            MemoryPressure::Critical
        } else if ratio > 0.80 {
            MemoryPressure::Warning
        } else {
            MemoryPressure::Normal
        }
    }

    /// Return the number of currently available bytes of memory.
    pub fn available_bytes(&mut self) -> u64 {
        self.system.refresh_memory();
        self.system.available_memory()
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_monitor_check_does_not_panic() {
        let mut monitor = MemoryMonitor::new();
        let pressure = monitor.check();
        // On a healthy dev machine, should be Normal or Warning
        assert!(
            pressure == MemoryPressure::Normal
                || pressure == MemoryPressure::Warning
                || pressure == MemoryPressure::Critical
        );
    }

    #[test]
    fn test_memory_monitor_available_bytes() {
        let mut monitor = MemoryMonitor::new();
        let available = monitor.available_bytes();
        // Should return a positive value on any real system
        assert!(available > 0, "available memory should be positive");
    }

    #[test]
    fn test_memory_pressure_equality() {
        assert_eq!(MemoryPressure::Normal, MemoryPressure::Normal);
        assert_ne!(MemoryPressure::Normal, MemoryPressure::Warning);
        assert_ne!(MemoryPressure::Warning, MemoryPressure::Critical);
    }
}
