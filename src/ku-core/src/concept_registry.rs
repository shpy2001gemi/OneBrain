//! # Concept Registry — Offline Concept Lookup
//!
//! Provides O(1) lookup from concept names to CCIDs using an in-memory hash table.
//! Loaded from the `concepts.obr` registry file (~200MB, ~8M concepts).
//!
//! ## Coverage: 99.9% of concept references (Zipf's law)
//!
//! ## Lookup Flow
//! 1. AI extracts concept name from text
//! 2. AI calls `resolve("ngựa vằn")` 
//! 3. Registry does hash lookup → returns CCID
//! 4. If not found → AI creates CCID via fallback

use std::collections::HashMap;
use crate::ccid::Ccid;

// ============================================================================
// ResolveResult — outcome of concept lookup
// ============================================================================

/// Result of resolving a concept name against the registry.
#[derive(Debug, Clone)]
pub enum ResolveResult {
    /// Exact match found — single unambiguous concept.
    Found(ResolvedConcept),
    /// Multiple matches found — AI must disambiguate using context.
    Ambiguous(Vec<ResolvedConcept>),
    /// Fuzzy match found — close but not exact (typo, missing diacritics).
    Fuzzy(ResolvedConcept),
    /// Not found in registry — AI should create fallback CCID.
    NotFound,
}

/// A resolved concept with its metadata.
#[derive(Debug, Clone)]
pub struct ResolvedConcept {
    /// 16-byte CCID (content-addressed concept identity).
    pub ccid: Ccid,
    /// Wikidata QID (0 if not from Wikidata).
    pub qid: u32,
    /// Category of the concept.
    pub category: ConceptCategory,
    /// Canonical name (language-agnostic, typically English).
    pub canonical_name: String,
}

/// Category of a concept in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConceptCategory {
    Entity = 0,
    Property = 1,
    Unit = 2,
    Taxon = 3,
    Place = 4,
    Person = 5,
    Event = 6,
    Substance = 7,
    Other = 255,
}

impl ConceptCategory {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Entity,
            1 => Self::Property,
            2 => Self::Unit,
            3 => Self::Taxon,
            4 => Self::Place,
            5 => Self::Person,
            6 => Self::Event,
            7 => Self::Substance,
            _ => Self::Other,
        }
    }
}

// ============================================================================
// CCID Collision Handling
// ============================================================================

/// Result of adding a concept with collision detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddResult {
    /// New concept inserted successfully.
    Added,
    /// Same CCID and same canonical name — harmless duplicate, labels merged.
    Deduplicated,
    /// Same CCID but DIFFERENT concept — true hash collision detected!
    /// The incoming concept was NOT inserted. See `collision_log()`.
    Collision,
}

/// Record of a detected CCID collision.
///
/// This should effectively never happen in practice (P ≈ 3.67×10⁻¹⁸),
/// but if it does, this record provides forensic evidence for analysis.
#[derive(Debug, Clone)]
pub struct CollisionRecord {
    /// The colliding 16-byte CCID.
    pub ccid: Ccid,
    /// Canonical name of the existing (winning) concept.
    pub existing_name: String,
    /// QID of the existing concept.
    pub existing_qid: u32,
    /// Canonical name of the incoming (rejected) concept.
    pub incoming_name: String,
    /// QID of the incoming concept.
    pub incoming_qid: u32,
    /// When the collision was detected (Unix epoch seconds).
    pub timestamp: u64,
}

impl CollisionRecord {
    /// Format the CCID as hex string for logging.
    pub fn ccid_hex(&self) -> String {
        crate::ccid::ccid_to_hex(&self.ccid)
    }
}

// ============================================================================
// ConceptRegistry — in-memory lookup table
// ============================================================================

/// In-memory concept registry for O(1) name → CCID lookup.
///
/// In production, loaded from `concepts.obr` (~200MB, ~8M entries).
/// For development/testing, can be built manually.
///
/// ## CCID Collision Handling
/// Although 128-bit CCID collision probability is ~3.67×10⁻¹⁸ (for 50B concepts),
/// the registry maintains a `ccid_index` for O(1) duplicate detection and
/// logs any collision events for forensic analysis.
pub struct ConceptRegistry {
    /// Primary index: lowercase name → list of matches.
    label_index: HashMap<String, Vec<usize>>,
    /// Fuzzy index: stripped diacritics → original name.
    fuzzy_index: HashMap<String, String>,
    /// All concept entries.
    entries: Vec<ResolvedConcept>,
    /// CCID → entry index (for O(1) collision detection).
    ccid_index: HashMap<Ccid, usize>,
    /// Collision log: records any CCID conflicts detected.
    collisions: Vec<CollisionRecord>,
}

