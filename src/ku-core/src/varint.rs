//! Variable-length integer encoding for ConceptIDs.
//!
//! OneBrain's tier-based varint scheme:
//! - Tier 0 (1 byte):  0x00-0x7F → values 0-127 (128 universal primitives)
//! - Tier 1 (2 bytes): 0x80XX → values 128-16,511 (~16K common concepts)
//! - Tier 2 (3 bytes): 0xC0XXXX → values 16,512-2,113,663 (~2M standard)
//! - Tier 3 (4 bytes): 0xE0XXXXXX → values 2,113,664-270,549,119 (~268M extended)
//! - Tier 3+ (5 bytes): 0xF0XXXXXXXX → values 270,549,120-34,628,173,567 (~34.6B community)
//! - Tier 5 (6 bytes): 0xF8XXXXXXXXXX → RESERVED for future (~4.4T)
//! - Tier 6 (7 bytes): 0xFCXXXXXXXXXXXX → RESERVED for future (~562T)
//! - Tier 7 (8 bytes): 0xFEXXXXXXXXXXXXXX → RESERVED for future (~72Q)
//! - 0xFF: SENTINEL — reserved forever
//!
//! Encoding prefix bits:
//! - 0xxxxxxx → 1 byte (7 bits)
//! - 10xxxxxx → 2 bytes (14 bits, offset by 128)
//! - 110xxxxx → 3 bytes (21 bits, offset by 128 + 2^14)
//! - 1110xxxx → 4 bytes (28 bits, offset by 128 + 2^14 + 2^21)
//! - 11110xxx → 5 bytes (35 bits, offset by 128 + 2^14 + 2^21 + 2^28)

use crate::error::KuError;

// Tier boundary constants
const TIER0_MAX: u64 = 127;
const TIER1_OFFSET: u64 = 128;
const TIER1_CAPACITY: u64 = 16_384; // 2^14
const TIER1_MAX: u64 = TIER0_MAX + TIER1_CAPACITY; // 16,511
const TIER2_OFFSET: u64 = TIER1_MAX + 1; // 16,512
const TIER2_CAPACITY: u64 = 2_097_152; // 2^21
const TIER2_MAX: u64 = TIER1_MAX + TIER2_CAPACITY; // 2,113,663
const TIER3_OFFSET: u64 = TIER2_MAX + 1; // 2,113,664
const TIER3_CAPACITY: u64 = 268_435_456; // 2^28
const TIER3_MAX: u64 = TIER2_MAX + TIER3_CAPACITY; // 270,549,119
const TIER3P_OFFSET: u64 = TIER3_MAX + 1; // 270,549,120
const TIER3P_CAPACITY: u64 = 34_359_738_368; // 2^35 = 8 * 2^32
const TIER3P_MAX: u64 = TIER3_MAX + TIER3P_CAPACITY - 1; // 34,628,173,487 (note: spec says 34,628,173,567 but 2^35 - 1 + offset)

/// Encode a u64 value as a variable-length byte sequence.
///
/// Returns the encoded bytes following the OneBrain varint tier system,
/// or an error if the value exceeds the 5-tier maximum (~34.6 billion).
pub fn encode_varint(value: u64) -> Result<Vec<u8>, KuError> {
    if value <= TIER0_MAX {
        // Tier 0: 1 byte, prefix 0xxxxxxx
        Ok(vec![value as u8])
    } else if value <= TIER1_MAX {
        // Tier 1: 2 bytes, prefix 10xxxxxx
        let v = (value - TIER1_OFFSET) as u16;
        Ok(vec![0x80 | ((v >> 8) as u8), (v & 0xFF) as u8])
    } else if value <= TIER2_MAX {
        // Tier 2: 3 bytes, prefix 110xxxxx
        let v = (value - TIER2_OFFSET) as u32;
        Ok(vec![
            0xC0 | ((v >> 16) as u8 & 0x1F),
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ])
    } else if value <= TIER3_MAX {
        // Tier 3: 4 bytes, prefix 1110xxxx
        let v = (value - TIER3_OFFSET) as u32;
        Ok(vec![
            0xE0 | ((v >> 24) as u8 & 0x0F),
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ])
    } else if value <= TIER3P_MAX {
        // Tier 3+: 5 bytes, prefix 11110xxx
        let v = value - TIER3P_OFFSET;
        Ok(vec![
            0xF0 | ((v >> 32) as u8 & 0x07),
            ((v >> 24) & 0xFF) as u8,
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ])
    } else {
        Err(KuError::InvalidData(format!(
            "Varint value {} exceeds 5-tier maximum ({})",
            value, TIER3P_MAX
        )))
    }
}

