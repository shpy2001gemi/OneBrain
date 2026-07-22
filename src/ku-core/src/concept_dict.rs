//! ConceptDict — Bidirectional concept name ↔ ConceptID lookup.
//!
//! This module provides an in-memory concept dictionary that maps between
//! human-readable concept names and numeric ConceptIDs used in Core DNA.
//!
//! # Design
//! - **In-memory HashMap** for fast lookup (< 1μs per resolve)
//! - **Pre-loaded** from a data source (file, network, or hardcoded)
//! - **Extensible** — new concepts can be registered at runtime
//! - **Multi-language** — concepts can have names in multiple languages
//!
//! # Future: SQLite Backend
//! The current in-memory implementation will be migrated to SQLite for:
//! - Persistence across sessions
//! - Larger dictionaries (>100K concepts)
//! - Queryable concept metadata (categories, tiers, etc.)
//!
//! # Varint Tier Mapping
//! ConceptIDs are assigned to tiers that correspond to varint byte widths:
//! | Tier | ID Range       | Bytes | Usage              |
//! |------|---------------|-------|--------------------|
//! | 0    | 0–127         | 1     | Core grammar       |
//! | 1    | 128–16,383    | 2     | Common concepts    |
//! | 2    | 16,384–2M     | 3     | Domain knowledge   |
//! | 3    | 2M–268M       | 4     | Specialized terms  |
//! | 4    | 268M+         | 5+    | Rare/unique        |

use crate::error::KuError;
use crate::types::ConceptId;
use std::collections::HashMap;

// ============================================================================
// ConceptDict — in-memory bidirectional lookup
// ============================================================================

/// A concept entry with multilingual names.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConceptEntry {
    /// Numeric concept ID (varint-encoded in Core DNA).
    pub id: ConceptId,
    /// Canonical name (language-neutral or default).
    pub name: String,
    /// Vietnamese name (optional).
    pub name_vi: Option<String>,
    /// English name (optional).
    pub name_en: Option<String>,
    /// Varint tier (0-4, determines byte width).
    pub tier: u8,
    /// Domain category (optional).
    pub category: Option<String>,
}

/// In-memory bidirectional concept dictionary.
///
/// Maps: name → ConceptId and ConceptId → ConceptEntry.
///
/// # Related
/// A simpler, encoding-only [`ConceptDict`](crate::text_parser::ConceptDict)
/// exists in `text_parser` for lightweight word→ID mapping during text parsing.
/// That struct is *not* interchangeable with this one.
///
/// # Thread Safety
/// Not thread-safe. Wrap in `Arc<RwLock<ConceptDict>>` for concurrent access.
pub struct ConceptDict {
    /// ID → Entry lookup.
    by_id: HashMap<ConceptId, ConceptEntry>,
    /// Name → ID lookup (case-insensitive, all languages).
    by_name: HashMap<String, ConceptId>,
    /// Next available ID for auto-registration.
    next_id: ConceptId,
}

impl ConceptDict {
    /// Create an empty dictionary.
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            by_name: HashMap::new(),
            next_id: 128, // Tier 0 (0-127) reserved for core grammar
        }
    }

    /// Create with pre-loaded entries.
    pub fn with_entries(entries: Vec<ConceptEntry>) -> Self {
        let mut dict = Self::new();
        let mut max_id: ConceptId = 127;
        for entry in entries {
            if entry.id > max_id {
                max_id = entry.id;
            }
            dict.insert_entry(entry);
        }
        dict.next_id = max_id + 1;
        dict
    }

    /// Insert a concept entry.
    fn insert_entry(&mut self, entry: ConceptEntry) {
        let id = entry.id;
        // Index all name variants (lowercase for case-insensitive lookup)
        self.by_name.insert(entry.name.to_lowercase(), id);
        if let Some(ref vi) = entry.name_vi {
            self.by_name.insert(vi.to_lowercase(), id);
        }
        if let Some(ref en) = entry.name_en {
            self.by_name.insert(en.to_lowercase(), id);
        }
        self.by_id.insert(id, entry);
    }

    /// Resolve a text name to a ConceptId.
    ///
    /// Case-insensitive lookup across all languages.
    /// Returns `Err(KuError::ConceptNotFound)` if not found.
    pub fn resolve(&self, name: &str) -> Result<ConceptId, KuError> {
        self.by_name
            .get(&name.to_lowercase())
            .copied()
            .ok_or_else(|| KuError::InvalidData(format!("Concept not found: '{}'", name)))
    }

    /// Try to resolve, returning None instead of error.
    pub fn try_resolve(&self, name: &str) -> Option<ConceptId> {
        self.by_name.get(&name.to_lowercase()).copied()
    }

    /// Get the canonical name for a ConceptId.
    pub fn name(&self, id: ConceptId) -> Option<&str> {
        self.by_id.get(&id).map(|e| e.name.as_str())
    }

    /// Get name in a specific language.
    pub fn name_lang(&self, id: ConceptId, lang: &str) -> Option<&str> {
        self.by_id.get(&id).and_then(|e| match lang {
            "vi" => e.name_vi.as_deref().or(Some(e.name.as_str())),
            "en" => e.name_en.as_deref().or(Some(e.name.as_str())),
            _ => Some(e.name.as_str()),
        })
    }

    /// Register a new concept and return its assigned ConceptId.
    ///
    /// The ID is auto-assigned from the next available slot.
    pub fn register(&mut self, name: &str) -> ConceptId {
        // Check if already exists
        if let Some(id) = self.try_resolve(name) {
            return id;
        }

        let id = self.next_id;
        self.next_id += 1;

        let tier = Self::tier_for_id(id);
        let entry = ConceptEntry {
            id,
            name: name.to_string(),
            name_vi: None,
            name_en: None,
            tier,
            category: None,
        };
        self.insert_entry(entry);
        id
    }

    /// Register with multilingual names.
    pub fn register_multilingual(
        &mut self,
        name: &str,
        name_vi: Option<&str>,
        name_en: Option<&str>,
    ) -> ConceptId {
        if let Some(id) = self.try_resolve(name) {
            return id;
        }

        let id = self.next_id;
        self.next_id += 1;

        let entry = ConceptEntry {
            id,
            name: name.to_string(),
            name_vi: name_vi.map(|s| s.to_string()),
            name_en: name_en.map(|s| s.to_string()),
            tier: Self::tier_for_id(id),
            category: None,
        };
        self.insert_entry(entry);
        id
    }

    /// Resolve or auto-register a concept.
    ///
    /// If the concept exists, returns its ID. Otherwise, registers it
    /// and returns the new ID. This is the primary method for KQL CREATE.
    pub fn resolve_or_register(&mut self, name: &str) -> ConceptId {
        match self.try_resolve(name) {
            Some(id) => id,
            None => self.register(name),
        }
    }

    /// Number of concepts in the dictionary.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether the dictionary is empty.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Get all entries (for serialization/export).
    pub fn entries(&self) -> impl Iterator<Item = &ConceptEntry> {
        self.by_id.values()
    }

    /// Determine varint tier for a given ID.
    fn tier_for_id(id: ConceptId) -> u8 {
        match id {
            0..=127 => 0,
            128..=16_383 => 1,
            16_384..=2_097_151 => 2,
            2_097_152..=268_435_455 => 3,
            _ => 4,
        }
    }
}

