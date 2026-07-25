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

use crate::ccid::Ccid;
use crate::concept_registry_manifest::ObrHeaderMetadata;
use std::collections::HashMap;

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

/// Operational failure while consulting a concept registry backend.
///
/// This is deliberately distinct from [`ResolveResult::NotFound`]: an absent
/// label may use the deterministic fallback namespace, while an unavailable or
/// damaged registry must stop v2 encoding instead of minting a different CCID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptLookupError {
    message: String,
}

impl ConceptLookupError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConceptLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConceptLookupError {}

/// Lookup contract shared by the legacy in-memory registry and bounded
/// on-demand registry backends.
pub trait ConceptLookup: Send + Sync {
    fn resolve(&self, name: &str) -> ResolveResult;

    /// Checked lookup used by production encoding. In-memory implementations
    /// cannot fail after construction, while indexed backends override this to
    /// preserve I/O and artifact-integrity failures.
    fn resolve_checked(&self, name: &str) -> Result<ResolveResult, ConceptLookupError> {
        Ok(self.resolve(name))
    }
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
#[derive(Debug)]
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

            if existing.canonical_name == concept.canonical_name && existing.qid == concept.qid {
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

    // ════════════════════════════════════════════════════════════════════
    // OBR1 Loader — load from concepts.obr binary file
    // ════════════════════════════════════════════════════════════════════

    /// OBR1 file magic bytes.
    const OBR_MAGIC: &'static [u8; 4] = b"OBR1";
    /// OBR header size in bytes.
    const OBR_HEADER_SIZE: usize = 32;
    const MAX_OBR_ENTRIES: u64 = 100_000_000;

    /// Read and validate only the fixed OBR header without allocating indexes.
    pub fn inspect_obr(path: &std::path::Path) -> Result<ObrHeaderMetadata, ObrLoadError> {
        use std::io::Read;

        let mut file = std::fs::File::open(path).map_err(ObrLoadError::Io)?;
        let file_len = file.metadata().map_err(ObrLoadError::Io)?.len();
        let mut header = [0u8; Self::OBR_HEADER_SIZE];
        file.read_exact(&mut header).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                ObrLoadError::TruncatedHeader
            } else {
                ObrLoadError::Io(error)
            }
        })?;
        if &header[0..4] != Self::OBR_MAGIC {
            return Err(ObrLoadError::InvalidMagic);
        }
        let schema_version = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if schema_version != 1 {
            return Err(ObrLoadError::UnsupportedVersion(schema_version));
        }
        let entry_count = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let label_count = u64::from_le_bytes(header[16..24].try_into().unwrap());
        if entry_count > Self::MAX_OBR_ENTRIES {
            return Err(ObrLoadError::ResourceLimit {
                entries: entry_count,
                max_entries: Self::MAX_OBR_ENTRIES,
            });
        }
        let minimum_len = (Self::OBR_HEADER_SIZE as u64)
            .checked_add(
                entry_count
                    .checked_mul(26)
                    .ok_or(ObrLoadError::ResourceLimit {
                        entries: entry_count,
                        max_entries: Self::MAX_OBR_ENTRIES,
                    })?,
            )
            .ok_or(ObrLoadError::ResourceLimit {
                entries: entry_count,
                max_entries: Self::MAX_OBR_ENTRIES,
            })?;
        if file_len < minimum_len {
            return Err(ObrLoadError::TruncatedBody {
                expected_minimum: minimum_len,
                actual: file_len,
            });
        }
        Ok(ObrHeaderMetadata {
            schema_version,
            entry_count,
            label_count,
        })
    }

    /// Load a ConceptRegistry from an OBR1 binary file.
    ///
    /// ## OBR1 Format
    /// ```text
    /// Header (32 bytes):
    ///   [0..4]   magic "OBR1"
    ///   [4..8]   version (u32 LE, must be 1)
    ///   [8..16]  entry_count (u64 LE)
    ///   [16..24] label_count (u64 LE)
    ///   [24..32] reserved
    ///
    /// Per entry:
    ///   [16 bytes]  CCID (blake3 truncated)
    ///   [4 bytes]   ext_id (u32 LE — Wikidata QID or source-specific ID)
    ///   [1 byte]    source (u8 — 0=Wikidata, 1=GeoNames, etc.)
    ///   [1 byte]    category (ConceptCategory as u8)
    ///   [2 bytes]   name_len (u16 LE)
    ///   [name_len]  name UTF-8 string (canonical name)
    ///   [2 bytes]   num_labels (u16 LE)
    ///   For each label:
    ///     [2 bytes]   len (u16 LE)
    ///     [len bytes] UTF-8 string
    /// ```
    pub fn load_obr(path: &std::path::Path) -> Result<Self, ObrLoadError> {
        use std::io::Read;

        let metadata = Self::inspect_obr(path)?;

        let mut file = std::fs::File::open(path).map_err(|e| ObrLoadError::Io(e))?;

        // Read header
        let mut header = [0u8; Self::OBR_HEADER_SIZE];
        file.read_exact(&mut header).map_err(ObrLoadError::Io)?;

        // Validate magic
        if &header[0..4] != Self::OBR_MAGIC {
            return Err(ObrLoadError::InvalidMagic);
        }

        // Validate version
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if version != 1 {
            return Err(ObrLoadError::UnsupportedVersion(version));
        }

        let entry_count = metadata.entry_count as usize;

        // Read entire file into memory for fast parsing (avoids syscall per entry)
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| ObrLoadError::Io(e))?;

        #[cfg(test)]
        eprintln!(
            "  [OBR] data_len={}, entry_count={}",
            data.len(),
            entry_count
        );

        // Pre-allocate
        let mut registry = Self::with_capacity(entry_count);

        let mut pos: usize = 0;
        let mut loaded: usize = 0;

        for entry_idx in 0..entry_count {
            // Minimum entry: CCID(16) + ext_id(4) + source(1) + cat(1) + name_len(2) + num_labels(2) = 26
            if pos + 26 > data.len() {
                return Err(ObrLoadError::TruncatedEntry(entry_idx as u64));
            }

            // CCID (16 bytes)
            let mut ccid = [0u8; 16];
            ccid.copy_from_slice(&data[pos..pos + 16]);
            pos += 16;

            // ext_id (u32 LE) — Wikidata QID or source-specific ID
            let qid = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            pos += 4;

            // source (u8) — 0=Wikidata, 1=GeoNames, etc.
            let _source = data[pos];
            pos += 1;

            // category (u8)
            let category = ConceptCategory::from_u8(data[pos]);
            pos += 1;

            // name_len (u16 LE) + name_bytes — canonical name
            if pos + 2 > data.len() {
                return Err(ObrLoadError::TruncatedEntry(entry_idx as u64));
            }
            let name_len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            pos += 2;
            if pos + name_len > data.len() {
                return Err(ObrLoadError::TruncatedEntry(entry_idx as u64));
            }

            let canonical_name = match std::str::from_utf8(&data[pos..pos + name_len]) {
                Ok(s) => s.to_string(),
                Err(_) => String::new(),
            };
            pos += name_len;

            // num_labels (u16 LE)
            if pos + 2 > data.len() {
                return Err(ObrLoadError::TruncatedEntry(entry_idx as u64));
            }
            let n_labels = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            pos += 2;

            // Parse labels: [label_len(u16) + label_bytes]*
            let mut labels = Vec::with_capacity(n_labels + 1);
            if !canonical_name.is_empty() {
                labels.push(canonical_name.clone());
            }

            for _i in 0..n_labels {
                if pos + 2 > data.len() {
                    return Err(ObrLoadError::TruncatedEntry(entry_idx as u64));
                }
                let len = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                pos += 2;
                if pos + len > data.len() {
                    return Err(ObrLoadError::TruncatedEntry(entry_idx as u64));
                }

                if let Ok(s) = std::str::from_utf8(&data[pos..pos + len]) {
                    labels.push(s.to_string());
                }
                pos += len;
            }

            if !canonical_name.is_empty() {
                let concept = ResolvedConcept {
                    ccid,
                    qid,
                    category,
                    canonical_name,
                };

                let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                registry.add(concept, &label_refs);
                loaded += 1;
            }
        }

        if loaded == 0 && entry_count > 0 {
            return Err(ObrLoadError::NoEntriesLoaded);
        }
        if loaded != entry_count {
            return Err(ObrLoadError::EntryCountMismatch {
                declared: entry_count as u64,
                loaded: loaded as u64,
            });
        }

        Ok(registry)
    }
}

