//! Bounded, on-demand Concept Registry backed by fixed-record sidecar indexes.
//!
//! The 1+ GiB OBR remains the source of truth. Label and CCID sidecars contain
//! only sorted `(key, OBR offset)` records, so startup never materializes the
//! complete registry or duplicates every label in hash maps.

use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use indexmap::IndexMap;
use memmap2::Mmap;

use crate::ccid::Ccid;
use crate::concept_registry::{
    ConceptCategory, ConceptLookup, ConceptLookupError, ConceptRegistry, ResolveResult,
    ResolvedConcept,
};
use crate::concept_registry_manifest::{
    ConceptRegistryIndexManifest, ConceptRegistryManifest, ObrHeaderMetadata,
};

pub const REGISTRY_INDEX_VERSION: u32 = 1;
pub const REGISTRY_INDEX_HEADER_SIZE: u64 = 64;
pub const REGISTRY_INDEX_RECORD_SIZE: u64 = 24;
pub const LABEL_INDEX_MAGIC: [u8; 4] = *b"OBLI";
pub const CCID_INDEX_MAGIC: [u8; 4] = *b"OBCI";
const MAX_AMBIGUOUS_MATCHES: usize = 1_024;

#[derive(Debug)]
pub enum IndexedRegistryError {
    Io(std::io::Error),
    InvalidHeader(&'static str),
    UnsupportedVersion(u32),
    ArtifactMismatch(&'static str),
    InvalidOffset(u64),
    InvalidUtf8,
    TooManyMatches(usize),
    Poisoned(&'static str),
}

impl fmt::Display for IndexedRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "indexed registry I/O error: {error}"),
            Self::InvalidHeader(reason) => write!(formatter, "invalid registry index: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported registry index version: {version}")
            }
            Self::ArtifactMismatch(reason) => {
                write!(formatter, "registry index artifact mismatch: {reason}")
            }
            Self::InvalidOffset(offset) => write!(formatter, "invalid OBR offset: {offset}"),
            Self::InvalidUtf8 => write!(formatter, "invalid UTF-8 in indexed OBR entry"),
            Self::TooManyMatches(count) => {
                write!(formatter, "label expands to too many concepts: {count}")
            }
            Self::Poisoned(lock) => write!(formatter, "indexed registry lock poisoned: {lock}"),
        }
    }
}

impl std::error::Error for IndexedRegistryError {}

impl From<std::io::Error> for IndexedRegistryError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy)]
struct IndexRecord {
    key: [u8; 16],
    offset: u64,
}

struct FixedIndex {
    mmap: Mmap,
    records: u64,
    file_len: u64,
}

impl FixedIndex {
    fn open(
        path: &Path,
        expected_magic: [u8; 4],
        expected_obr_checksum: &str,
    ) -> Result<Self, IndexedRegistryError> {
        let file = File::open(path)?;
        let file_len = file.metadata()?.len();
        if file_len < REGISTRY_INDEX_HEADER_SIZE {
            return Err(IndexedRegistryError::InvalidHeader("truncated header"));
        }
        // SAFETY: the mapping is read-only, the file is never mutated by this
        // process, and artifact length/header validation completes before any
        // record slice is exposed.
        let mmap = unsafe { Mmap::map(&file)? };
        let header = &mmap[..REGISTRY_INDEX_HEADER_SIZE as usize];
        if header[0..4] != expected_magic {
            return Err(IndexedRegistryError::InvalidHeader("magic"));
        }
        let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if version != REGISTRY_INDEX_VERSION {
            return Err(IndexedRegistryError::UnsupportedVersion(version));
        }
        let records = u64::from_le_bytes(header[8..16].try_into().unwrap());
        let expected_checksum = decode_checksum(expected_obr_checksum)?;
        if header[16..48] != expected_checksum {
            return Err(IndexedRegistryError::ArtifactMismatch("OBR checksum"));
        }
        let expected_len = REGISTRY_INDEX_HEADER_SIZE
            .checked_add(
                records
                    .checked_mul(REGISTRY_INDEX_RECORD_SIZE)
                    .ok_or(IndexedRegistryError::InvalidHeader("record count overflow"))?,
            )
            .ok_or(IndexedRegistryError::InvalidHeader("file size overflow"))?;
        if file_len != expected_len {
            return Err(IndexedRegistryError::InvalidHeader("file length"));
        }
        Ok(Self {
            mmap,
            records,
            file_len,
        })
    }

