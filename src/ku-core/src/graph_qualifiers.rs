//! # Graph Qualifiers — Contextual Bond Metadata
//!
//! Bond qualifiers enrich edges with contextual metadata, inspired by
//! Wikidata's qualifier system. A bond "A-[Causes]->B" can be qualified
//! with temporal scope, confidence, source attribution, and custom data.
//!
//! ## Examples
//! - "Einstein-[AuthoredBy]->Relativity" + ValidFrom(1905)
//! - "Vaccine-[Prevents]->Disease" + Confidence(0.95) + Source(paper_cid)
//! - "A-[Causes]->B" + Context("physics") + ValidUntil(2030)
//!
//! ## Design
//! - Qualifiers are stored as a compact Vec alongside each bond
//! - Fixed keys use an enum for type safety
//! - Custom keys use u16 for extensibility
//! - Values are typed (temporal, numeric, CID reference, string)

use serde::{Deserialize, Serialize};

// ============================================================================
// 1. QualifierKey — Well-known qualifier keys
// ============================================================================

/// Well-known qualifier keys.
///
/// Fixed keys cover common use cases. Custom(u16) allows
/// domain-specific extensions without modifying the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum QualifierKey {
    /// Temporal scope: bond is valid from this timestamp
    ValidFrom = 0,
    /// Temporal scope: bond is valid until this timestamp
    ValidUntil = 1,
    /// Creator's confidence in this bond [0.0, 1.0]
    Confidence = 2,
    /// Source evidence: CID of the KU that supports this bond
    Source = 3,
    /// Microtheory/context: restricts bond to a domain
    Context = 4,
    /// Geographic scope: where this bond applies
    Location = 5,
    /// Language: which language version this bond applies to
    Language = 6,
    /// Rank/priority among parallel bonds
    Rank = 7,
    /// Extensible: domain-specific qualifier
    Custom = 255,
}

// ============================================================================
// 2. BondQualifierValue — Typed qualifier value
// ============================================================================

/// Typed qualifier value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BondQualifierValue {
    /// Unix timestamp (seconds)
    Timestamp(u64),
    /// Floating-point value (confidence, score, etc.)
    Float(f64),
    /// Integer value (rank, count, etc.)
    Integer(i64),
    /// CID reference to another KU
    Cid([u8; 32]),
    /// Short string value (context name, language code, etc.)
    Text(String),
    /// Boolean flag
    Bool(bool),
}

// ============================================================================
// 3. BondQualifier — A single qualifier attached to a bond
// ============================================================================

/// A single qualifier attached to a bond.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BondQualifier {
    pub key: QualifierKey,
    /// Custom key ID (only used when key == Custom)
    pub custom_key_id: Option<u16>,
    pub value: BondQualifierValue,
}

impl BondQualifier {
    /// Create a standard qualifier.
    pub fn new(key: QualifierKey, value: BondQualifierValue) -> Self {
        Self {
            key,
            custom_key_id: None,
            value,
        }
    }

    /// Create a custom qualifier with a domain-specific key.
    pub fn custom(key_id: u16, value: BondQualifierValue) -> Self {
        Self {
            key: QualifierKey::Custom,
            custom_key_id: Some(key_id),
            value,
        }
    }

    /// Create a temporal ValidFrom qualifier.
    pub fn valid_from(timestamp: u64) -> Self {
        Self::new(
            QualifierKey::ValidFrom,
            BondQualifierValue::Timestamp(timestamp),
        )
    }

    /// Create a temporal ValidUntil qualifier.
    pub fn valid_until(timestamp: u64) -> Self {
        Self::new(
            QualifierKey::ValidUntil,
            BondQualifierValue::Timestamp(timestamp),
        )
    }

    /// Create a confidence qualifier (clamped to [0.0, 1.0]).
    pub fn confidence(value: f64) -> Self {
        Self::new(
            QualifierKey::Confidence,
            BondQualifierValue::Float(value.clamp(0.0, 1.0)),
        )
    }

    /// Create a source qualifier (reference to evidence KU).
    pub fn source(cid: [u8; 32]) -> Self {
        Self::new(QualifierKey::Source, BondQualifierValue::Cid(cid))
    }

    /// Create a context qualifier.
    pub fn context(name: &str) -> Self {
        Self::new(
            QualifierKey::Context,
            BondQualifierValue::Text(name.to_string()),
        )
    }