/// Decode a varint from a byte slice.
///
/// Returns `Ok((value, bytes_consumed))` on success, or `Err(KuError)` on failure.
pub fn decode_varint(bytes: &[u8]) -> Result<(u64, usize), KuError> {
    if bytes.is_empty() {
        return Err(KuError::VarintTruncated { needed: 1, got: 0 });
    }

    let first = bytes[0];

    if first & 0x80 == 0 {
        // Tier 0: 0xxxxxxx → 1 byte
        Ok((first as u64, 1))
    } else if first & 0xC0 == 0x80 {
        // Tier 1: 10xxxxxx → 2 bytes
        if bytes.len() < 2 {
            return Err(KuError::VarintTruncated {
                needed: 2,
                got: bytes.len(),
            });
        }
        let adjusted = (((first & 0x3F) as u16) << 8) | (bytes[1] as u16);
        Ok((adjusted as u64 + TIER1_OFFSET, 2))
    } else if first & 0xE0 == 0xC0 {
        // Tier 2: 110xxxxx → 3 bytes
        if bytes.len() < 3 {
            return Err(KuError::VarintTruncated {
                needed: 3,
                got: bytes.len(),
            });
        }
        let adjusted =
            (((first & 0x1F) as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32);
        Ok((adjusted as u64 + TIER2_OFFSET, 3))
    } else if first & 0xF0 == 0xE0 {
        // Tier 3: 1110xxxx → 4 bytes
        if bytes.len() < 4 {
            return Err(KuError::VarintTruncated {
                needed: 4,
                got: bytes.len(),
            });
        }
        let adjusted = (((first & 0x0F) as u32) << 24)
            | ((bytes[1] as u32) << 16)
            | ((bytes[2] as u32) << 8)
            | (bytes[3] as u32);
        Ok((adjusted as u64 + TIER3_OFFSET, 4))
    } else if first & 0xF8 == 0xF0 {
        // Tier 3+: 11110xxx → 5 bytes
        if bytes.len() < 5 {
            return Err(KuError::VarintTruncated {
                needed: 5,
                got: bytes.len(),
            });
        }
        let adjusted = (((first & 0x07) as u64) << 32)
            | ((bytes[1] as u64) << 24)
            | ((bytes[2] as u64) << 16)
            | ((bytes[3] as u64) << 8)
            | (bytes[4] as u64);
        Ok((adjusted as u64 + TIER3P_OFFSET, 5))
    } else if first & 0xFC == 0xF8 {
        // Tier 5: 111110xx → 6 bytes — RESERVED for future
        Err(KuError::InvalidData(
            "Varint Tier 5 (6-byte, prefix 111110xx) is reserved for future use".into(),
        ))
    } else if first & 0xFE == 0xFC {
        // Tier 6: 1111110x → 7 bytes — RESERVED for future
        Err(KuError::InvalidData(
            "Varint Tier 6 (7-byte, prefix 1111110x) is reserved for future use".into(),
        ))
    } else if first == 0xFE {
        // Tier 7: 11111110 → 8 bytes — RESERVED for future
        Err(KuError::InvalidData(
            "Varint Tier 7 (8-byte, prefix 11111110) is reserved for future use".into(),
        ))
    } else {
        // 0xFF: SENTINEL — reserved forever as escape hatch
        Err(KuError::InvalidVarintPrefix(first))
    }
}

#[cfg(test)]
mod varint_tests {
    use super::*;

    #[test]
    fn test_tier0_boundaries() {
        // Min
        let encoded = encode_varint(0).unwrap();
        assert_eq!(encoded, vec![0x00]);
        assert_eq!(decode_varint(&encoded).unwrap(), (0, 1));

        // Max tier 0
        let encoded = encode_varint(127).unwrap();
        assert_eq!(encoded, vec![0x7F]);
        assert_eq!(decode_varint(&encoded).unwrap(), (127, 1));

        // Typical: concept "DO" might be ID 1
        let encoded = encode_varint(1).unwrap();
        assert_eq!(encoded.len(), 1);
    }

    #[test]
    fn test_tier1_boundaries() {
        // Min tier 1
        let encoded = encode_varint(128).unwrap();
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded[0] & 0xC0, 0x80); // prefix 10xxxxxx
        assert_eq!(decode_varint(&encoded).unwrap(), (128, 2));

        // Max tier 1
        let encoded = encode_varint(16_511).unwrap();
        assert_eq!(encoded.len(), 2);
        assert_eq!(decode_varint(&encoded).unwrap(), (16_511, 2));

        // Mid tier 1: "water" concept ~300
        let encoded = encode_varint(300).unwrap();
        assert_eq!(encoded.len(), 2);
        assert_eq!(decode_varint(&encoded).unwrap(), (300, 2));
    }

    #[test]
    fn test_tier2_boundaries() {
        // Min tier 2
        let encoded = encode_varint(16_512).unwrap();
        assert_eq!(encoded.len(), 3);
        assert_eq!(encoded[0] & 0xE0, 0xC0); // prefix 110xxxxx
        assert_eq!(decode_varint(&encoded).unwrap(), (16_512, 3));

        // Max tier 2
        let encoded = encode_varint(2_113_663).unwrap();
        assert_eq!(encoded.len(), 3);
        assert_eq!(decode_varint(&encoded).unwrap(), (2_113_663, 3));

        // Mid tier 2: "photosynthesis" concept ~100_000
        let encoded = encode_varint(100_000).unwrap();
        assert_eq!(encoded.len(), 3);
        assert_eq!(decode_varint(&encoded).unwrap(), (100_000, 3));
    }

    #[test]
    fn test_tier3_boundaries() {
        // Min tier 3 (4 bytes)
        let encoded = encode_varint(2_113_664).unwrap();
        assert_eq!(encoded.len(), 4);
        assert_eq!(encoded[0] & 0xF0, 0xE0); // prefix 1110xxxx
        assert_eq!(decode_varint(&encoded).unwrap(), (2_113_664, 4));

        // Mid tier 3
        let encoded = encode_varint(100_000_000).unwrap();
        assert_eq!(encoded.len(), 4);
        assert_eq!(decode_varint(&encoded).unwrap(), (100_000_000, 4));

        // Max tier 3
        let encoded = encode_varint(270_549_119).unwrap();
        assert_eq!(encoded.len(), 4);
        assert_eq!(decode_varint(&encoded).unwrap(), (270_549_119, 4));
    }

    #[test]
    fn test_tier3plus_boundaries() {
        // Min tier 3+ (5 bytes)
        let encoded = encode_varint(270_549_120).unwrap();
        assert_eq!(encoded.len(), 5);
        assert_eq!(encoded[0] & 0xF8, 0xF0); // prefix 11110xxx
        assert_eq!(decode_varint(&encoded).unwrap(), (270_549_120, 5));

        // Large community concept
        let encoded = encode_varint(10_000_000_000).unwrap();
        assert_eq!(encoded.len(), 5);
        assert_eq!(decode_varint(&encoded).unwrap(), (10_000_000_000, 5));
    }

    #[test]
    fn test_roundtrip_all_tiers() {
        let test_values = [
            0,
            1,
            63,
            127, // Tier 0
            128,
            256,
            1000,
            16_511, // Tier 1
            16_512,
            50_000,
            2_113_663, // Tier 2
            2_113_664,
            100_000_000,
            270_549_119, // Tier 3 (4 bytes)
            270_549_120,
            5_000_000_000, // Tier 3+ (5 bytes)
        ];

        for &val in &test_values {
            let encoded = encode_varint(val).unwrap();
            let (decoded, consumed) = decode_varint(&encoded).unwrap();
            assert_eq!(decoded, val, "roundtrip failed for {}", val);
            assert_eq!(consumed, encoded.len(), "consumed mismatch for {}", val);
        }
    }

    #[test]
    fn test_varint_size_matches_tier() {
        // Verify the size of each tier matches spec
        assert_eq!(encode_varint(0).unwrap().len(), 1);
        assert_eq!(encode_varint(127).unwrap().len(), 1);
        assert_eq!(encode_varint(128).unwrap().len(), 2);
        assert_eq!(encode_varint(16_511).unwrap().len(), 2);
        assert_eq!(encode_varint(16_512).unwrap().len(), 3);
        assert_eq!(encode_varint(2_113_663).unwrap().len(), 3);
        assert_eq!(encode_varint(2_113_664).unwrap().len(), 4);
        assert_eq!(encode_varint(270_549_119).unwrap().len(), 4);
        assert_eq!(encode_varint(270_549_120).unwrap().len(), 5);
    }

    #[test]
    fn test_varint_overflow() {
        // Value exceeding maximum should return error
        let result = encode_varint(TIER3P_MAX + 1);
        assert!(result.is_err(), "Values above max should return error");
    }
}