impl ConceptRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            label_index: HashMap::new(),
            fuzzy_index: HashMap::new(),
            entries: Vec::new(),
            ccid_index: HashMap::new(),
            collisions: Vec::new(),
        }
    }

    /// Create a registry with pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            label_index: HashMap::with_capacity(cap * 3), // ~3 labels per concept
            fuzzy_index: HashMap::with_capacity(cap),
            entries: Vec::with_capacity(cap),
            ccid_index: HashMap::with_capacity(cap),
            collisions: Vec::new(),
        }
    }

    /// Add a concept entry with one or more labels.
    ///
    /// **Warning**: This method does NOT check for CCID collisions.
    /// Use `add_checked()` for collision-safe insertion.
    pub fn add(&mut self, concept: ResolvedConcept, labels: &[&str]) {
        let idx = self.entries.len();
        self.ccid_index.insert(concept.ccid, idx);
        self.entries.push(concept);

        for label in labels {
            let key = label.to_lowercase();
            self.label_index
                .entry(key.clone())
                .or_insert_with(Vec::new)
                .push(idx);

            // Also add stripped-diacritics version to fuzzy index
            let stripped = strip_vietnamese_diacritics(&key);
            if stripped != key {
                self.fuzzy_index.insert(stripped, key);
            }
        }
    }

    /// Add a concept with CCID collision detection.
    ///
    /// Returns:
    /// - `AddResult::Added` — new concept inserted successfully.
    /// - `AddResult::Deduplicated` — same CCID, same canonical name → harmless duplicate.
    /// - `AddResult::Collision` — same CCID, DIFFERENT canonical name → true collision!
    ///
    /// True collisions are logged internally and can be retrieved via `collision_log()`.
    pub fn add_checked(&mut self, concept: ResolvedConcept, labels: &[&str]) -> AddResult {
        // Check for existing CCID
        if let Some(&existing_idx) = self.ccid_index.get(&concept.ccid) {
            let existing = &self.entries[existing_idx];

            if existing.canonical_name == concept.canonical_name
                && existing.qid == concept.qid
            {
                // Same concept, same name → harmless dedup
                // Still add labels (might be new language labels)
                for label in labels {
                    let key = label.to_lowercase();
                    self.label_index
                        .entry(key.clone())
                        .or_insert_with(Vec::new)
                        .push(existing_idx);
                    let stripped = strip_vietnamese_diacritics(&key);
                    if stripped != key {
                        self.fuzzy_index.insert(stripped, key);
                    }
                }
                return AddResult::Deduplicated;
            }

            // Different concept, same CCID → TRUE COLLISION
            let record = CollisionRecord {
                ccid: concept.ccid,
                existing_name: existing.canonical_name.clone(),
                existing_qid: existing.qid,
                incoming_name: concept.canonical_name.clone(),
                incoming_qid: concept.qid,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            self.collisions.push(record);
            return AddResult::Collision;
        }

        // No collision — normal insert
        self.add(concept, labels);
        AddResult::Added
    }

    /// Resolve a concept name to its CCID.
    ///
    /// Returns `Found` for exact single match, `Ambiguous` for multiple matches,
    /// `Fuzzy` for close matches, or `NotFound`.
    pub fn resolve(&self, name: &str) -> ResolveResult {
        let key = name.to_lowercase();

        // 1. Exact match
        if let Some(indices) = self.label_index.get(&key) {
            if indices.len() == 1 {
                return ResolveResult::Found(self.entries[indices[0]].clone());
            } else {
                let matches: Vec<_> = indices.iter().map(|&i| self.entries[i].clone()).collect();
                return ResolveResult::Ambiguous(matches);
            }
        }

        // 2. Fuzzy match (stripped diacritics)
        let stripped = strip_vietnamese_diacritics(&key);
        if let Some(original) = self.fuzzy_index.get(&stripped) {
            if let Some(indices) = self.label_index.get(original) {
                return ResolveResult::Fuzzy(self.entries[indices[0]].clone());
            }
        }

        // 3. Not found
        ResolveResult::NotFound
    }

    /// Number of concepts in the registry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of label entries (for diagnostics).
    pub fn label_count(&self) -> usize {
        self.label_index.len()
    }

    /// Get the collision log.
    ///
    /// Should normally be empty. Any entries indicate a true 128-bit hash collision
    /// (probability ≈ 3.67×10⁻¹⁸) or a bug in canonical form generation.
    pub fn collision_log(&self) -> &[CollisionRecord] {
        &self.collisions
    }

    /// Number of collisions detected.
    pub fn collision_count(&self) -> usize {
        self.collisions.len()
    }

    /// Check if a CCID exists in the registry.
    pub fn has_ccid(&self, ccid: &Ccid) -> bool {
        self.ccid_index.contains_key(ccid)
    }

    /// Look up a concept by its CCID.
    pub fn get_by_ccid(&self, ccid: &Ccid) -> Option<&ResolvedConcept> {
        self.ccid_index.get(ccid).map(|&idx| &self.entries[idx])
    }
}