    /// Create a rank qualifier.
    pub fn rank(r: i64) -> Self {
        Self::new(QualifierKey::Rank, BondQualifierValue::Integer(r))
    }
}

// ============================================================================
// 4. QualifiedBond — A bond with attached qualifiers
// ============================================================================

/// A bond with attached qualifiers.
///
/// Wraps a bond key (source, target, relation) with zero or more qualifiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualifiedBond {
    pub source_cid: [u8; 32],
    pub target_cid: [u8; 32],
    pub relation: crate::types::RelationType,
    pub weight: u16,
    pub qualifiers: Vec<BondQualifier>,
}

impl QualifiedBond {
    /// Create a new bond with no qualifiers.
    pub fn new(
        source: [u8; 32],
        target: [u8; 32],
        relation: crate::types::RelationType,
        weight: u16,
    ) -> Self {
        Self {
            source_cid: source,
            target_cid: target,
            relation,
            weight,
            qualifiers: Vec::new(),
        }
    }

    /// Add a qualifier (builder pattern).
    pub fn with_qualifier(mut self, q: BondQualifier) -> Self {
        self.qualifiers.push(q);
        self
    }

    /// Get all qualifiers with a specific key.
    pub fn get_qualifiers(&self, key: QualifierKey) -> Vec<&BondQualifier> {
        self.qualifiers.iter().filter(|q| q.key == key).collect()
    }

    /// Get first qualifier with a specific key.
    pub fn get_qualifier(&self, key: QualifierKey) -> Option<&BondQualifier> {
        self.qualifiers.iter().find(|q| q.key == key)
    }

    /// Check if bond is temporally valid at a given time.
    pub fn is_valid_at(&self, timestamp: u64) -> bool {
        let valid_from = self
            .get_qualifier(QualifierKey::ValidFrom)
            .and_then(|q| match &q.value {
                BondQualifierValue::Timestamp(t) => Some(*t),
                _ => None,
            })
            .unwrap_or(0); // No ValidFrom = always valid from start

        let valid_until = self
            .get_qualifier(QualifierKey::ValidUntil)
            .and_then(|q| match &q.value {
                BondQualifierValue::Timestamp(t) => Some(*t),
                _ => None,
            })
            .unwrap_or(u64::MAX); // No ValidUntil = always valid

        timestamp >= valid_from && timestamp <= valid_until
    }

    /// Get confidence value (default: 1.0 if not specified).
    pub fn confidence(&self) -> f64 {
        self.get_qualifier(QualifierKey::Confidence)
            .and_then(|q| match &q.value {
                BondQualifierValue::Float(f) => Some(*f),
                _ => None,
            })
            .unwrap_or(1.0)
    }

    /// Get context name (if specified).
    pub fn context(&self) -> Option<&str> {
        self.get_qualifier(QualifierKey::Context)
            .and_then(|q| match &q.value {
                BondQualifierValue::Text(s) => Some(s.as_str()),
                _ => None,
            })
    }

    /// Count qualifiers.
    pub fn qualifier_count(&self) -> usize {
        self.qualifiers.len()
    }

