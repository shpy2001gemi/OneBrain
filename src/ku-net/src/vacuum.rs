//! # Vacuum Filters — SPEC B §8
//!
//! Space-efficient probabilistic membership filter.
//! Used for advertising KU ownership across the network.
//!
//! Based on Bloom filter principles with configurable
//! bits-per-item and false-positive rate.

use crate::constants::*;

// ─── Vacuum Filter ─────────────────────────────────────────────────────────

/// Space-efficient probabilistic membership filter.
///
/// Trade-off: uses `bits_per_item` bits per element with
/// a target false-positive rate. Supports insert, query, encode/decode.
#[derive(Debug, Clone)]
pub struct VacuumFilter {
    /// Bit array (packed into u64 words).
    bits: Vec<u64>,
    /// Number of bits in the filter.
    num_bits: usize,
    /// Number of hash functions.
    hash_count: u8,
    /// Number of items inserted.
    num_items: usize,
    /// Bits per item (configuration).
    bits_per_item: u8,
}

impl VacuumFilter {
    /// Create a new filter with given capacity and target false-positive rate.
    ///
    /// # Arguments
    /// * `capacity` — Expected number of items
    /// * `fpr` — Target false-positive rate (e.g., 0.001 = 0.1%)
    pub fn new(capacity: usize, fpr: f64) -> Self {
        // Optimal bits per item: -log2(fpr) * 1.44
        let bpi = (-fpr.log2() * 1.44).ceil() as u8;
        let bits_per_item = bpi.max(4).min(20);

        let num_bits = capacity * bits_per_item as usize;
        let num_words = (num_bits + 63) / 64;

        // Optimal hash count: bits_per_item * ln(2) ≈ bits_per_item * 0.693
        let hash_count = ((bits_per_item as f64) * 0.693).ceil() as u8;
        let hash_count = hash_count.max(1).min(16);

        Self {
            bits: vec![0u64; num_words],
            num_bits,
            hash_count,
            num_items: 0,
            bits_per_item,
        }
    }

    /// Create a filter with default parameters from constants.
    pub fn with_defaults(capacity: usize) -> Self {
        Self::new(capacity, VACUUM_TARGET_FPR)
    }

    /// Insert an item into the filter.
    pub fn insert(&mut self, item: &[u8]) {
        let hash = blake3::hash(item);
        let hash_bytes = hash.as_bytes();

        for i in 0..self.hash_count as usize {
            let bit_pos = self.bit_position(hash_bytes, i);
            self.set_bit(bit_pos);
        }
        self.num_items += 1;
    }

    /// Check if an item might be in the filter.
    ///
    /// Returns `true` if the item might be present (possible false positive).
    /// Returns `false` if the item is definitely not present.
    pub fn contains(&self, item: &[u8]) -> bool {
        let hash = blake3::hash(item);
        let hash_bytes = hash.as_bytes();

        for i in 0..self.hash_count as usize {
            let bit_pos = self.bit_position(hash_bytes, i);
            if !self.get_bit(bit_pos) {
                return false;
            }
        }
        true
    }

    /// Encode the filter to bytes for wire transmission.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();

        // Header: 4 bytes
        out.extend_from_slice(&(self.num_bits as u32).to_be_bytes());
        out.push(self.hash_count);
        out.push(self.bits_per_item);
        out.extend_from_slice(&(self.num_items as u16).to_be_bytes());

        // Bit array
        for word in &self.bits {
            out.extend_from_slice(&word.to_be_bytes());
        }