    fn record(&self, index: u64) -> Result<IndexRecord, IndexedRegistryError> {
        if index >= self.records {
            return Err(IndexedRegistryError::InvalidHeader("record index"));
        }
        let start =
            usize::try_from(REGISTRY_INDEX_HEADER_SIZE + index * REGISTRY_INDEX_RECORD_SIZE)
                .map_err(|_| IndexedRegistryError::InvalidHeader("record offset overflow"))?;
        let end = start + REGISTRY_INDEX_RECORD_SIZE as usize;
        let bytes = self
            .mmap
            .get(start..end)
            .ok_or(IndexedRegistryError::InvalidHeader("record bounds"))?;
        let mut key = [0u8; 16];
        key.copy_from_slice(&bytes[..16]);
        Ok(IndexRecord {
            key,
            offset: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
        })
    }

    fn offsets_for_key(&self, key: [u8; 16]) -> Result<Vec<u64>, IndexedRegistryError> {
        let mut low = 0;
        let mut high = self.records;
        while low < high {
            let mid = low + (high - low) / 2;
            if self.record(mid)?.key < key {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        let mut offsets = Vec::new();
        let mut cursor = low;
        while cursor < self.records {
            let record = self.record(cursor)?;
            if record.key != key {
                break;
            }
            offsets.push(record.offset);
            if offsets.len() > MAX_AMBIGUOUS_MATCHES {
                return Err(IndexedRegistryError::TooManyMatches(offsets.len()));
            }
            cursor += 1;
        }
        Ok(offsets)
    }
}

struct LookupCache {
    capacity: usize,
    entries: IndexMap<String, ResolveResult>,
}

impl LookupCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: IndexMap::with_capacity(capacity),
        }
    }

    fn get(&mut self, key: &str) -> Option<ResolveResult> {
        let value = self.entries.shift_remove(key)?;
        self.entries.insert(key.to_string(), value.clone());
        Some(value)
    }

    fn insert(&mut self, key: String, value: ResolveResult) {
        if self.capacity == 0 {
            return;
        }
        self.entries.shift_remove(&key);
        while self.entries.len() >= self.capacity {
            self.entries.shift_remove_index(0);
        }
        self.entries.insert(key, value);
    }
}

pub struct IndexedConceptRegistry {
    obr: Mutex<File>,
    obr_len: u64,
    label_index: FixedIndex,
    ccid_index: FixedIndex,
    cache: Mutex<LookupCache>,
    metadata: ObrHeaderMetadata,
}

impl IndexedConceptRegistry {
    pub fn label_index_path(obr_path: &Path) -> PathBuf {
        append_suffix(obr_path, ".labels.idx")
    }

    pub fn ccid_index_path(obr_path: &Path) -> PathBuf {
        append_suffix(obr_path, ".ccids.idx")
    }

    pub fn indexes_exist(obr_path: &Path) -> bool {
        Self::label_index_path(obr_path).is_file() && Self::ccid_index_path(obr_path).is_file()
    }

    pub fn open(
        obr_path: &Path,
        manifest: &ConceptRegistryManifest,
        cache_capacity: usize,
    ) -> Result<Self, IndexedRegistryError> {
        let metadata = ConceptRegistry::inspect_obr(obr_path)
            .map_err(|_| IndexedRegistryError::InvalidHeader("OBR header"))?;
        if metadata.entry_count != manifest.entry_count
            || metadata.label_count != manifest.label_count
        {
            return Err(IndexedRegistryError::ArtifactMismatch("manifest counts"));
        }
        let obr = File::open(obr_path)?;
        let obr_len = obr.metadata()?.len();
        let label_index = FixedIndex::open(
            &Self::label_index_path(obr_path),
            LABEL_INDEX_MAGIC,
            &manifest.obr_blake3,
        )?;
        let ccid_index = FixedIndex::open(
            &Self::ccid_index_path(obr_path),
            CCID_INDEX_MAGIC,
            &manifest.obr_blake3,
        )?;
        if let Some(expected) = &manifest.label_index {
            validate_index_manifest(expected, &label_index)?;
        }
        if let Some(expected) = &manifest.ccid_index {
            validate_index_manifest(expected, &ccid_index)?;
        }
        if ccid_index.records != metadata.entry_count {
            return Err(IndexedRegistryError::ArtifactMismatch(
                "CCID index record count",
            ));
        }
        Ok(Self {
            obr: Mutex::new(obr),
            obr_len,
            label_index,
            ccid_index,
            cache: Mutex::new(LookupCache::new(cache_capacity)),
            metadata,
        })
    }