    /// Estimated serialized size in bytes.
    pub fn estimated_size(&self) -> usize {
        // source(32) + target(32) + relation(1) + weight(2) = 67 base
        32 + 32
            + 1
            + 2
            + self
                .qualifiers
                .iter()
                .map(|q| {
                    // key(1) + custom_key_id(2) + value
                    1 + 2
                        + match &q.value {
                            BondQualifierValue::Timestamp(_) => 8,
                            BondQualifierValue::Float(_) => 8,
                            BondQualifierValue::Integer(_) => 8,
                            BondQualifierValue::Cid(_) => 32,
                            BondQualifierValue::Text(s) => 2 + s.len(),
                            BondQualifierValue::Bool(_) => 1,
                        }
                })
                .sum::<usize>()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RelationType;

    fn dummy_cid(fill: u8) -> [u8; 32] {
        [fill; 32]
    }

    // --- BondQualifier factory tests ---

    #[test]
    fn qualifier_valid_from() {
        let q = BondQualifier::valid_from(1_700_000_000);
        assert_eq!(q.key, QualifierKey::ValidFrom);
        assert_eq!(q.value, BondQualifierValue::Timestamp(1_700_000_000));
        assert!(q.custom_key_id.is_none());
    }

    #[test]
    fn qualifier_valid_until() {
        let q = BondQualifier::valid_until(1_800_000_000);
        assert_eq!(q.key, QualifierKey::ValidUntil);
        assert_eq!(q.value, BondQualifierValue::Timestamp(1_800_000_000));
    }

    #[test]
    fn qualifier_confidence_clamped() {
        // Values above 1.0 should be clamped
        let q_high = BondQualifier::confidence(1.5);
        assert_eq!(q_high.value, BondQualifierValue::Float(1.0));

        // Values below 0.0 should be clamped
        let q_low = BondQualifier::confidence(-0.5);
        assert_eq!(q_low.value, BondQualifierValue::Float(0.0));

        // Normal value should pass through
        let q_ok = BondQualifier::confidence(0.85);
        assert_eq!(q_ok.value, BondQualifierValue::Float(0.85));
    }

    #[test]
    fn qualifier_source() {
        let cid = dummy_cid(0xAB);
        let q = BondQualifier::source(cid);
        assert_eq!(q.key, QualifierKey::Source);
        assert_eq!(q.value, BondQualifierValue::Cid(cid));
    }

    #[test]
    fn qualifier_context() {
        let q = BondQualifier::context("physics");
        assert_eq!(q.key, QualifierKey::Context);
        assert_eq!(q.value, BondQualifierValue::Text("physics".to_string()));
    }

    #[test]
    fn qualifier_custom() {
        let q = BondQualifier::custom(42, BondQualifierValue::Bool(true));
        assert_eq!(q.key, QualifierKey::Custom);
        assert_eq!(q.custom_key_id, Some(42));
        assert_eq!(q.value, BondQualifierValue::Bool(true));
    }

    // --- QualifiedBond tests ---

    #[test]
    fn qualified_bond_new() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500);
        assert_eq!(bond.source_cid, dummy_cid(0x01));
        assert_eq!(bond.target_cid, dummy_cid(0x02));
        assert_eq!(bond.weight, 500);
        assert!(bond.qualifiers.is_empty());
    }

    #[test]
    fn qualified_bond_with_qualifier() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::confidence(0.9))
            .with_qualifier(BondQualifier::context("medicine"));

        assert_eq!(bond.qualifier_count(), 2);
    }

    #[test]
    fn qualified_bond_is_valid_at_no_temporal() {
        // No temporal qualifiers → always valid
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500);
        assert!(bond.is_valid_at(0));
        assert!(bond.is_valid_at(u64::MAX));
    }

    #[test]
    fn qualified_bond_is_valid_at_with_range() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::valid_from(100))
            .with_qualifier(BondQualifier::valid_until(200));

        assert!(bond.is_valid_at(100));
        assert!(bond.is_valid_at(150));
        assert!(bond.is_valid_at(200));
    }

    #[test]
    fn qualified_bond_is_valid_at_before_start() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::valid_from(100));

        assert!(!bond.is_valid_at(50));
        assert!(bond.is_valid_at(100));
        assert!(bond.is_valid_at(999));
    }

    #[test]
    fn qualified_bond_is_valid_at_after_end() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::valid_until(200));

        assert!(bond.is_valid_at(0));
        assert!(bond.is_valid_at(200));
        assert!(!bond.is_valid_at(201));
    }

    #[test]
    fn qualified_bond_confidence_default() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500);
        assert!((bond.confidence() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn qualified_bond_confidence_custom() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::confidence(0.42));

        assert!((bond.confidence() - 0.42).abs() < f64::EPSILON);
    }

    #[test]
    fn qualified_bond_context() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::context("biology"));

        assert_eq!(bond.context(), Some("biology"));
    }

    #[test]
    fn qualified_bond_get_qualifiers() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::context("physics"))
            .with_qualifier(BondQualifier::confidence(0.8))
            .with_qualifier(BondQualifier::context("chemistry"));

        let contexts = bond.get_qualifiers(QualifierKey::Context);
        assert_eq!(contexts.len(), 2);
    }

    #[test]
    fn qualified_bond_estimated_size() {
        let bond = QualifiedBond::new(dummy_cid(0x01), dummy_cid(0x02), RelationType::Causes, 500)
            .with_qualifier(BondQualifier::confidence(0.9))
            .with_qualifier(BondQualifier::context("test"));

        let size = bond.estimated_size();
        // Base: 32+32+1+2 = 67
        // Confidence: 1+2+8 = 11
        // Context("test"): 1+2+2+4 = 9
        // Total: 67 + 11 + 9 = 87
        assert_eq!(size, 87);
        assert!(size > 67); // Must be bigger than base
    }
}