        out
    }

    /// Decode a filter from wire bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, VacuumError> {
        if bytes.len() < 8 {
            return Err(VacuumError::TooShort);
        }

        let num_bits = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let hash_count = bytes[4];
        let bits_per_item = bytes[5];
        let num_items = u16::from_be_bytes([bytes[6], bytes[7]]) as usize;

        let num_words = (num_bits + 63) / 64;
        let expected_len = 8 + num_words * 8;

        if bytes.len() < expected_len {
            return Err(VacuumError::TooShort);
        }

        let mut bits = Vec::with_capacity(num_words);
        for i in 0..num_words {
            let offset = 8 + i * 8;
            let word = u64::from_be_bytes([
                bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3],
                bytes[offset + 4], bytes[offset + 5], bytes[offset + 6], bytes[offset + 7],
            ]);
            bits.push(word);
        }

        Ok(Self {
            bits,
            num_bits,
            hash_count,
            num_items,
            bits_per_item,
        })
    }

    /// Merge another filter into this one (OR operation).
    ///
    /// Both filters must have the same parameters.
    pub fn merge(&mut self, other: &VacuumFilter) -> Result<(), VacuumError> {
        if self.num_bits != other.num_bits || self.hash_count != other.hash_count {
            return Err(VacuumError::IncompatibleFilters);
        }

        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a |= *b;
        }
        self.num_items += other.num_items;
        Ok(())
    }

    /// Estimated false-positive rate based on current fill.
    pub fn estimated_fpr(&self) -> f64 {
        let k = self.hash_count as f64;
        let m = self.num_bits as f64;
        let n = self.num_items as f64;
        (1.0 - (-k * n / m).exp()).powf(k)
    }

    /// Number of items inserted.
    pub fn item_count(&self) -> usize {
        self.num_items
    }

    /// Size of the encoded filter in bytes.
    pub fn wire_size(&self) -> usize {
        8 + self.bits.len() * 8
    }

    // ─── Internal ──────────────────────────────────────────────────────────

    fn bit_position(&self, hash: &[u8; 32], index: usize) -> usize {
        // Use different sections of the hash for each hash function
        let offset = (index * 4) % 28;
        let val = u32::from_be_bytes([
            hash[offset], hash[offset + 1], hash[offset + 2], hash[offset + 3],
        ]);
        (val as usize) % self.num_bits
    }

    fn set_bit(&mut self, pos: usize) {
        let word = pos / 64;
        let bit = pos % 64;
        if word < self.bits.len() {
            self.bits[word] |= 1u64 << bit;
        }
    }

    fn get_bit(&self, pos: usize) -> bool {
        let word = pos / 64;
        let bit = pos % 64;
        word < self.bits.len() && (self.bits[word] & (1u64 << bit)) != 0
    }
}

/// Vacuum filter errors.
#[derive(Debug)]
pub enum VacuumError {
    /// Encoded data too short.
    TooShort,
    /// Filters have different parameters, cannot merge.
    IncompatibleFilters,
}

impl std::fmt::Display for VacuumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "Vacuum filter data too short"),
            Self::IncompatibleFilters => write!(f, "Cannot merge incompatible filters"),
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_contains() {
        let mut filter = VacuumFilter::with_defaults(1000);

        filter.insert(b"water boils at 100C");
        assert!(filter.contains(b"water boils at 100C"));
        assert!(!filter.contains(b"water freezes at 0C"));
    }

    #[test]
    fn test_no_false_negatives() {
        let mut filter = VacuumFilter::with_defaults(1000);

        // Insert 100 items
        for i in 0..100u32 {
            filter.insert(&i.to_be_bytes());
        }

        // All must be found (no false negatives)
        for i in 0..100u32 {
            assert!(filter.contains(&i.to_be_bytes()), "Item {} not found", i);
        }
    }

    #[test]
    fn test_fpr_within_bounds() {
        let capacity = 1000;
        let mut filter = VacuumFilter::new(capacity, 0.01); // 1% FPR

        // Insert exactly capacity items
        for i in 0..capacity as u32 {
            filter.insert(&i.to_be_bytes());
        }

        // Test 10,000 non-inserted items
        let mut false_positives = 0;
        for i in capacity as u32..(capacity as u32 + 10_000) {
            if filter.contains(&i.to_be_bytes()) {
                false_positives += 1;
            }
        }

        let observed_fpr = false_positives as f64 / 10_000.0;
        // Allow 5x the target FPR (statistical variance)
        assert!(observed_fpr < 0.05, "FPR too high: {:.4}", observed_fpr);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut filter = VacuumFilter::with_defaults(100);
        filter.insert(b"hello");
        filter.insert(b"world");

        let encoded = filter.encode();
        let decoded = VacuumFilter::decode(&encoded).unwrap();

        assert!(decoded.contains(b"hello"));
        assert!(decoded.contains(b"world"));
        assert!(!decoded.contains(b"missing"));
        assert_eq!(decoded.item_count(), 2);
    }

    #[test]
    fn test_merge_two_filters() {
        let mut filter_a = VacuumFilter::with_defaults(100);
        filter_a.insert(b"alpha");

        let mut filter_b = VacuumFilter::with_defaults(100);
        filter_b.insert(b"beta");

        filter_a.merge(&filter_b).unwrap();

        assert!(filter_a.contains(b"alpha"));
        assert!(filter_a.contains(b"beta"));
    }

    #[test]
    fn test_wire_size() {
        let filter = VacuumFilter::with_defaults(1000);
        let size = filter.wire_size();
        // Should be reasonable: ~1.25 KB for 1000 items at 10 bits/item
        assert!(size > 100 && size < 5000, "Wire size: {} bytes", size);
    }
}