    pub const fn metadata(&self) -> ObrHeaderMetadata {
        self.metadata
    }

    pub fn resolve_checked(&self, name: &str) -> Result<ResolveResult, IndexedRegistryError> {
        let normalized = normalize_label(name);
        if let Some(result) = self
            .cache
            .lock()
            .map_err(|_| IndexedRegistryError::Poisoned("cache"))?
            .get(&normalized)
        {
            return Ok(result);
        }
        let key = label_key(&normalized);
        let offsets = self.label_index.offsets_for_key(key)?;
        let mut matches = Vec::new();
        for offset in offsets {
            if let Some((concept, source)) = self.read_entry_matching_label(offset, &normalized)? {
                matches.push((source, offset, concept));
            }
        }
        // OBR source IDs encode the deterministic source priority, while the
        // entry offset preserves the builder's quality ranking within a source.
        // ConceptResolver intentionally uses the first ambiguous match, so this
        // order is part of the registry's observable resolution contract.
        matches.sort_by_key(|(source, offset, _)| (*source, *offset));
        let mut seen = HashSet::new();
        matches.retain(|(_, _, concept)| seen.insert(concept.ccid));
        let mut matches: Vec<_> = matches.into_iter().map(|(_, _, concept)| concept).collect();
        let result = match matches.len() {
            0 => ResolveResult::NotFound,
            1 => ResolveResult::Found(matches.remove(0)),
            _ => ResolveResult::Ambiguous(matches),
        };
        self.cache
            .lock()
            .map_err(|_| IndexedRegistryError::Poisoned("cache"))?
            .insert(normalized, result.clone());
        Ok(result)
    }

    pub fn get_by_ccid_checked(
        &self,
        ccid: &Ccid,
    ) -> Result<Option<ResolvedConcept>, IndexedRegistryError> {
        let offsets = self.ccid_index.offsets_for_key(*ccid)?;
        for offset in offsets {
            let (concept, _) = self.read_entry(offset)?;
            if concept.ccid == *ccid {
                return Ok(Some(concept));
            }
        }
        Ok(None)
    }

    fn read_entry(&self, offset: u64) -> Result<(ResolvedConcept, u8), IndexedRegistryError> {
        let mut file = self
            .obr
            .lock()
            .map_err(|_| IndexedRegistryError::Poisoned("OBR"))?;
        self.read_entry_header(&mut file, offset)
    }

    fn read_entry_matching_label(
        &self,
        offset: u64,
        normalized: &str,
    ) -> Result<Option<(ResolvedConcept, u8)>, IndexedRegistryError> {
        let mut file = self
            .obr
            .lock()
            .map_err(|_| IndexedRegistryError::Poisoned("OBR"))?;
        let (concept, source) = self.read_entry_header(&mut file, offset)?;
        if normalize_label(&concept.canonical_name) == normalized {
            return Ok(Some((concept, source)));
        }
        let n_labels = read_u16(&mut file)? as usize;
        for _ in 0..n_labels {
            let length = read_u16(&mut file)? as usize;
            let label = read_string(&mut file, length)?;
            if normalize_label(&label) == normalized {
                return Ok(Some((concept, source)));
            }
        }
        Ok(None)
    }

    fn read_entry_header(
        &self,
        file: &mut File,
        offset: u64,
    ) -> Result<(ResolvedConcept, u8), IndexedRegistryError> {
        if offset < 32 || offset >= self.obr_len {
            return Err(IndexedRegistryError::InvalidOffset(offset));
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut fixed = [0u8; 24];
        file.read_exact(&mut fixed)?;
        let mut ccid = [0u8; 16];
        ccid.copy_from_slice(&fixed[..16]);
        let qid = u32::from_le_bytes(fixed[16..20].try_into().unwrap());
        let source = fixed[20];
        let category = ConceptCategory::from_u8(fixed[21]);
        let name_len = u16::from_le_bytes(fixed[22..24].try_into().unwrap()) as usize;
        let canonical_name = read_string(file, name_len)?;
        Ok((
            ResolvedConcept {
                ccid,
                qid,
                category,
                canonical_name,
            },
            source,
        ))
    }
}

impl ConceptLookup for IndexedConceptRegistry {
    fn resolve(&self, name: &str) -> ResolveResult {
        self.resolve_checked(name)
            .unwrap_or(ResolveResult::NotFound)
    }