/// Errors that can occur when loading an OBR file.
#[derive(Debug)]
pub enum ObrLoadError {
    /// I/O error reading the file.
    Io(std::io::Error),
    /// File doesn't start with "OBR1" magic bytes.
    InvalidMagic,
    /// OBR version is not supported (only version 1 is supported).
    UnsupportedVersion(u32),
    /// Fixed header ended before 32 bytes.
    TruncatedHeader,
    /// File is too short to contain the declared number of minimum entries.
    TruncatedBody { expected_minimum: u64, actual: u64 },
    /// An individual variable-length record ended unexpectedly.
    TruncatedEntry(u64),
    /// Header requests an unsafe allocation.
    ResourceLimit { entries: u64, max_entries: u64 },
    /// File parsed but no valid entries were loaded.
    NoEntriesLoaded,
    /// Parsed record count differs from the authenticated header/manifest.
    EntryCountMismatch { declared: u64, loaded: u64 },
}

impl std::fmt::Display for ObrLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObrLoadError::Io(e) => write!(f, "OBR I/O error: {}", e),
            ObrLoadError::InvalidMagic => write!(f, "Invalid OBR magic (expected OBR1)"),
            ObrLoadError::UnsupportedVersion(v) => write!(f, "Unsupported OBR version: {}", v),
            ObrLoadError::TruncatedHeader => write!(f, "Truncated OBR header"),
            ObrLoadError::TruncatedBody {
                expected_minimum,
                actual,
            } => write!(
                f,
                "Truncated OBR body: expected at least {expected_minimum} bytes, found {actual}"
            ),
            ObrLoadError::TruncatedEntry(index) => {
                write!(f, "Truncated OBR entry at index {index}")
            }
            ObrLoadError::ResourceLimit {
                entries,
                max_entries,
            } => write!(
                f,
                "OBR entry count exceeds resource limit: {entries} > {max_entries}"
            ),
            ObrLoadError::NoEntriesLoaded => write!(f, "OBR file parsed but no entries loaded"),
            ObrLoadError::EntryCountMismatch { declared, loaded } => write!(
                f,
                "OBR entry count mismatch: declared={declared}, loaded={loaded}"
            ),
        }
    }
}