impl Default for ConceptDict {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let dict = ConceptDict::new();
        assert!(dict.is_empty());
        assert_eq!(dict.len(), 0);
    }

    #[test]
    fn test_register_and_resolve() {
        let mut dict = ConceptDict::new();
        let id = dict.register("water");
        assert_eq!(id, 128); // First user concept (tier 1)
        assert_eq!(dict.resolve("water").unwrap(), 128);
        assert_eq!(dict.name(128), Some("water"));
    }

    #[test]
    fn test_case_insensitive() {
        let mut dict = ConceptDict::new();
        let id = dict.register("Water");
        assert_eq!(dict.resolve("water").unwrap(), id);
        assert_eq!(dict.resolve("WATER").unwrap(), id);
        assert_eq!(dict.resolve("Water").unwrap(), id);
    }

    #[test]
    fn test_multilingual() {
        let mut dict = ConceptDict::new();
        let id = dict.register_multilingual("water", Some("nước"), Some("water"));
        assert_eq!(dict.resolve("nước").unwrap(), id);
        assert_eq!(dict.resolve("water").unwrap(), id);
        assert_eq!(dict.name_lang(id, "vi"), Some("nước"));
        assert_eq!(dict.name_lang(id, "en"), Some("water"));
    }

    #[test]
    fn test_resolve_or_register() {
        let mut dict = ConceptDict::new();
        let id1 = dict.resolve_or_register("boils_at");
        let id2 = dict.resolve_or_register("boils_at"); // same
        let id3 = dict.resolve_or_register("freezes_at"); // different
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_not_found() {
        let dict = ConceptDict::new();
        assert!(dict.resolve("nonexistent").is_err());
        assert_eq!(dict.try_resolve("nonexistent"), None);
    }

    #[test]
    fn test_duplicate_register() {
        let mut dict = ConceptDict::new();
        let id1 = dict.register("oxygen");
        let id2 = dict.register("oxygen"); // should return same ID
        assert_eq!(id1, id2);
        assert_eq!(dict.len(), 1);
    }

    #[test]
    fn test_tier_assignment() {
        assert_eq!(ConceptDict::tier_for_id(0), 0);
        assert_eq!(ConceptDict::tier_for_id(127), 0);
        assert_eq!(ConceptDict::tier_for_id(128), 1);
        assert_eq!(ConceptDict::tier_for_id(16_383), 1);
        assert_eq!(ConceptDict::tier_for_id(16_384), 2);
        assert_eq!(ConceptDict::tier_for_id(2_097_151), 2);
        assert_eq!(ConceptDict::tier_for_id(2_097_152), 3);
    }

    #[test]
    fn test_with_entries() {
        let entries = vec![
            ConceptEntry {
                id: 10,
                name: "is".to_string(),
                name_vi: Some("là".to_string()),
                name_en: None,
                tier: 0,
                category: None,
            },
            ConceptEntry {
                id: 301,
                name: "water".to_string(),
                name_vi: Some("nước".to_string()),
                name_en: Some("water".to_string()),
                tier: 1,
                category: Some("chemistry".to_string()),
            },
        ];
        let dict = ConceptDict::with_entries(entries);
        assert_eq!(dict.len(), 2);
        assert_eq!(dict.resolve("là").unwrap(), 10);
        assert_eq!(dict.resolve("nước").unwrap(), 301);
    }
}