impl Default for ConceptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper: Vietnamese diacritics stripping
// ============================================================================

/// Strip Vietnamese diacritics from a string for fuzzy matching.
///
/// "ngựa vằn" → "ngua van"
fn strip_vietnamese_diacritics(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ'
            | 'â' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' => 'a',
            'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => 'e',
            'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' => 'i',
            'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ'
            | 'ơ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => 'o',
            'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => 'u',
            'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
            'đ' => 'd',
            // Uppercase
            'Á' | 'À' | 'Ả' | 'Ã' | 'Ạ' | 'Ă' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ'
            | 'Â' | 'Ấ' | 'Ầ' | 'Ẩ' | 'Ẫ' | 'Ậ' => 'a',
            'É' | 'È' | 'Ẻ' | 'Ẽ' | 'Ẹ' | 'Ê' | 'Ế' | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' => 'e',
            'Í' | 'Ì' | 'Ỉ' | 'Ĩ' | 'Ị' => 'i',
            'Ó' | 'Ò' | 'Ỏ' | 'Õ' | 'Ọ' | 'Ô' | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ'
            | 'Ơ' | 'Ớ' | 'Ờ' | 'Ở' | 'Ỡ' | 'Ợ' => 'o',
            'Ú' | 'Ù' | 'Ủ' | 'Ũ' | 'Ụ' | 'Ư' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' => 'u',
            'Ý' | 'Ỳ' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'y',
            'Đ' => 'd',
            other => other,
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_concept(name: &str, qid: u32) -> ResolvedConcept {
        ResolvedConcept {
            ccid: crate::ccid::ccid(format!("wd:Q{}", qid).as_bytes()),
            qid,
            category: ConceptCategory::Entity,
            canonical_name: name.to_string(),
        }
    }

    #[test]
    fn test_resolve_exact() {
        let mut reg = ConceptRegistry::new();
        reg.add(make_concept("water", 283), &["water", "nước", "eau"]);

        match reg.resolve("water") {
            ResolveResult::Found(c) => assert_eq!(c.qid, 283),
            other => panic!("Expected Found, got {:?}", other),
        }

        match reg.resolve("nước") {
            ResolveResult::Found(c) => assert_eq!(c.qid, 283),
            other => panic!("Expected Found, got {:?}", other),
        }

        // Case insensitive
        match reg.resolve("WATER") {
            ResolveResult::Found(c) => assert_eq!(c.qid, 283),
            other => panic!("Expected Found, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_ambiguous() {
        let mut reg = ConceptRegistry::new();
        reg.add(make_concept("bank (financial)", 22687), &["bank"]);
        reg.add(make_concept("bank (river)", 202975), &["bank"]);

        match reg.resolve("bank") {
            ResolveResult::Ambiguous(matches) => assert_eq!(matches.len(), 2),
            other => panic!("Expected Ambiguous, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_fuzzy() {
        let mut reg = ConceptRegistry::new();
        reg.add(make_concept("zebra", 32789), &["ngựa vằn", "zebra"]);

        // Exact match works
        match reg.resolve("ngựa vằn") {
            ResolveResult::Found(c) => assert_eq!(c.qid, 32789),
            other => panic!("Expected Found, got {:?}", other),
        }

        // Fuzzy (no diacritics) also works
        match reg.resolve("ngua van") {
            ResolveResult::Fuzzy(c) => assert_eq!(c.qid, 32789),
            other => panic!("Expected Fuzzy, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_not_found() {
        let reg = ConceptRegistry::new();
        match reg.resolve("nonexistent") {
            ResolveResult::NotFound => {} // OK
            other => panic!("Expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_strip_diacritics() {
        assert_eq!(strip_vietnamese_diacritics("ngựa vằn"), "ngua van");
        assert_eq!(strip_vietnamese_diacritics("châu phi"), "chau phi");
        assert_eq!(strip_vietnamese_diacritics("đà nẵng"), "da nang");
        assert_eq!(strip_vietnamese_diacritics("hello"), "hello");
    }

    #[test]
    fn test_registry_counts() {
        let mut reg = ConceptRegistry::new();
        assert_eq!(reg.len(), 0);
        assert!(reg.is_empty());

        reg.add(make_concept("water", 283), &["water", "nước"]);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.label_count(), 2); // "water" + "nước"
    }

    // ── CCID Collision Handling Tests ─────────────────────────────────

    #[test]
    fn test_add_checked_new_concept() {
        let mut reg = ConceptRegistry::new();
        let result = reg.add_checked(make_concept("water", 283), &["water"]);
        assert_eq!(result, AddResult::Added);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.collision_count(), 0);
    }

    #[test]
    fn test_add_checked_dedup_same_concept() {
        let mut reg = ConceptRegistry::new();

        // Add water first time
        let r1 = reg.add_checked(make_concept("water", 283), &["water"]);
        assert_eq!(r1, AddResult::Added);

        // Add same concept again (same name, same QID → dedup)
        let r2 = reg.add_checked(make_concept("water", 283), &["eau", "nước"]);
        assert_eq!(r2, AddResult::Deduplicated);

        // Should NOT add a second entry
        assert_eq!(reg.len(), 1);
        // But should add the new labels
        assert_eq!(reg.collision_count(), 0);
    }

    #[test]
    fn test_add_checked_true_collision() {
        let mut reg = ConceptRegistry::new();

        // Add water normally
        let water_ccid = crate::ccid::ccid(b"wd:Q283");
        reg.add_checked(
            ResolvedConcept {
                ccid: water_ccid,
                qid: 283,
                category: ConceptCategory::Entity,
                canonical_name: "water".into(),
            },
            &["water"],
        );

        // Simulate collision: SAME CCID but different concept
        let result = reg.add_checked(
            ResolvedConcept {
                ccid: water_ccid, // SAME CCID!
                qid: 999999,
                category: ConceptCategory::Substance,
                canonical_name: "totally_different_thing".into(),
            },
            &["totally_different_thing"],
        );

        assert_eq!(result, AddResult::Collision);
        assert_eq!(reg.len(), 1); // Collision was rejected, not inserted
        assert_eq!(reg.collision_count(), 1);

        // Check collision record
        let log = reg.collision_log();
        assert_eq!(log[0].ccid, water_ccid);
        assert_eq!(log[0].existing_name, "water");
        assert_eq!(log[0].existing_qid, 283);
        assert_eq!(log[0].incoming_name, "totally_different_thing");
        assert_eq!(log[0].incoming_qid, 999999);
    }

    #[test]
    fn test_has_ccid_and_get_by_ccid() {
        let mut reg = ConceptRegistry::new();
        let water_ccid = crate::ccid::ccid(b"wd:Q283");
        let fire_ccid = crate::ccid::ccid(b"wd:Q3196");

        reg.add(make_concept("water", 283), &["water"]);

        assert!(reg.has_ccid(&water_ccid));
        assert!(!reg.has_ccid(&fire_ccid));

        let found = reg.get_by_ccid(&water_ccid).unwrap();
        assert_eq!(found.qid, 283);
        assert_eq!(found.canonical_name, "water");

        assert!(reg.get_by_ccid(&fire_ccid).is_none());
    }

    #[test]
    fn test_collision_record_hex() {
        let ccid = crate::ccid::ccid(b"wd:Q283");
        let record = CollisionRecord {
            ccid,
            existing_name: "a".into(),
            existing_qid: 1,
            incoming_name: "b".into(),
            incoming_qid: 2,
            timestamp: 0,
        };
        let hex = record.ccid_hex();
        assert_eq!(hex.len(), 32); // 16 bytes × 2 hex chars
    }

    #[test]
    fn test_multiple_collisions_logged() {
        let mut reg = ConceptRegistry::new();
        let ccid = crate::ccid::ccid(b"wd:Q283");

        // Add original
        reg.add_checked(
            ResolvedConcept { ccid, qid: 283, category: ConceptCategory::Entity, canonical_name: "water".into() },
            &["water"],
        );

        // Collision #1
        reg.add_checked(
            ResolvedConcept { ccid, qid: 100, category: ConceptCategory::Entity, canonical_name: "fake1".into() },
            &["fake1"],
        );
        // Collision #2
        reg.add_checked(
            ResolvedConcept { ccid, qid: 200, category: ConceptCategory::Entity, canonical_name: "fake2".into() },
            &["fake2"],
        );

        assert_eq!(reg.collision_count(), 2);
        assert_eq!(reg.collision_log()[0].incoming_name, "fake1");
        assert_eq!(reg.collision_log()[1].incoming_name, "fake2");
    }
}