    fn resolve_checked(&self, name: &str) -> Result<ResolveResult, ConceptLookupError> {
        IndexedConceptRegistry::resolve_checked(self, name)
            .map_err(|error| ConceptLookupError::new(error.to_string()))
    }
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn validate_index_manifest(
    expected: &ConceptRegistryIndexManifest,
    index: &FixedIndex,
) -> Result<(), IndexedRegistryError> {
    if expected.schema_version != REGISTRY_INDEX_VERSION
        || u64::from(expected.record_size) != REGISTRY_INDEX_RECORD_SIZE
        || expected.record_count != index.records
        || expected.file_size != index.file_len
    {
        return Err(IndexedRegistryError::ArtifactMismatch(
            "sidecar manifest metadata",
        ));
    }
    Ok(())
}

fn normalize_label(label: &str) -> String {
    label.to_lowercase()
}

fn label_key(label: &str) -> [u8; 16] {
    let hash = blake3::hash(label.as_bytes());
    hash.as_bytes()[..16].try_into().unwrap()
}

fn decode_checksum(value: &str) -> Result<[u8; 32], IndexedRegistryError> {
    if value.len() != 64 {
        return Err(IndexedRegistryError::InvalidHeader("checksum length"));
    }
    let mut bytes = [0u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| IndexedRegistryError::InvalidHeader("checksum encoding"))?;
        bytes[index] = u8::from_str_radix(text, 16)
            .map_err(|_| IndexedRegistryError::InvalidHeader("checksum hex"))?;
    }
    Ok(bytes)
}

fn read_u16(reader: &mut File) -> Result<u16, IndexedRegistryError> {
    let mut bytes = [0u8; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_string(reader: &mut File, length: usize) -> Result<String, IndexedRegistryError> {
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| IndexedRegistryError::InvalidUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn write_index(path: &Path, magic: [u8; 4], checksum: [u8; 32], mut records: Vec<IndexRecord>) {
        records.sort_by_key(|record| (record.key, record.offset));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&magic);
        bytes.extend_from_slice(&REGISTRY_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(records.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&checksum);
        bytes.extend_from_slice(&[0u8; 16]);
        for record in records {
            bytes.extend_from_slice(&record.key);
            bytes.extend_from_slice(&record.offset.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn resolves_from_fixed_indexes_without_materializing_the_obr() {
        let directory = std::env::temp_dir().join(format!(
            "onebrain-indexed-registry-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let obr_path = directory.join("concepts.obr");
        let ccid = crate::ccid::ccid(b"wd:Q283");
        let canonical = b"water";
        let label = "nước".as_bytes();
        let mut obr = Vec::new();
        obr.extend_from_slice(b"OBR1");
        obr.extend_from_slice(&1u32.to_le_bytes());
        obr.extend_from_slice(&1u64.to_le_bytes());
        obr.extend_from_slice(&1u64.to_le_bytes());
        obr.extend_from_slice(&[0u8; 8]);
        let offset = obr.len() as u64;
        obr.extend_from_slice(&ccid);
        obr.extend_from_slice(&283u32.to_le_bytes());
        obr.push(0);
        obr.push(7);
        obr.extend_from_slice(&(canonical.len() as u16).to_le_bytes());
        obr.extend_from_slice(canonical);
        obr.extend_from_slice(&1u16.to_le_bytes());
        obr.extend_from_slice(&(label.len() as u16).to_le_bytes());
        obr.extend_from_slice(label);
        std::fs::write(&obr_path, &obr).unwrap();

        let checksum = *blake3::hash(&obr).as_bytes();
        write_index(
            &IndexedConceptRegistry::label_index_path(&obr_path),
            LABEL_INDEX_MAGIC,
            checksum,
            vec![
                IndexRecord {
                    key: label_key("water"),
                    offset,
                },
                IndexRecord {
                    key: label_key("nước"),
                    offset,
                },
            ],
        );
        write_index(
            &IndexedConceptRegistry::ccid_index_path(&obr_path),
            CCID_INDEX_MAGIC,
            checksum,
            vec![IndexRecord { key: ccid, offset }],
        );
        let manifest = ConceptRegistryManifest {
            manifest_version: 1,
            obr_schema_version: 1,
            builder_version: "test".to_string(),
            dedup_policy_version: "test".to_string(),
            built_at_utc: "2026-07-23T00:00:00Z".to_string(),
            obr_blake3: blake3::hash(&obr).to_hex().to_string(),
            entry_count: 1,
            label_count: 1,
            sources: BTreeMap::new(),
            label_index: None,
            ccid_index: None,
        };

        let registry = IndexedConceptRegistry::open(&obr_path, &manifest, 2).unwrap();
        let first_ccid = match registry.resolve_checked("WATER").unwrap() {
            ResolveResult::Found(concept) => concept.ccid,
            other => panic!("unexpected resolution: {other:?}"),
        };
        assert_eq!(first_ccid, ccid);
        let second_peer_registry = IndexedConceptRegistry::open(&obr_path, &manifest, 2).unwrap();
        let second_ccid = match second_peer_registry.resolve_checked("water").unwrap() {
            ResolveResult::Found(concept) => concept.ccid,
            other => panic!("unexpected second-peer resolution: {other:?}"),
        };
        assert_eq!(first_ccid, second_ccid);
        assert_eq!(
            registry.get_by_ccid_checked(&ccid).unwrap().unwrap().qid,
            283
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ambiguous_matches_preserve_source_then_builder_priority() {
        let directory = std::env::temp_dir().join(format!(
            "onebrain-indexed-priority-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let obr_path = directory.join("concepts.obr");
        let lower_priority_ccid = crate::ccid::ccid(b"geonames:water");
        let preferred_ccid = crate::ccid::ccid(b"wd:Q283");
        let mut obr = Vec::new();
        obr.extend_from_slice(b"OBR1");
        obr.extend_from_slice(&1u32.to_le_bytes());
        obr.extend_from_slice(&2u64.to_le_bytes());
        obr.extend_from_slice(&2u64.to_le_bytes());
        obr.extend_from_slice(&[0u8; 8]);

        let append = |bytes: &mut Vec<u8>, ccid: [u8; 16], qid: u32, source: u8| {
            let offset = bytes.len() as u64;
            bytes.extend_from_slice(&ccid);
            bytes.extend_from_slice(&qid.to_le_bytes());
            bytes.push(source);
            bytes.push(7);
            bytes.extend_from_slice(&5u16.to_le_bytes());
            bytes.extend_from_slice(b"water");
            bytes.extend_from_slice(&0u16.to_le_bytes());
            offset
        };
        // Deliberately put the lower-priority source first in the OBR. Source
        // priority must dominate offset priority.
        let lower_offset = append(&mut obr, lower_priority_ccid, 0, 1);
        let preferred_offset = append(&mut obr, preferred_ccid, 283, 0);
        std::fs::write(&obr_path, &obr).unwrap();
        let checksum = *blake3::hash(&obr).as_bytes();
        write_index(
            &IndexedConceptRegistry::label_index_path(&obr_path),
            LABEL_INDEX_MAGIC,
            checksum,
            vec![
                IndexRecord {
                    key: label_key("water"),
                    offset: lower_offset,
                },
                IndexRecord {
                    key: label_key("water"),
                    offset: preferred_offset,
                },
            ],
        );
        write_index(
            &IndexedConceptRegistry::ccid_index_path(&obr_path),
            CCID_INDEX_MAGIC,
            checksum,
            vec![
                IndexRecord {
                    key: lower_priority_ccid,
                    offset: lower_offset,
                },
                IndexRecord {
                    key: preferred_ccid,
                    offset: preferred_offset,
                },
            ],
        );
        let manifest = ConceptRegistryManifest {
            manifest_version: 1,
            obr_schema_version: 1,
            builder_version: "test".to_string(),
            dedup_policy_version: "test".to_string(),
            built_at_utc: "2026-07-23T00:00:00Z".to_string(),
            obr_blake3: blake3::hash(&obr).to_hex().to_string(),
            entry_count: 2,
            label_count: 2,
            sources: BTreeMap::new(),
            label_index: None,
            ccid_index: None,
        };

        let registry = IndexedConceptRegistry::open(&obr_path, &manifest, 2).unwrap();
        match registry.resolve_checked("water").unwrap() {
            ResolveResult::Ambiguous(concepts) => {
                assert_eq!(concepts[0].ccid, preferred_ccid);
                assert_eq!(concepts[0].qid, 283);
            }
            other => panic!("unexpected resolution: {other:?}"),
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
