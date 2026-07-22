//! Error types for ku-core.

use std::fmt;

/// All possible errors produced by ku-core encoding / decoding operations.
#[derive(Debug)]
pub enum KuError {
    CborEncode(String),
    CborDecode(String),
    InvalidMagic([u8; 2]),
    UnsupportedVersion(u8),
    CrcMismatch {
        stored: u32,
        computed: u32,
    },
    VarintTruncated {
        needed: usize,
        got: usize,
    },
    InvalidVarintPrefix(u8),
    UnknownGeneType(u8),
    PayloadTruncated {
        expected: usize,
        got: usize,
    },
    /// ★ v5: Payload exceeds the u32 maximum (4 GB)
    PayloadTooLargeV5 {
        size: usize,
    },
    InvalidData(String),
}

impl fmt::Display for KuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KuError::CborEncode(msg) => write!(f, "CBOR encode error: {}", msg),
            KuError::CborDecode(msg) => write!(f, "CBOR decode error: {}", msg),
            KuError::InvalidMagic(m) => write!(f, "Invalid magic bytes: {:02X}{:02X}", m[0], m[1]),
            KuError::UnsupportedVersion(v) => write!(f, "Unsupported version: {}", v),
            KuError::CrcMismatch { stored, computed } => write!(
                f,
                "CRC mismatch: stored={:08X}, computed={:08X}",
                stored, computed
            ),
            KuError::VarintTruncated { needed, got } => {
                write!(f, "Varint truncated: needed {} bytes, got {}", needed, got)
            }
            KuError::InvalidVarintPrefix(b) => write!(f, "Invalid varint prefix: {:02X}", b),
            KuError::UnknownGeneType(g) => write!(f, "Unknown gene type: {}", g),
            KuError::PayloadTruncated { expected, got } => {
                write!(f, "Payload truncated: expected {}, got {}", expected, got)
            }
            KuError::PayloadTooLargeV5 { size } => write!(
                f,
                "Payload too large for v5: {} bytes (max {})",
                size,
                u32::MAX
            ),
            KuError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
        }
    }
}

impl std::error::Error for KuError {}
