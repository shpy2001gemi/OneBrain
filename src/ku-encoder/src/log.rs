//! Encoding log — tracks encoding attempts for debugging and quality analysis.
//!
//! Provides structured logging of each encoding attempt with timestamps,
//! confidence scores, decisions, and error information. Logs can be
//! serialized to JSON for post-hoc analysis.
//!
//! # Usage
//! ```rust,ignore
//! use ku_encoder::log::{EncodingLog, LogEntry};
//!
//! let mut log = EncodingLog::new();
//! log.add(LogEntry {
//!     timestamp_ms: 1700000000000,
//!     input_text: "Water boils at 100°C".into(),
//!     gene_type: Some("fact".into()),
//!     confidence: 0.85,
//!     wire_bytes_size: 42,
//!     duration_ms: 150,
//!     decision: "accept".into(),
//!     attempt: 1,
//!     success: true,
//!     error: None,
//! });
//! log.save(std::path::Path::new("encoding.log.json")).unwrap();
//! ```

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::EncoderError;

/// A single log entry recording one encoding attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// The input text that was encoded.
    pub input_text: String,
    /// Gene type detected (if any).
    pub gene_type: Option<String>,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
    /// Size of the encoded wire bytes in bytes.
    pub wire_bytes_size: usize,
    /// Duration of the encoding in milliseconds.
    pub duration_ms: u64,
    /// Decision made (e.g., "accept", "retry", "fallback_tier1", "reject").
    pub decision: String,
    /// Attempt number (1-indexed).
    pub attempt: u32,
    /// Whether the encoding was ultimately successful.
    pub success: bool,
    /// Error message if the encoding failed.
    pub error: Option<String>,
}

/// Encoding log that accumulates entries for debugging and quality analysis.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EncodingLog {
    /// All log entries in chronological order.
    pub entries: Vec<LogEntry>,
}

impl EncodingLog {
    /// Create a new empty encoding log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry to the log.
    pub fn add(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }

    /// Save the log to a JSON file.
    pub fn save(&self, path: &Path) -> Result<(), EncoderError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| EncoderError::CoreDnaError(e.to_string()))?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a log from a JSON file.
    pub fn load(path: &Path) -> Result<Self, EncoderError> {
        let json = std::fs::read_to_string(path)?;
        let log: Self =
            serde_json::from_str(&json).map_err(|e| EncoderError::CoreDnaError(e.to_string()))?;
        Ok(log)
    }

    /// Compute summary statistics for the log.
    pub fn summary(&self) -> LogSummary {
        let total = self.entries.len();
        let succeeded = self.entries.iter().filter(|e| e.success).count();
        let avg_confidence = if total > 0 {
            self.entries.iter().map(|e| e.confidence).sum::<f32>() / total as f32
        } else {
            0.0
        };
        LogSummary {
            total,
            succeeded,
            failed: total - succeeded,
            avg_confidence,
        }
    }
}

/// Summary statistics for an encoding log.
#[derive(Debug)]
pub struct LogSummary {
    /// Total number of encoding attempts.
    pub total: usize,
    /// Number of successful encodings.
    pub succeeded: usize,
    /// Number of failed encodings.
    pub failed: usize,
    /// Average confidence across all entries.
    pub avg_confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(success: bool, confidence: f32) -> LogEntry {
        LogEntry {
            timestamp_ms: 1700000000000,
            input_text: "test input".into(),
            gene_type: Some("fact".into()),
            confidence,
            wire_bytes_size: 42,
            duration_ms: 100,
            decision: if success { "accept" } else { "reject" }.into(),
            attempt: 1,
            success,
            error: if success {
                None
            } else {
                Some("test error".into())
            },
        }
    }

    #[test]
    fn test_log_add_and_summary() {
        let mut log = EncodingLog::new();
        log.add(sample_entry(true, 0.9));
        log.add(sample_entry(true, 0.8));
        log.add(sample_entry(false, 0.3));

        let summary = log.summary();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.succeeded, 2);
        assert_eq!(summary.failed, 1);
        // avg = (0.9 + 0.8 + 0.3) / 3 ≈ 0.667
        assert!((summary.avg_confidence - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_log_empty_summary() {
        let log = EncodingLog::new();
        let summary = log.summary();
        assert_eq!(summary.total, 0);
        assert_eq!(summary.succeeded, 0);
        assert_eq!(summary.failed, 0);
        assert!((summary.avg_confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_log_serde_roundtrip() {
        let mut log = EncodingLog::new();
        log.add(sample_entry(true, 0.85));

        let json = serde_json::to_string(&log).unwrap();
        let restored: EncodingLog = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.entries[0].confidence, 0.85);
    }

    #[test]
    fn test_log_save_and_load() {
        let mut log = EncodingLog::new();
        log.add(sample_entry(true, 0.9));
        log.add(sample_entry(false, 0.4));

        let dir = std::env::temp_dir();
        let path = dir.join("ku_encoder_test_log.json");

        log.save(&path).expect("save should succeed");
        let loaded = EncodingLog::load(&path).expect("load should succeed");

        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].success, true);
        assert_eq!(loaded.entries[1].success, false);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }
}