impl std::error::Error for ObrLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ObrLoadError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl Default for ConceptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConceptLookup for ConceptRegistry {
    fn resolve(&self, name: &str) -> ResolveResult {
        ConceptRegistry::resolve(self, name)
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
            'á' | 'à' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ấ' | 'ầ'
            | 'ẩ' | 'ẫ' | 'ậ' => 'a',
            'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => {
                'e'
            }
            'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' => 'i',
            'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ớ' | 'ờ'
            | 'ở' | 'ỡ' | 'ợ' => 'o',
            'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => {
                'u'
            }
            'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
            'đ' => 'd',
            // Uppercase
            'Á' | 'À' | 'Ả' | 'Ã' | 'Ạ' | 'Ă' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ' | 'Â' | 'Ấ' | 'Ầ'
            | 'Ẩ' | 'Ẫ' | 'Ậ' => 'a',
            'É' | 'È' | 'Ẻ' | 'Ẽ' | 'Ẹ' | 'Ê' | 'Ế' | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' => {
                'e'
            }
            'Í' | 'Ì' | 'Ỉ' | 'Ĩ' | 'Ị' => 'i',
            'Ó' | 'Ò' | 'Ỏ' | 'Õ' | 'Ọ' | 'Ô' | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ' | 'Ơ' | 'Ớ' | 'Ờ'
            | 'Ở' | 'Ỡ' | 'Ợ' => 'o',
            'Ú' | 'Ù' | 'Ủ' | 'Ũ' | 'Ụ' | 'Ư' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' => {
                'u'
            }
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
            ResolvedConcept {
                ccid,
                qid: 283,
                category: ConceptCategory::Entity,
                canonical_name: "water".into(),
            },
            &["water"],
        );

        // Collision #1
        reg.add_checked(
            ResolvedConcept {
                ccid,
                qid: 100,
                category: ConceptCategory::Entity,
                canonical_name: "fake1".into(),
            },
            &["fake1"],
        );
        // Collision #2
        reg.add_checked(
            ResolvedConcept {
                ccid,
                qid: 200,
                category: ConceptCategory::Entity,
                canonical_name: "fake2".into(),
            },
            &["fake2"],
        );

        assert_eq!(reg.collision_count(), 2);
        assert_eq!(reg.collision_log()[0].incoming_name, "fake1");
        assert_eq!(reg.collision_log()[1].incoming_name, "fake2");
    }

    // ── OBR Loader tests ─────────────────────────────────────────────

    /// Build a minimal OBR1 binary in memory for testing.
    /// Format: ccid(16) + ext_id(u32) + source(u8) + category(u8) +
    ///         name_len(u16) + name + num_labels(u16) + [label_len(u16) + label]*
    fn build_test_obr(entries: &[(&[u8; 16], u32, u8, u8, &str, &[&str])]) -> Vec<u8> {
        let mut buf = Vec::new();
        // Header
        buf.extend_from_slice(b"OBR1"); // magic
        buf.extend_from_slice(&1u32.to_le_bytes()); // version
        buf.extend_from_slice(&(entries.len() as u64).to_le_bytes()); // entry_count
        let label_count: u64 = entries.iter().map(|e| e.5.len() as u64).sum();
        buf.extend_from_slice(&label_count.to_le_bytes()); // label_count
        buf.extend_from_slice(&[0u8; 8]); // reserved

        for (ccid, ext_id, source, cat, name, labels) in entries {
            buf.extend_from_slice(*ccid); // CCID (16)
            buf.extend_from_slice(&ext_id.to_le_bytes()); // ext_id (u32 LE)
            buf.push(*source); // source (u8)
            buf.push(*cat); // category (u8)
            let name_bytes = name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes()); // name_len
            buf.extend_from_slice(name_bytes); // name
            buf.extend_from_slice(&(labels.len() as u16).to_le_bytes()); // num_labels
            for label in *labels {
                let bytes = label.as_bytes();
                buf.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                buf.extend_from_slice(bytes);
            }
        }

        buf
    }

    #[test]
    fn test_load_obr_synthetic() {
        let ccid1 = crate::ccid::ccid(b"wd:Q283");
        let ccid2 = crate::ccid::ccid(b"wd:Q5");

        let data = build_test_obr(&[
            // (ccid, ext_id, source, cat, name, labels)
            (&ccid1, 283, 0, 7, "water", &["nước", "eau"]),
            (&ccid2, 5, 0, 5, "human", &["con người"]),
        ]);

        // Write to temp file
        let dir = std::env::temp_dir().join("obr_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_concepts.obr");
        std::fs::write(&path, &data).unwrap();

        let reg = ConceptRegistry::load_obr(&path).unwrap();
        assert_eq!(reg.len(), 2);

        // Check "water" resolves
        match reg.resolve("water") {
            ResolveResult::Found(c) => {
                assert_eq!(c.qid, 283);
                assert_eq!(c.canonical_name, "water");
            }
            other => panic!("Expected Found, got {:?}", other),
        }

        // Check Vietnamese label
        match reg.resolve("nước") {
            ResolveResult::Found(c) => assert_eq!(c.qid, 283),
            other => panic!("Expected Found, got {:?}", other),
        }

        // Check "human"
        match reg.resolve("human") {
            ResolveResult::Found(c) => assert_eq!(c.qid, 5),
            other => panic!("Expected Found, got {:?}", other),
        }

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_obr_invalid_magic() {
        let dir = std::env::temp_dir().join("obr_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("bad_magic.obr");
        std::fs::write(&path, b"NOPE000000000000000000000000000000").unwrap();

        let result = ConceptRegistry::load_obr(&path);
        let err = result.unwrap_err();
        assert!(
            matches!(err, ObrLoadError::InvalidMagic),
            "Expected InvalidMagic, got: {}",
            err
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_obr_file_not_found() {
        let result = ConceptRegistry::load_obr(std::path::Path::new("/nonexistent/path.obr"));
        let err = result.unwrap_err();
        assert!(
            matches!(err, ObrLoadError::Io(_)),
            "Expected Io, got: {}",
            err
        );
    }

    /// Integration test: load real concepts.obr if available.
    ///
    /// The current production artifact is about 1.3 GB and the legacy loader
    /// materializes it in memory. Keep this outside the default workspace gate;
    /// P0 uses the small build/verify fixture plus indexed on-demand tests.
    #[test]
    #[ignore = "explicit full-registry drill; legacy materialization can require more than 8 GiB"]
    fn test_load_obr_real_file() {
        // Build path relative to CARGO_MANIFEST_DIR (ku-core crate root)
        // ku-core is at src/ku-core, so concepts.obr is at ../../onebrain_data/concepts.obr
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidates = [
            manifest_dir.join("../../onebrain_data/concepts.obr"),
            manifest_dir.join("../onebrain_data/concepts.obr"),
        ];
        let path = match candidates.iter().find(|p| p.exists()) {
            Some(p) => p,
            None => {
                eprintln!(
                    "  [SKIP] concepts.obr not found (tried from {:?})",
                    manifest_dir
                );
                return;
            }
        };

        let reg = ConceptRegistry::load_obr(path).expect("Failed to load concepts.obr");

        eprintln!(
            "  Path: {:?}",
            path.canonicalize().unwrap_or_else(|_| path.clone())
        );
        eprintln!(
            "  File size: {} bytes",
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        );

        // Should have millions of entries
        eprintln!(
            "  Loaded {} concepts, {} labels",
            reg.len(),
            reg.label_count()
        );
        assert!(
            reg.len() > 1_000_000,
            "Expected >1M concepts, got {}",
            reg.len()
        );

        // First entry should be "happiness" (from our header analysis)
        match reg.resolve("happiness") {
            ResolveResult::Found(c) => {
                eprintln!("  happiness → Q{} ({})", c.qid, c.canonical_name);
                assert!(!c.canonical_name.is_empty());
            }
            ResolveResult::Ambiguous(matches) => {
                eprintln!("  happiness → {} ambiguous matches", matches.len());
            }
            other => panic!("Expected Found/Ambiguous for 'happiness', got {:?}", other),
        }

        // "water" may be ambiguous (multiple Wikidata concepts with "water" label)
        match reg.resolve("water") {
            ResolveResult::Found(c) => {
                eprintln!("  water → Q{} ({})", c.qid, c.canonical_name);
            }
            ResolveResult::Ambiguous(matches) => {
                eprintln!("  water → {} ambiguous matches", matches.len());
                assert!(!matches.is_empty());
            }
            ResolveResult::Fuzzy(c) => {
                eprintln!("  water → Q{} (fuzzy)", c.qid);
            }
            ResolveResult::NotFound => {
                // water might genuinely not be a standalone Wikidata label
                eprintln!("  water → NotFound (may not be a primary Wikidata label)");
            }
        }
    }
}
